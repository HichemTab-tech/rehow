use crate::approval::{ApprovalKind, encode_approval_request, validate_summary};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::{
    collections::HashMap,
    env,
    ffi::CString,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

#[cfg(test)]
use std::sync::Barrier;

const HELPER_NAME: &str = "rehow-mcp-approve";
const EXPECTED_HELPER_DIGEST: Option<&str> = option_env!("REHOW_MCP_APPROVAL_HELPER_BLAKE3");
const MAX_HELPER_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CHILD_ENVIRONMENT_BYTES: usize = 16 * 1024;
const MAX_PENDING_PLANS: usize = 64;
const MAX_PLAN_LIFETIME: Duration = Duration::from_secs(90);
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApprovalHelperStatus {
    NotConfigured,
    Missing,
    Unsafe,
    Trusted,
}

pub(crate) struct ApprovalHelperProvenance {
    status: ApprovalHelperStatus,
    helper: Option<ApprovalHelper>,
}

impl ApprovalHelperProvenance {
    pub(crate) fn status(&self) -> ApprovalHelperStatus {
        self.status
    }
}

#[derive(Clone)]
struct ApprovalHelper {
    executable: Arc<File>,
}

pub(crate) fn capture_approval_helper() -> ApprovalHelperProvenance {
    let Some(expected_digest) = EXPECTED_HELPER_DIGEST else {
        return unavailable_provenance(ApprovalHelperStatus::NotConfigured);
    };
    let Some(candidate) = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(HELPER_NAME)))
    else {
        return unavailable_provenance(ApprovalHelperStatus::Unsafe);
    };
    capture_approval_helper_from(&candidate, expected_digest)
}

fn unavailable_provenance(status: ApprovalHelperStatus) -> ApprovalHelperProvenance {
    ApprovalHelperProvenance {
        status,
        helper: None,
    }
}

fn capture_approval_helper_from(path: &Path, expected_digest: &str) -> ApprovalHelperProvenance {
    if !valid_digest(expected_digest) {
        return unavailable_provenance(ApprovalHelperStatus::NotConfigured);
    }
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return unavailable_provenance(ApprovalHelperStatus::Missing);
        }
        Err(_) => return unavailable_provenance(ApprovalHelperStatus::Unsafe),
    };
    let effective_user = unsafe { libc::geteuid() };
    if !safe_helper_metadata(&before, effective_user) {
        return unavailable_provenance(ApprovalHelperStatus::Unsafe);
    }
    let mut executable = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(file) => file,
        Err(_) => return unavailable_provenance(ApprovalHelperStatus::Unsafe),
    };
    let held_before = match executable.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return unavailable_provenance(ApprovalHelperStatus::Unsafe),
    };
    if !safe_helper_metadata(&held_before, effective_user)
        || file_identity(&before) != file_identity(&held_before)
    {
        return unavailable_provenance(ApprovalHelperStatus::Unsafe);
    }

    let (snapshot, copied, digest) = match immutable_helper_snapshot(&mut executable) {
        Ok(snapshot) => snapshot,
        Err(_) => return unavailable_provenance(ApprovalHelperStatus::Unsafe),
    };
    if copied == 0 || copied > MAX_HELPER_BYTES {
        return unavailable_provenance(ApprovalHelperStatus::Unsafe);
    }
    let held_after = match executable.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return unavailable_provenance(ApprovalHelperStatus::Unsafe),
    };
    let after = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return unavailable_provenance(ApprovalHelperStatus::Unsafe),
    };
    if file_identity(&held_before) != file_identity(&held_after)
        || file_identity(&held_after) != file_identity(&after)
        || !safe_helper_metadata(&held_after, effective_user)
        || digest.to_hex().as_str() != expected_digest
    {
        return unavailable_provenance(ApprovalHelperStatus::Unsafe);
    }

    ApprovalHelperProvenance {
        status: ApprovalHelperStatus::Trusted,
        helper: Some(ApprovalHelper {
            executable: Arc::new(snapshot),
        }),
    }
}

fn immutable_helper_snapshot(source: &mut File) -> io::Result<(File, u64, blake3::Hash)> {
    let mut snapshot = executable_memfd()?;
    let mut hasher = blake3::Hasher::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "helper too large"))?;
        if total > MAX_HELPER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "helper too large",
            ));
        }
        hasher.update(&buffer[..read]);
        snapshot.write_all(&buffer[..read])?;
    }
    snapshot.flush()?;
    // SAFETY: snapshot is an owned private memfd and the mode removes all write permissions.
    if unsafe { libc::fchmod(snapshot.as_raw_fd(), 0o500) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let seals = libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
    // SAFETY: F_ADD_SEALS operates only on this private memfd.
    if unsafe { libc::fcntl(snapshot.as_raw_fd(), libc::F_ADD_SEALS, seals) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: F_GET_SEALS only reads this private memfd's seal mask.
    let active_seals = unsafe { libc::fcntl(snapshot.as_raw_fd(), libc::F_GET_SEALS) };
    if active_seals < 0 {
        return Err(io::Error::last_os_error());
    }
    if active_seals & seals != seals {
        return Err(io::Error::other("helper snapshot did not seal"));
    }
    let snapshot_metadata = snapshot.metadata()?;
    let bridge = format!("/proc/self/fd/{}", snapshot.as_raw_fd());
    let retained = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
        .open(bridge)?;
    let retained_metadata = retained.metadata()?;
    if snapshot_metadata.dev() != retained_metadata.dev()
        || snapshot_metadata.ino() != retained_metadata.ino()
        || snapshot_metadata.len() != retained_metadata.len()
    {
        return Err(io::Error::other("helper snapshot identity changed"));
    }
    drop(snapshot);
    Ok((retained, total, hasher.finalize()))
}

fn executable_memfd() -> io::Result<File> {
    let base = libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING;
    // MFD_EXEC is required on hardened new kernels and rejected by old kernels.
    // SAFETY: the name is fixed and memfd_create returns a newly owned descriptor.
    let mut descriptor =
        unsafe { libc::memfd_create(c"rehow-mcp-approve".as_ptr(), base | libc::MFD_EXEC) };
    if descriptor < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) {
        // SAFETY: old kernels create executable memfds by default.
        descriptor = unsafe { libc::memfd_create(c"rehow-mcp-approve".as_ptr(), base) };
    }
    if descriptor < 0 {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: memfd_create returned a newly owned descriptor.
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_helper_metadata(metadata: &Metadata, effective_user: u32) -> bool {
    metadata.is_file()
        && (1..=MAX_HELPER_BYTES).contains(&metadata.len())
        && metadata.nlink() == 1
        && (metadata.uid() == 0 || metadata.uid() == effective_user)
        && metadata.permissions().mode() & 0o022 == 0
        && metadata.permissions().mode() & 0o111 != 0
        && metadata.permissions().mode() & (libc::S_ISUID | libc::S_ISGID) == 0
}

fn file_identity(metadata: &Metadata) -> (u64, u64, u64, u32, u32, u32, i64, i64, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.uid(),
        metadata.gid(),
        metadata.mode(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ApprovalOperation {
    CategoryReorder = 1,
    CategoryRemove = 2,
    GuideDelete = 3,
    GuideTested = 4,
    AttachmentMetadataRemove = 5,
    GitCommit = 6,
    GitSync = 7,
}

impl ApprovalOperation {
    const fn required_kind(self) -> ApprovalKind {
        match self {
            Self::CategoryReorder => ApprovalKind::Broad,
            Self::CategoryRemove | Self::GuideDelete | Self::AttachmentMetadataRemove => {
                ApprovalKind::Destructive
            }
            Self::GuideTested => ApprovalKind::Tested,
            Self::GitCommit | Self::GitSync => ApprovalKind::GitWrite,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApprovalPlanTicket {
    pub(crate) token: String,
    pub(crate) lifetime: Duration,
    pub(crate) expires_at: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlanIssueError {
    Unavailable,
    Invalid,
    Capacity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlanExecution {
    Approved,
    Denied,
    Invalid,
}

enum PlanState {
    Pending,
    Active(Arc<AtomicBool>),
}

struct ApprovalPlan {
    workspace_handle: String,
    kind: ApprovalKind,
    operation: ApprovalOperation,
    binding: [u8; 32],
    summary: String,
    expires_at: Instant,
    state: PlanState,
}

pub(crate) struct ApprovalManager {
    helper: Option<ApprovalHelper>,
    #[cfg(test)]
    forced_outcome: Option<HelperOutcome>,
    #[cfg(test)]
    wait_for_cancellation: Option<(Arc<Barrier>, Arc<AtomicBool>)>,
    plan_key: [u8; 32],
    plans: Mutex<HashMap<String, ApprovalPlan>>,
    #[cfg(test)]
    finalize_barriers: Mutex<Option<(Arc<Barrier>, Arc<Barrier>)>>,
}

impl ApprovalManager {
    pub(crate) fn capture() -> (Self, ApprovalHelperStatus) {
        let provenance = capture_approval_helper();
        let status = provenance.status();
        (Self::new(provenance.helper), status)
    }

    fn new(helper: Option<ApprovalHelper>) -> Self {
        let mut plan_key = [0_u8; 32];
        plan_key[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        plan_key[16..].copy_from_slice(Uuid::new_v4().as_bytes());
        Self {
            helper,
            #[cfg(test)]
            forced_outcome: None,
            #[cfg(test)]
            wait_for_cancellation: None,
            plan_key,
            plans: Mutex::new(HashMap::new()),
            #[cfg(test)]
            finalize_barriers: Mutex::new(None),
        }
    }

    pub(crate) fn issue(
        &self,
        workspace_handle: &str,
        kind: ApprovalKind,
        operation: ApprovalOperation,
        binding: &[u8],
        summary: &str,
        lifetime: Duration,
    ) -> Result<ApprovalPlanTicket, PlanIssueError> {
        if self.helper.is_none() && !cfg!(test) {
            return Err(PlanIssueError::Unavailable);
        }
        #[cfg(test)]
        if self.helper.is_none() && self.forced_outcome.is_none() {
            return Err(PlanIssueError::Unavailable);
        }
        if !valid_workspace_handle(workspace_handle)
            || binding.is_empty()
            || operation.required_kind() != kind
            || lifetime.is_zero()
            || lifetime > MAX_PLAN_LIFETIME
            || validate_summary(summary).is_err()
        {
            return Err(PlanIssueError::Invalid);
        }
        let now = Instant::now();
        let expires_at = now.checked_add(lifetime).ok_or(PlanIssueError::Invalid)?;
        let mut plans = self.plans.lock().map_err(|_| PlanIssueError::Unavailable)?;
        prune_plans(&mut plans, now);
        if plans.len() >= MAX_PENDING_PLANS {
            return Err(PlanIssueError::Capacity);
        }
        let token = (0..8)
            .map(|_| {
                let mut random = [0_u8; 32];
                random[..16].copy_from_slice(Uuid::new_v4().as_bytes());
                random[16..].copy_from_slice(Uuid::new_v4().as_bytes());
                format!("plan:v1:{}", URL_SAFE_NO_PAD.encode(random))
            })
            .find(|token| !plans.contains_key(token))
            .ok_or(PlanIssueError::Capacity)?;
        plans.insert(
            token.clone(),
            ApprovalPlan {
                workspace_handle: workspace_handle.to_owned(),
                kind,
                operation,
                binding: self.binding_hash(workspace_handle, kind, operation, binding),
                summary: summary.to_owned(),
                expires_at,
                state: PlanState::Pending,
            },
        );
        Ok(ApprovalPlanTicket {
            token,
            lifetime,
            expires_at,
        })
    }

    pub(crate) fn execute(
        &self,
        token: &str,
        workspace_handle: &str,
        kind: ApprovalKind,
        operation: ApprovalOperation,
        binding: &[u8],
    ) -> PlanExecution {
        #[cfg(not(test))]
        let Some(helper) = self.helper.as_ref() else {
            return PlanExecution::Invalid;
        };
        #[cfg(test)]
        if self.helper.is_none() && self.forced_outcome.is_none() {
            return PlanExecution::Invalid;
        }
        let now = Instant::now();
        let expected_binding = self.binding_hash(workspace_handle, kind, operation, binding);
        let (summary, expires_at, cancellation) = {
            let Ok(mut plans) = self.plans.lock() else {
                return PlanExecution::Invalid;
            };
            prune_plans(&mut plans, now);
            let Some(plan) = plans.get_mut(token) else {
                return PlanExecution::Invalid;
            };
            if !matches!(plan.state, PlanState::Pending) {
                return PlanExecution::Invalid;
            }
            if plan.workspace_handle != workspace_handle
                || plan.kind != kind
                || plan.operation != operation
                || plan.binding != expected_binding
                || binding.is_empty()
            {
                plans.remove(token);
                return PlanExecution::Invalid;
            }
            let cancellation = Arc::new(AtomicBool::new(false));
            plan.state = PlanState::Active(Arc::clone(&cancellation));
            (plan.summary.clone(), plan.expires_at, cancellation)
        };

        #[cfg(not(test))]
        let outcome = helper.request(kind, &summary, expires_at, &cancellation);
        #[cfg(test)]
        let outcome = if let Some((started, observed)) = &self.wait_for_cancellation {
            started.wait();
            while !cancellation.load(Ordering::Acquire) && Instant::now() < expires_at {
                std::thread::sleep(Duration::from_millis(1));
            }
            observed.store(cancellation.load(Ordering::Acquire), Ordering::Release);
            HelperOutcome::Denied
        } else if let Some(outcome) = self.forced_outcome {
            outcome
        } else {
            self.helper
                .as_ref()
                .expect("the helper was checked above")
                .request(kind, &summary, expires_at, &cancellation)
        };
        #[cfg(test)]
        self.wait_at_finalize_barrier();
        let approved = if let Ok(mut plans) = self.plans.lock() {
            let still_active = matches!(
                plans.get(token).map(|plan| &plan.state),
                Some(PlanState::Active(active)) if Arc::ptr_eq(active, &cancellation)
            );
            let cancelled = cancellation.load(Ordering::Acquire);
            plans.remove(token);
            still_active
                && !cancelled
                && Instant::now() < expires_at
                && outcome == HelperOutcome::Approved
        } else {
            false
        };
        if approved {
            PlanExecution::Approved
        } else {
            PlanExecution::Denied
        }
    }

    pub(crate) fn cancel(&self, token: &str) -> bool {
        let now = Instant::now();
        let Ok(mut plans) = self.plans.lock() else {
            return false;
        };
        prune_plans(&mut plans, now);
        match plans.get(token).map(|plan| &plan.state) {
            Some(PlanState::Pending) => plans.remove(token).is_some(),
            Some(PlanState::Active(cancellation)) => {
                cancellation.store(true, Ordering::Release);
                true
            }
            None => false,
        }
    }

    #[cfg(test)]
    pub(crate) fn approving_for_tests() -> Self {
        let mut manager = Self::new(None);
        manager.forced_outcome = Some(HelperOutcome::Approved);
        manager
    }

    #[cfg(test)]
    pub(crate) fn denying_for_tests() -> Self {
        let mut manager = Self::new(None);
        manager.forced_outcome = Some(HelperOutcome::Denied);
        manager
    }

    #[cfg(test)]
    pub(crate) fn waiting_for_cancellation_for_tests(
        started: Arc<Barrier>,
        observed: Arc<AtomicBool>,
    ) -> Self {
        let mut manager = Self::new(None);
        manager.forced_outcome = Some(HelperOutcome::Denied);
        manager.wait_for_cancellation = Some((started, observed));
        manager
    }

    fn binding_hash(
        &self,
        workspace_handle: &str,
        kind: ApprovalKind,
        operation: ApprovalOperation,
        binding: &[u8],
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&self.plan_key);
        hasher.update(b"rehow-approval-plan-v1\0");
        hasher.update(&(workspace_handle.len() as u64).to_be_bytes());
        hasher.update(workspace_handle.as_bytes());
        hasher.update(&[kind as u8, operation as u8]);
        hasher.update(&(binding.len() as u64).to_be_bytes());
        hasher.update(binding);
        *hasher.finalize().as_bytes()
    }

    #[cfg(test)]
    fn wait_at_finalize_barrier(&self) {
        let barriers = self
            .finalize_barriers
            .lock()
            .ok()
            .and_then(|barriers| barriers.clone());
        if let Some((reached, release)) = barriers {
            reached.wait();
            release.wait();
        }
    }
}

fn valid_workspace_handle(value: &str) -> bool {
    value.len() == 38
        && value.starts_with("wsrun_")
        && value[6..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn prune_plans(plans: &mut HashMap<String, ApprovalPlan>, now: Instant) {
    plans.retain(|_, plan| {
        if now < plan.expires_at {
            true
        } else if let PlanState::Active(cancellation) = &plan.state {
            cancellation.store(true, Ordering::Release);
            true
        } else {
            false
        }
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HelperOutcome {
    Approved,
    Denied,
    Failed,
}

impl ApprovalHelper {
    fn request(
        &self,
        kind: ApprovalKind,
        summary: &str,
        deadline: Instant,
        cancellation: &AtomicBool,
    ) -> HelperOutcome {
        let request = match encode_approval_request(kind, summary) {
            Ok(request) => request,
            Err(_) => return HelperOutcome::Failed,
        };
        if cancellation.load(Ordering::Acquire) || Instant::now() >= deadline {
            return HelperOutcome::Denied;
        }
        let environment = match child_environment() {
            Some(environment) => environment,
            None => return HelperOutcome::Failed,
        };
        let argv = [
            c"rehow-mcp-approve".as_ptr().cast_mut(),
            std::ptr::null_mut(),
        ];
        let mut envp: Vec<*mut libc::c_char> = environment
            .iter()
            .map(|value| value.as_ptr().cast_mut())
            .collect();
        envp.push(std::ptr::null_mut());

        let (request_read, mut request_write) = match pipe() {
            Ok(pipe) => pipe,
            Err(_) => return HelperOutcome::Failed,
        };
        let (result_read, result_write) = match pipe() {
            Ok(pipe) => pipe,
            Err(_) => return HelperOutcome::Failed,
        };
        let child_request = match duplicate_high(request_read.as_raw_fd()) {
            Ok(descriptor) => descriptor,
            Err(_) => return HelperOutcome::Failed,
        };
        let child_result = match duplicate_high(result_write.as_raw_fd()) {
            Ok(descriptor) => descriptor,
            Err(_) => return HelperOutcome::Failed,
        };
        let child_executable = match duplicate_high(self.executable.as_raw_fd()) {
            Ok(descriptor) => descriptor,
            Err(_) => return HelperOutcome::Failed,
        };
        drop(request_read);
        drop(result_write);

        // SAFETY: getpid only reads the current process identity.
        let parent = unsafe { libc::getpid() };
        // SAFETY: all allocations and pointer arrays needed by the child are prepared above.
        let child = unsafe { libc::fork() };
        if child < 0 {
            return HelperOutcome::Failed;
        }
        if child == 0 {
            child_exec(
                child_request.as_raw_fd(),
                child_result.as_raw_fd(),
                child_executable.as_raw_fd(),
                parent,
                argv.as_ptr(),
                envp.as_ptr(),
            );
        }
        drop(child_request);
        drop(child_result);
        drop(child_executable);

        if !write_request(&mut request_write, &request, deadline, cancellation) {
            terminate_and_reap(child);
            return HelperOutcome::Failed;
        }
        drop(request_write);
        wait_for_helper(child, result_read, deadline, cancellation)
    }
}

fn child_environment() -> Option<Vec<CString>> {
    const ALLOWED: [&str; 8] = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XAUTHORITY",
        "DBUS_SESSION_BUS_ADDRESS",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
    ];
    let mut total = 0usize;
    let mut values = Vec::new();
    for name in ALLOWED {
        let Some(value) = env::var_os(name) else {
            continue;
        };
        let bytes = value.as_os_str().as_bytes();
        total = total.checked_add(name.len() + 1 + bytes.len() + 1)?;
        if total > MAX_CHILD_ENVIRONMENT_BYTES {
            return None;
        }
        let mut entry = Vec::with_capacity(name.len() + 1 + bytes.len());
        entry.extend_from_slice(name.as_bytes());
        entry.push(b'=');
        entry.extend_from_slice(bytes);
        values.push(CString::new(entry).ok()?);
    }
    Some(values)
}

fn pipe() -> io::Result<(File, File)> {
    let mut descriptors = [-1; 2];
    // SAFETY: descriptors points to exactly two writable integers.
    if unsafe { libc::pipe2(descriptors.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both successful pipe2 descriptors are newly owned.
    Ok(unsafe {
        (
            File::from_raw_fd(descriptors[0]),
            File::from_raw_fd(descriptors[1]),
        )
    })
}

fn write_request(
    destination: &mut File,
    mut request: &[u8],
    deadline: Instant,
    cancellation: &AtomicBool,
) -> bool {
    // SAFETY: F_GETFL/F_SETFL update only this owned request descriptor.
    let flags = unsafe { libc::fcntl(destination.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe {
            libc::fcntl(
                destination.as_raw_fd(),
                libc::F_SETFL,
                flags | libc::O_NONBLOCK,
            )
        } < 0
    {
        return false;
    }
    while !request.is_empty() {
        if cancellation.load(Ordering::Acquire) || Instant::now() >= deadline {
            return false;
        }
        match destination.write(request) {
            Ok(0) => return false,
            Ok(written) => request = &request[written..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let wait = remaining.min(CHILD_POLL_INTERVAL);
                let milliseconds = wait.as_millis().clamp(1, i32::MAX as u128) as libc::c_int;
                let mut poll = libc::pollfd {
                    fd: destination.as_raw_fd(),
                    events: libc::POLLOUT | libc::POLLERR | libc::POLLHUP,
                    revents: 0,
                };
                // SAFETY: poll points to one initialized descriptor for this call.
                let ready = unsafe { libc::poll(&mut poll, 1, milliseconds) };
                if ready < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    true
}

fn duplicate_high(descriptor: RawFd) -> io::Result<File> {
    // SAFETY: F_DUPFD_CLOEXEC duplicates the valid borrowed descriptor.
    let duplicate = unsafe { libc::fcntl(descriptor, libc::F_DUPFD_CLOEXEC, 10) };
    if duplicate < 0 {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: fcntl returned a newly owned descriptor.
        Ok(unsafe { File::from_raw_fd(duplicate) })
    }
}

fn child_exec(
    request: RawFd,
    result: RawFd,
    executable: RawFd,
    parent: libc::pid_t,
    argv: *const *mut libc::c_char,
    envp: *const *mut libc::c_char,
) -> ! {
    // SAFETY: this runs in the fork child and uses only async-signal-safe libc calls before exec.
    unsafe {
        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
            || libc::getppid() != parent
            || libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
            || libc::setsid() < 0
        {
            libc::_exit(126);
        }
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
        if null < 0
            || libc::dup2(null, libc::STDIN_FILENO) < 0
            || libc::dup2(null, libc::STDOUT_FILENO) < 0
            || libc::dup2(null, libc::STDERR_FILENO) < 0
            || libc::dup2(request, 3) < 0
            || libc::dup2(result, 4) < 0
            || libc::fcntl(3, libc::F_SETFD, 0) < 0
            || libc::fcntl(4, libc::F_SETFD, 0) < 0
        {
            libc::_exit(126);
        }
        // Do not close `null` explicitly: it may originally have occupied fd 3 or 4,
        // which the protocol pipes now own. Range closing handles it when it is > 4.
        if executable > 5 && libc::close_range(5, executable as u32 - 1, 0) != 0 {
            libc::_exit(126);
        }
        if executable < i32::MAX && libc::close_range(executable as u32 + 1, u32::MAX, 0) != 0 {
            libc::_exit(126);
        }
        libc::execveat(executable, c"".as_ptr(), argv, envp, libc::AT_EMPTY_PATH);
        libc::_exit(126);
    }
}

fn wait_for_helper(
    child: libc::pid_t,
    mut result: File,
    deadline: Instant,
    cancellation: &AtomicBool,
) -> HelperOutcome {
    // SAFETY: F_GETFL/F_SETFL update only this owned result descriptor.
    let flags = unsafe { libc::fcntl(result.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(result.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        terminate_and_reap(child);
        return HelperOutcome::Failed;
    }
    let mut response = Vec::with_capacity(2);
    let mut eof = false;
    let mut status = None;
    loop {
        if cancellation.load(Ordering::Acquire) || Instant::now() >= deadline {
            terminate_and_reap(child);
            return HelperOutcome::Denied;
        }
        drain_result(&mut result, &mut response, &mut eof);
        if response.len() > 1 {
            terminate_and_reap(child);
            return HelperOutcome::Failed;
        }
        if status.is_none() {
            let mut raw_status = 0;
            // SAFETY: raw_status is writable and child is this process's unreaped child.
            let waited = unsafe { libc::waitpid(child, &mut raw_status, libc::WNOHANG) };
            if waited == child {
                status = Some(raw_status);
            } else if waited < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
            {
                return HelperOutcome::Failed;
            }
        }
        if let Some(raw_status) = status {
            drain_result(&mut result, &mut response, &mut eof);
            if eof {
                if libc::WIFEXITED(raw_status) && libc::WEXITSTATUS(raw_status) == 0 {
                    return match response.as_slice() {
                        [1] => HelperOutcome::Approved,
                        [0] => HelperOutcome::Denied,
                        _ => HelperOutcome::Failed,
                    };
                }
                return HelperOutcome::Failed;
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let wait = remaining.min(CHILD_POLL_INTERVAL);
        let milliseconds = wait.as_millis().clamp(1, i32::MAX as u128) as libc::c_int;
        let mut poll = libc::pollfd {
            fd: result.as_raw_fd(),
            events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
            revents: 0,
        };
        // SAFETY: poll points to one initialized descriptor for the duration of the call.
        let polled = unsafe { libc::poll(&mut poll, 1, milliseconds) };
        if polled < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            terminate_and_reap(child);
            return HelperOutcome::Failed;
        }
    }
}

fn drain_result(result: &mut File, response: &mut Vec<u8>, eof: &mut bool) {
    let mut buffer = [0_u8; 2];
    loop {
        match result.read(&mut buffer) {
            Ok(0) => {
                *eof = true;
                return;
            }
            Ok(read) => response.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                response.extend_from_slice(&[0xff, 0xff]);
                return;
            }
        }
    }
}

fn terminate_and_reap(child: libc::pid_t) {
    // SAFETY: the negative PID targets only the child's session/process group; the direct kill
    // covers setsid/exec failure before group creation.
    unsafe {
        libc::kill(-child, libc::SIGKILL);
        libc::kill(child, libc::SIGKILL);
    }
    loop {
        let mut status = 0;
        // SAFETY: child is this process's child and status is writable.
        let waited = unsafe { libc::waitpid(child, &mut status, 0) };
        if waited == child
            || (waited < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD))
        {
            break;
        }
        if waited < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, process::Command};

    fn executable_fixture(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf, String) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(HELPER_NAME);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let digest = blake3::hash(bytes).to_hex().to_string();
        (directory, path, digest)
    }

    fn compile_protocol_helper(path: &Path, response: &[u8], delay_ms: u64, exit_code: i32) {
        let response = response
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let source = path.with_extension("rs");
        fs::write(
            &source,
            format!(
                r#"use std::{{fs::{{self, File}}, io::{{Read, Write}}, os::fd::FromRawFd, time::Duration}};
fn main() {{
    let mut request = unsafe {{ File::from_raw_fd(3) }};
    let mut bytes = Vec::new();
    request.read_to_end(&mut bytes).unwrap();
    std::thread::sleep(Duration::from_millis({delay_ms}));
    let mut result = unsafe {{ File::from_raw_fd(4) }};
    let leaked: Vec<_> = (5..64).filter(|descriptor| fs::read_link(format!("/proc/self/fd/{{descriptor}}")).is_ok()).collect();
    let descriptors_are_isolated = leaked.is_empty();
    let response: &[u8] = if descriptors_are_isolated {{ &[{response}] }} else {{ &[0] }};
    result.write_all(response).unwrap();
    result.flush().unwrap();
    std::process::exit({exit_code});
}}
"#
            ),
        )
        .unwrap();
        let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let output = Command::new(rustc)
            .arg("--edition=2024")
            .arg("-C")
            .arg("opt-level=0")
            .arg(&source)
            .arg("-o")
            .arg(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fixture compile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn compiled_provenance(path: &Path) -> ApprovalHelperProvenance {
        let digest = blake3::hash(&fs::read(path).unwrap()).to_hex().to_string();
        capture_approval_helper_from(path, &digest)
    }

    #[test]
    fn capture_requires_exact_safe_digest_and_metadata() {
        let (_directory, path, digest) = executable_fixture(b"not an executable, but safely held");
        assert_eq!(
            capture_approval_helper_from(&path, &digest).status(),
            ApprovalHelperStatus::Trusted
        );
        assert_eq!(
            capture_approval_helper_from(&path, &"0".repeat(64)).status(),
            ApprovalHelperStatus::Unsafe
        );
        assert_eq!(
            capture_approval_helper_from(&path, "ABC").status(),
            ApprovalHelperStatus::NotConfigured
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o722)).unwrap();
        assert_eq!(
            capture_approval_helper_from(&path, &digest).status(),
            ApprovalHelperStatus::Unsafe
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o4700)).unwrap();
        assert_eq!(
            capture_approval_helper_from(&path, &digest).status(),
            ApprovalHelperStatus::Unsafe
        );
    }

    #[test]
    fn capture_rejects_symlink_and_multiple_links() {
        let (directory, path, digest) = executable_fixture(b"held helper");
        let link = directory.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(
            capture_approval_helper_from(&link, &digest).status(),
            ApprovalHelperStatus::Unsafe
        );
        let hardlink = directory.path().join("hardlink");
        fs::hard_link(&path, hardlink).unwrap();
        assert_eq!(
            capture_approval_helper_from(&path, &digest).status(),
            ApprovalHelperStatus::Unsafe
        );
    }

    #[test]
    fn plans_are_bounded_bound_expiring_cancellable_and_single_use() {
        let (_directory, path, digest) = executable_fixture(b"held helper");
        let provenance = capture_approval_helper_from(&path, &digest);
        let manager = ApprovalManager::new(provenance.helper);
        let workspace = format!("wsrun_{}", Uuid::new_v4().simple());
        let ticket = manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"guide/revision/effect",
                "Move one draft Guide to recoverable trash.",
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(ticket.lifetime, Duration::from_secs(1));
        assert_eq!(
            manager.execute(
                &ticket.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"wrong",
            ),
            PlanExecution::Invalid
        );
        assert_eq!(
            manager.execute(
                &ticket.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"guide/revision/effect",
            ),
            PlanExecution::Invalid
        );

        let cancelled = manager
            .issue(
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::CategoryReorder,
                b"categories",
                "Reorder two Categories.",
                Duration::from_secs(1),
            )
            .unwrap();
        assert!(manager.cancel(&cancelled.token));
        assert!(!manager.cancel(&cancelled.token));
        assert_eq!(
            manager.execute(
                &cancelled.token,
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::CategoryReorder,
                b"categories",
            ),
            PlanExecution::Invalid
        );

        assert_eq!(
            manager.issue(
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::GuideDelete,
                b"wrong-kind",
                "Delete one Guide.",
                Duration::from_secs(1),
            ),
            Err(PlanIssueError::Invalid)
        );
        assert_eq!(
            manager.issue(
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::CategoryReorder,
                b"categories",
                "Reorder two Categories.",
                Duration::ZERO,
            ),
            Err(PlanIssueError::Invalid)
        );

        let expired = manager
            .issue(
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::CategoryReorder,
                b"expires",
                "Reorder two Categories.",
                Duration::from_millis(1),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(
            manager.execute(
                &expired.token,
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::CategoryReorder,
                b"expires",
            ),
            PlanExecution::Invalid
        );
        assert_eq!(
            manager.issue(
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::CategoryReorder,
                b"too-long",
                "Reorder two Categories.",
                MAX_PLAN_LIFETIME + Duration::from_millis(1),
            ),
            Err(PlanIssueError::Invalid)
        );
        assert_eq!(
            ApprovalManager::new(None).issue(
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::CategoryReorder,
                b"unavailable",
                "Reorder two Categories.",
                Duration::from_secs(1),
            ),
            Err(PlanIssueError::Unavailable)
        );
    }

    #[test]
    fn plan_capacity_is_enforced() {
        let (_directory, path, digest) = executable_fixture(b"held helper");
        let provenance = capture_approval_helper_from(&path, &digest);
        let manager = ApprovalManager::new(provenance.helper);
        let workspace = format!("wsrun_{}", Uuid::new_v4().simple());
        for index in 0..MAX_PENDING_PLANS {
            manager
                .issue(
                    &workspace,
                    ApprovalKind::Broad,
                    ApprovalOperation::CategoryReorder,
                    &index.to_be_bytes(),
                    "Reorder Categories.",
                    Duration::from_secs(1),
                )
                .unwrap();
        }
        assert_eq!(
            manager.issue(
                &workspace,
                ApprovalKind::Broad,
                ApprovalOperation::CategoryReorder,
                b"overflow",
                "Reorder Categories.",
                Duration::from_secs(1),
            ),
            Err(PlanIssueError::Capacity)
        );
    }

    #[test]
    fn child_environment_excludes_process_injection_and_effect_channels() {
        let environment = child_environment().unwrap();
        let names: Vec<&[u8]> = environment
            .iter()
            .map(|entry| entry.as_bytes().split(|byte| *byte == b'=').next().unwrap())
            .collect();
        for forbidden in [
            b"PATH".as_slice(),
            b"LD_PRELOAD".as_slice(),
            b"GTK_MODULES".as_slice(),
            b"HOME".as_slice(),
            b"SSH_AUTH_SOCK".as_slice(),
        ] {
            assert!(!names.contains(&forbidden));
        }
    }

    #[test]
    fn launcher_executes_the_held_inode_and_requires_exact_success() {
        let directory = tempfile::tempdir().unwrap();
        let helper = directory.path().join(HELPER_NAME);
        let replacement = directory.path().join("replacement");
        compile_protocol_helper(&helper, &[0], 0, 0);
        compile_protocol_helper(&replacement, &[1], 0, 0);
        let held = compiled_provenance(&helper);
        assert_eq!(held.status(), ApprovalHelperStatus::Trusted);
        fs::rename(&replacement, &helper).unwrap();

        let workspace = format!("wsrun_{}", Uuid::new_v4().simple());
        let manager = ApprovalManager::new(held.helper);
        let ticket = manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
                "Delete one Guide.",
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            manager.execute(
                &ticket.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
            ),
            PlanExecution::Denied
        );

        let replacement_manager = ApprovalManager::new(compiled_provenance(&helper).helper);
        let ticket = replacement_manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
                "Delete one Guide.",
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            replacement_manager.execute(
                &ticket.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
            ),
            PlanExecution::Approved
        );

        let trailing = directory.path().join("trailing");
        compile_protocol_helper(&trailing, &[1, 0], 0, 0);
        let trailing_manager = ApprovalManager::new(compiled_provenance(&trailing).helper);
        let ticket = trailing_manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
                "Delete one Guide.",
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            trailing_manager.execute(
                &ticket.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
            ),
            PlanExecution::Denied
        );

        let nonzero = directory.path().join("nonzero");
        compile_protocol_helper(&nonzero, &[1], 0, 7);
        let nonzero_manager = ApprovalManager::new(compiled_provenance(&nonzero).helper);
        let ticket = nonzero_manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
                "Delete one Guide.",
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            nonzero_manager.execute(
                &ticket.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
            ),
            PlanExecution::Denied
        );
    }

    #[test]
    fn launcher_executes_an_immutable_snapshot_after_same_inode_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let helper = directory.path().join(HELPER_NAME);
        let replacement = directory.path().join("replacement");
        compile_protocol_helper(&helper, &[0], 0, 0);
        compile_protocol_helper(&replacement, &[1], 0, 0);
        let held = compiled_provenance(&helper);
        let inode = fs::metadata(&helper).unwrap().ino();
        let replacement_bytes = fs::read(&replacement).unwrap();
        let mut destination = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&helper)
            .unwrap();
        destination.write_all(&replacement_bytes).unwrap();
        destination.flush().unwrap();
        drop(destination);
        assert_eq!(fs::metadata(&helper).unwrap().ino(), inode);

        let workspace = format!("wsrun_{}", Uuid::new_v4().simple());
        let manager = ApprovalManager::new(held.helper);
        let ticket = manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
                "Delete one Guide.",
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            manager.execute(
                &ticket.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
            ),
            PlanExecution::Denied
        );

        let current_manager = ApprovalManager::new(compiled_provenance(&helper).helper);
        let ticket = current_manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
                "Delete one Guide.",
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            current_manager.execute(
                &ticket.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"effect",
            ),
            PlanExecution::Approved
        );
    }

    #[test]
    fn cancellation_linearizes_before_approval_finalization() {
        let directory = tempfile::tempdir().unwrap();
        let helper = directory.path().join(HELPER_NAME);
        compile_protocol_helper(&helper, &[1], 0, 0);
        let manager = Arc::new(ApprovalManager::new(compiled_provenance(&helper).helper));
        let reached = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        *manager.finalize_barriers.lock().unwrap() =
            Some((Arc::clone(&reached), Arc::clone(&release)));
        let workspace = format!("wsrun_{}", Uuid::new_v4().simple());
        let ticket = manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"race",
                "Delete one Guide.",
                Duration::from_secs(5),
            )
            .unwrap();
        let token = ticket.token.clone();
        let execution_manager = Arc::clone(&manager);
        let execution_workspace = workspace.clone();
        let execution = std::thread::spawn(move || {
            execution_manager.execute(
                &token,
                &execution_workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"race",
            )
        });
        reached.wait();
        assert!(manager.cancel(&ticket.token));
        release.wait();
        assert_eq!(execution.join().unwrap(), PlanExecution::Denied);
        assert!(!manager.cancel(&ticket.token));
    }

    #[test]
    fn expiry_and_active_cancellation_kill_and_reap_the_helper() {
        let directory = tempfile::tempdir().unwrap();
        let helper = directory.path().join(HELPER_NAME);
        compile_protocol_helper(&helper, &[1], 10_000, 0);
        let manager = Arc::new(ApprovalManager::new(compiled_provenance(&helper).helper));
        let workspace = format!("wsrun_{}", Uuid::new_v4().simple());

        let expiring = manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"expiry",
                "Delete one Guide.",
                Duration::from_millis(100),
            )
            .unwrap();
        let started = Instant::now();
        assert_eq!(
            manager.execute(
                &expiring.token,
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"expiry",
            ),
            PlanExecution::Denied
        );
        assert!(started.elapsed() < Duration::from_secs(2));

        let active = manager
            .issue(
                &workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"cancel",
                "Delete one Guide.",
                Duration::from_secs(5),
            )
            .unwrap();
        let token = active.token.clone();
        let execution_manager = Arc::clone(&manager);
        let execution_workspace = workspace.clone();
        let execution = std::thread::spawn(move || {
            execution_manager.execute(
                &token,
                &execution_workspace,
                ApprovalKind::Destructive,
                ApprovalOperation::GuideDelete,
                b"cancel",
            )
        });
        std::thread::sleep(Duration::from_millis(75));
        assert!(manager.cancel(&active.token));
        assert_eq!(execution.join().unwrap(), PlanExecution::Denied);
        assert!(!manager.cancel(&active.token));
    }
}

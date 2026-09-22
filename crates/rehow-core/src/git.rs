//! Confined Git discovery/status and separately gated repository operations.

mod credential_proxy;
#[allow(
    dead_code,
    reason = "sealed HTTPS credentials connect to product Git commands in the next gate"
)]
mod https_credentials;
mod ssh_proxy;
#[allow(
    dead_code,
    reason = "sealed SSH resources connect to product Git commands in the next gate"
)]
mod ssh_sandbox;

pub use credential_proxy::run_git_credential_proxy;
pub use https_credentials::{
    GitHttpsCredentialBackingProvenance, GitHttpsCredentialBackingStatus,
    GitHttpsCredentialProvenance, GitHttpsCredentialProxyProvenance, GitHttpsCredentialProxyStatus,
    GitHttpsCredentialResourceError, GitHttpsCredentialResources, GitHttpsCredentialStatus,
    capture_git_https_credential_backing_provenance, capture_git_https_credential_provenance,
    capture_git_https_credential_proxy_provenance,
};
pub use ssh_proxy::run_git_ssh_proxy;
pub use ssh_sandbox::{
    GitSshProxyProvenance, GitSshProxyStatus, GitSshTransportError, GitSshTransportResources,
    capture_git_ssh_proxy_provenance,
};

use crate::storage::recovery::{
    GitCommitIntent, GitCommitIntentBranchRepresentation, GitCommitIntentError,
    GitCommitIntentStage, GitCommitStaging, GitCommitStagingError, GitCommitStagingSnapshot,
    GitSyncIntent, GitSyncIntentStage, create_git_commit_staging,
};
use crate::{
    protocol::{OperationId, PlanToken, Revision, Timestamp, current_timestamp, new_operation_id},
    storage::{
        ATTACHMENT_BYTES_LIMIT, ATTACHMENT_COUNT_LIMIT, CATEGORY_LIMIT, CloneDestinationSnapshot,
        EmptyWorkspaceTarget, GUIDE_COUNT_LIMIT, GitCloneMaterializationSource,
        GitCloneMaterializationStorageError, GitClonePackSource, GitCloneStaging,
        GitCloneStagingError, GitCommitMetadataStage, GitCommitMetadataStorageError,
        GitCommitPackInstallError, GitCommitRepositorySnapshot, GitCommitSnapshotError,
        GitConfigStorageError, GitInitializeStorageError, GitMaterializationPathKind,
        GitRepositoryCapabilityError, GitSyncMaterializationCurrent, GitSyncMaterializationSource,
        GitSyncMaterializationStorageError, GitSyncPreparedMaterialization, MANAGED_BYTES_LIMIT,
        WorkspaceInspection, WorkspaceView, activate_git_sync_materialization,
        activate_git_sync_materialization_plan, classify_git_materialization_path,
        create_git_clone_staging, execute_git_clone_materialization, finalize_git_clone_repository,
        inspect_workspace, open_initialized_workspace, prepare_git_sync_materialization,
        seal_git_clone_repository,
    },
};
use std::{
    env,
    ffi::CString,
    fmt,
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 1024 * 1024;
const PROCESS_TIMEOUT: Duration = Duration::from_secs(2);
const CLONE_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);
const CLONE_FETCH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const PUSH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const CLONE_LOCAL_READ_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const CLONE_DISCOVERY_OUTPUT_LIMIT: usize = 64 * 1024;
const SYNC_HISTORY_LIMIT: usize = 4_096;
const SYNC_HISTORY_OUTPUT_LIMIT: usize = (SYNC_HISTORY_LIMIT + 1) * 65;
const SYNC_FETCH_METADATA_RESERVE: u64 = 4 * 1024 * 1024;
const SYNC_FETCH_LARGE_FILE_SLOTS: u64 = 4;
const CLONE_BATCH_HEADER_LIMIT: usize = 160;
const CLONE_STAGING_FILE_LIMIT: u64 = 7 * 1024 * 1024 * 1024;
const MAX_REMOTES: usize = 32;
const MAX_REMOTE_NAME_BYTES: usize = 64;
const MAX_REMOTE_URL_BYTES: usize = 2048;
const MAX_GIT_CONFIG_BYTES: usize = 1024 * 1024;
const MAX_COMMIT_MESSAGE_SCALARS: usize = 200;
const MAX_GIT_IDENTITY_NAME_SCALARS: usize = 200;
const MAX_GIT_IDENTITY_EMAIL_BYTES: usize = 254;
const MAX_COMMIT_INDEX_BYTES: usize = 64 * 1024 * 1024;
const MAX_COMMIT_INDEX_ENTRIES: usize = 50_000;
const MAX_COMMIT_PATH_BYTES: usize = 4_096;
const MAX_COMMIT_TREE_BYTES: usize = 64 * 1024 * 1024;
const MAX_COMMIT_TREE_ENTRIES: usize = 50_000;
const MAX_COMMIT_STAGING_OBJECTS: usize = 100_000;
const MAX_COMMIT_STAGING_FILES: usize = 60_000;
const MAX_COMMIT_STAGING_BYTES: u64 = 3 * 1024 * 1024 * 1024;
const COMMIT_PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const COMMIT_PLAN_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const MAX_SSH_AGENT_PATH_BYTES: usize = 4_096;
const MAX_KNOWN_HOSTS_BYTES: u64 = 1024 * 1024;
#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
const MAX_CLONE_TREE_LISTING_BYTES: usize = 32 * 1024 * 1024;
#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
const MAX_CLONE_TREE_ENTRIES: usize =
    1 + CATEGORY_LIMIT + GUIDE_COUNT_LIMIT + ATTACHMENT_COUNT_LIMIT + 1;

#[cfg(test)]
thread_local! {
    static POST_CHECK_ALTERNATES: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static CLONE_PRESEAL_PAUSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static KNOWN_HOSTS_RACE: std::cell::RefCell<Option<KnownHostsRace>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
enum KnownHostsRace {
    Rewrite(PathBuf, Vec<u8>),
    Replace(PathBuf, Vec<u8>),
}

thread_local! {
    static ACTIVE_COMMIT_DEADLINE: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

struct GitCommitDeadlineGuard {
    previous: Option<Instant>,
}

impl GitCommitDeadlineGuard {
    fn enter() -> Self {
        let deadline = Instant::now()
            .checked_add(COMMIT_PLAN_TIMEOUT)
            .unwrap_or_else(Instant::now);
        let previous = ACTIVE_COMMIT_DEADLINE.with(|active| {
            let previous = active.get();
            active.set(Some(
                previous.map_or(deadline, |existing| existing.min(deadline)),
            ));
            previous
        });
        Self { previous }
    }
}

impl Drop for GitCommitDeadlineGuard {
    fn drop(&mut self) {
        ACTIVE_COMMIT_DEADLINE.with(|active| active.set(self.previous));
    }
}

fn active_git_commit_timeout(limit: Duration) -> Result<Duration, GitReadError> {
    ACTIVE_COMMIT_DEADLINE.with(|active| {
        let Some(deadline) = active.get() else {
            return Ok(limit);
        };
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(GitReadError::Timeout)?;
        if remaining.is_zero() {
            Err(GitReadError::Timeout)
        } else {
            Ok(limit.min(remaining))
        }
    })
}

fn active_git_commit_deadline_reached() -> bool {
    ACTIVE_COMMIT_DEADLINE.with(|active| {
        active
            .get()
            .is_some_and(|deadline| Instant::now() >= deadline)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitSshAgentStatus {
    NotConfigured,
    Trusted,
    Unsafe,
}

pub struct GitSshAgentProvenance {
    status: GitSshAgentStatus,
    _socket: Option<File>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitSshHostKeysStatus {
    NotConfigured,
    Trusted,
    Unsafe,
}

pub struct GitSshHostKeysProvenance {
    status: GitSshHostKeysStatus,
    _sources: Vec<Vec<u8>>,
}

impl GitSshHostKeysProvenance {
    pub fn status(&self) -> GitSshHostKeysStatus {
        self.status
    }

    pub fn source_count(&self) -> usize {
        self._sources.len()
    }
}

impl fmt::Debug for GitSshHostKeysProvenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitSshHostKeysProvenance")
            .field("status", &self.status)
            .field("source_count", &self.source_count())
            .finish_non_exhaustive()
    }
}

impl GitSshAgentProvenance {
    pub fn status(&self) -> GitSshAgentStatus {
        self.status
    }
}

impl fmt::Debug for GitSshAgentProvenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitSshAgentProvenance")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// Captures the launch-inherited SSH agent as an inode-held, path-redacted
/// capability. An unsafe value never prevents Rehow from starting; it simply
/// keeps authenticated SSH unavailable.
pub fn capture_git_ssh_agent_provenance() -> GitSshAgentProvenance {
    let path = env::var_os("SSH_AUTH_SOCK");
    capture_git_ssh_agent_provenance_from(path.as_deref())
}

/// Captures bounded stable snapshots of owner-safe user and root-owned system
/// known-host files. Host-key data is public trust material, but paths and file
/// identities remain native-only and are never included in `Debug`.
pub fn capture_git_ssh_host_keys_provenance() -> GitSshHostKeysProvenance {
    let home = env::var_os("HOME");
    let system_paths = [
        Path::new("/etc/ssh/ssh_known_hosts"),
        Path::new("/etc/ssh/ssh_known_hosts2"),
    ];
    capture_git_ssh_host_keys_from(home.as_deref(), &system_paths)
}

#[derive(Clone, Copy)]
enum KnownHostsOwner {
    EffectiveUser,
    Root,
}

enum KnownHostsCapture {
    Missing,
    Trusted(Vec<u8>),
    Unsafe,
}

fn capture_git_ssh_host_keys_from(
    home: Option<&std::ffi::OsStr>,
    system_paths: &[&Path],
) -> GitSshHostKeysProvenance {
    let effective_user = unsafe { libc::geteuid() };
    let mut sources = Vec::new();
    let mut unsafe_source = false;
    if let Some(home) = home {
        let home = Path::new(home);
        if safe_provenance_path(home) {
            match capture_known_hosts_file(
                &home.join(".ssh/known_hosts"),
                KnownHostsOwner::EffectiveUser,
                effective_user,
            ) {
                KnownHostsCapture::Missing => {}
                KnownHostsCapture::Trusted(bytes) => sources.push(bytes),
                KnownHostsCapture::Unsafe => unsafe_source = true,
            }
        } else {
            unsafe_source = true;
        }
    }
    for path in system_paths {
        match capture_known_hosts_file(path, KnownHostsOwner::Root, effective_user) {
            KnownHostsCapture::Missing => {}
            KnownHostsCapture::Trusted(bytes) => sources.push(bytes),
            KnownHostsCapture::Unsafe => unsafe_source = true,
        }
    }
    let status = if unsafe_source {
        GitSshHostKeysStatus::Unsafe
    } else if sources.is_empty() {
        GitSshHostKeysStatus::NotConfigured
    } else {
        GitSshHostKeysStatus::Trusted
    };
    if status != GitSshHostKeysStatus::Trusted {
        sources.clear();
    }
    GitSshHostKeysProvenance {
        status,
        _sources: sources,
    }
}

fn capture_known_hosts_file(
    path: &Path,
    owner: KnownHostsOwner,
    effective_user: u32,
) -> KnownHostsCapture {
    if !safe_provenance_path(path) {
        return KnownHostsCapture::Unsafe;
    }
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return KnownHostsCapture::Missing,
        Err(_) => return KnownHostsCapture::Unsafe,
    };
    let before_parents = match inspect_ssh_agent_parents(path.parent().unwrap(), effective_user) {
        Some(parents) => parents,
        None => return KnownHostsCapture::Unsafe,
    };
    if !known_hosts_metadata_safe(&before, owner, effective_user) {
        return KnownHostsCapture::Unsafe;
    }
    let mut file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(file) => file,
        Err(_) => return KnownHostsCapture::Unsafe,
    };
    let held_before = match file.metadata() {
        Ok(metadata) if known_hosts_metadata_safe(&metadata, owner, effective_user) => metadata,
        _ => return KnownHostsCapture::Unsafe,
    };
    if known_hosts_file_identity(&before) != known_hosts_file_identity(&held_before) {
        return KnownHostsCapture::Unsafe;
    }
    let first = match read_bounded_known_hosts(&mut file) {
        Some(bytes) => bytes,
        None => return KnownHostsCapture::Unsafe,
    };
    #[cfg(test)]
    apply_known_hosts_race();
    if file.seek(SeekFrom::Start(0)).is_err() {
        return KnownHostsCapture::Unsafe;
    }
    let second = match read_bounded_known_hosts(&mut file) {
        Some(bytes) => bytes,
        None => return KnownHostsCapture::Unsafe,
    };
    let held_after = match file.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return KnownHostsCapture::Unsafe,
    };
    let after = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return KnownHostsCapture::Unsafe,
    };
    let after_parents = match inspect_ssh_agent_parents(path.parent().unwrap(), effective_user) {
        Some(parents) => parents,
        None => return KnownHostsCapture::Unsafe,
    };
    if first != second
        || known_hosts_file_identity(&before) != known_hosts_file_identity(&held_after)
        || known_hosts_file_identity(&after) != known_hosts_file_identity(&held_after)
        || before_parents != after_parents
    {
        return KnownHostsCapture::Unsafe;
    }
    KnownHostsCapture::Trusted(first)
}

#[cfg(test)]
fn apply_known_hosts_race() {
    KNOWN_HOSTS_RACE.with(|race| {
        if let Some(race) = race.borrow_mut().take() {
            match race {
                KnownHostsRace::Rewrite(path, bytes) => fs::write(path, bytes).unwrap(),
                KnownHostsRace::Replace(path, bytes) => {
                    fs::remove_file(&path).unwrap();
                    fs::write(path, bytes).unwrap();
                }
            }
        }
    });
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct KnownHostsFileIdentity {
    device: u64,
    inode: u64,
    owner: u32,
    mode: u32,
    links: u64,
    size: u64,
}

fn known_hosts_file_identity(metadata: &fs::Metadata) -> KnownHostsFileIdentity {
    KnownHostsFileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        owner: metadata.uid(),
        mode: metadata.mode(),
        links: metadata.nlink(),
        size: metadata.size(),
    }
}

fn known_hosts_metadata_safe(
    metadata: &fs::Metadata,
    owner: KnownHostsOwner,
    effective_user: u32,
) -> bool {
    let expected_owner = match owner {
        KnownHostsOwner::EffectiveUser => effective_user,
        KnownHostsOwner::Root => 0,
    };
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == expected_owner
        && metadata.nlink() == 1
        && metadata.permissions().mode() & 0o022 == 0
        && metadata.size() <= MAX_KNOWN_HOSTS_BYTES
}

fn read_bounded_known_hosts(file: &mut File) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(MAX_KNOWN_HOSTS_BYTES.checked_add(1)?)
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).ok()? > MAX_KNOWN_HOSTS_BYTES {
        None
    } else {
        Some(bytes)
    }
}

fn safe_provenance_path(path: &Path) -> bool {
    path.is_absolute()
        && !path.as_os_str().as_bytes().is_empty()
        && path.as_os_str().as_bytes().len() <= MAX_SSH_AGENT_PATH_BYTES
        && !path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
}

fn capture_git_ssh_agent_provenance_from(path: Option<&std::ffi::OsStr>) -> GitSshAgentProvenance {
    let Some(path) = path else {
        return GitSshAgentProvenance {
            status: GitSshAgentStatus::NotConfigured,
            _socket: None,
        };
    };
    match inspect_ssh_agent_path(Path::new(path)) {
        Some(socket) => GitSshAgentProvenance {
            status: GitSshAgentStatus::Trusted,
            _socket: Some(socket),
        },
        None => GitSshAgentProvenance {
            status: GitSshAgentStatus::Unsafe,
            _socket: None,
        },
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct SshAgentFileIdentity {
    device: u64,
    inode: u64,
    owner: u32,
    mode: u32,
}

fn ssh_agent_file_identity(metadata: &fs::Metadata) -> SshAgentFileIdentity {
    SshAgentFileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        owner: metadata.uid(),
        mode: metadata.mode(),
    }
}

fn inspect_ssh_agent_path(path: &Path) -> Option<File> {
    if !safe_provenance_path(path) {
        return None;
    }
    let effective_user = unsafe { libc::geteuid() };
    let before_parents = inspect_ssh_agent_parents(path.parent()?, effective_user)?;
    let before = fs::symlink_metadata(path).ok()?;
    if !ssh_agent_socket_metadata_safe(&before, effective_user) {
        return None;
    }
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `path` is NUL-terminated, O_PATH holds the selected inode without
    // reading it, O_NOFOLLOW rejects a final symlink, and ownership/type checks
    // are repeated against the resulting descriptor below.
    let descriptor = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return None;
    }
    // SAFETY: `descriptor` is newly owned after a successful `open` call.
    let socket = unsafe { File::from_raw_fd(descriptor) };
    let held = socket.metadata().ok()?;
    let after =
        fs::symlink_metadata(Path::new(std::ffi::OsStr::from_bytes(path.as_bytes()))).ok()?;
    let after_parents = inspect_ssh_agent_parents(
        Path::new(std::ffi::OsStr::from_bytes(path.as_bytes())).parent()?,
        effective_user,
    )?;
    if !ssh_agent_socket_metadata_safe(&held, effective_user)
        || ssh_agent_file_identity(&before) != ssh_agent_file_identity(&held)
        || ssh_agent_file_identity(&after) != ssh_agent_file_identity(&held)
        || before_parents != after_parents
    {
        return None;
    }
    Some(socket)
}

fn inspect_ssh_agent_parents(
    start: &Path,
    effective_user: u32,
) -> Option<Vec<SshAgentFileIdentity>> {
    let mut identities = Vec::new();
    let mut current = Some(start);
    while let Some(path) = current {
        let metadata = fs::symlink_metadata(path).ok()?;
        let mode = metadata.permissions().mode();
        let owner_safe = metadata.uid() == effective_user && mode & 0o022 == 0;
        let root_safe =
            metadata.uid() == 0 && (mode & 0o022 == 0 || (mode & 0o1000 != 0 && mode & 0o002 != 0));
        if metadata.file_type().is_symlink() || !metadata.is_dir() || (!owner_safe && !root_safe) {
            return None;
        }
        identities.push(ssh_agent_file_identity(&metadata));
        current = path.parent();
    }
    Some(identities)
}

fn ssh_agent_socket_metadata_safe(metadata: &fs::Metadata, effective_user: u32) -> bool {
    metadata.file_type().is_socket()
        && metadata.uid() == effective_user
        && metadata.nlink() == 1
        && metadata.permissions().mode() & 0o022 == 0
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitStatus {
    available: bool,
    version: Option<String>,
    repository: GitRepositoryStatus,
}

impl GitStatus {
    pub fn available(&self) -> bool {
        self.available
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn repository(&self) -> &GitRepositoryStatus {
        &self.repository
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitRepositoryStatus {
    NotRepository,
    Unsafe,
    Ready(GitWorkingTreeStatus),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitWorkingTreeStatus {
    branch: Option<String>,
    detached: bool,
    upstream: Option<String>,
    divergence_known: bool,
    ahead: u32,
    behind: u32,
    changed: u32,
    untracked: u32,
    conflicts: u32,
    remotes: Vec<String>,
    remote_count: u32,
}

impl GitWorkingTreeStatus {
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }
    pub fn detached(&self) -> bool {
        self.detached
    }
    pub fn upstream(&self) -> Option<&str> {
        self.upstream.as_deref()
    }
    pub fn divergence_known(&self) -> bool {
        self.divergence_known
    }
    pub fn ahead(&self) -> u32 {
        self.ahead
    }
    pub fn behind(&self) -> u32 {
        self.behind
    }
    pub fn changed(&self) -> u32 {
        self.changed
    }
    pub fn untracked(&self) -> u32 {
        self.untracked
    }
    pub fn conflicts(&self) -> u32 {
        self.conflicts
    }
    pub fn remotes(&self) -> &[String] {
        &self.remotes
    }
    pub fn remote_count(&self) -> u32 {
        self.remote_count
    }
    pub fn clean(&self) -> bool {
        self.changed == 0 && self.untracked == 0 && self.conflicts == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitReadError {
    Spawn,
    ExitStatus,
    Signal,
    Timeout,
    OutputLimit,
    InvalidOutput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitCommitPlanError {
    Cancelled,
    InvalidMessage,
    InvalidIdentity,
    WorkspaceIssues,
    ReadOnly,
    NotRepository,
    UnsafeRepository,
    UnsupportedRepository,
    OperationInProgress,
    UnsupportedIndex,
    AmbiguousManagedHistory,
    PathCollision,
    NothingToCommit,
    ResourceLimit,
    PlanStale,
    RecoveryNotRequired,
    Io,
}

#[derive(Eq, PartialEq)]
#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
struct GitCommitIdentity {
    name: String,
    email: String,
}

impl fmt::Debug for GitCommitIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitIdentity")
            .field("name", &"<redacted>")
            .field("email", &"<redacted>")
            .finish()
    }
}

#[allow(dead_code, reason = "consumed by Phase 5C1 candidate construction")]
struct GitCommitRepositoryPreflight {
    workspace_id: String,
    capability: crate::storage::GitRepositoryCapability,
    repository: GitCommitRepositorySnapshot,
    branch_storage: crate::storage::GitCommitBranchSnapshot,
    branch: GitCommitBranch,
    identity: GitCommitIdentity,
    object_format: GitObjectFormat,
    head_tree_object_id: Option<String>,
    index: Option<GitIndexSnapshot>,
    tree: GitCommitTree,
    managed: GitCommitManagedInventory,
}

#[allow(dead_code, reason = "consumed by Phase 5C1 candidate construction")]
struct GitCommitPreparedStaging {
    staging: GitCommitStaging,
    prior_managed: GitCommitManagedInventory,
}

#[allow(dead_code, reason = "consumed by the complete Phase 5C1 opaque plan")]
struct GitCommitCandidate {
    prepared: GitCommitPreparedStaging,
    tree_object_id: String,
    commit_object_id: String,
    created: u32,
    updated: u32,
    deleted: u32,
    git_timestamp: String,
}

struct GitCommitPackedCandidate {
    object_ids: Vec<String>,
    sources: Vec<GitClonePackSource>,
    staging_snapshot: GitCommitStagingSnapshot,
}

#[allow(
    dead_code,
    reason = "Phase 5C2B publication consumes the private index source and snapshot"
)]
struct GitCommitIndexCandidate {
    source: GitClonePackSource,
    bytes: Vec<u8>,
    staging_snapshot: GitCommitStagingSnapshot,
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
struct GitSyncCandidate {
    index_bytes: Vec<u8>,
    pack_sources: Vec<GitClonePackSource>,
}

pub struct GitSyncPublicationOutcome {
    #[allow(
        dead_code,
        reason = "retained for internal recovery correlation and tests"
    )]
    operation_id: OperationId,
    changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GitPushOutcome {
    pushed: bool,
}

impl GitPushOutcome {
    pub fn pushed(self) -> bool {
        self.pushed
    }
}

impl fmt::Debug for GitSyncPublicationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitSyncPublicationOutcome")
            .field("changed", &self.changed)
            .field("authority", &"<redacted>")
            .finish()
    }
}

impl GitSyncPublicationOutcome {
    pub fn changed(&self) -> bool {
        self.changed
    }
}

#[allow(
    dead_code,
    reason = "Phase 5C3 returns this pathless result to the native commit workflow"
)]
pub struct GitCommitPublicationOutcome {
    operation_id: OperationId,
    workspace_changed: bool,
}

impl GitCommitPublicationOutcome {
    pub fn workspace_changed(&self) -> bool {
        self.workspace_changed
    }
}

impl fmt::Debug for GitCommitPublicationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitPublicationOutcome")
            .field("operation_id", &"<redacted>")
            .field("workspace_changed", &self.workspace_changed)
            .finish()
    }
}

impl fmt::Debug for GitCommitIndexCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitIndexCandidate")
            .field("bytes", &format_args!("<redacted:{}>", self.bytes.len()))
            .field("source", &"<redacted>")
            .field("staging_snapshot", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for GitCommitPackedCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitPackedCandidate")
            .field("objects", &self.object_ids.len())
            .field("sources", &self.sources.len())
            .field("staging_snapshot", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for GitCommitCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitCandidate")
            .field("prepared", &"<redacted>")
            .field("tree_object_id", &"<redacted>")
            .field("commit_object_id", &"<redacted>")
            .field("created", &self.created)
            .field("updated", &self.updated)
            .field("deleted", &self.deleted)
            .field("git_timestamp", &"<redacted>")
            .finish()
    }
}

#[allow(
    dead_code,
    reason = "consumed by Phase 5C2 apply and native confirmation"
)]
pub struct GitCommitPlan {
    token: PlanToken,
    branch: String,
    created: u32,
    updated: u32,
    deleted: u32,
    unrelated_changes: u32,
    message: String,
    staging_snapshot: GitCommitStagingSnapshot,
    preflight: GitCommitRepositoryPreflight,
    candidate: GitCommitCandidate,
}

pub struct GitCommitRecoveryPlan {
    token: PlanToken,
    operation_id: OperationId,
}

impl GitCommitRecoveryPlan {
    pub fn token(&self) -> &PlanToken {
        &self.token
    }
}

impl fmt::Debug for GitCommitRecoveryPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitRecoveryPlan")
            .field("token", &"<redacted>")
            .field("operation_id", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for GitCommitPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitPlan")
            .field("token", &"<redacted>")
            .field("branch", &self.branch)
            .field("created", &self.created)
            .field("updated", &self.updated)
            .field("deleted", &self.deleted)
            .field("unrelated_changes", &self.unrelated_changes)
            .field("message", &"<redacted>")
            .field("staging_snapshot", &"<redacted>")
            .field("preflight", &"<redacted>")
            .field("candidate", &"<redacted>")
            .finish()
    }
}

impl GitCommitPlan {
    pub fn token(&self) -> &PlanToken {
        &self.token
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn created(&self) -> u32 {
        self.created
    }

    pub fn updated(&self) -> u32 {
        self.updated
    }

    pub fn deleted(&self) -> u32 {
        self.deleted
    }

    pub fn unrelated_changes(&self) -> u32 {
        self.unrelated_changes
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

struct GitCandidateTreeChild {
    mode: u32,
    kind: GitCommitTreeEntryKind,
    object_id: String,
    name: Vec<u8>,
}

#[derive(Default)]
struct GitCandidateDirectory {
    children: std::collections::BTreeMap<Vec<u8>, GitCandidateTreeChild>,
    preserve_if_empty: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitCommitManagedKind {
    Manifest,
    GitIgnore,
    Category,
    Guide,
    Attachment,
}

enum GitCommitManagedExpectation {
    Document(Revision),
    Attachment {
        sha256: String,
        declared_size: Option<u64>,
    },
}

#[derive(Eq, PartialEq)]
struct GitCommitManagedFile {
    path: String,
    kind: GitCommitManagedKind,
    size: u64,
    revision: Revision,
    mode: u32,
    blob_object_id: String,
}

impl fmt::Debug for GitCommitManagedFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitManagedFile")
            .field("path", &"<redacted>")
            .field("kind", &self.kind)
            .field("size", &self.size)
            .field("revision", &"<redacted>")
            .field("mode", &format_args!("{:o}", self.mode))
            .field("blob_object_id", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Eq, PartialEq)]
struct GitCommitManagedInventory {
    files: Vec<GitCommitManagedFile>,
}

impl fmt::Debug for GitCommitRepositoryPreflight {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitRepositoryPreflight")
            .field("workspace_id", &"<redacted>")
            .field("capability", &"<redacted>")
            .field("repository", &"<redacted>")
            .field("branch_storage", &"<redacted>")
            .field("branch", &self.branch)
            .field("identity", &self.identity)
            .field("object_format", &self.object_format)
            .field("head_tree_object_id", &"<redacted>")
            .field(
                "index_entries",
                &self.index.as_ref().map_or(0, |index| index.entries.len()),
            )
            .field("tree_entries", &self.tree.entries.len())
            .field("managed_files", &self.managed.files.len())
            .finish()
    }
}

#[derive(Debug)]
pub struct GitInitPlan {
    token: PlanToken,
    workspace_id: String,
    manifest_revision: Revision,
    root_capability_hash: [u8; 32],
}

impl GitInitPlan {
    pub fn token(&self) -> &PlanToken {
        &self.token
    }

    pub fn initial_branch(&self) -> &'static str {
        "main"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitInitError {
    AlreadyRepository,
    PlanStale,
    ReadOnly,
    NestedRepository,
    Unsafe,
    Unsupported,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitRemoteTransport {
    Https,
    Ssh,
}

impl GitRemoteTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Ssh => "ssh",
        }
    }
}

#[derive(Clone, Copy)]
pub enum GitRemoteAuthentication<'a> {
    Anonymous,
    Https(&'a GitHttpsCredentialResources),
    Ssh(&'a GitSshTransportResources),
}

impl fmt::Debug for GitRemoteAuthentication<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Anonymous => "anonymous",
            Self::Https(_) => "https",
            Self::Ssh(_) => "ssh",
        };
        formatter
            .debug_struct("GitRemoteAuthentication")
            .field("kind", &kind)
            .finish()
    }
}

impl<'a> GitRemoteAuthentication<'a> {
    fn supports(self, transport: GitRemoteTransport) -> bool {
        matches!(
            (transport, self),
            (GitRemoteTransport::Https, Self::Anonymous | Self::Https(_))
                | (GitRemoteTransport::Ssh, Self::Ssh(_))
        )
    }

    fn ssh(self) -> Option<&'a GitSshTransportResources> {
        match self {
            Self::Ssh(resources) => Some(resources),
            Self::Anonymous | Self::Https(_) => None,
        }
    }

    fn https(self) -> Option<&'a GitHttpsCredentialResources> {
        match self {
            Self::Https(resources) => Some(resources),
            Self::Anonymous | Self::Ssh(_) => None,
        }
    }
}

pub struct GitRemotePlan {
    token: PlanToken,
    workspace_id: String,
    manifest_revision: Revision,
    root_capability_hash: [u8; 32],
    remote_name: String,
    remote_url: String,
    transport: GitRemoteTransport,
    host: String,
    expected_config: Vec<u8>,
    candidate_config: Vec<u8>,
    config_revision: [u8; 32],
    candidate_hash: [u8; 32],
    config_mode: u32,
}

impl fmt::Debug for GitRemotePlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitRemotePlan")
            .field("token", &"<redacted>")
            .field("workspace_id", &self.workspace_id)
            .field("manifest_revision", &self.manifest_revision)
            .field("root_capability_hash", &"<redacted>")
            .field("remote_name", &self.remote_name)
            .field("remote_url", &"<redacted>")
            .field("transport", &self.transport)
            .field("host", &self.host)
            .field("expected_config", &"<redacted>")
            .field("candidate_config", &"<redacted>")
            .field("config_revision", &"<redacted>")
            .field("candidate_hash", &"<redacted>")
            .field("config_mode", &self.config_mode)
            .finish()
    }
}

impl GitRemotePlan {
    pub fn token(&self) -> &PlanToken {
        &self.token
    }

    pub fn remote_name(&self) -> &str {
        &self.remote_name
    }

    pub fn remote_url(&self) -> &str {
        &self.remote_url
    }

    pub fn transport(&self) -> GitRemoteTransport {
        self.transport
    }

    pub fn host(&self) -> &str {
        &self.host
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitRemoteError {
    InvalidName,
    InvalidUrl,
    NotRepository,
    DuplicateRemote,
    MalformedConfig,
    RecoveryRequired,
    PlanStale,
    Unsafe,
    TooLarge,
    Unsupported,
    GitUnavailable,
    Io,
}

pub struct GitClonePlan {
    token: PlanToken,
    remote_url: String,
    transport: GitRemoteTransport,
    host: String,
    destination: CloneDestinationSnapshot,
    staging_id: [u8; 32],
}

impl fmt::Debug for GitClonePlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitClonePlan")
            .field("token", &"<redacted>")
            .field("remote_url", &"<redacted>")
            .field("transport", &self.transport)
            .field("host", &self.host)
            .field("destination", &"<redacted>")
            .field("staging_id", &"<redacted>")
            .finish()
    }
}

impl GitClonePlan {
    pub fn token(&self) -> &PlanToken {
        &self.token
    }

    pub fn remote_url(&self) -> &str {
        &self.remote_url
    }

    pub fn transport(&self) -> GitRemoteTransport {
        self.transport
    }

    pub fn host(&self) -> &str {
        &self.host
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitCloneError {
    InvalidUrl,
    DestinationChanged,
    ReadOnly,
    NestedRepository,
    Unsafe,
    Io,
}

pub fn plan_git_clone(
    destination: &EmptyWorkspaceTarget,
    remote_url: &str,
) -> Result<GitClonePlan, GitCloneError> {
    let url = validate_remote_url(remote_url).map_err(|_| GitCloneError::InvalidUrl)?;
    let snapshot = destination
        .clone_destination_snapshot()
        .map_err(map_clone_inspection)?;
    validate_clone_destination_facts(snapshot)?;
    let mut random = [0_u8; 32];
    fill_random(&mut random).map_err(|_| GitCloneError::Io)?;
    let mut staging_id = [0_u8; 32];
    fill_random(&mut staging_id).map_err(|_| GitCloneError::Io)?;
    Ok(GitClonePlan {
        token: PlanToken::from_random_bytes(random),
        remote_url: url.canonical,
        transport: url.transport,
        host: url.host,
        destination: snapshot,
        staging_id,
    })
}

pub fn revalidate_git_clone_plan(
    destination: &EmptyWorkspaceTarget,
    plan: &GitClonePlan,
) -> Result<(), GitCloneError> {
    let current = destination
        .clone_destination_snapshot()
        .map_err(map_clone_inspection)?;
    validate_clone_destination_facts(current)?;
    if current.device() != plan.destination.device()
        || current.inode() != plan.destination.inode()
        || current.root_hash() != plan.destination.root_hash()
        || current.capability_hash() != plan.destination.capability_hash()
    {
        return Err(GitCloneError::DestinationChanged);
    }
    Ok(())
}

fn validate_clone_destination_facts(
    snapshot: CloneDestinationSnapshot,
) -> Result<(), GitCloneError> {
    let facts = snapshot.facts();
    if !facts.git_probe_complete() || facts.unsafe_git_marker() || facts.existing_git_repository() {
        return Err(GitCloneError::Unsafe);
    }
    if facts.nested_git_repository() {
        return Err(GitCloneError::NestedRepository);
    }
    if !facts.writable() {
        return Err(GitCloneError::ReadOnly);
    }
    Ok(())
}

fn map_clone_inspection(error: crate::storage::WorkspaceInspectError) -> GitCloneError {
    match error {
        crate::storage::WorkspaceInspectError::ChangedDuringRead => {
            GitCloneError::DestinationChanged
        }
        crate::storage::WorkspaceInspectError::UnsafeRoot
        | crate::storage::WorkspaceInspectError::ResourceLimit => GitCloneError::Unsafe,
        crate::storage::WorkspaceInspectError::NotFound
        | crate::storage::WorkspaceInspectError::NotDirectory
        | crate::storage::WorkspaceInspectError::PermissionDenied
        | crate::storage::WorkspaceInspectError::Io => GitCloneError::Io,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) enum GitCloneStageError {
    DestinationChanged,
    Conflict,
    Unsafe,
    Unsupported,
    ResourceLimit,
    Io,
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) fn prepare_git_clone_staging(
    destination: &EmptyWorkspaceTarget,
    plan: &GitClonePlan,
    remote_head: &GitRemoteHead,
    state_home: &Path,
) -> Result<GitCloneStaging, GitCloneStageError> {
    revalidate_git_clone_plan(destination, plan).map_err(|error| match error {
        GitCloneError::DestinationChanged => GitCloneStageError::DestinationChanged,
        GitCloneError::ReadOnly
        | GitCloneError::NestedRepository
        | GitCloneError::Unsafe
        | GitCloneError::InvalidUrl => GitCloneStageError::Unsafe,
        GitCloneError::Io => GitCloneStageError::Io,
    })?;
    let stage_name = hex_lower(&plan.staging_id);
    let provenance = clone_stage_provenance(plan, remote_head);
    let config: &[u8] = match remote_head.object_format {
        GitObjectFormat::Sha1 => {
            b"[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = true\n"
        }
        GitObjectFormat::Sha256 => {
            b"[core]\n\trepositoryformatversion = 1\n\tfilemode = true\n\tbare = true\n[extensions]\n\tobjectformat = sha256\n"
        }
    };
    create_git_clone_staging(state_home, &stage_name, provenance.as_bytes(), config).map_err(
        |error| match error {
            GitCloneStagingError::Conflict => GitCloneStageError::Conflict,
            GitCloneStagingError::Unsafe => GitCloneStageError::Unsafe,
            GitCloneStagingError::Unsupported => GitCloneStageError::Unsupported,
            GitCloneStagingError::TooLarge => GitCloneStageError::ResourceLimit,
            GitCloneStagingError::Io => GitCloneStageError::Io,
        },
    )
}

fn clone_stage_provenance(plan: &GitClonePlan, remote_head: &GitRemoteHead) -> String {
    let mut url_hasher = blake3::Hasher::new();
    url_hasher.update(b"rehow:clone-url:v1\0");
    hash_clone_field(&mut url_hasher, plan.remote_url.as_bytes());
    let url_hash = *url_hasher.finalize().as_bytes();
    let object_format = match remote_head.object_format {
        GitObjectFormat::Sha1 => "sha1",
        GitObjectFormat::Sha256 => "sha256",
    };
    format!(
        "rehow-clone-stage-v1\ndestination_root={}\ndestination_device={}\ndestination_inode={}\ndestination_capability={}\nurl_hash={}\nobject_format={object_format}\nbranch_ref={}\nobject_id={}\nstage=prepared\n",
        hex_lower(&plan.destination.root_hash()),
        plan.destination.device(),
        plan.destination.inode(),
        hex_lower(&plan.destination.capability_hash()),
        hex_lower(&url_hash),
        remote_head.branch_ref,
        remote_head.object_id,
    )
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) enum GitCloneDiscoveryError {
    UnsupportedTransport,
    GitUnavailable,
    SandboxUnavailable,
    Cancelled,
    Timeout,
    OutputLimit,
    RemoteUnavailable,
    AuthenticationFailed,
    HostKeyRejected,
    RemoteRefMissing,
    InvalidAdvertisement,
    Io,
}

#[derive(Clone, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) struct GitRemoteHead {
    branch_ref: String,
    object_id: String,
    object_format: GitObjectFormat,
}

impl fmt::Debug for GitRemoteHead {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitRemoteHead")
            .field("branch_ref", &"<redacted>")
            .field("object_id", &"<redacted>")
            .field("object_format", &self.object_format)
            .finish()
    }
}

impl GitRemoteHead {
    #[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
    pub(crate) fn branch_ref(&self) -> &str {
        &self.branch_ref
    }

    #[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    #[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
    pub(crate) fn object_format(&self) -> GitObjectFormat {
        self.object_format
    }
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) fn discover_git_clone_remote_head(
    plan: &GitClonePlan,
    cancelled: &AtomicBool,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<GitRemoteHead, GitCloneDiscoveryError> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(GitCloneDiscoveryError::Cancelled);
    }
    if !authentication.supports(plan.transport) {
        return Err(GitCloneDiscoveryError::UnsupportedTransport);
    }
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCloneDiscoveryError::GitUnavailable)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCloneDiscoveryError::SandboxUnavailable)?;
    let command = clone_discovery_command_with_transport(
        &executable,
        &sandbox,
        &plan.remote_url,
        plan.transport,
        authentication,
    );
    let output = run_clone_command_for_transport(
        command,
        cancelled,
        CLONE_DISCOVERY_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        plan.transport,
        authentication,
    )?;
    parse_git_remote_head(&output.stdout)
}

fn discover_git_sync_remote_head(
    plan: &GitSyncPlan,
    cancelled: &AtomicBool,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<GitRemoteHead, GitSyncError> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(GitSyncError::Cancelled);
    }
    if !authentication.supports(plan.transport) {
        return Err(GitSyncError::UnsupportedTransport);
    }
    let executable = trusted_executable("/usr/bin/git").ok_or(GitSyncError::GitUnavailable)?;
    let sandbox = trusted_executable("/usr/bin/bwrap").ok_or(GitSyncError::SandboxUnavailable)?;
    let output = run_clone_command_for_transport_with_missing_ref(
        sync_discovery_command(
            &executable,
            &sandbox,
            &plan.remote_url,
            &plan.remote_ref,
            plan.transport,
            authentication,
        ),
        cancelled,
        CLONE_DISCOVERY_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        plan.transport,
        authentication,
    )
    .map_err(map_clone_discovery_to_sync)?;
    parse_git_sync_remote_ref(&output.stdout, &plan.remote_ref)
}

fn prepare_git_sync_staging(
    plan: &GitSyncPlan,
    remote_head: &GitRemoteHead,
    state_home: &Path,
) -> Result<GitCloneStaging, GitSyncError> {
    let stage_name = hex_lower(&plan.staging_id);
    let mut url_hasher = blake3::Hasher::new();
    url_hasher.update(b"rehow:sync-url:v1\0");
    hash_clone_field(&mut url_hasher, plan.remote_url.as_bytes());
    let object_format = match remote_head.object_format {
        GitObjectFormat::Sha1 => "sha1",
        GitObjectFormat::Sha256 => "sha256",
    };
    let local_head = plan
        .preflight
        .branch
        .expected_object_id
        .as_deref()
        .ok_or(GitSyncError::Unsupported)?;
    let provenance = format!(
        "rehow-sync-stage-v1\nworkspace_id={}\nroot_capability={}\ngit_device={}\ngit_inode={}\nurl_hash={}\nobject_format={object_format}\nremote_ref={}\nremote_object={}\nlocal_object={}\nstage=validated-fetch\n",
        plan.preflight.workspace_id,
        hex_lower(&plan.preflight.root_capability_hash),
        plan.preflight.git_identity.0,
        plan.preflight.git_identity.1,
        hex_lower(url_hasher.finalize().as_bytes()),
        plan.remote_ref,
        remote_head.object_id,
        local_head,
    );
    let config: &[u8] = match remote_head.object_format {
        GitObjectFormat::Sha1 => {
            b"[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = true\n"
        }
        GitObjectFormat::Sha256 => {
            b"[core]\n\trepositoryformatversion = 1\n\tfilemode = true\n\tbare = true\n[extensions]\n\tobjectformat = sha256\n"
        }
    };
    create_git_clone_staging(state_home, &stage_name, provenance.as_bytes(), config).map_err(
        |error| match error {
            GitCloneStagingError::Conflict => GitSyncError::StagingConflict,
            GitCloneStagingError::Unsafe => GitSyncError::Unsafe,
            GitCloneStagingError::Unsupported => GitSyncError::Unsupported,
            GitCloneStagingError::TooLarge => GitSyncError::ResourceLimit,
            GitCloneStagingError::Io => GitSyncError::Io,
        },
    )
}

fn fetch_git_sync_remote(
    plan: &GitSyncPlan,
    remote_head: &GitRemoteHead,
    staging: &GitCloneStaging,
    cancelled: &AtomicBool,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<(), GitSyncError> {
    if !authentication.supports(plan.transport) {
        return Err(GitSyncError::UnsupportedTransport);
    }
    let executable = trusted_executable("/usr/bin/git").ok_or(GitSyncError::GitUnavailable)?;
    let sandbox = trusted_executable("/usr/bin/bwrap").ok_or(GitSyncError::SandboxUnavailable)?;
    let output = run_clone_command_for_transport(
        clone_fetch_command_with_transport(
            &executable,
            &sandbox,
            staging.git_path(),
            staging.git_descriptor(),
            &plan.remote_url,
            remote_head.object_id(),
            plan.transport,
            authentication,
        ),
        cancelled,
        CLONE_FETCH_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        plan.transport,
        authentication,
    )
    .map_err(map_clone_discovery_to_sync)?;
    if !output.stdout.is_empty() {
        return Err(GitSyncError::InvalidRepository);
    }
    staging
        .verify_git_size_limit(CLONE_STAGING_FILE_LIMIT)
        .map_err(|error| match error {
            GitCloneStagingError::TooLarge => GitSyncError::ResourceLimit,
            GitCloneStagingError::Conflict => GitSyncError::StagingConflict,
            GitCloneStagingError::Unsafe => GitSyncError::Unsafe,
            GitCloneStagingError::Unsupported => GitSyncError::Unsupported,
            GitCloneStagingError::Io => GitSyncError::Io,
        })
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) fn fetch_git_clone_remote(
    plan: &GitClonePlan,
    remote_head: &GitRemoteHead,
    staging: &GitCloneStaging,
    cancelled: &AtomicBool,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<(), GitCloneDiscoveryError> {
    if !authentication.supports(plan.transport) {
        return Err(GitCloneDiscoveryError::UnsupportedTransport);
    }
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCloneDiscoveryError::GitUnavailable)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCloneDiscoveryError::SandboxUnavailable)?;
    let command = clone_fetch_command_with_transport(
        &executable,
        &sandbox,
        staging.git_path(),
        staging.git_descriptor(),
        &plan.remote_url,
        &remote_head.object_id,
        plan.transport,
        authentication,
    );
    let output = run_clone_command_for_transport(
        command,
        cancelled,
        CLONE_FETCH_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        plan.transport,
        authentication,
    )?;
    if !output.stdout.is_empty() {
        return Err(GitCloneDiscoveryError::InvalidAdvertisement);
    }
    staging
        .verify_git_size_limit(CLONE_STAGING_FILE_LIMIT)
        .map_err(|error| match error {
            GitCloneStagingError::TooLarge => GitCloneDiscoveryError::OutputLimit,
            GitCloneStagingError::Conflict
            | GitCloneStagingError::Unsafe
            | GitCloneStagingError::Unsupported
            | GitCloneStagingError::Io => GitCloneDiscoveryError::Io,
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) enum GitCloneInspectError {
    Cancelled,
    Timeout,
    OutputLimit,
    GitUnavailable,
    SandboxUnavailable,
    InvalidRepository,
    RefMismatch,
    Tree(GitCloneTreeError),
    Io,
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) fn inspect_fetched_git_clone_tree(
    staging: &GitCloneStaging,
    remote_head: &GitRemoteHead,
    cancelled: &AtomicBool,
) -> Result<GitCloneTree, GitCloneInspectError> {
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCloneInspectError::GitUnavailable)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCloneInspectError::SandboxUnavailable)?;
    let ref_output = run_clone_command(
        clone_read_command(
            &executable,
            &sandbox,
            staging.git_path(),
            staging.git_descriptor(),
            &["show-ref", "--verify", "--hash", "refs/rehow/clone/head"],
        ),
        cancelled,
        CLONE_LOCAL_READ_TIMEOUT,
        128,
    )
    .map_err(map_clone_inspect_process)?;
    let expected_ref = format!("{}\n", remote_head.object_id);
    if ref_output.stdout != expected_ref.as_bytes() {
        return Err(GitCloneInspectError::RefMismatch);
    }
    let type_output = run_clone_command(
        clone_read_command(
            &executable,
            &sandbox,
            staging.git_path(),
            staging.git_descriptor(),
            &["cat-file", "-t", remote_head.object_id()],
        ),
        cancelled,
        CLONE_LOCAL_READ_TIMEOUT,
        16,
    )
    .map_err(map_clone_inspect_process)?;
    if type_output.stdout != b"commit\n" {
        return Err(GitCloneInspectError::InvalidRepository);
    }
    let tree_output = run_clone_command(
        clone_read_command(
            &executable,
            &sandbox,
            staging.git_path(),
            staging.git_descriptor(),
            &[
                "ls-tree",
                "-r",
                "-l",
                "-z",
                "--full-tree",
                remote_head.object_id(),
            ],
        ),
        cancelled,
        CLONE_LOCAL_READ_TIMEOUT,
        MAX_CLONE_TREE_LISTING_BYTES,
    )
    .map_err(map_clone_inspect_process)?;
    validate_git_clone_tree(remote_head.object_id(), &tree_output.stdout)
        .map_err(GitCloneInspectError::Tree)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the Phase 5B3B clone apply gate")]
pub(crate) enum GitCloneContentError {
    Cancelled,
    Timeout,
    OutputLimit,
    GitUnavailable,
    SandboxUnavailable,
    InvalidRepository,
    BlobMismatch,
    InvalidWorkspace,
    WorkspaceIssues,
    Conflict,
    Unsafe,
    Unsupported,
    ResourceLimit,
    Io,
}

#[allow(dead_code, reason = "connected by the Phase 5B3B clone apply gate")]
pub(crate) struct ValidatedGitClone {
    tree: GitCloneTree,
    workspace: Box<WorkspaceView>,
}

/// Opaque native-held clone state that has passed every pre-materialization
/// validation gate. It deliberately exposes no URL, path, ref, OID, or content.
pub struct GitCloneValidation {
    #[allow(dead_code, reason = "consumed by the Phase 5B3C materialization gate")]
    plan: GitClonePlan,
    #[allow(dead_code, reason = "consumed by the Phase 5B3C materialization gate")]
    staging: GitCloneStaging,
    #[allow(dead_code, reason = "consumed by the Phase 5B3C materialization gate")]
    remote_head: GitRemoteHead,
    #[allow(dead_code, reason = "consumed by the Phase 5B3C materialization gate")]
    validated: ValidatedGitClone,
    #[allow(
        dead_code,
        reason = "consumed by the Phase 5B3C2 native finalization gate"
    )]
    operation_id: OperationId,
    #[allow(
        dead_code,
        reason = "consumed by the Phase 5B3C2 native finalization gate"
    )]
    created_at: Timestamp,
}

impl fmt::Debug for GitCloneValidation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCloneValidation")
            .field("state", &"<validated>")
            .finish()
    }
}

impl GitCloneValidation {
    pub fn remote_url(&self) -> &str {
        self.plan.remote_url()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitCloneApplyError {
    UnsupportedTransport,
    DestinationChanged,
    Cancelled,
    Timeout,
    ResourceLimit,
    GitUnavailable,
    SandboxUnavailable,
    RemoteUnavailable,
    AuthenticationFailed,
    HostKeyRejected,
    InvalidRepository,
    InvalidWorkspace,
    WorkspaceIssues,
    StagingConflict,
    Unsafe,
    Unsupported,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitCloneMaterializeError {
    DestinationChanged,
    StagingChanged,
    Busy,
    Conflict,
    Unsafe,
    Unsupported,
    ResourceLimit,
    ValidationMismatch,
    Io,
}

/// Opaque, single-use input to an isolated remote synchronization inspection.
/// URLs, object IDs, filesystem authority, and repository snapshots remain
/// core-private.
pub struct GitSyncPlan {
    token: PlanToken,
    remote_name: String,
    remote_url: String,
    transport: GitRemoteTransport,
    host: String,
    remote_branch: String,
    remote_ref: String,
    staging_id: [u8; 32],
    preflight: GitSyncRepositoryPreflight,
}

impl fmt::Debug for GitSyncPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitSyncPlan")
            .field("token", &"<redacted>")
            .field("remote_name", &self.remote_name)
            .field("remote_url", &"<redacted>")
            .field("transport", &self.transport)
            .field("host", &self.host)
            .field("remote_branch", &self.remote_branch)
            .field("remote_ref", &"<redacted>")
            .field("staging_id", &"<redacted>")
            .field("preflight", &"<redacted>")
            .finish()
    }
}

impl GitSyncPlan {
    pub fn token(&self) -> &PlanToken {
        &self.token
    }

    pub fn remote_name(&self) -> &str {
        &self.remote_name
    }

    pub fn transport(&self) -> GitRemoteTransport {
        self.transport
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn remote_branch(&self) -> &str {
        &self.remote_branch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitSyncInspectionState {
    UpToDate,
    RemoteChanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitSyncAncestry {
    UpToDate,
    LocalAhead,
    RemoteFastForward,
    Diverged,
    HistoryLimit,
}

pub struct GitSyncClassification {
    inspection: GitSyncInspection,
    ancestry: GitSyncAncestry,
}

#[allow(
    dead_code,
    reason = "opaque authority is consumed by the following Git metadata publication gate"
)]
pub(crate) struct GitSyncWorkspaceMaterialization {
    classification: GitSyncClassification,
    operation_id: OperationId,
    created_at: Timestamp,
    plan_hash: [u8; 32],
    changed: bool,
}

impl fmt::Debug for GitSyncWorkspaceMaterialization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitSyncWorkspaceMaterialization")
            .field("changed", &self.changed)
            .field("authority", &"<redacted>")
            .finish()
    }
}

#[allow(
    dead_code,
    reason = "consumed with the authority by the following Git metadata publication gate"
)]
impl GitSyncWorkspaceMaterialization {
    pub(crate) fn changed(&self) -> bool {
        self.changed
    }
}

impl fmt::Debug for GitSyncClassification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitSyncClassification")
            .field("ancestry", &self.ancestry)
            .field("authority", &"<redacted>")
            .finish()
    }
}

impl GitSyncClassification {
    pub fn token(&self) -> &PlanToken {
        self.inspection.plan.token()
    }

    pub fn remote_name(&self) -> &str {
        self.inspection.plan.remote_name()
    }

    pub fn transport(&self) -> GitRemoteTransport {
        self.inspection.plan.transport()
    }

    pub fn host(&self) -> &str {
        self.inspection.plan.host()
    }

    pub fn remote_branch(&self) -> &str {
        self.inspection.plan.remote_branch()
    }

    pub fn ancestry(&self) -> GitSyncAncestry {
        self.ancestry
    }

    pub fn local_changes(&self) -> bool {
        self.inspection.local_changes()
    }

    pub fn workspace_matches_remote(&self) -> bool {
        self.inspection.workspace_matches_remote()
    }
}

/// Validated, native-held incoming state for the later materialization/ref-CAS
/// gate. Debug and public accessors deliberately expose no content or Git IDs.
pub struct GitSyncInspection {
    #[allow(dead_code, reason = "consumed by the Phase 5D2 integration gate")]
    plan: GitSyncPlan,
    #[allow(dead_code, reason = "consumed by the Phase 5D2 integration gate")]
    staging: GitCloneStaging,
    #[allow(dead_code, reason = "consumed by the Phase 5D2 integration gate")]
    remote_head: GitRemoteHead,
    #[allow(dead_code, reason = "consumed by the Phase 5D2 integration gate")]
    validated: ValidatedGitClone,
    state: GitSyncInspectionState,
    local_changes: bool,
    workspace_matches_remote: bool,
}

impl fmt::Debug for GitSyncInspection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitSyncInspection")
            .field("state", &self.state)
            .field("local_changes", &self.local_changes)
            .field("workspace_matches_remote", &self.workspace_matches_remote)
            .field("authority", &"<redacted>")
            .finish()
    }
}

impl GitSyncInspection {
    pub fn state(&self) -> GitSyncInspectionState {
        self.state
    }

    pub fn local_changes(&self) -> bool {
        self.local_changes
    }

    pub fn workspace_matches_remote(&self) -> bool {
        self.workspace_matches_remote
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitSyncError {
    NotRepository,
    NoUpstream,
    InvalidConfiguration,
    UnsupportedTransport,
    RecoveryRequired,
    ReadOnly,
    WorkspaceIssues,
    PlanStale,
    Cancelled,
    Timeout,
    ResourceLimit,
    GitUnavailable,
    SandboxUnavailable,
    RemoteUnavailable,
    AuthenticationFailed,
    HostKeyRejected,
    RemoteChanged,
    RemoteStateUncertain,
    InvalidRepository,
    InvalidWorkspace,
    StagingConflict,
    Unsafe,
    Unsupported,
    Io,
}

struct GitSyncRepositoryPreflight {
    workspace_id: String,
    workspace_fingerprint: [u8; 32],
    manifest_revision: Revision,
    root_capability_hash: [u8; 32],
    git_identity: (u64, u64),
    repository: GitCommitRepositorySnapshot,
    branch_storage: crate::storage::GitCommitBranchSnapshot,
    branch: GitCommitBranch,
    object_format: GitObjectFormat,
    managed: GitCommitManagedInventory,
    working_tree_clean: bool,
}

impl fmt::Debug for GitSyncRepositoryPreflight {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitSyncRepositoryPreflight")
            .field("workspace", &"<redacted>")
            .field("repository", &"<redacted>")
            .field("branch", &self.branch.reference)
            .field("object_format", &self.object_format)
            .field("managed_files", &self.managed.files.len())
            .field("working_tree_clean", &self.working_tree_clean)
            .finish()
    }
}

/// Runs the complete pre-materialization clone pipeline. The destination is
/// revalidated before and after the operation and is never bound into a child
/// process or written by this function.
pub fn apply_git_clone_validation(
    destination: &EmptyWorkspaceTarget,
    plan: GitClonePlan,
    state_home: &Path,
    cancelled: &AtomicBool,
) -> Result<GitCloneValidation, GitCloneApplyError> {
    apply_git_clone_validation_with_authentication(
        destination,
        plan,
        state_home,
        cancelled,
        GitRemoteAuthentication::Anonymous,
    )
}

/// Runs clone validation with optional launch-captured SSH authority. HTTPS
/// ignores the authority; SSH fails closed when it is absent.
pub fn apply_git_clone_validation_with_ssh(
    destination: &EmptyWorkspaceTarget,
    plan: GitClonePlan,
    state_home: &Path,
    cancelled: &AtomicBool,
    ssh: Option<&GitSshTransportResources>,
) -> Result<GitCloneValidation, GitCloneApplyError> {
    let authentication = if plan.transport == GitRemoteTransport::Ssh {
        ssh.map_or(
            GitRemoteAuthentication::Anonymous,
            GitRemoteAuthentication::Ssh,
        )
    } else {
        GitRemoteAuthentication::Anonymous
    };
    apply_git_clone_validation_with_authentication(
        destination,
        plan,
        state_home,
        cancelled,
        authentication,
    )
}

pub fn apply_git_clone_validation_with_authentication(
    destination: &EmptyWorkspaceTarget,
    plan: GitClonePlan,
    state_home: &Path,
    cancelled: &AtomicBool,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<GitCloneValidation, GitCloneApplyError> {
    if !authentication.supports(plan.transport) {
        return Err(GitCloneApplyError::UnsupportedTransport);
    }
    revalidate_git_clone_plan(destination, &plan).map_err(map_clone_apply_destination)?;
    let result = (|| {
        let remote_head = discover_git_clone_remote_head(&plan, cancelled, authentication)
            .map_err(map_clone_apply_discovery)?;
        let staging = prepare_git_clone_staging(destination, &plan, &remote_head, state_home)
            .map_err(map_clone_apply_staging)?;
        fetch_git_clone_remote(&plan, &remote_head, &staging, cancelled, authentication)
            .map_err(map_clone_apply_discovery)?;
        let tree = inspect_fetched_git_clone_tree(&staging, &remote_head, cancelled)
            .map_err(map_clone_apply_inspection)?;
        let validated = validate_fetched_git_clone(&staging, tree, cancelled)
            .map_err(map_clone_apply_content)?;
        Ok((staging, remote_head, validated))
    })();
    match result {
        Ok((staging, remote_head, validated)) => {
            revalidate_git_clone_plan(destination, &plan)
                .map_err(|_| GitCloneApplyError::DestinationChanged)?;
            Ok(GitCloneValidation {
                plan,
                staging,
                remote_head,
                validated,
                operation_id: new_operation_id(),
                created_at: current_timestamp(),
            })
        }
        Err(error) => {
            revalidate_git_clone_plan(destination, &plan)
                .map_err(|_| GitCloneApplyError::DestinationChanged)?;
            Err(error)
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct GitSyncTarget {
    remote_name: String,
    remote_url: String,
    transport: GitRemoteTransport,
    host: String,
    remote_branch: String,
    remote_ref: String,
}

pub fn plan_git_sync(
    workspace: &WorkspaceView,
    state_home: &Path,
) -> Result<GitSyncPlan, GitSyncError> {
    if git_commit_recovery_required(workspace, state_home, None).map_err(map_commit_plan_to_sync)?
        || pending_git_sync_recovery_at(workspace, state_home, None)
            .map_err(map_commit_plan_to_sync)?
            .is_some()
    {
        return Err(GitSyncError::RecoveryRequired);
    }
    let (preflight, target) = inspect_git_sync_repository(workspace)?;
    let mut random = [0_u8; 32];
    fill_random(&mut random).map_err(|_| GitSyncError::Io)?;
    let mut staging_id = [0_u8; 32];
    fill_random(&mut staging_id).map_err(|_| GitSyncError::Io)?;
    Ok(GitSyncPlan {
        token: PlanToken::from_random_bytes(random),
        remote_name: target.remote_name,
        remote_url: target.remote_url,
        transport: target.transport,
        host: target.host,
        remote_branch: target.remote_branch,
        remote_ref: target.remote_ref,
        staging_id,
        preflight,
    })
}

/// Fetches the configured upstream into private XDG staging and validates its
/// entire portable Workspace snapshot. It never binds the active Workspace to
/// the Git child and never changes active objects, refs, index, or content.
pub fn apply_git_sync_inspection(
    workspace: &WorkspaceView,
    state_home: &Path,
    plan: GitSyncPlan,
    cancelled: &AtomicBool,
) -> Result<GitSyncInspection, GitSyncError> {
    apply_git_sync_inspection_with_authentication(
        workspace,
        state_home,
        plan,
        cancelled,
        GitRemoteAuthentication::Anonymous,
    )
}

/// Runs sync inspection with optional launch-captured SSH authority. HTTPS
/// ignores the authority; SSH fails closed when it is absent.
pub fn apply_git_sync_inspection_with_ssh(
    workspace: &WorkspaceView,
    state_home: &Path,
    plan: GitSyncPlan,
    cancelled: &AtomicBool,
    ssh: Option<&GitSshTransportResources>,
) -> Result<GitSyncInspection, GitSyncError> {
    let authentication = if plan.transport == GitRemoteTransport::Ssh {
        ssh.map_or(
            GitRemoteAuthentication::Anonymous,
            GitRemoteAuthentication::Ssh,
        )
    } else {
        GitRemoteAuthentication::Anonymous
    };
    apply_git_sync_inspection_with_authentication(
        workspace,
        state_home,
        plan,
        cancelled,
        authentication,
    )
}

pub fn apply_git_sync_inspection_with_authentication(
    workspace: &WorkspaceView,
    state_home: &Path,
    plan: GitSyncPlan,
    cancelled: &AtomicBool,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<GitSyncInspection, GitSyncError> {
    if !authentication.supports(plan.transport) {
        return Err(GitSyncError::UnsupportedTransport);
    }
    revalidate_git_sync_plan(workspace, &plan)?;
    let remote_head = discover_git_sync_remote_head(&plan, cancelled, authentication)?;
    if remote_head.object_format != plan.preflight.object_format {
        return Err(GitSyncError::InvalidRepository);
    }
    let staging = prepare_git_sync_staging(&plan, &remote_head, state_home)?;
    fetch_git_sync_remote(&plan, &remote_head, &staging, cancelled, authentication)?;
    finish_git_sync_inspection(workspace, plan, staging, remote_head, cancelled)
}

fn finish_git_sync_inspection(
    workspace: &WorkspaceView,
    plan: GitSyncPlan,
    staging: GitCloneStaging,
    remote_head: GitRemoteHead,
    cancelled: &AtomicBool,
) -> Result<GitSyncInspection, GitSyncError> {
    let tree = inspect_fetched_git_clone_tree(&staging, &remote_head, cancelled)
        .map_err(map_clone_inspection_to_sync)?;
    let validated =
        validate_fetched_git_clone(&staging, tree, cancelled).map_err(map_clone_content_to_sync)?;
    revalidate_git_sync_plan(workspace, &plan)?;
    if validated.workspace().manifest().workspace_id().to_string() != plan.preflight.workspace_id {
        return Err(GitSyncError::InvalidWorkspace);
    }
    let remote_matches_current = workspace_validation_fingerprint(validated.workspace())
        == plan.preflight.workspace_fingerprint;
    let local_head = plan
        .preflight
        .branch
        .expected_object_id
        .as_deref()
        .ok_or(GitSyncError::Unsupported)?;
    let state = if remote_head.object_id() == local_head {
        GitSyncInspectionState::UpToDate
    } else {
        GitSyncInspectionState::RemoteChanged
    };
    let local_changes = !plan.preflight.working_tree_clean;
    Ok(GitSyncInspection {
        plan,
        staging,
        remote_head,
        validated,
        state,
        local_changes,
        workspace_matches_remote: remote_matches_current,
    })
}

#[derive(Debug, Eq, PartialEq)]
struct GitSyncHistory {
    object_ids: Vec<String>,
    complete: bool,
}

/// Proves the relationship between the active branch and the already
/// validated remote snapshot using bounded history reads. The active
/// repository remains read-only and the remote history remains in private
/// staging.
pub fn classify_git_sync_ancestry(
    workspace: &WorkspaceView,
    inspection: GitSyncInspection,
    cancelled: &AtomicBool,
) -> Result<GitSyncClassification, GitSyncError> {
    classify_git_sync_ancestry_with_authentication(
        workspace,
        inspection,
        cancelled,
        GitRemoteAuthentication::Anonymous,
    )
}

/// Extends ancestry inspection with the same optional SSH authority used for
/// the initial fetch. The active repository remains read-only.
pub fn classify_git_sync_ancestry_with_ssh(
    workspace: &WorkspaceView,
    inspection: GitSyncInspection,
    cancelled: &AtomicBool,
    ssh: Option<&GitSshTransportResources>,
) -> Result<GitSyncClassification, GitSyncError> {
    let authentication = if inspection.plan.transport == GitRemoteTransport::Ssh {
        ssh.map_or(
            GitRemoteAuthentication::Anonymous,
            GitRemoteAuthentication::Ssh,
        )
    } else {
        GitRemoteAuthentication::Anonymous
    };
    classify_git_sync_ancestry_with_authentication(workspace, inspection, cancelled, authentication)
}

pub fn classify_git_sync_ancestry_with_authentication(
    workspace: &WorkspaceView,
    inspection: GitSyncInspection,
    cancelled: &AtomicBool,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<GitSyncClassification, GitSyncError> {
    if !authentication.supports(inspection.plan.transport) {
        return Err(GitSyncError::UnsupportedTransport);
    }
    revalidate_git_sync_plan(workspace, &inspection.plan)?;
    let local_head = inspection
        .plan
        .preflight
        .branch
        .expected_object_id
        .as_deref()
        .ok_or(GitSyncError::Unsupported)?;
    if local_head == inspection.remote_head.object_id() {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GitSyncError::Cancelled);
        }
        revalidate_git_sync_plan(workspace, &inspection.plan)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(GitSyncError::Cancelled);
        }
        return Ok(GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::UpToDate,
        });
    }

    deepen_git_sync_remote(&inspection, cancelled, authentication)?;
    let refreshed_tree =
        inspect_fetched_git_clone_tree(&inspection.staging, &inspection.remote_head, cancelled)
            .map_err(map_clone_inspection_to_sync)?;
    if &refreshed_tree != inspection.validated.tree() {
        return Err(GitSyncError::InvalidRepository);
    }
    let remote_history = read_remote_sync_history(&inspection, cancelled)?;
    let local_history = read_local_sync_history(workspace, &inspection.plan)?;
    let ancestry = classify_git_sync_histories(
        local_head,
        inspection.remote_head.object_id(),
        &local_history,
        &remote_history,
    );
    revalidate_git_sync_plan(workspace, &inspection.plan)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(GitSyncError::Cancelled);
    }
    Ok(GitSyncClassification {
        inspection,
        ancestry,
    })
}

/// Applies only the portable Workspace portion of a proven remote
/// fast-forward through the recovery journal. Git objects, index, and refs are
/// deliberately left for the following durable publication gate.
#[allow(
    dead_code,
    reason = "internal Phase 5D2B1 primitive awaiting the atomic metadata publication gate"
)]
pub(crate) fn apply_git_sync_workspace_materialization(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    classification: GitSyncClassification,
) -> Result<(Box<WorkspaceView>, GitSyncWorkspaceMaterialization), GitSyncError> {
    if classification.ancestry != GitSyncAncestry::RemoteFastForward
        || classification.inspection.local_changes()
    {
        return Err(GitSyncError::Unsupported);
    }
    let operation_id = new_operation_id();
    let created_at = current_timestamp();
    let prepared = prepare_git_sync_workspace_materialization(
        workspace,
        state_home,
        runtime_directory,
        &classification.inspection,
        operation_id,
        created_at,
    )?;
    let outcome =
        activate_git_sync_materialization(workspace, state_home, runtime_directory, prepared)
            .map_err(map_sync_materialization_to_sync)?;
    let fresh = verify_git_sync_workspace_materialization(workspace, &classification.inspection)?;
    Ok((
        fresh,
        GitSyncWorkspaceMaterialization {
            classification,
            operation_id,
            created_at,
            plan_hash: prepared.plan_hash,
            changed: outcome.changed,
        },
    ))
}

pub fn apply_git_sync_fast_forward(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    classification: GitSyncClassification,
) -> Result<(Box<WorkspaceView>, GitSyncPublicationOutcome), GitSyncError> {
    if classification.ancestry != GitSyncAncestry::RemoteFastForward
        || classification.inspection.local_changes()
    {
        return Err(GitSyncError::Unsupported);
    }
    let operation_id = new_operation_id();
    let prepared = prepare_git_sync_workspace_materialization(
        workspace,
        state_home,
        runtime_directory,
        &classification.inspection,
        operation_id,
        current_timestamp(),
    )?;
    let candidate = prepare_git_sync_candidate(&classification.inspection)?;
    install_git_sync_publication_intent(
        workspace,
        state_home,
        runtime_directory,
        &classification.inspection,
        &candidate,
        prepared,
    )?;
    let fresh =
        resume_git_sync_publication(workspace, state_home, runtime_directory, operation_id)?;
    Ok((
        fresh,
        GitSyncPublicationOutcome {
            operation_id,
            changed: true,
        },
    ))
}

/// Publishes exactly the local commit from a validated `localAhead`
/// classification to its exact configured upstream. The remote is checked
/// immediately before and after an ordinary (never forced) push.
pub fn apply_git_push(
    workspace: &WorkspaceView,
    classification: GitSyncClassification,
) -> Result<GitPushOutcome, GitSyncError> {
    apply_git_push_with_authentication(
        workspace,
        classification,
        GitRemoteAuthentication::Anonymous,
    )
}

/// Publishes through optional launch-captured SSH authority. HTTPS ignores the
/// authority; SSH fails closed when it is absent.
pub fn apply_git_push_with_ssh(
    workspace: &WorkspaceView,
    classification: GitSyncClassification,
    ssh: Option<&GitSshTransportResources>,
) -> Result<GitPushOutcome, GitSyncError> {
    let authentication = if classification.transport() == GitRemoteTransport::Ssh {
        ssh.map_or(
            GitRemoteAuthentication::Anonymous,
            GitRemoteAuthentication::Ssh,
        )
    } else {
        GitRemoteAuthentication::Anonymous
    };
    apply_git_push_with_authentication(workspace, classification, authentication)
}

pub fn apply_git_push_with_authentication(
    workspace: &WorkspaceView,
    classification: GitSyncClassification,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<GitPushOutcome, GitSyncError> {
    if !authentication.supports(classification.transport()) {
        return Err(GitSyncError::UnsupportedTransport);
    }
    let not_cancelled = AtomicBool::new(false);
    apply_git_push_with(
        workspace,
        classification,
        |plan| discover_git_sync_remote_head(plan, &not_cancelled, authentication),
        |workspace, plan, local_commit| {
            execute_exact_git_push(workspace, plan, local_commit, authentication)
        },
    )
}

fn apply_git_push_with<D, P>(
    workspace: &WorkspaceView,
    classification: GitSyncClassification,
    mut discover: D,
    publish: P,
) -> Result<GitPushOutcome, GitSyncError>
where
    D: FnMut(&GitSyncPlan) -> Result<GitRemoteHead, GitSyncError>,
    P: FnOnce(&WorkspaceView, &GitSyncPlan, &str) -> Result<(), GitSyncError>,
{
    if classification.ancestry != GitSyncAncestry::LocalAhead {
        return Err(GitSyncError::Unsupported);
    }
    let inspection = &classification.inspection;
    let plan = &inspection.plan;
    revalidate_git_sync_plan(workspace, plan)?;
    let expected_remote = inspection.remote_head.object_id();
    let local_commit = plan
        .preflight
        .branch
        .expected_object_id
        .as_deref()
        .ok_or(GitSyncError::Unsupported)?;
    let before = discover(plan)?;
    if before.object_id() != expected_remote {
        return Err(GitSyncError::RemoteChanged);
    }
    revalidate_git_sync_plan(workspace, plan)?;

    let push_result = publish(workspace, plan, local_commit);

    // A failed or timed-out client may still have been acknowledged by the
    // server. The exact postflight state is therefore authoritative.
    match discover(plan) {
        Ok(after) if after.object_id() == local_commit => Ok(GitPushOutcome { pushed: true }),
        Ok(after) if after.object_id() != expected_remote => Err(GitSyncError::RemoteChanged),
        Ok(_) => match push_result {
            Ok(()) => Err(GitSyncError::RemoteUnavailable),
            Err(error) => Err(error),
        },
        Err(GitSyncError::RemoteChanged) => Err(GitSyncError::RemoteChanged),
        Err(_) => Err(GitSyncError::RemoteStateUncertain),
    }
}

fn execute_exact_git_push(
    workspace: &WorkspaceView,
    plan: &GitSyncPlan,
    local_commit: &str,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<(), GitSyncError> {
    if !authentication.supports(plan.transport) {
        return Err(GitSyncError::UnsupportedTransport);
    }
    let executable = trusted_executable("/usr/bin/git").ok_or(GitSyncError::GitUnavailable)?;
    let sandbox = trusted_executable("/usr/bin/bwrap").ok_or(GitSyncError::SandboxUnavailable)?;
    let capability = workspace
        .git_repository_capability()
        .map_err(map_git_capability_to_sync)?;
    if capability
        .git_directory_identity()
        .map_err(map_git_snapshot_to_sync)?
        != plan.preflight.git_identity
    {
        return Err(GitSyncError::PlanStale);
    }
    let (command, mut synthetic_config) = git_push_command(
        &executable,
        &sandbox,
        &capability,
        plan.preflight.object_format,
        &plan.remote_url,
        local_commit,
        &plan.remote_ref,
        plan.transport,
        authentication,
    )?;
    let push_result = run_clone_command_for_transport(
        command,
        &AtomicBool::new(false),
        PUSH_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        plan.transport,
        authentication,
    )
    .map(|_| ())
    .map_err(map_clone_discovery_to_sync);
    let _ = synthetic_config.seek(SeekFrom::Start(0));
    push_result
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn install_git_sync_publication_intent(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    inspection: &GitSyncInspection,
    candidate: &GitSyncCandidate,
    prepared: GitSyncPreparedMaterialization,
) -> Result<(), GitSyncError> {
    revalidate_git_sync_plan(workspace, &inspection.plan)?;
    let session = workspace
        .open_git_commit_recovery_session(state_home, runtime_directory)
        .map_err(map_git_commit_intent_to_sync)?;
    if session
        .list_intents()
        .map_err(map_git_commit_intent_to_sync)?
        .iter()
        .any(|intent| intent.stage != GitCommitIntentStage::Complete)
        || session
            .list_sync_intents()
            .map_err(map_git_commit_intent_to_sync)?
            .iter()
            .any(|intent| intent.stage != GitSyncIntentStage::Complete)
    {
        return Err(GitSyncError::RecoveryRequired);
    }
    let (current, _) = inspect_git_sync_repository(workspace)?;
    if !same_git_sync_preflight(&inspection.plan.preflight, &current) {
        return Err(GitSyncError::PlanStale);
    }
    let mut pack_files = candidate
        .pack_sources
        .iter()
        .map(|source| inspection.staging.open_pack_source(source))
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_clone_staging_to_sync)?;
    let capability = workspace
        .git_repository_capability()
        .map_err(map_git_capability_to_sync)?;
    if capability
        .git_directory_identity()
        .map_err(map_git_snapshot_to_sync)?
        != current.git_identity
    {
        return Err(GitSyncError::PlanStale);
    }
    capability
        .install_commit_pack_sources(
            &candidate.pack_sources,
            &mut pack_files,
            inspection.remote_head.object_id().len(),
        )
        .map_err(map_git_commit_pack_install_to_sync)?;

    revalidate_git_sync_plan(workspace, &inspection.plan)?;
    let before_index = current
        .repository
        .index
        .as_ref()
        .ok_or(GitSyncError::InvalidRepository)?;
    let branch_relative = current
        .branch
        .reference
        .strip_prefix("refs/heads/")
        .ok_or(GitSyncError::Unsafe)?;
    let representation = match current.branch.representation {
        GitBranchRepresentation::Loose => GitCommitIntentBranchRepresentation::Loose,
        GitBranchRepresentation::Packed => GitCommitIntentBranchRepresentation::Packed,
        GitBranchRepresentation::Unborn => return Err(GitSyncError::Unsupported),
    };
    let mut candidate_ref = Vec::new();
    candidate_ref
        .try_reserve_exact(inspection.remote_head.object_id().len().saturating_add(1))
        .map_err(|_| GitSyncError::ResourceLimit)?;
    candidate_ref.extend_from_slice(inspection.remote_head.object_id().as_bytes());
    candidate_ref.push(b'\n');
    let head_hash = session
        .store_bytes(&current.repository.head.bytes)
        .map_err(map_git_commit_intent_to_sync)?;
    let config_hash = session
        .store_bytes(&current.repository.config.bytes)
        .map_err(map_git_commit_intent_to_sync)?;
    let before_index_hash = session
        .store_bytes(&before_index.bytes)
        .map_err(map_git_commit_intent_to_sync)?;
    let candidate_index_hash = session
        .store_bytes(&candidate.index_bytes)
        .map_err(map_git_commit_intent_to_sync)?;
    let packed_refs_hash = current
        .repository
        .packed_refs
        .as_ref()
        .map(|snapshot| session.store_bytes(&snapshot.bytes))
        .transpose()
        .map_err(map_git_commit_intent_to_sync)?;
    let shallow_hash = current
        .repository
        .shallow
        .as_ref()
        .map(|snapshot| session.store_bytes(&snapshot.bytes))
        .transpose()
        .map_err(map_git_commit_intent_to_sync)?;
    let before_ref_hash = current
        .branch_storage
        .target
        .as_ref()
        .map(|snapshot| session.store_bytes(&snapshot.bytes))
        .transpose()
        .map_err(map_git_commit_intent_to_sync)?;
    let candidate_ref_hash = session
        .store_bytes(&candidate_ref)
        .map_err(map_git_commit_intent_to_sync)?;
    verify_git_sync_candidate_objects(
        &capability,
        current.object_format,
        inspection.remote_head.object_id(),
        &candidate.index_bytes,
    )?;
    let (root_device, root_inode) = session.root_identity();
    let mut intent = GitSyncIntent {
        generation: 1,
        operation_id: prepared.operation_id,
        workspace_id: session.workspace_id(),
        root_hash: session.root_hash(),
        root_device,
        root_inode,
        git_device: current.git_identity.0,
        git_inode: current.git_identity.1,
        branch_relative: branch_relative.to_owned(),
        representation,
        head_hash,
        config_hash,
        before_index_hash,
        before_index_mode: before_index.mode(),
        candidate_index_hash,
        candidate_index_mode: before_index.mode(),
        packed_refs_hash,
        shallow_hash,
        shallow_mode: current
            .repository
            .shallow
            .as_ref()
            .map_or(0o600, |snapshot| snapshot.mode()),
        before_ref_hash,
        before_ref_mode: current
            .branch_storage
            .target
            .as_ref()
            .map_or(0o600, |snapshot| snapshot.mode()),
        candidate_ref_hash,
        candidate_ref_mode: current
            .branch_storage
            .target
            .as_ref()
            .map_or(0o600, |snapshot| snapshot.mode()),
        workspace_journal_hash: prepared.journal_hash,
        workspace_plan_hash: prepared.plan_hash,
        plan_hash: [0; 32],
        stage: GitSyncIntentStage::Prepared,
    };
    intent.plan_hash = git_sync_intent_plan_hash(&intent);
    session
        .install_sync_intent(&intent)
        .map_err(map_git_commit_intent_to_sync)
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn resume_git_sync_publication(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    operation_id: OperationId,
) -> Result<Box<WorkspaceView>, GitSyncError> {
    let session = workspace
        .open_git_commit_recovery_session(state_home, runtime_directory)
        .map_err(map_git_commit_intent_to_sync)?;
    let mut intent = session
        .read_sync_intent(operation_id)
        .map_err(map_git_commit_intent_to_sync)?;
    if intent.stage == GitSyncIntentStage::Conflict {
        return Err(GitSyncError::PlanStale);
    }
    verify_git_sync_intent_plan_hash(&intent)?;
    let capability = workspace
        .git_repository_capability()
        .map_err(map_git_capability_to_sync)?;
    if session.root_identity() != (intent.root_device, intent.root_inode)
        || capability
            .git_directory_identity()
            .map_err(map_git_snapshot_to_sync)?
            != (intent.git_device, intent.git_inode)
    {
        return Err(GitSyncError::Unsafe);
    }
    let head = session
        .read_bytes(&intent.head_hash)
        .map_err(map_git_commit_intent_to_sync)?;
    let config = session
        .read_bytes(&intent.config_hash)
        .map_err(map_git_commit_intent_to_sync)?;
    let before_index = session
        .read_bytes(&intent.before_index_hash)
        .map_err(map_git_commit_intent_to_sync)?;
    let candidate_index = session
        .read_bytes(&intent.candidate_index_hash)
        .map_err(map_git_commit_intent_to_sync)?;
    let packed_refs = intent
        .packed_refs_hash
        .as_ref()
        .map(|hash| session.read_bytes(hash))
        .transpose()
        .map_err(map_git_commit_intent_to_sync)?;
    let shallow = intent
        .shallow_hash
        .as_ref()
        .map(|hash| session.read_bytes(hash))
        .transpose()
        .map_err(map_git_commit_intent_to_sync)?;
    let before_ref = intent
        .before_ref_hash
        .as_ref()
        .map(|hash| session.read_bytes(hash))
        .transpose()
        .map_err(map_git_commit_intent_to_sync)?;
    let candidate_ref = session
        .read_bytes(&intent.candidate_ref_hash)
        .map_err(map_git_commit_intent_to_sync)?;
    let candidate_object_id = std::str::from_utf8(&candidate_ref)
        .ok()
        .and_then(|value| value.strip_suffix('\n'))
        .ok_or(GitSyncError::Unsafe)?;
    let object_format = validate_object_id(candidate_object_id).ok_or(GitSyncError::Unsafe)?;
    parse_git_index(&candidate_index, object_format).map_err(map_commit_plan_to_sync)?;
    capability
        .recover_commit_lock_residue(
            &intent.branch_relative,
            Some((&before_index, intent.before_index_mode)),
            (&candidate_index, intent.candidate_index_mode),
            before_ref
                .as_deref()
                .map(|bytes| (bytes, intent.before_ref_mode)),
            (&candidate_ref, intent.candidate_ref_mode),
        )
        .map_err(map_git_commit_metadata_to_sync)?;
    let repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    let branch = capability
        .read_commit_branch_snapshot(&intent.branch_relative)
        .map_err(map_git_snapshot_to_sync)?;
    if repository.head.bytes != head
        || repository.config.bytes != config
        || repository.packed_refs.as_ref().map(|value| &value.bytes) != packed_refs.as_ref()
        || repository.shallow.as_ref().map(|value| &value.bytes) != shallow.as_ref()
    {
        return mark_git_sync_intent_conflict(&session, &mut intent);
    }
    let index_is_before = git_commit_snapshot_matches(
        repository.index.as_ref(),
        Some(&before_index),
        intent.before_index_mode,
    );
    let index_is_candidate = git_commit_snapshot_matches(
        repository.index.as_ref(),
        Some(&candidate_index),
        intent.candidate_index_mode,
    );
    let ref_is_before = git_commit_snapshot_matches(
        branch.target.as_ref(),
        before_ref.as_deref(),
        intent.before_ref_mode,
    );
    let ref_is_candidate = git_commit_snapshot_matches(
        branch.target.as_ref(),
        Some(&candidate_ref),
        intent.candidate_ref_mode,
    );
    if !matches!(
        (
            index_is_before,
            index_is_candidate,
            ref_is_before,
            ref_is_candidate
        ),
        (true, false, true, false) | (false, true, true, false) | (false, true, false, true)
    ) {
        return mark_git_sync_intent_conflict(&session, &mut intent);
    }
    drop(capability);
    drop(session);

    if intent.stage == GitSyncIntentStage::Prepared {
        activate_git_sync_materialization_plan(
            workspace,
            state_home,
            runtime_directory,
            operation_id,
            intent.workspace_plan_hash,
            intent.workspace_journal_hash,
        )
        .map_err(map_sync_materialization_to_sync)?;
    }
    let session = workspace
        .open_git_commit_recovery_session(state_home, runtime_directory)
        .map_err(map_git_commit_intent_to_sync)?;
    intent = session
        .read_sync_intent(operation_id)
        .map_err(map_git_commit_intent_to_sync)?;
    if intent.stage == GitSyncIntentStage::Prepared {
        advance_git_sync_intent(
            &session,
            &mut intent,
            GitSyncIntentStage::WorkspacePublished,
        )?;
    }
    let fresh = workspace.fresh_snapshot().map_err(|error| match error {
        crate::storage::WorkspaceInspectError::ResourceLimit => GitSyncError::ResourceLimit,
        crate::storage::WorkspaceInspectError::ChangedDuringRead => GitSyncError::PlanStale,
        _ => GitSyncError::Io,
    })?;
    if !fresh.issues().is_empty()
        || !git_sync_workspace_matches_index(&fresh, object_format, &candidate_index)?
    {
        return mark_git_sync_intent_conflict(&session, &mut intent);
    }
    let capability = workspace
        .git_repository_capability()
        .map_err(map_git_capability_to_sync)?;
    verify_git_sync_candidate_objects(
        &capability,
        object_format,
        candidate_object_id,
        &candidate_index,
    )?;
    let repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    let branch = capability
        .read_commit_branch_snapshot(&intent.branch_relative)
        .map_err(map_git_snapshot_to_sync)?;
    if repository.head.bytes != head
        || repository.config.bytes != config
        || repository.packed_refs.as_ref().map(|value| &value.bytes) != packed_refs.as_ref()
        || repository.shallow.as_ref().map(|value| &value.bytes) != shallow.as_ref()
    {
        return mark_git_sync_intent_conflict(&session, &mut intent);
    }
    let index_is_before = git_commit_snapshot_matches(
        repository.index.as_ref(),
        Some(&before_index),
        intent.before_index_mode,
    );
    let index_is_candidate = git_commit_snapshot_matches(
        repository.index.as_ref(),
        Some(&candidate_index),
        intent.candidate_index_mode,
    );
    let ref_is_before = git_commit_snapshot_matches(
        branch.target.as_ref(),
        before_ref.as_deref(),
        intent.before_ref_mode,
    );
    let ref_is_candidate = git_commit_snapshot_matches(
        branch.target.as_ref(),
        Some(&candidate_ref),
        intent.candidate_ref_mode,
    );
    let branch_relative = intent.branch_relative.clone();
    match (
        index_is_before,
        index_is_candidate,
        ref_is_before,
        ref_is_candidate,
    ) {
        (true, false, true, false) if intent.stage == GitSyncIntentStage::WorkspacePublished => {
            capability
                .publish_commit_metadata(
                    &repository,
                    &branch,
                    &branch_relative,
                    &candidate_index,
                    &candidate_ref,
                    |stage| persist_recovered_git_sync_stage(&session, &mut intent, stage),
                )
                .map_err(map_git_commit_metadata_to_sync)?;
        }
        (false, true, true, false) => {
            if intent.stage == GitSyncIntentStage::WorkspacePublished {
                advance_git_sync_intent(&session, &mut intent, GitSyncIntentStage::IndexPublished)?;
            }
            if intent.stage != GitSyncIntentStage::IndexPublished {
                return mark_git_sync_intent_conflict(&session, &mut intent);
            }
            capability
                .publish_commit_ref_after_index(
                    &repository,
                    &branch,
                    &branch_relative,
                    &candidate_index,
                    &candidate_ref,
                    |stage| persist_recovered_git_sync_stage(&session, &mut intent, stage),
                )
                .map_err(map_git_commit_metadata_to_sync)?;
        }
        (false, true, false, true) => {
            if intent.stage == GitSyncIntentStage::WorkspacePublished {
                advance_git_sync_intent(&session, &mut intent, GitSyncIntentStage::IndexPublished)?;
            }
            if intent.stage == GitSyncIntentStage::IndexPublished {
                advance_git_sync_intent(&session, &mut intent, GitSyncIntentStage::RefPublished)?;
            }
        }
        _ => return mark_git_sync_intent_conflict(&session, &mut intent),
    }
    if !matches!(
        intent.stage,
        GitSyncIntentStage::RefPublished | GitSyncIntentStage::Complete
    ) {
        return Err(GitSyncError::RecoveryRequired);
    }
    verify_git_sync_candidate_objects(
        &capability,
        object_format,
        candidate_object_id,
        &candidate_index,
    )?;
    let final_repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    let final_branch = capability
        .read_commit_branch_snapshot(&intent.branch_relative)
        .map_err(map_git_snapshot_to_sync)?;
    let final_fresh = workspace.fresh_snapshot().map_err(|error| match error {
        crate::storage::WorkspaceInspectError::ResourceLimit => GitSyncError::ResourceLimit,
        crate::storage::WorkspaceInspectError::ChangedDuringRead => GitSyncError::PlanStale,
        _ => GitSyncError::Io,
    })?;
    if final_repository.head.bytes != head
        || final_repository.config.bytes != config
        || final_repository
            .packed_refs
            .as_ref()
            .map(|value| &value.bytes)
            != packed_refs.as_ref()
        || final_repository.shallow.as_ref().map(|value| &value.bytes) != shallow.as_ref()
        || !git_commit_snapshot_matches(
            final_repository.index.as_ref(),
            Some(&candidate_index),
            intent.candidate_index_mode,
        )
        || !git_commit_snapshot_matches(
            final_branch.target.as_ref(),
            Some(&candidate_ref),
            intent.candidate_ref_mode,
        )
        || !final_fresh.issues().is_empty()
        || !git_sync_workspace_matches_index(&final_fresh, object_format, &candidate_index)?
    {
        return mark_git_sync_intent_conflict(&session, &mut intent);
    }
    if intent.stage == GitSyncIntentStage::RefPublished {
        advance_git_sync_intent(&session, &mut intent, GitSyncIntentStage::Complete)?;
    }
    Ok(final_fresh)
}

pub fn resume_pending_git_sync_publication(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<(Box<WorkspaceView>, GitSyncPublicationOutcome), GitSyncError> {
    let operation_id = pending_git_sync_recovery_at(workspace, state_home, runtime_directory)
        .map_err(map_commit_plan_to_sync)?
        .ok_or(GitSyncError::InvalidConfiguration)?;
    let fresh =
        resume_git_sync_publication(workspace, state_home, runtime_directory, operation_id)?;
    Ok((
        fresh,
        GitSyncPublicationOutcome {
            operation_id,
            changed: true,
        },
    ))
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn verify_git_sync_candidate_objects(
    capability: &crate::storage::GitRepositoryCapability,
    object_format: GitObjectFormat,
    commit_id: &str,
    candidate_index: &[u8],
) -> Result<(), GitSyncError> {
    if validate_object_id(commit_id) != Some(object_format) {
        return Err(GitSyncError::Unsafe);
    }
    let executable = trusted_executable("/usr/bin/git").ok_or(GitSyncError::GitUnavailable)?;
    let sandbox = trusted_executable("/usr/bin/bwrap").ok_or(GitSyncError::SandboxUnavailable)?;
    let commit = run_git_commit_read(
        &executable,
        &sandbox,
        capability,
        object_format,
        &["cat-file", "commit", commit_id],
        OUTPUT_LIMIT,
    )
    .map_err(|error| map_commit_plan_to_sync(map_git_commit_read_error(error)))?;
    if git_object_id(object_format, "commit", &commit.stdout) != commit_id {
        return Err(GitSyncError::InvalidRepository);
    }
    let mut cursor = 0usize;
    let root_tree = take_git_commit_line(&commit.stdout, &mut cursor)
        .map_err(map_commit_plan_to_sync)?
        .strip_prefix(b"tree ")
        .and_then(|value| std::str::from_utf8(value).ok())
        .filter(|value| validate_object_id(value) == Some(object_format))
        .ok_or(GitSyncError::InvalidRepository)?;
    let tree_output = run_git_commit_read(
        &executable,
        &sandbox,
        capability,
        object_format,
        &["ls-tree", "-r", "-t", "-l", "--full-tree", "-z", commit_id],
        MAX_COMMIT_TREE_BYTES,
    )
    .map_err(|error| map_commit_plan_to_sync(map_git_commit_read_error(error)))?;
    let tree = parse_git_commit_tree(&tree_output.stdout, object_format)
        .map_err(map_commit_plan_to_sync)?;
    verify_git_commit_index_matches_tree(candidate_index, object_format, &tree)
        .map_err(map_commit_plan_to_sync)?;
    verify_git_commit_tree_objects(
        &executable,
        &sandbox,
        capability,
        object_format,
        commit_id,
        root_tree,
        &tree,
    )
    .map_err(map_commit_plan_to_sync)
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn git_sync_workspace_matches_index(
    workspace: &WorkspaceView,
    object_format: GitObjectFormat,
    candidate_index: &[u8],
) -> Result<bool, GitSyncError> {
    let index = parse_git_index(candidate_index, object_format).map_err(map_commit_plan_to_sync)?;
    let inventory = inspect_git_commit_managed_inventory(workspace, object_format)
        .map_err(map_commit_plan_to_sync)?;
    if index.entries.len() != inventory.files.len() {
        return Ok(false);
    }
    Ok(index
        .entries
        .iter()
        .zip(&inventory.files)
        .all(|(indexed, file)| {
            indexed.mode == 0o100644
                && indexed.path == file.path.as_bytes()
                && hex_lower(&indexed.object_id) == file.blob_object_id
        }))
}

fn prepare_git_sync_workspace_materialization(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    inspection: &GitSyncInspection,
    operation_id: OperationId,
    created_at: Timestamp,
) -> Result<GitSyncPreparedMaterialization, GitSyncError> {
    revalidate_git_sync_plan(workspace, &inspection.plan)?;
    let current_tree = inspect_fetched_git_clone_tree(
        &inspection.staging,
        &inspection.remote_head,
        &AtomicBool::new(false),
    )
    .map_err(map_clone_inspection_to_sync)?;
    if &current_tree != inspection.validated.tree() {
        return Err(GitSyncError::PlanStale);
    }
    let mut current = Vec::new();
    current
        .try_reserve_exact(inspection.plan.preflight.managed.files.len())
        .map_err(|_| GitSyncError::ResourceLimit)?;
    for file in &inspection.plan.preflight.managed.files {
        current.push(GitSyncMaterializationCurrent {
            relative_path: file.path.clone(),
            revision: file.revision,
            size: file.size,
            mode: file.mode,
        });
    }
    let tree = inspection.validated.tree();
    let mut incoming = Vec::new();
    incoming
        .try_reserve_exact(tree.entries().len())
        .map_err(|_| GitSyncError::ResourceLimit)?;
    for entry in tree.entries() {
        incoming.push(GitSyncMaterializationSource {
            relative_path: entry.relative_path().to_owned(),
            size: entry.size(),
        });
    }
    let plan_hash = git_sync_materialization_plan_hash(inspection, operation_id);
    prepare_git_sync_materialization(
        workspace,
        state_home,
        runtime_directory,
        operation_id,
        created_at,
        plan_hash,
        &current,
        &incoming,
        || {
            revalidate_git_sync_plan(workspace, &inspection.plan)
                .map_err(|_| GitSyncMaterializationStorageError::SourceChanged)
        },
        |index, relative_path, size| {
            let entry = tree
                .entries()
                .get(index)
                .ok_or(GitSyncMaterializationStorageError::SourceChanged)?;
            if entry.relative_path() != relative_path || entry.size() != size {
                return Err(GitSyncMaterializationStorageError::SourceChanged);
            }
            let bytes = inspection
                .staging
                .read_validation_file(relative_path, size)
                .map_err(map_sync_staging_to_materialization)?;
            if !git_blob_object_id_matches(tree.object_format(), &bytes, entry.object_id()) {
                return Err(GitSyncMaterializationStorageError::SourceChanged);
            }
            Ok(bytes)
        },
    )
    .map_err(map_sync_materialization_to_sync)
}

fn verify_git_sync_workspace_materialization(
    workspace: &WorkspaceView,
    inspection: &GitSyncInspection,
) -> Result<Box<WorkspaceView>, GitSyncError> {
    let fresh = workspace.fresh_snapshot().map_err(|error| match error {
        crate::storage::WorkspaceInspectError::ResourceLimit => GitSyncError::ResourceLimit,
        crate::storage::WorkspaceInspectError::ChangedDuringRead => GitSyncError::PlanStale,
        _ => GitSyncError::Io,
    })?;
    if !fresh.issues().is_empty()
        || fresh.manifest().workspace_id()
            != inspection.validated.workspace().manifest().workspace_id()
        || !git_sync_workspace_matches_tree(&fresh, inspection.validated.tree())?
    {
        return Err(GitSyncError::InvalidWorkspace);
    }
    Ok(fresh)
}

#[allow(
    dead_code,
    reason = "used by the staged sync materialization primitive"
)]
fn git_sync_materialization_plan_hash(
    inspection: &GitSyncInspection,
    operation_id: OperationId,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rehow:git-sync-materialization:v1\0");
    hash_clone_field(&mut hasher, operation_id.to_string().as_bytes());
    hasher.update(&inspection.plan.preflight.root_capability_hash);
    hasher.update(inspection.validated.tree().plan_hash().as_slice());
    hash_clone_field(&mut hasher, inspection.remote_head.object_id().as_bytes());
    *hasher.finalize().as_bytes()
}

#[allow(
    dead_code,
    reason = "used by the staged sync materialization primitive"
)]
fn git_sync_workspace_matches_tree(
    workspace: &WorkspaceView,
    tree: &GitCloneTree,
) -> Result<bool, GitSyncError> {
    let inventory = inspect_git_commit_managed_inventory(workspace, tree.object_format())
        .map_err(map_commit_plan_to_sync)?;
    if inventory.files.len() != tree.entries().len() {
        return Ok(false);
    }
    Ok(inventory
        .files
        .iter()
        .zip(tree.entries())
        .all(|(current, remote)| {
            current.path == remote.relative_path()
                && current.size == remote.size()
                && current.blob_object_id == remote.object_id()
        }))
}

fn deepen_git_sync_remote(
    inspection: &GitSyncInspection,
    cancelled: &AtomicBool,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<(), GitSyncError> {
    if !authentication.supports(inspection.plan.transport) {
        return Err(GitSyncError::UnsupportedTransport);
    }
    let executable = trusted_executable("/usr/bin/git").ok_or(GitSyncError::GitUnavailable)?;
    let sandbox = trusted_executable("/usr/bin/bwrap").ok_or(GitSyncError::SandboxUnavailable)?;
    let before = inspection
        .staging
        .git_state(CLONE_STAGING_FILE_LIMIT)
        .map_err(map_clone_staging_to_sync)?;
    let available = CLONE_STAGING_FILE_LIMIT
        .checked_sub(before.bytes)
        .and_then(|remaining| remaining.checked_sub(SYNC_FETCH_METADATA_RESERVE))
        .ok_or(GitSyncError::ResourceLimit)?;
    let file_limit = available / SYNC_FETCH_LARGE_FILE_SLOTS;
    if file_limit == 0 {
        return Err(GitSyncError::ResourceLimit);
    }
    let output = run_clone_command_for_transport(
        sync_history_fetch_command(
            &executable,
            &sandbox,
            inspection.staging.git_path(),
            inspection.staging.git_descriptor(),
            &inspection.plan.remote_url,
            inspection.remote_head.object_id(),
            file_limit,
            inspection.plan.transport,
            authentication,
        ),
        cancelled,
        CLONE_FETCH_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        inspection.plan.transport,
        authentication,
    )
    .map_err(map_clone_discovery_to_sync)?;
    if !output.stdout.is_empty() {
        return Err(GitSyncError::InvalidRepository);
    }
    inspection
        .staging
        .git_state(CLONE_STAGING_FILE_LIMIT)
        .map(|_| ())
        .map_err(map_clone_staging_to_sync)
}

fn read_remote_sync_history(
    inspection: &GitSyncInspection,
    cancelled: &AtomicBool,
) -> Result<GitSyncHistory, GitSyncError> {
    let executable = trusted_executable("/usr/bin/git").ok_or(GitSyncError::GitUnavailable)?;
    let sandbox = trusted_executable("/usr/bin/bwrap").ok_or(GitSyncError::SandboxUnavailable)?;
    let head = inspection.remote_head.object_id();
    let state_before = inspection
        .staging
        .git_state(CLONE_STAGING_FILE_LIMIT)
        .map_err(map_clone_staging_to_sync)?;
    let shallow_before = run_clone_command(
        clone_read_command(
            &executable,
            &sandbox,
            inspection.staging.git_path(),
            inspection.staging.git_descriptor(),
            &["rev-parse", "--is-shallow-repository"],
        ),
        cancelled,
        CLONE_LOCAL_READ_TIMEOUT,
        16,
    )
    .map_err(map_clone_discovery_to_sync)?;
    let history = run_clone_command(
        clone_read_command(
            &executable,
            &sandbox,
            inspection.staging.git_path(),
            inspection.staging.git_descriptor(),
            &["rev-list", "--max-count=4097", head],
        ),
        cancelled,
        CLONE_LOCAL_READ_TIMEOUT,
        SYNC_HISTORY_OUTPUT_LIMIT,
    )
    .map_err(map_clone_discovery_to_sync)?;
    let shallow_after = run_clone_command(
        clone_read_command(
            &executable,
            &sandbox,
            inspection.staging.git_path(),
            inspection.staging.git_descriptor(),
            &["rev-parse", "--is-shallow-repository"],
        ),
        cancelled,
        CLONE_LOCAL_READ_TIMEOUT,
        16,
    )
    .map_err(map_clone_discovery_to_sync)?;
    let state_after = inspection
        .staging
        .git_state(CLONE_STAGING_FILE_LIMIT)
        .map_err(map_clone_staging_to_sync)?;
    if state_before != state_after || shallow_before.stdout != shallow_after.stdout {
        return Err(GitSyncError::PlanStale);
    }
    let shallow = parse_git_shallow_state(&shallow_after.stdout)?;
    parse_git_sync_history(
        &history.stdout,
        inspection.remote_head.object_format(),
        head,
        shallow,
    )
}

fn read_local_sync_history(
    workspace: &WorkspaceView,
    plan: &GitSyncPlan,
) -> Result<GitSyncHistory, GitSyncError> {
    let executable = trusted_executable("/usr/bin/git").ok_or(GitSyncError::GitUnavailable)?;
    let sandbox = trusted_executable("/usr/bin/bwrap").ok_or(GitSyncError::SandboxUnavailable)?;
    let capability = workspace
        .git_repository_capability()
        .map_err(map_git_capability_to_sync)?;
    if capability
        .git_directory_identity()
        .map_err(map_git_snapshot_to_sync)?
        != plan.preflight.git_identity
    {
        return Err(GitSyncError::PlanStale);
    }
    let repository_before = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    let retained_shallow_before = capability
        .read_retained_shallow_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    if repository_before != plan.preflight.repository
        || retained_shallow_before != repository_before.shallow
    {
        return Err(GitSyncError::PlanStale);
    }
    let shallow = repository_before.shallow.is_some();
    let head = plan
        .preflight
        .branch
        .expected_object_id
        .as_deref()
        .ok_or(GitSyncError::Unsupported)?;
    let output = run_git_commit_read(
        &executable,
        &sandbox,
        &capability,
        plan.preflight.object_format,
        &["rev-list", "--max-count=4097", head],
        SYNC_HISTORY_OUTPUT_LIMIT,
    )
    .map_err(map_git_read_to_sync)?;
    let repository_after = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    let retained_shallow_after = capability
        .read_retained_shallow_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    if repository_after != repository_before
        || retained_shallow_after != retained_shallow_before
        || retained_shallow_after != repository_after.shallow
    {
        return Err(GitSyncError::PlanStale);
    }
    parse_git_sync_history(&output.stdout, plan.preflight.object_format, head, shallow)
}

fn parse_git_shallow_state(bytes: &[u8]) -> Result<bool, GitSyncError> {
    match bytes {
        b"true\n" => Ok(true),
        b"false\n" => Ok(false),
        _ => Err(GitSyncError::InvalidRepository),
    }
}

fn parse_git_sync_history(
    bytes: &[u8],
    object_format: GitObjectFormat,
    expected_head: &str,
    shallow: bool,
) -> Result<GitSyncHistory, GitSyncError> {
    if bytes.is_empty() || !bytes.ends_with(b"\n") {
        return Err(GitSyncError::InvalidRepository);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| GitSyncError::InvalidRepository)?;
    let mut object_ids = Vec::new();
    for line in text.trim_end_matches('\n').split('\n') {
        if validate_object_id(line) != Some(object_format)
            || object_ids.iter().any(|existing| existing == line)
            || object_ids.len() > SYNC_HISTORY_LIMIT
        {
            return Err(GitSyncError::InvalidRepository);
        }
        object_ids.push(line.to_owned());
    }
    if object_ids.first().map(String::as_str) != Some(expected_head) {
        return Err(GitSyncError::InvalidRepository);
    }
    let complete = !shallow && object_ids.len() <= SYNC_HISTORY_LIMIT;
    Ok(GitSyncHistory {
        object_ids,
        complete,
    })
}

fn classify_git_sync_histories(
    local_head: &str,
    remote_head: &str,
    local: &GitSyncHistory,
    remote: &GitSyncHistory,
) -> GitSyncAncestry {
    if local_head == remote_head {
        GitSyncAncestry::UpToDate
    } else if remote
        .object_ids
        .iter()
        .any(|object_id| object_id == local_head)
    {
        GitSyncAncestry::RemoteFastForward
    } else if local
        .object_ids
        .iter()
        .any(|object_id| object_id == remote_head)
    {
        GitSyncAncestry::LocalAhead
    } else if local.complete && remote.complete {
        GitSyncAncestry::Diverged
    } else {
        GitSyncAncestry::HistoryLimit
    }
}

fn revalidate_git_sync_plan(
    workspace: &WorkspaceView,
    plan: &GitSyncPlan,
) -> Result<(), GitSyncError> {
    let (current, target) = inspect_git_sync_repository(workspace)?;
    if !same_git_sync_preflight(&plan.preflight, &current)
        || target.remote_name != plan.remote_name
        || target.remote_url != plan.remote_url
        || target.transport != plan.transport
        || target.host != plan.host
        || target.remote_branch != plan.remote_branch
        || target.remote_ref != plan.remote_ref
    {
        return Err(GitSyncError::PlanStale);
    }
    Ok(())
}

fn same_git_sync_preflight(
    planned: &GitSyncRepositoryPreflight,
    current: &GitSyncRepositoryPreflight,
) -> bool {
    planned.workspace_id == current.workspace_id
        && planned.workspace_fingerprint == current.workspace_fingerprint
        && planned.manifest_revision == current.manifest_revision
        && planned.root_capability_hash == current.root_capability_hash
        && planned.git_identity == current.git_identity
        && planned.repository == current.repository
        && planned.branch_storage == current.branch_storage
        && planned.branch == current.branch
        && planned.object_format == current.object_format
        && planned.managed == current.managed
        && planned.working_tree_clean == current.working_tree_clean
}

fn inspect_git_sync_repository(
    workspace: &WorkspaceView,
) -> Result<(GitSyncRepositoryPreflight, GitSyncTarget), GitSyncError> {
    if !workspace.issues().is_empty() {
        return Err(GitSyncError::WorkspaceIssues);
    }
    let facts = workspace
        .git_initialization_facts()
        .map_err(map_git_initialization_to_sync)?;
    if !facts.writable() {
        return Err(GitSyncError::ReadOnly);
    }
    if !facts.git_probe_complete() || facts.unsafe_git_marker() || facts.nested_git_repository() {
        return Err(GitSyncError::Unsafe);
    }
    let capability = workspace
        .git_repository_capability()
        .map_err(map_git_capability_to_sync)?;
    let repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    if capability
        .read_retained_shallow_snapshot()
        .map_err(map_git_snapshot_to_sync)?
        != repository.shallow
    {
        return Err(GitSyncError::PlanStale);
    }
    let executable = trusted_executable("/usr/bin/git").ok_or(GitSyncError::GitUnavailable)?;
    let config_output = run_git(
        &executable,
        None,
        &[
            "config",
            "--file",
            capability.config_path(),
            "--no-includes",
            "-z",
            "--list",
        ],
    )
    .map_err(map_git_read_to_sync)?;
    let descriptor_config = fs::read(capability.config_path()).map_err(|_| GitSyncError::Io)?;
    if descriptor_config != repository.config.bytes {
        return Err(GitSyncError::PlanStale);
    }
    let object_format = parse_git_object_format(&config_output.stdout)
        .map_err(|_| GitSyncError::InvalidConfiguration)?;
    let reference = parse_git_head_reference(&repository.head.bytes)
        .map_err(|_| GitSyncError::InvalidConfiguration)?;
    let branch_relative = reference
        .strip_prefix("refs/heads/")
        .ok_or(GitSyncError::InvalidConfiguration)?;
    let branch_storage = capability
        .read_commit_branch_snapshot(branch_relative)
        .map_err(map_git_snapshot_to_sync)?;
    let branch = parse_git_commit_branch(
        &repository.head.bytes,
        branch_storage
            .target
            .as_ref()
            .map(|target| target.bytes.as_slice()),
        repository
            .packed_refs
            .as_ref()
            .map(|packed| packed.bytes.as_slice()),
        object_format,
    )
    .map_err(|_| GitSyncError::InvalidRepository)?;
    if branch.expected_object_id.is_none() {
        return Err(GitSyncError::Unsupported);
    }
    let target = parse_git_sync_target(&config_output.stdout, branch_relative)?;
    let git_identity = capability
        .git_directory_identity()
        .map_err(map_git_snapshot_to_sync)?;
    let managed = inspect_git_commit_managed_inventory(workspace, object_format)
        .map_err(map_commit_plan_to_sync)?;
    let working_tree_clean = match read_git_status(workspace)
        .map_err(map_git_read_to_sync)?
        .repository
    {
        GitRepositoryStatus::Ready(status) => status.clean(),
        GitRepositoryStatus::NotRepository => return Err(GitSyncError::NotRepository),
        GitRepositoryStatus::Unsafe => return Err(GitSyncError::Unsafe),
    };
    let (workspace_id, manifest_revision, root_capability_hash) =
        workspace.git_initialization_binding();
    let final_repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_snapshot_to_sync)?;
    let final_branch = capability
        .read_commit_branch_snapshot(branch_relative)
        .map_err(map_git_snapshot_to_sync)?;
    if final_repository != repository || final_branch != branch_storage {
        return Err(GitSyncError::PlanStale);
    }
    Ok((
        GitSyncRepositoryPreflight {
            workspace_id,
            workspace_fingerprint: workspace_validation_fingerprint(workspace),
            manifest_revision,
            root_capability_hash,
            git_identity,
            repository,
            branch_storage,
            branch,
            object_format,
            managed,
            working_tree_clean,
        },
        target,
    ))
}

/// Publishes only the already validated portable Workspace content through the
/// recovery journal. Git objects/config/refs remain a later finalization gate.
#[allow(
    dead_code,
    reason = "connected by the Phase 5B3C2 native finalization gate"
)]
pub(crate) fn apply_git_clone_content_materialization(
    destination: &EmptyWorkspaceTarget,
    validation: &GitCloneValidation,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<Box<WorkspaceView>, GitCloneMaterializeError> {
    if !destination
        .matches_clone_destination(validation.plan.destination)
        .map_err(|_| GitCloneMaterializeError::DestinationChanged)?
    {
        return Err(GitCloneMaterializeError::DestinationChanged);
    }
    let tree = validation.validated.tree();
    let mut sources = Vec::new();
    sources
        .try_reserve_exact(tree.entries().len())
        .map_err(|_| GitCloneMaterializeError::ResourceLimit)?;
    for entry in tree.entries() {
        sources.push(GitCloneMaterializationSource {
            relative_path: entry.relative_path().to_owned(),
            size: entry.size(),
        });
    }
    let marker = git_clone_materialization_marker(
        validation.operation_id,
        tree.plan_hash(),
        validation.plan.destination.root_hash(),
    );
    execute_git_clone_materialization(
        destination,
        state_home,
        runtime_directory,
        *validation.validated.workspace().manifest().workspace_id(),
        validation.operation_id,
        validation.created_at,
        tree.plan_hash(),
        &marker,
        &sources,
        |index, relative_path, size| {
            let entry = tree
                .entries()
                .get(index)
                .ok_or(GitCloneMaterializationStorageError::SourceChanged)?;
            if entry.relative_path() != relative_path || entry.size() != size {
                return Err(GitCloneMaterializationStorageError::SourceChanged);
            }
            let bytes = validation
                .staging
                .read_validation_file(relative_path, size)
                .map_err(|error| match map_clone_materialization_staging(error) {
                    GitCloneMaterializeError::ResourceLimit => {
                        GitCloneMaterializationStorageError::TooLarge
                    }
                    GitCloneMaterializeError::Unsupported => {
                        GitCloneMaterializationStorageError::Unsupported
                    }
                    GitCloneMaterializeError::Io => GitCloneMaterializationStorageError::Io,
                    _ => GitCloneMaterializationStorageError::SourceChanged,
                })?;
            if !git_blob_object_id_matches(tree.object_format(), &bytes, entry.object_id()) {
                return Err(GitCloneMaterializationStorageError::SourceChanged);
            }
            Ok(bytes)
        },
    )
    .map_err(map_clone_materialization_storage)?;
    let expected_paths = sources
        .iter()
        .map(|source| source.relative_path.clone())
        .collect::<Vec<_>>();
    destination
        .verify_git_clone_materialization(&marker, &expected_paths)
        .map_err(map_clone_materialization_inspection)?;
    for entry in tree.entries() {
        let bytes = destination
            .read_git_clone_materialized_file(entry.relative_path(), entry.size())
            .map_err(map_clone_materialization_inspection)?;
        if !git_blob_object_id_matches(tree.object_format(), &bytes, entry.object_id()) {
            return Err(GitCloneMaterializeError::ValidationMismatch);
        }
    }
    let workspace =
        open_initialized_workspace(destination).map_err(map_clone_materialization_inspection)?;
    if !workspace.issues().is_empty()
        || workspace_validation_fingerprint(&workspace)
            != workspace_validation_fingerprint(validation.validated.workspace())
    {
        return Err(GitCloneMaterializeError::ValidationMismatch);
    }
    destination
        .verify_git_clone_materialization(&marker, &expected_paths)
        .map_err(map_clone_materialization_inspection)?;
    Ok(workspace)
}

pub fn apply_git_clone_repository_finalization(
    destination: &EmptyWorkspaceTarget,
    validation: &GitCloneValidation,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<Box<WorkspaceView>, GitCloneMaterializeError> {
    if !destination
        .matches_clone_destination(validation.plan.destination)
        .map_err(|_| GitCloneMaterializeError::DestinationChanged)?
    {
        return Err(GitCloneMaterializeError::DestinationChanged);
    }
    let tree = validation.validated.tree();
    let marker = git_clone_materialization_marker(
        validation.operation_id,
        tree.plan_hash(),
        validation.plan.destination.root_hash(),
    );
    let expected_paths = tree
        .entries()
        .iter()
        .map(|entry| entry.relative_path().to_owned())
        .collect::<Vec<_>>();
    if destination
        .verify_git_clone_portable_content(&marker, &expected_paths)
        .is_err()
    {
        apply_git_clone_content_materialization(
            destination,
            validation,
            state_home,
            runtime_directory,
        )?;
    }
    destination
        .verify_git_clone_portable_content(&marker, &expected_paths)
        .map_err(map_clone_materialization_inspection)?;
    for entry in tree.entries() {
        let bytes = destination
            .read_git_clone_materialized_file(entry.relative_path(), entry.size())
            .map_err(map_clone_materialization_inspection)?;
        if !git_blob_object_id_matches(tree.object_format(), &bytes, entry.object_id()) {
            return Err(GitCloneMaterializeError::ValidationMismatch);
        }
    }

    let cancelled = AtomicBool::new(false);
    let current_tree =
        inspect_fetched_git_clone_tree(&validation.staging, &validation.remote_head, &cancelled)
            .map_err(|_| GitCloneMaterializeError::StagingChanged)?;
    if current_tree.plan_hash() != tree.plan_hash() || current_tree.commit_id() != tree.commit_id()
    {
        return Err(GitCloneMaterializeError::StagingChanged);
    }
    let index_staging_name =
        prepare_clone_index(&validation.staging, validation.remote_head.object_id())?;
    let index = validation
        .staging
        .inspect_index_source(&index_staging_name)
        .map_err(map_clone_materialization_staging)?;
    validation
        .staging
        .verify_git_size_limit(CLONE_STAGING_FILE_LIMIT)
        .map_err(map_clone_materialization_staging)?;
    let packs = validation
        .staging
        .inspect_pack_sources(
            validation.remote_head.object_id().len(),
            CLONE_STAGING_FILE_LIMIT,
        )
        .map_err(map_clone_materialization_staging)?;
    let branch_relative = validation
        .remote_head
        .branch_ref()
        .strip_prefix("refs/heads/")
        .ok_or(GitCloneMaterializeError::StagingChanged)?;
    let config = clone_worktree_config(
        validation.remote_head.object_format(),
        validation.plan.remote_url(),
        branch_relative,
    )?;
    let head = format!("ref: {}\n", validation.remote_head.branch_ref()).into_bytes();
    let shallow = format!("{}\n", validation.remote_head.object_id()).into_bytes();
    let completion_seal = git_clone_completion_seal(
        validation.operation_id,
        tree.plan_hash(),
        validation.plan.destination.root_hash(),
    );
    finalize_git_clone_repository(
        destination,
        &marker,
        &completion_seal,
        &packs,
        &index,
        &config,
        &head,
        &shallow,
        branch_relative,
        validation.remote_head.object_id().as_bytes(),
        |source| {
            let opened = if source.name == "index" {
                validation
                    .staging
                    .open_index_source(&index_staging_name, source)
            } else {
                validation.staging.open_pack_source(source)
            };
            opened.map_err(|error| match map_clone_materialization_staging(error) {
                GitCloneMaterializeError::ResourceLimit => {
                    GitCloneMaterializationStorageError::TooLarge
                }
                GitCloneMaterializeError::Unsupported => {
                    GitCloneMaterializationStorageError::Unsupported
                }
                GitCloneMaterializeError::Io => GitCloneMaterializationStorageError::Io,
                _ => GitCloneMaterializationStorageError::SourceChanged,
            })
        },
    )
    .map_err(map_clone_materialization_storage)?;

    let workspace =
        open_initialized_workspace(destination).map_err(map_clone_materialization_inspection)?;
    let status = read_git_status(&workspace).map_err(|_| GitCloneMaterializeError::Io)?;
    let GitRepositoryStatus::Ready(status) = status.repository() else {
        return Err(GitCloneMaterializeError::ValidationMismatch);
    };
    if status.branch() != Some(branch_relative)
        || status.detached()
        || status.upstream() != Some(&format!("origin/{branch_relative}"))
        || status.remotes() != ["origin"]
        || !status.clean()
        || !status.divergence_known()
        || status.ahead() != 0
        || status.behind() != 0
    {
        return Err(GitCloneMaterializeError::ValidationMismatch);
    }
    #[cfg(test)]
    if CLONE_PRESEAL_PAUSE.get() {
        thread::sleep(Duration::from_millis(50));
    }
    destination
        .verify_git_clone_portable_content(&marker, &expected_paths)
        .map_err(map_clone_materialization_inspection)?;
    seal_git_clone_repository(
        destination,
        &marker,
        &completion_seal,
        &packs,
        &index,
        &config,
        &head,
        &shallow,
        branch_relative,
        validation.remote_head.object_id().as_bytes(),
    )
    .map_err(map_clone_materialization_storage)?;
    destination
        .verify_git_clone_portable_content(&marker, &expected_paths)
        .map_err(map_clone_materialization_inspection)?;
    Ok(workspace)
}

fn clone_worktree_config(
    object_format: GitObjectFormat,
    remote_url: &str,
    branch_relative: &str,
) -> Result<Vec<u8>, GitCloneMaterializeError> {
    let remote_url = git_config_quote(remote_url)?;
    let branch = git_config_quote(branch_relative)?;
    let extension = match object_format {
        GitObjectFormat::Sha1 => "",
        GitObjectFormat::Sha256 => "[extensions]\n\tobjectformat = sha256\n",
    };
    let config = format!(
        "[core]\n\trepositoryformatversion = {}\n\tfilemode = true\n\tbare = false\n\tlogallrefupdates = true\n{extension}[remote \"origin\"]\n\turl = \"{remote_url}\"\n\tfetch = +refs/heads/{branch_relative}:refs/remotes/origin/{branch_relative}\n[branch \"{branch}\"]\n\tremote = origin\n\tmerge = refs/heads/{branch_relative}\n",
        u8::from(object_format == GitObjectFormat::Sha256),
    )
    .into_bytes();
    validate_git_config(&config).map_err(|_| GitCloneMaterializeError::Unsafe)?;
    Ok(config)
}

fn git_config_quote(value: &str) -> Result<String, GitCloneMaterializeError> {
    if value.chars().any(char::is_control) {
        return Err(GitCloneMaterializeError::Unsafe);
    }
    Ok(value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn git_clone_completion_seal(
    operation_id: OperationId,
    plan_hash: [u8; 32],
    root_hash: [u8; 32],
) -> Vec<u8> {
    format!(
        "rehow-clone-complete-v1\noperation_id={operation_id}\nplan_hash={}\nroot_hash={}\n",
        hex_lower(&plan_hash),
        hex_lower(&root_hash),
    )
    .into_bytes()
}

fn map_clone_materialization_inspection(
    error: crate::storage::WorkspaceInspectError,
) -> GitCloneMaterializeError {
    match error {
        crate::storage::WorkspaceInspectError::ChangedDuringRead => {
            GitCloneMaterializeError::Conflict
        }
        crate::storage::WorkspaceInspectError::UnsafeRoot => GitCloneMaterializeError::Unsafe,
        crate::storage::WorkspaceInspectError::ResourceLimit => {
            GitCloneMaterializeError::ResourceLimit
        }
        crate::storage::WorkspaceInspectError::NotFound
        | crate::storage::WorkspaceInspectError::NotDirectory
        | crate::storage::WorkspaceInspectError::PermissionDenied
        | crate::storage::WorkspaceInspectError::Io => GitCloneMaterializeError::Io,
    }
}

fn git_clone_materialization_marker(
    operation_id: OperationId,
    plan_hash: [u8; 32],
    root_hash: [u8; 32],
) -> Vec<u8> {
    format!(
        "rehow-clone-finalize-v1\noperation_id={operation_id}\nplan_hash={}\nroot_hash={}\nstage=content\n",
        hex_lower(&plan_hash),
        hex_lower(&root_hash),
    )
    .into_bytes()
}

fn workspace_validation_fingerprint(workspace: &WorkspaceView) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(b"rehow:clone-workspace-view:v1\0");
    hash_clone_field(
        &mut hash,
        workspace.manifest().workspace_id().to_string().as_bytes(),
    );
    hash_clone_field(
        &mut hash,
        workspace.manifest_revision().to_string().as_bytes(),
    );
    for category in workspace.categories() {
        hash_clone_field(&mut hash, category.relative_directory().as_bytes());
        hash_clone_field(
            &mut hash,
            category.descriptor_revision().to_string().as_bytes(),
        );
        for guide in category.guides() {
            hash_clone_field(&mut hash, guide.relative_path().as_bytes());
            hash_clone_field(&mut hash, guide.revision().to_string().as_bytes());
        }
    }
    *hash.finalize().as_bytes()
}

fn map_clone_materialization_staging(error: GitCloneStagingError) -> GitCloneMaterializeError {
    match error {
        GitCloneStagingError::Conflict | GitCloneStagingError::Unsafe => {
            GitCloneMaterializeError::StagingChanged
        }
        GitCloneStagingError::Unsupported => GitCloneMaterializeError::Unsupported,
        GitCloneStagingError::TooLarge => GitCloneMaterializeError::ResourceLimit,
        GitCloneStagingError::Io => GitCloneMaterializeError::Io,
    }
}

fn map_clone_materialization_storage(
    error: GitCloneMaterializationStorageError,
) -> GitCloneMaterializeError {
    match error {
        GitCloneMaterializationStorageError::SourceChanged => {
            GitCloneMaterializeError::StagingChanged
        }
        GitCloneMaterializationStorageError::Busy => GitCloneMaterializeError::Busy,
        GitCloneMaterializationStorageError::Conflict => GitCloneMaterializeError::Conflict,
        GitCloneMaterializationStorageError::Unsafe => GitCloneMaterializeError::Unsafe,
        GitCloneMaterializationStorageError::Unsupported => GitCloneMaterializeError::Unsupported,
        GitCloneMaterializationStorageError::TooLarge => GitCloneMaterializeError::ResourceLimit,
        GitCloneMaterializationStorageError::Io => GitCloneMaterializeError::Io,
    }
}

fn map_clone_apply_destination(error: GitCloneError) -> GitCloneApplyError {
    match error {
        GitCloneError::Io => GitCloneApplyError::Io,
        GitCloneError::InvalidUrl
        | GitCloneError::DestinationChanged
        | GitCloneError::ReadOnly
        | GitCloneError::NestedRepository
        | GitCloneError::Unsafe => GitCloneApplyError::DestinationChanged,
    }
}

fn map_clone_apply_discovery(error: GitCloneDiscoveryError) -> GitCloneApplyError {
    match error {
        GitCloneDiscoveryError::UnsupportedTransport => GitCloneApplyError::UnsupportedTransport,
        GitCloneDiscoveryError::GitUnavailable => GitCloneApplyError::GitUnavailable,
        GitCloneDiscoveryError::SandboxUnavailable => GitCloneApplyError::SandboxUnavailable,
        GitCloneDiscoveryError::Cancelled => GitCloneApplyError::Cancelled,
        GitCloneDiscoveryError::Timeout => GitCloneApplyError::Timeout,
        GitCloneDiscoveryError::OutputLimit => GitCloneApplyError::ResourceLimit,
        GitCloneDiscoveryError::RemoteUnavailable | GitCloneDiscoveryError::RemoteRefMissing => {
            GitCloneApplyError::RemoteUnavailable
        }
        GitCloneDiscoveryError::AuthenticationFailed => GitCloneApplyError::AuthenticationFailed,
        GitCloneDiscoveryError::HostKeyRejected => GitCloneApplyError::HostKeyRejected,
        GitCloneDiscoveryError::InvalidAdvertisement => GitCloneApplyError::InvalidRepository,
        GitCloneDiscoveryError::Io => GitCloneApplyError::Io,
    }
}

fn map_clone_apply_staging(error: GitCloneStageError) -> GitCloneApplyError {
    match error {
        GitCloneStageError::DestinationChanged => GitCloneApplyError::DestinationChanged,
        GitCloneStageError::Conflict => GitCloneApplyError::StagingConflict,
        GitCloneStageError::Unsafe => GitCloneApplyError::Unsafe,
        GitCloneStageError::Unsupported => GitCloneApplyError::Unsupported,
        GitCloneStageError::ResourceLimit => GitCloneApplyError::ResourceLimit,
        GitCloneStageError::Io => GitCloneApplyError::Io,
    }
}

fn map_clone_apply_inspection(error: GitCloneInspectError) -> GitCloneApplyError {
    match error {
        GitCloneInspectError::Cancelled => GitCloneApplyError::Cancelled,
        GitCloneInspectError::Timeout => GitCloneApplyError::Timeout,
        GitCloneInspectError::OutputLimit => GitCloneApplyError::ResourceLimit,
        GitCloneInspectError::GitUnavailable => GitCloneApplyError::GitUnavailable,
        GitCloneInspectError::SandboxUnavailable => GitCloneApplyError::SandboxUnavailable,
        GitCloneInspectError::Tree(GitCloneTreeError::ResourceLimit) => {
            GitCloneApplyError::ResourceLimit
        }
        GitCloneInspectError::InvalidRepository
        | GitCloneInspectError::RefMismatch
        | GitCloneInspectError::Tree(_) => GitCloneApplyError::InvalidRepository,
        GitCloneInspectError::Io => GitCloneApplyError::Io,
    }
}

fn map_clone_apply_content(error: GitCloneContentError) -> GitCloneApplyError {
    match error {
        GitCloneContentError::Cancelled => GitCloneApplyError::Cancelled,
        GitCloneContentError::Timeout => GitCloneApplyError::Timeout,
        GitCloneContentError::OutputLimit | GitCloneContentError::ResourceLimit => {
            GitCloneApplyError::ResourceLimit
        }
        GitCloneContentError::GitUnavailable => GitCloneApplyError::GitUnavailable,
        GitCloneContentError::SandboxUnavailable => GitCloneApplyError::SandboxUnavailable,
        GitCloneContentError::InvalidRepository | GitCloneContentError::BlobMismatch => {
            GitCloneApplyError::InvalidRepository
        }
        GitCloneContentError::InvalidWorkspace => GitCloneApplyError::InvalidWorkspace,
        GitCloneContentError::WorkspaceIssues => GitCloneApplyError::WorkspaceIssues,
        GitCloneContentError::Conflict => GitCloneApplyError::StagingConflict,
        GitCloneContentError::Unsafe => GitCloneApplyError::Unsafe,
        GitCloneContentError::Unsupported => GitCloneApplyError::Unsupported,
        GitCloneContentError::Io => GitCloneApplyError::Io,
    }
}

impl fmt::Debug for ValidatedGitClone {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValidatedGitClone")
            .field("tree", &self.tree)
            .field("workspace", &"<validated>")
            .finish()
    }
}

impl ValidatedGitClone {
    #[allow(dead_code, reason = "connected by the Phase 5B3B clone apply gate")]
    pub(crate) fn tree(&self) -> &GitCloneTree {
        &self.tree
    }

    #[allow(dead_code, reason = "connected by the Phase 5B3B clone apply gate")]
    pub(crate) fn workspace(&self) -> &WorkspaceView {
        &self.workspace
    }
}

/// Streams the already bounded tree into a private mirror and validates it
/// through the canonical Workspace loader. The selected destination remains
/// unopened by the child and unchanged by this operation.
#[allow(dead_code, reason = "connected by the Phase 5B3B clone apply gate")]
pub(crate) fn validate_fetched_git_clone(
    staging: &GitCloneStaging,
    tree: GitCloneTree,
    cancelled: &AtomicBool,
) -> Result<ValidatedGitClone, GitCloneContentError> {
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCloneContentError::GitUnavailable)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCloneContentError::SandboxUnavailable)?;
    stream_clone_tree(
        clone_batch_read_command(
            &executable,
            &sandbox,
            staging.git_path(),
            staging.git_descriptor(),
        ),
        staging,
        &tree,
        cancelled,
    )?;
    staging
        .finish_validation_tree()
        .map_err(map_clone_staging_content_error)?;
    let inspection = inspect_workspace(Path::new(staging.validation_path()))
        .map_err(|_| GitCloneContentError::InvalidWorkspace)?;
    let (_, classification) = inspection.into_parts();
    let WorkspaceInspection::Compatible(workspace) = classification else {
        return Err(GitCloneContentError::InvalidWorkspace);
    };
    if !workspace.issues().is_empty() {
        return Err(GitCloneContentError::WorkspaceIssues);
    }
    Ok(ValidatedGitClone { tree, workspace })
}

fn map_clone_staging_content_error(error: GitCloneStagingError) -> GitCloneContentError {
    match error {
        GitCloneStagingError::Conflict => GitCloneContentError::Conflict,
        GitCloneStagingError::Unsafe => GitCloneContentError::Unsafe,
        GitCloneStagingError::Unsupported => GitCloneContentError::Unsupported,
        GitCloneStagingError::TooLarge => GitCloneContentError::ResourceLimit,
        GitCloneStagingError::Io => GitCloneContentError::Io,
    }
}

fn stream_clone_tree(
    mut command: Command,
    staging: &GitCloneStaging,
    tree: &GitCloneTree,
    cancelled: &AtomicBool,
) -> Result<(), GitCloneContentError> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(GitCloneContentError::Cancelled);
    }
    let mut child = command.spawn().map_err(|_| GitCloneContentError::Io)?;
    let stdin = child.stdin.take().ok_or(GitCloneContentError::Io)?;
    let stdout = child.stdout.take().ok_or(GitCloneContentError::Io)?;
    let stderr = child.stderr.take().ok_or(GitCloneContentError::Io)?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stderr_reader =
        read_capped_with_limit(stderr, Arc::clone(&exceeded), CLONE_DISCOVERY_OUTPUT_LIMIT);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let started = Instant::now();
    let mut worker_result = None;
    let mut process_status = None;
    let mut terminal_error = None;

    thread::scope(|scope| {
        scope.spawn(move || {
            let result = stream_clone_blobs(stdin, stdout, staging, tree, cancelled);
            let _ = sender.send(result);
        });
        loop {
            if worker_result.is_none() {
                match receiver.try_recv() {
                    Ok(result) => worker_result = Some(result),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        terminal_error = Some(GitCloneContentError::Io);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
            if process_status.is_none() {
                match child.try_wait() {
                    Ok(Some(status)) => process_status = Some(status),
                    Ok(None) => {}
                    Err(_) => terminal_error = Some(GitCloneContentError::Io),
                }
            }
            if worker_result.is_some() && process_status.is_some() {
                break;
            }
            if terminal_error.is_none() {
                terminal_error = if cancelled.load(Ordering::Relaxed) {
                    Some(GitCloneContentError::Cancelled)
                } else if exceeded.load(Ordering::Relaxed) {
                    Some(GitCloneContentError::OutputLimit)
                } else if started.elapsed() >= CLONE_LOCAL_READ_TIMEOUT {
                    Some(GitCloneContentError::Timeout)
                } else {
                    None
                };
            }
            if terminal_error.is_some() {
                terminate_child(&mut child);
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    });
    let _ = stderr_reader.join().map_err(|_| GitCloneContentError::Io)?;
    if let Some(error) = terminal_error {
        return Err(error);
    }
    let result = worker_result.ok_or(GitCloneContentError::Io)?;
    result?;
    if process_status.is_some_and(|status| status.success()) {
        Ok(())
    } else {
        Err(GitCloneContentError::InvalidRepository)
    }
}

fn stream_clone_blobs(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    staging: &GitCloneStaging,
    tree: &GitCloneTree,
    cancelled: &AtomicBool,
) -> Result<(), GitCloneContentError> {
    let mut writer = BufWriter::new(stdin);
    let mut reader = BufReader::new(stdout);
    for entry in tree.entries() {
        if cancelled.load(Ordering::Relaxed) {
            return Err(GitCloneContentError::Cancelled);
        }
        writer
            .write_all(entry.object_id().as_bytes())
            .and_then(|_| writer.write_all(b"\n"))
            .and_then(|_| writer.flush())
            .map_err(|_| GitCloneContentError::Io)?;
        let header = read_clone_batch_header(&mut reader)?;
        let expected = format!("{} blob {}\n", entry.object_id(), entry.size());
        if header != expected.as_bytes() {
            return Err(GitCloneContentError::BlobMismatch);
        }
        let size =
            usize::try_from(entry.size()).map_err(|_| GitCloneContentError::ResourceLimit)?;
        let mut bytes = vec![0_u8; size];
        reader
            .read_exact(&mut bytes)
            .map_err(|_| GitCloneContentError::BlobMismatch)?;
        let mut terminator = [0_u8; 1];
        reader
            .read_exact(&mut terminator)
            .map_err(|_| GitCloneContentError::BlobMismatch)?;
        if terminator != *b"\n" {
            return Err(GitCloneContentError::BlobMismatch);
        }
        if !git_blob_object_id_matches(tree.object_format(), &bytes, entry.object_id()) {
            return Err(GitCloneContentError::BlobMismatch);
        }
        staging
            .write_validation_file(entry.relative_path(), &bytes)
            .map_err(map_clone_staging_content_error)?;
    }
    drop(writer);
    let mut unexpected = [0_u8; 1];
    if reader
        .read(&mut unexpected)
        .map_err(|_| GitCloneContentError::Io)?
        != 0
    {
        return Err(GitCloneContentError::BlobMismatch);
    }
    Ok(())
}

fn git_blob_object_id_matches(format: GitObjectFormat, bytes: &[u8], expected: &str) -> bool {
    git_blob_object_id(format, bytes) == expected
}

fn git_blob_object_id(format: GitObjectFormat, bytes: &[u8]) -> String {
    git_object_id(format, "blob", bytes)
}

fn git_object_id(format: GitObjectFormat, kind: &str, bytes: &[u8]) -> String {
    use sha1::Digest as _;

    let header = format!("{kind} {}\0", bytes.len());
    match format {
        GitObjectFormat::Sha1 => {
            let mut digest = sha1::Sha1::new();
            digest.update(header.as_bytes());
            digest.update(bytes);
            hex_lower(&digest.finalize())
        }
        GitObjectFormat::Sha256 => {
            let mut digest = sha2::Sha256::new();
            digest.update(header.as_bytes());
            digest.update(bytes);
            hex_lower(&digest.finalize())
        }
    }
}

fn read_clone_batch_header(reader: &mut impl Read) -> Result<Vec<u8>, GitCloneContentError> {
    let mut header = Vec::with_capacity(96);
    while header.len() < CLONE_BATCH_HEADER_LIMIT {
        let mut byte = [0_u8; 1];
        reader
            .read_exact(&mut byte)
            .map_err(|_| GitCloneContentError::BlobMismatch)?;
        header.push(byte[0]);
        if byte[0] == b'\n' {
            return Ok(header);
        }
    }
    Err(GitCloneContentError::BlobMismatch)
}

fn map_clone_inspect_process(error: GitCloneDiscoveryError) -> GitCloneInspectError {
    match error {
        GitCloneDiscoveryError::Cancelled => GitCloneInspectError::Cancelled,
        GitCloneDiscoveryError::Timeout => GitCloneInspectError::Timeout,
        GitCloneDiscoveryError::OutputLimit => GitCloneInspectError::OutputLimit,
        GitCloneDiscoveryError::GitUnavailable => GitCloneInspectError::GitUnavailable,
        GitCloneDiscoveryError::SandboxUnavailable => GitCloneInspectError::SandboxUnavailable,
        GitCloneDiscoveryError::RemoteUnavailable
        | GitCloneDiscoveryError::RemoteRefMissing
        | GitCloneDiscoveryError::AuthenticationFailed
        | GitCloneDiscoveryError::HostKeyRejected
        | GitCloneDiscoveryError::InvalidAdvertisement
        | GitCloneDiscoveryError::UnsupportedTransport => GitCloneInspectError::InvalidRepository,
        GitCloneDiscoveryError::Io => GitCloneInspectError::Io,
    }
}

fn run_clone_command(
    command: Command,
    cancelled: &AtomicBool,
    timeout: Duration,
    output_limit: usize,
) -> Result<ProcessOutput, GitCloneDiscoveryError> {
    run_clone_command_inner(
        command,
        cancelled,
        timeout,
        output_limit,
        false,
        false,
        false,
    )
}

fn run_clone_command_for_transport(
    command: Command,
    cancelled: &AtomicBool,
    timeout: Duration,
    output_limit: usize,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<ProcessOutput, GitCloneDiscoveryError> {
    run_clone_command_inner(
        command,
        cancelled,
        timeout,
        output_limit,
        false,
        transport == GitRemoteTransport::Ssh,
        transport == GitRemoteTransport::Https && authentication.https().is_some(),
    )
}

fn run_clone_command_for_transport_with_missing_ref(
    command: Command,
    cancelled: &AtomicBool,
    timeout: Duration,
    output_limit: usize,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<ProcessOutput, GitCloneDiscoveryError> {
    run_clone_command_inner(
        command,
        cancelled,
        timeout,
        output_limit,
        true,
        transport == GitRemoteTransport::Ssh,
        transport == GitRemoteTransport::Https && authentication.https().is_some(),
    )
}

fn run_clone_command_inner(
    mut command: Command,
    cancelled: &AtomicBool,
    timeout: Duration,
    output_limit: usize,
    classify_missing_ref: bool,
    classify_ssh: bool,
    classify_https_authentication: bool,
) -> Result<ProcessOutput, GitCloneDiscoveryError> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(GitCloneDiscoveryError::Cancelled);
    }
    let mut child = command.spawn().map_err(|_| GitCloneDiscoveryError::Io)?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take().ok_or(GitCloneDiscoveryError::Io)?;
    let stderr = child.stderr.take().ok_or(GitCloneDiscoveryError::Io)?;
    let stdout_reader = read_capped_with_limit(stdout, Arc::clone(&exceeded), output_limit);
    let stderr_reader = read_capped_with_limit(stderr, Arc::clone(&exceeded), output_limit);
    let started = Instant::now();
    let status = loop {
        let error = if cancelled.load(Ordering::Relaxed) {
            Some(GitCloneDiscoveryError::Cancelled)
        } else if exceeded.load(Ordering::Relaxed) {
            Some(GitCloneDiscoveryError::OutputLimit)
        } else if started.elapsed() >= timeout {
            Some(GitCloneDiscoveryError::Timeout)
        } else {
            None
        };
        if let Some(error) = error {
            terminate_child(&mut child);
            break Err(error);
        }
        match child.try_wait().map_err(|_| GitCloneDiscoveryError::Io)? {
            Some(status) => break Ok(status),
            None => thread::sleep(Duration::from_millis(2)),
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| GitCloneDiscoveryError::Io)?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| GitCloneDiscoveryError::Io)?;
    let status = status?;
    if exceeded.load(Ordering::Relaxed) {
        return Err(GitCloneDiscoveryError::OutputLimit);
    }
    if classify_missing_ref && status.code() == Some(2) {
        return Err(GitCloneDiscoveryError::RemoteRefMissing);
    }
    if !status.success() {
        if classify_ssh {
            if stderr_contains(&stderr, b"Host key verification failed")
                || stderr_contains(&stderr, b"REMOTE HOST IDENTIFICATION HAS CHANGED")
                || stderr_contains(&stderr, b"No ED25519 host key is known")
                || stderr_contains(&stderr, b"No RSA host key is known")
                || stderr_contains(&stderr, b"No ECDSA host key is known")
            {
                return Err(GitCloneDiscoveryError::HostKeyRejected);
            }
            if stderr_contains(&stderr, b"Permission denied (publickey")
                || stderr_contains(&stderr, b"Permission denied, please try again")
            {
                return Err(GitCloneDiscoveryError::AuthenticationFailed);
            }
        }
        if classify_https_authentication
            && (stderr_contains(&stderr, b"Authentication failed")
                || stderr_contains(&stderr, b"returned error: 401")
                || stderr_contains(&stderr, b"could not read Username"))
        {
            return Err(GitCloneDiscoveryError::AuthenticationFailed);
        }
        return Err(GitCloneDiscoveryError::RemoteUnavailable);
    }
    Ok(ProcessOutput { stdout })
}

fn stderr_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
fn clone_discovery_command(executable: &Path, sandbox: &Path, remote_url: &str) -> Command {
    clone_discovery_command_with_transport(
        executable,
        sandbox,
        remote_url,
        GitRemoteTransport::Https,
        GitRemoteAuthentication::Anonymous,
    )
}

fn clone_discovery_command_with_transport(
    executable: &Path,
    sandbox: &Path,
    remote_url: &str,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Command {
    remote_discovery_command(
        executable,
        sandbox,
        remote_url,
        "HEAD",
        true,
        transport,
        authentication,
    )
}

fn sync_discovery_command(
    executable: &Path,
    sandbox: &Path,
    remote_url: &str,
    remote_ref: &str,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Command {
    remote_discovery_command(
        executable,
        sandbox,
        remote_url,
        remote_ref,
        false,
        transport,
        authentication,
    )
}

fn remote_discovery_command(
    executable: &Path,
    sandbox: &Path,
    remote_url: &str,
    remote_ref: &str,
    symref: bool,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Command {
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/home/rehow")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--share-net",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--ro-bind",
            "/etc",
            "/etc",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--dir",
            "/home",
            "--dir",
            "/home/rehow",
            "--dir",
            "/run",
            "--dir",
            "/run/systemd",
            "--ro-bind-try",
            "/run/systemd/resolve",
            "/run/systemd/resolve",
            "--chdir",
            "/tmp",
        ]);
    if transport == GitRemoteTransport::Ssh {
        command
            .env("GIT_SSH", "/home/rehow/bin/rehow-git-ssh")
            .env("GIT_SSH_VARIANT", "ssh");
        if let Some(ssh) = authentication.ssh() {
            ssh.add_bubblewrap_mounts(&mut command);
        }
    } else if let Some(credentials) = authentication.https()
        && let Some(host) = https_credential_host(remote_url)
    {
        credentials.configure_bubblewrap(&mut command, &host);
    }
    command.arg("--").arg(executable).args([
        "--no-pager",
        "--literal-pathspecs",
        "-c",
        "credential.helper=",
    ]);
    if authentication.https().is_some() {
        command.args([
            "-c",
            credential_proxy::CREDENTIAL_HELPER_CONFIG,
            "-c",
            "credential.useHttpPath=false",
        ]);
    }
    command
        .args([
            "-c",
            "credential.interactive=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "http.followRedirects=false",
            "-c",
            "http.proxy=",
            "-c",
            "protocol.allow=never",
            "-c",
            if transport == GitRemoteTransport::Https {
                "protocol.https.allow=always"
            } else {
                "protocol.https.allow=never"
            },
            "-c",
            if transport == GitRemoteTransport::Ssh {
                "protocol.ssh.allow=always"
            } else {
                "protocol.ssh.allow=never"
            },
            "ls-remote",
        ])
        .args(symref.then_some("--symref"))
        .arg("--exit-code")
        .arg(remote_url)
        .arg(remote_ref)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    command
}

#[allow(clippy::too_many_arguments)]
fn git_push_command(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    object_format: GitObjectFormat,
    remote_url: &str,
    local_commit: &str,
    remote_ref: &str,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Result<(Command, File), GitSyncError> {
    if validate_object_id(local_commit) != Some(object_format)
        || !remote_ref.starts_with("refs/heads/")
        || !valid_ref_name(remote_ref)
    {
        return Err(GitSyncError::InvalidConfiguration);
    }
    let config = create_git_format_config(object_format).map_err(map_git_read_to_sync)?;
    let config_descriptor = config.as_raw_fd();
    let config_descriptor_text = config_descriptor.to_string();
    let refspec = format!("{local_commit}:{remote_ref}");
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/home/rehow")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--share-net",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--ro-bind",
            "/etc",
            "/etc",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/repo.git",
            "--ro-bind",
        ])
        .arg(repository.objects_path())
        .arg("/repo.git/objects")
        .args(["--dir", "/repo.git/refs", "--dir", "/repo.git/refs/heads"])
        .args(["--ro-bind"])
        .arg(repository.head_path())
        .arg("/repo.git/HEAD")
        .args([
            "--file",
            &config_descriptor_text,
            "/repo.git/config",
            "--tmpfs",
            "/repo.git/objects/info",
            "--tmpfs",
            "/tmp",
            "--dir",
            "/home",
            "--dir",
            "/home/rehow",
            "--dir",
            "/run",
            "--dir",
            "/run/systemd",
            "--ro-bind-try",
            "/run/systemd/resolve",
            "/run/systemd/resolve",
            "--chdir",
            "/tmp",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
        ]);
    for (source, destination) in [(repository.shallow_path(), "/repo.git/shallow")] {
        if let Some(source) = source {
            command.args(["--ro-bind"]).arg(source).arg(destination);
        }
    }
    if transport == GitRemoteTransport::Ssh {
        command
            .env("GIT_SSH", "/home/rehow/bin/rehow-git-ssh")
            .env("GIT_SSH_VARIANT", "ssh");
        if let Some(ssh) = authentication.ssh() {
            ssh.add_bubblewrap_mounts(&mut command);
        }
    } else if let Some(credentials) = authentication.https()
        && let Some(host) = https_credential_host(remote_url)
    {
        credentials.configure_bubblewrap(&mut command, &host);
    }
    command.args(["--"]).arg(executable).args([
        "--no-pager",
        "--literal-pathspecs",
        "-c",
        "credential.helper=",
    ]);
    if authentication.https().is_some() {
        command.args([
            "-c",
            credential_proxy::CREDENTIAL_HELPER_CONFIG,
            "-c",
            "credential.useHttpPath=false",
        ]);
    }
    command
        .args([
            "-c",
            "credential.interactive=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "http.followRedirects=false",
            "-c",
            "http.proxy=",
            "-c",
            "protocol.allow=never",
            "-c",
            if transport == GitRemoteTransport::Https {
                "protocol.https.allow=always"
            } else {
                "protocol.https.allow=never"
            },
            "-c",
            if transport == GitRemoteTransport::Ssh {
                "protocol.ssh.allow=always"
            } else {
                "protocol.ssh.allow=never"
            },
            "push",
            "--porcelain",
            "--no-verify",
            "--no-follow-tags",
            "--recurse-submodules=no",
            "--",
        ])
        .arg(remote_url)
        .arg(refspec)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    // SAFETY: only the sealed synthetic-config descriptor is exposed to the
    // fixed Bubblewrap file copy.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(config_descriptor, libc::F_GETFD);
            if flags < 0
                || libc::fcntl(config_descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok((command, config))
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
fn clone_fetch_command(
    executable: &Path,
    sandbox: &Path,
    staging_git_path: &str,
    staging_git_descriptor: i32,
    remote_url: &str,
    object_id: &str,
) -> Command {
    clone_fetch_command_with_transport(
        executable,
        sandbox,
        staging_git_path,
        staging_git_descriptor,
        remote_url,
        object_id,
        GitRemoteTransport::Https,
        GitRemoteAuthentication::Anonymous,
    )
}

#[allow(clippy::too_many_arguments)]
fn clone_fetch_command_with_transport(
    executable: &Path,
    sandbox: &Path,
    staging_git_path: &str,
    staging_git_descriptor: i32,
    remote_url: &str,
    object_id: &str,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Command {
    remote_fetch_command(
        executable,
        sandbox,
        staging_git_path,
        staging_git_descriptor,
        remote_url,
        object_id,
        1,
        CLONE_STAGING_FILE_LIMIT,
        transport,
        authentication,
    )
}

#[allow(clippy::too_many_arguments)]
fn sync_history_fetch_command(
    executable: &Path,
    sandbox: &Path,
    staging_git_path: &str,
    staging_git_descriptor: i32,
    remote_url: &str,
    object_id: &str,
    file_limit: u64,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Command {
    remote_fetch_command(
        executable,
        sandbox,
        staging_git_path,
        staging_git_descriptor,
        remote_url,
        object_id,
        SYNC_HISTORY_LIMIT,
        file_limit,
        transport,
        authentication,
    )
}

#[allow(clippy::too_many_arguments)]
fn remote_fetch_command(
    executable: &Path,
    sandbox: &Path,
    staging_git_path: &str,
    staging_git_descriptor: i32,
    remote_url: &str,
    object_id: &str,
    depth: usize,
    file_limit: u64,
    transport: GitRemoteTransport,
    authentication: GitRemoteAuthentication<'_>,
) -> Command {
    let refspec = format!("{object_id}:refs/rehow/clone/head");
    let depth = format!("--depth={depth}");
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/home/rehow")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--share-net",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--ro-bind",
            "/etc",
            "/etc",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--dir",
            "/home",
            "--dir",
            "/home/rehow",
            "--dir",
            "/run",
            "--dir",
            "/run/systemd",
            "--ro-bind-try",
            "/run/systemd/resolve",
            "/run/systemd/resolve",
            "--bind",
            staging_git_path,
            "/repo.git",
            "--chdir",
            "/tmp",
        ]);
    if transport == GitRemoteTransport::Ssh {
        command
            .env("GIT_SSH", "/home/rehow/bin/rehow-git-ssh")
            .env("GIT_SSH_VARIANT", "ssh");
        if let Some(ssh) = authentication.ssh() {
            ssh.add_bubblewrap_mounts(&mut command);
        }
    } else if let Some(credentials) = authentication.https()
        && let Some(host) = https_credential_host(remote_url)
    {
        credentials.configure_bubblewrap(&mut command, &host);
    }
    command.arg("--").arg(executable).args([
        "--no-pager",
        "--literal-pathspecs",
        "--git-dir=/repo.git",
        "-c",
        "credential.helper=",
    ]);
    if authentication.https().is_some() {
        command.args([
            "-c",
            credential_proxy::CREDENTIAL_HELPER_CONFIG,
            "-c",
            "credential.useHttpPath=false",
        ]);
    }
    command
        .args([
            "-c",
            "credential.interactive=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "fetch.fsckObjects=true",
            "-c",
            "transfer.fsckObjects=true",
            "-c",
            "fetch.unpackLimit=1",
            "-c",
            "transfer.unpackLimit=1",
            "-c",
            "pack.writeReverseIndex=false",
            "-c",
            "fetch.writeCommitGraph=false",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "-c",
            "submodule.recurse=false",
            "-c",
            "http.followRedirects=false",
            "-c",
            "http.proxy=",
            "-c",
            "protocol.version=2",
            "-c",
            "protocol.allow=never",
            "-c",
            if transport == GitRemoteTransport::Https {
                "protocol.https.allow=always"
            } else {
                "protocol.https.allow=never"
            },
            "-c",
            if transport == GitRemoteTransport::Ssh {
                "protocol.ssh.allow=always"
            } else {
                "protocol.ssh.allow=never"
            },
            "fetch",
            "--quiet",
            &depth,
            "--filter=blob:limit=67108865",
            "--no-tags",
            "--no-write-fetch-head",
            "--no-recurse-submodules",
            "--no-auto-maintenance",
            "--no-show-forced-updates",
        ])
        .arg(remote_url)
        .arg(refspec)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    configure_clone_child(&mut command, staging_git_descriptor, Some(file_limit));
    command
}

fn clone_read_command(
    executable: &Path,
    sandbox: &Path,
    staging_git_path: &str,
    staging_git_descriptor: i32,
    git_arguments: &[&str],
) -> Command {
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--ro-bind",
            staging_git_path,
            "/repo.git",
            "--chdir",
            "/tmp",
            "--",
        ])
        .arg(executable)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "--git-dir=/repo.git",
            "-c",
            "credential.helper=",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "diff.external=",
            "-c",
            "protocol.allow=never",
        ])
        .args(git_arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    configure_clone_child(&mut command, staging_git_descriptor, None);
    command
}

fn prepare_clone_index(
    staging: &GitCloneStaging,
    object_id: &str,
) -> Result<String, GitCloneMaterializeError> {
    let executable = trusted_executable("/usr/bin/git").ok_or(GitCloneMaterializeError::Io)?;
    let sandbox = trusted_executable("/usr/bin/bwrap").ok_or(GitCloneMaterializeError::Io)?;
    let staging_name = format!("rehow-index-{}", new_operation_id());
    let sandbox_index = format!("/repo.git/{staging_name}");
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--bind",
            staging.git_path(),
            "/repo.git",
            "--setenv",
            "GIT_INDEX_FILE",
        ])
        .arg(&sandbox_index)
        .args(["--chdir", "/tmp", "--"])
        .arg(executable)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "--git-dir=/repo.git",
            "-c",
            "credential.helper=",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "protocol.allow=never",
            "read-tree",
            object_id,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    configure_clone_child(&mut command, staging.git_descriptor(), None);
    let output = run_clone_command(
        command,
        &AtomicBool::new(false),
        CLONE_LOCAL_READ_TIMEOUT,
        128,
    )
    .map_err(|_| GitCloneMaterializeError::StagingChanged)?;
    if output.stdout.is_empty() {
        Ok(staging_name)
    } else {
        Err(GitCloneMaterializeError::StagingChanged)
    }
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn prepare_git_sync_candidate(
    inspection: &GitSyncInspection,
) -> Result<GitSyncCandidate, GitSyncError> {
    let staging_name = prepare_clone_index(&inspection.staging, inspection.remote_head.object_id())
        .map_err(map_clone_materialize_to_sync)?;
    let source = inspection
        .staging
        .inspect_index_source(&staging_name)
        .map_err(map_clone_staging_to_sync)?;
    let mut file = inspection
        .staging
        .open_index_source(&staging_name, &source)
        .map_err(map_clone_staging_to_sync)?;
    let size = usize::try_from(source.size).map_err(|_| GitSyncError::ResourceLimit)?;
    let mut index_bytes = Vec::new();
    index_bytes
        .try_reserve_exact(size)
        .map_err(|_| GitSyncError::ResourceLimit)?;
    file.read_to_end(&mut index_bytes)
        .map_err(|_| GitSyncError::Io)?;
    if index_bytes.len() != size {
        return Err(GitSyncError::PlanStale);
    }
    let mut index = parse_git_index(&index_bytes, inspection.remote_head.object_format())
        .map_err(map_commit_plan_to_sync)?;
    if index.has_cache_tree {
        index_bytes = git_index_without_extensions(
            &index_bytes,
            inspection.remote_head.object_format(),
            &index,
        )
        .map_err(map_commit_plan_to_sync)?;
        index = parse_git_index(&index_bytes, inspection.remote_head.object_format())
            .map_err(map_commit_plan_to_sync)?;
    }
    if index.entries.len() != inspection.validated.tree().entries().len()
        || !index
            .entries
            .iter()
            .zip(inspection.validated.tree().entries())
            .all(|(indexed, expected)| {
                indexed.mode == 0o100644
                    && indexed.path == expected.relative_path().as_bytes()
                    && hex_lower(&indexed.object_id) == expected.object_id()
            })
    {
        return Err(GitSyncError::InvalidRepository);
    }
    inspection
        .staging
        .verify_git_size_limit(CLONE_STAGING_FILE_LIMIT)
        .map_err(map_clone_staging_to_sync)?;
    let pack_sources = inspection
        .staging
        .inspect_pack_sources(
            inspection.remote_head.object_id().len(),
            CLONE_STAGING_FILE_LIMIT,
        )
        .map_err(map_clone_staging_to_sync)?;
    if pack_sources.is_empty() {
        return Err(GitSyncError::InvalidRepository);
    }
    let current_tree = inspect_fetched_git_clone_tree(
        &inspection.staging,
        &inspection.remote_head,
        &AtomicBool::new(false),
    )
    .map_err(map_clone_inspection_to_sync)?;
    if &current_tree != inspection.validated.tree() {
        return Err(GitSyncError::PlanStale);
    }
    Ok(GitSyncCandidate {
        index_bytes,
        pack_sources,
    })
}

fn clone_batch_read_command(
    executable: &Path,
    sandbox: &Path,
    staging_git_path: &str,
    staging_git_descriptor: i32,
) -> Command {
    let mut command = clone_read_command(
        executable,
        sandbox,
        staging_git_path,
        staging_git_descriptor,
        &["cat-file", "--batch"],
    );
    command.stdin(Stdio::piped());
    command
}

fn configure_clone_child(command: &mut Command, descriptor: i32, file_limit: Option<u64>) {
    // SAFETY: this closure runs after fork and before exec. It only tightens
    // the child umask, updates the selected descriptor flag, and sets an
    // inherited resource limit.
    unsafe {
        command.pre_exec(move || {
            libc::umask(0o077);
            let flags = libc::fcntl(descriptor, libc::F_GETFD);
            if flags < 0 || libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0 {
                return Err(io::Error::last_os_error());
            }
            if let Some(file_limit) = file_limit {
                let limit = libc::rlimit {
                    rlim_cur: file_limit,
                    rlim_max: file_limit,
                };
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
fn parse_git_remote_head(bytes: &[u8]) -> Result<GitRemoteHead, GitCloneDiscoveryError> {
    if bytes.is_empty() || bytes.len() > CLONE_DISCOVERY_OUTPUT_LIMIT || !bytes.ends_with(b"\n") {
        return Err(GitCloneDiscoveryError::InvalidAdvertisement);
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| GitCloneDiscoveryError::InvalidAdvertisement)?;
    let mut lines = text.strip_suffix('\n').unwrap_or(text).split('\n');
    let symbolic = lines
        .next()
        .ok_or(GitCloneDiscoveryError::InvalidAdvertisement)?;
    let direct = lines
        .next()
        .ok_or(GitCloneDiscoveryError::InvalidAdvertisement)?;
    if lines.next().is_some() {
        return Err(GitCloneDiscoveryError::InvalidAdvertisement);
    }
    let branch_ref = symbolic
        .strip_prefix("ref: ")
        .and_then(|line| line.strip_suffix("\tHEAD"))
        .ok_or(GitCloneDiscoveryError::InvalidAdvertisement)?;
    if !branch_ref.starts_with("refs/heads/") || !valid_ref_name(branch_ref) {
        return Err(GitCloneDiscoveryError::InvalidAdvertisement);
    }
    let object_id = direct
        .strip_suffix("\tHEAD")
        .ok_or(GitCloneDiscoveryError::InvalidAdvertisement)?;
    let object_format =
        validate_object_id(object_id).ok_or(GitCloneDiscoveryError::InvalidAdvertisement)?;
    Ok(GitRemoteHead {
        branch_ref: branch_ref.to_owned(),
        object_id: object_id.to_owned(),
        object_format,
    })
}

fn parse_git_sync_remote_ref(
    bytes: &[u8],
    remote_ref: &str,
) -> Result<GitRemoteHead, GitSyncError> {
    if bytes.is_empty()
        || bytes.len() > CLONE_DISCOVERY_OUTPUT_LIMIT
        || !bytes.ends_with(b"\n")
        || !valid_ref_name(remote_ref)
    {
        return Err(GitSyncError::InvalidRepository);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| GitSyncError::InvalidRepository)?;
    let line = text
        .strip_suffix('\n')
        .filter(|line| !line.contains('\n'))
        .ok_or(GitSyncError::InvalidRepository)?;
    let (object_id, advertised_ref) = line
        .split_once('\t')
        .ok_or(GitSyncError::InvalidRepository)?;
    if advertised_ref != remote_ref {
        return Err(GitSyncError::InvalidRepository);
    }
    let object_format = validate_object_id(object_id).ok_or(GitSyncError::InvalidRepository)?;
    Ok(GitRemoteHead {
        branch_ref: remote_ref.to_owned(),
        object_id: object_id.to_owned(),
        object_format,
    })
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
fn terminate_child(child: &mut std::process::Child) {
    kill_process_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

fn kill_process_group(child_id: u32) {
    if let Ok(process_group) = i32::try_from(child_id) {
        // SAFETY: confined children are placed in fresh process groups whose
        // id is the child's pid; a negative id targets only that group.
        unsafe {
            libc::kill(-process_group, libc::SIGKILL);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCloneTreeError {
    InvalidCommit,
    InvalidOutput,
    UnsupportedObject,
    UnsafePath,
    MissingManifest,
    ResourceLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
pub(crate) enum GitObjectFormat {
    Sha1,
    Sha256,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
pub(crate) struct GitCloneTreeEntry {
    relative_path: String,
    object_id: String,
    size: u64,
    kind: GitMaterializationPathKind,
}

impl GitCloneTreeEntry {
    #[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    #[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
    pub(crate) fn size(&self) -> u64 {
        self.size
    }

    #[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
    pub(crate) fn kind(&self) -> GitMaterializationPathKind {
        self.kind
    }
}

#[derive(Clone, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
pub(crate) struct GitCloneTree {
    commit_id: String,
    object_format: GitObjectFormat,
    entries: Vec<GitCloneTreeEntry>,
    plan_hash: [u8; 32],
}

impl fmt::Debug for GitCloneTree {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCloneTree")
            .field("commit_id", &"<redacted>")
            .field("object_format", &self.object_format)
            .field("entries", &self.entries.len())
            .field("plan_hash", &"<redacted>")
            .finish()
    }
}

impl GitCloneTree {
    #[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
    pub(crate) fn commit_id(&self) -> &str {
        &self.commit_id
    }

    #[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
    pub(crate) fn object_format(&self) -> GitObjectFormat {
        self.object_format
    }

    #[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
    pub(crate) fn entries(&self) -> &[GitCloneTreeEntry] {
        &self.entries
    }

    #[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
    pub(crate) fn plan_hash(&self) -> [u8; 32] {
        self.plan_hash
    }
}

#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
pub(crate) fn validate_git_clone_tree(
    commit_id: &str,
    listing: &[u8],
) -> Result<GitCloneTree, GitCloneTreeError> {
    let object_format = validate_object_id(commit_id).ok_or(GitCloneTreeError::InvalidCommit)?;
    if listing.len() > MAX_CLONE_TREE_LISTING_BYTES {
        return Err(GitCloneTreeError::ResourceLimit);
    }
    if !listing.is_empty() && !listing.ends_with(&[0]) {
        return Err(GitCloneTreeError::InvalidOutput);
    }
    if listing.is_empty() {
        return Err(GitCloneTreeError::MissingManifest);
    }
    let records = listing.strip_suffix(&[0]).unwrap_or(listing);

    let mut entries = Vec::new();
    let mut previous_path: Option<&[u8]> = None;
    let mut category_count = 0usize;
    let mut guide_count = 0usize;
    let mut attachment_count = 0usize;
    let mut managed_bytes = 0u64;
    let mut attachment_bytes = 0u64;
    let mut has_manifest = false;

    for record in records.split(|byte| *byte == 0) {
        if record.is_empty() {
            return Err(GitCloneTreeError::InvalidOutput);
        }
        if entries.len() >= MAX_CLONE_TREE_ENTRIES {
            return Err(GitCloneTreeError::ResourceLimit);
        }
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or(GitCloneTreeError::InvalidOutput)?;
        let header =
            std::str::from_utf8(&record[..tab]).map_err(|_| GitCloneTreeError::InvalidOutput)?;
        let path_bytes = &record[tab + 1..];
        let path = std::str::from_utf8(path_bytes).map_err(|_| GitCloneTreeError::UnsafePath)?;
        if previous_path.is_some_and(|previous| previous >= path_bytes) {
            return Err(GitCloneTreeError::InvalidOutput);
        }
        previous_path = Some(path_bytes);

        let mut fields = header.split_ascii_whitespace();
        let mode = fields.next().ok_or(GitCloneTreeError::InvalidOutput)?;
        let object_type = fields.next().ok_or(GitCloneTreeError::InvalidOutput)?;
        let object_id = fields.next().ok_or(GitCloneTreeError::InvalidOutput)?;
        let size_text = fields.next().ok_or(GitCloneTreeError::InvalidOutput)?;
        if fields.next().is_some()
            || mode != "100644"
            || object_type != "blob"
            || validate_object_id(object_id) != Some(object_format)
        {
            return Err(GitCloneTreeError::UnsupportedObject);
        }
        let size = parse_canonical_u64(size_text).ok_or(GitCloneTreeError::InvalidOutput)?;
        let (kind, file_limit) =
            classify_git_materialization_path(path).ok_or(GitCloneTreeError::UnsafePath)?;
        if size > file_limit {
            return Err(GitCloneTreeError::ResourceLimit);
        }
        match kind {
            GitMaterializationPathKind::Manifest => {
                if has_manifest {
                    return Err(GitCloneTreeError::InvalidOutput);
                }
                has_manifest = true;
                managed_bytes = managed_bytes
                    .checked_add(size)
                    .ok_or(GitCloneTreeError::ResourceLimit)?;
            }
            GitMaterializationPathKind::Category => {
                category_count = category_count
                    .checked_add(1)
                    .ok_or(GitCloneTreeError::ResourceLimit)?;
                managed_bytes = managed_bytes
                    .checked_add(size)
                    .ok_or(GitCloneTreeError::ResourceLimit)?;
            }
            GitMaterializationPathKind::Guide => {
                guide_count = guide_count
                    .checked_add(1)
                    .ok_or(GitCloneTreeError::ResourceLimit)?;
                managed_bytes = managed_bytes
                    .checked_add(size)
                    .ok_or(GitCloneTreeError::ResourceLimit)?;
            }
            GitMaterializationPathKind::Attachment => {
                attachment_count = attachment_count
                    .checked_add(1)
                    .ok_or(GitCloneTreeError::ResourceLimit)?;
                attachment_bytes = attachment_bytes
                    .checked_add(size)
                    .ok_or(GitCloneTreeError::ResourceLimit)?;
            }
            GitMaterializationPathKind::Ignore => {}
        }
        if category_count > CATEGORY_LIMIT
            || guide_count > GUIDE_COUNT_LIMIT
            || attachment_count > ATTACHMENT_COUNT_LIMIT
            || managed_bytes > MANAGED_BYTES_LIMIT
            || attachment_bytes > ATTACHMENT_BYTES_LIMIT
        {
            return Err(GitCloneTreeError::ResourceLimit);
        }
        entries.push(GitCloneTreeEntry {
            relative_path: path.to_owned(),
            object_id: object_id.to_owned(),
            size,
            kind,
        });
    }
    if !has_manifest {
        return Err(GitCloneTreeError::MissingManifest);
    }

    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow:git-clone-tree:v1\0");
    hash_clone_field(&mut plan, commit_id.as_bytes());
    plan.update(&(entries.len() as u64).to_le_bytes());
    for entry in &entries {
        hash_clone_field(&mut plan, entry.relative_path.as_bytes());
        hash_clone_field(&mut plan, entry.object_id.as_bytes());
        plan.update(&entry.size.to_le_bytes());
    }
    Ok(GitCloneTree {
        commit_id: commit_id.to_owned(),
        object_format,
        entries,
        plan_hash: *plan.finalize().as_bytes(),
    })
}

#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
fn validate_object_id(value: &str) -> Option<GitObjectFormat> {
    let format = match value.len() {
        40 => GitObjectFormat::Sha1,
        64 => GitObjectFormat::Sha256,
        _ => return None,
    };
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        .then_some(format)
}

fn parse_git_commit_object_id_line(
    bytes: &[u8],
    object_format: GitObjectFormat,
) -> Result<String, GitCommitPlanError> {
    let line = bytes
        .strip_suffix(b"\n")
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let value = std::str::from_utf8(line).map_err(|_| GitCommitPlanError::UnsafeRepository)?;
    if validate_object_id(value) != Some(object_format) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    Ok(value.to_owned())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitCommitTreeEntryKind {
    Tree,
    Blob,
    Commit,
}

#[derive(Eq, PartialEq)]
struct GitCommitTreeEntry {
    mode: u32,
    kind: GitCommitTreeEntryKind,
    object_id: String,
    size: Option<u64>,
    path: Vec<u8>,
}

impl fmt::Debug for GitCommitTreeEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitTreeEntry")
            .field("mode", &format_args!("{:o}", self.mode))
            .field("kind", &self.kind)
            .field("object_id", &"<redacted>")
            .field("size", &self.size)
            .field("path", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Eq, PartialEq)]
struct GitCommitTree {
    entries: Vec<GitCommitTreeEntry>,
}

fn parse_git_commit_tree(
    listing: &[u8],
    object_format: GitObjectFormat,
) -> Result<GitCommitTree, GitCommitPlanError> {
    if listing.len() > MAX_COMMIT_TREE_BYTES {
        return Err(GitCommitPlanError::ResourceLimit);
    }
    if !listing.is_empty() && !listing.ends_with(&[0]) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    if listing.is_empty() {
        return Ok(GitCommitTree {
            entries: Vec::new(),
        });
    }
    let records = listing.strip_suffix(&[0]).unwrap_or(listing);
    let mut entries = Vec::new();
    let mut previous_path: Option<&[u8]> = None;
    let mut tree_paths = std::collections::HashSet::new();
    for record in records.split(|byte| *byte == 0) {
        if record.is_empty() {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        if entries.len() >= MAX_COMMIT_TREE_ENTRIES {
            return Err(GitCommitPlanError::ResourceLimit);
        }
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or(GitCommitPlanError::UnsafeRepository)?;
        let header = std::str::from_utf8(&record[..tab])
            .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
        let path = &record[tab + 1..];
        if !valid_opaque_git_path(path) || previous_path.is_some_and(|previous| previous >= path) {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        previous_path = Some(path);
        if let Some(parent_end) = path.iter().rposition(|byte| *byte == b'/')
            && !tree_paths.contains(&path[..parent_end])
        {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        let mut fields = header.split_ascii_whitespace();
        let mode = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let object_type = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let object_id = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let size = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        if fields.next().is_some() || validate_object_id(object_id) != Some(object_format) {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        let (mode, kind, size) = match (mode, object_type, size) {
            ("040000", "tree", "-") => (0o040000, GitCommitTreeEntryKind::Tree, None),
            ("100644", "blob", size) => (
                0o100644,
                GitCommitTreeEntryKind::Blob,
                Some(parse_canonical_u64(size).ok_or(GitCommitPlanError::UnsafeRepository)?),
            ),
            ("100755", "blob", size) => (
                0o100755,
                GitCommitTreeEntryKind::Blob,
                Some(parse_canonical_u64(size).ok_or(GitCommitPlanError::UnsafeRepository)?),
            ),
            ("120000", "blob", size) => (
                0o120000,
                GitCommitTreeEntryKind::Blob,
                Some(parse_canonical_u64(size).ok_or(GitCommitPlanError::UnsafeRepository)?),
            ),
            ("160000", "commit", "-") => (0o160000, GitCommitTreeEntryKind::Commit, None),
            _ => return Err(GitCommitPlanError::UnsafeRepository),
        };
        let path = path.to_vec();
        if kind == GitCommitTreeEntryKind::Tree && !tree_paths.insert(path.clone()) {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        entries.push(GitCommitTreeEntry {
            mode,
            kind,
            object_id: object_id.to_owned(),
            size,
            path,
        });
    }
    Ok(GitCommitTree { entries })
}

fn git_commit_path_collision(
    tree: &GitCommitTree,
    prior_managed: &[Vec<u8>],
    current_managed: &[Vec<u8>],
) -> bool {
    let mut prior = prior_managed.iter().map(Vec::as_slice).collect::<Vec<_>>();
    prior.sort_unstable();
    prior.dedup();
    current_managed.iter().any(|current| {
        let current = current.as_slice();
        if prior.binary_search(&current).is_ok() {
            return false;
        }
        if tree
            .entries
            .binary_search_by(|entry| entry.path.as_slice().cmp(current))
            .is_ok()
        {
            return true;
        }
        let descendant_index = tree
            .entries
            .partition_point(|entry| entry.path.as_slice() <= current);
        let descendant = tree.entries.get(descendant_index);
        if descendant.is_some_and(|entry| path_is_ancestor(current, &entry.path)) {
            return true;
        }
        current
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'/')
            .any(|(end, _)| {
                let ancestor = &current[..end];
                let Ok(index) = tree
                    .entries
                    .binary_search_by(|entry| entry.path.as_slice().cmp(ancestor))
                else {
                    return false;
                };
                let entry = &tree.entries[index];
                entry.kind != GitCommitTreeEntryKind::Tree
                    || !sorted_paths_have_descendant(&prior, ancestor)
            })
    })
}

fn prior_managed_candidate_paths(tree: &GitCommitTree) -> Result<Vec<Vec<u8>>, GitCommitPlanError> {
    let mut paths = Vec::new();
    paths
        .try_reserve(tree.entries.len().min(MAX_COMMIT_INDEX_ENTRIES))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    for entry in &tree.entries {
        let Ok(path) = std::str::from_utf8(&entry.path) else {
            continue;
        };
        let Some((kind, _)) = classify_git_materialization_path(path) else {
            continue;
        };
        if kind != GitMaterializationPathKind::Ignore {
            if entry.mode != 0o100644 || entry.kind != GitCommitTreeEntryKind::Blob {
                return Err(GitCommitPlanError::AmbiguousManagedHistory);
            }
            let mut retained = Vec::new();
            retained
                .try_reserve_exact(entry.path.len())
                .map_err(|_| GitCommitPlanError::ResourceLimit)?;
            retained.extend_from_slice(&entry.path);
            paths.push(retained);
        }
    }
    Ok(paths)
}

fn path_is_ancestor(ancestor: &[u8], descendant: &[u8]) -> bool {
    descendant
        .strip_prefix(ancestor)
        .is_some_and(|suffix| suffix.starts_with(b"/"))
}

fn sorted_paths_have_descendant(sorted: &[&[u8]], ancestor: &[u8]) -> bool {
    let index = sorted.partition_point(|path| *path <= ancestor);
    sorted
        .get(index)
        .is_some_and(|path| path_is_ancestor(ancestor, path))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
enum GitBranchRepresentation {
    Unborn,
    Loose,
    Packed,
}

#[derive(Eq, PartialEq)]
#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
struct GitCommitBranch {
    reference: String,
    display: String,
    expected_object_id: Option<String>,
    representation: GitBranchRepresentation,
}

impl fmt::Debug for GitCommitBranch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitBranch")
            .field("reference", &self.reference)
            .field("display", &self.display)
            .field("expected_object_id", &"<redacted>")
            .field("representation", &self.representation)
            .finish()
    }
}

#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
fn parse_git_commit_branch(
    head: &[u8],
    loose_target: Option<&[u8]>,
    packed_refs: Option<&[u8]>,
    object_format: GitObjectFormat,
) -> Result<GitCommitBranch, GitCommitPlanError> {
    let reference = parse_git_head_reference(head)?;
    let display = reference
        .strip_prefix("refs/heads/")
        .filter(|value| !value.is_empty() && value.chars().count() <= 255)
        .ok_or(GitCommitPlanError::UnsafeRepository)?;

    let loose_object_id = loose_target
        .map(|bytes| parse_direct_ref(bytes, object_format))
        .transpose()?;
    let packed_object_id = packed_refs
        .map(|bytes| parse_packed_branch_target(bytes, reference, object_format))
        .transpose()?
        .flatten();
    let (expected_object_id, representation) = match (loose_object_id, packed_object_id) {
        (None, None) => (None, GitBranchRepresentation::Unborn),
        (Some(object_id), None) => (Some(object_id), GitBranchRepresentation::Loose),
        (None, Some(object_id)) => (Some(object_id), GitBranchRepresentation::Packed),
        // Git gives a valid loose ref precedence over an older packed entry.
        // Rehow still parses and binds the complete packed-refs file, but the
        // direct loose ref is authoritative for this branch.
        (Some(object_id), Some(_)) => (Some(object_id), GitBranchRepresentation::Loose),
    };

    Ok(GitCommitBranch {
        reference: reference.to_owned(),
        display: display.to_owned(),
        expected_object_id,
        representation,
    })
}

fn parse_git_head_reference(head: &[u8]) -> Result<&str, GitCommitPlanError> {
    std::str::from_utf8(head)
        .ok()
        .and_then(|value| value.strip_prefix("ref: "))
        .and_then(|value| value.strip_suffix('\n'))
        .filter(|value| !value.contains(['\n', '\r']))
        .filter(|value| value.starts_with("refs/heads/") && valid_ref_name(value))
        .ok_or(GitCommitPlanError::UnsafeRepository)
}

fn parse_direct_ref(
    bytes: &[u8],
    object_format: GitObjectFormat,
) -> Result<String, GitCommitPlanError> {
    let object_id = std::str::from_utf8(bytes)
        .ok()
        .and_then(|value| value.strip_suffix('\n'))
        .filter(|value| !value.contains(['\n', '\r']))
        .filter(|value| validate_object_id(value) == Some(object_format))
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    Ok(object_id.to_owned())
}

fn parse_packed_branch_target(
    bytes: &[u8],
    target: &str,
    object_format: GitObjectFormat,
) -> Result<Option<String>, GitCommitPlanError> {
    if bytes.len() > MAX_GIT_CONFIG_BYTES || (!bytes.is_empty() && !bytes.ends_with(b"\n")) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| GitCommitPlanError::UnsafeRepository)?;
    let mut target_object_id = None;
    let mut previous_was_tag = false;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            previous_was_tag = false;
            continue;
        }
        if let Some(peeled) = line.strip_prefix('^') {
            if !previous_was_tag || validate_object_id(peeled) != Some(object_format) {
                return Err(GitCommitPlanError::UnsafeRepository);
            }
            previous_was_tag = false;
            continue;
        }
        let (object_id, reference) = line
            .split_once(' ')
            .filter(|(object_id, reference)| {
                !reference.contains(' ')
                    && validate_object_id(object_id) == Some(object_format)
                    && valid_ref_name(reference)
            })
            .ok_or(GitCommitPlanError::UnsafeRepository)?;
        previous_was_tag = reference.starts_with("refs/tags/");
        if reference == target {
            if target_object_id.is_some() {
                return Err(GitCommitPlanError::UnsafeRepository);
            }
            target_object_id = Some(object_id.to_owned());
        }
    }
    Ok(target_object_id)
}

#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
#[derive(Eq, PartialEq)]
struct GitIndexSnapshot {
    version: u32,
    entries: Vec<GitIndexEntry>,
    has_cache_tree: bool,
    entries_end: usize,
}

impl fmt::Debug for GitIndexSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitIndexSnapshot")
            .field("version", &self.version)
            .field("entries", &self.entries.len())
            .field("has_cache_tree", &self.has_cache_tree)
            .finish()
    }
}

#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
#[derive(Eq, PartialEq)]
struct GitIndexEntry {
    mode: u32,
    object_id: Vec<u8>,
    path: Vec<u8>,
}

impl fmt::Debug for GitIndexEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitIndexEntry")
            .field("mode", &format_args!("{:o}", self.mode))
            .field("object_id", &"<redacted>")
            .field("path", &"<redacted>")
            .finish()
    }
}

#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
fn parse_git_index(
    bytes: &[u8],
    object_format: GitObjectFormat,
) -> Result<GitIndexSnapshot, GitCommitPlanError> {
    let hash_len = match object_format {
        GitObjectFormat::Sha1 => 20,
        GitObjectFormat::Sha256 => 32,
    };
    if bytes.len() > MAX_COMMIT_INDEX_BYTES || bytes.len() < 12 + hash_len {
        return Err(GitCommitPlanError::UnsupportedIndex);
    }
    let (payload, checksum) = bytes.split_at(bytes.len() - hash_len);
    if git_index_checksum(payload, object_format).as_slice() != checksum {
        return Err(GitCommitPlanError::UnsupportedIndex);
    }
    if payload.get(..4) != Some(b"DIRC") {
        return Err(GitCommitPlanError::UnsupportedIndex);
    }
    let version = read_be_u32(payload, 4)?;
    if !matches!(version, 2 | 3) {
        return Err(GitCommitPlanError::UnsupportedIndex);
    }
    let entry_count =
        usize::try_from(read_be_u32(payload, 8)?).map_err(|_| GitCommitPlanError::ResourceLimit)?;
    if entry_count > MAX_COMMIT_INDEX_ENTRIES {
        return Err(GitCommitPlanError::ResourceLimit);
    }
    let fixed_len = 40usize
        .checked_add(hash_len)
        .and_then(|value| value.checked_add(2))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    let mut cursor = 12usize;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(entry_count)
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    for _ in 0..entry_count {
        let start = cursor;
        let fixed_end = cursor
            .checked_add(fixed_len)
            .filter(|end| *end <= payload.len())
            .ok_or(GitCommitPlanError::UnsupportedIndex)?;
        let mode = read_be_u32(payload, cursor + 24)?;
        if !matches!(mode, 0o100644 | 0o100755 | 0o120000 | 0o160000) {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        let object_start = cursor + 40;
        let object_end = object_start + hash_len;
        let object_id = payload[object_start..object_end].to_vec();
        if object_id.iter().all(|byte| *byte == 0) {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        let flags = u16::from_be_bytes(
            payload[object_end..fixed_end]
                .try_into()
                .map_err(|_| GitCommitPlanError::UnsupportedIndex)?,
        );
        if flags & 0xf000 != 0 {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        cursor = fixed_end;
        let name_end = payload[cursor..]
            .iter()
            .position(|byte| *byte == 0)
            .and_then(|relative| cursor.checked_add(relative))
            .ok_or(GitCommitPlanError::UnsupportedIndex)?;
        let path = &payload[cursor..name_end];
        if !valid_opaque_git_path(path) {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        let encoded_len = usize::from(flags & 0x0fff);
        if encoded_len != 0x0fff && encoded_len != path.len() {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        let unpadded_end = name_end
            .checked_add(1)
            .ok_or(GitCommitPlanError::ResourceLimit)?;
        let entry_len = unpadded_end
            .checked_sub(start)
            .ok_or(GitCommitPlanError::UnsupportedIndex)?;
        let padded_len = entry_len
            .checked_add(7)
            .map(|value| value & !7)
            .ok_or(GitCommitPlanError::ResourceLimit)?;
        cursor = start
            .checked_add(padded_len)
            .filter(|end| *end <= payload.len())
            .ok_or(GitCommitPlanError::UnsupportedIndex)?;
        if payload[unpadded_end..cursor].iter().any(|byte| *byte != 0) {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        if entries
            .last()
            .is_some_and(|previous: &GitIndexEntry| previous.path.as_slice() >= path)
        {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        entries.push(GitIndexEntry {
            mode,
            object_id,
            path: path.to_vec(),
        });
    }

    let entries_end = cursor;
    let mut has_cache_tree = false;
    while cursor < payload.len() {
        let header_end = cursor
            .checked_add(8)
            .filter(|end| *end <= payload.len())
            .ok_or(GitCommitPlanError::UnsupportedIndex)?;
        let signature = &payload[cursor..cursor + 4];
        let extension_len = usize::try_from(read_be_u32(payload, cursor + 4)?)
            .map_err(|_| GitCommitPlanError::ResourceLimit)?;
        let extension_end = header_end
            .checked_add(extension_len)
            .filter(|end| *end <= payload.len())
            .ok_or(GitCommitPlanError::UnsupportedIndex)?;
        if signature != b"TREE" || has_cache_tree {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        has_cache_tree = true;
        cursor = extension_end;
    }
    Ok(GitIndexSnapshot {
        version,
        entries,
        has_cache_tree,
        entries_end,
    })
}

fn git_index_without_extensions(
    bytes: &[u8],
    object_format: GitObjectFormat,
    snapshot: &GitIndexSnapshot,
) -> Result<Vec<u8>, GitCommitPlanError> {
    let hash_len = match object_format {
        GitObjectFormat::Sha1 => 20,
        GitObjectFormat::Sha256 => 32,
    };
    let payload_end = bytes
        .len()
        .checked_sub(hash_len)
        .ok_or(GitCommitPlanError::UnsupportedIndex)?;
    if snapshot.entries_end > payload_end {
        return Err(GitCommitPlanError::UnsupportedIndex);
    }
    let mut sanitized = Vec::new();
    sanitized
        .try_reserve_exact(snapshot.entries_end.saturating_add(hash_len))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    sanitized.extend_from_slice(&bytes[..snapshot.entries_end]);
    sanitized.extend_from_slice(&git_index_checksum(&sanitized, object_format));
    Ok(sanitized)
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Result<u32, GitCommitPlanError> {
    let end = offset
        .checked_add(4)
        .filter(|end| *end <= bytes.len())
        .ok_or(GitCommitPlanError::UnsupportedIndex)?;
    Ok(u32::from_be_bytes(
        bytes[offset..end]
            .try_into()
            .map_err(|_| GitCommitPlanError::UnsupportedIndex)?,
    ))
}

fn git_index_checksum(payload: &[u8], object_format: GitObjectFormat) -> Vec<u8> {
    use sha1::Digest as _;
    match object_format {
        GitObjectFormat::Sha1 => sha1::Sha1::digest(payload).to_vec(),
        GitObjectFormat::Sha256 => sha2::Sha256::digest(payload).to_vec(),
    }
}

fn valid_opaque_git_path(path: &[u8]) -> bool {
    !path.is_empty()
        && path.len() <= MAX_COMMIT_PATH_BYTES
        && !path.starts_with(b"/")
        && !path.ends_with(b"/")
        && path
            .split(|byte| *byte == b'/')
            .all(|component| !component.is_empty() && component != b"." && component != b"..")
}

#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
fn parse_canonical_u64(value: &str) -> Option<u64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value.parse().ok()
}

#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
fn hash_clone_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[derive(Debug)]
struct ValidRemoteUrl {
    canonical: String,
    transport: GitRemoteTransport,
    host: String,
}

pub fn plan_git_remote_connect(
    workspace: &WorkspaceView,
    remote_name: &str,
    remote_url: &str,
) -> Result<GitRemotePlan, GitRemoteError> {
    if !valid_remote_name(remote_name) {
        return Err(GitRemoteError::InvalidName);
    }
    let url = validate_remote_url(remote_url)?;
    let capability = workspace
        .git_repository_capability()
        .map_err(map_git_remote_capability)?;
    let files = capability
        .read_config_files()
        .map_err(map_git_remote_storage)?;
    let (expected_config, candidate_config, config_mode) = match files.lock {
        None => {
            let candidate = append_remote_config(&files.config, remote_name, &url.canonical)?;
            (files.config, candidate, files.config_mode)
        }
        Some(lock) if files.lock_mode == Some(files.config_mode) => {
            let pre_exchange = append_remote_config(&files.config, remote_name, &url.canonical)
                .ok()
                .filter(|candidate| candidate == &lock);
            let post_exchange = append_remote_config(&lock, remote_name, &url.canonical)
                .ok()
                .filter(|candidate| candidate == &files.config);
            match (pre_exchange, post_exchange) {
                (Some(candidate), None) => (files.config, candidate, files.config_mode),
                (None, Some(candidate)) => (lock, candidate, files.config_mode),
                _ => return Err(GitRemoteError::RecoveryRequired),
            }
        }
        Some(_) => return Err(GitRemoteError::RecoveryRequired),
    };
    let (workspace_id, manifest_revision, root_capability_hash) =
        workspace.git_initialization_binding();
    let mut random = [0_u8; 32];
    fill_random(&mut random).map_err(|_| GitRemoteError::Io)?;
    let config_revision = git_config_revision(config_mode, &expected_config);
    let candidate_hash = git_config_revision(config_mode, &candidate_config);
    Ok(GitRemotePlan {
        token: PlanToken::from_random_bytes(random),
        workspace_id,
        manifest_revision,
        root_capability_hash,
        remote_name: remote_name.to_owned(),
        remote_url: url.canonical,
        transport: url.transport,
        host: url.host,
        expected_config,
        candidate_config,
        config_revision,
        candidate_hash,
        config_mode,
    })
}

pub fn apply_git_remote_connect(
    workspace: &WorkspaceView,
    plan: &GitRemotePlan,
) -> Result<(), GitRemoteError> {
    let (workspace_id, manifest_revision, root_capability_hash) =
        workspace.git_initialization_binding();
    if workspace_id != plan.workspace_id
        || manifest_revision != plan.manifest_revision
        || root_capability_hash != plan.root_capability_hash
    {
        return Err(GitRemoteError::PlanStale);
    }
    if git_config_revision(plan.config_mode, &plan.expected_config) != plan.config_revision
        || git_config_revision(plan.config_mode, &plan.candidate_config) != plan.candidate_hash
    {
        return Err(GitRemoteError::PlanStale);
    }
    if !workspace
        .git_manifest_revision_is_current(plan.manifest_revision)
        .map_err(|error| map_git_remote_storage(map_git_init_storage_to_remote(error)))?
    {
        return Err(GitRemoteError::PlanStale);
    }
    let capability = workspace
        .git_repository_capability()
        .map_err(map_git_remote_capability)?;
    capability
        .apply_config_candidate(
            &plan.expected_config,
            &plan.candidate_config,
            plan.config_mode,
        )
        .map_err(map_git_remote_storage)
}

fn git_config_revision(mode: u32, bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rehow:git-config:v1\0");
    hasher.update(&mode.to_le_bytes());
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn map_git_init_storage_to_remote(error: GitInitializeStorageError) -> GitConfigStorageError {
    match error {
        GitInitializeStorageError::Conflict => GitConfigStorageError::Conflict,
        GitInitializeStorageError::Unsafe => GitConfigStorageError::Unsafe,
        GitInitializeStorageError::Unsupported => GitConfigStorageError::Unsupported,
        GitInitializeStorageError::Io => GitConfigStorageError::Io,
    }
}

fn map_git_remote_capability(error: GitRepositoryCapabilityError) -> GitRemoteError {
    match error {
        GitRepositoryCapabilityError::NotRepository => GitRemoteError::NotRepository,
        GitRepositoryCapabilityError::Unsafe => GitRemoteError::Unsafe,
        GitRepositoryCapabilityError::Io => GitRemoteError::Io,
    }
}

fn map_git_remote_storage(error: GitConfigStorageError) -> GitRemoteError {
    match error {
        GitConfigStorageError::Conflict => GitRemoteError::PlanStale,
        GitConfigStorageError::Unsafe => GitRemoteError::Unsafe,
        GitConfigStorageError::Unsupported => GitRemoteError::Unsupported,
        GitConfigStorageError::TooLarge => GitRemoteError::TooLarge,
        GitConfigStorageError::Io => GitRemoteError::Io,
    }
}

fn valid_remote_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REMOTE_NAME_BYTES
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        && !value.ends_with(".lock")
        && !value.contains("..")
}

fn validate_remote_url(value: &str) -> Result<ValidRemoteUrl, GitRemoteError> {
    if value.is_empty()
        || value.len() > MAX_REMOTE_URL_BYTES
        || !value.is_ascii()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        || value.contains(['\\', '%', '#', '?'])
    {
        return Err(GitRemoteError::InvalidUrl);
    }
    if value.contains("://") {
        validate_remote_uri(value)
    } else {
        validate_scp_remote(value)
    }
}

fn validate_remote_uri(value: &str) -> Result<ValidRemoteUrl, GitRemoteError> {
    let parsed = url::Url::parse(value).map_err(|_| GitRemoteError::InvalidUrl)?;
    let transport = match parsed.scheme() {
        "https" => GitRemoteTransport::Https,
        "ssh" => GitRemoteTransport::Ssh,
        _ => return Err(GitRemoteError::InvalidUrl),
    };
    if parsed.as_str() != value
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.port() == Some(0)
        || (transport == GitRemoteTransport::Https && !parsed.username().is_empty())
        || (transport == GitRemoteTransport::Ssh
            && !parsed.username().is_empty()
            && !valid_ssh_user(parsed.username()))
    {
        return Err(GitRemoteError::InvalidUrl);
    }
    let host = parsed.host_str().ok_or(GitRemoteError::InvalidUrl)?;
    if !valid_remote_host(host) || !valid_remote_path(parsed.path(), true) {
        return Err(GitRemoteError::InvalidUrl);
    }
    Ok(ValidRemoteUrl {
        canonical: value.to_owned(),
        transport,
        host: host.to_owned(),
    })
}

fn https_credential_host(remote_url: &str) -> Option<String> {
    let parsed = url::Url::parse(remote_url).ok()?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    let mut host = match parsed.host()? {
        url::Host::Domain(value) => value.to_owned(),
        url::Host::Ipv4(value) => value.to_string(),
        // IPv6-literal remotes remain outside the conservative remote URL
        // grammar. Do not authenticate a host form planning cannot accept.
        url::Host::Ipv6(_) => return None,
    };
    if let Some(port) = parsed.port() {
        use std::fmt::Write as _;
        write!(&mut host, ":{port}").ok()?;
    }
    Some(host)
}

fn validate_scp_remote(value: &str) -> Result<ValidRemoteUrl, GitRemoteError> {
    let (authority, path) = value.split_once(':').ok_or(GitRemoteError::InvalidUrl)?;
    if authority.contains(':') || path.contains(':') || !valid_remote_path(path, false) {
        return Err(GitRemoteError::InvalidUrl);
    }
    let (user, host) = match authority.rsplit_once('@') {
        Some((user, host)) if valid_ssh_user(user) => (Some(user), host),
        Some(_) => return Err(GitRemoteError::InvalidUrl),
        None => (None, authority),
    };
    if !valid_remote_host(host) || user == Some("") {
        return Err(GitRemoteError::InvalidUrl);
    }
    Ok(ValidRemoteUrl {
        canonical: value.to_owned(),
        transport: GitRemoteTransport::Ssh,
        host: host.to_owned(),
    })
}

fn valid_ssh_user(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn valid_remote_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value == value.to_ascii_lowercase()
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn valid_remote_path(value: &str, absolute: bool) -> bool {
    if value.is_empty() || value == "/" || (absolute != value.starts_with('/')) {
        return false;
    }
    value.trim_start_matches('/').split('/').all(|part| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part.len() <= 255
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._~+-@".contains(&byte))
    })
}

fn append_remote_config(
    config: &[u8],
    remote_name: &str,
    remote_url: &str,
) -> Result<Vec<u8>, GitRemoteError> {
    let keys = validate_git_config(config)?;
    let prefix = format!("remote.{remote_name}.");
    if keys.iter().any(|key| key.starts_with(&prefix))
        || contains_remote_section(config, remote_name)
    {
        return Err(GitRemoteError::DuplicateRemote);
    }
    let separator = if config.is_empty() || config.ends_with(b"\n") {
        ""
    } else {
        "\n"
    };
    let section = format!("{separator}[remote \"{remote_name}\"]\n\turl = \"{remote_url}\"\n");
    let size = config
        .len()
        .checked_add(section.len())
        .ok_or(GitRemoteError::TooLarge)?;
    if size > MAX_GIT_CONFIG_BYTES {
        return Err(GitRemoteError::TooLarge);
    }
    let mut candidate = Vec::new();
    candidate
        .try_reserve_exact(size)
        .map_err(|_| GitRemoteError::TooLarge)?;
    candidate.extend_from_slice(config);
    candidate.extend_from_slice(section.as_bytes());
    validate_git_config(&candidate)?;
    Ok(candidate)
}

fn contains_remote_section(config: &[u8], remote_name: &str) -> bool {
    config.split(|byte| *byte == b'\n').any(|line| {
        let Ok(line) = std::str::from_utf8(line) else {
            return false;
        };
        let line = line.trim_matches(|character: char| character.is_ascii_whitespace());
        let Some(close) = line.find(']') else {
            return false;
        };
        if !line.starts_with('[') {
            return false;
        }
        let inner = line[1..close].trim();
        if let Some((section, subsection)) = inner.split_once('.')
            && section.eq_ignore_ascii_case("remote")
            && subsection == remote_name
        {
            return true;
        }
        let Some((section, subsection)) = inner.split_once(char::is_whitespace) else {
            return false;
        };
        section.eq_ignore_ascii_case("remote")
            && subsection
                .trim()
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                == Some(remote_name)
    })
}

fn validate_git_config(bytes: &[u8]) -> Result<Vec<String>, GitRemoteError> {
    if bytes.len() > MAX_GIT_CONFIG_BYTES {
        return Err(GitRemoteError::TooLarge);
    }
    let executable = trusted_executable("/usr/bin/git").ok_or(GitRemoteError::GitUnavailable)?;
    // A memory file gives Git a held, immutable-for-this-call input without
    // creating a pathname or writing repository state during planning.
    // SAFETY: static name is valid and a successful call returns an owned fd.
    let raw = unsafe { libc::memfd_create(c"rehow-git-config".as_ptr(), 0) };
    if raw < 0 {
        return Err(GitRemoteError::Unsupported);
    }
    // SAFETY: successful memfd_create returned a newly owned descriptor.
    let mut file = unsafe { fs::File::from_raw_fd(raw) };
    file.write_all(bytes).map_err(|_| GitRemoteError::Io)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| GitRemoteError::Io)?;
    let path = format!("/proc/self/fd/{}", file.as_raw_fd());
    let output = run_git(
        &executable,
        None,
        &[
            "config",
            "--file",
            &path,
            "--no-includes",
            "-z",
            "--name-only",
            "--list",
        ],
    )
    .map_err(|error| match error {
        GitReadError::ExitStatus | GitReadError::InvalidOutput => GitRemoteError::MalformedConfig,
        GitReadError::OutputLimit => GitRemoteError::TooLarge,
        GitReadError::Spawn => GitRemoteError::GitUnavailable,
        GitReadError::Signal | GitReadError::Timeout => GitRemoteError::Io,
    })?;
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let key = std::str::from_utf8(record).map_err(|_| GitRemoteError::MalformedConfig)?;
            if key.is_empty() || key.len() > 1024 || key.chars().any(char::is_control) {
                Err(GitRemoteError::MalformedConfig)
            } else {
                Ok(key.to_owned())
            }
        })
        .collect()
}

pub fn plan_git_initialize(workspace: &WorkspaceView) -> Result<GitInitPlan, GitInitError> {
    ensure_git_initialization_facts(workspace)?;
    ensure_git_initializable(workspace, false)?;
    let (workspace_id, manifest_revision, root_capability_hash) =
        workspace.git_initialization_binding();
    let mut random = [0_u8; 32];
    fill_random(&mut random).map_err(|_| GitInitError::Io)?;
    Ok(GitInitPlan {
        token: PlanToken::from_random_bytes(random),
        workspace_id,
        manifest_revision,
        root_capability_hash,
    })
}

pub fn apply_git_initialize(
    workspace: &WorkspaceView,
    plan: &GitInitPlan,
) -> Result<(), GitInitError> {
    let (workspace_id, manifest_revision, root_capability_hash) =
        workspace.git_initialization_binding();
    if workspace_id != plan.workspace_id
        || manifest_revision != plan.manifest_revision
        || root_capability_hash != plan.root_capability_hash
    {
        return Err(GitInitError::PlanStale);
    }
    if !workspace
        .git_manifest_revision_is_current(plan.manifest_revision)
        .map_err(map_git_init_storage)?
    {
        return Err(GitInitError::PlanStale);
    }
    ensure_git_initialization_facts(workspace)?;
    ensure_git_initializable(workspace, true)?;
    workspace
        .initialize_git_repository()
        .map_err(map_git_init_storage)?;
    workspace
        .git_repository_capability()
        .map(|_| ())
        .map_err(|error| match error {
            GitRepositoryCapabilityError::NotRepository | GitRepositoryCapabilityError::Unsafe => {
                GitInitError::Unsafe
            }
            GitRepositoryCapabilityError::Io => GitInitError::Io,
        })
}

fn ensure_git_initialization_facts(workspace: &WorkspaceView) -> Result<(), GitInitError> {
    let facts = workspace
        .git_initialization_facts()
        .map_err(map_git_init_storage)?;
    if !facts.git_probe_complete() || facts.unsafe_git_marker() {
        return Err(GitInitError::Unsafe);
    }
    if facts.nested_git_repository() {
        return Err(GitInitError::NestedRepository);
    }
    if !facts.writable() {
        return Err(GitInitError::ReadOnly);
    }
    Ok(())
}

fn ensure_git_initializable(workspace: &WorkspaceView, applying: bool) -> Result<(), GitInitError> {
    match workspace.git_repository_capability() {
        Ok(_) => Err(if applying {
            GitInitError::PlanStale
        } else {
            GitInitError::AlreadyRepository
        }),
        Err(GitRepositoryCapabilityError::NotRepository) => Ok(()),
        Err(GitRepositoryCapabilityError::Unsafe) => {
            if workspace
                .git_initialization_resumable()
                .map_err(map_git_init_storage)?
            {
                Ok(())
            } else {
                Err(GitInitError::Unsafe)
            }
        }
        Err(GitRepositoryCapabilityError::Io) => Err(GitInitError::Io),
    }
}

fn map_git_init_storage(error: GitInitializeStorageError) -> GitInitError {
    match error {
        GitInitializeStorageError::Conflict => GitInitError::PlanStale,
        GitInitializeStorageError::Unsafe => GitInitError::Unsafe,
        GitInitializeStorageError::Unsupported => GitInitError::Unsupported,
        GitInitializeStorageError::Io => GitInitError::Io,
    }
}

fn fill_random(bytes: &mut [u8]) -> Result<(), std::io::Error> {
    let mut filled = 0usize;
    while filled < bytes.len() {
        // SAFETY: the remaining slice is writable for the requested length.
        let result = unsafe {
            libc::getrandom(bytes[filled..].as_mut_ptr().cast(), bytes.len() - filled, 0)
        };
        if result > 0 {
            filled += result as usize;
        } else if result == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "system randomness returned no bytes",
            ));
        } else {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
    Ok(())
}

pub fn read_git_status(workspace: &WorkspaceView) -> Result<GitStatus, GitReadError> {
    let Some(executable) = trusted_executable("/usr/bin/git") else {
        return Ok(GitStatus {
            available: false,
            version: None,
            repository: GitRepositoryStatus::NotRepository,
        });
    };
    let version_output = run_git(&executable, None, &["version"])?;
    let version = parse_version(&version_output.stdout)?;
    let repository = match workspace.git_repository_capability() {
        Err(GitRepositoryCapabilityError::NotRepository) => GitRepositoryStatus::NotRepository,
        Err(GitRepositoryCapabilityError::Unsafe | GitRepositoryCapabilityError::Io) => {
            GitRepositoryStatus::Unsafe
        }
        Ok(capability) => {
            #[cfg(test)]
            POST_CHECK_ALTERNATES.with_borrow_mut(|path| {
                if let Some(path) = path.take() {
                    fs::write(path, "/tmp/hostile-object-store\n").unwrap();
                }
            });
            let Some(sandbox) = trusted_executable("/usr/bin/bwrap") else {
                return Ok(GitStatus {
                    available: true,
                    version: Some(version),
                    repository: GitRepositoryStatus::Unsafe,
                });
            };
            let status = run_git(
                &executable,
                Some((&sandbox, &capability)),
                &[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "-z",
                    "--untracked-files=normal",
                    "--ignore-submodules=all",
                ],
            )?;
            let config_output = run_git(
                &executable,
                None,
                &[
                    "config",
                    "--file",
                    capability.config_path(),
                    "--no-includes",
                    "-z",
                    "--list",
                ],
            );
            let config_bytes = match config_output {
                Ok(output) => output.stdout,
                Err(GitReadError::ExitStatus) => Vec::new(),
                Err(error) => return Err(error),
            };
            let (mut parsed, upstream_ref) = parse_status(&status.stdout, &config_bytes)?;
            if let Some(upstream_ref) = upstream_ref {
                let range = format!("HEAD...{upstream_ref}");
                if let Ok(output) = run_git(
                    &executable,
                    Some((&sandbox, &capability)),
                    &["rev-list", "--left-right", "--count", &range, "--"],
                ) && let Ok((ahead, behind)) = parse_divergence(&output.stdout)
                {
                    parsed.ahead = ahead;
                    parsed.behind = behind;
                    parsed.divergence_known = true;
                }
            }
            GitRepositoryStatus::Ready(parsed)
        }
    };
    Ok(GitStatus {
        available: true,
        version: Some(version),
        repository,
    })
}

fn trusted_executable(candidate: &str) -> Option<PathBuf> {
    let candidate = Path::new(candidate);
    let Ok(metadata) = fs::metadata(candidate) else {
        return None;
    };
    if metadata.is_file() && metadata.uid() == 0 && metadata.permissions().mode() & 0o111 != 0 {
        Some(candidate.to_owned())
    } else {
        None
    }
}

struct ProcessOutput {
    stdout: Vec<u8>,
}

fn run_git(
    executable: &Path,
    repository: Option<(&Path, &crate::storage::GitRepositoryCapability)>,
    arguments: &[&str],
) -> Result<ProcessOutput, GitReadError> {
    let mut command = if let Some((sandbox, _)) = repository {
        Command::new(sandbox)
    } else {
        Command::new(executable)
    };
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false");
    if let Some((_, repository)) = repository {
        command.args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--ro-bind",
            repository.work_tree_path(),
            "/workspace",
            "--tmpfs",
            "/repo.git",
            "--ro-bind",
            repository.objects_path(),
            "/repo.git/objects",
            "--ro-bind",
            repository.refs_path(),
            "/repo.git/refs",
            "--ro-bind",
            repository.head_path(),
            "/repo.git/HEAD",
            "--tmpfs",
            "/repo.git/objects/info",
            "--tmpfs",
            "/tmp",
            "--chdir",
            "/workspace",
            "--setenv",
            "GIT_WORK_TREE",
            "/workspace",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
        ]);
        for (source, destination) in [
            (repository.index_path(), "/repo.git/index"),
            (repository.packed_refs_path(), "/repo.git/packed-refs"),
            (repository.shallow_path(), "/repo.git/shallow"),
        ] {
            if let Some(source) = source {
                command.args(["--ro-bind", source, destination]);
            }
        }
        command.args(["--", "/usr/bin/git"]);
    }
    command
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "diff.external=",
            "-c",
            "color.ui=false",
            "-c",
            "status.submoduleSummary=false",
        ])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    run_bounded_git_command(command, PROCESS_TIMEOUT, OUTPUT_LIMIT)
}

fn run_git_commit_read(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    object_format: GitObjectFormat,
    arguments: &[&str],
    output_limit: usize,
) -> Result<ProcessOutput, GitReadError> {
    run_git_commit_read_with_input(
        executable,
        sandbox,
        repository,
        object_format,
        arguments,
        output_limit,
        None,
    )
}

fn run_git_commit_read_with_input(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    object_format: GitObjectFormat,
    arguments: &[&str],
    output_limit: usize,
    input: Option<Vec<u8>>,
) -> Result<ProcessOutput, GitReadError> {
    let mut synthetic_config = create_git_format_config(object_format)?;
    let config_descriptor = synthetic_config.as_raw_fd();
    let config_descriptor_text = config_descriptor.to_string();
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/repo.git",
            "--ro-bind",
            repository.objects_path(),
            "/repo.git/objects",
            "--ro-bind",
            repository.refs_path(),
            "/repo.git/refs",
            "--ro-bind",
            repository.head_path(),
            "/repo.git/HEAD",
            "--file",
            &config_descriptor_text,
            "/repo.git/config",
            "--tmpfs",
            "/repo.git/objects/info",
            "--tmpfs",
            "/tmp",
            "--chdir",
            "/tmp",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
        ]);
    for (source, destination) in [
        (repository.packed_refs_path(), "/repo.git/packed-refs"),
        (repository.shallow_path(), "/repo.git/shallow"),
    ] {
        if let Some(source) = source {
            command.args(["--ro-bind", source, destination]);
        }
    }
    command
        .args(["--", executable.to_str().ok_or(GitReadError::Spawn)?])
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "diff.external=",
            "-c",
            "color.ui=false",
            "-c",
            "protocol.allow=never",
        ])
        .args(arguments)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: the closure only clears CLOEXEC on the owned synthetic-config
    // descriptor between fork and exec so Bubblewrap can copy it into the
    // otherwise private repository view.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(config_descriptor, libc::F_GETFD);
            if flags < 0
                || libc::fcntl(config_descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let output = run_bounded_git_command_with_input(
        command,
        COMMIT_PROCESS_TIMEOUT,
        output_limit,
        OUTPUT_LIMIT,
        input,
    );
    let _ = synthetic_config.seek(SeekFrom::Start(0));
    output
}

fn create_git_format_config(object_format: GitObjectFormat) -> Result<File, GitReadError> {
    // SAFETY: the static name is NUL-terminated and flags are valid.
    let raw = unsafe {
        libc::memfd_create(
            c"rehow-git-format".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if raw < 0 {
        return Err(GitReadError::Spawn);
    }
    // SAFETY: successful memfd_create returned a newly owned descriptor.
    let mut file = unsafe { File::from_raw_fd(raw) };
    let config = match object_format {
        GitObjectFormat::Sha1 => b"[core]\n\trepositoryformatversion = 0\n".as_slice(),
        GitObjectFormat::Sha256 => {
            b"[core]\n\trepositoryformatversion = 1\n[extensions]\n\tobjectformat = sha256\n"
                .as_slice()
        }
    };
    file.write_all(config).map_err(|_| GitReadError::Spawn)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| GitReadError::Spawn)?;
    // SAFETY: the descriptor is owned and sealing constants are valid.
    let seals = libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) } != 0 {
        return Err(GitReadError::Spawn);
    }
    Ok(file)
}

fn run_bounded_git_command(
    command: Command,
    timeout: Duration,
    output_limit: usize,
) -> Result<ProcessOutput, GitReadError> {
    run_bounded_git_command_with_input(command, timeout, output_limit, output_limit, None)
}

fn run_bounded_git_command_with_input(
    mut command: Command,
    timeout: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
    input: Option<Vec<u8>>,
) -> Result<ProcessOutput, GitReadError> {
    let timeout = active_git_commit_timeout(timeout)?;
    let mut child = command.spawn().map_err(|_| GitReadError::Spawn)?;
    let input_writer = input.map(|input| {
        let mut stdin = child.stdin.take().ok_or(GitReadError::Spawn)?;
        Ok::<_, GitReadError>(thread::spawn(move || stdin.write_all(&input)))
    });
    let input_writer = input_writer.transpose()?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take().ok_or(GitReadError::Spawn)?;
    let stderr = child.stderr.take().ok_or(GitReadError::Spawn)?;
    let stdout_reader = read_capped_with_limit(stdout, Arc::clone(&exceeded), stdout_limit);
    let stderr_reader = read_capped_with_limit(stderr, Arc::clone(&exceeded), stderr_limit);
    let started = Instant::now();
    let mut status = None;
    loop {
        if exceeded.load(Ordering::Relaxed) {
            terminate_child(&mut child);
            return Err(GitReadError::OutputLimit);
        }
        if started.elapsed() >= timeout {
            terminate_child(&mut child);
            return Err(GitReadError::Timeout);
        }
        if status.is_none()
            && let Some(exit_status) = child.try_wait().map_err(|_| GitReadError::Spawn)?
        {
            // A helper can exit after forking descendants that retain the
            // stdio pipes. Close that avenue before joining reader threads.
            kill_process_group(child.id());
            status = Some(exit_status);
        }
        if status.is_some()
            && stdout_reader.is_finished()
            && stderr_reader.is_finished()
            && input_writer
                .as_ref()
                .is_none_or(thread::JoinHandle::is_finished)
        {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let stdout = stdout_reader.join().map_err(|_| GitReadError::Spawn)?;
    let _ = stderr_reader.join().map_err(|_| GitReadError::Spawn)?;
    if let Some(writer) = input_writer {
        let _ = writer.join().map_err(|_| GitReadError::Spawn)?;
    }
    let status = status.ok_or(GitReadError::Spawn)?;
    if exceeded.load(Ordering::Relaxed) {
        return Err(GitReadError::OutputLimit);
    }
    if !status.success() {
        return if status.code().is_some() {
            Err(GitReadError::ExitStatus)
        } else {
            Err(GitReadError::Signal)
        };
    }
    Ok(ProcessOutput { stdout })
}

fn read_capped_with_limit<R: Read + Send + 'static>(
    mut reader: R,
    exceeded: Arc<AtomicBool>,
    limit: usize,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut retained = Vec::new();
        let mut buffer = [0_u8; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            if retained.len().saturating_add(count) > limit {
                exceeded.store(true, Ordering::Relaxed);
            } else {
                retained.extend_from_slice(&buffer[..count]);
            }
        }
        retained
    })
}

fn parse_version(bytes: &[u8]) -> Result<String, GitReadError> {
    let text = std::str::from_utf8(bytes).map_err(|_| GitReadError::InvalidOutput)?;
    let version = text
        .trim()
        .strip_prefix("git version ")
        .ok_or(GitReadError::InvalidOutput)?;
    if version.is_empty()
        || version.len() > 64
        || !version.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(GitReadError::InvalidOutput);
    }
    Ok(version.to_owned())
}

fn parse_status(
    status: &[u8],
    config_records: &[u8],
) -> Result<(GitWorkingTreeStatus, Option<String>), GitReadError> {
    let mut result = GitWorkingTreeStatus {
        branch: None,
        detached: false,
        upstream: None,
        divergence_known: false,
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicts: 0,
        remotes: Vec::new(),
        remote_count: 0,
    };
    let mut records = status
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        if record.first() == Some(&b'#') {
            let text = std::str::from_utf8(record).map_err(|_| GitReadError::InvalidOutput)?;
            if let Some(value) = text.strip_prefix("# branch.head ") {
                if value == "(detached)" {
                    result.detached = true;
                } else {
                    result.branch = safe_git_label(value);
                }
            } else if let Some(value) = text.strip_prefix("# branch.upstream ") {
                result.upstream = safe_git_label(value);
            } else if let Some(value) = text.strip_prefix("# branch.ab +") {
                let (ahead, behind) = value.split_once(" -").ok_or(GitReadError::InvalidOutput)?;
                result.ahead = ahead.parse().map_err(|_| GitReadError::InvalidOutput)?;
                result.behind = behind.parse().map_err(|_| GitReadError::InvalidOutput)?;
            } else if !text.starts_with("# branch.oid ") && !text.starts_with("# stash ") {
                return Err(GitReadError::InvalidOutput);
            }
        } else {
            match record.first().copied() {
                Some(b'1') => result.changed = result.changed.saturating_add(1),
                Some(b'2') => {
                    result.changed = result.changed.saturating_add(1);
                    records.next().ok_or(GitReadError::InvalidOutput)?;
                }
                Some(b'u') => result.conflicts = result.conflicts.saturating_add(1),
                Some(b'?') => result.untracked = result.untracked.saturating_add(1),
                Some(b'!') => {}
                _ => return Err(GitReadError::InvalidOutput),
            }
        }
    }
    let branch = result.branch.clone();
    let mut upstream_remote = None;
    let mut upstream_merge = None;
    for record in config_records
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let Some(separator) = record.iter().position(|byte| *byte == b'\n') else {
            continue;
        };
        let (key, value) = record.split_at(separator);
        let value = &value[1..];
        let Ok(key) = std::str::from_utf8(key) else {
            continue;
        };
        if let Some(name) = key
            .strip_prefix("remote.")
            .and_then(|key| key.strip_suffix(".url"))
        {
            result.remote_count = result.remote_count.saturating_add(1);
            if result.remotes.len() < MAX_REMOTES
                && let Some(name) = safe_git_label(name)
            {
                result.remotes.push(name);
            }
        }
        if let Some(branch) = branch.as_deref() {
            if key == format!("branch.{branch}.remote") {
                upstream_remote = std::str::from_utf8(value).ok().and_then(safe_git_label);
            } else if key == format!("branch.{branch}.merge") {
                upstream_merge = std::str::from_utf8(value).ok().and_then(safe_git_label);
            }
        }
    }
    result.remotes.sort();
    result.remotes.dedup();
    let upstream_ref = match (upstream_remote, upstream_merge) {
        (Some(remote), Some(merge)) if valid_ref_name(&merge) => {
            if remote == "." {
                result.upstream = merge.strip_prefix("refs/heads/").and_then(safe_git_label);
                Some(merge)
            } else if valid_ref_component(&remote) {
                if let Some(branch) = merge.strip_prefix("refs/heads/") {
                    let reference = format!("refs/remotes/{remote}/{branch}");
                    if valid_ref_name(&reference) {
                        result.upstream = safe_git_label(&format!("{remote}/{branch}"));
                        Some(reference)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    };
    Ok((result, upstream_ref))
}

fn parse_divergence(bytes: &[u8]) -> Result<(u32, u32), GitReadError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| GitReadError::InvalidOutput)?
        .trim();
    let (ahead, behind) = text
        .split_once(|character: char| character.is_ascii_whitespace())
        .ok_or(GitReadError::InvalidOutput)?;
    Ok((
        ahead.parse().map_err(|_| GitReadError::InvalidOutput)?,
        behind
            .trim()
            .parse()
            .map_err(|_| GitReadError::InvalidOutput)?,
    ))
}

fn valid_ref_component(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn valid_ref_name(value: &str) -> bool {
    value.starts_with("refs/")
        && !value.contains("..")
        && !value.contains("//")
        && !value.contains("@{")
        && !value.ends_with('/')
        && value
            .split('/')
            .all(|part| valid_ref_component(part) && !part.ends_with(".lock"))
}

fn parse_git_sync_target(
    config_records: &[u8],
    branch_relative: &str,
) -> Result<GitSyncTarget, GitSyncError> {
    if config_records.len() > MAX_GIT_CONFIG_BYTES
        || (!config_records.is_empty() && !config_records.ends_with(b"\0"))
    {
        return Err(GitSyncError::InvalidConfiguration);
    }
    let remote_key = format!("branch.{branch_relative}.remote");
    let merge_key = format!("branch.{branch_relative}.merge");
    let mut remote_name = None;
    let mut remote_ref = None;
    for record in config_records
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let separator = record
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(GitSyncError::InvalidConfiguration)?;
        let key = std::str::from_utf8(&record[..separator])
            .map_err(|_| GitSyncError::InvalidConfiguration)?;
        let value = std::str::from_utf8(&record[separator + 1..])
            .map_err(|_| GitSyncError::InvalidConfiguration)?;
        if key == remote_key {
            if remote_name.replace(value.to_owned()).is_some() {
                return Err(GitSyncError::InvalidConfiguration);
            }
        } else if key == merge_key && remote_ref.replace(value.to_owned()).is_some() {
            return Err(GitSyncError::InvalidConfiguration);
        }
    }
    let remote_name = remote_name.ok_or(GitSyncError::NoUpstream)?;
    let remote_ref = remote_ref.ok_or(GitSyncError::NoUpstream)?;
    if !valid_remote_name(&remote_name)
        || remote_name == "."
        || !remote_ref.starts_with("refs/heads/")
        || !valid_ref_name(&remote_ref)
    {
        return Err(GitSyncError::InvalidConfiguration);
    }
    let url_key = format!("remote.{remote_name}.url");
    let mut remote_url = None;
    for record in config_records
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let separator = record
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(GitSyncError::InvalidConfiguration)?;
        let key = std::str::from_utf8(&record[..separator])
            .map_err(|_| GitSyncError::InvalidConfiguration)?;
        if key == url_key {
            let value = std::str::from_utf8(&record[separator + 1..])
                .map_err(|_| GitSyncError::InvalidConfiguration)?;
            if remote_url.replace(value.to_owned()).is_some() {
                return Err(GitSyncError::InvalidConfiguration);
            }
        }
    }
    let remote_url = remote_url.ok_or(GitSyncError::NoUpstream)?;
    let validated =
        validate_remote_url(&remote_url).map_err(|_| GitSyncError::InvalidConfiguration)?;
    let remote_branch = remote_ref
        .strip_prefix("refs/heads/")
        .and_then(safe_git_label)
        .ok_or(GitSyncError::InvalidConfiguration)?;
    Ok(GitSyncTarget {
        remote_name,
        remote_url: validated.canonical,
        transport: validated.transport,
        host: validated.host,
        remote_branch,
        remote_ref,
    })
}

#[allow(dead_code, reason = "consumed by Phase 5C1 candidate construction")]
fn inspect_git_commit_repository(
    workspace: &WorkspaceView,
) -> Result<GitCommitRepositoryPreflight, GitCommitPlanError> {
    if !workspace.issues().is_empty() {
        return Err(GitCommitPlanError::WorkspaceIssues);
    }
    let facts = workspace
        .git_initialization_facts()
        .map_err(|error| match error {
            GitInitializeStorageError::Conflict => GitCommitPlanError::PlanStale,
            GitInitializeStorageError::Unsafe => GitCommitPlanError::UnsafeRepository,
            GitInitializeStorageError::Unsupported => GitCommitPlanError::UnsupportedRepository,
            GitInitializeStorageError::Io => GitCommitPlanError::Io,
        })?;
    if !facts.writable() {
        return Err(GitCommitPlanError::ReadOnly);
    }
    let capability = workspace
        .git_repository_capability()
        .map_err(|error| match error {
            GitRepositoryCapabilityError::NotRepository => GitCommitPlanError::NotRepository,
            GitRepositoryCapabilityError::Unsafe => GitCommitPlanError::UnsafeRepository,
            GitRepositoryCapabilityError::Io => GitCommitPlanError::Io,
        })?;
    let repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_commit_snapshot_error)?;
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let config_output = run_git(
        &executable,
        None,
        &[
            "config",
            "--file",
            capability.config_path(),
            "--no-includes",
            "-z",
            "--list",
        ],
    )
    .map_err(map_git_commit_read_error)?;
    let descriptor_config =
        fs::read(capability.config_path()).map_err(|_| GitCommitPlanError::Io)?;
    if descriptor_config != repository.config.bytes {
        return Err(GitCommitPlanError::PlanStale);
    }
    let (object_format, identity) = parse_git_commit_config(&config_output.stdout)?;
    let reference = parse_git_head_reference(&repository.head.bytes)?;
    let branch_relative = reference
        .strip_prefix("refs/heads/")
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let branch_storage = capability
        .read_commit_branch_snapshot(branch_relative)
        .map_err(map_git_commit_snapshot_error)?;
    let branch = parse_git_commit_branch(
        &repository.head.bytes,
        branch_storage
            .target
            .as_ref()
            .map(|target| target.bytes.as_slice()),
        repository
            .packed_refs
            .as_ref()
            .map(|packed| packed.bytes.as_slice()),
        object_format,
    )?;
    let index = match (
        branch.expected_object_id.is_none(),
        repository.index.as_ref(),
    ) {
        (true, None) => None,
        (false, Some(index)) => Some(parse_git_index(&index.bytes, object_format)?),
        _ => return Err(GitCommitPlanError::UnsupportedIndex),
    };
    let (head_tree_object_id, tree) = if let Some(parent) = branch.expected_object_id.as_deref() {
        let sandbox = trusted_executable("/usr/bin/bwrap")
            .ok_or(GitCommitPlanError::UnsupportedRepository)?;
        let tree_expression = format!("{parent}^{{tree}}");
        let tree_output = run_git_commit_read(
            &executable,
            &sandbox,
            &capability,
            object_format,
            &["rev-parse", "--verify", &tree_expression],
            128,
        )
        .map_err(map_git_commit_read_error)?;
        let head_tree_object_id =
            parse_git_commit_object_id_line(&tree_output.stdout, object_format)?;
        let output = run_git_commit_read(
            &executable,
            &sandbox,
            &capability,
            object_format,
            &["ls-tree", "-r", "-t", "-l", "--full-tree", "-z", parent],
            MAX_COMMIT_TREE_BYTES,
        )
        .map_err(map_git_commit_read_error)?;
        let tree = parse_git_commit_tree(&output.stdout, object_format)?;
        verify_git_commit_tree_objects(
            &executable,
            &sandbox,
            &capability,
            object_format,
            parent,
            &head_tree_object_id,
            &tree,
        )?;
        (Some(head_tree_object_id), tree)
    } else {
        (
            None,
            GitCommitTree {
                entries: Vec::new(),
            },
        )
    };
    let managed = inspect_git_commit_managed_inventory(workspace, object_format)?;
    let prior_managed = prior_managed_candidate_paths(&tree)?;
    let mut current_managed = Vec::new();
    current_managed
        .try_reserve_exact(managed.files.len())
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    for file in &managed.files {
        let mut path = Vec::new();
        path.try_reserve_exact(file.path.len())
            .map_err(|_| GitCommitPlanError::ResourceLimit)?;
        path.extend_from_slice(file.path.as_bytes());
        current_managed.push(path);
    }
    if git_commit_path_collision(&tree, &prior_managed, &current_managed) {
        return Err(GitCommitPlanError::PathCollision);
    }
    let final_repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_commit_snapshot_error)?;
    let final_branch = capability
        .read_commit_branch_snapshot(branch_relative)
        .map_err(map_git_commit_snapshot_error)?;
    if final_repository != repository || final_branch != branch_storage {
        return Err(GitCommitPlanError::PlanStale);
    }
    Ok(GitCommitRepositoryPreflight {
        workspace_id: workspace.manifest().workspace_id().to_string(),
        capability,
        repository,
        branch_storage,
        branch,
        identity,
        object_format,
        head_tree_object_id,
        index,
        tree,
        managed,
    })
}

#[allow(dead_code, reason = "consumed by Phase 5C1 candidate construction")]
fn prepare_git_commit_staging(
    preflight: &GitCommitRepositoryPreflight,
    state_home: &Path,
) -> Result<GitCommitPreparedStaging, GitCommitPlanError> {
    let operation_id = new_operation_id().to_string();
    let mut stage_hasher = blake3::Hasher::new();
    stage_hasher.update(b"rehow:git-commit-stage:v1\0");
    hash_clone_field(&mut stage_hasher, operation_id.as_bytes());
    let stage_name = hex_lower(stage_hasher.finalize().as_bytes());
    let object_format = match preflight.object_format {
        GitObjectFormat::Sha1 => "sha1",
        GitObjectFormat::Sha256 => "sha256",
    };
    let provenance = format!(
        "rehow-git-commit-stage-v1\noperation_id={operation_id}\nobject_format={object_format}\nbranch={}\nparent={}\nstage=prepared\n",
        preflight.branch.reference,
        preflight
            .branch
            .expected_object_id
            .as_deref()
            .unwrap_or("unborn"),
    );
    let config: &[u8] = match preflight.object_format {
        GitObjectFormat::Sha1 => {
            b"[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = true\n"
        }
        GitObjectFormat::Sha256 => {
            b"[core]\n\trepositoryformatversion = 1\n\tfilemode = true\n\tbare = true\n[extensions]\n\tobjectformat = sha256\n"
        }
    };
    let private_index = match (
        preflight.repository.index.as_ref(),
        preflight.index.as_ref(),
    ) {
        (Some(index), Some(snapshot)) => Some(git_index_without_extensions(
            &index.bytes,
            preflight.object_format,
            snapshot,
        )?),
        (None, None) => None,
        _ => return Err(GitCommitPlanError::UnsupportedIndex),
    };
    let staging = create_git_commit_staging(
        state_home,
        &stage_name,
        provenance.as_bytes(),
        config,
        private_index.as_deref(),
    )
    .map_err(map_git_commit_staging_error)?;

    match (
        preflight.branch.expected_object_id.as_ref(),
        preflight.head_tree_object_id.as_ref(),
        preflight.repository.index.as_ref(),
    ) {
        (Some(_), Some(expected_tree), Some(_)) => {
            let executable = trusted_executable("/usr/bin/git")
                .ok_or(GitCommitPlanError::UnsupportedRepository)?;
            let sandbox = trusted_executable("/usr/bin/bwrap")
                .ok_or(GitCommitPlanError::UnsupportedRepository)?;
            let actual_tree = run_git_commit_staging_write_tree(
                &executable,
                &sandbox,
                &preflight.capability,
                &staging,
                preflight.object_format,
            )?;
            if &actual_tree != expected_tree {
                return Err(GitCommitPlanError::UnsupportedIndex);
            }
        }
        (None, None, None) => {}
        _ => return Err(GitCommitPlanError::UnsupportedIndex),
    }
    let prior_managed = validate_prior_git_commit_workspace(preflight, &staging)?;
    staging
        .verify_size_limit(MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    Ok(GitCommitPreparedStaging {
        staging,
        prior_managed,
    })
}

#[allow(dead_code, reason = "consumed by the complete Phase 5C1 opaque plan")]
fn construct_git_commit_candidate(
    workspace: &WorkspaceView,
    preflight: &GitCommitRepositoryPreflight,
    prepared: GitCommitPreparedStaging,
    message: &str,
) -> Result<GitCommitCandidate, GitCommitPlanError> {
    let message = normalize_git_commit_message(message)?;
    if preflight
        .managed
        .files
        .len()
        .checked_add(prepared.prior_managed.files.len())
        .is_none_or(|count| count > MAX_COMMIT_STAGING_FILES)
    {
        return Err(GitCommitPlanError::ResourceLimit);
    }
    materialize_current_git_commit_workspace(workspace, preflight, &prepared.staging)?;
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    hash_current_git_commit_blobs(
        &executable,
        &sandbox,
        &preflight.capability,
        &prepared.staging,
        preflight.object_format,
        &preflight.managed,
    )?;
    let (created, updated, deleted) =
        git_commit_effect_counts(&prepared.prior_managed, &preflight.managed)?;
    if created == 0 && updated == 0 && deleted == 0 {
        return Err(GitCommitPlanError::NothingToCommit);
    }
    let tree_object_id = build_git_commit_candidate_tree(
        &executable,
        &sandbox,
        &preflight.capability,
        &prepared.staging,
        preflight.object_format,
        &preflight.tree,
        &prepared.prior_managed,
        &preflight.managed,
    )?;
    if preflight
        .head_tree_object_id
        .as_ref()
        .is_some_and(|head| head == &tree_object_id)
    {
        return Err(GitCommitPlanError::NothingToCommit);
    }
    let timestamp = current_timestamp().to_string();
    let (commit_object_id, git_timestamp) = create_git_commit_candidate_commit(
        &executable,
        &sandbox,
        &preflight.capability,
        &prepared.staging,
        preflight.object_format,
        &tree_object_id,
        preflight.branch.expected_object_id.as_deref(),
        &preflight.identity,
        &timestamp,
        &message,
    )?;
    prepared
        .staging
        .verify_size_limit(MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    Ok(GitCommitCandidate {
        prepared,
        tree_object_id,
        commit_object_id,
        created,
        updated,
        deleted,
        git_timestamp,
    })
}

pub fn plan_git_commit(
    workspace: &WorkspaceView,
    state_home: &Path,
    message: &str,
) -> Result<GitCommitPlan, GitCommitPlanError> {
    let _deadline = GitCommitDeadlineGuard::enter();
    let started = Instant::now();
    let message = normalize_git_commit_message(message)?;
    ensure_no_incomplete_git_commit_intent(workspace, state_home)?;
    let preflight = inspect_git_commit_repository(workspace)?;
    ensure_git_commit_plan_time(started)?;
    let prepared = prepare_git_commit_staging(&preflight, state_home)?;
    ensure_git_commit_plan_time(started)?;
    let candidate = construct_git_commit_candidate(workspace, &preflight, prepared, &message)?;
    ensure_git_commit_plan_time(started)?;
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let unrelated_changes = count_unrelated_git_commit_changes(
        &executable,
        &sandbox,
        &preflight.capability,
        preflight.object_format,
        &candidate.prepared.prior_managed,
        &preflight.managed,
    )?;
    ensure_git_commit_plan_time(started)?;
    let final_preflight = inspect_git_commit_repository(workspace)?;
    if !same_git_commit_preflight(&preflight, &final_preflight) {
        return Err(GitCommitPlanError::PlanStale);
    }
    candidate
        .prepared
        .staging
        .verify_size_limit(MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    let staging_snapshot = candidate
        .prepared
        .staging
        .snapshot(MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    if !candidate
        .prepared
        .staging
        .verify_snapshot(staging_snapshot, MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?
    {
        return Err(GitCommitPlanError::PlanStale);
    }
    ensure_git_commit_plan_time(started)?;
    let mut random = [0_u8; 32];
    fill_random(&mut random).map_err(|_| GitCommitPlanError::Io)?;
    let branch = preflight.branch.display.clone();
    let (created, updated, deleted) = (candidate.created, candidate.updated, candidate.deleted);
    Ok(GitCommitPlan {
        token: PlanToken::from_random_bytes(random),
        branch,
        created,
        updated,
        deleted,
        unrelated_changes,
        message,
        staging_snapshot,
        preflight,
        candidate,
    })
}

fn ensure_no_incomplete_git_commit_intent(
    workspace: &WorkspaceView,
    state_home: &Path,
) -> Result<(), GitCommitPlanError> {
    if pending_git_commit_recovery(workspace, state_home)?.is_some()
        || pending_git_sync_recovery_at(workspace, state_home, None)?.is_some()
    {
        Err(GitCommitPlanError::OperationInProgress)
    } else {
        Ok(())
    }
}

#[allow(
    dead_code,
    reason = "Phase 5C3 maps this pathless recovery fact into native Git status"
)]
pub fn pending_git_commit_recovery(
    workspace: &WorkspaceView,
    state_home: &Path,
) -> Result<Option<OperationId>, GitCommitPlanError> {
    pending_git_commit_recovery_at(workspace, state_home, None)
}

fn pending_git_commit_recovery_at(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<Option<OperationId>, GitCommitPlanError> {
    let session = workspace
        .open_git_commit_recovery_session(state_home, runtime_directory)
        .map_err(map_git_commit_intent_error)?;
    let intents = session
        .list_intents()
        .map_err(map_git_commit_intent_error)?;
    for intent in intents {
        if intent.stage == GitCommitIntentStage::Conflict {
            session
                .quarantine_staging(
                    &intent.staging_name,
                    (intent.staging_device, intent.staging_inode),
                )
                .map_err(map_git_commit_intent_error)?;
            continue;
        }
        if git_commit_intent_is_resumable(intent.stage) {
            return Ok(Some(intent.operation_id));
        }
    }
    Ok(None)
}

fn git_commit_intent_is_resumable(stage: GitCommitIntentStage) -> bool {
    matches!(
        stage,
        GitCommitIntentStage::Prepared
            | GitCommitIntentStage::IndexPublished
            | GitCommitIntentStage::RefPublished
    )
}

fn pending_git_sync_recovery_at(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<Option<OperationId>, GitCommitPlanError> {
    let session = workspace
        .open_git_commit_recovery_session(state_home, runtime_directory)
        .map_err(map_git_commit_intent_error)?;
    Ok(session
        .list_sync_intents()
        .map_err(map_git_commit_intent_error)?
        .into_iter()
        .find(|intent| intent.stage != GitSyncIntentStage::Complete)
        .map(|intent| intent.operation_id))
}

pub fn git_sync_recovery_required(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<bool, GitSyncError> {
    Ok(
        pending_git_sync_recovery_at(workspace, state_home, runtime_directory)
            .map_err(map_commit_plan_to_sync)?
            .is_some(),
    )
}

pub fn git_commit_recovery_required(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<bool, GitCommitPlanError> {
    Ok(pending_git_commit_recovery_at(workspace, state_home, runtime_directory)?.is_some())
}

pub fn cancel_git_commit_plan(plan: GitCommitPlan) -> Result<(), GitCommitPlanError> {
    plan.candidate
        .prepared
        .staging
        .quarantine()
        .map_err(map_git_commit_staging_error)
}

fn prepare_git_commit_pack(
    plan: &mut GitCommitPlan,
) -> Result<GitCommitPackedCandidate, GitCommitPlanError> {
    let _deadline = GitCommitDeadlineGuard::enter();
    let staging = &plan.candidate.prepared.staging;
    if !staging
        .verify_snapshot(plan.staging_snapshot, MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?
    {
        return Err(GitCommitPlanError::PlanStale);
    }
    let object_id_hex_len = match plan.preflight.object_format {
        GitObjectFormat::Sha1 => 40,
        GitObjectFormat::Sha256 => 64,
    };
    let object_ids = staging
        .inspect_loose_object_ids(
            object_id_hex_len,
            MAX_COMMIT_STAGING_OBJECTS,
            MAX_COMMIT_STAGING_BYTES,
        )
        .map_err(map_git_commit_staging_error)?;
    if object_ids.is_empty() {
        return Ok(GitCommitPackedCandidate {
            object_ids,
            sources: Vec::new(),
            staging_snapshot: plan.staging_snapshot,
        });
    }
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let mut input = Vec::new();
    input
        .try_reserve_exact(object_ids.len().saturating_mul(object_id_hex_len + 1))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    for object_id in &object_ids {
        input.extend_from_slice(object_id.as_bytes());
        input.push(b'\n');
    }
    let command = git_commit_staging_write_command(
        &executable,
        &sandbox,
        &plan.preflight.capability,
        staging,
        &[
            "-c",
            "pack.writeReverseIndex=false",
            "pack-objects",
            "--compression=0",
            "--window=0",
            "--no-reuse-delta",
            "--no-reuse-object",
            "/repo.git/objects/pack/pack",
        ],
    );
    let output = run_bounded_git_command_with_input(
        command,
        COMMIT_PROCESS_TIMEOUT,
        128,
        OUTPUT_LIMIT,
        Some(input),
    )
    .map_err(map_git_commit_read_error)?;
    let pack_hash = parse_git_commit_object_id_line(&output.stdout, plan.preflight.object_format)?;
    let sources = staging
        .inspect_pack_sources(object_id_hex_len, MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    verify_git_commit_pack(
        &executable,
        &sandbox,
        staging,
        plan.preflight.object_format,
        &object_ids,
        &sources,
        &pack_hash,
    )?;
    staging
        .verify_size_limit(MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    let staging_snapshot = staging
        .snapshot(MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    plan.staging_snapshot = staging_snapshot;
    Ok(GitCommitPackedCandidate {
        object_ids,
        sources,
        staging_snapshot,
    })
}

#[allow(
    dead_code,
    reason = "Phase 5C2B publication consumes this private candidate index"
)]
fn prepare_git_commit_candidate_index(
    plan: &mut GitCommitPlan,
) -> Result<GitCommitIndexCandidate, GitCommitPlanError> {
    let staging = &plan.candidate.prepared.staging;
    if !staging
        .verify_snapshot(plan.staging_snapshot, MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?
    {
        return Err(GitCommitPlanError::PlanStale);
    }
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let mut command = git_commit_staging_write_command(
        &executable,
        &sandbox,
        &plan.preflight.capability,
        staging,
        &["read-tree", &plan.candidate.tree_object_id],
    );
    command.stdin(Stdio::null());
    let output = run_bounded_git_command(command, COMMIT_PROCESS_TIMEOUT, 128)
        .map_err(map_git_commit_read_error)?;
    if !output.stdout.is_empty() {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let actual_tree = run_git_commit_staging_write_tree(
        &executable,
        &sandbox,
        &plan.preflight.capability,
        staging,
        plan.preflight.object_format,
    )?;
    if actual_tree != plan.candidate.tree_object_id {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut source = staging
        .inspect_index_source(MAX_COMMIT_INDEX_BYTES as u64)
        .map_err(map_git_commit_staging_error)?;
    let mut bytes = read_git_commit_staging_index(staging, &source)?;
    let snapshot = parse_git_index(&bytes, plan.preflight.object_format)?;
    if snapshot.has_cache_tree {
        bytes = git_index_without_extensions(&bytes, plan.preflight.object_format, &snapshot)?;
        staging
            .replace_index(&bytes)
            .map_err(map_git_commit_staging_error)?;
        source = staging
            .inspect_index_source(MAX_COMMIT_INDEX_BYTES as u64)
            .map_err(map_git_commit_staging_error)?;
        bytes = read_git_commit_staging_index(staging, &source)?;
        if parse_git_index(&bytes, plan.preflight.object_format)?.has_cache_tree {
            return Err(GitCommitPlanError::UnsupportedIndex);
        }
        let actual_tree = run_git_commit_staging_write_tree(
            &executable,
            &sandbox,
            &plan.preflight.capability,
            staging,
            plan.preflight.object_format,
        )?;
        if actual_tree != plan.candidate.tree_object_id {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
    }
    staging
        .verify_size_limit(MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    let staging_snapshot = staging
        .snapshot(MAX_COMMIT_STAGING_BYTES)
        .map_err(map_git_commit_staging_error)?;
    plan.staging_snapshot = staging_snapshot;
    Ok(GitCommitIndexCandidate {
        source,
        bytes,
        staging_snapshot,
    })
}

fn read_git_commit_staging_index(
    staging: &GitCommitStaging,
    source: &GitClonePackSource,
) -> Result<Vec<u8>, GitCommitPlanError> {
    let mut file = staging
        .open_index_source(source, MAX_COMMIT_INDEX_BYTES as u64)
        .map_err(map_git_commit_staging_error)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(
            usize::try_from(source.size).map_err(|_| GitCommitPlanError::ResourceLimit)?,
        )
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    file.read_to_end(&mut bytes)
        .map_err(|_| GitCommitPlanError::Io)?;
    if bytes.len() as u64 != source.size {
        return Err(GitCommitPlanError::PlanStale);
    }
    Ok(bytes)
}

#[allow(
    dead_code,
    reason = "Phase 5C2B consumes this after the recoverable index/ref transaction is added"
)]
fn install_git_commit_candidate_pack(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    plan: &mut GitCommitPlan,
) -> Result<GitCommitPackedCandidate, GitCommitPlanError> {
    let packed = prepare_git_commit_pack(plan)?;
    workspace
        .with_git_commit_writer_lock(state_home, runtime_directory, || {
            let current = inspect_git_commit_repository(workspace)?;
            if !same_git_commit_preflight(&plan.preflight, &current)
                || !plan
                    .candidate
                    .prepared
                    .staging
                    .verify_snapshot(packed.staging_snapshot, MAX_COMMIT_STAGING_BYTES)
                    .map_err(map_git_commit_staging_error)?
            {
                return Err(GitCommitPlanError::PlanStale);
            }
            if packed.sources.is_empty() {
                return Ok(());
            }
            let mut files = packed
                .sources
                .iter()
                .map(|source| plan.candidate.prepared.staging.open_pack_source(source))
                .collect::<Result<Vec<_>, _>>()
                .map_err(map_git_commit_staging_error)?;
            let object_id_hex_len = match plan.preflight.object_format {
                GitObjectFormat::Sha1 => 40,
                GitObjectFormat::Sha256 => 64,
            };
            current
                .capability
                .install_commit_pack_sources(&packed.sources, &mut files, object_id_hex_len)
                .map_err(map_git_commit_pack_install_error)
        })
        .map_err(map_git_commit_staging_error)??;
    Ok(packed)
}

#[allow(
    dead_code,
    reason = "Phase 5C2B wraps this exact metadata transaction in a durable intent"
)]
fn publish_git_commit_metadata_without_intent(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    plan: &mut GitCommitPlan,
) -> Result<(), GitCommitPlanError> {
    install_git_commit_candidate_pack(workspace, state_home, runtime_directory, plan)?;
    let candidate_index = prepare_git_commit_candidate_index(plan)?;
    let mut candidate_ref = Vec::new();
    candidate_ref
        .try_reserve_exact(plan.candidate.commit_object_id.len().saturating_add(1))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    candidate_ref.extend_from_slice(plan.candidate.commit_object_id.as_bytes());
    candidate_ref.push(b'\n');
    workspace
        .with_git_commit_writer_lock(state_home, runtime_directory, || {
            let current = inspect_git_commit_repository(workspace)?;
            if !same_git_commit_preflight(&plan.preflight, &current)
                || !plan
                    .candidate
                    .prepared
                    .staging
                    .verify_snapshot(candidate_index.staging_snapshot, MAX_COMMIT_STAGING_BYTES)
                    .map_err(map_git_commit_staging_error)?
            {
                return Err(GitCommitPlanError::PlanStale);
            }
            let branch_relative = plan
                .preflight
                .branch
                .reference
                .strip_prefix("refs/heads/")
                .ok_or(GitCommitPlanError::UnsafeRepository)?;
            current
                .capability
                .publish_commit_metadata(
                    &plan.preflight.repository,
                    &plan.preflight.branch_storage,
                    branch_relative,
                    &candidate_index.bytes,
                    &candidate_ref,
                    |_| Ok(()),
                )
                .map_err(map_git_commit_metadata_storage_error)
        })
        .map_err(map_git_commit_staging_error)??;
    Ok(())
}

#[allow(
    dead_code,
    reason = "Phase 5C3 exposes this only through native-confirmed plan consumption"
)]
fn publish_git_commit_with_intent(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    plan: &mut GitCommitPlan,
) -> Result<GitCommitPublicationOutcome, GitCommitPlanError> {
    publish_git_commit_with_intent_cancellable(
        workspace,
        state_home,
        runtime_directory,
        plan,
        &|| false,
    )
}

fn publish_git_commit_with_intent_cancellable(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    plan: &mut GitCommitPlan,
    cancelled: &impl Fn() -> bool,
) -> Result<GitCommitPublicationOutcome, GitCommitPlanError> {
    install_git_commit_candidate_pack(workspace, state_home, runtime_directory, plan)?;
    let candidate_index = prepare_git_commit_candidate_index(plan)?;
    let mut candidate_ref = Vec::new();
    candidate_ref
        .try_reserve_exact(plan.candidate.commit_object_id.len().saturating_add(1))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    candidate_ref.extend_from_slice(plan.candidate.commit_object_id.as_bytes());
    candidate_ref.push(b'\n');

    let session = workspace
        .open_git_commit_recovery_session(state_home, runtime_directory)
        .map_err(map_git_commit_intent_error)?;
    for intent in session
        .list_intents()
        .map_err(map_git_commit_intent_error)?
    {
        if intent.stage == GitCommitIntentStage::Conflict {
            session
                .quarantine_staging(
                    &intent.staging_name,
                    (intent.staging_device, intent.staging_inode),
                )
                .map_err(map_git_commit_intent_error)?;
        } else if git_commit_intent_is_resumable(intent.stage) {
            return Err(GitCommitPlanError::OperationInProgress);
        }
    }
    let current = inspect_git_commit_repository(workspace)?;
    if !same_git_commit_preflight(&plan.preflight, &current)
        || !plan
            .candidate
            .prepared
            .staging
            .verify_snapshot(candidate_index.staging_snapshot, MAX_COMMIT_STAGING_BYTES)
            .map_err(map_git_commit_staging_error)?
    {
        return Err(GitCommitPlanError::PlanStale);
    }
    let branch_relative = plan
        .preflight
        .branch
        .reference
        .strip_prefix("refs/heads/")
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let (git_device, git_inode) = current
        .capability
        .git_directory_identity()
        .map_err(map_git_commit_snapshot_error)?;
    let head_hash = session
        .store_bytes(&plan.preflight.repository.head.bytes)
        .map_err(map_git_commit_intent_error)?;
    let config_hash = session
        .store_bytes(&plan.preflight.repository.config.bytes)
        .map_err(map_git_commit_intent_error)?;
    let before_index_hash = plan
        .preflight
        .repository
        .index
        .as_ref()
        .map(|snapshot| session.store_bytes(&snapshot.bytes))
        .transpose()
        .map_err(map_git_commit_intent_error)?;
    let candidate_index_hash = session
        .store_bytes(&candidate_index.bytes)
        .map_err(map_git_commit_intent_error)?;
    let packed_refs_hash = plan
        .preflight
        .repository
        .packed_refs
        .as_ref()
        .map(|snapshot| session.store_bytes(&snapshot.bytes))
        .transpose()
        .map_err(map_git_commit_intent_error)?;
    let before_ref_hash = plan
        .preflight
        .branch_storage
        .target
        .as_ref()
        .map(|snapshot| session.store_bytes(&snapshot.bytes))
        .transpose()
        .map_err(map_git_commit_intent_error)?;
    let candidate_ref_hash = session
        .store_bytes(&candidate_ref)
        .map_err(map_git_commit_intent_error)?;
    let representation = match plan.preflight.branch.representation {
        GitBranchRepresentation::Unborn => GitCommitIntentBranchRepresentation::Unborn,
        GitBranchRepresentation::Loose => GitCommitIntentBranchRepresentation::Loose,
        GitBranchRepresentation::Packed => GitCommitIntentBranchRepresentation::Packed,
    };
    let before_index_mode = plan
        .preflight
        .repository
        .index
        .as_ref()
        .map_or(0o600, |snapshot| snapshot.mode());
    let before_ref_mode = plan
        .preflight
        .branch_storage
        .target
        .as_ref()
        .map_or(0o600, |snapshot| snapshot.mode());
    let operation_id = new_operation_id();
    let (root_device, root_inode) = session.root_identity();
    let staging_name = plan.candidate.prepared.staging.stage_name().to_owned();
    let (staging_device, staging_inode) = plan
        .candidate
        .prepared
        .staging
        .stage_identity()
        .map_err(map_git_commit_staging_error)?;
    let mut intent = GitCommitIntent {
        generation: 1,
        operation_id,
        workspace_id: session.workspace_id(),
        root_hash: session.root_hash(),
        root_device,
        root_inode,
        git_device,
        git_inode,
        branch_relative: branch_relative.to_owned(),
        representation,
        head_hash,
        config_hash,
        before_index_hash,
        before_index_mode,
        candidate_index_hash,
        candidate_index_mode: before_index_mode,
        packed_refs_hash,
        before_ref_hash,
        before_ref_mode,
        candidate_ref_hash,
        candidate_ref_mode: before_ref_mode,
        plan_hash: [0; 32],
        stage: GitCommitIntentStage::Prepared,
        staging_name,
        staging_device,
        staging_inode,
    };
    intent.plan_hash = git_commit_intent_plan_hash(&intent, &plan.candidate.commit_object_id);
    if cancelled() {
        return Err(GitCommitPlanError::Cancelled);
    }
    session
        .install_intent(&intent)
        .map_err(map_git_commit_intent_error)?;

    let publication = current.capability.publish_commit_metadata(
        &plan.preflight.repository,
        &plan.preflight.branch_storage,
        branch_relative,
        &candidate_index.bytes,
        &candidate_ref,
        |stage| {
            let durable_stage = match stage {
                GitCommitMetadataStage::IndexPublished => GitCommitIntentStage::IndexPublished,
                GitCommitMetadataStage::RefPublished => GitCommitIntentStage::RefPublished,
            };
            let next = intent
                .next_stage(durable_stage)
                .map_err(|_| GitCommitMetadataStorageError::Unsafe)?;
            session
                .install_intent(&next)
                .map_err(map_git_commit_intent_to_metadata_error)?;
            intent = next;
            Ok(())
        },
    );
    if let Err(error) = publication {
        return Err(map_git_commit_metadata_storage_error(error));
    }
    let workspace_changed = verify_completed_git_commit_publication(
        workspace,
        &current.capability,
        &intent,
        &plan.preflight.repository.head.bytes,
        &plan.preflight.repository.config.bytes,
        plan.preflight
            .repository
            .packed_refs
            .as_ref()
            .map(|value| value.bytes.as_slice()),
        &candidate_index.bytes,
        &candidate_ref,
    )?;
    session
        .quarantine_staging(
            &intent.staging_name,
            (intent.staging_device, intent.staging_inode),
        )
        .map_err(map_git_commit_intent_error)?;
    intent = intent
        .next_stage(GitCommitIntentStage::Complete)
        .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
    session
        .install_intent(&intent)
        .map_err(map_git_commit_intent_error)?;
    Ok(GitCommitPublicationOutcome {
        operation_id,
        workspace_changed,
    })
}

pub fn apply_git_commit(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    plan: GitCommitPlan,
) -> Result<GitCommitPublicationOutcome, GitCommitPlanError> {
    apply_git_commit_with_cancellation(workspace, state_home, runtime_directory, plan, || false)
}

pub fn apply_git_commit_with_cancellation(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    mut plan: GitCommitPlan,
    cancelled: impl Fn() -> bool,
) -> Result<GitCommitPublicationOutcome, GitCommitPlanError> {
    match publish_git_commit_with_intent_cancellable(
        workspace,
        state_home,
        runtime_directory,
        &mut plan,
        &cancelled,
    ) {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            if matches!(
                pending_git_commit_intent_owns_plan_staging(
                    workspace,
                    state_home,
                    runtime_directory,
                    &plan,
                ),
                Ok(false)
            ) {
                let _ = cancel_git_commit_plan(plan);
            }
            Err(error)
        }
    }
}

fn pending_git_commit_intent_owns_plan_staging(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    plan: &GitCommitPlan,
) -> Result<bool, GitCommitPlanError> {
    let expected_name = plan.candidate.prepared.staging.stage_name();
    let expected_identity = plan
        .candidate
        .prepared
        .staging
        .stage_identity()
        .map_err(map_git_commit_staging_error)?;
    let session = workspace
        .open_git_commit_recovery_session(state_home, runtime_directory)
        .map_err(map_git_commit_intent_error)?;
    let intents = session
        .list_intents()
        .map_err(map_git_commit_intent_error)?;
    for intent in intents {
        if intent.stage == GitCommitIntentStage::Conflict {
            session
                .quarantine_staging(
                    &intent.staging_name,
                    (intent.staging_device, intent.staging_inode),
                )
                .map_err(map_git_commit_intent_error)?;
            continue;
        }
        if git_commit_intent_is_resumable(intent.stage)
            && intent.staging_name == expected_name
            && (intent.staging_device, intent.staging_inode) == expected_identity
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[allow(
    dead_code,
    reason = "Phase 5C3 and Git status call this for discovered incomplete intents"
)]
fn resume_git_commit_publication(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    operation_id: OperationId,
) -> Result<bool, GitCommitPlanError> {
    let session = workspace
        .open_git_commit_recovery_session(state_home, runtime_directory)
        .map_err(map_git_commit_intent_error)?;
    let mut intent = session
        .read_intent(operation_id)
        .map_err(map_git_commit_intent_error)?;
    if intent.stage == GitCommitIntentStage::Conflict {
        session
            .quarantine_staging(
                &intent.staging_name,
                (intent.staging_device, intent.staging_inode),
            )
            .map_err(map_git_commit_intent_error)?;
        return Err(GitCommitPlanError::PlanStale);
    }
    let capability = workspace
        .git_repository_capability()
        .map_err(|error| match error {
            GitRepositoryCapabilityError::NotRepository => GitCommitPlanError::NotRepository,
            GitRepositoryCapabilityError::Unsafe => GitCommitPlanError::UnsafeRepository,
            GitRepositoryCapabilityError::Io => GitCommitPlanError::Io,
        })?;
    let (git_device, git_inode) = capability
        .git_directory_identity()
        .map_err(map_git_commit_snapshot_error)?;
    if session.root_identity() != (intent.root_device, intent.root_inode)
        || (git_device, git_inode) != (intent.git_device, intent.git_inode)
    {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let head = session
        .read_bytes(&intent.head_hash)
        .map_err(map_git_commit_intent_error)?;
    let config = session
        .read_bytes(&intent.config_hash)
        .map_err(map_git_commit_intent_error)?;
    let before_index = intent
        .before_index_hash
        .as_ref()
        .map(|hash| session.read_bytes(hash))
        .transpose()
        .map_err(map_git_commit_intent_error)?;
    let candidate_index = session
        .read_bytes(&intent.candidate_index_hash)
        .map_err(map_git_commit_intent_error)?;
    let packed_refs = intent
        .packed_refs_hash
        .as_ref()
        .map(|hash| session.read_bytes(hash))
        .transpose()
        .map_err(map_git_commit_intent_error)?;
    let before_ref = intent
        .before_ref_hash
        .as_ref()
        .map(|hash| session.read_bytes(hash))
        .transpose()
        .map_err(map_git_commit_intent_error)?;
    let candidate_ref = session
        .read_bytes(&intent.candidate_ref_hash)
        .map_err(map_git_commit_intent_error)?;
    verify_git_commit_intent_plan_hash(&intent, &candidate_ref)?;
    capability
        .recover_commit_lock_residue(
            &intent.branch_relative,
            before_index
                .as_deref()
                .map(|bytes| (bytes, intent.before_index_mode)),
            (&candidate_index, intent.candidate_index_mode),
            before_ref
                .as_deref()
                .map(|bytes| (bytes, intent.before_ref_mode)),
            (&candidate_ref, intent.candidate_ref_mode),
        )
        .map_err(map_git_commit_metadata_storage_error)?;
    let repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_commit_snapshot_error)?;
    let branch = capability
        .read_commit_branch_snapshot(&intent.branch_relative)
        .map_err(map_git_commit_snapshot_error)?;
    if repository.head.bytes != head
        || repository.config.bytes != config
        || repository.packed_refs.as_ref().map(|value| &value.bytes) != packed_refs.as_ref()
    {
        return mark_git_commit_intent_conflict(&session, &mut intent);
    }
    let index_is_before = git_commit_snapshot_matches(
        repository.index.as_ref(),
        before_index.as_deref(),
        intent.before_index_mode,
    );
    let index_is_candidate = git_commit_snapshot_matches(
        repository.index.as_ref(),
        Some(&candidate_index),
        intent.candidate_index_mode,
    );
    let ref_is_before = git_commit_snapshot_matches(
        branch.target.as_ref(),
        before_ref.as_deref(),
        intent.before_ref_mode,
    );
    let ref_is_candidate = git_commit_snapshot_matches(
        branch.target.as_ref(),
        Some(&candidate_ref),
        intent.candidate_ref_mode,
    );
    let branch_relative = intent.branch_relative.clone();

    match (
        index_is_before,
        index_is_candidate,
        ref_is_before,
        ref_is_candidate,
    ) {
        (true, false, true, false) if intent.stage == GitCommitIntentStage::Prepared => {
            capability
                .publish_commit_metadata(
                    &repository,
                    &branch,
                    &branch_relative,
                    &candidate_index,
                    &candidate_ref,
                    |stage| persist_recovered_git_commit_stage(&session, &mut intent, stage),
                )
                .map_err(map_git_commit_metadata_storage_error)?;
        }
        (true, false, true, false) => {
            return mark_git_commit_intent_conflict(&session, &mut intent);
        }
        (false, true, true, false) => {
            if intent.stage == GitCommitIntentStage::Prepared {
                advance_git_commit_intent(
                    &session,
                    &mut intent,
                    GitCommitIntentStage::IndexPublished,
                )?;
            }
            if intent.stage != GitCommitIntentStage::IndexPublished {
                return mark_git_commit_intent_conflict(&session, &mut intent);
            }
            capability
                .publish_commit_ref_after_index(
                    &repository,
                    &branch,
                    &branch_relative,
                    &candidate_index,
                    &candidate_ref,
                    |stage| persist_recovered_git_commit_stage(&session, &mut intent, stage),
                )
                .map_err(map_git_commit_metadata_storage_error)?;
        }
        (false, true, false, true) => {
            if intent.stage == GitCommitIntentStage::Prepared {
                advance_git_commit_intent(
                    &session,
                    &mut intent,
                    GitCommitIntentStage::IndexPublished,
                )?;
            }
            if intent.stage == GitCommitIntentStage::IndexPublished {
                advance_git_commit_intent(
                    &session,
                    &mut intent,
                    GitCommitIntentStage::RefPublished,
                )?;
            }
            if !matches!(
                intent.stage,
                GitCommitIntentStage::RefPublished | GitCommitIntentStage::Complete
            ) {
                return Err(GitCommitPlanError::UnsafeRepository);
            }
        }
        _ => return mark_git_commit_intent_conflict(&session, &mut intent),
    }
    if !matches!(
        intent.stage,
        GitCommitIntentStage::RefPublished | GitCommitIntentStage::Complete
    ) {
        return Err(GitCommitPlanError::OperationInProgress);
    }
    let workspace_changed = verify_completed_git_commit_publication(
        workspace,
        &capability,
        &intent,
        &head,
        &config,
        packed_refs.as_deref(),
        &candidate_index,
        &candidate_ref,
    )?;
    session
        .quarantine_staging(
            &intent.staging_name,
            (intent.staging_device, intent.staging_inode),
        )
        .map_err(map_git_commit_intent_error)?;
    if intent.stage == GitCommitIntentStage::RefPublished {
        advance_git_commit_intent(&session, &mut intent, GitCommitIntentStage::Complete)?;
    }
    Ok(workspace_changed)
}

pub fn resume_pending_git_commit_publication(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<GitCommitPublicationOutcome, GitCommitPlanError> {
    let operation_id = pending_git_commit_recovery_at(workspace, state_home, runtime_directory)?
        .ok_or(GitCommitPlanError::RecoveryNotRequired)?;
    let workspace_changed =
        resume_git_commit_publication(workspace, state_home, runtime_directory, operation_id)?;
    Ok(GitCommitPublicationOutcome {
        operation_id,
        workspace_changed,
    })
}

pub fn plan_git_commit_recovery(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
) -> Result<GitCommitRecoveryPlan, GitCommitPlanError> {
    let operation_id = pending_git_commit_recovery_at(workspace, state_home, runtime_directory)?
        .ok_or(GitCommitPlanError::RecoveryNotRequired)?;
    let mut random = [0_u8; 32];
    fill_random(&mut random).map_err(|_| GitCommitPlanError::Io)?;
    Ok(GitCommitRecoveryPlan {
        token: PlanToken::from_random_bytes(random),
        operation_id,
    })
}

pub fn resume_planned_git_commit_publication(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    plan: GitCommitRecoveryPlan,
) -> Result<GitCommitPublicationOutcome, GitCommitPlanError> {
    let current = pending_git_commit_recovery_at(workspace, state_home, runtime_directory)?
        .ok_or(GitCommitPlanError::RecoveryNotRequired)?;
    if current != plan.operation_id {
        return Err(GitCommitPlanError::PlanStale);
    }
    let workspace_changed =
        resume_git_commit_publication(workspace, state_home, runtime_directory, current)?;
    Ok(GitCommitPublicationOutcome {
        operation_id: current,
        workspace_changed,
    })
}

#[allow(clippy::too_many_arguments)]
fn verify_completed_git_commit_publication(
    workspace: &WorkspaceView,
    capability: &crate::storage::GitRepositoryCapability,
    intent: &GitCommitIntent,
    expected_head: &[u8],
    expected_config: &[u8],
    expected_packed_refs: Option<&[u8]>,
    candidate_index: &[u8],
    candidate_ref: &[u8],
) -> Result<bool, GitCommitPlanError> {
    let repository = capability
        .read_commit_repository_snapshot()
        .map_err(map_git_commit_snapshot_error)?;
    let branch = capability
        .read_commit_branch_snapshot(&intent.branch_relative)
        .map_err(map_git_commit_snapshot_error)?;
    if repository.head.bytes != expected_head
        || repository.config.bytes != expected_config
        || repository
            .packed_refs
            .as_ref()
            .map(|value| value.bytes.as_slice())
            != expected_packed_refs
        || !git_commit_snapshot_matches(
            repository.index.as_ref(),
            Some(candidate_index),
            intent.candidate_index_mode,
        )
        || !git_commit_snapshot_matches(
            branch.target.as_ref(),
            Some(candidate_ref),
            intent.candidate_ref_mode,
        )
    {
        return Err(GitCommitPlanError::PlanStale);
    }
    let expected_commit = std::str::from_utf8(candidate_ref)
        .ok()
        .and_then(|value| value.strip_suffix('\n'))
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let object_format =
        validate_object_id(expected_commit).ok_or(GitCommitPlanError::UnsafeRepository)?;
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let commit = run_git_commit_read(
        &executable,
        &sandbox,
        capability,
        object_format,
        &["cat-file", "commit", expected_commit],
        OUTPUT_LIMIT,
    )
    .map_err(map_git_commit_read_error)?;
    if git_object_id(object_format, "commit", &commit.stdout) != expected_commit {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut cursor = 0usize;
    let tree_line = take_git_commit_line(&commit.stdout, &mut cursor)?;
    let root_tree = tree_line
        .strip_prefix(b"tree ")
        .and_then(|value| std::str::from_utf8(value).ok())
        .filter(|value| validate_object_id(value) == Some(object_format))
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let tree_output = run_git_commit_read(
        &executable,
        &sandbox,
        capability,
        object_format,
        &[
            "ls-tree",
            "-r",
            "-t",
            "-l",
            "--full-tree",
            "-z",
            expected_commit,
        ],
        MAX_COMMIT_TREE_BYTES,
    )
    .map_err(map_git_commit_read_error)?;
    let tree = parse_git_commit_tree(&tree_output.stdout, object_format)?;
    verify_git_commit_index_matches_tree(candidate_index, object_format, &tree)?;
    verify_git_commit_tree_objects(
        &executable,
        &sandbox,
        capability,
        object_format,
        expected_commit,
        root_tree,
        &tree,
    )?;
    let fresh = workspace.fresh_snapshot().map_err(|error| match error {
        crate::storage::WorkspaceInspectError::ResourceLimit => GitCommitPlanError::ResourceLimit,
        crate::storage::WorkspaceInspectError::Io => GitCommitPlanError::Io,
        _ => GitCommitPlanError::PlanStale,
    })?;
    let fresh_managed = match inspect_git_commit_managed_inventory(&fresh, object_format) {
        Ok(managed) => managed,
        Err(GitCommitPlanError::WorkspaceIssues)
        | Err(GitCommitPlanError::AmbiguousManagedHistory)
        | Err(GitCommitPlanError::PlanStale) => return Ok(true),
        Err(error) => return Err(error),
    };
    git_commit_managed_effects_changed(&fresh_managed, &tree)
}

fn git_commit_managed_effects_changed(
    current: &GitCommitManagedInventory,
    candidate_tree: &GitCommitTree,
) -> Result<bool, GitCommitPlanError> {
    let candidate_paths = prior_managed_candidate_paths(candidate_tree)?;
    if candidate_paths.len() != current.files.len() {
        return Ok(true);
    }
    for (path, file) in candidate_paths.iter().zip(&current.files) {
        if path.as_slice() != file.path.as_bytes() {
            return Ok(true);
        }
        let Ok(index) = candidate_tree
            .entries
            .binary_search_by(|entry| entry.path.as_slice().cmp(path))
        else {
            return Ok(true);
        };
        if candidate_tree.entries[index].object_id != file.blob_object_id {
            return Ok(true);
        }
    }
    Ok(false)
}

fn verify_git_commit_index_matches_tree(
    index_bytes: &[u8],
    object_format: GitObjectFormat,
    tree: &GitCommitTree,
) -> Result<(), GitCommitPlanError> {
    let index = parse_git_index(index_bytes, object_format)?;
    if index.has_cache_tree {
        return Err(GitCommitPlanError::UnsupportedIndex);
    }
    let mut entries = index.entries.iter();
    for tree_entry in tree
        .entries
        .iter()
        .filter(|entry| entry.kind != GitCommitTreeEntryKind::Tree)
    {
        let index_entry = entries.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        if index_entry.mode != tree_entry.mode
            || index_entry.path != tree_entry.path
            || index_entry.object_id != decode_git_object_id(&tree_entry.object_id, object_format)?
        {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
    }
    if entries.next().is_some() {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    Ok(())
}

fn git_commit_snapshot_matches(
    actual: Option<&crate::storage::GitCommitFileSnapshot>,
    expected: Option<&[u8]>,
    mode: u32,
) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some(actual), Some(expected)) => actual.bytes == expected && actual.mode() == mode,
        _ => false,
    }
}

fn persist_recovered_git_commit_stage(
    session: &crate::storage::recovery::GitCommitRecoverySession,
    intent: &mut GitCommitIntent,
    stage: GitCommitMetadataStage,
) -> Result<(), GitCommitMetadataStorageError> {
    let stage = match stage {
        GitCommitMetadataStage::IndexPublished => GitCommitIntentStage::IndexPublished,
        GitCommitMetadataStage::RefPublished => GitCommitIntentStage::RefPublished,
    };
    advance_git_commit_intent(session, intent, stage).map_err(|error| match error {
        GitCommitPlanError::OperationInProgress | GitCommitPlanError::PlanStale => {
            GitCommitMetadataStorageError::Conflict
        }
        GitCommitPlanError::UnsafeRepository => GitCommitMetadataStorageError::Unsafe,
        GitCommitPlanError::UnsupportedRepository => GitCommitMetadataStorageError::Unsupported,
        GitCommitPlanError::ResourceLimit => GitCommitMetadataStorageError::TooLarge,
        _ => GitCommitMetadataStorageError::Io,
    })
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn persist_recovered_git_sync_stage(
    session: &crate::storage::recovery::GitCommitRecoverySession,
    intent: &mut GitSyncIntent,
    stage: GitCommitMetadataStage,
) -> Result<(), GitCommitMetadataStorageError> {
    let stage = match stage {
        GitCommitMetadataStage::IndexPublished => GitSyncIntentStage::IndexPublished,
        GitCommitMetadataStage::RefPublished => GitSyncIntentStage::RefPublished,
    };
    advance_git_sync_intent(session, intent, stage).map_err(|error| match error {
        GitSyncError::RecoveryRequired | GitSyncError::PlanStale => {
            GitCommitMetadataStorageError::Conflict
        }
        GitSyncError::Unsafe => GitCommitMetadataStorageError::Unsafe,
        GitSyncError::Unsupported => GitCommitMetadataStorageError::Unsupported,
        GitSyncError::ResourceLimit => GitCommitMetadataStorageError::TooLarge,
        _ => GitCommitMetadataStorageError::Io,
    })
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn advance_git_sync_intent(
    session: &crate::storage::recovery::GitCommitRecoverySession,
    intent: &mut GitSyncIntent,
    stage: GitSyncIntentStage,
) -> Result<(), GitSyncError> {
    let next = intent.next_stage(stage).map_err(|_| GitSyncError::Unsafe)?;
    session
        .install_sync_intent(&next)
        .map_err(map_git_commit_intent_to_sync)?;
    *intent = next;
    Ok(())
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn mark_git_sync_intent_conflict<T>(
    session: &crate::storage::recovery::GitCommitRecoverySession,
    intent: &mut GitSyncIntent,
) -> Result<T, GitSyncError> {
    if matches!(
        intent.stage,
        GitSyncIntentStage::Prepared
            | GitSyncIntentStage::WorkspacePublished
            | GitSyncIntentStage::IndexPublished
            | GitSyncIntentStage::RefPublished
    ) {
        advance_git_sync_intent(session, intent, GitSyncIntentStage::Conflict)?;
    }
    Err(GitSyncError::PlanStale)
}

fn advance_git_commit_intent(
    session: &crate::storage::recovery::GitCommitRecoverySession,
    intent: &mut GitCommitIntent,
    stage: GitCommitIntentStage,
) -> Result<(), GitCommitPlanError> {
    let next = intent
        .next_stage(stage)
        .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
    session
        .install_intent(&next)
        .map_err(map_git_commit_intent_error)?;
    *intent = next;
    Ok(())
}

fn mark_git_commit_intent_conflict<T>(
    session: &crate::storage::recovery::GitCommitRecoverySession,
    intent: &mut GitCommitIntent,
) -> Result<T, GitCommitPlanError> {
    session
        .quarantine_staging(
            &intent.staging_name,
            (intent.staging_device, intent.staging_inode),
        )
        .map_err(map_git_commit_intent_error)?;
    if git_commit_intent_is_resumable(intent.stage) {
        advance_git_commit_intent(session, intent, GitCommitIntentStage::Conflict)?;
    }
    Err(GitCommitPlanError::PlanStale)
}

fn hash_git_commit_intent_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn verify_git_commit_intent_plan_hash(
    intent: &GitCommitIntent,
    candidate_ref: &[u8],
) -> Result<(), GitCommitPlanError> {
    let commit_id = std::str::from_utf8(candidate_ref)
        .ok()
        .and_then(|value| value.strip_suffix('\n'))
        .filter(|value| validate_object_id(value).is_some())
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    if git_commit_intent_plan_hash(intent, commit_id) != intent.plan_hash {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    Ok(())
}

fn git_commit_intent_plan_hash(intent: &GitCommitIntent, commit_id: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rehow:git-commit-publication-intent:v2\0");
    hash_git_commit_intent_field(&mut hasher, intent.operation_id.to_string().as_bytes());
    hash_git_commit_intent_field(&mut hasher, intent.workspace_id.to_string().as_bytes());
    hasher.update(&intent.root_hash);
    hasher.update(&intent.root_device.to_be_bytes());
    hasher.update(&intent.root_inode.to_be_bytes());
    hasher.update(&intent.git_device.to_be_bytes());
    hasher.update(&intent.git_inode.to_be_bytes());
    hash_git_commit_intent_field(&mut hasher, intent.branch_relative.as_bytes());
    hasher.update(&[match intent.representation {
        GitCommitIntentBranchRepresentation::Unborn => 0,
        GitCommitIntentBranchRepresentation::Loose => 1,
        GitCommitIntentBranchRepresentation::Packed => 2,
    }]);
    hasher.update(&intent.head_hash);
    hasher.update(&intent.config_hash);
    hash_git_commit_intent_optional_hash(&mut hasher, intent.before_index_hash.as_ref());
    hasher.update(&intent.before_index_mode.to_be_bytes());
    hasher.update(&intent.candidate_index_hash);
    hasher.update(&intent.candidate_index_mode.to_be_bytes());
    hash_git_commit_intent_optional_hash(&mut hasher, intent.packed_refs_hash.as_ref());
    hash_git_commit_intent_optional_hash(&mut hasher, intent.before_ref_hash.as_ref());
    hasher.update(&intent.before_ref_mode.to_be_bytes());
    hasher.update(&intent.candidate_ref_hash);
    hasher.update(&intent.candidate_ref_mode.to_be_bytes());
    hash_git_commit_intent_field(&mut hasher, intent.staging_name.as_bytes());
    hasher.update(&intent.staging_device.to_be_bytes());
    hasher.update(&intent.staging_inode.to_be_bytes());
    hash_git_commit_intent_field(&mut hasher, commit_id.as_bytes());
    *hasher.finalize().as_bytes()
}

fn hash_git_commit_intent_optional_hash(hasher: &mut blake3::Hasher, value: Option<&[u8; 32]>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(value);
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn git_sync_intent_plan_hash(intent: &GitSyncIntent) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rehow:git-sync-publication-intent:v1\0");
    hash_git_commit_intent_field(&mut hasher, intent.operation_id.to_string().as_bytes());
    hash_git_commit_intent_field(&mut hasher, intent.workspace_id.to_string().as_bytes());
    hasher.update(&intent.root_hash);
    hasher.update(&intent.root_device.to_be_bytes());
    hasher.update(&intent.root_inode.to_be_bytes());
    hasher.update(&intent.git_device.to_be_bytes());
    hasher.update(&intent.git_inode.to_be_bytes());
    hash_git_commit_intent_field(&mut hasher, intent.branch_relative.as_bytes());
    hasher.update(&[match intent.representation {
        GitCommitIntentBranchRepresentation::Unborn => 0,
        GitCommitIntentBranchRepresentation::Loose => 1,
        GitCommitIntentBranchRepresentation::Packed => 2,
    }]);
    hasher.update(&intent.head_hash);
    hasher.update(&intent.config_hash);
    hasher.update(&intent.before_index_hash);
    hasher.update(&intent.before_index_mode.to_be_bytes());
    hasher.update(&intent.candidate_index_hash);
    hasher.update(&intent.candidate_index_mode.to_be_bytes());
    hash_git_commit_intent_optional_hash(&mut hasher, intent.packed_refs_hash.as_ref());
    hash_git_commit_intent_optional_hash(&mut hasher, intent.shallow_hash.as_ref());
    hasher.update(&intent.shallow_mode.to_be_bytes());
    hash_git_commit_intent_optional_hash(&mut hasher, intent.before_ref_hash.as_ref());
    hasher.update(&intent.before_ref_mode.to_be_bytes());
    hasher.update(&intent.candidate_ref_hash);
    hasher.update(&intent.candidate_ref_mode.to_be_bytes());
    hash_git_commit_intent_optional_hash(&mut hasher, intent.workspace_journal_hash.as_ref());
    hasher.update(&intent.workspace_plan_hash);
    *hasher.finalize().as_bytes()
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn verify_git_sync_intent_plan_hash(intent: &GitSyncIntent) -> Result<(), GitSyncError> {
    if git_sync_intent_plan_hash(intent) == intent.plan_hash {
        Ok(())
    } else {
        Err(GitSyncError::Unsafe)
    }
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn map_git_commit_intent_to_sync(error: GitCommitIntentError) -> GitSyncError {
    match error {
        GitCommitIntentError::Conflict => GitSyncError::RecoveryRequired,
        GitCommitIntentError::Unsafe => GitSyncError::Unsafe,
        GitCommitIntentError::Unsupported => GitSyncError::Unsupported,
        GitCommitIntentError::TooLarge => GitSyncError::ResourceLimit,
        GitCommitIntentError::Io => GitSyncError::Io,
    }
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn map_git_commit_pack_install_to_sync(error: GitCommitPackInstallError) -> GitSyncError {
    match error {
        GitCommitPackInstallError::Conflict => GitSyncError::PlanStale,
        GitCommitPackInstallError::Unsafe => GitSyncError::Unsafe,
        GitCommitPackInstallError::Unsupported => GitSyncError::Unsupported,
        GitCommitPackInstallError::TooLarge => GitSyncError::ResourceLimit,
        GitCommitPackInstallError::Io => GitSyncError::Io,
    }
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn map_git_commit_metadata_to_sync(error: GitCommitMetadataStorageError) -> GitSyncError {
    match error {
        GitCommitMetadataStorageError::Conflict => GitSyncError::PlanStale,
        GitCommitMetadataStorageError::Unsafe => GitSyncError::Unsafe,
        GitCommitMetadataStorageError::Unsupported => GitSyncError::Unsupported,
        GitCommitMetadataStorageError::TooLarge => GitSyncError::ResourceLimit,
        GitCommitMetadataStorageError::Io => GitSyncError::Io,
    }
}

fn map_git_commit_pack_install_error(error: GitCommitPackInstallError) -> GitCommitPlanError {
    match error {
        GitCommitPackInstallError::Conflict => GitCommitPlanError::PlanStale,
        GitCommitPackInstallError::Unsafe => GitCommitPlanError::UnsafeRepository,
        GitCommitPackInstallError::Unsupported => GitCommitPlanError::UnsupportedRepository,
        GitCommitPackInstallError::TooLarge => GitCommitPlanError::ResourceLimit,
        GitCommitPackInstallError::Io => GitCommitPlanError::Io,
    }
}

fn map_git_commit_metadata_storage_error(
    error: GitCommitMetadataStorageError,
) -> GitCommitPlanError {
    match error {
        GitCommitMetadataStorageError::Conflict => GitCommitPlanError::PlanStale,
        GitCommitMetadataStorageError::Unsafe => GitCommitPlanError::UnsafeRepository,
        GitCommitMetadataStorageError::Unsupported => GitCommitPlanError::UnsupportedRepository,
        GitCommitMetadataStorageError::TooLarge => GitCommitPlanError::ResourceLimit,
        GitCommitMetadataStorageError::Io => GitCommitPlanError::Io,
    }
}

fn map_git_commit_intent_error(error: GitCommitIntentError) -> GitCommitPlanError {
    match error {
        GitCommitIntentError::Conflict => GitCommitPlanError::OperationInProgress,
        GitCommitIntentError::Unsafe => GitCommitPlanError::UnsafeRepository,
        GitCommitIntentError::Unsupported => GitCommitPlanError::UnsupportedRepository,
        GitCommitIntentError::TooLarge => GitCommitPlanError::ResourceLimit,
        GitCommitIntentError::Io => GitCommitPlanError::Io,
    }
}

fn map_git_commit_intent_to_metadata_error(
    error: GitCommitIntentError,
) -> GitCommitMetadataStorageError {
    match error {
        GitCommitIntentError::Conflict => GitCommitMetadataStorageError::Conflict,
        GitCommitIntentError::Unsafe => GitCommitMetadataStorageError::Unsafe,
        GitCommitIntentError::Unsupported => GitCommitMetadataStorageError::Unsupported,
        GitCommitIntentError::TooLarge => GitCommitMetadataStorageError::TooLarge,
        GitCommitIntentError::Io => GitCommitMetadataStorageError::Io,
    }
}

fn ensure_git_commit_plan_time(started: Instant) -> Result<(), GitCommitPlanError> {
    if started.elapsed() >= COMMIT_PLAN_TIMEOUT || active_git_commit_deadline_reached() {
        Err(GitCommitPlanError::ResourceLimit)
    } else {
        Ok(())
    }
}

fn same_git_commit_preflight(
    planned: &GitCommitRepositoryPreflight,
    current: &GitCommitRepositoryPreflight,
) -> bool {
    planned.workspace_id == current.workspace_id
        && planned.repository == current.repository
        && planned.branch_storage == current.branch_storage
        && planned.branch == current.branch
        && planned.identity == current.identity
        && planned.object_format == current.object_format
        && planned.head_tree_object_id == current.head_tree_object_id
        && planned.index == current.index
        && planned.tree == current.tree
        && planned.managed == current.managed
}

fn materialize_current_git_commit_workspace(
    workspace: &WorkspaceView,
    preflight: &GitCommitRepositoryPreflight,
    staging: &GitCommitStaging,
) -> Result<(), GitCommitPlanError> {
    if workspace.manifest().workspace_id().to_string() != preflight.workspace_id
        || !workspace.issues().is_empty()
    {
        return Err(GitCommitPlanError::PlanStale);
    }
    for expected in &preflight.managed.files {
        if active_git_commit_deadline_reached() {
            return Err(GitCommitPlanError::ResourceLimit);
        }
        let (_, limit) = classify_git_materialization_path(&expected.path)
            .ok_or(GitCommitPlanError::PlanStale)?;
        let snapshot = workspace
            .read_git_commit_managed_file(&expected.path, limit)
            .map_err(map_git_commit_workspace_read_error)?;
        let size =
            u64::try_from(snapshot.bytes.len()).map_err(|_| GitCommitPlanError::ResourceLimit)?;
        if snapshot.revision != expected.revision
            || size != expected.size
            || snapshot.mode & 0o111 != 0
            || git_blob_object_id(preflight.object_format, &snapshot.bytes)
                != expected.blob_object_id
        {
            return Err(GitCommitPlanError::PlanStale);
        }
        staging
            .write_candidate_file(&expected.path, &snapshot.bytes)
            .map_err(map_git_commit_staging_error)?;
    }
    staging
        .finish_candidate_tree()
        .map_err(map_git_commit_staging_error)
}

fn git_commit_effect_counts(
    prior: &GitCommitManagedInventory,
    current: &GitCommitManagedInventory,
) -> Result<(u32, u32, u32), GitCommitPlanError> {
    let mut prior_index = 0usize;
    let mut current_index = 0usize;
    let mut created = 0u32;
    let mut updated = 0u32;
    let mut deleted = 0u32;
    while prior_index < prior.files.len() || current_index < current.files.len() {
        match (
            prior.files.get(prior_index),
            current.files.get(current_index),
        ) {
            (Some(prior), Some(current)) => match prior.path.cmp(&current.path) {
                std::cmp::Ordering::Less => {
                    deleted = deleted
                        .checked_add(1)
                        .ok_or(GitCommitPlanError::ResourceLimit)?;
                    prior_index += 1;
                }
                std::cmp::Ordering::Greater => {
                    created = created
                        .checked_add(1)
                        .ok_or(GitCommitPlanError::ResourceLimit)?;
                    current_index += 1;
                }
                std::cmp::Ordering::Equal => {
                    if prior.blob_object_id != current.blob_object_id {
                        updated = updated
                            .checked_add(1)
                            .ok_or(GitCommitPlanError::ResourceLimit)?;
                    }
                    prior_index += 1;
                    current_index += 1;
                }
            },
            (Some(_), None) => {
                deleted = deleted
                    .checked_add(1)
                    .ok_or(GitCommitPlanError::ResourceLimit)?;
                prior_index += 1;
            }
            (None, Some(_)) => {
                created = created
                    .checked_add(1)
                    .ok_or(GitCommitPlanError::ResourceLimit)?;
                current_index += 1;
            }
            (None, None) => break,
        }
    }
    Ok((created, updated, deleted))
}

fn count_unrelated_git_commit_changes(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    object_format: GitObjectFormat,
    prior: &GitCommitManagedInventory,
    current: &GitCommitManagedInventory,
) -> Result<u32, GitCommitPlanError> {
    let mut synthetic_config =
        create_git_format_config(object_format).map_err(map_git_commit_read_error)?;
    let config_descriptor = synthetic_config.as_raw_fd();
    let config_descriptor_text = config_descriptor.to_string();
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--ro-bind",
            repository.work_tree_path(),
            "/workspace",
            "--tmpfs",
            "/repo.git",
            "--ro-bind",
            repository.objects_path(),
            "/repo.git/objects",
            "--ro-bind",
            repository.refs_path(),
            "/repo.git/refs",
            "--ro-bind",
            repository.head_path(),
            "/repo.git/HEAD",
            "--file",
            &config_descriptor_text,
            "/repo.git/config",
            "--tmpfs",
            "/repo.git/objects/info",
            "--tmpfs",
            "/tmp",
            "--chdir",
            "/workspace",
            "--setenv",
            "GIT_WORK_TREE",
            "/workspace",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
        ]);
    for (source, destination) in [
        (repository.index_path(), "/repo.git/index"),
        (repository.packed_refs_path(), "/repo.git/packed-refs"),
        (repository.shallow_path(), "/repo.git/shallow"),
    ] {
        if let Some(source) = source {
            command.args(["--ro-bind", source, destination]);
        }
    }
    command
        .args(["--", executable.to_str().ok_or(GitCommitPlanError::Io)?])
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "credential.helper=",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "diff.external=",
            "-c",
            "color.ui=false",
            "-c",
            "core.autocrlf=false",
            "-c",
            "status.submoduleSummary=false",
            "-c",
            "protocol.allow=never",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=normal",
            "--ignore-submodules=all",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    // SAFETY: the closure only clears CLOEXEC on the owned, sealed
    // synthetic-config descriptor so Bubblewrap can copy it into the
    // otherwise read-only repository view.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(config_descriptor, libc::F_GETFD);
            if flags < 0
                || libc::fcntl(config_descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let output = run_bounded_git_command(command, COMMIT_PROCESS_TIMEOUT, MAX_COMMIT_TREE_BYTES);
    let _ = synthetic_config.seek(SeekFrom::Start(0));
    let output = output.map_err(map_git_commit_read_error)?;
    parse_unrelated_git_commit_changes(&output.stdout, prior, current)
}

fn parse_unrelated_git_commit_changes(
    status: &[u8],
    prior: &GitCommitManagedInventory,
    current: &GitCommitManagedInventory,
) -> Result<u32, GitCommitPlanError> {
    if !status.is_empty() && !status.ends_with(&[0]) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut managed = prior
        .files
        .iter()
        .chain(&current.files)
        .map(|file| file.path.as_bytes())
        .collect::<Vec<_>>();
    managed.sort_unstable();
    managed.dedup();
    let mut records = status
        .strip_suffix(&[0])
        .unwrap_or(status)
        .split(|byte| *byte == 0);
    let mut unrelated = 0u32;
    while let Some(record) = records.next() {
        if record.is_empty() {
            if status.is_empty() {
                break;
            }
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        let mut paths = [&[][..], &[][..]];
        let path_count = match record[0] {
            b'1' if record.get(1) == Some(&b' ') => {
                paths[0] = porcelain_v2_path(record, 9)?;
                1
            }
            b'2' if record.get(1) == Some(&b' ') => {
                paths[0] = porcelain_v2_path(record, 10)?;
                paths[1] = records
                    .next()
                    .filter(|path| !path.is_empty())
                    .ok_or(GitCommitPlanError::UnsafeRepository)?;
                2
            }
            b'u' if record.get(1) == Some(&b' ') => {
                paths[0] = porcelain_v2_path(record, 11)?;
                1
            }
            b'?' | b'!' if record.get(1) == Some(&b' ') && record.len() > 2 => {
                paths[0] = &record[2..];
                1
            }
            _ => return Err(GitCommitPlanError::UnsafeRepository),
        };
        if paths[..path_count]
            .iter()
            .any(|path| managed.binary_search(path).is_err())
        {
            unrelated = unrelated
                .checked_add(1)
                .ok_or(GitCommitPlanError::ResourceLimit)?;
        }
    }
    Ok(unrelated)
}

fn porcelain_v2_path(record: &[u8], field_count: usize) -> Result<&[u8], GitCommitPlanError> {
    let mut start = 0usize;
    for _ in 1..field_count {
        let offset = record[start..]
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or(GitCommitPlanError::UnsafeRepository)?;
        start = start
            .checked_add(offset.saturating_add(1))
            .ok_or(GitCommitPlanError::ResourceLimit)?;
    }
    let path = record
        .get(start..)
        .filter(|path| !path.is_empty())
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    Ok(path)
}

fn run_git_commit_staging_write_tree(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    staging: &GitCommitStaging,
    object_format: GitObjectFormat,
) -> Result<String, GitCommitPlanError> {
    let mut command =
        git_commit_staging_write_command(executable, sandbox, repository, staging, &["write-tree"]);
    command.stdin(Stdio::null());
    let output = run_bounded_git_command(command, COMMIT_PROCESS_TIMEOUT, 128)
        .map_err(map_git_commit_read_error)?;
    parse_git_commit_object_id_line(&output.stdout, object_format)
}

fn git_commit_staging_write_command(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    staging: &GitCommitStaging,
    arguments: &[&str],
) -> Command {
    let descriptor = staging.git_descriptor();
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--bind",
            staging.git_path(),
            "/repo.git",
            "--ro-bind",
            repository.objects_path(),
            "/active-objects",
            "--tmpfs",
            "/active-objects/info",
            "--chdir",
            "/tmp",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
            "--setenv",
            "GIT_INDEX_FILE",
            "/repo.git/index",
            "--setenv",
            "GIT_OBJECT_DIRECTORY",
            "/repo.git/objects",
            "--setenv",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "/active-objects",
            "--",
        ])
        .arg(executable)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "credential.helper=",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "diff.external=",
            "-c",
            "protocol.allow=never",
        ])
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    // SAFETY: only the retained staging descriptor has CLOEXEC cleared for the
    // Bubblewrap bind; the child receives no arbitrary descriptor selection.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(descriptor, libc::F_GETFD);
            if flags < 0 || libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}

fn hash_current_git_commit_blobs(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    staging: &GitCommitStaging,
    object_format: GitObjectFormat,
    managed: &GitCommitManagedInventory,
) -> Result<(), GitCommitPlanError> {
    let git_descriptor = staging.git_descriptor();
    let candidate_descriptor = staging.candidate_descriptor();
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--bind",
            staging.git_path(),
            "/repo.git",
            "--ro-bind",
            repository.objects_path(),
            "/active-objects",
            "--tmpfs",
            "/active-objects/info",
            "--ro-bind",
            staging.candidate_path(),
            "/candidate",
            "--chdir",
            "/candidate",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
            "--setenv",
            "GIT_OBJECT_DIRECTORY",
            "/repo.git/objects",
            "--setenv",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "/active-objects",
            "--",
        ])
        .arg(executable)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "credential.helper=",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "diff.external=",
            "-c",
            "protocol.allow=never",
            "hash-object",
            "-w",
            "--stdin-paths",
            "--no-filters",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    // SAFETY: only the two core-created staging descriptors are inherited for
    // fixed Bubblewrap bind sources.
    unsafe {
        command.pre_exec(move || {
            for descriptor in [git_descriptor, candidate_descriptor] {
                let flags = libc::fcntl(descriptor, libc::F_GETFD);
                if flags < 0
                    || libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    stream_current_git_commit_hashes(command, object_format, managed)
}

fn stream_current_git_commit_hashes(
    mut command: Command,
    object_format: GitObjectFormat,
    managed: &GitCommitManagedInventory,
) -> Result<(), GitCommitPlanError> {
    let mut child = command.spawn().map_err(|_| GitCommitPlanError::Io)?;
    let stdin = child.stdin.take().ok_or(GitCommitPlanError::Io)?;
    let stdout = child.stdout.take().ok_or(GitCommitPlanError::Io)?;
    let stderr = child.stderr.take().ok_or(GitCommitPlanError::Io)?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stderr_reader = read_capped_with_limit(stderr, Arc::clone(&exceeded), OUTPUT_LIMIT);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let started = Instant::now();
    let mut worker_result = None;
    let mut process_status = None;
    let mut terminal_error = None;
    thread::scope(|scope| {
        scope.spawn(move || {
            let mut writer = BufWriter::new(stdin);
            let mut reader = BufReader::new(stdout);
            let result = (|| {
                for file in &managed.files {
                    writer
                        .write_all(file.path.as_bytes())
                        .and_then(|_| writer.write_all(b"\n"))
                        .and_then(|_| writer.flush())
                        .map_err(|_| GitCommitPlanError::Io)?;
                    let line = read_git_commit_batch_header(&mut reader)?;
                    let object_id = parse_git_commit_object_id_line(&line, object_format)?;
                    if object_id != file.blob_object_id {
                        return Err(GitCommitPlanError::PlanStale);
                    }
                }
                drop(writer);
                let mut unexpected = [0_u8; 1];
                if reader
                    .read(&mut unexpected)
                    .map_err(|_| GitCommitPlanError::Io)?
                    != 0
                {
                    return Err(GitCommitPlanError::UnsafeRepository);
                }
                Ok(())
            })();
            let _ = sender.send(result);
        });
        loop {
            if worker_result.is_none() {
                match receiver.try_recv() {
                    Ok(result) => worker_result = Some(result),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        terminal_error = Some(GitCommitPlanError::Io);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
            if process_status.is_none() {
                match child.try_wait() {
                    Ok(Some(status)) => process_status = Some(status),
                    Ok(None) => {}
                    Err(_) => terminal_error = Some(GitCommitPlanError::Io),
                }
            }
            if worker_result.is_some() && process_status.is_some() {
                break;
            }
            if terminal_error.is_none()
                && (exceeded.load(Ordering::Relaxed)
                    || started.elapsed() >= COMMIT_PLAN_TIMEOUT
                    || active_git_commit_deadline_reached())
            {
                terminal_error = Some(GitCommitPlanError::ResourceLimit);
            }
            if terminal_error.is_some() {
                terminate_child(&mut child);
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    });
    let _ = stderr_reader.join().map_err(|_| GitCommitPlanError::Io)?;
    if let Some(error) = terminal_error {
        return Err(error);
    }
    worker_result.ok_or(GitCommitPlanError::Io)??;
    if process_status.is_some_and(|status| status.success()) {
        Ok(())
    } else {
        Err(GitCommitPlanError::UnsafeRepository)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_git_commit_candidate_tree(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    staging: &GitCommitStaging,
    object_format: GitObjectFormat,
    head: &GitCommitTree,
    prior: &GitCommitManagedInventory,
    current: &GitCommitManagedInventory,
) -> Result<String, GitCommitPlanError> {
    let prior_paths = prior
        .files
        .iter()
        .map(|file| file.path.as_bytes().to_vec())
        .collect::<std::collections::BTreeSet<_>>();
    let mut prior_ancestors = std::collections::BTreeSet::new();
    for path in &prior_paths {
        for (index, byte) in path.iter().enumerate() {
            if *byte == b'/' {
                prior_ancestors.insert(path[..index].to_vec());
            }
        }
    }
    let mut directories = std::collections::BTreeMap::<Vec<u8>, GitCandidateDirectory>::new();
    directories.insert(Vec::new(), GitCandidateDirectory::default());
    for entry in &head.entries {
        if entry.kind == GitCommitTreeEntryKind::Tree {
            ensure_git_candidate_directories(&mut directories, &entry.path)?;
            let directory = directories
                .get_mut(&entry.path)
                .ok_or(GitCommitPlanError::UnsafeRepository)?;
            directory.preserve_if_empty = !prior_ancestors.contains(&entry.path);
        } else if !prior_paths.contains(&entry.path) {
            insert_git_candidate_leaf(
                &mut directories,
                &entry.path,
                entry.mode,
                entry.kind,
                &entry.object_id,
            )?;
        }
    }
    for file in &current.files {
        insert_git_candidate_leaf(
            &mut directories,
            file.path.as_bytes(),
            0o100644,
            GitCommitTreeEntryKind::Blob,
            &file.blob_object_id,
        )?;
    }
    let object_count = current
        .files
        .len()
        .checked_add(directories.len())
        .and_then(|count| count.checked_add(1))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    if object_count > MAX_COMMIT_STAGING_OBJECTS
        || directories.len() > MAX_COMMIT_TREE_ENTRIES.saturating_add(1)
    {
        return Err(GitCommitPlanError::ResourceLimit);
    }
    let command = git_commit_staging_write_command(
        executable,
        sandbox,
        repository,
        staging,
        &["mktree", "-z", "--batch"],
    );
    stream_git_commit_candidate_trees(command, object_format, directories)
}

fn ensure_git_candidate_directories(
    directories: &mut std::collections::BTreeMap<Vec<u8>, GitCandidateDirectory>,
    path: &[u8],
) -> Result<(), GitCommitPlanError> {
    if path.is_empty() || !valid_opaque_git_path(path) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    for (index, byte) in path.iter().enumerate() {
        if *byte == b'/' {
            directories.entry(path[..index].to_vec()).or_default();
        }
    }
    directories.entry(path.to_vec()).or_default();
    Ok(())
}

fn insert_git_candidate_leaf(
    directories: &mut std::collections::BTreeMap<Vec<u8>, GitCandidateDirectory>,
    path: &[u8],
    mode: u32,
    kind: GitCommitTreeEntryKind,
    object_id: &str,
) -> Result<(), GitCommitPlanError> {
    if path.is_empty()
        || !valid_opaque_git_path(path)
        || kind == GitCommitTreeEntryKind::Tree
        || directories.contains_key(path)
    {
        return Err(GitCommitPlanError::PathCollision);
    }
    let split = path.iter().rposition(|byte| *byte == b'/');
    let (parent, name) = split.map_or((&[][..], path), |index| {
        (&path[..index], &path[index + 1..])
    });
    if !parent.is_empty() {
        ensure_git_candidate_directories(directories, parent)?;
    }
    let directory = directories
        .get_mut(parent)
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let child = GitCandidateTreeChild {
        mode,
        kind,
        object_id: object_id.to_owned(),
        name: name.to_vec(),
    };
    if directory.children.insert(name.to_vec(), child).is_some() {
        return Err(GitCommitPlanError::PathCollision);
    }
    Ok(())
}

fn stream_git_commit_candidate_trees(
    mut command: Command,
    object_format: GitObjectFormat,
    directories: std::collections::BTreeMap<Vec<u8>, GitCandidateDirectory>,
) -> Result<String, GitCommitPlanError> {
    let mut child = command.spawn().map_err(|_| GitCommitPlanError::Io)?;
    let stdin = child.stdin.take().ok_or(GitCommitPlanError::Io)?;
    let stdout = child.stdout.take().ok_or(GitCommitPlanError::Io)?;
    let stderr = child.stderr.take().ok_or(GitCommitPlanError::Io)?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stderr_reader = read_capped_with_limit(stderr, Arc::clone(&exceeded), OUTPUT_LIMIT);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let started = Instant::now();
    let mut worker_result = None;
    let mut process_status = None;
    let mut terminal_error = None;
    thread::scope(|scope| {
        scope.spawn(move || {
            let result =
                stream_git_commit_candidate_tree_bytes(stdin, stdout, object_format, directories);
            let _ = sender.send(result);
        });
        loop {
            if worker_result.is_none() {
                match receiver.try_recv() {
                    Ok(result) => worker_result = Some(result),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        terminal_error = Some(GitCommitPlanError::Io);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
            if process_status.is_none() {
                match child.try_wait() {
                    Ok(Some(status)) => process_status = Some(status),
                    Ok(None) => {}
                    Err(_) => terminal_error = Some(GitCommitPlanError::Io),
                }
            }
            if worker_result.is_some() && process_status.is_some() {
                break;
            }
            if terminal_error.is_none()
                && (exceeded.load(Ordering::Relaxed)
                    || started.elapsed() >= COMMIT_PLAN_TIMEOUT
                    || active_git_commit_deadline_reached())
            {
                terminal_error = Some(GitCommitPlanError::ResourceLimit);
            }
            if terminal_error.is_some() {
                terminate_child(&mut child);
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    });
    let _ = stderr_reader.join().map_err(|_| GitCommitPlanError::Io)?;
    if let Some(error) = terminal_error {
        return Err(error);
    }
    let root = worker_result.ok_or(GitCommitPlanError::Io)??;
    if process_status.is_some_and(|status| status.success()) {
        Ok(root)
    } else {
        Err(GitCommitPlanError::UnsafeRepository)
    }
}

fn stream_git_commit_candidate_tree_bytes(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    object_format: GitObjectFormat,
    mut directories: std::collections::BTreeMap<Vec<u8>, GitCandidateDirectory>,
) -> Result<String, GitCommitPlanError> {
    let mut paths = directories.keys().cloned().collect::<Vec<_>>();
    paths.sort_unstable_by(|left, right| {
        right
            .iter()
            .filter(|byte| **byte == b'/')
            .count()
            .cmp(&left.iter().filter(|byte| **byte == b'/').count())
            .then_with(|| right.len().cmp(&left.len()))
            .then_with(|| left.cmp(right))
    });
    let mut writer = BufWriter::new(stdin);
    let mut reader = BufReader::new(stdout);
    let mut root = None;
    for path in paths {
        let directory = directories
            .remove(&path)
            .ok_or(GitCommitPlanError::UnsafeRepository)?;
        if !path.is_empty() && directory.children.is_empty() && !directory.preserve_if_empty {
            continue;
        }
        for child in directory.children.values() {
            let kind = match child.kind {
                GitCommitTreeEntryKind::Tree => "tree",
                GitCommitTreeEntryKind::Blob => "blob",
                GitCommitTreeEntryKind::Commit => "commit",
            };
            write!(writer, "{:06o} {kind} {}\t", child.mode, child.object_id)
                .map_err(|_| GitCommitPlanError::Io)?;
            writer
                .write_all(&child.name)
                .and_then(|_| writer.write_all(&[0]))
                .map_err(|_| GitCommitPlanError::Io)?;
        }
        writer
            .write_all(&[0])
            .and_then(|_| writer.flush())
            .map_err(|_| GitCommitPlanError::Io)?;
        let line = read_git_commit_batch_header(&mut reader)?;
        let object_id = parse_git_commit_object_id_line(&line, object_format)?;
        if git_tree_object_id(object_format, directory.children.values())? != object_id {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        if path.is_empty() {
            root = Some(object_id);
        } else {
            let split = path.iter().rposition(|byte| *byte == b'/');
            let (parent, name) = split.map_or((&[][..], path.as_slice()), |index| {
                (&path[..index], &path[index + 1..])
            });
            let parent = directories
                .get_mut(parent)
                .ok_or(GitCommitPlanError::UnsafeRepository)?;
            let child = GitCandidateTreeChild {
                mode: 0o040000,
                kind: GitCommitTreeEntryKind::Tree,
                object_id,
                name: name.to_vec(),
            };
            if parent.children.insert(name.to_vec(), child).is_some() {
                return Err(GitCommitPlanError::PathCollision);
            }
        }
    }
    drop(writer);
    let mut unexpected = [0_u8; 1];
    if reader
        .read(&mut unexpected)
        .map_err(|_| GitCommitPlanError::Io)?
        != 0
    {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    root.ok_or(GitCommitPlanError::UnsafeRepository)
}

fn git_tree_object_id<'a>(
    object_format: GitObjectFormat,
    children: impl Iterator<Item = &'a GitCandidateTreeChild>,
) -> Result<String, GitCommitPlanError> {
    let mut children = children.collect::<Vec<_>>();
    children.sort_unstable_by(|left, right| git_tree_child_cmp(left, right));
    let mut bytes = Vec::new();
    for child in children {
        let mode = match child.mode {
            0o040000 => "40000".to_owned(),
            0o100644 => "100644".to_owned(),
            0o100755 => "100755".to_owned(),
            0o120000 => "120000".to_owned(),
            0o160000 => "160000".to_owned(),
            _ => return Err(GitCommitPlanError::UnsafeRepository),
        };
        bytes.extend_from_slice(mode.as_bytes());
        bytes.push(b' ');
        bytes.extend_from_slice(&child.name);
        bytes.push(0);
        bytes.extend_from_slice(&decode_git_object_id(&child.object_id, object_format)?);
    }
    Ok(git_object_id(object_format, "tree", &bytes))
}

fn git_tree_child_cmp(
    left: &GitCandidateTreeChild,
    right: &GitCandidateTreeChild,
) -> std::cmp::Ordering {
    let common = left.name.len().min(right.name.len());
    let prefix = left.name[..common].cmp(&right.name[..common]);
    if prefix != std::cmp::Ordering::Equal {
        return prefix;
    }
    let left_terminator = if left.kind == GitCommitTreeEntryKind::Tree {
        b'/'
    } else {
        0
    };
    let right_terminator = if right.kind == GitCommitTreeEntryKind::Tree {
        b'/'
    } else {
        0
    };
    match (left.name.get(common), right.name.get(common)) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(left), None) => left.cmp(&right_terminator),
        (None, Some(right)) => left_terminator.cmp(right),
        (None, None) => left_terminator.cmp(&right_terminator),
    }
}

fn decode_git_object_id(
    value: &str,
    object_format: GitObjectFormat,
) -> Result<Vec<u8>, GitCommitPlanError> {
    if validate_object_id(value) != Some(object_format) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(value.len() / 2)
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    for pair in pairs {
        let high = hex_nibble(pair[0]).ok_or(GitCommitPlanError::UnsafeRepository)?;
        let low = hex_nibble(pair[1]).ok_or(GitCommitPlanError::UnsafeRepository)?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn create_git_commit_candidate_commit(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    staging: &GitCommitStaging,
    object_format: GitObjectFormat,
    tree_object_id: &str,
    parent: Option<&str>,
    identity: &GitCommitIdentity,
    timestamp: &str,
    message: &str,
) -> Result<(String, String), GitCommitPlanError> {
    let mut arguments = vec!["commit-tree", tree_object_id];
    if let Some(parent) = parent {
        arguments.extend(["-p", parent]);
    }
    let mut command =
        git_commit_staging_write_command(executable, sandbox, repository, staging, &arguments);
    command
        .env("GIT_AUTHOR_NAME", &identity.name)
        .env("GIT_AUTHOR_EMAIL", &identity.email)
        .env("GIT_AUTHOR_DATE", timestamp)
        .env("GIT_COMMITTER_NAME", &identity.name)
        .env("GIT_COMMITTER_EMAIL", &identity.email)
        .env("GIT_COMMITTER_DATE", timestamp);
    let mut input = Vec::new();
    input
        .try_reserve_exact(message.len().saturating_add(1))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    input.extend_from_slice(message.as_bytes());
    input.push(b'\n');
    let output = run_bounded_git_command_with_input(
        command,
        COMMIT_PROCESS_TIMEOUT,
        128,
        OUTPUT_LIMIT,
        Some(input),
    )
    .map_err(map_git_commit_read_error)?;
    let commit_object_id = parse_git_commit_object_id_line(&output.stdout, object_format)?;

    let mut read_command = git_commit_staging_read_command(
        executable,
        sandbox,
        repository,
        staging,
        &["cat-file", "commit", &commit_object_id],
    );
    read_command.stdin(Stdio::null());
    let commit = run_bounded_git_command(read_command, COMMIT_PROCESS_TIMEOUT, OUTPUT_LIMIT)
        .map_err(map_git_commit_read_error)?;
    if git_object_id(object_format, "commit", &commit.stdout) != commit_object_id {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let git_timestamp = verify_git_commit_candidate_bytes(
        &commit.stdout,
        tree_object_id,
        parent,
        identity,
        message,
    )?;
    Ok((commit_object_id, git_timestamp))
}

fn verify_git_commit_candidate_bytes(
    bytes: &[u8],
    tree_object_id: &str,
    parent: Option<&str>,
    identity: &GitCommitIdentity,
    message: &str,
) -> Result<String, GitCommitPlanError> {
    let mut cursor = 0usize;
    let tree = take_git_commit_line(bytes, &mut cursor)?;
    if tree != format!("tree {tree_object_id}").as_bytes() {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    if let Some(parent) = parent {
        let line = take_git_commit_line(bytes, &mut cursor)?;
        if line != format!("parent {parent}").as_bytes() {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
    }
    let author = take_git_commit_line(bytes, &mut cursor)?;
    let committer = take_git_commit_line(bytes, &mut cursor)?;
    let separator = take_git_commit_line(bytes, &mut cursor)?;
    if !separator.is_empty() {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let author_prefix = format!("author {} <{}> ", identity.name, identity.email);
    let committer_prefix = format!("committer {} <{}> ", identity.name, identity.email);
    let author_timestamp = author
        .strip_prefix(author_prefix.as_bytes())
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let committer_timestamp = committer
        .strip_prefix(committer_prefix.as_bytes())
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    if author_timestamp != committer_timestamp
        || !valid_git_commit_timestamp(author_timestamp)
        || &bytes[cursor..] != format!("{message}\n").as_bytes()
    {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    std::str::from_utf8(author_timestamp)
        .map(str::to_owned)
        .map_err(|_| GitCommitPlanError::UnsafeRepository)
}

fn take_git_commit_line<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
) -> Result<&'a [u8], GitCommitPlanError> {
    let remaining = bytes
        .get(*cursor..)
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let end = remaining
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    *cursor = cursor
        .checked_add(end.saturating_add(1))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    Ok(&remaining[..end])
}

fn valid_git_commit_timestamp(value: &[u8]) -> bool {
    let Some(separator) = value.iter().position(|byte| *byte == b' ') else {
        return false;
    };
    let seconds = &value[..separator];
    let timezone = &value[separator + 1..];
    !seconds.is_empty()
        && seconds.len() <= 20
        && seconds.iter().all(u8::is_ascii_digit)
        && timezone == b"+0000"
}

fn validate_prior_git_commit_workspace(
    preflight: &GitCommitRepositoryPreflight,
    staging: &GitCommitStaging,
) -> Result<GitCommitManagedInventory, GitCommitPlanError> {
    let paths = prior_managed_candidate_paths(&preflight.tree)?;
    if paths.is_empty() {
        return Ok(GitCommitManagedInventory { files: Vec::new() });
    }
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(paths.len())
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    for path in &paths {
        let index = preflight
            .tree
            .entries
            .binary_search_by(|entry| entry.path.as_slice().cmp(path.as_slice()))
            .map_err(|_| GitCommitPlanError::AmbiguousManagedHistory)?;
        let entry = &preflight.tree.entries[index];
        let text = std::str::from_utf8(&entry.path)
            .map_err(|_| GitCommitPlanError::AmbiguousManagedHistory)?;
        let (_, limit) = classify_git_materialization_path(text)
            .ok_or(GitCommitPlanError::AmbiguousManagedHistory)?;
        if entry.kind != GitCommitTreeEntryKind::Blob
            || entry.mode != 0o100644
            || entry.size.is_none_or(|size| size > limit)
        {
            return Err(GitCommitPlanError::AmbiguousManagedHistory);
        }
        entries.push(entry);
    }

    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCommitPlanError::UnsupportedRepository)?;
    stream_prior_git_commit_blobs(
        git_commit_staging_read_command(
            &executable,
            &sandbox,
            &preflight.capability,
            staging,
            &["cat-file", "--batch"],
        ),
        staging,
        preflight.object_format,
        &entries,
    )?;
    staging
        .finish_validation_tree()
        .map_err(map_git_commit_staging_error)?;
    let inspection = inspect_workspace(Path::new(staging.validation_path()))
        .map_err(map_prior_git_commit_inspection_error)?;
    let (_, classification) = inspection.into_parts();
    let WorkspaceInspection::Compatible(workspace) = classification else {
        return Err(GitCommitPlanError::AmbiguousManagedHistory);
    };
    if !workspace.issues().is_empty()
        || workspace.manifest().workspace_id().to_string() != preflight.workspace_id
    {
        return Err(GitCommitPlanError::AmbiguousManagedHistory);
    }
    let prior = inspect_git_commit_managed_inventory(&workspace, preflight.object_format).map_err(
        |error| match error {
            GitCommitPlanError::ResourceLimit => GitCommitPlanError::ResourceLimit,
            GitCommitPlanError::Io => GitCommitPlanError::Io,
            _ => GitCommitPlanError::AmbiguousManagedHistory,
        },
    )?;
    if prior.files.len() != entries.len()
        || prior.files.iter().zip(entries).any(|(file, entry)| {
            file.path.as_bytes() != entry.path
                || file.size != entry.size.unwrap_or(u64::MAX)
                || file.blob_object_id != entry.object_id
        })
    {
        return Err(GitCommitPlanError::AmbiguousManagedHistory);
    }
    Ok(prior)
}

fn git_commit_staging_read_command(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    staging: &GitCommitStaging,
    arguments: &[&str],
) -> Command {
    let descriptor = staging.git_descriptor();
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--ro-bind",
            staging.git_path(),
            "/repo.git",
            "--ro-bind",
            repository.objects_path(),
            "/active-objects",
            "--tmpfs",
            "/active-objects/info",
            "--chdir",
            "/tmp",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
            "--setenv",
            "GIT_OBJECT_DIRECTORY",
            "/repo.git/objects",
            "--setenv",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "/active-objects",
            "--",
        ])
        .arg(executable)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "credential.helper=",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "diff.external=",
            "-c",
            "protocol.allow=never",
        ])
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    // SAFETY: only the retained staging descriptor has CLOEXEC cleared for the
    // fixed Bubblewrap read-only bind.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(descriptor, libc::F_GETFD);
            if flags < 0 || libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}

fn stream_prior_git_commit_blobs(
    mut command: Command,
    staging: &GitCommitStaging,
    object_format: GitObjectFormat,
    entries: &[&GitCommitTreeEntry],
) -> Result<(), GitCommitPlanError> {
    let mut child = command.spawn().map_err(|_| GitCommitPlanError::Io)?;
    let stdin = child.stdin.take().ok_or(GitCommitPlanError::Io)?;
    let stdout = child.stdout.take().ok_or(GitCommitPlanError::Io)?;
    let stderr = child.stderr.take().ok_or(GitCommitPlanError::Io)?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stderr_reader = read_capped_with_limit(stderr, Arc::clone(&exceeded), OUTPUT_LIMIT);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let started = Instant::now();
    let mut worker_result = None;
    let mut process_status = None;
    let mut terminal_error = None;
    thread::scope(|scope| {
        scope.spawn(move || {
            let result =
                stream_prior_git_commit_blob_bytes(stdin, stdout, staging, object_format, entries);
            let _ = sender.send(result);
        });
        loop {
            if worker_result.is_none() {
                match receiver.try_recv() {
                    Ok(result) => worker_result = Some(result),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        terminal_error = Some(GitCommitPlanError::Io);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
            if process_status.is_none() {
                match child.try_wait() {
                    Ok(Some(status)) => process_status = Some(status),
                    Ok(None) => {}
                    Err(_) => terminal_error = Some(GitCommitPlanError::Io),
                }
            }
            if worker_result.is_some() && process_status.is_some() {
                break;
            }
            if terminal_error.is_none() {
                terminal_error = if exceeded.load(Ordering::Relaxed)
                    || started.elapsed() >= COMMIT_PLAN_TIMEOUT
                    || active_git_commit_deadline_reached()
                {
                    Some(GitCommitPlanError::ResourceLimit)
                } else {
                    None
                };
            }
            if terminal_error.is_some() {
                terminate_child(&mut child);
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    });
    let _ = stderr_reader.join().map_err(|_| GitCommitPlanError::Io)?;
    if let Some(error) = terminal_error {
        return Err(error);
    }
    worker_result.ok_or(GitCommitPlanError::Io)??;
    if process_status.is_some_and(|status| status.success()) {
        Ok(())
    } else {
        Err(GitCommitPlanError::UnsafeRepository)
    }
}

fn stream_prior_git_commit_blob_bytes(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    staging: &GitCommitStaging,
    object_format: GitObjectFormat,
    entries: &[&GitCommitTreeEntry],
) -> Result<(), GitCommitPlanError> {
    let mut writer = BufWriter::new(stdin);
    let mut reader = BufReader::new(stdout);
    for entry in entries {
        writer
            .write_all(entry.object_id.as_bytes())
            .and_then(|_| writer.write_all(b"\n"))
            .and_then(|_| writer.flush())
            .map_err(|_| GitCommitPlanError::Io)?;
        let header = read_git_commit_batch_header(&mut reader)?;
        let size = entry
            .size
            .and_then(|size| usize::try_from(size).ok())
            .ok_or(GitCommitPlanError::ResourceLimit)?;
        let expected = format!("{} blob {size}\n", entry.object_id);
        if header != expected.as_bytes() {
            return Err(GitCommitPlanError::AmbiguousManagedHistory);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| GitCommitPlanError::ResourceLimit)?;
        bytes.resize(size, 0);
        reader
            .read_exact(&mut bytes)
            .map_err(|_| GitCommitPlanError::AmbiguousManagedHistory)?;
        let mut terminator = [0_u8; 1];
        reader
            .read_exact(&mut terminator)
            .map_err(|_| GitCommitPlanError::AmbiguousManagedHistory)?;
        if terminator != *b"\n"
            || !git_blob_object_id_matches(object_format, &bytes, &entry.object_id)
        {
            return Err(GitCommitPlanError::AmbiguousManagedHistory);
        }
        let path = std::str::from_utf8(&entry.path)
            .map_err(|_| GitCommitPlanError::AmbiguousManagedHistory)?;
        staging
            .write_validation_file(path, &bytes)
            .map_err(map_git_commit_staging_error)?;
    }
    drop(writer);
    let mut unexpected = [0_u8; 1];
    if reader
        .read(&mut unexpected)
        .map_err(|_| GitCommitPlanError::Io)?
        != 0
    {
        return Err(GitCommitPlanError::AmbiguousManagedHistory);
    }
    Ok(())
}

fn read_git_commit_batch_header(reader: &mut impl Read) -> Result<Vec<u8>, GitCommitPlanError> {
    let mut header = Vec::with_capacity(96);
    while header.len() < CLONE_BATCH_HEADER_LIMIT {
        let mut byte = [0_u8; 1];
        reader
            .read_exact(&mut byte)
            .map_err(|_| GitCommitPlanError::AmbiguousManagedHistory)?;
        header.push(byte[0]);
        if byte[0] == b'\n' {
            return Ok(header);
        }
    }
    Err(GitCommitPlanError::AmbiguousManagedHistory)
}

fn verify_git_commit_pack(
    executable: &Path,
    sandbox: &Path,
    staging: &GitCommitStaging,
    object_format: GitObjectFormat,
    object_ids: &[String],
    sources: &[GitClonePackSource],
    pack_hash: &str,
) -> Result<(), GitCommitPlanError> {
    if sources.len() != 2 || validate_object_id(pack_hash) != Some(object_format) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let expected_stem = format!("pack-{pack_hash}");
    let pack_source = sources
        .iter()
        .find(|source| source.name == format!("{expected_stem}.pack"))
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let index_source = sources
        .iter()
        .find(|source| source.name == format!("{expected_stem}.idx"))
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let hash_len = match object_format {
        GitObjectFormat::Sha1 => 20,
        GitObjectFormat::Sha256 => 32,
    };
    if index_source.size > MAX_COMMIT_INDEX_BYTES as u64
        || pack_source.size < (12 + hash_len) as u64
    {
        return Err(GitCommitPlanError::ResourceLimit);
    }
    let mut index = staging
        .open_pack_source(index_source)
        .map_err(map_git_commit_staging_error)?;
    let index_len =
        usize::try_from(index_source.size).map_err(|_| GitCommitPlanError::ResourceLimit)?;
    let mut index_bytes = Vec::new();
    index_bytes
        .try_reserve_exact(index_len)
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    index
        .read_to_end(&mut index_bytes)
        .map_err(|_| GitCommitPlanError::Io)?;
    if index_bytes.len() != index_len {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let pack_checksum = verify_git_commit_pack_index(
        &index_bytes,
        object_format,
        object_ids,
        pack_source.size,
        pack_hash,
    )?;
    verify_git_commit_pack_file(
        staging,
        pack_source,
        object_format,
        object_ids.len(),
        &pack_checksum,
    )?;

    let expectations =
        inspect_git_commit_pack_expectations(executable, sandbox, staging, object_ids)?;
    let command = git_commit_pack_only_read_command(executable, sandbox, staging, true);
    stream_and_verify_git_commit_objects(command, object_format, expectations)
}

fn verify_git_commit_pack_index(
    bytes: &[u8],
    object_format: GitObjectFormat,
    object_ids: &[String],
    pack_size: u64,
    pack_hash: &str,
) -> Result<Vec<u8>, GitCommitPlanError> {
    let hash_len = match object_format {
        GitObjectFormat::Sha1 => 20,
        GitObjectFormat::Sha256 => 32,
    };
    if bytes.len() < 8 + 256 * 4 + 2 * hash_len
        || bytes.get(..4) != Some(&[0xff, b't', b'O', b'c'])
        || read_be_u32(bytes, 4)? != 2
    {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let fanout_start = 8usize;
    let mut previous = 0u32;
    for index in 0..256 {
        let value = read_be_u32(bytes, fanout_start + index * 4)?;
        if value < previous {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        previous = value;
    }
    let count = usize::try_from(previous).map_err(|_| GitCommitPlanError::ResourceLimit)?;
    if count != object_ids.len() || count > MAX_COMMIT_STAGING_OBJECTS {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let names_start = fanout_start + 256 * 4;
    let names_end = names_start
        .checked_add(count.saturating_mul(hash_len))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    let crc_end = names_end
        .checked_add(count.saturating_mul(4))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    let offsets_end = crc_end
        .checked_add(count.saturating_mul(4))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    if offsets_end > bytes.len() {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    for (index, object_id) in object_ids.iter().enumerate() {
        let start = names_start + index * hash_len;
        let end = start + hash_len;
        if bytes.get(start..end) != Some(decode_git_object_id(object_id, object_format)?.as_slice())
        {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
    }
    let mut large_indexes = std::collections::BTreeSet::new();
    for index in 0..count {
        let value = read_be_u32(bytes, crc_end + index * 4)?;
        if value & 0x8000_0000 != 0 {
            large_indexes.insert((value & 0x7fff_ffff) as usize);
        } else if value < 12 || u64::from(value) >= pack_size.saturating_sub(hash_len as u64) {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
    }
    if large_indexes.iter().copied().ne(0..large_indexes.len()) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let large_end = offsets_end
        .checked_add(large_indexes.len().saturating_mul(8))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    let expected_len = large_end
        .checked_add(2 * hash_len)
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    if bytes.len() != expected_len {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    for index in 0..large_indexes.len() {
        let start = offsets_end + index * 8;
        let value = u64::from_be_bytes(
            bytes[start..start + 8]
                .try_into()
                .map_err(|_| GitCommitPlanError::UnsafeRepository)?,
        );
        if value < 12 || value >= pack_size.saturating_sub(hash_len as u64) {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
    }
    let pack_checksum = bytes[large_end..large_end + hash_len].to_vec();
    if hex_lower(&pack_checksum) != pack_hash
        || git_index_checksum(&bytes[..bytes.len() - hash_len], object_format).as_slice()
            != &bytes[bytes.len() - hash_len..]
    {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    Ok(pack_checksum)
}

fn verify_git_commit_pack_file(
    staging: &GitCommitStaging,
    source: &GitClonePackSource,
    object_format: GitObjectFormat,
    expected_count: usize,
    expected_checksum: &[u8],
) -> Result<(), GitCommitPlanError> {
    use sha1::Digest as _;
    let hash_len = expected_checksum.len();
    let body_len = source
        .size
        .checked_sub(hash_len as u64)
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let mut file = staging
        .open_pack_source(source)
        .map_err(map_git_commit_staging_error)?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header)
        .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
    let version = read_be_u32(&header, 4)?;
    let count =
        usize::try_from(read_be_u32(&header, 8)?).map_err(|_| GitCommitPlanError::ResourceLimit)?;
    if &header[..4] != b"PACK"
        || !matches!(version, 2 | 3)
        || count != expected_count
        || count > MAX_COMMIT_STAGING_OBJECTS
    {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut sha1 = (object_format == GitObjectFormat::Sha1).then(sha1::Sha1::new);
    let mut sha256 = (object_format == GitObjectFormat::Sha256).then(sha2::Sha256::new);
    if let Some(digest) = &mut sha1 {
        digest.update(header);
    }
    if let Some(digest) = &mut sha256 {
        digest.update(header);
    }
    let mut remaining = body_len
        .checked_sub(header.len() as u64)
        .ok_or(GitCommitPlanError::UnsafeRepository)?;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        if active_git_commit_deadline_reached() {
            return Err(GitCommitPlanError::ResourceLimit);
        }
        let amount = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| GitCommitPlanError::ResourceLimit)?;
        file.read_exact(&mut buffer[..amount])
            .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
        if let Some(digest) = &mut sha1 {
            digest.update(&buffer[..amount]);
        }
        if let Some(digest) = &mut sha256 {
            digest.update(&buffer[..amount]);
        }
        remaining -= amount as u64;
    }
    let actual = match (sha1, sha256) {
        (Some(digest), None) => digest.finalize().to_vec(),
        (None, Some(digest)) => digest.finalize().to_vec(),
        _ => return Err(GitCommitPlanError::UnsafeRepository),
    };
    let mut trailer = vec![0u8; hash_len];
    file.read_exact(&mut trailer)
        .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
    let mut unexpected = [0u8; 1];
    if file
        .read(&mut unexpected)
        .map_err(|_| GitCommitPlanError::Io)?
        != 0
        || actual != trailer
        || trailer != expected_checksum
    {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    Ok(())
}

fn inspect_git_commit_pack_expectations(
    executable: &Path,
    sandbox: &Path,
    staging: &GitCommitStaging,
    object_ids: &[String],
) -> Result<Vec<GitCommitObjectExpectation>, GitCommitPlanError> {
    let mut input = Vec::new();
    for object_id in object_ids {
        input.extend_from_slice(object_id.as_bytes());
        input.push(b'\n');
    }
    let command = git_commit_pack_only_read_command(executable, sandbox, staging, false);
    let output = run_bounded_git_command_with_input(
        command,
        COMMIT_PROCESS_TIMEOUT,
        MAX_COMMIT_TREE_BYTES,
        OUTPUT_LIMIT,
        Some(input),
    )
    .map_err(map_git_commit_read_error)?;
    let text =
        std::str::from_utf8(&output.stdout).map_err(|_| GitCommitPlanError::UnsafeRepository)?;
    let mut lines = text.lines();
    let mut expectations = Vec::new();
    for object_id in object_ids {
        let mut fields = lines
            .next()
            .ok_or(GitCommitPlanError::UnsafeRepository)?
            .split_ascii_whitespace();
        let actual_id = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let object_type = match fields.next() {
            Some("blob") => "blob",
            Some("tree") => "tree",
            Some("commit") => "commit",
            _ => return Err(GitCommitPlanError::UnsafeRepository),
        };
        let size = fields
            .next()
            .and_then(parse_canonical_u64)
            .ok_or(GitCommitPlanError::UnsafeRepository)?;
        if fields.next().is_some() || actual_id != object_id {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        expectations.push(GitCommitObjectExpectation {
            object_id: object_id.clone(),
            object_type,
            size: Some(size),
        });
    }
    if lines.next().is_some() {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    Ok(expectations)
}

fn git_commit_pack_only_read_command(
    executable: &Path,
    sandbox: &Path,
    staging: &GitCommitStaging,
    contents: bool,
) -> Command {
    let descriptor = staging.git_descriptor();
    let pack_path = format!("{}/objects/pack", staging.git_path());
    let config_path = format!("{}/config", staging.git_path());
    let head_path = format!("{}/HEAD", staging.git_path());
    let refs_path = format!("{}/refs", staging.git_path());
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/repo.git",
            "--dir",
            "/repo.git/objects",
            "--ro-bind",
            &pack_path,
            "/repo.git/objects/pack",
            "--dir",
            "/repo.git/objects/info",
            "--ro-bind",
            &config_path,
            "/repo.git/config",
            "--ro-bind",
            &head_path,
            "/repo.git/HEAD",
            "--ro-bind",
            &refs_path,
            "/repo.git/refs",
            "--tmpfs",
            "/tmp",
            "--chdir",
            "/tmp",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
            "--",
        ])
        .arg(executable)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.allow=never",
        ])
        .arg("cat-file")
        .arg(if contents {
            "--batch"
        } else {
            "--batch-check=%(objectname) %(objecttype) %(objectsize)"
        })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    // SAFETY: only the retained core-created staging descriptor is exposed to
    // Bubblewrap for fixed pack/config/HEAD/ref paths.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(descriptor, libc::F_GETFD);
            if flags < 0 || libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}

fn map_prior_git_commit_inspection_error(
    error: crate::storage::WorkspaceInspectError,
) -> GitCommitPlanError {
    match error {
        crate::storage::WorkspaceInspectError::ResourceLimit => GitCommitPlanError::ResourceLimit,
        crate::storage::WorkspaceInspectError::Io => GitCommitPlanError::Io,
        crate::storage::WorkspaceInspectError::ChangedDuringRead
        | crate::storage::WorkspaceInspectError::UnsafeRoot
        | crate::storage::WorkspaceInspectError::NotFound
        | crate::storage::WorkspaceInspectError::NotDirectory
        | crate::storage::WorkspaceInspectError::PermissionDenied => {
            GitCommitPlanError::AmbiguousManagedHistory
        }
    }
}

fn map_git_commit_staging_error(error: GitCommitStagingError) -> GitCommitPlanError {
    match error {
        GitCommitStagingError::Conflict => GitCommitPlanError::OperationInProgress,
        GitCommitStagingError::Unsafe => GitCommitPlanError::UnsafeRepository,
        GitCommitStagingError::Unsupported => GitCommitPlanError::UnsupportedRepository,
        GitCommitStagingError::TooLarge => GitCommitPlanError::ResourceLimit,
        GitCommitStagingError::Io => GitCommitPlanError::Io,
    }
}

fn verify_git_commit_tree_objects(
    executable: &Path,
    sandbox: &Path,
    capability: &crate::storage::GitRepositoryCapability,
    object_format: GitObjectFormat,
    parent: &str,
    root_tree: &str,
    tree: &GitCommitTree,
) -> Result<(), GitCommitPlanError> {
    let mut expected = Vec::new();
    expected
        .try_reserve_exact(tree.entries.len().saturating_add(2))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    expected.push(GitCommitObjectExpectation {
        object_id: parent.to_owned(),
        object_type: "commit",
        size: None,
    });
    expected.push(GitCommitObjectExpectation {
        object_id: root_tree.to_owned(),
        object_type: "tree",
        size: None,
    });
    for entry in &tree.entries {
        let object_type = match entry.kind {
            GitCommitTreeEntryKind::Tree => "tree",
            GitCommitTreeEntryKind::Blob => "blob",
            GitCommitTreeEntryKind::Commit => "commit",
        };
        expected.push(GitCommitObjectExpectation {
            object_id: entry.object_id.clone(),
            object_type,
            size: entry.size,
        });
    }
    let (command, _config) =
        git_commit_object_stream_command(executable, sandbox, capability, object_format)?;
    stream_and_verify_git_commit_objects(command, object_format, expected)
}

struct GitCommitObjectExpectation {
    object_id: String,
    object_type: &'static str,
    size: Option<u64>,
}

fn git_commit_object_stream_command(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    object_format: GitObjectFormat,
) -> Result<(Command, File), GitCommitPlanError> {
    let config = create_git_format_config(object_format).map_err(map_git_commit_read_error)?;
    let config_descriptor = config.as_raw_fd();
    let config_descriptor_text = config_descriptor.to_string();
    let mut command = Command::new(sandbox);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_EDITOR", "false")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/repo.git",
            "--ro-bind",
            repository.objects_path(),
            "/repo.git/objects",
            "--ro-bind",
            repository.refs_path(),
            "/repo.git/refs",
            "--ro-bind",
            repository.head_path(),
            "/repo.git/HEAD",
            "--file",
            &config_descriptor_text,
            "/repo.git/config",
            "--tmpfs",
            "/repo.git/objects/info",
            "--tmpfs",
            "/tmp",
            "--chdir",
            "/tmp",
            "--setenv",
            "GIT_DIR",
            "/repo.git",
        ]);
    for (source, destination) in [
        (repository.packed_refs_path(), "/repo.git/packed-refs"),
        (repository.shallow_path(), "/repo.git/shallow"),
    ] {
        if let Some(source) = source {
            command.args(["--ro-bind", source, destination]);
        }
    }
    command
        .args(["--"])
        .arg(executable)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.alternateRefsCommand=",
            "-c",
            "protocol.allow=never",
            "cat-file",
            "--batch",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    // SAFETY: only the sealed synthetic-config descriptor is exposed to the
    // fixed Bubblewrap file copy.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(config_descriptor, libc::F_GETFD);
            if flags < 0
                || libc::fcntl(config_descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok((command, config))
}

fn stream_and_verify_git_commit_objects(
    mut command: Command,
    object_format: GitObjectFormat,
    expected: Vec<GitCommitObjectExpectation>,
) -> Result<(), GitCommitPlanError> {
    let mut child = command.spawn().map_err(|_| GitCommitPlanError::Io)?;
    let stdin = child.stdin.take().ok_or(GitCommitPlanError::Io)?;
    let stdout = child.stdout.take().ok_or(GitCommitPlanError::Io)?;
    let stderr = child.stderr.take().ok_or(GitCommitPlanError::Io)?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stderr_reader = read_capped_with_limit(stderr, Arc::clone(&exceeded), OUTPUT_LIMIT);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let started = Instant::now();
    let mut worker_result = None;
    let mut process_status = None;
    let mut terminal_error = None;
    thread::scope(|scope| {
        scope.spawn(move || {
            let result =
                stream_and_verify_git_commit_object_bytes(stdin, stdout, object_format, &expected);
            let _ = sender.send(result);
        });
        loop {
            if worker_result.is_none() {
                match receiver.try_recv() {
                    Ok(result) => worker_result = Some(result),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        terminal_error = Some(GitCommitPlanError::Io);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
            if process_status.is_none() {
                match child.try_wait() {
                    Ok(Some(status)) => process_status = Some(status),
                    Ok(None) => {}
                    Err(_) => terminal_error = Some(GitCommitPlanError::Io),
                }
            }
            if worker_result.is_some() && process_status.is_some() {
                break;
            }
            if terminal_error.is_none()
                && (exceeded.load(Ordering::Relaxed)
                    || started.elapsed() >= COMMIT_PROCESS_TIMEOUT
                    || active_git_commit_deadline_reached())
            {
                terminal_error = Some(GitCommitPlanError::ResourceLimit);
            }
            if terminal_error.is_some() {
                terminate_child(&mut child);
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    });
    let _ = stderr_reader.join().map_err(|_| GitCommitPlanError::Io)?;
    if let Some(error) = terminal_error {
        return Err(error);
    }
    worker_result.ok_or(GitCommitPlanError::Io)??;
    if process_status.is_some_and(|status| status.success()) {
        Ok(())
    } else {
        Err(GitCommitPlanError::UnsafeRepository)
    }
}

fn stream_and_verify_git_commit_object_bytes(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    object_format: GitObjectFormat,
    expected: &[GitCommitObjectExpectation],
) -> Result<(), GitCommitPlanError> {
    let mut writer = BufWriter::new(stdin);
    let mut reader = BufReader::new(stdout);
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    for object in expected {
        writer
            .write_all(object.object_id.as_bytes())
            .and_then(|_| writer.write_all(b"\n"))
            .and_then(|_| writer.flush())
            .map_err(|_| GitCommitPlanError::Io)?;
        let header = read_git_commit_batch_header(&mut reader)?;
        let text = std::str::from_utf8(
            header
                .strip_suffix(b"\n")
                .ok_or(GitCommitPlanError::UnsafeRepository)?,
        )
        .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
        let mut fields = text.split_ascii_whitespace();
        let object_id = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let object_type = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let size = fields
            .next()
            .and_then(parse_canonical_u64)
            .ok_or(GitCommitPlanError::UnsafeRepository)?;
        if fields.next().is_some()
            || object_id != object.object_id
            || object_type != object.object_type
            || object.size.is_some_and(|expected| expected != size)
        {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
        total = total
            .checked_add(size)
            .filter(|total| *total <= MAX_COMMIT_STAGING_BYTES)
            .ok_or(GitCommitPlanError::ResourceLimit)?;
        let mut digest = GitObjectDigest::new(object_format, object.object_type, size);
        let mut remaining = size;
        while remaining > 0 {
            if active_git_commit_deadline_reached() {
                return Err(GitCommitPlanError::ResourceLimit);
            }
            let amount = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| GitCommitPlanError::ResourceLimit)?;
            reader
                .read_exact(&mut buffer[..amount])
                .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
            digest.update(&buffer[..amount]);
            remaining -= amount as u64;
        }
        let mut terminator = [0u8; 1];
        reader
            .read_exact(&mut terminator)
            .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
        if terminator != *b"\n" || digest.finish() != object.object_id {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
    }
    drop(writer);
    let mut unexpected = [0u8; 1];
    if reader
        .read(&mut unexpected)
        .map_err(|_| GitCommitPlanError::Io)?
        != 0
    {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    Ok(())
}

enum GitObjectDigest {
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
}

impl GitObjectDigest {
    fn new(format: GitObjectFormat, kind: &str, size: u64) -> Self {
        use sha1::Digest as _;
        let header = format!("{kind} {size}\0");
        match format {
            GitObjectFormat::Sha1 => {
                let mut digest = sha1::Sha1::new();
                digest.update(header.as_bytes());
                Self::Sha1(digest)
            }
            GitObjectFormat::Sha256 => {
                let mut digest = sha2::Sha256::new();
                digest.update(header.as_bytes());
                Self::Sha256(digest)
            }
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        use sha1::Digest as _;
        match self {
            Self::Sha1(digest) => digest.update(bytes),
            Self::Sha256(digest) => digest.update(bytes),
        }
    }

    fn finish(self) -> String {
        use sha1::Digest as _;
        match self {
            Self::Sha1(digest) => hex_lower(&digest.finalize()),
            Self::Sha256(digest) => hex_lower(&digest.finalize()),
        }
    }
}

fn inspect_git_commit_managed_inventory(
    workspace: &WorkspaceView,
    object_format: GitObjectFormat,
) -> Result<GitCommitManagedInventory, GitCommitPlanError> {
    if !workspace.issues().is_empty() {
        return Err(GitCommitPlanError::WorkspaceIssues);
    }
    let (guide_count, attachment_count) = workspace.categories().iter().try_fold(
        (0usize, 0usize),
        |(guides, attachments), category| {
            let guides = guides
                .checked_add(category.guides().len())
                .ok_or(GitCommitPlanError::ResourceLimit)?;
            let attachments = category
                .guides()
                .iter()
                .try_fold(attachments, |count, guide| {
                    count
                        .checked_add(
                            guide
                                .guide()
                                .attachments()
                                .map_or(0, |attachments| attachments.len()),
                        )
                        .ok_or(GitCommitPlanError::ResourceLimit)
                })?;
            Ok::<_, GitCommitPlanError>((guides, attachments))
        },
    )?;
    let mut expected_count = 1usize
        .checked_add(workspace.categories().len())
        .and_then(|count| count.checked_add(guide_count))
        .and_then(|count| count.checked_add(attachment_count))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    let mut expected = Vec::new();
    expected
        .try_reserve_exact(expected_count.saturating_add(1))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    expected.push((
        "rehow.yaml".to_owned(),
        GitCommitManagedKind::Manifest,
        GitCommitManagedExpectation::Document(workspace.manifest_revision()),
    ));
    if let Some(snapshot) = workspace
        .read_optional_git_commit_managed_file(".gitignore", 64 * 1024)
        .map_err(map_git_commit_workspace_read_error)?
    {
        expected_count = expected_count
            .checked_add(1)
            .ok_or(GitCommitPlanError::ResourceLimit)?;
        expected.push((
            ".gitignore".to_owned(),
            GitCommitManagedKind::GitIgnore,
            GitCommitManagedExpectation::Document(snapshot.revision),
        ));
    }
    for category in workspace.categories() {
        let descriptor = format!("{}/category.yaml", category.relative_directory());
        expected.push((
            descriptor,
            GitCommitManagedKind::Category,
            GitCommitManagedExpectation::Document(category.descriptor_revision()),
        ));
        for guide in category.guides() {
            expected.push((
                guide.relative_path().to_owned(),
                GitCommitManagedKind::Guide,
                GitCommitManagedExpectation::Document(guide.revision()),
            ));
            for attachment in guide.guide().attachments().unwrap_or_default() {
                expected.push((
                    attachment.path().to_owned(),
                    GitCommitManagedKind::Attachment,
                    GitCommitManagedExpectation::Attachment {
                        sha256: attachment.sha256().to_owned(),
                        declared_size: attachment.size_bytes(),
                    },
                ));
            }
        }
    }
    if expected.len() != expected_count
        || expected.len()
            > 2usize
                .saturating_add(CATEGORY_LIMIT)
                .saturating_add(GUIDE_COUNT_LIMIT)
                .saturating_add(ATTACHMENT_COUNT_LIMIT)
    {
        return Err(GitCommitPlanError::ResourceLimit);
    }
    expected.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    if expected
        .windows(2)
        .any(|pair| pair[0].0.as_bytes() >= pair[1].0.as_bytes())
    {
        return Err(GitCommitPlanError::WorkspaceIssues);
    }

    let mut files = Vec::new();
    files
        .try_reserve_exact(expected.len())
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    let mut descriptor_bytes = 0u64;
    let mut attachment_bytes = 0u64;
    for (path, kind, expectation) in expected {
        if active_git_commit_deadline_reached() {
            return Err(GitCommitPlanError::ResourceLimit);
        }
        let (classified, limit) =
            classify_git_materialization_path(&path).ok_or(GitCommitPlanError::WorkspaceIssues)?;
        let expected_class = match kind {
            GitCommitManagedKind::Manifest => GitMaterializationPathKind::Manifest,
            GitCommitManagedKind::GitIgnore => GitMaterializationPathKind::Ignore,
            GitCommitManagedKind::Category => GitMaterializationPathKind::Category,
            GitCommitManagedKind::Guide => GitMaterializationPathKind::Guide,
            GitCommitManagedKind::Attachment => GitMaterializationPathKind::Attachment,
        };
        if classified != expected_class {
            return Err(GitCommitPlanError::WorkspaceIssues);
        }
        let snapshot = workspace
            .read_git_commit_managed_file(&path, limit)
            .map_err(map_git_commit_workspace_read_error)?;
        if snapshot.mode & 0o111 != 0 {
            return Err(GitCommitPlanError::WorkspaceIssues);
        }
        let size =
            u64::try_from(snapshot.bytes.len()).map_err(|_| GitCommitPlanError::ResourceLimit)?;
        match expectation {
            GitCommitManagedExpectation::Document(expected_revision) => {
                if snapshot.revision != expected_revision {
                    return Err(GitCommitPlanError::PlanStale);
                }
                descriptor_bytes = descriptor_bytes
                    .checked_add(size)
                    .filter(|total| *total <= MANAGED_BYTES_LIMIT)
                    .ok_or(GitCommitPlanError::ResourceLimit)?;
            }
            GitCommitManagedExpectation::Attachment {
                sha256,
                declared_size,
            } => {
                use sha2::Digest as _;
                if declared_size.is_some_and(|declared| declared != size)
                    || hex_lower(&sha2::Sha256::digest(&snapshot.bytes)) != sha256
                {
                    return Err(GitCommitPlanError::PlanStale);
                }
                attachment_bytes = attachment_bytes
                    .checked_add(size)
                    .filter(|total| *total <= ATTACHMENT_BYTES_LIMIT)
                    .ok_or(GitCommitPlanError::ResourceLimit)?;
            }
        }
        files.push(GitCommitManagedFile {
            path,
            kind,
            size,
            revision: snapshot.revision,
            mode: snapshot.mode,
            blob_object_id: git_blob_object_id(object_format, &snapshot.bytes),
        });
    }
    Ok(GitCommitManagedInventory { files })
}

fn map_git_commit_workspace_read_error(error: GitCommitSnapshotError) -> GitCommitPlanError {
    match error {
        GitCommitSnapshotError::Conflict => GitCommitPlanError::PlanStale,
        GitCommitSnapshotError::Unsafe | GitCommitSnapshotError::OperationInProgress => {
            GitCommitPlanError::WorkspaceIssues
        }
        GitCommitSnapshotError::TooLarge => GitCommitPlanError::ResourceLimit,
        GitCommitSnapshotError::Io => GitCommitPlanError::Io,
    }
}

fn map_git_commit_snapshot_error(error: GitCommitSnapshotError) -> GitCommitPlanError {
    match error {
        GitCommitSnapshotError::OperationInProgress => GitCommitPlanError::OperationInProgress,
        GitCommitSnapshotError::Conflict => GitCommitPlanError::PlanStale,
        GitCommitSnapshotError::Unsafe => GitCommitPlanError::UnsafeRepository,
        GitCommitSnapshotError::TooLarge => GitCommitPlanError::ResourceLimit,
        GitCommitSnapshotError::Io => GitCommitPlanError::Io,
    }
}

fn map_git_commit_read_error(error: GitReadError) -> GitCommitPlanError {
    match error {
        GitReadError::Timeout | GitReadError::OutputLimit => GitCommitPlanError::ResourceLimit,
        GitReadError::ExitStatus | GitReadError::InvalidOutput => {
            GitCommitPlanError::UnsafeRepository
        }
        GitReadError::Spawn | GitReadError::Signal => GitCommitPlanError::Io,
    }
}

fn map_git_initialization_to_sync(error: GitInitializeStorageError) -> GitSyncError {
    match error {
        GitInitializeStorageError::Conflict => GitSyncError::PlanStale,
        GitInitializeStorageError::Unsafe => GitSyncError::Unsafe,
        GitInitializeStorageError::Unsupported => GitSyncError::Unsupported,
        GitInitializeStorageError::Io => GitSyncError::Io,
    }
}

fn map_git_capability_to_sync(error: GitRepositoryCapabilityError) -> GitSyncError {
    match error {
        GitRepositoryCapabilityError::NotRepository => GitSyncError::NotRepository,
        GitRepositoryCapabilityError::Unsafe => GitSyncError::Unsafe,
        GitRepositoryCapabilityError::Io => GitSyncError::Io,
    }
}

fn map_git_snapshot_to_sync(error: GitCommitSnapshotError) -> GitSyncError {
    match error {
        GitCommitSnapshotError::OperationInProgress => GitSyncError::RecoveryRequired,
        GitCommitSnapshotError::Conflict => GitSyncError::PlanStale,
        GitCommitSnapshotError::Unsafe => GitSyncError::Unsafe,
        GitCommitSnapshotError::TooLarge => GitSyncError::ResourceLimit,
        GitCommitSnapshotError::Io => GitSyncError::Io,
    }
}

fn map_git_read_to_sync(error: GitReadError) -> GitSyncError {
    match error {
        GitReadError::Spawn | GitReadError::Signal => GitSyncError::Io,
        GitReadError::Timeout => GitSyncError::Timeout,
        GitReadError::OutputLimit => GitSyncError::ResourceLimit,
        GitReadError::ExitStatus | GitReadError::InvalidOutput => {
            GitSyncError::InvalidConfiguration
        }
    }
}

fn map_commit_plan_to_sync(error: GitCommitPlanError) -> GitSyncError {
    match error {
        GitCommitPlanError::WorkspaceIssues => GitSyncError::WorkspaceIssues,
        GitCommitPlanError::ReadOnly => GitSyncError::ReadOnly,
        GitCommitPlanError::NotRepository => GitSyncError::NotRepository,
        GitCommitPlanError::OperationInProgress => GitSyncError::RecoveryRequired,
        GitCommitPlanError::ResourceLimit => GitSyncError::ResourceLimit,
        GitCommitPlanError::PlanStale => GitSyncError::PlanStale,
        GitCommitPlanError::Cancelled | GitCommitPlanError::Io => GitSyncError::Io,
        GitCommitPlanError::UnsafeRepository => GitSyncError::Unsafe,
        GitCommitPlanError::UnsupportedRepository
        | GitCommitPlanError::UnsupportedIndex
        | GitCommitPlanError::AmbiguousManagedHistory
        | GitCommitPlanError::PathCollision => GitSyncError::Unsupported,
        GitCommitPlanError::InvalidMessage
        | GitCommitPlanError::InvalidIdentity
        | GitCommitPlanError::NothingToCommit
        | GitCommitPlanError::RecoveryNotRequired => GitSyncError::InvalidConfiguration,
    }
}

fn map_clone_discovery_to_sync(error: GitCloneDiscoveryError) -> GitSyncError {
    match error {
        GitCloneDiscoveryError::UnsupportedTransport => GitSyncError::UnsupportedTransport,
        GitCloneDiscoveryError::GitUnavailable => GitSyncError::GitUnavailable,
        GitCloneDiscoveryError::SandboxUnavailable => GitSyncError::SandboxUnavailable,
        GitCloneDiscoveryError::Cancelled => GitSyncError::Cancelled,
        GitCloneDiscoveryError::Timeout => GitSyncError::Timeout,
        GitCloneDiscoveryError::OutputLimit => GitSyncError::ResourceLimit,
        GitCloneDiscoveryError::RemoteUnavailable => GitSyncError::RemoteUnavailable,
        GitCloneDiscoveryError::RemoteRefMissing => GitSyncError::RemoteChanged,
        GitCloneDiscoveryError::AuthenticationFailed => GitSyncError::AuthenticationFailed,
        GitCloneDiscoveryError::HostKeyRejected => GitSyncError::HostKeyRejected,
        GitCloneDiscoveryError::InvalidAdvertisement => GitSyncError::InvalidRepository,
        GitCloneDiscoveryError::Io => GitSyncError::Io,
    }
}

fn map_clone_inspection_to_sync(error: GitCloneInspectError) -> GitSyncError {
    match error {
        GitCloneInspectError::Cancelled => GitSyncError::Cancelled,
        GitCloneInspectError::Timeout => GitSyncError::Timeout,
        GitCloneInspectError::OutputLimit => GitSyncError::ResourceLimit,
        GitCloneInspectError::GitUnavailable => GitSyncError::GitUnavailable,
        GitCloneInspectError::SandboxUnavailable => GitSyncError::SandboxUnavailable,
        GitCloneInspectError::InvalidRepository
        | GitCloneInspectError::RefMismatch
        | GitCloneInspectError::Tree(_) => GitSyncError::InvalidRepository,
        GitCloneInspectError::Io => GitSyncError::Io,
    }
}

fn map_clone_content_to_sync(error: GitCloneContentError) -> GitSyncError {
    match error {
        GitCloneContentError::Cancelled => GitSyncError::Cancelled,
        GitCloneContentError::Timeout => GitSyncError::Timeout,
        GitCloneContentError::OutputLimit | GitCloneContentError::ResourceLimit => {
            GitSyncError::ResourceLimit
        }
        GitCloneContentError::GitUnavailable => GitSyncError::GitUnavailable,
        GitCloneContentError::SandboxUnavailable => GitSyncError::SandboxUnavailable,
        GitCloneContentError::InvalidRepository | GitCloneContentError::BlobMismatch => {
            GitSyncError::InvalidRepository
        }
        GitCloneContentError::InvalidWorkspace | GitCloneContentError::WorkspaceIssues => {
            GitSyncError::InvalidWorkspace
        }
        GitCloneContentError::Conflict => GitSyncError::StagingConflict,
        GitCloneContentError::Unsafe => GitSyncError::Unsafe,
        GitCloneContentError::Unsupported => GitSyncError::Unsupported,
        GitCloneContentError::Io => GitSyncError::Io,
    }
}

fn map_clone_staging_to_sync(error: GitCloneStagingError) -> GitSyncError {
    match error {
        GitCloneStagingError::TooLarge => GitSyncError::ResourceLimit,
        GitCloneStagingError::Conflict => GitSyncError::StagingConflict,
        GitCloneStagingError::Unsafe => GitSyncError::Unsafe,
        GitCloneStagingError::Unsupported => GitSyncError::Unsupported,
        GitCloneStagingError::Io => GitSyncError::Io,
    }
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
fn map_clone_materialize_to_sync(error: GitCloneMaterializeError) -> GitSyncError {
    match error {
        GitCloneMaterializeError::DestinationChanged
        | GitCloneMaterializeError::StagingChanged
        | GitCloneMaterializeError::Conflict => GitSyncError::PlanStale,
        GitCloneMaterializeError::Busy => GitSyncError::RecoveryRequired,
        GitCloneMaterializeError::Unsafe => GitSyncError::Unsafe,
        GitCloneMaterializeError::Unsupported => GitSyncError::Unsupported,
        GitCloneMaterializeError::ResourceLimit => GitSyncError::ResourceLimit,
        GitCloneMaterializeError::ValidationMismatch => GitSyncError::InvalidRepository,
        GitCloneMaterializeError::Io => GitSyncError::Io,
    }
}

#[allow(
    dead_code,
    reason = "used by the staged sync materialization primitive"
)]
fn map_sync_staging_to_materialization(
    error: GitCloneStagingError,
) -> GitSyncMaterializationStorageError {
    match error {
        GitCloneStagingError::Conflict => GitSyncMaterializationStorageError::Conflict,
        GitCloneStagingError::Unsafe => GitSyncMaterializationStorageError::Unsafe,
        GitCloneStagingError::Unsupported => GitSyncMaterializationStorageError::Unsupported,
        GitCloneStagingError::TooLarge => GitSyncMaterializationStorageError::TooLarge,
        GitCloneStagingError::Io => GitSyncMaterializationStorageError::Io,
    }
}

#[allow(
    dead_code,
    reason = "used by the staged sync materialization primitive"
)]
fn map_sync_materialization_to_sync(error: GitSyncMaterializationStorageError) -> GitSyncError {
    match error {
        GitSyncMaterializationStorageError::SourceChanged
        | GitSyncMaterializationStorageError::Conflict => GitSyncError::PlanStale,
        GitSyncMaterializationStorageError::Busy => GitSyncError::RecoveryRequired,
        GitSyncMaterializationStorageError::Unsafe => GitSyncError::Unsafe,
        GitSyncMaterializationStorageError::Unsupported => GitSyncError::Unsupported,
        GitSyncMaterializationStorageError::TooLarge => GitSyncError::ResourceLimit,
        GitSyncMaterializationStorageError::Io => GitSyncError::Io,
    }
}

fn parse_git_object_format(records: &[u8]) -> Result<GitObjectFormat, GitCommitPlanError> {
    if !records.is_empty() && !records.ends_with(b"\0") {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut repository_version = None;
    let mut object_format = None;
    for record in records
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let separator = record
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(GitCommitPlanError::UnsafeRepository)?;
        let key = std::str::from_utf8(&record[..separator])
            .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
        let value = &record[separator + 1..];
        match key {
            "core.repositoryformatversion" => {
                set_single_config_value(&mut repository_version, value)?;
            }
            "extensions.objectformat" => set_single_config_value(&mut object_format, value)?,
            key if key.starts_with("extensions.") => {
                return Err(GitCommitPlanError::UnsupportedRepository);
            }
            _ => {}
        }
    }
    match (repository_version.as_deref(), object_format.as_deref()) {
        (Some(b"0"), None) => Ok(GitObjectFormat::Sha1),
        (Some(b"1"), Some(b"sha256")) => Ok(GitObjectFormat::Sha256),
        _ => Err(GitCommitPlanError::UnsupportedRepository),
    }
}

fn parse_git_commit_config(
    records: &[u8],
) -> Result<(GitObjectFormat, GitCommitIdentity), GitCommitPlanError> {
    if !records.is_empty() && !records.ends_with(b"\0") {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut repository_version = None;
    let mut object_format = None;
    let mut name = None;
    let mut email = None;
    for record in records
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let separator = record
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(GitCommitPlanError::UnsafeRepository)?;
        let key = std::str::from_utf8(&record[..separator])
            .map_err(|_| GitCommitPlanError::UnsafeRepository)?;
        let value = &record[separator + 1..];
        match key {
            "core.repositoryformatversion" => {
                set_single_config_value(&mut repository_version, value)?;
            }
            "extensions.objectformat" => set_single_config_value(&mut object_format, value)?,
            "user.name" => set_single_config_value(&mut name, value)?,
            "user.email" => set_single_config_value(&mut email, value)?,
            key if key.starts_with("extensions.") => {
                return Err(GitCommitPlanError::UnsupportedRepository);
            }
            _ => {}
        }
    }
    let format = match (repository_version.as_deref(), object_format.as_deref()) {
        (Some(b"0"), None) => GitObjectFormat::Sha1,
        (Some(b"1"), Some(b"sha256")) => GitObjectFormat::Sha256,
        _ => return Err(GitCommitPlanError::UnsupportedRepository),
    };
    let name = std::str::from_utf8(name.as_deref().ok_or(GitCommitPlanError::InvalidIdentity)?)
        .map_err(|_| GitCommitPlanError::InvalidIdentity)?;
    let email = std::str::from_utf8(
        email
            .as_deref()
            .ok_or(GitCommitPlanError::InvalidIdentity)?,
    )
    .map_err(|_| GitCommitPlanError::InvalidIdentity)?;
    Ok((format, validate_git_commit_identity(name, email)?))
}

fn set_single_config_value(
    slot: &mut Option<Vec<u8>>,
    value: &[u8],
) -> Result<(), GitCommitPlanError> {
    if slot.is_some() {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut retained = Vec::new();
    retained
        .try_reserve_exact(value.len())
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    retained.extend_from_slice(value);
    *slot = Some(retained);
    Ok(())
}

#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
fn normalize_git_commit_message(value: &str) -> Result<String, GitCommitPlanError> {
    let value = value.trim();
    let scalar_count = value.chars().count();
    if scalar_count == 0
        || scalar_count > MAX_COMMIT_MESSAGE_SCALARS
        || value.chars().any(unsafe_commit_text_scalar)
    {
        return Err(GitCommitPlanError::InvalidMessage);
    }
    let mut normalized = String::new();
    normalized
        .try_reserve_exact(value.len())
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    normalized.push_str(value);
    Ok(normalized)
}

#[allow(dead_code, reason = "consumed by the Phase 5C1 repository preflight")]
fn validate_git_commit_identity(
    name: &str,
    email: &str,
) -> Result<GitCommitIdentity, GitCommitPlanError> {
    let name_scalars = name.chars().count();
    if name_scalars == 0
        || name_scalars > MAX_GIT_IDENTITY_NAME_SCALARS
        || name.trim() != name
        || name.contains(['<', '>'])
        || name.chars().any(unsafe_commit_text_scalar)
    {
        return Err(GitCommitPlanError::InvalidIdentity);
    }
    if email.len() < 3
        || email.len() > MAX_GIT_IDENTITY_EMAIL_BYTES
        || !email.is_ascii()
        || email.bytes().any(|byte| {
            byte.is_ascii_control() || byte.is_ascii_whitespace() || matches!(byte, b'<' | b'>')
        })
    {
        return Err(GitCommitPlanError::InvalidIdentity);
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err(GitCommitPlanError::InvalidIdentity);
    }
    let mut safe_name = String::new();
    let mut safe_email = String::new();
    safe_name
        .try_reserve_exact(name.len())
        .and_then(|()| safe_email.try_reserve_exact(email.len()))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    safe_name.push_str(name);
    safe_email.push_str(email);
    Ok(GitCommitIdentity {
        name: safe_name,
        email: safe_email,
    })
}

fn unsafe_commit_text_scalar(character: char) -> bool {
    let scalar = character as u32;
    character.is_control()
        || matches!(
            scalar,
            0x061c
                | 0x200e..=0x200f
                | 0x2028..=0x202e
                | 0x2066..=0x2069
        )
        || (0xfdd0..=0xfdef).contains(&scalar)
        || scalar & 0xfffe == 0xfffe
}

fn safe_git_label(value: &str) -> Option<String> {
    if value.is_empty() || value.chars().count() > 255 || value.chars().any(char::is_control) {
        None
    } else {
        Some(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        GitCloneFinalizeFault, GitCloneScaffoldFault, GitCommitMetadataFault,
        GitCommitMetadataRace, GitConfigFault, GitConfigRace, GitInitFault, GitSyncDirectoryRace,
        WorkspaceFault, WorkspaceInspection, inspect_workspace, set_git_clone_finalize_fault,
        set_git_clone_scaffold_fault, set_git_commit_metadata_fault, set_git_commit_metadata_race,
        set_git_config_fault, set_git_config_race, set_git_init_fault, set_git_sync_directory_race,
        set_git_sync_fail_after_directory_preparation, set_workspace_fault,
    };
    use std::{
        fs,
        net::{TcpListener, TcpStream},
        os::unix::{
            fs::{PermissionsExt, symlink},
            net::UnixListener,
        },
        process::{Child, Command},
    };

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("rehow-git-{}", uuid::Uuid::now_v7()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct ChildGuard(Child);

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn command_arguments(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    fn command_environment(command: &Command) -> std::collections::HashMap<String, Option<String>> {
        command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    fn assert_https_credential_boundary(command: &Command, expected_host: &str) {
        let arguments = command_arguments(command);
        assert_eq!(
            arguments
                .iter()
                .filter(|argument| {
                    argument.as_str() == credential_proxy::CREDENTIAL_HELPER_CONFIG
                })
                .count(),
            1
        );
        for required in [
            "credential.helper=",
            "credential.useHttpPath=false",
            credential_proxy::SANDBOX_CREDENTIAL_PROXY,
            credential_proxy::SANDBOX_CREDENTIAL_HELPER,
        ] {
            assert!(
                arguments.iter().any(|argument| argument == required),
                "{required}"
            );
        }
        assert!(
            !arguments
                .iter()
                .any(|argument| argument.contains("workspace"))
        );
        let environment = command_environment(command);
        assert_eq!(
            environment["REHOW_CREDENTIAL_HOST"].as_deref(),
            Some(expected_host)
        );
        assert_eq!(
            environment["REHOW_CREDENTIAL_HELPER_KIND"].as_deref(),
            Some("direct")
        );
    }

    fn assert_no_https_credential_boundary(command: &Command) {
        let arguments = command_arguments(command);
        assert!(
            !arguments
                .iter()
                .any(|argument| argument == credential_proxy::CREDENTIAL_HELPER_CONFIG)
        );
        assert!(
            !arguments
                .iter()
                .any(|argument| argument == credential_proxy::SANDBOX_CREDENTIAL_PROXY)
        );
        assert!(
            !arguments
                .iter()
                .any(|argument| argument == credential_proxy::SANDBOX_CREDENTIAL_HELPER)
        );
        let environment = command_environment(command);
        assert!(!environment.contains_key("REHOW_CREDENTIAL_HOST"));
        assert!(!environment.contains_key("REHOW_CREDENTIAL_HELPER_KIND"));
    }

    fn workspace(temp: &Temp) -> Box<WorkspaceView> {
        fs::create_dir(temp.0.join("categories")).unwrap();
        fs::write(temp.0.join("rehow.yaml"), "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Git\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n").unwrap();
        open_workspace(temp)
    }

    #[test]
    fn ssh_agent_provenance_holds_only_a_private_socket_inode_and_redacts_debug() {
        let temp = Temp::new();
        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o700)).unwrap();
        let socket_path = temp.0.join("agent.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).unwrap();

        let provenance = capture_git_ssh_agent_provenance_from(Some(socket_path.as_os_str()));
        assert_eq!(provenance.status(), GitSshAgentStatus::Trusted);
        let debug = format!("{provenance:?}");
        assert!(debug.contains("Trusted"));
        assert!(!debug.contains("agent.sock"));
        assert!(!debug.contains(temp.0.to_str().unwrap()));

        let held = provenance._socket.as_ref().unwrap().metadata().unwrap();
        drop(listener);
        fs::remove_file(&socket_path).unwrap();
        let replacement = UnixListener::bind(&socket_path).unwrap();
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).unwrap();
        let replacement_metadata = fs::symlink_metadata(&socket_path).unwrap();
        assert_ne!(
            (held.dev(), held.ino()),
            (replacement_metadata.dev(), replacement_metadata.ino())
        );
        assert_eq!(
            provenance
                ._socket
                .as_ref()
                .unwrap()
                .metadata()
                .unwrap()
                .ino(),
            held.ino()
        );
        drop(replacement);
    }

    #[test]
    fn ssh_agent_provenance_fails_closed_for_hostile_path_forms() {
        assert_eq!(
            capture_git_ssh_agent_provenance_from(None).status(),
            GitSshAgentStatus::NotConfigured
        );
        assert_eq!(
            capture_git_ssh_agent_provenance_from(Some(std::ffi::OsStr::new("relative.sock")))
                .status(),
            GitSshAgentStatus::Unsafe
        );

        let temp = Temp::new();
        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o700)).unwrap();
        let regular = temp.0.join("regular");
        fs::write(&regular, b"not a socket").unwrap();
        assert!(inspect_ssh_agent_path(&regular).is_none());

        let socket_path = temp.0.join("agent.sock");
        let _listener = UnixListener::bind(&socket_path).unwrap();
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).unwrap();
        let linked = temp.0.join("linked.sock");
        symlink(&socket_path, &linked).unwrap();
        assert!(inspect_ssh_agent_path(&linked).is_none());
        assert!(inspect_ssh_agent_path(&temp.0.join("child/../agent.sock")).is_none());

        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o660)).unwrap();
        assert!(inspect_ssh_agent_path(&socket_path).is_none());
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).unwrap();

        let oversized = PathBuf::from(format!("/{}", "a".repeat(MAX_SSH_AGENT_PATH_BYTES)));
        assert!(inspect_ssh_agent_path(&oversized).is_none());

        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(inspect_ssh_agent_path(&socket_path).is_none());
    }

    #[test]
    fn known_hosts_provenance_snapshots_stable_bounded_bytes_and_redacts_debug() {
        let home = Temp::new();
        fs::set_permissions(&home.0, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(home.0.join(".ssh")).unwrap();
        fs::set_permissions(home.0.join(".ssh"), fs::Permissions::from_mode(0o700)).unwrap();
        let known_hosts = home.0.join(".ssh/known_hosts");
        let initial = b"example.invalid ssh-ed25519 AAAAC3Nza\x80\n";
        fs::write(&known_hosts, initial).unwrap();
        fs::set_permissions(&known_hosts, fs::Permissions::from_mode(0o600)).unwrap();

        let provenance = capture_git_ssh_host_keys_from(Some(home.0.as_os_str()), &[]);
        assert_eq!(provenance.status(), GitSshHostKeysStatus::Trusted);
        assert_eq!(provenance.source_count(), 1);
        assert_eq!(provenance._sources[0], initial);
        let debug = format!("{provenance:?}");
        assert!(debug.contains("Trusted"));
        assert!(!debug.contains("example.invalid"));
        assert!(!debug.contains(home.0.to_str().unwrap()));

        fs::write(&known_hosts, b"replacement\n").unwrap();
        assert_eq!(provenance._sources[0], initial);

        let exact = home.0.join(".ssh/exact");
        let file = File::create(&exact).unwrap();
        file.set_len(MAX_KNOWN_HOSTS_BYTES).unwrap();
        drop(file);
        fs::set_permissions(&exact, fs::Permissions::from_mode(0o600)).unwrap();
        let exact_metadata = fs::symlink_metadata(&exact).unwrap();
        assert!(known_hosts_metadata_safe(
            &exact_metadata,
            KnownHostsOwner::EffectiveUser,
            unsafe { libc::geteuid() }
        ));
        assert!(
            inspect_ssh_agent_parents(exact.parent().unwrap(), unsafe { libc::geteuid() })
                .is_some()
        );
        assert!(matches!(
            capture_known_hosts_file(
                &exact,
                KnownHostsOwner::EffectiveUser,
                unsafe { libc::geteuid() }
            ),
            KnownHostsCapture::Trusted(bytes) if bytes.len() as u64 == MAX_KNOWN_HOSTS_BYTES
        ));
        let oversized = home.0.join(".ssh/oversized");
        let file = File::create(&oversized).unwrap();
        file.set_len(MAX_KNOWN_HOSTS_BYTES + 1).unwrap();
        drop(file);
        fs::set_permissions(&oversized, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            capture_known_hosts_file(&oversized, KnownHostsOwner::EffectiveUser, unsafe {
                libc::geteuid()
            }),
            KnownHostsCapture::Unsafe
        ));
    }

    #[test]
    fn known_hosts_provenance_refuses_unsafe_sources_and_discards_trusted_peers() {
        assert_eq!(
            capture_git_ssh_host_keys_from(None, &[]).status(),
            GitSshHostKeysStatus::NotConfigured
        );
        assert_eq!(
            capture_git_ssh_host_keys_from(Some(std::ffi::OsStr::new("relative")), &[]).status(),
            GitSshHostKeysStatus::Unsafe
        );

        let home = Temp::new();
        fs::set_permissions(&home.0, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(home.0.join(".ssh")).unwrap();
        fs::set_permissions(home.0.join(".ssh"), fs::Permissions::from_mode(0o700)).unwrap();
        let known_hosts = home.0.join(".ssh/known_hosts");
        fs::write(&known_hosts, b"trusted\n").unwrap();
        fs::set_permissions(&known_hosts, fs::Permissions::from_mode(0o600)).unwrap();

        let unsafe_system = home.0.join("unsafe-system");
        symlink(&known_hosts, &unsafe_system).unwrap();
        let provenance =
            capture_git_ssh_host_keys_from(Some(home.0.as_os_str()), &[unsafe_system.as_path()]);
        assert_eq!(provenance.status(), GitSshHostKeysStatus::Unsafe);
        assert_eq!(provenance.source_count(), 0);

        KNOWN_HOSTS_RACE.with(|race| {
            *race.borrow_mut() = Some(KnownHostsRace::Rewrite(
                known_hosts.clone(),
                b"hostile\n".to_vec(),
            ));
        });
        assert_eq!(
            capture_git_ssh_host_keys_from(Some(home.0.as_os_str()), &[]).status(),
            GitSshHostKeysStatus::Unsafe
        );
        fs::write(&known_hosts, b"trusted again\n").unwrap();
        KNOWN_HOSTS_RACE.with(|race| {
            *race.borrow_mut() = Some(KnownHostsRace::Replace(
                known_hosts.clone(),
                b"replaced during read\n".to_vec(),
            ));
        });
        assert_eq!(
            capture_git_ssh_host_keys_from(Some(home.0.as_os_str()), &[]).status(),
            GitSshHostKeysStatus::Unsafe
        );
        fs::set_permissions(&known_hosts, fs::Permissions::from_mode(0o600)).unwrap();

        fs::remove_file(&known_hosts).unwrap();
        fs::write(&known_hosts, b"linked\n").unwrap();
        fs::set_permissions(&known_hosts, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&known_hosts, home.0.join(".ssh/second-link")).unwrap();
        assert_eq!(
            capture_git_ssh_host_keys_from(Some(home.0.as_os_str()), &[]).status(),
            GitSshHostKeysStatus::Unsafe
        );

        fs::remove_file(home.0.join(".ssh/second-link")).unwrap();
        fs::set_permissions(&known_hosts, fs::Permissions::from_mode(0o622)).unwrap();
        assert_eq!(
            capture_git_ssh_host_keys_from(Some(home.0.as_os_str()), &[]).status(),
            GitSshHostKeysStatus::Unsafe
        );
    }

    fn open_workspace(temp: &Temp) -> Box<WorkspaceView> {
        let (_, inspection) = inspect_workspace(&temp.0).unwrap().into_parts();
        let WorkspaceInspection::Compatible(workspace) = inspection else {
            panic!()
        };
        workspace
    }

    fn initialized_workspace(temp: &Temp) -> Box<WorkspaceView> {
        let workspace = workspace(temp);
        let plan = plan_git_initialize(&workspace).unwrap();
        apply_git_initialize(&workspace, &plan).unwrap();
        open_workspace(temp)
    }

    fn fixture_git(temp: &Temp, arguments: &[&str]) {
        let status = Command::new("/usr/bin/git")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(arguments)
            .current_dir(&temp.0)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "git fixture command failed: {arguments:?}"
        );
    }

    fn fixture_git_stdout(temp: &Temp, arguments: &[&str]) -> String {
        let output = Command::new("/usr/bin/git")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(arguments)
            .current_dir(&temp.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git fixture command failed: {arguments:?}"
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn configure_fixture_identity(temp: &Temp) {
        fixture_git(temp, &["config", "--local", "user.name", "Rehow Test"]);
        fixture_git(
            temp,
            &["config", "--local", "user.email", "rehow@example.invalid"],
        );
    }

    fn configured_sync_workspace(temp: &Temp) -> Box<WorkspaceView> {
        drop(initialized_workspace(temp));
        configure_fixture_identity(temp);
        fixture_git(temp, &["add", "--", "rehow.yaml"]);
        fixture_git(temp, &["commit", "-qm", "base"]);
        fixture_git(
            temp,
            &[
                "remote",
                "add",
                "origin",
                "https://example.invalid/team/rehow.git",
            ],
        );
        fixture_git(temp, &["config", "branch.main.remote", "origin"]);
        fixture_git(temp, &["config", "branch.main.merge", "refs/heads/main"]);
        open_workspace(temp)
    }

    fn write_fixture_guide(temp: &Temp) {
        let category = temp.0.join("categories/linux-system--b6ba3a5551");
        fs::create_dir_all(&category).unwrap();
        fs::write(
            category.join("category.yaml"),
            b"schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux System\norder: 1000\nstatus: active\ncreated_at: \"2026-09-05T10:01:00Z\"\nupdated_at: \"2026-09-05T10:01:00Z\"\n",
        )
        .unwrap();
        fs::write(
            category.join("install--36c5735e10.md"),
            b"---\nschema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: Install a Tarball\nsummary: Install a downloaded archive.\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags: []\naliases: []\nstatus: draft\ncreated_at: \"2026-09-05T10:03:00Z\"\nupdated_at: \"2026-09-05T10:03:00Z\"\n---\n\n## Steps\n\nExtract it.\n",
        )
        .unwrap();
    }

    fn empty_target(temp: &Temp) -> EmptyWorkspaceTarget {
        let (_, inspection) = inspect_workspace(&temp.0).unwrap().into_parts();
        let WorkspaceInspection::Empty(target) = inspection else {
            panic!("expected empty destination")
        };
        target
    }

    fn config_backups(temp: &Temp) -> Vec<Vec<u8>> {
        let mut backups = fs::read_dir(temp.0.join(".git"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("rehow-config-before-v1-")
            })
            .map(|entry| fs::read(entry.path()).unwrap())
            .collect::<Vec<_>>();
        backups.sort();
        backups
    }

    fn clone_tree_listing(entries: &[(&str, &str, u64)]) -> Vec<u8> {
        let mut entries = entries.to_vec();
        entries.sort_unstable_by_key(|entry| entry.0);
        let mut listing = Vec::new();
        for (path, object_id, size) in entries {
            listing
                .extend_from_slice(format!("100644 blob {object_id} {size}\t{path}\0").as_bytes());
        }
        listing
    }

    fn index_fixture(
        format: GitObjectFormat,
        version: u32,
        entries: &[(u32, &[u8], u16)],
        extension: Option<(&[u8; 4], &[u8])>,
    ) -> Vec<u8> {
        let hash_len = match format {
            GitObjectFormat::Sha1 => 20,
            GitObjectFormat::Sha256 => 32,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"DIRC");
        bytes.extend_from_slice(&version.to_be_bytes());
        bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (ordinal, (mode, path, extra_flags)) in entries.iter().enumerate() {
            let start = bytes.len();
            let mut stat = [0_u8; 40];
            stat[24..28].copy_from_slice(&mode.to_be_bytes());
            bytes.extend_from_slice(&stat);
            bytes.extend(std::iter::repeat_n((ordinal + 1) as u8, hash_len));
            let path_len = u16::try_from(path.len()).unwrap().min(0x0fff);
            bytes.extend_from_slice(&(path_len | extra_flags).to_be_bytes());
            bytes.extend_from_slice(path);
            bytes.push(0);
            while (bytes.len() - start) % 8 != 0 {
                bytes.push(0);
            }
        }
        if let Some((signature, payload)) = extension {
            bytes.extend_from_slice(signature);
            bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            bytes.extend_from_slice(payload);
        }
        bytes.extend_from_slice(&git_index_checksum(&bytes, format));
        bytes
    }

    fn commit_tree_listing(entries: &[(&str, &str, &str, &str, &[u8])]) -> Vec<u8> {
        let mut listing = Vec::new();
        for (mode, object_type, object_id, size, path) in entries {
            listing
                .extend_from_slice(format!("{mode} {object_type} {object_id} {size}\t").as_bytes());
            listing.extend_from_slice(path);
            listing.push(0);
        }
        listing
    }

    #[test]
    fn commit_message_is_trimmed_bounded_and_single_line() {
        assert_eq!(
            normalize_git_commit_message("  Remember the fix  ").unwrap(),
            "Remember the fix"
        );
        assert_eq!(
            normalize_git_commit_message(&"é".repeat(MAX_COMMIT_MESSAGE_SCALARS)).unwrap(),
            "é".repeat(MAX_COMMIT_MESSAGE_SCALARS)
        );
        for invalid in [
            String::new(),
            " ".to_owned(),
            "a".repeat(MAX_COMMIT_MESSAGE_SCALARS + 1),
            "two\nlines".to_owned(),
            "tab\there".to_owned(),
            "bidi\u{202e}text".to_owned(),
            "separator\u{2028}text".to_owned(),
            "noncharacter\u{fdd0}".to_owned(),
        ] {
            assert_eq!(
                normalize_git_commit_message(&invalid),
                Err(GitCommitPlanError::InvalidMessage),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn commit_identity_is_bounded_canonical_and_redacted() {
        let identity = validate_git_commit_identity(
            &"N".repeat(MAX_GIT_IDENTITY_NAME_SCALARS),
            "person@example.test",
        )
        .unwrap();
        assert_eq!(identity.email, "person@example.test");
        let debug = format!("{identity:?}");
        assert!(!debug.contains("person"));
        assert!(!debug.contains("example"));

        let invalid_names = [
            String::new(),
            " leading".to_owned(),
            "trailing ".to_owned(),
            "bad<name".to_owned(),
            "bad\nname".to_owned(),
            "bad\u{2066}name".to_owned(),
            "N".repeat(MAX_GIT_IDENTITY_NAME_SCALARS + 1),
        ];
        for name in invalid_names {
            assert!(matches!(
                validate_git_commit_identity(&name, "person@example.test"),
                Err(GitCommitPlanError::InvalidIdentity)
            ));
        }

        let invalid_emails = [
            "a@",
            "@b",
            "a@@b",
            "a b@example.test",
            "a<b@example.test",
            "a\n@example.test",
        ];
        for email in invalid_emails {
            assert!(matches!(
                validate_git_commit_identity("Name", email),
                Err(GitCommitPlanError::InvalidIdentity)
            ));
        }
        let oversized_email = format!("{}@b", "a".repeat(MAX_GIT_IDENTITY_EMAIL_BYTES));
        assert!(matches!(
            validate_git_commit_identity("Name", &oversized_email),
            Err(GitCommitPlanError::InvalidIdentity)
        ));
    }

    #[test]
    fn commit_branch_accepts_git_loose_over_packed_precedence() {
        let object_id = "0123456789abcdef0123456789abcdef01234567";
        let head = b"ref: refs/heads/team/main\n";
        let loose = format!("{object_id}\n");
        let branch =
            parse_git_commit_branch(head, Some(loose.as_bytes()), None, GitObjectFormat::Sha1)
                .unwrap();
        assert_eq!(branch.reference, "refs/heads/team/main");
        assert_eq!(branch.display, "team/main");
        assert_eq!(branch.expected_object_id.as_deref(), Some(object_id));
        assert_eq!(branch.representation, GitBranchRepresentation::Loose);
        assert!(!format!("{branch:?}").contains(object_id));

        let packed = format!("# pack-refs with: peeled\n{object_id} refs/heads/team/main\n");
        assert_eq!(
            parse_git_commit_branch(head, None, Some(packed.as_bytes()), GitObjectFormat::Sha1)
                .unwrap()
                .representation,
            GitBranchRepresentation::Packed
        );
        assert_eq!(
            parse_git_commit_branch(head, None, None, GitObjectFormat::Sha1)
                .unwrap()
                .representation,
            GitBranchRepresentation::Unborn
        );
        let stale_packed = "# pack-refs with: peeled\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa refs/heads/team/main\n";
        let shadowed = parse_git_commit_branch(
            head,
            Some(loose.as_bytes()),
            Some(stale_packed.as_bytes()),
            GitObjectFormat::Sha1,
        )
        .unwrap();
        assert_eq!(shadowed.expected_object_id.as_deref(), Some(object_id));
        assert_eq!(shadowed.representation, GitBranchRepresentation::Loose);

        for (candidate_head, candidate_loose, candidate_packed) in [
            (
                b"0123456789abcdef0123456789abcdef01234567\n".as_slice(),
                None,
                None,
            ),
            (
                head.as_slice(),
                Some(b"ref: refs/heads/other\n".as_slice()),
                None,
            ),
        ] {
            assert!(matches!(
                parse_git_commit_branch(
                    candidate_head,
                    candidate_loose,
                    candidate_packed,
                    GitObjectFormat::Sha1
                ),
                Err(GitCommitPlanError::UnsafeRepository)
            ));
        }
    }

    #[test]
    fn commit_branch_rejects_duplicate_malformed_and_wrong_width_packed_targets() {
        let object_id = "0123456789abcdef0123456789abcdef01234567";
        let head = b"ref: refs/heads/main\n";
        for packed in [
            format!("{object_id} refs/heads/main\n{object_id} refs/heads/main\n"),
            format!("{object_id} refs/heads/main"),
            format!("{} refs/heads/main\n", "a".repeat(64)),
            format!("^{object_id}\n"),
            format!("{object_id} refs/heads/../main\n"),
        ] {
            assert!(matches!(
                parse_git_commit_branch(head, None, Some(packed.as_bytes()), GitObjectFormat::Sha1),
                Err(GitCommitPlanError::UnsafeRepository)
            ));
        }
        assert!(matches!(
            parse_git_commit_branch(
                b"ref: refs/heads/.hidden\n",
                None,
                None,
                GitObjectFormat::Sha1
            ),
            Err(GitCommitPlanError::UnsafeRepository)
        ));
    }

    #[test]
    fn commit_index_accepts_only_bounded_clean_v2_v3_with_optional_cache_tree() {
        let sha1 = index_fixture(
            GitObjectFormat::Sha1,
            2,
            &[
                (0o100644, b"alpha".as_slice(), 0),
                (0o100755, b"dir/non-utf8-\xff".as_slice(), 0),
            ],
            Some((b"TREE", b"cache")),
        );
        let parsed = parse_git_index(&sha1, GitObjectFormat::Sha1).unwrap();
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.entries.len(), 2);
        assert!(parsed.has_cache_tree);
        assert_eq!(
            git_index_without_extensions(&sha1, GitObjectFormat::Sha1, &parsed).unwrap(),
            index_fixture(
                GitObjectFormat::Sha1,
                2,
                &[
                    (0o100644, b"alpha".as_slice(), 0),
                    (0o100755, b"dir/non-utf8-\xff".as_slice(), 0),
                ],
                None,
            )
        );
        assert_eq!(parsed.entries[1].mode, 0o100755);
        assert_eq!(parsed.entries[1].object_id.len(), 20);
        assert_eq!(parsed.entries[1].path, b"dir/non-utf8-\xff");
        assert!(!format!("{:?}", parsed.entries[1]).contains("non-utf8"));

        let sha256 = index_fixture(
            GitObjectFormat::Sha256,
            3,
            &[(0o120000, b"link".as_slice(), 0)],
            None,
        );
        let parsed = parse_git_index(&sha256, GitObjectFormat::Sha256).unwrap();
        assert_eq!(parsed.version, 3);
        assert_eq!(parsed.entries[0].object_id.len(), 32);
        assert!(!parsed.has_cache_tree);
    }

    #[test]
    fn commit_index_rejects_checksum_flags_order_extensions_and_limits() {
        let valid = index_fixture(
            GitObjectFormat::Sha1,
            2,
            &[(0o100644, b"alpha".as_slice(), 0)],
            None,
        );
        let mut bad_checksum = valid.clone();
        bad_checksum[12] ^= 1;
        let staged = index_fixture(
            GitObjectFormat::Sha1,
            2,
            &[(0o100644, b"alpha".as_slice(), 0x1000)],
            None,
        );
        let extended = index_fixture(
            GitObjectFormat::Sha1,
            3,
            &[(0o100644, b"alpha".as_slice(), 0x4000)],
            None,
        );
        let unsorted = index_fixture(
            GitObjectFormat::Sha1,
            2,
            &[
                (0o100644, b"zeta".as_slice(), 0),
                (0o100644, b"alpha".as_slice(), 0),
            ],
            None,
        );
        let unknown_extension = index_fixture(
            GitObjectFormat::Sha1,
            2,
            &[(0o100644, b"alpha".as_slice(), 0)],
            Some((b"FSMN", b"hostile")),
        );
        let wrong_version = index_fixture(GitObjectFormat::Sha1, 4, &[], None);
        for bytes in [
            bad_checksum,
            staged,
            extended,
            unsorted,
            unknown_extension,
            wrong_version,
        ] {
            assert!(matches!(
                parse_git_index(&bytes, GitObjectFormat::Sha1),
                Err(GitCommitPlanError::UnsupportedIndex)
            ));
        }

        let mut too_many = Vec::new();
        too_many.extend_from_slice(b"DIRC");
        too_many.extend_from_slice(&2_u32.to_be_bytes());
        too_many.extend_from_slice(&((MAX_COMMIT_INDEX_ENTRIES + 1) as u32).to_be_bytes());
        too_many.extend_from_slice(&git_index_checksum(&too_many, GitObjectFormat::Sha1));
        assert!(matches!(
            parse_git_index(&too_many, GitObjectFormat::Sha1),
            Err(GitCommitPlanError::ResourceLimit)
        ));
    }

    #[test]
    fn commit_tree_parses_bounded_opaque_paths_and_supported_git_modes() {
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let listing = commit_tree_listing(&[
            ("040000", "tree", oid, "-", b"dir"),
            ("100755", "blob", oid, "7", b"dir/executable"),
            ("120000", "blob", oid, "6", b"dir/link"),
            ("100644", "blob", oid, "3", b"dir/non-utf8-\xff"),
            ("160000", "commit", oid, "-", b"submodule"),
        ]);
        let tree = parse_git_commit_tree(&listing, GitObjectFormat::Sha1).unwrap();
        assert_eq!(tree.entries.len(), 5);
        assert_eq!(tree.entries[0].kind, GitCommitTreeEntryKind::Tree);
        assert_eq!(tree.entries[1].mode, 0o100755);
        assert_eq!(tree.entries[1].size, Some(7));
        assert_eq!(tree.entries[4].kind, GitCommitTreeEntryKind::Commit);
        let debug = format!("{:?}", tree.entries[3]);
        assert!(!debug.contains("non-utf8"));
        assert!(!debug.contains(oid));
        assert!(
            parse_git_commit_tree(&[], GitObjectFormat::Sha1)
                .unwrap()
                .entries
                .is_empty()
        );
    }

    #[test]
    fn commit_tree_rejects_malformed_order_hierarchy_type_and_width() {
        let oid = "0123456789abcdef0123456789abcdef01234567";
        for listing in [
            commit_tree_listing(&[("100644", "blob", oid, "1", b"dir/file")]),
            commit_tree_listing(&[
                ("100644", "blob", oid, "1", b"z"),
                ("100644", "blob", oid, "1", b"a"),
            ]),
            commit_tree_listing(&[("100755", "tree", oid, "-", b"dir")]),
            commit_tree_listing(&[("100644", "blob", oid, "-", b"file")]),
            commit_tree_listing(&[("100644", "blob", &"a".repeat(64), "1", b"file")]),
            b"100644 blob 0123456789abcdef0123456789abcdef01234567 1\tfile".to_vec(),
        ] {
            assert!(matches!(
                parse_git_commit_tree(&listing, GitObjectFormat::Sha1),
                Err(GitCommitPlanError::UnsafeRepository)
            ));
        }
    }

    #[test]
    fn commit_tree_detects_only_new_managed_ownership_collisions() {
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let tree = parse_git_commit_tree(
            &commit_tree_listing(&[
                ("040000", "tree", oid, "-", b"categories"),
                ("040000", "tree", oid, "-", b"categories/existing"),
                (
                    "100644",
                    "blob",
                    oid,
                    "1",
                    b"categories/existing/category.yaml",
                ),
                ("040000", "tree", oid, "-", b"categories/unrelated"),
                ("100644", "blob", oid, "1", b"categories/unrelated/file"),
                ("100644", "blob", oid, "1", b"rehow.yaml"),
            ]),
            GitObjectFormat::Sha1,
        )
        .unwrap();
        let prior = vec![
            b"categories/existing/category.yaml".to_vec(),
            b"rehow.yaml".to_vec(),
        ];
        assert!(!git_commit_path_collision(
            &tree,
            &prior,
            &[
                b"categories/existing/category.yaml".to_vec(),
                b"categories/new/category.yaml".to_vec(),
                b"rehow.yaml".to_vec(),
            ]
        ));
        for current in [
            b"categories/unrelated".to_vec(),
            b"categories/unrelated/file/child".to_vec(),
            b"categories/unrelated/new.yaml".to_vec(),
        ] {
            assert!(git_commit_path_collision(&tree, &prior, &[current]));
        }
        assert!(!git_commit_path_collision(
            &tree,
            &prior,
            &[b"rehow.yaml".to_vec()]
        ));
    }

    #[test]
    fn commit_repository_preflight_accepts_stable_unborn_and_parented_sha1_state() {
        let unborn = Temp::new();
        drop(initialized_workspace(&unborn));
        configure_fixture_identity(&unborn);
        let unborn_workspace = open_workspace(&unborn);
        let preflight = inspect_git_commit_repository(&unborn_workspace).unwrap();
        assert_eq!(preflight.object_format, GitObjectFormat::Sha1);
        assert_eq!(preflight.branch.display, "main");
        assert_eq!(
            preflight.branch.representation,
            GitBranchRepresentation::Unborn
        );
        assert!(preflight.index.is_none());
        assert!(!format!("{preflight:?}").contains("rehow@example.invalid"));

        let parented = Temp::new();
        drop(initialized_workspace(&parented));
        configure_fixture_identity(&parented);
        fixture_git(&parented, &["add", "--", "rehow.yaml"]);
        fixture_git(&parented, &["commit", "-qm", "fixture"]);
        let workspace = open_workspace(&parented);
        let capability = workspace.git_repository_capability().unwrap();
        let sandbox = trusted_executable("/usr/bin/bwrap").unwrap();
        let executable = trusted_executable("/usr/bin/git").unwrap();
        let parent = fs::read_to_string(parented.0.join(".git/refs/heads/main")).unwrap();
        let listing = run_git_commit_read(
            &executable,
            &sandbox,
            &capability,
            GitObjectFormat::Sha1,
            &[
                "ls-tree",
                "-r",
                "-t",
                "-l",
                "--full-tree",
                "-z",
                parent.trim(),
            ],
            MAX_COMMIT_TREE_BYTES,
        )
        .unwrap();
        parse_git_commit_tree(&listing.stdout, GitObjectFormat::Sha1).unwrap();
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        assert_eq!(preflight.object_format, GitObjectFormat::Sha1);
        assert_eq!(
            preflight.branch.representation,
            GitBranchRepresentation::Loose
        );
        assert!(preflight.branch.expected_object_id.is_some());
        assert_eq!(preflight.index.as_ref().unwrap().entries.len(), 1);
    }

    #[test]
    fn commit_repository_preflight_refuses_every_operation_marker_and_branch_lock() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        let workspace = open_workspace(&temp);
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "REBASE_HEAD",
            "BISECT_LOG",
            "BISECT_START",
            "AUTO_MERGE",
            "index.lock",
            "HEAD.lock",
            "packed-refs.lock",
            "config.lock",
            "shallow.lock",
        ] {
            fs::write(temp.0.join(".git").join(marker), b"busy").unwrap();
            assert_eq!(
                inspect_git_commit_repository(&workspace).unwrap_err(),
                GitCommitPlanError::OperationInProgress,
                "marker {marker} was accepted"
            );
            fs::remove_file(temp.0.join(".git").join(marker)).unwrap();
        }
        for marker in ["rebase-apply", "rebase-merge", "sequencer"] {
            fs::create_dir(temp.0.join(".git").join(marker)).unwrap();
            assert_eq!(
                inspect_git_commit_repository(&workspace).unwrap_err(),
                GitCommitPlanError::OperationInProgress,
                "marker {marker} was accepted"
            );
            fs::remove_dir(temp.0.join(".git").join(marker)).unwrap();
        }
        fs::write(temp.0.join(".git/refs/heads/main.lock"), b"busy").unwrap();
        assert_eq!(
            inspect_git_commit_repository(&workspace).unwrap_err(),
            GitCommitPlanError::OperationInProgress
        );
    }

    #[test]
    fn commit_repository_preflight_refuses_ambiguous_identity_and_unsafe_metadata() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fs::OpenOptions::new()
            .append(true)
            .open(temp.0.join(".git/config"))
            .unwrap()
            .write_all(b"[user]\n\tname = Other Name\n")
            .unwrap();
        let workspace = open_workspace(&temp);
        assert_eq!(
            inspect_git_commit_repository(&workspace).unwrap_err(),
            GitCommitPlanError::UnsafeRepository
        );

        let symlink_marker = Temp::new();
        drop(initialized_workspace(&symlink_marker));
        configure_fixture_identity(&symlink_marker);
        symlink("/dev/null", symlink_marker.0.join(".git/CHERRY_PICK_HEAD")).unwrap();
        let workspace = open_workspace(&symlink_marker);
        assert_eq!(
            inspect_git_commit_repository(&workspace).unwrap_err(),
            GitCommitPlanError::OperationInProgress
        );
    }

    #[test]
    fn commit_repository_preflight_supports_sha256_without_active_config_authority() {
        let temp = Temp::new();
        drop(workspace(&temp));
        fixture_git(
            &temp,
            &[
                "init",
                "-q",
                "--object-format=sha256",
                "--initial-branch=main",
            ],
        );
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "fixture"]);
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: SHA-256");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        assert_eq!(preflight.object_format, GitObjectFormat::Sha256);
        assert_eq!(
            preflight.branch.expected_object_id.as_ref().unwrap().len(),
            64
        );
        assert_eq!(
            preflight.index.as_ref().unwrap().entries[0].object_id.len(),
            32
        );
        assert_eq!(preflight.managed.files[0].blob_object_id.len(), 64);
        assert_eq!(preflight.head_tree_object_id.as_ref().unwrap().len(), 64);
        let state = Temp::new();
        let prepared = prepare_git_commit_staging(&preflight, &state.0).unwrap();
        assert_eq!(prepared.prior_managed.files.len(), 1);
        let candidate = construct_git_commit_candidate(
            &workspace,
            &preflight,
            prepared,
            "Update SHA-256 Workspace",
        )
        .unwrap();
        assert_eq!(candidate.tree_object_id.len(), 64);
        assert_eq!(candidate.commit_object_id.len(), 64);
    }

    #[test]
    fn commit_private_write_tree_matches_head_without_active_mutation() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "fixture"]);
        let workspace = open_workspace(&temp);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        let active_before = preflight
            .capability
            .read_commit_repository_snapshot()
            .unwrap();
        let state = Temp::new();
        let staging = prepare_git_commit_staging(&preflight, &state.0).unwrap();
        assert!(staging.staging.stage_path().starts_with("/proc/self/fd/"));
        assert!(
            Path::new(staging.staging.git_path())
                .join("index")
                .is_file()
        );
        assert_eq!(staging.prior_managed.files.len(), 1);
        assert_eq!(
            preflight
                .capability
                .read_commit_repository_snapshot()
                .unwrap(),
            active_before
        );
        assert!(matches!(
            construct_git_commit_candidate(&workspace, &preflight, staging, "No managed change"),
            Err(GitCommitPlanError::NothingToCommit)
        ));

        let unborn = Temp::new();
        drop(initialized_workspace(&unborn));
        configure_fixture_identity(&unborn);
        let unborn_workspace = open_workspace(&unborn);
        let preflight = inspect_git_commit_repository(&unborn_workspace).unwrap();
        assert!(preflight.head_tree_object_id.is_none());
        let unborn_state = Temp::new();
        let staging = prepare_git_commit_staging(&preflight, &unborn_state.0).unwrap();
        assert!(!Path::new(staging.staging.git_path()).join("index").exists());
        assert!(staging.prior_managed.files.is_empty());
        let candidate = construct_git_commit_candidate(
            &unborn_workspace,
            &preflight,
            staging,
            "Initial Rehow Workspace",
        )
        .unwrap();
        assert_eq!(
            (candidate.created, candidate.updated, candidate.deleted),
            (1, 0, 0)
        );
    }

    #[test]
    fn commit_candidate_constructs_verified_managed_update_and_preserves_active_git() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fs::write(temp.0.join("unrelated.txt"), b"preserve exactly\n").unwrap();
        fixture_git(&temp, &["add", "--", "rehow.yaml", "unrelated.txt"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        fs::write(
            temp.0.join("unrelated.txt"),
            b"modified but not committed\n",
        )
        .unwrap();
        fs::write(temp.0.join("untracked.tmp"), b"leave untracked\n").unwrap();
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Changed");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        let active_before = preflight
            .capability
            .read_commit_repository_snapshot()
            .unwrap();
        let state = Temp::new();
        let prepared = prepare_git_commit_staging(&preflight, &state.0).unwrap();
        let candidate = construct_git_commit_candidate(
            &workspace,
            &preflight,
            prepared,
            "Update Rehow guide library",
        )
        .unwrap();
        assert_eq!(
            (candidate.created, candidate.updated, candidate.deleted),
            (0, 1, 0)
        );
        assert_eq!(
            validate_object_id(&candidate.tree_object_id),
            Some(GitObjectFormat::Sha1)
        );
        assert_eq!(
            validate_object_id(&candidate.commit_object_id),
            Some(GitObjectFormat::Sha1)
        );
        assert!(valid_git_commit_timestamp(
            candidate.git_timestamp.as_bytes()
        ));
        assert_eq!(candidate.prepared.prior_managed.files.len(), 1);
        let executable = trusted_executable("/usr/bin/git").unwrap();
        let sandbox = trusted_executable("/usr/bin/bwrap").unwrap();
        let mut command = git_commit_staging_read_command(
            &executable,
            &sandbox,
            &preflight.capability,
            &candidate.prepared.staging,
            &[
                "ls-tree",
                "-r",
                "-t",
                "-l",
                "--full-tree",
                "-z",
                &candidate.tree_object_id,
            ],
        );
        command.stdin(Stdio::null());
        let output =
            run_bounded_git_command(command, COMMIT_PROCESS_TIMEOUT, OUTPUT_LIMIT).unwrap();
        let candidate_tree = parse_git_commit_tree(&output.stdout, GitObjectFormat::Sha1).unwrap();
        let unrelated_before = preflight
            .tree
            .entries
            .iter()
            .find(|entry| entry.path == b"unrelated.txt")
            .unwrap();
        let unrelated_after = candidate_tree
            .entries
            .iter()
            .find(|entry| entry.path == b"unrelated.txt")
            .unwrap();
        assert_eq!(unrelated_after.mode, unrelated_before.mode);
        assert_eq!(unrelated_after.object_id, unrelated_before.object_id);
        assert_eq!(
            count_unrelated_git_commit_changes(
                &executable,
                &sandbox,
                &preflight.capability,
                preflight.object_format,
                &candidate.prepared.prior_managed,
                &preflight.managed,
            )
            .unwrap(),
            2
        );
        assert_eq!(
            preflight
                .capability
                .read_commit_repository_snapshot()
                .unwrap(),
            active_before
        );
        assert_eq!(
            fs::read_to_string(temp.0.join("unrelated.txt")).unwrap(),
            "modified but not committed\n"
        );
    }

    #[test]
    fn commit_candidate_counts_managed_creates_and_deletes() {
        let created = Temp::new();
        drop(initialized_workspace(&created));
        configure_fixture_identity(&created);
        fixture_git(&created, &["add", "--", "rehow.yaml"]);
        fixture_git(&created, &["commit", "-qm", "base"]);
        write_fixture_guide(&created);
        let workspace = open_workspace(&created);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        let state = Temp::new();
        let prepared = prepare_git_commit_staging(&preflight, &state.0).unwrap();
        let candidate =
            construct_git_commit_candidate(&workspace, &preflight, prepared, "Add Linux guide")
                .unwrap();
        assert_eq!(
            (candidate.created, candidate.updated, candidate.deleted),
            (2, 0, 0)
        );

        let deleted = Temp::new();
        drop(initialized_workspace(&deleted));
        configure_fixture_identity(&deleted);
        write_fixture_guide(&deleted);
        fixture_git(&deleted, &["add", "--", "rehow.yaml", "categories"]);
        fixture_git(&deleted, &["commit", "-qm", "with guide"]);
        fs::remove_dir_all(deleted.0.join("categories/linux-system--b6ba3a5551")).unwrap();
        let workspace = open_workspace(&deleted);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        let state = Temp::new();
        let prepared = prepare_git_commit_staging(&preflight, &state.0).unwrap();
        let candidate = construct_git_commit_candidate(
            &workspace,
            &preflight,
            prepared,
            "Remove obsolete Linux guide",
        )
        .unwrap();
        assert_eq!(
            (candidate.created, candidate.updated, candidate.deleted),
            (0, 0, 2)
        );
    }

    #[test]
    fn commit_candidate_pack_is_exact_verified_and_keeps_active_objects_unchanged() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Packed candidate");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let mut plan = plan_git_commit(&workspace, &state.0, "Pack managed candidate").unwrap();
        let commit_id = plan.candidate.commit_object_id.clone();
        let active_pack = temp.0.join(".git/objects/pack");
        let active_before = fs::read_dir(&active_pack)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();

        let packed = prepare_git_commit_pack(&mut plan).unwrap();
        assert!(!packed.object_ids.is_empty());
        assert!(packed.object_ids.contains(&commit_id));
        assert_eq!(packed.sources.len(), 2);
        assert!(packed.sources[0].name.ends_with(".idx"));
        assert!(packed.sources[1].name.ends_with(".pack"));
        assert_eq!(packed.staging_snapshot, plan.staging_snapshot);
        assert!(!format!("{packed:?}").contains(&commit_id));
        assert_eq!(
            fs::read_dir(active_pack)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>(),
            active_before
        );
        cancel_git_commit_plan(plan).unwrap();
    }

    #[test]
    fn commit_candidate_pack_installs_append_only_without_publishing_commit() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let head_before = fixture_git_stdout(&temp, &["rev-parse", "HEAD"]);
        let index_before = fs::read(temp.0.join(".git/index")).unwrap();
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Installed pack");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let mut plan = plan_git_commit(&workspace, &state.0, "Install candidate pack").unwrap();
        let commit_id = plan.candidate.commit_object_id.clone();

        let packed =
            install_git_commit_candidate_pack(&workspace, &state.0, None, &mut plan).unwrap();
        let installed = fs::read_dir(temp.0.join(".git/objects/pack"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(installed.len(), 2);
        assert!(
            packed
                .sources
                .iter()
                .all(|source| installed.contains(&source.name))
        );
        fixture_git(&temp, &["cat-file", "-e", &commit_id]);
        assert_eq!(
            fixture_git_stdout(&temp, &["rev-parse", "HEAD"]),
            head_before
        );
        assert_eq!(fs::read(temp.0.join(".git/index")).unwrap(), index_before);

        install_git_commit_candidate_pack(&workspace, &state.0, None, &mut plan).unwrap();
        assert_eq!(
            fs::read_dir(temp.0.join(".git/objects/pack"))
                .unwrap()
                .count(),
            2
        );
        cancel_git_commit_plan(plan).unwrap();
    }

    #[test]
    fn commit_candidate_index_is_ordinary_exact_and_keeps_active_index_unchanged() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let active_index_before = fs::read(temp.0.join(".git/index")).unwrap();
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Candidate index");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let mut plan = plan_git_commit(&workspace, &state.0, "Build candidate index").unwrap();
        prepare_git_commit_pack(&mut plan).unwrap();

        let candidate = prepare_git_commit_candidate_index(&mut plan).unwrap();
        let parsed = parse_git_index(&candidate.bytes, GitObjectFormat::Sha1).unwrap();
        assert_eq!(candidate.source.name, "index");
        assert_eq!(candidate.source.size, candidate.bytes.len() as u64);
        assert!(!parsed.has_cache_tree);
        assert_eq!(candidate.staging_snapshot, plan.staging_snapshot);
        assert_eq!(
            fs::read(temp.0.join(".git/index")).unwrap(),
            active_index_before
        );
        let debug = format!("{candidate:?}");
        assert!(debug.contains("<redacted:"));
        assert!(!debug.contains(&plan.candidate.tree_object_id));
        cancel_git_commit_plan(plan).unwrap();
    }

    #[test]
    fn commit_metadata_publishes_index_then_ref_without_touching_reflogs_or_unrelated_files() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fs::write(temp.0.join("unrelated.txt"), b"tracked before\n").unwrap();
        fixture_git(&temp, &["add", "--", "rehow.yaml", "unrelated.txt"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        fs::write(temp.0.join("unrelated.txt"), b"tracked after\n").unwrap();
        fs::write(temp.0.join("untracked.tmp"), b"untracked\n").unwrap();
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Published metadata");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let head_log_path = temp.0.join(".git/logs/HEAD");
        let branch_log_path = temp.0.join(".git/logs/refs/heads/main");
        let head_log_before = fs::read(&head_log_path).unwrap();
        let branch_log_before = fs::read(&branch_log_path).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let plan = plan_git_commit(&workspace, &state.0, "Publish managed metadata").unwrap();
        let commit_id = plan.candidate.commit_object_id.clone();
        let tree_id = plan.candidate.tree_object_id.clone();
        let staging_name = plan.candidate.prepared.staging.stage_name().to_owned();

        let outcome = apply_git_commit(&workspace, &state.0, None, plan).unwrap();
        assert!(!outcome.workspace_changed());
        let operation_id = outcome.operation_id;

        assert_eq!(fixture_git_stdout(&temp, &["rev-parse", "HEAD"]), commit_id);
        assert_eq!(fixture_git_stdout(&temp, &["write-tree"]), tree_id);
        assert_eq!(
            fs::read_to_string(temp.0.join("unrelated.txt")).unwrap(),
            "tracked after\n"
        );
        assert_eq!(
            fs::read_to_string(temp.0.join("untracked.tmp")).unwrap(),
            "untracked\n"
        );
        assert_eq!(fs::read(head_log_path).unwrap(), head_log_before);
        assert_eq!(fs::read(branch_log_path).unwrap(), branch_log_before);
        assert!(!temp.0.join(".git/index.lock").exists());
        assert!(!temp.0.join(".git/refs/heads/main.lock").exists());
        assert!(
            !state
                .0
                .join("rehow/commit-staging")
                .join(&staging_name)
                .exists()
        );
        assert!(
            state
                .0
                .join("rehow/commit-quarantine")
                .join(&staging_name)
                .is_dir()
        );
        let session = workspace
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        let intent = session.read_intent(operation_id).unwrap();
        assert_eq!(intent.stage, GitCommitIntentStage::Complete);
        assert_eq!(intent.generation, 4);
        assert_eq!(
            session.read_bytes(&intent.candidate_ref_hash).unwrap(),
            format!("{commit_id}\n").as_bytes()
        );
        drop(session);
        assert_eq!(
            resume_pending_git_commit_publication(&workspace, &state.0, None).unwrap_err(),
            GitCommitPlanError::RecoveryNotRequired
        );
    }

    #[test]
    fn commit_publication_resumes_after_durable_index_and_ref_boundaries() {
        for fault in [
            GitCommitMetadataFault::IndexLockPublished,
            GitCommitMetadataFault::IndexExchanged,
            GitCommitMetadataFault::IndexPublished,
            GitCommitMetadataFault::RefLockPublished,
            GitCommitMetadataFault::RefExchanged,
            GitCommitMetadataFault::RefPublished,
        ] {
            let temp = Temp::new();
            drop(initialized_workspace(&temp));
            configure_fixture_identity(&temp);
            fixture_git(&temp, &["add", "--", "rehow.yaml"]);
            fixture_git(&temp, &["commit", "-qm", "base"]);
            let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
                .unwrap()
                .replace("name: Git", "name: Interrupted publication");
            fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
            let workspace = open_workspace(&temp);
            let state = Temp::new();
            let mut plan =
                plan_git_commit(&workspace, &state.0, "Resume managed publication").unwrap();
            let commit_id = plan.candidate.commit_object_id.clone();
            set_git_commit_metadata_fault(fault);
            assert_eq!(
                publish_git_commit_with_intent(&workspace, &state.0, None, &mut plan).unwrap_err(),
                GitCommitPlanError::UnsupportedRepository
            );
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            let intents = session.list_intents().unwrap();
            assert_eq!(intents.len(), 1);
            let operation_id = intents[0].operation_id;
            assert_eq!(
                git_commit_intent_plan_hash(&intents[0], &commit_id),
                intents[0].plan_hash
            );
            assert_eq!(
                intents[0].stage,
                match fault {
                    GitCommitMetadataFault::IndexLockPublished
                    | GitCommitMetadataFault::IndexExchanged => GitCommitIntentStage::Prepared,
                    GitCommitMetadataFault::IndexPublished
                    | GitCommitMetadataFault::RefLockPublished
                    | GitCommitMetadataFault::RefExchanged => {
                        GitCommitIntentStage::IndexPublished
                    }
                    GitCommitMetadataFault::RefPublished => GitCommitIntentStage::RefPublished,
                }
            );
            drop(session);
            assert_eq!(
                pending_git_commit_recovery(&workspace, &state.0).unwrap(),
                Some(operation_id)
            );
            assert_eq!(
                plan_git_commit(&workspace, &state.0, "Blocked by recovery").unwrap_err(),
                GitCommitPlanError::OperationInProgress
            );
            let workspace_changed = fault == GitCommitMetadataFault::RefPublished;
            if workspace_changed {
                let raced = fs::read_to_string(temp.0.join("rehow.yaml"))
                    .unwrap()
                    .replace(
                        "name: Interrupted publication",
                        "name: Changed after publication",
                    );
                fs::write(temp.0.join("rehow.yaml"), raced).unwrap();
            }

            assert_eq!(
                resume_pending_git_commit_publication(&workspace, &state.0, None)
                    .unwrap()
                    .workspace_changed(),
                workspace_changed
            );
            assert_eq!(fixture_git_stdout(&temp, &["rev-parse", "HEAD"]), commit_id);
            if workspace_changed {
                assert!(
                    fs::read_to_string(temp.0.join("rehow.yaml"))
                        .unwrap()
                        .contains("Changed after publication")
                );
                let status = read_git_status(&workspace).unwrap();
                let GitRepositoryStatus::Ready(repository) = status.repository() else {
                    panic!("expected ready Git status");
                };
                assert!(repository.changed() > 0);
                let refreshed = open_workspace(&temp);
                let next = plan_git_commit(&refreshed, &state.0, "Commit raced edit").unwrap();
                cancel_git_commit_plan(next).unwrap();
            } else {
                assert_eq!(
                    plan_git_commit(&workspace, &state.0, "Already committed").unwrap_err(),
                    GitCommitPlanError::NothingToCommit
                );
            }
            assert_eq!(
                pending_git_commit_recovery(&workspace, &state.0).unwrap(),
                None
            );
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            assert_eq!(
                session.read_intent(operation_id).unwrap().stage,
                GitCommitIntentStage::Complete
            );
            drop(session);
        }
    }

    #[test]
    fn commit_recovery_plan_is_bound_to_the_exact_interrupted_operation() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: First interrupted commit");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let mut first = plan_git_commit(&workspace, &state.0, "First interruption").unwrap();
        let commit_id = first.candidate.commit_object_id.clone();
        set_git_commit_metadata_fault(GitCommitMetadataFault::IndexLockPublished);
        assert_eq!(
            publish_git_commit_with_intent(&workspace, &state.0, None, &mut first).unwrap_err(),
            GitCommitPlanError::UnsupportedRepository
        );
        let recovery_for_first = plan_git_commit_recovery(&workspace, &state.0, None).unwrap();
        let first_operation = pending_git_commit_recovery(&workspace, &state.0)
            .unwrap()
            .unwrap();
        let session = workspace
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        let first_intent = session.read_intent(first_operation).unwrap();
        session
            .quarantine_staging(
                &first_intent.staging_name,
                (first_intent.staging_device, first_intent.staging_inode),
            )
            .unwrap();
        session
            .install_intent(
                &first_intent
                    .next_stage(GitCommitIntentStage::Conflict)
                    .unwrap(),
            )
            .unwrap();
        drop(session);
        drop(first);
        assert_eq!(
            pending_git_commit_recovery(&workspace, &state.0).unwrap(),
            None
        );
        let mut second_intent = first_intent.clone();
        second_intent.operation_id = new_operation_id();
        second_intent.generation = 1;
        second_intent.stage = GitCommitIntentStage::Prepared;
        second_intent.plan_hash = git_commit_intent_plan_hash(&second_intent, &commit_id);
        let session = workspace
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        session.install_intent(&second_intent).unwrap();
        drop(session);
        let second_operation = pending_git_commit_recovery(&workspace, &state.0)
            .unwrap()
            .unwrap();
        assert_ne!(first_operation, second_operation);
        assert_eq!(
            resume_planned_git_commit_publication(&workspace, &state.0, None, recovery_for_first,)
                .unwrap_err(),
            GitCommitPlanError::PlanStale
        );
        assert_eq!(
            pending_git_commit_recovery(&workspace, &state.0).unwrap(),
            Some(second_operation)
        );
    }

    #[test]
    fn commit_apply_refuses_a_second_intent_and_quarantines_only_its_staging() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Concurrent intent");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let mut first = plan_git_commit(&workspace, &state.0, "First plan").unwrap();
        let second = plan_git_commit(&workspace, &state.0, "Second plan").unwrap();
        let first_staging = first.candidate.prepared.staging.stage_name().to_owned();
        let second_staging = second.candidate.prepared.staging.stage_name().to_owned();

        set_git_commit_metadata_fault(GitCommitMetadataFault::IndexLockPublished);
        assert_eq!(
            publish_git_commit_with_intent(&workspace, &state.0, None, &mut first).unwrap_err(),
            GitCommitPlanError::UnsupportedRepository
        );
        assert_eq!(
            apply_git_commit(&workspace, &state.0, None, second).unwrap_err(),
            GitCommitPlanError::OperationInProgress
        );
        assert!(
            state
                .0
                .join("rehow/commit-staging")
                .join(&first_staging)
                .is_dir()
        );
        assert!(
            state
                .0
                .join("rehow/commit-quarantine")
                .join(&second_staging)
                .is_dir()
        );
        let session = workspace
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        assert_eq!(
            session
                .list_intents()
                .unwrap()
                .into_iter()
                .filter(|intent| intent.stage != GitCommitIntentStage::Complete)
                .count(),
            1
        );
        drop(session);
        resume_pending_git_commit_publication(&workspace, &state.0, None).unwrap();
    }

    #[test]
    fn commit_ref_races_restore_exact_index_and_preserve_foreign_ref() {
        for race in [
            GitCommitMetadataRace::ReplaceRefBeforePublication,
            GitCommitMetadataRace::ReplaceRefAfterValidation,
        ] {
            let temp = Temp::new();
            drop(initialized_workspace(&temp));
            configure_fixture_identity(&temp);
            fixture_git(&temp, &["add", "--", "rehow.yaml"]);
            fixture_git(&temp, &["commit", "-qm", "base"]);
            let index_before = fs::read(temp.0.join(".git/index")).unwrap();
            let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
                .unwrap()
                .replace("name: Git", "name: Ref race");
            fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
            let workspace = open_workspace(&temp);
            let state = Temp::new();
            let mut plan = plan_git_commit(&workspace, &state.0, "Lose ref race").unwrap();
            set_git_commit_metadata_race(race);

            assert_eq!(
                publish_git_commit_with_intent(&workspace, &state.0, None, &mut plan).unwrap_err(),
                GitCommitPlanError::PlanStale
            );
            assert_eq!(fs::read(temp.0.join(".git/index")).unwrap(), index_before);
            let foreign = b"1111111111111111111111111111111111111111\n";
            assert_eq!(
                fs::read(temp.0.join(".git/refs/heads/main")).unwrap(),
                foreign
            );
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            let intents = session.list_intents().unwrap();
            assert_eq!(intents.len(), 1);
            assert_eq!(intents[0].stage, GitCommitIntentStage::IndexPublished);
            let operation_id = intents[0].operation_id;
            let staging_name = intents[0].staging_name.clone();
            drop(session);
            assert_eq!(
                resume_git_commit_publication(&workspace, &state.0, None, operation_id)
                    .unwrap_err(),
                GitCommitPlanError::PlanStale
            );
            assert_eq!(fs::read(temp.0.join(".git/index")).unwrap(), index_before);
            assert_eq!(
                fs::read(temp.0.join(".git/refs/heads/main")).unwrap(),
                foreign
            );
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            assert_eq!(
                session.read_intent(operation_id).unwrap().stage,
                GitCommitIntentStage::Conflict
            );
            drop(session);
            assert!(
                !state
                    .0
                    .join("rehow/commit-staging")
                    .join(&staging_name)
                    .exists()
            );
            assert!(
                state
                    .0
                    .join("rehow/commit-quarantine")
                    .join(&staging_name)
                    .is_dir()
            );
            assert_eq!(
                pending_git_commit_recovery(&workspace, &state.0).unwrap(),
                None
            );
            assert_eq!(
                resume_pending_git_commit_publication(&workspace, &state.0, None).unwrap_err(),
                GitCommitPlanError::RecoveryNotRequired
            );
            drop(plan);
        }
    }

    #[test]
    fn commit_recovery_preserves_foreign_index_and_post_ref_state() {
        for (fault, replace_index) in [
            (GitCommitMetadataFault::IndexPublished, true),
            (GitCommitMetadataFault::RefPublished, false),
        ] {
            let temp = Temp::new();
            drop(initialized_workspace(&temp));
            configure_fixture_identity(&temp);
            fixture_git(&temp, &["add", "--", "rehow.yaml"]);
            fixture_git(&temp, &["commit", "-qm", "base"]);
            let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
                .unwrap()
                .replace("name: Git", "name: Foreign recovery state");
            fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
            let workspace = open_workspace(&temp);
            let state = Temp::new();
            let mut plan = plan_git_commit(&workspace, &state.0, "Interrupted commit").unwrap();
            set_git_commit_metadata_fault(fault);
            assert_eq!(
                publish_git_commit_with_intent(&workspace, &state.0, None, &mut plan).unwrap_err(),
                GitCommitPlanError::UnsupportedRepository
            );
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            let intent = session.list_intents().unwrap().remove(0);
            drop(session);

            let foreign = if replace_index {
                b"foreign index bytes".as_slice()
            } else {
                b"1111111111111111111111111111111111111111\n".as_slice()
            };
            let target = if replace_index {
                temp.0.join(".git/index")
            } else {
                temp.0.join(".git/refs/heads/main")
            };
            fs::write(&target, foreign).unwrap();
            assert_eq!(
                resume_git_commit_publication(&workspace, &state.0, None, intent.operation_id)
                    .unwrap_err(),
                GitCommitPlanError::PlanStale
            );
            assert_eq!(fs::read(target).unwrap(), foreign);
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            assert_eq!(
                session.read_intent(intent.operation_id).unwrap().stage,
                GitCommitIntentStage::Conflict
            );
            drop(session);
            assert_eq!(
                pending_git_commit_recovery(&workspace, &state.0).unwrap(),
                None
            );
            assert!(
                state
                    .0
                    .join("rehow/commit-quarantine")
                    .join(&intent.staging_name)
                    .is_dir()
            );
            drop(plan);
        }
    }

    #[test]
    fn commit_publication_supports_unborn_packed_and_sha256_branches() {
        let unborn = Temp::new();
        drop(initialized_workspace(&unborn));
        configure_fixture_identity(&unborn);
        let unborn_workspace = open_workspace(&unborn);
        let state = Temp::new();
        let mut plan =
            plan_git_commit(&unborn_workspace, &state.0, "Initial managed commit").unwrap();
        let unborn_commit = plan.candidate.commit_object_id.clone();
        assert!(
            !publish_git_commit_with_intent(&unborn_workspace, &state.0, None, &mut plan)
                .unwrap()
                .workspace_changed
        );
        assert_eq!(
            fixture_git_stdout(&unborn, &["rev-parse", "HEAD"]),
            unborn_commit
        );
        assert!(!unborn.0.join(".git/logs").exists());

        let packed = Temp::new();
        drop(initialized_workspace(&packed));
        configure_fixture_identity(&packed);
        fixture_git(&packed, &["add", "--", "rehow.yaml"]);
        fixture_git(&packed, &["commit", "-qm", "base"]);
        fixture_git(&packed, &["pack-refs", "--all", "--prune"]);
        assert!(!packed.0.join(".git/refs/heads/main").exists());
        let packed_refs_before = fs::read(packed.0.join(".git/packed-refs")).unwrap();
        let changed = fs::read_to_string(packed.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Packed branch");
        fs::write(packed.0.join("rehow.yaml"), changed).unwrap();
        let packed_workspace = open_workspace(&packed);
        let state = Temp::new();
        let mut plan =
            plan_git_commit(&packed_workspace, &state.0, "Shadow packed branch").unwrap();
        let packed_commit = plan.candidate.commit_object_id.clone();
        set_git_commit_metadata_fault(GitCommitMetadataFault::IndexPublished);
        assert_eq!(
            publish_git_commit_with_intent(&packed_workspace, &state.0, None, &mut plan)
                .unwrap_err(),
            GitCommitPlanError::UnsupportedRepository
        );
        let session = packed_workspace
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        let operation_id = session.list_intents().unwrap()[0].operation_id;
        drop(session);
        assert!(
            !resume_git_commit_publication(&packed_workspace, &state.0, None, operation_id)
                .unwrap()
        );
        assert_eq!(
            fixture_git_stdout(&packed, &["rev-parse", "HEAD"]),
            packed_commit
        );
        assert_eq!(
            fs::read(packed.0.join(".git/packed-refs")).unwrap(),
            packed_refs_before
        );

        let sha256 = Temp::new();
        drop(workspace(&sha256));
        fixture_git(
            &sha256,
            &[
                "init",
                "-q",
                "--object-format=sha256",
                "--initial-branch=main",
            ],
        );
        configure_fixture_identity(&sha256);
        fixture_git(&sha256, &["add", "--", "rehow.yaml"]);
        fixture_git(&sha256, &["commit", "-qm", "base"]);
        let changed = fs::read_to_string(sha256.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: SHA-256 publication");
        fs::write(sha256.0.join("rehow.yaml"), changed).unwrap();
        let sha256_workspace = open_workspace(&sha256);
        let state = Temp::new();
        let mut plan =
            plan_git_commit(&sha256_workspace, &state.0, "Publish SHA-256 commit").unwrap();
        let sha256_commit = plan.candidate.commit_object_id.clone();
        assert!(
            !publish_git_commit_with_intent(&sha256_workspace, &state.0, None, &mut plan)
                .unwrap()
                .workspace_changed
        );
        assert_eq!(sha256_commit.len(), 64);
        assert_eq!(
            fixture_git_stdout(&sha256, &["rev-parse", "HEAD"]),
            sha256_commit
        );
    }

    #[test]
    fn commit_candidate_pack_refuses_stale_workspace_before_active_install() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Planned pack");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let mut plan = plan_git_commit(&workspace, &state.0, "Stale candidate").unwrap();
        prepare_git_commit_pack(&mut plan).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(temp.0.join("rehow.yaml"))
            .unwrap()
            .write_all(b"\n# changed after planning\n")
            .unwrap();

        assert_eq!(
            install_git_commit_candidate_pack(&workspace, &state.0, None, &mut plan).unwrap_err(),
            GitCommitPlanError::PlanStale
        );
        assert_eq!(
            fs::read_dir(temp.0.join(".git/objects/pack"))
                .unwrap()
                .count(),
            0
        );
        cancel_git_commit_plan(plan).unwrap();
    }

    #[test]
    fn commit_candidate_pack_refuses_a_mismatched_active_pack_collision() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Collision candidate");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let mut plan = plan_git_commit(&workspace, &state.0, "Collision candidate").unwrap();
        let packed = prepare_git_commit_pack(&mut plan).unwrap();
        let pack_source = packed
            .sources
            .iter()
            .find(|source| source.name.ends_with(".pack"))
            .unwrap();
        let collision_path = temp.0.join(".git/objects/pack").join(&pack_source.name);
        let collision = vec![0x5a; usize::try_from(pack_source.size).unwrap()];
        fs::write(&collision_path, &collision).unwrap();
        let mut files = packed
            .sources
            .iter()
            .map(|source| {
                plan.candidate
                    .prepared
                    .staging
                    .open_pack_source(source)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            plan.preflight
                .capability
                .install_commit_pack_sources(&packed.sources, &mut files, 40,),
            Err(GitCommitPackInstallError::Conflict)
        );
        assert_eq!(fs::read(collision_path).unwrap(), collision);
        assert_eq!(
            fs::read_dir(temp.0.join(".git/objects/pack"))
                .unwrap()
                .count(),
            1
        );
        cancel_git_commit_plan(plan).unwrap();
    }

    #[test]
    fn commit_plan_exposes_only_bounded_summary_and_is_redacted() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fs::write(temp.0.join("other.txt"), b"tracked before\n").unwrap();
        fixture_git(&temp, &["add", "--", "rehow.yaml", "other.txt"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        fs::write(temp.0.join("other.txt"), b"tracked after\n").unwrap();
        fs::write(temp.0.join("untracked.txt"), b"untracked\n").unwrap();
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Planned");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let plan = plan_git_commit(&workspace, &state.0, "  Update local library  ").unwrap();
        assert_eq!(plan.branch(), "main");
        assert_eq!((plan.created(), plan.updated(), plan.deleted()), (0, 1, 0));
        assert_eq!(plan.unrelated_changes(), 2);
        assert_eq!(format!("{:?}", plan.token()), "PlanToken(<redacted>)");
        let debug = format!("{plan:?}");
        assert!(!debug.contains("Update local library"));
        assert!(!debug.contains("rehow.yaml"));
        assert!(!debug.contains(&plan.candidate.tree_object_id));
        assert!(!debug.contains(&plan.candidate.commit_object_id));
        assert!(!debug.contains("rehow@example.invalid"));
        cancel_git_commit_plan(plan).unwrap();
        assert_eq!(
            fs::read_dir(state.0.join("rehow/commit-staging"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            fs::read_dir(state.0.join("rehow/commit-quarantine"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn commit_plan_honors_the_shared_overall_deadline() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        ACTIVE_COMMIT_DEADLINE.with(|active| active.set(Some(Instant::now())));
        let result = plan_git_commit(&workspace, &state.0, "Bounded plan");
        ACTIVE_COMMIT_DEADLINE.with(|active| active.set(None));
        assert!(matches!(result, Err(GitCommitPlanError::ResourceLimit)));
    }

    #[test]
    fn commit_plan_ignores_hooks_filters_attributes_and_signing_configuration() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let canary = temp.0.join("must-not-run");
        let hook = temp.0.join(".git/hooks/pre-commit");
        fs::create_dir_all(hook.parent().unwrap()).unwrap();
        fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", canary.display())).unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
        fixture_git(
            &temp,
            &[
                "config",
                "--local",
                "filter.evil.clean",
                &format!("/usr/bin/touch {}", canary.display()),
            ],
        );
        fixture_git(&temp, &["config", "--local", "commit.gpgSign", "true"]);
        fs::write(temp.0.join(".gitattributes"), b"rehow.yaml filter=evil\n").unwrap();
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Isolated");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let plan = plan_git_commit(&workspace, &state.0, "Isolated candidate").unwrap();
        assert_eq!(plan.updated(), 1);
        assert!(!canary.exists());
        cancel_git_commit_plan(plan).unwrap();
    }

    #[test]
    fn commit_plan_supports_a_direct_packed_branch() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        fixture_git(&temp, &["pack-refs", "--all"]);
        assert!(!temp.0.join(".git/refs/heads/main").exists());
        let changed = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Packed");
        fs::write(temp.0.join("rehow.yaml"), changed).unwrap();
        let workspace = open_workspace(&temp);
        let state = Temp::new();
        let plan = plan_git_commit(&workspace, &state.0, "Packed branch candidate").unwrap();
        assert_eq!(plan.branch(), "main");
        assert_eq!(
            plan.preflight.branch.representation,
            GitBranchRepresentation::Packed
        );
        cancel_git_commit_plan(plan).unwrap();
    }

    #[test]
    fn commit_private_write_tree_refuses_semantically_dirty_copied_index() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "fixture"]);
        let clean_index = fs::read(temp.0.join(".git/index")).unwrap();
        let clean_snapshot = parse_git_index(&clean_index, GitObjectFormat::Sha1).unwrap();
        let clean_payload_end = clean_index.len() - 20;
        let cache_tree = clean_index[clean_snapshot.entries_end..clean_payload_end].to_vec();
        assert!(clean_snapshot.has_cache_tree);
        fs::write(temp.0.join("unrelated.txt"), b"staged only\n").unwrap();
        fixture_git(&temp, &["add", "--", "unrelated.txt"]);
        let dirty_index = fs::read(temp.0.join(".git/index")).unwrap();
        let dirty_snapshot = parse_git_index(&dirty_index, GitObjectFormat::Sha1).unwrap();
        let mut forged = dirty_index[..dirty_snapshot.entries_end].to_vec();
        forged.extend_from_slice(&cache_tree);
        forged.extend_from_slice(&git_index_checksum(&forged, GitObjectFormat::Sha1));
        fs::write(temp.0.join(".git/index"), forged).unwrap();
        let workspace = open_workspace(&temp);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        assert!(preflight.index.as_ref().unwrap().has_cache_tree);
        let state = Temp::new();
        assert!(matches!(
            prepare_git_commit_staging(&preflight, &state.0),
            Err(GitCommitPlanError::UnsupportedIndex)
        ));
        assert_eq!(
            fs::read_dir(state.0.join("rehow/commit-staging"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn commit_repository_preflight_rehashes_preserved_loose_object_content() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        let manifest = fs::read(temp.0.join("rehow.yaml")).unwrap();
        fs::write(temp.0.join("unrelated.bin"), vec![b'x'; manifest.len()]).unwrap();
        fixture_git(&temp, &["add", "--", "rehow.yaml", "unrelated.bin"]);
        fixture_git(&temp, &["commit", "-qm", "fixture"]);
        let managed = fixture_git_stdout(&temp, &["rev-parse", "HEAD:rehow.yaml"]);
        let unrelated = fixture_git_stdout(&temp, &["rev-parse", "HEAD:unrelated.bin"]);
        assert_ne!(managed, unrelated);
        let loose = |object_id: &str| {
            temp.0
                .join(".git/objects")
                .join(&object_id[..2])
                .join(&object_id[2..])
        };
        fs::remove_file(loose(&managed)).unwrap();
        fs::copy(loose(&unrelated), loose(&managed)).unwrap();
        let workspace = open_workspace(&temp);
        assert!(matches!(
            inspect_git_commit_repository(&workspace),
            Err(GitCommitPlanError::UnsafeRepository)
        ));
    }

    #[test]
    fn commit_prior_workspace_validation_refuses_malformed_or_replaced_identity() {
        let malformed = Temp::new();
        fixture_git(&malformed, &["init", "-q", "--initial-branch=main"]);
        configure_fixture_identity(&malformed);
        fs::write(malformed.0.join("rehow.yaml"), b"schema_version: [\n").unwrap();
        fixture_git(&malformed, &["add", "--", "rehow.yaml"]);
        fixture_git(&malformed, &["commit", "-qm", "malformed prior"]);
        fs::create_dir(malformed.0.join("categories")).unwrap();
        fs::write(malformed.0.join("rehow.yaml"), "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Git\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n").unwrap();
        let malformed_workspace = open_workspace(&malformed);
        let preflight = inspect_git_commit_repository(&malformed_workspace).unwrap();
        let state = Temp::new();
        assert!(matches!(
            prepare_git_commit_staging(&preflight, &state.0),
            Err(GitCommitPlanError::AmbiguousManagedHistory)
        ));

        let replaced = Temp::new();
        fixture_git(&replaced, &["init", "-q", "--initial-branch=main"]);
        configure_fixture_identity(&replaced);
        drop(workspace(&replaced));
        fixture_git(&replaced, &["add", "--", "rehow.yaml"]);
        fixture_git(&replaced, &["commit", "-qm", "valid prior"]);
        let changed = fs::read_to_string(replaced.0.join("rehow.yaml"))
            .unwrap()
            .replace(
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e10",
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e11",
            );
        fs::write(replaced.0.join("rehow.yaml"), changed).unwrap();
        let replaced_workspace = open_workspace(&replaced);
        let preflight = inspect_git_commit_repository(&replaced_workspace).unwrap();
        let state = Temp::new();
        assert!(matches!(
            prepare_git_commit_staging(&preflight, &state.0),
            Err(GitCommitPlanError::AmbiguousManagedHistory)
        ));
    }

    #[test]
    fn commit_repository_reads_ignore_replace_refs() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "base"]);
        let base = fs::read_to_string(temp.0.join(".git/refs/heads/main"))
            .unwrap()
            .trim()
            .to_owned();
        let replaced = fs::read_to_string(temp.0.join("rehow.yaml"))
            .unwrap()
            .replace(
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e10",
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e11",
            );
        fs::write(temp.0.join("rehow.yaml"), replaced).unwrap();
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "replacement"]);
        let replacement = fs::read_to_string(temp.0.join(".git/refs/heads/main"))
            .unwrap()
            .trim()
            .to_owned();
        fixture_git(&temp, &["reset", "--hard", "-q", &base]);
        fixture_git(&temp, &["replace", &base, &replacement]);
        let workspace = open_workspace(&temp);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        let state = Temp::new();
        let prepared = prepare_git_commit_staging(&preflight, &state.0).unwrap();
        assert_eq!(prepared.prior_managed.files.len(), 1);
    }

    #[test]
    fn commit_repository_preflight_rejects_stale_or_executable_managed_content() {
        let stale = Temp::new();
        drop(initialized_workspace(&stale));
        configure_fixture_identity(&stale);
        let workspace = open_workspace(&stale);
        let changed = fs::read_to_string(stale.0.join("rehow.yaml"))
            .unwrap()
            .replace("name: Git", "name: Changed");
        fs::write(stale.0.join("rehow.yaml"), changed).unwrap();
        assert_eq!(
            inspect_git_commit_repository(&workspace).unwrap_err(),
            GitCommitPlanError::PlanStale
        );

        let executable = Temp::new();
        drop(initialized_workspace(&executable));
        configure_fixture_identity(&executable);
        let workspace = open_workspace(&executable);
        let mut mode = fs::metadata(executable.0.join("rehow.yaml"))
            .unwrap()
            .permissions();
        mode.set_mode(0o700);
        fs::set_permissions(executable.0.join("rehow.yaml"), mode).unwrap();
        assert_eq!(
            inspect_git_commit_repository(&workspace).unwrap_err(),
            GitCommitPlanError::WorkspaceIssues
        );
    }

    #[test]
    fn commit_repository_preflight_rejects_unrelated_tree_ownership_collision() {
        const CATEGORY: &str = "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux System\norder: 1000\nstatus: active\ncreated_at: \"2026-09-05T10:01:00Z\"\nupdated_at: \"2026-09-05T10:01:00Z\"\n";
        let temp = Temp::new();
        fixture_git(&temp, &["init", "-q", "--initial-branch=main"]);
        configure_fixture_identity(&temp);
        fs::create_dir_all(temp.0.join("categories/unrelated")).unwrap();
        fs::write(temp.0.join("categories/unrelated/file"), b"unrelated").unwrap();
        fixture_git(&temp, &["add", "--", "categories/unrelated/file"]);
        fixture_git(&temp, &["commit", "-qm", "unrelated tree"]);
        fs::remove_dir_all(temp.0.join("categories")).unwrap();
        fs::write(temp.0.join("rehow.yaml"), "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Git\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n").unwrap();
        let category = temp.0.join("categories/linux-system--b6ba3a5551");
        fs::create_dir_all(&category).unwrap();
        fs::write(category.join("category.yaml"), CATEGORY).unwrap();
        let workspace = open_workspace(&temp);
        assert_eq!(
            inspect_git_commit_repository(&workspace).unwrap_err(),
            GitCommitPlanError::PathCollision
        );
    }

    #[test]
    fn clone_tree_accepts_only_bounded_canonical_workspace_files() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let blob = "89abcdef0123456789abcdef0123456789abcdef";
        let listing = clone_tree_listing(&[
            ("rehow.yaml", blob, 160),
            ("categories/linux--f936c5735e10/category.yaml", blob, 220),
            (
                "categories/linux--f936c5735e10/install--36c5735e10.md",
                blob,
                1_024,
            ),
            (
                "attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.desktop",
                blob,
                512,
            ),
            (".gitignore", blob, 16),
        ]);
        let tree = validate_git_clone_tree(commit, &listing).unwrap();
        assert_eq!(tree.commit_id(), commit);
        assert_eq!(tree.object_format(), GitObjectFormat::Sha1);
        assert_eq!(tree.entries().len(), 5);
        assert_eq!(tree.entries()[0].relative_path(), ".gitignore");
        assert_eq!(tree.entries()[0].object_id(), blob);
        assert_eq!(tree.entries()[0].size(), 16);
        assert_eq!(tree.entries()[0].kind(), GitMaterializationPathKind::Ignore);
        assert_ne!(tree.plan_hash(), [0; 32]);

        let sha256_commit = "a".repeat(64);
        let sha256_blob = "b".repeat(64);
        let sha256_listing = clone_tree_listing(&[("rehow.yaml", &sha256_blob, 1)]);
        assert_eq!(
            validate_git_clone_tree(&sha256_commit, &sha256_listing)
                .unwrap()
                .object_format(),
            GitObjectFormat::Sha256
        );
    }

    #[test]
    fn clone_tree_rejects_hostile_types_paths_and_framing() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let blob = "89abcdef0123456789abcdef0123456789abcdef";
        for record in [
            format!("100755 blob {blob} 1\trehow.yaml\0"),
            format!("120000 blob {blob} 1\trehow.yaml\0"),
            format!("160000 commit {blob} 1\trehow.yaml\0"),
        ] {
            assert_eq!(
                validate_git_clone_tree(commit, record.as_bytes()),
                Err(GitCloneTreeError::UnsupportedObject)
            );
        }
        for path in [
            ".git/config",
            "README.md",
            "categories/../rehow.yaml",
            "categories/linux/guide.txt",
            "attachments/not-a-guide-id/file",
            "categories/linux/line\nbreak.md",
        ] {
            let listing = clone_tree_listing(&[(path, blob, 1)]);
            assert_eq!(
                validate_git_clone_tree(commit, &listing),
                Err(GitCloneTreeError::UnsafePath),
                "{path:?}"
            );
        }
        let missing_nul = format!("100644 blob {blob} 1\trehow.yaml");
        assert_eq!(
            validate_git_clone_tree(commit, missing_nul.as_bytes()),
            Err(GitCloneTreeError::InvalidOutput)
        );
        let duplicate =
            format!("100644 blob {blob} 1\trehow.yaml\0100644 blob {blob} 1\trehow.yaml\0");
        assert_eq!(
            validate_git_clone_tree(commit, duplicate.as_bytes()),
            Err(GitCloneTreeError::InvalidOutput)
        );
        let unsorted =
            format!("100644 blob {blob} 1\trehow.yaml\0100644 blob {blob} 1\t.gitignore\0");
        assert_eq!(
            validate_git_clone_tree(commit, unsorted.as_bytes()),
            Err(GitCloneTreeError::InvalidOutput)
        );
    }

    #[test]
    fn clone_tree_enforces_object_ids_sizes_counts_and_totals() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let blob = "89abcdef0123456789abcdef0123456789abcdef";
        for bad_commit in ["", "ABCDEF", &"g".repeat(40), &"a".repeat(41)] {
            assert_eq!(
                validate_git_clone_tree(bad_commit, &[]),
                Err(GitCloneTreeError::InvalidCommit)
            );
        }
        let missing_manifest = clone_tree_listing(&[(".gitignore", blob, 1)]);
        assert_eq!(
            validate_git_clone_tree(commit, &missing_manifest),
            Err(GitCloneTreeError::MissingManifest)
        );
        let wrong_object = clone_tree_listing(&[("rehow.yaml", &"a".repeat(64), 1)]);
        assert_eq!(
            validate_git_clone_tree(commit, &wrong_object),
            Err(GitCloneTreeError::UnsupportedObject)
        );
        for size in [262_145, u64::MAX] {
            let oversized = clone_tree_listing(&[("rehow.yaml", blob, size)]);
            assert_eq!(
                validate_git_clone_tree(commit, &oversized),
                Err(GitCloneTreeError::ResourceLimit)
            );
        }
        let leading_zero = format!("100644 blob {blob} 01\trehow.yaml\0");
        assert_eq!(
            validate_git_clone_tree(commit, leading_zero.as_bytes()),
            Err(GitCloneTreeError::InvalidOutput)
        );

        let guide_size = 2 * 1024 * 1024u64;
        let mut entries = vec![("rehow.yaml".to_owned(), blob.to_owned(), 1)];
        for index in 0..1_025 {
            entries.push((
                format!("categories/linux/guide-{index:04}.md"),
                blob.to_owned(),
                guide_size,
            ));
        }
        let borrowed = entries
            .iter()
            .map(|(path, oid, size)| (path.as_str(), oid.as_str(), *size))
            .collect::<Vec<_>>();
        assert_eq!(
            validate_git_clone_tree(commit, &clone_tree_listing(&borrowed)),
            Err(GitCloneTreeError::ResourceLimit)
        );
    }

    #[test]
    fn clone_tree_enforces_exact_and_one_over_collection_envelopes() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let blob = "89abcdef0123456789abcdef0123456789abcdef";

        let mut managed_exact = vec![("rehow.yaml".to_owned(), blob.to_owned(), 0)];
        for index in 0..1_024 {
            managed_exact.push((
                format!("categories/linux/guide-{index:04}.md"),
                blob.to_owned(),
                2 * 1024 * 1024,
            ));
        }
        let borrowed = managed_exact
            .iter()
            .map(|(path, oid, size)| (path.as_str(), oid.as_str(), *size))
            .collect::<Vec<_>>();
        assert!(validate_git_clone_tree(commit, &clone_tree_listing(&borrowed)).is_ok());
        managed_exact[0].2 = 1;
        let borrowed = managed_exact
            .iter()
            .map(|(path, oid, size)| (path.as_str(), oid.as_str(), *size))
            .collect::<Vec<_>>();
        assert_eq!(
            validate_git_clone_tree(commit, &clone_tree_listing(&borrowed)),
            Err(GitCloneTreeError::ResourceLimit)
        );

        let mut attachment_exact = vec![("rehow.yaml".to_owned(), blob.to_owned(), 1)];
        for index in 0..64 {
            attachment_exact.push((
                format!("attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/file-{index:02}"),
                blob.to_owned(),
                64 * 1024 * 1024,
            ));
        }
        let borrowed = attachment_exact
            .iter()
            .map(|(path, oid, size)| (path.as_str(), oid.as_str(), *size))
            .collect::<Vec<_>>();
        assert!(validate_git_clone_tree(commit, &clone_tree_listing(&borrowed)).is_ok());
        attachment_exact.push((
            "attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/one-over".to_owned(),
            blob.to_owned(),
            1,
        ));
        let borrowed = attachment_exact
            .iter()
            .map(|(path, oid, size)| (path.as_str(), oid.as_str(), *size))
            .collect::<Vec<_>>();
        assert_eq!(
            validate_git_clone_tree(commit, &clone_tree_listing(&borrowed)),
            Err(GitCloneTreeError::ResourceLimit)
        );

        let over_listing = vec![b'x'; MAX_CLONE_TREE_LISTING_BYTES + 1];
        assert_eq!(
            validate_git_clone_tree(commit, &over_listing),
            Err(GitCloneTreeError::ResourceLimit)
        );
    }

    #[test]
    fn clone_tree_accepts_root_git_ls_tree_long_output() {
        let temp = Temp::new();
        fs::create_dir(temp.0.join("categories")).unwrap();
        fs::write(temp.0.join("rehow.yaml"), "schema_version: 1\n").unwrap();
        fs::write(temp.0.join(".gitignore"), ".rehow/\n").unwrap();

        let run = |arguments: &[&str]| {
            Command::new("/usr/bin/git")
                .env_clear()
                .env("LANG", "C")
                .env("LC_ALL", "C")
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(["-C", temp.0.to_str().unwrap()])
                .args([
                    "--no-pager",
                    "--literal-pathspecs",
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "core.fsmonitor=false",
                ])
                .args(arguments)
                .output()
                .unwrap()
        };
        assert!(
            Command::new("/usr/bin/git")
                .env_clear()
                .env("LANG", "C")
                .env("LC_ALL", "C")
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(["init", "--quiet", "--template="])
                .arg(&temp.0)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            run(&["add", "--", ".gitignore", "rehow.yaml"])
                .status
                .success()
        );
        let tree = run(&["write-tree"]);
        assert!(tree.status.success());
        let tree_id = std::str::from_utf8(&tree.stdout).unwrap().trim();
        let commit = Command::new("/usr/bin/git")
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Rehow Test")
            .env("GIT_AUTHOR_EMAIL", "test@rehow.invalid")
            .env("GIT_COMMITTER_NAME", "Rehow Test")
            .env("GIT_COMMITTER_EMAIL", "test@rehow.invalid")
            .args(["-C", temp.0.to_str().unwrap(), "commit-tree", tree_id])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(commit.status.success());
        let commit_id = std::str::from_utf8(&commit.stdout).unwrap().trim();
        let listing = run(&["ls-tree", "-r", "-l", "-z", "--full-tree", commit_id]);
        assert!(listing.status.success());
        let validated = validate_git_clone_tree(commit_id, &listing.stdout).unwrap();
        assert_eq!(validated.entries().len(), 2);
    }

    #[test]
    fn clone_tree_debug_redacts_paths_object_ids_and_hashes() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let blob = "89abcdef0123456789abcdef0123456789abcdef";
        let listing = clone_tree_listing(&[("rehow.yaml", blob, 1)]);
        let debug = format!("{:?}", validate_git_clone_tree(commit, &listing).unwrap());
        assert!(!debug.contains(commit));
        assert!(!debug.contains(blob));
        assert!(!debug.contains("rehow.yaml"));
        assert!(debug.contains("entries: 1"));
    }

    #[test]
    fn clone_plan_binds_an_unchanged_empty_destination_and_redacts_authority() {
        let temp = Temp::new();
        let target = empty_target(&temp);
        let url = "https://github.com/example/rehow.git";
        let plan = plan_git_clone(&target, url).unwrap();
        assert_eq!(plan.remote_url(), url);
        assert_eq!(plan.transport(), GitRemoteTransport::Https);
        assert_eq!(plan.host(), "github.com");
        revalidate_git_clone_plan(&target, &plan).unwrap();

        let debug = format!("{plan:?}");
        assert!(!debug.contains(url));
        let token = serde_json::to_string(plan.token()).unwrap();
        assert!(!debug.contains(token.trim_matches('"')));
        assert!(debug.contains("destination: \"<redacted>\""));

        fs::write(temp.0.join("appeared-after-plan"), b"external").unwrap();
        assert_eq!(
            revalidate_git_clone_plan(&target, &plan),
            Err(GitCloneError::DestinationChanged)
        );
    }

    #[test]
    fn clone_plan_refuses_invalid_urls_and_nested_repositories() {
        let temp = Temp::new();
        let target = empty_target(&temp);
        assert!(matches!(
            plan_git_clone(&target, "https://user:secret@example.com/rehow.git"),
            Err(GitCloneError::InvalidUrl)
        ));

        let parent = Temp::new();
        fs::create_dir(parent.0.join(".git")).unwrap();
        fs::create_dir(parent.0.join("destination")).unwrap();
        let (_, inspection) = inspect_workspace(&parent.0.join("destination"))
            .unwrap()
            .into_parts();
        let WorkspaceInspection::Empty(nested) = inspection else {
            panic!("expected empty nested destination")
        };
        assert!(matches!(
            plan_git_clone(&nested, "git@example.com:team/rehow.git"),
            Err(GitCloneError::NestedRepository)
        ));
    }

    #[test]
    fn clone_apply_orchestration_rejects_transport_cancel_and_destination_drift() {
        let ssh_destination = Temp::new();
        let ssh_target = empty_target(&ssh_destination);
        let ssh_plan = plan_git_clone(&ssh_target, "git@example.invalid:team/rehow.git").unwrap();
        assert_eq!(
            apply_git_clone_validation(
                &ssh_target,
                ssh_plan,
                &Temp::new().0,
                &AtomicBool::new(false)
            )
            .err()
            .unwrap(),
            GitCloneApplyError::UnsupportedTransport
        );

        let cancelled_destination = Temp::new();
        let cancelled_target = empty_target(&cancelled_destination);
        let cancelled_plan =
            plan_git_clone(&cancelled_target, "https://example.invalid/team/rehow.git").unwrap();
        assert_eq!(
            apply_git_clone_validation(
                &cancelled_target,
                cancelled_plan,
                &Temp::new().0,
                &AtomicBool::new(true)
            )
            .err()
            .unwrap(),
            GitCloneApplyError::Cancelled
        );
        assert!(cancelled_destination.0.read_dir().unwrap().next().is_none());

        let changed_destination = Temp::new();
        let changed_target = empty_target(&changed_destination);
        let changed_plan =
            plan_git_clone(&changed_target, "https://example.invalid/team/rehow.git").unwrap();
        fs::write(changed_destination.0.join("external"), b"preserve").unwrap();
        assert_eq!(
            apply_git_clone_validation(
                &changed_target,
                changed_plan,
                &Temp::new().0,
                &AtomicBool::new(false)
            )
            .err()
            .unwrap(),
            GitCloneApplyError::DestinationChanged
        );
        assert_eq!(
            fs::read(changed_destination.0.join("external")).unwrap(),
            b"preserve"
        );
    }

    #[test]
    fn clone_remote_head_parser_is_exact_bounded_and_redacted() {
        let sha1 = "0123456789abcdef0123456789abcdef01234567";
        let parsed =
            parse_git_remote_head(format!("ref: refs/heads/main\tHEAD\n{sha1}\tHEAD\n").as_bytes())
                .unwrap();
        assert_eq!(parsed.branch_ref(), "refs/heads/main");
        assert_eq!(parsed.object_id(), sha1);
        assert_eq!(parsed.object_format(), GitObjectFormat::Sha1);
        let debug = format!("{parsed:?}");
        assert!(!debug.contains("refs/heads/main"));
        assert!(!debug.contains(sha1));

        let sha256 = "a".repeat(64);
        assert_eq!(
            parse_git_remote_head(
                format!("ref: refs/heads/team/rehow\tHEAD\n{sha256}\tHEAD\n").as_bytes()
            )
            .unwrap()
            .object_format(),
            GitObjectFormat::Sha256
        );

        for hostile in [
            String::new(),
            format!("ref: refs/tags/main\tHEAD\n{sha1}\tHEAD\n"),
            format!("ref: refs/heads/../main\tHEAD\n{sha1}\tHEAD\n"),
            format!("ref: refs/heads/main\tHEAD\n{}\tHEAD\n", "A".repeat(40)),
            format!("ref: refs/heads/main\tHEAD\n{sha1}\tOTHER\n"),
            format!("ref: refs/heads/main\tHEAD\n{sha1}\tHEAD"),
            format!("ref: refs/heads/main\tHEAD\n{sha1}\tHEAD\nextra\n"),
        ] {
            assert_eq!(
                parse_git_remote_head(hostile.as_bytes()),
                Err(GitCloneDiscoveryError::InvalidAdvertisement),
                "{hostile:?}"
            );
        }
    }

    #[test]
    fn clone_blob_ids_and_batch_headers_are_exact() {
        assert!(git_blob_object_id_matches(
            GitObjectFormat::Sha1,
            b"",
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        ));
        assert!(!git_blob_object_id_matches(
            GitObjectFormat::Sha1,
            b"changed",
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        ));
        let mut valid = std::io::Cursor::new(b"abc blob 3\nbody\n");
        assert_eq!(
            read_clone_batch_header(&mut valid).unwrap(),
            b"abc blob 3\n"
        );
        let mut unterminated = std::io::Cursor::new(vec![b'x'; CLONE_BATCH_HEADER_LIMIT]);
        assert_eq!(
            read_clone_batch_header(&mut unterminated),
            Err(GitCloneContentError::BlobMismatch)
        );
    }

    #[test]
    fn clone_runner_cancels_process_groups_and_caps_output() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut slow = Command::new("/usr/bin/sh");
        slow.args(["-c", "sleep 30 & wait"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        slow.process_group(0);
        let trigger = Arc::clone(&cancelled);
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            trigger.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        assert_eq!(
            run_clone_command(slow, &cancelled, Duration::from_secs(2), 1024)
                .err()
                .unwrap(),
            GitCloneDiscoveryError::Cancelled
        );
        canceller.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));

        let mut noisy = Command::new("/usr/bin/head");
        noisy
            .args(["-c", "8192", "/dev/zero"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        noisy.process_group(0);
        assert_eq!(
            run_clone_command(noisy, &AtomicBool::new(false), Duration::from_secs(2), 1024)
                .err()
                .unwrap(),
            GitCloneDiscoveryError::OutputLimit
        );

        let exit_two = || {
            let mut command = Command::new("/usr/bin/sh");
            command
                .args(["-c", "exit 2"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command
        };
        assert_eq!(
            run_clone_command_for_transport_with_missing_ref(
                exit_two(),
                &AtomicBool::new(false),
                Duration::from_secs(2),
                1024,
                GitRemoteTransport::Https,
                GitRemoteAuthentication::Anonymous,
            )
            .err()
            .unwrap(),
            GitCloneDiscoveryError::RemoteRefMissing
        );
        assert_eq!(
            run_clone_command(
                exit_two(),
                &AtomicBool::new(false),
                Duration::from_secs(2),
                1024,
            )
            .err()
            .unwrap(),
            GitCloneDiscoveryError::RemoteUnavailable
        );

        for (message, expected) in [
            (
                "Permission denied (publickey).",
                GitCloneDiscoveryError::AuthenticationFailed,
            ),
            (
                "Host key verification failed.",
                GitCloneDiscoveryError::HostKeyRejected,
            ),
        ] {
            let mut command = Command::new("/usr/bin/sh");
            command
                .args(["-c", "printf '%s\\n' \"$1\" >&2; exit 255", "sh", message])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            assert_eq!(
                run_clone_command_for_transport(
                    command,
                    &AtomicBool::new(false),
                    Duration::from_secs(2),
                    1024,
                    GitRemoteTransport::Ssh,
                    GitRemoteAuthentication::Anonymous,
                )
                .err()
                .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn clone_discovery_command_is_config_free_checkout_free_and_url_explicit() {
        let url = "https://example.invalid/team/rehow.git";
        let command =
            clone_discovery_command(Path::new("/usr/bin/git"), Path::new("/usr/bin/bwrap"), url);
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for required in [
            "--unshare-all",
            "--share-net",
            "credential.helper=",
            "credential.interactive=false",
            "core.hooksPath=/dev/null",
            "http.followRedirects=false",
            "protocol.allow=never",
            "protocol.https.allow=always",
            "protocol.ssh.allow=never",
            "ls-remote",
            "--symref",
            "--exit-code",
            url,
            "HEAD",
        ] {
            assert!(
                arguments.iter().any(|argument| argument == required),
                "{required}"
            );
        }
        for forbidden in ["clone", "checkout", "fetch", "--upload-pack", "--exec"] {
            assert!(!arguments.iter().any(|argument| argument == forbidden));
        }
        let environment = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(environment["GIT_CONFIG_NOSYSTEM"].as_deref(), Some("1"));
        assert_eq!(
            environment["GIT_CONFIG_GLOBAL"].as_deref(),
            Some("/dev/null")
        );
        assert_eq!(environment["GIT_TERMINAL_PROMPT"].as_deref(), Some("0"));
        assert_eq!(environment["HOME"].as_deref(), Some("/home/rehow"));
        assert_no_https_credential_boundary(&command);

        let ssh_url = "ssh://git@example.invalid:2222/team/rehow.git";
        let ssh = clone_discovery_command_with_transport(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            ssh_url,
            GitRemoteTransport::Ssh,
            GitRemoteAuthentication::Anonymous,
        );
        let ssh_arguments = ssh
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for required in [
            "protocol.https.allow=never",
            "protocol.ssh.allow=always",
            ssh_url,
        ] {
            assert!(ssh_arguments.iter().any(|argument| argument == required));
        }
        let ssh_environment = ssh
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            ssh_environment["GIT_SSH"].as_deref(),
            Some("/home/rehow/bin/rehow-git-ssh")
        );
        assert_eq!(ssh_environment["GIT_SSH_VARIANT"].as_deref(), Some("ssh"));
        assert_no_https_credential_boundary(&ssh);

        let sync = sync_discovery_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            url,
            "refs/heads/team/main",
            GitRemoteTransport::Https,
            GitRemoteAuthentication::Anonymous,
        );
        let sync_arguments = sync
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for required in [
            "credential.helper=",
            "credential.interactive=false",
            "protocol.allow=never",
            "protocol.https.allow=always",
            "protocol.ssh.allow=never",
            "ls-remote",
            "--exit-code",
            url,
            "refs/heads/team/main",
        ] {
            assert!(
                sync_arguments.iter().any(|argument| argument == required),
                "{required}"
            );
        }
        assert!(!sync_arguments.iter().any(|argument| argument == "--symref"));

        let ssh_sync = sync_discovery_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            "ssh://git@example.invalid/team/rehow.git",
            "refs/heads/team/main",
            GitRemoteTransport::Ssh,
            GitRemoteAuthentication::Anonymous,
        );
        let ssh_sync_arguments = ssh_sync
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let ssh_sync_environment = ssh_sync
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            ssh_sync_environment["GIT_SSH"].as_deref(),
            Some("/home/rehow/bin/rehow-git-ssh")
        );
        for required in ["protocol.https.allow=never", "protocol.ssh.allow=always"] {
            assert!(
                ssh_sync_arguments
                    .iter()
                    .any(|argument| argument == required),
                "{required}"
            );
        }

        let fetch = clone_fetch_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            "/proc/self/fd/99",
            99,
            url,
            "0123456789abcdef0123456789abcdef01234567",
        );
        let fetch_arguments = fetch
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for required in [
            "--bind",
            "/proc/self/fd/99",
            "/repo.git",
            "--git-dir=/repo.git",
            "fetch.fsckObjects=true",
            "transfer.fsckObjects=true",
            "fetch.unpackLimit=1",
            "transfer.unpackLimit=1",
            "pack.writeReverseIndex=false",
            "fetch.writeCommitGraph=false",
            "submodule.recurse=false",
            "fetch",
            "--depth=1",
            "--filter=blob:limit=67108865",
            "--no-tags",
            "--no-write-fetch-head",
            "--no-recurse-submodules",
            "--no-auto-maintenance",
            url,
            "0123456789abcdef0123456789abcdef01234567:refs/rehow/clone/head",
        ] {
            assert!(
                fetch_arguments.iter().any(|argument| argument == required),
                "{required}"
            );
        }
        for forbidden in ["clone", "checkout", "pull", "submodule"] {
            assert!(!fetch_arguments.iter().any(|argument| argument == forbidden));
        }

        let history_fetch = sync_history_fetch_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            "/proc/self/fd/99",
            99,
            url,
            "0123456789abcdef0123456789abcdef01234567",
            1024 * 1024,
            GitRemoteTransport::Https,
            GitRemoteAuthentication::Anonymous,
        );
        let history_arguments = history_fetch
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            history_arguments
                .iter()
                .any(|argument| argument == "--depth=4096")
        );
        assert!(
            !history_arguments
                .iter()
                .any(|argument| argument == "--depth=1")
        );
    }

    #[test]
    fn authenticated_https_discovery_fetch_and_history_use_only_fixed_credentials() {
        let resources = https_credentials::test_direct_resources();
        let url = "https://example.invalid:8443/team/rehow.git";
        let object_id = "0123456789abcdef0123456789abcdef01234567";
        let discovery = clone_discovery_command_with_transport(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            url,
            GitRemoteTransport::Https,
            GitRemoteAuthentication::Https(&resources),
        );
        assert_https_credential_boundary(&discovery, "example.invalid:8443");

        let fetch = clone_fetch_command_with_transport(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            "/proc/self/fd/99",
            99,
            url,
            object_id,
            GitRemoteTransport::Https,
            GitRemoteAuthentication::Https(&resources),
        );
        assert_https_credential_boundary(&fetch, "example.invalid:8443");
        let fetch_arguments = command_arguments(&fetch);
        assert!(
            fetch_arguments
                .iter()
                .any(|argument| argument == "--depth=1")
        );
        assert_eq!(
            fetch_arguments
                .windows(2)
                .filter(|arguments| *arguments == ["-c", "gc.auto=0"])
                .count(),
            1
        );

        let history = sync_history_fetch_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            "/proc/self/fd/99",
            99,
            url,
            object_id,
            1024 * 1024,
            GitRemoteTransport::Https,
            GitRemoteAuthentication::Https(&resources),
        );
        assert_https_credential_boundary(&history, "example.invalid:8443");
        assert!(
            command_arguments(&history)
                .iter()
                .any(|argument| argument == "--depth=4096")
        );
    }

    #[test]
    fn push_command_is_exact_config_free_read_only_and_never_forced() {
        let temp = Temp::new();
        let workspace = configured_sync_workspace(&temp);
        let capability = workspace.git_repository_capability().unwrap();
        let local_commit = fixture_git_stdout(&temp, &["rev-parse", "HEAD"]);
        let local_commit = local_commit.trim();
        let remote_url = "https://example.invalid/team/rehow.git";
        let remote_ref = "refs/heads/main";
        let (command, _config) = git_push_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            &capability,
            GitObjectFormat::Sha1,
            remote_url,
            local_commit,
            remote_ref,
            GitRemoteTransport::Https,
            GitRemoteAuthentication::Anonymous,
        )
        .unwrap();
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for required in [
            "--unshare-all",
            "--share-net",
            "--ro-bind",
            "/repo.git/objects",
            "/repo.git/refs",
            "/repo.git/HEAD",
            "credential.helper=",
            "credential.interactive=false",
            "core.hooksPath=/dev/null",
            "http.followRedirects=false",
            "http.proxy=",
            "protocol.allow=never",
            "protocol.https.allow=always",
            "protocol.ssh.allow=never",
            "push",
            "--porcelain",
            "--no-verify",
            "--no-follow-tags",
            "--recurse-submodules=no",
            remote_url,
            &format!("{local_commit}:{remote_ref}"),
        ] {
            assert!(
                arguments.iter().any(|argument| argument == required),
                "missing {required}"
            );
        }
        assert!(
            !arguments
                .iter()
                .any(|argument| argument == capability.refs_path())
        );
        assert!(!arguments.iter().any(|argument| {
            capability
                .packed_refs_path()
                .is_some_and(|packed| argument == packed)
        }));
        for forbidden in [
            "--force",
            "--force-with-lease",
            "--mirror",
            "--delete",
            "--tags",
            "--follow-tags",
            "--set-upstream",
            "--atomic",
        ] {
            assert!(!arguments.iter().any(|argument| argument == forbidden));
        }
        let environment = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(environment["GIT_CONFIG_NOSYSTEM"].as_deref(), Some("1"));
        assert_eq!(
            environment["GIT_CONFIG_GLOBAL"].as_deref(),
            Some("/dev/null")
        );
        assert_eq!(environment["GIT_TERMINAL_PROMPT"].as_deref(), Some("0"));
        assert_eq!(environment["GIT_OPTIONAL_LOCKS"].as_deref(), Some("0"));
        assert_no_https_credential_boundary(&command);

        let (ssh_command, _ssh_config) = git_push_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            &capability,
            GitObjectFormat::Sha1,
            "ssh://git@example.invalid/team/rehow.git",
            local_commit,
            remote_ref,
            GitRemoteTransport::Ssh,
            GitRemoteAuthentication::Anonymous,
        )
        .unwrap();
        let ssh_arguments = ssh_command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let ssh_environment = ssh_command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            ssh_environment["GIT_SSH"].as_deref(),
            Some("/home/rehow/bin/rehow-git-ssh")
        );
        for required in ["protocol.https.allow=never", "protocol.ssh.allow=always"] {
            assert!(ssh_arguments.iter().any(|argument| argument == required));
        }
        assert_no_https_credential_boundary(&ssh_command);

        let resources = https_credentials::test_direct_resources();
        let authenticated_url = "https://example.invalid:8443/team/rehow.git";
        let (https_command, _https_config) = git_push_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            &capability,
            GitObjectFormat::Sha1,
            authenticated_url,
            local_commit,
            remote_ref,
            GitRemoteTransport::Https,
            GitRemoteAuthentication::Https(&resources),
        )
        .unwrap();
        assert_https_credential_boundary(&https_command, "example.invalid:8443");
        let https_arguments = command_arguments(&https_command);
        for required in [
            "push",
            "--porcelain",
            "--no-verify",
            "--no-follow-tags",
            "--recurse-submodules=no",
            authenticated_url,
            &format!("{local_commit}:{remote_ref}"),
        ] {
            assert!(https_arguments.iter().any(|argument| argument == required));
        }
        for forbidden in ["--force", "--force-with-lease", "--delete", "--mirror"] {
            assert!(!https_arguments.iter().any(|argument| argument == forbidden));
        }
    }

    #[test]
    fn clone_discovery_refuses_ssh_and_precancel_before_spawning() {
        let temp = Temp::new();
        let target = empty_target(&temp);
        let https = plan_git_clone(&target, "https://example.invalid/team/rehow.git").unwrap();
        let cancelled = AtomicBool::new(true);
        assert_eq!(
            discover_git_clone_remote_head(&https, &cancelled, GitRemoteAuthentication::Anonymous,),
            Err(GitCloneDiscoveryError::Cancelled)
        );
        let ssh = plan_git_clone(&target, "git@example.invalid:team/rehow.git").unwrap();
        assert_eq!(
            discover_git_clone_remote_head(
                &ssh,
                &AtomicBool::new(false),
                GitRemoteAuthentication::Anonymous,
            ),
            Err(GitCloneDiscoveryError::UnsupportedTransport)
        );
    }

    #[test]
    fn clone_staging_is_private_provenance_bound_and_leaves_destination_empty() {
        let destination = Temp::new();
        let state = Temp::new();
        let target = empty_target(&destination);
        let url = "https://example.invalid/team/rehow.git";
        let plan = plan_git_clone(&target, url).unwrap();
        let head = parse_git_remote_head(
            b"ref: refs/heads/main\tHEAD\n0123456789abcdef0123456789abcdef01234567\tHEAD\n",
        )
        .unwrap();
        let staging = prepare_git_clone_staging(&target, &plan, &head, &state.0).unwrap();
        assert!(destination.0.read_dir().unwrap().next().is_none());
        let git_path = Path::new(staging.git_path());
        assert_eq!(
            fs::metadata(git_path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::read(git_path.join("config")).unwrap(),
            b"[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = true\n"
        );
        let stage_root = Path::new(staging.stage_path());
        let provenance = fs::read_to_string(stage_root.join("provenance")).unwrap();
        assert!(provenance.starts_with("rehow-clone-stage-v1\n"));
        assert!(provenance.contains("object_format=sha1\n"));
        assert!(provenance.contains("branch_ref=refs/heads/main\n"));
        assert!(!provenance.contains(url));
        let token = serde_json::to_string(plan.token()).unwrap();
        assert!(!provenance.contains(token.trim_matches('"')));
        assert_eq!(
            prepare_git_clone_staging(&target, &plan, &head, &state.0)
                .err()
                .unwrap(),
            GitCloneStageError::Conflict
        );
    }

    #[test]
    fn clone_staging_size_scan_rejects_limits_and_unsafe_entries() {
        let destination = Temp::new();
        let state = Temp::new();
        let target = empty_target(&destination);
        let plan = plan_git_clone(&target, "https://example.invalid/team/rehow.git").unwrap();
        let head = parse_git_remote_head(
            b"ref: refs/heads/main\tHEAD\n0123456789abcdef0123456789abcdef01234567\tHEAD\n",
        )
        .unwrap();
        let staging = prepare_git_clone_staging(&target, &plan, &head, &state.0).unwrap();
        assert_eq!(
            staging.verify_git_size_limit(1),
            Err(GitCloneStagingError::TooLarge)
        );
        let initial = staging.git_state(u64::MAX).unwrap();
        let shallow_path = Path::new(staging.git_path()).join("shallow");
        fs::write(&shallow_path, b"0123456789abcdef0123456789abcdef01234567\n").unwrap();
        fs::set_permissions(&shallow_path, fs::Permissions::from_mode(0o600)).unwrap();
        let changed = staging.git_state(u64::MAX).unwrap();
        assert!(changed.bytes > initial.bytes);
        assert_ne!(changed.binding, initial.binding);
        symlink("/tmp", Path::new(staging.git_path()).join("unsafe-link")).unwrap();
        assert_eq!(
            staging.verify_git_size_limit(u64::MAX),
            Err(GitCloneStagingError::Unsafe)
        );
    }

    #[test]
    fn clone_staging_refuses_changed_destination_before_state_creation() {
        let destination = Temp::new();
        let state = Temp::new();
        let target = empty_target(&destination);
        let plan = plan_git_clone(&target, "https://example.invalid/team/rehow.git").unwrap();
        let head = parse_git_remote_head(
            format!("ref: refs/heads/main\tHEAD\n{}\tHEAD\n", "a".repeat(64)).as_bytes(),
        )
        .unwrap();
        fs::write(destination.0.join("external"), b"preserve").unwrap();
        assert_eq!(
            prepare_git_clone_staging(&target, &plan, &head, &state.0)
                .err()
                .unwrap(),
            GitCloneStageError::DestinationChanged
        );
        assert!(state.0.read_dir().unwrap().next().is_none());
        assert_eq!(
            fs::read(destination.0.join("external")).unwrap(),
            b"preserve"
        );
    }

    fn commit_clone_fixture(source: &Temp, manifest: &[u8]) -> String {
        fs::write(source.0.join("rehow.yaml"), manifest).unwrap();
        assert!(
            Command::new("/usr/bin/git")
                .env_clear()
                .env("LANG", "C")
                .env("LC_ALL", "C")
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(["init", "--quiet", "--template="])
                .arg(&source.0)
                .status()
                .unwrap()
                .success()
        );
        let run = |arguments: &[&str]| {
            Command::new("/usr/bin/git")
                .env_clear()
                .env("LANG", "C")
                .env("LC_ALL", "C")
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(["-C", source.0.to_str().unwrap()])
                .args(arguments)
                .output()
                .unwrap()
        };
        assert!(run(&["add", "--all", "--"]).status.success());
        let tree = run(&["write-tree"]);
        assert!(tree.status.success());
        let tree_id = std::str::from_utf8(&tree.stdout).unwrap().trim();
        let commit = Command::new("/usr/bin/git")
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Rehow Test")
            .env("GIT_AUTHOR_EMAIL", "test@rehow.invalid")
            .env("GIT_COMMITTER_NAME", "Rehow Test")
            .env("GIT_COMMITTER_EMAIL", "test@rehow.invalid")
            .args(["-C", source.0.to_str().unwrap(), "commit-tree", tree_id])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(commit.status.success());
        std::str::from_utf8(&commit.stdout)
            .unwrap()
            .trim()
            .to_owned()
    }

    #[test]
    fn authenticated_ssh_clone_and_sync_use_only_sealed_launch_resources() {
        let infrastructure = Temp::new();
        fs::set_permissions(&infrastructure.0, fs::Permissions::from_mode(0o700)).unwrap();
        let home = Temp::new();
        fs::set_permissions(&home.0, fs::Permissions::from_mode(0o700)).unwrap();
        let ssh_directory = home.0.join(".ssh");
        fs::create_dir(&ssh_directory).unwrap();
        fs::set_permissions(&ssh_directory, fs::Permissions::from_mode(0o700)).unwrap();

        let host_key = infrastructure.0.join("host-key");
        let client_key = infrastructure.0.join("client-key");
        for key in [&host_key, &client_key] {
            assert!(
                Command::new("/usr/bin/ssh-keygen")
                    .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                    .arg(key)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let authorized_keys = infrastructure.0.join("authorized-keys");
        fs::copy(client_key.with_extension("pub"), &authorized_keys).unwrap();
        fs::set_permissions(&authorized_keys, fs::Permissions::from_mode(0o600)).unwrap();

        let agent_socket = infrastructure.0.join("agent.sock");
        let agent = Command::new("/usr/bin/ssh-agent")
            .args(["-D", "-a"])
            .arg(&agent_socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut agent = ChildGuard(agent);
        for _ in 0..200 {
            if agent_socket.exists() {
                break;
            }
            assert!(agent.0.try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(5));
        }
        assert!(agent_socket.exists());
        assert!(
            Command::new("/usr/bin/ssh-add")
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", "/dev/null")
                .env("SSH_AUTH_SOCK", &agent_socket)
                .arg(&client_key)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        );

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let user = std::env::var("USER").unwrap();
        let sshd = Command::new("/usr/bin/sshd")
            .args(["-D", "-e", "-p"])
            .arg(port.to_string())
            .arg("-h")
            .arg(&host_key)
            .arg("-o")
            .arg(format!(
                "PidFile={}",
                infrastructure.0.join("sshd.pid").display()
            ))
            .arg("-o")
            .arg(format!("AuthorizedKeysFile={}", authorized_keys.display()))
            .args([
                "-o",
                "StrictModes=no",
                "-o",
                "PasswordAuthentication=no",
                "-o",
                "KbdInteractiveAuthentication=no",
                "-o",
                "UsePAM=no",
                "-o",
                "PermitRootLogin=no",
                "-o",
            ])
            .arg(format!("AllowUsers={user}"))
            .args(["-o", "LogLevel=ERROR"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut sshd = ChildGuard(sshd);
        let address = format!("127.0.0.1:{port}");
        for _ in 0..200 {
            if TcpStream::connect(&address).is_ok() {
                break;
            }
            assert!(sshd.0.try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(5));
        }
        assert!(TcpStream::connect(&address).is_ok());

        let host_public = fs::read_to_string(host_key.with_extension("pub")).unwrap();
        let mut fields = host_public.split_ascii_whitespace();
        let algorithm = fields.next().unwrap();
        let key = fields.next().unwrap();
        let known_hosts_path = ssh_directory.join("known_hosts");
        fs::write(
            &known_hosts_path,
            format!("[127.0.0.1]:{port} {algorithm} {key}\n"),
        )
        .unwrap();
        fs::set_permissions(&known_hosts_path, fs::Permissions::from_mode(0o600)).unwrap();

        let built_helper = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("rehow-git-ssh");
        let helper = infrastructure.0.join("rehow-git-ssh");
        fs::copy(&built_helper, &helper).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        let agent_provenance =
            capture_git_ssh_agent_provenance_from(Some(agent_socket.as_os_str()));
        let host_keys_provenance = capture_git_ssh_host_keys_from(Some(home.0.as_os_str()), &[]);
        assert_eq!(
            host_keys_provenance._sources,
            vec![fs::read(&known_hosts_path).unwrap()]
        );
        let proxy_provenance = ssh_sandbox::capture_git_ssh_proxy_from(Some(&helper));
        let resources = GitSshTransportResources::prepare(
            &agent_provenance,
            &host_keys_provenance,
            &proxy_provenance,
        )
        .unwrap();
        let client_public = fs::read_to_string(client_key.with_extension("pub")).unwrap();
        let mut client_fields = client_public.split_ascii_whitespace();
        let wrong_algorithm = client_fields.next().unwrap();
        let wrong_key = client_fields.next().unwrap();
        fs::write(
            &known_hosts_path,
            format!("[127.0.0.1]:{port} {wrong_algorithm} {wrong_key}\n"),
        )
        .unwrap();
        let wrong_host_keys = capture_git_ssh_host_keys_from(Some(home.0.as_os_str()), &[]);
        let wrong_resources = GitSshTransportResources::prepare(
            &agent_provenance,
            &wrong_host_keys,
            &proxy_provenance,
        )
        .unwrap();

        let source = Temp::new();
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: SSH Clone\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let commit = commit_clone_fixture(&source, manifest);
        assert!(
            Command::new("/usr/bin/git")
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .args([
                    "-C",
                    source.0.to_str().unwrap(),
                    "update-ref",
                    "refs/heads/main"
                ])
                .arg(&commit)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("/usr/bin/git")
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .args([
                    "-C",
                    source.0.to_str().unwrap(),
                    "symbolic-ref",
                    "HEAD",
                    "refs/heads/main",
                ])
                .status()
                .unwrap()
                .success()
        );

        let remote = Temp::new();
        assert!(
            Command::new("/usr/bin/git")
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .args(["clone", "--quiet", "--bare"])
                .arg(&source.0)
                .arg(&remote.0)
                .status()
                .unwrap()
                .success()
        );
        let destination = Temp::new();
        let state = Temp::new();
        let target = empty_target(&destination);
        let url = format!("ssh://{user}@127.0.0.1:{port}{}", remote.0.display());
        let plan = plan_git_clone(&target, &url).unwrap();
        let validation = apply_git_clone_validation_with_ssh(
            &target,
            plan,
            &state.0,
            &AtomicBool::new(false),
            Some(&resources),
        )
        .unwrap();
        assert_eq!(validation.remote_url(), url);
        assert!(destination.0.read_dir().unwrap().next().is_none());

        let rejected_destination = Temp::new();
        let rejected_state = Temp::new();
        let rejected_target = empty_target(&rejected_destination);
        let rejected_plan = plan_git_clone(&rejected_target, &url).unwrap();
        assert_eq!(
            apply_git_clone_validation_with_ssh(
                &rejected_target,
                rejected_plan,
                &rejected_state.0,
                &AtomicBool::new(false),
                Some(&wrong_resources),
            )
            .err()
            .unwrap(),
            GitCloneApplyError::HostKeyRejected
        );
        assert!(rejected_destination.0.read_dir().unwrap().next().is_none());

        let sync_workspace = Temp::new();
        drop(initialized_workspace(&sync_workspace));
        configure_fixture_identity(&sync_workspace);
        fixture_git(&sync_workspace, &["add", "--", "rehow.yaml"]);
        fixture_git(&sync_workspace, &["commit", "-qm", "local base"]);
        fixture_git(&sync_workspace, &["remote", "add", "origin", &url]);
        fixture_git(&sync_workspace, &["config", "branch.main.remote", "origin"]);
        fixture_git(
            &sync_workspace,
            &["config", "branch.main.merge", "refs/heads/main"],
        );
        let manifest_before = fs::read(sync_workspace.0.join("rehow.yaml")).unwrap();
        let head_before = fs::read(sync_workspace.0.join(".git/HEAD")).unwrap();
        let index_before = fs::read(sync_workspace.0.join(".git/index")).unwrap();
        let config_before = fs::read(sync_workspace.0.join(".git/config")).unwrap();
        let sync_view = open_workspace(&sync_workspace);
        let missing_resource_state = Temp::new();
        let missing_resource_plan = plan_git_sync(&sync_view, &missing_resource_state.0).unwrap();
        assert_eq!(
            apply_git_sync_inspection(
                &sync_view,
                &missing_resource_state.0,
                missing_resource_plan,
                &AtomicBool::new(false),
            )
            .err()
            .unwrap(),
            GitSyncError::UnsupportedTransport
        );
        let sync_state = Temp::new();
        let sync_plan = plan_git_sync(&sync_view, &sync_state.0).unwrap();
        assert_eq!(sync_plan.transport(), GitRemoteTransport::Ssh);
        let inspection = apply_git_sync_inspection_with_ssh(
            &sync_view,
            &sync_state.0,
            sync_plan,
            &AtomicBool::new(false),
            Some(&resources),
        )
        .unwrap();
        let classification = classify_git_sync_ancestry_with_ssh(
            &sync_view,
            inspection,
            &AtomicBool::new(false),
            Some(&resources),
        )
        .unwrap();
        assert_eq!(classification.ancestry(), GitSyncAncestry::Diverged);
        assert_eq!(
            fs::read(sync_workspace.0.join("rehow.yaml")).unwrap(),
            manifest_before
        );
        assert_eq!(
            fs::read(sync_workspace.0.join(".git/HEAD")).unwrap(),
            head_before
        );
        assert_eq!(
            fs::read(sync_workspace.0.join(".git/index")).unwrap(),
            index_before
        );
        assert_eq!(
            fs::read(sync_workspace.0.join(".git/config")).unwrap(),
            config_before
        );

        let rejected_sync_state = Temp::new();
        let rejected_sync_plan = plan_git_sync(&sync_view, &rejected_sync_state.0).unwrap();
        assert_eq!(
            apply_git_sync_inspection_with_ssh(
                &sync_view,
                &rejected_sync_state.0,
                rejected_sync_plan,
                &AtomicBool::new(false),
                Some(&wrong_resources),
            )
            .err()
            .unwrap(),
            GitSyncError::HostKeyRejected
        );
        assert_eq!(
            fs::read(sync_workspace.0.join("rehow.yaml")).unwrap(),
            manifest_before
        );
        assert_eq!(
            fs::read(sync_workspace.0.join(".git/HEAD")).unwrap(),
            head_before
        );
        assert_eq!(
            fs::read(sync_workspace.0.join(".git/index")).unwrap(),
            index_before
        );
        assert_eq!(
            fs::read(sync_workspace.0.join(".git/config")).unwrap(),
            config_before
        );

        let push_workspace = Temp::new();
        assert!(
            Command::new("/usr/bin/git")
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .args(["clone", "--quiet", "--no-local"])
                .arg(&remote.0)
                .arg(&push_workspace.0)
                .status()
                .unwrap()
                .success()
        );
        fs::create_dir_all(push_workspace.0.join("categories")).unwrap();
        configure_fixture_identity(&push_workspace);
        fixture_git(&push_workspace, &["remote", "set-url", "origin", &url]);
        fs::write(
            push_workspace.0.join("rehow.yaml"),
            b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: SSH Push\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-20T00:00:00Z\"\n",
        )
        .unwrap();
        fixture_git(&push_workspace, &["add", "--", "rehow.yaml"]);
        fixture_git(&push_workspace, &["commit", "-qm", "local ahead"]);
        let push_view = open_workspace(&push_workspace);
        let inspect_local_ahead = |push_state: &Temp| {
            let plan = plan_git_sync(&push_view, &push_state.0).unwrap();
            let inspection = apply_git_sync_inspection_with_ssh(
                &push_view,
                &push_state.0,
                plan,
                &AtomicBool::new(false),
                Some(&resources),
            )
            .unwrap();
            classify_git_sync_ancestry_with_ssh(
                &push_view,
                inspection,
                &AtomicBool::new(false),
                Some(&resources),
            )
            .unwrap()
        };
        let rejected_push_state = Temp::new();
        let rejected_push = inspect_local_ahead(&rejected_push_state);
        assert_eq!(rejected_push.ancestry(), GitSyncAncestry::LocalAhead);
        assert_eq!(
            apply_git_push_with_ssh(&push_view, rejected_push, Some(&wrong_resources))
                .err()
                .unwrap(),
            GitSyncError::HostKeyRejected
        );
        assert_eq!(
            fixture_git_stdout(&remote, &["rev-parse", "refs/heads/main"]),
            commit
        );

        let push_state = Temp::new();
        let push_classification = inspect_local_ahead(&push_state);
        assert_eq!(push_classification.ancestry(), GitSyncAncestry::LocalAhead);
        let local_head = fixture_git_stdout(&push_workspace, &["rev-parse", "HEAD"]);
        assert!(
            apply_git_push_with_ssh(&push_view, push_classification, Some(&resources))
                .unwrap()
                .pushed()
        );
        assert_eq!(
            fixture_git_stdout(&remote, &["rev-parse", "refs/heads/main"]),
            local_head
        );
    }

    fn stage_sync_fixture(
        workspace: &WorkspaceView,
        plan: GitSyncPlan,
        state: &Temp,
        source: &Temp,
        remote_manifest: &[u8],
    ) -> GitSyncInspection {
        let commit_id = commit_clone_fixture(source, remote_manifest);
        let remote_head = GitRemoteHead {
            branch_ref: "refs/heads/main".to_owned(),
            object_id: commit_id,
            object_format: GitObjectFormat::Sha1,
        };
        let staging = prepare_git_sync_staging(&plan, &remote_head, &state.0).unwrap();
        let mut fetch = Command::new("/usr/bin/git");
        fetch
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args([
                "--no-pager",
                "--literal-pathspecs",
                &format!("--git-dir={}", staging.git_path()),
                "-c",
                "protocol.file.allow=always",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "fetch.unpackLimit=1",
                "-c",
                "transfer.unpackLimit=1",
                "-c",
                "pack.writeReverseIndex=false",
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-write-fetch-head",
            ])
            .arg(source.0.to_str().unwrap())
            .arg(format!("{}:refs/rehow/clone/head", remote_head.object_id()));
        configure_clone_child(&mut fetch, staging.git_descriptor(), None);
        assert!(fetch.status().unwrap().success());
        finish_git_sync_inspection(
            workspace,
            plan,
            staging,
            remote_head,
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    fn clean_sync_publication_fixture(
        remote_name: &str,
    ) -> (
        Temp,
        Temp,
        Box<WorkspaceView>,
        GitSyncClassification,
        String,
    ) {
        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        fs::write(temp.0.join(".gitignore"), b".rehow/\n").unwrap();
        fixture_git(&temp, &["add", "--", ".gitignore"]);
        fixture_git(&temp, &["commit", "-qm", "ignore recovery trash"]);
        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let source = Temp::new();
        write_fixture_guide(&source);
        fs::write(source.0.join(".gitignore"), b".rehow/\n").unwrap();
        let remote_manifest = format!(
            "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: {remote_name}\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n"
        );
        let inspection = stage_sync_fixture(
            &workspace,
            plan,
            &state,
            &source,
            remote_manifest.as_bytes(),
        );
        let remote_head = inspection.remote_head.object_id().to_owned();
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };
        (temp, state, workspace, classification, remote_head)
    }

    fn stage_clone_fixture(
        target: &EmptyWorkspaceTarget,
        state: &Temp,
        source: &Temp,
        commit_id: &str,
    ) -> (GitClonePlan, GitRemoteHead, GitCloneStaging) {
        let plan = plan_git_clone(target, "https://example.invalid/team/rehow.git").unwrap();
        let head = parse_git_remote_head(
            format!("ref: refs/heads/main\tHEAD\n{commit_id}\tHEAD\n").as_bytes(),
        )
        .unwrap();
        let staging = prepare_git_clone_staging(target, &plan, &head, &state.0).unwrap();
        let mut fetch = Command::new("/usr/bin/git");
        fetch
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args([
                "--no-pager",
                "--literal-pathspecs",
                &format!("--git-dir={}", staging.git_path()),
                "-c",
                "protocol.file.allow=always",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "fetch.unpackLimit=1",
                "-c",
                "transfer.unpackLimit=1",
                "-c",
                "pack.writeReverseIndex=false",
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-write-fetch-head",
            ])
            .arg(source.0.to_str().unwrap())
            .arg(format!("{commit_id}:refs/rehow/clone/head"));
        configure_clone_child(&mut fetch, staging.git_descriptor(), None);
        assert!(fetch.status().unwrap().success());
        (plan, head, staging)
    }

    #[test]
    fn clone_content_streams_and_validates_in_private_staging() {
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Cloned\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let commit_id = commit_clone_fixture(&source, manifest);
        let target = empty_target(&destination);
        let (_, head, staging) = stage_clone_fixture(&target, &state, &source, &commit_id);
        let cancelled = AtomicBool::new(false);
        let tree = inspect_fetched_git_clone_tree(&staging, &head, &cancelled).unwrap();
        let validated = validate_fetched_git_clone(&staging, tree, &cancelled).unwrap();
        let packs = staging
            .inspect_pack_sources(head.object_id().len(), CLONE_STAGING_FILE_LIMIT)
            .unwrap();

        assert_eq!(validated.workspace().manifest().name(), "Cloned");
        assert_eq!(validated.tree().commit_id(), commit_id);
        assert!(validated.workspace().issues().is_empty());
        assert_eq!(packs.len(), 2);
        assert!(packs.iter().any(|source| source.name.ends_with(".pack")));
        assert!(packs.iter().any(|source| source.name.ends_with(".idx")));
        for source in &packs {
            let file = staging.open_pack_source(source).unwrap();
            assert_eq!(file.metadata().unwrap().len(), source.size);
        }
        assert!(destination.0.read_dir().unwrap().next().is_none());
    }

    fn validated_clone_fixture(
        destination: &Temp,
        state: &Temp,
        source: &Temp,
        manifest: &[u8],
    ) -> (EmptyWorkspaceTarget, GitCloneValidation) {
        let commit_id = commit_clone_fixture(source, manifest);
        let target = empty_target(destination);
        let (plan, remote_head, staging) = stage_clone_fixture(&target, state, source, &commit_id);
        let cancelled = AtomicBool::new(false);
        let tree = inspect_fetched_git_clone_tree(&staging, &remote_head, &cancelled).unwrap();
        let validated = validate_fetched_git_clone(&staging, tree, &cancelled).unwrap();
        (
            target,
            GitCloneValidation {
                plan,
                staging,
                remote_head,
                validated,
                operation_id: new_operation_id(),
                created_at: current_timestamp(),
            },
        )
    }

    #[test]
    fn clone_materialization_journals_validated_content_and_reopens_it() {
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Materialized\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let category = source.0.join("categories/linux-system--b6ba3a5551");
        fs::create_dir_all(&category).unwrap();
        fs::write(
            category.join("category.yaml"),
            b"schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux System\norder: 1000\nstatus: active\ncreated_at: \"2026-09-05T10:01:00Z\"\nupdated_at: \"2026-09-05T10:01:00Z\"\n",
        )
        .unwrap();
        use sha2::Digest as _;
        let attachment_bytes = b"validated attachment\n";
        let attachment_sha = hex_lower(&sha2::Sha256::digest(attachment_bytes));
        let guide = format!(
            "---\nschema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: Install a Tarball\nsummary: Install a downloaded archive.\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags: []\naliases: []\nattachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.txt\n    title: Example text\n    sha256: {attachment_sha}\n    size_bytes: {}\nstatus: draft\ncreated_at: \"2026-09-05T10:03:00Z\"\nupdated_at: \"2026-09-05T10:03:00Z\"\n---\n\n## Steps\n\nExtract it.\n",
            attachment_bytes.len()
        );
        fs::write(category.join("install--36c5735e10.md"), guide).unwrap();
        let attachment = source
            .0
            .join("attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.txt");
        fs::create_dir_all(attachment.parent().unwrap()).unwrap();
        fs::write(&attachment, attachment_bytes).unwrap();
        fs::write(source.0.join(".gitignore"), b"*.tmp\n").unwrap();
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);

        let workspace =
            apply_git_clone_content_materialization(&target, &validation, &state.0, None).unwrap();
        assert_eq!(workspace.manifest().name(), "Materialized");
        assert_eq!(
            fs::read(destination.0.join("rehow.yaml")).unwrap(),
            manifest
        );
        assert!(destination.0.join("categories").is_dir());
        assert!(
            destination
                .0
                .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md")
                .is_file()
        );
        assert_eq!(
            fs::read(destination.0.join(".gitignore")).unwrap(),
            b"*.tmp\n"
        );
        assert_eq!(
            fs::read(
                destination
                    .0
                    .join("attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.txt")
            )
            .unwrap(),
            attachment_bytes
        );
        assert!(destination.0.join(".git/rehow-clone").is_file());
        assert!(!destination.0.join(".git/HEAD").exists());
        assert!(!destination.0.join(".git/config").exists());
    }

    #[test]
    fn clone_materialization_refuses_destination_and_staging_changes() {
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Protected\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";

        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);
        fs::write(destination.0.join("external"), b"preserve").unwrap();
        assert_eq!(
            apply_git_clone_content_materialization(&target, &validation, &state.0, None)
                .err()
                .unwrap(),
            GitCloneMaterializeError::Conflict
        );
        assert_eq!(
            fs::read(destination.0.join("external")).unwrap(),
            b"preserve"
        );
        assert!(!destination.0.join("rehow.yaml").exists());

        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);
        fs::create_dir(destination.0.join(".git")).unwrap();
        fs::write(destination.0.join(".git/foreign"), b"preserve").unwrap();
        assert!(
            apply_git_clone_content_materialization(&target, &validation, &state.0, None).is_err()
        );
        assert_eq!(
            fs::read(destination.0.join(".git/foreign")).unwrap(),
            b"preserve"
        );
        assert!(!destination.0.join(".git/rehow-clone").exists());
        assert!(!destination.0.join("rehow.yaml").exists());

        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);
        let changed = vec![b'x'; manifest.len()];
        fs::write(
            Path::new(validation.staging.validation_path()).join("rehow.yaml"),
            changed,
        )
        .unwrap();
        assert_eq!(
            apply_git_clone_content_materialization(&target, &validation, &state.0, None)
                .err()
                .unwrap(),
            GitCloneMaterializeError::StagingChanged
        );
        assert!(destination.0.read_dir().unwrap().next().is_none());
    }

    #[test]
    fn clone_materialization_resumes_the_exact_installed_journal() {
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Resumed\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);

        set_workspace_fault(WorkspaceFault::BeforeLiveRename);
        assert_eq!(
            apply_git_clone_content_materialization(&target, &validation, &state.0, None)
                .err()
                .unwrap(),
            GitCloneMaterializeError::Unsupported
        );
        assert!(destination.0.join(".git/rehow-clone").is_file());
        assert!(!destination.0.join("rehow.yaml").exists());

        let workspace =
            apply_git_clone_content_materialization(&target, &validation, &state.0, None).unwrap();
        assert_eq!(workspace.manifest().name(), "Resumed");
        assert_eq!(
            fs::read(destination.0.join("rehow.yaml")).unwrap(),
            manifest
        );

        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);
        set_workspace_fault(WorkspaceFault::BeforeLiveRename);
        assert!(
            apply_git_clone_content_materialization(&target, &validation, &state.0, None).is_err()
        );
        fs::write(destination.0.join("foreign"), b"preserve").unwrap();
        assert_eq!(
            apply_git_clone_content_materialization(&target, &validation, &state.0, None)
                .err()
                .unwrap(),
            GitCloneMaterializeError::Conflict
        );
        assert_eq!(
            fs::read(destination.0.join("foreign")).unwrap(),
            b"preserve"
        );
        assert!(!destination.0.join("rehow.yaml").exists());

        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);
        set_workspace_fault(WorkspaceFault::BeforeLiveRename);
        assert!(
            apply_git_clone_content_materialization(&target, &validation, &state.0, None).is_err()
        );
        fs::remove_file(destination.0.join(".git/rehow-clone")).unwrap();
        assert!(
            apply_git_clone_content_materialization(&target, &validation, &state.0, None).is_err()
        );
        assert!(!destination.0.join(".git/rehow-clone").exists());
        assert!(!destination.0.join("rehow.yaml").exists());
    }

    #[test]
    fn clone_materialization_resumes_each_private_scaffold_boundary() {
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Scaffold resume\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        for fault in [
            GitCloneScaffoldFault::Marker,
            GitCloneScaffoldFault::Objects,
            GitCloneScaffoldFault::Info,
            GitCloneScaffoldFault::Pack,
            GitCloneScaffoldFault::Refs,
            GitCloneScaffoldFault::Heads,
            GitCloneScaffoldFault::Remotes,
            GitCloneScaffoldFault::Origin,
            GitCloneScaffoldFault::Tags,
        ] {
            let destination = Temp::new();
            let state = Temp::new();
            let source = Temp::new();
            let (target, validation) =
                validated_clone_fixture(&destination, &state, &source, manifest);
            set_git_clone_scaffold_fault(fault);
            assert_eq!(
                apply_git_clone_content_materialization(&target, &validation, &state.0, None)
                    .err()
                    .unwrap(),
                GitCloneMaterializeError::Unsupported,
                "{fault:?}"
            );
            assert!(destination.0.join(".git/rehow-clone").is_file());
            let workspace =
                apply_git_clone_content_materialization(&target, &validation, &state.0, None)
                    .unwrap();
            assert_eq!(workspace.manifest().name(), "Scaffold resume");
        }
    }

    #[test]
    fn clone_finalization_installs_validated_objects_refs_and_config() {
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Finalized\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);

        let workspace =
            apply_git_clone_repository_finalization(&target, &validation, &state.0, None).unwrap();
        let status = read_git_status(&workspace).unwrap();
        let GitRepositoryStatus::Ready(status) = status.repository() else {
            panic!("finalized clone was not a repository")
        };
        assert_eq!(status.branch(), Some("main"));
        assert_eq!(status.upstream(), Some("origin/main"));
        assert_eq!(status.remotes(), ["origin"]);
        assert!(status.clean());
        assert!(destination.0.join(".git/rehow-clone-complete").is_file());
        assert_eq!(
            fs::read_to_string(destination.0.join(".git/refs/heads/main")).unwrap(),
            format!("{}\n", validation.remote_head.object_id())
        );
        assert_eq!(
            fs::read_to_string(destination.0.join(".git/refs/remotes/origin/main")).unwrap(),
            format!("{}\n", validation.remote_head.object_id())
        );
        assert_eq!(
            fs::read(destination.0.join("rehow.yaml")).unwrap(),
            manifest
        );

        let replayed =
            apply_git_clone_repository_finalization(&target, &validation, &state.0, None).unwrap();
        let GitRepositoryStatus::Ready(replayed_status) =
            read_git_status(&replayed).unwrap().repository().clone()
        else {
            panic!("replayed clone was not a repository")
        };
        assert!(replayed_status.clean());
    }

    #[test]
    fn clone_finalization_resumes_every_publication_boundary() {
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Finalization replay\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        for fault in [
            GitCloneFinalizeFault::Pack,
            GitCloneFinalizeFault::Shallow,
            GitCloneFinalizeFault::Config,
            GitCloneFinalizeFault::LocalRef,
            GitCloneFinalizeFault::RemoteRef,
            GitCloneFinalizeFault::Head,
            GitCloneFinalizeFault::Seal,
        ] {
            let destination = Temp::new();
            let state = Temp::new();
            let source = Temp::new();
            let (target, validation) =
                validated_clone_fixture(&destination, &state, &source, manifest);
            set_git_clone_finalize_fault(fault);
            assert_eq!(
                apply_git_clone_repository_finalization(&target, &validation, &state.0, None)
                    .err()
                    .unwrap(),
                GitCloneMaterializeError::Unsupported,
                "{fault:?}"
            );
            let workspace =
                apply_git_clone_repository_finalization(&target, &validation, &state.0, None)
                    .unwrap();
            let GitRepositoryStatus::Ready(status) =
                read_git_status(&workspace).unwrap().repository().clone()
            else {
                panic!("replayed clone was not a repository")
            };
            assert!(status.clean(), "{fault:?}");
        }
    }

    #[test]
    fn clone_finalization_refuses_pack_collision_and_supports_nested_branch() {
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Finalization guard\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);
        apply_git_clone_content_materialization(&target, &validation, &state.0, None).unwrap();
        let packs = validation
            .staging
            .inspect_pack_sources(
                validation.remote_head.object_id().len(),
                CLONE_STAGING_FILE_LIMIT,
            )
            .unwrap();
        let collision = destination.0.join(".git/objects/pack").join(&packs[0].name);
        fs::write(&collision, vec![b'x'; packs[0].size as usize]).unwrap();
        assert_eq!(
            apply_git_clone_repository_finalization(&target, &validation, &state.0, None)
                .err()
                .unwrap(),
            GitCloneMaterializeError::Unsafe
        );
        assert_eq!(
            fs::read(&collision).unwrap(),
            vec![b'x'; packs[0].size as usize]
        );
        assert!(!destination.0.join(".git/rehow-clone-complete").exists());

        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let (target, mut validation) =
            validated_clone_fixture(&destination, &state, &source, manifest);
        validation.remote_head.branch_ref = "refs/heads/team/main".to_owned();
        let workspace =
            apply_git_clone_repository_finalization(&target, &validation, &state.0, None).unwrap();
        let GitRepositoryStatus::Ready(status) =
            read_git_status(&workspace).unwrap().repository().clone()
        else {
            panic!("nested-branch clone was not a repository")
        };
        assert_eq!(status.branch(), Some("team/main"));
        assert_eq!(status.upstream(), Some("origin/team/main"));
        assert!(status.clean());
    }

    #[test]
    fn clone_finalization_refuses_changed_staging_pack_without_sealing() {
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Changed staging\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);
        apply_git_clone_content_materialization(&target, &validation, &state.0, None).unwrap();
        let pack = validation
            .staging
            .inspect_pack_sources(
                validation.remote_head.object_id().len(),
                CLONE_STAGING_FILE_LIMIT,
            )
            .unwrap()
            .into_iter()
            .find(|entry| entry.name.ends_with(".pack"))
            .unwrap();
        let path = Path::new(validation.staging.git_path())
            .join("objects/pack")
            .join(pack.name);
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] ^= 0xff;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, bytes).unwrap();

        assert_eq!(
            apply_git_clone_repository_finalization(&target, &validation, &state.0, None)
                .err()
                .unwrap(),
            GitCloneMaterializeError::StagingChanged
        );
        assert!(!destination.0.join(".git/rehow-clone-complete").exists());
    }

    #[test]
    fn clone_finalization_refuses_ignored_file_added_before_seal() {
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        fs::write(source.0.join(".gitignore"), b"*.tmp\n").unwrap();
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Ignored race\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let (target, validation) = validated_clone_fixture(&destination, &state, &source, manifest);
        let root = destination.0.clone();
        let writer = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !root.join(".git/HEAD").exists() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
            fs::write(root.join("intruder.tmp"), b"preserve").unwrap();
        });
        CLONE_PRESEAL_PAUSE.set(true);
        let result =
            apply_git_clone_repository_finalization(&target, &validation, &state.0, None).err();
        CLONE_PRESEAL_PAUSE.set(false);
        writer.join().unwrap();

        assert_eq!(result, Some(GitCloneMaterializeError::Conflict));
        assert_eq!(
            fs::read(destination.0.join("intruder.tmp")).unwrap(),
            b"preserve"
        );
        assert!(!destination.0.join(".git/rehow-clone-complete").exists());
    }

    #[test]
    fn clone_content_rejects_malformed_workspace_without_touching_destination() {
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let commit_id = commit_clone_fixture(&source, b"schema_version: nope\n");
        let target = empty_target(&destination);
        let (_, head, staging) = stage_clone_fixture(&target, &state, &source, &commit_id);
        let cancelled = AtomicBool::new(false);
        let tree = inspect_fetched_git_clone_tree(&staging, &head, &cancelled).unwrap();

        assert_eq!(
            validate_fetched_git_clone(&staging, tree, &cancelled).unwrap_err(),
            GitCloneContentError::InvalidWorkspace
        );
        assert!(destination.0.read_dir().unwrap().next().is_none());
    }

    #[test]
    fn clone_content_rejects_workspace_scan_issues() {
        let destination = Temp::new();
        let state = Temp::new();
        let source = Temp::new();
        let attachment = source
            .0
            .join("attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/orphan.txt");
        fs::create_dir_all(attachment.parent().unwrap()).unwrap();
        fs::write(&attachment, b"orphan").unwrap();
        let manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Cloned\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n";
        let commit_id = commit_clone_fixture(&source, manifest);
        let target = empty_target(&destination);
        let (_, head, staging) = stage_clone_fixture(&target, &state, &source, &commit_id);
        let cancelled = AtomicBool::new(false);
        let tree = inspect_fetched_git_clone_tree(&staging, &head, &cancelled).unwrap();

        assert_eq!(
            validate_fetched_git_clone(&staging, tree, &cancelled).unwrap_err(),
            GitCloneContentError::WorkspaceIssues
        );
        assert!(destination.0.read_dir().unwrap().next().is_none());
    }

    #[test]
    fn clone_inspection_requires_head_to_be_an_exact_commit() {
        let source = Temp::new();
        let commit_id = commit_clone_fixture(
            &source,
            b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Types\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
        );
        let git_output = |arguments: &[&str]| {
            Command::new("/usr/bin/git")
                .env_clear()
                .env("LANG", "C")
                .env("LC_ALL", "C")
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(["-C", source.0.to_str().unwrap()])
                .args(arguments)
                .output()
                .unwrap()
        };
        let tree = git_output(&["rev-parse", &format!("{commit_id}^{{tree}}")]);
        assert!(tree.status.success());
        let tree_id = std::str::from_utf8(&tree.stdout).unwrap().trim().to_owned();
        let tag = Command::new("/usr/bin/git")
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_COMMITTER_NAME", "Rehow Test")
            .env("GIT_COMMITTER_EMAIL", "test@rehow.invalid")
            .args([
                "-C",
                source.0.to_str().unwrap(),
                "tag",
                "-a",
                "snapshot",
                "-m",
                "snapshot",
                &commit_id,
            ])
            .status()
            .unwrap();
        assert!(tag.success());
        let tag = git_output(&["rev-parse", "refs/tags/snapshot"]);
        assert!(tag.status.success());
        let tag_id = std::str::from_utf8(&tag.stdout).unwrap().trim().to_owned();

        for object_id in [tree_id, tag_id] {
            let destination = Temp::new();
            let state = Temp::new();
            let target = empty_target(&destination);
            let (_, head, staging) = stage_clone_fixture(&target, &state, &source, &object_id);
            assert_eq!(
                inspect_fetched_git_clone_tree(&staging, &head, &AtomicBool::new(false)),
                Err(GitCloneInspectError::InvalidRepository)
            );
            assert!(destination.0.read_dir().unwrap().next().is_none());
        }
    }

    #[test]
    fn sync_target_and_remote_ref_parsers_are_exact() {
        let records = b"core.repositoryformatversion\n0\0branch.main.remote\norigin\0branch.main.merge\nrefs/heads/team/main\0remote.origin.url\nhttps://example.invalid/team/rehow.git\0";
        let target = parse_git_sync_target(records, "main").unwrap();
        assert_eq!(target.remote_name, "origin");
        assert_eq!(target.remote_branch, "team/main");
        assert_eq!(target.transport, GitRemoteTransport::Https);
        let object_id = "0123456789abcdef0123456789abcdef01234567";
        let head = parse_git_sync_remote_ref(
            format!("{object_id}\trefs/heads/team/main\n").as_bytes(),
            "refs/heads/team/main",
        )
        .unwrap();
        assert_eq!(head.object_id(), object_id);
        assert_eq!(head.branch_ref(), "refs/heads/team/main");

        for malformed in [
            b"branch.main.remote\norigin\0branch.main.merge\nrefs/heads/main\0"
                .as_slice(),
            b"branch.main.remote\norigin\0branch.main.remote\nbackup\0branch.main.merge\nrefs/heads/main\0remote.origin.url\nhttps://example.invalid/a.git\0"
                .as_slice(),
            b"branch.main.remote\norigin\0branch.main.merge\nrefs/heads/main\0remote.origin.url\nhttps://user@example.invalid/a.git\0"
                .as_slice(),
        ] {
            assert!(parse_git_sync_target(malformed, "main").is_err());
        }
        assert!(
            parse_git_sync_remote_ref(
                format!("{object_id}\trefs/heads/other\n").as_bytes(),
                "refs/heads/main"
            )
            .is_err()
        );
    }

    #[test]
    fn sync_history_parser_and_classification_are_conservative() {
        let local_head = "1111111111111111111111111111111111111111";
        let remote_head = "2222222222222222222222222222222222222222";
        let common = "3333333333333333333333333333333333333333";
        let history = |ids: &[&str], complete| GitSyncHistory {
            object_ids: ids.iter().map(|id| (*id).to_owned()).collect(),
            complete,
        };

        assert_eq!(
            classify_git_sync_histories(
                local_head,
                local_head,
                &history(&[local_head], true),
                &history(&[local_head], true),
            ),
            GitSyncAncestry::UpToDate
        );
        assert_eq!(
            classify_git_sync_histories(
                local_head,
                remote_head,
                &history(&[local_head, common], true),
                &history(&[remote_head, local_head, common], true),
            ),
            GitSyncAncestry::RemoteFastForward
        );
        assert_eq!(
            classify_git_sync_histories(
                local_head,
                remote_head,
                &history(&[local_head, remote_head, common], true),
                &history(&[remote_head, common], true),
            ),
            GitSyncAncestry::LocalAhead
        );
        assert_eq!(
            classify_git_sync_histories(
                local_head,
                remote_head,
                &history(&[local_head, common], true),
                &history(&[remote_head, common], true),
            ),
            GitSyncAncestry::Diverged
        );
        assert_eq!(
            classify_git_sync_histories(
                local_head,
                remote_head,
                &history(&[local_head], false),
                &history(&[remote_head], true),
            ),
            GitSyncAncestry::HistoryLimit
        );

        let parsed = parse_git_sync_history(
            format!("{remote_head}\n{common}\n").as_bytes(),
            GitObjectFormat::Sha1,
            remote_head,
            false,
        )
        .unwrap();
        assert_eq!(parsed.object_ids, vec![remote_head, common]);
        assert!(parsed.complete);
        assert_eq!(parse_git_shallow_state(b"true\n"), Ok(true));
        assert_eq!(parse_git_shallow_state(b"false\n"), Ok(false));
        assert!(parse_git_shallow_state(b"true").is_err());
        assert!(
            parse_git_sync_history(
                format!("{remote_head}\n{remote_head}\n").as_bytes(),
                GitObjectFormat::Sha1,
                remote_head,
                false,
            )
            .is_err()
        );
        assert!(
            parse_git_sync_history(
                format!("{common}\n").as_bytes(),
                GitObjectFormat::Sha1,
                remote_head,
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn sync_plan_is_bound_and_redacts_remote_authority() {
        let temp = Temp::new();
        let state = Temp::new();
        let workspace = configured_sync_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let debug = format!("{plan:?}");

        assert_eq!(plan.remote_name(), "origin");
        assert_eq!(plan.host(), "example.invalid");
        assert_eq!(plan.remote_branch(), "main");
        assert_eq!(plan.transport(), GitRemoteTransport::Https);
        assert!(!debug.contains("https://"));
        let token = serde_json::to_string(plan.token()).unwrap();
        assert!(!debug.contains(token.trim_matches('"')));
        assert!(!debug.contains(plan.preflight.branch.expected_object_id.as_ref().unwrap()));

        let shallow = temp.0.join(".git/shallow");
        fs::write(
            &shallow,
            format!(
                "{}\n",
                plan.preflight.branch.expected_object_id.as_ref().unwrap()
            ),
        )
        .unwrap();
        assert_eq!(
            revalidate_git_sync_plan(&open_workspace(&temp), &plan),
            Err(GitSyncError::PlanStale)
        );
        fs::remove_file(shallow).unwrap();

        fixture_git(&temp, &["config", "branch.main.merge", "refs/heads/other"]);
        assert_eq!(
            revalidate_git_sync_plan(&open_workspace(&temp), &plan),
            Err(GitSyncError::PlanStale)
        );
    }

    #[test]
    fn push_refuses_anything_except_a_validated_local_ahead_authority() {
        let (_temp, _state, workspace, classification, _remote_head) =
            clean_sync_publication_fixture("Remote");
        assert_eq!(
            apply_git_push(&workspace, classification),
            Err(GitSyncError::Unsupported)
        );
    }

    #[test]
    fn push_binds_remote_preflight_and_requires_an_exact_postflight() {
        let (_temp, _state, workspace, mut classification, expected_remote) =
            clean_sync_publication_fixture("Remote");
        classification.ancestry = GitSyncAncestry::LocalAhead;
        let local_commit = classification
            .inspection
            .plan
            .preflight
            .branch
            .expected_object_id
            .clone()
            .unwrap();
        let remote_ref = classification.inspection.plan.remote_ref.clone();
        let calls = std::cell::Cell::new(0_u8);
        let published = std::cell::Cell::new(false);
        let outcome = apply_git_push_with(
            &workspace,
            classification,
            |_| {
                let object_id = if calls.replace(calls.get() + 1) == 0 {
                    expected_remote.clone()
                } else {
                    local_commit.clone()
                };
                Ok(GitRemoteHead {
                    branch_ref: remote_ref.clone(),
                    object_id,
                    object_format: GitObjectFormat::Sha1,
                })
            },
            |_, _, exact_local| {
                assert_eq!(exact_local, local_commit);
                published.set(true);
                Ok(())
            },
        )
        .unwrap();
        assert!(outcome.pushed());
        assert!(published.get());
        assert_eq!(calls.get(), 2);

        let (_temp, _state, workspace, mut classification, expected_remote) =
            clean_sync_publication_fixture("Remote");
        classification.ancestry = GitSyncAncestry::LocalAhead;
        let changed_remote = "f".repeat(40);
        let published = std::cell::Cell::new(false);
        assert_eq!(
            apply_git_push_with(
                &workspace,
                classification,
                |_| {
                    Ok(GitRemoteHead {
                        branch_ref: "refs/heads/main".to_owned(),
                        object_id: changed_remote.clone(),
                        object_format: GitObjectFormat::Sha1,
                    })
                },
                |_, _, _| {
                    published.set(true);
                    Ok(())
                },
            ),
            Err(GitSyncError::RemoteChanged)
        );
        assert_ne!(changed_remote, expected_remote);
        assert!(!published.get());

        let (_temp, _state, workspace, mut classification, _expected_remote) =
            clean_sync_publication_fixture("Remote");
        classification.ancestry = GitSyncAncestry::LocalAhead;
        let published = std::cell::Cell::new(false);
        assert_eq!(
            apply_git_push_with(
                &workspace,
                classification,
                |_| Err(GitSyncError::RemoteChanged),
                |_, _, _| {
                    published.set(true);
                    Ok(())
                },
            ),
            Err(GitSyncError::RemoteChanged)
        );
        assert!(!published.get());
    }

    #[test]
    fn push_reports_ambiguous_postflight_without_claiming_failure_or_success() {
        let (_temp, _state, workspace, mut classification, expected_remote) =
            clean_sync_publication_fixture("Remote");
        classification.ancestry = GitSyncAncestry::LocalAhead;
        let calls = std::cell::Cell::new(0_u8);
        assert_eq!(
            apply_git_push_with(
                &workspace,
                classification,
                |_| {
                    if calls.replace(calls.get() + 1) == 0 {
                        Ok(GitRemoteHead {
                            branch_ref: "refs/heads/main".to_owned(),
                            object_id: expected_remote.clone(),
                            object_format: GitObjectFormat::Sha1,
                        })
                    } else {
                        Err(GitSyncError::RemoteUnavailable)
                    }
                },
                |_, _, _| Err(GitSyncError::Timeout),
            ),
            Err(GitSyncError::RemoteStateUncertain)
        );

        let (_temp, _state, workspace, mut classification, expected_remote) =
            clean_sync_publication_fixture("Remote");
        classification.ancestry = GitSyncAncestry::LocalAhead;
        assert_eq!(
            apply_git_push_with(
                &workspace,
                classification,
                |_| {
                    Ok(GitRemoteHead {
                        branch_ref: "refs/heads/main".to_owned(),
                        object_id: expected_remote.clone(),
                        object_format: GitObjectFormat::Sha1,
                    })
                },
                |_, _, _| Err(GitSyncError::RemoteUnavailable),
            ),
            Err(GitSyncError::RemoteUnavailable)
        );

        let (_temp, _state, workspace, mut classification, expected_remote) =
            clean_sync_publication_fixture("Remote");
        classification.ancestry = GitSyncAncestry::LocalAhead;
        let calls = std::cell::Cell::new(0_u8);
        assert_eq!(
            apply_git_push_with(
                &workspace,
                classification,
                |_| {
                    if calls.replace(calls.get() + 1) == 0 {
                        Ok(GitRemoteHead {
                            branch_ref: "refs/heads/main".to_owned(),
                            object_id: expected_remote.clone(),
                            object_format: GitObjectFormat::Sha1,
                        })
                    } else {
                        Err(GitSyncError::RemoteChanged)
                    }
                },
                |_, _, _| Ok(()),
            ),
            Err(GitSyncError::RemoteChanged)
        );
    }

    #[test]
    fn sync_validates_remote_snapshot_without_touching_active_workspace() {
        let temp = Temp::new();
        let state = Temp::new();
        let workspace = configured_sync_workspace(&temp);
        let before_manifest = fs::read(temp.0.join("rehow.yaml")).unwrap();
        let before_head = fixture_git_stdout(&temp, &["rev-parse", "HEAD"]);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();

        let source = Temp::new();
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Remote\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let commit_id = commit_clone_fixture(&source, remote_manifest);
        let remote_head = GitRemoteHead {
            branch_ref: "refs/heads/main".to_owned(),
            object_id: commit_id.clone(),
            object_format: GitObjectFormat::Sha1,
        };
        let staging = prepare_git_sync_staging(&plan, &remote_head, &state.0).unwrap();
        let mut fetch = Command::new("/usr/bin/git");
        fetch
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args([
                "--no-pager",
                "--literal-pathspecs",
                &format!("--git-dir={}", staging.git_path()),
                "-c",
                "protocol.file.allow=always",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "fetch.unpackLimit=1",
                "-c",
                "transfer.unpackLimit=1",
                "-c",
                "pack.writeReverseIndex=false",
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-write-fetch-head",
            ])
            .arg(source.0.to_str().unwrap())
            .arg(format!("{commit_id}:refs/rehow/clone/head"));
        configure_clone_child(&mut fetch, staging.git_descriptor(), None);
        assert!(fetch.status().unwrap().success());

        let mut inspection = finish_git_sync_inspection(
            &workspace,
            plan,
            staging,
            remote_head,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(inspection.state(), GitSyncInspectionState::RemoteChanged);
        assert!(!inspection.local_changes());
        assert!(!inspection.workspace_matches_remote());
        assert_eq!(inspection.validated.workspace().manifest().name(), "Remote");
        assert_eq!(
            fs::read(temp.0.join("rehow.yaml")).unwrap(),
            before_manifest
        );
        assert_eq!(
            fixture_git_stdout(&temp, &["rev-parse", "HEAD"]),
            before_head
        );
        assert!(!format!("{inspection:?}").contains(&commit_id));

        inspection.remote_head.object_id = inspection
            .plan
            .preflight
            .branch
            .expected_object_id
            .clone()
            .unwrap();
        assert!(matches!(
            classify_git_sync_ancestry(&workspace, inspection, &AtomicBool::new(true)),
            Err(GitSyncError::Cancelled)
        ));
    }

    #[test]
    fn sync_materialization_updates_creates_and_trashes_without_publishing_git() {
        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        write_fixture_guide(&temp);
        let removed_path = temp
            .0
            .join("categories/linux-system--b6ba3a5551/remove--36c5735e11.md");
        fs::write(
            &removed_path,
            b"---\nschema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e11\ntitle: Remove Me\nsummary: This Guide exists only locally.\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags: []\naliases: []\nstatus: draft\ncreated_at: \"2026-09-05T10:04:00Z\"\nupdated_at: \"2026-09-05T10:04:00Z\"\n---\n\n## Steps\n\nRemove it.\n",
        )
        .unwrap();
        let removed_category = temp.0.join("categories/obsolete--b6ba3a5552");
        fs::create_dir(&removed_category).unwrap();
        fs::write(
            removed_category.join("category.yaml"),
            b"schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5552\nname: Obsolete\norder: 2000\nstatus: active\ncreated_at: \"2026-09-05T10:05:00Z\"\nupdated_at: \"2026-09-05T10:05:00Z\"\n",
        )
        .unwrap();
        fixture_git(&temp, &["add", "--all", "--"]);
        fixture_git(&temp, &["commit", "-qm", "local guides"]);

        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let before_head = fixture_git_stdout(&temp, &["rev-parse", "HEAD"]);

        let source = Temp::new();
        write_fixture_guide(&source);
        let remote_guide_path = source
            .0
            .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md");
        let remote_guide = fs::read_to_string(&remote_guide_path)
            .unwrap()
            .replace("Extract it.", "Extract the remote archive.");
        fs::write(&remote_guide_path, remote_guide).unwrap();
        fs::write(source.0.join(".gitignore"), b"*.tmp\n").unwrap();
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Remote\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };

        let (fresh, materialization) =
            apply_git_sync_workspace_materialization(&workspace, &state.0, None, classification)
                .unwrap();

        assert!(materialization.changed());
        assert_eq!(fresh.manifest().name(), "Remote");
        assert_eq!(fs::read(temp.0.join(".gitignore")).unwrap(), b"*.tmp\n");
        assert!(
            fs::read_to_string(&remote_guide_path)
                .unwrap()
                .contains("remote archive")
        );
        assert!(
            fs::read_to_string(
                temp.0
                    .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md")
            )
            .unwrap()
            .contains("remote archive")
        );
        assert!(!removed_path.exists());
        assert!(!removed_category.exists());
        assert!(
            temp.0
                .join(format!(
                    ".rehow/trash/{}/categories/linux-system--b6ba3a5551/remove--36c5735e11.md",
                    materialization.operation_id
                ))
                .is_file()
        );
        assert!(
            temp.0
                .join(format!(
                    ".rehow/trash/{}/categories/obsolete--b6ba3a5552/category.yaml",
                    materialization.operation_id
                ))
                .is_file()
        );
        assert_eq!(
            fixture_git_stdout(&temp, &["rev-parse", "HEAD"]),
            before_head
        );
        let debug = format!("{materialization:?}");
        assert!(!debug.contains(&materialization.operation_id.to_string()));
        assert!(!debug.contains("Remote"));
    }

    #[test]
    fn sync_fast_forward_durably_publishes_workspace_index_and_branch() {
        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        fs::write(temp.0.join(".gitignore"), b".rehow/\n").unwrap();
        fixture_git(&temp, &["add", "--", ".gitignore"]);
        fixture_git(&temp, &["commit", "-qm", "ignore recovery trash"]);
        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let source = Temp::new();
        write_fixture_guide(&source);
        fs::write(source.0.join(".gitignore"), b".rehow/\n").unwrap();
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Remote\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let remote_head = inspection.remote_head.object_id().to_owned();
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };

        let (fresh, outcome) =
            apply_git_sync_fast_forward(&workspace, &state.0, None, classification).unwrap();

        assert!(outcome.changed());
        assert_eq!(fresh.manifest().name(), "Remote");
        assert_eq!(
            fixture_git_stdout(&temp, &["rev-parse", "HEAD"]),
            remote_head
        );
        let status = fixture_git_stdout(&temp, &["status", "--porcelain=v1"]);
        assert!(status.is_empty(), "unexpected status: {status:?}");
        let session = fresh
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        let intent = session.read_sync_intent(outcome.operation_id).unwrap();
        assert_eq!(intent.stage, GitSyncIntentStage::Complete);
        let debug = format!("{outcome:?}");
        assert!(!debug.contains(&outcome.operation_id.to_string()));
        assert!(!debug.contains(&remote_head));
    }

    #[test]
    fn sync_fast_forward_publishes_a_new_commit_with_no_portable_change() {
        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        fs::write(temp.0.join(".gitignore"), b".rehow/\n").unwrap();
        fixture_git(&temp, &["add", "--", ".gitignore"]);
        fixture_git(&temp, &["commit", "-qm", "ignore recovery trash"]);
        let manifest = fs::read(temp.0.join("rehow.yaml")).unwrap();
        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let source = Temp::new();
        fs::write(source.0.join(".gitignore"), b".rehow/\n").unwrap();
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, &manifest);
        assert!(inspection.workspace_matches_remote());
        let remote_head = inspection.remote_head.object_id().to_owned();
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };

        let (fresh, outcome) =
            apply_git_sync_fast_forward(&workspace, &state.0, None, classification).unwrap();

        assert!(outcome.changed());
        assert_eq!(fresh.manifest().name(), workspace.manifest().name());
        assert_eq!(
            fixture_git_stdout(&temp, &["rev-parse", "HEAD"]),
            remote_head
        );
        let session = fresh
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        let intent = session.read_sync_intent(outcome.operation_id).unwrap();
        assert!(intent.workspace_journal_hash.is_none());
        assert_eq!(intent.stage, GitSyncIntentStage::Complete);
    }

    #[test]
    fn sync_fast_forward_resumes_after_index_publication_interruption() {
        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        fs::write(temp.0.join(".gitignore"), b".rehow/\n").unwrap();
        fixture_git(&temp, &["add", "--", ".gitignore"]);
        fixture_git(&temp, &["commit", "-qm", "ignore recovery trash"]);
        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let source = Temp::new();
        write_fixture_guide(&source);
        fs::write(source.0.join(".gitignore"), b".rehow/\n").unwrap();
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Resumed\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let remote_head = inspection.remote_head.object_id().to_owned();
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };
        set_git_commit_metadata_fault(GitCommitMetadataFault::IndexPublished);

        assert_eq!(
            apply_git_sync_fast_forward(&workspace, &state.0, None, classification)
                .err()
                .unwrap(),
            GitSyncError::Unsupported
        );
        assert_eq!(
            plan_git_sync(&workspace, &state.0).err().unwrap(),
            GitSyncError::RecoveryRequired
        );
        assert_eq!(
            plan_git_commit(&workspace, &state.0, "must wait")
                .err()
                .unwrap(),
            GitCommitPlanError::OperationInProgress
        );
        let session = workspace
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        let intents = session.list_sync_intents().unwrap();
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].stage, GitSyncIntentStage::IndexPublished);
        let operation_id = intents[0].operation_id;
        drop(session);

        let fresh = resume_git_sync_publication(&workspace, &state.0, None, operation_id).unwrap();

        assert_eq!(fresh.manifest().name(), "Resumed");
        assert_eq!(
            fixture_git_stdout(&temp, &["rev-parse", "HEAD"]),
            remote_head
        );
        assert!(fixture_git_stdout(&temp, &["status", "--porcelain=v1"]).is_empty());
        let session = fresh
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        assert_eq!(
            session.read_sync_intent(operation_id).unwrap().stage,
            GitSyncIntentStage::Complete
        );
    }

    #[test]
    fn sync_fast_forward_resumes_every_git_metadata_interruption() {
        for fault in [
            GitCommitMetadataFault::IndexLockPublished,
            GitCommitMetadataFault::IndexExchanged,
            GitCommitMetadataFault::IndexPublished,
            GitCommitMetadataFault::RefLockPublished,
            GitCommitMetadataFault::RefExchanged,
            GitCommitMetadataFault::RefPublished,
        ] {
            let (temp, state, workspace, classification, remote_head) =
                clean_sync_publication_fixture("Interrupted");
            set_git_commit_metadata_fault(fault);
            assert!(
                apply_git_sync_fast_forward(&workspace, &state.0, None, classification).is_err(),
                "fault {fault:?}"
            );
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            let intents = session.list_sync_intents().unwrap();
            assert_eq!(intents.len(), 1, "fault {fault:?}");
            let operation_id = intents[0].operation_id;
            drop(session);

            let fresh =
                resume_git_sync_publication(&workspace, &state.0, None, operation_id).unwrap();

            assert_eq!(fresh.manifest().name(), "Interrupted", "fault {fault:?}");
            assert_eq!(
                fixture_git_stdout(&temp, &["rev-parse", "HEAD"]),
                remote_head,
                "fault {fault:?}"
            );
            let session = fresh
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            assert_eq!(
                session.read_sync_intent(operation_id).unwrap().stage,
                GitSyncIntentStage::Complete,
                "fault {fault:?}"
            );
        }
    }

    #[test]
    fn sync_fast_forward_publishes_a_loose_shadow_for_a_packed_current_branch() {
        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        fs::write(temp.0.join(".gitignore"), b".rehow/\n").unwrap();
        fixture_git(&temp, &["add", "--", ".gitignore"]);
        fixture_git(&temp, &["commit", "-qm", "ignore recovery trash"]);
        fixture_git(&temp, &["pack-refs", "--all", "--prune"]);
        let before_head = fixture_git_stdout(&temp, &["rev-parse", "HEAD"]);
        let packed_refs_before = fs::read(temp.0.join(".git/packed-refs")).unwrap();
        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let source = Temp::new();
        write_fixture_guide(&source);
        fs::write(source.0.join(".gitignore"), b".rehow/\n").unwrap();
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Packed refusal\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let remote_head = inspection.remote_head.object_id().to_owned();
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };

        let (fresh, outcome) =
            apply_git_sync_fast_forward(&workspace, &state.0, None, classification).unwrap();
        assert!(outcome.changed());
        assert_ne!(remote_head, before_head);
        assert_eq!(
            fixture_git_stdout(&temp, &["rev-parse", "HEAD"]),
            remote_head
        );
        assert_eq!(
            fs::read(temp.0.join(".git/packed-refs")).unwrap(),
            packed_refs_before
        );
        assert!(temp.0.join(".git/refs/heads/main").is_file());
        let capability = fresh.git_repository_capability().unwrap();
        let repository = capability.read_commit_repository_snapshot().unwrap();
        let branch = capability.read_commit_branch_snapshot("main").unwrap();
        assert_eq!(
            parse_git_commit_branch(
                &repository.head.bytes,
                branch.target.as_ref().map(|target| target.bytes.as_slice()),
                repository
                    .packed_refs
                    .as_ref()
                    .map(|packed| packed.bytes.as_slice()),
                GitObjectFormat::Sha1,
            )
            .unwrap()
            .representation,
            GitBranchRepresentation::Loose
        );
        let session = fresh
            .open_git_commit_recovery_session(&state.0, None)
            .unwrap();
        assert_eq!(
            session
                .read_sync_intent(outcome.operation_id)
                .unwrap()
                .stage,
            GitSyncIntentStage::Complete
        );
    }

    #[test]
    fn sync_fast_forward_preserves_post_ref_workspace_and_ref_changes_as_conflicts() {
        for mutate_ref in [false, true] {
            let (temp, state, workspace, classification, _) =
                clean_sync_publication_fixture("Conflict");
            set_git_commit_metadata_fault(GitCommitMetadataFault::RefPublished);
            assert!(
                apply_git_sync_fast_forward(&workspace, &state.0, None, classification).is_err()
            );
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            let intent = session.list_sync_intents().unwrap().remove(0);
            assert_eq!(intent.stage, GitSyncIntentStage::RefPublished);
            drop(session);
            let (path, foreign) = if mutate_ref {
                (
                    temp.0.join(".git/refs/heads/main"),
                    b"foreign-ref\n".to_vec(),
                )
            } else {
                (
                    temp.0.join("rehow.yaml"),
                    fs::read_to_string(temp.0.join("rehow.yaml"))
                        .unwrap()
                        .replace("name: Conflict", "name: Foreign")
                        .into_bytes(),
                )
            };
            fs::write(&path, &foreign).unwrap();

            let resume_error =
                resume_git_sync_publication(&workspace, &state.0, None, intent.operation_id)
                    .err()
                    .unwrap();
            assert_eq!(
                resume_error,
                GitSyncError::PlanStale,
                "mutate_ref={mutate_ref}"
            );
            assert_eq!(fs::read(&path).unwrap(), foreign);
            let session = workspace
                .open_git_commit_recovery_session(&state.0, None)
                .unwrap();
            assert_eq!(
                session.read_sync_intent(intent.operation_id).unwrap().stage,
                GitSyncIntentStage::Conflict
            );
        }
    }

    #[test]
    fn sync_materialization_preserves_a_replacement_directory_during_cleanup() {
        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        let removed_category = temp.0.join("categories/obsolete--b6ba3a5552");
        fs::create_dir(&removed_category).unwrap();
        fs::write(
            removed_category.join("category.yaml"),
            b"schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5552\nname: Obsolete\norder: 2000\nstatus: active\ncreated_at: \"2026-09-05T10:05:00Z\"\nupdated_at: \"2026-09-05T10:05:00Z\"\n",
        )
        .unwrap();
        fixture_git(&temp, &["add", "--all", "--"]);
        fixture_git(&temp, &["commit", "-qm", "obsolete category"]);
        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();

        let source = Temp::new();
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Remote\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };
        set_git_sync_directory_race(GitSyncDirectoryRace::ReplaceCategory);

        assert_eq!(
            apply_git_sync_workspace_materialization(&workspace, &state.0, None, classification,)
                .err()
                .unwrap(),
            GitSyncError::PlanStale
        );
        assert!(temp.0.join("categories/.rehow-race-original").is_dir());
        let operation = fs::read_dir(temp.0.join(".rehow/trash"))
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| entry.path().join("directories").is_dir())
            .unwrap()
            .path();
        assert_eq!(
            fs::read(
                operation.join("directories/categories/obsolete--b6ba3a5552/replacement-marker")
            )
            .unwrap(),
            b"preserve"
        );
    }

    #[test]
    fn sync_materialization_preserves_a_replacement_attachment_directory() {
        use sha2::Digest as _;

        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        write_fixture_guide(&temp);
        let bytes = b"attachment\n";
        let sha = hex_lower(&sha2::Sha256::digest(bytes));
        let guide_path = temp
            .0
            .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md");
        let guide = fs::read_to_string(&guide_path).unwrap().replace(
            "aliases: []\nstatus:",
            &format!(
                "aliases: []\nattachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.txt\n    title: Example\n    sha256: {sha}\n    size_bytes: {}\nstatus:",
                bytes.len()
            ),
        );
        fs::write(&guide_path, guide).unwrap();
        let attachment_directory = temp
            .0
            .join("attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10");
        fs::create_dir_all(&attachment_directory).unwrap();
        fs::write(attachment_directory.join("example.txt"), bytes).unwrap();
        fixture_git(&temp, &["add", "--all", "--"]);
        fixture_git(&temp, &["commit", "-qm", "local attachment"]);
        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();

        let source = Temp::new();
        write_fixture_guide(&source);
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Remote\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };
        set_git_sync_directory_race(GitSyncDirectoryRace::ReplaceAttachment);

        assert_eq!(
            apply_git_sync_workspace_materialization(&workspace, &state.0, None, classification,)
                .err()
                .unwrap(),
            GitSyncError::PlanStale
        );
        assert!(temp.0.join("attachments/.rehow-race-original").is_dir());
        let operation = fs::read_dir(temp.0.join(".rehow/trash"))
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| entry.path().join("directories").is_dir())
            .unwrap()
            .path();
        assert_eq!(
            fs::read(operation.join(
                "directories/attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/replacement-marker"
            ))
            .unwrap(),
            b"preserve"
        );
    }

    #[test]
    fn sync_materialization_quarantines_an_empty_new_directory_after_failure() {
        let temp = Temp::new();
        let state = Temp::new();
        let workspace = configured_sync_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let source = Temp::new();
        write_fixture_guide(&source);
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Remote\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };
        set_git_sync_fail_after_directory_preparation();

        assert_eq!(
            apply_git_sync_workspace_materialization(&workspace, &state.0, None, classification,)
                .err()
                .unwrap(),
            GitSyncError::PlanStale
        );
        assert!(!temp.0.join("categories/linux-system--b6ba3a5551").exists());
        let operation = fs::read_dir(temp.0.join(".rehow/trash"))
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| entry.path().join("directories").is_dir())
            .unwrap()
            .path();
        let quarantined = operation.join("directories/categories/linux-system--b6ba3a5551");
        assert!(quarantined.is_dir());
        assert!(fs::read_dir(quarantined).unwrap().next().is_none());
    }

    #[test]
    fn sync_materialization_quarantines_a_new_attachment_root_after_failure() {
        use sha2::Digest as _;

        let temp = Temp::new();
        let state = Temp::new();
        let workspace = configured_sync_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();
        let source = Temp::new();
        write_fixture_guide(&source);
        let guide_id = "0199a1d7-7b0d-7d8e-a38d-f936c5735e10";
        let bytes = b"attachment\n";
        let sha = hex_lower(&sha2::Sha256::digest(bytes));
        let guide_path = source
            .0
            .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md");
        let guide = fs::read_to_string(&guide_path).unwrap().replace(
            "aliases: []\nstatus:",
            &format!(
                "aliases: []\nattachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/{guide_id}/example.txt\n    title: Example\n    sha256: {sha}\n    size_bytes: {}\nstatus:",
                bytes.len()
            ),
        );
        fs::write(&guide_path, guide).unwrap();
        let source_attachment_directory = source.0.join(format!("attachments/{guide_id}"));
        fs::create_dir_all(&source_attachment_directory).unwrap();
        fs::write(source_attachment_directory.join("example.txt"), bytes).unwrap();
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Remote\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };
        set_git_sync_fail_after_directory_preparation();

        assert_eq!(
            apply_git_sync_workspace_materialization(&workspace, &state.0, None, classification,)
                .err()
                .unwrap(),
            GitSyncError::PlanStale
        );
        assert!(!temp.0.join("attachments").exists());
        let operation = fs::read_dir(temp.0.join(".rehow/trash"))
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| entry.path().join("directories").is_dir())
            .unwrap()
            .path();
        let leaf = operation.join(format!("directories/attachments/{guide_id}"));
        let root = operation.join("directories/roots/attachments");
        assert!(leaf.is_dir());
        assert!(root.is_dir());
        assert!(fs::read_dir(leaf).unwrap().next().is_none());
        assert!(fs::read_dir(root).unwrap().next().is_none());
    }

    #[test]
    fn sync_materialization_preserves_a_preexisting_empty_attachment_directory() {
        use sha2::Digest as _;

        let temp = Temp::new();
        let state = Temp::new();
        drop(configured_sync_workspace(&temp));
        write_fixture_guide(&temp);
        fixture_git(&temp, &["add", "--all", "--"]);
        fixture_git(&temp, &["commit", "-qm", "local guide"]);
        let guide_id = "0199a1d7-7b0d-7d8e-a38d-f936c5735e10";
        let attachment_directory = temp.0.join(format!("attachments/{guide_id}"));
        fs::create_dir_all(&attachment_directory).unwrap();
        let workspace = open_workspace(&temp);
        let plan = plan_git_sync(&workspace, &state.0).unwrap();

        let source = Temp::new();
        write_fixture_guide(&source);
        let bytes = b"attachment\n";
        let sha = hex_lower(&sha2::Sha256::digest(bytes));
        let guide_path = source
            .0
            .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md");
        let guide = fs::read_to_string(&guide_path).unwrap().replace(
            "aliases: []\nstatus:",
            &format!(
                "aliases: []\nattachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/{guide_id}/example.txt\n    title: Example\n    sha256: {sha}\n    size_bytes: {}\nstatus:",
                bytes.len()
            ),
        );
        fs::write(&guide_path, guide).unwrap();
        let source_attachment_directory = source.0.join(format!("attachments/{guide_id}"));
        fs::create_dir_all(&source_attachment_directory).unwrap();
        fs::write(source_attachment_directory.join("example.txt"), bytes).unwrap();
        let remote_manifest = b"schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Remote\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T01:00:00Z\"\n";
        let inspection = stage_sync_fixture(&workspace, plan, &state, &source, remote_manifest);
        let classification = GitSyncClassification {
            inspection,
            ancestry: GitSyncAncestry::RemoteFastForward,
        };
        set_git_sync_fail_after_directory_preparation();

        assert_eq!(
            apply_git_sync_workspace_materialization(&workspace, &state.0, None, classification,)
                .err()
                .unwrap(),
            GitSyncError::PlanStale
        );
        assert!(attachment_directory.is_dir());
        assert!(
            fs::read_dir(&attachment_directory)
                .unwrap()
                .next()
                .is_none()
        );
        let trash = temp.0.join(".rehow/trash");
        if trash.exists() {
            for operation in fs::read_dir(trash).unwrap() {
                assert!(
                    !operation
                        .unwrap()
                        .path()
                        .join(format!("directories/attachments/{guide_id}"))
                        .exists()
                );
            }
        }
    }

    #[test]
    #[ignore = "explicit public-network clone sandbox smoke test"]
    fn clone_public_https_fetch_stays_in_private_staging() {
        let destination = Temp::new();
        let state = Temp::new();
        let target = empty_target(&destination);
        let plan = plan_git_clone(&target, "https://github.com/octocat/Hello-World.git").unwrap();
        let cancelled = AtomicBool::new(false);
        let head =
            discover_git_clone_remote_head(&plan, &cancelled, GitRemoteAuthentication::Anonymous)
                .unwrap();
        let staging = prepare_git_clone_staging(&target, &plan, &head, &state.0).unwrap();
        fetch_git_clone_remote(
            &plan,
            &head,
            &staging,
            &cancelled,
            GitRemoteAuthentication::Anonymous,
        )
        .unwrap();
        assert!(destination.0.read_dir().unwrap().next().is_none());
        assert_eq!(
            fs::read_to_string(Path::new(staging.git_path()).join("refs/rehow/clone/head"))
                .unwrap(),
            format!("{}\n", head.object_id())
        );
        assert!(!Path::new(staging.git_path()).join("index").exists());
        assert_eq!(
            inspect_fetched_git_clone_tree(&staging, &head, &cancelled),
            Err(GitCloneInspectError::Tree(GitCloneTreeError::UnsafePath))
        );
        assert!(destination.0.read_dir().unwrap().next().is_none());
    }

    #[test]
    fn remote_name_and_url_grammar_is_conservative_and_secret_free() {
        for name in ["origin", "upstream-2", "team.mirror", "a_b"] {
            assert!(valid_remote_name(name), "{name}");
        }
        for name in ["", ".origin", "Origin", "a..b", "name.lock", "slash/name"] {
            assert!(!valid_remote_name(name), "{name}");
        }
        assert!(valid_remote_name(&format!("a{}", "b".repeat(63))));
        assert!(!valid_remote_name(&format!("a{}", "b".repeat(64))));
        for (value, transport, host) in [
            (
                "https://github.com/example/rehow.git",
                GitRemoteTransport::Https,
                "github.com",
            ),
            (
                "ssh://git@github.com/example/rehow.git",
                GitRemoteTransport::Ssh,
                "github.com",
            ),
            (
                "git@github.com:example/rehow.git",
                GitRemoteTransport::Ssh,
                "github.com",
            ),
        ] {
            let parsed = validate_remote_url(value).unwrap();
            assert_eq!(parsed.canonical, value);
            assert_eq!(parsed.transport, transport);
            assert_eq!(parsed.host, host);
        }
        for value in [
            "http://github.com/example/rehow.git",
            "https://user@github.com/example/rehow.git",
            "https://github.com/example/rehow.git?token=secret",
            "ssh://git:secret@github.com/example/rehow.git",
            "ssh://git@GitHub.com/example/rehow.git",
            "https://[::1]/example/rehow.git",
            "file:///tmp/rehow.git",
            "git@github.com:../rehow.git",
            "git@github.com:example/%72ehow.git",
            "-uploader pack",
        ] {
            assert_eq!(
                validate_remote_url(value).unwrap_err(),
                GitRemoteError::InvalidUrl,
                "{value}"
            );
        }
        fn sized_https_url(length: usize) -> String {
            let mut value = "https://example.invalid/".to_owned();
            while length.saturating_sub(value.len()) > 255 {
                value.push_str(&"a".repeat(255));
                value.push('/');
            }
            value.push_str(&"a".repeat(length - value.len()));
            value
        }
        let exact = sized_https_url(MAX_REMOTE_URL_BYTES);
        let over = sized_https_url(MAX_REMOTE_URL_BYTES + 1);
        assert_eq!(exact.len(), MAX_REMOTE_URL_BYTES);
        assert!(validate_remote_url(&exact).is_ok());
        assert_eq!(
            validate_remote_url(&over).unwrap_err(),
            GitRemoteError::InvalidUrl
        );
    }

    #[test]
    fn connects_remote_without_network_and_preserves_existing_config_bytes() {
        let temp = Temp::new();
        let workspace = initialized_workspace(&temp);
        let config_path = temp.0.join(".git/config");
        let before = fs::read(&config_path).unwrap();
        let remote_url = "https://offline.example.invalid/team/rehow.git";
        let plan = plan_git_remote_connect(&workspace, "origin", remote_url).unwrap();
        assert_eq!(plan.remote_name(), "origin");
        assert_eq!(plan.transport(), GitRemoteTransport::Https);
        assert_eq!(plan.host(), "offline.example.invalid");
        let debug = format!("{plan:?}");
        assert!(!debug.contains(remote_url));
        assert!(!debug.contains("team/rehow"));
        apply_git_remote_connect(&workspace, &plan).unwrap();

        let after = fs::read(&config_path).unwrap();
        assert!(after.starts_with(&before));
        assert_eq!(
            &after[before.len()..],
            b"[remote \"origin\"]\n\turl = \"https://offline.example.invalid/team/rehow.git\"\n"
        );
        assert!(!temp.0.join(".git/config.lock").exists());
        assert_eq!(config_backups(&temp), vec![before]);
        assert!(
            validate_git_config(&after)
                .unwrap()
                .iter()
                .any(|key| key == "remote.origin.url")
        );
        assert!(matches!(
            plan_git_remote_connect(&workspace, "origin", remote_url),
            Err(GitRemoteError::DuplicateRemote)
        ));
    }

    #[test]
    fn remote_connect_rejects_stale_and_unrelated_config_state() {
        let temp = Temp::new();
        let workspace = initialized_workspace(&temp);
        let plan =
            plan_git_remote_connect(&workspace, "origin", "git@example.invalid:team/rehow.git")
                .unwrap();
        let config_path = temp.0.join(".git/config");
        let mut changed = fs::read(&config_path).unwrap();
        changed.extend_from_slice(b"[safe]\n\tdirectory = /unrelated\n");
        fs::write(&config_path, &changed).unwrap();
        assert_eq!(
            apply_git_remote_connect(&workspace, &plan),
            Err(GitRemoteError::PlanStale)
        );
        assert_eq!(fs::read(&config_path).unwrap(), changed);

        fs::write(temp.0.join(".git/config.lock"), b"unrelated\n").unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "backup",
                "ssh://git@example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::RecoveryRequired)
        ));
        assert_eq!(
            fs::read(temp.0.join(".git/config.lock")).unwrap(),
            b"unrelated\n"
        );
    }

    #[test]
    fn same_remote_request_resumes_both_atomic_interruption_states() {
        for fault in [GitConfigFault::LockPublished, GitConfigFault::Exchanged] {
            let temp = Temp::new();
            let workspace = initialized_workspace(&temp);
            let url = "ssh://git@example.invalid/team/rehow.git";
            let plan = plan_git_remote_connect(&workspace, "origin", url).unwrap();
            set_git_config_fault(fault);
            assert_eq!(
                apply_git_remote_connect(&workspace, &plan),
                Err(GitRemoteError::Unsupported)
            );
            assert!(temp.0.join(".git/config.lock").exists());

            let reopened = open_workspace(&temp);
            let resumed = plan_git_remote_connect(&reopened, "origin", url).unwrap();
            apply_git_remote_connect(&reopened, &resumed).unwrap();
            assert!(!temp.0.join(".git/config.lock").exists());
            let config = fs::read_to_string(temp.0.join(".git/config")).unwrap();
            assert_eq!(config.matches("[remote \"origin\"]").count(), 1);
            assert_eq!(config_backups(&temp).len(), 1);
        }

        let committed = Temp::new();
        let workspace = initialized_workspace(&committed);
        let url = "ssh://git@example.invalid/team/rehow.git";
        let plan = plan_git_remote_connect(&workspace, "origin", url).unwrap();
        set_git_config_fault(GitConfigFault::Quarantined);
        assert_eq!(
            apply_git_remote_connect(&workspace, &plan),
            Err(GitRemoteError::Unsupported)
        );
        assert!(!committed.0.join(".git/config.lock").exists());
        assert_eq!(config_backups(&committed).len(), 1);
        assert!(matches!(
            plan_git_remote_connect(&open_workspace(&committed), "origin", url),
            Err(GitRemoteError::DuplicateRemote)
        ));
    }

    #[test]
    fn concurrent_name_replacements_are_quarantined_never_deleted_or_rolled_back() {
        let before_exchange = Temp::new();
        let workspace = initialized_workspace(&before_exchange);
        let plan = plan_git_remote_connect(
            &workspace,
            "origin",
            "https://example.invalid/team/rehow.git",
        )
        .unwrap();
        set_git_config_race(GitConfigRace::ReplaceConfigBeforeExchange);
        assert_eq!(
            apply_git_remote_connect(&workspace, &plan),
            Err(GitRemoteError::PlanStale)
        );
        assert!(
            config_backups(&before_exchange)
                .iter()
                .any(|bytes| bytes == b"[external]\n\tvalue = before-exchange\n")
        );
        assert!(!before_exchange.0.join(".git/config.lock").exists());

        let before_quarantine = Temp::new();
        let workspace = initialized_workspace(&before_quarantine);
        let expected = fs::read(before_quarantine.0.join(".git/config")).unwrap();
        let plan = plan_git_remote_connect(
            &workspace,
            "origin",
            "https://example.invalid/team/rehow.git",
        )
        .unwrap();
        set_git_config_race(GitConfigRace::ReplaceLockBeforeQuarantine);
        assert_eq!(
            apply_git_remote_connect(&workspace, &plan),
            Err(GitRemoteError::PlanStale)
        );
        assert_eq!(
            fs::read(before_quarantine.0.join(".git/external-saved-lock")).unwrap(),
            expected
        );
        assert!(
            config_backups(&before_quarantine)
                .iter()
                .any(|bytes| bytes == b"[external]\n\tvalue = before-quarantine\n")
        );
        assert!(!before_quarantine.0.join(".git/config.lock").exists());
    }

    #[test]
    fn remote_connect_refuses_malformed_and_unsafe_config_entries() {
        let malformed = Temp::new();
        let workspace = initialized_workspace(&malformed);
        fs::write(malformed.0.join(".git/config"), b"[broken\n").unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::MalformedConfig)
        ));

        let linked = Temp::new();
        let workspace = initialized_workspace(&linked);
        fs::hard_link(
            linked.0.join(".git/config"),
            linked.0.join("config-second-link"),
        )
        .unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::Unsafe)
        ));

        let symlinked_lock = Temp::new();
        let workspace = initialized_workspace(&symlinked_lock);
        symlink(
            symlinked_lock.0.join("rehow.yaml"),
            symlinked_lock.0.join(".git/config.lock"),
        )
        .unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::Unsafe)
        ));

        let empty_remote = Temp::new();
        let workspace = initialized_workspace(&empty_remote);
        let config_path = empty_remote.0.join(".git/config");
        let mut config = fs::read(&config_path).unwrap();
        config.extend_from_slice(b"[REMOTE \"origin\"]\n# deliberately empty\n");
        fs::write(&config_path, config).unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::DuplicateRemote)
        ));

        let unsafe_mode = Temp::new();
        let workspace = initialized_workspace(&unsafe_mode);
        fs::set_permissions(
            unsafe_mode.0.join(".git/config"),
            fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::Unsafe)
        ));

        let unsafe_directory = Temp::new();
        let workspace = initialized_workspace(&unsafe_directory);
        fs::set_permissions(
            unsafe_directory.0.join(".git"),
            fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::Unsafe)
        ));

        let oversized = Temp::new();
        let workspace = initialized_workspace(&oversized);
        fs::write(
            oversized.0.join(".git/config"),
            vec![b'#'; MAX_GIT_CONFIG_BYTES + 1],
        )
        .unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::TooLarge)
        ));

        let exact_limit = Temp::new();
        let workspace = initialized_workspace(&exact_limit);
        let mut exact_config = Vec::with_capacity(MAX_GIT_CONFIG_BYTES);
        exact_config.push(b'#');
        exact_config.resize(MAX_GIT_CONFIG_BYTES - 1, b'a');
        exact_config.push(b'\n');
        fs::write(exact_limit.0.join(".git/config"), &exact_config).unwrap();
        assert_eq!(
            workspace
                .git_repository_capability()
                .unwrap()
                .read_config_files()
                .unwrap()
                .config
                .len(),
            MAX_GIT_CONFIG_BYTES
        );
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::TooLarge)
        ));

        let oversized_lock = Temp::new();
        let workspace = initialized_workspace(&oversized_lock);
        fs::write(
            oversized_lock.0.join(".git/config.lock"),
            vec![b'x'; MAX_GIT_CONFIG_BYTES + 1],
        )
        .unwrap();
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::TooLarge)
        ));

        let special_lock = Temp::new();
        let workspace = initialized_workspace(&special_lock);
        let fifo =
            std::ffi::CString::new(special_lock.0.join(".git/config.lock").to_str().unwrap())
                .unwrap();
        // SAFETY: path is a valid NUL-terminated temporary path.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(matches!(
            plan_git_remote_connect(
                &workspace,
                "origin",
                "https://example.invalid/team/rehow.git"
            ),
            Err(GitRemoteError::Unsafe)
        ));
    }

    #[test]
    fn reads_bounded_status_without_running_repository_programs() {
        let temp = Temp::new();
        Command::new("/usr/bin/git")
            .args(["init", "-q"])
            .current_dir(&temp.0)
            .status()
            .unwrap();
        for (key, value) in [
            ("user.name", "Rehow Test"),
            ("user.email", "rehow@example.invalid"),
        ] {
            Command::new("/usr/bin/git")
                .args(["config", key, value])
                .current_dir(&temp.0)
                .status()
                .unwrap();
        }
        fs::write(temp.0.join("tracked"), "before\n").unwrap();
        fs::write(temp.0.join(".gitattributes"), "tracked filter=attack\n").unwrap();
        Command::new("/usr/bin/git")
            .args(["add", "tracked", ".gitattributes"])
            .current_dir(&temp.0)
            .status()
            .unwrap();
        Command::new("/usr/bin/git")
            .args(["commit", "-qm", "fixture"])
            .current_dir(&temp.0)
            .status()
            .unwrap();
        let sentinel = temp.0.join("executed");
        let malicious = temp.0.join("malicious");
        fs::write(
            &malicious,
            format!("#!/bin/sh\ntouch {}\n", sentinel.display()),
        )
        .unwrap();
        let mut mode = fs::metadata(&malicious).unwrap().permissions();
        mode.set_mode(0o700);
        fs::set_permissions(&malicious, mode).unwrap();
        for (key, value) in [
            ("core.fsmonitor", malicious.to_str().unwrap()),
            ("core.hooksPath", malicious.to_str().unwrap()),
            ("filter.attack.process", malicious.to_str().unwrap()),
        ] {
            Command::new("/usr/bin/git")
                .args(["config", key, value])
                .current_dir(&temp.0)
                .status()
                .unwrap();
        }
        let included = temp.0.join("included-config");
        fs::write(
            &included,
            format!("[filter \"attack\"]\nclean = {}\n", malicious.display()),
        )
        .unwrap();
        Command::new("/usr/bin/git")
            .args(["config", "include.path", included.to_str().unwrap()])
            .current_dir(&temp.0)
            .status()
            .unwrap();
        Command::new("/usr/bin/git")
            .args([
                "remote",
                "add",
                "origin",
                "https://secret@example.invalid/repo?token=secret",
            ])
            .current_dir(&temp.0)
            .status()
            .unwrap();
        for arguments in [
            ["update-ref", "refs/remotes/origin/master", "HEAD"],
            ["config", "branch.master.remote", "origin"],
            ["config", "branch.master.merge", "refs/heads/master"],
        ] {
            Command::new("/usr/bin/git")
                .args(arguments)
                .current_dir(&temp.0)
                .status()
                .unwrap();
        }
        fs::write(temp.0.join("tracked"), "after\n").unwrap();
        let workspace = workspace(&temp);
        let status = read_git_status(&workspace).unwrap();
        let GitRepositoryStatus::Ready(repository) = status.repository() else {
            panic!()
        };
        assert!(status.available());
        assert!(status.version().is_some());
        assert_eq!(repository.branch(), Some("master"));
        assert_eq!(repository.upstream(), Some("origin/master"));
        assert!(repository.divergence_known());
        assert_eq!((repository.ahead(), repository.behind()), (0, 0));
        assert!(repository.changed() >= 1);
        assert!(repository.untracked() >= 2);
        assert_eq!(repository.remotes(), &["origin"]);
        assert!(!sentinel.exists());
    }

    #[test]
    fn git_runner_rechecks_output_limit_after_fast_exit() {
        let mut command = Command::new("/usr/bin/head");
        command
            .args(["-c", "8192", "/dev/zero"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        assert_eq!(
            run_bounded_git_command(command, Duration::from_secs(2), 1024)
                .err()
                .unwrap(),
            GitReadError::OutputLimit
        );
    }

    #[test]
    fn bounded_runner_kills_exited_child_descendants_holding_pipes() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "/usr/bin/sleep 30 & exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: this test command has no other threads between fork and exec;
        // the child only creates its own process group.
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            });
        }

        let started = Instant::now();
        let output = run_bounded_git_command(command, Duration::from_secs(2), 1024).unwrap();

        assert!(output.stdout.is_empty());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn rejects_gitfiles_and_external_object_alternates() {
        let temp = Temp::new();
        Command::new("/usr/bin/git")
            .args(["init", "-q"])
            .current_dir(&temp.0)
            .status()
            .unwrap();
        fs::create_dir_all(temp.0.join(".git/objects/info")).unwrap();
        fs::write(temp.0.join(".git/objects/info/alternates"), "/tmp\n").unwrap();
        let workspace = workspace(&temp);
        assert!(matches!(
            read_git_status(&workspace).unwrap().repository(),
            GitRepositoryStatus::Unsafe
        ));
    }

    #[test]
    fn post_check_alternates_are_hidden_from_the_sandbox() {
        let temp = Temp::new();
        Command::new("/usr/bin/git")
            .args(["init", "-q"])
            .current_dir(&temp.0)
            .status()
            .unwrap();
        let workspace = workspace(&temp);
        let alternates = temp.0.join(".git/objects/info/alternates");
        POST_CHECK_ALTERNATES.with_borrow_mut(|path| *path = Some(alternates.clone()));
        assert!(matches!(
            read_git_status(&workspace).unwrap().repository(),
            GitRepositoryStatus::Ready(_)
        ));
        assert!(alternates.exists());
    }

    #[test]
    fn porcelain_paths_remain_opaque_non_utf8_bytes() {
        let mut status = b"# branch.head main\0# branch.oid 0123456789abcdef\0".to_vec();
        status.extend_from_slice(b"1 .M N... 100644 100644 100644 a b non-utf8-\xff\0");
        status.extend_from_slice(b"? untracked-\xfe\0");
        status.extend_from_slice(
            b"2 R. N... 100644 100644 100644 a b R100 renamed-\xfd\0original-\xfc\0",
        );
        let (parsed, _) = parse_status(&status, b"").unwrap();
        assert_eq!(parsed.changed(), 2);
        assert_eq!(parsed.untracked(), 1);
        assert_eq!(parsed.branch(), Some("main"));
    }

    #[test]
    fn initializes_a_private_main_repository_without_running_git() {
        let temp = Temp::new();
        let workspace = workspace(&temp);
        let plan = plan_git_initialize(&workspace).unwrap();
        assert_eq!(plan.initial_branch(), "main");
        apply_git_initialize(&workspace, &plan).unwrap();

        assert_eq!(
            fs::read(temp.0.join(".git/HEAD")).unwrap(),
            b"ref: refs/heads/main\n"
        );
        assert_eq!(
            fs::metadata(temp.0.join(".git"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(temp.0.join(".git/config"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(temp.0.join(".git/rehow-init").exists());
        let status = read_git_status(&workspace).unwrap();
        let GitRepositoryStatus::Ready(repository) = status.repository() else {
            panic!()
        };
        assert_eq!(repository.branch(), Some("main"));
        assert!(matches!(
            apply_git_initialize(&workspace, &plan),
            Err(GitInitError::PlanStale)
        ));
    }

    #[test]
    fn rejects_stale_plans_and_unsafe_git_residue() {
        let replaced_session = Temp::new();
        let first_open = workspace(&replaced_session);
        let first_plan = plan_git_initialize(&first_open).unwrap();
        let second_open = open_workspace(&replaced_session);
        assert!(matches!(
            apply_git_initialize(&second_open, &first_plan),
            Err(GitInitError::PlanStale)
        ));
        assert!(!replaced_session.0.join(".git").exists());

        let stale = Temp::new();
        let stale_workspace = workspace(&stale);
        let stale_plan = plan_git_initialize(&stale_workspace).unwrap();
        fs::write(stale.0.join("rehow.yaml"), "changed\n").unwrap();
        assert!(matches!(
            apply_git_initialize(&stale_workspace, &stale_plan),
            Err(GitInitError::PlanStale)
        ));
        assert!(!stale.0.join(".git").exists());

        let residue = Temp::new();
        fs::create_dir(residue.0.join(".git")).unwrap();
        fs::set_permissions(residue.0.join(".git"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(residue.0.join(".git/unknown"), "do not replace").unwrap();
        let residue_workspace = workspace(&residue);
        assert!(matches!(
            plan_git_initialize(&residue_workspace),
            Err(GitInitError::Unsafe)
        ));
        assert_eq!(
            fs::read_to_string(residue.0.join(".git/unknown")).unwrap(),
            "do not replace"
        );

        let linked = Temp::new();
        let outside = linked.0.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, linked.0.join(".git")).unwrap();
        let linked_workspace = workspace(&linked);
        assert!(matches!(
            plan_git_initialize(&linked_workspace),
            Err(GitInitError::Unsafe)
        ));

        let parent = Temp::new();
        fs::create_dir(parent.0.join(".git")).unwrap();
        let nested = Temp(parent.0.join("nested"));
        fs::create_dir(&nested.0).unwrap();
        let nested_workspace = workspace(&nested);
        assert!(matches!(
            plan_git_initialize(&nested_workspace),
            Err(GitInitError::NestedRepository)
        ));
        assert!(!nested.0.join(".git").exists());
    }

    #[test]
    fn resumes_each_canonical_interruption_without_overwriting_residue() {
        let unpublished = Temp::new();
        let opened = workspace(&unpublished);
        let plan = plan_git_initialize(&opened).unwrap();
        set_git_init_fault(GitInitFault::FileBeforePublication);
        assert!(matches!(
            apply_git_initialize(&opened, &plan),
            Err(GitInitError::Unsupported)
        ));
        assert_eq!(fs::read_dir(unpublished.0.join(".git")).unwrap().count(), 0);
        let resumed = plan_git_initialize(&opened).unwrap();
        apply_git_initialize(&opened, &resumed).unwrap();

        for fault in [GitInitFault::MarkerCreated, GitInitFault::DirectoriesSynced] {
            let temp = Temp::new();
            let opened = workspace(&temp);
            let plan = plan_git_initialize(&opened).unwrap();
            set_git_init_fault(fault);
            assert!(matches!(
                apply_git_initialize(&opened, &plan),
                Err(GitInitError::Unsupported)
            ));
            assert!(temp.0.join(".git/rehow-init").exists());

            let reopened = open_workspace(&temp);
            let resumed = plan_git_initialize(&reopened).unwrap();
            apply_git_initialize(&reopened, &resumed).unwrap();
            assert!(temp.0.join(".git/rehow-init").exists());
            assert!(matches!(
                read_git_status(&reopened).unwrap().repository(),
                GitRepositoryStatus::Ready(_)
            ));
        }

        let temp = Temp::new();
        let opened = workspace(&temp);
        let plan = plan_git_initialize(&opened).unwrap();
        set_git_init_fault(GitInitFault::FilesSynced);
        assert!(matches!(
            apply_git_initialize(&opened, &plan),
            Err(GitInitError::Unsupported)
        ));
        let reopened = open_workspace(&temp);
        assert!(matches!(
            read_git_status(&reopened).unwrap().repository(),
            GitRepositoryStatus::Ready(_)
        ));
        assert!(matches!(
            plan_git_initialize(&reopened),
            Err(GitInitError::AlreadyRepository)
        ));
    }
}

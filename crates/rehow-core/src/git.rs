//! Confined Git discovery/status and separately gated repository operations.

use crate::{
    protocol::{PlanToken, Revision},
    storage::{
        GitConfigStorageError, GitInitializeStorageError, GitRepositoryCapabilityError,
        WorkspaceView,
    },
};
use std::{
    fmt, fs,
    io::{Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, PermissionsExt},
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
const MAX_REMOTES: usize = 32;
const MAX_REMOTE_NAME_BYTES: usize = 64;
const MAX_REMOTE_URL_BYTES: usize = 2048;
const MAX_GIT_CONFIG_BYTES: usize = 1024 * 1024;

#[cfg(test)]
thread_local! {
    static POST_CHECK_ALTERNATES: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
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
    let mut child = command.spawn().map_err(|_| GitReadError::Spawn)?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take().ok_or(GitReadError::Spawn)?;
    let stderr = child.stderr.take().ok_or(GitReadError::Spawn)?;
    let stdout_reader = read_capped(stdout, Arc::clone(&exceeded));
    let stderr_reader = read_capped(stderr, Arc::clone(&exceeded));
    let started = Instant::now();
    let status = loop {
        if exceeded.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(GitReadError::OutputLimit);
        }
        if started.elapsed() >= PROCESS_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break Err(GitReadError::Timeout);
        }
        match child.try_wait().map_err(|_| GitReadError::Spawn)? {
            Some(status) => break Ok(status),
            None => thread::sleep(Duration::from_millis(2)),
        }
    };
    let stdout = stdout_reader.join().map_err(|_| GitReadError::Spawn)?;
    let _ = stderr_reader.join().map_err(|_| GitReadError::Spawn)?;
    let status = status?;
    if !status.success() {
        return if status.code().is_some() {
            Err(GitReadError::ExitStatus)
        } else {
            Err(GitReadError::Signal)
        };
    }
    Ok(ProcessOutput { stdout })
}

fn read_capped<R: Read + Send + 'static>(
    mut reader: R,
    exceeded: Arc<AtomicBool>,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut retained = Vec::new();
        let mut buffer = [0_u8; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            if retained.len().saturating_add(count) > OUTPUT_LIMIT {
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
        GitConfigFault, GitConfigRace, GitInitFault, WorkspaceInspection, inspect_workspace,
        set_git_config_fault, set_git_config_race, set_git_init_fault,
    };
    use std::{fs, os::unix::fs::symlink, process::Command};

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

    fn workspace(temp: &Temp) -> Box<WorkspaceView> {
        fs::create_dir(temp.0.join("categories")).unwrap();
        fs::write(temp.0.join("rehow.yaml"), "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Git\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n").unwrap();
        open_workspace(temp)
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

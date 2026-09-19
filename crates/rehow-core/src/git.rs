//! Confined Git discovery/status and separately gated repository operations.

use crate::storage::recovery::{
    GitCommitStaging, GitCommitStagingError, create_git_commit_staging,
};
use crate::{
    protocol::{OperationId, PlanToken, Revision, Timestamp, current_timestamp, new_operation_id},
    storage::{
        ATTACHMENT_BYTES_LIMIT, ATTACHMENT_COUNT_LIMIT, CATEGORY_LIMIT, CloneDestinationSnapshot,
        EmptyWorkspaceTarget, GUIDE_COUNT_LIMIT, GitCloneMaterializationSource,
        GitCloneMaterializationStorageError, GitCloneStaging, GitCloneStagingError,
        GitCommitRepositorySnapshot, GitCommitSnapshotError, GitConfigStorageError,
        GitInitializeStorageError, GitMaterializationPathKind, GitRepositoryCapabilityError,
        MANAGED_BYTES_LIMIT, WorkspaceInspection, WorkspaceView, classify_git_materialization_path,
        create_git_clone_staging, execute_git_clone_materialization, finalize_git_clone_repository,
        inspect_workspace, open_initialized_workspace, seal_git_clone_repository,
    },
};
use std::{
    fmt,
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::{MetadataExt, PermissionsExt},
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
const CLONE_LOCAL_READ_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const CLONE_DISCOVERY_OUTPUT_LIMIT: usize = 64 * 1024;
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
const MAX_COMMIT_STAGING_BYTES: u64 = 3 * 1024 * 1024 * 1024;
const COMMIT_PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const COMMIT_PLAN_TIMEOUT: Duration = Duration::from_secs(2 * 60);
#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
const MAX_CLONE_TREE_LISTING_BYTES: usize = 32 * 1024 * 1024;
#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
const MAX_CLONE_TREE_ENTRIES: usize =
    1 + CATEGORY_LIMIT + GUIDE_COUNT_LIMIT + ATTACHMENT_COUNT_LIMIT + 1;

#[cfg(test)]
thread_local! {
    static POST_CHECK_ALTERNATES: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    static CLONE_PRESEAL_PAUSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
    Io,
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitCommitManagedKind {
    Manifest,
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
) -> Result<GitRemoteHead, GitCloneDiscoveryError> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(GitCloneDiscoveryError::Cancelled);
    }
    if plan.transport != GitRemoteTransport::Https {
        return Err(GitCloneDiscoveryError::UnsupportedTransport);
    }
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCloneDiscoveryError::GitUnavailable)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCloneDiscoveryError::SandboxUnavailable)?;
    let command = clone_discovery_command(&executable, &sandbox, &plan.remote_url);
    let output = run_clone_command(
        command,
        cancelled,
        CLONE_DISCOVERY_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
    )?;
    parse_git_remote_head(&output.stdout)
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
pub(crate) fn fetch_git_clone_remote(
    plan: &GitClonePlan,
    remote_head: &GitRemoteHead,
    staging: &GitCloneStaging,
    cancelled: &AtomicBool,
) -> Result<(), GitCloneDiscoveryError> {
    if plan.transport != GitRemoteTransport::Https {
        return Err(GitCloneDiscoveryError::UnsupportedTransport);
    }
    let executable =
        trusted_executable("/usr/bin/git").ok_or(GitCloneDiscoveryError::GitUnavailable)?;
    let sandbox =
        trusted_executable("/usr/bin/bwrap").ok_or(GitCloneDiscoveryError::SandboxUnavailable)?;
    let command = clone_fetch_command(
        &executable,
        &sandbox,
        staging.git_path(),
        staging.git_descriptor(),
        &plan.remote_url,
        &remote_head.object_id,
    );
    let output = run_clone_command(
        command,
        cancelled,
        CLONE_FETCH_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
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

/// Runs the complete pre-materialization clone pipeline. The destination is
/// revalidated before and after the operation and is never bound into a child
/// process or written by this function.
pub fn apply_git_clone_validation(
    destination: &EmptyWorkspaceTarget,
    plan: GitClonePlan,
    state_home: &Path,
    cancelled: &AtomicBool,
) -> Result<GitCloneValidation, GitCloneApplyError> {
    if plan.transport != GitRemoteTransport::Https {
        return Err(GitCloneApplyError::UnsupportedTransport);
    }
    revalidate_git_clone_plan(destination, &plan).map_err(map_clone_apply_destination)?;
    let result = (|| {
        let remote_head =
            discover_git_clone_remote_head(&plan, cancelled).map_err(map_clone_apply_discovery)?;
        let staging = prepare_git_clone_staging(destination, &plan, &remote_head, state_home)
            .map_err(map_clone_apply_staging)?;
        fetch_git_clone_remote(&plan, &remote_head, &staging, cancelled)
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
        GitCloneDiscoveryError::RemoteUnavailable => GitCloneApplyError::RemoteUnavailable,
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
    use sha1::Digest as _;

    let header = format!("blob {}\0", bytes.len());
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
        | GitCloneDiscoveryError::InvalidAdvertisement
        | GitCloneDiscoveryError::UnsupportedTransport => GitCloneInspectError::InvalidRepository,
        GitCloneDiscoveryError::Io => GitCloneInspectError::Io,
    }
}

fn run_clone_command(
    mut command: Command,
    cancelled: &AtomicBool,
    timeout: Duration,
    output_limit: usize,
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
    let _ = stderr_reader
        .join()
        .map_err(|_| GitCloneDiscoveryError::Io)?;
    let status = status?;
    if exceeded.load(Ordering::Relaxed) {
        return Err(GitCloneDiscoveryError::OutputLimit);
    }
    if !status.success() {
        return Err(GitCloneDiscoveryError::RemoteUnavailable);
    }
    Ok(ProcessOutput { stdout })
}

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
fn clone_discovery_command(executable: &Path, sandbox: &Path, remote_url: &str) -> Command {
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
            "--",
        ])
        .arg(executable)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "credential.helper=",
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
            "protocol.https.allow=always",
            "-c",
            "protocol.ssh.allow=never",
            "ls-remote",
            "--symref",
            "--exit-code",
        ])
        .arg(remote_url)
        .arg("HEAD")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    command
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
    let refspec = format!("{object_id}:refs/rehow/clone/head");
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
            "protocol.https.allow=always",
            "-c",
            "protocol.ssh.allow=never",
            "fetch",
            "--quiet",
            "--depth=1",
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
    configure_clone_child(
        &mut command,
        staging_git_descriptor,
        Some(CLONE_STAGING_FILE_LIMIT),
    );
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
    // SAFETY: this closure runs after fork and before exec. It only updates the
    // selected child descriptor flag and an inherited resource limit.
    unsafe {
        command.pre_exec(move || {
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

#[allow(dead_code, reason = "connected by the Phase 5B3B fetch apply gate")]
fn terminate_child(child: &mut std::process::Child) {
    if let Ok(process_group) = i32::try_from(child.id()) {
        // SAFETY: the child was placed in a fresh process group whose id is
        // the child's pid; a negative id targets only that group.
        unsafe {
            libc::kill(-process_group, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
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

#[derive(Debug)]
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
        (Some(_), Some(_)) => return Err(GitCommitPlanError::UnsafeRepository),
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
struct GitIndexSnapshot {
    version: u32,
    entries: Vec<GitIndexEntry>,
    has_cache_tree: bool,
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
    })
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
    let status = loop {
        if exceeded.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(GitReadError::OutputLimit);
        }
        if started.elapsed() >= timeout {
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
    if let Some(writer) = input_writer {
        let _ = writer.join().map_err(|_| GitReadError::Spawn)?;
    }
    let status = status?;
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
    let staging = create_git_commit_staging(
        state_home,
        &stage_name,
        provenance.as_bytes(),
        config,
        preflight
            .repository
            .index
            .as_ref()
            .map(|index| index.bytes.as_slice()),
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

fn run_git_commit_staging_write_tree(
    executable: &Path,
    sandbox: &Path,
    repository: &crate::storage::GitRepositoryCapability,
    staging: &GitCommitStaging,
    object_format: GitObjectFormat,
) -> Result<String, GitCommitPlanError> {
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
            "write-tree",
        ])
        .stdin(Stdio::null())
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
    let output = run_bounded_git_command(command, COMMIT_PROCESS_TIMEOUT, 128)
        .map_err(map_git_commit_read_error)?;
    parse_git_commit_object_id_line(&output.stdout, object_format)
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
    expected.push((parent, "commit", None));
    expected.push((root_tree, "tree", None));
    for entry in &tree.entries {
        let object_type = match entry.kind {
            GitCommitTreeEntryKind::Tree => "tree",
            GitCommitTreeEntryKind::Blob => "blob",
            GitCommitTreeEntryKind::Commit => "commit",
        };
        expected.push((entry.object_id.as_str(), object_type, entry.size));
    }
    let mut input = Vec::new();
    input
        .try_reserve_exact(expected.len().saturating_mul(65))
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    for (object_id, _, _) in &expected {
        input.extend_from_slice(object_id.as_bytes());
        input.push(b'\n');
    }
    let output = run_git_commit_read_with_input(
        executable,
        sandbox,
        capability,
        object_format,
        &[
            "cat-file",
            "--batch-check=%(objectname) %(objecttype) %(objectsize)",
        ],
        MAX_COMMIT_TREE_BYTES,
        Some(input),
    )
    .map_err(map_git_commit_read_error)?;
    if !output.stdout.ends_with(b"\n") {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    let mut lines = output.stdout.split(|byte| *byte == b'\n');
    for (expected_id, expected_type, expected_size) in expected {
        let line = lines.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let text = std::str::from_utf8(line).map_err(|_| GitCommitPlanError::UnsafeRepository)?;
        let mut fields = text.split_ascii_whitespace();
        let object_id = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let object_type = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        let object_size = fields.next().ok_or(GitCommitPlanError::UnsafeRepository)?;
        if fields.next().is_some()
            || object_id != expected_id
            || object_type != expected_type
            || parse_canonical_u64(object_size).is_none()
            || expected_size.is_some_and(|size| parse_canonical_u64(object_size) != Some(size))
        {
            return Err(GitCommitPlanError::UnsafeRepository);
        }
    }
    if lines.any(|line| !line.is_empty()) {
        return Err(GitCommitPlanError::UnsafeRepository);
    }
    Ok(())
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
    let expected_count = 1usize
        .checked_add(workspace.categories().len())
        .and_then(|count| count.checked_add(guide_count))
        .and_then(|count| count.checked_add(attachment_count))
        .ok_or(GitCommitPlanError::ResourceLimit)?;
    let mut expected = Vec::new();
    expected
        .try_reserve_exact(expected_count)
        .map_err(|_| GitCommitPlanError::ResourceLimit)?;
    expected.push((
        "rehow.yaml".to_owned(),
        GitCommitManagedKind::Manifest,
        GitCommitManagedExpectation::Document(workspace.manifest_revision()),
    ));
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
            > 1usize
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
        let (classified, limit) =
            classify_git_materialization_path(&path).ok_or(GitCommitPlanError::WorkspaceIssues)?;
        let expected_class = match kind {
            GitCommitManagedKind::Manifest => GitMaterializationPathKind::Manifest,
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
        GitCloneFinalizeFault, GitCloneScaffoldFault, GitConfigFault, GitConfigRace, GitInitFault,
        WorkspaceFault, WorkspaceInspection, inspect_workspace, set_git_clone_finalize_fault,
        set_git_clone_scaffold_fault, set_git_config_fault, set_git_config_race,
        set_git_init_fault, set_workspace_fault,
    };
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        process::Command,
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

    fn configure_fixture_identity(temp: &Temp) {
        fixture_git(temp, &["config", "--local", "user.name", "Rehow Test"]);
        fixture_git(
            temp,
            &["config", "--local", "user.email", "rehow@example.invalid"],
        );
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
    fn commit_branch_accepts_exactly_one_direct_representation() {
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
            (
                head.as_slice(),
                Some(loose.as_bytes()),
                Some(packed.as_bytes()),
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
        let workspace = open_workspace(&unborn);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
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

        let unborn = Temp::new();
        drop(initialized_workspace(&unborn));
        configure_fixture_identity(&unborn);
        let workspace = open_workspace(&unborn);
        let preflight = inspect_git_commit_repository(&workspace).unwrap();
        assert!(preflight.head_tree_object_id.is_none());
        let unborn_state = Temp::new();
        let staging = prepare_git_commit_staging(&preflight, &unborn_state.0).unwrap();
        assert!(!Path::new(staging.staging.git_path()).join("index").exists());
        assert!(staging.prior_managed.files.is_empty());
    }

    #[test]
    fn commit_private_write_tree_refuses_semantically_dirty_copied_index() {
        let temp = Temp::new();
        drop(initialized_workspace(&temp));
        configure_fixture_identity(&temp);
        fixture_git(&temp, &["add", "--", "rehow.yaml"]);
        fixture_git(&temp, &["commit", "-qm", "fixture"]);
        let workspace = open_workspace(&temp);
        let mut preflight = inspect_git_commit_repository(&workspace).unwrap();
        fs::write(temp.0.join("unrelated.txt"), b"staged only\n").unwrap();
        fixture_git(&temp, &["add", "--", "unrelated.txt"]);
        let dirty_index = fs::read(temp.0.join(".git/index")).unwrap();
        fixture_git(&temp, &["reset", "--hard", "-q", "HEAD"]);
        preflight.repository.index.as_mut().unwrap().bytes = dirty_index;
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
    }

    #[test]
    fn clone_discovery_refuses_ssh_and_precancel_before_spawning() {
        let temp = Temp::new();
        let target = empty_target(&temp);
        let https = plan_git_clone(&target, "https://example.invalid/team/rehow.git").unwrap();
        let cancelled = AtomicBool::new(true);
        assert_eq!(
            discover_git_clone_remote_head(&https, &cancelled),
            Err(GitCloneDiscoveryError::Cancelled)
        );
        let ssh = plan_git_clone(&target, "git@example.invalid:team/rehow.git").unwrap();
        assert_eq!(
            discover_git_clone_remote_head(&ssh, &AtomicBool::new(false)),
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
    #[ignore = "explicit public-network clone sandbox smoke test"]
    fn clone_public_https_fetch_stays_in_private_staging() {
        let destination = Temp::new();
        let state = Temp::new();
        let target = empty_target(&destination);
        let plan = plan_git_clone(&target, "https://github.com/octocat/Hello-World.git").unwrap();
        let cancelled = AtomicBool::new(false);
        let head = discover_git_clone_remote_head(&plan, &cancelled).unwrap();
        let staging = prepare_git_clone_staging(&target, &plan, &head, &state.0).unwrap();
        fetch_git_clone_remote(&plan, &head, &staging, &cancelled).unwrap();
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

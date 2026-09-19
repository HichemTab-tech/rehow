#![allow(
    dead_code,
    reason = "the pure Phase 1E-B1 journal boundary is connected to durable I/O in Phase 1E-B2"
)]

use crate::protocol::{GuideId, OperationId, Revision, Timestamp, WorkspaceId};
use std::{
    collections::HashSet,
    fmt,
    fs::{File, Metadata},
    os::unix::fs::MetadataExt,
    path::Path,
    str::FromStr,
};
use unicode_normalization::UnicodeNormalization;

mod store;
mod workspace;

#[cfg(test)]
pub(crate) use workspace::{WorkspaceFault, set_workspace_fault};

pub(crate) struct GitCloneStaging {
    inner: store::CloneStaging,
}

pub(crate) struct GitCommitStaging {
    inner: store::CommitStaging,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GitCommitStagingSnapshot {
    inner: store::CommitStagingSnapshot,
}

impl GitCommitStaging {
    pub(crate) fn stage_name(&self) -> &str {
        self.inner.stage_name()
    }

    pub(crate) fn stage_identity(&self) -> Result<(u64, u64), GitCommitStagingError> {
        self.inner
            .stage_identity()
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn git_path(&self) -> &str {
        self.inner.git_path()
    }

    pub(crate) fn git_descriptor(&self) -> i32 {
        self.inner.git_descriptor()
    }

    pub(crate) fn stage_path(&self) -> &str {
        self.inner.stage_path()
    }

    pub(crate) fn validation_path(&self) -> &str {
        self.inner.validation_path()
    }

    pub(crate) fn candidate_path(&self) -> &str {
        self.inner.candidate_path()
    }

    pub(crate) fn candidate_descriptor(&self) -> i32 {
        self.inner.candidate_descriptor()
    }

    pub(crate) fn write_validation_file(
        &self,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<(), GitCommitStagingError> {
        self.inner
            .write_validation_file(relative_path, bytes)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn finish_validation_tree(&self) -> Result<(), GitCommitStagingError> {
        self.inner
            .finish_validation_tree()
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn write_candidate_file(
        &self,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<(), GitCommitStagingError> {
        self.inner
            .write_candidate_file(relative_path, bytes)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn finish_candidate_tree(&self) -> Result<(), GitCommitStagingError> {
        self.inner
            .finish_candidate_tree()
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn verify_size_limit(&self, limit: u64) -> Result<(), GitCommitStagingError> {
        self.inner
            .verify_size_limit(limit)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn snapshot(
        &self,
        limit: u64,
    ) -> Result<GitCommitStagingSnapshot, GitCommitStagingError> {
        self.inner
            .snapshot(limit)
            .map(|inner| GitCommitStagingSnapshot { inner })
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn verify_snapshot(
        &self,
        expected: GitCommitStagingSnapshot,
        limit: u64,
    ) -> Result<bool, GitCommitStagingError> {
        self.inner
            .verify_snapshot(expected.inner, limit)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn inspect_loose_object_ids(
        &self,
        object_id_hex_len: usize,
        object_limit: usize,
        aggregate_limit: u64,
    ) -> Result<Vec<String>, GitCommitStagingError> {
        self.inner
            .inspect_loose_object_ids(object_id_hex_len, object_limit, aggregate_limit)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn inspect_pack_sources(
        &self,
        object_id_hex_len: usize,
        aggregate_limit: u64,
    ) -> Result<Vec<GitClonePackSource>, GitCommitStagingError> {
        self.inner
            .inspect_pack_sources(object_id_hex_len, aggregate_limit)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn open_pack_source(
        &self,
        source: &GitClonePackSource,
    ) -> Result<File, GitCommitStagingError> {
        self.inner
            .open_pack_source(source)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn inspect_index_source(
        &self,
        limit: u64,
    ) -> Result<GitClonePackSource, GitCommitStagingError> {
        self.inner
            .inspect_index_source(limit)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn open_index_source(
        &self,
        source: &GitClonePackSource,
        limit: u64,
    ) -> Result<File, GitCommitStagingError> {
        self.inner
            .open_index_source(source, limit)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn replace_index(&self, bytes: &[u8]) -> Result<(), GitCommitStagingError> {
        self.inner
            .replace_index(bytes)
            .map_err(map_commit_staging_error)
    }

    pub(crate) fn quarantine(self) -> Result<(), GitCommitStagingError> {
        self.inner.quarantine().map_err(map_commit_staging_error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitStagingError {
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

pub(crate) struct GitCommitRecoverySession {
    store: store::RecoveryStore,
    lock: store::WriterLock,
    workspace_id: WorkspaceId,
    root_hash: ContentHash,
    root_device: u64,
    root_inode: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitIntentError {
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitIntentStage {
    Prepared,
    IndexPublished,
    RefPublished,
    Complete,
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitIntentBranchRepresentation {
    Unborn,
    Loose,
    Packed,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct GitCommitIntent {
    pub(crate) generation: u64,
    pub(crate) operation_id: OperationId,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) root_hash: ContentHash,
    pub(crate) root_device: u64,
    pub(crate) root_inode: u64,
    pub(crate) git_device: u64,
    pub(crate) git_inode: u64,
    pub(crate) branch_relative: String,
    pub(crate) representation: GitCommitIntentBranchRepresentation,
    pub(crate) head_hash: ContentHash,
    pub(crate) config_hash: ContentHash,
    pub(crate) before_index_hash: Option<ContentHash>,
    pub(crate) before_index_mode: u32,
    pub(crate) candidate_index_hash: ContentHash,
    pub(crate) candidate_index_mode: u32,
    pub(crate) packed_refs_hash: Option<ContentHash>,
    pub(crate) before_ref_hash: Option<ContentHash>,
    pub(crate) before_ref_mode: u32,
    pub(crate) candidate_ref_hash: ContentHash,
    pub(crate) candidate_ref_mode: u32,
    pub(crate) plan_hash: ContentHash,
    pub(crate) stage: GitCommitIntentStage,
    pub(crate) staging_name: String,
    pub(crate) staging_device: u64,
    pub(crate) staging_inode: u64,
}

impl fmt::Debug for GitCommitIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitIntent")
            .field("generation", &self.generation)
            .field("operation_id", &self.operation_id)
            .field("workspace_id", &self.workspace_id)
            .field("branch_relative", &self.branch_relative)
            .field("representation", &self.representation)
            .field("stage", &self.stage)
            .field("content", &"<redacted>")
            .finish()
    }
}

impl GitCloneStaging {
    pub(crate) fn git_path(&self) -> &str {
        self.inner.git_path()
    }

    pub(crate) fn git_descriptor(&self) -> i32 {
        self.inner.git_descriptor()
    }

    pub(crate) fn stage_path(&self) -> &str {
        self.inner.stage_path()
    }

    pub(crate) fn validation_path(&self) -> &str {
        self.inner.validation_path()
    }

    pub(crate) fn write_validation_file(
        &self,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<(), GitCloneStagingError> {
        self.inner
            .write_validation_file(relative_path, bytes)
            .map_err(map_clone_staging_error)
    }

    pub(crate) fn finish_validation_tree(&self) -> Result<(), GitCloneStagingError> {
        self.inner
            .finish_validation_tree()
            .map_err(map_clone_staging_error)
    }

    pub(crate) fn read_validation_file(
        &self,
        relative_path: &str,
        expected_size: u64,
    ) -> Result<Vec<u8>, GitCloneStagingError> {
        self.inner
            .read_validation_file(relative_path, expected_size)
            .map_err(map_clone_staging_error)
    }

    pub(crate) fn verify_git_size_limit(&self, limit: u64) -> Result<(), GitCloneStagingError> {
        self.inner
            .verify_git_size_limit(limit)
            .map_err(map_clone_staging_error)
    }

    pub(crate) fn inspect_pack_sources(
        &self,
        object_id_hex_len: usize,
        aggregate_limit: u64,
    ) -> Result<Vec<GitClonePackSource>, GitCloneStagingError> {
        self.inner
            .inspect_pack_sources(object_id_hex_len, aggregate_limit)
            .map_err(map_clone_staging_error)
    }

    pub(crate) fn open_pack_source(
        &self,
        source: &GitClonePackSource,
    ) -> Result<File, GitCloneStagingError> {
        self.inner
            .open_pack_source(source)
            .map_err(map_clone_staging_error)
    }

    pub(crate) fn inspect_index_source(
        &self,
        staging_name: &str,
    ) -> Result<GitClonePackSource, GitCloneStagingError> {
        self.inner
            .inspect_index_source(staging_name)
            .map_err(map_clone_staging_error)
    }

    pub(crate) fn open_index_source(
        &self,
        staging_name: &str,
        source: &GitClonePackSource,
    ) -> Result<File, GitCloneStagingError> {
        self.inner
            .open_index_source(staging_name, source)
            .map_err(map_clone_staging_error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCloneStagingError {
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

pub(crate) struct GitCloneMaterializationSource {
    pub(crate) relative_path: String,
    pub(crate) size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitClonePackSource {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCloneMaterializationStorageError {
    SourceChanged,
    Busy,
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GitCloneMaterializationStorageOutcome;

pub(crate) fn create_git_clone_staging(
    state_home: &Path,
    stage_name: &str,
    provenance: &[u8],
    git_config: &[u8],
) -> Result<GitCloneStaging, GitCloneStagingError> {
    store::CloneStaging::create(state_home, stage_name, provenance, git_config)
        .map(|inner| GitCloneStaging { inner })
        .map_err(map_clone_staging_error)
}

pub(crate) fn create_git_commit_staging(
    state_home: &Path,
    stage_name: &str,
    provenance: &[u8],
    git_config: &[u8],
    index: Option<&[u8]>,
) -> Result<GitCommitStaging, GitCommitStagingError> {
    store::CommitStaging::create(state_home, stage_name, provenance, git_config, index)
        .map(|inner| GitCommitStaging { inner })
        .map_err(map_commit_staging_error)
}

pub(super) fn with_git_commit_writer_lock<T, E, F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    action: F,
) -> Result<Result<T, E>, GitCommitStagingError>
where
    F: FnOnce() -> Result<T, E>,
{
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_commit_staging_error)?;
    let _lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_commit_staging_error)?;
    Ok(action())
}

pub(super) fn open_git_commit_recovery_session(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
) -> Result<GitCommitRecoverySession, GitCommitIntentError> {
    let (root_device, root_inode) = root.identity().map_err(map_git_commit_root_error)?;
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_git_commit_intent_store)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_git_commit_intent_store)?;
    Ok(GitCommitRecoverySession {
        store,
        lock,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
    })
}

impl GitCommitRecoverySession {
    pub(crate) fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    pub(crate) fn root_hash(&self) -> ContentHash {
        self.root_hash
    }

    pub(crate) fn root_identity(&self) -> (u64, u64) {
        (self.root_device, self.root_inode)
    }

    pub(crate) fn store_bytes(&self, bytes: &[u8]) -> Result<ContentHash, GitCommitIntentError> {
        self.store
            .store_object(&self.lock, bytes)
            .map_err(map_git_commit_intent_store)
    }

    pub(crate) fn read_bytes(&self, hash: &ContentHash) -> Result<Vec<u8>, GitCommitIntentError> {
        self.store
            .read_object(&self.lock, hash)
            .map_err(map_git_commit_intent_store)
    }

    pub(crate) fn install_intent(
        &self,
        intent: &GitCommitIntent,
    ) -> Result<(), GitCommitIntentError> {
        self.store
            .install_git_commit_intent(&self.lock, intent)
            .map_err(map_git_commit_intent_store)
    }

    pub(crate) fn read_intent(
        &self,
        operation_id: OperationId,
    ) -> Result<GitCommitIntent, GitCommitIntentError> {
        self.store
            .read_git_commit_intent(&self.lock, operation_id)
            .map_err(map_git_commit_intent_store)
    }

    pub(crate) fn list_intents(&self) -> Result<Vec<GitCommitIntent>, GitCommitIntentError> {
        self.store
            .list_git_commit_intents(&self.lock)
            .map_err(map_git_commit_intent_store)
    }

    pub(crate) fn quarantine_staging(
        &self,
        stage_name: &str,
        stage_identity: (u64, u64),
    ) -> Result<(), GitCommitIntentError> {
        self.store
            .quarantine_git_commit_staging(&self.lock, stage_name, stage_identity)
            .map_err(map_git_commit_intent_store)
    }
}

fn map_git_commit_root_error(error: super::linux::MutationIoError) -> GitCommitIntentError {
    match error {
        super::linux::MutationIoError::Conflict => GitCommitIntentError::Conflict,
        super::linux::MutationIoError::Unsafe => GitCommitIntentError::Unsafe,
        super::linux::MutationIoError::Unsupported => GitCommitIntentError::Unsupported,
        super::linux::MutationIoError::TooLarge => GitCommitIntentError::TooLarge,
        super::linux::MutationIoError::Io => GitCommitIntentError::Io,
    }
}

fn map_git_commit_intent_store(error: store::StoreError) -> GitCommitIntentError {
    match error {
        store::StoreError::Conflict | store::StoreError::Busy => GitCommitIntentError::Conflict,
        store::StoreError::Unsafe | store::StoreError::Corrupt => GitCommitIntentError::Unsafe,
        store::StoreError::Unsupported => GitCommitIntentError::Unsupported,
        store::StoreError::TooLarge => GitCommitIntentError::TooLarge,
        store::StoreError::Io => GitCommitIntentError::Io,
        #[cfg(test)]
        store::StoreError::Injected => GitCommitIntentError::Io,
    }
}

fn map_commit_staging_error(error: store::StoreError) -> GitCommitStagingError {
    match error {
        store::StoreError::Conflict | store::StoreError::Busy => GitCommitStagingError::Conflict,
        store::StoreError::Unsafe | store::StoreError::Corrupt => GitCommitStagingError::Unsafe,
        store::StoreError::Unsupported => GitCommitStagingError::Unsupported,
        store::StoreError::TooLarge => GitCommitStagingError::TooLarge,
        store::StoreError::Io => GitCommitStagingError::Io,
        #[cfg(test)]
        store::StoreError::Injected => GitCommitStagingError::Io,
    }
}

fn map_clone_staging_error(error: store::StoreError) -> GitCloneStagingError {
    match error {
        store::StoreError::Conflict | store::StoreError::Busy => GitCloneStagingError::Conflict,
        store::StoreError::Unsafe | store::StoreError::Corrupt => GitCloneStagingError::Unsafe,
        store::StoreError::Unsupported => GitCloneStagingError::Unsupported,
        store::StoreError::TooLarge => GitCloneStagingError::TooLarge,
        store::StoreError::Io => GitCloneStagingError::Io,
        #[cfg(test)]
        store::StoreError::Injected => GitCloneStagingError::Io,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_git_clone_materialization<F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    operation_id: OperationId,
    created_at: Timestamp,
    plan_hash: [u8; 32],
    marker: &[u8],
    sources: &[GitCloneMaterializationSource],
    mut read_file: F,
) -> Result<GitCloneMaterializationStorageOutcome, GitCloneMaterializationStorageError>
where
    F: FnMut(usize, &str, u64) -> Result<Vec<u8>, GitCloneMaterializationStorageError>,
{
    if sources.is_empty() || sources.len() > MAX_TARGETS || marker.len() > 16 * 1024 {
        return Err(GitCloneMaterializationStorageError::TooLarge);
    }
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_clone_materialization)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_clone_materialization)?;

    let persisted = match store.read_journal(&lock, operation_id) {
        Ok(journal) => Some(journal),
        Err(read_error) => match store.journal_missing(&lock, operation_id) {
            Ok(true) => None,
            Ok(false) => return Err(map_store_clone_materialization(read_error)),
            Err(error) => return Err(map_store_clone_materialization(error)),
        },
    };

    let mut targets = Vec::new();
    targets
        .try_reserve_exact(sources.len())
        .map_err(|_| GitCloneMaterializationStorageError::TooLarge)?;
    let mut categories = HashSet::new();
    let mut attachments = HashSet::new();
    let mut previous: Option<&str> = None;
    for (index, source) in sources.iter().enumerate() {
        if !valid_active_path(&source.relative_path)
            || previous.is_some_and(|path| path >= source.relative_path.as_str())
        {
            return Err(GitCloneMaterializationStorageError::Unsafe);
        }
        previous = Some(&source.relative_path);
        if let Some(remainder) = source.relative_path.strip_prefix("categories/")
            && let Some((directory, _)) = remainder.split_once('/')
        {
            categories
                .try_reserve(1)
                .map_err(|_| GitCloneMaterializationStorageError::TooLarge)?;
            categories.insert(directory.to_owned());
        }
        if let Some(remainder) = source.relative_path.strip_prefix("attachments/")
            && let Some((guide_id, _)) = remainder.split_once('/')
        {
            attachments
                .try_reserve(1)
                .map_err(|_| GitCloneMaterializationStorageError::TooLarge)?;
            attachments.insert(guide_id.to_owned());
        }
        let bytes = read_file(index, &source.relative_path, source.size)?;
        if bytes.len() as u64 != source.size {
            return Err(GitCloneMaterializationStorageError::SourceChanged);
        }
        let object_hash = store
            .store_object(&lock, &bytes)
            .map_err(map_store_clone_materialization)?;
        targets.push(Target {
            relative_path: source.relative_path.clone(),
            expected: ExpectedState::Absent,
            intended: IntendedState::Object {
                object_hash,
                mode: 0o600,
            },
            temp_name: format!(".rehow-tmp-{operation_id}-{index}"),
            step: TargetStep::Prepared,
        });
    }
    let (root_device, root_inode) = root.identity().map_err(map_io_clone_materialization)?;
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::GitMaterialize,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash,
        targets,
        stage: JournalStage::Prepared,
        completion: false,
    };
    if let Err(error) = journal.encode() {
        return Err(match error {
            JournalCodecError::TooLarge | JournalCodecError::ResourceLimit => {
                GitCloneMaterializationStorageError::TooLarge
            }
            JournalCodecError::Invalid => GitCloneMaterializationStorageError::Unsafe,
        });
    }

    let expected_paths = journal
        .targets
        .iter()
        .map(|target| target.relative_path.clone())
        .collect::<Vec<_>>();
    let temp_names = journal
        .targets
        .iter()
        .map(|target| target.temp_name.clone())
        .collect::<Vec<_>>();
    if let Some(persisted) = persisted {
        if !journal_matches_plan(&persisted, &journal) {
            return Err(GitCloneMaterializationStorageError::Conflict);
        }
        root.verify_git_clone_inventory(marker, &expected_paths, &temp_names, false, true)
            .map_err(map_io_clone_materialization)?;
    } else {
        let root_entries = root.entries(2).map_err(map_inspect_clone_materialization)?;
        if root_entries.is_empty() {
            root.verify_empty_workspace_root()
                .map_err(map_io_clone_materialization)?;
            root.create_git_clone_scaffold(marker)
                .map_err(map_io_clone_materialization)?;
        } else if root_entries == [".git"] {
            root.resume_git_clone_scaffold(marker)
                .map_err(map_io_clone_materialization)?;
        } else {
            return Err(GitCloneMaterializationStorageError::Conflict);
        }
        if root.entries(2).map_err(map_inspect_clone_materialization)? != [".git"] {
            return Err(GitCloneMaterializationStorageError::Conflict);
        }
        store
            .install_journal(&lock, &journal)
            .map_err(map_store_clone_materialization)?;
    }

    let mut categories = categories.into_iter().collect::<Vec<_>>();
    categories.sort_unstable();
    let mut attachments = attachments.into_iter().collect::<Vec<_>>();
    attachments.sort_unstable();
    root.prepare_git_clone_directories(&categories, &attachments)
        .map_err(map_io_clone_materialization)?;
    workspace::resume_journal(root, &store, &lock, operation_id)
        .map_err(map_workspace_clone_materialization)?;
    Ok(GitCloneMaterializationStorageOutcome)
}

fn journal_matches_plan(persisted: &Journal, prepared: &Journal) -> bool {
    persisted.operation_id == prepared.operation_id
        && persisted.operation_kind == prepared.operation_kind
        && persisted.workspace_id == prepared.workspace_id
        && persisted.root_hash == prepared.root_hash
        && persisted.root_device == prepared.root_device
        && persisted.root_inode == prepared.root_inode
        && persisted.created_at == prepared.created_at
        && persisted.plan_hash == prepared.plan_hash
        && persisted.targets.len() == prepared.targets.len()
        && persisted
            .targets
            .iter()
            .zip(&prepared.targets)
            .all(|(left, right)| left.same_intent(right))
}

fn map_store_clone_materialization(
    error: store::StoreError,
) -> GitCloneMaterializationStorageError {
    match error {
        store::StoreError::Busy => GitCloneMaterializationStorageError::Busy,
        store::StoreError::Conflict | store::StoreError::Corrupt => {
            GitCloneMaterializationStorageError::Conflict
        }
        store::StoreError::Unsafe => GitCloneMaterializationStorageError::Unsafe,
        store::StoreError::Unsupported => GitCloneMaterializationStorageError::Unsupported,
        store::StoreError::TooLarge => GitCloneMaterializationStorageError::TooLarge,
        store::StoreError::Io => GitCloneMaterializationStorageError::Io,
        #[cfg(test)]
        store::StoreError::Injected => GitCloneMaterializationStorageError::Io,
    }
}

fn map_io_clone_materialization(
    error: super::linux::MutationIoError,
) -> GitCloneMaterializationStorageError {
    match error {
        super::linux::MutationIoError::Conflict => GitCloneMaterializationStorageError::Conflict,
        super::linux::MutationIoError::Unsafe => GitCloneMaterializationStorageError::Unsafe,
        super::linux::MutationIoError::Unsupported => {
            GitCloneMaterializationStorageError::Unsupported
        }
        super::linux::MutationIoError::TooLarge => GitCloneMaterializationStorageError::TooLarge,
        super::linux::MutationIoError::Io => GitCloneMaterializationStorageError::Io,
    }
}

fn map_inspect_clone_materialization(
    error: super::WorkspaceInspectError,
) -> GitCloneMaterializationStorageError {
    match error {
        super::WorkspaceInspectError::UnsafeRoot => GitCloneMaterializationStorageError::Unsafe,
        super::WorkspaceInspectError::ResourceLimit => {
            GitCloneMaterializationStorageError::TooLarge
        }
        super::WorkspaceInspectError::ChangedDuringRead => {
            GitCloneMaterializationStorageError::Conflict
        }
        super::WorkspaceInspectError::NotFound
        | super::WorkspaceInspectError::NotDirectory
        | super::WorkspaceInspectError::PermissionDenied
        | super::WorkspaceInspectError::Io => GitCloneMaterializationStorageError::Io,
    }
}

fn map_workspace_clone_materialization(
    error: workspace::WorkspaceMutationError,
) -> GitCloneMaterializationStorageError {
    match error {
        workspace::WorkspaceMutationError::Conflict => {
            GitCloneMaterializationStorageError::Conflict
        }
        workspace::WorkspaceMutationError::Unsafe => GitCloneMaterializationStorageError::Unsafe,
        workspace::WorkspaceMutationError::Unsupported => {
            GitCloneMaterializationStorageError::Unsupported
        }
        workspace::WorkspaceMutationError::TooLarge => {
            GitCloneMaterializationStorageError::TooLarge
        }
        workspace::WorkspaceMutationError::Io => GitCloneMaterializationStorageError::Io,
    }
}

#[cfg(test)]
pub(super) fn set_attachment_source_change_fault() {
    store::set_source_change_fault();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentUpdateError<E> {
    Prepare(E),
    Busy,
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentUpdateOutcome {
    revision: Revision,
    changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentMoveOutcome {
    revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentRemoveError {
    Busy,
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentRemoveOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MultiDocumentUpdateError<E> {
    Prepare(E),
    InvalidPlan,
    Busy,
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MultiDocumentUpdateOutcome {
    changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AttachmentImportStorageOutcome {
    revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AttachmentRemovalStorageOutcome {
    revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AttachmentSourceState {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
    mode: u32,
    links: u64,
}

impl AttachmentSourceState {
    pub(crate) fn capture(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
            mode: metadata.mode(),
            links: metadata.nlink(),
        }
    }

    pub(crate) fn matches(self, metadata: &Metadata) -> bool {
        self == Self::capture(metadata)
    }

    pub(crate) fn size(self) -> u64 {
        self.size
    }
}

pub(crate) struct AttachmentImportPreview {
    relative_path: String,
    revision: Revision,
}

impl AttachmentImportPreview {
    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub(crate) fn revision(&self) -> Revision {
        self.revision
    }
}

pub(crate) struct AttachmentRemovalPreview {
    relative_path: String,
    guide_revision: Revision,
    attachment_revision: Revision,
    object_hash: [u8; 32],
    sha256: [u8; 32],
    size: u64,
    mode: u32,
}

impl AttachmentRemovalPreview {
    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub(crate) fn guide_revision(&self) -> Revision {
        self.guide_revision
    }

    pub(crate) fn size(&self) -> u64 {
        self.size
    }
}

pub(crate) struct PreparedDocument<'a> {
    relative_path: &'a str,
    revision: Revision,
    bytes: &'a [u8],
}

impl PreparedDocument<'_> {
    pub(crate) fn relative_path(&self) -> &str {
        self.relative_path
    }

    pub(crate) fn revision(&self) -> Revision {
        self.revision
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceInitializeError {
    Busy,
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
    Committed { revision: Revision },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceInitializeOutcome {
    revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CategoryCreateStorageError {
    Busy,
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CategoryCreateStorageOutcome {
    revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CategoryRemoveStorageError {
    Busy,
    Conflict,
    NotEmpty,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CategoryRemoveStorageOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentCreateError {
    Busy,
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentCreateOutcome {
    revision: Revision,
}

impl CategoryCreateStorageOutcome {
    pub(crate) fn revision(self) -> Revision {
        self.revision
    }
}

impl DocumentCreateOutcome {
    pub(crate) fn revision(self) -> Revision {
        self.revision
    }
}

impl WorkspaceInitializeOutcome {
    pub(crate) fn revision(self) -> Revision {
        self.revision
    }
}

impl DocumentUpdateOutcome {
    pub(crate) fn revision(self) -> Revision {
        self.revision
    }

    pub(crate) fn changed(self) -> bool {
        self.changed
    }
}

impl DocumentMoveOutcome {
    pub(crate) fn revision(self) -> Revision {
        self.revision
    }
}

impl AttachmentImportStorageOutcome {
    pub(crate) fn revision(self) -> Revision {
        self.revision
    }
}

impl AttachmentRemovalStorageOutcome {
    pub(crate) fn revision(self) -> Revision {
        self.revision
    }
}

impl MultiDocumentUpdateOutcome {
    pub(crate) fn changed(self) -> bool {
        self.changed
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_workspace_initialize(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    operation_id: OperationId,
    created_at: Timestamp,
    manifest: &[u8],
) -> Result<WorkspaceInitializeOutcome, WorkspaceInitializeError> {
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_initialize)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_initialize)?;
    root.verify_empty_workspace_root()
        .map_err(map_io_initialize)?;
    let target = root
        .open_managed_target("rehow.yaml")
        .map_err(map_io_initialize)?;
    if !matches!(
        target
            .state(super::DESCRIPTOR_LIMIT)
            .map_err(map_io_initialize)?,
        super::linux::ManagedState::Missing
    ) {
        return Err(WorkspaceInitializeError::Conflict);
    }

    let manifest_hash = store
        .store_object(&lock, manifest)
        .map_err(map_store_initialize)?;
    let (root_device, root_inode) = root.identity().map_err(map_io_initialize)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-workspace-initialize-plan-v1\0");
    hash_plan_field(&mut plan, manifest);
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::Create,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![Target {
            relative_path: "rehow.yaml".to_owned(),
            expected: ExpectedState::Absent,
            intended: IntendedState::Object {
                object_hash: manifest_hash,
                mode: 0o600,
            },
            temp_name: format!(".rehow-tmp-{operation_id}"),
            step: TargetStep::Prepared,
        }],
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if let Err(error) = execution {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            return Err(map_workspace_initialize(error));
        }
    }
    let revision = super::document_revision("rehow.yaml", manifest);
    root.create_categories_directory()
        .map_err(|_| WorkspaceInitializeError::Committed { revision })?;
    Ok(WorkspaceInitializeOutcome { revision })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_category_create(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    relative_directory: &str,
    operation_id: OperationId,
    created_at: Timestamp,
    descriptor: &[u8],
) -> Result<CategoryCreateStorageOutcome, CategoryCreateStorageError> {
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_category_create)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_category_create)?;
    root.create_category_directory(relative_directory)
        .map_err(map_io_category_create)?;
    let relative_path = format!("{relative_directory}/category.yaml");
    let target = root
        .open_managed_target(&relative_path)
        .map_err(|_| CategoryCreateStorageError::Incomplete)?;
    if !matches!(
        target
            .state(super::DESCRIPTOR_LIMIT)
            .map_err(|_| CategoryCreateStorageError::Incomplete)?,
        super::linux::ManagedState::Missing
    ) {
        return Err(CategoryCreateStorageError::Incomplete);
    }

    let object_hash = store
        .store_object(&lock, descriptor)
        .map_err(|_| CategoryCreateStorageError::Incomplete)?;
    let (root_device, root_inode) = root
        .identity()
        .map_err(|_| CategoryCreateStorageError::Incomplete)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-category-create-plan-v1\0");
    hash_plan_field(&mut plan, relative_path.as_bytes());
    hash_plan_field(&mut plan, descriptor);
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::Create,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![Target {
            relative_path: relative_path.clone(),
            expected: ExpectedState::Absent,
            intended: IntendedState::Object {
                object_hash,
                mode: 0o600,
            },
            temp_name: format!(".rehow-tmp-{operation_id}"),
            step: TargetStep::Prepared,
        }],
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if execution.is_err() {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            return Err(CategoryCreateStorageError::Incomplete);
        }
    }
    Ok(CategoryCreateStorageOutcome {
        revision: super::document_revision(&relative_path, descriptor),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_document_create(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    relative_path: &str,
    operation_id: OperationId,
    created_at: Timestamp,
    content: &[u8],
) -> Result<DocumentCreateOutcome, DocumentCreateError> {
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_document_create)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_document_create)?;
    let target = root
        .open_managed_target(relative_path)
        .map_err(map_io_document_create)?;
    if !matches!(
        target
            .state(super::GUIDE_LIMIT)
            .map_err(map_io_document_create)?,
        super::linux::ManagedState::Missing
    ) {
        return Err(DocumentCreateError::Conflict);
    }
    let object_hash = store
        .store_object(&lock, content)
        .map_err(map_store_document_create)?;
    let (root_device, root_inode) = root.identity().map_err(map_io_document_create)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-document-create-plan-v1\0");
    hash_plan_field(&mut plan, relative_path.as_bytes());
    hash_plan_field(&mut plan, content);
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::Create,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![Target {
            relative_path: relative_path.to_owned(),
            expected: ExpectedState::Absent,
            intended: IntendedState::Object {
                object_hash,
                mode: 0o600,
            },
            temp_name: format!(".rehow-tmp-{operation_id}"),
            step: TargetStep::Prepared,
        }],
        stage: JournalStage::Prepared,
        completion: false,
    };
    if workspace::execute_new_journal(root, &store, &lock, journal.clone()).is_err() {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            return Err(DocumentCreateError::Incomplete);
        }
    }
    Ok(DocumentCreateOutcome {
        revision: super::document_revision(relative_path, content),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_category_remove(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    relative_directory: &str,
    expected_revision: Revision,
    operation_id: OperationId,
    created_at: Timestamp,
) -> Result<CategoryRemoveStorageOutcome, CategoryRemoveStorageError> {
    let Some(directory_name) = relative_directory.strip_prefix("categories/") else {
        return Err(CategoryRemoveStorageError::Unsafe);
    };
    if !super::valid_category_directory(directory_name) {
        return Err(CategoryRemoveStorageError::Unsafe);
    }
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_category_remove)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_category_remove)?;
    let categories = root
        .open_private_directory("categories")
        .map_err(map_io_category_remove)?;
    let category = categories
        .open_private_directory(directory_name)
        .map_err(map_io_category_remove)?;
    let entries = category.entries(2).map_err(map_inspect_category_remove)?;
    if entries.len() != 1 || entries[0] != "category.yaml" {
        return Err(CategoryRemoveStorageError::NotEmpty);
    }
    let relative_path = format!("{relative_directory}/category.yaml");
    let target = root
        .open_managed_target(&relative_path)
        .map_err(map_io_category_remove)?;
    let current = match target
        .state(super::GUIDE_LIMIT)
        .map_err(map_io_category_remove)?
    {
        super::linux::ManagedState::Missing => {
            return Err(CategoryRemoveStorageError::Conflict);
        }
        super::linux::ManagedState::Regular(state) => state,
    };
    if super::document_revision(&relative_path, &current.bytes) != expected_revision {
        return Err(CategoryRemoveStorageError::Conflict);
    }
    let object_hash = store
        .store_object(&lock, &current.bytes)
        .map_err(map_store_category_remove)?;
    let destination = format!(".rehow/trash/{operation_id}/{relative_path}");
    let (root_device, root_inode) = root.identity().map_err(map_io_category_remove)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-category-remove-plan-v1\0");
    hash_plan_field(&mut plan, relative_directory.as_bytes());
    hash_plan_field(&mut plan, expected_revision.to_string().as_bytes());
    plan.update(&object_hash);
    plan.update(&current.mode.to_be_bytes());
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::Delete,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![Target {
            relative_path,
            expected: ExpectedState::Existing {
                revision: expected_revision,
                object_hash,
                mode: current.mode,
            },
            intended: IntendedState::Trash {
                object_hash,
                mode: current.mode,
                destination,
            },
            temp_name: format!(".rehow-tmp-{operation_id}"),
            step: TargetStep::Prepared,
        }],
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if let Err(error) = execution {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            return Err(map_workspace_category_remove(error));
        }
    }
    Ok(CategoryRemoveStorageOutcome)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_document_update<E, F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    relative_path: &str,
    expected_revision: Revision,
    operation_id: OperationId,
    created_at: Timestamp,
    prepare: F,
) -> Result<DocumentUpdateOutcome, DocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_update)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_update)?;
    let target = root
        .open_managed_target(relative_path)
        .map_err(map_io_update)?;
    let current = match target.state(super::GUIDE_LIMIT).map_err(map_io_update)? {
        super::linux::ManagedState::Missing => return Err(DocumentUpdateError::Conflict),
        super::linux::ManagedState::Regular(state) => state,
    };
    let actual_revision = super::document_revision(relative_path, &current.bytes);
    if actual_revision != expected_revision {
        return Err(DocumentUpdateError::Conflict);
    }
    let candidate = prepare(&current.bytes).map_err(DocumentUpdateError::Prepare)?;
    if candidate.as_ref() == current.bytes {
        return Ok(DocumentUpdateOutcome {
            revision: actual_revision,
            changed: false,
        });
    }

    let old_hash = store
        .store_object(&lock, &current.bytes)
        .map_err(map_store_update)?;
    let new_hash = store
        .store_object(&lock, &candidate)
        .map_err(map_store_update)?;
    let (root_device, root_inode) = root.identity().map_err(map_io_update)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-document-update-plan-v1\0");
    hash_plan_field(&mut plan, relative_path.as_bytes());
    hash_plan_field(&mut plan, expected_revision.to_string().as_bytes());
    plan.update(&old_hash);
    plan.update(&new_hash);
    plan.update(&current.mode.to_be_bytes());
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::Update,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![Target {
            relative_path: relative_path.to_owned(),
            expected: ExpectedState::Existing {
                revision: expected_revision,
                object_hash: old_hash,
                mode: current.mode,
            },
            intended: IntendedState::Object {
                object_hash: new_hash,
                mode: current.mode,
            },
            temp_name: format!(".rehow-tmp-{operation_id}"),
            step: TargetStep::Prepared,
        }],
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if let Err(error) = execution {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            return Err(map_workspace_update(error));
        }
    }
    Ok(DocumentUpdateOutcome {
        revision: super::document_revision(relative_path, &candidate),
        changed: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn preview_attachment_import<E, F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    guide_relative_path: &str,
    guide_id: GuideId,
    expected_revision: Revision,
    filename: &str,
    prepare: F,
) -> Result<AttachmentImportPreview, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8], &str) -> Result<Box<[u8]>, E>,
{
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_multi)?;
    let _lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_multi)?;
    let guide_target = root
        .open_managed_target(guide_relative_path)
        .map_err(map_io_multi)?;
    let current = match guide_target
        .state(super::GUIDE_LIMIT)
        .map_err(map_io_multi)?
    {
        super::linux::ManagedState::Missing => return Err(MultiDocumentUpdateError::Conflict),
        super::linux::ManagedState::Regular(state) => state,
    };
    if super::document_revision(guide_relative_path, &current.bytes) != expected_revision {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let relative_path = root
        .preview_attachment_path(&guide_id, filename)
        .map_err(map_io_multi)?;
    let candidate =
        prepare(&current.bytes, &relative_path).map_err(MultiDocumentUpdateError::Prepare)?;
    if candidate.as_ref() == current.bytes {
        return Err(MultiDocumentUpdateError::InvalidPlan);
    }
    Ok(AttachmentImportPreview {
        relative_path,
        revision: super::document_revision(guide_relative_path, &candidate),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_attachment_import<E, F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    guide_relative_path: &str,
    guide_id: GuideId,
    expected_revision: Revision,
    operation_id: OperationId,
    created_at: Timestamp,
    filename: &str,
    source: &mut File,
    source_state: &AttachmentSourceState,
    approved_relative_path: &str,
    approved_revision: Revision,
    approved_object_hash: [u8; 32],
    approved_sha256: [u8; 32],
    approved_size: u64,
    prepare: F,
) -> Result<AttachmentImportStorageOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8], &str, [u8; 32], u64) -> Result<Box<[u8]>, E>,
{
    const IMPORT_LIMIT: u64 = 32 * 1024 * 1024;

    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_multi)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_multi)?;
    let guide_target = root
        .open_managed_target(guide_relative_path)
        .map_err(map_io_multi)?;
    let current = match guide_target
        .state(super::GUIDE_LIMIT)
        .map_err(map_io_multi)?
    {
        super::linux::ManagedState::Missing => return Err(MultiDocumentUpdateError::Conflict),
        super::linux::ManagedState::Regular(state) => state,
    };
    if super::document_revision(guide_relative_path, &current.bytes) != expected_revision {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let attachment_relative_path = root
        .preview_attachment_path(&guide_id, filename)
        .map_err(map_io_multi)?;
    if attachment_relative_path != approved_relative_path {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let candidate = prepare(
        &current.bytes,
        &attachment_relative_path,
        approved_sha256,
        approved_size,
    )
    .map_err(MultiDocumentUpdateError::Prepare)?;
    if candidate.as_ref() == current.bytes
        || super::document_revision(guide_relative_path, &candidate) != approved_revision
    {
        return Err(MultiDocumentUpdateError::InvalidPlan);
    }
    let old_hash = store
        .store_object(&lock, &current.bytes)
        .map_err(map_store_multi)?;
    let new_hash = store
        .store_object(&lock, &candidate)
        .map_err(map_store_multi)?;
    root.prepare_attachment_directory(&guide_id)
        .map_err(map_io_multi)?;
    let final_relative_path = root
        .available_attachment_path(&guide_id, filename)
        .map_err(map_io_multi)?;
    if final_relative_path != attachment_relative_path {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let streamed = store
        .store_streamed_object(
            &lock,
            source,
            IMPORT_LIMIT,
            &store::StreamedObjectExpectation {
                source_state,
                object_hash: &approved_object_hash,
                sha256: &approved_sha256,
                size: approved_size,
            },
        )
        .map_err(map_store_multi)?;
    let (root_device, root_inode) = root.identity().map_err(map_io_multi)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-attachment-import-plan-v1\0");
    hash_plan_field(&mut plan, guide_relative_path.as_bytes());
    hash_plan_field(&mut plan, attachment_relative_path.as_bytes());
    hash_plan_field(&mut plan, expected_revision.to_string().as_bytes());
    plan.update(&streamed.object_hash);
    plan.update(&old_hash);
    plan.update(&new_hash);
    plan.update(&streamed.size.to_be_bytes());
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::MultiFile,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![
            Target {
                relative_path: attachment_relative_path,
                expected: ExpectedState::Absent,
                intended: IntendedState::Object {
                    object_hash: streamed.object_hash,
                    mode: 0o600,
                },
                temp_name: format!(".rehow-tmp-{operation_id}-attachment"),
                step: TargetStep::Prepared,
            },
            Target {
                relative_path: guide_relative_path.to_owned(),
                expected: ExpectedState::Existing {
                    revision: expected_revision,
                    object_hash: old_hash,
                    mode: current.mode,
                },
                intended: IntendedState::Object {
                    object_hash: new_hash,
                    mode: current.mode,
                },
                temp_name: format!(".rehow-tmp-{operation_id}-guide"),
                step: TargetStep::Prepared,
            },
        ],
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if let Err(error) = execution {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            if streamed.created
                && store
                    .journal_missing(&lock, operation_id)
                    .map_err(map_store_multi)?
            {
                store
                    .remove_created_object(&lock, &streamed.object_hash)
                    .map_err(map_store_multi)?;
            }
            return Err(map_workspace_multi(error));
        }
    }
    Ok(AttachmentImportStorageOutcome {
        revision: super::document_revision(guide_relative_path, &candidate),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn preview_attachment_removal<E, F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    guide_relative_path: &str,
    expected_guide_revision: Revision,
    attachment_relative_path: &str,
    declared_sha256: [u8; 32],
    declared_size: Option<u64>,
    prepare: F,
) -> Result<AttachmentRemovalPreview, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    const ATTACHMENT_LIMIT: u64 = 64 * 1024 * 1024;

    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_multi)?;
    let _lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_multi)?;
    let guide_target = root
        .open_managed_target(guide_relative_path)
        .map_err(map_io_multi)?;
    let current = match guide_target
        .state(super::GUIDE_LIMIT)
        .map_err(map_io_multi)?
    {
        super::linux::ManagedState::Missing => return Err(MultiDocumentUpdateError::Conflict),
        super::linux::ManagedState::Regular(state) => state,
    };
    if super::document_revision(guide_relative_path, &current.bytes) != expected_guide_revision {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let candidate = prepare(&current.bytes).map_err(MultiDocumentUpdateError::Prepare)?;
    if candidate.as_ref() == current.bytes {
        return Err(MultiDocumentUpdateError::InvalidPlan);
    }
    let attachment = root
        .open_managed_target(attachment_relative_path)
        .map_err(map_io_multi)?;
    let super::linux::AttachmentState::Regular {
        object_hash,
        sha256,
        revision,
        size,
        mode,
    } = attachment
        .attachment_state(attachment_relative_path, ATTACHMENT_LIMIT)
        .map_err(map_io_multi)?
    else {
        return Err(MultiDocumentUpdateError::Conflict);
    };
    if sha256 != declared_sha256 || declared_size.is_some_and(|declared| declared != size) {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    Ok(AttachmentRemovalPreview {
        relative_path: attachment_relative_path.to_owned(),
        guide_revision: super::document_revision(guide_relative_path, &candidate),
        attachment_revision: revision,
        object_hash,
        sha256,
        size,
        mode,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_attachment_removal<E, F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    guide_relative_path: &str,
    expected_guide_revision: Revision,
    operation_id: OperationId,
    created_at: Timestamp,
    approved: &AttachmentRemovalPreview,
    prepare: F,
) -> Result<AttachmentRemovalStorageOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    const ATTACHMENT_LIMIT: u64 = 64 * 1024 * 1024;

    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_multi)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_multi)?;
    let guide_target = root
        .open_managed_target(guide_relative_path)
        .map_err(map_io_multi)?;
    let current = match guide_target
        .state(super::GUIDE_LIMIT)
        .map_err(map_io_multi)?
    {
        super::linux::ManagedState::Missing => return Err(MultiDocumentUpdateError::Conflict),
        super::linux::ManagedState::Regular(state) => state,
    };
    if super::document_revision(guide_relative_path, &current.bytes) != expected_guide_revision {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let candidate = prepare(&current.bytes).map_err(MultiDocumentUpdateError::Prepare)?;
    if candidate.as_ref() == current.bytes
        || super::document_revision(guide_relative_path, &candidate) != approved.guide_revision
    {
        return Err(MultiDocumentUpdateError::InvalidPlan);
    }
    let attachment = root
        .open_managed_target(&approved.relative_path)
        .map_err(map_io_multi)?;
    if attachment
        .attachment_state(&approved.relative_path, ATTACHMENT_LIMIT)
        .map_err(map_io_multi)?
        != (super::linux::AttachmentState::Regular {
            object_hash: approved.object_hash,
            sha256: approved.sha256,
            revision: approved.attachment_revision,
            size: approved.size,
            mode: approved.mode,
        })
    {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let destination = format!(".rehow/trash/{operation_id}/{}", approved.relative_path);
    let trash_target = root
        .prepare_trash_target(&destination)
        .map_err(map_io_multi)?;
    if !matches!(
        trash_target
            .attachment_state(&approved.relative_path, ATTACHMENT_LIMIT)
            .map_err(map_io_multi)?,
        super::linux::AttachmentState::Missing
    ) {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    trash_target.sync_directory().map_err(map_io_multi)?;
    let (root_device, root_inode) = root.identity().map_err(map_io_multi)?;
    let old_guide_hash = store
        .store_object(&lock, &current.bytes)
        .map_err(map_store_multi)?;
    let new_guide_hash = store
        .store_object(&lock, &candidate)
        .map_err(map_store_multi)?;
    let mut source = attachment
        .open_attachment_file(&approved.relative_path, ATTACHMENT_LIMIT)
        .map_err(map_io_multi)?
        .ok_or(MultiDocumentUpdateError::Conflict)?;
    let source_state = AttachmentSourceState::capture(
        &source
            .metadata()
            .map_err(|_| MultiDocumentUpdateError::Io)?,
    );
    let streamed = store
        .store_streamed_object(
            &lock,
            &mut source,
            ATTACHMENT_LIMIT,
            &store::StreamedObjectExpectation {
                source_state: &source_state,
                object_hash: &approved.object_hash,
                sha256: &approved.sha256,
                size: approved.size,
            },
        )
        .map_err(map_store_multi)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-attachment-removal-plan-v1\0");
    hash_plan_field(&mut plan, guide_relative_path.as_bytes());
    hash_plan_field(&mut plan, approved.relative_path.as_bytes());
    hash_plan_field(&mut plan, expected_guide_revision.to_string().as_bytes());
    hash_plan_field(
        &mut plan,
        approved.attachment_revision.to_string().as_bytes(),
    );
    plan.update(&old_guide_hash);
    plan.update(&new_guide_hash);
    plan.update(&streamed.object_hash);
    plan.update(&approved.sha256);
    plan.update(&approved.size.to_be_bytes());
    plan.update(&approved.mode.to_be_bytes());
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::MultiFile,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![
            Target {
                relative_path: guide_relative_path.to_owned(),
                expected: ExpectedState::Existing {
                    revision: expected_guide_revision,
                    object_hash: old_guide_hash,
                    mode: current.mode,
                },
                intended: IntendedState::Object {
                    object_hash: new_guide_hash,
                    mode: current.mode,
                },
                temp_name: format!(".rehow-tmp-{operation_id}-guide"),
                step: TargetStep::Prepared,
            },
            Target {
                relative_path: approved.relative_path.clone(),
                expected: ExpectedState::Existing {
                    revision: approved.attachment_revision,
                    object_hash: streamed.object_hash,
                    mode: approved.mode,
                },
                intended: IntendedState::Trash {
                    object_hash: streamed.object_hash,
                    mode: approved.mode,
                    destination,
                },
                temp_name: format!(".rehow-tmp-{operation_id}-attachment"),
                step: TargetStep::Prepared,
            },
        ],
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if let Err(error) = execution {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            if streamed.created
                && store
                    .journal_missing(&lock, operation_id)
                    .map_err(map_store_multi)?
            {
                store
                    .remove_created_object(&lock, &streamed.object_hash)
                    .map_err(map_store_multi)?;
            }
            return Err(map_workspace_multi(error));
        }
    }
    Ok(AttachmentRemovalStorageOutcome {
        revision: approved.guide_revision,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_document_move<E, F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    source_relative_path: &str,
    destination_relative_path: &str,
    expected_revision: Revision,
    operation_id: OperationId,
    created_at: Timestamp,
    prepare: F,
) -> Result<DocumentMoveOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    if source_relative_path == destination_relative_path {
        return Err(MultiDocumentUpdateError::InvalidPlan);
    }
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_multi)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_multi)?;
    let source = root
        .open_managed_target(source_relative_path)
        .map_err(map_io_multi)?;
    let current = match source.state(super::GUIDE_LIMIT).map_err(map_io_multi)? {
        super::linux::ManagedState::Missing => return Err(MultiDocumentUpdateError::Conflict),
        super::linux::ManagedState::Regular(state) => state,
    };
    let actual_revision = super::document_revision(source_relative_path, &current.bytes);
    if actual_revision != expected_revision {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let destination = root
        .open_managed_target(destination_relative_path)
        .map_err(map_io_multi)?;
    if !matches!(
        destination
            .state(super::GUIDE_LIMIT)
            .map_err(map_io_multi)?,
        super::linux::ManagedState::Missing
    ) {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let candidate = prepare(&current.bytes).map_err(MultiDocumentUpdateError::Prepare)?;
    let old_hash = store
        .store_object(&lock, &current.bytes)
        .map_err(map_store_multi)?;
    let new_hash = store
        .store_object(&lock, &candidate)
        .map_err(map_store_multi)?;
    let (root_device, root_inode) = root.identity().map_err(map_io_multi)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-document-move-plan-v1\0");
    hash_plan_field(&mut plan, source_relative_path.as_bytes());
    hash_plan_field(&mut plan, destination_relative_path.as_bytes());
    hash_plan_field(&mut plan, expected_revision.to_string().as_bytes());
    plan.update(&old_hash);
    plan.update(&new_hash);
    plan.update(&current.mode.to_be_bytes());
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::MultiFile,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![
            Target {
                relative_path: destination_relative_path.to_owned(),
                expected: ExpectedState::Absent,
                intended: IntendedState::Object {
                    object_hash: new_hash,
                    mode: current.mode,
                },
                temp_name: format!(".rehow-tmp-{operation_id}-destination"),
                step: TargetStep::Prepared,
            },
            Target {
                relative_path: source_relative_path.to_owned(),
                expected: ExpectedState::Existing {
                    revision: expected_revision,
                    object_hash: old_hash,
                    mode: current.mode,
                },
                intended: IntendedState::Absent,
                temp_name: format!(".rehow-tmp-{operation_id}-source"),
                step: TargetStep::Prepared,
            },
        ],
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if let Err(error) = execution {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            return Err(map_workspace_multi(error));
        }
    }
    Ok(DocumentMoveOutcome {
        revision: super::document_revision(destination_relative_path, &candidate),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_document_remove(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    relative_path: &str,
    expected_revision: Revision,
    operation_id: OperationId,
    created_at: Timestamp,
) -> Result<DocumentRemoveOutcome, DocumentRemoveError> {
    let components = relative_path.split('/').collect::<Vec<_>>();
    if !matches!(
        components.as_slice(),
        ["categories", directory, guide]
            if super::valid_category_directory(directory) && super::valid_guide_filename(guide)
    ) {
        return Err(DocumentRemoveError::Unsafe);
    }
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_document_remove)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_document_remove)?;
    let target = root
        .open_managed_target(relative_path)
        .map_err(map_io_document_remove)?;
    let current = match target
        .state(super::GUIDE_LIMIT)
        .map_err(map_io_document_remove)?
    {
        super::linux::ManagedState::Missing => return Err(DocumentRemoveError::Conflict),
        super::linux::ManagedState::Regular(state) => state,
    };
    if super::document_revision(relative_path, &current.bytes) != expected_revision {
        return Err(DocumentRemoveError::Conflict);
    }
    let object_hash = store
        .store_object(&lock, &current.bytes)
        .map_err(map_store_document_remove)?;
    let destination = format!(".rehow/trash/{operation_id}/{relative_path}");
    let (root_device, root_inode) = root.identity().map_err(map_io_document_remove)?;
    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-document-remove-plan-v1\0");
    hash_plan_field(&mut plan, relative_path.as_bytes());
    hash_plan_field(&mut plan, expected_revision.to_string().as_bytes());
    plan.update(&object_hash);
    plan.update(&current.mode.to_be_bytes());
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::Delete,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets: vec![Target {
            relative_path: relative_path.to_owned(),
            expected: ExpectedState::Existing {
                revision: expected_revision,
                object_hash,
                mode: current.mode,
            },
            intended: IntendedState::Trash {
                object_hash,
                mode: current.mode,
                destination,
            },
            temp_name: format!(".rehow-tmp-{operation_id}"),
            step: TargetStep::Prepared,
        }],
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if let Err(error) = execution {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            return Err(map_workspace_document_remove(error));
        }
    }
    Ok(DocumentRemoveOutcome)
}

struct CurrentDocument {
    relative_path: String,
    revision: Revision,
    bytes: Vec<u8>,
    mode: u32,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_multi_document_update<E, F>(
    root: &super::linux::DirectoryCapability,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: WorkspaceId,
    inventory_directory: &str,
    expected_inventory_entries: &[String],
    requests: &[super::DocumentUpdateRequest],
    operation_id: OperationId,
    created_at: Timestamp,
    prepare: F,
) -> Result<MultiDocumentUpdateOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[PreparedDocument<'_>]) -> Result<Vec<Box<[u8]>>, E>,
{
    if requests.is_empty() || requests.len() > MAX_TARGETS {
        return Err(MultiDocumentUpdateError::InvalidPlan);
    }
    let store = store::RecoveryStore::open(
        state_home,
        workspace_id,
        root.root_hash(),
        root.capability_hash(),
    )
    .map_err(map_store_multi)?;
    let lock = store
        .acquire_lock(runtime_directory)
        .map_err(map_store_multi)?;
    if expected_inventory_entries.len() > MAX_TARGETS {
        return Err(MultiDocumentUpdateError::InvalidPlan);
    }
    let inventory = root
        .open_private_directory(inventory_directory)
        .map_err(map_io_multi)?;
    let actual_entries = inventory.entries(MAX_TARGETS).map_err(map_inspect_multi)?;
    let mut expected_entries = HashSet::new();
    expected_entries
        .try_reserve(expected_inventory_entries.len())
        .map_err(|_| MultiDocumentUpdateError::TooLarge)?;
    for entry in expected_inventory_entries {
        if !expected_entries.insert(entry.as_str()) {
            return Err(MultiDocumentUpdateError::InvalidPlan);
        }
    }
    if actual_entries.len() != expected_entries.len()
        || actual_entries
            .iter()
            .any(|entry| !expected_entries.contains(entry.as_str()))
    {
        return Err(MultiDocumentUpdateError::Conflict);
    }
    let mut paths = HashSet::new();
    paths
        .try_reserve(requests.len())
        .map_err(|_| MultiDocumentUpdateError::TooLarge)?;
    let mut current = Vec::new();
    current
        .try_reserve_exact(requests.len())
        .map_err(|_| MultiDocumentUpdateError::TooLarge)?;
    for request in requests {
        if !paths.insert(request.relative_path()) {
            return Err(MultiDocumentUpdateError::InvalidPlan);
        }
        let target = root
            .open_managed_target(request.relative_path())
            .map_err(map_io_multi)?;
        let state = match target.state(super::GUIDE_LIMIT).map_err(map_io_multi)? {
            super::linux::ManagedState::Missing => {
                return Err(MultiDocumentUpdateError::Conflict);
            }
            super::linux::ManagedState::Regular(state) => state,
        };
        let revision = super::document_revision(request.relative_path(), &state.bytes);
        if revision != request.expected_revision() {
            return Err(MultiDocumentUpdateError::Conflict);
        }
        current.push(CurrentDocument {
            relative_path: request.relative_path().to_owned(),
            revision,
            bytes: state.bytes,
            mode: state.mode,
        });
    }
    let prepared = current
        .iter()
        .map(|document| PreparedDocument {
            relative_path: &document.relative_path,
            revision: document.revision,
            bytes: &document.bytes,
        })
        .collect::<Vec<_>>();
    let candidates = prepare(&prepared).map_err(MultiDocumentUpdateError::Prepare)?;
    if candidates.len() != current.len() {
        return Err(MultiDocumentUpdateError::InvalidPlan);
    }

    let mut plan = blake3::Hasher::new();
    plan.update(b"rehow-multi-document-update-plan-v1\0");
    plan.update(&(current.len() as u64).to_be_bytes());
    let mut targets = Vec::new();
    targets
        .try_reserve_exact(current.len())
        .map_err(|_| MultiDocumentUpdateError::TooLarge)?;
    for (index, (document, candidate)) in current.iter().zip(&candidates).enumerate() {
        hash_plan_field(&mut plan, document.relative_path.as_bytes());
        hash_plan_field(&mut plan, document.revision.to_string().as_bytes());
        plan.update(blake3::hash(&document.bytes).as_bytes());
        plan.update(blake3::hash(candidate).as_bytes());
        plan.update(&document.mode.to_be_bytes());
        if candidate.as_ref() == document.bytes {
            continue;
        }
        let old_hash = store
            .store_object(&lock, &document.bytes)
            .map_err(map_store_multi)?;
        let new_hash = store
            .store_object(&lock, candidate)
            .map_err(map_store_multi)?;
        targets.push(Target {
            relative_path: document.relative_path.clone(),
            expected: ExpectedState::Existing {
                revision: document.revision,
                object_hash: old_hash,
                mode: document.mode,
            },
            intended: IntendedState::Object {
                object_hash: new_hash,
                mode: document.mode,
            },
            temp_name: format!(".rehow-tmp-{operation_id}-{index}"),
            step: TargetStep::Prepared,
        });
    }
    if targets.is_empty() {
        return Ok(MultiDocumentUpdateOutcome { changed: false });
    }
    let (root_device, root_inode) = root.identity().map_err(map_io_multi)?;
    let journal = Journal {
        generation: 1,
        operation_id,
        operation_kind: OperationKind::MultiFile,
        workspace_id,
        root_hash: root.root_hash(),
        root_device,
        root_inode,
        created_at,
        plan_hash: *plan.finalize().as_bytes(),
        targets,
        stage: JournalStage::Prepared,
        completion: false,
    };
    let execution = workspace::execute_new_journal(root, &store, &lock, journal.clone());
    if let Err(error) = execution {
        let committed = store
            .read_journal(&lock, operation_id)
            .is_ok_and(|persisted| {
                committed_matches(&persisted, &journal)
                    && workspace::resume_journal(root, &store, &lock, operation_id).is_ok()
            });
        if !committed {
            return Err(map_workspace_multi(error));
        }
    }
    Ok(MultiDocumentUpdateOutcome { changed: true })
}

fn committed_matches(committed: &Journal, prepared: &Journal) -> bool {
    committed.stage == JournalStage::Committed
        && committed.completion
        && committed.operation_id == prepared.operation_id
        && committed.operation_kind == prepared.operation_kind
        && committed.workspace_id == prepared.workspace_id
        && committed.root_hash == prepared.root_hash
        && committed.root_device == prepared.root_device
        && committed.root_inode == prepared.root_inode
        && committed.created_at == prepared.created_at
        && committed.plan_hash == prepared.plan_hash
        && committed.targets.len() == prepared.targets.len()
        && committed
            .targets
            .iter()
            .zip(&prepared.targets)
            .all(|(left, right)| left.step == TargetStep::Complete && left.same_intent(right))
}

fn hash_plan_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn map_store_update<E>(error: store::StoreError) -> DocumentUpdateError<E> {
    match error {
        store::StoreError::Busy => DocumentUpdateError::Busy,
        store::StoreError::Conflict | store::StoreError::Corrupt => DocumentUpdateError::Conflict,
        store::StoreError::Unsafe => DocumentUpdateError::Unsafe,
        store::StoreError::Unsupported => DocumentUpdateError::Unsupported,
        store::StoreError::TooLarge => DocumentUpdateError::TooLarge,
        store::StoreError::Io => DocumentUpdateError::Io,
        #[cfg(test)]
        store::StoreError::Injected => DocumentUpdateError::Io,
    }
}

fn map_store_multi<E>(error: store::StoreError) -> MultiDocumentUpdateError<E> {
    match error {
        store::StoreError::Busy => MultiDocumentUpdateError::Busy,
        store::StoreError::Conflict | store::StoreError::Corrupt => {
            MultiDocumentUpdateError::Conflict
        }
        store::StoreError::Unsafe => MultiDocumentUpdateError::Unsafe,
        store::StoreError::Unsupported => MultiDocumentUpdateError::Unsupported,
        store::StoreError::TooLarge => MultiDocumentUpdateError::TooLarge,
        store::StoreError::Io => MultiDocumentUpdateError::Io,
        #[cfg(test)]
        store::StoreError::Injected => MultiDocumentUpdateError::Io,
    }
}

fn map_io_multi<E>(error: super::linux::MutationIoError) -> MultiDocumentUpdateError<E> {
    match error {
        super::linux::MutationIoError::Unsafe => MultiDocumentUpdateError::Unsafe,
        super::linux::MutationIoError::Conflict => MultiDocumentUpdateError::Conflict,
        super::linux::MutationIoError::Unsupported => MultiDocumentUpdateError::Unsupported,
        super::linux::MutationIoError::TooLarge => MultiDocumentUpdateError::TooLarge,
        super::linux::MutationIoError::Io => MultiDocumentUpdateError::Io,
    }
}

fn map_inspect_multi<E>(error: super::WorkspaceInspectError) -> MultiDocumentUpdateError<E> {
    match error {
        super::WorkspaceInspectError::UnsafeRoot => MultiDocumentUpdateError::Unsafe,
        super::WorkspaceInspectError::ResourceLimit => MultiDocumentUpdateError::TooLarge,
        super::WorkspaceInspectError::ChangedDuringRead => MultiDocumentUpdateError::Conflict,
        super::WorkspaceInspectError::NotFound
        | super::WorkspaceInspectError::NotDirectory
        | super::WorkspaceInspectError::PermissionDenied
        | super::WorkspaceInspectError::Io => MultiDocumentUpdateError::Io,
    }
}

fn map_workspace_multi<E>(error: workspace::WorkspaceMutationError) -> MultiDocumentUpdateError<E> {
    match error {
        workspace::WorkspaceMutationError::Unsafe => MultiDocumentUpdateError::Unsafe,
        workspace::WorkspaceMutationError::Conflict => MultiDocumentUpdateError::Conflict,
        workspace::WorkspaceMutationError::Unsupported => MultiDocumentUpdateError::Unsupported,
        workspace::WorkspaceMutationError::TooLarge => MultiDocumentUpdateError::TooLarge,
        workspace::WorkspaceMutationError::Io => MultiDocumentUpdateError::Io,
    }
}

fn map_io_update<E>(error: super::linux::MutationIoError) -> DocumentUpdateError<E> {
    match error {
        super::linux::MutationIoError::Unsafe => DocumentUpdateError::Unsafe,
        super::linux::MutationIoError::Conflict => DocumentUpdateError::Conflict,
        super::linux::MutationIoError::Unsupported => DocumentUpdateError::Unsupported,
        super::linux::MutationIoError::TooLarge => DocumentUpdateError::TooLarge,
        super::linux::MutationIoError::Io => DocumentUpdateError::Io,
    }
}

fn map_workspace_update<E>(error: workspace::WorkspaceMutationError) -> DocumentUpdateError<E> {
    match error {
        workspace::WorkspaceMutationError::Unsafe => DocumentUpdateError::Unsafe,
        workspace::WorkspaceMutationError::Conflict => DocumentUpdateError::Conflict,
        workspace::WorkspaceMutationError::Unsupported => DocumentUpdateError::Unsupported,
        workspace::WorkspaceMutationError::TooLarge => DocumentUpdateError::TooLarge,
        workspace::WorkspaceMutationError::Io => DocumentUpdateError::Io,
    }
}

fn map_store_document_remove(error: store::StoreError) -> DocumentRemoveError {
    match error {
        store::StoreError::Busy => DocumentRemoveError::Busy,
        store::StoreError::Conflict | store::StoreError::Corrupt => DocumentRemoveError::Conflict,
        store::StoreError::Unsafe => DocumentRemoveError::Unsafe,
        store::StoreError::Unsupported => DocumentRemoveError::Unsupported,
        store::StoreError::TooLarge => DocumentRemoveError::TooLarge,
        store::StoreError::Io => DocumentRemoveError::Io,
        #[cfg(test)]
        store::StoreError::Injected => DocumentRemoveError::Io,
    }
}

fn map_io_document_remove(error: super::linux::MutationIoError) -> DocumentRemoveError {
    match error {
        super::linux::MutationIoError::Unsafe => DocumentRemoveError::Unsafe,
        super::linux::MutationIoError::Conflict => DocumentRemoveError::Conflict,
        super::linux::MutationIoError::Unsupported => DocumentRemoveError::Unsupported,
        super::linux::MutationIoError::TooLarge => DocumentRemoveError::TooLarge,
        super::linux::MutationIoError::Io => DocumentRemoveError::Io,
    }
}

fn map_workspace_document_remove(error: workspace::WorkspaceMutationError) -> DocumentRemoveError {
    match error {
        workspace::WorkspaceMutationError::Unsafe => DocumentRemoveError::Unsafe,
        workspace::WorkspaceMutationError::Conflict => DocumentRemoveError::Conflict,
        workspace::WorkspaceMutationError::Unsupported => DocumentRemoveError::Unsupported,
        workspace::WorkspaceMutationError::TooLarge => DocumentRemoveError::TooLarge,
        workspace::WorkspaceMutationError::Io => DocumentRemoveError::Io,
    }
}

fn map_store_initialize(error: store::StoreError) -> WorkspaceInitializeError {
    match error {
        store::StoreError::Busy => WorkspaceInitializeError::Busy,
        store::StoreError::Conflict | store::StoreError::Corrupt => {
            WorkspaceInitializeError::Conflict
        }
        store::StoreError::Unsafe => WorkspaceInitializeError::Unsafe,
        store::StoreError::Unsupported => WorkspaceInitializeError::Unsupported,
        store::StoreError::TooLarge => WorkspaceInitializeError::TooLarge,
        store::StoreError::Io => WorkspaceInitializeError::Io,
        #[cfg(test)]
        store::StoreError::Injected => WorkspaceInitializeError::Io,
    }
}

fn map_io_initialize(error: super::linux::MutationIoError) -> WorkspaceInitializeError {
    match error {
        super::linux::MutationIoError::Unsafe => WorkspaceInitializeError::Unsafe,
        super::linux::MutationIoError::Conflict => WorkspaceInitializeError::Conflict,
        super::linux::MutationIoError::Unsupported => WorkspaceInitializeError::Unsupported,
        super::linux::MutationIoError::TooLarge => WorkspaceInitializeError::TooLarge,
        super::linux::MutationIoError::Io => WorkspaceInitializeError::Io,
    }
}

fn map_workspace_initialize(error: workspace::WorkspaceMutationError) -> WorkspaceInitializeError {
    match error {
        workspace::WorkspaceMutationError::Unsafe => WorkspaceInitializeError::Unsafe,
        workspace::WorkspaceMutationError::Conflict => WorkspaceInitializeError::Conflict,
        workspace::WorkspaceMutationError::Unsupported => WorkspaceInitializeError::Unsupported,
        workspace::WorkspaceMutationError::TooLarge => WorkspaceInitializeError::TooLarge,
        workspace::WorkspaceMutationError::Io => WorkspaceInitializeError::Io,
    }
}

fn map_store_category_create(error: store::StoreError) -> CategoryCreateStorageError {
    match error {
        store::StoreError::Busy => CategoryCreateStorageError::Busy,
        store::StoreError::Conflict | store::StoreError::Corrupt => {
            CategoryCreateStorageError::Conflict
        }
        store::StoreError::Unsafe => CategoryCreateStorageError::Unsafe,
        store::StoreError::Unsupported => CategoryCreateStorageError::Unsupported,
        store::StoreError::TooLarge => CategoryCreateStorageError::TooLarge,
        store::StoreError::Io => CategoryCreateStorageError::Io,
        #[cfg(test)]
        store::StoreError::Injected => CategoryCreateStorageError::Io,
    }
}

fn map_io_category_create(error: super::linux::MutationIoError) -> CategoryCreateStorageError {
    match error {
        super::linux::MutationIoError::Unsafe => CategoryCreateStorageError::Unsafe,
        super::linux::MutationIoError::Conflict => CategoryCreateStorageError::Conflict,
        super::linux::MutationIoError::Unsupported => CategoryCreateStorageError::Unsupported,
        super::linux::MutationIoError::TooLarge => CategoryCreateStorageError::TooLarge,
        super::linux::MutationIoError::Io => CategoryCreateStorageError::Io,
    }
}

fn map_store_document_create(error: store::StoreError) -> DocumentCreateError {
    match error {
        store::StoreError::Busy => DocumentCreateError::Busy,
        store::StoreError::Conflict | store::StoreError::Corrupt => DocumentCreateError::Conflict,
        store::StoreError::Unsafe => DocumentCreateError::Unsafe,
        store::StoreError::Unsupported => DocumentCreateError::Unsupported,
        store::StoreError::TooLarge => DocumentCreateError::TooLarge,
        store::StoreError::Io => DocumentCreateError::Io,
        #[cfg(test)]
        store::StoreError::Injected => DocumentCreateError::Io,
    }
}

fn map_io_document_create(error: super::linux::MutationIoError) -> DocumentCreateError {
    match error {
        super::linux::MutationIoError::Unsafe => DocumentCreateError::Unsafe,
        super::linux::MutationIoError::Conflict => DocumentCreateError::Conflict,
        super::linux::MutationIoError::Unsupported => DocumentCreateError::Unsupported,
        super::linux::MutationIoError::TooLarge => DocumentCreateError::TooLarge,
        super::linux::MutationIoError::Io => DocumentCreateError::Io,
    }
}

fn map_store_category_remove(error: store::StoreError) -> CategoryRemoveStorageError {
    match error {
        store::StoreError::Busy => CategoryRemoveStorageError::Busy,
        store::StoreError::Conflict | store::StoreError::Corrupt => {
            CategoryRemoveStorageError::Conflict
        }
        store::StoreError::Unsafe => CategoryRemoveStorageError::Unsafe,
        store::StoreError::Unsupported => CategoryRemoveStorageError::Unsupported,
        store::StoreError::TooLarge => CategoryRemoveStorageError::TooLarge,
        store::StoreError::Io => CategoryRemoveStorageError::Io,
        #[cfg(test)]
        store::StoreError::Injected => CategoryRemoveStorageError::Io,
    }
}

fn map_io_category_remove(error: super::linux::MutationIoError) -> CategoryRemoveStorageError {
    match error {
        super::linux::MutationIoError::Unsafe => CategoryRemoveStorageError::Unsafe,
        super::linux::MutationIoError::Conflict => CategoryRemoveStorageError::Conflict,
        super::linux::MutationIoError::Unsupported => CategoryRemoveStorageError::Unsupported,
        super::linux::MutationIoError::TooLarge => CategoryRemoveStorageError::TooLarge,
        super::linux::MutationIoError::Io => CategoryRemoveStorageError::Io,
    }
}

fn map_inspect_category_remove(error: super::WorkspaceInspectError) -> CategoryRemoveStorageError {
    match error {
        super::WorkspaceInspectError::UnsafeRoot => CategoryRemoveStorageError::Unsafe,
        super::WorkspaceInspectError::ResourceLimit => CategoryRemoveStorageError::NotEmpty,
        super::WorkspaceInspectError::ChangedDuringRead => CategoryRemoveStorageError::Conflict,
        super::WorkspaceInspectError::NotFound
        | super::WorkspaceInspectError::NotDirectory
        | super::WorkspaceInspectError::PermissionDenied
        | super::WorkspaceInspectError::Io => CategoryRemoveStorageError::Io,
    }
}

fn map_workspace_category_remove(
    error: workspace::WorkspaceMutationError,
) -> CategoryRemoveStorageError {
    match error {
        workspace::WorkspaceMutationError::Unsafe => CategoryRemoveStorageError::Unsafe,
        workspace::WorkspaceMutationError::Conflict => CategoryRemoveStorageError::Conflict,
        workspace::WorkspaceMutationError::Unsupported => CategoryRemoveStorageError::Unsupported,
        workspace::WorkspaceMutationError::TooLarge => CategoryRemoveStorageError::TooLarge,
        workspace::WorkspaceMutationError::Io => CategoryRemoveStorageError::Io,
    }
}

pub(crate) const JOURNAL_VERSION: u64 = 1;
// One clone journal must represent the complete accepted Workspace envelope:
// manifest, optional .gitignore, Categories, Guides, and Attachments.
pub(crate) const MAX_JOURNAL_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_TARGETS: usize = 21_002;
const MAX_PATH_BYTES: usize = 1_024;
const MAX_COMPONENT_BYTES: usize = 255;

pub(crate) type ContentHash = [u8; 32];

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Journal {
    pub(crate) generation: u64,
    pub(crate) operation_id: OperationId,
    pub(crate) operation_kind: OperationKind,
    pub(crate) workspace_id: WorkspaceId,
    pub(crate) root_hash: ContentHash,
    pub(crate) root_device: u64,
    pub(crate) root_inode: u64,
    pub(crate) created_at: Timestamp,
    pub(crate) plan_hash: ContentHash,
    pub(crate) targets: Vec<Target>,
    pub(crate) stage: JournalStage,
    pub(crate) completion: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationKind {
    Create,
    Update,
    Delete,
    MultiFile,
    Recovery,
    Migration,
    GitMaterialize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JournalStage {
    Prepared,
    Applying,
    Committed,
    Conflict,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct Target {
    pub(crate) relative_path: String,
    pub(crate) expected: ExpectedState,
    pub(crate) intended: IntendedState,
    pub(crate) temp_name: String,
    pub(crate) step: TargetStep,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum ExpectedState {
    Absent,
    Existing {
        revision: Revision,
        object_hash: ContentHash,
        mode: u32,
    },
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum IntendedState {
    Absent,
    Object {
        object_hash: ContentHash,
        mode: u32,
    },
    Trash {
        object_hash: ContentHash,
        mode: u32,
        destination: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetStep {
    Prepared,
    Swapped,
    DirectorySynced,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JournalCodecError {
    TooLarge,
    ResourceLimit,
    Invalid,
}

const GIT_COMMIT_INTENT_VERSION: u64 = 1;
const MAX_GIT_COMMIT_INTENT_BYTES: usize = 16 * 1024;

impl GitCommitIntent {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, JournalCodecError> {
        self.validate()?;
        let mut writer = Writer::new()?;
        writer.map(27)?;
        writer.key(0)?;
        writer.uint(GIT_COMMIT_INTENT_VERSION)?;
        writer.key(1)?;
        writer.uint(self.generation)?;
        writer.key(2)?;
        writer.text(&self.operation_id.to_string())?;
        writer.key(3)?;
        writer.text(&self.workspace_id.to_string())?;
        writer.key(4)?;
        writer.bytes(&self.root_hash)?;
        writer.key(5)?;
        writer.uint(self.root_device)?;
        writer.key(6)?;
        writer.uint(self.root_inode)?;
        writer.key(7)?;
        writer.uint(self.git_device)?;
        writer.key(8)?;
        writer.uint(self.git_inode)?;
        writer.key(9)?;
        writer.text(&self.branch_relative)?;
        writer.key(10)?;
        writer.uint(self.representation.code())?;
        writer.key(11)?;
        writer.bytes(&self.head_hash)?;
        writer.key(12)?;
        writer.bytes(&self.config_hash)?;
        writer.key(13)?;
        writer.optional_hash(self.before_index_hash.as_ref())?;
        writer.key(14)?;
        writer.uint(u64::from(self.before_index_mode))?;
        writer.key(15)?;
        writer.bytes(&self.candidate_index_hash)?;
        writer.key(16)?;
        writer.uint(u64::from(self.candidate_index_mode))?;
        writer.key(17)?;
        writer.optional_hash(self.packed_refs_hash.as_ref())?;
        writer.key(18)?;
        writer.optional_hash(self.before_ref_hash.as_ref())?;
        writer.key(19)?;
        writer.uint(u64::from(self.before_ref_mode))?;
        writer.key(20)?;
        writer.bytes(&self.candidate_ref_hash)?;
        writer.key(21)?;
        writer.uint(u64::from(self.candidate_ref_mode))?;
        writer.key(22)?;
        writer.bytes(&self.plan_hash)?;
        writer.key(23)?;
        writer.uint(self.stage.code())?;
        writer.key(24)?;
        writer.text(&self.staging_name)?;
        writer.key(25)?;
        writer.uint(self.staging_device)?;
        writer.key(26)?;
        writer.uint(self.staging_inode)?;
        if writer.bytes.len() > MAX_GIT_COMMIT_INTENT_BYTES {
            return Err(JournalCodecError::TooLarge);
        }
        Ok(writer.bytes)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, JournalCodecError> {
        if bytes.len() > MAX_GIT_COMMIT_INTENT_BYTES {
            return Err(JournalCodecError::TooLarge);
        }
        let mut reader = Reader::new(bytes);
        reader.map(27)?;
        reader.key(0)?;
        if reader.uint()? != GIT_COMMIT_INTENT_VERSION {
            return Err(JournalCodecError::Invalid);
        }
        reader.key(1)?;
        let generation = reader.uint()?;
        reader.key(2)?;
        let operation_id = reader.protocol_text::<OperationId>(36)?;
        reader.key(3)?;
        let workspace_id = reader.protocol_text::<WorkspaceId>(36)?;
        reader.key(4)?;
        let root_hash = reader.hash()?;
        reader.key(5)?;
        let root_device = reader.uint()?;
        reader.key(6)?;
        let root_inode = reader.uint()?;
        reader.key(7)?;
        let git_device = reader.uint()?;
        reader.key(8)?;
        let git_inode = reader.uint()?;
        reader.key(9)?;
        let branch_relative = reader.owned_text(MAX_PATH_BYTES)?;
        reader.key(10)?;
        let representation = GitCommitIntentBranchRepresentation::from_code(reader.uint()?)?;
        reader.key(11)?;
        let head_hash = reader.hash()?;
        reader.key(12)?;
        let config_hash = reader.hash()?;
        reader.key(13)?;
        let before_index_hash = reader.optional_hash()?;
        reader.key(14)?;
        let before_index_mode = reader.mode()?;
        reader.key(15)?;
        let candidate_index_hash = reader.hash()?;
        reader.key(16)?;
        let candidate_index_mode = reader.mode()?;
        reader.key(17)?;
        let packed_refs_hash = reader.optional_hash()?;
        reader.key(18)?;
        let before_ref_hash = reader.optional_hash()?;
        reader.key(19)?;
        let before_ref_mode = reader.mode()?;
        reader.key(20)?;
        let candidate_ref_hash = reader.hash()?;
        reader.key(21)?;
        let candidate_ref_mode = reader.mode()?;
        reader.key(22)?;
        let plan_hash = reader.hash()?;
        reader.key(23)?;
        let stage = GitCommitIntentStage::from_code(reader.uint()?)?;
        reader.key(24)?;
        let staging_name = reader.owned_text(64)?;
        reader.key(25)?;
        let staging_device = reader.uint()?;
        reader.key(26)?;
        let staging_inode = reader.uint()?;
        if !reader.finished() {
            return Err(JournalCodecError::Invalid);
        }
        let intent = Self {
            generation,
            operation_id,
            workspace_id,
            root_hash,
            root_device,
            root_inode,
            git_device,
            git_inode,
            branch_relative,
            representation,
            head_hash,
            config_hash,
            before_index_hash,
            before_index_mode,
            candidate_index_hash,
            candidate_index_mode,
            packed_refs_hash,
            before_ref_hash,
            before_ref_mode,
            candidate_ref_hash,
            candidate_ref_mode,
            plan_hash,
            stage,
            staging_name,
            staging_device,
            staging_inode,
        };
        intent.validate()?;
        Ok(intent)
    }

    fn validate(&self) -> Result<(), JournalCodecError> {
        if self.generation == 0
            || !valid_git_commit_intent_branch(&self.branch_relative)
            || self.staging_name.len() != 64
            || !self
                .staging_name
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || self.staging_device == 0
            || self.staging_inode == 0
            || !matches!(self.before_index_mode, 0o600 | 0o644)
            || !matches!(self.candidate_index_mode, 0o600 | 0o644)
            || !matches!(self.before_ref_mode, 0o600 | 0o644)
            || !matches!(self.candidate_ref_mode, 0o600 | 0o644)
            || (self.representation == GitCommitIntentBranchRepresentation::Unborn
                && (self.before_index_hash.is_some() || self.before_ref_hash.is_some()))
            || (self.representation == GitCommitIntentBranchRepresentation::Loose
                && (self.before_index_hash.is_none() || self.before_ref_hash.is_none()))
            || (self.representation == GitCommitIntentBranchRepresentation::Packed
                && (self.before_index_hash.is_none()
                    || self.before_ref_hash.is_some()
                    || self.packed_refs_hash.is_none()))
        {
            return Err(JournalCodecError::Invalid);
        }
        Ok(())
    }

    pub(crate) fn next_stage(
        &self,
        stage: GitCommitIntentStage,
    ) -> Result<Self, JournalCodecError> {
        let valid = matches!(
            (self.stage, stage),
            (
                GitCommitIntentStage::Prepared,
                GitCommitIntentStage::IndexPublished
            ) | (
                GitCommitIntentStage::IndexPublished,
                GitCommitIntentStage::RefPublished
            ) | (
                GitCommitIntentStage::RefPublished,
                GitCommitIntentStage::Complete
            ) | (
                GitCommitIntentStage::Prepared
                    | GitCommitIntentStage::IndexPublished
                    | GitCommitIntentStage::RefPublished,
                GitCommitIntentStage::Conflict
            )
        );
        if !valid {
            return Err(JournalCodecError::Invalid);
        }
        let mut next = self.clone();
        next.generation = next
            .generation
            .checked_add(1)
            .ok_or(JournalCodecError::ResourceLimit)?;
        next.stage = stage;
        Ok(next)
    }

    pub(super) fn is_next_generation_of(&self, next: &Self) -> bool {
        self.operation_id == next.operation_id
            && self.workspace_id == next.workspace_id
            && self.root_hash == next.root_hash
            && self.root_device == next.root_device
            && self.root_inode == next.root_inode
            && self.git_device == next.git_device
            && self.git_inode == next.git_inode
            && self.branch_relative == next.branch_relative
            && self.representation == next.representation
            && self.head_hash == next.head_hash
            && self.config_hash == next.config_hash
            && self.before_index_hash == next.before_index_hash
            && self.before_index_mode == next.before_index_mode
            && self.candidate_index_hash == next.candidate_index_hash
            && self.candidate_index_mode == next.candidate_index_mode
            && self.packed_refs_hash == next.packed_refs_hash
            && self.before_ref_hash == next.before_ref_hash
            && self.before_ref_mode == next.before_ref_mode
            && self.candidate_ref_hash == next.candidate_ref_hash
            && self.candidate_ref_mode == next.candidate_ref_mode
            && self.plan_hash == next.plan_hash
            && self.staging_name == next.staging_name
            && self.staging_device == next.staging_device
            && self.staging_inode == next.staging_inode
            && self.generation.checked_add(1) == Some(next.generation)
            && self
                .next_stage(next.stage)
                .is_ok_and(|expected| expected == *next)
    }
}

fn valid_git_commit_intent_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PATH_BYTES
        && value.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && !part.ends_with('.')
                && !part.ends_with(".lock")
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}

impl Journal {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, JournalCodecError> {
        self.validate()?;
        let mut writer = Writer::new()?;
        writer.map(13)?;
        writer.key(0)?;
        writer.uint(JOURNAL_VERSION)?;
        writer.key(1)?;
        writer.uint(self.generation)?;
        writer.key(2)?;
        writer.text(&self.operation_id.to_string())?;
        writer.key(3)?;
        writer.uint(self.operation_kind.code())?;
        writer.key(4)?;
        writer.text(&self.workspace_id.to_string())?;
        writer.key(5)?;
        writer.bytes(&self.root_hash)?;
        writer.key(6)?;
        writer.uint(self.root_device)?;
        writer.key(7)?;
        writer.uint(self.root_inode)?;
        writer.key(8)?;
        writer.text(&self.created_at.to_string())?;
        writer.key(9)?;
        writer.bytes(&self.plan_hash)?;
        writer.key(10)?;
        writer.array(self.targets.len() as u64)?;
        for target in &self.targets {
            target.encode(&mut writer)?;
        }
        writer.key(11)?;
        writer.uint(self.stage.code())?;
        writer.key(12)?;
        writer.boolean(self.completion)?;
        if writer.bytes.len() > MAX_JOURNAL_BYTES {
            return Err(JournalCodecError::TooLarge);
        }
        Ok(writer.bytes)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, JournalCodecError> {
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(JournalCodecError::TooLarge);
        }
        let mut reader = Reader::new(bytes);
        reader.map(13)?;
        reader.key(0)?;
        if reader.uint()? != JOURNAL_VERSION {
            return Err(JournalCodecError::Invalid);
        }
        reader.key(1)?;
        let generation = reader.uint()?;
        reader.key(2)?;
        let operation_id = reader.protocol_text::<OperationId>(36)?;
        reader.key(3)?;
        let operation_kind = OperationKind::from_code(reader.uint()?)?;
        reader.key(4)?;
        let workspace_id = reader.protocol_text::<WorkspaceId>(36)?;
        reader.key(5)?;
        let root_hash = reader.hash()?;
        reader.key(6)?;
        let root_device = reader.uint()?;
        reader.key(7)?;
        let root_inode = reader.uint()?;
        reader.key(8)?;
        let created_at = reader.protocol_text::<Timestamp>(30)?;
        reader.key(9)?;
        let plan_hash = reader.hash()?;
        reader.key(10)?;
        let target_count = reader.array_len(MAX_TARGETS)?;
        if target_count == 0 {
            return Err(JournalCodecError::Invalid);
        }
        let mut targets = Vec::new();
        targets
            .try_reserve_exact(target_count)
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        for _ in 0..target_count {
            targets.push(Target::decode(&mut reader)?);
        }
        reader.key(11)?;
        let stage = JournalStage::from_code(reader.uint()?)?;
        reader.key(12)?;
        let completion = reader.boolean()?;
        if !reader.finished() {
            return Err(JournalCodecError::Invalid);
        }
        let journal = Self {
            generation,
            operation_id,
            operation_kind,
            workspace_id,
            root_hash,
            root_device,
            root_inode,
            created_at,
            plan_hash,
            targets,
            stage,
            completion,
        };
        journal.validate()?;
        Ok(journal)
    }

    fn validate(&self) -> Result<(), JournalCodecError> {
        if self.generation == 0 || self.targets.is_empty() || self.targets.len() > MAX_TARGETS {
            return Err(JournalCodecError::Invalid);
        }
        let committed = self.stage == JournalStage::Committed;
        if committed != self.completion
            || (committed
                && self
                    .targets
                    .iter()
                    .any(|target| target.step != TargetStep::Complete))
            || (self.stage == JournalStage::Prepared
                && self
                    .targets
                    .iter()
                    .any(|target| target.step != TargetStep::Prepared))
        {
            return Err(JournalCodecError::Invalid);
        }
        let mut paths = HashSet::new();
        paths
            .try_reserve(self.targets.len())
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        for target in &self.targets {
            target.validate(self.operation_kind)?;
            if let IntendedState::Trash { destination, .. } = &target.intended {
                let expected = format!(
                    ".rehow/trash/{}/{}",
                    self.operation_id, target.relative_path
                );
                if destination != &expected {
                    return Err(JournalCodecError::Invalid);
                }
            }
            if !paths.insert(target.relative_path.as_str()) {
                return Err(JournalCodecError::Invalid);
            }
        }
        let mut prior_complete = true;
        for target in &self.targets {
            if !prior_complete && target.step != TargetStep::Prepared {
                return Err(JournalCodecError::Invalid);
            }
            prior_complete = target.step == TargetStep::Complete;
        }
        for (index, target) in self.targets.iter().enumerate() {
            for prior in &self.targets[..index] {
                if parent(&target.relative_path) == parent(&prior.relative_path)
                    && target.temp_name == prior.temp_name
                {
                    return Err(JournalCodecError::Invalid);
                }
            }
        }
        Ok(())
    }

    fn is_next_generation_of(&self, next: &Self) -> bool {
        !self.completion
            && self.generation.checked_add(1) == Some(next.generation)
            && self.operation_id == next.operation_id
            && self.operation_kind == next.operation_kind
            && self.workspace_id == next.workspace_id
            && self.root_hash == next.root_hash
            && self.root_device == next.root_device
            && self.root_inode == next.root_inode
            && self.created_at == next.created_at
            && self.plan_hash == next.plan_hash
            && self.targets.len() == next.targets.len()
            && self
                .targets
                .iter()
                .zip(&next.targets)
                .all(|(current, candidate)| current.same_intent(candidate))
            && match (self.stage, next.stage) {
                (JournalStage::Prepared, JournalStage::Applying)
                | (JournalStage::Prepared | JournalStage::Applying, JournalStage::Conflict)
                | (JournalStage::Applying, JournalStage::Committed) => self.same_target_steps(next),
                (JournalStage::Applying, JournalStage::Applying) => {
                    self.has_one_exact_step_advance(next)
                }
                _ => false,
            }
    }

    fn same_target_steps(&self, other: &Self) -> bool {
        self.targets
            .iter()
            .zip(&other.targets)
            .all(|(current, candidate)| current.step == candidate.step)
    }

    fn has_one_exact_step_advance(&self, other: &Self) -> bool {
        let mut advances = 0usize;
        for (current, candidate) in self.targets.iter().zip(&other.targets) {
            if current.step == candidate.step {
                continue;
            }
            if current.step.next() != Some(candidate.step) {
                return false;
            }
            advances += 1;
        }
        advances == 1
    }
}

impl Target {
    fn same_intent(&self, other: &Self) -> bool {
        self.relative_path == other.relative_path
            && self.expected == other.expected
            && self.intended == other.intended
            && self.temp_name == other.temp_name
    }

    fn encode(&self, writer: &mut Writer) -> Result<(), JournalCodecError> {
        writer.map(5)?;
        writer.key(0)?;
        writer.text(&self.relative_path)?;
        writer.key(1)?;
        self.expected.encode(writer)?;
        writer.key(2)?;
        self.intended.encode(writer)?;
        writer.key(3)?;
        writer.text(&self.temp_name)?;
        writer.key(4)?;
        writer.uint(self.step.code())
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, JournalCodecError> {
        reader.map(5)?;
        reader.key(0)?;
        let relative_path = reader.owned_text(MAX_PATH_BYTES)?;
        reader.key(1)?;
        let expected = ExpectedState::decode(reader)?;
        reader.key(2)?;
        let intended = IntendedState::decode(reader)?;
        reader.key(3)?;
        let temp_name = reader.owned_text(MAX_COMPONENT_BYTES)?;
        reader.key(4)?;
        let step = TargetStep::from_code(reader.uint()?)?;
        Ok(Self {
            relative_path,
            expected,
            intended,
            temp_name,
            step,
        })
    }

    fn validate(&self, operation: OperationKind) -> Result<(), JournalCodecError> {
        if !valid_active_path(&self.relative_path) || !valid_temp(&self.temp_name) {
            return Err(JournalCodecError::Invalid);
        }
        self.expected.validate()?;
        self.intended.validate()?;
        if matches!(self.expected, ExpectedState::Absent)
            && matches!(self.intended, IntendedState::Absent)
        {
            return Err(JournalCodecError::Invalid);
        }
        if let IntendedState::Trash {
            object_hash, mode, ..
        } = &self.intended
        {
            let ExpectedState::Existing {
                object_hash: old_hash,
                mode: old_mode,
                ..
            } = &self.expected
            else {
                return Err(JournalCodecError::Invalid);
            };
            if object_hash != old_hash || mode != old_mode {
                return Err(JournalCodecError::Invalid);
            }
        }
        let valid_operation = match operation {
            OperationKind::Create => {
                matches!(self.expected, ExpectedState::Absent)
                    && matches!(self.intended, IntendedState::Object { .. })
            }
            OperationKind::Update => {
                matches!(self.expected, ExpectedState::Existing { .. })
                    && matches!(self.intended, IntendedState::Object { .. })
            }
            OperationKind::Delete => {
                matches!(self.expected, ExpectedState::Existing { .. })
                    && matches!(
                        self.intended,
                        IntendedState::Absent | IntendedState::Trash { .. }
                    )
            }
            OperationKind::GitMaterialize => {
                matches!(self.expected, ExpectedState::Absent)
                    && matches!(self.intended, IntendedState::Object { mode: 0o600, .. })
            }
            OperationKind::MultiFile | OperationKind::Recovery | OperationKind::Migration => true,
        };
        if valid_operation {
            Ok(())
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
}

impl ExpectedState {
    fn encode(&self, writer: &mut Writer) -> Result<(), JournalCodecError> {
        match self {
            Self::Absent => {
                writer.map(1)?;
                writer.key(0)?;
                writer.uint(0)
            }
            Self::Existing {
                revision,
                object_hash,
                mode,
            } => {
                writer.map(4)?;
                writer.key(0)?;
                writer.uint(1)?;
                writer.key(1)?;
                writer.text(&revision.to_string())?;
                writer.key(2)?;
                writer.bytes(object_hash)?;
                writer.key(3)?;
                writer.uint(u64::from(*mode))
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, JournalCodecError> {
        let length = reader.map_len()?;
        reader.key(0)?;
        match (length, reader.uint()?) {
            (1, 0) => Ok(Self::Absent),
            (4, 1) => {
                reader.key(1)?;
                let revision = reader.protocol_text::<Revision>(71)?;
                reader.key(2)?;
                let object_hash = reader.hash()?;
                reader.key(3)?;
                let mode = reader.mode()?;
                Ok(Self::Existing {
                    revision,
                    object_hash,
                    mode,
                })
            }
            _ => Err(JournalCodecError::Invalid),
        }
    }

    fn validate(&self) -> Result<(), JournalCodecError> {
        match self {
            Self::Absent => Ok(()),
            Self::Existing { mode, .. } if valid_mode(*mode) => Ok(()),
            Self::Existing { .. } => Err(JournalCodecError::Invalid),
        }
    }
}

impl IntendedState {
    fn encode(&self, writer: &mut Writer) -> Result<(), JournalCodecError> {
        match self {
            Self::Absent => {
                writer.map(1)?;
                writer.key(0)?;
                writer.uint(0)
            }
            Self::Object { object_hash, mode } => {
                writer.map(3)?;
                writer.key(0)?;
                writer.uint(1)?;
                writer.key(1)?;
                writer.bytes(object_hash)?;
                writer.key(2)?;
                writer.uint(u64::from(*mode))
            }
            Self::Trash {
                object_hash,
                mode,
                destination,
            } => {
                writer.map(4)?;
                writer.key(0)?;
                writer.uint(2)?;
                writer.key(1)?;
                writer.bytes(object_hash)?;
                writer.key(2)?;
                writer.uint(u64::from(*mode))?;
                writer.key(3)?;
                writer.text(destination)
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, JournalCodecError> {
        let length = reader.map_len()?;
        reader.key(0)?;
        match (length, reader.uint()?) {
            (1, 0) => Ok(Self::Absent),
            (3, 1) => {
                reader.key(1)?;
                let object_hash = reader.hash()?;
                reader.key(2)?;
                let mode = reader.mode()?;
                Ok(Self::Object { object_hash, mode })
            }
            (4, 2) => {
                reader.key(1)?;
                let object_hash = reader.hash()?;
                reader.key(2)?;
                let mode = reader.mode()?;
                reader.key(3)?;
                let destination = reader.owned_text(MAX_PATH_BYTES)?;
                Ok(Self::Trash {
                    object_hash,
                    mode,
                    destination,
                })
            }
            _ => Err(JournalCodecError::Invalid),
        }
    }

    fn validate(&self) -> Result<(), JournalCodecError> {
        match self {
            Self::Absent => Ok(()),
            Self::Object { mode, .. } if valid_mode(*mode) => Ok(()),
            Self::Trash {
                mode, destination, ..
            } if valid_mode(*mode) && valid_trash_path(destination) => Ok(()),
            Self::Object { .. } | Self::Trash { .. } => Err(JournalCodecError::Invalid),
        }
    }
}

macro_rules! coded_enum {
    ($type:ty, $($variant:path => $code:literal),+ $(,)?) => {
        impl $type {
            fn code(self) -> u64 {
                match self { $($variant => $code),+ }
            }
            fn from_code(code: u64) -> Result<Self, JournalCodecError> {
                match code { $($code => Ok($variant)),+, _ => Err(JournalCodecError::Invalid) }
            }
        }
    };
}

coded_enum!(OperationKind,
    OperationKind::Create => 0,
    OperationKind::Update => 1,
    OperationKind::Delete => 2,
    OperationKind::MultiFile => 3,
    OperationKind::Recovery => 4,
    OperationKind::Migration => 5,
    OperationKind::GitMaterialize => 6,
);
coded_enum!(JournalStage,
    JournalStage::Prepared => 0,
    JournalStage::Applying => 1,
    JournalStage::Committed => 2,
    JournalStage::Conflict => 3,
);

coded_enum!(TargetStep,
    TargetStep::Prepared => 0,
    TargetStep::Swapped => 1,
    TargetStep::DirectorySynced => 2,
    TargetStep::Complete => 3,
);
coded_enum!(GitCommitIntentBranchRepresentation,
    GitCommitIntentBranchRepresentation::Unborn => 0,
    GitCommitIntentBranchRepresentation::Loose => 1,
    GitCommitIntentBranchRepresentation::Packed => 2,
);
coded_enum!(GitCommitIntentStage,
    GitCommitIntentStage::Prepared => 0,
    GitCommitIntentStage::IndexPublished => 1,
    GitCommitIntentStage::RefPublished => 2,
    GitCommitIntentStage::Complete => 3,
    GitCommitIntentStage::Conflict => 4,
);

impl TargetStep {
    fn next(self) -> Option<Self> {
        match self {
            Self::Prepared => Some(Self::Swapped),
            Self::Swapped => Some(Self::DirectorySynced),
            Self::DirectorySynced => Some(Self::Complete),
            Self::Complete => None,
        }
    }
}

struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn new() -> Result<Self, JournalCodecError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve(512)
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        Ok(Self { bytes })
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), JournalCodecError> {
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(JournalCodecError::TooLarge)?;
        if next > MAX_JOURNAL_BYTES {
            return Err(JournalCodecError::TooLarge);
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn head(&mut self, major: u8, value: u64) -> Result<(), JournalCodecError> {
        let prefix = major << 5;
        if value < 24 {
            self.append(&[prefix | value as u8])
        } else if u8::try_from(value).is_ok() {
            self.append(&[prefix | 24, value as u8])
        } else if u16::try_from(value).is_ok() {
            self.append(&[prefix | 25])?;
            self.append(&(value as u16).to_be_bytes())
        } else if u32::try_from(value).is_ok() {
            self.append(&[prefix | 26])?;
            self.append(&(value as u32).to_be_bytes())
        } else {
            self.append(&[prefix | 27])?;
            self.append(&value.to_be_bytes())
        }
    }

    fn uint(&mut self, value: u64) -> Result<(), JournalCodecError> {
        self.head(0, value)
    }
    fn key(&mut self, value: u64) -> Result<(), JournalCodecError> {
        self.uint(value)
    }
    fn bytes(&mut self, value: &[u8]) -> Result<(), JournalCodecError> {
        self.head(2, value.len() as u64)?;
        self.append(value)
    }
    fn optional_hash(&mut self, value: Option<&ContentHash>) -> Result<(), JournalCodecError> {
        self.bytes(value.map_or(&[], |hash| hash.as_slice()))
    }
    fn text(&mut self, value: &str) -> Result<(), JournalCodecError> {
        self.head(3, value.len() as u64)?;
        self.append(value.as_bytes())
    }
    fn array(&mut self, length: u64) -> Result<(), JournalCodecError> {
        self.head(4, length)
    }
    fn map(&mut self, length: u64) -> Result<(), JournalCodecError> {
        self.head(5, length)
    }
    fn boolean(&mut self, value: bool) -> Result<(), JournalCodecError> {
        self.append(&[if value { 0xf5 } else { 0xf4 }])
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], JournalCodecError> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(JournalCodecError::Invalid)?;
        let result = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(result)
    }

    fn head(&mut self) -> Result<(u8, u64), JournalCodecError> {
        let initial = self.take(1)?[0];
        let major = initial >> 5;
        let additional = initial & 0x1f;
        if major == 7 && additional >= 24 {
            return Err(JournalCodecError::Invalid);
        }
        let value = match additional {
            value @ 0..=23 => u64::from(value),
            24 => {
                let value = u64::from(self.take(1)?[0]);
                if value < 24 {
                    return Err(JournalCodecError::Invalid);
                }
                value
            }
            25 => {
                let value = u64::from(u16::from_be_bytes(
                    self.take(2)?
                        .try_into()
                        .map_err(|_| JournalCodecError::Invalid)?,
                ));
                if value <= u64::from(u8::MAX) {
                    return Err(JournalCodecError::Invalid);
                }
                value
            }
            26 => {
                let value = u64::from(u32::from_be_bytes(
                    self.take(4)?
                        .try_into()
                        .map_err(|_| JournalCodecError::Invalid)?,
                ));
                if value <= u64::from(u16::MAX) {
                    return Err(JournalCodecError::Invalid);
                }
                value
            }
            27 => {
                let value = u64::from_be_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| JournalCodecError::Invalid)?,
                );
                if value <= u64::from(u32::MAX) {
                    return Err(JournalCodecError::Invalid);
                }
                value
            }
            _ => return Err(JournalCodecError::Invalid),
        };
        Ok((major, value))
    }

    fn expected_head(&mut self, major: u8) -> Result<u64, JournalCodecError> {
        let (actual, value) = self.head()?;
        if actual == major {
            Ok(value)
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn uint(&mut self) -> Result<u64, JournalCodecError> {
        self.expected_head(0)
    }
    fn key(&mut self, expected: u64) -> Result<(), JournalCodecError> {
        if self.uint()? == expected {
            Ok(())
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn map(&mut self, expected: u64) -> Result<(), JournalCodecError> {
        if self.expected_head(5)? == expected {
            Ok(())
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn map_len(&mut self) -> Result<u64, JournalCodecError> {
        self.expected_head(5)
    }
    fn array_len(&mut self, maximum: usize) -> Result<usize, JournalCodecError> {
        let value =
            usize::try_from(self.expected_head(4)?).map_err(|_| JournalCodecError::Invalid)?;
        if value <= maximum {
            Ok(value)
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn borrowed_text(&mut self, maximum: usize) -> Result<&'a str, JournalCodecError> {
        let length =
            usize::try_from(self.expected_head(3)?).map_err(|_| JournalCodecError::Invalid)?;
        if length > maximum {
            return Err(JournalCodecError::Invalid);
        }
        std::str::from_utf8(self.take(length)?).map_err(|_| JournalCodecError::Invalid)
    }
    fn owned_text(&mut self, maximum: usize) -> Result<String, JournalCodecError> {
        let borrowed = self.borrowed_text(maximum)?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(borrowed.len())
            .map_err(|_| JournalCodecError::ResourceLimit)?;
        owned.push_str(borrowed);
        Ok(owned)
    }
    fn protocol_text<T: FromStr>(&mut self, maximum: usize) -> Result<T, JournalCodecError> {
        self.borrowed_text(maximum)?
            .parse()
            .map_err(|_| JournalCodecError::Invalid)
    }
    fn hash(&mut self) -> Result<ContentHash, JournalCodecError> {
        if self.expected_head(2)? != 32 {
            return Err(JournalCodecError::Invalid);
        }
        self.take(32)?
            .try_into()
            .map_err(|_| JournalCodecError::Invalid)
    }
    fn optional_hash(&mut self) -> Result<Option<ContentHash>, JournalCodecError> {
        let length = self.expected_head(2)?;
        match length {
            0 => Ok(None),
            32 => Ok(Some(
                self.take(32)?
                    .try_into()
                    .map_err(|_| JournalCodecError::Invalid)?,
            )),
            _ => Err(JournalCodecError::Invalid),
        }
    }
    fn mode(&mut self) -> Result<u32, JournalCodecError> {
        let mode = u32::try_from(self.uint()?).map_err(|_| JournalCodecError::Invalid)?;
        if valid_mode(mode) {
            Ok(mode)
        } else {
            Err(JournalCodecError::Invalid)
        }
    }
    fn boolean(&mut self) -> Result<bool, JournalCodecError> {
        match self.take(1)?[0] {
            0xf4 => Ok(false),
            0xf5 => Ok(true),
            _ => Err(JournalCodecError::Invalid),
        }
    }
    fn finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn valid_mode(mode: u32) -> bool {
    mode <= 0o777
}

pub(super) fn valid_active_path(path: &str) -> bool {
    if path.len() > MAX_PATH_BYTES {
        return false;
    }
    if path == "rehow.yaml" || path == ".gitignore" {
        return true;
    }
    let parts = path.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["categories", directory, "category.yaml"] => super::valid_category_directory(directory),
        ["categories", directory, guide] => {
            super::valid_category_directory(directory) && super::valid_guide_filename(guide)
        }
        ["attachments", guide_id, filename] => {
            GuideId::from_str(guide_id).is_ok() && valid_attachment_filename(filename)
        }
        _ => false,
    }
}

fn valid_attachment_filename(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_COMPONENT_BYTES
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', '\0'])
        && value.nfc().eq(value.chars())
        && !value.chars().any(|character| {
            matches!(character as u32, 0x00..=0x1f|0x7f..=0x9f|0x061c|0x200e|0x200f|0x2028..=0x202e|0x2066..=0x2069|0xfeff|0xfdd0..=0xfdef)
                || matches!(character as u32 & 0xffff, 0xfffe | 0xffff)
        })
}

pub(super) fn valid_trash_path(path: &str) -> bool {
    let Some(remainder) = path.strip_prefix(".rehow/trash/") else {
        return false;
    };
    let Some((operation, active_path)) = remainder.split_once('/') else {
        return false;
    };
    OperationId::from_str(operation).is_ok() && valid_active_path(active_path)
}

fn valid_temp(value: &str) -> bool {
    value.len() <= MAX_COMPONENT_BYTES
        && value.strip_prefix(".rehow-tmp-").is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_git_commit_intent() -> GitCommitIntent {
        GitCommitIntent {
            generation: 1,
            operation_id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".parse().unwrap(),
            workspace_id: "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            root_hash: [0x11; 32],
            root_device: 8,
            root_inode: 42,
            git_device: 8,
            git_inode: 84,
            branch_relative: "team/rehow".to_owned(),
            representation: GitCommitIntentBranchRepresentation::Loose,
            head_hash: [0x22; 32],
            config_hash: [0x33; 32],
            before_index_hash: Some([0x44; 32]),
            before_index_mode: 0o600,
            candidate_index_hash: [0x55; 32],
            candidate_index_mode: 0o600,
            packed_refs_hash: Some([0x66; 32]),
            before_ref_hash: Some([0x77; 32]),
            before_ref_mode: 0o644,
            candidate_ref_hash: [0x88; 32],
            candidate_ref_mode: 0o644,
            plan_hash: [0x99; 32],
            stage: GitCommitIntentStage::Prepared,
            staging_name: "ab".repeat(32),
            staging_device: 8,
            staging_inode: 126,
        }
    }

    #[test]
    fn git_commit_intent_round_trips_rejects_truncation_and_redacts_content() {
        let intent = sample_git_commit_intent();
        let encoded = intent.encode().unwrap();
        assert_eq!(GitCommitIntent::decode(&encoded).unwrap(), intent);
        for length in 0..encoded.len() {
            assert!(
                GitCommitIntent::decode(&encoded[..length]).is_err(),
                "prefix {length}"
            );
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            GitCommitIntent::decode(&trailing),
            Err(JournalCodecError::Invalid)
        );
        let debug = format!("{intent:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(&"99".repeat(32)));
    }

    #[test]
    fn git_commit_intent_enforces_representations_modes_and_stage_order() {
        let mut intent = sample_git_commit_intent();
        intent.branch_relative = "../escape".to_owned();
        assert_eq!(intent.encode(), Err(JournalCodecError::Invalid));

        intent = sample_git_commit_intent();
        intent.candidate_ref_mode = 0o666;
        assert_eq!(intent.encode(), Err(JournalCodecError::Invalid));

        intent = sample_git_commit_intent();
        intent.representation = GitCommitIntentBranchRepresentation::Unborn;
        assert_eq!(intent.encode(), Err(JournalCodecError::Invalid));
        intent.before_index_hash = None;
        intent.before_ref_hash = None;
        assert_eq!(
            GitCommitIntent::decode(&intent.encode().unwrap()).unwrap(),
            intent
        );

        intent = sample_git_commit_intent();
        intent.representation = GitCommitIntentBranchRepresentation::Packed;
        intent.before_ref_hash = None;
        intent.packed_refs_hash = None;
        assert_eq!(intent.encode(), Err(JournalCodecError::Invalid));

        let prepared = sample_git_commit_intent();
        assert_eq!(
            prepared.next_stage(GitCommitIntentStage::Complete),
            Err(JournalCodecError::Invalid)
        );
        let index = prepared
            .next_stage(GitCommitIntentStage::IndexPublished)
            .unwrap();
        assert!(prepared.is_next_generation_of(&index));
        let reference = index
            .next_stage(GitCommitIntentStage::RefPublished)
            .unwrap();
        let complete = reference
            .next_stage(GitCommitIntentStage::Complete)
            .unwrap();
        let reference_conflict = reference
            .next_stage(GitCommitIntentStage::Conflict)
            .unwrap();
        assert_eq!(reference_conflict.stage, GitCommitIntentStage::Conflict);
        assert_eq!(complete.generation, 4);
        assert_eq!(complete.stage, GitCommitIntentStage::Complete);
        assert_eq!(
            complete.next_stage(GitCommitIntentStage::Conflict),
            Err(JournalCodecError::Invalid)
        );

        let mut changed = index.clone();
        changed.plan_hash = [0xaa; 32];
        assert!(!prepared.is_next_generation_of(&changed));
    }

    fn sample() -> Journal {
        Journal {
            generation: 1,
            operation_id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".parse().unwrap(),
            operation_kind: OperationKind::Create,
            workspace_id: "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            root_hash: [0x11; 32],
            root_device: 8,
            root_inode: 42,
            created_at: "2026-09-18T00:00:00Z".parse().unwrap(),
            plan_hash: [0x22; 32],
            targets: vec![Target {
                relative_path: "categories/linux/guide.md".to_owned(),
                expected: ExpectedState::Absent,
                intended: IntendedState::Object {
                    object_hash: [0x33; 32],
                    mode: 0o600,
                },
                temp_name: ".rehow-tmp-abcdef012345".to_owned(),
                step: TargetStep::Prepared,
            }],
            stage: JournalStage::Prepared,
            completion: false,
        }
    }

    #[test]
    fn canonical_vector_is_frozen_and_round_trips() {
        let journal = sample();
        let encoded = journal.encode().unwrap();
        let hex = encoded
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "ad0001010102782430313939613164372d376230642d376438652d613338642d663933366335373335653130030004782430313939613139662d346431352d376361372d613634352d6537353331316361613731300558201111111111111111111111111111111111111111111111111111111111111111060807182a0874323032362d30392d31385430303a30303a30305a09582022222222222222222222222222222222222222222222222222222222222222220a81a500781963617465676f726965732f6c696e75782f67756964652e6d6401a1000002a3000101582033333333333333333333333333333333333333333333333333333333333333330219018003772e7265686f772d746d702d61626364656630313233343504000b000cf4"
        );
        assert!(Journal::decode(&encoded).unwrap() == journal);
    }

    #[test]
    fn every_truncated_prefix_and_trailing_byte_is_rejected() {
        let encoded = sample().encode().unwrap();
        for length in 0..encoded.len() {
            assert!(
                Journal::decode(&encoded[..length]).is_err(),
                "prefix {length}"
            );
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(matches!(
            Journal::decode(&trailing),
            Err(JournalCodecError::Invalid)
        ));
    }

    #[test]
    fn noncanonical_and_unsupported_cbor_is_rejected() {
        let encoded = sample().encode().unwrap();
        for prefix in [
            vec![0xb8, 13],
            vec![0xbf],
            vec![0xd8, 0],
            vec![0xfa, 0, 0, 0, 0],
        ] {
            let mut hostile = prefix;
            hostile.extend_from_slice(&encoded[1..]);
            assert!(matches!(
                Journal::decode(&hostile),
                Err(JournalCodecError::Invalid)
            ));
        }
        let mut reordered = encoded;
        reordered[1] = 1;
        assert!(matches!(
            Journal::decode(&reordered),
            Err(JournalCodecError::Invalid)
        ));
        let mut non_shortest_key = vec![0xad, 0x18, 0x00];
        non_shortest_key.extend_from_slice(&sample().encode().unwrap()[2..]);
        assert!(matches!(
            Journal::decode(&non_shortest_key),
            Err(JournalCodecError::Invalid)
        ));
        let mut extra_key_claim = sample().encode().unwrap();
        extra_key_claim[0] = 0xae;
        assert!(matches!(
            Journal::decode(&extra_key_claim),
            Err(JournalCodecError::Invalid)
        ));
        let mut bad_operation_id = sample().encode().unwrap();
        let offset = bad_operation_id
            .windows(4)
            .position(|window| window == b"0199")
            .unwrap();
        bad_operation_id[offset] = b'A';
        assert!(matches!(
            Journal::decode(&bad_operation_id),
            Err(JournalCodecError::Invalid)
        ));
    }

    #[test]
    fn closed_state_and_path_invariants_fail_before_encoding() {
        let mut journal = sample();
        journal.generation = 0;
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.targets[0].relative_path = "../escape".to_owned();
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.targets[0].temp_name = "target.md".to_owned();
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.completion = true;
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        if let IntendedState::Object { mode, .. } = &mut journal.targets[0].intended {
            *mode = 0o1_600;
        }
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.operation_kind = OperationKind::Delete;
        journal.targets[0].expected = ExpectedState::Existing {
            revision: format!("rev:v1:{}", "44".repeat(32)).parse().unwrap(),
            object_hash: [0x44; 32],
            mode: 0o600,
        };
        journal.targets[0].intended = IntendedState::Trash {
            object_hash: [0x55; 32],
            mode: 0o600,
            destination:
                ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/categories/linux/guide.md"
                    .to_owned(),
        };
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.operation_kind = OperationKind::Delete;
        journal.targets[0].expected = ExpectedState::Existing {
            revision: format!("rev:v1:{}", "44".repeat(32)).parse().unwrap(),
            object_hash: [0x44; 32],
            mode: 0o600,
        };
        journal.targets[0].intended = IntendedState::Trash {
            object_hash: [0x44; 32],
            mode: 0o600,
            destination:
                ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e11/categories/linux/guide.md"
                    .to_owned(),
        };
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal.targets[0].intended = IntendedState::Trash {
            object_hash: [0x44; 32],
            mode: 0o600,
            destination: ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/rehow.yaml".to_owned(),
        };
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));

        let mut journal = sample();
        journal.operation_kind = OperationKind::GitMaterialize;
        journal.targets[0].expected = ExpectedState::Existing {
            revision: format!("rev:v1:{}", "44".repeat(32)).parse().unwrap(),
            object_hash: [0x44; 32],
            mode: 0o600,
        };
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        let mut journal = sample();
        journal.operation_kind = OperationKind::GitMaterialize;
        if let IntendedState::Object { mode, .. } = &mut journal.targets[0].intended {
            *mode = 0o644;
        }
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        let mut journal = sample();
        journal.operation_kind = OperationKind::GitMaterialize;
        journal.targets[0].intended = IntendedState::Absent;
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
    }

    #[test]
    fn every_closed_discriminant_round_trips() {
        for operation in [
            OperationKind::Create,
            OperationKind::Update,
            OperationKind::Delete,
            OperationKind::MultiFile,
            OperationKind::Recovery,
            OperationKind::Migration,
            OperationKind::GitMaterialize,
        ] {
            assert_eq!(OperationKind::from_code(operation.code()), Ok(operation));
        }
        for stage in [
            JournalStage::Prepared,
            JournalStage::Applying,
            JournalStage::Committed,
            JournalStage::Conflict,
        ] {
            assert_eq!(JournalStage::from_code(stage.code()), Ok(stage));
        }
        for step in [
            TargetStep::Prepared,
            TargetStep::Swapped,
            TargetStep::DirectorySynced,
            TargetStep::Complete,
        ] {
            assert_eq!(TargetStep::from_code(step.code()), Ok(step));
        }
        assert_eq!(OperationKind::from_code(7), Err(JournalCodecError::Invalid));
        assert_eq!(JournalStage::from_code(4), Err(JournalCodecError::Invalid));
        assert_eq!(TargetStep::from_code(4), Err(JournalCodecError::Invalid));
    }

    #[test]
    fn bounds_duplicates_and_managed_path_grammar_are_enforced() {
        assert_eq!(
            Journal::decode(&vec![0; MAX_JOURNAL_BYTES + 1]).err(),
            Some(JournalCodecError::TooLarge)
        );
        let mut journal = sample();
        journal.targets = vec![journal.targets[0].clone(); MAX_TARGETS + 1];
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));
        journal = sample();
        journal.targets.push(journal.targets[0].clone());
        assert_eq!(journal.encode(), Err(JournalCodecError::Invalid));

        for path in [
            "/rehow.yaml",
            "categories/Linux/guide.md",
            "categories/linux/nested/guide.md",
            "attachments/not-an-id/file.png",
            "attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/../file.png",
            ".rehow/trash/file.md",
        ] {
            let mut invalid = sample();
            invalid.targets[0].relative_path = path.to_owned();
            assert_eq!(invalid.encode(), Err(JournalCodecError::Invalid), "{path}");
        }
        assert!(valid_active_path("rehow.yaml"));
        assert!(valid_active_path(".gitignore"));
        assert!(valid_active_path("categories/linux/category.yaml"));
        assert!(valid_active_path("categories/linux/guide.md"));
        assert!(valid_active_path(
            "attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/screenshot.png"
        ));

        let mut out_of_order = sample();
        let mut later = out_of_order.targets[0].clone();
        later.relative_path = "rehow.yaml".to_owned();
        later.step = TargetStep::Swapped;
        out_of_order.targets.push(later);
        out_of_order.stage = JournalStage::Applying;
        assert_eq!(out_of_order.encode(), Err(JournalCodecError::Invalid));
    }

    #[test]
    fn applying_conflict_and_committed_stage_invariants_round_trip() {
        for stage in [JournalStage::Applying, JournalStage::Conflict] {
            let mut journal = sample();
            journal.stage = stage;
            journal.targets[0].step = TargetStep::Swapped;
            let encoded = journal.encode().unwrap();
            assert!(Journal::decode(&encoded).unwrap() == journal);
        }
        let mut invalid = sample();
        invalid.stage = JournalStage::Committed;
        invalid.completion = true;
        assert_eq!(invalid.encode(), Err(JournalCodecError::Invalid));
    }

    #[test]
    fn update_delete_trash_and_committed_states_round_trip() {
        let old = ExpectedState::Existing {
            revision: format!("rev:v1:{}", "44".repeat(32)).parse().unwrap(),
            object_hash: [0x44; 32],
            mode: 0o640,
        };
        let cases = [
            (
                OperationKind::Update,
                IntendedState::Object {
                    object_hash: [0x55; 32],
                    mode: 0o640,
                },
            ),
            (OperationKind::Delete, IntendedState::Absent),
            (
                OperationKind::Delete,
                IntendedState::Trash {
                    object_hash: [0x44; 32],
                    mode: 0o640,
                    destination: ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/categories/linux/guide.md".to_owned(),
                },
            ),
        ];
        for (operation, intended) in cases {
            let mut journal = sample();
            journal.operation_kind = operation;
            journal.targets[0].expected = old.clone();
            journal.targets[0].intended = intended;
            journal.targets[0].step = TargetStep::Complete;
            journal.stage = JournalStage::Committed;
            journal.completion = true;
            let encoded = journal.encode().unwrap();
            assert!(Journal::decode(&encoded).unwrap() == journal);
        }
    }
}

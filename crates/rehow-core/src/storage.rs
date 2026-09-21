//! Descriptor-confined Workspace inspection and private mutation primitives.
//!
//! The retained root descriptor prevents later reads or writes from being
//! redirected by renaming the path that the user selected. Mutation remains a
//! crate-private recovery boundary and is not exposed to UI or MCP adapters.

pub(crate) mod recovery;
mod watch;

pub(crate) use recovery::{
    AttachmentImportPreview, AttachmentImportStorageOutcome, AttachmentRemovalPreview,
    AttachmentRemovalStorageOutcome, AttachmentSourceState, CancellableDocumentUpdateError,
    CategoryCreateStorageError, CategoryCreateStorageOutcome, CategoryRemoveStorageError,
    CategoryRemoveStorageOutcome, DocumentCreateError, DocumentCreateOutcome, DocumentMoveOutcome,
    DocumentRemoveError, DocumentRemoveOutcome, DocumentUpdateError, DocumentUpdateOutcome,
    GitCloneMaterializationSource, GitCloneMaterializationStorageError,
    GitCloneMaterializationStorageOutcome, GitClonePackSource, GitCloneStaging,
    GitCloneStagingError, GitSyncMaterializationCurrent, GitSyncMaterializationSource,
    GitSyncMaterializationStorageError, GitSyncMaterializationStorageOutcome,
    GitSyncPreparedMaterialization, MultiDocumentUpdateError, MultiDocumentUpdateOutcome,
    PreparedDocument, WorkspaceInitializeError, WorkspaceInitializeOutcome,
    create_git_clone_staging,
};
pub use watch::{WorkspaceWatchError, WorkspaceWatchSignal, WorkspaceWatcher};

#[cfg(test)]
pub(crate) fn set_attachment_changed_fault() {
    linux::set_attachment_changed_fault();
}

#[cfg(test)]
pub(crate) fn set_attachment_import_source_changed_fault() {
    recovery::set_attachment_source_change_fault();
}

#[cfg(test)]
pub(crate) use linux::{
    GitCloneFinalizeFault, GitCloneScaffoldFault, GitInitFault, set_git_clone_finalize_fault,
    set_git_clone_scaffold_fault, set_git_init_fault,
};

#[cfg(test)]
pub(crate) use recovery::{
    WorkspaceFault, set_git_sync_fail_after_directory_preparation, set_workspace_fault,
};

#[cfg(not(target_os = "linux"))]
compile_error!("Rehow's confined Workspace storage currently requires Linux");

use crate::{
    domain::{Category, Guide, GuideStatus, WorkspaceManifest},
    protocol::{Revision, Snapshot},
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::{MetadataExt, PermissionsExt},
    },
    path::Path,
};

const GIT_CONFIG_LIMIT: usize = 1024 * 1024;
const GIT_COMMIT_INDEX_LIMIT: usize = 64 * 1024 * 1024;
const GIT_COMMIT_REF_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct GitCommitFileSnapshot {
    pub(crate) bytes: Vec<u8>,
    metadata: GitConfigMetadata,
}

impl fmt::Debug for GitCommitFileSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitFileSnapshot")
            .field("bytes", &format_args!("<redacted:{}>", self.bytes.len()))
            .field("metadata", &self.metadata)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitCommitRepositorySnapshot {
    pub(crate) head: GitCommitFileSnapshot,
    pub(crate) config: GitCommitFileSnapshot,
    pub(crate) index: Option<GitCommitFileSnapshot>,
    pub(crate) packed_refs: Option<GitCommitFileSnapshot>,
    pub(crate) shallow: Option<GitCommitFileSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GitCommitBranchSnapshot {
    pub(crate) target: Option<GitCommitFileSnapshot>,
}

#[derive(Eq, PartialEq)]
pub(crate) struct GitCommitManagedFileSnapshot {
    pub(crate) bytes: Vec<u8>,
    pub(crate) revision: Revision,
    pub(crate) mode: u32,
}

impl fmt::Debug for GitCommitManagedFileSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitCommitManagedFileSnapshot")
            .field("bytes", &format_args!("<redacted:{}>", self.bytes.len()))
            .field("revision", &"<redacted>")
            .field("mode", &format_args!("{:o}", self.mode))
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitSnapshotError {
    OperationInProgress,
    Conflict,
    Unsafe,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitPackInstallError {
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitMetadataStorageError {
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitMetadataStage {
    IndexPublished,
    RefPublished,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitMetadataFault {
    IndexLockPublished,
    IndexExchanged,
    IndexPublished,
    RefLockPublished,
    RefExchanged,
    RefPublished,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitCommitMetadataRace {
    ReplaceRefBeforePublication,
    ReplaceRefAfterValidation,
}

#[cfg(test)]
thread_local! {
    static GIT_COMMIT_METADATA_FAULT: std::cell::Cell<Option<GitCommitMetadataFault>> = const { std::cell::Cell::new(None) };
    static GIT_COMMIT_METADATA_RACE: std::cell::Cell<Option<GitCommitMetadataRace>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_git_commit_metadata_fault(fault: GitCommitMetadataFault) {
    GIT_COMMIT_METADATA_FAULT.set(Some(fault));
}

#[cfg(test)]
pub(crate) fn set_git_commit_metadata_race(race: GitCommitMetadataRace) {
    GIT_COMMIT_METADATA_RACE.set(Some(race));
}

#[cfg(test)]
fn git_commit_metadata_fail(
    point: GitCommitMetadataFault,
) -> Result<(), GitCommitMetadataStorageError> {
    if GIT_COMMIT_METADATA_FAULT.get() == Some(point) {
        GIT_COMMIT_METADATA_FAULT.set(None);
        Err(GitCommitMetadataStorageError::Unsupported)
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn git_commit_metadata_race(
    point: GitCommitMetadataRace,
    directory: &OwnedFd,
    name: &str,
) -> Result<(), GitCommitMetadataStorageError> {
    if GIT_COMMIT_METADATA_RACE.get() != Some(point) {
        return Ok(());
    }
    GIT_COMMIT_METADATA_RACE.set(None);
    let root = std::path::PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
    let race = root.join("rehow-race-ref");
    std::fs::write(&race, b"1111111111111111111111111111111111111111\n")
        .map_err(|_| GitCommitMetadataStorageError::Io)?;
    std::fs::set_permissions(&race, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| GitCommitMetadataStorageError::Io)?;
    std::fs::rename(race, root.join(name)).map_err(|_| GitCommitMetadataStorageError::Io)
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct GitConfigFiles {
    pub(crate) config: Vec<u8>,
    pub(crate) config_mode: u32,
    config_identity: (u64, u64),
    pub(crate) lock: Option<Vec<u8>>,
    pub(crate) lock_mode: Option<u32>,
    lock_identity: Option<(u64, u64)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitConfigStorageError {
    Conflict,
    Unsafe,
    Unsupported,
    TooLarge,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GitConfigMetadata {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
    mode: u32,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitConfigFault {
    LockPublished,
    Exchanged,
    Quarantined,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitConfigRace {
    ReplaceConfigBeforeExchange,
    ReplaceLockBeforeQuarantine,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitSyncDirectoryRace {
    ReplaceCategory,
    ReplaceAttachment,
}

#[cfg(test)]
thread_local! {
    static GIT_CONFIG_FAULT: std::cell::Cell<Option<GitConfigFault>> = const { std::cell::Cell::new(None) };
    static GIT_CONFIG_RACE: std::cell::Cell<Option<GitConfigRace>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
thread_local! {
    static GIT_SYNC_DIRECTORY_RACE: std::cell::Cell<Option<GitSyncDirectoryRace>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_git_sync_directory_race(race: GitSyncDirectoryRace) {
    GIT_SYNC_DIRECTORY_RACE.set(Some(race));
}

#[cfg(test)]
fn git_sync_directory_race(
    point: GitSyncDirectoryRace,
    parent: &OwnedFd,
    name: &str,
) -> Result<(), linux::MutationIoError> {
    if GIT_SYNC_DIRECTORY_RACE.get() != Some(point) {
        return Ok(());
    }
    GIT_SYNC_DIRECTORY_RACE.set(None);
    let name = std::ffi::CString::new(name).map_err(|_| linux::MutationIoError::Unsafe)?;
    let backup = c".rehow-race-original";
    // SAFETY: the retained parent and NUL-terminated names are valid.
    if unsafe {
        libc::renameat(
            parent.as_raw_fd(),
            name.as_ptr(),
            parent.as_raw_fd(),
            backup.as_ptr(),
        )
    } != 0
    {
        return Err(linux::MutationIoError::Io);
    }
    // SAFETY: the retained parent and validated component are valid.
    if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
        return Err(linux::MutationIoError::Io);
    }
    let replacement = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if replacement < 0 {
        return Err(linux::MutationIoError::Io);
    }
    // SAFETY: successful `openat` returned a newly owned descriptor.
    let replacement = unsafe { OwnedFd::from_raw_fd(replacement) };
    let marker = unsafe {
        libc::openat(
            replacement.as_raw_fd(),
            c"replacement-marker".as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    };
    if marker < 0 {
        return Err(linux::MutationIoError::Io);
    }
    // SAFETY: successful `openat` returned a newly owned descriptor.
    let mut marker = unsafe { File::from_raw_fd(marker) };
    marker
        .write_all(b"preserve")
        .map_err(|_| linux::MutationIoError::Io)?;
    marker
        .sync_all()
        .map_err(|_| linux::MutationIoError::Unsupported)?;
    // SAFETY: both retained directory descriptors are valid.
    if unsafe { libc::fsync(replacement.as_raw_fd()) } != 0
        || unsafe { libc::fsync(parent.as_raw_fd()) } != 0
    {
        return Err(linux::MutationIoError::Unsupported);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn set_git_config_fault(fault: GitConfigFault) {
    GIT_CONFIG_FAULT.set(Some(fault));
}

#[cfg(test)]
pub(crate) fn set_git_config_race(race: GitConfigRace) {
    GIT_CONFIG_RACE.set(Some(race));
}

#[cfg(test)]
fn git_config_fail(point: GitConfigFault) -> Result<(), GitConfigStorageError> {
    if GIT_CONFIG_FAULT.get() == Some(point) {
        GIT_CONFIG_FAULT.set(None);
        Err(GitConfigStorageError::Unsupported)
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn git_config_race(point: GitConfigRace, directory: &OwnedFd) -> Result<(), GitConfigStorageError> {
    use std::os::unix::fs::PermissionsExt as _;
    if GIT_CONFIG_RACE.get() != Some(point) {
        return Ok(());
    }
    GIT_CONFIG_RACE.set(None);
    let root = std::path::PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
    match point {
        GitConfigRace::ReplaceConfigBeforeExchange => {
            std::fs::write(
                root.join("race-config"),
                b"[external]\n\tvalue = before-exchange\n",
            )
            .map_err(|_| GitConfigStorageError::Io)?;
            std::fs::set_permissions(
                root.join("race-config"),
                std::fs::Permissions::from_mode(0o600),
            )
            .map_err(|_| GitConfigStorageError::Io)?;
            std::fs::rename(root.join("race-config"), root.join("config"))
                .map_err(|_| GitConfigStorageError::Io)?;
        }
        GitConfigRace::ReplaceLockBeforeQuarantine => {
            std::fs::rename(root.join("config.lock"), root.join("external-saved-lock"))
                .map_err(|_| GitConfigStorageError::Io)?;
            std::fs::write(
                root.join("config.lock"),
                b"[external]\n\tvalue = before-quarantine\n",
            )
            .map_err(|_| GitConfigStorageError::Io)?;
            std::fs::set_permissions(
                root.join("config.lock"),
                std::fs::Permissions::from_mode(0o600),
            )
            .map_err(|_| GitConfigStorageError::Io)?;
        }
    }
    Ok(())
}

pub(crate) struct GitRepositoryCapability {
    _work_tree: OwnedFd,
    _git_dir: OwnedFd,
    _objects: OwnedFd,
    _config: OwnedFd,
    _refs: OwnedFd,
    _head: OwnedFd,
    _index: Option<OwnedFd>,
    _packed_refs: Option<OwnedFd>,
    _shallow: Option<OwnedFd>,
    work_tree_path: String,
    objects_path: String,
    config_path: String,
    refs_path: String,
    head_path: String,
    index_path: Option<String>,
    packed_refs_path: Option<String>,
    shallow_path: Option<String>,
}

impl GitRepositoryCapability {
    pub(crate) fn work_tree_path(&self) -> &str {
        &self.work_tree_path
    }

    pub(crate) fn objects_path(&self) -> &str {
        &self.objects_path
    }

    pub(crate) fn config_path(&self) -> &str {
        &self.config_path
    }

    pub(crate) fn refs_path(&self) -> &str {
        &self.refs_path
    }
    pub(crate) fn head_path(&self) -> &str {
        &self.head_path
    }
    pub(crate) fn index_path(&self) -> Option<&str> {
        self.index_path.as_deref()
    }
    pub(crate) fn packed_refs_path(&self) -> Option<&str> {
        self.packed_refs_path.as_deref()
    }
    pub(crate) fn shallow_path(&self) -> Option<&str> {
        self.shallow_path.as_deref()
    }

    pub(crate) fn read_retained_shallow_snapshot(
        &self,
    ) -> Result<Option<GitCommitFileSnapshot>, GitCommitSnapshotError> {
        self._shallow
            .as_ref()
            .map(|descriptor| {
                read_git_descriptor(descriptor, GIT_COMMIT_REF_LIMIT)
                    .map(|(bytes, metadata)| GitCommitFileSnapshot { bytes, metadata })
            })
            .transpose()
    }

    pub(crate) fn git_directory_identity(&self) -> Result<(u64, u64), GitCommitSnapshotError> {
        let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: retained descriptor and output storage are valid.
        if unsafe { libc::fstat(self._git_dir.as_raw_fd(), status.as_mut_ptr()) } != 0 {
            return Err(GitCommitSnapshotError::Io);
        }
        // SAFETY: successful fstat initialized the structure.
        let status = unsafe { status.assume_init() };
        if status.st_uid != unsafe { libc::geteuid() }
            || status.st_mode & libc::S_IFMT != libc::S_IFDIR
            || status.st_mode & 0o022 != 0
        {
            return Err(GitCommitSnapshotError::Unsafe);
        }
        Ok((status.st_dev, status.st_ino))
    }

    pub(crate) fn install_commit_pack_sources(
        &self,
        sources: &[GitClonePackSource],
        files: &mut [File],
        object_id_hex_len: usize,
    ) -> Result<(), GitCommitPackInstallError> {
        if sources.len() != 2
            || files.len() != sources.len()
            || !matches!(object_id_hex_len, 40 | 64)
        {
            return Err(GitCommitPackInstallError::Unsafe);
        }
        validate_git_directory_for_mutation(&self._git_dir).map_err(map_git_pack_install_config)?;
        let pack =
            open_git_directory(&self._objects, "pack").map_err(map_git_pack_install_snapshot)?;
        let mut stem: Option<&str> = None;
        let mut index_position = None;
        let mut pack_position = None;
        for (position, source) in sources.iter().enumerate() {
            let Some((candidate_stem, suffix)) = source.name.rsplit_once('.') else {
                return Err(GitCommitPackInstallError::Unsafe);
            };
            if candidate_stem.len() != 5 + object_id_hex_len
                || !candidate_stem.starts_with("pack-")
                || !candidate_stem[5..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || stem.is_some_and(|value| value != candidate_stem)
            {
                return Err(GitCommitPackInstallError::Unsafe);
            }
            stem = Some(candidate_stem);
            match suffix {
                "idx" if index_position.is_none() => index_position = Some(position),
                "pack" if pack_position.is_none() => pack_position = Some(position),
                _ => return Err(GitCommitPackInstallError::Unsafe),
            }
        }
        let (Some(pack_position), Some(index_position)) = (pack_position, index_position) else {
            return Err(GitCommitPackInstallError::Unsafe);
        };
        install_git_commit_pack_file(&pack, &sources[pack_position], &mut files[pack_position])?;
        install_git_commit_pack_file(&pack, &sources[index_position], &mut files[index_position])?;
        // SAFETY: the retained objects directory is valid and owns the pack child.
        if unsafe { libc::fsync(self._objects.as_raw_fd()) } != 0 {
            return Err(GitCommitPackInstallError::Unsupported);
        }
        Ok(())
    }

    pub(crate) fn publish_commit_metadata<F>(
        &self,
        expected_repository: &GitCommitRepositorySnapshot,
        expected_branch: &GitCommitBranchSnapshot,
        branch_relative: &str,
        candidate_index: &[u8],
        candidate_ref: &[u8],
        mut persist_stage: F,
    ) -> Result<(), GitCommitMetadataStorageError>
    where
        F: FnMut(GitCommitMetadataStage) -> Result<(), GitCommitMetadataStorageError>,
    {
        if candidate_index.is_empty()
            || candidate_index.len() > GIT_COMMIT_INDEX_LIMIT
            || candidate_ref.is_empty()
            || candidate_ref.len() > GIT_COMMIT_REF_LIMIT
        {
            return Err(GitCommitMetadataStorageError::TooLarge);
        }
        let current_repository = self
            .read_commit_repository_snapshot()
            .map_err(map_git_commit_metadata_snapshot)?;
        let current_branch = self
            .read_commit_branch_snapshot(branch_relative)
            .map_err(map_git_commit_metadata_snapshot)?;
        if &current_repository != expected_repository || &current_branch != expected_branch {
            return Err(GitCommitMetadataStorageError::Conflict);
        }
        let index_mode = expected_repository
            .index
            .as_ref()
            .map_or(0o600, GitCommitFileSnapshot::mode);
        publish_git_commit_file(
            &self._git_dir,
            &self._git_dir,
            "index",
            expected_repository.index.as_ref(),
            candidate_index,
            index_mode,
            GIT_COMMIT_INDEX_LIMIT,
            "index",
        )?;

        let after_index = self
            .read_commit_repository_snapshot()
            .map_err(map_git_commit_metadata_snapshot)?;
        let after_index_branch = self
            .read_commit_branch_snapshot(branch_relative)
            .map_err(map_git_commit_metadata_snapshot)?;
        let candidate_index_snapshot = after_index
            .index
            .as_ref()
            .filter(|snapshot| snapshot.bytes == candidate_index && snapshot.mode() == index_mode)
            .cloned()
            .ok_or(GitCommitMetadataStorageError::Conflict)?;
        if after_index.head != expected_repository.head
            || after_index.config != expected_repository.config
            || after_index.packed_refs != expected_repository.packed_refs
            || &after_index_branch != expected_branch
        {
            restore_git_commit_index(
                &self._git_dir,
                &candidate_index_snapshot,
                expected_repository.index.as_ref(),
            )?;
            return Err(GitCommitMetadataStorageError::Conflict);
        }
        if let Err(error) = persist_stage(GitCommitMetadataStage::IndexPublished) {
            restore_git_commit_index(
                &self._git_dir,
                &candidate_index_snapshot,
                expected_repository.index.as_ref(),
            )?;
            return Err(error);
        }
        #[cfg(test)]
        git_commit_metadata_fail(GitCommitMetadataFault::IndexPublished)?;

        let branch_directory = open_git_branch_parent(&self._refs, branch_relative)
            .map_err(map_git_commit_metadata_snapshot)?;
        let branch_name = branch_relative
            .rsplit('/')
            .next()
            .ok_or(GitCommitMetadataStorageError::Unsafe)?;
        let ref_mode = expected_branch
            .target
            .as_ref()
            .map_or(0o600, GitCommitFileSnapshot::mode);
        #[cfg(test)]
        git_commit_metadata_race(
            GitCommitMetadataRace::ReplaceRefBeforePublication,
            &branch_directory,
            branch_name,
        )?;
        if let Err(error) = publish_git_commit_file(
            &branch_directory,
            &self._git_dir,
            branch_name,
            expected_branch.target.as_ref(),
            candidate_ref,
            ref_mode,
            GIT_COMMIT_REF_LIMIT,
            "ref",
        ) {
            if error == GitCommitMetadataStorageError::Conflict {
                let published_ref =
                    read_git_entry(&branch_directory, branch_name, GIT_COMMIT_REF_LIMIT)
                        .map_err(map_git_commit_metadata_snapshot)?
                        .is_some_and(|(bytes, metadata)| {
                            bytes == candidate_ref && metadata.mode == ref_mode
                        });
                if !published_ref {
                    restore_git_commit_index(
                        &self._git_dir,
                        &candidate_index_snapshot,
                        expected_repository.index.as_ref(),
                    )?;
                }
            }
            return Err(error);
        }
        persist_stage(GitCommitMetadataStage::RefPublished)?;
        #[cfg(test)]
        git_commit_metadata_fail(GitCommitMetadataFault::RefPublished)?;

        let final_repository = self
            .read_commit_repository_snapshot()
            .map_err(map_git_commit_metadata_snapshot)?;
        let final_branch = self
            .read_commit_branch_snapshot(branch_relative)
            .map_err(map_git_commit_metadata_snapshot)?;
        if final_repository.head != expected_repository.head
            || final_repository.config != expected_repository.config
            || final_repository.packed_refs != expected_repository.packed_refs
            || final_repository.index.as_ref().is_none_or(|snapshot| {
                snapshot.bytes != candidate_index || snapshot.mode() != index_mode
            })
            || final_branch.target.as_ref().is_none_or(|snapshot| {
                snapshot.bytes != candidate_ref || snapshot.mode() != ref_mode
            })
        {
            return Err(GitCommitMetadataStorageError::Conflict);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn publish_commit_ref_after_index<F>(
        &self,
        expected_repository: &GitCommitRepositorySnapshot,
        expected_branch: &GitCommitBranchSnapshot,
        branch_relative: &str,
        candidate_index: &[u8],
        candidate_ref: &[u8],
        mut persist_stage: F,
    ) -> Result<(), GitCommitMetadataStorageError>
    where
        F: FnMut(GitCommitMetadataStage) -> Result<(), GitCommitMetadataStorageError>,
    {
        let current_repository = self
            .read_commit_repository_snapshot()
            .map_err(map_git_commit_metadata_snapshot)?;
        let current_branch = self
            .read_commit_branch_snapshot(branch_relative)
            .map_err(map_git_commit_metadata_snapshot)?;
        if &current_repository != expected_repository
            || &current_branch != expected_branch
            || current_repository
                .index
                .as_ref()
                .is_none_or(|snapshot| snapshot.bytes != candidate_index)
        {
            return Err(GitCommitMetadataStorageError::Conflict);
        }
        let branch_directory = open_git_branch_parent(&self._refs, branch_relative)
            .map_err(map_git_commit_metadata_snapshot)?;
        let branch_name = branch_relative
            .rsplit('/')
            .next()
            .ok_or(GitCommitMetadataStorageError::Unsafe)?;
        let ref_mode = expected_branch
            .target
            .as_ref()
            .map_or(0o600, GitCommitFileSnapshot::mode);
        #[cfg(test)]
        git_commit_metadata_race(
            GitCommitMetadataRace::ReplaceRefBeforePublication,
            &branch_directory,
            branch_name,
        )?;
        publish_git_commit_file(
            &branch_directory,
            &self._git_dir,
            branch_name,
            expected_branch.target.as_ref(),
            candidate_ref,
            ref_mode,
            GIT_COMMIT_REF_LIMIT,
            "ref",
        )?;
        persist_stage(GitCommitMetadataStage::RefPublished)?;
        #[cfg(test)]
        git_commit_metadata_fail(GitCommitMetadataFault::RefPublished)?;
        let final_repository = self
            .read_commit_repository_snapshot()
            .map_err(map_git_commit_metadata_snapshot)?;
        let final_branch = self
            .read_commit_branch_snapshot(branch_relative)
            .map_err(map_git_commit_metadata_snapshot)?;
        if final_repository != current_repository
            || final_branch.target.as_ref().is_none_or(|snapshot| {
                snapshot.bytes != candidate_ref || snapshot.mode() != ref_mode
            })
        {
            return Err(GitCommitMetadataStorageError::Conflict);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn recover_commit_lock_residue(
        &self,
        branch_relative: &str,
        before_index: Option<(&[u8], u32)>,
        candidate_index: (&[u8], u32),
        before_ref: Option<(&[u8], u32)>,
        candidate_ref: (&[u8], u32),
    ) -> Result<(), GitCommitMetadataStorageError> {
        reconcile_git_commit_lock(
            &self._git_dir,
            &self._git_dir,
            "index",
            before_index,
            candidate_index,
            GIT_COMMIT_INDEX_LIMIT,
            "index-lock",
        )?;
        let branch_directory = open_git_branch_parent(&self._refs, branch_relative)
            .map_err(map_git_commit_metadata_snapshot)?;
        let branch_name = branch_relative
            .rsplit('/')
            .next()
            .ok_or(GitCommitMetadataStorageError::Unsafe)?;
        reconcile_git_commit_lock(
            &branch_directory,
            &self._git_dir,
            branch_name,
            before_ref,
            candidate_ref,
            GIT_COMMIT_REF_LIMIT,
            "ref-lock",
        )
    }

    pub(crate) fn read_commit_repository_snapshot(
        &self,
    ) -> Result<GitCommitRepositorySnapshot, GitCommitSnapshotError> {
        validate_git_directory_for_mutation(&self._git_dir).map_err(map_git_commit_snapshot)?;
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "REBASE_HEAD",
            "BISECT_LOG",
            "BISECT_START",
            "AUTO_MERGE",
            "rebase-apply",
            "rebase-merge",
            "sequencer",
            "index.lock",
            "HEAD.lock",
            "packed-refs.lock",
            "config.lock",
            "shallow.lock",
        ] {
            if git_entry_exists(&self._git_dir, marker)? {
                return Err(GitCommitSnapshotError::OperationInProgress);
            }
        }
        let head = read_git_entry(&self._git_dir, "HEAD", GIT_COMMIT_REF_LIMIT)?
            .ok_or(GitCommitSnapshotError::Unsafe)?;
        let config = read_git_entry(&self._git_dir, "config", GIT_CONFIG_LIMIT)?
            .ok_or(GitCommitSnapshotError::Unsafe)?;
        let index = read_git_entry(&self._git_dir, "index", GIT_COMMIT_INDEX_LIMIT)?;
        let packed_refs = read_git_entry(&self._git_dir, "packed-refs", GIT_COMMIT_REF_LIMIT)?;
        let shallow = read_git_entry(&self._git_dir, "shallow", GIT_COMMIT_REF_LIMIT)?;
        Ok(GitCommitRepositorySnapshot {
            head: GitCommitFileSnapshot {
                bytes: head.0,
                metadata: head.1,
            },
            config: GitCommitFileSnapshot {
                bytes: config.0,
                metadata: config.1,
            },
            index: index.map(|(bytes, metadata)| GitCommitFileSnapshot { bytes, metadata }),
            packed_refs: packed_refs
                .map(|(bytes, metadata)| GitCommitFileSnapshot { bytes, metadata }),
            shallow: shallow.map(|(bytes, metadata)| GitCommitFileSnapshot { bytes, metadata }),
        })
    }

    pub(crate) fn read_commit_branch_snapshot(
        &self,
        branch_relative: &str,
    ) -> Result<GitCommitBranchSnapshot, GitCommitSnapshotError> {
        let mut components = branch_relative.split('/').peekable();
        if components.peek().is_none() || components.clone().any(|part| !valid_git_ref_part(part)) {
            return Err(GitCommitSnapshotError::Unsafe);
        }
        let mut directory = open_git_directory(&self._refs, "heads")?;
        while let Some(part) = components.next() {
            if components.peek().is_none() {
                let lock_name = format!("{part}.lock");
                if git_entry_exists(&directory, &lock_name)? {
                    return Err(GitCommitSnapshotError::OperationInProgress);
                }
                let target = read_git_entry(&directory, part, GIT_COMMIT_REF_LIMIT)?
                    .map(|(bytes, metadata)| GitCommitFileSnapshot { bytes, metadata });
                return Ok(GitCommitBranchSnapshot { target });
            }
            directory = open_git_directory(&directory, part)?;
        }
        Err(GitCommitSnapshotError::Unsafe)
    }

    pub(crate) fn read_config_files(&self) -> Result<GitConfigFiles, GitConfigStorageError> {
        validate_git_directory_for_mutation(&self._git_dir)?;
        let (config, config_metadata) = read_git_config_entry(&self._git_dir, "config")?
            .ok_or(GitConfigStorageError::Unsafe)?;
        let lock = read_git_config_entry(&self._git_dir, "config.lock")?;
        Ok(GitConfigFiles {
            config,
            config_mode: config_metadata.mode,
            config_identity: (config_metadata.device, config_metadata.inode),
            lock: lock.as_ref().map(|(bytes, _)| bytes.clone()),
            lock_mode: lock.as_ref().map(|(_, metadata)| metadata.mode),
            lock_identity: lock.map(|(_, metadata)| (metadata.device, metadata.inode)),
        })
    }

    pub(crate) fn apply_config_candidate(
        &self,
        expected: &[u8],
        candidate: &[u8],
        mode: u32,
    ) -> Result<(), GitConfigStorageError> {
        if expected == candidate || !matches!(mode, 0o600 | 0o644) {
            return Err(GitConfigStorageError::Unsafe);
        }
        let mut state = self.read_config_files()?;
        if state.config == expected && state.config_mode == mode && state.lock.is_none() {
            publish_git_config_lock(&self._git_dir, candidate, mode)?;
            sync_git_directory(&self._git_dir)?;
            #[cfg(test)]
            git_config_fail(GitConfigFault::LockPublished)?;
            state = self.read_config_files()?;
        }
        if state.config == expected
            && state.config_mode == mode
            && state.lock.as_deref() == Some(candidate)
            && state.lock_mode == Some(mode)
        {
            let expected_config_identity = state.config_identity;
            let expected_lock_identity = state.lock_identity;
            #[cfg(test)]
            git_config_race(GitConfigRace::ReplaceConfigBeforeExchange, &self._git_dir)?;
            exchange_git_config_and_lock(&self._git_dir)?;
            sync_git_directory(&self._git_dir)?;
            #[cfg(test)]
            git_config_fail(GitConfigFault::Exchanged)?;
            state = self.read_config_files()?;
            let exact_exchange = state.config == candidate
                && state.config_mode == mode
                && state.config_identity == expected_lock_identity.unwrap_or_default()
                && state.lock.as_deref() == Some(expected)
                && state.lock_mode == Some(mode)
                && state.lock_identity == Some(expected_config_identity);
            let candidate_identity = state.config_identity;
            let displaced_identity = state.lock_identity;
            #[cfg(test)]
            git_config_race(GitConfigRace::ReplaceLockBeforeQuarantine, &self._git_dir)?;
            let backup = quarantine_git_config_lock(&self._git_dir)?;
            #[cfg(test)]
            git_config_fail(GitConfigFault::Quarantined)?;
            sync_git_directory(&self._git_dir)?;
            let preserved = read_git_config_entry(&self._git_dir, &backup)?;
            let current = self.read_config_files()?;
            if !exact_exchange
                || current.config != candidate
                || current.config_mode != mode
                || current.config_identity != candidate_identity
                || preserved.as_ref().is_none_or(|(bytes, metadata)| {
                    bytes != expected
                        || metadata.mode != mode
                        || Some((metadata.device, metadata.inode)) != displaced_identity
                })
            {
                return Err(GitConfigStorageError::Conflict);
            }
            return Ok(());
        } else if state.config == candidate
            && state.config_mode == mode
            && state.lock.as_deref() == Some(expected)
            && state.lock_mode == Some(mode)
        {
            let candidate_identity = state.config_identity;
            let displaced_identity = state.lock_identity;
            #[cfg(test)]
            git_config_race(GitConfigRace::ReplaceLockBeforeQuarantine, &self._git_dir)?;
            let backup = quarantine_git_config_lock(&self._git_dir)?;
            #[cfg(test)]
            git_config_fail(GitConfigFault::Quarantined)?;
            sync_git_directory(&self._git_dir)?;
            let preserved = read_git_config_entry(&self._git_dir, &backup)?;
            let current = self.read_config_files()?;
            if current.config != candidate
                || current.config_mode != mode
                || current.config_identity != candidate_identity
                || preserved.as_ref().is_none_or(|(bytes, metadata)| {
                    bytes != expected
                        || metadata.mode != mode
                        || Some((metadata.device, metadata.inode)) != displaced_identity
                })
            {
                return Err(GitConfigStorageError::Conflict);
            }
            return Ok(());
        } else if state.config != candidate
            || state.config_mode != mode
            || state.lock.as_deref() != Some(expected)
            || state.lock_mode != Some(mode)
        {
            return Err(GitConfigStorageError::Conflict);
        }

        Err(GitConfigStorageError::Conflict)
    }
}

impl GitCommitFileSnapshot {
    pub(crate) fn mode(&self) -> u32 {
        self.metadata.mode
    }
}

fn read_git_config_entry(
    directory: &OwnedFd,
    name: &str,
) -> Result<Option<(Vec<u8>, GitConfigMetadata)>, GitConfigStorageError> {
    read_git_entry(directory, name, GIT_CONFIG_LIMIT).map_err(|error| match error {
        GitCommitSnapshotError::Conflict => GitConfigStorageError::Conflict,
        GitCommitSnapshotError::Unsafe | GitCommitSnapshotError::OperationInProgress => {
            GitConfigStorageError::Unsafe
        }
        GitCommitSnapshotError::TooLarge => GitConfigStorageError::TooLarge,
        GitCommitSnapshotError::Io => GitConfigStorageError::Io,
    })
}

fn read_git_entry(
    directory: &OwnedFd,
    name: &str,
    limit: usize,
) -> Result<Option<(Vec<u8>, GitConfigMetadata)>, GitCommitSnapshotError> {
    let name = std::ffi::CString::new(name).map_err(|_| GitCommitSnapshotError::Unsafe)?;
    // SAFETY: the retained directory and validated NUL-terminated leaf are valid.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if raw < 0 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ENOENT) => Ok(None),
            Some(libc::ELOOP | libc::ENXIO | libc::ENODEV) => Err(GitCommitSnapshotError::Unsafe),
            _ => Err(GitCommitSnapshotError::Io),
        };
    }
    // SAFETY: successful openat returned a newly owned descriptor.
    let mut file = unsafe { File::from_raw_fd(raw) };
    let before = git_config_metadata(&file).map_err(map_git_commit_snapshot)?;
    if before.size > limit as u64 {
        return Err(GitCommitSnapshotError::TooLarge);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(before.size as usize)
        .map_err(|_| GitCommitSnapshotError::TooLarge)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| GitCommitSnapshotError::Io)?;
    Read::by_ref(&mut file)
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| GitCommitSnapshotError::Io)?;
    if bytes.len() > limit {
        return Err(GitCommitSnapshotError::TooLarge);
    }
    let after = git_config_metadata(&file).map_err(map_git_commit_snapshot)?;
    if before != after || bytes.len() as u64 != after.size {
        return Err(GitCommitSnapshotError::Conflict);
    }
    Ok(Some((bytes, after)))
}

fn read_git_descriptor(
    descriptor: &OwnedFd,
    limit: usize,
) -> Result<(Vec<u8>, GitConfigMetadata), GitCommitSnapshotError> {
    let duplicate = descriptor
        .try_clone()
        .map_err(|_| GitCommitSnapshotError::Io)?;
    let mut file = File::from(duplicate);
    let before = git_config_metadata(&file).map_err(map_git_commit_snapshot)?;
    if before.size > limit as u64 {
        return Err(GitCommitSnapshotError::TooLarge);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(before.size as usize)
        .map_err(|_| GitCommitSnapshotError::TooLarge)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| GitCommitSnapshotError::Io)?;
    Read::by_ref(&mut file)
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| GitCommitSnapshotError::Io)?;
    if bytes.len() > limit {
        return Err(GitCommitSnapshotError::TooLarge);
    }
    let after = git_config_metadata(&file).map_err(map_git_commit_snapshot)?;
    if before != after || bytes.len() as u64 != after.size {
        return Err(GitCommitSnapshotError::Conflict);
    }
    Ok((bytes, after))
}

fn map_git_commit_snapshot(error: GitConfigStorageError) -> GitCommitSnapshotError {
    match error {
        GitConfigStorageError::Conflict => GitCommitSnapshotError::Conflict,
        GitConfigStorageError::Unsafe => GitCommitSnapshotError::Unsafe,
        GitConfigStorageError::Unsupported => GitCommitSnapshotError::Unsafe,
        GitConfigStorageError::TooLarge => GitCommitSnapshotError::TooLarge,
        GitConfigStorageError::Io => GitCommitSnapshotError::Io,
    }
}

fn git_entry_exists(directory: &OwnedFd, name: &str) -> Result<bool, GitCommitSnapshotError> {
    let name = std::ffi::CString::new(name).map_err(|_| GitCommitSnapshotError::Unsafe)?;
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: retained directory, NUL-terminated name, and output storage are valid.
    if unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            status.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        return Ok(true);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ENOENT) => Ok(false),
        _ => Err(GitCommitSnapshotError::Io),
    }
}

fn open_git_directory(directory: &OwnedFd, name: &str) -> Result<OwnedFd, GitCommitSnapshotError> {
    if !valid_git_ref_part(name) {
        return Err(GitCommitSnapshotError::Unsafe);
    }
    let name = std::ffi::CString::new(name).map_err(|_| GitCommitSnapshotError::Unsafe)?;
    // SAFETY: retained directory and validated NUL-terminated component are valid.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOENT) => Err(GitCommitSnapshotError::Conflict),
            Some(libc::ELOOP | libc::ENOTDIR) => Err(GitCommitSnapshotError::Unsafe),
            _ => Err(GitCommitSnapshotError::Io),
        };
    }
    // SAFETY: successful openat returned a newly owned descriptor.
    let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: descriptor and output storage are valid.
    if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
        return Err(GitCommitSnapshotError::Io);
    }
    // SAFETY: successful fstat initialized the structure.
    let status = unsafe { status.assume_init() };
    if status.st_uid != unsafe { libc::geteuid() }
        || status.st_mode & libc::S_IFMT != libc::S_IFDIR
        || status.st_mode & 0o022 != 0
    {
        return Err(GitCommitSnapshotError::Unsafe);
    }
    Ok(descriptor)
}

fn map_git_pack_install_config(error: GitConfigStorageError) -> GitCommitPackInstallError {
    match error {
        GitConfigStorageError::Conflict => GitCommitPackInstallError::Conflict,
        GitConfigStorageError::Unsafe => GitCommitPackInstallError::Unsafe,
        GitConfigStorageError::Unsupported => GitCommitPackInstallError::Unsupported,
        GitConfigStorageError::TooLarge => GitCommitPackInstallError::TooLarge,
        GitConfigStorageError::Io => GitCommitPackInstallError::Io,
    }
}

fn map_git_pack_install_snapshot(error: GitCommitSnapshotError) -> GitCommitPackInstallError {
    match error {
        GitCommitSnapshotError::OperationInProgress | GitCommitSnapshotError::Conflict => {
            GitCommitPackInstallError::Conflict
        }
        GitCommitSnapshotError::Unsafe => GitCommitPackInstallError::Unsafe,
        GitCommitSnapshotError::TooLarge => GitCommitPackInstallError::TooLarge,
        GitCommitSnapshotError::Io => GitCommitPackInstallError::Io,
    }
}

fn install_git_commit_pack_file(
    directory: &OwnedFd,
    source: &GitClonePackSource,
    input: &mut File,
) -> Result<(), GitCommitPackInstallError> {
    let name = std::ffi::CString::new(source.name.as_bytes())
        .map_err(|_| GitCommitPackInstallError::Unsafe)?;
    let source_before = input
        .metadata()
        .map_err(|_| GitCommitPackInstallError::Io)?;
    if !source_before.file_type().is_file()
        || source_before.nlink() != 1
        || source_before.uid() != unsafe { libc::geteuid() }
        || source_before.len() != source.size
        || source_before.permissions().mode() & 0o022 != 0
    {
        return Err(GitCommitPackInstallError::Unsafe);
    }
    input
        .seek(SeekFrom::Start(0))
        .map_err(|_| GitCommitPackInstallError::Io)?;
    // SAFETY: retained directory and static dot component are valid.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c".".as_ptr(),
            libc::O_WRONLY | libc::O_TMPFILE | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o444,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EISDIR) => {
                Err(GitCommitPackInstallError::Unsupported)
            }
            _ => Err(GitCommitPackInstallError::Io),
        };
    }
    // SAFETY: successful openat returned a newly owned descriptor.
    let mut output = unsafe { File::from_raw_fd(raw) };
    // SAFETY: the owned descriptor is valid.
    if unsafe { libc::fchmod(output.as_raw_fd(), 0o444) } != 0 {
        return Err(GitCommitPackInstallError::Io);
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|_| GitCommitPackInstallError::Io)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or(GitCommitPackInstallError::TooLarge)?;
        if size > source.size {
            return Err(GitCommitPackInstallError::Conflict);
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|_| GitCommitPackInstallError::Io)?;
    }
    let source_after = input
        .metadata()
        .map_err(|_| GitCommitPackInstallError::Io)?;
    if !same_git_pack_metadata(&source_before, &source_after)
        || size != source.size
        || hasher.finalize().as_bytes() != &source.hash
    {
        return Err(GitCommitPackInstallError::Conflict);
    }
    output
        .sync_all()
        .map_err(|_| GitCommitPackInstallError::Unsupported)?;
    // SAFETY: output is an owned unnamed inode and target is confined.
    let mut published = unsafe {
        libc::linkat(
            output.as_raw_fd(),
            c"".as_ptr(),
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::AT_EMPTY_PATH,
        )
    };
    if published != 0
        && matches!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM | libc::EINVAL | libc::ENOENT)
        )
    {
        let source_path = std::ffi::CString::new(format!("/proc/self/fd/{}", output.as_raw_fd()))
            .map_err(|_| GitCommitPackInstallError::Io)?;
        // SAFETY: source names the still-open inode and target is confined.
        published = unsafe {
            libc::linkat(
                libc::AT_FDCWD,
                source_path.as_ptr(),
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::AT_SYMLINK_FOLLOW,
            )
        };
    }
    if published != 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EEXIST) => verify_git_commit_pack_file(directory, source),
            Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EPERM) => {
                Err(GitCommitPackInstallError::Unsupported)
            }
            _ => Err(GitCommitPackInstallError::Io),
        };
    }
    // SAFETY: retained pack directory is valid.
    if unsafe { libc::fsync(directory.as_raw_fd()) } != 0 {
        return Err(GitCommitPackInstallError::Unsupported);
    }
    verify_git_commit_pack_file(directory, source)
}

fn verify_git_commit_pack_file(
    directory: &OwnedFd,
    source: &GitClonePackSource,
) -> Result<(), GitCommitPackInstallError> {
    let name = std::ffi::CString::new(source.name.as_bytes())
        .map_err(|_| GitCommitPackInstallError::Unsafe)?;
    // SAFETY: retained directory and validated name are valid.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOENT) => Err(GitCommitPackInstallError::Conflict),
            Some(libc::ELOOP | libc::ENXIO | libc::ENODEV) => {
                Err(GitCommitPackInstallError::Unsafe)
            }
            _ => Err(GitCommitPackInstallError::Io),
        };
    }
    // SAFETY: successful openat returned an owned descriptor.
    let mut file = unsafe { File::from_raw_fd(raw) };
    let before = file.metadata().map_err(|_| GitCommitPackInstallError::Io)?;
    if !before.file_type().is_file()
        || before.nlink() != 1
        || before.uid() != unsafe { libc::geteuid() }
        || before.len() != source.size
        || before.permissions().mode() & 0o022 != 0
    {
        return Err(GitCommitPackInstallError::Unsafe);
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| GitCommitPackInstallError::Io)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or(GitCommitPackInstallError::TooLarge)?;
        if size > source.size {
            return Err(GitCommitPackInstallError::Conflict);
        }
        hasher.update(&buffer[..read]);
    }
    let after = file.metadata().map_err(|_| GitCommitPackInstallError::Io)?;
    if !same_git_pack_metadata(&before, &after)
        || size != source.size
        || hasher.finalize().as_bytes() != &source.hash
    {
        return Err(GitCommitPackInstallError::Conflict);
    }
    Ok(())
}

fn same_git_pack_metadata(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
        && left.permissions().mode() == right.permissions().mode()
        && left.nlink() == right.nlink()
}

fn map_git_commit_metadata_snapshot(
    error: GitCommitSnapshotError,
) -> GitCommitMetadataStorageError {
    match error {
        GitCommitSnapshotError::OperationInProgress | GitCommitSnapshotError::Conflict => {
            GitCommitMetadataStorageError::Conflict
        }
        GitCommitSnapshotError::Unsafe => GitCommitMetadataStorageError::Unsafe,
        GitCommitSnapshotError::TooLarge => GitCommitMetadataStorageError::TooLarge,
        GitCommitSnapshotError::Io => GitCommitMetadataStorageError::Io,
    }
}

fn open_git_branch_parent(
    refs: &OwnedFd,
    branch_relative: &str,
) -> Result<OwnedFd, GitCommitSnapshotError> {
    let mut components = branch_relative.split('/').peekable();
    if components.peek().is_none() || components.clone().any(|part| !valid_git_ref_part(part)) {
        return Err(GitCommitSnapshotError::Unsafe);
    }
    let mut directory = open_git_directory(refs, "heads")?;
    while let Some(component) = components.next() {
        if components.peek().is_none() {
            return Ok(directory);
        }
        directory = open_git_directory(&directory, component)?;
    }
    Err(GitCommitSnapshotError::Unsafe)
}

#[allow(clippy::too_many_arguments)]
fn publish_git_commit_file(
    directory: &OwnedFd,
    backup_directory: &OwnedFd,
    name: &str,
    expected: Option<&GitCommitFileSnapshot>,
    candidate: &[u8],
    mode: u32,
    limit: usize,
    backup_kind: &str,
) -> Result<(), GitCommitMetadataStorageError> {
    if candidate.is_empty()
        || candidate.len() > limit
        || !matches!(mode, 0o600 | 0o644)
        || !valid_git_ref_part(name)
    {
        return Err(GitCommitMetadataStorageError::Unsafe);
    }
    let lock_name = format!("{name}.lock");
    if read_git_entry(directory, &lock_name, limit)
        .map_err(map_git_commit_metadata_snapshot)?
        .is_some()
    {
        return Err(GitCommitMetadataStorageError::Conflict);
    }
    let current =
        read_git_entry(directory, name, limit).map_err(map_git_commit_metadata_snapshot)?;
    if !git_commit_file_matches(current.as_ref(), expected) {
        return Err(GitCommitMetadataStorageError::Conflict);
    }
    publish_git_commit_lock(directory, &lock_name, candidate, mode)?;
    let locked = read_git_entry(directory, &lock_name, limit)
        .map_err(map_git_commit_metadata_snapshot)?
        .ok_or(GitCommitMetadataStorageError::Conflict)?;
    if locked.0 != candidate || locked.1.mode != mode {
        return Err(GitCommitMetadataStorageError::Conflict);
    }
    #[cfg(test)]
    match backup_kind {
        "index" => git_commit_metadata_fail(GitCommitMetadataFault::IndexLockPublished)?,
        "ref" => git_commit_metadata_fail(GitCommitMetadataFault::RefLockPublished)?,
        _ => {}
    }
    let current =
        read_git_entry(directory, name, limit).map_err(map_git_commit_metadata_snapshot)?;
    if !git_commit_file_matches(current.as_ref(), expected) {
        quarantine_git_commit_entry(directory, &lock_name, backup_directory, "foreign-lock")?;
        return Err(GitCommitMetadataStorageError::Conflict);
    }
    #[cfg(test)]
    if backup_kind == "ref" {
        git_commit_metadata_race(
            GitCommitMetadataRace::ReplaceRefAfterValidation,
            directory,
            name,
        )?;
    }

    if expected.is_some() {
        exchange_git_commit_entries(directory, name, &lock_name)?;
    } else {
        rename_git_commit_no_replace(directory, &lock_name, directory, name)?;
    }
    sync_git_commit_directory(directory)?;
    #[cfg(test)]
    match backup_kind {
        "index" => git_commit_metadata_fail(GitCommitMetadataFault::IndexExchanged)?,
        "ref" => git_commit_metadata_fail(GitCommitMetadataFault::RefExchanged)?,
        _ => {}
    }
    let installed = read_git_entry(directory, name, limit)
        .map_err(map_git_commit_metadata_snapshot)?
        .ok_or(GitCommitMetadataStorageError::Conflict)?;
    if !git_commit_entry_matches(&installed, &locked) {
        return Err(GitCommitMetadataStorageError::Conflict);
    }
    if let Some(expected) = expected {
        let displaced = read_git_entry(directory, &lock_name, limit)
            .map_err(map_git_commit_metadata_snapshot)?
            .ok_or(GitCommitMetadataStorageError::Conflict)?;
        if !git_commit_displaced_matches(&displaced, expected) {
            restore_git_commit_exchange_after_race(
                directory,
                name,
                &lock_name,
                &installed,
                &displaced,
                backup_directory,
                limit,
            )?;
            return Err(GitCommitMetadataStorageError::Conflict);
        }
        let backup =
            quarantine_git_commit_entry(directory, &lock_name, backup_directory, backup_kind)?;
        let preserved = read_git_entry(backup_directory, &backup, limit)
            .map_err(map_git_commit_metadata_snapshot)?
            .ok_or(GitCommitMetadataStorageError::Conflict)?;
        if !git_commit_displaced_matches(&preserved, expected) {
            return Err(GitCommitMetadataStorageError::Conflict);
        }
    }
    sync_git_commit_directory(backup_directory)?;
    Ok(())
}

fn restore_git_commit_index(
    git_directory: &OwnedFd,
    candidate: &GitCommitFileSnapshot,
    before: Option<&GitCommitFileSnapshot>,
) -> Result<(), GitCommitMetadataStorageError> {
    match before {
        Some(before) => publish_git_commit_file(
            git_directory,
            git_directory,
            "index",
            Some(candidate),
            &before.bytes,
            before.mode(),
            GIT_COMMIT_INDEX_LIMIT,
            "index-candidate",
        ),
        None => {
            let current = read_git_entry(git_directory, "index", GIT_COMMIT_INDEX_LIMIT)
                .map_err(map_git_commit_metadata_snapshot)?;
            if !git_commit_file_matches(current.as_ref(), Some(candidate)) {
                return Err(GitCommitMetadataStorageError::Conflict);
            }
            quarantine_git_commit_entry(git_directory, "index", git_directory, "index-candidate")?;
            sync_git_commit_directory(git_directory)?;
            if read_git_entry(git_directory, "index", GIT_COMMIT_INDEX_LIMIT)
                .map_err(map_git_commit_metadata_snapshot)?
                .is_some()
            {
                return Err(GitCommitMetadataStorageError::Conflict);
            }
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn reconcile_git_commit_lock(
    directory: &OwnedFd,
    backup_directory: &OwnedFd,
    name: &str,
    before: Option<(&[u8], u32)>,
    candidate: (&[u8], u32),
    limit: usize,
    backup_kind: &str,
) -> Result<(), GitCommitMetadataStorageError> {
    let lock_name = format!("{name}.lock");
    let Some(lock) =
        read_git_entry(directory, &lock_name, limit).map_err(map_git_commit_metadata_snapshot)?
    else {
        return Ok(());
    };
    let target =
        read_git_entry(directory, name, limit).map_err(map_git_commit_metadata_snapshot)?;
    let target_is_before = git_commit_bytes_mode_match(target.as_ref(), before);
    let target_is_candidate = git_commit_bytes_mode_match(target.as_ref(), Some(candidate));
    let lock_is_before = git_commit_bytes_mode_match(Some(&lock), before);
    let lock_is_candidate = git_commit_bytes_mode_match(Some(&lock), Some(candidate));
    if (target_is_before && lock_is_candidate) || (target_is_candidate && lock_is_before) {
        quarantine_git_commit_entry(directory, &lock_name, backup_directory, backup_kind)?;
        sync_git_commit_directory(directory)?;
        sync_git_commit_directory(backup_directory)?;
        Ok(())
    } else {
        Err(GitCommitMetadataStorageError::Conflict)
    }
}

fn git_commit_bytes_mode_match(
    actual: Option<&(Vec<u8>, GitConfigMetadata)>,
    expected: Option<(&[u8], u32)>,
) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some((bytes, metadata)), Some((expected_bytes, expected_mode))) => {
            bytes == expected_bytes && metadata.mode == expected_mode
        }
        _ => false,
    }
}

fn git_commit_file_matches(
    actual: Option<&(Vec<u8>, GitConfigMetadata)>,
    expected: Option<&GitCommitFileSnapshot>,
) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some((bytes, metadata)), Some(expected)) => {
            bytes == &expected.bytes && metadata == &expected.metadata
        }
        _ => false,
    }
}

fn git_commit_displaced_matches(
    actual: &(Vec<u8>, GitConfigMetadata),
    expected: &GitCommitFileSnapshot,
) -> bool {
    actual.0 == expected.bytes
        && actual.1.device == expected.metadata.device
        && actual.1.inode == expected.metadata.inode
        && actual.1.size == expected.metadata.size
        && actual.1.mode == expected.metadata.mode
}

fn git_commit_entry_matches(
    actual: &(Vec<u8>, GitConfigMetadata),
    expected: &(Vec<u8>, GitConfigMetadata),
) -> bool {
    actual.0 == expected.0
        && actual.1.device == expected.1.device
        && actual.1.inode == expected.1.inode
        && actual.1.size == expected.1.size
        && actual.1.mode == expected.1.mode
}

#[allow(clippy::too_many_arguments)]
fn restore_git_commit_exchange_after_race(
    directory: &OwnedFd,
    name: &str,
    lock_name: &str,
    candidate: &(Vec<u8>, GitConfigMetadata),
    displaced: &(Vec<u8>, GitConfigMetadata),
    backup_directory: &OwnedFd,
    limit: usize,
) -> Result<(), GitCommitMetadataStorageError> {
    let current = read_git_entry(directory, name, limit)
        .map_err(map_git_commit_metadata_snapshot)?
        .ok_or(GitCommitMetadataStorageError::Conflict)?;
    let current_lock = read_git_entry(directory, lock_name, limit)
        .map_err(map_git_commit_metadata_snapshot)?
        .ok_or(GitCommitMetadataStorageError::Conflict)?;
    if !git_commit_entry_matches(&current, candidate)
        || !git_commit_entry_matches(&current_lock, displaced)
    {
        return Err(GitCommitMetadataStorageError::Conflict);
    }
    exchange_git_commit_entries(directory, name, lock_name)?;
    sync_git_commit_directory(directory)?;
    let restored = read_git_entry(directory, name, limit)
        .map_err(map_git_commit_metadata_snapshot)?
        .ok_or(GitCommitMetadataStorageError::Conflict)?;
    let candidate_lock = read_git_entry(directory, lock_name, limit)
        .map_err(map_git_commit_metadata_snapshot)?
        .ok_or(GitCommitMetadataStorageError::Conflict)?;
    if !git_commit_entry_matches(&restored, displaced)
        || !git_commit_entry_matches(&candidate_lock, candidate)
    {
        return Err(GitCommitMetadataStorageError::Conflict);
    }
    let backup =
        quarantine_git_commit_entry(directory, lock_name, backup_directory, "candidate-race")?;
    let preserved = read_git_entry(backup_directory, &backup, limit)
        .map_err(map_git_commit_metadata_snapshot)?
        .ok_or(GitCommitMetadataStorageError::Conflict)?;
    if !git_commit_entry_matches(&preserved, candidate) {
        return Err(GitCommitMetadataStorageError::Conflict);
    }
    sync_git_commit_directory(directory)?;
    sync_git_commit_directory(backup_directory)
}

fn publish_git_commit_lock(
    directory: &OwnedFd,
    name: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), GitCommitMetadataStorageError> {
    let name = std::ffi::CString::new(name).map_err(|_| GitCommitMetadataStorageError::Unsafe)?;
    // SAFETY: retained directory and static dot component are valid.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c".".as_ptr(),
            libc::O_WRONLY | libc::O_TMPFILE | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EISDIR) => {
                Err(GitCommitMetadataStorageError::Unsupported)
            }
            _ => Err(GitCommitMetadataStorageError::Io),
        };
    }
    // SAFETY: successful openat returned an owned descriptor.
    let mut file = unsafe { File::from_raw_fd(raw) };
    // SAFETY: the owned descriptor is valid.
    if unsafe { libc::fchmod(file.as_raw_fd(), mode) } != 0 {
        return Err(GitCommitMetadataStorageError::Io);
    }
    file.write_all(bytes)
        .map_err(|_| GitCommitMetadataStorageError::Io)?;
    file.sync_all()
        .map_err(|_| GitCommitMetadataStorageError::Unsupported)?;
    // SAFETY: file is an unnamed inode and target is a confined validated component.
    let mut result = unsafe {
        libc::linkat(
            file.as_raw_fd(),
            c"".as_ptr(),
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::AT_EMPTY_PATH,
        )
    };
    if result != 0
        && matches!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM | libc::EINVAL | libc::ENOENT)
        )
    {
        let source = std::ffi::CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|_| GitCommitMetadataStorageError::Io)?;
        // SAFETY: source names the open inode and target is confined.
        result = unsafe {
            libc::linkat(
                libc::AT_FDCWD,
                source.as_ptr(),
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::AT_SYMLINK_FOLLOW,
            )
        };
    }
    if result != 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EEXIST) => Err(GitCommitMetadataStorageError::Conflict),
            Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EPERM) => {
                Err(GitCommitMetadataStorageError::Unsupported)
            }
            _ => Err(GitCommitMetadataStorageError::Io),
        };
    }
    sync_git_commit_directory(directory)
}

fn exchange_git_commit_entries(
    directory: &OwnedFd,
    left: &str,
    right: &str,
) -> Result<(), GitCommitMetadataStorageError> {
    let left = std::ffi::CString::new(left).map_err(|_| GitCommitMetadataStorageError::Unsafe)?;
    let right = std::ffi::CString::new(right).map_err(|_| GitCommitMetadataStorageError::Unsafe)?;
    // SAFETY: names and retained directory are valid.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            directory.as_raw_fd(),
            left.as_ptr(),
            directory.as_raw_fd(),
            right.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP) => {
                Err(GitCommitMetadataStorageError::Unsupported)
            }
            Some(libc::ENOENT) => Err(GitCommitMetadataStorageError::Conflict),
            _ => Err(GitCommitMetadataStorageError::Io),
        }
    }
}

fn rename_git_commit_no_replace(
    source_directory: &OwnedFd,
    source: &str,
    destination_directory: &OwnedFd,
    destination: &str,
) -> Result<(), GitCommitMetadataStorageError> {
    let source =
        std::ffi::CString::new(source).map_err(|_| GitCommitMetadataStorageError::Unsafe)?;
    let destination =
        std::ffi::CString::new(destination).map_err(|_| GitCommitMetadataStorageError::Unsafe)?;
    // SAFETY: names and retained directories are valid.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            source_directory.as_raw_fd(),
            source.as_ptr(),
            destination_directory.as_raw_fd(),
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EEXIST | libc::ENOENT) => Err(GitCommitMetadataStorageError::Conflict),
            Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP) => {
                Err(GitCommitMetadataStorageError::Unsupported)
            }
            _ => Err(GitCommitMetadataStorageError::Io),
        }
    }
}

fn quarantine_git_commit_entry(
    source_directory: &OwnedFd,
    source: &str,
    backup_directory: &OwnedFd,
    kind: &str,
) -> Result<String, GitCommitMetadataStorageError> {
    for _ in 0..8 {
        let mut random = [0_u8; 16];
        fill_storage_random(&mut random).map_err(map_git_commit_metadata_config)?;
        let mut suffix = String::with_capacity(32);
        for byte in random {
            use std::fmt::Write as _;
            write!(&mut suffix, "{byte:02x}").expect("writing to String cannot fail");
        }
        let backup = format!("rehow-{kind}-before-v1-{suffix}");
        match rename_git_commit_no_replace(source_directory, source, backup_directory, &backup) {
            Ok(()) => return Ok(backup),
            Err(GitCommitMetadataStorageError::Conflict) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(GitCommitMetadataStorageError::Conflict)
}

fn map_git_commit_metadata_config(error: GitConfigStorageError) -> GitCommitMetadataStorageError {
    match error {
        GitConfigStorageError::Conflict => GitCommitMetadataStorageError::Conflict,
        GitConfigStorageError::Unsafe => GitCommitMetadataStorageError::Unsafe,
        GitConfigStorageError::Unsupported => GitCommitMetadataStorageError::Unsupported,
        GitConfigStorageError::TooLarge => GitCommitMetadataStorageError::TooLarge,
        GitConfigStorageError::Io => GitCommitMetadataStorageError::Io,
    }
}

fn sync_git_commit_directory(directory: &OwnedFd) -> Result<(), GitCommitMetadataStorageError> {
    // SAFETY: retained directory descriptor is valid.
    if unsafe { libc::fsync(directory.as_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(GitCommitMetadataStorageError::Unsupported)
    }
}

fn valid_git_ref_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.ends_with(".lock")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn git_config_metadata(file: &File) -> Result<GitConfigMetadata, GitConfigStorageError> {
    let metadata = file.metadata().map_err(|_| GitConfigStorageError::Io)?;
    let mode = metadata.permissions().mode() & 0o7777;
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || !matches!(mode, 0o600 | 0o644)
    {
        return Err(GitConfigStorageError::Unsafe);
    }
    Ok(GitConfigMetadata {
        device: metadata.dev(),
        inode: metadata.ino(),
        size: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
        mode,
    })
}

fn validate_git_directory_for_mutation(directory: &OwnedFd) -> Result<(), GitConfigStorageError> {
    // SAFETY: descriptor/status are valid.
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(directory.as_raw_fd(), status.as_mut_ptr()) } != 0 {
        return Err(GitConfigStorageError::Io);
    }
    // SAFETY: successful fstat initialized the structure.
    let status = unsafe { status.assume_init() };
    if status.st_uid != unsafe { libc::geteuid() }
        || status.st_mode & libc::S_IFMT != libc::S_IFDIR
        || status.st_mode & 0o022 != 0
    {
        return Err(GitConfigStorageError::Unsafe);
    }
    Ok(())
}

fn publish_git_config_lock(
    directory: &OwnedFd,
    candidate: &[u8],
    mode: u32,
) -> Result<(), GitConfigStorageError> {
    // SAFETY: retained directory and static dot component are valid.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c".".as_ptr(),
            libc::O_WRONLY | libc::O_TMPFILE | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EISDIR) => {
                Err(GitConfigStorageError::Unsupported)
            }
            _ => Err(GitConfigStorageError::Io),
        };
    }
    // SAFETY: successful openat returned a newly owned descriptor.
    let mut file = unsafe { File::from_raw_fd(raw) };
    // SAFETY: the owned descriptor is valid.
    if unsafe { libc::fchmod(file.as_raw_fd(), mode) } != 0 {
        return Err(GitConfigStorageError::Io);
    }
    file.write_all(candidate)
        .map_err(|_| GitConfigStorageError::Io)?;
    file.sync_all()
        .map_err(|_| GitConfigStorageError::Unsupported)?;
    // SAFETY: descriptors and NUL-terminated names are valid.
    let mut result = unsafe {
        libc::linkat(
            file.as_raw_fd(),
            c"".as_ptr(),
            directory.as_raw_fd(),
            c"config.lock".as_ptr(),
            libc::AT_EMPTY_PATH,
        )
    };
    if result != 0
        && matches!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM | libc::EINVAL | libc::ENOENT)
        )
    {
        let source = std::ffi::CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|_| GitConfigStorageError::Io)?;
        // SAFETY: source names the open unnamed inode and target is retained.
        result = unsafe {
            libc::linkat(
                libc::AT_FDCWD,
                source.as_ptr(),
                directory.as_raw_fd(),
                c"config.lock".as_ptr(),
                libc::AT_SYMLINK_FOLLOW,
            )
        };
    }
    if result != 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EEXIST) => Err(GitConfigStorageError::Conflict),
            Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EPERM) => {
                Err(GitConfigStorageError::Unsupported)
            }
            _ => Err(GitConfigStorageError::Io),
        };
    }
    Ok(())
}

fn exchange_git_config_and_lock(directory: &OwnedFd) -> Result<(), GitConfigStorageError> {
    // SAFETY: syscall receives a retained directory and static leaf names.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            directory.as_raw_fd(),
            c"config".as_ptr(),
            directory.as_raw_fd(),
            c"config.lock".as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP) => {
                Err(GitConfigStorageError::Unsupported)
            }
            Some(libc::ENOENT) => Err(GitConfigStorageError::Conflict),
            _ => Err(GitConfigStorageError::Io),
        }
    }
}

fn quarantine_git_config_lock(directory: &OwnedFd) -> Result<String, GitConfigStorageError> {
    for _ in 0..8 {
        let mut random = [0_u8; 16];
        fill_storage_random(&mut random)?;
        let mut suffix = String::with_capacity(32);
        for byte in random {
            use std::fmt::Write as _;
            write!(&mut suffix, "{byte:02x}").expect("writing to String cannot fail");
        }
        let backup = format!("rehow-config-before-v1-{suffix}");
        let backup_c =
            std::ffi::CString::new(backup.as_str()).map_err(|_| GitConfigStorageError::Io)?;
        // No-replace rename clears Git's conventional lock name while moving
        // whichever inode currently occupies it to an inert, unique backup.
        // A concurrent replacement is preserved rather than unlinked.
        // SAFETY: retained directory and NUL-terminated leaf names are valid.
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                directory.as_raw_fd(),
                c"config.lock".as_ptr(),
                directory.as_raw_fd(),
                backup_c.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result == 0 {
            return Ok(backup);
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EEXIST) => continue,
            Some(libc::ENOENT) => return Err(GitConfigStorageError::Conflict),
            Some(libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP) => {
                return Err(GitConfigStorageError::Unsupported);
            }
            _ => return Err(GitConfigStorageError::Io),
        }
    }
    Err(GitConfigStorageError::Conflict)
}

fn fill_storage_random(bytes: &mut [u8]) -> Result<(), GitConfigStorageError> {
    let mut filled = 0usize;
    while filled < bytes.len() {
        // SAFETY: the remaining slice is writable for the requested length.
        let result = unsafe {
            libc::getrandom(bytes[filled..].as_mut_ptr().cast(), bytes.len() - filled, 0)
        };
        if result > 0 {
            filled += result as usize;
        } else if result == 0
            || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
        {
            return Err(GitConfigStorageError::Io);
        }
    }
    Ok(())
}

fn sync_git_directory(directory: &OwnedFd) -> Result<(), GitConfigStorageError> {
    // SAFETY: retained directory descriptor is valid.
    if unsafe { libc::fsync(directory.as_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(GitConfigStorageError::Unsupported)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitRepositoryCapabilityError {
    NotRepository,
    Unsafe,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GitInitializeStorageError {
    Conflict,
    Unsafe,
    Unsupported,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceIssueKind {
    UnsafeEntry,
    UnsupportedEntry,
    MissingCategoryDescriptor,
    MalformedCategory,
    MalformedGuide,
    FileTooLarge,
    CategoryMismatch,
    DuplicateCategoryId,
    DuplicateGuideId,
    VerificationMismatch,
    MissingAttachment,
    OrphanAttachment,
    AttachmentHashMismatch,
    AttachmentSizeMismatch,
}

#[derive(Debug, Eq, PartialEq)]
pub struct WorkspaceIssue {
    relative_path: String,
    kind: WorkspaceIssueKind,
}

impl WorkspaceIssue {
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn kind(&self) -> WorkspaceIssueKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MalformedWorkspaceReason {
    Manifest,
    CategoriesMissing,
    CategoriesUnsafe,
}

pub enum WorkspaceInspection {
    Empty(EmptyWorkspaceTarget),
    Compatible(Box<WorkspaceView>),
    NewerUnsupported { version: u32 },
    OrdinaryMarkdown,
    UnrelatedNonempty,
    Malformed { reason: MalformedWorkspaceReason },
}

pub struct EmptyWorkspaceTarget {
    root: linux::DirectoryCapability,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct CloneDestinationSnapshot {
    device: u64,
    inode: u64,
    root_hash: [u8; 32],
    capability_hash: [u8; 32],
    facts: WorkspaceFolderFacts,
}

impl CloneDestinationSnapshot {
    pub(crate) fn device(self) -> u64 {
        self.device
    }

    pub(crate) fn inode(self) -> u64 {
        self.inode
    }

    pub(crate) fn root_hash(self) -> [u8; 32] {
        self.root_hash
    }

    pub(crate) fn capability_hash(self) -> [u8; 32] {
        self.capability_hash
    }

    pub(crate) fn facts(self) -> WorkspaceFolderFacts {
        self.facts
    }
}

impl EmptyWorkspaceTarget {
    pub(crate) fn clone_destination_snapshot(
        &self,
    ) -> Result<CloneDestinationSnapshot, WorkspaceInspectError> {
        self.root
            .verify_empty_workspace_root()
            .map_err(map_clone_destination_error)?;
        let (device, inode) = self.root.identity().map_err(map_clone_destination_error)?;
        Ok(CloneDestinationSnapshot {
            device,
            inode,
            root_hash: self.root.root_hash(),
            capability_hash: self.root.capability_hash(),
            facts: inspect_folder_facts(&self.root)?,
        })
    }

    pub(crate) fn matches_clone_destination(
        &self,
        expected: CloneDestinationSnapshot,
    ) -> Result<bool, WorkspaceInspectError> {
        let (device, inode) = self.root.identity().map_err(map_clone_destination_error)?;
        Ok(device == expected.device()
            && inode == expected.inode()
            && self.root.root_hash() == expected.root_hash()
            && self.root.capability_hash() == expected.capability_hash())
    }

    pub(crate) fn verify_git_clone_materialization(
        &self,
        marker: &[u8],
        expected_paths: &[String],
    ) -> Result<(), WorkspaceInspectError> {
        let temp_names = vec![String::new(); expected_paths.len()];
        self.root
            .verify_git_clone_inventory(marker, expected_paths, &temp_names, true, true)
            .map_err(map_clone_destination_error)
    }

    pub(crate) fn verify_git_clone_portable_content(
        &self,
        marker: &[u8],
        expected_paths: &[String],
    ) -> Result<(), WorkspaceInspectError> {
        let temp_names = vec![String::new(); expected_paths.len()];
        self.root
            .verify_git_clone_inventory(marker, expected_paths, &temp_names, true, false)
            .map_err(map_clone_destination_error)
    }

    pub(crate) fn read_git_clone_materialized_file(
        &self,
        relative_path: &str,
        expected_size: u64,
    ) -> Result<Vec<u8>, WorkspaceInspectError> {
        self.root
            .read_git_clone_file(relative_path, expected_size)
            .map_err(map_clone_destination_error)
    }
}

fn map_clone_destination_error(error: linux::MutationIoError) -> WorkspaceInspectError {
    match error {
        linux::MutationIoError::Unsafe => WorkspaceInspectError::UnsafeRoot,
        linux::MutationIoError::Conflict => WorkspaceInspectError::ChangedDuringRead,
        linux::MutationIoError::Unsupported | linux::MutationIoError::Io => {
            WorkspaceInspectError::Io
        }
        linux::MutationIoError::TooLarge => WorkspaceInspectError::ResourceLimit,
    }
}

fn map_git_commit_managed_read(error: linux::MutationIoError) -> GitCommitSnapshotError {
    match error {
        linux::MutationIoError::Conflict => GitCommitSnapshotError::Conflict,
        linux::MutationIoError::Unsafe => GitCommitSnapshotError::Unsafe,
        linux::MutationIoError::Unsupported => GitCommitSnapshotError::Unsafe,
        linux::MutationIoError::TooLarge => GitCommitSnapshotError::TooLarge,
        linux::MutationIoError::Io => GitCommitSnapshotError::Io,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceFolderFacts {
    writable: bool,
    existing_git_repository: bool,
    nested_git_repository: bool,
    git_probe_complete: bool,
    unsafe_git_marker: bool,
}

impl WorkspaceFolderFacts {
    pub fn writable(self) -> bool {
        self.writable
    }

    pub fn existing_git_repository(self) -> bool {
        self.existing_git_repository
    }

    pub fn nested_git_repository(self) -> bool {
        self.nested_git_repository
    }

    pub fn git_probe_complete(self) -> bool {
        self.git_probe_complete
    }

    pub fn unsafe_git_marker(self) -> bool {
        self.unsafe_git_marker
    }

    pub fn after_git_initialize(self) -> Self {
        Self {
            existing_git_repository: true,
            git_probe_complete: true,
            unsafe_git_marker: false,
            ..self
        }
    }
}

pub struct WorkspaceInspectionReport {
    facts: WorkspaceFolderFacts,
    classification: WorkspaceInspection,
}

/// A Workspace root whose originally selected directory is retained by descriptor.
pub struct WorkspaceRootSelection {
    root: linux::DirectoryCapability,
    canonical_path: std::path::PathBuf,
}

impl WorkspaceRootSelection {
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }
}

impl WorkspaceInspectionReport {
    pub fn facts(&self) -> WorkspaceFolderFacts {
        self.facts
    }

    pub fn classification(&self) -> &WorkspaceInspection {
        &self.classification
    }

    pub fn into_parts(self) -> (WorkspaceFolderFacts, WorkspaceInspection) {
        (self.facts, self.classification)
    }
}

pub struct WorkspaceView {
    // Keep the root capability alive for the lifetime of every loaded record.
    _root: linux::DirectoryCapability,
    manifest: WorkspaceManifest,
    manifest_revision: Revision,
    categories: Vec<LoadedCategory>,
    issues: Vec<WorkspaceIssue>,
    search_index: crate::search::SearchIndex,
}

pub struct LoadedCategory {
    relative_directory: String,
    descriptor_revision: Revision,
    category: Category,
    guides: Vec<LoadedGuide>,
}

pub struct LoadedGuide {
    relative_path: String,
    revision: Revision,
    guide: Guide,
    effective_status: GuideStatus,
}

impl WorkspaceView {
    pub fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }

    pub fn manifest_revision(&self) -> Revision {
        self.manifest_revision
    }

    pub fn categories(&self) -> &[LoadedCategory] {
        &self.categories
    }

    pub fn issues(&self) -> &[WorkspaceIssue] {
        &self.issues
    }

    pub(crate) fn search_index(&self) -> &crate::search::SearchIndex {
        &self.search_index
    }

    pub(crate) fn fresh_snapshot(&self) -> Result<Box<WorkspaceView>, WorkspaceInspectError> {
        let root = self._root.duplicate().map_err(map_io)?;
        let WorkspaceInspection::Compatible(snapshot) = inspect_open_root(root)? else {
            return Err(WorkspaceInspectError::ChangedDuringRead);
        };
        Ok(snapshot)
    }

    pub(crate) fn git_repository_capability(
        &self,
    ) -> Result<GitRepositoryCapability, GitRepositoryCapabilityError> {
        self._root.git_repository_capability()
    }

    pub(crate) fn with_git_commit_writer_lock<T, E, F>(
        &self,
        state_home: &Path,
        runtime_directory: Option<&Path>,
        action: F,
    ) -> Result<Result<T, E>, recovery::GitCommitStagingError>
    where
        F: FnOnce() -> Result<T, E>,
    {
        recovery::with_git_commit_writer_lock(
            &self._root,
            state_home,
            runtime_directory,
            *self.manifest.workspace_id(),
            action,
        )
    }

    pub(crate) fn open_git_commit_recovery_session(
        &self,
        state_home: &Path,
        runtime_directory: Option<&Path>,
    ) -> Result<recovery::GitCommitRecoverySession, recovery::GitCommitIntentError> {
        recovery::open_git_commit_recovery_session(
            &self._root,
            state_home,
            runtime_directory,
            *self.manifest.workspace_id(),
        )
    }

    pub(crate) fn read_git_commit_managed_file(
        &self,
        relative_path: &str,
        limit: u64,
    ) -> Result<GitCommitManagedFileSnapshot, GitCommitSnapshotError> {
        let limit = usize::try_from(limit).map_err(|_| GitCommitSnapshotError::TooLarge)?;
        let target = self
            ._root
            .open_managed_target(relative_path)
            .map_err(map_git_commit_managed_read)?;
        let state = target.state(limit).map_err(map_git_commit_managed_read)?;
        let linux::ManagedState::Regular(state) = state else {
            return Err(GitCommitSnapshotError::Conflict);
        };
        let revision = document_revision(relative_path, &state.bytes);
        Ok(GitCommitManagedFileSnapshot {
            bytes: state.bytes,
            revision,
            mode: state.mode,
        })
    }

    pub(crate) fn read_optional_git_commit_managed_file(
        &self,
        relative_path: &str,
        limit: u64,
    ) -> Result<Option<GitCommitManagedFileSnapshot>, GitCommitSnapshotError> {
        let limit = usize::try_from(limit).map_err(|_| GitCommitSnapshotError::TooLarge)?;
        let target = self
            ._root
            .open_managed_target(relative_path)
            .map_err(map_git_commit_managed_read)?;
        match target.state(limit).map_err(map_git_commit_managed_read)? {
            linux::ManagedState::Missing => Ok(None),
            linux::ManagedState::Regular(state) => Ok(Some(GitCommitManagedFileSnapshot {
                revision: document_revision(relative_path, &state.bytes),
                bytes: state.bytes,
                mode: state.mode,
            })),
        }
    }

    pub(crate) fn git_initialization_binding(&self) -> (String, Revision, [u8; 32]) {
        (
            self.manifest.workspace_id().to_string(),
            self.manifest_revision,
            self._root.capability_hash(),
        )
    }

    pub(crate) fn git_initialization_facts(
        &self,
    ) -> Result<WorkspaceFolderFacts, GitInitializeStorageError> {
        inspect_folder_facts(&self._root).map_err(|error| match error {
            WorkspaceInspectError::UnsafeRoot
            | WorkspaceInspectError::ChangedDuringRead
            | WorkspaceInspectError::ResourceLimit => GitInitializeStorageError::Unsafe,
            WorkspaceInspectError::NotFound
            | WorkspaceInspectError::NotDirectory
            | WorkspaceInspectError::PermissionDenied
            | WorkspaceInspectError::Io => GitInitializeStorageError::Io,
        })
    }

    pub(crate) fn git_initialization_resumable(&self) -> Result<bool, GitInitializeStorageError> {
        let marker = git_initialization_marker(
            self.manifest.workspace_id().to_string().as_str(),
            self.manifest_revision,
            self._root.root_hash(),
        );
        self._root.git_initialization_resumable(&marker)
    }

    pub(crate) fn git_manifest_revision_is_current(
        &self,
        expected: Revision,
    ) -> Result<bool, GitInitializeStorageError> {
        let bytes = self
            ._root
            .read_regular("rehow.yaml", DESCRIPTOR_LIMIT)
            .map_err(|error| match error {
                linux::EntryReadError::Unsafe
                | linux::EntryReadError::TooLarge
                | linux::EntryReadError::Changed => GitInitializeStorageError::Unsafe,
                linux::EntryReadError::Io(_) => GitInitializeStorageError::Io,
            })?;
        Ok(document_revision("rehow.yaml", &bytes) == expected)
    }

    pub(crate) fn initialize_git_repository(&self) -> Result<(), GitInitializeStorageError> {
        let workspace_id = self.manifest.workspace_id().to_string();
        let marker = git_initialization_marker(
            &workspace_id,
            self.manifest_revision,
            self._root.root_hash(),
        );
        self._root.initialize_git_repository(&marker)
    }
}

pub fn category_inventory_snapshot(workspace: &WorkspaceView) -> Snapshot {
    fn field(hasher: &mut blake3::Hasher, value: &[u8]) {
        hasher.update(&(value.len() as u64).to_be_bytes());
        hasher.update(value);
    }

    let mut categories = workspace.categories.iter().collect::<Vec<_>>();
    categories
        .sort_unstable_by(|left, right| left.relative_directory.cmp(&right.relative_directory));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rehow-category-inventory-snapshot-v1\0");
    field(
        &mut hasher,
        workspace.manifest.workspace_id().to_string().as_bytes(),
    );
    field(
        &mut hasher,
        workspace.manifest_revision.to_string().as_bytes(),
    );
    hasher.update(&(categories.len() as u64).to_be_bytes());
    for category in categories {
        field(&mut hasher, category.relative_directory.as_bytes());
        field(&mut hasher, category.category.id().to_string().as_bytes());
        field(
            &mut hasher,
            category.descriptor_revision.to_string().as_bytes(),
        );
    }
    Snapshot::from_digest_bytes(*hasher.finalize().as_bytes())
}

fn git_initialization_marker(
    workspace_id: &str,
    revision: Revision,
    root_hash: [u8; 32],
) -> Vec<u8> {
    let mut root = String::with_capacity(64);
    for byte in root_hash {
        use std::fmt::Write as _;
        write!(&mut root, "{byte:02x}").expect("writing to String cannot fail");
    }
    format!(
        "rehow-git-init-v1\nworkspace_id={workspace_id}\nmanifest_revision={revision}\nroot={root}\n"
    )
    .into_bytes()
}

impl LoadedCategory {
    pub fn relative_directory(&self) -> &str {
        &self.relative_directory
    }

    pub fn descriptor_revision(&self) -> Revision {
        self.descriptor_revision
    }

    pub fn category(&self) -> &Category {
        &self.category
    }

    pub fn guides(&self) -> &[LoadedGuide] {
        &self.guides
    }
}

impl LoadedGuide {
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn guide(&self) -> &Guide {
        &self.guide
    }

    pub fn effective_status(&self) -> GuideStatus {
        self.effective_status
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceInspectError {
    NotFound,
    NotDirectory,
    PermissionDenied,
    UnsafeRoot,
    ResourceLimit,
    ChangedDuringRead,
    Io,
}

impl fmt::Display for WorkspaceInspectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "Workspace folder was not found",
            Self::NotDirectory => "Workspace selection is not a folder",
            Self::PermissionDenied => "Workspace folder cannot be read",
            Self::UnsafeRoot => "Workspace folder is unsafe to inspect",
            Self::ResourceLimit => "Workspace exceeds a safe inspection limit",
            Self::ChangedDuringRead => "Workspace changed during inspection; try again",
            Self::Io => "Workspace inspection failed",
        })
    }
}

impl std::error::Error for WorkspaceInspectError {}

const DESCRIPTOR_LIMIT: usize = 256 * 1024;
const GUIDE_LIMIT: usize = 2 * 1024 * 1024;
pub(crate) const CATEGORY_LIMIT: usize = 1_000;
pub(crate) const GUIDE_COUNT_LIMIT: usize = 10_000;
pub(crate) const MANAGED_BYTES_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
pub(crate) const ATTACHMENT_COUNT_LIMIT: usize = 10_000;
const ATTACHMENT_FILE_LIMIT: u64 = 64 * 1024 * 1024;
pub(crate) const ATTACHMENT_BYTES_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const ISSUE_LIMIT: usize = 10_000;
const GIT_ANCESTOR_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
pub(crate) enum GitMaterializationPathKind {
    Manifest,
    Category,
    Guide,
    Attachment,
    Ignore,
}

#[allow(dead_code, reason = "connected by the next Phase 5B3 fetch gate")]
pub(crate) fn classify_git_materialization_path(
    relative_path: &str,
) -> Option<(GitMaterializationPathKind, u64)> {
    if relative_path == ".gitignore" {
        return Some((GitMaterializationPathKind::Ignore, 64 * 1024));
    }
    if !recovery::valid_active_path(relative_path) {
        return None;
    }
    if relative_path == "rehow.yaml" {
        Some((
            GitMaterializationPathKind::Manifest,
            DESCRIPTOR_LIMIT as u64,
        ))
    } else if relative_path.ends_with("/category.yaml") {
        Some((
            GitMaterializationPathKind::Category,
            DESCRIPTOR_LIMIT as u64,
        ))
    } else if relative_path.starts_with("categories/") {
        Some((GitMaterializationPathKind::Guide, GUIDE_LIMIT as u64))
    } else {
        Some((
            GitMaterializationPathKind::Attachment,
            ATTACHMENT_FILE_LIMIT,
        ))
    }
}

pub(crate) struct DocumentUpdateRequest {
    relative_path: String,
    expected_revision: Revision,
}

impl DocumentUpdateRequest {
    pub(crate) fn new(relative_path: String, expected_revision: Revision) -> Self {
        Self {
            relative_path,
            expected_revision,
        }
    }

    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub(crate) fn expected_revision(&self) -> Revision {
        self.expected_revision
    }
}

pub fn inspect_workspace(path: &Path) -> Result<WorkspaceInspectionReport, WorkspaceInspectError> {
    inspect_selected_workspace(select_workspace_root(path)?)
}

pub fn select_workspace_root(path: &Path) -> Result<WorkspaceRootSelection, WorkspaceInspectError> {
    let root = linux::DirectoryCapability::open_root(path)?;
    let canonical_path = std::fs::canonicalize(path).map_err(map_io)?;
    if !root.matches_path(&canonical_path)? {
        return Err(WorkspaceInspectError::ChangedDuringRead);
    }
    Ok(WorkspaceRootSelection {
        root,
        canonical_path,
    })
}

pub fn inspect_selected_workspace(
    selection: WorkspaceRootSelection,
) -> Result<WorkspaceInspectionReport, WorkspaceInspectError> {
    let root = selection.root;
    let facts = inspect_folder_facts(&root)?;
    let classification = inspect_open_root(root)?;
    Ok(WorkspaceInspectionReport {
        facts,
        classification,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_update<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_path: &str,
    expected_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    prepare: F,
) -> Result<DocumentUpdateOutcome, DocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    recovery::execute_document_update(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_path,
        expected_revision,
        None,
        operation_id,
        created_at,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_update_cancellable<E, F, C>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_path: &str,
    expected_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    cancelled: &C,
    prepare: F,
) -> Result<DocumentUpdateOutcome, CancellableDocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
    C: Fn() -> bool,
{
    recovery::execute_document_update_cancellable(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_path,
        expected_revision,
        None,
        operation_id,
        created_at,
        cancelled,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_category_document_update<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_path: &str,
    expected_revision: Revision,
    expected_category_snapshot: Snapshot,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    prepare: F,
) -> Result<DocumentUpdateOutcome, DocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    recovery::execute_document_update(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_path,
        expected_revision,
        Some(expected_category_snapshot),
        operation_id,
        created_at,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_attachment_import<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_relative_path: &str,
    guide_id: crate::protocol::GuideId,
    expected_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
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
    recovery::execute_attachment_import(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        guide_relative_path,
        guide_id,
        expected_revision,
        operation_id,
        created_at,
        filename,
        source,
        source_state,
        approved_relative_path,
        approved_revision,
        approved_object_hash,
        approved_sha256,
        approved_size,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn preview_attachment_import<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_relative_path: &str,
    guide_id: crate::protocol::GuideId,
    expected_revision: Revision,
    filename: &str,
    prepare: F,
) -> Result<AttachmentImportPreview, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8], &str) -> Result<Box<[u8]>, E>,
{
    recovery::preview_attachment_import(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        guide_relative_path,
        guide_id,
        expected_revision,
        filename,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn preview_attachment_removal<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
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
    recovery::preview_attachment_removal(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        guide_relative_path,
        expected_guide_revision,
        attachment_relative_path,
        declared_sha256,
        declared_size,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_attachment_removal<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_relative_path: &str,
    expected_guide_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    approved: &AttachmentRemovalPreview,
    prepare: F,
) -> Result<AttachmentRemovalStorageOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    recovery::execute_attachment_removal(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        guide_relative_path,
        expected_guide_revision,
        operation_id,
        created_at,
        approved,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_move<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    source_relative_path: &str,
    destination_relative_path: &str,
    expected_revision: Revision,
    prerequisite_path: &str,
    expected_prerequisite_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    prepare: F,
) -> Result<DocumentMoveOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    recovery::execute_document_move(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        source_relative_path,
        destination_relative_path,
        expected_revision,
        prerequisite_path,
        expected_prerequisite_revision,
        operation_id,
        created_at,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_remove(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_path: &str,
    expected_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
) -> Result<DocumentRemoveOutcome, DocumentRemoveError> {
    recovery::execute_document_remove(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_path,
        expected_revision,
        operation_id,
        created_at,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_multi_document_update<E, F, C>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    expected_category_snapshot: Snapshot,
    cancelled: &C,
    inventory_directory: &str,
    expected_inventory_entries: &[String],
    requests: &[DocumentUpdateRequest],
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    prepare: F,
) -> Result<MultiDocumentUpdateOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[PreparedDocument<'_>]) -> Result<Vec<Box<[u8]>>, E>,
    C: Fn() -> bool,
{
    recovery::execute_multi_document_update(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        expected_category_snapshot,
        cancelled,
        inventory_directory,
        expected_inventory_entries,
        requests,
        operation_id,
        created_at,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_category_remove<C>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_directory: &str,
    expected_revision: Revision,
    expected_category_snapshot: Snapshot,
    cancelled: &C,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
) -> Result<CategoryRemoveStorageOutcome, CategoryRemoveStorageError>
where
    C: Fn() -> bool,
{
    recovery::execute_category_remove(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_directory,
        expected_revision,
        expected_category_snapshot,
        cancelled,
        operation_id,
        created_at,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_workspace_initialize(
    target: &EmptyWorkspaceTarget,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: crate::protocol::WorkspaceId,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    manifest: &[u8],
) -> Result<WorkspaceInitializeOutcome, WorkspaceInitializeError> {
    recovery::execute_workspace_initialize(
        &target.root,
        state_home,
        runtime_directory,
        workspace_id,
        operation_id,
        created_at,
        manifest,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_git_clone_materialization<F>(
    target: &EmptyWorkspaceTarget,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: crate::protocol::WorkspaceId,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    plan_hash: [u8; 32],
    marker: &[u8],
    sources: &[GitCloneMaterializationSource],
    read_file: F,
) -> Result<GitCloneMaterializationStorageOutcome, GitCloneMaterializationStorageError>
where
    F: FnMut(usize, &str, u64) -> Result<Vec<u8>, GitCloneMaterializationStorageError>,
{
    recovery::execute_git_clone_materialization(
        &target.root,
        state_home,
        runtime_directory,
        workspace_id,
        operation_id,
        created_at,
        plan_hash,
        marker,
        sources,
        read_file,
    )
}

#[allow(clippy::too_many_arguments)]
#[allow(
    dead_code,
    reason = "internal Phase 5D2B1 primitive awaiting the atomic metadata publication gate"
)]
pub(crate) fn execute_git_sync_materialization<V, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    plan_hash: [u8; 32],
    current: &[GitSyncMaterializationCurrent],
    incoming: &[GitSyncMaterializationSource],
    validate_current: V,
    read_file: F,
) -> Result<GitSyncMaterializationStorageOutcome, GitSyncMaterializationStorageError>
where
    V: FnOnce() -> Result<(), GitSyncMaterializationStorageError>,
    F: FnMut(usize, &str, u64) -> Result<Vec<u8>, GitSyncMaterializationStorageError>,
{
    recovery::execute_git_sync_materialization(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        operation_id,
        created_at,
        plan_hash,
        current,
        incoming,
        validate_current,
        read_file,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_git_sync_materialization<V, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    plan_hash: [u8; 32],
    current: &[GitSyncMaterializationCurrent],
    incoming: &[GitSyncMaterializationSource],
    validate_current: V,
    read_file: F,
) -> Result<GitSyncPreparedMaterialization, GitSyncMaterializationStorageError>
where
    V: FnOnce() -> Result<(), GitSyncMaterializationStorageError>,
    F: FnMut(usize, &str, u64) -> Result<Vec<u8>, GitSyncMaterializationStorageError>,
{
    recovery::prepare_git_sync_materialization(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        operation_id,
        created_at,
        plan_hash,
        current,
        incoming,
        validate_current,
        read_file,
    )
}

pub(crate) fn activate_git_sync_materialization(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    prepared: GitSyncPreparedMaterialization,
) -> Result<GitSyncMaterializationStorageOutcome, GitSyncMaterializationStorageError> {
    recovery::activate_git_sync_materialization(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        prepared,
    )
}

#[allow(
    dead_code,
    reason = "completed core sync gate awaits the native adapter"
)]
pub(crate) fn activate_git_sync_materialization_plan(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    operation_id: crate::protocol::OperationId,
    plan_hash: [u8; 32],
    journal_hash: Option<[u8; 32]>,
) -> Result<GitSyncMaterializationStorageOutcome, GitSyncMaterializationStorageError> {
    recovery::activate_git_sync_materialization_plan(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        operation_id,
        plan_hash,
        journal_hash,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finalize_git_clone_repository<F>(
    target: &EmptyWorkspaceTarget,
    marker: &[u8],
    completion_seal: &[u8],
    pack_sources: &[GitClonePackSource],
    index_source: &GitClonePackSource,
    config: &[u8],
    head: &[u8],
    shallow: &[u8],
    branch_relative: &str,
    object_id: &[u8],
    open_source: F,
) -> Result<(), GitCloneMaterializationStorageError>
where
    F: FnMut(&GitClonePackSource) -> Result<File, GitCloneMaterializationStorageError>,
{
    target.root.finalize_git_clone_repository(
        marker,
        completion_seal,
        pack_sources,
        index_source,
        config,
        head,
        shallow,
        branch_relative,
        object_id,
        open_source,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn seal_git_clone_repository(
    target: &EmptyWorkspaceTarget,
    marker: &[u8],
    completion_seal: &[u8],
    pack_sources: &[GitClonePackSource],
    index_source: &GitClonePackSource,
    config: &[u8],
    head: &[u8],
    shallow: &[u8],
    branch_relative: &str,
    object_id: &[u8],
) -> Result<(), GitCloneMaterializationStorageError> {
    target.root.seal_git_clone_repository(
        marker,
        completion_seal,
        pack_sources,
        index_source,
        config,
        head,
        shallow,
        branch_relative,
        object_id,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_category_create(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    expected_category_snapshot: Snapshot,
    relative_directory: &str,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    descriptor: &[u8],
) -> Result<CategoryCreateStorageOutcome, CategoryCreateStorageError> {
    recovery::execute_category_create(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        expected_category_snapshot,
        relative_directory,
        operation_id,
        created_at,
        descriptor,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_create(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    prerequisite_path: &str,
    expected_prerequisite_revision: crate::protocol::Revision,
    relative_path: &str,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    content: &[u8],
) -> Result<DocumentCreateOutcome, DocumentCreateError> {
    recovery::execute_document_create(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        prerequisite_path,
        expected_prerequisite_revision,
        relative_path,
        operation_id,
        created_at,
        content,
    )
}

pub(crate) fn open_initialized_workspace(
    target: &EmptyWorkspaceTarget,
) -> Result<Box<WorkspaceView>, WorkspaceInspectError> {
    let root = target.root.duplicate().map_err(map_io)?;
    let WorkspaceInspection::Compatible(workspace) = inspect_open_root(root)? else {
        return Err(WorkspaceInspectError::ChangedDuringRead);
    };
    Ok(workspace)
}

pub(crate) fn refresh_workspace(
    workspace: &mut WorkspaceView,
) -> Result<(), WorkspaceInspectError> {
    let root = workspace._root.duplicate().map_err(map_io)?;
    let WorkspaceInspection::Compatible(refreshed) =
        inspect_open_root_with_index(root, Some(&workspace.search_index))?
    else {
        return Err(WorkspaceInspectError::ChangedDuringRead);
    };
    *workspace = *refreshed;
    Ok(())
}

fn inspect_open_root(
    root: linux::DirectoryCapability,
) -> Result<WorkspaceInspection, WorkspaceInspectError> {
    inspect_open_root_with_index(root, None)
}

fn inspect_open_root_with_index(
    root: linux::DirectoryCapability,
    previous_index: Option<&crate::search::SearchIndex>,
) -> Result<WorkspaceInspection, WorkspaceInspectError> {
    let root_entries = root.entries(20_000)?;
    if root_entries.is_empty() {
        return Ok(WorkspaceInspection::Empty(EmptyWorkspaceTarget { root }));
    }
    if !root_entries.iter().any(|name| name == "rehow.yaml") {
        let mut remaining_entries = 20_000usize.saturating_sub(root_entries.len());
        return Ok(
            if contains_safe_markdown(&root, &root_entries, 4, &mut remaining_entries)? {
                WorkspaceInspection::OrdinaryMarkdown
            } else {
                WorkspaceInspection::UnrelatedNonempty
            },
        );
    }

    let manifest_bytes = match root.read_regular("rehow.yaml", DESCRIPTOR_LIMIT) {
        Ok(bytes) => bytes,
        Err(linux::EntryReadError::Unsafe) => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest,
            });
        }
        Err(linux::EntryReadError::TooLarge) => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest,
            });
        }
        Err(linux::EntryReadError::Changed) => {
            return Err(WorkspaceInspectError::ChangedDuringRead);
        }
        Err(linux::EntryReadError::Io(error)) => return Err(map_io(error)),
    };
    let manifest_revision = document_revision("rehow.yaml", &manifest_bytes);
    let manifest_bytes_len = manifest_bytes.len();
    let manifest = match crate::parsing::parse_yaml_document(manifest_bytes.into_boxed_slice())
        .ok()
        .and_then(|parsed| crate::domain::content::bind_workspace_manifest(parsed).ok())
    {
        Some(crate::domain::content::ManifestBindingOutcome::Current(bound)) => bound.into_record(),
        Some(crate::domain::content::ManifestBindingOutcome::NewerUnsupported {
            version, ..
        }) => return Ok(WorkspaceInspection::NewerUnsupported { version }),
        None => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest,
            });
        }
    };

    let categories_dir = match root.open_directory("categories") {
        Ok(dir) => dir,
        Err(linux::OpenDirectoryError::Missing) => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::CategoriesMissing,
            });
        }
        Err(linux::OpenDirectoryError::Unsafe) => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::CategoriesUnsafe,
            });
        }
        Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
    };

    let mut category_names = categories_dir.entries(CATEGORY_LIMIT)?;
    category_names.sort_unstable();

    let mut managed_bytes =
        u64::try_from(manifest_bytes_len).map_err(|_| WorkspaceInspectError::ResourceLimit)?;
    let mut categories = Vec::new();
    let mut issues = Vec::new();
    let mut category_ids = HashSet::new();
    let mut guide_ids = HashSet::new();
    let mut guide_candidates = 0usize;
    categories
        .try_reserve_exact(category_names.len())
        .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
    issues
        .try_reserve(category_names.len())
        .map_err(|_| WorkspaceInspectError::ResourceLimit)?;

    for category_name in category_names {
        if !valid_category_directory(&category_name) {
            push_issue(
                &mut issues,
                "categories/<unsupported>".to_owned(),
                WorkspaceIssueKind::UnsupportedEntry,
            )?;
            continue;
        }
        let category_dir = match categories_dir.open_directory(&category_name) {
            Ok(dir) => dir,
            Err(linux::OpenDirectoryError::Missing) => continue,
            Err(linux::OpenDirectoryError::Unsafe) => {
                push_issue(
                    &mut issues,
                    format!("categories/{category_name}"),
                    WorkspaceIssueKind::UnsafeEntry,
                )?;
                continue;
            }
            Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
        };
        let descriptor_path = format!("categories/{category_name}/category.yaml");
        let descriptor = match category_dir.read_regular("category.yaml", DESCRIPTOR_LIMIT) {
            Ok(bytes) => bytes,
            Err(linux::EntryReadError::Unsafe | linux::EntryReadError::TooLarge) => {
                push_issue(
                    &mut issues,
                    descriptor_path,
                    WorkspaceIssueKind::MalformedCategory,
                )?;
                continue;
            }
            Err(linux::EntryReadError::Changed) => {
                return Err(WorkspaceInspectError::ChangedDuringRead);
            }
            Err(linux::EntryReadError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                push_issue(
                    &mut issues,
                    descriptor_path,
                    WorkspaceIssueKind::MissingCategoryDescriptor,
                )?;
                continue;
            }
            Err(linux::EntryReadError::Io(error)) => return Err(map_io(error)),
        };
        add_managed_bytes(&mut managed_bytes, descriptor.len())?;
        let descriptor_revision = document_revision(&descriptor_path, &descriptor);
        let bound = match crate::parsing::parse_yaml_document(descriptor.into_boxed_slice())
            .ok()
            .and_then(|parsed| crate::domain::content::bind_category(parsed).ok())
        {
            Some(bound) => bound,
            None => {
                push_issue(
                    &mut issues,
                    descriptor_path,
                    WorkspaceIssueKind::MalformedCategory,
                )?;
                continue;
            }
        };
        category_ids
            .try_reserve(1)
            .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
        if !category_ids.insert(*bound.record().id()) {
            push_issue(
                &mut issues,
                format!("categories/{category_name}/category.yaml"),
                WorkspaceIssueKind::DuplicateCategoryId,
            )?;
            continue;
        }
        let category = bound.into_record();

        let mut guide_names = category_dir.entries(GUIDE_COUNT_LIMIT.saturating_add(1))?;
        guide_names.sort_unstable();
        let mut guides = Vec::new();
        guides
            .try_reserve(guide_names.len().min(128))
            .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
        for guide_name in guide_names {
            if guide_name == "category.yaml" {
                continue;
            }
            if !valid_guide_filename(&guide_name) {
                push_issue(
                    &mut issues,
                    format!("categories/{category_name}/<unsupported>"),
                    WorkspaceIssueKind::UnsupportedEntry,
                )?;
                continue;
            }
            if guide_candidates >= GUIDE_COUNT_LIMIT {
                return Err(WorkspaceInspectError::ResourceLimit);
            }
            guide_candidates += 1;
            let guide_path = format!("categories/{category_name}/{guide_name}");
            let bytes = match category_dir.read_regular(&guide_name, GUIDE_LIMIT) {
                Ok(bytes) => bytes,
                Err(linux::EntryReadError::Unsafe) => {
                    push_issue(&mut issues, guide_path, WorkspaceIssueKind::UnsafeEntry)?;
                    continue;
                }
                Err(linux::EntryReadError::TooLarge) => {
                    push_issue(&mut issues, guide_path, WorkspaceIssueKind::FileTooLarge)?;
                    continue;
                }
                Err(linux::EntryReadError::Changed) => {
                    return Err(WorkspaceInspectError::ChangedDuringRead);
                }
                Err(linux::EntryReadError::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound =>
                {
                    continue;
                }
                Err(linux::EntryReadError::Io(error)) => return Err(map_io(error)),
            };
            add_managed_bytes(&mut managed_bytes, bytes.len())?;
            let revision = document_revision(&guide_path, &bytes);
            let bound_guide = match crate::parsing::parse_guide_source(bytes.into_boxed_slice())
                .ok()
                .and_then(|parsed| crate::domain::content::guide::bind_guide(parsed).ok())
            {
                Some(bound) => bound,
                None => {
                    push_issue(&mut issues, guide_path, WorkspaceIssueKind::MalformedGuide)?;
                    continue;
                }
            };
            if bound_guide.record().category_id() != category.id() {
                push_issue(
                    &mut issues,
                    guide_path,
                    WorkspaceIssueKind::CategoryMismatch,
                )?;
                continue;
            }
            guide_ids
                .try_reserve(1)
                .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
            if !guide_ids.insert(*bound_guide.record().id()) {
                push_issue(
                    &mut issues,
                    guide_path.clone(),
                    WorkspaceIssueKind::DuplicateGuideId,
                )?;
                continue;
            }
            let effective_status = if bound_guide.record().declared_status() == GuideStatus::Tested
            {
                let calculated = crate::domain::content::verification::verification_hash(
                    &bound_guide,
                )
                .map_err(|error| match error {
                    crate::domain::content::verification::VerificationFailure::ResourceLimit => {
                        WorkspaceInspectError::ResourceLimit
                    }
                    crate::domain::content::verification::VerificationFailure::Invariant => {
                        WorkspaceInspectError::Io
                    }
                })?;
                if bound_guide
                    .record()
                    .verification()
                    .and_then(|verification| verification.verified_content_hash())
                    == Some(&calculated)
                {
                    GuideStatus::Tested
                } else {
                    push_issue(
                        &mut issues,
                        guide_path.clone(),
                        WorkspaceIssueKind::VerificationMismatch,
                    )?;
                    GuideStatus::Stale
                }
            } else {
                bound_guide.record().declared_status()
            };
            guides.push(LoadedGuide {
                relative_path: guide_path,
                revision,
                guide: bound_guide.into_record(),
                effective_status,
            });
        }
        guides.sort_by(|left, right| {
            left.guide()
                .title()
                .cmp(right.guide().title())
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        categories.push(LoadedCategory {
            relative_directory: format!("categories/{category_name}"),
            descriptor_revision,
            category,
            guides,
        });
    }

    categories.sort_by(|left, right| {
        left.category()
            .order()
            .cmp(&right.category().order())
            .then_with(|| left.category().name().cmp(right.category().name()))
            .then_with(|| left.relative_directory.cmp(&right.relative_directory))
    });

    inspect_attachments(&root, &categories, &mut issues)?;

    let search_index = crate::search::SearchIndex::reconcile(&categories, previous_index);
    Ok(WorkspaceInspection::Compatible(Box::new(WorkspaceView {
        _root: root,
        manifest,
        manifest_revision,
        categories,
        issues,
        search_index,
    })))
}

struct ExpectedAttachment {
    sha256: String,
    size_bytes: Option<u64>,
}

fn inspect_attachments(
    root: &linux::DirectoryCapability,
    categories: &[LoadedCategory],
    issues: &mut Vec<WorkspaceIssue>,
) -> Result<(), WorkspaceInspectError> {
    let mut expected = HashMap::new();
    let mut known_guide_ids = HashSet::new();
    let mut declared_count = 0usize;
    for category in categories {
        for guide in category.guides() {
            known_guide_ids
                .try_reserve(1)
                .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
            known_guide_ids.insert(*guide.guide().id());
            for attachment in guide.guide().attachments().unwrap_or_default() {
                increment_attachment_count(&mut declared_count)?;
                expected
                    .try_reserve(1)
                    .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
                expected.insert(
                    copy_inspection_text(attachment.path())?,
                    ExpectedAttachment {
                        sha256: copy_inspection_text(attachment.sha256())?,
                        size_bytes: attachment.size_bytes(),
                    },
                );
            }
        }
    }

    let attachments = match root.open_directory("attachments") {
        Ok(directory) => directory,
        Err(linux::OpenDirectoryError::Missing) => {
            push_missing_attachments(&expected, issues)?;
            return Ok(());
        }
        Err(linux::OpenDirectoryError::Unsafe) => {
            push_issue(
                issues,
                "attachments".to_owned(),
                WorkspaceIssueKind::UnsafeEntry,
            )?;
            return Ok(());
        }
        Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
    };

    let mut guide_directories = attachments.entries(ATTACHMENT_COUNT_LIMIT)?;
    guide_directories.sort_unstable();
    let mut candidate_count = 0usize;
    let mut aggregate_bytes = 0u64;
    for guide_directory in guide_directories {
        let Ok(directory_guide_id) = guide_directory.parse::<crate::protocol::GuideId>() else {
            push_issue(
                issues,
                "attachments/<unsupported>".to_owned(),
                WorkspaceIssueKind::UnsupportedEntry,
            )?;
            continue;
        };
        let directory = match attachments.open_directory(&guide_directory) {
            Ok(directory) => directory,
            Err(linux::OpenDirectoryError::Missing) => continue,
            Err(linux::OpenDirectoryError::Unsafe) => {
                push_issue(
                    issues,
                    format!("attachments/{guide_directory}"),
                    WorkspaceIssueKind::UnsafeEntry,
                )?;
                continue;
            }
            Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
        };
        let remaining = ATTACHMENT_COUNT_LIMIT.saturating_sub(candidate_count);
        let mut filenames = directory.entries(remaining)?;
        filenames.sort_unstable();
        if filenames.is_empty() && !known_guide_ids.contains(&directory_guide_id) {
            push_issue(
                issues,
                format!("attachments/{guide_directory}"),
                WorkspaceIssueKind::OrphanAttachment,
            )?;
        }
        for filename in filenames {
            increment_attachment_count(&mut candidate_count)?;
            let relative_path = format!("attachments/{guide_directory}/{filename}");
            if !valid_attachment_filename(&filename) {
                push_issue(
                    issues,
                    format!("attachments/{guide_directory}/<unsupported>"),
                    WorkspaceIssueKind::UnsupportedEntry,
                )?;
                continue;
            }
            let remaining_bytes = ATTACHMENT_BYTES_LIMIT.saturating_sub(aggregate_bytes);
            let streamed = match directory.hash_attachment(
                &filename,
                ATTACHMENT_FILE_LIMIT,
                remaining_bytes,
            ) {
                Ok(streamed) => streamed,
                Err(linux::AttachmentReadError::Unsafe) => {
                    push_issue(issues, relative_path, WorkspaceIssueKind::UnsafeEntry)?;
                    continue;
                }
                Err(linux::AttachmentReadError::TooLarge(size)) => {
                    add_attachment_bytes(&mut aggregate_bytes, size)?;
                    push_issue(issues, relative_path, WorkspaceIssueKind::FileTooLarge)?;
                    continue;
                }
                Err(linux::AttachmentReadError::AggregateLimit) => {
                    return Err(WorkspaceInspectError::ResourceLimit);
                }
                Err(linux::AttachmentReadError::Changed) => {
                    return Err(WorkspaceInspectError::ChangedDuringRead);
                }
                Err(linux::AttachmentReadError::Io(error)) => return Err(map_io(error)),
            };
            add_attachment_bytes(&mut aggregate_bytes, streamed.size)?;
            match expected.remove(&relative_path) {
                Some(metadata) => {
                    if metadata.sha256.as_bytes() != hex_sha256(&streamed.sha256).as_bytes() {
                        push_issue(
                            issues,
                            relative_path.clone(),
                            WorkspaceIssueKind::AttachmentHashMismatch,
                        )?;
                    }
                    if metadata
                        .size_bytes
                        .is_some_and(|declared| declared != streamed.size)
                    {
                        push_issue(
                            issues,
                            relative_path,
                            WorkspaceIssueKind::AttachmentSizeMismatch,
                        )?;
                    }
                }
                None => push_issue(issues, relative_path, WorkspaceIssueKind::OrphanAttachment)?,
            }
        }
    }
    push_missing_attachments(&expected, issues)?;
    Ok(())
}

fn push_missing_attachments(
    expected: &HashMap<String, ExpectedAttachment>,
    issues: &mut Vec<WorkspaceIssue>,
) -> Result<(), WorkspaceInspectError> {
    let mut missing = Vec::new();
    missing
        .try_reserve_exact(expected.len())
        .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
    missing.extend(expected.keys());
    missing.sort_unstable();
    for path in missing {
        push_issue(
            issues,
            copy_inspection_text(path)?,
            WorkspaceIssueKind::MissingAttachment,
        )?;
    }
    Ok(())
}

fn copy_inspection_text(value: &str) -> Result<String, WorkspaceInspectError> {
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
    output.push_str(value);
    Ok(output)
}

fn add_attachment_bytes(total: &mut u64, amount: u64) -> Result<(), WorkspaceInspectError> {
    *total = total
        .checked_add(amount)
        .ok_or(WorkspaceInspectError::ResourceLimit)?;
    if *total > ATTACHMENT_BYTES_LIMIT {
        return Err(WorkspaceInspectError::ResourceLimit);
    }
    Ok(())
}

fn increment_attachment_count(count: &mut usize) -> Result<(), WorkspaceInspectError> {
    if *count >= ATTACHMENT_COUNT_LIMIT {
        return Err(WorkspaceInspectError::ResourceLimit);
    }
    *count += 1;
    Ok(())
}

fn valid_attachment_filename(value: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    !value.is_empty()
        && value.len() <= 255
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', '\0'])
        && value.nfc().eq(value.chars())
        && !value.chars().any(|character| {
            matches!(character as u32, 0x00..=0x1f|0x7f..=0x9f|0x061c|0x200e|0x200f|0x2028..=0x202e|0x2066..=0x2069|0xfeff|0xfdd0..=0xfdef)
                || matches!(character as u32 & 0xffff, 0xfffe | 0xffff)
        })
}

fn hex_sha256(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

fn inspect_folder_facts(
    root: &linux::DirectoryCapability,
) -> Result<WorkspaceFolderFacts, WorkspaceInspectError> {
    let writable = root.writable()?;
    let mut facts = WorkspaceFolderFacts {
        writable,
        existing_git_repository: false,
        nested_git_repository: false,
        git_probe_complete: true,
        unsafe_git_marker: false,
    };
    apply_git_marker(root.git_marker(), true, &mut facts);

    let mut ancestor = match root.open_parent() {
        Ok(value) => value,
        Err(_) => {
            facts.git_probe_complete = false;
            return Ok(facts);
        }
    };
    for _ in 0..GIT_ANCESTOR_LIMIT {
        let Some(directory) = ancestor else {
            return Ok(facts);
        };
        apply_git_marker(directory.git_marker(), false, &mut facts);
        ancestor = match directory.open_parent() {
            Ok(value) => value,
            Err(_) => {
                facts.git_probe_complete = false;
                return Ok(facts);
            }
        };
    }
    if ancestor.is_some() {
        facts.git_probe_complete = false;
    }
    Ok(facts)
}

fn apply_git_marker(
    marker: Result<linux::GitMarker, std::io::Error>,
    selected_root: bool,
    facts: &mut WorkspaceFolderFacts,
) {
    match marker {
        Ok(linux::GitMarker::Missing) => {}
        Ok(linux::GitMarker::Repository) if selected_root => {
            facts.existing_git_repository = true;
        }
        Ok(linux::GitMarker::Repository) => {
            facts.nested_git_repository = true;
        }
        Ok(linux::GitMarker::Unsafe) => {
            facts.unsafe_git_marker = true;
        }
        Err(_) => {
            facts.git_probe_complete = false;
        }
    }
}

fn contains_safe_markdown(
    directory: &linux::DirectoryCapability,
    names: &[String],
    remaining_depth: usize,
    remaining_entries: &mut usize,
) -> Result<bool, WorkspaceInspectError> {
    for name in names {
        if name.ends_with(".md") {
            match directory.read_regular(name, GUIDE_LIMIT) {
                Ok(_) | Err(linux::EntryReadError::TooLarge) => return Ok(true),
                Err(linux::EntryReadError::Unsafe) => {}
                Err(linux::EntryReadError::Changed) => {
                    return Err(WorkspaceInspectError::ChangedDuringRead);
                }
                Err(linux::EntryReadError::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(linux::EntryReadError::Io(error)) => return Err(map_io(error)),
            }
        }
        if remaining_depth == 0 {
            continue;
        }
        let child = match directory.open_directory(name) {
            Ok(child) => child,
            Err(linux::OpenDirectoryError::Missing | linux::OpenDirectoryError::Unsafe) => {
                continue;
            }
            Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
        };
        if *remaining_entries == 0 {
            return Err(WorkspaceInspectError::ResourceLimit);
        }
        let children = child.entries(*remaining_entries)?;
        *remaining_entries = (*remaining_entries).saturating_sub(children.len());
        if contains_safe_markdown(&child, &children, remaining_depth - 1, remaining_entries)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn valid_category_directory(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_guide_filename(value: &str) -> bool {
    value
        .strip_suffix(".md")
        .is_some_and(valid_category_directory)
}

fn current_category_entry_count(
    root: &linux::DirectoryCapability,
) -> Result<usize, WorkspaceInspectError> {
    let categories = match root.open_directory("categories") {
        Ok(directory) => directory,
        Err(linux::OpenDirectoryError::Missing) => {
            return Err(WorkspaceInspectError::ChangedDuringRead);
        }
        Err(linux::OpenDirectoryError::Unsafe) => {
            return Err(WorkspaceInspectError::UnsafeRoot);
        }
        Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
    };
    categories
        .entries(CATEGORY_LIMIT.saturating_add(1))
        .map(|entries| entries.len())
}

fn current_guide_candidate_count(
    root: &linux::DirectoryCapability,
) -> Result<usize, WorkspaceInspectError> {
    let categories = match root.open_directory("categories") {
        Ok(directory) => directory,
        Err(linux::OpenDirectoryError::Missing) => {
            return Err(WorkspaceInspectError::ChangedDuringRead);
        }
        Err(linux::OpenDirectoryError::Unsafe) => {
            return Err(WorkspaceInspectError::UnsafeRoot);
        }
        Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
    };
    let category_names = categories.entries(CATEGORY_LIMIT)?;
    let mut guide_count = 0usize;
    for category_name in category_names {
        if !valid_category_directory(&category_name) {
            continue;
        }
        let category = match categories.open_directory(&category_name) {
            Ok(directory) => directory,
            Err(linux::OpenDirectoryError::Missing) => {
                return Err(WorkspaceInspectError::ChangedDuringRead);
            }
            Err(linux::OpenDirectoryError::Unsafe) => {
                return Err(WorkspaceInspectError::UnsafeRoot);
            }
            Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
        };
        for name in category.entries(GUIDE_COUNT_LIMIT.saturating_add(2))? {
            if name != "category.yaml" && valid_guide_filename(&name) {
                guide_count = guide_count
                    .checked_add(1)
                    .ok_or(WorkspaceInspectError::ResourceLimit)?;
                if guide_count >= GUIDE_COUNT_LIMIT {
                    return Ok(guide_count);
                }
            }
        }
    }
    Ok(guide_count)
}

fn add_managed_bytes(total: &mut u64, amount: usize) -> Result<(), WorkspaceInspectError> {
    *total = total
        .checked_add(u64::try_from(amount).map_err(|_| WorkspaceInspectError::ResourceLimit)?)
        .ok_or(WorkspaceInspectError::ResourceLimit)?;
    if *total > MANAGED_BYTES_LIMIT {
        return Err(WorkspaceInspectError::ResourceLimit);
    }
    Ok(())
}

fn push_issue(
    issues: &mut Vec<WorkspaceIssue>,
    relative_path: String,
    kind: WorkspaceIssueKind,
) -> Result<(), WorkspaceInspectError> {
    if issues.len() >= ISSUE_LIMIT {
        return Err(WorkspaceInspectError::ResourceLimit);
    }
    issues
        .try_reserve(1)
        .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
    issues.push(WorkspaceIssue {
        relative_path,
        kind,
    });
    Ok(())
}

fn document_revision(relative_path: &str, bytes: &[u8]) -> Revision {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rehow-document-revision-v1\0");
    hash_frame(&mut hasher, relative_path.as_bytes());
    hash_frame(&mut hasher, bytes);
    Revision::from_digest_bytes(*hasher.finalize().as_bytes())
}

fn hash_frame(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn map_io(error: std::io::Error) -> WorkspaceInspectError {
    match error.kind() {
        std::io::ErrorKind::NotFound => WorkspaceInspectError::NotFound,
        std::io::ErrorKind::NotADirectory => WorkspaceInspectError::NotDirectory,
        std::io::ErrorKind::PermissionDenied => WorkspaceInspectError::PermissionDenied,
        _ => WorkspaceInspectError::Io,
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{
        GitCloneMaterializationStorageError, GitClonePackSource, GitInitializeStorageError,
        WorkspaceInspectError,
    };
    use crate::protocol::Revision;
    use std::{
        collections::{BTreeMap, BTreeSet},
        ffi::{CStr, CString, OsStr},
        fs::File,
        io::{self, Read, Seek, SeekFrom, Write},
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::Path,
    };

    pub(super) struct DirectoryCapability {
        descriptor: OwnedFd,
        root_hash: [u8; 32],
        capability_hash: [u8; 32],
    }

    pub(super) enum OpenDirectoryError {
        Missing,
        Unsafe,
        Io(io::Error),
    }

    pub(super) enum EntryReadError {
        Unsafe,
        TooLarge,
        Changed,
        Io(io::Error),
    }

    pub(super) enum AttachmentReadError {
        Unsafe,
        TooLarge(u64),
        AggregateLimit,
        Changed,
        Io(io::Error),
    }

    pub(super) struct StreamedAttachment {
        pub(super) size: u64,
        pub(super) sha256: [u8; 32],
    }

    pub(super) enum GitMarker {
        Missing,
        Repository,
        Unsafe,
    }

    pub(super) struct ManagedTarget {
        directory: DirectoryCapability,
        name: String,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub(super) struct RegularState {
        pub(super) bytes: Vec<u8>,
        pub(super) mode: u32,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub(super) enum ManagedState {
        Missing,
        Regular(RegularState),
    }

    #[derive(Debug, Eq, PartialEq)]
    pub(super) enum HashedState {
        Missing,
        Regular {
            hash: [u8; 32],
            size: u64,
            mode: u32,
        },
    }

    #[derive(Debug, Eq, PartialEq)]
    pub(super) enum AttachmentState {
        Missing,
        Regular {
            object_hash: [u8; 32],
            sha256: [u8; 32],
            revision: Revision,
            size: u64,
            mode: u32,
        },
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum MutationIoError {
        Unsafe,
        Conflict,
        Unsupported,
        TooLarge,
        Io,
    }

    #[cfg(test)]
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub(super) enum ManagedFault {
        TempAfterWrite,
        TempAfterFileSync,
        ReusedTempAfterFileSync,
    }

    #[cfg(test)]
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub(crate) enum GitInitFault {
        FileBeforePublication,
        MarkerCreated,
        DirectoriesSynced,
        FilesSynced,
    }

    #[cfg(test)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum GitCloneScaffoldFault {
        Marker,
        Objects,
        Info,
        Pack,
        Refs,
        Heads,
        Remotes,
        Origin,
        Tags,
    }

    #[cfg(test)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum GitCloneFinalizeFault {
        Pack,
        Shallow,
        Config,
        LocalRef,
        RemoteRef,
        Head,
        Seal,
    }

    #[cfg(test)]
    thread_local! {
        static MANAGED_FAULT: std::cell::Cell<Option<ManagedFault>> = const { std::cell::Cell::new(None) };
        static ATTACHMENT_CHANGED_FAULT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static ATTACHMENT_GROWTH_FAULT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static ATTACHMENT_STATE_CHANGED_FAULT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static GIT_INIT_FAULT: std::cell::Cell<Option<GitInitFault>> = const { std::cell::Cell::new(None) };
        static GIT_CLONE_SCAFFOLD_FAULT: std::cell::Cell<Option<GitCloneScaffoldFault>> = const { std::cell::Cell::new(None) };
        static GIT_CLONE_FINALIZE_FAULT: std::cell::Cell<Option<GitCloneFinalizeFault>> = const { std::cell::Cell::new(None) };
    }

    #[cfg(test)]
    pub(crate) fn set_git_init_fault(fault: GitInitFault) {
        GIT_INIT_FAULT.set(Some(fault));
    }

    #[cfg(test)]
    pub(crate) fn set_git_clone_scaffold_fault(fault: GitCloneScaffoldFault) {
        GIT_CLONE_SCAFFOLD_FAULT.set(Some(fault));
    }

    #[cfg(test)]
    fn git_clone_scaffold_fail(point: GitCloneScaffoldFault) -> Result<(), MutationIoError> {
        if GIT_CLONE_SCAFFOLD_FAULT.get() == Some(point) {
            GIT_CLONE_SCAFFOLD_FAULT.set(None);
            Err(MutationIoError::Unsupported)
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(crate) fn set_git_clone_finalize_fault(fault: GitCloneFinalizeFault) {
        GIT_CLONE_FINALIZE_FAULT.set(Some(fault));
    }

    #[cfg(test)]
    fn git_clone_finalize_fail(point: GitCloneFinalizeFault) -> Result<(), MutationIoError> {
        if GIT_CLONE_FINALIZE_FAULT.get() == Some(point) {
            GIT_CLONE_FINALIZE_FAULT.set(None);
            Err(MutationIoError::Unsupported)
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    fn git_init_fail(point: GitInitFault) -> Result<(), GitInitializeStorageError> {
        if GIT_INIT_FAULT.get() == Some(point) {
            GIT_INIT_FAULT.set(None);
            Err(GitInitializeStorageError::Unsupported)
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(super) fn set_attachment_changed_fault() {
        ATTACHMENT_CHANGED_FAULT.set(true);
    }

    #[cfg(test)]
    pub(super) fn set_attachment_growth_fault() {
        ATTACHMENT_GROWTH_FAULT.set(true);
    }

    #[cfg(test)]
    pub(super) fn set_attachment_state_changed_fault() {
        ATTACHMENT_STATE_CHANGED_FAULT.set(true);
    }

    #[cfg(test)]
    pub(super) fn set_managed_fault(fault: ManagedFault) {
        MANAGED_FAULT.set(Some(fault));
    }

    #[cfg(test)]
    fn managed_fail(point: ManagedFault) -> Result<(), MutationIoError> {
        if MANAGED_FAULT.get() == Some(point) {
            MANAGED_FAULT.set(None);
            Err(MutationIoError::Unsupported)
        } else {
            Ok(())
        }
    }

    impl DirectoryCapability {
        pub(super) fn open_root(path: &Path) -> Result<Self, WorkspaceInspectError> {
            let raw_path = CString::new(path.as_os_str().as_bytes())
                .map_err(|_| WorkspaceInspectError::UnsafeRoot)?;
            // SAFETY: `path` is NUL-terminated and valid for the duration of the call.
            let raw = unsafe {
                libc::open(
                    raw_path.as_ptr(),
                    // Root selection may intentionally arrive through a portal or
                    // `/proc/self/fd` symlink. The opened descriptor is authoritative;
                    // canonicalization below must resolve back to this exact inode.
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            };
            if raw < 0 {
                return Err(super::map_io(io::Error::last_os_error()));
            }
            // SAFETY: a successful `open` returns a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            let (device, inode) =
                descriptor_identity(&descriptor).map_err(|_| WorkspaceInspectError::UnsafeRoot)?;
            let canonical = std::fs::canonicalize(path).map_err(super::map_io)?;
            let metadata = std::fs::metadata(&canonical).map_err(super::map_io)?;
            if metadata.dev() != device || metadata.ino() != inode {
                return Err(WorkspaceInspectError::ChangedDuringRead);
            }
            let mut root_hasher = blake3::Hasher::new();
            root_hasher.update(b"rehow-canonical-root-v1\0");
            root_hasher.update(&(canonical.as_os_str().as_bytes().len() as u64).to_be_bytes());
            root_hasher.update(canonical.as_os_str().as_bytes());
            let root_hash = *root_hasher.finalize().as_bytes();
            let mut epoch = [0_u8; 32];
            fill_random(&mut epoch).map_err(|_| WorkspaceInspectError::Io)?;
            let mut capability_hasher = blake3::Hasher::new();
            capability_hasher.update(b"rehow-open-capability-v1\0");
            capability_hasher.update(&root_hash);
            capability_hasher.update(&device.to_be_bytes());
            capability_hasher.update(&inode.to_be_bytes());
            capability_hasher.update(&epoch);
            let capability_hash = *capability_hasher.finalize().as_bytes();
            Ok(Self {
                descriptor,
                root_hash,
                capability_hash,
            })
        }

        pub(super) fn matches_path(&self, path: &Path) -> Result<bool, WorkspaceInspectError> {
            let metadata = std::fs::metadata(path).map_err(super::map_io)?;
            let (device, inode) = self
                .identity()
                .map_err(super::map_clone_destination_error)?;
            Ok(metadata.is_dir() && metadata.dev() == device && metadata.ino() == inode)
        }

        pub(super) fn entries(&self, limit: usize) -> Result<Vec<String>, WorkspaceInspectError> {
            // Reopen `.` so each scan receives an independent directory-stream
            // offset while remaining confined to the retained capability.
            // SAFETY: the descriptor and static component are valid.
            let duplicate = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c".".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if duplicate < 0 {
                return Err(super::map_io(io::Error::last_os_error()));
            }
            // SAFETY: `duplicate` is a newly owned directory descriptor; `fdopendir`
            // consumes it on success.
            let raw_stream = unsafe { libc::fdopendir(duplicate) };
            if raw_stream.is_null() {
                // SAFETY: `fdopendir` did not consume the descriptor on failure.
                unsafe { libc::close(duplicate) };
                return Err(super::map_io(io::Error::last_os_error()));
            }
            let stream = DirectoryStream(raw_stream);
            let mut names = Vec::new();
            loop {
                // SAFETY: this thread exclusively owns `stream` until `closedir`.
                unsafe { *libc::__errno_location() = 0 };
                // SAFETY: `stream` is valid and open.
                let entry = unsafe { libc::readdir(stream.0) };
                if entry.is_null() {
                    // SAFETY: Linux exposes thread-local errno through this pointer.
                    let errno = unsafe { *libc::__errno_location() };
                    if errno != 0 {
                        return Err(super::map_io(io::Error::from_raw_os_error(errno)));
                    }
                    break;
                }
                // SAFETY: `d_name` is a NUL-terminated array owned by `stream` until
                // the next `readdir`; it is copied before then.
                let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if bytes == b"." || bytes == b".." {
                    continue;
                }
                if names.len() >= limit {
                    return Err(WorkspaceInspectError::ResourceLimit);
                }
                let name =
                    std::str::from_utf8(bytes).map_err(|_| WorkspaceInspectError::UnsafeRoot)?;
                names
                    .try_reserve(1)
                    .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
                names.push(name.to_owned());
            }
            Ok(names)
        }

        pub(super) fn writable(&self) -> Result<bool, WorkspaceInspectError> {
            // SAFETY: the retained descriptor is valid and `.` is a static,
            // NUL-terminated relative component.
            let result = unsafe {
                libc::faccessat(
                    self.descriptor.as_raw_fd(),
                    c".".as_ptr(),
                    libc::W_OK | libc::X_OK,
                    libc::AT_EACCESS,
                )
            };
            if result == 0 {
                return Ok(true);
            }
            let error = io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::EACCES) | Some(libc::EROFS) => Ok(false),
                _ => Err(super::map_io(error)),
            }
        }

        pub(super) fn git_marker(&self) -> Result<GitMarker, io::Error> {
            // `O_PATH | O_NOFOLLOW` obtains metadata for the directory entry
            // itself, including a symlink, without opening its target.
            // SAFETY: the descriptor is valid and `.git` is static and NUL-terminated.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c".git".as_ptr(),
                    libc::O_PATH | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(libc::ENOENT) => Ok(GitMarker::Missing),
                    _ => Err(error),
                };
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            let file = unsafe { File::from_raw_fd(raw) };
            let metadata = file.metadata()?;
            if metadata.file_type().is_dir() {
                return Ok(GitMarker::Repository);
            }
            if !metadata.file_type().is_file() || metadata.nlink() != 1 {
                return Ok(GitMarker::Unsafe);
            }
            drop(file);
            match self.read_regular(".git", 4 * 1024) {
                Ok(bytes) if valid_gitdir_file(&bytes) => Ok(GitMarker::Repository),
                Ok(_)
                | Err(
                    EntryReadError::Unsafe | EntryReadError::TooLarge | EntryReadError::Changed,
                ) => Ok(GitMarker::Unsafe),
                Err(EntryReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                    Ok(GitMarker::Missing)
                }
                Err(EntryReadError::Io(error)) => Err(error),
            }
        }

        pub(super) fn open_parent(&self) -> Result<Option<Self>, io::Error> {
            // SAFETY: the descriptor is valid and `..` is static and NUL-terminated.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c"..".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            let parent = unsafe { OwnedFd::from_raw_fd(raw) };
            let current_identity = descriptor_identity(&self.descriptor)?;
            let parent_identity = descriptor_identity(&parent)?;
            if current_identity == parent_identity {
                Ok(None)
            } else {
                Ok(Some(Self {
                    descriptor: parent,
                    root_hash: self.root_hash,
                    capability_hash: self.capability_hash,
                }))
            }
        }

        pub(super) fn open_directory(&self, name: &str) -> Result<Self, OpenDirectoryError> {
            let name = component(name).map_err(|_| OpenDirectoryError::Unsafe)?;
            // SAFETY: the directory descriptor and NUL-terminated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(libc::ENOENT) => Err(OpenDirectoryError::Missing),
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(OpenDirectoryError::Unsafe),
                    _ => Err(OpenDirectoryError::Io(error)),
                };
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            Ok(Self {
                descriptor: unsafe { OwnedFd::from_raw_fd(raw) },
                root_hash: self.root_hash,
                capability_hash: self.capability_hash,
            })
        }

        pub(super) fn read_regular(
            &self,
            name: &str,
            limit: usize,
        ) -> Result<Vec<u8>, EntryReadError> {
            self.read_regular_state(name, limit)
                .map(|state| state.bytes)
        }

        pub(super) fn hash_attachment(
            &self,
            name: &str,
            file_limit: u64,
            aggregate_remaining: u64,
        ) -> Result<StreamedAttachment, AttachmentReadError> {
            use sha2::Digest;

            let name = component(name).map_err(|_| AttachmentReadError::Unsafe)?;
            // SAFETY: the directory descriptor and NUL-terminated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(libc::ELOOP) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
                        Err(AttachmentReadError::Unsafe)
                    }
                    _ => Err(AttachmentReadError::Io(error)),
                };
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            let metadata = file.metadata().map_err(AttachmentReadError::Io)?;
            if !metadata.file_type().is_file() || metadata.nlink() != 1 {
                return Err(AttachmentReadError::Unsafe);
            }
            if metadata.len() > aggregate_remaining {
                return Err(AttachmentReadError::AggregateLimit);
            }
            if metadata.len() > file_limit {
                return Err(AttachmentReadError::TooLarge(metadata.len()));
            }

            let mut hasher = sha2::Sha256::new();
            let mut buffer = [0_u8; 64 * 1024];
            let mut size = 0_u64;
            loop {
                let remaining_file = file_limit.saturating_sub(size).saturating_add(1);
                let remaining_aggregate =
                    aggregate_remaining.saturating_sub(size).saturating_add(1);
                let read_limit = usize::try_from(
                    remaining_file
                        .min(remaining_aggregate)
                        .min(buffer.len() as u64),
                )
                .map_err(|_| AttachmentReadError::TooLarge(size))?;
                let read = file
                    .read(&mut buffer[..read_limit])
                    .map_err(AttachmentReadError::Io)?;
                if read == 0 {
                    break;
                }
                size = size
                    .checked_add(read as u64)
                    .ok_or(AttachmentReadError::Changed)?;
                #[cfg(test)]
                if ATTACHMENT_GROWTH_FAULT.replace(false) {
                    size = file_limit.saturating_add(1);
                }
                if size > file_limit || size > aggregate_remaining {
                    return Err(AttachmentReadError::Changed);
                }
                hasher.update(&buffer[..read]);
            }

            #[cfg(test)]
            if ATTACHMENT_CHANGED_FAULT.replace(false) {
                return Err(AttachmentReadError::Changed);
            }

            let final_metadata = file.metadata().map_err(AttachmentReadError::Io)?;
            if !final_metadata.file_type().is_file() || final_metadata.nlink() != 1 {
                return Err(AttachmentReadError::Unsafe);
            }
            if metadata.dev() != final_metadata.dev()
                || metadata.ino() != final_metadata.ino()
                || metadata.len() != final_metadata.len()
                || metadata.mtime() != final_metadata.mtime()
                || metadata.mtime_nsec() != final_metadata.mtime_nsec()
                || metadata.ctime() != final_metadata.ctime()
                || metadata.ctime_nsec() != final_metadata.ctime_nsec()
                || metadata.mode() != final_metadata.mode()
                || size != final_metadata.len()
            {
                return Err(AttachmentReadError::Changed);
            }
            Ok(StreamedAttachment {
                size,
                sha256: hasher.finalize().into(),
            })
        }

        fn read_regular_state(
            &self,
            name: &str,
            limit: usize,
        ) -> Result<RegularState, EntryReadError> {
            let name = component(name).map_err(|_| EntryReadError::Unsafe)?;
            // SAFETY: the directory descriptor and NUL-terminated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(libc::ELOOP) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
                        Err(EntryReadError::Unsafe)
                    }
                    _ => Err(EntryReadError::Io(error)),
                };
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            let metadata = file.metadata().map_err(EntryReadError::Io)?;
            if !metadata.file_type().is_file() || metadata.nlink() != 1 {
                return Err(EntryReadError::Unsafe);
            }
            if metadata.len() > limit as u64 {
                return Err(EntryReadError::TooLarge);
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(metadata.len() as usize)
                .map_err(|_| EntryReadError::TooLarge)?;
            Read::by_ref(&mut file)
                .take((limit as u64).saturating_add(1))
                .read_to_end(&mut bytes)
                .map_err(EntryReadError::Io)?;
            if bytes.len() > limit {
                return Err(EntryReadError::TooLarge);
            }
            let final_metadata = file.metadata().map_err(EntryReadError::Io)?;
            if !final_metadata.file_type().is_file() || final_metadata.nlink() != 1 {
                return Err(EntryReadError::Unsafe);
            }
            if metadata.dev() != final_metadata.dev()
                || metadata.ino() != final_metadata.ino()
                || metadata.len() != final_metadata.len()
                || metadata.mtime() != final_metadata.mtime()
                || metadata.mtime_nsec() != final_metadata.mtime_nsec()
                || metadata.ctime() != final_metadata.ctime()
                || metadata.ctime_nsec() != final_metadata.ctime_nsec()
                || metadata.mode() != final_metadata.mode()
            {
                return Err(EntryReadError::Changed);
            }
            Ok(RegularState {
                bytes,
                mode: final_metadata.mode() & 0o7777,
            })
        }

        pub(super) fn open_managed_target(
            &self,
            relative_path: &str,
        ) -> Result<ManagedTarget, MutationIoError> {
            if !super::recovery::valid_active_path(relative_path) {
                return Err(MutationIoError::Unsafe);
            }
            let mut parts = relative_path.split('/').peekable();
            let mut directory = self.duplicate().map_err(|_| MutationIoError::Io)?;
            while let Some(part) = parts.next() {
                if parts.peek().is_none() {
                    return Ok(ManagedTarget {
                        directory,
                        name: part.to_owned(),
                    });
                }
                directory = directory
                    .open_directory(part)
                    .map_err(|error| match error {
                        OpenDirectoryError::Missing => MutationIoError::Conflict,
                        OpenDirectoryError::Unsafe => MutationIoError::Unsafe,
                        OpenDirectoryError::Io(_) => MutationIoError::Io,
                    })?;
            }
            Err(MutationIoError::Unsafe)
        }

        pub(super) fn verify_empty_workspace_root(&self) -> Result<(), MutationIoError> {
            let entries = self.entries(20_000).map_err(|error| match error {
                WorkspaceInspectError::ResourceLimit => MutationIoError::TooLarge,
                WorkspaceInspectError::UnsafeRoot => MutationIoError::Unsafe,
                WorkspaceInspectError::ChangedDuringRead => MutationIoError::Conflict,
                WorkspaceInspectError::NotFound
                | WorkspaceInspectError::NotDirectory
                | WorkspaceInspectError::PermissionDenied
                | WorkspaceInspectError::Io => MutationIoError::Io,
            })?;
            if !entries.is_empty() {
                return Err(MutationIoError::Conflict);
            }
            Ok(())
        }

        pub(super) fn create_categories_directory(&self) -> Result<(), MutationIoError> {
            self.create_private_directory_exclusive("categories")?;
            Ok(())
        }

        pub(super) fn create_git_clone_scaffold(
            &self,
            marker: &[u8],
        ) -> Result<(), MutationIoError> {
            let git_dir = self.create_private_directory_exclusive(".git")?;
            git_dir
                .create_or_verify_private_file("rehow-clone", marker)
                .map_err(map_clone_materialization_git)?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Marker)?;
            let objects = git_dir.create_private_directory_exclusive("objects")?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Objects)?;
            let info = objects.create_private_directory_exclusive("info")?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Info)?;
            let pack = objects.create_private_directory_exclusive("pack")?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Pack)?;
            let refs = git_dir.create_private_directory_exclusive("refs")?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Refs)?;
            let heads = refs.create_private_directory_exclusive("heads")?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Heads)?;
            let remotes = refs.create_private_directory_exclusive("remotes")?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Remotes)?;
            let origin = remotes.create_private_directory_exclusive("origin")?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Origin)?;
            let tags = refs.create_private_directory_exclusive("tags")?;
            #[cfg(test)]
            git_clone_scaffold_fail(GitCloneScaffoldFault::Tags)?;
            for directory in [&info, &pack, &heads, &origin, &tags] {
                if !directory
                    .entries(1)
                    .map_err(map_clone_materialization_inspect)?
                    .is_empty()
                {
                    return Err(MutationIoError::Unsafe);
                }
                directory
                    .sync_directory()
                    .map_err(map_clone_materialization_git)?;
            }
            let remote_entries = remotes
                .entries(2)
                .map_err(map_clone_materialization_inspect)?;
            if remote_entries.len() != 1 || !remote_entries.iter().any(|entry| entry == "origin") {
                return Err(MutationIoError::Unsafe);
            }
            let object_entries = objects
                .entries(3)
                .map_err(map_clone_materialization_inspect)?;
            if object_entries.len() != 2
                || ["info", "pack"]
                    .iter()
                    .any(|required| !object_entries.iter().any(|entry| entry == required))
            {
                return Err(MutationIoError::Unsafe);
            }
            let ref_entries = refs.entries(4).map_err(map_clone_materialization_inspect)?;
            if ref_entries.len() != 3
                || ["heads", "remotes", "tags"]
                    .iter()
                    .any(|required| !ref_entries.iter().any(|entry| entry == required))
            {
                return Err(MutationIoError::Unsafe);
            }
            objects
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            remotes
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            refs.sync_directory()
                .map_err(map_clone_materialization_git)?;
            git_dir
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            self.sync_directory()
                .map_err(map_clone_materialization_git)?;
            self.verify_git_clone_scaffold(marker)
        }

        pub(super) fn resume_git_clone_scaffold(
            &self,
            marker: &[u8],
        ) -> Result<(), MutationIoError> {
            let git_dir = self.open_private_directory(".git")?;
            verify_allowed_entries(&git_dir, &["objects", "refs", "rehow-clone"])?;
            let entries = git_dir
                .entries(4)
                .map_err(map_clone_materialization_inspect)?;
            if !entries.iter().any(|entry| entry == "rehow-clone") {
                return Err(MutationIoError::Unsafe);
            }
            git_dir
                .verify_private_file("rehow-clone", marker)
                .map_err(map_clone_materialization_git)?;

            if let Some(objects) = git_dir.optional_private_directory("objects")? {
                verify_allowed_entries(&objects, &["info", "pack"])?;
                if let Some(info) = objects.optional_private_directory("info")? {
                    verify_exact_entries(&info, &[])?;
                }
                if let Some(pack) = objects.optional_private_directory("pack")? {
                    verify_exact_entries(&pack, &[])?;
                }
            }
            if let Some(refs) = git_dir.optional_private_directory("refs")? {
                verify_allowed_entries(&refs, &["heads", "remotes", "tags"])?;
                if let Some(heads) = refs.optional_private_directory("heads")? {
                    verify_exact_entries(&heads, &[])?;
                }
                if let Some(remotes) = refs.optional_private_directory("remotes")? {
                    verify_allowed_entries(&remotes, &["origin"])?;
                    if let Some(origin) = remotes.optional_private_directory("origin")? {
                        verify_exact_entries(&origin, &[])?;
                    }
                }
                if let Some(tags) = refs.optional_private_directory("tags")? {
                    verify_exact_entries(&tags, &[])?;
                }
            }

            let objects = git_dir.ensure_private_directory("objects")?;
            let info = objects.ensure_private_directory("info")?;
            let pack = objects.ensure_private_directory("pack")?;
            let refs = git_dir.ensure_private_directory("refs")?;
            let heads = refs.ensure_private_directory("heads")?;
            let remotes = refs.ensure_private_directory("remotes")?;
            let origin = remotes.ensure_private_directory("origin")?;
            let tags = refs.ensure_private_directory("tags")?;
            for directory in [&info, &pack, &heads, &origin, &tags] {
                directory
                    .sync_directory()
                    .map_err(map_clone_materialization_git)?;
            }
            objects
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            remotes
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            refs.sync_directory()
                .map_err(map_clone_materialization_git)?;
            git_dir
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            self.sync_directory()
                .map_err(map_clone_materialization_git)?;
            self.verify_git_clone_scaffold(marker)
        }

        pub(super) fn verify_git_clone_scaffold(
            &self,
            marker: &[u8],
        ) -> Result<(), MutationIoError> {
            let git_dir = self.open_private_directory(".git")?;
            git_dir
                .verify_private_file("rehow-clone", marker)
                .map_err(map_clone_materialization_git)?;
            verify_exact_entries(&git_dir, &["objects", "refs", "rehow-clone"])?;
            let objects = git_dir.open_private_directory("objects")?;
            verify_exact_entries(&objects, &["info", "pack"])?;
            let info = objects.open_private_directory("info")?;
            let pack = objects.open_private_directory("pack")?;
            verify_exact_entries(&info, &[])?;
            verify_exact_entries(&pack, &[])?;
            let refs = git_dir.open_private_directory("refs")?;
            verify_exact_entries(&refs, &["heads", "remotes", "tags"])?;
            let heads = refs.open_private_directory("heads")?;
            let remotes = refs.open_private_directory("remotes")?;
            let tags = refs.open_private_directory("tags")?;
            verify_exact_entries(&heads, &[])?;
            verify_exact_entries(&remotes, &["origin"])?;
            verify_exact_entries(&tags, &[])?;
            let origin = remotes.open_private_directory("origin")?;
            verify_exact_entries(&origin, &[])
        }

        pub(super) fn verify_git_clone_inventory(
            &self,
            marker: &[u8],
            expected_paths: &[String],
            temp_names: &[String],
            complete: bool,
            exact_scaffold: bool,
        ) -> Result<(), MutationIoError> {
            if expected_paths.len() != temp_names.len() {
                return Err(MutationIoError::Unsafe);
            }
            if exact_scaffold {
                self.verify_git_clone_scaffold(marker)?;
            } else {
                let git = self.open_private_directory(".git")?;
                git.verify_private_file("rehow-clone", marker)
                    .map_err(map_clone_materialization_git)?;
            }

            let mut child_directories = BTreeMap::<String, BTreeSet<String>>::new();
            let mut final_files = BTreeMap::<String, BTreeSet<String>>::new();
            let mut temporary_files = BTreeMap::<String, BTreeSet<String>>::new();
            child_directories
                .entry(String::new())
                .or_default()
                .insert("categories".to_owned());
            child_directories
                .entry("categories".to_owned())
                .or_default();
            for (path, temp_name) in expected_paths.iter().zip(temp_names) {
                if !super::recovery::valid_active_path(path) {
                    return Err(MutationIoError::Unsafe);
                }
                let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
                final_files
                    .entry(parent.to_owned())
                    .or_default()
                    .insert(name.to_owned());
                temporary_files
                    .entry(parent.to_owned())
                    .or_default()
                    .insert(temp_name.to_owned());

                let mut prior = String::new();
                for part in parent.split('/').filter(|part| !part.is_empty()) {
                    child_directories
                        .entry(prior.clone())
                        .or_default()
                        .insert(part.to_owned());
                    prior = if prior.is_empty() {
                        part.to_owned()
                    } else {
                        format!("{prior}/{part}")
                    };
                    child_directories.entry(prior.clone()).or_default();
                }
            }

            let mut directory_paths = child_directories.keys().cloned().collect::<Vec<_>>();
            directory_paths.sort_by_key(|path| path.matches('/').count());
            for path in directory_paths {
                let directory = if path.is_empty() {
                    self.duplicate().map_err(|_| MutationIoError::Io)?
                } else {
                    match self.open_git_clone_directory_path(&path) {
                        Ok(directory) => directory,
                        Err(MutationIoError::Conflict) if !complete => continue,
                        Err(error) => return Err(error),
                    }
                };
                let mut allowed = child_directories.get(&path).cloned().unwrap_or_default();
                allowed.extend(final_files.get(&path).into_iter().flatten().cloned());
                if !complete {
                    allowed.extend(temporary_files.get(&path).into_iter().flatten().cloned());
                }
                if path.is_empty() {
                    allowed.insert(".git".to_owned());
                }
                let actual = directory
                    .entries(super::recovery::MAX_TARGETS)
                    .map_err(map_clone_materialization_inspect)?
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                if (complete && actual != allowed)
                    || (!complete && !actual.is_subset(&allowed))
                    || (path.is_empty() && !actual.contains(".git"))
                {
                    return Err(MutationIoError::Conflict);
                }
            }
            Ok(())
        }

        pub(super) fn read_git_clone_file(
            &self,
            relative_path: &str,
            expected_size: u64,
        ) -> Result<Vec<u8>, MutationIoError> {
            if !super::recovery::valid_active_path(relative_path) {
                return Err(MutationIoError::Unsafe);
            }
            let limit = usize::try_from(expected_size).map_err(|_| MutationIoError::TooLarge)?;
            let (parent, name) = relative_path
                .rsplit_once('/')
                .unwrap_or(("", relative_path));
            let directory = if parent.is_empty() {
                self.duplicate().map_err(|_| MutationIoError::Io)?
            } else {
                self.open_git_clone_directory_path(parent)?
            };
            let state = directory
                .read_regular_state(name, limit)
                .map_err(|error| match error {
                    EntryReadError::Unsafe | EntryReadError::Changed => MutationIoError::Unsafe,
                    EntryReadError::TooLarge => MutationIoError::TooLarge,
                    EntryReadError::Io(_) => MutationIoError::Io,
                })?;
            if state.mode != 0o600 || state.bytes.len() as u64 != expected_size {
                return Err(MutationIoError::Conflict);
            }
            Ok(state.bytes)
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn finalize_git_clone_repository<F>(
            &self,
            marker: &[u8],
            completion_seal: &[u8],
            pack_sources: &[GitClonePackSource],
            index_source: &GitClonePackSource,
            config: &[u8],
            head: &[u8],
            shallow: &[u8],
            branch_relative: &str,
            object_id: &[u8],
            mut open_source: F,
        ) -> Result<(), GitCloneMaterializationStorageError>
        where
            F: FnMut(&GitClonePackSource) -> Result<File, GitCloneMaterializationStorageError>,
        {
            if pack_sources.is_empty()
                || pack_sources.len() > 128
                || config.len() > 64 * 1024
                || head.len() > 1024
                || shallow.len() > 1024
                || completion_seal.len() > 16 * 1024
                || index_source.name != "index"
                || index_source.size > 32 * 1024 * 1024
                || !valid_clone_branch_path(branch_relative)
                || !matches!(object_id.len(), 40 | 64)
                || !object_id
                    .iter()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
            {
                return Err(GitCloneMaterializationStorageError::Unsafe);
            }
            let mut previous: Option<&str> = None;
            for source in pack_sources {
                if previous.is_some_and(|name| name >= source.name.as_str())
                    || !valid_clone_pack_name(&source.name, object_id.len())
                    || source.size > 7 * 1024 * 1024 * 1024
                {
                    return Err(GitCloneMaterializationStorageError::Unsafe);
                }
                previous = Some(&source.name);
            }

            self.verify_git_clone_finalization_inventory(
                marker,
                completion_seal,
                pack_sources,
                index_source,
                config,
                head,
                shallow,
                branch_relative,
                object_id,
                false,
                false,
            )
            .map_err(map_clone_finalization_io)?;
            let git = self
                .open_private_directory(".git")
                .map_err(map_clone_finalization_io)?;
            let objects = git
                .open_private_directory("objects")
                .map_err(map_clone_finalization_io)?;
            let pack = objects
                .open_private_directory("pack")
                .map_err(map_clone_finalization_io)?;
            for source in pack_sources {
                let mut input = open_source(source)?;
                pack.create_or_verify_private_streamed_file(
                    &source.name,
                    &mut input,
                    source.size,
                    &source.hash,
                )
                .map_err(map_clone_finalization_io)?;
                #[cfg(test)]
                git_clone_finalize_fail(GitCloneFinalizeFault::Pack)
                    .map_err(map_clone_finalization_io)?;
            }
            let mut index = open_source(index_source)?;
            git.create_or_verify_private_streamed_file(
                "index",
                &mut index,
                index_source.size,
                &index_source.hash,
            )
            .map_err(map_clone_finalization_io)?;
            git.create_or_verify_private_file("shallow", shallow)
                .map_err(map_clone_materialization_git)
                .map_err(map_clone_finalization_io)?;
            #[cfg(test)]
            git_clone_finalize_fail(GitCloneFinalizeFault::Shallow)
                .map_err(map_clone_finalization_io)?;
            git.create_or_verify_private_file("config", config)
                .map_err(map_clone_materialization_git)
                .map_err(map_clone_finalization_io)?;
            #[cfg(test)]
            git_clone_finalize_fail(GitCloneFinalizeFault::Config)
                .map_err(map_clone_finalization_io)?;

            let refs = git
                .open_private_directory("refs")
                .map_err(map_clone_finalization_io)?;
            let heads = refs
                .open_private_directory("heads")
                .map_err(map_clone_finalization_io)?;
            let remotes = refs
                .open_private_directory("remotes")
                .map_err(map_clone_finalization_io)?;
            let origin = remotes
                .open_private_directory("origin")
                .map_err(map_clone_finalization_io)?;
            install_clone_ref(&heads, branch_relative, object_id)
                .map_err(map_clone_finalization_io)?;
            #[cfg(test)]
            git_clone_finalize_fail(GitCloneFinalizeFault::LocalRef)
                .map_err(map_clone_finalization_io)?;
            install_clone_ref(&origin, branch_relative, object_id)
                .map_err(map_clone_finalization_io)?;
            #[cfg(test)]
            git_clone_finalize_fail(GitCloneFinalizeFault::RemoteRef)
                .map_err(map_clone_finalization_io)?;
            git.create_or_verify_private_file("HEAD", head)
                .map_err(map_clone_materialization_git)
                .map_err(map_clone_finalization_io)?;
            #[cfg(test)]
            git_clone_finalize_fail(GitCloneFinalizeFault::Head)
                .map_err(map_clone_finalization_io)?;
            self.verify_git_clone_finalization_inventory(
                marker,
                completion_seal,
                pack_sources,
                index_source,
                config,
                head,
                shallow,
                branch_relative,
                object_id,
                true,
                false,
            )
            .map_err(map_clone_finalization_io)
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn seal_git_clone_repository(
            &self,
            marker: &[u8],
            completion_seal: &[u8],
            pack_sources: &[GitClonePackSource],
            index_source: &GitClonePackSource,
            config: &[u8],
            head: &[u8],
            shallow: &[u8],
            branch_relative: &str,
            object_id: &[u8],
        ) -> Result<(), GitCloneMaterializationStorageError> {
            self.verify_git_clone_finalization_inventory(
                marker,
                completion_seal,
                pack_sources,
                index_source,
                config,
                head,
                shallow,
                branch_relative,
                object_id,
                true,
                false,
            )
            .map_err(map_clone_finalization_io)?;
            let git = self
                .open_private_directory(".git")
                .map_err(map_clone_finalization_io)?;
            git.create_or_verify_private_file("rehow-clone-complete", completion_seal)
                .map_err(map_clone_materialization_git)
                .map_err(map_clone_finalization_io)?;
            #[cfg(test)]
            git_clone_finalize_fail(GitCloneFinalizeFault::Seal)
                .map_err(map_clone_finalization_io)?;
            self.verify_git_clone_finalization_inventory(
                marker,
                completion_seal,
                pack_sources,
                index_source,
                config,
                head,
                shallow,
                branch_relative,
                object_id,
                true,
                true,
            )
            .map_err(map_clone_finalization_io)
        }

        #[allow(clippy::too_many_arguments)]
        fn verify_git_clone_finalization_inventory(
            &self,
            marker: &[u8],
            completion_seal: &[u8],
            pack_sources: &[GitClonePackSource],
            index_source: &GitClonePackSource,
            config: &[u8],
            head: &[u8],
            shallow: &[u8],
            branch_relative: &str,
            object_id: &[u8],
            metadata_complete: bool,
            seal_required: bool,
        ) -> Result<(), MutationIoError> {
            let git = self.open_private_directory(".git")?;
            let allowed = BTreeSet::from([
                "HEAD".to_owned(),
                "config".to_owned(),
                "index".to_owned(),
                "objects".to_owned(),
                "refs".to_owned(),
                "rehow-clone".to_owned(),
                "rehow-clone-complete".to_owned(),
                "shallow".to_owned(),
            ]);
            let actual = git
                .entries(allowed.len().saturating_add(1))
                .map_err(map_clone_materialization_inspect)?
                .into_iter()
                .collect::<BTreeSet<_>>();
            let required = ["objects", "refs", "rehow-clone"];
            let mut metadata_without_seal = allowed.clone();
            metadata_without_seal.remove("rehow-clone-complete");
            if (metadata_complete && actual != metadata_without_seal && actual != allowed)
                || (!metadata_complete && !actual.is_subset(&allowed))
                || (seal_required && actual != allowed)
                || required.iter().any(|name| !actual.contains(*name))
            {
                return Err(MutationIoError::Conflict);
            }
            git.verify_private_file("rehow-clone", marker)
                .map_err(map_clone_materialization_git)?;
            for (name, expected) in [
                ("HEAD", head),
                ("config", config),
                ("shallow", shallow),
                ("rehow-clone-complete", completion_seal),
            ] {
                if actual.contains(name) {
                    git.verify_private_file(name, expected)
                        .map_err(map_clone_materialization_git)?;
                } else if metadata_complete && (name != "rehow-clone-complete" || seal_required) {
                    return Err(MutationIoError::Conflict);
                }
            }
            if actual.contains("index") {
                git.verify_private_file_hash("index", index_source.size, &index_source.hash)?;
            } else if metadata_complete {
                return Err(MutationIoError::Conflict);
            }

            let objects = git.open_private_directory("objects")?;
            verify_exact_entries(&objects, &["info", "pack"])?;
            let info = objects.open_private_directory("info")?;
            verify_exact_entries(&info, &[])?;
            let pack = objects.open_private_directory("pack")?;
            let expected_pack = pack_sources
                .iter()
                .map(|source| source.name.clone())
                .collect::<BTreeSet<_>>();
            let actual_pack = pack
                .entries(expected_pack.len().saturating_add(1))
                .map_err(map_clone_materialization_inspect)?
                .into_iter()
                .collect::<BTreeSet<_>>();
            if (metadata_complete && actual_pack != expected_pack)
                || (!metadata_complete && !actual_pack.is_subset(&expected_pack))
            {
                return Err(MutationIoError::Conflict);
            }
            for name in &actual_pack {
                let source = pack_sources
                    .iter()
                    .find(|source| &source.name == name)
                    .ok_or(MutationIoError::Unsafe)?;
                pack.verify_private_file_hash(name, source.size, &source.hash)?;
            }

            let refs = git.open_private_directory("refs")?;
            verify_exact_entries(&refs, &["heads", "remotes", "tags"])?;
            let tags = refs.open_private_directory("tags")?;
            verify_exact_entries(&tags, &[])?;
            let remotes = refs.open_private_directory("remotes")?;
            verify_exact_entries(&remotes, &["origin"])?;
            let heads = refs.open_private_directory("heads")?;
            let origin = remotes.open_private_directory("origin")?;
            verify_clone_ref_tree(&heads, branch_relative, object_id, metadata_complete)?;
            verify_clone_ref_tree(&origin, branch_relative, object_id, metadata_complete)
        }

        fn verify_private_file_hash(
            &self,
            name: &str,
            expected_size: u64,
            expected_hash: &[u8; 32],
        ) -> Result<(), MutationIoError> {
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: retained directory and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ELOOP | libc::ENXIO | libc::ENODEV) => Err(MutationIoError::Unsafe),
                    Some(libc::ENOENT) => Err(MutationIoError::Conflict),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful openat returned an owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            let before = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !before.file_type().is_file()
                || before.nlink() != 1
                || before.mode() & 0o7777 != 0o600
                || before.len() != expected_size
            {
                return Err(MutationIoError::Unsafe);
            }
            let mut hasher = blake3::Hasher::new();
            let mut buffer = [0_u8; 64 * 1024];
            let mut size = 0u64;
            loop {
                let read = file.read(&mut buffer).map_err(|_| MutationIoError::Io)?;
                if read == 0 {
                    break;
                }
                size = size
                    .checked_add(read as u64)
                    .ok_or(MutationIoError::TooLarge)?;
                if size > expected_size {
                    return Err(MutationIoError::Conflict);
                }
                hasher.update(&buffer[..read]);
            }
            let after = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !same_metadata(&before, &after)
                || size != expected_size
                || hasher.finalize().as_bytes() != expected_hash
            {
                return Err(MutationIoError::Conflict);
            }
            Ok(())
        }

        fn create_or_verify_private_streamed_file(
            &self,
            name: &str,
            source: &mut File,
            expected_size: u64,
            expected_hash: &[u8; 32],
        ) -> Result<(), MutationIoError> {
            let original_name = name;
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            let source_before = source.metadata().map_err(|_| MutationIoError::Io)?;
            if !source_before.file_type().is_file()
                || source_before.nlink() != 1
                || source_before.len() != expected_size
            {
                return Err(MutationIoError::Unsafe);
            }
            source
                .seek(SeekFrom::Start(0))
                .map_err(|_| MutationIoError::Io)?;
            // SAFETY: retained directory and static dot component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c".".as_ptr(),
                    libc::O_WRONLY | libc::O_TMPFILE | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    0o600,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EISDIR) => {
                        Err(MutationIoError::Unsupported)
                    }
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful openat returned an owned descriptor.
            let mut output = unsafe { File::from_raw_fd(raw) };
            if unsafe { libc::fchmod(output.as_raw_fd(), 0o600) } != 0 {
                return Err(MutationIoError::Io);
            }
            let mut hasher = blake3::Hasher::new();
            let mut buffer = [0_u8; 64 * 1024];
            let mut size = 0u64;
            loop {
                let read = source.read(&mut buffer).map_err(|_| MutationIoError::Io)?;
                if read == 0 {
                    break;
                }
                size = size
                    .checked_add(read as u64)
                    .ok_or(MutationIoError::TooLarge)?;
                if size > expected_size {
                    return Err(MutationIoError::Conflict);
                }
                hasher.update(&buffer[..read]);
                output
                    .write_all(&buffer[..read])
                    .map_err(|_| MutationIoError::Io)?;
            }
            let source_after = source.metadata().map_err(|_| MutationIoError::Io)?;
            if !same_metadata(&source_before, &source_after)
                || size != expected_size
                || hasher.finalize().as_bytes() != expected_hash
            {
                return Err(MutationIoError::Conflict);
            }
            output
                .sync_all()
                .map_err(|_| MutationIoError::Unsupported)?;
            // SAFETY: both descriptors and NUL-terminated names are valid.
            let mut published = unsafe {
                libc::linkat(
                    output.as_raw_fd(),
                    c"".as_ptr(),
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::AT_EMPTY_PATH,
                )
            };
            if published != 0
                && matches!(
                    io::Error::last_os_error().raw_os_error(),
                    Some(libc::EPERM | libc::EINVAL | libc::ENOENT)
                )
            {
                let source_path = CString::new(format!("/proc/self/fd/{}", output.as_raw_fd()))
                    .map_err(|_| MutationIoError::Io)?;
                // SAFETY: source names the still-open inode and target is confined.
                published = unsafe {
                    libc::linkat(
                        libc::AT_FDCWD,
                        source_path.as_ptr(),
                        self.descriptor.as_raw_fd(),
                        name.as_ptr(),
                        libc::AT_SYMLINK_FOLLOW,
                    )
                };
            }
            if published != 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => {
                        self.verify_private_file_hash(original_name, expected_size, expected_hash)
                    }
                    Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EPERM) => {
                        Err(MutationIoError::Unsupported)
                    }
                    _ => Err(MutationIoError::Io),
                };
            }
            self.sync_directory()
                .map_err(map_clone_materialization_git)?;
            self.verify_private_file_hash(original_name, expected_size, expected_hash)
        }

        fn open_git_clone_directory_path(&self, path: &str) -> Result<Self, MutationIoError> {
            let mut directory = self.duplicate().map_err(|_| MutationIoError::Io)?;
            for part in path.split('/') {
                directory = directory.open_private_directory(part)?;
            }
            Ok(directory)
        }

        pub(super) fn prepare_git_clone_directories(
            &self,
            category_directories: &[String],
            attachment_directories: &[String],
        ) -> Result<(), MutationIoError> {
            let categories = self.ensure_private_directory("categories")?;
            for name in category_directories {
                if !super::valid_category_directory(name) {
                    return Err(MutationIoError::Unsafe);
                }
                categories.ensure_private_directory(name)?;
            }
            categories
                .sync_directory()
                .map_err(map_clone_materialization_git)?;

            if !attachment_directories.is_empty() {
                let attachments = self.ensure_private_directory("attachments")?;
                for guide_id in attachment_directories {
                    if guide_id.parse::<crate::protocol::GuideId>().is_err() {
                        return Err(MutationIoError::Unsafe);
                    }
                    attachments.ensure_private_directory(guide_id)?;
                }
                attachments
                    .sync_directory()
                    .map_err(map_clone_materialization_git)?;
            }
            self.sync_directory().map_err(map_clone_materialization_git)
        }

        pub(super) fn prepare_git_sync_directories(
            &self,
            operation_id: crate::protocol::OperationId,
            category_directories: &[String],
            attachment_directories: &[String],
        ) -> Result<(), MutationIoError> {
            let categories = self.open_owned_workspace_directory("categories")?;
            for name in category_directories {
                if !super::valid_category_directory(name) {
                    return Err(MutationIoError::Unsafe);
                }
                self.open_or_publish_git_sync_directory(
                    &categories,
                    operation_id,
                    "categories",
                    name,
                )?;
            }
            categories
                .sync_directory()
                .map_err(map_clone_materialization_git)?;

            if !attachment_directories.is_empty() {
                let attachments = self.open_or_publish_git_sync_directory(
                    self,
                    operation_id,
                    "roots",
                    "attachments",
                )?;
                for guide_id in attachment_directories {
                    if guide_id.parse::<crate::protocol::GuideId>().is_err() {
                        return Err(MutationIoError::Unsafe);
                    }
                    self.open_or_publish_git_sync_directory(
                        &attachments,
                        operation_id,
                        "attachments",
                        guide_id,
                    )?;
                }
                attachments
                    .sync_directory()
                    .map_err(map_clone_materialization_git)?;
            }
            self.sync_directory().map_err(map_clone_materialization_git)
        }

        fn open_or_publish_git_sync_directory(
            &self,
            active_parent: &Self,
            operation_id: crate::protocol::OperationId,
            kind: &str,
            name: &str,
        ) -> Result<Self, MutationIoError> {
            if !matches!(kind, "roots" | "categories" | "attachments") {
                return Err(MutationIoError::Unsafe);
            }
            match active_parent.open_owned_workspace_directory(name) {
                Ok(directory) => {
                    self.prepare_git_sync_directory_owner(
                        operation_id,
                        kind,
                        name,
                        &directory,
                        false,
                    )?;
                    return Ok(directory);
                }
                Err(MutationIoError::Conflict) => {}
                Err(error) => return Err(error),
            }

            let rehow = self.ensure_private_directory(".rehow")?;
            let trash = rehow.ensure_private_directory("trash")?;
            let operation = trash.ensure_private_directory(&operation_id.to_string())?;
            let staging = operation.ensure_private_directory("directory-staging")?;
            let staging_kind = staging.ensure_private_directory(kind)?;
            let staged = match staging_kind.optional_private_directory(name)? {
                Some(directory) => directory,
                None => staging_kind.create_private_directory_exclusive(name)?,
            };
            if !staged
                .entries(1)
                .map_err(map_clone_materialization_inspect)?
                .is_empty()
            {
                return Err(MutationIoError::Conflict);
            }
            self.prepare_git_sync_directory_owner(operation_id, kind, name, &staged, true)?;
            let staged_identity = staged.identity()?;
            let source = ManagedTarget {
                directory: staging_kind.duplicate().map_err(|_| MutationIoError::Io)?,
                name: name.to_owned(),
            };
            let destination = ManagedTarget {
                directory: active_parent.duplicate().map_err(|_| MutationIoError::Io)?,
                name: name.to_owned(),
            };
            source.move_to(&destination)?;
            let active = active_parent.open_owned_workspace_directory(name)?;
            if active.identity()? != staged_identity {
                return Err(MutationIoError::Conflict);
            }
            staging_kind
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            active_parent
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            Ok(active)
        }

        fn prepare_git_sync_directory_owner(
            &self,
            operation_id: crate::protocol::OperationId,
            kind: &str,
            name: &str,
            directory: &Self,
            created: bool,
        ) -> Result<(), MutationIoError> {
            let identity = directory.identity()?;
            let expected = git_sync_directory_owner_record(operation_id, kind, name, identity);
            let Some(records) =
                self.git_sync_directory_owner_records(operation_id, kind, created)?
            else {
                return Ok(());
            };
            if created {
                records
                    .create_or_verify_private_file(name, &expected)
                    .map_err(map_clone_materialization_git)
            } else if records
                .entry_exists(name)
                .map_err(|_| MutationIoError::Io)?
            {
                records
                    .verify_private_file(name, &expected)
                    .map_err(map_clone_materialization_git)
            } else {
                Ok(())
            }
        }

        fn git_sync_directory_owner_records(
            &self,
            operation_id: crate::protocol::OperationId,
            kind: &str,
            create: bool,
        ) -> Result<Option<Self>, MutationIoError> {
            if !matches!(kind, "roots" | "categories" | "attachments") {
                return Err(MutationIoError::Unsafe);
            }
            let operation = operation_id.to_string();
            let mut directory = self.duplicate().map_err(|_| MutationIoError::Io)?;
            for part in [".rehow", "trash", &operation, "directory-owners", kind] {
                directory = if create {
                    directory.ensure_private_directory(part)?
                } else {
                    let Some(next) = directory.optional_private_directory(part)? else {
                        return Ok(None);
                    };
                    next
                };
            }
            Ok(Some(directory))
        }

        fn open_owned_workspace_directory(&self, name: &str) -> Result<Self, MutationIoError> {
            let directory = match self.open_directory(name) {
                Ok(directory) => directory,
                Err(OpenDirectoryError::Missing) => return Err(MutationIoError::Conflict),
                Err(OpenDirectoryError::Unsafe) => return Err(MutationIoError::Unsafe),
                Err(OpenDirectoryError::Io(_)) => return Err(MutationIoError::Io),
            };
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptor/status are valid.
            if unsafe { libc::fstat(directory.descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
                return Err(MutationIoError::Io);
            }
            // SAFETY: successful `fstat` initialized the structure.
            let status = unsafe { status.assume_init() };
            if status.st_uid != unsafe { libc::geteuid() }
                || status.st_mode & libc::S_IFMT != libc::S_IFDIR
                || status.st_mode & 0o022 != 0
            {
                return Err(MutationIoError::Unsafe);
            }
            Ok(directory)
        }

        pub(super) fn git_initialization_resumable(
            &self,
            marker: &[u8],
        ) -> Result<bool, GitInitializeStorageError> {
            let Some(git_dir) = self
                .optional_private_directory(".git")
                .map_err(map_git_init_mutation)?
            else {
                return Ok(false);
            };
            let entries = git_dir.git_init_entries(8)?;
            if entries.is_empty() {
                return Ok(true);
            }
            if !entries.iter().any(|entry| entry == "rehow-init") {
                return Ok(false);
            }
            git_dir.validate_partial_git_directory(marker)?;
            Ok(true)
        }

        pub(super) fn initialize_git_repository(
            &self,
            marker: &[u8],
        ) -> Result<(), GitInitializeStorageError> {
            const HEAD: &[u8] = b"ref: refs/heads/main\n";
            const CONFIG: &[u8] = b"[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\tlogallrefupdates = true\n";

            let git_dir = match self.optional_private_directory(".git") {
                Ok(Some(directory)) => directory,
                Ok(None) => self
                    .create_private_directory_exclusive(".git")
                    .map_err(map_git_init_mutation)?,
                Err(error) => return Err(map_git_init_mutation(error)),
            };
            let entries = git_dir.git_init_entries(8)?;
            if !entries.is_empty() {
                if !entries.iter().any(|entry| entry == "rehow-init") {
                    return Err(GitInitializeStorageError::Conflict);
                }
                git_dir.validate_partial_git_directory(marker)?;
            }
            git_dir.create_or_verify_private_file("rehow-init", marker)?;
            #[cfg(test)]
            git_init_fail(GitInitFault::MarkerCreated)?;

            let objects = git_dir
                .ensure_private_directory("objects")
                .map_err(map_git_init_mutation)?;
            let info = objects
                .ensure_private_directory("info")
                .map_err(map_git_init_mutation)?;
            let pack = objects
                .ensure_private_directory("pack")
                .map_err(map_git_init_mutation)?;
            let refs = git_dir
                .ensure_private_directory("refs")
                .map_err(map_git_init_mutation)?;
            let heads = refs
                .ensure_private_directory("heads")
                .map_err(map_git_init_mutation)?;
            let tags = refs
                .ensure_private_directory("tags")
                .map_err(map_git_init_mutation)?;
            for directory in [&info, &pack, &heads, &tags] {
                if !directory.git_init_entries(1)?.is_empty() {
                    return Err(GitInitializeStorageError::Unsafe);
                }
                directory.sync_directory()?;
            }
            objects.sync_directory()?;
            refs.sync_directory()?;
            git_dir.sync_directory()?;
            #[cfg(test)]
            git_init_fail(GitInitFault::DirectoriesSynced)?;

            git_dir.create_or_verify_private_file("HEAD", HEAD)?;
            git_dir.create_or_verify_private_file("config", CONFIG)?;
            #[cfg(test)]
            git_init_fail(GitInitFault::FilesSynced)?;

            git_dir.validate_complete_git_directory(marker)?;
            git_dir.sync_directory()?;
            self.sync_directory()?;
            Ok(())
        }

        fn validate_partial_git_directory(
            &self,
            marker: &[u8],
        ) -> Result<(), GitInitializeStorageError> {
            const HEAD: &[u8] = b"ref: refs/heads/main\n";
            const CONFIG: &[u8] = b"[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\tlogallrefupdates = true\n";
            let entries = self.git_init_entries(8)?;
            if entries.iter().any(|entry| {
                !matches!(
                    entry.as_str(),
                    "rehow-init" | "HEAD" | "config" | "objects" | "refs"
                )
            }) {
                return Err(GitInitializeStorageError::Unsafe);
            }
            self.verify_private_file("rehow-init", marker)?;
            if entries.iter().any(|entry| entry == "HEAD") {
                self.verify_private_file("HEAD", HEAD)?;
            }
            if entries.iter().any(|entry| entry == "config") {
                self.verify_private_file("config", CONFIG)?;
            }
            self.validate_optional_git_tree("objects", &["info", "pack"])?;
            self.validate_optional_git_tree("refs", &["heads", "tags"])?;
            Ok(())
        }

        fn validate_complete_git_directory(
            &self,
            marker: &[u8],
        ) -> Result<(), GitInitializeStorageError> {
            self.validate_partial_git_directory(marker)?;
            let entries = self.git_init_entries(8)?;
            for required in ["rehow-init", "HEAD", "config", "objects", "refs"] {
                if !entries.iter().any(|entry| entry == required) {
                    return Err(GitInitializeStorageError::Unsafe);
                }
            }
            for (parent, children) in [("objects", ["info", "pack"]), ("refs", ["heads", "tags"])] {
                let directory = self
                    .open_private_directory(parent)
                    .map_err(map_git_init_mutation)?;
                let actual = directory.git_init_entries(3)?;
                if children
                    .iter()
                    .any(|required| !actual.iter().any(|entry| entry == required))
                {
                    return Err(GitInitializeStorageError::Unsafe);
                }
            }
            Ok(())
        }

        fn validate_optional_git_tree(
            &self,
            name: &str,
            allowed: &[&str],
        ) -> Result<(), GitInitializeStorageError> {
            let entries = self.git_init_entries(8)?;
            if !entries.iter().any(|entry| entry == name) {
                return Ok(());
            }
            let directory = self
                .open_private_directory(name)
                .map_err(map_git_init_mutation)?;
            let children = directory.git_init_entries(allowed.len() + 1)?;
            if children
                .iter()
                .any(|entry| !allowed.contains(&entry.as_str()))
            {
                return Err(GitInitializeStorageError::Unsafe);
            }
            for child in children {
                let leaf = directory
                    .open_private_directory(&child)
                    .map_err(map_git_init_mutation)?;
                if !leaf.git_init_entries(1)?.is_empty() {
                    return Err(GitInitializeStorageError::Unsafe);
                }
            }
            Ok(())
        }

        fn git_init_entries(&self, limit: usize) -> Result<Vec<String>, GitInitializeStorageError> {
            self.entries(limit).map_err(|error| match error {
                WorkspaceInspectError::ResourceLimit => GitInitializeStorageError::Unsafe,
                WorkspaceInspectError::UnsafeRoot | WorkspaceInspectError::ChangedDuringRead => {
                    GitInitializeStorageError::Unsafe
                }
                WorkspaceInspectError::NotFound
                | WorkspaceInspectError::NotDirectory
                | WorkspaceInspectError::PermissionDenied
                | WorkspaceInspectError::Io => GitInitializeStorageError::Io,
            })
        }

        fn create_or_verify_private_file(
            &self,
            name: &str,
            expected: &[u8],
        ) -> Result<(), GitInitializeStorageError> {
            let original_name = name;
            let name = component(name).map_err(|_| GitInitializeStorageError::Unsafe)?;
            // `O_TMPFILE` keeps partial bytes unnamed. The canonical name appears
            // only after the complete inode and its bytes are durable.
            // SAFETY: retained directory and static dot component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c".".as_ptr(),
                    libc::O_WRONLY | libc::O_TMPFILE | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    0o600,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EISDIR) => {
                        Err(GitInitializeStorageError::Unsupported)
                    }
                    _ => Err(GitInitializeStorageError::Io),
                };
            }
            // SAFETY: successful openat returned a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
                return Err(GitInitializeStorageError::Io);
            }
            file.write_all(expected)
                .map_err(|_| GitInitializeStorageError::Io)?;
            file.sync_all()
                .map_err(|_| GitInitializeStorageError::Unsupported)?;
            #[cfg(test)]
            git_init_fail(GitInitFault::FileBeforePublication)?;

            // Prefer the descriptor-only Linux publication primitive. Some
            // kernels/filesystems require the documented procfd fallback for
            // an `O_TMPFILE` inode owned by an unprivileged process.
            // SAFETY: both descriptors and NUL-terminated names are valid.
            let mut published = unsafe {
                libc::linkat(
                    file.as_raw_fd(),
                    c"".as_ptr(),
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::AT_EMPTY_PATH,
                )
            };
            if published != 0
                && matches!(
                    io::Error::last_os_error().raw_os_error(),
                    Some(libc::EPERM | libc::EINVAL | libc::ENOENT)
                )
            {
                let source = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))
                    .map_err(|_| GitInitializeStorageError::Io)?;
                // SAFETY: source names the still-open unnamed inode; target is
                // the retained directory and validated canonical component.
                published = unsafe {
                    libc::linkat(
                        libc::AT_FDCWD,
                        source.as_ptr(),
                        self.descriptor.as_raw_fd(),
                        name.as_ptr(),
                        libc::AT_SYMLINK_FOLLOW,
                    )
                };
            }
            if published != 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => self.verify_private_file(original_name, expected),
                    Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EPERM) => {
                        Err(GitInitializeStorageError::Unsupported)
                    }
                    _ => Err(GitInitializeStorageError::Io),
                };
            }
            self.sync_directory()
        }

        fn verify_private_file(
            &self,
            name: &str,
            expected: &[u8],
        ) -> Result<(), GitInitializeStorageError> {
            let state =
                self.read_regular_state(name, expected.len())
                    .map_err(|error| match error {
                        EntryReadError::Unsafe
                        | EntryReadError::TooLarge
                        | EntryReadError::Changed => GitInitializeStorageError::Unsafe,
                        EntryReadError::Io(_) => GitInitializeStorageError::Io,
                    })?;
            if state.mode != 0o600 || state.bytes != expected {
                return Err(GitInitializeStorageError::Unsafe);
            }
            Ok(())
        }

        fn sync_directory(&self) -> Result<(), GitInitializeStorageError> {
            // SAFETY: retained directory descriptor is valid.
            if unsafe { libc::fsync(self.descriptor.as_raw_fd()) } != 0 {
                Err(GitInitializeStorageError::Unsupported)
            } else {
                Ok(())
            }
        }

        pub(super) fn create_category_directory(
            &self,
            relative_directory: &str,
        ) -> Result<(), MutationIoError> {
            let Some(name) = relative_directory.strip_prefix("categories/") else {
                return Err(MutationIoError::Unsafe);
            };
            if !super::valid_category_directory(name) {
                return Err(MutationIoError::Unsafe);
            }
            let categories = self.open_owned_workspace_directory("categories")?;
            categories.create_private_directory_exclusive(name)?;
            Ok(())
        }

        pub(super) fn prepare_attachment_directory(
            &self,
            guide_id: &crate::protocol::GuideId,
        ) -> Result<(), MutationIoError> {
            let attachments = self.ensure_private_directory("attachments")?;
            attachments.ensure_private_directory(&guide_id.to_string())?;
            Ok(())
        }

        pub(super) fn available_attachment_path(
            &self,
            guide_id: &crate::protocol::GuideId,
            filename: &str,
        ) -> Result<String, MutationIoError> {
            for suffix in 1_u32..=10_000 {
                let candidate = if suffix == 1 {
                    filename.to_owned()
                } else {
                    attachment_filename_with_suffix(filename, suffix)
                };
                let relative_path = format!("attachments/{guide_id}/{candidate}");
                if !super::recovery::valid_active_path(&relative_path) {
                    return Err(MutationIoError::Unsafe);
                }
                let target = self.open_managed_target(&relative_path)?;
                if !target.exists()? {
                    return Ok(relative_path);
                }
            }
            Err(MutationIoError::Conflict)
        }

        pub(super) fn preview_attachment_path(
            &self,
            guide_id: &crate::protocol::GuideId,
            filename: &str,
        ) -> Result<String, MutationIoError> {
            let Some(attachments) = self.optional_private_directory("attachments")? else {
                return checked_attachment_path(guide_id, filename);
            };
            let Some(directory) = attachments.optional_private_directory(&guide_id.to_string())?
            else {
                return checked_attachment_path(guide_id, filename);
            };
            available_attachment_path_in(&directory, guide_id, filename)
        }

        pub(super) fn quarantine_empty_category_directory(
            &self,
            relative_directory: &str,
            operation_id: crate::protocol::OperationId,
        ) -> Result<bool, MutationIoError> {
            let Some(name) = relative_directory.strip_prefix("categories/") else {
                return Err(MutationIoError::Unsafe);
            };
            if !super::valid_category_directory(name) {
                return Err(MutationIoError::Unsafe);
            }
            let categories = self.open_owned_workspace_directory("categories")?;
            self.quarantine_empty_owned_directory(
                &categories,
                name,
                "categories",
                operation_id,
                false,
            )
        }

        pub(super) fn quarantine_empty_attachment_directory(
            &self,
            relative_path: &str,
            operation_id: crate::protocol::OperationId,
        ) -> Result<bool, MutationIoError> {
            let Some(remainder) = relative_path.strip_prefix("attachments/") else {
                return Err(MutationIoError::Unsafe);
            };
            let Some((guide_id, _)) = remainder.split_once('/') else {
                return Err(MutationIoError::Unsafe);
            };
            if guide_id.parse::<crate::protocol::GuideId>().is_err() {
                return Err(MutationIoError::Unsafe);
            }
            let attachments = match self.open_owned_workspace_directory("attachments") {
                Ok(directory) => directory,
                Err(MutationIoError::Conflict) => return Ok(true),
                Err(error) => return Err(error),
            };
            self.quarantine_empty_owned_directory(
                &attachments,
                guide_id,
                "attachments",
                operation_id,
                false,
            )
        }

        pub(super) fn quarantine_empty_git_sync_category_directory(
            &self,
            relative_directory: &str,
            operation_id: crate::protocol::OperationId,
        ) -> Result<bool, MutationIoError> {
            let Some(name) = relative_directory.strip_prefix("categories/") else {
                return Err(MutationIoError::Unsafe);
            };
            if !super::valid_category_directory(name) {
                return Err(MutationIoError::Unsafe);
            }
            let categories = self.open_owned_workspace_directory("categories")?;
            self.quarantine_empty_owned_directory(
                &categories,
                name,
                "categories",
                operation_id,
                true,
            )
        }

        pub(super) fn quarantine_empty_git_sync_attachment_directory(
            &self,
            relative_path: &str,
            operation_id: crate::protocol::OperationId,
        ) -> Result<bool, MutationIoError> {
            let Some(remainder) = relative_path.strip_prefix("attachments/") else {
                return Err(MutationIoError::Unsafe);
            };
            let Some((guide_id, _)) = remainder.split_once('/') else {
                return Err(MutationIoError::Unsafe);
            };
            if guide_id.parse::<crate::protocol::GuideId>().is_err() {
                return Err(MutationIoError::Unsafe);
            }
            let attachments = match self.open_owned_workspace_directory("attachments") {
                Ok(directory) => directory,
                Err(MutationIoError::Conflict) => return Ok(true),
                Err(error) => return Err(error),
            };
            self.quarantine_empty_owned_directory(
                &attachments,
                guide_id,
                "attachments",
                operation_id,
                true,
            )
        }

        pub(super) fn quarantine_empty_git_sync_attachments_root(
            &self,
            operation_id: crate::protocol::OperationId,
        ) -> Result<bool, MutationIoError> {
            self.quarantine_empty_owned_directory(self, "attachments", "roots", operation_id, true)
        }

        fn quarantine_empty_owned_directory(
            &self,
            active_parent: &Self,
            name: &str,
            kind: &str,
            operation_id: crate::protocol::OperationId,
            require_git_sync_owner: bool,
        ) -> Result<bool, MutationIoError> {
            let active = match active_parent.open_owned_workspace_directory(name) {
                Ok(directory) => directory,
                Err(MutationIoError::Conflict) => return Ok(true),
                Err(error) => return Err(error),
            };
            if require_git_sync_owner {
                let identity = active.identity()?;
                let expected = git_sync_directory_owner_record(operation_id, kind, name, identity);
                let Some(records) =
                    self.git_sync_directory_owner_records(operation_id, kind, false)?
                else {
                    return Ok(false);
                };
                if !records
                    .entry_exists(name)
                    .map_err(|_| MutationIoError::Io)?
                {
                    return Ok(false);
                }
                records
                    .verify_private_file(name, &expected)
                    .map_err(map_clone_materialization_git)?;
            }
            if !active
                .entries(1)
                .map_err(map_clone_materialization_inspect)?
                .is_empty()
            {
                return Ok(false);
            }
            let active_identity = active.identity()?;
            let rehow = self.ensure_private_directory(".rehow")?;
            let trash = rehow.ensure_private_directory("trash")?;
            let operation = trash.ensure_private_directory(&operation_id.to_string())?;
            let directories = operation.ensure_private_directory("directories")?;
            let destination_parent = directories.ensure_private_directory(kind)?;
            match destination_parent.open_directory(name) {
                Err(OpenDirectoryError::Missing) => {}
                Ok(_) => return Err(MutationIoError::Conflict),
                Err(OpenDirectoryError::Unsafe) => return Err(MutationIoError::Unsafe),
                Err(OpenDirectoryError::Io(_)) => return Err(MutationIoError::Io),
            }
            let source = ManagedTarget {
                directory: active_parent.duplicate().map_err(|_| MutationIoError::Io)?,
                name: name.to_owned(),
            };
            let destination = ManagedTarget {
                directory: destination_parent
                    .duplicate()
                    .map_err(|_| MutationIoError::Io)?,
                name: name.to_owned(),
            };
            #[cfg(test)]
            super::git_sync_directory_race(
                if kind == "categories" {
                    super::GitSyncDirectoryRace::ReplaceCategory
                } else {
                    super::GitSyncDirectoryRace::ReplaceAttachment
                },
                &active_parent.descriptor,
                name,
            )?;
            source.move_to(&destination)?;
            let quarantined = destination_parent.open_owned_workspace_directory(name)?;
            if quarantined.identity()? != active_identity
                || !quarantined
                    .entries(1)
                    .map_err(map_clone_materialization_inspect)?
                    .is_empty()
            {
                return Err(MutationIoError::Conflict);
            }
            active_parent
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            destination_parent
                .sync_directory()
                .map_err(map_clone_materialization_git)?;
            Ok(true)
        }

        pub(super) fn category_directory_exists(
            &self,
            relative_directory: &str,
        ) -> Result<bool, MutationIoError> {
            let Some(name) = relative_directory.strip_prefix("categories/") else {
                return Err(MutationIoError::Unsafe);
            };
            if !super::valid_category_directory(name) {
                return Err(MutationIoError::Unsafe);
            }
            let categories = self.open_owned_workspace_directory("categories")?;
            match categories.open_owned_workspace_directory(name) {
                Ok(_) => Ok(true),
                Err(MutationIoError::Conflict) => Ok(false),
                Err(error) => Err(error),
            }
        }

        pub(super) fn open_trash_target(
            &self,
            relative_path: &str,
        ) -> Result<ManagedTarget, MutationIoError> {
            self.trash_target(relative_path, false)
        }

        pub(super) fn prepare_trash_target(
            &self,
            relative_path: &str,
        ) -> Result<ManagedTarget, MutationIoError> {
            self.trash_target(relative_path, true)
        }

        fn trash_target(
            &self,
            relative_path: &str,
            create_parents: bool,
        ) -> Result<ManagedTarget, MutationIoError> {
            if !super::recovery::valid_trash_path(relative_path) {
                return Err(MutationIoError::Unsafe);
            }
            let mut parts = relative_path.split('/').peekable();
            let mut directory = self.duplicate().map_err(|_| MutationIoError::Io)?;
            while let Some(part) = parts.next() {
                if parts.peek().is_none() {
                    return Ok(ManagedTarget {
                        directory,
                        name: part.to_owned(),
                    });
                }
                directory = if create_parents {
                    directory.ensure_private_directory(part)?
                } else {
                    directory.open_private_directory(part)?
                };
            }
            Err(MutationIoError::Unsafe)
        }

        pub(super) fn identity(&self) -> Result<(u64, u64), MutationIoError> {
            descriptor_identity(&self.descriptor).map_err(|_| MutationIoError::Io)
        }

        pub(super) fn root_hash(&self) -> [u8; 32] {
            self.root_hash
        }

        pub(super) fn capability_hash(&self) -> [u8; 32] {
            self.capability_hash
        }

        pub(super) fn watch_path(&self) -> std::path::PathBuf {
            std::path::PathBuf::from(format!("/proc/self/fd/{}", self.descriptor.as_raw_fd()))
        }

        pub(super) fn git_repository_capability(
            &self,
        ) -> Result<super::GitRepositoryCapability, super::GitRepositoryCapabilityError> {
            use super::GitRepositoryCapabilityError as Error;

            let git_dir = match self.open_directory(".git") {
                Ok(directory) => directory,
                Err(OpenDirectoryError::Missing) => return Err(Error::NotRepository),
                Err(OpenDirectoryError::Unsafe) => return Err(Error::Unsafe),
                Err(OpenDirectoryError::Io(_)) => return Err(Error::Io),
            };
            if git_dir.entry_exists("commondir").map_err(|_| Error::Io)? {
                return Err(Error::Unsafe);
            }
            let objects = match git_dir.open_directory("objects") {
                Ok(objects) => {
                    match objects.open_directory("info") {
                        Ok(info) if info.entry_exists("alternates").map_err(|_| Error::Io)? => {
                            return Err(Error::Unsafe);
                        }
                        Ok(_) => {}
                        Err(OpenDirectoryError::Missing) => return Err(Error::Unsafe),
                        Err(OpenDirectoryError::Unsafe) => return Err(Error::Unsafe),
                        Err(OpenDirectoryError::Io(_)) => return Err(Error::Io),
                    }
                    objects
                }
                Err(OpenDirectoryError::Missing | OpenDirectoryError::Unsafe) => {
                    return Err(Error::Unsafe);
                }
                Err(OpenDirectoryError::Io(_)) => return Err(Error::Io),
            };
            let refs = git_dir.open_directory("refs").map_err(|_| Error::Unsafe)?;
            let head = git_dir
                .open_inheritable_regular("HEAD")
                .map_err(|_| Error::Unsafe)?;
            let config = git_dir
                .open_inheritable_regular("config")
                .map_err(|_| Error::Unsafe)?;
            let index = git_dir
                .open_optional_inheritable_regular("index")
                .map_err(|_| Error::Unsafe)?;
            let packed_refs = git_dir
                .open_optional_inheritable_regular("packed-refs")
                .map_err(|_| Error::Unsafe)?;
            let shallow = git_dir
                .open_optional_inheritable_regular("shallow")
                .map_err(|_| Error::Unsafe)?;

            let work_tree = self.inheritable_duplicate().map_err(|_| Error::Io)?;
            let git_dir = git_dir.inheritable_duplicate().map_err(|_| Error::Io)?;
            let objects = objects.inheritable_duplicate().map_err(|_| Error::Io)?;
            let refs = refs.inheritable_duplicate().map_err(|_| Error::Io)?;
            let work_tree_path = format!("/proc/self/fd/{}", work_tree.as_raw_fd());
            let objects_path = format!("/proc/self/fd/{}", objects.as_raw_fd());
            let config_path = format!("/proc/self/fd/{}", config.as_raw_fd());
            let refs_path = format!("/proc/self/fd/{}", refs.as_raw_fd());
            let head_path = format!("/proc/self/fd/{}", head.as_raw_fd());
            let index_path = index
                .as_ref()
                .map(|fd| format!("/proc/self/fd/{}", fd.as_raw_fd()));
            let packed_refs_path = packed_refs
                .as_ref()
                .map(|fd| format!("/proc/self/fd/{}", fd.as_raw_fd()));
            let shallow_path = shallow
                .as_ref()
                .map(|fd| format!("/proc/self/fd/{}", fd.as_raw_fd()));
            Ok(super::GitRepositoryCapability {
                _work_tree: work_tree,
                _git_dir: git_dir,
                _objects: objects,
                _config: config,
                _refs: refs,
                _head: head,
                _index: index,
                _packed_refs: packed_refs,
                _shallow: shallow,
                work_tree_path,
                objects_path,
                config_path,
                refs_path,
                head_path,
                index_path,
                packed_refs_path,
                shallow_path,
            })
        }

        fn open_inheritable_regular(&self, name: &str) -> Result<OwnedFd, io::Error> {
            let name = component(name)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid component"))?;
            // SAFETY: descriptor and NUL-terminated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful `openat` returned a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            let metadata = File::from(descriptor.try_clone()?).metadata()?;
            if !metadata.file_type().is_file() || metadata.nlink() != 1 {
                return Err(io::Error::other("unsafe regular file"));
            }
            Ok(descriptor)
        }

        fn open_optional_inheritable_regular(
            &self,
            name: &str,
        ) -> Result<Option<OwnedFd>, io::Error> {
            match self.open_inheritable_regular(name) {
                Ok(file) => Ok(Some(file)),
                Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(None),
                Err(error) => Err(error),
            }
        }

        fn inheritable_duplicate(&self) -> Result<OwnedFd, io::Error> {
            // SAFETY: `F_DUPFD` duplicates the valid descriptor and returns a newly owned fd.
            let raw = unsafe { libc::fcntl(self.descriptor.as_raw_fd(), libc::F_DUPFD, 3) };
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful `fcntl(F_DUPFD)` returns a newly owned descriptor.
            Ok(unsafe { OwnedFd::from_raw_fd(raw) })
        }

        fn entry_exists(&self, name: &str) -> Result<bool, io::Error> {
            let name = component(name)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid component"))?;
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptor/name/status are valid and `AT_SYMLINK_NOFOLLOW` inspects the entry.
            let result = unsafe {
                libc::fstatat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    status.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if result == 0 {
                return Ok(true);
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ENOENT) {
                Ok(false)
            } else {
                Err(error)
            }
        }

        pub(super) fn duplicate(&self) -> Result<Self, io::Error> {
            // Reopen `.` to get an independent directory-stream offset while
            // remaining bound to the retained directory rather than a path.
            // SAFETY: the descriptor and static NUL-terminated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c".".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                Err(io::Error::last_os_error())
            } else {
                // SAFETY: successful `openat` returns a newly owned descriptor.
                Ok(Self {
                    descriptor: unsafe { OwnedFd::from_raw_fd(raw) },
                    root_hash: self.root_hash,
                    capability_hash: self.capability_hash,
                })
            }
        }

        fn ensure_private_directory(&self, name: &str) -> Result<Self, MutationIoError> {
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: the retained parent and validated component are valid.
            let created =
                unsafe { libc::mkdirat(self.descriptor.as_raw_fd(), name.as_ptr(), 0o700) } == 0;
            if !created {
                match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => {}
                    _ => return Err(MutationIoError::Io),
                }
            }
            // SAFETY: the retained parent and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            if created {
                // SAFETY: descriptor is owned and valid.
                if unsafe { libc::fchmod(descriptor.as_raw_fd(), 0o700) } != 0 {
                    return Err(MutationIoError::Io);
                }
            }
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptor/status are valid.
            if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
                return Err(MutationIoError::Io);
            }
            // SAFETY: successful `fstat` initialized the structure.
            let status = unsafe { status.assume_init() };
            if status.st_uid != unsafe { libc::geteuid() }
                || status.st_mode & libc::S_IFMT != libc::S_IFDIR
                || status.st_mode & 0o7777 != 0o700
            {
                return Err(MutationIoError::Unsafe);
            }
            // SAFETY: the parent descriptor is a retained directory.
            if unsafe { libc::fsync(self.descriptor.as_raw_fd()) } != 0 {
                return Err(MutationIoError::Unsupported);
            }
            Ok(Self {
                descriptor,
                root_hash: self.root_hash,
                capability_hash: self.capability_hash,
            })
        }

        fn create_private_directory_exclusive(&self, name: &str) -> Result<Self, MutationIoError> {
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: the retained parent and validated component are valid.
            if unsafe { libc::mkdirat(self.descriptor.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => Err(MutationIoError::Conflict),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: the retained parent and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return Err(MutationIoError::Unsafe);
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            // SAFETY: descriptor is valid and owned.
            if unsafe { libc::fchmod(descriptor.as_raw_fd(), 0o700) } != 0 {
                return Err(MutationIoError::Io);
            }
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptor/status are valid.
            if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
                return Err(MutationIoError::Io);
            }
            // SAFETY: successful `fstat` initialized the structure.
            let status = unsafe { status.assume_init() };
            if status.st_uid != unsafe { libc::geteuid() }
                || status.st_mode & libc::S_IFMT != libc::S_IFDIR
                || status.st_mode & 0o7777 != 0o700
            {
                return Err(MutationIoError::Unsafe);
            }
            // SAFETY: the retained parent descriptor is valid.
            if unsafe { libc::fsync(self.descriptor.as_raw_fd()) } != 0 {
                return Err(MutationIoError::Unsupported);
            }
            Ok(Self {
                descriptor,
                root_hash: self.root_hash,
                capability_hash: self.capability_hash,
            })
        }

        pub(super) fn open_private_directory(&self, name: &str) -> Result<Self, MutationIoError> {
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: the retained parent and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Err(MutationIoError::Conflict),
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptor/status are valid.
            if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
                return Err(MutationIoError::Io);
            }
            // SAFETY: successful `fstat` initialized the structure.
            let status = unsafe { status.assume_init() };
            if status.st_uid != unsafe { libc::geteuid() }
                || status.st_mode & libc::S_IFMT != libc::S_IFDIR
                || status.st_mode & 0o7777 != 0o700
            {
                return Err(MutationIoError::Unsafe);
            }
            Ok(Self {
                descriptor,
                root_hash: self.root_hash,
                capability_hash: self.capability_hash,
            })
        }

        fn optional_private_directory(&self, name: &str) -> Result<Option<Self>, MutationIoError> {
            match self.open_private_directory(name) {
                Ok(directory) => Ok(Some(directory)),
                Err(MutationIoError::Conflict) => Ok(None),
                Err(error) => Err(error),
            }
        }
    }

    fn fill_random(bytes: &mut [u8]) -> Result<(), io::Error> {
        let mut filled = 0usize;
        while filled < bytes.len() {
            // SAFETY: the remaining slice is writable for the requested length.
            let result = unsafe {
                libc::getrandom(bytes[filled..].as_mut_ptr().cast(), bytes.len() - filled, 0)
            };
            if result > 0 {
                filled += result as usize;
                continue;
            }
            if result == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "system randomness returned no bytes",
                ));
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        Ok(())
    }

    fn verify_exact_entries(
        directory: &DirectoryCapability,
        expected: &[&str],
    ) -> Result<(), MutationIoError> {
        let actual = directory
            .entries(expected.len().saturating_add(1))
            .map_err(map_clone_materialization_inspect)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let expected = expected
            .iter()
            .map(|entry| (*entry).to_owned())
            .collect::<BTreeSet<_>>();
        if actual == expected {
            Ok(())
        } else {
            Err(MutationIoError::Unsafe)
        }
    }

    fn verify_allowed_entries(
        directory: &DirectoryCapability,
        allowed: &[&str],
    ) -> Result<(), MutationIoError> {
        let actual = directory
            .entries(allowed.len().saturating_add(1))
            .map_err(map_clone_materialization_inspect)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let allowed = allowed
            .iter()
            .map(|entry| (*entry).to_owned())
            .collect::<BTreeSet<_>>();
        if actual.is_subset(&allowed) {
            Ok(())
        } else {
            Err(MutationIoError::Unsafe)
        }
    }

    fn valid_clone_pack_name(name: &str, object_id_hex_len: usize) -> bool {
        let Some((stem, suffix)) = name.rsplit_once('.') else {
            return false;
        };
        stem.len() == 5 + object_id_hex_len
            && stem.starts_with("pack-")
            && stem[5..]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && matches!(suffix, "pack" | "idx")
    }

    fn valid_clone_branch_path(path: &str) -> bool {
        !path.is_empty()
            && path.len() <= 1024
            && !path.starts_with('/')
            && !path.ends_with('/')
            && !path.contains("//")
            && !path.contains("..")
            && !path.contains("@{")
            && !path.ends_with('.')
            && path.split('/').all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && !part.starts_with('.')
                    && !part.ends_with(".lock")
                    && part.len() <= 255
                    && part.bytes().all(|byte| {
                        byte > 0x20
                            && byte != 0x7f
                            && !matches!(byte, b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
                    })
            })
    }

    fn install_clone_ref(
        root: &DirectoryCapability,
        branch_relative: &str,
        object_id: &[u8],
    ) -> Result<(), MutationIoError> {
        let mut parts = branch_relative.split('/').peekable();
        let mut directory = root.duplicate().map_err(|_| MutationIoError::Io)?;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                let mut value = Vec::with_capacity(object_id.len() + 1);
                value.extend_from_slice(object_id);
                value.push(b'\n');
                return directory
                    .create_or_verify_private_file(part, &value)
                    .map_err(map_clone_materialization_git);
            }
            directory = directory.ensure_private_directory(part)?;
        }
        Err(MutationIoError::Unsafe)
    }

    fn verify_clone_ref_tree(
        root: &DirectoryCapability,
        branch_relative: &str,
        object_id: &[u8],
        complete: bool,
    ) -> Result<(), MutationIoError> {
        let parts = branch_relative.split('/').collect::<Vec<_>>();
        let mut directory = root.duplicate().map_err(|_| MutationIoError::Io)?;
        for (index, part) in parts.iter().enumerate() {
            let actual = directory
                .entries(2)
                .map_err(map_clone_materialization_inspect)?;
            if actual.iter().any(|entry| entry != part) || actual.len() > 1 {
                return Err(MutationIoError::Conflict);
            }
            let present = actual.iter().any(|entry| entry == part);
            if !present {
                return if complete {
                    Err(MutationIoError::Conflict)
                } else {
                    Ok(())
                };
            }
            if index + 1 == parts.len() {
                let mut value = Vec::with_capacity(object_id.len() + 1);
                value.extend_from_slice(object_id);
                value.push(b'\n');
                return directory
                    .verify_private_file(part, &value)
                    .map_err(map_clone_materialization_git);
            }
            directory = directory.open_private_directory(part)?;
        }
        Err(MutationIoError::Unsafe)
    }

    fn map_git_init_mutation(error: MutationIoError) -> GitInitializeStorageError {
        match error {
            MutationIoError::Conflict => GitInitializeStorageError::Conflict,
            MutationIoError::Unsafe | MutationIoError::TooLarge => {
                GitInitializeStorageError::Unsafe
            }
            MutationIoError::Unsupported => GitInitializeStorageError::Unsupported,
            MutationIoError::Io => GitInitializeStorageError::Io,
        }
    }

    fn map_clone_materialization_git(error: GitInitializeStorageError) -> MutationIoError {
        match error {
            GitInitializeStorageError::Conflict => MutationIoError::Conflict,
            GitInitializeStorageError::Unsafe => MutationIoError::Unsafe,
            GitInitializeStorageError::Unsupported => MutationIoError::Unsupported,
            GitInitializeStorageError::Io => MutationIoError::Io,
        }
    }

    fn git_sync_directory_owner_record(
        operation_id: crate::protocol::OperationId,
        kind: &str,
        name: &str,
        (device, inode): (u64, u64),
    ) -> Vec<u8> {
        format!(
            "rehow-git-sync-directory-owner-v1\n{operation_id}\n{kind}\n{name}\n{device}\n{inode}\n"
        )
        .into_bytes()
    }

    fn map_clone_materialization_inspect(error: WorkspaceInspectError) -> MutationIoError {
        match error {
            WorkspaceInspectError::UnsafeRoot => MutationIoError::Unsafe,
            WorkspaceInspectError::ResourceLimit => MutationIoError::TooLarge,
            WorkspaceInspectError::ChangedDuringRead => MutationIoError::Conflict,
            WorkspaceInspectError::NotFound
            | WorkspaceInspectError::NotDirectory
            | WorkspaceInspectError::PermissionDenied
            | WorkspaceInspectError::Io => MutationIoError::Io,
        }
    }

    fn map_clone_finalization_io(error: MutationIoError) -> GitCloneMaterializationStorageError {
        match error {
            MutationIoError::Conflict => GitCloneMaterializationStorageError::Conflict,
            MutationIoError::Unsafe => GitCloneMaterializationStorageError::Unsafe,
            MutationIoError::Unsupported => GitCloneMaterializationStorageError::Unsupported,
            MutationIoError::TooLarge => GitCloneMaterializationStorageError::TooLarge,
            MutationIoError::Io => GitCloneMaterializationStorageError::Io,
        }
    }

    impl ManagedTarget {
        fn exists(&self) -> Result<bool, MutationIoError> {
            let name = component(&self.name).map_err(|_| MutationIoError::Unsafe)?;
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: retained directory, validated component, and output storage are valid.
            if unsafe {
                libc::fstatat(
                    self.directory.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    status.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Ok(false),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful fstatat initialized the structure.
            let status = unsafe { status.assume_init() };
            if status.st_mode & libc::S_IFMT != libc::S_IFREG || status.st_nlink != 1 {
                return Err(MutationIoError::Unsafe);
            }
            Ok(true)
        }

        pub(super) fn state(&self, limit: usize) -> Result<ManagedState, MutationIoError> {
            self.named_state(&self.name, limit)
        }

        pub(super) fn temp_state(
            &self,
            temp_name: &str,
            limit: usize,
        ) -> Result<ManagedState, MutationIoError> {
            self.named_state(temp_name, limit)
        }

        fn named_state(&self, name: &str, limit: usize) -> Result<ManagedState, MutationIoError> {
            match self.directory.read_regular_state(name, limit) {
                Ok(state) => Ok(ManagedState::Regular(state)),
                Err(EntryReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                    Ok(ManagedState::Missing)
                }
                Err(EntryReadError::Unsafe) => Err(MutationIoError::Unsafe),
                Err(EntryReadError::TooLarge) => Err(MutationIoError::TooLarge),
                Err(EntryReadError::Changed) => Err(MutationIoError::Conflict),
                Err(EntryReadError::Io(_)) => Err(MutationIoError::Io),
            }
        }

        pub(super) fn hashed_state(&self, limit: u64) -> Result<HashedState, MutationIoError> {
            self.named_hashed_state(&self.name, limit)
        }

        pub(super) fn attachment_state(
            &self,
            revision_path: &str,
            limit: u64,
        ) -> Result<AttachmentState, MutationIoError> {
            use sha2::Digest;

            let Some(mut file) = self.open_attachment_file(revision_path, limit)? else {
                return Ok(AttachmentState::Missing);
            };
            let before = file.metadata().map_err(|_| MutationIoError::Io)?;
            let mut object = blake3::Hasher::new();
            let mut sha256 = sha2::Sha256::new();
            let mut revision = blake3::Hasher::new();
            revision.update(b"rehow-document-revision-v1\0");
            super::hash_frame(&mut revision, revision_path.as_bytes());
            revision.update(&before.len().to_be_bytes());
            let mut buffer = [0_u8; 64 * 1024];
            let mut size = 0_u64;
            loop {
                let read = file.read(&mut buffer).map_err(|_| MutationIoError::Io)?;
                if read == 0 {
                    break;
                }
                size = size
                    .checked_add(read as u64)
                    .ok_or(MutationIoError::TooLarge)?;
                if size > limit {
                    return Err(MutationIoError::TooLarge);
                }
                #[cfg(test)]
                if ATTACHMENT_STATE_CHANGED_FAULT.replace(false) {
                    // SAFETY: the descriptor is retained by this isolated test read.
                    if unsafe { libc::fchmod(file.as_raw_fd(), before.mode() ^ 0o100) } != 0 {
                        return Err(MutationIoError::Io);
                    }
                }
                object.update(&buffer[..read]);
                sha256.update(&buffer[..read]);
                revision.update(&buffer[..read]);
            }
            let after = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !same_metadata(&before, &after) || size != after.len() {
                return Err(MutationIoError::Conflict);
            }
            let mode = after.mode() & 0o7777;
            if mode > 0o777 {
                return Err(MutationIoError::Unsafe);
            }
            Ok(AttachmentState::Regular {
                object_hash: *object.finalize().as_bytes(),
                sha256: sha256.finalize().into(),
                revision: Revision::from_digest_bytes(*revision.finalize().as_bytes()),
                size,
                mode,
            })
        }

        pub(super) fn open_attachment_file(
            &self,
            revision_path: &str,
            limit: u64,
        ) -> Result<Option<File>, MutationIoError> {
            if !revision_path.starts_with("attachments/")
                || !super::recovery::valid_active_path(revision_path)
            {
                return Err(MutationIoError::Unsafe);
            }
            let name = component(&self.name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: directory descriptor and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.directory.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Ok(None),
                    Some(libc::ELOOP) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
                        Err(MutationIoError::Unsafe)
                    }
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful openat returns a newly owned descriptor.
            let file = unsafe { File::from_raw_fd(raw) };
            let metadata = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !metadata.file_type().is_file()
                || metadata.nlink() != 1
                || metadata.mode() & 0o7777 > 0o777
            {
                return Err(MutationIoError::Unsafe);
            }
            if metadata.len() > limit {
                return Err(MutationIoError::TooLarge);
            }
            Ok(Some(file))
        }

        pub(super) fn temp_hashed_state(
            &self,
            temp_name: &str,
            limit: u64,
        ) -> Result<HashedState, MutationIoError> {
            self.named_hashed_state(temp_name, limit)
        }

        fn named_hashed_state(
            &self,
            name: &str,
            limit: u64,
        ) -> Result<HashedState, MutationIoError> {
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: directory descriptor and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.directory.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Ok(HashedState::Missing),
                    Some(libc::ELOOP) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
                        Err(MutationIoError::Unsafe)
                    }
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful openat returns a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            let before = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !before.file_type().is_file() || before.nlink() != 1 {
                return Err(MutationIoError::Unsafe);
            }
            if before.len() > limit {
                return Err(MutationIoError::TooLarge);
            }
            let mut hasher = blake3::Hasher::new();
            let mut buffer = [0_u8; 64 * 1024];
            let mut size = 0_u64;
            loop {
                let read = file.read(&mut buffer).map_err(|_| MutationIoError::Io)?;
                if read == 0 {
                    break;
                }
                size = size
                    .checked_add(read as u64)
                    .ok_or(MutationIoError::TooLarge)?;
                if size > limit {
                    return Err(MutationIoError::TooLarge);
                }
                hasher.update(&buffer[..read]);
            }
            let after = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !same_metadata(&before, &after) || size != after.len() {
                return Err(MutationIoError::Conflict);
            }
            Ok(HashedState::Regular {
                hash: *hasher.finalize().as_bytes(),
                size,
                mode: after.mode() & 0o7777,
            })
        }

        pub(super) fn write_temp_stream(
            &self,
            temp_name: &str,
            source: &mut File,
            expected_hash: &[u8; 32],
            expected_size: u64,
            mode: u32,
        ) -> Result<(), MutationIoError> {
            if mode > 0o777 || expected_size > 64 * 1024 * 1024 {
                return Err(MutationIoError::Unsafe);
            }
            let temp_name = component(temp_name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: directory descriptor and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.directory.descriptor.as_raw_fd(),
                    temp_name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK,
                    mode,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => Err(MutationIoError::Conflict),
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful openat returns a newly owned descriptor.
            let mut output = unsafe { File::from_raw_fd(raw) };
            // SAFETY: descriptor is valid and the mode is bounded.
            if unsafe { libc::fchmod(output.as_raw_fd(), mode) } != 0 {
                return Err(MutationIoError::Io);
            }
            source
                .seek(SeekFrom::Start(0))
                .map_err(|_| MutationIoError::Io)?;
            let source_before = source.metadata().map_err(|_| MutationIoError::Io)?;
            let mut hasher = blake3::Hasher::new();
            let mut buffer = [0_u8; 64 * 1024];
            let mut size = 0_u64;
            loop {
                let read = source.read(&mut buffer).map_err(|_| MutationIoError::Io)?;
                if read == 0 {
                    break;
                }
                size = size
                    .checked_add(read as u64)
                    .ok_or(MutationIoError::TooLarge)?;
                if size > expected_size {
                    return Err(MutationIoError::Conflict);
                }
                hasher.update(&buffer[..read]);
                output
                    .write_all(&buffer[..read])
                    .map_err(|_| MutationIoError::Io)?;
            }
            let source_after = source.metadata().map_err(|_| MutationIoError::Io)?;
            if !same_metadata(&source_before, &source_after)
                || size != expected_size
                || hasher.finalize().as_bytes() != expected_hash
            {
                return Err(MutationIoError::Conflict);
            }
            output
                .sync_all()
                .map_err(|_| MutationIoError::Unsupported)?;
            let output_metadata = output.metadata().map_err(|_| MutationIoError::Io)?;
            if !output_metadata.file_type().is_file()
                || output_metadata.nlink() != 1
                || output_metadata.len() != expected_size
                || output_metadata.mode() & 0o7777 != mode
            {
                return Err(MutationIoError::Unsafe);
            }
            Ok(())
        }

        pub(super) fn sync_existing_temp_hashed(
            &self,
            temp_name: &str,
            expected_hash: &[u8; 32],
            expected_size: u64,
            mode: u32,
        ) -> Result<(), MutationIoError> {
            if self.temp_hashed_state(temp_name, 64 * 1024 * 1024)?
                != (HashedState::Regular {
                    hash: *expected_hash,
                    size: expected_size,
                    mode,
                })
            {
                return Err(MutationIoError::Conflict);
            }
            let name = component(temp_name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: retained directory and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.directory.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                return Err(MutationIoError::Io);
            }
            // SAFETY: successful openat returns a newly owned descriptor.
            let file = unsafe { File::from_raw_fd(raw) };
            file.sync_all().map_err(|_| MutationIoError::Unsupported)?;
            if self.temp_hashed_state(temp_name, 64 * 1024 * 1024)?
                == (HashedState::Regular {
                    hash: *expected_hash,
                    size: expected_size,
                    mode,
                })
            {
                Ok(())
            } else {
                Err(MutationIoError::Conflict)
            }
        }

        pub(super) fn write_temp(
            &self,
            temp_name: &str,
            bytes: &[u8],
            mode: u32,
        ) -> Result<(), MutationIoError> {
            if mode > 0o777 {
                return Err(MutationIoError::Unsafe);
            }
            let temp_name = component(temp_name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: directory descriptor and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.directory.descriptor.as_raw_fd(),
                    temp_name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK,
                    mode,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => Err(MutationIoError::Conflict),
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            // SAFETY: descriptor is valid and the mode was bounded above.
            if unsafe { libc::fchmod(file.as_raw_fd(), mode) } != 0 {
                return Err(MutationIoError::Io);
            }
            file.write_all(bytes).map_err(|_| MutationIoError::Io)?;
            #[cfg(test)]
            managed_fail(ManagedFault::TempAfterWrite)?;
            file.sync_all().map_err(|_| MutationIoError::Unsupported)?;
            #[cfg(test)]
            managed_fail(ManagedFault::TempAfterFileSync)?;
            let metadata = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !metadata.file_type().is_file()
                || metadata.nlink() != 1
                || metadata.len() != bytes.len() as u64
                || metadata.mode() & 0o7777 != mode
            {
                return Err(MutationIoError::Unsafe);
            }
            Ok(())
        }

        pub(super) fn sync_existing_temp(
            &self,
            temp_name: &str,
            expected: &[u8],
            mode: u32,
        ) -> Result<(), MutationIoError> {
            let temp_name = component(temp_name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: directory descriptor and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.directory.descriptor.as_raw_fd(),
                    temp_name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Err(MutationIoError::Conflict),
                    Some(libc::ELOOP) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
                        Err(MutationIoError::Unsafe)
                    }
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            let before = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !before.file_type().is_file() || before.nlink() != 1 {
                return Err(MutationIoError::Unsafe);
            }
            if before.len() != expected.len() as u64 || before.mode() & 0o7777 != mode {
                return Err(MutationIoError::Conflict);
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(expected.len())
                .map_err(|_| MutationIoError::TooLarge)?;
            Read::by_ref(&mut file)
                .take((expected.len() as u64).saturating_add(1))
                .read_to_end(&mut bytes)
                .map_err(|_| MutationIoError::Io)?;
            let after_read = file.metadata().map_err(|_| MutationIoError::Io)?;
            if bytes != expected
                || before.dev() != after_read.dev()
                || before.ino() != after_read.ino()
                || before.len() != after_read.len()
                || before.mtime() != after_read.mtime()
                || before.mtime_nsec() != after_read.mtime_nsec()
                || before.ctime() != after_read.ctime()
                || before.ctime_nsec() != after_read.ctime_nsec()
                || before.mode() != after_read.mode()
            {
                return Err(MutationIoError::Conflict);
            }
            file.sync_all().map_err(|_| MutationIoError::Unsupported)?;
            #[cfg(test)]
            managed_fail(ManagedFault::ReusedTempAfterFileSync)?;
            let after_sync = file.metadata().map_err(|_| MutationIoError::Io)?;
            if after_read.dev() != after_sync.dev()
                || after_read.ino() != after_sync.ino()
                || after_read.len() != after_sync.len()
                || after_read.mtime() != after_sync.mtime()
                || after_read.mtime_nsec() != after_sync.mtime_nsec()
                || after_read.ctime() != after_sync.ctime()
                || after_read.ctime_nsec() != after_sync.ctime_nsec()
                || after_read.mode() != after_sync.mode()
            {
                return Err(MutationIoError::Conflict);
            }
            Ok(())
        }

        pub(super) fn install_no_replace(&self, temp_name: &str) -> Result<(), MutationIoError> {
            self.rename(temp_name, &self.name, libc::RENAME_NOREPLACE)
        }

        pub(super) fn exchange(&self, temp_name: &str) -> Result<(), MutationIoError> {
            self.rename(temp_name, &self.name, libc::RENAME_EXCHANGE)
        }

        pub(super) fn move_to_temp(&self, temp_name: &str) -> Result<(), MutationIoError> {
            self.rename(&self.name, temp_name, libc::RENAME_NOREPLACE)
        }

        pub(super) fn move_to(&self, destination: &Self) -> Result<(), MutationIoError> {
            self.rename_between(
                &self.name,
                destination,
                &destination.name,
                libc::RENAME_NOREPLACE,
            )
        }

        pub(super) fn move_temp_to(
            &self,
            temp_name: &str,
            destination: &Self,
        ) -> Result<(), MutationIoError> {
            self.rename_between(
                temp_name,
                destination,
                &destination.name,
                libc::RENAME_NOREPLACE,
            )
        }

        fn rename(&self, from: &str, to: &str, flags: u32) -> Result<(), MutationIoError> {
            self.rename_between(from, self, to, flags)
        }

        fn rename_between(
            &self,
            from: &str,
            destination: &Self,
            to: &str,
            flags: u32,
        ) -> Result<(), MutationIoError> {
            let from = component(from).map_err(|_| MutationIoError::Unsafe)?;
            let to = component(to).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: both names and retained directory descriptors are valid.
            let result = unsafe {
                libc::syscall(
                    libc::SYS_renameat2,
                    self.directory.descriptor.as_raw_fd(),
                    from.as_ptr(),
                    destination.directory.descriptor.as_raw_fd(),
                    to.as_ptr(),
                    flags,
                )
            };
            if result == 0 {
                return Ok(());
            }
            match io::Error::last_os_error().raw_os_error() {
                Some(libc::EEXIST) | Some(libc::ENOENT) => Err(MutationIoError::Conflict),
                Some(libc::ENOSYS)
                | Some(libc::EINVAL)
                | Some(libc::EOPNOTSUPP)
                | Some(libc::EXDEV) => Err(MutationIoError::Unsupported),
                Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                _ => Err(MutationIoError::Io),
            }
        }

        pub(super) fn sync_directory(&self) -> Result<(), MutationIoError> {
            // SAFETY: descriptor is a retained directory.
            if unsafe { libc::fsync(self.directory.descriptor.as_raw_fd()) } == 0 {
                Ok(())
            } else {
                Err(MutationIoError::Unsupported)
            }
        }
    }

    fn same_metadata(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
        left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.len() == right.len()
            && left.mtime() == right.mtime()
            && left.mtime_nsec() == right.mtime_nsec()
            && left.ctime() == right.ctime()
            && left.ctime_nsec() == right.ctime_nsec()
            && left.mode() == right.mode()
    }

    fn attachment_filename_with_suffix(filename: &str, suffix: u32) -> String {
        let suffix = format!("-{suffix}");
        let (stem, extension) = filename
            .rsplit_once('.')
            .map_or((filename, ""), |(stem, ext)| {
                if stem.is_empty() || ext.is_empty() {
                    (filename, "")
                } else {
                    (stem, ext)
                }
            });
        let extension_bytes = if extension.is_empty() {
            0
        } else {
            extension.len() + 1
        };
        let max_stem = 255_usize.saturating_sub(suffix.len() + extension_bytes);
        let mut end = stem.len().min(max_stem);
        while end > 0 && !stem.is_char_boundary(end) {
            end -= 1;
        }
        let stem = if end == 0 { "attachment" } else { &stem[..end] };
        if extension.is_empty() {
            format!("{stem}{suffix}")
        } else {
            format!("{stem}{suffix}.{extension}")
        }
    }

    fn checked_attachment_path(
        guide_id: &crate::protocol::GuideId,
        filename: &str,
    ) -> Result<String, MutationIoError> {
        let path = format!("attachments/{guide_id}/{filename}");
        if super::recovery::valid_active_path(&path) {
            Ok(path)
        } else {
            Err(MutationIoError::Unsafe)
        }
    }

    fn available_attachment_path_in(
        directory: &DirectoryCapability,
        guide_id: &crate::protocol::GuideId,
        filename: &str,
    ) -> Result<String, MutationIoError> {
        for suffix in 1_u32..=10_000 {
            let candidate = if suffix == 1 {
                filename.to_owned()
            } else {
                attachment_filename_with_suffix(filename, suffix)
            };
            let relative_path = checked_attachment_path(guide_id, &candidate)?;
            let target = ManagedTarget {
                directory: directory.duplicate().map_err(|_| MutationIoError::Io)?,
                name: candidate,
            };
            if !target.exists()? {
                return Ok(relative_path);
            }
        }
        Err(MutationIoError::Conflict)
    }

    fn descriptor_identity(descriptor: &OwnedFd) -> Result<(u64, u64), io::Error> {
        let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `status` points to writable storage and the descriptor is valid.
        if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful `fstat` initialized the complete structure.
        let status = unsafe { status.assume_init() };
        Ok((status.st_dev, status.st_ino))
    }

    fn valid_gitdir_file(bytes: &[u8]) -> bool {
        let line = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        let Some(target) = line.strip_prefix(b"gitdir: ") else {
            return false;
        };
        !target.is_empty()
            && !target.contains(&0)
            && !target.contains(&b'\n')
            && !target.contains(&b'\r')
    }

    struct DirectoryStream(*mut libc::DIR);

    impl Drop for DirectoryStream {
        fn drop(&mut self) {
            // SAFETY: `DirectoryStream` exclusively owns a successful `fdopendir`
            // result and closes it exactly once.
            unsafe { libc::closedir(self.0) };
        }
    }

    fn component(value: &str) -> Result<CString, ()> {
        let bytes = OsStr::new(value).as_bytes();
        if bytes.is_empty()
            || bytes.len() > 255
            || bytes == b"."
            || bytes == b".."
            || bytes.contains(&b'/')
            || bytes.contains(&b'\\')
        {
            return Err(());
        }
        CString::new(bytes).map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let number = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rehow-storage-test-{}-{number}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn manifest(version: u32) -> String {
        format!(
            "schema_version: {version}\nworkspace_id: 0199a19f-4d15-7ca7-a645-e75311caa710\nname: Test Workspace\ncreated_at: \"2026-09-05T10:00:00Z\"\nupdated_at: \"2026-09-05T10:00:00Z\"\n"
        )
    }

    const CATEGORY: &str = "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux System\norder: 1000\nstatus: active\ncreated_at: \"2026-09-05T10:01:00Z\"\nupdated_at: \"2026-09-05T10:01:00Z\"\n";
    const GUIDE: &str = "---\nschema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: Install a Tarball\nsummary: Install a downloaded archive.\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags:\n  - linux\naliases:\n  - manual install\nstatus: draft\ncreated_at: \"2026-09-05T10:03:00Z\"\nupdated_at: \"2026-09-05T10:03:00Z\"\n---\n\n## Steps\n\nExtract the archive.\n";

    fn write_compatible(root: &Path) {
        fs::write(root.join("rehow.yaml"), manifest(1)).unwrap();
        let category = root.join("categories/linux-system--b6ba3a5551");
        fs::create_dir_all(&category).unwrap();
        fs::write(category.join("category.yaml"), CATEGORY).unwrap();
        fs::write(category.join("install--36c5735e10.md"), GUIDE).unwrap();
    }

    fn write_attachment_guide(root: &Path, sha256: &str, size: u64) -> PathBuf {
        let category = root.join("categories/linux-system--b6ba3a5551");
        let guide = GUIDE.replacen(
            "status: draft\n",
            &format!(
                "attachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.txt\n    title: Example text\n    sha256: {sha256}\n    size_bytes: {size}\nstatus: draft\n"
            ),
            1,
        );
        fs::write(category.join("install--36c5735e10.md"), guide).unwrap();
        let directory = root
            .join("attachments")
            .join("0199a1d7-7b0d-7d8e-a38d-f936c5735e10");
        fs::create_dir_all(&directory).unwrap();
        directory.join("example.txt")
    }

    fn classification(root: &Path) -> WorkspaceInspection {
        inspect_workspace(root).unwrap().into_parts().1
    }

    #[test]
    fn correlates_attachment_hash_and_size_without_loading_bytes_into_the_view() {
        let root = TestDirectory::new();
        write_compatible(root.path());
        let path = write_attachment_guide(
            root.path(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            5,
        );
        fs::write(path, b"hello").unwrap();

        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(view.issues().is_empty());
    }

    #[test]
    fn reports_missing_orphan_hash_and_size_attachment_issues() {
        let root = TestDirectory::new();
        write_compatible(root.path());
        let path = write_attachment_guide(
            root.path(),
            "0cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            4,
        );
        fs::write(&path, b"hello").unwrap();
        let orphan_directory = root
            .path()
            .join("attachments")
            .join("0199a1d7-7b0d-7d8e-a38d-f936c5735e11");
        fs::create_dir(&orphan_directory).unwrap();
        fs::write(orphan_directory.join("orphan.txt"), b"orphan").unwrap();

        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        let kinds = view
            .issues()
            .iter()
            .map(WorkspaceIssue::kind)
            .collect::<Vec<_>>();
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::AttachmentHashMismatch),
            "{kinds:?}"
        );
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::AttachmentSizeMismatch)
        );
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::OrphanAttachment)
        );

        fs::remove_file(path).unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::MissingAttachment)
        );
    }

    #[test]
    fn refuses_symlink_and_hardlinked_attachment_candidates() {
        use std::os::unix::fs::symlink;

        let root = TestDirectory::new();
        write_compatible(root.path());
        let path = write_attachment_guide(
            root.path(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            5,
        );
        let source = root.path().join("source.txt");
        fs::write(&source, b"hello").unwrap();
        fs::hard_link(&source, &path).unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::UnsafeEntry)
        );

        fs::remove_file(&path).unwrap();
        symlink(&source, &path).unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::UnsafeEntry)
        );
    }

    #[test]
    fn rejects_invalid_attachment_hierarchy_and_special_files() {
        use std::{ffi::CString, os::unix::ffi::OsStrExt, os::unix::net::UnixListener};

        let root = TestDirectory::new();
        write_compatible(root.path());
        let attachments = root.path().join("attachments");
        let invalid_directory = attachments.join("not-a-guide-id");
        fs::create_dir_all(&invalid_directory).unwrap();
        fs::write(invalid_directory.join("file.txt"), b"ignored").unwrap();
        let directory = attachments.join("0199a1d7-7b0d-7d8e-a38d-f936c5735e10");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("bad\\name"), b"ignored").unwrap();
        let fifo = directory.join("pipe");
        let fifo = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: the path is a valid NUL-terminated test path in an isolated directory.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let _socket = UnixListener::bind(directory.join("socket")).unwrap();

        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(
            view.issues()
                .iter()
                .filter(|issue| issue.kind() == WorkspaceIssueKind::UnsupportedEntry)
                .count()
                >= 2
        );
        assert_eq!(
            view.issues()
                .iter()
                .filter(|issue| issue.kind() == WorkspaceIssueKind::UnsafeEntry)
                .count(),
            2
        );
    }

    #[test]
    fn attachment_stream_and_aggregate_limits_accept_exact_and_refuse_one_over() {
        let root = TestDirectory::new();
        fs::write(root.path().join("five"), b"12345").unwrap();
        fs::write(root.path().join("six"), b"123456").unwrap();
        let capability = linux::DirectoryCapability::open_root(root.path()).unwrap();
        let Ok(exact) = capability.hash_attachment("five", 5, 5) else {
            panic!("exact Attachment limit should hash");
        };
        assert_eq!(exact.size, 5);
        assert!(matches!(
            capability.hash_attachment("six", 5, 10),
            Err(linux::AttachmentReadError::TooLarge(6))
        ));
        assert!(matches!(
            capability.hash_attachment("six", 10, 5),
            Err(linux::AttachmentReadError::AggregateLimit)
        ));
        linux::set_attachment_growth_fault();
        assert!(matches!(
            capability.hash_attachment("five", 5, 5),
            Err(linux::AttachmentReadError::Changed)
        ));

        let mut total = ATTACHMENT_BYTES_LIMIT;
        assert!(add_attachment_bytes(&mut total, 0).is_ok());
        assert_eq!(
            add_attachment_bytes(&mut total, 1),
            Err(WorkspaceInspectError::ResourceLimit)
        );

        let mut count = ATTACHMENT_COUNT_LIMIT - 1;
        assert!(increment_attachment_count(&mut count).is_ok());
        assert_eq!(count, ATTACHMENT_COUNT_LIMIT);
        assert_eq!(
            increment_attachment_count(&mut count),
            Err(WorkspaceInspectError::ResourceLimit)
        );
    }

    #[test]
    fn removal_attachment_reader_is_bounded_stable_and_rejects_special_files() {
        use std::{
            ffi::CString,
            os::unix::{ffi::OsStrExt, fs::PermissionsExt, net::UnixListener},
        };

        let root = TestDirectory::new();
        let guide_id: crate::protocol::GuideId =
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".parse().unwrap();
        let directory = root.path().join("attachments").join(guide_id.to_string());
        fs::create_dir_all(&directory).unwrap();
        let capability = linux::DirectoryCapability::open_root(root.path()).unwrap();

        fs::write(directory.join("exact.bin"), b"12345").unwrap();
        let exact_path = format!("attachments/{guide_id}/exact.bin");
        let exact = capability.open_managed_target(&exact_path).unwrap();
        assert!(matches!(
            exact.attachment_state(&exact_path, 5),
            Ok(linux::AttachmentState::Regular { size: 5, .. })
        ));
        assert_eq!(
            exact.attachment_state(&exact_path, 4),
            Err(linux::MutationIoError::TooLarge)
        );

        linux::set_attachment_state_changed_fault();
        assert_eq!(
            exact.attachment_state(&exact_path, 5),
            Err(linux::MutationIoError::Conflict)
        );
        fs::set_permissions(
            directory.join("exact.bin"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fs::set_permissions(
            directory.join("exact.bin"),
            fs::Permissions::from_mode(0o4600),
        )
        .unwrap();
        assert_eq!(
            exact.attachment_state(&exact_path, 5),
            Err(linux::MutationIoError::Unsafe)
        );

        let fifo_path = directory.join("pipe");
        let fifo = CString::new(fifo_path.as_os_str().as_bytes()).unwrap();
        // SAFETY: this creates a FIFO only in the isolated test directory.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let fifo_relative = format!("attachments/{guide_id}/pipe");
        assert_eq!(
            capability
                .open_managed_target(&fifo_relative)
                .unwrap()
                .attachment_state(&fifo_relative, 5),
            Err(linux::MutationIoError::Unsafe)
        );

        let _socket = UnixListener::bind(directory.join("socket")).unwrap();
        let socket_relative = format!("attachments/{guide_id}/socket");
        assert_eq!(
            capability
                .open_managed_target(&socket_relative)
                .unwrap()
                .attachment_state(&socket_relative, 5),
            Err(linux::MutationIoError::Unsafe)
        );
    }

    #[test]
    fn classifies_empty_and_unrelated_folders() {
        let empty = TestDirectory::new();
        assert!(matches!(
            classification(empty.path()),
            WorkspaceInspection::Empty(_)
        ));
        fs::write(empty.path().join("notes.txt"), "hello").unwrap();
        assert!(matches!(
            classification(empty.path()),
            WorkspaceInspection::UnrelatedNonempty
        ));
    }

    #[test]
    fn selected_root_descriptor_survives_path_replacement() {
        let base = TestDirectory::new();
        let selected_path = base.path().join("selected");
        let moved_path = base.path().join("original");
        fs::create_dir(&selected_path).unwrap();
        write_compatible(&selected_path);
        let selection = select_workspace_root(&selected_path).unwrap();

        fs::rename(&selected_path, &moved_path).unwrap();
        fs::create_dir(&selected_path).unwrap();

        let report = inspect_selected_workspace(selection).unwrap();
        let WorkspaceInspection::Compatible(view) = report.classification() else {
            panic!("the originally selected descriptor must remain authoritative");
        };
        assert_eq!(view.manifest().name(), "Test Workspace");
        assert!(matches!(
            classification(&selected_path),
            WorkspaceInspection::Empty(_)
        ));
    }

    #[test]
    fn classifies_root_markdown_collection() {
        let root = TestDirectory::new();
        fs::write(root.path().join("notes.md"), "# Notes\n").unwrap();
        assert!(matches!(
            classification(root.path()),
            WorkspaceInspection::OrdinaryMarkdown
        ));
    }

    #[test]
    fn classifies_nested_markdown_without_following_a_symlink() {
        let root = TestDirectory::new();
        let nested = root.path().join("notes/linux");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("install.md"), "# Install\n").unwrap();
        assert!(matches!(
            classification(root.path()),
            WorkspaceInspection::OrdinaryMarkdown
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let links = TestDirectory::new();
            symlink(nested.join("install.md"), links.path().join("outside.md")).unwrap();
            assert!(matches!(
                classification(links.path()),
                WorkspaceInspection::UnrelatedNonempty
            ));
        }
    }

    #[test]
    fn loads_a_compatible_workspace_and_revisions() {
        let root = TestDirectory::new();
        write_compatible(root.path());
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert_eq!(view.manifest().name(), "Test Workspace");
        assert_eq!(view.categories().len(), 1);
        assert_eq!(view.categories()[0].guides().len(), 1);
        assert_eq!(
            view.categories()[0].guides()[0].guide().title(),
            "Install a Tarball"
        );
        assert!(view.issues().is_empty());
        assert_eq!(
            view.manifest_revision().to_string(),
            document_revision("rehow.yaml", manifest(1).as_bytes()).to_string()
        );
    }

    #[test]
    fn detects_newer_and_malformed_manifests() {
        let newer = TestDirectory::new();
        fs::write(newer.path().join("rehow.yaml"), manifest(2)).unwrap();
        assert!(matches!(
            classification(newer.path()),
            WorkspaceInspection::NewerUnsupported { version: 2 }
        ));

        let malformed = TestDirectory::new();
        fs::write(malformed.path().join("rehow.yaml"), "not: [valid").unwrap();
        assert!(matches!(
            classification(malformed.path()),
            WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest
            }
        ));

        let interrupted_initialization = TestDirectory::new();
        fs::write(
            interrupted_initialization.path().join("rehow.yaml"),
            manifest(1),
        )
        .unwrap();
        assert!(matches!(
            classification(interrupted_initialization.path()),
            WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::CategoriesMissing
            }
        ));
    }

    #[test]
    fn isolates_a_guide_in_the_wrong_category() {
        let root = TestDirectory::new();
        write_compatible(root.path());
        let path = root
            .path()
            .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md");
        let changed = GUIDE.replace(
            "0199a1c2-d334-7d17-bd9c-f8b6ba3a5551",
            "0199a1c2-d334-7d17-bd9c-f8b6ba3a5552",
        );
        fs::write(path, changed).unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(view.categories()[0].guides().is_empty());
        assert_eq!(
            view.issues()[0].kind(),
            WorkspaceIssueKind::CategoryMismatch
        );
    }

    #[test]
    fn isolates_oversize_and_duplicate_guides() {
        let root = TestDirectory::new();
        write_compatible(root.path());
        let category = root.path().join("categories/linux-system--b6ba3a5551");
        fs::write(category.join("duplicate--0000000001.md"), GUIDE).unwrap();
        fs::write(
            category.join("oversize--0000000002.md"),
            vec![b'x'; GUIDE_LIMIT + 1],
        )
        .unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert_eq!(view.categories()[0].guides().len(), 1);
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::DuplicateGuideId)
        );
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::FileTooLarge)
        );
    }

    #[test]
    fn bounded_enumeration_refuses_one_over() {
        let root = TestDirectory::new();
        fs::write(root.path().join("a"), "a").unwrap();
        fs::write(root.path().join("b"), "b").unwrap();
        let capability = linux::DirectoryCapability::open_root(root.path()).unwrap();
        assert_eq!(
            capability.entries(1).unwrap_err(),
            WorkspaceInspectError::ResourceLimit
        );
    }

    #[test]
    fn reports_writability_and_root_or_ancestor_git_markers() {
        let repository = TestDirectory::new();
        fs::create_dir(repository.path().join(".git")).unwrap();
        let root_report = inspect_workspace(repository.path()).unwrap();
        assert!(root_report.facts().writable());
        assert!(root_report.facts().existing_git_repository());
        assert!(!root_report.facts().nested_git_repository());
        assert!(root_report.facts().git_probe_complete());
        assert!(!root_report.facts().unsafe_git_marker());

        let nested = repository.path().join("notes/rehow");
        fs::create_dir_all(&nested).unwrap();
        let nested_report = inspect_workspace(&nested).unwrap();
        assert!(!nested_report.facts().existing_git_repository());
        assert!(nested_report.facts().nested_git_repository());
        assert!(nested_report.facts().git_probe_complete());

        let worktree = TestDirectory::new();
        fs::write(worktree.path().join(".git"), "gitdir: ../metadata\n").unwrap();
        assert!(
            inspect_workspace(worktree.path())
                .unwrap()
                .facts()
                .existing_git_repository()
        );
    }

    #[cfg(unix)]
    #[test]
    fn reports_non_writable_and_unsafe_git_markers_without_following_them() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = TestDirectory::new();
        let outside = TestDirectory::new();
        symlink(outside.path(), root.path().join(".git")).unwrap();
        let unsafe_report = inspect_workspace(root.path()).unwrap();
        assert!(!unsafe_report.facts().existing_git_repository());
        assert!(unsafe_report.facts().unsafe_git_marker());

        fs::remove_file(root.path().join(".git")).unwrap();
        fs::write(root.path().join(".git"), "ordinary file\n").unwrap();
        let invalid_file_report = inspect_workspace(root.path()).unwrap();
        assert!(!invalid_file_report.facts().existing_git_repository());
        assert!(invalid_file_report.facts().unsafe_git_marker());

        fs::remove_file(root.path().join(".git")).unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let read_only_report = inspect_workspace(root.path()).unwrap();
        assert!(!read_only_report.facts().writable());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_socket_disguised_as_a_guide_without_blocking() {
        use std::os::unix::net::UnixListener;

        let root = TestDirectory::new();
        write_compatible(root.path());
        let guide = root
            .path()
            .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md");
        fs::remove_file(&guide).unwrap();
        let _listener = UnixListener::bind(&guide).unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(view.categories()[0].guides().is_empty());
        assert_eq!(view.issues()[0].kind(), WorkspaceIssueKind::UnsafeEntry);
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_managed_symlinks_or_accepts_hardlinks() {
        use std::os::unix::fs::symlink;

        let root = TestDirectory::new();
        let outside = TestDirectory::new();
        fs::write(outside.path().join("manifest"), manifest(1)).unwrap();
        symlink(
            outside.path().join("manifest"),
            root.path().join("rehow.yaml"),
        )
        .unwrap();
        assert!(matches!(
            classification(root.path()),
            WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest
            }
        ));

        fs::remove_file(root.path().join("rehow.yaml")).unwrap();
        fs::hard_link(
            outside.path().join("manifest"),
            root.path().join("rehow.yaml"),
        )
        .unwrap();
        assert!(matches!(
            classification(root.path()),
            WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest
            }
        ));
    }

    #[test]
    fn revision_is_path_and_byte_sensitive() {
        let first = document_revision("rehow.yaml", b"abc");
        let same = document_revision("rehow.yaml", b"abc");
        let moved = document_revision("other.yaml", b"abc");
        let changed = document_revision("rehow.yaml", b"abd");
        assert_eq!(first, same);
        assert_ne!(first, moved);
        assert_ne!(first, changed);
        assert_eq!(
            first.to_string(),
            "rev:v1:ec918848dad4d31267309c57c165f16c9d725f0d6eebfae93ec1d60212eea0c2"
        );
    }
}

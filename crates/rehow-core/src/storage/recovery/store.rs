use super::{
    ContentHash, GitClonePackSource, GitCommitIntent, Journal, JournalCodecError,
    MAX_GIT_COMMIT_INTENT_BYTES, MAX_JOURNAL_BYTES,
};
use crate::protocol::{OperationId, WorkspaceId};
use std::{
    ffi::{CStr, CString, OsStr},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_OBJECT_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_TREE_ENTRY_LIMIT: usize = 100_000;
const COMMIT_STAGING_ENTRY_LIMIT: usize = 240_512;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    static SOURCE_CHANGE_FAULT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn set_source_change_fault() {
    SOURCE_CHANGE_FAULT.set(true);
}

pub(super) struct RecoveryStore {
    state_rehow: Directory,
    objects: Directory,
    journals: Directory,
    capability_hash: ContentHash,
    workspace_id: WorkspaceId,
    root_hash: ContentHash,
    #[cfg(test)]
    fault: std::cell::Cell<Option<FaultPoint>>,
}

pub(super) struct WriterLock {
    descriptor: OwnedFd,
    capability_hash: ContentHash,
}

pub(super) struct StreamedObject {
    pub(super) object_hash: ContentHash,
    pub(super) sha256: [u8; 32],
    pub(super) size: u64,
    pub(super) created: bool,
}

pub(super) struct StreamedObjectExpectation<'a> {
    pub(super) source_state: &'a super::AttachmentSourceState,
    pub(super) object_hash: &'a ContentHash,
    pub(super) sha256: &'a [u8; 32],
    pub(super) size: u64,
}

pub(super) struct CloneStaging {
    _stage: OwnedFd,
    _git: OwnedFd,
    validation: Directory,
    stage_path: String,
    git_path: String,
    validation_path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CloneGitState {
    pub(super) bytes: u64,
    pub(super) binding: [u8; 32],
}

pub(super) struct CommitStaging {
    commit_staging: Directory,
    commit_quarantine: Directory,
    _stage: OwnedFd,
    _git: OwnedFd,
    validation: Directory,
    candidate: Directory,
    stage_path: String,
    git_path: String,
    validation_path: String,
    candidate_path: String,
    stage_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CommitStagingSnapshot {
    device: u64,
    inode: u64,
    bytes: u64,
    entries: u64,
    digest: [u8; 32],
}

impl CommitStaging {
    pub(super) fn stage_name(&self) -> &str {
        &self.stage_name
    }

    pub(super) fn stage_identity(&self) -> Result<(u64, u64), StoreError> {
        identity(&self._stage)
    }

    pub(super) fn create(
        state_home: &Path,
        stage_name: &str,
        provenance: &[u8],
        git_config: &[u8],
        index: Option<&[u8]>,
    ) -> Result<Self, StoreError> {
        const MAX_OPERATIONS: usize = 8;
        const MAX_AGGREGATE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
        const MAX_INDEX_BYTES: usize = 64 * 1024 * 1024;
        if !valid_stage_name(stage_name)
            || provenance.len() > 16 * 1024
            || git_config.len() > 16 * 1024
            || index.is_some_and(|bytes| bytes.len() > MAX_INDEX_BYTES)
        {
            return Err(StoreError::Unsafe);
        }
        let state_home = Directory::open_base(state_home)?;
        let state_rehow = state_home.ensure("rehow")?;
        let commit_staging = state_rehow.ensure("commit-staging")?;
        let commit_quarantine = state_rehow.ensure("commit-quarantine")?;
        let mut operations = 0usize;
        let mut aggregate = 0u64;
        for root in [&commit_staging, &commit_quarantine] {
            let names = root.entries(MAX_OPERATIONS + 1)?;
            for name in names {
                if !valid_stage_name(&name) {
                    return Err(StoreError::Unsafe);
                }
                operations = operations.checked_add(1).ok_or(StoreError::TooLarge)?;
                if operations >= MAX_OPERATIONS {
                    return Err(StoreError::TooLarge);
                }
                let operation = root.open_directory(&name)?;
                let mut operation_entries = 0usize;
                aggregate = aggregate
                    .checked_add(operation.bounded_tree_bytes_with_entry_limit(
                        MAX_AGGREGATE_BYTES.saturating_sub(aggregate),
                        &mut operation_entries,
                        COMMIT_STAGING_ENTRY_LIMIT,
                    )?)
                    .ok_or(StoreError::TooLarge)?;
                if aggregate > MAX_AGGREGATE_BYTES {
                    return Err(StoreError::TooLarge);
                }
            }
        }
        let prospective = provenance.len() as u64
            + git_config.len() as u64
            + index.map_or(0, |bytes| bytes.len() as u64);
        if aggregate.saturating_add(prospective) > MAX_AGGREGATE_BYTES {
            return Err(StoreError::TooLarge);
        }

        let stage = commit_staging.ensure_new(stage_name)?;
        stage.write_new_file("provenance", provenance)?;
        let git = stage.ensure_new("repo.git")?;
        let validation = stage.ensure_new("validation")?;
        let candidate = stage.ensure_new("candidate")?;
        git.write_new_file("HEAD", b"ref: refs/rehow/commit/head\n")?;
        git.write_new_file("config", git_config)?;
        if let Some(index) = index {
            git.write_new_file("index", index)?;
        }
        let objects = git.ensure_new("objects")?;
        let object_info = objects.ensure_new("info")?;
        let object_pack = objects.ensure_new("pack")?;
        let refs = git.ensure_new("refs")?;
        let rehow = refs.ensure_new("rehow")?;
        let commit = rehow.ensure_new("commit")?;
        let heads = refs.ensure_new("heads")?;
        let tags = refs.ensure_new("tags")?;
        for directory in [
            &object_info,
            &object_pack,
            &commit,
            &heads,
            &tags,
            &objects,
            &rehow,
            &refs,
            &git,
            &validation,
            &candidate,
            &stage,
        ] {
            directory.sync()?;
        }
        commit_staging.sync()?;
        commit_quarantine.sync()?;
        state_rehow.sync()?;
        state_home.sync()?;

        let stage_descriptor = stage.cloexec_duplicate()?;
        let git_descriptor = git.cloexec_duplicate()?;
        let stage_path = format!("/proc/self/fd/{}", stage_descriptor.as_raw_fd());
        let git_path = format!("/proc/self/fd/{}", git_descriptor.as_raw_fd());
        let validation_path = format!("/proc/self/fd/{}", validation.descriptor.as_raw_fd());
        let candidate_path = format!("/proc/self/fd/{}", candidate.descriptor.as_raw_fd());
        Ok(Self {
            commit_staging,
            commit_quarantine,
            _stage: stage_descriptor,
            _git: git_descriptor,
            validation,
            candidate,
            stage_path,
            git_path,
            validation_path,
            candidate_path,
            stage_name: stage_name.to_owned(),
        })
    }

    pub(super) fn stage_path(&self) -> &str {
        &self.stage_path
    }

    pub(super) fn git_path(&self) -> &str {
        &self.git_path
    }

    pub(super) fn git_descriptor(&self) -> i32 {
        self._git.as_raw_fd()
    }

    pub(super) fn validation_path(&self) -> &str {
        &self.validation_path
    }

    pub(super) fn candidate_path(&self) -> &str {
        &self.candidate_path
    }

    pub(super) fn candidate_descriptor(&self) -> i32 {
        self.candidate.descriptor.as_raw_fd()
    }

    pub(super) fn write_validation_file(
        &self,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        if bytes.len() > MAX_OBJECT_BYTES || !super::valid_active_path(relative_path) {
            return Err(StoreError::Unsafe);
        }
        let mut parts = relative_path.split('/').peekable();
        let mut directory = self.validation.duplicate()?;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                directory.write_new_file(part, bytes)?;
            } else {
                directory = directory.ensure(part)?;
            }
        }
        Ok(())
    }

    pub(super) fn finish_validation_tree(&self) -> Result<(), StoreError> {
        let categories = self.validation.ensure("categories")?;
        categories.sync()?;
        self.validation.sync()
    }

    pub(super) fn write_candidate_file(
        &self,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        if bytes.len() > MAX_OBJECT_BYTES || !super::valid_active_path(relative_path) {
            return Err(StoreError::Unsafe);
        }
        let mut parts = relative_path.split('/').peekable();
        let mut directory = self.candidate.duplicate()?;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                directory.write_new_file(part, bytes)?;
            } else {
                directory = directory.ensure(part)?;
            }
        }
        Ok(())
    }

    pub(super) fn finish_candidate_tree(&self) -> Result<(), StoreError> {
        let categories = self.candidate.ensure("categories")?;
        categories.sync()?;
        self.candidate.sync()
    }

    pub(super) fn verify_size_limit(&self, limit: u64) -> Result<(), StoreError> {
        let stage = Directory {
            descriptor: self._stage.try_clone().map_err(|_| StoreError::Io)?,
        };
        let mut entries = 0usize;
        let _ = stage.bounded_tree_bytes_with_entry_limit(
            limit,
            &mut entries,
            COMMIT_STAGING_ENTRY_LIMIT,
        )?;
        Ok(())
    }

    pub(super) fn snapshot(&self, limit: u64) -> Result<CommitStagingSnapshot, StoreError> {
        let root_status = status(self._stage.as_raw_fd())?;
        if root_status.st_uid != unsafe { libc::geteuid() }
            || root_status.st_mode & libc::S_IFMT != libc::S_IFDIR
        {
            return Err(StoreError::Unsafe);
        }
        let root = Directory {
            descriptor: self._stage.try_clone().map_err(|_| StoreError::Io)?,
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"rehow:git-commit-staging-snapshot:v1\0");
        let mut bytes = 0u64;
        let mut entries = 0usize;
        root.snapshot_tree(
            &mut hasher,
            b"",
            limit,
            &mut bytes,
            &mut entries,
            COMMIT_STAGING_ENTRY_LIMIT,
        )?;
        Ok(CommitStagingSnapshot {
            device: root_status.st_dev,
            inode: root_status.st_ino,
            bytes,
            entries: u64::try_from(entries).map_err(|_| StoreError::TooLarge)?,
            digest: *hasher.finalize().as_bytes(),
        })
    }

    pub(super) fn verify_snapshot(
        &self,
        expected: CommitStagingSnapshot,
        limit: u64,
    ) -> Result<bool, StoreError> {
        self.snapshot(limit).map(|current| current == expected)
    }

    pub(super) fn inspect_loose_object_ids(
        &self,
        object_id_hex_len: usize,
        object_limit: usize,
        aggregate_limit: u64,
    ) -> Result<Vec<String>, StoreError> {
        if !matches!(object_id_hex_len, 40 | 64) || object_limit == 0 {
            return Err(StoreError::Unsafe);
        }
        let git = Directory {
            descriptor: self._git.try_clone().map_err(|_| StoreError::Io)?,
        };
        let objects = git.open_directory("objects")?;
        let directories = objects.entries(258)?;
        let mut object_ids = Vec::new();
        let mut total = 0u64;
        for prefix in directories {
            if matches!(prefix.as_str(), "info" | "pack") {
                continue;
            }
            if prefix.len() != 2 || !prefix.bytes().all(is_lower_hex) {
                return Err(StoreError::Unsafe);
            }
            let directory = objects.open_directory_relaxed(&prefix)?;
            for suffix in directory.entries(object_limit.saturating_add(1))? {
                if object_ids.len() >= object_limit {
                    return Err(StoreError::TooLarge);
                }
                if suffix.len() != object_id_hex_len - 2 || !suffix.bytes().all(is_lower_hex) {
                    return Err(StoreError::Unsafe);
                }
                let (size, _) = directory
                    .hash_regular_relaxed(&suffix, aggregate_limit.saturating_sub(total))?;
                total = total.checked_add(size).ok_or(StoreError::TooLarge)?;
                if total > aggregate_limit {
                    return Err(StoreError::TooLarge);
                }
                object_ids.push(format!("{prefix}{suffix}"));
            }
        }
        object_ids.sort_unstable();
        if object_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(StoreError::Corrupt);
        }
        Ok(object_ids)
    }

    pub(super) fn inspect_pack_sources(
        &self,
        object_id_hex_len: usize,
        aggregate_limit: u64,
    ) -> Result<Vec<GitClonePackSource>, StoreError> {
        inspect_pack_sources(&self._git, object_id_hex_len, aggregate_limit, true)
    }

    pub(super) fn open_pack_source(&self, source: &GitClonePackSource) -> Result<File, StoreError> {
        open_pack_source(&self._git, source)
    }

    pub(super) fn inspect_index_source(
        &self,
        limit: u64,
    ) -> Result<GitClonePackSource, StoreError> {
        let git = Directory {
            descriptor: self._git.try_clone().map_err(|_| StoreError::Io)?,
        };
        let (size, hash) = git.hash_regular_relaxed("index", limit)?;
        Ok(GitClonePackSource {
            name: "index".to_owned(),
            size,
            hash,
        })
    }

    pub(super) fn open_index_source(
        &self,
        source: &GitClonePackSource,
        limit: u64,
    ) -> Result<File, StoreError> {
        if source.name != "index" || source.size > limit {
            return Err(StoreError::Unsafe);
        }
        let git = Directory {
            descriptor: self._git.try_clone().map_err(|_| StoreError::Io)?,
        };
        let mut file = git.open_regular_relaxed("index", source.size)?;
        verify_open_source(&mut file, source)?;
        Ok(file)
    }

    pub(super) fn replace_index(&self, bytes: &[u8]) -> Result<(), StoreError> {
        let git = Directory {
            descriptor: self._git.try_clone().map_err(|_| StoreError::Io)?,
        };
        let temp_name = temp_name("commit-index");
        let mut temp = TempEntry::create(&git, &temp_name)?;
        temp.file.write_all(bytes).map_err(|_| StoreError::Io)?;
        temp.file.sync_all().map_err(|_| StoreError::Unsupported)?;
        validate_file_descriptor(&temp.file, 0o600, Some(bytes.len() as u64))?;
        let temp_component = component(&temp_name)?;
        // SAFETY: both names are fixed validated components within retained private staging.
        if unsafe {
            libc::renameat(
                git.descriptor.as_raw_fd(),
                temp_component.as_ptr(),
                git.descriptor.as_raw_fd(),
                c"index".as_ptr(),
            )
        } != 0
        {
            return Err(StoreError::Io);
        }
        temp.installed = true;
        git.sync()?;
        let installed = git.open_file("index", libc::O_RDONLY)?;
        validate_file(&installed, 0o600, Some(bytes.len() as u64))
    }

    pub(super) fn quarantine(self) -> Result<(), StoreError> {
        let expected_identity = identity(&self._stage)?;
        let source = self.commit_staging.open_directory(&self.stage_name)?;
        if identity(&source.descriptor)? != expected_identity {
            return Err(StoreError::Conflict);
        }
        rename_no_replace_between(
            &self.commit_staging,
            &self.stage_name,
            &self.commit_quarantine,
            &self.stage_name,
        )?;
        self.commit_staging.sync()?;
        self.commit_quarantine.sync()?;
        let quarantined = self.commit_quarantine.open_directory(&self.stage_name)?;
        if identity(&quarantined.descriptor)? != expected_identity {
            return Err(StoreError::Conflict);
        }
        Ok(())
    }
}

fn valid_stage_name(stage_name: &str) -> bool {
    stage_name.len() == 64
        && stage_name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl CloneStaging {
    pub(super) fn create(
        state_home: &Path,
        stage_name: &str,
        provenance: &[u8],
        git_config: &[u8],
    ) -> Result<Self, StoreError> {
        if !valid_stage_name(stage_name)
            || provenance.len() > 16 * 1024
            || git_config.len() > 16 * 1024
        {
            return Err(StoreError::Unsafe);
        }
        let state_home = Directory::open_base(state_home)?;
        let state_rehow = state_home.ensure("rehow")?;
        let clone_staging = state_rehow.ensure("clone-staging")?;
        let stage = clone_staging.ensure_new(stage_name)?;
        stage.write_new_file("provenance", provenance)?;
        let git = stage.ensure_new("repo.git")?;
        let validation = stage.ensure_new("validation")?;
        git.write_new_file("HEAD", b"ref: refs/rehow/clone/head\n")?;
        git.write_new_file("config", git_config)?;
        let objects = git.ensure_new("objects")?;
        let object_info = objects.ensure_new("info")?;
        let object_pack = objects.ensure_new("pack")?;
        let refs = git.ensure_new("refs")?;
        let rehow = refs.ensure_new("rehow")?;
        let clone = rehow.ensure_new("clone")?;
        let heads = refs.ensure_new("heads")?;
        let tags = refs.ensure_new("tags")?;
        for directory in [
            &object_info,
            &object_pack,
            &clone,
            &heads,
            &tags,
            &objects,
            &rehow,
            &refs,
            &git,
            &validation,
            &stage,
        ] {
            directory.sync()?;
        }
        clone_staging.sync()?;
        state_rehow.sync()?;
        state_home.sync()?;

        let stage_descriptor = stage.cloexec_duplicate()?;
        let git_descriptor = git.cloexec_duplicate()?;
        let stage_path = format!("/proc/self/fd/{}", stage_descriptor.as_raw_fd());
        let git_path = format!("/proc/self/fd/{}", git_descriptor.as_raw_fd());
        let validation_path = format!("/proc/self/fd/{}", validation.descriptor.as_raw_fd());
        Ok(Self {
            _stage: stage_descriptor,
            _git: git_descriptor,
            validation,
            stage_path,
            git_path,
            validation_path,
        })
    }

    pub(super) fn git_path(&self) -> &str {
        &self.git_path
    }

    pub(super) fn git_descriptor(&self) -> i32 {
        self._git.as_raw_fd()
    }

    pub(super) fn stage_path(&self) -> &str {
        &self.stage_path
    }

    pub(super) fn validation_path(&self) -> &str {
        &self.validation_path
    }

    pub(super) fn write_validation_file(
        &self,
        relative_path: &str,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        if bytes.len() > MAX_OBJECT_BYTES
            || (relative_path != ".gitignore" && !super::valid_active_path(relative_path))
        {
            return Err(StoreError::Unsafe);
        }
        let mut parts = relative_path.split('/').peekable();
        let mut directory = self.validation.duplicate()?;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                directory.write_new_file(part, bytes)?;
            } else {
                directory = directory.ensure(part)?;
            }
        }
        Ok(())
    }

    pub(super) fn finish_validation_tree(&self) -> Result<(), StoreError> {
        let categories = self.validation.ensure("categories")?;
        categories.sync()?;
        self.validation.sync()
    }

    pub(super) fn read_validation_file(
        &self,
        relative_path: &str,
        expected_size: u64,
    ) -> Result<Vec<u8>, StoreError> {
        if expected_size > MAX_OBJECT_BYTES as u64
            || (relative_path != ".gitignore" && !super::valid_active_path(relative_path))
        {
            return Err(StoreError::Unsafe);
        }
        let mut parts = relative_path.split('/').peekable();
        let mut directory = self.validation.duplicate()?;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                let limit = usize::try_from(expected_size).map_err(|_| StoreError::TooLarge)?;
                let bytes = directory.read_regular(part, limit)?;
                return if bytes.len() as u64 == expected_size {
                    Ok(bytes)
                } else {
                    Err(StoreError::Corrupt)
                };
            }
            directory = directory.open_directory(part)?;
        }
        Err(StoreError::Unsafe)
    }

    pub(super) fn verify_git_size_limit(&self, limit: u64) -> Result<(), StoreError> {
        self.git_state(limit).map(|_| ())
    }

    pub(super) fn git_state(&self, limit: u64) -> Result<CloneGitState, StoreError> {
        let git = Directory {
            descriptor: self._git.try_clone().map_err(|_| StoreError::Io)?,
        };
        let directory_before = status(git.descriptor.as_raw_fd())?;
        let mut entries = 0usize;
        let bytes = git.bounded_tree_bytes(limit, &mut entries)?;
        let shallow = if git.missing("shallow")? {
            None
        } else {
            let bytes = git.read_regular("shallow", 1024 * 1024)?;
            let name = component("shallow")?;
            let state = status_at(git.descriptor.as_raw_fd(), &name)?;
            validate_status(&state, 0o600, Some(bytes.len() as u64))?;
            Some((bytes, state))
        };
        let directory_after = status(git.descriptor.as_raw_fd())?;
        if !same_file_state(&directory_before, &directory_after) {
            return Err(StoreError::Conflict);
        }
        let mut binding = blake3::Hasher::new();
        binding.update(b"rehow:clone-git-state:v1\0");
        hash_file_state(&mut binding, &directory_after);
        match shallow {
            Some((bytes, state)) => {
                binding.update(b"shallow\0");
                hash_file_state(&mut binding, &state);
                binding.update(&bytes);
            }
            None => {
                binding.update(b"no-shallow\0");
            }
        }
        Ok(CloneGitState {
            bytes,
            binding: *binding.finalize().as_bytes(),
        })
    }

    pub(super) fn inspect_pack_sources(
        &self,
        object_id_hex_len: usize,
        aggregate_limit: u64,
    ) -> Result<Vec<GitClonePackSource>, StoreError> {
        inspect_pack_sources(&self._git, object_id_hex_len, aggregate_limit, false)
    }

    pub(super) fn open_pack_source(&self, source: &GitClonePackSource) -> Result<File, StoreError> {
        open_pack_source(&self._git, source)
    }
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn inspect_pack_sources(
    git_descriptor: &OwnedFd,
    object_id_hex_len: usize,
    aggregate_limit: u64,
    require_single_pack: bool,
) -> Result<Vec<GitClonePackSource>, StoreError> {
    if !matches!(object_id_hex_len, 40 | 64) {
        return Err(StoreError::Unsafe);
    }
    let git = Directory {
        descriptor: git_descriptor.try_clone().map_err(|_| StoreError::Io)?,
    };
    let pack = git.open_directory("objects")?.open_directory("pack")?;
    let entries = pack.entries(192)?;
    let mut stems = std::collections::BTreeMap::<String, (bool, bool)>::new();
    for name in &entries {
        let Some((stem, suffix)) = name.rsplit_once('.') else {
            return Err(StoreError::Unsafe);
        };
        if stem.len() != 5 + object_id_hex_len
            || !stem.starts_with("pack-")
            || !stem[5..].bytes().all(is_lower_hex)
        {
            return Err(StoreError::Unsafe);
        }
        let pair = stems.entry(stem.to_owned()).or_default();
        match suffix {
            "pack" if !pair.0 => pair.0 = true,
            "idx" if !pair.1 => pair.1 = true,
            "promisor" if !require_single_pack => {}
            _ => return Err(StoreError::Unsafe),
        }
    }
    if stems.is_empty()
        || (require_single_pack && stems.len() != 1)
        || stems.values().any(|pair| *pair != (true, true))
    {
        return Err(StoreError::Corrupt);
    }

    let mut sources = Vec::new();
    let mut total = 0u64;
    for name in entries
        .into_iter()
        .filter(|name| name.ends_with(".pack") || name.ends_with(".idx"))
    {
        let (size, hash) =
            pack.hash_regular_relaxed(&name, aggregate_limit.saturating_sub(total))?;
        total = total.checked_add(size).ok_or(StoreError::TooLarge)?;
        if total > aggregate_limit {
            return Err(StoreError::TooLarge);
        }
        sources.push(GitClonePackSource { name, size, hash });
    }
    sources.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(sources)
}

fn open_pack_source(
    git_descriptor: &OwnedFd,
    source: &GitClonePackSource,
) -> Result<File, StoreError> {
    let git = Directory {
        descriptor: git_descriptor.try_clone().map_err(|_| StoreError::Io)?,
    };
    let pack = git.open_directory("objects")?.open_directory("pack")?;
    let mut file = pack.open_regular_relaxed(&source.name, source.size)?;
    verify_open_source(&mut file, source)?;
    Ok(file)
}

fn verify_open_source(file: &mut File, source: &GitClonePackSource) -> Result<(), StoreError> {
    let before = status(file.as_raw_fd())?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(|_| StoreError::Io)?;
        if read == 0 {
            break;
        }
        size = size.checked_add(read as u64).ok_or(StoreError::TooLarge)?;
        if size > source.size {
            return Err(StoreError::Corrupt);
        }
        hasher.update(&buffer[..read]);
    }
    let after = status(file.as_raw_fd())?;
    if size != source.size
        || !same_file_state(&before, &after)
        || hasher.finalize().as_bytes() != &source.hash
    {
        return Err(StoreError::Corrupt);
    }
    file.seek(SeekFrom::Start(0)).map_err(|_| StoreError::Io)?;
    Ok(())
}

impl CloneStaging {
    pub(super) fn inspect_index_source(
        &self,
        staging_name: &str,
    ) -> Result<GitClonePackSource, StoreError> {
        validate_clone_index_name(staging_name)?;
        let git = Directory {
            descriptor: self._git.try_clone().map_err(|_| StoreError::Io)?,
        };
        let (size, hash) = git.hash_regular_relaxed(staging_name, 32 * 1024 * 1024)?;
        Ok(GitClonePackSource {
            name: "index".to_owned(),
            size,
            hash,
        })
    }

    pub(super) fn open_index_source(
        &self,
        staging_name: &str,
        source: &GitClonePackSource,
    ) -> Result<File, StoreError> {
        validate_clone_index_name(staging_name)?;
        if source.name != "index" || source.size > 32 * 1024 * 1024 {
            return Err(StoreError::Unsafe);
        }
        let git = Directory {
            descriptor: self._git.try_clone().map_err(|_| StoreError::Io)?,
        };
        let mut file = git.open_regular_relaxed(staging_name, source.size)?;
        let before = status(file.as_raw_fd())?;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            let read = file.read(&mut buffer).map_err(|_| StoreError::Io)?;
            if read == 0 {
                break;
            }
            size = size.checked_add(read as u64).ok_or(StoreError::TooLarge)?;
            hasher.update(&buffer[..read]);
        }
        let after = status(file.as_raw_fd())?;
        if size != source.size
            || !same_file_state(&before, &after)
            || hasher.finalize().as_bytes() != &source.hash
        {
            return Err(StoreError::Corrupt);
        }
        file.seek(SeekFrom::Start(0)).map_err(|_| StoreError::Io)?;
        Ok(file)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StoreError {
    Unsafe,
    Busy,
    Conflict,
    Corrupt,
    Unsupported,
    TooLarge,
    Io,
    #[cfg(test)]
    Injected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FaultPoint {
    ObjectAfterWrite,
    ObjectAfterFileSync,
    ObjectAfterRename,
    ObjectAfterDirectorySync,
    JournalAfterWrite,
    JournalAfterFileSync,
    JournalAfterRename,
    JournalAfterDirectorySync,
    QuarantineAfterRename,
    QuarantineAfterStagingSync,
    QuarantineAfterQuarantineSync,
    QuarantineAfterStateSync,
}

impl RecoveryStore {
    pub(super) fn quarantine_git_commit_staging(
        &self,
        lock: &WriterLock,
        stage_name: &str,
        expected_identity: (u64, u64),
    ) -> Result<(), StoreError> {
        self.require_lock(lock)?;
        if !valid_stage_name(stage_name) {
            return Err(StoreError::Unsafe);
        }
        let staging = self.state_rehow.open_directory("commit-staging")?;
        let quarantine = self.state_rehow.open_directory("commit-quarantine")?;
        let source = staging.optional_directory(stage_name)?;
        let retained = quarantine.optional_directory(stage_name)?;
        match (source, retained) {
            (Some(source), None) => {
                if identity(&source.descriptor)? != expected_identity {
                    return Err(StoreError::Conflict);
                }
                rename_no_replace_between(&staging, stage_name, &quarantine, stage_name)?;
                self.fail(FaultPoint::QuarantineAfterRename)?;
            }
            (None, Some(retained)) => {
                if identity(&retained.descriptor)? != expected_identity {
                    return Err(StoreError::Conflict);
                }
            }
            _ => return Err(StoreError::Conflict),
        }
        let retained = quarantine.open_directory(stage_name)?;
        if identity(&retained.descriptor)? != expected_identity {
            return Err(StoreError::Conflict);
        }
        // Repeat the complete durability sequence when the retained entry already exists. A
        // prior attempt may have completed the rename but failed at any following sync.
        staging.sync()?;
        self.fail(FaultPoint::QuarantineAfterStagingSync)?;
        quarantine.sync()?;
        self.fail(FaultPoint::QuarantineAfterQuarantineSync)?;
        self.state_rehow.sync()?;
        self.fail(FaultPoint::QuarantineAfterStateSync)?;
        Ok(())
    }

    pub(super) fn open(
        state_home: &Path,
        workspace_id: WorkspaceId,
        root_hash: ContentHash,
        capability_hash: ContentHash,
    ) -> Result<Self, StoreError> {
        let state_home = Directory::open_base(state_home)?;
        let state_rehow = state_home.ensure("rehow")?;
        let workspaces = state_rehow.ensure("workspaces")?;
        let workspace_name = format!("{}-{}", workspace_id, hex(&root_hash));
        let workspace = workspaces.ensure(&workspace_name)?;
        let objects = workspace.ensure("objects")?;
        let journals = workspace.ensure("journals")?;
        Ok(Self {
            state_rehow,
            objects,
            journals,
            capability_hash,
            workspace_id,
            root_hash,
            #[cfg(test)]
            fault: std::cell::Cell::new(None),
        })
    }

    pub(super) fn acquire_lock(
        &self,
        runtime_directory: Option<&Path>,
    ) -> Result<WriterLock, StoreError> {
        let lock_directory = if let Some(runtime_directory) = runtime_directory {
            Directory::open_base(runtime_directory)?.ensure("rehow")?
        } else {
            self.state_rehow.ensure("locks")?
        };
        let name = format!("{}.lock", hex(&self.capability_hash));
        let descriptor = lock_directory.open_or_create_file(&name)?;
        validate_file(&descriptor, 0o600, None)?;
        // SAFETY: the validated descriptor is open and owned for the guard lifetime.
        if unsafe { libc::flock(descriptor.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return match io::Error::last_os_error().raw_os_error() {
                Some(libc::EWOULDBLOCK) => Err(StoreError::Busy),
                _ => Err(StoreError::Io),
            };
        }
        let current = lock_directory.open_file(&name, libc::O_RDWR)?;
        if identity(&descriptor)? != identity(&current)? {
            return Err(StoreError::Unsafe);
        }
        Ok(WriterLock {
            descriptor,
            capability_hash: self.capability_hash,
        })
    }

    pub(super) fn store_object(
        &self,
        lock: &WriterLock,
        bytes: &[u8],
    ) -> Result<ContentHash, StoreError> {
        self.require_lock(lock)?;
        if bytes.len() > MAX_OBJECT_BYTES {
            return Err(StoreError::TooLarge);
        }
        let hash = *blake3::hash(bytes).as_bytes();
        let name = hex(&hash);
        match self.read_object_by_name(&name) {
            Ok(existing) if existing == bytes => return Ok(hash),
            Ok(_) => return Err(StoreError::Corrupt),
            Err(StoreError::Io) if self.objects.missing(&name)? => {}
            Err(error) => return Err(error),
        }

        let temp_name = temp_name("object");
        let mut temp = TempEntry::create(&self.objects, &temp_name)?;
        temp.file.write_all(bytes).map_err(|_| StoreError::Io)?;
        self.fail(FaultPoint::ObjectAfterWrite)?;
        temp.file.sync_all().map_err(|_| StoreError::Io)?;
        self.fail(FaultPoint::ObjectAfterFileSync)?;
        validate_file_descriptor(&temp.file, 0o600, Some(bytes.len() as u64))?;
        match rename_no_replace(&self.objects, &temp_name, &name) {
            Ok(()) => temp.installed = true,
            Err(StoreError::Conflict) => {
                let existing = self.read_object_by_name(&name)?;
                if existing != bytes {
                    return Err(StoreError::Corrupt);
                }
                return Ok(hash);
            }
            Err(error) => return Err(error),
        }
        self.fail(FaultPoint::ObjectAfterRename)?;
        self.objects.sync()?;
        self.fail(FaultPoint::ObjectAfterDirectorySync)?;
        let installed = self.read_object_by_name(&name)?;
        if installed == bytes {
            Ok(hash)
        } else {
            Err(StoreError::Corrupt)
        }
    }

    pub(super) fn read_object(
        &self,
        lock: &WriterLock,
        hash: &ContentHash,
    ) -> Result<Vec<u8>, StoreError> {
        self.require_lock(lock)?;
        self.read_object_by_name(&hex(hash))
    }

    pub(super) fn store_streamed_object(
        &self,
        lock: &WriterLock,
        source: &mut File,
        limit: u64,
        expected: &StreamedObjectExpectation<'_>,
    ) -> Result<StreamedObject, StoreError> {
        use sha2::Digest;

        self.require_lock(lock)?;
        if limit > MAX_OBJECT_BYTES as u64 {
            return Err(StoreError::TooLarge);
        }
        let before = source.metadata().map_err(|_| StoreError::Io)?;
        if !before.file_type().is_file() || before.nlink() != 1 {
            return Err(StoreError::Unsafe);
        }
        if !expected.source_state.matches(&before) || before.len() != expected.size {
            return Err(StoreError::Conflict);
        }
        if before.len() > limit {
            return Err(StoreError::TooLarge);
        }
        source
            .seek(SeekFrom::Start(0))
            .map_err(|_| StoreError::Io)?;
        let temp_name = temp_name("object");
        let mut temp = TempEntry::create(&self.objects, &temp_name)?;
        let mut blake3 = blake3::Hasher::new();
        let mut sha256 = sha2::Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut size = 0_u64;
        loop {
            let remaining = limit.saturating_sub(size).saturating_add(1);
            let read_limit = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| StoreError::TooLarge)?;
            let read = source
                .read(&mut buffer[..read_limit])
                .map_err(|_| StoreError::Io)?;
            if read == 0 {
                break;
            }
            #[cfg(test)]
            if SOURCE_CHANGE_FAULT.replace(false) {
                let changed_mode = before.mode() ^ 0o100;
                // SAFETY: the retained selected-file descriptor is valid; this test-only fault
                // changes metadata so the production stability check must reject the copy.
                if unsafe { libc::fchmod(source.as_raw_fd(), changed_mode) } != 0 {
                    return Err(StoreError::Io);
                }
            }
            size = size.checked_add(read as u64).ok_or(StoreError::TooLarge)?;
            if size > limit {
                return Err(StoreError::TooLarge);
            }
            blake3.update(&buffer[..read]);
            sha256.update(&buffer[..read]);
            temp.file
                .write_all(&buffer[..read])
                .map_err(|_| StoreError::Io)?;
        }
        let after = source.metadata().map_err(|_| StoreError::Io)?;
        if !after.file_type().is_file()
            || after.nlink() != 1
            || before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.len() != after.len()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
            || before.mode() != after.mode()
            || size != after.len()
        {
            return Err(StoreError::Conflict);
        }
        temp.file.sync_all().map_err(|_| StoreError::Unsupported)?;
        validate_file_descriptor(&temp.file, 0o600, Some(size))?;
        let object_hash = *blake3.finalize().as_bytes();
        let sha256: [u8; 32] = sha256.finalize().into();
        if &object_hash != expected.object_hash
            || &sha256 != expected.sha256
            || size != expected.size
        {
            return Err(StoreError::Conflict);
        }
        let name = hex(&object_hash);
        match rename_no_replace(&self.objects, &temp_name, &name) {
            Ok(()) => temp.installed = true,
            Err(StoreError::Conflict) => {
                self.open_object(lock, &object_hash)?;
                return Ok(StreamedObject {
                    object_hash,
                    sha256,
                    size,
                    created: false,
                });
            }
            Err(error) => return Err(error),
        }
        self.objects.sync()?;
        self.open_object(lock, &object_hash)?;
        Ok(StreamedObject {
            object_hash,
            sha256,
            size,
            created: true,
        })
    }

    pub(super) fn remove_created_object(
        &self,
        lock: &WriterLock,
        hash: &ContentHash,
    ) -> Result<(), StoreError> {
        self.require_lock(lock)?;
        self.open_object(lock, hash)?;
        let name = component(&hex(hash))?;
        // SAFETY: directory descriptor and validated object name are valid.
        if unsafe { libc::unlinkat(self.objects.descriptor.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(StoreError::Io);
        }
        self.objects.sync()
    }

    pub(super) fn open_object(
        &self,
        lock: &WriterLock,
        hash: &ContentHash,
    ) -> Result<(File, u64), StoreError> {
        self.require_lock(lock)?;
        let descriptor = self.objects.open_file(&hex(hash), libc::O_RDONLY)?;
        validate_file(&descriptor, 0o600, None)?;
        let initial = status(descriptor.as_raw_fd())?;
        if initial.st_size < 0 || initial.st_size as u64 > MAX_OBJECT_BYTES as u64 {
            return Err(StoreError::TooLarge);
        }
        let mut file = File::from(descriptor);
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut size = 0_u64;
        loop {
            let read = file.read(&mut buffer).map_err(|_| StoreError::Io)?;
            if read == 0 {
                break;
            }
            size = size.checked_add(read as u64).ok_or(StoreError::TooLarge)?;
            if size > MAX_OBJECT_BYTES as u64 {
                return Err(StoreError::TooLarge);
            }
            hasher.update(&buffer[..read]);
        }
        let final_status = status(file.as_raw_fd())?;
        if !same_file_state(&initial, &final_status)
            || size != final_status.st_size as u64
            || hasher.finalize().as_bytes() != hash
        {
            return Err(StoreError::Corrupt);
        }
        file.seek(SeekFrom::Start(0)).map_err(|_| StoreError::Io)?;
        Ok((file, size))
    }

    pub(super) fn install_journal(
        &self,
        lock: &WriterLock,
        journal: &Journal,
    ) -> Result<(), StoreError> {
        self.require_lock(lock)?;
        if journal.workspace_id != self.workspace_id || journal.root_hash != self.root_hash {
            return Err(StoreError::Conflict);
        }
        let bytes = journal.encode().map_err(map_codec)?;
        let name = format!("{}.cbor", journal.operation_id);
        if journal.generation == 1 {
            if !self.journals.missing(&name)? {
                return Err(StoreError::Conflict);
            }
        } else {
            let previous = self.read_journal_by_name(&name, journal.operation_id)?;
            if !previous.is_next_generation_of(journal) {
                return Err(StoreError::Conflict);
            }
        }

        let temp_name = temp_name("journal");
        let mut temp = TempEntry::create(&self.journals, &temp_name)?;
        temp.file.write_all(&bytes).map_err(|_| StoreError::Io)?;
        self.fail(FaultPoint::JournalAfterWrite)?;
        temp.file.sync_all().map_err(|_| StoreError::Io)?;
        self.fail(FaultPoint::JournalAfterFileSync)?;
        validate_file_descriptor(&temp.file, 0o600, Some(bytes.len() as u64))?;
        if journal.generation == 1 {
            rename_no_replace(&self.journals, &temp_name, &name)?;
        } else {
            rename_replace(&self.journals, &temp_name, &name)?;
        }
        temp.installed = true;
        self.fail(FaultPoint::JournalAfterRename)?;
        self.journals.sync()?;
        self.fail(FaultPoint::JournalAfterDirectorySync)?;
        if self.read_journal_by_name(&name, journal.operation_id)? == *journal {
            Ok(())
        } else {
            Err(StoreError::Corrupt)
        }
    }

    pub(super) fn read_journal(
        &self,
        lock: &WriterLock,
        operation_id: OperationId,
    ) -> Result<Journal, StoreError> {
        self.require_lock(lock)?;
        self.read_journal_by_name(&format!("{operation_id}.cbor"), operation_id)
    }

    pub(super) fn journal_missing(
        &self,
        lock: &WriterLock,
        operation_id: OperationId,
    ) -> Result<bool, StoreError> {
        self.require_lock(lock)?;
        self.journals.missing(&format!("{operation_id}.cbor"))
    }

    pub(super) fn install_git_commit_intent(
        &self,
        lock: &WriterLock,
        intent: &GitCommitIntent,
    ) -> Result<(), StoreError> {
        self.require_lock(lock)?;
        if intent.workspace_id != self.workspace_id || intent.root_hash != self.root_hash {
            return Err(StoreError::Conflict);
        }
        let bytes = intent.encode().map_err(map_codec)?;
        let name = format!("{}.gitcommit", intent.operation_id);
        if intent.generation == 1 {
            if !self.journals.missing(&name)? {
                return Err(StoreError::Conflict);
            }
        } else {
            let previous = self.read_git_commit_intent_by_name(&name, intent.operation_id)?;
            if !previous.is_next_generation_of(intent) {
                return Err(StoreError::Conflict);
            }
        }
        let temp_name = temp_name("git-commit-intent");
        let mut temp = TempEntry::create(&self.journals, &temp_name)?;
        temp.file.write_all(&bytes).map_err(|_| StoreError::Io)?;
        temp.file.sync_all().map_err(|_| StoreError::Unsupported)?;
        validate_file_descriptor(&temp.file, 0o600, Some(bytes.len() as u64))?;
        if intent.generation == 1 {
            rename_no_replace(&self.journals, &temp_name, &name)?;
        } else {
            rename_replace(&self.journals, &temp_name, &name)?;
        }
        temp.installed = true;
        self.journals.sync()?;
        if self.read_git_commit_intent_by_name(&name, intent.operation_id)? == *intent {
            Ok(())
        } else {
            Err(StoreError::Corrupt)
        }
    }

    pub(super) fn read_git_commit_intent(
        &self,
        lock: &WriterLock,
        operation_id: OperationId,
    ) -> Result<GitCommitIntent, StoreError> {
        self.require_lock(lock)?;
        self.read_git_commit_intent_by_name(&format!("{operation_id}.gitcommit"), operation_id)
    }

    pub(super) fn list_git_commit_intents(
        &self,
        lock: &WriterLock,
    ) -> Result<Vec<GitCommitIntent>, StoreError> {
        self.require_lock(lock)?;
        let entries = self.journals.entries(1_024)?;
        let mut intents = Vec::new();
        for name in entries {
            let Some(operation) = name.strip_suffix(".gitcommit") else {
                continue;
            };
            let operation_id = operation.parse().map_err(|_| StoreError::Corrupt)?;
            intents.push(self.read_git_commit_intent_by_name(&name, operation_id)?);
        }
        intents.sort_by_key(|intent| intent.operation_id.to_string());
        Ok(intents)
    }

    fn read_object_by_name(&self, name: &str) -> Result<Vec<u8>, StoreError> {
        let bytes = self.objects.read_regular(name, MAX_OBJECT_BYTES)?;
        if hex(blake3::hash(&bytes).as_bytes()) != name {
            return Err(StoreError::Corrupt);
        }
        Ok(bytes)
    }

    fn read_journal_by_name(
        &self,
        name: &str,
        operation_id: OperationId,
    ) -> Result<Journal, StoreError> {
        let bytes = self.journals.read_regular(name, MAX_JOURNAL_BYTES)?;
        let journal = Journal::decode(&bytes).map_err(map_codec)?;
        if journal.operation_id == operation_id
            && journal.workspace_id == self.workspace_id
            && journal.root_hash == self.root_hash
        {
            Ok(journal)
        } else {
            Err(StoreError::Corrupt)
        }
    }

    fn read_git_commit_intent_by_name(
        &self,
        name: &str,
        operation_id: OperationId,
    ) -> Result<GitCommitIntent, StoreError> {
        let bytes = self
            .journals
            .read_regular(name, MAX_GIT_COMMIT_INTENT_BYTES)?;
        let intent = GitCommitIntent::decode(&bytes).map_err(map_codec)?;
        if intent.operation_id == operation_id
            && intent.workspace_id == self.workspace_id
            && intent.root_hash == self.root_hash
        {
            Ok(intent)
        } else {
            Err(StoreError::Corrupt)
        }
    }

    fn require_lock(&self, lock: &WriterLock) -> Result<(), StoreError> {
        if lock.capability_hash == self.capability_hash {
            validate_file(&lock.descriptor, 0o600, None)
        } else {
            Err(StoreError::Unsafe)
        }
    }

    #[cfg(test)]
    fn set_fault(&self, fault: FaultPoint) {
        self.fault.set(Some(fault));
    }

    #[cfg(test)]
    fn fail(&self, point: FaultPoint) -> Result<(), StoreError> {
        if self.fault.get() == Some(point) {
            self.fault.set(None);
            Err(StoreError::Injected)
        } else {
            Ok(())
        }
    }

    #[cfg(not(test))]
    fn fail(&self, _point: FaultPoint) -> Result<(), StoreError> {
        Ok(())
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        // SAFETY: descriptor ownership remains with the guard; unlocking cannot invalidate it.
        unsafe { libc::flock(self.descriptor.as_raw_fd(), libc::LOCK_UN) };
    }
}

struct Directory {
    descriptor: OwnedFd,
}

struct DirectoryStream(*mut libc::DIR);

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        // SAFETY: this guard uniquely owns the live stream.
        unsafe { libc::closedir(self.0) };
    }
}

impl Directory {
    fn entries(&self, limit: usize) -> Result<Vec<String>, StoreError> {
        // SAFETY: openat on `.` creates an independent no-follow directory description.
        let raw = unsafe {
            libc::openat(
                self.descriptor.as_raw_fd(),
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if raw < 0 {
            return Err(StoreError::Io);
        }
        // SAFETY: fdopendir consumes the duplicated descriptor on success.
        let stream = unsafe { libc::fdopendir(raw) };
        if stream.is_null() {
            // SAFETY: fdopendir did not consume the descriptor on failure.
            unsafe { libc::close(raw) };
            return Err(StoreError::Io);
        }
        let stream = DirectoryStream(stream);
        let mut names = Vec::new();
        loop {
            // SAFETY: clear and inspect the current thread's errno around readdir.
            unsafe { *libc::__errno_location() = 0 };
            // SAFETY: stream remains valid for this call.
            let entry = unsafe { libc::readdir(stream.0) };
            if entry.is_null() {
                if unsafe { *libc::__errno_location() } != 0 {
                    return Err(StoreError::Io);
                }
                break;
            }
            // SAFETY: d_name is NUL-terminated for the live entry.
            let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            if names.len() >= limit {
                return Err(StoreError::TooLarge);
            }
            let name = std::str::from_utf8(bytes).map_err(|_| StoreError::Unsafe)?;
            names.push(name.to_owned());
        }
        names.sort_unstable();
        Ok(names)
    }

    fn open_regular_relaxed(&self, name: &str, expected_size: u64) -> Result<File, StoreError> {
        let descriptor = self.open_file(name, libc::O_RDONLY)?;
        let current = status(descriptor.as_raw_fd())?;
        if current.st_uid != unsafe { libc::geteuid() }
            || current.st_mode & libc::S_IFMT != libc::S_IFREG
            || current.st_nlink != 1
            || current.st_size < 0
            || current.st_size as u64 != expected_size
            || current.st_mode & 0o022 != 0
        {
            return Err(StoreError::Unsafe);
        }
        Ok(File::from(descriptor))
    }

    fn hash_regular_relaxed(&self, name: &str, limit: u64) -> Result<(u64, [u8; 32]), StoreError> {
        let descriptor = self.open_file(name, libc::O_RDONLY)?;
        let before = status(descriptor.as_raw_fd())?;
        if before.st_uid != unsafe { libc::geteuid() }
            || before.st_mode & libc::S_IFMT != libc::S_IFREG
            || before.st_nlink != 1
            || before.st_size < 0
            || before.st_size as u64 > limit
            || before.st_mode & 0o022 != 0
        {
            return Err(StoreError::Unsafe);
        }
        let mut file = File::from(descriptor);
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            let read = file.read(&mut buffer).map_err(|_| StoreError::Io)?;
            if read == 0 {
                break;
            }
            size = size.checked_add(read as u64).ok_or(StoreError::TooLarge)?;
            if size > limit {
                return Err(StoreError::TooLarge);
            }
            hasher.update(&buffer[..read]);
        }
        let after = status(file.as_raw_fd())?;
        if size != before.st_size as u64 || !same_file_state(&before, &after) {
            return Err(StoreError::Corrupt);
        }
        Ok((size, *hasher.finalize().as_bytes()))
    }

    fn bounded_tree_bytes(&self, limit: u64, entries: &mut usize) -> Result<u64, StoreError> {
        self.bounded_tree_bytes_with_entry_limit(limit, entries, DEFAULT_TREE_ENTRY_LIMIT)
    }

    fn snapshot_tree(
        &self,
        hasher: &mut blake3::Hasher,
        prefix: &[u8],
        limit: u64,
        total: &mut u64,
        entries: &mut usize,
        entry_limit: usize,
    ) -> Result<(), StoreError> {
        let names = self.entries(entry_limit.saturating_sub(*entries).saturating_add(1))?;
        for name in names {
            *entries = entries.checked_add(1).ok_or(StoreError::TooLarge)?;
            if *entries > entry_limit {
                return Err(StoreError::TooLarge);
            }
            let mut path = Vec::new();
            path.try_reserve_exact(prefix.len().saturating_add(name.len()).saturating_add(1))
                .map_err(|_| StoreError::TooLarge)?;
            if !prefix.is_empty() {
                path.extend_from_slice(prefix);
                path.push(b'/');
            }
            path.extend_from_slice(name.as_bytes());
            let name_c = component(&name)?;
            let entry_status = status_at(self.descriptor.as_raw_fd(), &name_c)?;
            if entry_status.st_uid != unsafe { libc::geteuid() } {
                return Err(StoreError::Unsafe);
            }
            snapshot_hash_frame(hasher, &path);
            match entry_status.st_mode & libc::S_IFMT {
                libc::S_IFREG if entry_status.st_nlink == 1 && entry_status.st_size >= 0 => {
                    let remaining = limit.saturating_sub(*total);
                    let (size, digest) = self.hash_regular_relaxed(&name, remaining)?;
                    *total = total.checked_add(size).ok_or(StoreError::TooLarge)?;
                    if *total > limit {
                        return Err(StoreError::TooLarge);
                    }
                    hasher.update(b"file\0");
                    hasher.update(&(entry_status.st_mode & 0o7777).to_be_bytes());
                    hasher.update(&size.to_be_bytes());
                    hasher.update(&digest);
                }
                libc::S_IFDIR => {
                    hasher.update(b"directory\0");
                    hasher.update(&(entry_status.st_mode & 0o7777).to_be_bytes());
                    let child = self.open_directory_relaxed(&name)?;
                    if identity(&child.descriptor)? != (entry_status.st_dev, entry_status.st_ino) {
                        return Err(StoreError::Corrupt);
                    }
                    child.snapshot_tree(hasher, &path, limit, total, entries, entry_limit)?;
                }
                _ => return Err(StoreError::Unsafe),
            }
            let final_status = status_at(self.descriptor.as_raw_fd(), &name_c)?;
            if !same_file_state(&entry_status, &final_status) {
                return Err(StoreError::Corrupt);
            }
        }
        Ok(())
    }

    fn bounded_tree_bytes_with_entry_limit(
        &self,
        limit: u64,
        entries: &mut usize,
        entry_limit: usize,
    ) -> Result<u64, StoreError> {
        // SAFETY: openat on `.` creates an independent no-follow directory description.
        let raw = unsafe {
            libc::openat(
                self.descriptor.as_raw_fd(),
                c".".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if raw < 0 {
            return Err(StoreError::Io);
        }
        // SAFETY: fdopendir consumes the duplicated directory descriptor on success.
        let stream = unsafe { libc::fdopendir(raw) };
        if stream.is_null() {
            // SAFETY: fdopendir failed and did not consume the descriptor.
            unsafe { libc::close(raw) };
            return Err(StoreError::Io);
        }
        let stream = DirectoryStream(stream);
        let mut total = 0u64;
        loop {
            // SAFETY: Linux exposes the calling thread's errno slot; clearing it
            // distinguishes readdir EOF from an I/O failure.
            unsafe { *libc::__errno_location() = 0 };
            // SAFETY: stream remains valid until closed below.
            let entry = unsafe { libc::readdir(stream.0) };
            if entry.is_null() {
                // SAFETY: this reads the same thread-local errno slot.
                if unsafe { *libc::__errno_location() } != 0 {
                    return Err(StoreError::Io);
                }
                break;
            }
            // SAFETY: readdir returns a NUL-terminated d_name within the live entry.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            *entries = entries.checked_add(1).ok_or(StoreError::TooLarge)?;
            if *entries > entry_limit {
                return Err(StoreError::TooLarge);
            }
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptors, entry name, and output storage are valid.
            if unsafe {
                libc::fstatat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    status.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return Err(StoreError::Io);
            }
            // SAFETY: successful fstatat initialized the complete structure.
            let status = unsafe { status.assume_init() };
            if status.st_uid != unsafe { libc::geteuid() } {
                return Err(StoreError::Unsafe);
            }
            match status.st_mode & libc::S_IFMT {
                libc::S_IFREG if status.st_nlink == 1 && status.st_size >= 0 => {
                    total = total
                        .checked_add(status.st_size as u64)
                        .ok_or(StoreError::TooLarge)?;
                }
                libc::S_IFDIR => {
                    // SAFETY: name and parent descriptor are valid; no symlink is followed.
                    let child = unsafe {
                        libc::openat(
                            self.descriptor.as_raw_fd(),
                            name.as_ptr(),
                            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                        )
                    };
                    if child < 0 {
                        return Err(StoreError::Unsafe);
                    }
                    // SAFETY: successful openat returned a newly owned descriptor.
                    let child = Self {
                        descriptor: unsafe { OwnedFd::from_raw_fd(child) },
                    };
                    total = total
                        .checked_add(child.bounded_tree_bytes_with_entry_limit(
                            limit.saturating_sub(total),
                            entries,
                            entry_limit,
                        )?)
                        .ok_or(StoreError::TooLarge)?;
                }
                _ => {
                    return Err(StoreError::Unsafe);
                }
            }
            if total > limit {
                return Err(StoreError::TooLarge);
            }
        }
        Ok(total)
    }

    fn duplicate(&self) -> Result<Self, StoreError> {
        // SAFETY: F_DUPFD_CLOEXEC duplicates the valid descriptor.
        let raw = unsafe { libc::fcntl(self.descriptor.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if raw < 0 {
            return Err(StoreError::Io);
        }
        // SAFETY: successful fcntl returns a newly owned descriptor.
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
        validate_directory(&descriptor, true)?;
        Ok(Self { descriptor })
    }

    fn open_base(path: &Path) -> Result<Self, StoreError> {
        let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| StoreError::Unsafe)?;
        // SAFETY: path is a valid NUL-terminated byte string for this call.
        let raw = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if raw < 0 {
            return Err(map_open_error(io::Error::last_os_error()));
        }
        // SAFETY: successful open returns a newly owned descriptor.
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
        validate_directory(&descriptor, false)?;
        Ok(Self { descriptor })
    }

    fn ensure(&self, name: &str) -> Result<Self, StoreError> {
        let name = component(name)?;
        // SAFETY: parent descriptor and component are valid.
        let created =
            unsafe { libc::mkdirat(self.descriptor.as_raw_fd(), name.as_ptr(), 0o700) } == 0;
        if !created {
            match io::Error::last_os_error().raw_os_error() {
                Some(libc::EEXIST) => {}
                _ => return Err(StoreError::Io),
            }
        }
        // SAFETY: parent descriptor and component are valid.
        let raw = unsafe {
            libc::openat(
                self.descriptor.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if raw < 0 {
            return Err(map_open_error(io::Error::last_os_error()));
        }
        // SAFETY: successful openat returns a newly owned descriptor.
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
        if created {
            // SAFETY: descriptor is valid and owned.
            if unsafe { libc::fchmod(descriptor.as_raw_fd(), 0o700) } != 0 {
                return Err(StoreError::Io);
            }
        }
        validate_directory(&descriptor, true)?;
        self.sync()?;
        Ok(Self { descriptor })
    }

    fn open_directory(&self, name: &str) -> Result<Self, StoreError> {
        let name = component(name)?;
        // SAFETY: parent descriptor and component are valid; no symlink is followed.
        let raw = unsafe {
            libc::openat(
                self.descriptor.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if raw < 0 {
            return Err(map_open_error(io::Error::last_os_error()));
        }
        // SAFETY: successful openat returned a newly owned descriptor.
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
        validate_directory(&descriptor, true)?;
        Ok(Self { descriptor })
    }

    fn optional_directory(&self, name: &str) -> Result<Option<Self>, StoreError> {
        match self.open_directory(name) {
            Ok(directory) => Ok(Some(directory)),
            Err(StoreError::Io) => {
                let name = component(name)?;
                let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
                // SAFETY: descriptor/component/status are valid; no target is followed.
                let result = unsafe {
                    libc::fstatat(
                        self.descriptor.as_raw_fd(),
                        name.as_ptr(),
                        status.as_mut_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                if result == 0 {
                    Err(StoreError::Unsafe)
                } else if io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                    Ok(None)
                } else {
                    Err(StoreError::Io)
                }
            }
            Err(error) => Err(error),
        }
    }

    fn open_directory_relaxed(&self, name: &str) -> Result<Self, StoreError> {
        let name = component(name)?;
        // SAFETY: parent descriptor and component are valid; no symlink is followed.
        let raw = unsafe {
            libc::openat(
                self.descriptor.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if raw < 0 {
            return Err(map_open_error(io::Error::last_os_error()));
        }
        // SAFETY: successful openat returned a newly owned descriptor.
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
        validate_directory(&descriptor, false)?;
        Ok(Self { descriptor })
    }

    fn ensure_new(&self, name: &str) -> Result<Self, StoreError> {
        let name = component(name)?;
        // SAFETY: parent descriptor and component are valid.
        if unsafe { libc::mkdirat(self.descriptor.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return match io::Error::last_os_error().raw_os_error() {
                Some(libc::EEXIST) => Err(StoreError::Conflict),
                _ => Err(StoreError::Io),
            };
        }
        // SAFETY: parent descriptor and newly created component are valid.
        let raw = unsafe {
            libc::openat(
                self.descriptor.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if raw < 0 {
            return Err(map_open_error(io::Error::last_os_error()));
        }
        // SAFETY: successful openat returns a newly owned descriptor.
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: descriptor is valid and owned.
        if unsafe { libc::fchmod(descriptor.as_raw_fd(), 0o700) } != 0 {
            return Err(StoreError::Io);
        }
        validate_directory(&descriptor, true)?;
        self.sync()?;
        Ok(Self { descriptor })
    }

    fn write_new_file(&self, name: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let temp_name = temp_name("clone");
        let mut temp = TempEntry::create(self, &temp_name)?;
        temp.file.write_all(bytes).map_err(|_| StoreError::Io)?;
        temp.file.sync_all().map_err(|_| StoreError::Unsupported)?;
        validate_file_descriptor(&temp.file, 0o600, Some(bytes.len() as u64))?;
        rename_no_replace(self, &temp_name, name)?;
        temp.installed = true;
        self.sync()?;
        let installed = self.open_file(name, libc::O_RDONLY)?;
        validate_file(&installed, 0o600, Some(bytes.len() as u64))
    }

    fn cloexec_duplicate(&self) -> Result<OwnedFd, StoreError> {
        // SAFETY: F_DUPFD_CLOEXEC duplicates the valid directory descriptor.
        let raw = unsafe { libc::fcntl(self.descriptor.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if raw < 0 {
            return Err(StoreError::Io);
        }
        // SAFETY: successful F_DUPFD returns a newly owned descriptor.
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
        validate_directory(&descriptor, true)?;
        Ok(descriptor)
    }

    fn open_file(&self, name: &str, access: i32) -> Result<OwnedFd, StoreError> {
        let name = component(name)?;
        // SAFETY: directory descriptor and component are valid.
        let raw = unsafe {
            libc::openat(
                self.descriptor.as_raw_fd(),
                name.as_ptr(),
                access | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            )
        };
        if raw < 0 {
            return Err(map_open_error(io::Error::last_os_error()));
        }
        // SAFETY: successful openat returns a newly owned descriptor.
        Ok(unsafe { OwnedFd::from_raw_fd(raw) })
    }

    fn open_or_create_file(&self, name: &str) -> Result<OwnedFd, StoreError> {
        let name = component(name)?;
        // SAFETY: directory descriptor and component are valid.
        let mut raw = unsafe {
            libc::openat(
                self.descriptor.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDWR
                    | libc::O_CREAT
                    | libc::O_EXCL
                    | libc::O_CLOEXEC
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK,
                0o600,
            )
        };
        let created = raw >= 0;
        if raw < 0 {
            match io::Error::last_os_error().raw_os_error() {
                Some(libc::EEXIST) => {
                    // SAFETY: directory descriptor and component are valid.
                    raw = unsafe {
                        libc::openat(
                            self.descriptor.as_raw_fd(),
                            name.as_ptr(),
                            libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                        )
                    };
                    if raw < 0 {
                        return Err(map_open_error(io::Error::last_os_error()));
                    }
                }
                _ => return Err(map_open_error(io::Error::last_os_error())),
            }
        }
        // SAFETY: successful openat returns a newly owned descriptor.
        let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
        if created {
            // SAFETY: descriptor is valid and owned.
            if unsafe { libc::fchmod(descriptor.as_raw_fd(), 0o600) } != 0 {
                return Err(StoreError::Io);
            }
        }
        validate_file(&descriptor, 0o600, None)?;
        self.sync()?;
        Ok(descriptor)
    }

    fn missing(&self, name: &str) -> Result<bool, StoreError> {
        match self.open_file(name, libc::O_RDONLY) {
            Ok(descriptor) => {
                validate_file(&descriptor, 0o600, None)?;
                Ok(false)
            }
            Err(StoreError::Io) => {
                let name = component(name)?;
                let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
                // SAFETY: descriptor/component/status are valid; no target is followed.
                let result = unsafe {
                    libc::fstatat(
                        self.descriptor.as_raw_fd(),
                        name.as_ptr(),
                        status.as_mut_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                if result == 0 {
                    return Err(StoreError::Unsafe);
                }
                match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Ok(true),
                    _ => Err(StoreError::Io),
                }
            }
            Err(error) => Err(error),
        }
    }

    fn read_regular(&self, name: &str, limit: usize) -> Result<Vec<u8>, StoreError> {
        let descriptor = self.open_file(name, libc::O_RDONLY)?;
        validate_file(&descriptor, 0o600, None)?;
        let initial_status = status(descriptor.as_raw_fd())?;
        let mut file = File::from(descriptor);
        let metadata = file.metadata().map_err(|_| StoreError::Io)?;
        if metadata.len() > limit as u64 {
            return Err(StoreError::TooLarge);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(metadata.len() as usize)
            .map_err(|_| StoreError::TooLarge)?;
        Read::by_ref(&mut file)
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| StoreError::Io)?;
        if bytes.len() > limit {
            return Err(StoreError::TooLarge);
        }
        validate_file_descriptor(&file, 0o600, Some(bytes.len() as u64))?;
        let final_status = status(file.as_raw_fd())?;
        if !same_file_state(&initial_status, &final_status) {
            return Err(StoreError::Corrupt);
        }
        Ok(bytes)
    }

    fn sync(&self) -> Result<(), StoreError> {
        // SAFETY: directory descriptor is valid and fsync supports directories on Linux filesystems.
        if unsafe { libc::fsync(self.descriptor.as_raw_fd()) } == 0 {
            Ok(())
        } else {
            Err(StoreError::Unsupported)
        }
    }
}

struct TempEntry<'a> {
    directory: &'a Directory,
    name: String,
    file: File,
    installed: bool,
}

impl<'a> TempEntry<'a> {
    fn create(directory: &'a Directory, name: &str) -> Result<Self, StoreError> {
        let component = component(name)?;
        // SAFETY: directory descriptor and component are valid.
        let raw = unsafe {
            libc::openat(
                directory.descriptor.as_raw_fd(),
                component.as_ptr(),
                libc::O_WRONLY
                    | libc::O_CREAT
                    | libc::O_EXCL
                    | libc::O_CLOEXEC
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK,
                0o600,
            )
        };
        if raw < 0 {
            return Err(map_open_error(io::Error::last_os_error()));
        }
        // SAFETY: successful openat returns a newly owned descriptor.
        let file = unsafe { File::from_raw_fd(raw) };
        // SAFETY: file descriptor is valid and owned.
        if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
            return Err(StoreError::Io);
        }
        Ok(Self {
            directory,
            name: name.to_owned(),
            file,
            installed: false,
        })
    }
}

impl Drop for TempEntry<'_> {
    fn drop(&mut self) {
        if self.installed {
            return;
        }
        if let Ok(name) = component(&self.name) {
            // SAFETY: this guard owns the exclusive temp name in the validated directory.
            unsafe { libc::unlinkat(self.directory.descriptor.as_raw_fd(), name.as_ptr(), 0) };
        }
    }
}

fn validate_directory(descriptor: &OwnedFd, strict_mode: bool) -> Result<(), StoreError> {
    let status = status(descriptor.as_raw_fd())?;
    if status.st_uid != unsafe { libc::geteuid() }
        || status.st_mode & libc::S_IFMT != libc::S_IFDIR
        || (strict_mode && status.st_mode & 0o7777 != 0o700)
    {
        Err(StoreError::Unsafe)
    } else {
        Ok(())
    }
}

fn validate_file(descriptor: &OwnedFd, mode: u32, size: Option<u64>) -> Result<(), StoreError> {
    let status = status(descriptor.as_raw_fd())?;
    validate_status(&status, mode, size)
}

fn validate_file_descriptor(file: &File, mode: u32, size: Option<u64>) -> Result<(), StoreError> {
    let status = status(file.as_raw_fd())?;
    validate_status(&status, mode, size)
}

fn validate_status(status: &libc::stat, mode: u32, size: Option<u64>) -> Result<(), StoreError> {
    if status.st_uid != unsafe { libc::geteuid() }
        || status.st_mode & libc::S_IFMT != libc::S_IFREG
        || status.st_mode & 0o7777 != mode
        || status.st_nlink != 1
        || size.is_some_and(|expected| status.st_size < 0 || status.st_size as u64 != expected)
    {
        Err(StoreError::Unsafe)
    } else {
        Ok(())
    }
}

fn same_file_state(left: &libc::stat, right: &libc::stat) -> bool {
    left.st_dev == right.st_dev
        && left.st_ino == right.st_ino
        && left.st_size == right.st_size
        && left.st_mtime == right.st_mtime
        && left.st_mtime_nsec == right.st_mtime_nsec
        && left.st_ctime == right.st_ctime
        && left.st_ctime_nsec == right.st_ctime_nsec
}

fn hash_file_state(hasher: &mut blake3::Hasher, state: &libc::stat) {
    for value in [
        state.st_dev as i128,
        state.st_ino as i128,
        state.st_size as i128,
        state.st_mtime as i128,
        state.st_mtime_nsec as i128,
        state.st_ctime as i128,
        state.st_ctime_nsec as i128,
        state.st_mode as i128,
        state.st_nlink as i128,
    ] {
        hasher.update(&value.to_le_bytes());
    }
}

fn status(descriptor: i32) -> Result<libc::stat, StoreError> {
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: descriptor is valid and status points to writable storage.
    if unsafe { libc::fstat(descriptor, status.as_mut_ptr()) } != 0 {
        return Err(StoreError::Io);
    }
    // SAFETY: successful fstat initialized the complete structure.
    Ok(unsafe { status.assume_init() })
}

fn status_at(descriptor: i32, name: &CStr) -> Result<libc::stat, StoreError> {
    let mut result = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: descriptor, name, and writable output storage are valid.
    if unsafe {
        libc::fstatat(
            descriptor,
            name.as_ptr(),
            result.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(StoreError::Io);
    }
    // SAFETY: successful fstatat initialized the structure.
    Ok(unsafe { result.assume_init() })
}

fn identity(descriptor: &OwnedFd) -> Result<(u64, u64), StoreError> {
    let status = status(descriptor.as_raw_fd())?;
    Ok((status.st_dev, status.st_ino))
}

fn rename_no_replace(directory: &Directory, from: &str, to: &str) -> Result<(), StoreError> {
    let from = component(from)?;
    let to = component(to)?;
    // SAFETY: both names and the shared directory descriptor are valid.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            directory.descriptor.as_raw_fd(),
            from.as_ptr(),
            directory.descriptor.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        match io::Error::last_os_error().raw_os_error() {
            Some(libc::EEXIST) => Err(StoreError::Conflict),
            Some(libc::ENOSYS) | Some(libc::EINVAL) | Some(libc::EOPNOTSUPP) => {
                Err(StoreError::Unsupported)
            }
            _ => Err(StoreError::Io),
        }
    }
}

fn rename_no_replace_between(
    source: &Directory,
    from: &str,
    destination: &Directory,
    to: &str,
) -> Result<(), StoreError> {
    let from = component(from)?;
    let to = component(to)?;
    // SAFETY: both retained parent descriptors and validated names are valid.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            source.descriptor.as_raw_fd(),
            from.as_ptr(),
            destination.descriptor.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        match io::Error::last_os_error().raw_os_error() {
            Some(libc::EEXIST) => Err(StoreError::Conflict),
            Some(libc::ENOSYS) | Some(libc::EINVAL) | Some(libc::EOPNOTSUPP) => {
                Err(StoreError::Unsupported)
            }
            _ => Err(StoreError::Io),
        }
    }
}

fn rename_replace(directory: &Directory, from: &str, to: &str) -> Result<(), StoreError> {
    let from = component(from)?;
    let to = component(to)?;
    // SAFETY: both names and the shared directory descriptor are valid.
    if unsafe {
        libc::renameat(
            directory.descriptor.as_raw_fd(),
            from.as_ptr(),
            directory.descriptor.as_raw_fd(),
            to.as_ptr(),
        )
    } == 0
    {
        Ok(())
    } else {
        Err(StoreError::Io)
    }
}

fn component(value: &str) -> Result<CString, StoreError> {
    let bytes = OsStr::new(value).as_bytes();
    if bytes.is_empty()
        || bytes.len() > 255
        || bytes == b"."
        || bytes == b".."
        || bytes.contains(&b'/')
        || bytes.contains(&b'\\')
    {
        return Err(StoreError::Unsafe);
    }
    CString::new(bytes).map_err(|_| StoreError::Unsafe)
}

fn validate_clone_index_name(value: &str) -> Result<(), StoreError> {
    let Some(suffix) = value.strip_prefix("rehow-index-") else {
        return Err(StoreError::Unsafe);
    };
    if suffix.len() != 36
        || !suffix.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
    {
        return Err(StoreError::Unsafe);
    }
    component(value).map(|_| ())
}

fn temp_name(kind: &str) -> String {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(".{kind}-{}-{sequence}.tmp", std::process::id())
}

fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn snapshot_hash_frame(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn map_open_error(error: io::Error) -> StoreError {
    match error.raw_os_error() {
        Some(libc::ELOOP) | Some(libc::ENOTDIR) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
            StoreError::Unsafe
        }
        _ => StoreError::Io,
    }
}

fn map_codec(error: JournalCodecError) -> StoreError {
    match error {
        JournalCodecError::TooLarge => StoreError::TooLarge,
        JournalCodecError::ResourceLimit => StoreError::Io,
        JournalCodecError::Invalid => StoreError::Corrupt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::recovery::{
        ExpectedState, IntendedState, JournalStage, OperationKind, Target, TargetStep,
    };
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        path::PathBuf,
    };

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rehow-recovery-store-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn ids() -> (WorkspaceId, OperationId) {
        (
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".parse().unwrap(),
        )
    }

    fn store(state: &TestDirectory) -> RecoveryStore {
        RecoveryStore::open(&state.0, ids().0, [0x11; 32], [0x22; 32]).unwrap()
    }

    fn journal(generation: u64) -> Journal {
        Journal {
            generation,
            operation_id: ids().1,
            operation_kind: OperationKind::Create,
            workspace_id: ids().0,
            root_hash: [0x11; 32],
            root_device: 8,
            root_inode: 42,
            created_at: "2026-09-18T00:00:00Z".parse().unwrap(),
            plan_hash: [0x33; 32],
            targets: vec![Target {
                relative_path: "categories/linux/guide.md".to_owned(),
                expected: ExpectedState::Absent,
                intended: IntendedState::Object {
                    object_hash: [0x44; 32],
                    mode: 0o600,
                },
                temp_name: ".rehow-tmp-abcdef".to_owned(),
                step: TargetStep::Prepared,
            }],
            stage: JournalStage::Prepared,
            completion: false,
        }
    }

    #[test]
    fn commit_staging_is_private_complete_and_retained() {
        let state = TestDirectory::new("commit-staging");
        let name = "a".repeat(64);
        {
            let staging = CommitStaging::create(
                &state.0,
                &name,
                b"provenance",
                b"[core]\n\trepositoryformatversion = 0\n",
                Some(b"index"),
            )
            .unwrap();
            assert!(staging.stage_path().starts_with("/proc/self/fd/"));
            assert!(staging.git_path().starts_with("/proc/self/fd/"));
            assert!(staging.validation_path().starts_with("/proc/self/fd/"));
            assert!(staging.git_descriptor() >= 3);
            staging
                .write_validation_file("rehow.yaml", b"schema-version: 1\n")
                .unwrap();
            staging.finish_validation_tree().unwrap();
            staging.verify_size_limit(1024 * 1024).unwrap();
        }

        let stage = state.0.join("rehow/commit-staging").join(name);
        assert_eq!(
            fs::metadata(&stage).unwrap().permissions().mode() & 0o7777,
            0o700
        );
        for path in [
            "provenance",
            "repo.git/HEAD",
            "repo.git/config",
            "repo.git/index",
            "repo.git/objects/info",
            "repo.git/objects/pack",
            "repo.git/refs/rehow/commit",
            "repo.git/refs/heads",
            "repo.git/refs/tags",
            "validation/categories",
        ] {
            assert!(stage.join(path).exists(), "missing {path}");
        }
        assert_eq!(
            fs::metadata(stage.join("repo.git/index"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o600
        );
    }

    #[test]
    fn commit_staging_refuses_collisions_and_unrecognized_entries() {
        let state = TestDirectory::new("commit-staging-unsafe");
        let name = "b".repeat(64);
        CommitStaging::create(&state.0, &name, b"p", b"[core]\n", None).unwrap();
        assert!(matches!(
            CommitStaging::create(&state.0, &name, b"p", b"[core]\n", None),
            Err(StoreError::Conflict)
        ));
        fs::write(
            state.0.join("rehow/commit-quarantine/unrecognized"),
            b"hostile",
        )
        .unwrap();
        assert!(matches!(
            CommitStaging::create(&state.0, &"c".repeat(64), b"p", b"[core]\n", None),
            Err(StoreError::Unsafe)
        ));
    }

    #[test]
    fn commit_staging_enforces_operation_quota() {
        let state = TestDirectory::new("commit-staging-quota");
        for index in 0..8 {
            CommitStaging::create(&state.0, &format!("{index:064x}"), b"p", b"[core]\n", None)
                .unwrap();
        }
        assert!(matches!(
            CommitStaging::create(&state.0, &format!("{:064x}", 8), b"p", b"[core]\n", None,),
            Err(StoreError::TooLarge)
        ));
    }

    #[test]
    fn commit_staging_quarantine_moves_only_the_held_operation() {
        let state = TestDirectory::new("commit-staging-quarantine");
        let name = "d".repeat(64);
        let staging = CommitStaging::create(&state.0, &name, b"p", b"[core]\n", None).unwrap();
        staging.quarantine().unwrap();
        assert!(!state.0.join("rehow/commit-staging").join(&name).exists());
        assert!(state.0.join("rehow/commit-quarantine").join(&name).is_dir());

        let other = "e".repeat(64);
        let staging = CommitStaging::create(&state.0, &other, b"p", b"[core]\n", None).unwrap();
        fs::create_dir(state.0.join("rehow/commit-quarantine").join(&other)).unwrap();
        fs::set_permissions(
            state.0.join("rehow/commit-quarantine").join(&other),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(matches!(staging.quarantine(), Err(StoreError::Conflict)));
        assert!(state.0.join("rehow/commit-staging").join(other).is_dir());
    }

    #[test]
    fn commit_staging_quarantine_retry_repeats_every_parent_sync() {
        for (index, fault) in [
            FaultPoint::QuarantineAfterRename,
            FaultPoint::QuarantineAfterStagingSync,
            FaultPoint::QuarantineAfterQuarantineSync,
            FaultPoint::QuarantineAfterStateSync,
        ]
        .into_iter()
        .enumerate()
        {
            let state = TestDirectory::new("commit-staging-quarantine-fault");
            let name = format!("{index:064x}");
            let staging = CommitStaging::create(&state.0, &name, b"p", b"[core]\n", None).unwrap();
            let expected_identity = staging.stage_identity().unwrap();
            let store = store(&state);
            let lock = store.acquire_lock(None).unwrap();

            store.set_fault(fault);
            assert_eq!(
                store.quarantine_git_commit_staging(&lock, &name, expected_identity),
                Err(StoreError::Injected)
            );
            assert!(!state.0.join("rehow/commit-staging").join(&name).exists());
            assert!(state.0.join("rehow/commit-quarantine").join(&name).is_dir());

            store
                .quarantine_git_commit_staging(&lock, &name, expected_identity)
                .unwrap();
            store
                .quarantine_git_commit_staging(&lock, &name, expected_identity)
                .unwrap();
        }
    }

    #[test]
    fn commit_staging_snapshot_detects_late_content_changes() {
        let state = TestDirectory::new("commit-staging-snapshot");
        let name = "f".repeat(64);
        let staging = CommitStaging::create(&state.0, &name, b"p", b"[core]\n", None).unwrap();
        let snapshot = staging.snapshot(1024 * 1024).unwrap();
        assert!(staging.verify_snapshot(snapshot, 1024 * 1024).unwrap());
        staging
            .write_candidate_file("rehow.yaml", b"schema_version: 1\n")
            .unwrap();
        assert!(!staging.verify_snapshot(snapshot, 1024 * 1024).unwrap());
        let updated = staging.snapshot(1024 * 1024).unwrap();
        assert!(staging.verify_snapshot(updated, 1024 * 1024).unwrap());
    }

    #[test]
    fn secure_layout_objects_and_modes_are_verified() {
        let state = TestDirectory::new("layout");
        let store = store(&state);
        let lock = store.acquire_lock(None).unwrap();
        let hash = store.store_object(&lock, b"before image").unwrap();
        assert_eq!(store.read_object(&lock, &hash).unwrap(), b"before image");
        assert_eq!(store.store_object(&lock, b"before image").unwrap(), hash);
        let root = state.0.join("rehow");
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o7777,
            0o700
        );
        let object = root
            .join("workspaces")
            .join(format!("{}-{}", ids().0, hex(&[0x11; 32])))
            .join("objects")
            .join(hex(&hash));
        let metadata = fs::metadata(object).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o600);
        assert_eq!(metadata.nlink(), 1);
    }

    #[test]
    fn unsafe_state_entries_and_corrupt_objects_fail_closed() {
        let state = TestDirectory::new("unsafe");
        fs::create_dir(state.0.join("real")).unwrap();
        std::os::unix::fs::symlink(state.0.join("real"), state.0.join("rehow")).unwrap();
        assert!(matches!(
            RecoveryStore::open(&state.0, ids().0, [0x11; 32], [0x22; 32]),
            Err(StoreError::Unsafe)
        ));

        let state = TestDirectory::new("mode");
        fs::create_dir(state.0.join("rehow")).unwrap();
        fs::set_permissions(state.0.join("rehow"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            RecoveryStore::open(&state.0, ids().0, [0x11; 32], [0x22; 32]),
            Err(StoreError::Unsafe)
        ));

        let state = TestDirectory::new("corrupt");
        let store = store(&state);
        let lock = store.acquire_lock(None).unwrap();
        let hash = store.store_object(&lock, b"valid").unwrap();
        let name = hex(&hash);
        let path = state
            .0
            .join("rehow/workspaces")
            .join(format!("{}-{}", ids().0, hex(&[0x11; 32])))
            .join("objects")
            .join(name);
        fs::write(&path, b"corrupt").unwrap();
        assert_eq!(store.read_object(&lock, &hash), Err(StoreError::Corrupt));
        let hardlink = path.with_extension("hardlink");
        fs::hard_link(&path, &hardlink).unwrap();
        assert_eq!(store.read_object(&lock, &hash), Err(StoreError::Unsafe));
    }

    #[test]
    fn journal_generation_compare_and_swap_is_durable() {
        let state = TestDirectory::new("journal");
        let store = store(&state);
        let lock = store.acquire_lock(None).unwrap();
        let first = journal(1);
        store.install_journal(&lock, &first).unwrap();
        assert!(store.read_journal(&lock, ids().1).unwrap() == first);
        assert_eq!(
            store.install_journal(&lock, &first),
            Err(StoreError::Conflict)
        );
        let mut second = journal(2);
        second.stage = JournalStage::Applying;
        store.install_journal(&lock, &second).unwrap();
        assert!(store.read_journal(&lock, ids().1).unwrap() == second);
        let mut skipped_step = second.clone();
        skipped_step.generation = 3;
        skipped_step.targets[0].step = TargetStep::DirectorySynced;
        assert_eq!(
            store.install_journal(&lock, &skipped_step),
            Err(StoreError::Conflict)
        );
        let mut third = second.clone();
        third.generation = 3;
        third.targets[0].step = TargetStep::Swapped;
        store.install_journal(&lock, &third).unwrap();
        let mut skipped = third.clone();
        skipped.generation = 5;
        assert_eq!(
            store.install_journal(&lock, &skipped),
            Err(StoreError::Conflict)
        );
        let mut changed = third.clone();
        changed.generation = 4;
        changed.plan_hash = [0x99; 32];
        assert_eq!(
            store.install_journal(&lock, &changed),
            Err(StoreError::Conflict)
        );
        let mut regressed = third.clone();
        regressed.generation = 4;
        regressed.stage = JournalStage::Prepared;
        regressed.targets[0].step = TargetStep::Prepared;
        assert_eq!(
            store.install_journal(&lock, &regressed),
            Err(StoreError::Conflict)
        );
        let mut wrong_workspace = third.clone();
        wrong_workspace.generation = 4;
        wrong_workspace.workspace_id = "0199a19f-4d15-7ca7-a645-e75311caa711".parse().unwrap();
        assert_eq!(
            store.install_journal(&lock, &wrong_workspace),
            Err(StoreError::Conflict)
        );
    }

    #[test]
    fn lock_contention_and_runtime_preference_are_deterministic() {
        let state = TestDirectory::new("lock-state");
        let runtime = TestDirectory::new("lock-runtime");
        let store = store(&state);
        let first = store.acquire_lock(Some(&runtime.0)).unwrap();
        assert!(matches!(
            store.acquire_lock(Some(&runtime.0)),
            Err(StoreError::Busy)
        ));
        drop(first);
        assert!(store.acquire_lock(Some(&runtime.0)).is_ok());
        assert!(runtime.0.join("rehow").is_dir());
        assert!(!state.0.join("rehow/locks").exists());

        let fallback = store.acquire_lock(None).unwrap();
        drop(fallback);
        let lock_path = state
            .0
            .join("rehow/locks")
            .join(format!("{}.lock", hex(&[0x22; 32])));
        fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(store.acquire_lock(None), Err(StoreError::Unsafe)));
    }

    #[test]
    fn injected_store_failures_never_report_success_and_remain_recoverable() {
        for (index, point) in [
            FaultPoint::ObjectAfterWrite,
            FaultPoint::ObjectAfterFileSync,
            FaultPoint::ObjectAfterRename,
            FaultPoint::ObjectAfterDirectorySync,
        ]
        .into_iter()
        .enumerate()
        {
            let state = TestDirectory::new("object-fault");
            let store = store(&state);
            let lock = store.acquire_lock(None).unwrap();
            let bytes = format!("fault object {index}");
            store.set_fault(point);
            assert_eq!(
                store.store_object(&lock, bytes.as_bytes()),
                Err(StoreError::Injected)
            );
            let hash = *blake3::hash(bytes.as_bytes()).as_bytes();
            match point {
                FaultPoint::ObjectAfterRename | FaultPoint::ObjectAfterDirectorySync => {
                    assert_eq!(store.read_object(&lock, &hash).unwrap(), bytes.as_bytes());
                }
                _ => assert!(matches!(
                    store.read_object(&lock, &hash),
                    Err(StoreError::Io)
                )),
            }
        }

        for point in [
            FaultPoint::JournalAfterWrite,
            FaultPoint::JournalAfterFileSync,
            FaultPoint::JournalAfterRename,
            FaultPoint::JournalAfterDirectorySync,
        ] {
            let state = TestDirectory::new("journal-fault");
            let store = store(&state);
            let lock = store.acquire_lock(None).unwrap();
            store.set_fault(point);
            assert_eq!(
                store.install_journal(&lock, &journal(1)),
                Err(StoreError::Injected)
            );
            match point {
                FaultPoint::JournalAfterRename | FaultPoint::JournalAfterDirectorySync => {
                    assert!(store.read_journal(&lock, ids().1).is_ok());
                }
                _ => assert!(matches!(
                    store.read_journal(&lock, ids().1),
                    Err(StoreError::Io)
                )),
            }
        }
    }
}

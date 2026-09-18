use super::{ContentHash, Journal, JournalCodecError, MAX_JOURNAL_BYTES};
use crate::protocol::{OperationId, WorkspaceId};
use std::{
    ffi::{CString, OsStr},
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_OBJECT_BYTES: usize = 64 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
}

impl RecoveryStore {
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

impl Directory {
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

fn status(descriptor: i32) -> Result<libc::stat, StoreError> {
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: descriptor is valid and status points to writable storage.
    if unsafe { libc::fstat(descriptor, status.as_mut_ptr()) } != 0 {
        return Err(StoreError::Io);
    }
    // SAFETY: successful fstat initialized the complete structure.
    Ok(unsafe { status.assume_init() })
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

use serde::{Deserialize, Serialize};
use std::{
    env,
    ffi::{CString, OsString},
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{DirBuilderExt, MetadataExt, PermissionsExt},
        },
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const SCHEMA_VERSION: u32 = 1;
const DIRECTORY_NAME: &str = "rehow";
const FILE_NAME: &str = "preferences-v1.json";
const MAX_PREFERENCES_BYTES: u64 = 16 * 1024;
const MAX_PATH_BYTES: usize = 4 * 1024;
const ALLOWED_SYNC_INTERVALS: [u32; 5] = [15, 30, 60, 240, 1_440];
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DesktopPreferences {
    recent_workspace: Option<PathBuf>,
    sync_on_start: bool,
    sync_interval_minutes: Option<u32>,
}

impl DesktopPreferences {
    pub(crate) fn recent_workspace(&self) -> Option<&Path> {
        self.recent_workspace.as_deref()
    }

    pub(crate) fn remember_workspace(&mut self, path: PathBuf) -> Result<(), PreferencesError> {
        validate_workspace_path(&path)?;
        self.recent_workspace = Some(path);
        Ok(())
    }

    pub(crate) fn sync_on_start(&self) -> bool {
        self.sync_on_start
    }

    pub(crate) fn sync_interval_minutes(&self) -> Option<u32> {
        self.sync_interval_minutes
    }

    pub(crate) fn set_sync_policy(
        &mut self,
        sync_on_start: bool,
        sync_interval_minutes: Option<u32>,
    ) -> Result<(), PreferencesError> {
        validate_sync_interval(sync_interval_minutes)?;
        self.sync_on_start = sync_on_start;
        self.sync_interval_minutes = sync_interval_minutes;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreferencesError {
    LocationUnavailable,
    Unsafe,
    Invalid,
    Io,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPreferences {
    schema_version: u32,
    recent_workspace_hex: Option<String>,
    sync_on_start: bool,
    sync_interval_minutes: Option<u32>,
}

pub(crate) fn load() -> Result<DesktopPreferences, PreferencesError> {
    load_from_base(&config_base()?)
}

pub(crate) fn save(preferences: &DesktopPreferences) -> Result<(), PreferencesError> {
    save_to_base(&config_base()?, preferences)
}

fn config_base() -> Result<PathBuf, PreferencesError> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(".config"))
        })
        .ok_or(PreferencesError::LocationUnavailable)
}

fn load_from_base(base: &Path) -> Result<DesktopPreferences, PreferencesError> {
    let Some(base) = open_existing_directory(base)? else {
        return Ok(DesktopPreferences::default());
    };
    let Some(directory) = open_directory_at(&base, DIRECTORY_NAME)? else {
        return Ok(DesktopPreferences::default());
    };
    let Some(mut file) = open_preferences_file(&directory)? else {
        return Ok(DesktopPreferences::default());
    };
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_PREFERENCES_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PreferencesError::Io)?;
    if bytes.len() as u64 > MAX_PREFERENCES_BYTES {
        return Err(PreferencesError::Invalid);
    }
    decode_preferences(&bytes)
}

fn save_to_base(base: &Path, preferences: &DesktopPreferences) -> Result<(), PreferencesError> {
    let stored = StoredPreferences {
        schema_version: SCHEMA_VERSION,
        recent_workspace_hex: preferences
            .recent_workspace
            .as_deref()
            .map(encode_path)
            .transpose()?,
        sync_on_start: preferences.sync_on_start,
        sync_interval_minutes: preferences.sync_interval_minutes,
    };
    validate_sync_interval(stored.sync_interval_minutes)?;
    let bytes = serde_json::to_vec(&stored).map_err(|_| PreferencesError::Invalid)?;
    if bytes.len() as u64 > MAX_PREFERENCES_BYTES {
        return Err(PreferencesError::Invalid);
    }

    if !base.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(base)
            .map_err(|_| PreferencesError::Io)?;
    }
    let base = open_existing_directory(base)?.ok_or(PreferencesError::Io)?;
    let directory = match open_directory_at(&base, DIRECTORY_NAME)? {
        Some(directory) => directory,
        None => {
            mkdir_at(&base, DIRECTORY_NAME)?;
            open_directory_at(&base, DIRECTORY_NAME)?.ok_or(PreferencesError::Io)?
        }
    };
    if let Some(existing) = open_preferences_file(&directory)? {
        drop(existing);
    }

    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(".preferences-{}-{sequence}.tmp", std::process::id());
    let mut temporary = create_file_at(&directory, &temp_name)?;
    let result = (|| {
        temporary
            .write_all(&bytes)
            .and_then(|()| temporary.sync_all())
            .map_err(|_| PreferencesError::Io)?;
        rename_at(&directory, &temp_name, FILE_NAME)?;
        directory.sync_all().map_err(|_| PreferencesError::Io)
    })();
    if result.is_err() {
        let _ = unlink_at(&directory, &temp_name);
    }
    result
}

fn decode_preferences(bytes: &[u8]) -> Result<DesktopPreferences, PreferencesError> {
    let stored: StoredPreferences =
        serde_json::from_slice(bytes).map_err(|_| PreferencesError::Invalid)?;
    if stored.schema_version != SCHEMA_VERSION {
        return Err(PreferencesError::Invalid);
    }
    validate_sync_interval(stored.sync_interval_minutes)?;
    let recent_workspace = stored
        .recent_workspace_hex
        .as_deref()
        .map(decode_path)
        .transpose()?;
    Ok(DesktopPreferences {
        recent_workspace,
        sync_on_start: stored.sync_on_start,
        sync_interval_minutes: stored.sync_interval_minutes,
    })
}

fn validate_sync_interval(value: Option<u32>) -> Result<(), PreferencesError> {
    if value.is_some_and(|value| !ALLOWED_SYNC_INTERVALS.contains(&value)) {
        return Err(PreferencesError::Invalid);
    }
    Ok(())
}

fn validate_workspace_path(path: &Path) -> Result<(), PreferencesError> {
    let bytes = path.as_os_str().as_bytes();
    if !path.is_absolute() || bytes.is_empty() || bytes.len() > MAX_PATH_BYTES || bytes.contains(&0)
    {
        return Err(PreferencesError::Invalid);
    }
    Ok(())
}

fn encode_path(path: &Path) -> Result<String, PreferencesError> {
    validate_workspace_path(path)?;
    let bytes = path.as_os_str().as_bytes();
    let mut encoded = String::new();
    encoded
        .try_reserve_exact(bytes.len().saturating_mul(2))
        .map_err(|_| PreferencesError::Io)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(encoded)
}

fn decode_path(encoded: &str) -> Result<PathBuf, PreferencesError> {
    if encoded.len() > MAX_PATH_BYTES * 2 || !encoded.len().is_multiple_of(2) {
        return Err(PreferencesError::Invalid);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(encoded.len() / 2)
        .map_err(|_| PreferencesError::Io)?;
    for pair in encoded.as_bytes().as_chunks::<2>().0 {
        let high = hex_digit(pair[0]).ok_or(PreferencesError::Invalid)?;
        let low = hex_digit(pair[1]).ok_or(PreferencesError::Invalid)?;
        bytes.push((high << 4) | low);
    }
    let path = PathBuf::from(OsString::from_vec(bytes));
    validate_workspace_path(&path)?;
    Ok(path)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn open_existing_directory(path: &Path) -> Result<Option<File>, PreferencesError> {
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| PreferencesError::Invalid)?;
    // SAFETY: `path` is NUL-terminated and the returned descriptor is immediately owned by File.
    let raw = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOENT) => Ok(None),
            Some(libc::ELOOP | libc::ENOTDIR) => Err(PreferencesError::Unsafe),
            _ => Err(PreferencesError::Io),
        };
    }
    // SAFETY: successful `open` returned one owned descriptor.
    let file = unsafe { File::from_raw_fd(raw) };
    validate_directory(&file)?;
    Ok(Some(file))
}

fn open_directory_at(parent: &File, name: &str) -> Result<Option<File>, PreferencesError> {
    let name = CString::new(name).map_err(|_| PreferencesError::Invalid)?;
    // SAFETY: fixed component is NUL-terminated; returned descriptor is immediately owned.
    let raw = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOENT) => Ok(None),
            Some(libc::ELOOP | libc::ENOTDIR) => Err(PreferencesError::Unsafe),
            _ => Err(PreferencesError::Io),
        };
    }
    // SAFETY: successful `openat` returned one owned descriptor.
    let file = unsafe { File::from_raw_fd(raw) };
    validate_directory(&file)?;
    Ok(Some(file))
}

fn validate_directory(directory: &File) -> Result<(), PreferencesError> {
    let metadata = directory.metadata().map_err(|_| PreferencesError::Io)?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(PreferencesError::Unsafe);
    }
    Ok(())
}

fn mkdir_at(parent: &File, name: &str) -> Result<(), PreferencesError> {
    let name = CString::new(name).map_err(|_| PreferencesError::Invalid)?;
    // SAFETY: fixed component is NUL-terminated and confined to the retained parent descriptor.
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) };
    if result == 0 {
        parent.sync_all().map_err(|_| PreferencesError::Io)?;
        return Ok(());
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EEXIST) => Ok(()),
        _ => Err(PreferencesError::Io),
    }
}

fn open_preferences_file(directory: &File) -> Result<Option<File>, PreferencesError> {
    let name = CString::new(FILE_NAME).expect("fixed preference filename has no NUL");
    // SAFETY: fixed component is NUL-terminated; returned descriptor is immediately owned.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOENT) => Ok(None),
            Some(libc::ELOOP) => Err(PreferencesError::Unsafe),
            _ => Err(PreferencesError::Io),
        };
    }
    // SAFETY: successful `openat` returned one owned descriptor.
    let file = unsafe { File::from_raw_fd(raw) };
    let metadata = file.metadata().map_err(|_| PreferencesError::Io)?;
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o022 != 0
        || metadata.len() > MAX_PREFERENCES_BYTES
    {
        return Err(PreferencesError::Unsafe);
    }
    Ok(Some(file))
}

fn create_file_at(directory: &File, name: &str) -> Result<File, PreferencesError> {
    let name = CString::new(name).map_err(|_| PreferencesError::Invalid)?;
    // SAFETY: generated component is NUL-terminated; O_EXCL creates one private file.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if raw < 0 {
        return Err(PreferencesError::Io);
    }
    // SAFETY: successful `openat` returned one owned descriptor.
    Ok(unsafe { File::from_raw_fd(raw) })
}

fn rename_at(directory: &File, source: &str, destination: &str) -> Result<(), PreferencesError> {
    let source = CString::new(source).map_err(|_| PreferencesError::Invalid)?;
    let destination = CString::new(destination).map_err(|_| PreferencesError::Invalid)?;
    // SAFETY: both fixed/generated components are confined to the same retained directory.
    let result = unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            source.as_ptr(),
            directory.as_raw_fd(),
            destination.as_ptr(),
        )
    };
    (result == 0).then_some(()).ok_or(PreferencesError::Io)
}

fn unlink_at(directory: &File, name: &str) -> Result<(), PreferencesError> {
    let name = CString::new(name).map_err(|_| PreferencesError::Invalid)?;
    // SAFETY: generated temporary component is confined to the retained private directory.
    let result = unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
    (result == 0).then_some(()).ok_or(PreferencesError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::symlink, sync::atomic::AtomicU64};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let number = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "rehow-preferences-test-{}-{number}",
                std::process::id()
            ));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn preferences_round_trip_non_utf8_path_and_sync_policy() {
        let directory = TestDirectory::new();
        let mut preferences = DesktopPreferences::default();
        let path = PathBuf::from(OsString::from_vec(b"/tmp/rehow-\xff".to_vec()));
        preferences.remember_workspace(path.clone()).unwrap();
        preferences.set_sync_policy(true, Some(60)).unwrap();
        save_to_base(&directory.0, &preferences).unwrap();

        let loaded = load_from_base(&directory.0).unwrap();
        assert_eq!(loaded.recent_workspace(), Some(path.as_path()));
        assert!(loaded.sync_on_start());
        assert_eq!(loaded.sync_interval_minutes(), Some(60));
        let metadata = fs::symlink_metadata(directory.0.join(DIRECTORY_NAME)).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        let metadata =
            fs::symlink_metadata(directory.0.join(DIRECTORY_NAME).join(FILE_NAME)).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn preferences_reject_unknown_interval_schema_and_fields() {
        let directory = TestDirectory::new();
        let mut preferences = DesktopPreferences::default();
        assert_eq!(
            preferences.set_sync_policy(false, Some(1)),
            Err(PreferencesError::Invalid)
        );
        fs::create_dir(directory.0.join(DIRECTORY_NAME)).unwrap();
        fs::set_permissions(
            directory.0.join(DIRECTORY_NAME),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        fs::write(
            directory.0.join(DIRECTORY_NAME).join(FILE_NAME),
            br#"{"schema_version":2,"recent_workspace_hex":null,"sync_on_start":false,"sync_interval_minutes":null}"#,
        )
        .unwrap();
        assert_eq!(load_from_base(&directory.0), Err(PreferencesError::Invalid));
    }

    #[test]
    fn preferences_refuse_symlink_and_oversize_files() {
        let directory = TestDirectory::new();
        let preferences = directory.0.join(DIRECTORY_NAME);
        fs::create_dir(&preferences).unwrap();
        fs::set_permissions(&preferences, fs::Permissions::from_mode(0o700)).unwrap();
        let target = directory.0.join("foreign");
        fs::write(&target, b"foreign").unwrap();
        symlink(&target, preferences.join(FILE_NAME)).unwrap();
        assert_eq!(load_from_base(&directory.0), Err(PreferencesError::Unsafe));
        fs::remove_file(preferences.join(FILE_NAME)).unwrap();
        let file = File::create(preferences.join(FILE_NAME)).unwrap();
        file.set_len(MAX_PREFERENCES_BYTES + 1).unwrap();
        assert_eq!(load_from_base(&directory.0), Err(PreferencesError::Unsafe));
    }
}

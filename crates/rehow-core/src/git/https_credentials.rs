use super::credential_proxy::{SANDBOX_CREDENTIAL_HELPER, SANDBOX_CREDENTIAL_PROXY};
use super::{
    CLONE_DISCOVERY_OUTPUT_LIMIT, PROCESS_TIMEOUT, inspect_ssh_agent_parents,
    run_bounded_git_command_with_input, safe_provenance_path,
};
use std::{
    env,
    ffi::CString,
    fmt,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{
            ffi::OsStrExt, fs::MetadataExt, fs::OpenOptionsExt, fs::PermissionsExt,
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MAX_CREDENTIAL_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_HELPER_NAME_BYTES: usize = 128;
const SANDBOX_GH_CONFIG: &str = "/home/rehow/.config/gh/hosts.yml";

#[cfg(test)]
thread_local! {
    static CONFIG_RACE: std::cell::RefCell<Option<(PathBuf, Vec<u8>)>> = const { std::cell::RefCell::new(None) };
    static HELPER_RACE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHttpsCredentialStatus {
    NotConfigured,
    Trusted,
    Unsafe,
}

pub struct GitHttpsCredentialProvenance {
    status: GitHttpsCredentialStatus,
    helper: Option<CredentialHelperCapability>,
}

impl GitHttpsCredentialProvenance {
    pub fn status(&self) -> GitHttpsCredentialStatus {
        self.status
    }
}

impl fmt::Debug for GitHttpsCredentialProvenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitHttpsCredentialProvenance")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHttpsCredentialProxyStatus {
    NotConfigured,
    Trusted,
    Unsafe,
}

pub struct GitHttpsCredentialProxyProvenance {
    status: GitHttpsCredentialProxyStatus,
    executable: Option<File>,
}

impl GitHttpsCredentialProxyProvenance {
    pub fn status(&self) -> GitHttpsCredentialProxyStatus {
        self.status
    }
}

impl fmt::Debug for GitHttpsCredentialProxyProvenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitHttpsCredentialProxyProvenance")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

pub fn capture_git_https_credential_proxy_provenance() -> GitHttpsCredentialProxyProvenance {
    let candidate = env::current_exe().ok().and_then(|executable| {
        executable
            .parent()
            .map(|parent| parent.join("rehow-git-credential"))
    });
    capture_credential_proxy_from(candidate.as_deref())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHttpsCredentialBackingStatus {
    NotConfigured,
    Trusted,
    Unsafe,
}

pub struct GitHttpsCredentialBackingProvenance {
    status: GitHttpsCredentialBackingStatus,
    gh_config: Option<File>,
}

impl GitHttpsCredentialBackingProvenance {
    pub fn status(&self) -> GitHttpsCredentialBackingStatus {
        self.status
    }
}

impl fmt::Debug for GitHttpsCredentialBackingProvenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitHttpsCredentialBackingProvenance")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// Retains only the ordinary GitHub CLI credential file inode. Rehow never
/// parses, logs, copies, or returns its contents.
pub fn capture_git_https_credential_backing_provenance() -> GitHttpsCredentialBackingProvenance {
    let path = match gh_config_path() {
        Ok(Some(path)) => path,
        Ok(None) => return unavailable_backing(GitHttpsCredentialBackingStatus::NotConfigured),
        Err(()) => return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe),
    };
    capture_credential_backing_from(Some(&path))
}

fn gh_config_path() -> Result<Option<PathBuf>, ()> {
    let directory = if let Some(value) = env::var_os("GH_CONFIG_DIR") {
        PathBuf::from(value)
    } else if let Some(value) = env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(value).join("gh")
    } else if let Some(value) = env::var_os("HOME") {
        PathBuf::from(value).join(".config/gh")
    } else {
        return Ok(None);
    };
    if !safe_provenance_path(&directory) {
        return Err(());
    }
    Ok(Some(directory.join("hosts.yml")))
}

fn unavailable_backing(
    status: GitHttpsCredentialBackingStatus,
) -> GitHttpsCredentialBackingProvenance {
    GitHttpsCredentialBackingProvenance {
        status,
        gh_config: None,
    }
}

fn capture_credential_backing_from(
    candidate: Option<&Path>,
) -> GitHttpsCredentialBackingProvenance {
    let Some(candidate) = candidate else {
        return unavailable_backing(GitHttpsCredentialBackingStatus::NotConfigured);
    };
    if !safe_provenance_path(candidate) {
        return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe);
    }
    let effective_user = unsafe { libc::geteuid() };
    let before = match fs::symlink_metadata(candidate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return unavailable_backing(GitHttpsCredentialBackingStatus::NotConfigured);
        }
        Err(_) => return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe),
    };
    let Some(parent) = candidate.parent() else {
        return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe);
    };
    let Some(before_parents) = inspect_ssh_agent_parents(parent, effective_user) else {
        return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe);
    };
    if !credential_backing_metadata_safe(&before, effective_user) {
        return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe);
    }
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(candidate)
    {
        Ok(file) => file,
        Err(_) => return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe),
    };
    let held_before = match file.metadata() {
        Ok(metadata) if credential_backing_metadata_safe(&metadata, effective_user) => metadata,
        _ => return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe),
    };
    if backing_identity(&before) != backing_identity(&held_before) {
        return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe);
    }
    let Some(snapshot) = sealed_credential_backing_snapshot(&file, held_before.size()) else {
        return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe);
    };
    let held_after = match file.metadata() {
        Ok(metadata) if credential_backing_metadata_safe(&metadata, effective_user) => metadata,
        _ => return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe),
    };
    let after = match fs::symlink_metadata(candidate) {
        Ok(metadata) => metadata,
        Err(_) => return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe),
    };
    let Some(after_parents) = inspect_ssh_agent_parents(parent, effective_user) else {
        return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe);
    };
    if backing_identity(&before) != backing_identity(&held_after)
        || backing_identity(&after) != backing_identity(&held_after)
        || backing_identity(&held_before) != backing_identity(&held_after)
        || before_parents != after_parents
    {
        return unavailable_backing(GitHttpsCredentialBackingStatus::Unsafe);
    }
    GitHttpsCredentialBackingProvenance {
        status: GitHttpsCredentialBackingStatus::Trusted,
        gh_config: Some(snapshot),
    }
}

fn credential_backing_metadata_safe(metadata: &fs::Metadata, effective_user: u32) -> bool {
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == effective_user
        && metadata.nlink() == 1
        && metadata.permissions().mode() & 0o077 == 0
        && metadata.size() <= MAX_CREDENTIAL_CONFIG_BYTES
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct CredentialBackingIdentity {
    file: FileIdentity,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

fn backing_identity(metadata: &fs::Metadata) -> CredentialBackingIdentity {
    CredentialBackingIdentity {
        file: file_identity(metadata),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
    }
}

fn sealed_credential_backing_snapshot(source: &File, size: u64) -> Option<File> {
    if size > MAX_CREDENTIAL_CONFIG_BYTES {
        return None;
    }
    let descriptor = unsafe {
        libc::memfd_create(
            c"rehow-gh-config".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if descriptor < 0 {
        return None;
    }
    let snapshot = unsafe { File::from_raw_fd(descriptor) };
    let mut source_offset: libc::off_t = 0;
    let mut remaining = usize::try_from(size).ok()?;
    while remaining > 0 {
        let copied = unsafe {
            libc::sendfile(
                snapshot.as_raw_fd(),
                source.as_raw_fd(),
                &mut source_offset,
                remaining,
            )
        };
        if copied < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return None;
        }
        if copied == 0 {
            return None;
        }
        remaining = remaining.checked_sub(usize::try_from(copied).ok()?)?;
    }
    if unsafe { libc::fchmod(snapshot.as_raw_fd(), 0o400) } != 0 {
        return None;
    }
    let seals = libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
    if unsafe { libc::fcntl(snapshot.as_raw_fd(), libc::F_ADD_SEALS, seals) } != 0 {
        return None;
    }
    Some(snapshot)
}

fn capture_credential_proxy_from(candidate: Option<&Path>) -> GitHttpsCredentialProxyProvenance {
    let Some(candidate) = candidate else {
        return GitHttpsCredentialProxyProvenance {
            status: GitHttpsCredentialProxyStatus::NotConfigured,
            executable: None,
        };
    };
    let effective_user = unsafe { libc::geteuid() };
    let expected_owner = match fs::symlink_metadata(candidate) {
        Ok(metadata) if metadata.uid() == 0 || metadata.uid() == effective_user => metadata.uid(),
        Ok(_) => {
            return GitHttpsCredentialProxyProvenance {
                status: GitHttpsCredentialProxyStatus::Unsafe,
                executable: None,
            };
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return GitHttpsCredentialProxyProvenance {
                status: GitHttpsCredentialProxyStatus::NotConfigured,
                executable: None,
            };
        }
        Err(_) => {
            return GitHttpsCredentialProxyProvenance {
                status: GitHttpsCredentialProxyStatus::Unsafe,
                executable: None,
            };
        }
    };
    match capture_helper_executable_result(candidate, expected_owner, effective_user) {
        FileCapture::Missing => GitHttpsCredentialProxyProvenance {
            status: GitHttpsCredentialProxyStatus::NotConfigured,
            executable: None,
        },
        FileCapture::Trusted(executable) => GitHttpsCredentialProxyProvenance {
            status: GitHttpsCredentialProxyStatus::Trusted,
            executable: Some(executable),
        },
        FileCapture::Unsafe => GitHttpsCredentialProxyProvenance {
            status: GitHttpsCredentialProxyStatus::Unsafe,
            executable: None,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHttpsCredentialResourceError {
    CredentialsUnavailable,
    ProxyUnavailable,
    BackingUnavailable,
    Io,
}

/// Opaque launch-held authority for one fixed credential helper invocation.
/// It exposes no source path, helper identity, descriptor, or credential data.
pub struct GitHttpsCredentialResources {
    kind: CredentialHelperKind,
    helper: File,
    proxy: File,
    gh_config: Option<File>,
}

impl fmt::Debug for GitHttpsCredentialResources {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitHttpsCredentialResources")
            .field("authority", &"<redacted>")
            .finish()
    }
}

impl GitHttpsCredentialResources {
    pub fn prepare(
        credentials: &GitHttpsCredentialProvenance,
        proxy: &GitHttpsCredentialProxyProvenance,
        backing: &GitHttpsCredentialBackingProvenance,
    ) -> Result<Self, GitHttpsCredentialResourceError> {
        if credentials.status != GitHttpsCredentialStatus::Trusted {
            return Err(GitHttpsCredentialResourceError::CredentialsUnavailable);
        }
        if proxy.status != GitHttpsCredentialProxyStatus::Trusted {
            return Err(GitHttpsCredentialResourceError::ProxyUnavailable);
        }
        let helper = credentials
            .helper
            .as_ref()
            .ok_or(GitHttpsCredentialResourceError::CredentialsUnavailable)?;
        let gh_config = if helper.kind == CredentialHelperKind::GithubCli {
            Some(
                backing
                    .gh_config
                    .as_ref()
                    .filter(|_| backing.status == GitHttpsCredentialBackingStatus::Trusted)
                    .ok_or(GitHttpsCredentialResourceError::BackingUnavailable)?
                    .try_clone()
                    .map_err(|_| GitHttpsCredentialResourceError::Io)?,
            )
        } else {
            None
        };
        Ok(Self {
            kind: helper.kind,
            helper: helper
                .executable
                .try_clone()
                .map_err(|_| GitHttpsCredentialResourceError::Io)?,
            proxy: proxy
                .executable
                .as_ref()
                .ok_or(GitHttpsCredentialResourceError::ProxyUnavailable)?
                .try_clone()
                .map_err(|_| GitHttpsCredentialResourceError::Io)?,
            gh_config,
        })
    }

    pub(crate) fn helper_kind_env(&self) -> &'static str {
        match self.kind {
            CredentialHelperKind::Direct => "direct",
            CredentialHelperKind::GithubCli => "githubCli",
        }
    }

    pub(crate) fn configure_bubblewrap(&self, command: &mut Command, expected_host: &str) {
        command
            .env("REHOW_CREDENTIAL_HOST", expected_host)
            .env("REHOW_CREDENTIAL_HELPER_KIND", self.helper_kind_env());
        self.add_bubblewrap_mounts(command);
    }

    pub(crate) fn add_bubblewrap_mounts(&self, command: &mut Command) {
        let proxy_fd = self.proxy.as_raw_fd();
        let helper_fd = self.helper.as_raw_fd();
        command
            .args(["--dir", "/home/rehow/bin", "--ro-bind-fd"])
            .arg(proxy_fd.to_string())
            .arg(SANDBOX_CREDENTIAL_PROXY)
            .args(["--ro-bind-fd"])
            .arg(helper_fd.to_string())
            .arg(SANDBOX_CREDENTIAL_HELPER);
        let gh_config_fd = self.gh_config.as_ref().map(AsRawFd::as_raw_fd);
        if let Some(gh_config_fd) = gh_config_fd {
            command
                .args([
                    "--dir",
                    "/home/rehow/.config",
                    "--dir",
                    "/home/rehow/.config/gh",
                    "--perms",
                    "0400",
                ])
                .args(["--ro-bind-data"])
                .arg(gh_config_fd.to_string())
                .arg(SANDBOX_GH_CONFIG);
        }
        unsafe {
            command.pre_exec(move || {
                for descriptor in [proxy_fd, helper_fd] {
                    clear_close_on_exec(descriptor)?;
                }
                if let Some(descriptor) = gh_config_fd {
                    if libc::lseek(descriptor, 0, libc::SEEK_SET) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    clear_close_on_exec(descriptor)?;
                }
                Ok(())
            });
        }
    }
}

fn clear_close_on_exec(descriptor: RawFd) -> std::io::Result<()> {
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
    if flags < 0
        || unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } != 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

struct CredentialHelperCapability {
    kind: CredentialHelperKind,
    executable: File,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CredentialHelperKind {
    Direct,
    GithubCli,
}

#[derive(Clone, Copy)]
enum ConfigOwner {
    EffectiveUser,
    Root,
}

struct ConfigSource {
    path: PathBuf,
    owner: ConfigOwner,
}

enum FileCapture<T> {
    Missing,
    Trusted(T),
    Unsafe,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    owner: u32,
    mode: u32,
    links: u64,
    size: u64,
}

/// Captures only trusted global helper selection and the selected executable's
/// inode. It does not invoke a helper or read credential protocol data.
pub fn capture_git_https_credential_provenance() -> GitHttpsCredentialProvenance {
    let effective_user = unsafe { libc::geteuid() };
    let git = Path::new("/usr/bin/git");
    let _git_capability = match capture_helper_executable_result(git, 0, effective_user) {
        FileCapture::Trusted(executable) => executable,
        FileCapture::Missing | FileCapture::Unsafe => {
            return unavailable(GitHttpsCredentialStatus::Unsafe);
        }
    };
    let home = env::var_os("HOME");
    let xdg = env::var_os("XDG_CONFIG_HOME");
    let mut sources = vec![ConfigSource {
        path: PathBuf::from("/etc/gitconfig"),
        owner: ConfigOwner::Root,
    }];

    if let Some(xdg) = xdg.as_deref() {
        let xdg = Path::new(xdg);
        if !safe_provenance_path(xdg) {
            return unavailable(GitHttpsCredentialStatus::Unsafe);
        }
        sources.push(ConfigSource {
            path: xdg.join("git/config"),
            owner: ConfigOwner::EffectiveUser,
        });
    } else if let Some(home) = home.as_deref() {
        let home = Path::new(home);
        if !safe_provenance_path(home) {
            return unavailable(GitHttpsCredentialStatus::Unsafe);
        }
        sources.push(ConfigSource {
            path: home.join(".config/git/config"),
            owner: ConfigOwner::EffectiveUser,
        });
    }
    if let Some(home) = home.as_deref() {
        let home = Path::new(home);
        if !safe_provenance_path(home) {
            return unavailable(GitHttpsCredentialStatus::Unsafe);
        }
        sources.push(ConfigSource {
            path: home.join(".gitconfig"),
            owner: ConfigOwner::EffectiveUser,
        });
    }

    let helper_directories = [Path::new("/usr/lib/git-core"), Path::new("/usr/bin")];
    capture_from_sources(
        git,
        &sources,
        &helper_directories,
        Path::new("/usr/bin/gh"),
        effective_user,
        0,
    )
}

fn unavailable(status: GitHttpsCredentialStatus) -> GitHttpsCredentialProvenance {
    GitHttpsCredentialProvenance {
        status,
        helper: None,
    }
}

fn capture_from_sources(
    git: &Path,
    sources: &[ConfigSource],
    helper_directories: &[&Path],
    gh_path: &Path,
    effective_user: u32,
    helper_owner: u32,
) -> GitHttpsCredentialProvenance {
    let mut effective_helpers = Vec::new();
    let mut aggregate_bytes = 0_u64;
    for source in sources {
        let bytes = match capture_config_file(source, effective_user) {
            FileCapture::Missing => continue,
            FileCapture::Trusted(bytes) => bytes,
            FileCapture::Unsafe => return unavailable(GitHttpsCredentialStatus::Unsafe),
        };
        aggregate_bytes = match aggregate_bytes.checked_add(bytes.len() as u64) {
            Some(total) if total <= MAX_CREDENTIAL_CONFIG_BYTES => total,
            _ => return unavailable(GitHttpsCredentialStatus::Unsafe),
        };
        let entries = match parse_config_snapshot(git, bytes) {
            Some(entries) => entries,
            None => return unavailable(GitHttpsCredentialStatus::Unsafe),
        };
        for (key, value) in entries {
            if key.eq_ignore_ascii_case("credential.helper") {
                if value.is_empty() {
                    effective_helpers.clear();
                } else {
                    effective_helpers.push(value);
                }
            } else {
                let lower = key.to_ascii_lowercase();
                if lower.len() >= "credential..helper".len()
                    && lower.starts_with("credential.")
                    && lower.ends_with(".helper")
                {
                    return unavailable(GitHttpsCredentialStatus::Unsafe);
                }
            }
        }
    }

    let [helper] = effective_helpers.as_slice() else {
        return unavailable(if effective_helpers.is_empty() {
            GitHttpsCredentialStatus::NotConfigured
        } else {
            GitHttpsCredentialStatus::Unsafe
        });
    };
    match resolve_helper(
        helper,
        helper_directories,
        gh_path,
        helper_owner,
        effective_user,
    ) {
        Some(helper) => GitHttpsCredentialProvenance {
            status: GitHttpsCredentialStatus::Trusted,
            helper: Some(helper),
        },
        None => unavailable(GitHttpsCredentialStatus::Unsafe),
    }
}

fn capture_config_file(source: &ConfigSource, effective_user: u32) -> FileCapture<Vec<u8>> {
    if !safe_provenance_path(&source.path) {
        return FileCapture::Unsafe;
    }
    let before = match fs::symlink_metadata(&source.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return FileCapture::Missing;
        }
        Err(_) => return FileCapture::Unsafe,
    };
    let Some(parent) = source.path.parent() else {
        return FileCapture::Unsafe;
    };
    let Some(before_parents) = inspect_ssh_agent_parents(parent, effective_user) else {
        return FileCapture::Unsafe;
    };
    if !config_metadata_safe(&before, source.owner, effective_user) {
        return FileCapture::Unsafe;
    }
    let mut file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&source.path)
    {
        Ok(file) => file,
        Err(_) => return FileCapture::Unsafe,
    };
    let held_before = match file.metadata() {
        Ok(metadata) if config_metadata_safe(&metadata, source.owner, effective_user) => metadata,
        _ => return FileCapture::Unsafe,
    };
    if file_identity(&before) != file_identity(&held_before) {
        return FileCapture::Unsafe;
    }
    let Some(first) = read_bounded(&mut file) else {
        return FileCapture::Unsafe;
    };
    #[cfg(test)]
    CONFIG_RACE.with(|race| {
        if let Some((path, bytes)) = race.borrow_mut().take() {
            let _ = fs::remove_file(&path);
            let _ = fs::write(&path, bytes);
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    });
    if file.seek(SeekFrom::Start(0)).is_err() {
        return FileCapture::Unsafe;
    }
    let Some(second) = read_bounded(&mut file) else {
        return FileCapture::Unsafe;
    };
    let held_after = match file.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return FileCapture::Unsafe,
    };
    let after = match fs::symlink_metadata(&source.path) {
        Ok(metadata) => metadata,
        Err(_) => return FileCapture::Unsafe,
    };
    let Some(after_parents) = inspect_ssh_agent_parents(parent, effective_user) else {
        return FileCapture::Unsafe;
    };
    if first != second
        || file_identity(&before) != file_identity(&held_after)
        || file_identity(&after) != file_identity(&held_after)
        || before_parents != after_parents
    {
        return FileCapture::Unsafe;
    }
    FileCapture::Trusted(first)
}

fn config_metadata_safe(metadata: &fs::Metadata, owner: ConfigOwner, effective_user: u32) -> bool {
    let expected_owner = match owner {
        ConfigOwner::EffectiveUser => effective_user,
        ConfigOwner::Root => 0,
    };
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == expected_owner
        && metadata.nlink() == 1
        && metadata.permissions().mode() & 0o022 == 0
        && metadata.size() <= MAX_CREDENTIAL_CONFIG_BYTES
}

fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        owner: metadata.uid(),
        mode: metadata.mode(),
        links: metadata.nlink(),
        size: metadata.size(),
    }
}

fn read_bounded(file: &mut File) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(MAX_CREDENTIAL_CONFIG_BYTES.checked_add(1)?)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_CREDENTIAL_CONFIG_BYTES {
        None
    } else {
        Some(bytes)
    }
}

fn parse_config_snapshot(git: &Path, bytes: Vec<u8>) -> Option<Vec<(String, String)>> {
    let mut command = Command::new(git);
    command
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["config", "--file", "-", "--no-includes", "--null", "--list"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = run_bounded_git_command_with_input(
        command,
        PROCESS_TIMEOUT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        CLONE_DISCOVERY_OUTPUT_LIMIT,
        Some(bytes),
    )
    .ok()?;
    let mut entries = Vec::new();
    for record in output.stdout.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let split = record.iter().position(|byte| *byte == b'\n')?;
        let key = std::str::from_utf8(&record[..split]).ok()?.to_owned();
        let value = std::str::from_utf8(&record[split + 1..]).ok()?.to_owned();
        entries.push((key, value));
    }
    Some(entries)
}

fn resolve_helper(
    value: &str,
    helper_directories: &[&Path],
    gh_path: &Path,
    helper_owner: u32,
    effective_user: u32,
) -> Option<CredentialHelperCapability> {
    let gh_value = format!("!{} auth git-credential", gh_path.to_str()?);
    if value == gh_value {
        return Some(CredentialHelperCapability {
            kind: CredentialHelperKind::GithubCli,
            executable: capture_helper_executable(gh_path, helper_owner, effective_user)?,
        });
    }
    if value.is_empty()
        || value.len() > MAX_HELPER_NAME_BYTES
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        || value.starts_with('!')
    {
        return None;
    }
    let configured = Path::new(value);
    if configured.is_absolute() {
        let parent = configured.parent()?;
        if !helper_directories.contains(&parent)
            || !configured
                .file_name()?
                .as_bytes()
                .starts_with(b"git-credential-")
        {
            return None;
        }
        return Some(CredentialHelperCapability {
            kind: CredentialHelperKind::Direct,
            executable: capture_helper_executable(configured, helper_owner, effective_user)?,
        });
    }
    if value.contains('/')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return None;
    }
    for directory in helper_directories {
        let candidate = directory.join(format!("git-credential-{value}"));
        match capture_helper_executable_result(&candidate, helper_owner, effective_user) {
            FileCapture::Missing => {}
            FileCapture::Trusted(executable) => {
                return Some(CredentialHelperCapability {
                    kind: CredentialHelperKind::Direct,
                    executable,
                });
            }
            FileCapture::Unsafe => return None,
        }
    }
    None
}

fn capture_helper_executable(
    path: &Path,
    expected_owner: u32,
    effective_user: u32,
) -> Option<File> {
    match capture_helper_executable_result(path, expected_owner, effective_user) {
        FileCapture::Trusted(file) => Some(file),
        FileCapture::Missing | FileCapture::Unsafe => None,
    }
}

fn capture_helper_executable_result(
    path: &Path,
    expected_owner: u32,
    effective_user: u32,
) -> FileCapture<File> {
    if !safe_provenance_path(path) {
        return FileCapture::Unsafe;
    }
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return FileCapture::Missing;
        }
        Err(_) => return FileCapture::Unsafe,
    };
    let Some(parent) = path.parent() else {
        return FileCapture::Unsafe;
    };
    let Some(before_parents) = inspect_ssh_agent_parents(parent, effective_user) else {
        return FileCapture::Unsafe;
    };
    if !helper_metadata_safe(&before, expected_owner) {
        return FileCapture::Unsafe;
    }
    #[cfg(test)]
    HELPER_RACE.with(|race| {
        if let Some(path) = race.borrow_mut().take() {
            let _ = fs::remove_file(&path);
            let _ = fs::write(&path, b"replacement");
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
        }
    });
    let path_bytes = match CString::new(path.as_os_str().as_bytes()) {
        Ok(path) => path,
        Err(_) => return FileCapture::Unsafe,
    };
    let descriptor = unsafe {
        libc::open(
            path_bytes.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return FileCapture::Unsafe;
    }
    let executable = unsafe { File::from_raw_fd(descriptor) };
    let held = match executable.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return FileCapture::Unsafe,
    };
    let after = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return FileCapture::Unsafe,
    };
    let Some(after_parents) = inspect_ssh_agent_parents(parent, effective_user) else {
        return FileCapture::Unsafe;
    };
    if !helper_metadata_safe(&held, expected_owner)
        || file_identity(&before) != file_identity(&held)
        || file_identity(&after) != file_identity(&held)
        || before_parents != after_parents
    {
        return FileCapture::Unsafe;
    }
    FileCapture::Trusted(executable)
}

fn helper_metadata_safe(metadata: &fs::Metadata, expected_owner: u32) -> bool {
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == expected_owner
        && metadata.nlink() == 1
        && metadata.permissions().mode() & 0o022 == 0
        && metadata.permissions().mode() & 0o111 != 0
}

#[cfg(test)]
pub(super) fn test_direct_resources() -> GitHttpsCredentialResources {
    let proxy = capture_credential_proxy_from(Some(Path::new("/usr/bin/true")));
    let effective_user = unsafe { libc::geteuid() };
    let helper =
        match capture_helper_executable_result(Path::new("/usr/bin/true"), 0, effective_user) {
            FileCapture::Trusted(helper) => helper,
            FileCapture::Missing | FileCapture::Unsafe => {
                panic!("system test helper unavailable")
            }
        };
    let credentials = GitHttpsCredentialProvenance {
        status: GitHttpsCredentialStatus::Trusted,
        helper: Some(CredentialHelperCapability {
            kind: CredentialHelperKind::Direct,
            executable: helper,
        }),
    };
    let backing = unavailable_backing(GitHttpsCredentialBackingStatus::NotConfigured);
    GitHttpsCredentialResources::prepare(&credentials, &proxy, &backing).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, process::Stdio};

    struct Temp(PathBuf);

    impl Temp {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "rehow-https-credential-{}-{}",
                std::process::id(),
                crate::protocol::new_operation_id()
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn executable(path: &Path) {
        fs::write(path, b"fixture").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn capture(root: &Temp, config: &[u8]) -> GitHttpsCredentialProvenance {
        let config_path = root.0.join("config");
        fs::write(&config_path, config).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let source = ConfigSource {
            path: config_path,
            owner: ConfigOwner::EffectiveUser,
        };
        let helper_dir = root.0.join("helpers");
        fs::create_dir(&helper_dir).unwrap();
        fs::set_permissions(&helper_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let helper = helper_dir.join("git-credential-safe");
        executable(&helper);
        let gh = helper_dir.join("gh");
        executable(&gh);
        let uid = unsafe { libc::geteuid() };
        capture_from_sources(
            Path::new("/usr/bin/git"),
            &[source],
            &[helper_dir.as_path()],
            &gh,
            uid,
            uid,
        )
    }

    #[test]
    fn captures_named_absolute_and_exact_gh_helpers_without_disclosure() {
        let named = Temp::new();
        let provenance = capture(&named, b"[credential]\nhelper = safe\n");
        assert_eq!(provenance.status(), GitHttpsCredentialStatus::Trusted);
        assert_eq!(
            format!("{provenance:?}"),
            "GitHttpsCredentialProvenance { status: Trusted, .. }"
        );
        assert!(!format!("{provenance:?}").contains(named.0.to_str().unwrap()));

        let absolute = Temp::new();
        let helper = absolute.0.join("helpers/git-credential-safe");
        let config = format!("[credential]\nhelper = {}\n", helper.display());
        assert_eq!(
            capture(&absolute, config.as_bytes()).status(),
            GitHttpsCredentialStatus::Trusted
        );

        let gh = Temp::new();
        let gh_path = gh.0.join("helpers/gh");
        let config = format!(
            "[credential]\nhelper = !{} auth git-credential\n",
            gh_path.display()
        );
        assert_eq!(
            capture(&gh, config.as_bytes()).status(),
            GitHttpsCredentialStatus::Trusted
        );
    }

    #[test]
    fn applies_reset_semantics_and_rejects_ambiguous_or_executable_config() {
        let reset = Temp::new();
        assert_eq!(
            capture(
                &reset,
                b"[credential]\nhelper = first\nhelper =\nhelper = safe\n"
            )
            .status(),
            GitHttpsCredentialStatus::Trusted
        );
        for config in [
            b"[credential]\nhelper = safe\nhelper = safe\n".as_slice(),
            b"[credential \"https://example.invalid\"]\nhelper = safe\n".as_slice(),
            b"[credential \"\"]\nhelper = safe\n".as_slice(),
            b"[credential]\nhelper = !echo stolen\n".as_slice(),
            b"[credential]\nhelper = safe --argument\n".as_slice(),
            b"[credential]\nhelper = relative/path\n".as_slice(),
            b"[credential]\nhelper = missing\n".as_slice(),
            b"[credential\nhelper = safe\n".as_slice(),
        ] {
            let root = Temp::new();
            assert_eq!(
                capture(&root, config).status(),
                GitHttpsCredentialStatus::Unsafe
            );
        }
        let empty = Temp::new();
        assert_eq!(
            capture(&empty, b"[user]\nname = Example\n").status(),
            GitHttpsCredentialStatus::NotConfigured
        );

        let included = Temp::new();
        let hostile_include = included.0.join("hostile");
        fs::write(
            &hostile_include,
            b"[credential]\nhelper = !echo should-not-run\n",
        )
        .unwrap();
        let config = format!(
            "[include]\npath = {}\n[credential]\nhelper = safe\n",
            hostile_include.display()
        );
        assert_eq!(
            capture(&included, config.as_bytes()).status(),
            GitHttpsCredentialStatus::Trusted
        );
    }

    #[test]
    fn refuses_unsafe_config_and_helper_metadata() {
        let root = Temp::new();
        let config_path = root.0.join("config");
        fs::write(&config_path, b"[credential]\nhelper = safe\n").unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o622)).unwrap();
        let helper_dir = root.0.join("helpers");
        fs::create_dir(&helper_dir).unwrap();
        fs::set_permissions(&helper_dir, fs::Permissions::from_mode(0o700)).unwrap();
        executable(&helper_dir.join("git-credential-safe"));
        let uid = unsafe { libc::geteuid() };
        let source = ConfigSource {
            path: config_path.clone(),
            owner: ConfigOwner::EffectiveUser,
        };
        assert_eq!(
            capture_from_sources(
                Path::new("/usr/bin/git"),
                &[source],
                &[helper_dir.as_path()],
                &helper_dir.join("gh"),
                uid,
                uid,
            )
            .status(),
            GitHttpsCredentialStatus::Unsafe
        );

        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        if uid != 0 {
            assert_eq!(
                capture_from_sources(
                    Path::new("/usr/bin/git"),
                    &[ConfigSource {
                        path: config_path.clone(),
                        owner: ConfigOwner::Root,
                    }],
                    &[helper_dir.as_path()],
                    &helper_dir.join("gh"),
                    uid,
                    uid,
                )
                .status(),
                GitHttpsCredentialStatus::Unsafe
            );
            assert_eq!(
                capture_from_sources(
                    Path::new("/usr/bin/git"),
                    &[ConfigSource {
                        path: config_path.clone(),
                        owner: ConfigOwner::EffectiveUser,
                    }],
                    &[helper_dir.as_path()],
                    &helper_dir.join("gh"),
                    uid,
                    0,
                )
                .status(),
                GitHttpsCredentialStatus::Unsafe
            );
        }
        fs::set_permissions(
            helper_dir.join("git-credential-safe"),
            fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        let source = ConfigSource {
            path: config_path.clone(),
            owner: ConfigOwner::EffectiveUser,
        };
        assert_eq!(
            capture_from_sources(
                Path::new("/usr/bin/git"),
                &[source],
                &[helper_dir.as_path()],
                &helper_dir.join("gh"),
                uid,
                uid,
            )
            .status(),
            GitHttpsCredentialStatus::Unsafe
        );

        fs::set_permissions(
            helper_dir.join("git-credential-safe"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        fs::hard_link(
            helper_dir.join("git-credential-safe"),
            helper_dir.join("helper-hardlink"),
        )
        .unwrap();
        let source = ConfigSource {
            path: config_path,
            owner: ConfigOwner::EffectiveUser,
        };
        assert_eq!(
            capture_from_sources(
                Path::new("/usr/bin/git"),
                &[source],
                &[helper_dir.as_path()],
                &helper_dir.join("gh"),
                uid,
                uid,
            )
            .status(),
            GitHttpsCredentialStatus::Unsafe
        );
    }

    #[test]
    fn refuses_linked_oversized_and_output_amplifying_config() {
        use std::os::unix::fs::symlink;

        let linked = Temp::new();
        let real = linked.0.join("real-config");
        fs::write(&real, b"[credential]\nhelper = safe\n").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let link = linked.0.join("config-link");
        symlink(&real, &link).unwrap();
        let uid = unsafe { libc::geteuid() };
        assert_eq!(
            capture_from_sources(
                Path::new("/usr/bin/git"),
                &[ConfigSource {
                    path: link,
                    owner: ConfigOwner::EffectiveUser,
                }],
                &[],
                Path::new("/missing/gh"),
                uid,
                uid,
            )
            .status(),
            GitHttpsCredentialStatus::Unsafe
        );

        let hardlinked = Temp::new();
        let original = hardlinked.0.join("original-config");
        let alias = hardlinked.0.join("config");
        fs::write(&original, b"[credential]\nhelper = safe\n").unwrap();
        fs::set_permissions(&original, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&original, &alias).unwrap();
        assert_eq!(
            capture_from_sources(
                Path::new("/usr/bin/git"),
                &[ConfigSource {
                    path: alias,
                    owner: ConfigOwner::EffectiveUser,
                }],
                &[],
                Path::new("/missing/gh"),
                uid,
                uid,
            )
            .status(),
            GitHttpsCredentialStatus::Unsafe
        );

        let oversized = Temp::new();
        let path = oversized.0.join("config");
        fs::write(&path, vec![b'x'; MAX_CREDENTIAL_CONFIG_BYTES as usize + 1]).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            capture_from_sources(
                Path::new("/usr/bin/git"),
                &[ConfigSource {
                    path,
                    owner: ConfigOwner::EffectiveUser,
                }],
                &[],
                Path::new("/missing/gh"),
                uid,
                uid,
            )
            .status(),
            GitHttpsCredentialStatus::Unsafe
        );

        let amplified = Temp::new();
        let mut config = Vec::from("[safe]\n".as_bytes());
        for index in 0..5_000 {
            config.extend_from_slice(format!("key{index} = value{index}\n").as_bytes());
        }
        assert!(config.len() < MAX_CREDENTIAL_CONFIG_BYTES as usize);
        assert_eq!(
            capture(&amplified, &config).status(),
            GitHttpsCredentialStatus::Unsafe
        );
    }

    #[test]
    fn replacement_races_fail_closed() {
        let config_race = Temp::new();
        let config_path = config_race.0.join("config");
        fs::write(&config_path, b"[credential]\nhelper = safe\n").unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        CONFIG_RACE.with(|race| {
            *race.borrow_mut() = Some((
                config_path.clone(),
                b"[credential]\nhelper = evil\n".to_vec(),
            ));
        });
        let uid = unsafe { libc::geteuid() };
        assert_eq!(
            capture_from_sources(
                Path::new("/usr/bin/git"),
                &[ConfigSource {
                    path: config_path,
                    owner: ConfigOwner::EffectiveUser,
                }],
                &[],
                Path::new("/missing/gh"),
                uid,
                uid,
            )
            .status(),
            GitHttpsCredentialStatus::Unsafe
        );

        let helper_race = Temp::new();
        let config_path = helper_race.0.join("config");
        fs::write(&config_path, b"[credential]\nhelper = safe\n").unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let helper_dir = helper_race.0.join("helpers");
        fs::create_dir(&helper_dir).unwrap();
        fs::set_permissions(&helper_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let helper = helper_dir.join("git-credential-safe");
        executable(&helper);
        HELPER_RACE.with(|race| *race.borrow_mut() = Some(helper));
        assert_eq!(
            capture_from_sources(
                Path::new("/usr/bin/git"),
                &[ConfigSource {
                    path: config_path,
                    owner: ConfigOwner::EffectiveUser,
                }],
                &[helper_dir.as_path()],
                &helper_dir.join("gh"),
                uid,
                uid,
            )
            .status(),
            GitHttpsCredentialStatus::Unsafe
        );
    }

    #[test]
    fn proxy_capture_and_resources_are_redacted_fixed_and_networkless() {
        let proxy = capture_credential_proxy_from(Some(Path::new("/usr/bin/true")));
        assert_eq!(proxy.status(), GitHttpsCredentialProxyStatus::Trusted);
        assert!(!format!("{proxy:?}").contains("/usr/bin/true"));
        let uid = unsafe { libc::geteuid() };
        let helper = match capture_helper_executable_result(Path::new("/usr/bin/true"), 0, uid) {
            FileCapture::Trusted(helper) => helper,
            FileCapture::Missing | FileCapture::Unsafe => panic!("system helper unavailable"),
        };
        let credentials = GitHttpsCredentialProvenance {
            status: GitHttpsCredentialStatus::Trusted,
            helper: Some(CredentialHelperCapability {
                kind: CredentialHelperKind::Direct,
                executable: helper,
            }),
        };
        let backing = unavailable_backing(GitHttpsCredentialBackingStatus::NotConfigured);
        let resources =
            GitHttpsCredentialResources::prepare(&credentials, &proxy, &backing).unwrap();
        assert_eq!(resources.helper_kind_env(), "direct");
        assert_eq!(
            format!("{resources:?}"),
            "GitHttpsCredentialResources { authority: \"<redacted>\" }"
        );

        let discovery = super::super::remote_discovery_command(
            Path::new("/usr/bin/git"),
            Path::new("/usr/bin/bwrap"),
            "https://example.invalid:8443/team/rehow.git",
            "HEAD",
            true,
            super::super::GitRemoteTransport::Https,
            super::super::GitRemoteAuthentication::Https(&resources),
        );
        let environment = discovery
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            environment["REHOW_CREDENTIAL_HOST"].as_deref(),
            Some("example.invalid:8443")
        );
        assert_eq!(
            environment["REHOW_CREDENTIAL_HELPER_KIND"].as_deref(),
            Some("direct")
        );
        let arguments = discovery
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments.iter().any(|argument| {
            argument == super::super::credential_proxy::CREDENTIAL_HELPER_CONFIG
        }));
        assert!(
            arguments
                .iter()
                .any(|argument| argument == SANDBOX_CREDENTIAL_PROXY)
        );
        assert!(
            arguments
                .iter()
                .any(|argument| argument == SANDBOX_CREDENTIAL_HELPER)
        );
        assert!(
            !arguments
                .iter()
                .any(|argument| argument.contains("workspace"))
        );

        let mut rejected = Command::new("/usr/bin/sh");
        rejected
            .args(["-c", "printf 'fatal: Authentication failed\\n' >&2; exit 1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        assert_eq!(
            super::super::run_clone_command_for_transport(
                rejected,
                &std::sync::atomic::AtomicBool::new(false),
                std::time::Duration::from_secs(2),
                1024,
                super::super::GitRemoteTransport::Https,
                super::super::GitRemoteAuthentication::Https(&resources),
            )
            .err()
            .unwrap(),
            super::super::GitCloneDiscoveryError::AuthenticationFailed
        );

        let mut command = Command::new("/usr/bin/bwrap");
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
            "--tmpfs",
            "/tmp",
            "--dir",
            "/home",
            "--dir",
            "/home/rehow",
        ]);
        resources.add_bubblewrap_mounts(&mut command);
        command
            .args([
                "--",
                "/usr/bin/test",
                "-x",
                SANDBOX_CREDENTIAL_PROXY,
                "-a",
                "-x",
                SANDBOX_CREDENTIAL_HELPER,
                "-a",
                "!",
                "-e",
                "/workspace",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());

        let missing = capture_credential_proxy_from(None);
        assert!(matches!(
            GitHttpsCredentialResources::prepare(&credentials, &missing, &backing),
            Err(GitHttpsCredentialResourceError::ProxyUnavailable)
        ));
        let unavailable = unavailable(GitHttpsCredentialStatus::NotConfigured);
        assert!(matches!(
            GitHttpsCredentialResources::prepare(&unavailable, &proxy, &backing),
            Err(GitHttpsCredentialResourceError::CredentialsUnavailable)
        ));

        let temp = Temp::new();
        let unsafe_proxy = temp.0.join("rehow-git-credential");
        fs::copy("/usr/bin/true", &unsafe_proxy).unwrap();
        fs::set_permissions(&unsafe_proxy, fs::Permissions::from_mode(0o722)).unwrap();
        assert_eq!(
            capture_credential_proxy_from(Some(&unsafe_proxy)).status(),
            GitHttpsCredentialProxyStatus::Unsafe
        );
    }

    #[test]
    fn github_backing_is_private_descriptor_held_and_required() {
        let temp = Temp::new();
        let config_dir = temp.0.join("gh");
        fs::create_dir(&config_dir).unwrap();
        fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let config = config_dir.join("hosts.yml");
        fs::write(&config, b"example.invalid:\n  oauth_token: canary\n").unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
        let backing = capture_credential_backing_from(Some(&config));
        assert_eq!(backing.status(), GitHttpsCredentialBackingStatus::Trusted);
        assert!(!format!("{backing:?}").contains(temp.0.to_str().unwrap()));
        assert!(!format!("{backing:?}").contains("canary"));
        fs::write(&config, b"replacement\n").unwrap();
        let mut snapshot = backing.gh_config.as_ref().unwrap().try_clone().unwrap();
        snapshot.seek(SeekFrom::Start(0)).unwrap();
        let mut snapshot_bytes = Vec::new();
        snapshot.read_to_end(&mut snapshot_bytes).unwrap();
        assert_eq!(snapshot_bytes, b"example.invalid:\n  oauth_token: canary\n");
        snapshot_bytes.fill(0);

        let proxy = capture_credential_proxy_from(Some(Path::new("/usr/bin/true")));
        let uid = unsafe { libc::geteuid() };
        let helper = match capture_helper_executable_result(Path::new("/usr/bin/true"), 0, uid) {
            FileCapture::Trusted(helper) => helper,
            FileCapture::Missing | FileCapture::Unsafe => panic!("system helper unavailable"),
        };
        let credentials = GitHttpsCredentialProvenance {
            status: GitHttpsCredentialStatus::Trusted,
            helper: Some(CredentialHelperCapability {
                kind: CredentialHelperKind::GithubCli,
                executable: helper,
            }),
        };
        let resources =
            GitHttpsCredentialResources::prepare(&credentials, &proxy, &backing).unwrap();
        assert_eq!(resources.helper_kind_env(), "githubCli");

        let mut command = Command::new("/usr/bin/bwrap");
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
            "--tmpfs",
            "/tmp",
            "--dir",
            "/home",
            "--dir",
            "/home/rehow",
        ]);
        resources.add_bubblewrap_mounts(&mut command);
        command
            .args(["--", "/usr/bin/test", "-f", SANDBOX_GH_CONFIG])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let missing = unavailable_backing(GitHttpsCredentialBackingStatus::NotConfigured);
        assert!(matches!(
            GitHttpsCredentialResources::prepare(&credentials, &proxy, &missing),
            Err(GitHttpsCredentialResourceError::BackingUnavailable)
        ));

        fs::set_permissions(&config, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            capture_credential_backing_from(Some(&config)).status(),
            GitHttpsCredentialBackingStatus::Unsafe
        );
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&config, config_dir.join("alias")).unwrap();
        assert_eq!(
            capture_credential_backing_from(Some(&config)).status(),
            GitHttpsCredentialBackingStatus::Unsafe
        );
    }
}

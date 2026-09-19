use super::{
    GitSshAgentProvenance, GitSshAgentStatus, GitSshHostKeysProvenance, GitSshHostKeysStatus,
    inspect_ssh_agent_parents, safe_provenance_path, ssh_agent_file_identity,
};
use std::{
    ffi::CString,
    fs::{self, File},
    io::{self, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            process::CommandExt,
        },
    },
    path::Path,
    process::Command,
};

const SANDBOX_PROXY: &str = "/home/rehow/bin/rehow-git-ssh";
const SANDBOX_SSH_CLIENT: &str = "/home/rehow/bin/ssh";
const SANDBOX_AGENT: &str = "/run/rehow-agent/agent.sock";
const SANDBOX_USER_KNOWN_HOSTS: &str = "/home/rehow/.ssh/known_hosts";
const SANDBOX_SYSTEM_KNOWN_HOSTS: &str = "/home/rehow/.ssh/system_known_hosts";
const MAX_COMBINED_KNOWN_HOSTS_BYTES: usize = 3 * 1024 * 1024 + 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitSshProxyStatus {
    NotConfigured,
    Trusted,
    Unsafe,
}

pub struct GitSshProxyProvenance {
    status: GitSshProxyStatus,
    executable: Option<File>,
}

impl GitSshProxyProvenance {
    pub fn status(&self) -> GitSshProxyStatus {
        self.status
    }
}

impl std::fmt::Debug for GitSshProxyProvenance {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GitSshProxyProvenance")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

pub fn capture_git_ssh_proxy_provenance() -> GitSshProxyProvenance {
    let candidate = std::env::current_exe().ok().and_then(|executable| {
        executable
            .parent()
            .map(|parent| parent.join("rehow-git-ssh"))
    });
    capture_git_ssh_proxy_from(candidate.as_deref())
}

pub(super) fn capture_git_ssh_proxy_from(candidate: Option<&Path>) -> GitSshProxyProvenance {
    let Some(candidate) = candidate else {
        return GitSshProxyProvenance {
            status: GitSshProxyStatus::NotConfigured,
            executable: None,
        };
    };
    match capture_proxy_executable(candidate) {
        ProxyCapture::Missing => GitSshProxyProvenance {
            status: GitSshProxyStatus::NotConfigured,
            executable: None,
        },
        ProxyCapture::Trusted(executable) => GitSshProxyProvenance {
            status: GitSshProxyStatus::Trusted,
            executable: Some(executable),
        },
        ProxyCapture::Unsafe => GitSshProxyProvenance {
            status: GitSshProxyStatus::Unsafe,
            executable: None,
        },
    }
}

enum ProxyCapture {
    Missing,
    Trusted(File),
    Unsafe,
}

fn capture_proxy_executable(path: &Path) -> ProxyCapture {
    if !safe_provenance_path(path) {
        return ProxyCapture::Unsafe;
    }
    let effective_user = unsafe { libc::geteuid() };
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return ProxyCapture::Missing,
        Err(_) => return ProxyCapture::Unsafe,
    };
    let before_parents = match inspect_ssh_agent_parents(path.parent().unwrap(), effective_user) {
        Some(parents) => parents,
        None => return ProxyCapture::Unsafe,
    };
    if !proxy_metadata_safe(&before, effective_user) {
        return ProxyCapture::Unsafe;
    }
    let executable = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(executable) => executable,
        Err(_) => return ProxyCapture::Unsafe,
    };
    let held = match executable.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return ProxyCapture::Unsafe,
    };
    let after = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return ProxyCapture::Unsafe,
    };
    let after_parents = match inspect_ssh_agent_parents(path.parent().unwrap(), effective_user) {
        Some(parents) => parents,
        None => return ProxyCapture::Unsafe,
    };
    if !proxy_metadata_safe(&held, effective_user)
        || ssh_agent_file_identity(&before) != ssh_agent_file_identity(&held)
        || ssh_agent_file_identity(&after) != ssh_agent_file_identity(&held)
        || before_parents != after_parents
    {
        return ProxyCapture::Unsafe;
    }
    ProxyCapture::Trusted(executable)
}

fn proxy_metadata_safe(metadata: &fs::Metadata, effective_user: u32) -> bool {
    metadata.is_file()
        && metadata.nlink() == 1
        && (metadata.uid() == 0 || metadata.uid() == effective_user)
        && metadata.permissions().mode() & 0o022 == 0
        && metadata.permissions().mode() & 0o111 != 0
}

fn capture_system_ssh_client(path: &Path) -> Option<File> {
    match capture_proxy_executable(path) {
        ProxyCapture::Trusted(executable) if executable.metadata().ok()?.uid() == 0 => {
            Some(executable)
        }
        ProxyCapture::Missing | ProxyCapture::Unsafe | ProxyCapture::Trusted(_) => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitSshTransportError {
    AgentUnavailable,
    HostKeysUnavailable,
    ProxyUnavailable,
    SshUnavailable,
    ResourceLimit,
    Io,
}

/// Opaque, launch-bound SSH authority for one or more confined Git reads.
/// It exposes neither source paths nor descriptors.
pub struct GitSshTransportResources {
    agent: File,
    proxy: File,
    ssh_client: File,
    known_hosts: File,
    system_known_hosts: File,
}

impl std::fmt::Debug for GitSshTransportResources {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GitSshTransportResources")
            .field("authority", &"<redacted>")
            .finish()
    }
}

impl GitSshTransportResources {
    pub fn prepare(
        agent: &GitSshAgentProvenance,
        host_keys: &GitSshHostKeysProvenance,
        proxy: &GitSshProxyProvenance,
    ) -> Result<Self, GitSshTransportError> {
        if agent.status != GitSshAgentStatus::Trusted {
            return Err(GitSshTransportError::AgentUnavailable);
        }
        if host_keys.status != GitSshHostKeysStatus::Trusted {
            return Err(GitSshTransportError::HostKeysUnavailable);
        }
        if proxy.status != GitSshProxyStatus::Trusted {
            return Err(GitSshTransportError::ProxyUnavailable);
        }
        let agent = agent
            ._socket
            .as_ref()
            .ok_or(GitSshTransportError::AgentUnavailable)?
            .try_clone()
            .map_err(|_| GitSshTransportError::Io)?;
        let proxy = proxy
            .executable
            .as_ref()
            .ok_or(GitSshTransportError::ProxyUnavailable)?
            .try_clone()
            .map_err(|_| GitSshTransportError::Io)?;
        let ssh_client = capture_system_ssh_client(Path::new("/usr/bin/ssh"))
            .ok_or(GitSshTransportError::SshUnavailable)?;
        let mut combined = Vec::new();
        for source in &host_keys._sources {
            let extra = source
                .len()
                .checked_add(1)
                .ok_or(GitSshTransportError::ResourceLimit)?;
            if combined
                .len()
                .checked_add(extra)
                .ok_or(GitSshTransportError::ResourceLimit)?
                > MAX_COMBINED_KNOWN_HOSTS_BYTES
            {
                return Err(GitSshTransportError::ResourceLimit);
            }
            combined.extend_from_slice(source);
            if !source.ends_with(b"\n") {
                combined.push(b'\n');
            }
        }
        let known_hosts = sealed_memfd("rehow-known-hosts", &combined)?;
        let system_known_hosts = sealed_memfd("rehow-system-known-hosts", b"")?;
        Ok(Self {
            agent,
            proxy,
            ssh_client,
            known_hosts,
            system_known_hosts,
        })
    }

    pub(crate) fn add_bubblewrap_mounts(&self, command: &mut Command) {
        let agent_fd = self.agent.as_raw_fd();
        let proxy_fd = self.proxy.as_raw_fd();
        let ssh_client_fd = self.ssh_client.as_raw_fd();
        let known_hosts_fd = self.known_hosts.as_raw_fd();
        let system_known_hosts_fd = self.system_known_hosts.as_raw_fd();
        command
            .args(["--dir", "/run/rehow-agent", "--ro-bind-fd"])
            .arg(agent_fd.to_string())
            .arg(SANDBOX_AGENT)
            .args(["--dir", "/home/rehow/bin", "--ro-bind-fd"])
            .arg(proxy_fd.to_string())
            .arg(SANDBOX_PROXY)
            .args(["--ro-bind-fd"])
            .arg(ssh_client_fd.to_string())
            .arg(SANDBOX_SSH_CLIENT)
            .args([
                "--dir",
                "/home/rehow/.ssh",
                "--perms",
                "0400",
                "--ro-bind-data",
            ])
            .arg(known_hosts_fd.to_string())
            .arg(SANDBOX_USER_KNOWN_HOSTS)
            .args(["--perms", "0400", "--ro-bind-data"])
            .arg(system_known_hosts_fd.to_string())
            .arg(SANDBOX_SYSTEM_KNOWN_HOSTS);
        // SAFETY: these exact four descriptors are consumed by Bubblewrap's
        // fixed mount/file arguments before it execs the selected child.
        unsafe {
            command.pre_exec(move || {
                for descriptor in [known_hosts_fd, system_known_hosts_fd] {
                    if libc::lseek(descriptor, 0, libc::SEEK_SET) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                for descriptor in [
                    agent_fd,
                    proxy_fd,
                    ssh_client_fd,
                    known_hosts_fd,
                    system_known_hosts_fd,
                ] {
                    clear_close_on_exec(descriptor)?;
                }
                Ok(())
            });
        }
    }

    #[cfg(test)]
    fn known_hosts_seals(&self) -> i32 {
        unsafe { libc::fcntl(self.known_hosts.as_raw_fd(), libc::F_GET_SEALS) }
    }
}

fn clear_close_on_exec(descriptor: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
    if flags < 0
        || unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } != 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn sealed_memfd(name: &str, bytes: &[u8]) -> Result<File, GitSshTransportError> {
    let name = CString::new(name).map_err(|_| GitSshTransportError::Io)?;
    let descriptor =
        unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
    if descriptor < 0 {
        return Err(GitSshTransportError::Io);
    }
    let mut file = unsafe { File::from_raw_fd(descriptor) };
    file.write_all(bytes)
        .map_err(|_| GitSshTransportError::Io)?;
    file.flush().map_err(|_| GitSshTransportError::Io)?;
    if unsafe { libc::fchmod(file.as_raw_fd(), 0o400) } != 0 {
        return Err(GitSshTransportError::Io);
    }
    let seals = libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) } != 0 {
        return Err(GitSshTransportError::Io);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| GitSshTransportError::Io)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::net::UnixListener, path::PathBuf, process::Stdio};

    struct Temp(PathBuf);

    impl Temp {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("rehow-ssh-sandbox-{}", uuid::Uuid::now_v7()));
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

    #[test]
    fn proxy_capture_is_identity_held_and_redacted() {
        let proxy = capture_git_ssh_proxy_from(Some(Path::new("/usr/bin/true")));
        assert_eq!(proxy.status(), GitSshProxyStatus::Trusted);
        let debug = format!("{proxy:?}");
        assert!(!debug.contains("/usr/bin/true"));

        let temp = Temp::new();
        let unsafe_proxy = temp.0.join("proxy");
        fs::copy("/usr/bin/true", &unsafe_proxy).unwrap();
        fs::set_permissions(&unsafe_proxy, fs::Permissions::from_mode(0o722)).unwrap();
        assert_eq!(
            capture_git_ssh_proxy_from(Some(&unsafe_proxy)).status(),
            GitSshProxyStatus::Unsafe
        );
        let linked = temp.0.join("linked");
        std::os::unix::fs::symlink("/usr/bin/true", &linked).unwrap();
        assert_eq!(
            capture_git_ssh_proxy_from(Some(&linked)).status(),
            GitSshProxyStatus::Unsafe
        );
        assert!(capture_system_ssh_client(Path::new("/usr/bin/ssh")).is_some());
        fs::set_permissions(&unsafe_proxy, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(capture_system_ssh_client(&unsafe_proxy).is_none());
    }

    #[test]
    fn resource_preparation_requires_every_trusted_capability_and_bounds_aggregation() {
        let temp = Temp::new();
        let socket_path = temp.0.join("agent.sock");
        let _listener = UnixListener::bind(&socket_path).unwrap();
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).unwrap();
        let trusted_agent = GitSshAgentProvenance {
            status: GitSshAgentStatus::Trusted,
            _socket: Some(
                fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(&socket_path)
                    .unwrap(),
            ),
        };
        let missing_agent = GitSshAgentProvenance {
            status: GitSshAgentStatus::NotConfigured,
            _socket: None,
        };
        let trusted_keys = GitSshHostKeysProvenance {
            status: GitSshHostKeysStatus::Trusted,
            _sources: vec![b"key\n".to_vec()],
        };
        let missing_keys = GitSshHostKeysProvenance {
            status: GitSshHostKeysStatus::NotConfigured,
            _sources: Vec::new(),
        };
        let trusted_proxy = capture_git_ssh_proxy_from(Some(Path::new("/usr/bin/true")));
        let missing_proxy = capture_git_ssh_proxy_from(None);
        assert!(matches!(
            GitSshTransportResources::prepare(&missing_agent, &trusted_keys, &trusted_proxy),
            Err(GitSshTransportError::AgentUnavailable)
        ));
        assert!(matches!(
            GitSshTransportResources::prepare(&trusted_agent, &missing_keys, &trusted_proxy),
            Err(GitSshTransportError::HostKeysUnavailable)
        ));
        assert!(matches!(
            GitSshTransportResources::prepare(&trusted_agent, &trusted_keys, &missing_proxy),
            Err(GitSshTransportError::ProxyUnavailable)
        ));
        let oversized = GitSshHostKeysProvenance {
            status: GitSshHostKeysStatus::Trusted,
            _sources: vec![vec![0; MAX_COMBINED_KNOWN_HOSTS_BYTES]],
        };
        assert!(matches!(
            GitSshTransportResources::prepare(&trusted_agent, &oversized, &trusted_proxy),
            Err(GitSshTransportError::ResourceLimit)
        ));
    }

    #[test]
    fn sealed_resources_mount_only_fixed_agent_keys_and_proxy_in_networkless_sandbox() {
        let temp = Temp::new();
        let socket_path = temp.0.join("agent.sock");
        let _listener = UnixListener::bind(&socket_path).unwrap();
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).unwrap();
        let agent = GitSshAgentProvenance {
            status: GitSshAgentStatus::Trusted,
            _socket: Some(
                fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                    .open(&socket_path)
                    .unwrap(),
            ),
        };
        let host_keys = GitSshHostKeysProvenance {
            status: GitSshHostKeysStatus::Trusted,
            _sources: vec![b"example.invalid ssh-ed25519 AAAAC3Nza\n".to_vec()],
        };
        let proxy = capture_git_ssh_proxy_from(Some(Path::new("/usr/bin/true")));
        let resources = GitSshTransportResources::prepare(&agent, &host_keys, &proxy).unwrap();
        let required_seals =
            libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
        assert_eq!(
            resources.known_hosts_seals() & required_seals,
            required_seals
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
            "--dir",
            "/run",
        ]);
        resources.add_bubblewrap_mounts(&mut command);
        command
            .args([
                "--",
                "/usr/bin/test",
                "-S",
                SANDBOX_AGENT,
                "-a",
                "-r",
                SANDBOX_USER_KNOWN_HOSTS,
                "-a",
                "-r",
                SANDBOX_SYSTEM_KNOWN_HOSTS,
                "-a",
                "-x",
                SANDBOX_PROXY,
                "-a",
                "-x",
                SANDBOX_SSH_CLIENT,
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
    }
}

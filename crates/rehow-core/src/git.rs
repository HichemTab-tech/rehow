//! Bounded, noninteractive, read-only Git discovery and status.

use crate::storage::{GitRepositoryCapabilityError, WorkspaceView};
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
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
    use crate::storage::{WorkspaceInspection, inspect_workspace};
    use std::{fs, process::Command};

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
        let (_, inspection) = inspect_workspace(&temp.0).unwrap().into_parts();
        let WorkspaceInspection::Compatible(workspace) = inspection else {
            panic!()
        };
        workspace
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
}

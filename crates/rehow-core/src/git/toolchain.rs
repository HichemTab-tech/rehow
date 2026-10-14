//! Trusted discovery of the Git, OpenSSH, and Bubblewrap executables and the
//! observable Git sandbox state (ADR 0064).
//!
//! Executables are resolved only from a fixed list of absolute system/Nix/
//! Flatpak directories. `PATH` (and anything else a repository could
//! influence) is never consulted. Each candidate is canonicalized; the final
//! regular file and every ancestor directory must be immutable to other users.

use std::{
    ffi::CString,
    fs, io,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
        io::{AsRawFd, FromRawFd},
    },
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Fixed directory family a trusted executable was resolved from.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitExecutableSource {
    /// `/usr/bin` (the historical fixed location).
    UsrBin,
    /// `/bin` on systems without a merged `/usr`.
    Bin,
    /// `/usr/local/bin`.
    UsrLocalBin,
    /// NixOS system profile, `/run/current-system/sw/bin`.
    NixSystemProfile,
    /// Multi-user Nix default profile, `/nix/var/nix/profiles/default/bin`.
    NixDefaultProfile,
    /// The launching user's `$HOME/.nix-profile/bin`.
    NixUserProfile,
    /// A Flatpak application bundle, `/app/bin`.
    FlatpakApp,
}

impl GitExecutableSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UsrBin => "usrBin",
            Self::Bin => "bin",
            Self::UsrLocalBin => "usrLocalBin",
            Self::NixSystemProfile => "nixSystemProfile",
            Self::NixDefaultProfile => "nixDefaultProfile",
            Self::NixUserProfile => "nixUserProfile",
            Self::FlatpakApp => "flatpakApp",
        }
    }
}

/// Whether repository Git commands run inside Bubblewrap.
///
/// `Unsandboxed` is an explicit, user-visible degraded mode: only the
/// read-only status path runs, directly under the hardened environment and a
/// private synthetic Git directory. Every mutating or network operation still
/// requires `Bubblewrap` and fails with `SandboxUnavailable` (or the
/// operation's closed equivalent) otherwise.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GitSandbox {
    Bubblewrap,
    Unsandboxed,
}

impl GitSandbox {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bubblewrap => "bubblewrap",
            Self::Unsandboxed => "unsandboxed",
        }
    }
}

/// Launch-independent summary of the local Git toolchain. It exposes no path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GitEnvironment {
    git_source: Option<GitExecutableSource>,
    sandbox: GitSandbox,
}

impl GitEnvironment {
    pub fn git_available(&self) -> bool {
        self.git_source.is_some()
    }

    pub fn git_source(&self) -> Option<GitExecutableSource> {
        self.git_source
    }

    pub fn sandbox(&self) -> GitSandbox {
        self.sandbox
    }
}

/// Resolves the trusted Git executable and whether Bubblewrap is usable with
/// it. This may start one short `git --version` probe inside Bubblewrap the
/// first time a given pair is seen.
pub fn inspect_git_environment() -> GitEnvironment {
    let git = discover_executable("git");
    let sandbox = match &git {
        Some(git) if super::trusted_sandbox_for(&git.path).is_some() => GitSandbox::Bubblewrap,
        _ => GitSandbox::Unsandboxed,
    };
    GitEnvironment {
        git_source: git.map(|git| git.source),
        sandbox,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrustedExecutable {
    /// Canonical absolute path; this exact path is executed.
    pub(crate) path: PathBuf,
    pub(crate) source: GitExecutableSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SearchRoot {
    pub(crate) directory: PathBuf,
    pub(crate) source: GitExecutableSource,
}

#[cfg(test)]
thread_local! {
    static SEARCH_ROOTS_OVERRIDE: std::cell::RefCell<Option<Vec<SearchRoot>>> =
        const { std::cell::RefCell::new(None) };
    static SANDBOX_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test-only seam: replaces the fixed search roots on the current thread.
#[cfg(test)]
pub(crate) fn set_test_search_roots(roots: Option<Vec<SearchRoot>>) {
    SEARCH_ROOTS_OVERRIDE.with_borrow_mut(|current| *current = roots);
}

/// Test-only seam: makes Bubblewrap unavailable on the current thread.
#[cfg(test)]
pub(crate) fn set_test_sandbox_disabled(disabled: bool) {
    SANDBOX_DISABLED.set(disabled);
}

#[cfg(test)]
pub(crate) fn test_sandbox_disabled() -> bool {
    SANDBOX_DISABLED.get()
}

#[cfg(not(test))]
pub(crate) fn test_sandbox_disabled() -> bool {
    false
}

fn search_roots() -> Vec<SearchRoot> {
    #[cfg(test)]
    if let Some(roots) = SEARCH_ROOTS_OVERRIDE.with_borrow(Clone::clone) {
        return roots;
    }
    default_search_roots(std::env::var_os("HOME").as_deref().map(Path::new))
}

/// Fixed trusted directories, in priority order. `/usr/bin` stays first so an
/// existing root-owned `/usr/bin/git` keeps the historical behavior.
pub(crate) fn default_search_roots(home: Option<&Path>) -> Vec<SearchRoot> {
    let mut roots = vec![
        root("/usr/bin", GitExecutableSource::UsrBin),
        root("/bin", GitExecutableSource::Bin),
        root("/usr/local/bin", GitExecutableSource::UsrLocalBin),
        root(
            "/run/current-system/sw/bin",
            GitExecutableSource::NixSystemProfile,
        ),
        root(
            "/nix/var/nix/profiles/default/bin",
            GitExecutableSource::NixDefaultProfile,
        ),
    ];
    if let Some(home) = home
        && super::safe_provenance_path(home)
    {
        roots.push(SearchRoot {
            directory: home.join(".nix-profile/bin"),
            source: GitExecutableSource::NixUserProfile,
        });
    }
    roots.push(root("/app/bin", GitExecutableSource::FlatpakApp));
    roots
}

fn root(directory: &str, source: GitExecutableSource) -> SearchRoot {
    SearchRoot {
        directory: PathBuf::from(directory),
        source,
    }
}

pub(crate) fn discover_executable(name: &str) -> Option<TrustedExecutable> {
    let effective_user = unsafe { libc::geteuid() };
    discover_executable_in(name, &search_roots(), effective_user)
}

pub(crate) fn discover_executable_in(
    name: &str,
    roots: &[SearchRoot],
    effective_user: u32,
) -> Option<TrustedExecutable> {
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return None;
    }
    roots.iter().find_map(|root| {
        if !root.directory.is_absolute() {
            return None;
        }
        verify_executable(&root.directory.join(name), effective_user).map(|path| {
            TrustedExecutable {
                path,
                source: root.source,
            }
        })
    })
}

/// Returns the canonical path when `candidate` resolves to a regular,
/// executable file that no other user can modify or replace.
pub(crate) fn verify_executable(candidate: &Path, effective_user: u32) -> Option<PathBuf> {
    // Missing or dangling candidates are simply skipped.
    fs::symlink_metadata(candidate).ok()?;
    let canonical = fs::canonicalize(candidate).ok()?;
    if !super::safe_provenance_path(&canonical) {
        return None;
    }
    let parent = canonical.parent()?;
    let before_parents = inspect_executable_ancestors(parent, effective_user)?;
    let held = open_path_nofollow(&canonical).ok()?;
    let held_metadata = held.metadata().ok()?;
    let read_only = descriptor_on_read_only_mount(&held);
    if !executable_metadata_safe(&held_metadata, effective_user, read_only) {
        return None;
    }
    let after = fs::symlink_metadata(&canonical).ok()?;
    let after_parents = inspect_executable_ancestors(parent, effective_user)?;
    if after.dev() != held_metadata.dev()
        || after.ino() != held_metadata.ino()
        || before_parents != after_parents
    {
        return None;
    }
    Some(canonical)
}

pub(crate) fn executable_metadata_safe(
    metadata: &fs::Metadata,
    effective_user: u32,
    read_only_mount: bool,
) -> bool {
    let mode = metadata.permissions().mode();
    let owner_trusted = metadata.uid() == 0 || metadata.uid() == effective_user || read_only_mount;
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && owner_trusted
        && mode & 0o022 == 0
        && mode & 0o111 != 0
}

/// Accepts a directory owned by root or the effective user that others cannot
/// write, a root-owned sticky directory (`/tmp`, `/nix/store`), or any
/// directory on a read-only mount (Flatpak runtimes, read-only Nix stores).
pub(crate) fn executable_directory_safe(
    metadata: &fs::Metadata,
    effective_user: u32,
    read_only_mount: bool,
) -> bool {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return false;
    }
    if read_only_mount {
        return true;
    }
    let mode = metadata.permissions().mode();
    (metadata.uid() == effective_user && mode & 0o022 == 0)
        || (metadata.uid() == 0 && (mode & 0o022 == 0 || mode & 0o1000 != 0))
}

type DirectoryIdentity = (u64, u64, u32, u32);

fn inspect_executable_ancestors(
    start: &Path,
    effective_user: u32,
) -> Option<Vec<DirectoryIdentity>> {
    let mut identities = Vec::new();
    let mut current = Some(start);
    while let Some(path) = current {
        let metadata = fs::symlink_metadata(path).ok()?;
        if !executable_directory_safe(&metadata, effective_user, path_on_read_only_mount(path)) {
            return None;
        }
        identities.push((
            metadata.dev(),
            metadata.ino(),
            metadata.uid(),
            metadata.mode(),
        ));
        current = path.parent();
    }
    Some(identities)
}

fn open_path_nofollow(path: &Path) -> io::Result<fs::File> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: `path` is NUL-terminated; O_PATH holds the inode without
    // reading it and O_NOFOLLOW rejects a final symlink.
    let descriptor = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `descriptor` is newly owned after a successful `open`.
    Ok(unsafe { fs::File::from_raw_fd(descriptor) })
}

fn descriptor_on_read_only_mount(file: &fs::File) -> bool {
    let mut buffer = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: the descriptor is valid and `buffer` is writable.
    let result = unsafe { libc::fstatvfs(file.as_raw_fd(), buffer.as_mut_ptr()) };
    // SAFETY: initialized on success.
    result == 0 && unsafe { buffer.assume_init() }.f_flag & libc::ST_RDONLY != 0
}

fn path_on_read_only_mount(path: &Path) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut buffer = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `buffer` is writable.
    let result = unsafe { libc::statvfs(path.as_ptr(), buffer.as_mut_ptr()) };
    // SAFETY: initialized on success.
    result == 0 && unsafe { buffer.assume_init() }.f_flag & libc::ST_RDONLY != 0
}

/// Extra read-only mounts that make `executable` (and its dynamic loader and
/// libraries) visible inside a Bubblewrap view that otherwise only exposes
/// `/usr`. Returns `None` when the executable cannot be exposed safely.
pub(crate) fn sandbox_toolchain_mounts(executable: &Path) -> Option<&'static [&'static str]> {
    if executable.starts_with("/usr") {
        Some(&[])
    } else if executable.starts_with("/nix/store") {
        Some(&["--ro-bind", "/nix/store", "/nix/store"])
    } else {
        None
    }
}

/// Successful Bubblewrap probes, keyed by the exact canonical pair.
static SANDBOX_PROBES: Mutex<Vec<(PathBuf, PathBuf)>> = Mutex::new(Vec::new());

pub(crate) fn sandbox_probe_cached(sandbox: &Path, git: &Path) -> bool {
    SANDBOX_PROBES.lock().is_ok_and(|probes| {
        probes
            .iter()
            .any(|(cached_sandbox, cached_git)| cached_sandbox == sandbox && cached_git == git)
    })
}

pub(crate) fn record_sandbox_probe(sandbox: &Path, git: &Path) {
    if let Ok(mut probes) = SANDBOX_PROBES.lock()
        && !probes
            .iter()
            .any(|(cached_sandbox, cached_git)| cached_sandbox == sandbox && cached_git == git)
    {
        probes.push((sandbox.to_owned(), git.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    struct Temp(PathBuf);

    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "rehow-toolchain-{}",
                crate::protocol::new_operation_id()
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }

        fn dir(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fake_executable(path: &Path, mode: u32) {
        fs::write(path, b"#!/bin/false\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn user() -> u32 {
        unsafe { libc::geteuid() }
    }

    fn roots(entries: &[(&Path, GitExecutableSource)]) -> Vec<SearchRoot> {
        entries
            .iter()
            .map(|(directory, source)| SearchRoot {
                directory: directory.to_path_buf(),
                source: *source,
            })
            .collect()
    }

    #[test]
    fn discovery_uses_first_trusted_root_and_reports_its_source() {
        let temp = Temp::new();
        let first = temp.dir("first");
        let second = temp.dir("second");
        fake_executable(&second.join("git"), 0o755);
        let roots = roots(&[
            (&first, GitExecutableSource::UsrBin),
            (&second, GitExecutableSource::NixSystemProfile),
        ]);
        let found = discover_executable_in("git", &roots, user()).unwrap();
        assert_eq!(found.path, second.join("git"));
        assert_eq!(found.source, GitExecutableSource::NixSystemProfile);

        fake_executable(&first.join("git"), 0o755);
        let found = discover_executable_in("git", &roots, user()).unwrap();
        assert_eq!(found.path, first.join("git"));
        assert_eq!(found.source, GitExecutableSource::UsrBin);
    }

    #[test]
    fn discovery_ignores_path_and_rejects_relative_roots_and_names() {
        let temp = Temp::new();
        let bin = temp.dir("bin");
        fake_executable(&bin.join("git"), 0o755);
        let relative = roots(&[(Path::new("bin"), GitExecutableSource::UsrBin)]);
        assert!(discover_executable_in("git", &relative, user()).is_none());
        let absolute = roots(&[(&bin, GitExecutableSource::UsrBin)]);
        for name in ["", ".", "..", "../bin/git", "sub/git"] {
            assert!(discover_executable_in(name, &absolute, user()).is_none());
        }
        assert!(discover_executable_in("missing", &absolute, user()).is_none());
    }

    #[test]
    fn discovery_rejects_writable_non_executable_and_non_regular_candidates() {
        let temp = Temp::new();
        let bin = temp.dir("bin");
        let roots = roots(&[(&bin, GitExecutableSource::UsrBin)]);
        for mode in [0o775, 0o757, 0o777, 0o644] {
            fake_executable(&bin.join("git"), mode);
            assert!(
                discover_executable_in("git", &roots, user()).is_none(),
                "mode {mode:o} must be rejected"
            );
        }
        fs::remove_file(bin.join("git")).unwrap();
        fs::create_dir(bin.join("git")).unwrap();
        assert!(discover_executable_in("git", &roots, user()).is_none());
    }

    #[test]
    fn discovery_rejects_foreign_owner_unless_mount_is_read_only() {
        let temp = Temp::new();
        let bin = temp.dir("bin");
        fake_executable(&bin.join("git"), 0o755);
        let metadata = fs::metadata(bin.join("git")).unwrap();
        // Simulate a file owned by another user: pretend the effective user
        // differs from both root and the actual owner.
        let foreign_user = metadata.uid().wrapping_add(4242);
        if metadata.uid() != 0 {
            assert!(!executable_metadata_safe(&metadata, foreign_user, false));
            assert!(executable_metadata_safe(&metadata, foreign_user, true));
            let roots = roots(&[(&bin, GitExecutableSource::UsrBin)]);
            assert!(discover_executable_in("git", &roots, foreign_user).is_none());
        }
        assert!(executable_metadata_safe(&metadata, metadata.uid(), false));
    }

    #[test]
    fn discovery_rejects_writable_ancestors_but_accepts_sticky_root_directories() {
        let temp = Temp::new();
        let shared = temp.dir("shared");
        let bin = shared.join("bin");
        fs::create_dir(&bin).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        fake_executable(&bin.join("git"), 0o755);
        let roots = roots(&[(&bin, GitExecutableSource::UsrBin)]);
        assert!(discover_executable_in("git", &roots, user()).is_some());
        for mode in [0o775, 0o777, 0o1777] {
            fs::set_permissions(&shared, fs::Permissions::from_mode(mode)).unwrap();
            // A user-owned sticky world-writable directory is still unsafe;
            // only root-owned sticky directories are accepted.
            assert!(
                discover_executable_in("git", &roots, user()).is_none() || user() == 0,
                "ancestor mode {mode:o} must be rejected"
            );
        }
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o755)).unwrap();

        let sticky_root = fs::metadata("/tmp").unwrap();
        if sticky_root.uid() == 0 && sticky_root.permissions().mode() & 0o1000 != 0 {
            assert!(executable_directory_safe(&sticky_root, user(), false));
        }
        let nix_like = fs::metadata(&shared).unwrap();
        assert!(executable_directory_safe(&nix_like, user(), false));
        assert!(
            !executable_directory_safe(&nix_like, nix_like.uid().wrapping_add(4242), false)
                || nix_like.uid() == 0
        );
        assert!(executable_directory_safe(
            &nix_like,
            nix_like.uid().wrapping_add(4242),
            true
        ));
    }

    #[test]
    fn discovery_canonicalizes_symlinks_and_checks_the_target() {
        let temp = Temp::new();
        let store = temp.dir("store/hash-git/bin");
        let profile = temp.dir("profile/bin");
        fake_executable(&store.join("git"), 0o555);
        symlink(store.join("git"), profile.join("git")).unwrap();
        let roots = roots(&[(&profile, GitExecutableSource::NixUserProfile)]);
        let found = discover_executable_in("git", &roots, user()).unwrap();
        assert_eq!(found.path, store.join("git"));
        assert_eq!(found.source, GitExecutableSource::NixUserProfile);

        // The link itself may live in a safe directory, but an unsafe target
        // is still rejected after canonicalization.
        fs::set_permissions(store.join("git"), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(discover_executable_in("git", &roots, user()).is_none());
        fs::set_permissions(store.join("git"), fs::Permissions::from_mode(0o555)).unwrap();
        let store_parent = temp.0.join("store");
        fs::set_permissions(&store_parent, fs::Permissions::from_mode(0o777)).unwrap();
        if user() != 0 {
            assert!(discover_executable_in("git", &roots, user()).is_none());
        }
        fs::set_permissions(&store_parent, fs::Permissions::from_mode(0o755)).unwrap();

        // Dangling links are skipped in favour of the next root.
        let dangling = temp.dir("dangling");
        symlink(temp.0.join("nowhere"), dangling.join("git")).unwrap();
        let roots = roots_with_fallback(&dangling, &store);
        let found = discover_executable_in("git", &roots, user()).unwrap();
        assert_eq!(found.path, store.join("git"));
    }

    fn roots_with_fallback(first: &Path, second: &Path) -> Vec<SearchRoot> {
        roots(&[
            (first, GitExecutableSource::UsrBin),
            (second, GitExecutableSource::UsrLocalBin),
        ])
    }

    #[test]
    fn default_roots_are_fixed_absolute_and_ignore_unsafe_home() {
        let roots = default_search_roots(Some(Path::new("/home/person")));
        assert_eq!(roots[0].directory, Path::new("/usr/bin"));
        assert_eq!(roots[0].source, GitExecutableSource::UsrBin);
        assert!(roots.iter().all(|root| root.directory.is_absolute()));
        assert!(
            roots
                .iter()
                .any(|root| root.directory == Path::new("/home/person/.nix-profile/bin"))
        );
        for home in ["relative", "/home/../etc", ""] {
            let roots = default_search_roots(Some(Path::new(home)));
            assert!(
                !roots
                    .iter()
                    .any(|root| root.source == GitExecutableSource::NixUserProfile)
            );
        }
        assert!(
            !default_search_roots(None)
                .iter()
                .any(|root| root.source == GitExecutableSource::NixUserProfile)
        );
    }

    #[test]
    fn sandbox_mounts_expose_only_usr_or_the_nix_store() {
        assert_eq!(
            sandbox_toolchain_mounts(Path::new("/usr/bin/git")),
            Some(&[][..])
        );
        assert_eq!(
            sandbox_toolchain_mounts(Path::new("/nix/store/abc-git/bin/git")),
            Some(&["--ro-bind", "/nix/store", "/nix/store"][..])
        );
        assert_eq!(
            sandbox_toolchain_mounts(Path::new("/opt/git/bin/git")),
            None
        );
        assert_eq!(sandbox_toolchain_mounts(Path::new("/home/u/bin/git")), None);
    }

    #[test]
    fn source_and_sandbox_names_are_stable() {
        assert_eq!(GitExecutableSource::UsrBin.as_str(), "usrBin");
        assert_eq!(
            GitExecutableSource::NixSystemProfile.as_str(),
            "nixSystemProfile"
        );
        assert_eq!(GitSandbox::Bubblewrap.as_str(), "bubblewrap");
        assert_eq!(GitSandbox::Unsandboxed.as_str(), "unsandboxed");
    }
}

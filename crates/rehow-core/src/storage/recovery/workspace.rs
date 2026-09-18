use super::valid_temp;
use crate::storage::linux::{
    DirectoryCapability, ManagedState, ManagedTarget, MutationIoError, RegularState,
};

const TARGET_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkspaceMutationError {
    Unsafe,
    Conflict,
    Unsupported,
    TooLarge,
    Io,
}

pub(super) struct DisplacedTarget {
    target: ManagedTarget,
    temp_name: String,
    prior: RegularState,
}

pub(super) enum InstallOutcome {
    Created,
    Replaced(DisplacedTarget),
}

pub(super) fn install_object(
    root: &DirectoryCapability,
    relative_path: &str,
    expected: Option<(&[u8], u32)>,
    intended: &[u8],
    mode: u32,
    temp_name: &str,
) -> Result<InstallOutcome, WorkspaceMutationError> {
    if !valid_temp(temp_name) {
        return Err(WorkspaceMutationError::Unsafe);
    }
    let target = root.open_managed_target(relative_path).map_err(map_io)?;
    let prior = target.state(TARGET_LIMIT).map_err(map_io)?;
    match (&prior, expected) {
        (ManagedState::Missing, None) => {}
        (ManagedState::Regular(current), Some((expected, expected_mode)))
            if current.bytes == expected && current.mode == expected_mode => {}
        _ => return Err(WorkspaceMutationError::Conflict),
    }
    match target.temp_state(temp_name, TARGET_LIMIT).map_err(map_io)? {
        ManagedState::Missing => target
            .write_temp(temp_name, intended, mode)
            .map_err(map_io)?,
        ManagedState::Regular(existing) if existing.bytes == intended && existing.mode == mode => {}
        ManagedState::Regular(_) => return Err(WorkspaceMutationError::Conflict),
    }

    match prior {
        ManagedState::Missing => {
            target.install_no_replace(temp_name).map_err(map_io)?;
            verify(&target, intended, mode)?;
            target.sync_directory().map_err(map_io)?;
            verify(&target, intended, mode)?;
            Ok(InstallOutcome::Created)
        }
        ManagedState::Regular(prior) => {
            target.exchange(temp_name).map_err(map_io)?;
            let target_matches = verify(&target, intended, mode).is_ok();
            let displaced_matches = matches!(
                target.temp_state(temp_name, TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Regular(ref displaced)
                    if displaced.bytes == prior.bytes && displaced.mode == prior.mode
            );
            if !target_matches || !displaced_matches {
                return Err(WorkspaceMutationError::Conflict);
            }
            target.sync_directory().map_err(map_io)?;
            verify(&target, intended, mode)?;
            Ok(InstallOutcome::Replaced(DisplacedTarget {
                target,
                temp_name: temp_name.to_owned(),
                prior,
            }))
        }
    }
}

impl DisplacedTarget {
    pub(super) fn verify(&self) -> Result<(), WorkspaceMutationError> {
        match self
            .target
            .temp_state(&self.temp_name, TARGET_LIMIT)
            .map_err(map_io)?
        {
            ManagedState::Regular(state) if state == self.prior => Ok(()),
            _ => Err(WorkspaceMutationError::Conflict),
        }
    }
}

fn verify(target: &ManagedTarget, bytes: &[u8], mode: u32) -> Result<(), WorkspaceMutationError> {
    match target.state(TARGET_LIMIT).map_err(map_io)? {
        ManagedState::Regular(state) if state.bytes == bytes && state.mode == mode => Ok(()),
        _ => Err(WorkspaceMutationError::Conflict),
    }
}

fn map_io(error: MutationIoError) -> WorkspaceMutationError {
    match error {
        MutationIoError::Unsafe => WorkspaceMutationError::Unsafe,
        MutationIoError::Conflict => WorkspaceMutationError::Conflict,
        MutationIoError::Unsupported => WorkspaceMutationError::Unsupported,
        MutationIoError::TooLarge => WorkspaceMutationError::TooLarge,
        MutationIoError::Io => WorkspaceMutationError::Io,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rehow-workspace-mutation-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn capability(&self) -> DirectoryCapability {
            DirectoryCapability::open_root(&self.0).unwrap()
        }

        fn category(&self) -> PathBuf {
            let category = self.0.join("categories/linux");
            fs::create_dir_all(&category).unwrap();
            category
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn create_and_exchange_preserve_verified_displaced_bytes() {
        let root = TestDirectory::new("install");
        let category = root.category();
        let capability = root.capability();
        let relative = "categories/linux/guide.md";
        assert!(matches!(
            install_object(
                &capability,
                relative,
                None,
                b"first",
                0o640,
                ".rehow-tmp-create"
            )
            .unwrap(),
            InstallOutcome::Created
        ));
        assert_eq!(fs::read(category.join("guide.md")).unwrap(), b"first");
        assert_eq!(
            fs::metadata(category.join("guide.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o640
        );

        let InstallOutcome::Replaced(displaced) = install_object(
            &capability,
            relative,
            Some((b"first", 0o640)),
            b"second",
            0o600,
            ".rehow-tmp-update",
        )
        .unwrap() else {
            panic!("expected replacement");
        };
        displaced.verify().unwrap();
        assert_eq!(fs::read(category.join("guide.md")).unwrap(), b"second");
        assert_eq!(
            fs::read(category.join(".rehow-tmp-update")).unwrap(),
            b"first"
        );
        assert_eq!(
            install_object(
                &capability,
                relative,
                Some((b"stale", 0o600)),
                b"third",
                0o600,
                ".rehow-tmp-stale"
            )
            .err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert_eq!(fs::read(category.join("guide.md")).unwrap(), b"second");
    }

    #[test]
    fn unsafe_targets_and_conflicting_temps_fail_closed() {
        let root = TestDirectory::new("unsafe");
        let category = root.category();
        let outside = root.0.join("outside");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, category.join("guide.md")).unwrap();
        let capability = root.capability();
        assert_eq!(
            install_object(
                &capability,
                "categories/linux/guide.md",
                Some((b"outside", 0o600)),
                b"new",
                0o600,
                ".rehow-tmp-link"
            )
            .err(),
            Some(WorkspaceMutationError::Unsafe)
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside");

        fs::remove_file(category.join("guide.md")).unwrap();
        fs::write(category.join(".rehow-tmp-collision"), b"hostile").unwrap();
        assert_eq!(
            install_object(
                &capability,
                "categories/linux/guide.md",
                None,
                b"new",
                0o600,
                ".rehow-tmp-collision"
            )
            .err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert!(!category.join("guide.md").exists());
        assert_eq!(
            fs::read(category.join(".rehow-tmp-collision")).unwrap(),
            b"hostile"
        );

        fs::write(category.join("guide.md"), b"linked").unwrap();
        fs::hard_link(
            category.join("guide.md"),
            category.join("guide-hardlink.md"),
        )
        .unwrap();
        assert_eq!(
            install_object(
                &capability,
                "categories/linux/guide.md",
                Some((b"linked", 0o600)),
                b"new",
                0o600,
                ".rehow-tmp-hardlink"
            )
            .err(),
            Some(WorkspaceMutationError::Unsafe)
        );
    }

    #[test]
    fn each_pre_sync_aftermath_remains_classifiable() {
        let root = TestDirectory::new("aftermath");
        let category = root.category();
        fs::write(category.join("guide.md"), b"old").unwrap();
        fs::set_permissions(category.join("guide.md"), fs::Permissions::from_mode(0o600)).unwrap();
        let capability = root.capability();
        let target = capability
            .open_managed_target("categories/linux/guide.md")
            .unwrap();
        target
            .write_temp(".rehow-tmp-aftermath", b"new", 0o600)
            .unwrap();
        assert!(
            matches!(target.state(16).unwrap(), ManagedState::Regular(state) if state.bytes == b"old")
        );
        assert!(
            matches!(target.temp_state(".rehow-tmp-aftermath", 16).unwrap(), ManagedState::Regular(state) if state.bytes == b"new")
        );

        target.exchange(".rehow-tmp-aftermath").unwrap();
        assert!(
            matches!(target.state(16).unwrap(), ManagedState::Regular(state) if state.bytes == b"new")
        );
        assert!(
            matches!(target.temp_state(".rehow-tmp-aftermath", 16).unwrap(), ManagedState::Regular(state) if state.bytes == b"old")
        );
        target.sync_directory().unwrap();
    }
}

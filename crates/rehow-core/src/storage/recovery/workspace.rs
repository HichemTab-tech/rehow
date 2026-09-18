use super::{
    ExpectedState, IntendedState, Journal, JournalStage, TargetStep,
    store::{RecoveryStore, StoreError, WriterLock},
    valid_temp,
};
use crate::protocol::OperationId;
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
    let current = target.state(TARGET_LIMIT).map_err(map_io)?;
    let temp = target.temp_state(temp_name, TARGET_LIMIT).map_err(map_io)?;

    if state_matches(&current, Some((intended, mode))) {
        match (&temp, expected) {
            (ManagedState::Missing, None) => {
                target.sync_directory().map_err(map_io)?;
                verify(&target, intended, mode)?;
                return Ok(InstallOutcome::Created);
            }
            (ManagedState::Regular(displaced), Some((expected, expected_mode)))
                if displaced.bytes == expected && displaced.mode == expected_mode =>
            {
                target.sync_directory().map_err(map_io)?;
                verify(&target, intended, mode)?;
                return Ok(InstallOutcome::Replaced(DisplacedTarget {
                    target,
                    temp_name: temp_name.to_owned(),
                    prior: RegularState {
                        bytes: expected.to_vec(),
                        mode: expected_mode,
                    },
                }));
            }
            _ => return Err(WorkspaceMutationError::Conflict),
        }
    }

    if !state_matches(&current, expected) {
        return Err(WorkspaceMutationError::Conflict);
    }
    match temp {
        ManagedState::Missing => target
            .write_temp(temp_name, intended, mode)
            .map_err(map_io)?,
        ManagedState::Regular(existing) if existing.bytes == intended && existing.mode == mode => {}
        ManagedState::Regular(_) => return Err(WorkspaceMutationError::Conflict),
    }

    match current {
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

pub(super) fn execute_new_journal(
    root: &DirectoryCapability,
    store: &RecoveryStore,
    lock: &WriterLock,
    journal: Journal,
) -> Result<Journal, WorkspaceMutationError> {
    preflight(root, store, lock, &journal)?;
    if journal.generation != 1
        || journal.stage != JournalStage::Prepared
        || journal.completion
        || journal
            .targets
            .iter()
            .any(|target| target.step != TargetStep::Prepared)
    {
        return Err(WorkspaceMutationError::Conflict);
    }
    store.install_journal(lock, &journal).map_err(map_store)?;
    continue_journal(root, store, lock, journal)
}

pub(super) fn resume_journal(
    root: &DirectoryCapability,
    store: &RecoveryStore,
    lock: &WriterLock,
    operation_id: OperationId,
) -> Result<Journal, WorkspaceMutationError> {
    let journal = store.read_journal(lock, operation_id).map_err(map_store)?;
    preflight(root, store, lock, &journal)?;
    continue_journal(root, store, lock, journal)
}

fn preflight(
    root: &DirectoryCapability,
    store: &RecoveryStore,
    lock: &WriterLock,
    journal: &Journal,
) -> Result<(), WorkspaceMutationError> {
    if root.identity().map_err(map_io)? != (journal.root_device, journal.root_inode) {
        return Err(WorkspaceMutationError::Conflict);
    }
    for target in &journal.targets {
        if !matches!(target.intended, IntendedState::Object { .. }) {
            return Err(WorkspaceMutationError::Unsupported);
        }
        if let ExpectedState::Existing { object_hash, .. } = &target.expected {
            store.read_object(lock, object_hash).map_err(map_store)?;
        }
        if let IntendedState::Object { object_hash, .. } = &target.intended {
            store.read_object(lock, object_hash).map_err(map_store)?;
        }
    }
    Ok(())
}

fn continue_journal(
    root: &DirectoryCapability,
    store: &RecoveryStore,
    lock: &WriterLock,
    mut journal: Journal,
) -> Result<Journal, WorkspaceMutationError> {
    if journal.stage == JournalStage::Committed && journal.completion {
        return Ok(journal);
    }
    if journal.stage == JournalStage::Conflict {
        return Err(WorkspaceMutationError::Conflict);
    }
    if journal.stage == JournalStage::Prepared {
        journal.stage = JournalStage::Applying;
        advance(store, lock, &mut journal)?;
    }

    for index in 0..journal.targets.len() {
        if journal.targets[index].step == TargetStep::Complete {
            continue;
        }
        let target = journal.targets[index].clone();
        let expected_object = match &target.expected {
            ExpectedState::Absent => None,
            ExpectedState::Existing {
                revision,
                object_hash,
                mode,
            } => {
                let bytes = store.read_object(lock, object_hash).map_err(map_store)?;
                if super::super::document_revision(&target.relative_path, &bytes) != *revision {
                    return mark_conflict(store, lock, &mut journal);
                }
                Some((bytes, *mode))
            }
        };
        let IntendedState::Object { object_hash, mode } = target.intended else {
            return Err(WorkspaceMutationError::Unsupported);
        };
        let intended = store.read_object(lock, &object_hash).map_err(map_store)?;
        let expected = expected_object
            .as_ref()
            .map(|(bytes, mode)| (bytes.as_slice(), *mode));

        let outcome = match install_object(
            root,
            &target.relative_path,
            expected,
            &intended,
            mode,
            &target.temp_name,
        ) {
            Ok(outcome) => outcome,
            Err(WorkspaceMutationError::Conflict | WorkspaceMutationError::Unsafe) => {
                return mark_conflict(store, lock, &mut journal);
            }
            Err(error) => return Err(error),
        };
        if let InstallOutcome::Replaced(displaced) = outcome {
            displaced.verify()?;
        }
        if journal.targets[index].step == TargetStep::Prepared {
            journal.targets[index].step = TargetStep::Swapped;
            advance(store, lock, &mut journal)?;
        }
        if journal.targets[index].step == TargetStep::Swapped {
            journal.targets[index].step = TargetStep::DirectorySynced;
            advance(store, lock, &mut journal)?;
        }
        journal.targets[index].step = TargetStep::Complete;
        advance(store, lock, &mut journal)?;
    }
    journal.stage = JournalStage::Committed;
    journal.completion = true;
    advance(store, lock, &mut journal)?;
    Ok(journal)
}

fn advance(
    store: &RecoveryStore,
    lock: &WriterLock,
    journal: &mut Journal,
) -> Result<(), WorkspaceMutationError> {
    journal.generation = journal
        .generation
        .checked_add(1)
        .ok_or(WorkspaceMutationError::Conflict)?;
    store.install_journal(lock, journal).map_err(map_store)
}

fn mark_conflict(
    store: &RecoveryStore,
    lock: &WriterLock,
    journal: &mut Journal,
) -> Result<Journal, WorkspaceMutationError> {
    journal.stage = JournalStage::Conflict;
    advance(store, lock, journal)?;
    Err(WorkspaceMutationError::Conflict)
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

fn state_matches(state: &ManagedState, expected: Option<(&[u8], u32)>) -> bool {
    match (state, expected) {
        (ManagedState::Missing, None) => true,
        (ManagedState::Regular(state), Some((bytes, mode))) => {
            state.bytes == bytes && state.mode == mode
        }
        _ => false,
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

fn map_store(error: StoreError) -> WorkspaceMutationError {
    match error {
        StoreError::Unsafe => WorkspaceMutationError::Unsafe,
        StoreError::Busy | StoreError::Conflict | StoreError::Corrupt => {
            WorkspaceMutationError::Conflict
        }
        StoreError::Unsupported => WorkspaceMutationError::Unsupported,
        StoreError::TooLarge => WorkspaceMutationError::TooLarge,
        StoreError::Io => WorkspaceMutationError::Io,
        #[cfg(test)]
        StoreError::Injected => WorkspaceMutationError::Io,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::recovery::{OperationKind, Target};
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

    fn update_journal(
        root: &DirectoryCapability,
        store: &RecoveryStore,
        lock: &WriterLock,
        old: &[u8],
        new: &[u8],
    ) -> Journal {
        let old_hash = store.store_object(lock, old).unwrap();
        let new_hash = store.store_object(lock, new).unwrap();
        let (root_device, root_inode) = root.identity().unwrap();
        Journal {
            generation: 1,
            operation_id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".parse().unwrap(),
            operation_kind: OperationKind::Update,
            workspace_id: "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            root_hash: [0x11; 32],
            root_device,
            root_inode,
            created_at: "2026-09-18T00:00:00Z".parse().unwrap(),
            plan_hash: [0x33; 32],
            targets: vec![Target {
                relative_path: "categories/linux/guide.md".to_owned(),
                expected: ExpectedState::Existing {
                    revision: crate::storage::document_revision("categories/linux/guide.md", old),
                    object_hash: old_hash,
                    mode: 0o600,
                },
                intended: IntendedState::Object {
                    object_hash: new_hash,
                    mode: 0o600,
                },
                temp_name: ".rehow-tmp-journal".to_owned(),
                step: TargetStep::Prepared,
            }],
            stage: JournalStage::Prepared,
            completion: false,
        }
    }

    fn journal_fixture(label: &str) -> (TestDirectory, TestDirectory, DirectoryCapability) {
        let workspace = TestDirectory::new(label);
        let category = workspace.category();
        fs::write(category.join("guide.md"), b"old").unwrap();
        fs::set_permissions(category.join("guide.md"), fs::Permissions::from_mode(0o600)).unwrap();
        let capability = workspace.capability();
        (workspace, TestDirectory::new("state"), capability)
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

    #[test]
    fn journal_executor_commits_and_replays_a_post_exchange_crash() {
        let (workspace, state, capability) = journal_fixture("journal");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let journal = update_journal(&capability, &store, &lock, b"old", b"new");
        let committed = execute_new_journal(&capability, &store, &lock, journal).unwrap();
        assert_eq!(committed.stage, JournalStage::Committed);
        assert!(committed.completion);
        assert_eq!(committed.targets[0].step, TargetStep::Complete);
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"new"
        );
        assert!(store.read_journal(&lock, committed.operation_id).unwrap() == committed);

        let (workspace, state, capability) = journal_fixture("resume");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let journal = update_journal(&capability, &store, &lock, b"old", b"new");
        store.install_journal(&lock, &journal).unwrap();
        let InstallOutcome::Replaced(displaced) = install_object(
            &capability,
            "categories/linux/guide.md",
            Some((b"old", 0o600)),
            b"new",
            0o600,
            ".rehow-tmp-journal",
        )
        .unwrap() else {
            panic!("expected replacement");
        };
        displaced.verify().unwrap();
        let committed = resume_journal(&capability, &store, &lock, journal.operation_id).unwrap();
        assert_eq!(committed.stage, JournalStage::Committed);
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"new"
        );
    }

    #[test]
    fn journal_executor_records_external_conflict_without_overwrite() {
        let (workspace, state, capability) = journal_fixture("conflict");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let journal = update_journal(&capability, &store, &lock, b"old", b"new");
        store.install_journal(&lock, &journal).unwrap();
        fs::write(workspace.0.join("categories/linux/guide.md"), b"external").unwrap();
        assert_eq!(
            resume_journal(&capability, &store, &lock, journal.operation_id).err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"external"
        );
        assert_eq!(
            store
                .read_journal(&lock, journal.operation_id)
                .unwrap()
                .stage,
            JournalStage::Conflict
        );
    }
}

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

#[cfg(test)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum WorkspaceFault {
    BeforeLiveRename,
    AfterLiveRenameBeforeSync,
    AfterSourceDirectorySync,
}

#[cfg(test)]
thread_local! {
    static WORKSPACE_FAULT: std::cell::Cell<Option<WorkspaceFault>> = const { std::cell::Cell::new(None) };
}

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
    target.sync_directory().map_err(map_io)?;
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
        ManagedState::Regular(existing) if existing.bytes == intended && existing.mode == mode => {
            target
                .sync_existing_temp(temp_name, intended, mode)
                .map_err(map_io)?;
        }
        ManagedState::Regular(_) => return Err(WorkspaceMutationError::Conflict),
    }
    target.sync_directory().map_err(map_io)?;

    match current {
        ManagedState::Missing => {
            fail(WorkspaceFaultPoint::BeforeLiveRename)?;
            target.install_no_replace(temp_name).map_err(map_io)?;
            fail(WorkspaceFaultPoint::AfterLiveRenameBeforeSync)?;
            verify(&target, intended, mode)?;
            target.sync_directory().map_err(map_io)?;
            verify(&target, intended, mode)?;
            Ok(InstallOutcome::Created)
        }
        ManagedState::Regular(prior) => {
            fail(WorkspaceFaultPoint::BeforeLiveRename)?;
            target.exchange(temp_name).map_err(map_io)?;
            fail(WorkspaceFaultPoint::AfterLiveRenameBeforeSync)?;
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

pub(super) fn remove_object(
    root: &DirectoryCapability,
    relative_path: &str,
    expected: &[u8],
    mode: u32,
    temp_name: &str,
) -> Result<(), WorkspaceMutationError> {
    if !valid_temp(temp_name) {
        return Err(WorkspaceMutationError::Unsafe);
    }
    let target = root.open_managed_target(relative_path).map_err(map_io)?;
    target.sync_directory().map_err(map_io)?;
    let current = target.state(TARGET_LIMIT).map_err(map_io)?;
    let temp = target.temp_state(temp_name, TARGET_LIMIT).map_err(map_io)?;
    match (&current, &temp) {
        (ManagedState::Missing, ManagedState::Regular(displaced))
            if displaced.bytes == expected && displaced.mode == mode =>
        {
            target.sync_directory().map_err(map_io)?;
            return Ok(());
        }
        (ManagedState::Regular(active), ManagedState::Missing)
            if active.bytes == expected && active.mode == mode => {}
        _ => return Err(WorkspaceMutationError::Conflict),
    }
    fail(WorkspaceFaultPoint::BeforeLiveRename)?;
    target.move_to_temp(temp_name).map_err(map_io)?;
    fail(WorkspaceFaultPoint::AfterLiveRenameBeforeSync)?;
    if !matches!(
        target.state(TARGET_LIMIT).map_err(map_io)?,
        ManagedState::Missing
    ) || !matches!(
        target.temp_state(temp_name, TARGET_LIMIT).map_err(map_io)?,
        ManagedState::Regular(ref displaced)
            if displaced.bytes == expected && displaced.mode == mode
    ) {
        return Err(WorkspaceMutationError::Conflict);
    }
    target.sync_directory().map_err(map_io)
}

pub(super) fn trash_object(
    root: &DirectoryCapability,
    relative_path: &str,
    expected: &[u8],
    mode: u32,
    destination: &str,
) -> Result<(), WorkspaceMutationError> {
    let target = root.open_managed_target(relative_path).map_err(map_io)?;
    target.sync_directory().map_err(map_io)?;
    let current = target.state(TARGET_LIMIT).map_err(map_io)?;
    match &current {
        ManagedState::Missing => {
            let trash = root.open_trash_target(destination).map_err(map_io)?;
            if !matches!(
                trash.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Regular(ref saved)
                    if saved.bytes == expected && saved.mode == mode
            ) {
                return Err(WorkspaceMutationError::Conflict);
            }
            target.sync_directory().map_err(map_io)?;
            trash.sync_directory().map_err(map_io)?;
            return Ok(());
        }
        ManagedState::Regular(active) if active.bytes == expected && active.mode == mode => {}
        _ => return Err(WorkspaceMutationError::Conflict),
    }
    let trash = root.prepare_trash_target(destination).map_err(map_io)?;
    trash.sync_directory().map_err(map_io)?;
    if !matches!(
        trash.state(TARGET_LIMIT).map_err(map_io)?,
        ManagedState::Missing
    ) {
        return Err(WorkspaceMutationError::Conflict);
    }
    fail(WorkspaceFaultPoint::BeforeLiveRename)?;
    target.move_to(&trash).map_err(map_io)?;
    fail(WorkspaceFaultPoint::AfterLiveRenameBeforeSync)?;
    if !matches!(
        target.state(TARGET_LIMIT).map_err(map_io)?,
        ManagedState::Missing
    ) || !matches!(
        trash.state(TARGET_LIMIT).map_err(map_io)?,
        ManagedState::Regular(ref saved) if saved.bytes == expected && saved.mode == mode
    ) {
        return Err(WorkspaceMutationError::Conflict);
    }
    target.sync_directory().map_err(map_io)?;
    fail(WorkspaceFaultPoint::AfterSourceDirectorySync)?;
    trash.sync_directory().map_err(map_io)
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
        if let ExpectedState::Existing {
            revision,
            object_hash,
            ..
        } = &target.expected
        {
            let old = store.read_object(lock, object_hash).map_err(map_store)?;
            if super::super::document_revision(&target.relative_path, &old) != *revision {
                return Err(WorkspaceMutationError::Conflict);
            }
        }
        if let IntendedState::Object { object_hash, .. }
        | IntendedState::Trash { object_hash, .. } = &target.intended
        {
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
        verify_journal_applied(root, store, lock, &journal)?;
        cleanup_committed(root, store, lock, &journal)?;
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
        let expected = expected_object
            .as_ref()
            .map(|(bytes, mode)| (bytes.as_slice(), *mode));
        let applied = if target.step == TargetStep::Prepared {
            apply_prepared_target(root, store, lock, &target, expected)
        } else {
            verify_applied_target(root, store, lock, journal.operation_id, &target, expected)
        };
        match applied {
            Ok(()) => {}
            Err(WorkspaceMutationError::Conflict | WorkspaceMutationError::Unsafe) => {
                return mark_conflict(store, lock, &mut journal);
            }
            Err(error) => return Err(error),
        }
        if journal.targets[index].step == TargetStep::Complete {
            continue;
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
    verify_journal_applied(root, store, lock, &journal)?;
    cleanup_committed(root, store, lock, &journal)?;
    Ok(journal)
}

fn apply_prepared_target(
    root: &DirectoryCapability,
    store: &RecoveryStore,
    lock: &WriterLock,
    target: &super::Target,
    expected: Option<(&[u8], u32)>,
) -> Result<(), WorkspaceMutationError> {
    match &target.intended {
        IntendedState::Object { object_hash, mode } => {
            let intended = store.read_object(lock, object_hash).map_err(map_store)?;
            install_object(
                root,
                &target.relative_path,
                expected,
                &intended,
                *mode,
                &target.temp_name,
            )
            .and_then(|outcome| match outcome {
                InstallOutcome::Created => Ok(()),
                InstallOutcome::Replaced(displaced) => displaced.verify(),
            })
        }
        IntendedState::Absent => {
            let Some((expected, mode)) = expected else {
                return Err(WorkspaceMutationError::Conflict);
            };
            remove_object(
                root,
                &target.relative_path,
                expected,
                mode,
                &target.temp_name,
            )
        }
        IntendedState::Trash {
            object_hash,
            mode,
            destination,
        } => {
            let Some((expected, expected_mode)) = expected else {
                return Err(WorkspaceMutationError::Conflict);
            };
            if *object_hash != *blake3::hash(expected).as_bytes() || *mode != expected_mode {
                return Err(WorkspaceMutationError::Conflict);
            }
            trash_object(root, &target.relative_path, expected, *mode, destination)
        }
    }
}

fn verify_journal_applied(
    root: &DirectoryCapability,
    store: &RecoveryStore,
    lock: &WriterLock,
    journal: &Journal,
) -> Result<(), WorkspaceMutationError> {
    for target in &journal.targets {
        let expected_object = match &target.expected {
            ExpectedState::Absent => None,
            ExpectedState::Existing {
                revision,
                object_hash,
                mode,
            } => {
                let bytes = store.read_object(lock, object_hash).map_err(map_store)?;
                if super::super::document_revision(&target.relative_path, &bytes) != *revision {
                    return Err(WorkspaceMutationError::Conflict);
                }
                Some((bytes, *mode))
            }
        };
        verify_applied_target(
            root,
            store,
            lock,
            journal.operation_id,
            target,
            expected_object
                .as_ref()
                .map(|(bytes, mode)| (bytes.as_slice(), *mode)),
        )?;
    }
    Ok(())
}

fn verify_applied_target(
    root: &DirectoryCapability,
    store: &RecoveryStore,
    lock: &WriterLock,
    operation_id: OperationId,
    target: &super::Target,
    expected: Option<(&[u8], u32)>,
) -> Result<(), WorkspaceMutationError> {
    let active = root
        .open_managed_target(&target.relative_path)
        .map_err(map_io)?;
    active.sync_directory().map_err(map_io)?;
    match &target.intended {
        IntendedState::Object { object_hash, mode } => {
            let intended = store.read_object(lock, object_hash).map_err(map_store)?;
            if !matches!(
                active.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Regular(ref state)
                    if state.bytes == intended && state.mode == *mode
            ) {
                return Err(WorkspaceMutationError::Conflict);
            }
            match expected {
                None => {
                    if !matches!(
                        active
                            .temp_state(&target.temp_name, TARGET_LIMIT)
                            .map_err(map_io)?,
                        ManagedState::Missing
                    ) {
                        return Err(WorkspaceMutationError::Conflict);
                    }
                }
                Some((old, old_mode)) => {
                    verify_before_image(root, &active, operation_id, target, old, old_mode)?;
                }
            }
        }
        IntendedState::Absent => {
            let Some((old, old_mode)) = expected else {
                return Err(WorkspaceMutationError::Conflict);
            };
            if !matches!(
                active.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Missing
            ) {
                return Err(WorkspaceMutationError::Conflict);
            }
            verify_before_image(root, &active, operation_id, target, old, old_mode)?;
        }
        IntendedState::Trash {
            destination,
            object_hash,
            mode,
        } => {
            let Some((old, old_mode)) = expected else {
                return Err(WorkspaceMutationError::Conflict);
            };
            if *object_hash != *blake3::hash(old).as_bytes() || *mode != old_mode {
                return Err(WorkspaceMutationError::Conflict);
            }
            let trash = root.open_trash_target(destination).map_err(map_io)?;
            trash.sync_directory().map_err(map_io)?;
            if !matches!(
                active.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Missing
            ) || !matches!(
                trash.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Regular(ref state)
                    if state.bytes == old && state.mode == old_mode
            ) {
                return Err(WorkspaceMutationError::Conflict);
            }
        }
    }
    Ok(())
}

fn verify_before_image(
    root: &DirectoryCapability,
    active: &ManagedTarget,
    operation_id: OperationId,
    target: &super::Target,
    expected: &[u8],
    mode: u32,
) -> Result<(), WorkspaceMutationError> {
    match active
        .temp_state(&target.temp_name, TARGET_LIMIT)
        .map_err(map_io)?
    {
        ManagedState::Regular(state) if state.bytes == expected && state.mode == mode => {
            active.sync_directory().map_err(map_io)
        }
        ManagedState::Regular(_) => Err(WorkspaceMutationError::Conflict),
        ManagedState::Missing => {
            let destination = quarantine_destination(operation_id, &target.relative_path);
            let trash = root.open_trash_target(&destination).map_err(map_io)?;
            if !matches!(
                trash.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Regular(ref state)
                    if state.bytes == expected && state.mode == mode
            ) {
                return Err(WorkspaceMutationError::Conflict);
            }
            active.sync_directory().map_err(map_io)?;
            fail(WorkspaceFaultPoint::AfterSourceDirectorySync)?;
            trash.sync_directory().map_err(map_io)
        }
    }
}

fn quarantine_destination(operation_id: OperationId, relative_path: &str) -> String {
    format!(".rehow/trash/{operation_id}/{relative_path}")
}

fn quarantine_before_image(
    root: &DirectoryCapability,
    active: &ManagedTarget,
    operation_id: OperationId,
    target: &super::Target,
    expected: &[u8],
    mode: u32,
) -> Result<(), WorkspaceMutationError> {
    let destination = quarantine_destination(operation_id, &target.relative_path);
    match active
        .temp_state(&target.temp_name, TARGET_LIMIT)
        .map_err(map_io)?
    {
        ManagedState::Regular(state) if state.bytes == expected && state.mode == mode => {
            active.sync_directory().map_err(map_io)?;
            let trash = root.prepare_trash_target(&destination).map_err(map_io)?;
            trash.sync_directory().map_err(map_io)?;
            if !matches!(
                trash.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Missing
            ) {
                return Err(WorkspaceMutationError::Conflict);
            }
            fail(WorkspaceFaultPoint::BeforeLiveRename)?;
            active
                .move_temp_to(&target.temp_name, &trash)
                .map_err(map_io)?;
            fail(WorkspaceFaultPoint::AfterLiveRenameBeforeSync)?;
            if !matches!(
                active
                    .temp_state(&target.temp_name, TARGET_LIMIT)
                    .map_err(map_io)?,
                ManagedState::Missing
            ) || !matches!(
                trash.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Regular(ref saved)
                    if saved.bytes == expected && saved.mode == mode
            ) {
                return Err(WorkspaceMutationError::Conflict);
            }
            active.sync_directory().map_err(map_io)?;
            trash.sync_directory().map_err(map_io)
        }
        ManagedState::Regular(_) => Err(WorkspaceMutationError::Conflict),
        ManagedState::Missing => {
            let trash = root.open_trash_target(&destination).map_err(map_io)?;
            if !matches!(
                trash.state(TARGET_LIMIT).map_err(map_io)?,
                ManagedState::Regular(ref saved)
                    if saved.bytes == expected && saved.mode == mode
            ) {
                return Err(WorkspaceMutationError::Conflict);
            }
            active.sync_directory().map_err(map_io)?;
            trash.sync_directory().map_err(map_io)
        }
    }
}

fn cleanup_committed(
    root: &DirectoryCapability,
    store: &RecoveryStore,
    lock: &WriterLock,
    journal: &Journal,
) -> Result<(), WorkspaceMutationError> {
    if journal.stage != JournalStage::Committed || !journal.completion {
        return Err(WorkspaceMutationError::Conflict);
    }
    for target in &journal.targets {
        let ExpectedState::Existing {
            object_hash, mode, ..
        } = &target.expected
        else {
            continue;
        };
        let old = store.read_object(lock, object_hash).map_err(map_store)?;
        match &target.intended {
            IntendedState::Object {
                object_hash: new_hash,
                mode: new_mode,
            } => {
                let active = root
                    .open_managed_target(&target.relative_path)
                    .map_err(map_io)?;
                let new = store.read_object(lock, new_hash).map_err(map_store)?;
                if !matches!(
                    active.state(TARGET_LIMIT).map_err(map_io)?,
                    ManagedState::Regular(ref state)
                        if state.bytes == new && state.mode == *new_mode
                ) {
                    return Err(WorkspaceMutationError::Conflict);
                }
                quarantine_before_image(root, &active, journal.operation_id, target, &old, *mode)?;
            }
            IntendedState::Absent => {
                let active = root
                    .open_managed_target(&target.relative_path)
                    .map_err(map_io)?;
                if !matches!(
                    active.state(TARGET_LIMIT).map_err(map_io)?,
                    ManagedState::Missing
                ) {
                    return Err(WorkspaceMutationError::Conflict);
                }
                quarantine_before_image(root, &active, journal.operation_id, target, &old, *mode)?;
            }
            IntendedState::Trash {
                destination,
                object_hash: intended_hash,
                mode: intended_mode,
            } => {
                if intended_hash != object_hash || intended_mode != mode {
                    return Err(WorkspaceMutationError::Conflict);
                }
                let active = root
                    .open_managed_target(&target.relative_path)
                    .map_err(map_io)?;
                let trash = root.open_trash_target(destination).map_err(map_io)?;
                if !matches!(
                    active.state(TARGET_LIMIT).map_err(map_io)?,
                    ManagedState::Missing
                ) || !matches!(
                    trash.state(TARGET_LIMIT).map_err(map_io)?,
                    ManagedState::Regular(ref saved)
                        if saved.bytes == old && saved.mode == *mode
                ) {
                    return Err(WorkspaceMutationError::Conflict);
                }
            }
        }
    }
    Ok(())
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum WorkspaceFaultPoint {
    BeforeLiveRename,
    AfterLiveRenameBeforeSync,
    AfterSourceDirectorySync,
}

#[cfg(test)]
fn fail(point: WorkspaceFaultPoint) -> Result<(), WorkspaceMutationError> {
    let wanted = match point {
        WorkspaceFaultPoint::BeforeLiveRename => WorkspaceFault::BeforeLiveRename,
        WorkspaceFaultPoint::AfterLiveRenameBeforeSync => WorkspaceFault::AfterLiveRenameBeforeSync,
        WorkspaceFaultPoint::AfterSourceDirectorySync => WorkspaceFault::AfterSourceDirectorySync,
    };
    if WORKSPACE_FAULT.get() == Some(wanted) {
        WORKSPACE_FAULT.set(None);
        Err(WorkspaceMutationError::Unsupported)
    } else {
        Ok(())
    }
}

#[cfg(not(test))]
fn fail(_point: WorkspaceFaultPoint) -> Result<(), WorkspaceMutationError> {
    Ok(())
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

    fn delete_journal(
        root: &DirectoryCapability,
        store: &RecoveryStore,
        lock: &WriterLock,
        trash: bool,
    ) -> Journal {
        let old = b"old";
        let old_hash = store.store_object(lock, old).unwrap();
        let (root_device, root_inode) = root.identity().unwrap();
        let operation_id: OperationId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".parse().unwrap();
        Journal {
            generation: 1,
            operation_id,
            operation_kind: OperationKind::Delete,
            workspace_id: "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            root_hash: [0x11; 32],
            root_device,
            root_inode,
            created_at: "2026-09-18T00:00:00Z".parse().unwrap(),
            plan_hash: [0x34; 32],
            targets: vec![Target {
                relative_path: "categories/linux/guide.md".to_owned(),
                expected: ExpectedState::Existing {
                    revision: crate::storage::document_revision("categories/linux/guide.md", old),
                    object_hash: old_hash,
                    mode: 0o600,
                },
                intended: if trash {
                    IntendedState::Trash {
                        object_hash: old_hash,
                        mode: 0o600,
                        destination: format!(
                            ".rehow/trash/{operation_id}/categories/linux/guide.md"
                        ),
                    }
                } else {
                    IntendedState::Absent
                },
                temp_name: ".rehow-tmp-delete".to_owned(),
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

    fn private_parents(path: &std::path::Path, root: &std::path::Path) {
        fs::create_dir_all(path).unwrap();
        let mut ancestor = Some(path);
        while let Some(directory) = ancestor {
            if directory == root {
                break;
            }
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
            ancestor = directory.parent();
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

    #[test]
    fn unsupported_before_rename_preserves_old_and_post_rename_failure_resumes() {
        for (label, fault) in [
            (
                "temp-write-fault",
                crate::storage::linux::ManagedFault::TempAfterWrite,
            ),
            (
                "temp-sync-fault",
                crate::storage::linux::ManagedFault::TempAfterFileSync,
            ),
        ] {
            let root = TestDirectory::new(label);
            let category = root.category();
            fs::write(category.join("guide.md"), b"old").unwrap();
            fs::set_permissions(category.join("guide.md"), fs::Permissions::from_mode(0o600))
                .unwrap();
            let capability = root.capability();
            crate::storage::linux::set_managed_fault(fault);
            assert_eq!(
                install_object(
                    &capability,
                    "categories/linux/guide.md",
                    Some((b"old", 0o600)),
                    b"new",
                    0o600,
                    ".rehow-tmp-fault"
                )
                .err(),
                Some(WorkspaceMutationError::Unsupported)
            );
            assert_eq!(fs::read(category.join("guide.md")).unwrap(), b"old");
            assert_eq!(fs::read(category.join(".rehow-tmp-fault")).unwrap(), b"new");
            crate::storage::linux::set_managed_fault(
                crate::storage::linux::ManagedFault::ReusedTempAfterFileSync,
            );
            assert_eq!(
                install_object(
                    &capability,
                    "categories/linux/guide.md",
                    Some((b"old", 0o600)),
                    b"new",
                    0o600,
                    ".rehow-tmp-fault"
                )
                .err(),
                Some(WorkspaceMutationError::Unsupported)
            );
            assert_eq!(fs::read(category.join("guide.md")).unwrap(), b"old");
            let InstallOutcome::Replaced(displaced) = install_object(
                &capability,
                "categories/linux/guide.md",
                Some((b"old", 0o600)),
                b"new",
                0o600,
                ".rehow-tmp-fault",
            )
            .unwrap() else {
                panic!("expected replacement");
            };
            displaced.verify().unwrap();
        }

        let before = TestDirectory::new("before-rename-fault");
        let category = before.category();
        fs::write(category.join("guide.md"), b"old").unwrap();
        fs::set_permissions(category.join("guide.md"), fs::Permissions::from_mode(0o600)).unwrap();
        let capability = before.capability();
        WORKSPACE_FAULT.set(Some(WorkspaceFault::BeforeLiveRename));
        assert_eq!(
            install_object(
                &capability,
                "categories/linux/guide.md",
                Some((b"old", 0o600)),
                b"new",
                0o600,
                ".rehow-tmp-fault"
            )
            .err(),
            Some(WorkspaceMutationError::Unsupported)
        );
        assert_eq!(fs::read(category.join("guide.md")).unwrap(), b"old");
        assert_eq!(fs::read(category.join(".rehow-tmp-fault")).unwrap(), b"new");

        let after = TestDirectory::new("after-rename-fault");
        let category = after.category();
        fs::write(category.join("guide.md"), b"old").unwrap();
        fs::set_permissions(category.join("guide.md"), fs::Permissions::from_mode(0o600)).unwrap();
        let capability = after.capability();
        WORKSPACE_FAULT.set(Some(WorkspaceFault::AfterLiveRenameBeforeSync));
        assert_eq!(
            install_object(
                &capability,
                "categories/linux/guide.md",
                Some((b"old", 0o600)),
                b"new",
                0o600,
                ".rehow-tmp-fault"
            )
            .err(),
            Some(WorkspaceMutationError::Unsupported)
        );
        assert_eq!(fs::read(category.join("guide.md")).unwrap(), b"new");
        assert_eq!(fs::read(category.join(".rehow-tmp-fault")).unwrap(), b"old");
        let InstallOutcome::Replaced(displaced) = install_object(
            &capability,
            "categories/linux/guide.md",
            Some((b"old", 0o600)),
            b"new",
            0o600,
            ".rehow-tmp-fault",
        )
        .unwrap() else {
            panic!("expected resumed replacement");
        };
        displaced.verify().unwrap();
    }

    #[test]
    fn create_delete_and_trash_relocate_idempotently_across_faults() {
        for (label, fault) in [
            ("create-before", WorkspaceFault::BeforeLiveRename),
            ("create-after", WorkspaceFault::AfterLiveRenameBeforeSync),
        ] {
            let root = TestDirectory::new(label);
            let category = root.category();
            let capability = root.capability();
            WORKSPACE_FAULT.set(Some(fault));
            assert_eq!(
                install_object(
                    &capability,
                    "categories/linux/guide.md",
                    None,
                    b"new",
                    0o600,
                    ".rehow-tmp-create"
                )
                .err(),
                Some(WorkspaceMutationError::Unsupported)
            );
            assert!(matches!(
                install_object(
                    &capability,
                    "categories/linux/guide.md",
                    None,
                    b"new",
                    0o600,
                    ".rehow-tmp-create"
                )
                .unwrap(),
                InstallOutcome::Created
            ));
            assert_eq!(fs::read(category.join("guide.md")).unwrap(), b"new");
        }

        for (label, fault) in [
            ("delete-before", WorkspaceFault::BeforeLiveRename),
            ("delete-after", WorkspaceFault::AfterLiveRenameBeforeSync),
        ] {
            let (workspace, _state, capability) = journal_fixture(label);
            WORKSPACE_FAULT.set(Some(fault));
            assert_eq!(
                remove_object(
                    &capability,
                    "categories/linux/guide.md",
                    b"old",
                    0o600,
                    ".rehow-tmp-delete"
                )
                .err(),
                Some(WorkspaceMutationError::Unsupported)
            );
            remove_object(
                &capability,
                "categories/linux/guide.md",
                b"old",
                0o600,
                ".rehow-tmp-delete",
            )
            .unwrap();
            assert!(!workspace.0.join("categories/linux/guide.md").exists());
            assert_eq!(
                fs::read(workspace.0.join("categories/linux/.rehow-tmp-delete")).unwrap(),
                b"old"
            );
        }

        for (label, fault) in [
            ("trash-before", WorkspaceFault::BeforeLiveRename),
            ("trash-after", WorkspaceFault::AfterLiveRenameBeforeSync),
            (
                "trash-destination-sync",
                WorkspaceFault::AfterSourceDirectorySync,
            ),
        ] {
            let (workspace, _state, capability) = journal_fixture(label);
            let destination =
                ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/categories/linux/guide.md";
            WORKSPACE_FAULT.set(Some(fault));
            assert_eq!(
                trash_object(
                    &capability,
                    "categories/linux/guide.md",
                    b"old",
                    0o600,
                    destination
                )
                .err(),
                Some(WorkspaceMutationError::Unsupported)
            );
            trash_object(
                &capability,
                "categories/linux/guide.md",
                b"old",
                0o600,
                destination,
            )
            .unwrap();
            assert!(!workspace.0.join("categories/linux/guide.md").exists());
            assert_eq!(fs::read(workspace.0.join(destination)).unwrap(), b"old");
        }
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

    #[test]
    fn ordinary_delete_commits_cleans_and_resumes_after_move() {
        let (workspace, state, capability) = journal_fixture("delete");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let journal = delete_journal(&capability, &store, &lock, false);
        let committed = execute_new_journal(&capability, &store, &lock, journal).unwrap();
        assert_eq!(committed.stage, JournalStage::Committed);
        let category = workspace.0.join("categories/linux");
        let quarantine = workspace.0.join(format!(
            ".rehow/trash/{}/categories/linux/guide.md",
            committed.operation_id
        ));
        assert!(!category.join("guide.md").exists());
        assert!(!category.join(".rehow-tmp-delete").exists());
        assert_eq!(fs::read(quarantine).unwrap(), b"old");

        let (workspace, state, capability) = journal_fixture("delete-resume");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let journal = delete_journal(&capability, &store, &lock, false);
        store.install_journal(&lock, &journal).unwrap();
        remove_object(
            &capability,
            "categories/linux/guide.md",
            b"old",
            0o600,
            ".rehow-tmp-delete",
        )
        .unwrap();
        assert!(
            workspace
                .0
                .join("categories/linux/.rehow-tmp-delete")
                .exists()
        );
        let committed = resume_journal(&capability, &store, &lock, journal.operation_id).unwrap();
        assert_eq!(committed.stage, JournalStage::Committed);
        assert!(
            !workspace
                .0
                .join("categories/linux/.rehow-tmp-delete")
                .exists()
        );
        assert_eq!(
            fs::read(workspace.0.join(format!(
                ".rehow/trash/{}/categories/linux/guide.md",
                committed.operation_id
            )))
            .unwrap(),
            b"old"
        );
    }

    #[test]
    fn trash_delete_is_durable_and_conflicting_destination_is_preserved() {
        let (workspace, state, capability) = journal_fixture("trash");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let journal = delete_journal(&capability, &store, &lock, true);
        let destination = workspace.0.join(format!(
            ".rehow/trash/{}/categories/linux/guide.md",
            journal.operation_id
        ));
        execute_new_journal(&capability, &store, &lock, journal).unwrap();
        assert!(!workspace.0.join("categories/linux/guide.md").exists());
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert_eq!(
            fs::metadata(destination.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o700
        );

        let (workspace, state, capability) = journal_fixture("trash-conflict");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let journal = delete_journal(&capability, &store, &lock, true);
        let destination = workspace.0.join(format!(
            ".rehow/trash/{}/categories/linux/guide.md",
            journal.operation_id
        ));
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        let mut ancestor = destination.parent();
        while let Some(path) = ancestor {
            if path == workspace.0 {
                break;
            }
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            ancestor = path.parent();
        }
        fs::write(&destination, b"hostile").unwrap();
        assert_eq!(
            execute_new_journal(&capability, &store, &lock, journal.clone()).err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"old"
        );
        assert_eq!(fs::read(destination).unwrap(), b"hostile");
        assert_eq!(
            store
                .read_journal(&lock, journal.operation_id)
                .unwrap()
                .stage,
            JournalStage::Conflict
        );
    }

    #[test]
    fn committed_update_quarantines_before_image_and_rejects_hardlinks() {
        let (workspace, state, capability) = journal_fixture("cleanup");
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
        resume_journal(&capability, &store, &lock, committed.operation_id).unwrap();
        assert!(
            !workspace
                .0
                .join("categories/linux/.rehow-tmp-journal")
                .exists()
        );
        assert_eq!(
            fs::read(workspace.0.join(format!(
                ".rehow/trash/{}/categories/linux/guide.md",
                committed.operation_id
            )))
            .unwrap(),
            b"old"
        );

        let (workspace, state, capability) = journal_fixture("cleanup-hardlink");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let mut journal = update_journal(&capability, &store, &lock, b"old", b"new");
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
        journal.stage = JournalStage::Committed;
        journal.completion = true;
        journal.targets[0].step = TargetStep::Complete;
        store.install_journal(&lock, &journal).unwrap();
        fs::hard_link(
            workspace.0.join("categories/linux/.rehow-tmp-journal"),
            workspace.0.join("categories/linux/cleanup-hardlink"),
        )
        .unwrap();
        assert_eq!(
            resume_journal(&capability, &store, &lock, journal.operation_id).err(),
            Some(WorkspaceMutationError::Unsafe)
        );
        assert!(
            workspace
                .0
                .join("categories/linux/.rehow-tmp-journal")
                .exists()
        );
    }

    #[test]
    fn committed_quarantine_resumes_on_both_sides_of_the_rename() {
        for (label, fault, temp_remains) in [
            (
                "quarantine-before-rename",
                WorkspaceFault::BeforeLiveRename,
                true,
            ),
            (
                "quarantine-after-rename",
                WorkspaceFault::AfterLiveRenameBeforeSync,
                false,
            ),
        ] {
            let (workspace, state, capability) = journal_fixture(label);
            let store = RecoveryStore::open(
                &state.0,
                "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
                [0x11; 32],
                [0x22; 32],
            )
            .unwrap();
            let lock = store.acquire_lock(None).unwrap();
            let mut journal = update_journal(&capability, &store, &lock, b"old", b"new");
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
            journal.stage = JournalStage::Committed;
            journal.completion = true;
            journal.targets[0].step = TargetStep::Complete;
            store.install_journal(&lock, &journal).unwrap();

            WORKSPACE_FAULT.set(Some(fault));
            assert_eq!(
                resume_journal(&capability, &store, &lock, journal.operation_id).err(),
                Some(WorkspaceMutationError::Unsupported)
            );
            assert_eq!(
                workspace
                    .0
                    .join("categories/linux/.rehow-tmp-journal")
                    .exists(),
                temp_remains
            );

            resume_journal(&capability, &store, &lock, journal.operation_id).unwrap();
            assert!(
                !workspace
                    .0
                    .join("categories/linux/.rehow-tmp-journal")
                    .exists()
            );
            assert_eq!(
                fs::read(workspace.0.join(format!(
                    ".rehow/trash/{}/categories/linux/guide.md",
                    journal.operation_id
                )))
                .unwrap(),
                b"old"
            );
        }
    }

    #[test]
    fn hostile_trash_hierarchy_never_redirects_deletion() {
        let (workspace, state, capability) = journal_fixture("trash-symlink");
        let outside = TestDirectory::new("trash-outside");
        symlink(&outside.0, workspace.0.join(".rehow")).unwrap();
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let journal = delete_journal(&capability, &store, &lock, true);
        assert_eq!(
            execute_new_journal(&capability, &store, &lock, journal).err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"old"
        );
        assert!(fs::read_dir(&outside.0).unwrap().next().is_none());

        let (workspace, _state, capability) = journal_fixture("trash-mode");
        fs::create_dir(workspace.0.join(".rehow")).unwrap();
        fs::set_permissions(
            workspace.0.join(".rehow"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let destination =
            ".rehow/trash/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/categories/linux/guide.md";
        assert_eq!(
            trash_object(
                &capability,
                "categories/linux/guide.md",
                b"old",
                0o600,
                destination
            )
            .err(),
            Some(WorkspaceMutationError::Unsafe)
        );
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"old"
        );

        for kind in ["symlink", "hardlink", "fifo"] {
            let (workspace, _state, capability) = journal_fixture(kind);
            let destination_path = workspace.0.join(destination);
            private_parents(destination_path.parent().unwrap(), &workspace.0);
            let outside = workspace.0.join("outside");
            fs::write(&outside, b"outside").unwrap();
            match kind {
                "symlink" => symlink(&outside, &destination_path).unwrap(),
                "hardlink" => fs::hard_link(&outside, &destination_path).unwrap(),
                "fifo" => {
                    use std::os::unix::ffi::OsStrExt as _;
                    let path =
                        std::ffi::CString::new(destination_path.as_os_str().as_bytes()).unwrap();
                    // SAFETY: the path is NUL-terminated and inside the isolated test root.
                    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                }
                _ => unreachable!(),
            }
            assert!(matches!(
                trash_object(
                    &capability,
                    "categories/linux/guide.md",
                    b"old",
                    0o600,
                    destination
                ),
                Err(WorkspaceMutationError::Unsafe | WorkspaceMutationError::Conflict)
            ));
            assert_eq!(
                fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
                b"old"
            );
            assert_eq!(fs::read(&outside).unwrap(), b"outside");
        }
    }

    #[test]
    fn complete_steps_and_committed_create_are_revalidated_from_disk() {
        let (workspace, state, capability) = journal_fixture("complete-update-old");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let mut update = update_journal(&capability, &store, &lock, b"old", b"new");
        update.stage = JournalStage::Applying;
        update.targets[0].step = TargetStep::Complete;
        store.install_journal(&lock, &update).unwrap();
        assert_eq!(
            resume_journal(&capability, &store, &lock, update.operation_id).err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"old"
        );

        let (workspace, state, capability) = journal_fixture("complete-delete-old");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let mut delete = delete_journal(&capability, &store, &lock, false);
        delete.stage = JournalStage::Applying;
        delete.targets[0].step = TargetStep::Complete;
        store.install_journal(&lock, &delete).unwrap();
        assert_eq!(
            resume_journal(&capability, &store, &lock, delete.operation_id).err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"old"
        );

        let (workspace, state, capability) = journal_fixture("complete-trash-old");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let mut trash = delete_journal(&capability, &store, &lock, true);
        trash.stage = JournalStage::Applying;
        trash.targets[0].step = TargetStep::Complete;
        store.install_journal(&lock, &trash).unwrap();
        assert_eq!(
            resume_journal(&capability, &store, &lock, trash.operation_id).err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert!(!workspace.0.join(".rehow").exists());

        let workspace = TestDirectory::new("complete-create-missing");
        workspace.category();
        let state = TestDirectory::new("state");
        let capability = workspace.capability();
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let new_hash = store.store_object(&lock, b"new").unwrap();
        let (root_device, root_inode) = capability.identity().unwrap();
        let mut create = Journal {
            generation: 1,
            operation_id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e13".parse().unwrap(),
            operation_kind: OperationKind::Create,
            workspace_id: "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            root_hash: [0x11; 32],
            root_device,
            root_inode,
            created_at: "2026-09-18T00:00:00Z".parse().unwrap(),
            plan_hash: [0x36; 32],
            targets: vec![Target {
                relative_path: "categories/linux/new.md".to_owned(),
                expected: ExpectedState::Absent,
                intended: IntendedState::Object {
                    object_hash: new_hash,
                    mode: 0o600,
                },
                temp_name: ".rehow-tmp-complete-create".to_owned(),
                step: TargetStep::Complete,
            }],
            stage: JournalStage::Applying,
            completion: false,
        };
        store.install_journal(&lock, &create).unwrap();
        assert_eq!(
            resume_journal(&capability, &store, &lock, create.operation_id).err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert!(!workspace.0.join("categories/linux/new.md").exists());

        create.operation_id = "0199a1d7-7b0d-7d8e-a38d-f936c5735e14".parse().unwrap();
        create.generation = 1;
        create.stage = JournalStage::Committed;
        create.completion = true;
        let state = TestDirectory::new("committed-state");
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x23; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        store.store_object(&lock, b"new").unwrap();
        store.install_journal(&lock, &create).unwrap();
        assert_eq!(
            resume_journal(&capability, &store, &lock, create.operation_id).err(),
            Some(WorkspaceMutationError::Conflict)
        );
        assert!(!workspace.0.join("categories/linux/new.md").exists());
    }

    #[test]
    fn mixed_old_and_new_targets_resume_in_journal_order() {
        let (workspace, state, capability) = journal_fixture("mixed");
        let second_directory = workspace.0.join("categories/system");
        fs::create_dir_all(&second_directory).unwrap();
        fs::write(second_directory.join("other.md"), b"old-two").unwrap();
        fs::set_permissions(
            second_directory.join("other.md"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let store = RecoveryStore::open(
            &state.0,
            "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            [0x11; 32],
            [0x22; 32],
        )
        .unwrap();
        let lock = store.acquire_lock(None).unwrap();
        let (root_device, root_inode) = capability.identity().unwrap();
        let old_one = store.store_object(&lock, b"old").unwrap();
        let new_one = store.store_object(&lock, b"new-one").unwrap();
        let old_two = store.store_object(&lock, b"old-two").unwrap();
        let new_two = store.store_object(&lock, b"new-two").unwrap();
        let journal = Journal {
            generation: 1,
            operation_id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e12".parse().unwrap(),
            operation_kind: OperationKind::MultiFile,
            workspace_id: "0199a19f-4d15-7ca7-a645-e75311caa710".parse().unwrap(),
            root_hash: [0x11; 32],
            root_device,
            root_inode,
            created_at: "2026-09-18T00:00:00Z".parse().unwrap(),
            plan_hash: [0x35; 32],
            targets: vec![
                Target {
                    relative_path: "categories/linux/guide.md".to_owned(),
                    expected: ExpectedState::Existing {
                        revision: crate::storage::document_revision(
                            "categories/linux/guide.md",
                            b"old",
                        ),
                        object_hash: old_one,
                        mode: 0o600,
                    },
                    intended: IntendedState::Object {
                        object_hash: new_one,
                        mode: 0o600,
                    },
                    temp_name: ".rehow-tmp-mixed".to_owned(),
                    step: TargetStep::Prepared,
                },
                Target {
                    relative_path: "categories/system/other.md".to_owned(),
                    expected: ExpectedState::Existing {
                        revision: crate::storage::document_revision(
                            "categories/system/other.md",
                            b"old-two",
                        ),
                        object_hash: old_two,
                        mode: 0o600,
                    },
                    intended: IntendedState::Object {
                        object_hash: new_two,
                        mode: 0o600,
                    },
                    temp_name: ".rehow-tmp-mixed".to_owned(),
                    step: TargetStep::Prepared,
                },
            ],
            stage: JournalStage::Prepared,
            completion: false,
        };
        store.install_journal(&lock, &journal).unwrap();
        let InstallOutcome::Replaced(first) = install_object(
            &capability,
            "categories/linux/guide.md",
            Some((b"old", 0o600)),
            b"new-one",
            0o600,
            ".rehow-tmp-mixed",
        )
        .unwrap() else {
            panic!("expected replacement");
        };
        first.verify().unwrap();

        let committed = resume_journal(&capability, &store, &lock, journal.operation_id).unwrap();
        assert_eq!(committed.stage, JournalStage::Committed);
        assert_eq!(
            fs::read(workspace.0.join("categories/linux/guide.md")).unwrap(),
            b"new-one"
        );
        assert_eq!(
            fs::read(workspace.0.join("categories/system/other.md")).unwrap(),
            b"new-two"
        );
        assert!(
            !workspace
                .0
                .join("categories/linux/.rehow-tmp-mixed")
                .exists()
        );
        assert!(
            !workspace
                .0
                .join("categories/system/.rehow-tmp-mixed")
                .exists()
        );
        assert_eq!(
            fs::read(workspace.0.join(format!(
                ".rehow/trash/{}/categories/linux/guide.md",
                committed.operation_id
            )))
            .unwrap(),
            b"old"
        );
        assert_eq!(
            fs::read(workspace.0.join(format!(
                ".rehow/trash/{}/categories/system/other.md",
                committed.operation_id
            )))
            .unwrap(),
            b"old-two"
        );
    }
}

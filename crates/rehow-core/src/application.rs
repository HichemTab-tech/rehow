//! Crate-private application orchestration over domain plans and confined storage.

#![allow(
    dead_code,
    reason = "the private application proof is exposed to adapters only after its later gate"
)]

use crate::{
    domain::content::{
        guide::bind_guide,
        mutation::{
            GuideMutation, MutationFailureKind, MutationOutcome, MutationStamp, plan_guide_mutation,
        },
    },
    parsing::parse_guide_source,
    protocol::{GuideId, OperationId, Revision, Timestamp, current_timestamp, new_operation_id},
    storage::{DocumentUpdateError, WorkspaceView, execute_document_update, refresh_workspace},
};
use std::path::Path;

pub const GUIDE_BODY_INPUT_LIMIT: usize = 2 * 1024 * 1024;

pub enum GuideEdit {
    SetTitle(String),
    SetSummary(String),
    ReplaceBody(Vec<u8>),
    MarkStale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuideUpdateError {
    NotFound,
    RevisionConflict,
    InvalidGuide,
    InvalidEdit,
    UnsafeRoundTrip,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    CommittedRefreshRequired { revision: Revision },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuideUpdateOutcome {
    revision: Revision,
    changed: bool,
}

impl GuideUpdateOutcome {
    pub fn revision(self) -> Revision {
        self.revision
    }

    pub fn changed(self) -> bool {
        self.changed
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_guide(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    edit: GuideEdit,
) -> Result<GuideUpdateOutcome, GuideUpdateError> {
    update_guide_with_context(
        workspace,
        state_home,
        runtime_directory,
        guide_id,
        expected_revision,
        new_operation_id(),
        current_timestamp(),
        edit,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_guide_with_context(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    operation_id: OperationId,
    at: Timestamp,
    edit: GuideEdit,
) -> Result<GuideUpdateOutcome, GuideUpdateError> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides().iter().map(move |guide| (category, guide)))
        .find(|(_, guide)| guide.guide().id() == &guide_id)
        .ok_or(GuideUpdateError::NotFound)?;
    if loaded.1.revision() != expected_revision {
        return Err(GuideUpdateError::RevisionConflict);
    }
    let relative_path = loaded.1.relative_path().to_owned();
    let category_id = *loaded.0.category().id();
    let stamp = MutationStamp::from_trusted_time(at);

    let result = execute_document_update(
        workspace,
        state_home,
        runtime_directory,
        &relative_path,
        expected_revision,
        operation_id,
        at,
        |bytes| {
            let parsed =
                parse_guide_source(copy_bytes(bytes)?).map_err(|_| PlanError::InvalidGuide)?;
            let bound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
            if bound.record().id() != &guide_id || bound.record().category_id() != &category_id {
                return Err(PlanError::InvalidGuide);
            }
            let mutation = match edit {
                GuideEdit::SetTitle(value) => GuideMutation::SetTitle(value),
                GuideEdit::SetSummary(value) => GuideMutation::SetSummary(value),
                GuideEdit::ReplaceBody(value) => GuideMutation::ReplaceBody(value),
                GuideEdit::MarkStale => GuideMutation::MarkStale,
            };
            match plan_guide_mutation(&bound, mutation, &stamp).map_err(map_plan_failure)? {
                MutationOutcome::NoOp => copy_bytes(bytes),
                MutationOutcome::Candidate(candidate) => {
                    copy_bytes(candidate.bound().source_bytes())
                }
            }
        },
    )
    .map_err(map_update_error)?;

    if result.changed() && refresh_workspace(workspace).is_err() {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: result.revision(),
        });
    }
    Ok(GuideUpdateOutcome {
        revision: result.revision(),
        changed: result.changed(),
    })
}

#[derive(Clone, Copy)]
enum PlanError {
    InvalidGuide,
    InvalidEdit,
    UnsafeRoundTrip,
    ResourceLimit,
}

fn copy_bytes(bytes: &[u8]) -> Result<Box<[u8]>, PlanError> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len())
        .map_err(|_| PlanError::ResourceLimit)?;
    copy.extend_from_slice(bytes);
    Ok(copy.into_boxed_slice())
}

fn map_plan_failure(error: crate::domain::content::mutation::MutationFailure) -> PlanError {
    match error.kind() {
        MutationFailureKind::InvalidValue | MutationFailureKind::InvalidTransition => {
            PlanError::InvalidEdit
        }
        MutationFailureKind::ReadOnly
        | MutationFailureKind::CommentUnsafe
        | MutationFailureKind::PresentationUnsafe => PlanError::UnsafeRoundTrip,
        MutationFailureKind::ResourceLimit => PlanError::ResourceLimit,
        MutationFailureKind::CandidateMismatch | MutationFailureKind::BindingInvariant => {
            PlanError::InvalidGuide
        }
    }
}

fn map_update_error(error: DocumentUpdateError<PlanError>) -> GuideUpdateError {
    match error {
        DocumentUpdateError::Prepare(PlanError::InvalidGuide) => GuideUpdateError::InvalidGuide,
        DocumentUpdateError::Prepare(PlanError::InvalidEdit) => GuideUpdateError::InvalidEdit,
        DocumentUpdateError::Prepare(PlanError::UnsafeRoundTrip) => {
            GuideUpdateError::UnsafeRoundTrip
        }
        DocumentUpdateError::Prepare(PlanError::ResourceLimit) | DocumentUpdateError::TooLarge => {
            GuideUpdateError::ResourceLimit
        }
        DocumentUpdateError::Busy => GuideUpdateError::Busy,
        DocumentUpdateError::Conflict => GuideUpdateError::RevisionConflict,
        DocumentUpdateError::Unsafe => GuideUpdateError::UnsafeStorage,
        DocumentUpdateError::Unsupported => GuideUpdateError::UnsupportedFilesystem,
        DocumentUpdateError::Io => GuideUpdateError::Io,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::GuideStatus, storage::WorkspaceInspection, storage::inspect_workspace};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        str::FromStr,
        sync::atomic::{AtomicU64, Ordering},
    };

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const GUIDE_ID: &str = "0199a1d7-7b0d-7d8e-a38d-f936c5735e10";
    const GUIDE_PATH: &str = "categories/linux-system--b6ba3a5551/install--36c5735e10.md";
    const MANIFEST: &str = "schema_version: 1\nworkspace_id: 0199a19f-4d15-7ca7-a645-e75311caa710\nname: Test Workspace\ncreated_at: \"2026-09-05T10:00:00Z\"\nupdated_at: \"2026-09-05T10:00:00Z\"\n";
    const CATEGORY: &str = "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux System\norder: 1000\nstatus: active\ncreated_at: \"2026-09-05T10:01:00Z\"\nupdated_at: \"2026-09-05T10:01:00Z\"\n";
    const GUIDE: &str = "---\nschema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: Install a Tarball\nsummary: Install a downloaded archive.\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags:\n  - linux\naliases:\n  - manual install\nstatus: draft\ncreated_at: \"2026-09-05T10:03:00Z\"\nupdated_at: \"2026-09-05T10:03:00Z\"\nfuture_key: keep-me\n---\n\n## Steps\n\nExtract the archive.\n";

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let number = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rehow-application-{label}-{}-{number}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture(label: &str, guide: &str) -> (TestDirectory, TestDirectory, Box<WorkspaceView>) {
        let workspace = TestDirectory::new(label);
        let state = TestDirectory::new("state");
        fs::write(workspace.path().join("rehow.yaml"), MANIFEST).unwrap();
        let category = workspace.path().join("categories/linux-system--b6ba3a5551");
        fs::create_dir_all(&category).unwrap();
        fs::write(category.join("category.yaml"), CATEGORY).unwrap();
        fs::write(category.join("install--36c5735e10.md"), guide).unwrap();
        fs::set_permissions(
            category.join("install--36c5735e10.md"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let inspection = inspect_workspace(workspace.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(view) = inspection else {
            panic!("expected compatible Workspace");
        };
        (workspace, state, view)
    }

    fn revision(view: &WorkspaceView) -> Revision {
        view.categories()[0].guides()[0].revision()
    }

    fn operation(value: &str) -> OperationId {
        OperationId::from_str(value).unwrap()
    }

    fn at() -> Timestamp {
        Timestamp::from_str("2026-09-18T12:00:00Z").unwrap()
    }

    #[test]
    fn update_is_revision_checked_preserves_unknowns_and_refreshes() {
        let (workspace, state, mut view) = fixture("update", GUIDE);
        let original_revision = revision(&view);
        let outcome = update_guide_with_context(
            &mut view,
            state.path(),
            None,
            GuideId::from_str(GUIDE_ID).unwrap(),
            original_revision,
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e20"),
            at(),
            GuideEdit::ReplaceBody(b"## Steps\n\nUse the verified command.\n".to_vec()),
        )
        .unwrap();
        assert!(outcome.changed());
        assert_ne!(outcome.revision(), original_revision);
        assert_eq!(revision(&view), outcome.revision());
        assert_eq!(
            view.categories()[0].guides()[0].guide().body(),
            b"## Steps\n\nUse the verified command.\n"
        );
        let bytes = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        assert!(
            bytes
                .windows(b"future_key: keep-me".len())
                .any(|window| window == b"future_key: keep-me")
        );

        let no_op = update_guide_with_context(
            &mut view,
            state.path(),
            None,
            GuideId::from_str(GUIDE_ID).unwrap(),
            outcome.revision(),
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e21"),
            at(),
            GuideEdit::ReplaceBody(b"## Steps\n\nUse the verified command.\n".to_vec()),
        )
        .unwrap();
        assert!(!no_op.changed());
        assert_eq!(no_op.revision(), outcome.revision());
        assert_eq!(
            update_guide_with_context(
                &mut view,
                state.path(),
                None,
                GuideId::from_str(GUIDE_ID).unwrap(),
                outcome.revision(),
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e20"),
                at(),
                GuideEdit::SetTitle("Reused operation".to_owned()),
            ),
            Err(GuideUpdateError::RevisionConflict)
        );
        assert_eq!(
            view.categories()[0].guides()[0].guide().title(),
            "Install a Tarball"
        );
    }

    #[test]
    fn public_update_generates_context_and_commits() {
        let (_workspace, state, mut view) = fixture("public-update", GUIDE);
        let expected = revision(&view);
        let outcome = update_guide(
            &mut view,
            state.path(),
            None,
            GuideId::from_str(GUIDE_ID).unwrap(),
            expected,
            GuideEdit::ReplaceBody(b"## Steps\n\nSaved from the desktop.\n".to_vec()),
        )
        .unwrap();

        assert!(outcome.changed());
        assert_eq!(revision(&view), outcome.revision());
    }

    #[test]
    fn stale_or_externally_changed_revision_never_overwrites() {
        let (workspace, state, mut view) = fixture("conflict", GUIDE);
        let expected = revision(&view);
        let path = workspace.path().join(GUIDE_PATH);
        let external = GUIDE.replace("Extract the archive.", "External edit.");
        fs::write(&path, external.as_bytes()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            update_guide_with_context(
                &mut view,
                state.path(),
                None,
                GuideId::from_str(GUIDE_ID).unwrap(),
                expected,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e22"),
                at(),
                GuideEdit::SetTitle("Changed".to_owned()),
            ),
            Err(GuideUpdateError::RevisionConflict)
        );
        assert_eq!(fs::read_to_string(path).unwrap(), external);
    }

    #[test]
    fn tested_edit_becomes_stale_and_hostile_replacement_is_refused() {
        let tested = GUIDE.replacen(
            "status: draft\n",
            "status: tested\nlast_verified_at: \"2026-09-05T10:03:00Z\"\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
            1,
        );
        let (workspace, state, mut view) = fixture("tested", &tested);
        let expected = revision(&view);
        let outcome = update_guide_with_context(
            &mut view,
            state.path(),
            None,
            GuideId::from_str(GUIDE_ID).unwrap(),
            expected,
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e23"),
            at(),
            GuideEdit::SetSummary("Updated summary".to_owned()),
        )
        .unwrap();
        assert!(outcome.changed());
        assert_eq!(
            view.categories()[0].guides()[0].guide().declared_status(),
            GuideStatus::Stale
        );

        let expected = revision(&view);
        let target = workspace.path().join(GUIDE_PATH);
        let hardlink = workspace.path().join("hostile.md");
        fs::hard_link(&target, &hardlink).unwrap();
        assert_eq!(
            update_guide_with_context(
                &mut view,
                state.path(),
                None,
                GuideId::from_str(GUIDE_ID).unwrap(),
                expected,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e24"),
                at(),
                GuideEdit::MarkStale,
            ),
            Err(GuideUpdateError::UnsafeStorage)
        );
    }
}

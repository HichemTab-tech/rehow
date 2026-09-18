//! Crate-private application orchestration over domain plans and confined storage.

#![allow(
    dead_code,
    reason = "the private application proof is exposed to adapters only after its later gate"
)]

use crate::{
    domain::content::{
        CategoryStatus, bind_category,
        emission::{
            EmitFailureKind, EmitField, emit_new_category, emit_new_guide,
            emit_new_workspace_manifest,
        },
        guide::{BoundGuide, bind_guide},
        mutation::{
            CategoryMutation, GuideMutation, MutationFailureKind, MutationOutcome, MutationStamp,
            plan_category_mutation, plan_guide_mutation,
        },
    },
    parsing::{parse_guide_source, parse_yaml_document},
    protocol::{
        CategoryId, GuideId, OperationId, Revision, Timestamp, current_timestamp, new_category_id,
        new_guide_id, new_operation_id, new_workspace_id,
    },
    storage::{
        CategoryCreateStorageError, DocumentCreateError, DocumentUpdateError, EmptyWorkspaceTarget,
        WorkspaceInitializeError, WorkspaceView, execute_category_create, execute_document_create,
        execute_document_update, execute_workspace_initialize, open_initialized_workspace,
        refresh_workspace,
    },
};
use std::path::Path;

pub const GUIDE_BODY_INPUT_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CategoryCreateError {
    InvalidName,
    InvalidSummary,
    Busy,
    Conflict,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    Incomplete,
    CommittedRefreshRequired {
        category_id: CategoryId,
        revision: Revision,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryCreateOutcome {
    category_id: CategoryId,
    revision: Revision,
}

impl CategoryCreateOutcome {
    pub fn category_id(self) -> CategoryId {
        self.category_id
    }

    pub fn revision(self) -> Revision {
        self.revision
    }
}

pub fn create_category(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    name: &str,
    summary: Option<&str>,
) -> Result<CategoryCreateOutcome, CategoryCreateError> {
    if workspace.categories().len() >= 1_000 {
        return Err(CategoryCreateError::ResourceLimit);
    }
    let category_id = new_category_id();
    let operation_id = new_operation_id();
    let timestamp = current_timestamp();
    let order = workspace
        .categories()
        .iter()
        .map(|loaded| loaded.category().order())
        .max()
        .map_or(1_000, |current| current.saturating_add(1_000));
    let descriptor = emit_new_category(&category_id, name, summary, order, &timestamp).map_err(
        |error| match (error.kind(), error.field()) {
            (EmitFailureKind::ResourceLimit, _) => CategoryCreateError::ResourceLimit,
            (EmitFailureKind::InvalidValue, EmitField::Summary) => {
                CategoryCreateError::InvalidSummary
            }
            (EmitFailureKind::InvalidValue, _) => CategoryCreateError::InvalidName,
            (EmitFailureKind::CandidateMismatch, _) => CategoryCreateError::Io,
        },
    )?;
    let relative_directory = category_directory_name(name, &category_id);
    let result = execute_category_create(
        workspace,
        state_home,
        runtime_directory,
        &relative_directory,
        operation_id,
        timestamp,
        descriptor.source_bytes(),
    )
    .map_err(map_category_create_error)?;
    if refresh_workspace(workspace).is_err()
        || !workspace.categories().iter().any(|loaded| {
            loaded.category().id() == &category_id
                && loaded.descriptor_revision() == result.revision()
        })
    {
        return Err(CategoryCreateError::CommittedRefreshRequired {
            category_id,
            revision: result.revision(),
        });
    }
    Ok(CategoryCreateOutcome {
        category_id,
        revision: result.revision(),
    })
}

fn category_directory_name(name: &str, category_id: &CategoryId) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() && slug.len() < 80 {
                slug.push('-');
            }
            separator = false;
            if slug.len() < 80 {
                slug.push(character.to_ascii_lowercase());
            }
        } else {
            separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("category");
    }
    let id = category_id.to_string();
    let suffix = &id[id.len() - 10..];
    format!("categories/{slug}--{suffix}")
}

fn map_category_create_error(error: CategoryCreateStorageError) -> CategoryCreateError {
    match error {
        CategoryCreateStorageError::Busy => CategoryCreateError::Busy,
        CategoryCreateStorageError::Conflict => CategoryCreateError::Conflict,
        CategoryCreateStorageError::Unsafe => CategoryCreateError::UnsafeStorage,
        CategoryCreateStorageError::Unsupported => CategoryCreateError::UnsupportedFilesystem,
        CategoryCreateStorageError::TooLarge => CategoryCreateError::ResourceLimit,
        CategoryCreateStorageError::Io => CategoryCreateError::Io,
        CategoryCreateStorageError::Incomplete => CategoryCreateError::Incomplete,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CategoryUpdateError {
    NotFound,
    DefaultCategory,
    RevisionConflict,
    InvalidCategory,
    InvalidName,
    UnsafeRoundTrip,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    CommittedRefreshRequired { revision: Revision },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryUpdateOutcome {
    revision: Revision,
    changed: bool,
}

impl CategoryUpdateOutcome {
    pub fn revision(self) -> Revision {
        self.revision
    }

    pub fn changed(self) -> bool {
        self.changed
    }
}

pub fn rename_category(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    expected_revision: Revision,
    name: String,
) -> Result<CategoryUpdateOutcome, CategoryUpdateError> {
    update_category_with_mutation(
        workspace,
        state_home,
        runtime_directory,
        category_id,
        expected_revision,
        CategoryMutation::SetName(name),
    )
}

pub fn set_category_archived(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    expected_revision: Revision,
    archived: bool,
) -> Result<CategoryUpdateOutcome, CategoryUpdateError> {
    if archived
        && workspace
            .manifest()
            .settings()
            .and_then(|settings| settings.default_category_id())
            == Some(&category_id)
    {
        return Err(CategoryUpdateError::DefaultCategory);
    }
    update_category_with_mutation(
        workspace,
        state_home,
        runtime_directory,
        category_id,
        expected_revision,
        if archived {
            CategoryMutation::Archive
        } else {
            CategoryMutation::Restore
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn update_category_with_mutation(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    expected_revision: Revision,
    mutation: CategoryMutation,
) -> Result<CategoryUpdateOutcome, CategoryUpdateError> {
    let loaded = workspace
        .categories()
        .iter()
        .find(|category| category.category().id() == &category_id)
        .ok_or(CategoryUpdateError::NotFound)?;
    if loaded.descriptor_revision() != expected_revision {
        return Err(CategoryUpdateError::RevisionConflict);
    }
    let relative_directory = loaded.relative_directory().to_owned();
    let relative_path = format!("{relative_directory}/category.yaml");
    let at = current_timestamp();
    let stamp = MutationStamp::from_trusted_time(at);
    let result = execute_document_update(
        workspace,
        state_home,
        runtime_directory,
        &relative_path,
        expected_revision,
        new_operation_id(),
        at,
        |bytes| {
            let parsed = parse_yaml_document(copy_bytes(bytes)?)
                .map_err(|_| CategoryPlanError::InvalidCategory)?;
            let bound = bind_category(parsed).map_err(|_| CategoryPlanError::InvalidCategory)?;
            if bound.record().id() != &category_id {
                return Err(CategoryPlanError::InvalidCategory);
            }
            match plan_category_mutation(&bound, mutation, &stamp)
                .map_err(map_category_plan_failure)?
            {
                MutationOutcome::NoOp => copy_bytes(bytes).map_err(CategoryPlanError::from),
                MutationOutcome::Candidate(candidate) => {
                    copy_bytes(candidate.bound().source_bytes()).map_err(CategoryPlanError::from)
                }
            }
        },
    )
    .map_err(map_category_update_error)?;

    if result.changed()
        && (refresh_workspace(workspace).is_err()
            || !workspace.categories().iter().any(|category| {
                category.category().id() == &category_id
                    && category.relative_directory() == relative_directory
                    && category.descriptor_revision() == result.revision()
            }))
    {
        return Err(CategoryUpdateError::CommittedRefreshRequired {
            revision: result.revision(),
        });
    }
    Ok(CategoryUpdateOutcome {
        revision: result.revision(),
        changed: result.changed(),
    })
}

#[derive(Clone, Copy)]
enum CategoryPlanError {
    InvalidCategory,
    InvalidName,
    UnsafeRoundTrip,
    ResourceLimit,
}

impl From<PlanError> for CategoryPlanError {
    fn from(error: PlanError) -> Self {
        match error {
            PlanError::ResourceLimit => Self::ResourceLimit,
            PlanError::InvalidGuide | PlanError::InvalidEdit => Self::InvalidCategory,
            PlanError::UnsafeRoundTrip => Self::UnsafeRoundTrip,
        }
    }
}

fn map_category_plan_failure(
    error: crate::domain::content::mutation::MutationFailure,
) -> CategoryPlanError {
    match error.kind() {
        MutationFailureKind::InvalidValue | MutationFailureKind::InvalidTransition => {
            CategoryPlanError::InvalidName
        }
        MutationFailureKind::ReadOnly
        | MutationFailureKind::CommentUnsafe
        | MutationFailureKind::PresentationUnsafe => CategoryPlanError::UnsafeRoundTrip,
        MutationFailureKind::ResourceLimit => CategoryPlanError::ResourceLimit,
        MutationFailureKind::CandidateMismatch | MutationFailureKind::BindingInvariant => {
            CategoryPlanError::InvalidCategory
        }
    }
}

fn map_category_update_error(error: DocumentUpdateError<CategoryPlanError>) -> CategoryUpdateError {
    match error {
        DocumentUpdateError::Prepare(CategoryPlanError::InvalidCategory) => {
            CategoryUpdateError::InvalidCategory
        }
        DocumentUpdateError::Prepare(CategoryPlanError::InvalidName) => {
            CategoryUpdateError::InvalidName
        }
        DocumentUpdateError::Prepare(CategoryPlanError::UnsafeRoundTrip) => {
            CategoryUpdateError::UnsafeRoundTrip
        }
        DocumentUpdateError::Prepare(CategoryPlanError::ResourceLimit)
        | DocumentUpdateError::TooLarge => CategoryUpdateError::ResourceLimit,
        DocumentUpdateError::Busy => CategoryUpdateError::Busy,
        DocumentUpdateError::Conflict => CategoryUpdateError::RevisionConflict,
        DocumentUpdateError::Unsafe => CategoryUpdateError::UnsafeStorage,
        DocumentUpdateError::Unsupported => CategoryUpdateError::UnsupportedFilesystem,
        DocumentUpdateError::Io => CategoryUpdateError::Io,
    }
}

const NEW_GUIDE_BODY: &[u8] = b"## Steps\n\n1. Describe the first step.\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuideCreateError {
    CategoryNotFound,
    CategoryArchived,
    InvalidTitle,
    InvalidSummary,
    Busy,
    Conflict,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    Incomplete,
    CommittedRefreshRequired {
        guide_id: GuideId,
        revision: Revision,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuideCreateOutcome {
    guide_id: GuideId,
    revision: Revision,
}

impl GuideCreateOutcome {
    pub fn guide_id(self) -> GuideId {
        self.guide_id
    }

    pub fn revision(self) -> Revision {
        self.revision
    }
}

pub fn create_guide(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    title: &str,
    summary: &str,
) -> Result<GuideCreateOutcome, GuideCreateError> {
    if workspace
        .categories()
        .iter()
        .map(|category| category.guides().len())
        .sum::<usize>()
        >= 10_000
    {
        return Err(GuideCreateError::ResourceLimit);
    }
    let category = workspace
        .categories()
        .iter()
        .find(|loaded| loaded.category().id() == &category_id)
        .ok_or(GuideCreateError::CategoryNotFound)?;
    if category.category().status() != CategoryStatus::Active {
        return Err(GuideCreateError::CategoryArchived);
    }
    let relative_directory = category.relative_directory().to_owned();
    let guide_id = new_guide_id();
    let operation_id = new_operation_id();
    let timestamp = current_timestamp();
    let document = emit_new_guide(
        &guide_id,
        title,
        summary,
        &category_id,
        &timestamp,
        NEW_GUIDE_BODY,
    )
    .map_err(|error| match (error.kind(), error.field()) {
        (EmitFailureKind::ResourceLimit, _) => GuideCreateError::ResourceLimit,
        (EmitFailureKind::InvalidValue, EmitField::Summary) => GuideCreateError::InvalidSummary,
        (EmitFailureKind::InvalidValue, EmitField::Title) => GuideCreateError::InvalidTitle,
        (EmitFailureKind::InvalidValue, _) | (EmitFailureKind::CandidateMismatch, _) => {
            GuideCreateError::Io
        }
    })?;
    let relative_path = format!("{relative_directory}/{}", guide_file_name(title, &guide_id));
    let result = execute_document_create(
        workspace,
        state_home,
        runtime_directory,
        &relative_path,
        operation_id,
        timestamp,
        document.source_bytes(),
    )
    .map_err(map_guide_create_error)?;
    if refresh_workspace(workspace).is_err()
        || !workspace.categories().iter().any(|category| {
            category.guides().iter().any(|loaded| {
                loaded.guide().id() == &guide_id && loaded.revision() == result.revision()
            })
        })
    {
        return Err(GuideCreateError::CommittedRefreshRequired {
            guide_id,
            revision: result.revision(),
        });
    }
    Ok(GuideCreateOutcome {
        guide_id,
        revision: result.revision(),
    })
}

fn guide_file_name(title: &str, guide_id: &GuideId) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() && slug.len() < 80 {
                slug.push('-');
            }
            separator = false;
            if slug.len() < 80 {
                slug.push(character.to_ascii_lowercase());
            }
        } else {
            separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("guide");
    }
    let id = guide_id.to_string();
    format!("{slug}--{}.md", &id[id.len() - 10..])
}

fn map_guide_create_error(error: DocumentCreateError) -> GuideCreateError {
    match error {
        DocumentCreateError::Busy => GuideCreateError::Busy,
        DocumentCreateError::Conflict => GuideCreateError::Conflict,
        DocumentCreateError::Unsafe => GuideCreateError::UnsafeStorage,
        DocumentCreateError::Unsupported => GuideCreateError::UnsupportedFilesystem,
        DocumentCreateError::TooLarge => GuideCreateError::ResourceLimit,
        DocumentCreateError::Io => GuideCreateError::Io,
        DocumentCreateError::Incomplete => GuideCreateError::Incomplete,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceInitializationError {
    InvalidName,
    Busy,
    Conflict,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    CommittedRefreshRequired { revision: Revision },
}

pub fn initialize_workspace(
    target: &EmptyWorkspaceTarget,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    name: &str,
) -> Result<Box<WorkspaceView>, WorkspaceInitializationError> {
    let workspace_id = new_workspace_id();
    let operation_id = new_operation_id();
    let timestamp = current_timestamp();
    let manifest =
        emit_new_workspace_manifest(&workspace_id, name, &timestamp, None).map_err(|error| {
            match error.kind() {
                EmitFailureKind::InvalidValue | EmitFailureKind::CandidateMismatch => {
                    WorkspaceInitializationError::InvalidName
                }
                EmitFailureKind::ResourceLimit => WorkspaceInitializationError::ResourceLimit,
            }
        })?;
    let outcome = execute_workspace_initialize(
        target,
        state_home,
        runtime_directory,
        workspace_id,
        operation_id,
        timestamp,
        manifest.source_bytes(),
    )
    .map_err(map_workspace_initialization_error)?;
    let workspace = open_initialized_workspace(target).map_err(|_| {
        WorkspaceInitializationError::CommittedRefreshRequired {
            revision: outcome.revision(),
        }
    })?;
    if workspace.manifest_revision() != outcome.revision() {
        return Err(WorkspaceInitializationError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    }
    Ok(workspace)
}

fn map_workspace_initialization_error(
    error: WorkspaceInitializeError,
) -> WorkspaceInitializationError {
    match error {
        WorkspaceInitializeError::Busy => WorkspaceInitializationError::Busy,
        WorkspaceInitializeError::Conflict => WorkspaceInitializationError::Conflict,
        WorkspaceInitializeError::Unsafe => WorkspaceInitializationError::UnsafeStorage,
        WorkspaceInitializeError::Unsupported => {
            WorkspaceInitializationError::UnsupportedFilesystem
        }
        WorkspaceInitializeError::TooLarge => WorkspaceInitializationError::ResourceLimit,
        WorkspaceInitializeError::Io => WorkspaceInitializationError::Io,
        WorkspaceInitializeError::Committed { revision } => {
            WorkspaceInitializationError::CommittedRefreshRequired { revision }
        }
    }
}

pub enum GuideEdit {
    SetTitle(String),
    SetSummary(String),
    SetMetadata { title: String, summary: String },
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
            match edit {
                GuideEdit::SetMetadata { title, summary } => {
                    let title_candidate =
                        apply_guide_mutation(&bound, GuideMutation::SetTitle(title), &stamp)?;
                    let parsed =
                        parse_guide_source(title_candidate).map_err(|_| PlanError::InvalidGuide)?;
                    let rebound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
                    apply_guide_mutation(&rebound, GuideMutation::SetSummary(summary), &stamp)
                }
                GuideEdit::SetTitle(value) => {
                    apply_guide_mutation(&bound, GuideMutation::SetTitle(value), &stamp)
                }
                GuideEdit::SetSummary(value) => {
                    apply_guide_mutation(&bound, GuideMutation::SetSummary(value), &stamp)
                }
                GuideEdit::ReplaceBody(value) => {
                    apply_guide_mutation(&bound, GuideMutation::ReplaceBody(value), &stamp)
                }
                GuideEdit::MarkStale => {
                    apply_guide_mutation(&bound, GuideMutation::MarkStale, &stamp)
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

fn apply_guide_mutation(
    bound: &BoundGuide,
    mutation: GuideMutation,
    stamp: &MutationStamp,
) -> Result<Box<[u8]>, PlanError> {
    match plan_guide_mutation(bound, mutation, stamp).map_err(map_plan_failure)? {
        MutationOutcome::NoOp => copy_bytes(bound.source_bytes()),
        MutationOutcome::Candidate(candidate) => copy_bytes(candidate.bound().source_bytes()),
    }
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
    use crate::{
        domain::{CategoryStatus, GuideStatus},
        storage::WorkspaceInspection,
        storage::inspect_workspace,
    };
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

    fn empty_target(directory: &TestDirectory) -> EmptyWorkspaceTarget {
        let inspection = inspect_workspace(directory.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Empty(target) = inspection else {
            panic!("expected empty Workspace target");
        };
        target
    }

    fn initialized_workspace(label: &str) -> (TestDirectory, TestDirectory, Box<WorkspaceView>) {
        let workspace = TestDirectory::new(label);
        let state = TestDirectory::new("initialized-state");
        let target = empty_target(&workspace);
        let view = initialize_workspace(&target, state.path(), None, "Personal Rehow").unwrap();
        (workspace, state, view)
    }

    #[test]
    fn initializes_and_opens_an_empty_workspace() {
        let workspace = TestDirectory::new("initialize");
        let state = TestDirectory::new("initialize-state");
        let target = empty_target(&workspace);
        let view = initialize_workspace(&target, state.path(), None, "Personal Rehow").unwrap();

        assert_eq!(view.manifest().name(), "Personal Rehow");
        assert!(view.categories().is_empty());
        assert!(view.issues().is_empty());
        assert_eq!(
            fs::metadata(workspace.path().join("rehow.yaml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(workspace.path().join("categories"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[test]
    fn initialization_refuses_every_change_after_selection() {
        let changed = TestDirectory::new("initialize-changed");
        let state = TestDirectory::new("initialize-state");
        let target = empty_target(&changed);
        fs::write(changed.path().join("appeared.txt"), b"do not overwrite").unwrap();
        let changed_result = initialize_workspace(&target, state.path(), None, "Personal Rehow");
        assert!(
            matches!(changed_result, Err(WorkspaceInitializationError::Conflict)),
            "unexpected result: {:?}",
            changed_result.err()
        );
        assert_eq!(
            fs::read(changed.path().join("appeared.txt")).unwrap(),
            b"do not overwrite"
        );
        assert!(!changed.path().join("rehow.yaml").exists());

        let directory_change = TestDirectory::new("initialize-directory-change");
        let target = empty_target(&directory_change);
        fs::create_dir(directory_change.path().join("categories")).unwrap();
        fs::set_permissions(
            directory_change.path().join("categories"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(matches!(
            initialize_workspace(&target, state.path(), None, "Personal Rehow"),
            Err(WorkspaceInitializationError::Conflict)
        ));
        assert!(!directory_change.path().join("rehow.yaml").exists());
    }

    #[test]
    fn invalid_workspace_name_writes_nothing() {
        let workspace = TestDirectory::new("initialize-invalid");
        let state = TestDirectory::new("initialize-state");
        let target = empty_target(&workspace);
        assert!(matches!(
            initialize_workspace(&target, state.path(), None, "   "),
            Err(WorkspaceInitializationError::InvalidName)
        ));
        assert!(fs::read_dir(workspace.path()).unwrap().next().is_none());
    }

    #[test]
    fn creates_categories_with_canonical_order_paths_and_modes() {
        let (workspace, state, mut view) = initialized_workspace("category-create");
        let first = create_category(
            &mut view,
            state.path(),
            None,
            "Linux System",
            Some("Boot, devices, and desktop configuration."),
        )
        .unwrap();
        assert_eq!(view.categories().len(), 1);
        let loaded = &view.categories()[0];
        assert_eq!(loaded.category().id(), &first.category_id());
        assert_eq!(loaded.category().name(), "Linux System");
        assert_eq!(
            loaded.category().summary(),
            Some("Boot, devices, and desktop configuration.")
        );
        assert_eq!(loaded.category().order(), 1_000);
        assert_eq!(loaded.category().status(), CategoryStatus::Active);
        assert!(
            loaded
                .relative_directory()
                .starts_with("categories/linux-system--")
        );
        assert_eq!(loaded.descriptor_revision(), first.revision());
        assert_eq!(
            fs::metadata(workspace.path().join(loaded.relative_directory()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(
                workspace
                    .path()
                    .join(loaded.relative_directory())
                    .join("category.yaml")
            )
            .unwrap()
            .permissions()
            .mode()
                & 0o777,
            0o600
        );

        create_category(&mut view, state.path(), None, "Shell", None).unwrap();
        assert_eq!(view.categories().len(), 2);
        assert_eq!(view.categories()[1].category().order(), 2_000);
    }

    #[test]
    fn renames_category_without_moving_its_directory() {
        let (workspace, state, mut view) = initialized_workspace("category-rename");
        let created = create_category(&mut view, state.path(), None, "Linux System", None).unwrap();
        let original_directory = view.categories()[0].relative_directory().to_owned();
        let outcome = rename_category(
            &mut view,
            state.path(),
            None,
            created.category_id(),
            created.revision(),
            "Linux & Desktop".to_owned(),
        )
        .unwrap();
        assert!(outcome.changed());
        assert_ne!(outcome.revision(), created.revision());
        assert_eq!(view.categories()[0].category().name(), "Linux & Desktop");
        assert_eq!(
            view.categories()[0].relative_directory(),
            original_directory
        );
        assert!(workspace.path().join(original_directory).is_dir());

        let no_op = rename_category(
            &mut view,
            state.path(),
            None,
            created.category_id(),
            outcome.revision(),
            "Linux & Desktop".to_owned(),
        )
        .unwrap();
        assert!(!no_op.changed());
        assert_eq!(no_op.revision(), outcome.revision());
    }

    #[test]
    fn category_rename_rejects_invalid_and_stale_input() {
        let (_workspace, state, mut view) = initialized_workspace("category-rename-invalid");
        let created = create_category(&mut view, state.path(), None, "Linux System", None).unwrap();
        assert_eq!(
            rename_category(
                &mut view,
                state.path(),
                None,
                created.category_id(),
                created.revision(),
                " ".to_owned(),
            ),
            Err(CategoryUpdateError::InvalidName)
        );
        assert_eq!(
            rename_category(
                &mut view,
                state.path(),
                None,
                created.category_id(),
                Revision::from_str(&format!("rev:v1:{}", "11".repeat(32))).unwrap(),
                "Changed".to_owned(),
            ),
            Err(CategoryUpdateError::RevisionConflict)
        );
        assert_eq!(view.categories()[0].category().name(), "Linux System");
    }

    #[test]
    fn archives_and_restores_category_without_moving_content() {
        let (workspace, state, mut view) = initialized_workspace("category-archive");
        let created = create_category(&mut view, state.path(), None, "Linux System", None).unwrap();
        create_guide(
            &mut view,
            state.path(),
            None,
            created.category_id(),
            "Install a Tarball",
            "Install a downloaded archive.",
        )
        .unwrap();
        let original_directory = view.categories()[0].relative_directory().to_owned();
        let archived = set_category_archived(
            &mut view,
            state.path(),
            None,
            created.category_id(),
            created.revision(),
            true,
        )
        .unwrap();
        assert!(archived.changed());
        assert_eq!(
            view.categories()[0].category().status(),
            CategoryStatus::Archived
        );
        assert_eq!(
            view.categories()[0].relative_directory(),
            original_directory
        );
        assert_eq!(view.categories()[0].guides().len(), 1);
        assert!(workspace.path().join(&original_directory).is_dir());
        assert!(matches!(
            create_guide(
                &mut view,
                state.path(),
                None,
                created.category_id(),
                "Blocked",
                "Archived Categories refuse new Guides."
            ),
            Err(GuideCreateError::CategoryArchived)
        ));

        let restored = set_category_archived(
            &mut view,
            state.path(),
            None,
            created.category_id(),
            archived.revision(),
            false,
        )
        .unwrap();
        assert!(restored.changed());
        assert_eq!(
            view.categories()[0].category().status(),
            CategoryStatus::Active
        );
        assert_eq!(
            view.categories()[0].relative_directory(),
            original_directory
        );
        assert_eq!(view.categories()[0].guides().len(), 1);
    }

    #[test]
    fn refuses_to_archive_the_default_category() {
        let workspace = TestDirectory::new("default-category-archive");
        let state = TestDirectory::new("default-category-state");
        fs::write(
            workspace.path().join("rehow.yaml"),
            format!(
                "{MANIFEST}settings:\n  default_category_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\n"
            ),
        )
        .unwrap();
        let category = workspace.path().join("categories/linux-system--b6ba3a5551");
        fs::create_dir_all(&category).unwrap();
        fs::write(category.join("category.yaml"), CATEGORY).unwrap();
        let inspection = inspect_workspace(workspace.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(mut view) = inspection else {
            panic!("expected compatible Workspace");
        };
        let revision = view.categories()[0].descriptor_revision();
        assert_eq!(
            set_category_archived(
                &mut view,
                state.path(),
                None,
                CategoryId::from_str("0199a1c2-d334-7d17-bd9c-f8b6ba3a5551").unwrap(),
                revision,
                true,
            ),
            Err(CategoryUpdateError::DefaultCategory)
        );
        assert_eq!(
            view.categories()[0].category().status(),
            CategoryStatus::Active
        );
    }

    #[test]
    fn category_slug_falls_back_and_invalid_input_writes_nothing() {
        let id = CategoryId::from_str("0199a1c2-d334-7d17-bd9c-f8b6ba3a5551").unwrap();
        assert_eq!(
            category_directory_name("系统", &id),
            "categories/category--b6ba3a5551"
        );
        let (workspace, state, mut view) = initialized_workspace("category-invalid");
        assert!(matches!(
            create_category(&mut view, state.path(), None, " ", None),
            Err(CategoryCreateError::InvalidName)
        ));
        assert!(matches!(
            create_category(
                &mut view,
                state.path(),
                None,
                "Valid",
                Some(&"x".repeat(501))
            ),
            Err(CategoryCreateError::InvalidSummary)
        ));
        assert!(
            fs::read_dir(workspace.path().join("categories"))
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn creates_a_canonical_draft_guide_in_an_active_category() {
        let (workspace, state, mut view) = initialized_workspace("guide-create");
        let category = create_category(&mut view, state.path(), None, "Linux System", None)
            .unwrap()
            .category_id();
        let created = create_guide(
            &mut view,
            state.path(),
            None,
            category,
            "Install a Tarball",
            "Install a downloaded archive.",
        )
        .unwrap();

        let loaded_category = &view.categories()[0];
        let loaded = &loaded_category.guides()[0];
        assert_eq!(loaded.guide().id(), &created.guide_id());
        assert_eq!(loaded.guide().title(), "Install a Tarball");
        assert_eq!(loaded.guide().summary(), "Install a downloaded archive.");
        assert_eq!(loaded.guide().category_id(), &category);
        assert_eq!(loaded.guide().declared_status(), GuideStatus::Draft);
        assert_eq!(loaded.guide().body(), NEW_GUIDE_BODY);
        assert_eq!(loaded.revision(), created.revision());
        assert!(loaded.relative_path().starts_with(&format!(
            "{}/install-a-tarball--",
            loaded_category.relative_directory()
        )));
        assert_eq!(
            fs::metadata(workspace.path().join(loaded.relative_path()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn guide_creation_validates_before_writing() {
        let guide_id = GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap();
        assert_eq!(guide_file_name("系统", &guide_id), "guide--36c5735e10.md");
        let (workspace, state, mut view) = initialized_workspace("guide-create-invalid");
        let category = create_category(&mut view, state.path(), None, "Linux System", None)
            .unwrap()
            .category_id();
        let category_path = workspace
            .path()
            .join(view.categories()[0].relative_directory());
        assert!(matches!(
            create_guide(&mut view, state.path(), None, category, " ", "Summary"),
            Err(GuideCreateError::InvalidTitle)
        ));
        assert!(matches!(
            create_guide(
                &mut view,
                state.path(),
                None,
                category,
                "Valid",
                &"x".repeat(501)
            ),
            Err(GuideCreateError::InvalidSummary)
        ));
        assert!(matches!(
            create_guide(
                &mut view,
                state.path(),
                None,
                CategoryId::from_str("0199a1c2-d334-7d17-bd9c-f8b6ba3a5552").unwrap(),
                "Valid",
                "Summary"
            ),
            Err(GuideCreateError::CategoryNotFound)
        ));
        assert_eq!(fs::read_dir(category_path).unwrap().count(), 1);
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
    fn metadata_update_is_atomic_revision_checked_and_preserves_body() {
        let (workspace, state, mut view) = fixture("metadata-update", GUIDE);
        let expected = revision(&view);
        let outcome = update_guide(
            &mut view,
            state.path(),
            None,
            GuideId::from_str(GUIDE_ID).unwrap(),
            expected,
            GuideEdit::SetMetadata {
                title: "Install an Application Tarball".to_owned(),
                summary: "Extract and register a downloaded application archive.".to_owned(),
            },
        )
        .unwrap();
        assert!(outcome.changed());
        assert_ne!(outcome.revision(), expected);
        let guide = view.categories()[0].guides()[0].guide();
        assert_eq!(guide.title(), "Install an Application Tarball");
        assert_eq!(
            guide.summary(),
            "Extract and register a downloaded application archive."
        );
        assert_eq!(guide.body(), b"## Steps\n\nExtract the archive.\n");
        let bytes = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        assert!(
            bytes
                .windows(19)
                .any(|window| window == b"future_key: keep-me")
        );

        let no_op = update_guide(
            &mut view,
            state.path(),
            None,
            GuideId::from_str(GUIDE_ID).unwrap(),
            outcome.revision(),
            GuideEdit::SetMetadata {
                title: "Install an Application Tarball".to_owned(),
                summary: "Extract and register a downloaded application archive.".to_owned(),
            },
        )
        .unwrap();
        assert!(!no_op.changed());
        assert_eq!(no_op.revision(), outcome.revision());
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
            GuideEdit::SetMetadata {
                title: "Install a Verified Tarball".to_owned(),
                summary: "Updated summary".to_owned(),
            },
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

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
            AttachmentInput, CategoryMutation, GuideMutation, MutationFailureKind, MutationField,
            MutationOutcome, MutationStamp, plan_category_mutation, plan_guide_mutation,
        },
        verification::{VerificationFailure, verification_hash},
    },
    markdown::{GuideReadinessError, validate_tested_readiness},
    parsing::{parse_guide_source, parse_yaml_document},
    protocol::{
        AttachmentId, CategoryId, GuideId, OperationId, Revision, Snapshot, Timestamp,
        current_timestamp, new_attachment_id, new_category_id, new_guide_id, new_operation_id,
        new_workspace_id,
    },
    storage::{
        AttachmentRemovalStorageError, AttachmentSourceState, CancellableDocumentUpdateError,
        CategoryCreateStorageError, CategoryRemoveStorageError, DocumentCreateError,
        DocumentRemoveError, DocumentUpdateError, DocumentUpdateRequest, EmptyWorkspaceTarget,
        ManagedAttachmentCandidate as StorageManagedAttachmentCandidate, MarkdownImportSelection,
        MultiDocumentUpdateError, WorkspaceInitializeError, WorkspaceInspectError, WorkspaceView,
        category_inventory_snapshot, discover_managed_attachment_candidates,
        execute_attachment_import, execute_attachment_removal, execute_category_create,
        execute_category_document_update, execute_category_remove, execute_document_create,
        execute_document_move, execute_document_remove, execute_document_update,
        execute_document_update_cancellable, execute_managed_attachment_add,
        execute_multi_document_update, execute_workspace_initialize, managed_reveal_parent,
        open_initialized_workspace, preview_attachment_import, preview_attachment_removal,
        read_managed_regular, refresh_workspace,
    },
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, fs::MetadataExt, fs::OpenOptionsExt},
    },
    path::Path,
};
use unicode_normalization::UnicodeNormalization;

pub const GUIDE_BODY_INPUT_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceRefreshError {
    NotFound,
    NotDirectory,
    PermissionDenied,
    UnsafeStorage,
    ResourceLimit,
    ChangedDuringRead,
    Io,
}

pub fn refresh_open_workspace(workspace: &mut WorkspaceView) -> Result<(), WorkspaceRefreshError> {
    refresh_workspace(workspace).map_err(|error| match error {
        WorkspaceInspectError::NotFound => WorkspaceRefreshError::NotFound,
        WorkspaceInspectError::NotDirectory => WorkspaceRefreshError::NotDirectory,
        WorkspaceInspectError::PermissionDenied => WorkspaceRefreshError::PermissionDenied,
        WorkspaceInspectError::UnsafeRoot => WorkspaceRefreshError::UnsafeStorage,
        WorkspaceInspectError::ResourceLimit => WorkspaceRefreshError::ResourceLimit,
        WorkspaceInspectError::ChangedDuringRead => WorkspaceRefreshError::ChangedDuringRead,
        WorkspaceInspectError::Io => WorkspaceRefreshError::Io,
    })
}

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
    let expected_category_snapshot = category_inventory_snapshot(workspace);
    create_category_at_snapshot(
        workspace,
        state_home,
        runtime_directory,
        expected_category_snapshot,
        name,
        summary,
    )
}

pub fn create_category_at_snapshot(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    expected_category_snapshot: Snapshot,
    name: &str,
    summary: Option<&str>,
) -> Result<CategoryCreateOutcome, CategoryCreateError> {
    if category_inventory_snapshot(workspace) != expected_category_snapshot {
        return Err(CategoryCreateError::Conflict);
    }
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
        expected_category_snapshot,
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
    InvalidSummary,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryOrderRevision {
    category_id: CategoryId,
    revision: Revision,
}

impl CategoryOrderRevision {
    pub fn new(category_id: CategoryId, revision: Revision) -> Self {
        Self {
            category_id,
            revision,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CategoryReorderError {
    InvalidEffectSet,
    RevisionConflict,
    Cancelled,
    InvalidCategory,
    UnsafeRoundTrip,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    CommittedRefreshRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryReorderOutcome {
    changed: bool,
}

impl CategoryReorderOutcome {
    pub fn changed(self) -> bool {
        self.changed
    }
}

pub fn reorder_categories(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    ordered: &[CategoryOrderRevision],
) -> Result<CategoryReorderOutcome, CategoryReorderError> {
    let expected_category_snapshot = category_inventory_snapshot(workspace);
    reorder_categories_at_snapshot(
        workspace,
        state_home,
        runtime_directory,
        expected_category_snapshot,
        || false,
        ordered,
    )
}

pub fn reorder_categories_at_snapshot<C>(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    expected_category_snapshot: Snapshot,
    cancelled: C,
    ordered: &[CategoryOrderRevision],
) -> Result<CategoryReorderOutcome, CategoryReorderError>
where
    C: Fn() -> bool,
{
    if cancelled() {
        return Err(CategoryReorderError::Cancelled);
    }
    if category_inventory_snapshot(workspace) != expected_category_snapshot
        || !workspace.issues().is_empty()
        || ordered.len() != workspace.categories().len()
    {
        return Err(CategoryReorderError::InvalidEffectSet);
    }
    if ordered.is_empty() {
        return Ok(CategoryReorderOutcome { changed: false });
    }
    let mut seen = HashSet::new();
    seen.try_reserve(ordered.len())
        .map_err(|_| CategoryReorderError::ResourceLimit)?;
    let mut requests = Vec::new();
    requests
        .try_reserve_exact(ordered.len())
        .map_err(|_| CategoryReorderError::ResourceLimit)?;
    let mut expected_directories = Vec::new();
    expected_directories
        .try_reserve_exact(ordered.len())
        .map_err(|_| CategoryReorderError::ResourceLimit)?;
    for item in ordered {
        if !seen.insert(item.category_id) {
            return Err(CategoryReorderError::InvalidEffectSet);
        }
        let loaded = workspace
            .categories()
            .iter()
            .find(|category| category.category().id() == &item.category_id)
            .ok_or(CategoryReorderError::InvalidEffectSet)?;
        if loaded.descriptor_revision() != item.revision {
            return Err(CategoryReorderError::RevisionConflict);
        }
        requests.push(DocumentUpdateRequest::new(
            format!("{}/category.yaml", loaded.relative_directory()),
            item.revision,
        ));
        expected_directories.push(
            loaded
                .relative_directory()
                .strip_prefix("categories/")
                .ok_or(CategoryReorderError::InvalidCategory)?
                .to_owned(),
        );
    }

    let at = current_timestamp();
    let stamp = MutationStamp::from_trusted_time(at);
    let outcome = execute_multi_document_update(
        workspace,
        state_home,
        runtime_directory,
        expected_category_snapshot,
        &cancelled,
        "categories",
        &expected_directories,
        &requests,
        new_operation_id(),
        at,
        |documents| {
            if documents.len() != ordered.len() {
                return Err(CategoryPlanError::InvalidCategory);
            }
            let mut candidates = Vec::new();
            candidates
                .try_reserve_exact(documents.len())
                .map_err(|_| CategoryPlanError::ResourceLimit)?;
            for (index, (document, item)) in documents.iter().zip(ordered).enumerate() {
                if document.revision() != item.revision
                    || document.relative_path() != requests[index].relative_path()
                {
                    return Err(CategoryPlanError::InvalidCategory);
                }
                let parsed = parse_yaml_document(copy_bytes(document.bytes())?)
                    .map_err(|_| CategoryPlanError::InvalidCategory)?;
                let bound =
                    bind_category(parsed).map_err(|_| CategoryPlanError::InvalidCategory)?;
                if bound.record().id() != &item.category_id {
                    return Err(CategoryPlanError::InvalidCategory);
                }
                let ordinal = i64::try_from(index)
                    .ok()
                    .and_then(|value| value.checked_add(1))
                    .and_then(|value| value.checked_mul(1_000))
                    .ok_or(CategoryPlanError::ResourceLimit)?;
                let bytes = match plan_category_mutation(
                    &bound,
                    CategoryMutation::SetOrder(ordinal),
                    &stamp,
                )
                .map_err(map_category_plan_failure)?
                {
                    MutationOutcome::NoOp => copy_bytes(bound.source_bytes())?,
                    MutationOutcome::Candidate(candidate) => {
                        copy_bytes(candidate.bound().source_bytes())?
                    }
                };
                candidates.push(bytes);
            }
            Ok(candidates)
        },
    )
    .map_err(map_category_reorder_error)?;

    if outcome.changed() && refresh_workspace(workspace).is_err() {
        return Err(CategoryReorderError::CommittedRefreshRequired);
    }
    let matches =
        workspace.categories().len() == ordered.len()
            && workspace.categories().iter().zip(ordered).enumerate().all(
                |(index, (loaded, item))| {
                    let expected_order = i64::try_from(index)
                        .ok()
                        .and_then(|value| value.checked_add(1))
                        .and_then(|value| value.checked_mul(1_000));
                    loaded.category().id() == &item.category_id
                        && Some(loaded.category().order()) == expected_order
                },
            );
    if !matches {
        return Err(if outcome.changed() {
            CategoryReorderError::CommittedRefreshRequired
        } else {
            CategoryReorderError::InvalidEffectSet
        });
    }
    Ok(CategoryReorderOutcome {
        changed: outcome.changed(),
    })
}

fn map_category_reorder_error(
    error: MultiDocumentUpdateError<CategoryPlanError>,
) -> CategoryReorderError {
    match error {
        MultiDocumentUpdateError::Prepare(CategoryPlanError::InvalidCategory) => {
            CategoryReorderError::InvalidCategory
        }
        MultiDocumentUpdateError::Prepare(
            CategoryPlanError::InvalidName | CategoryPlanError::InvalidSummary,
        )
        | MultiDocumentUpdateError::InvalidPlan => CategoryReorderError::InvalidEffectSet,
        MultiDocumentUpdateError::Cancelled => CategoryReorderError::Cancelled,
        MultiDocumentUpdateError::Prepare(CategoryPlanError::UnsafeRoundTrip) => {
            CategoryReorderError::UnsafeRoundTrip
        }
        MultiDocumentUpdateError::Prepare(CategoryPlanError::ResourceLimit)
        | MultiDocumentUpdateError::TooLarge => CategoryReorderError::ResourceLimit,
        MultiDocumentUpdateError::Busy => CategoryReorderError::Busy,
        MultiDocumentUpdateError::Conflict => CategoryReorderError::RevisionConflict,
        MultiDocumentUpdateError::Unsafe => CategoryReorderError::UnsafeStorage,
        MultiDocumentUpdateError::Unsupported => CategoryReorderError::UnsupportedFilesystem,
        MultiDocumentUpdateError::Io => CategoryReorderError::Io,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CategoryRemoveError {
    NotFound,
    DefaultCategory,
    NotEmpty,
    Cancelled,
    RevisionConflict,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    RecoveryRequired { operation_id: OperationId },
    CommittedRefreshRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryRemoveOutcome;

pub fn remove_category(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    expected_revision: Revision,
) -> Result<CategoryRemoveOutcome, CategoryRemoveError> {
    let expected_category_snapshot = category_inventory_snapshot(workspace);
    remove_category_at_snapshot(
        workspace,
        state_home,
        runtime_directory,
        expected_category_snapshot,
        || false,
        category_id,
        expected_revision,
    )
}

pub fn remove_category_at_snapshot<C>(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    expected_category_snapshot: Snapshot,
    cancelled: C,
    category_id: CategoryId,
    expected_revision: Revision,
) -> Result<CategoryRemoveOutcome, CategoryRemoveError>
where
    C: Fn() -> bool,
{
    if cancelled() {
        return Err(CategoryRemoveError::Cancelled);
    }
    if !workspace.issues().is_empty()
        || category_inventory_snapshot(workspace) != expected_category_snapshot
    {
        return Err(CategoryRemoveError::RevisionConflict);
    }
    let loaded = workspace
        .categories()
        .iter()
        .find(|category| category.category().id() == &category_id)
        .ok_or(CategoryRemoveError::NotFound)?;
    if loaded.descriptor_revision() != expected_revision {
        return Err(CategoryRemoveError::RevisionConflict);
    }
    if workspace
        .manifest()
        .settings()
        .and_then(|settings| settings.default_category_id())
        == Some(&category_id)
    {
        return Err(CategoryRemoveError::DefaultCategory);
    }
    if !loaded.guides().is_empty() {
        return Err(CategoryRemoveError::NotEmpty);
    }
    let relative_directory = loaded.relative_directory().to_owned();
    execute_category_remove(
        workspace,
        state_home,
        runtime_directory,
        &relative_directory,
        expected_revision,
        expected_category_snapshot,
        &cancelled,
        new_operation_id(),
        current_timestamp(),
    )
    .map_err(map_category_remove_error)?;
    if refresh_workspace(workspace).is_err()
        || workspace
            .categories()
            .iter()
            .any(|category| category.category().id() == &category_id)
    {
        return Err(CategoryRemoveError::CommittedRefreshRequired);
    }
    Ok(CategoryRemoveOutcome)
}

fn map_category_remove_error(error: CategoryRemoveStorageError) -> CategoryRemoveError {
    match error {
        CategoryRemoveStorageError::Busy => CategoryRemoveError::Busy,
        CategoryRemoveStorageError::Cancelled => CategoryRemoveError::Cancelled,
        CategoryRemoveStorageError::Conflict => CategoryRemoveError::RevisionConflict,
        CategoryRemoveStorageError::NotEmpty => CategoryRemoveError::NotEmpty,
        CategoryRemoveStorageError::Unsafe => CategoryRemoveError::UnsafeStorage,
        CategoryRemoveStorageError::Unsupported => CategoryRemoveError::UnsupportedFilesystem,
        CategoryRemoveStorageError::TooLarge => CategoryRemoveError::ResourceLimit,
        CategoryRemoveStorageError::Io => CategoryRemoveError::Io,
        CategoryRemoveStorageError::RecoveryRequired { operation_id } => {
            CategoryRemoveError::RecoveryRequired { operation_id }
        }
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
    update_category_with_mutations(
        workspace,
        state_home,
        runtime_directory,
        category_id,
        expected_revision,
        [CategoryMutation::SetName(name)],
    )
}

pub fn update_category_details(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    expected_revision: Revision,
    name: String,
    summary: Option<String>,
) -> Result<CategoryUpdateOutcome, CategoryUpdateError> {
    update_category_with_mutations(
        workspace,
        state_home,
        runtime_directory,
        category_id,
        expected_revision,
        [
            CategoryMutation::SetName(name),
            CategoryMutation::SetSummary(summary),
        ],
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
    update_category_with_mutations(
        workspace,
        state_home,
        runtime_directory,
        category_id,
        expected_revision,
        [if archived {
            CategoryMutation::Archive
        } else {
            CategoryMutation::Restore
        }],
    )
}

#[allow(clippy::too_many_arguments)]
fn update_category_with_mutations<const N: usize>(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    expected_revision: Revision,
    mutations: [CategoryMutation; N],
) -> Result<CategoryUpdateOutcome, CategoryUpdateError> {
    let expected_category_snapshot = category_inventory_snapshot(workspace);
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
    let result = execute_category_document_update(
        workspace,
        state_home,
        runtime_directory,
        &relative_path,
        expected_revision,
        expected_category_snapshot,
        new_operation_id(),
        at,
        |bytes| {
            let parsed = parse_yaml_document(copy_bytes(bytes)?)
                .map_err(|_| CategoryPlanError::InvalidCategory)?;
            let bound = bind_category(parsed).map_err(|_| CategoryPlanError::InvalidCategory)?;
            if bound.record().id() != &category_id {
                return Err(CategoryPlanError::InvalidCategory);
            }
            let mut candidate_bytes = copy_bytes(bound.source_bytes())?;
            for mutation in mutations {
                let parsed = parse_yaml_document(candidate_bytes)
                    .map_err(|_| CategoryPlanError::InvalidCategory)?;
                let candidate_bound =
                    bind_category(parsed).map_err(|_| CategoryPlanError::InvalidCategory)?;
                candidate_bytes = match plan_category_mutation(&candidate_bound, mutation, &stamp)
                    .map_err(map_category_plan_failure)?
                {
                    MutationOutcome::NoOp => copy_bytes(candidate_bound.source_bytes())?,
                    MutationOutcome::Candidate(candidate) => {
                        copy_bytes(candidate.bound().source_bytes())?
                    }
                };
            }
            Ok(candidate_bytes)
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
    InvalidSummary,
    UnsafeRoundTrip,
    ResourceLimit,
}

impl From<PlanError> for CategoryPlanError {
    fn from(error: PlanError) -> Self {
        match error {
            PlanError::ResourceLimit => Self::ResourceLimit,
            PlanError::InvalidGuide
            | PlanError::InvalidEdit
            | PlanError::NotReady
            | PlanError::AttachmentLimit
            | PlanError::DuplicateAttachment => Self::InvalidCategory,
            PlanError::UnsafeRoundTrip => Self::UnsafeRoundTrip,
        }
    }
}

fn map_category_plan_failure(
    error: crate::domain::content::mutation::MutationFailure,
) -> CategoryPlanError {
    match error.kind() {
        MutationFailureKind::InvalidValue | MutationFailureKind::InvalidTransition => {
            match error.field() {
                MutationField::Summary => CategoryPlanError::InvalidSummary,
                _ => CategoryPlanError::InvalidName,
            }
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
        DocumentUpdateError::Prepare(CategoryPlanError::InvalidSummary) => {
            CategoryUpdateError::InvalidSummary
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
    let expected_category_revision = workspace
        .categories()
        .iter()
        .find(|loaded| loaded.category().id() == &category_id)
        .ok_or(GuideCreateError::CategoryNotFound)?
        .descriptor_revision();
    create_guide_at_revision(
        workspace,
        state_home,
        runtime_directory,
        category_id,
        expected_category_revision,
        title,
        summary,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_guide_at_revision(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    expected_category_revision: Revision,
    title: &str,
    summary: &str,
) -> Result<GuideCreateOutcome, GuideCreateError> {
    create_guide_with_body_at_revision(
        workspace,
        state_home,
        runtime_directory,
        category_id,
        expected_category_revision,
        title,
        summary,
        NEW_GUIDE_BODY,
    )
}

pub fn create_guide_with_body(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    title: &str,
    summary: &str,
    body: &[u8],
) -> Result<GuideCreateOutcome, GuideCreateError> {
    let expected_category_revision = workspace
        .categories()
        .iter()
        .find(|loaded| loaded.category().id() == &category_id)
        .ok_or(GuideCreateError::CategoryNotFound)?
        .descriptor_revision();
    create_guide_with_body_at_revision(
        workspace,
        state_home,
        runtime_directory,
        category_id,
        expected_category_revision,
        title,
        summary,
        body,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_guide_with_body_at_revision(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    category_id: CategoryId,
    expected_category_revision: Revision,
    title: &str,
    summary: &str,
    body: &[u8],
) -> Result<GuideCreateOutcome, GuideCreateError> {
    if body.len() > GUIDE_BODY_INPUT_LIMIT {
        return Err(GuideCreateError::ResourceLimit);
    }
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
    if category.descriptor_revision() != expected_category_revision {
        return Err(GuideCreateError::Conflict);
    }
    if category.category().status() != CategoryStatus::Active {
        return Err(GuideCreateError::CategoryArchived);
    }
    let relative_directory = category.relative_directory().to_owned();
    let guide_id = new_guide_id();
    let operation_id = new_operation_id();
    let timestamp = current_timestamp();
    let document = emit_new_guide(&guide_id, title, summary, &category_id, &timestamp, body)
        .map_err(|error| match (error.kind(), error.field()) {
            (EmitFailureKind::ResourceLimit, _) => GuideCreateError::ResourceLimit,
            (EmitFailureKind::InvalidValue, EmitField::Summary) => GuideCreateError::InvalidSummary,
            (EmitFailureKind::InvalidValue, EmitField::Title) => GuideCreateError::InvalidTitle,
            (EmitFailureKind::InvalidValue, _) | (EmitFailureKind::CandidateMismatch, _) => {
                GuideCreateError::Io
            }
        })?;
    let relative_path = format!("{relative_directory}/{}", guide_file_name(title, &guide_id));
    let category_descriptor_path = format!("{relative_directory}/category.yaml");
    let result = execute_document_create(
        workspace,
        state_home,
        runtime_directory,
        &category_descriptor_path,
        expected_category_revision,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownImportError {
    SourceUnavailable,
    SourceChanged,
    SameFolder,
    NothingToImport,
    Initialize(WorkspaceInitializationError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownImportPartialFailure {
    Category(CategoryCreateError),
    Guide(GuideCreateError),
}

pub struct MarkdownImportOutcome {
    workspace: Box<WorkspaceView>,
    imported: usize,
    total: usize,
    failure: Option<MarkdownImportPartialFailure>,
}

#[cfg(test)]
thread_local! {
    static MARKDOWN_IMPORT_GUIDE_FAILURE_AT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static MARKDOWN_IMPORT_AFTER_PREPARE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_markdown_import_guide_failure_at(index: usize) {
    MARKDOWN_IMPORT_GUIDE_FAILURE_AT.set(Some(index));
}

fn markdown_import_guide_failure(index: usize) -> bool {
    #[cfg(test)]
    {
        MARKDOWN_IMPORT_GUIDE_FAILURE_AT.with(|fault| {
            if fault.get() == Some(index) {
                fault.set(None);
                true
            } else {
                false
            }
        })
    }
    #[cfg(not(test))]
    {
        let _ = index;
        false
    }
}

#[cfg(test)]
fn set_markdown_import_after_prepare_hook(hook: impl FnOnce() + 'static) {
    MARKDOWN_IMPORT_AFTER_PREPARE_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_markdown_import_after_prepare_hook() {
    MARKDOWN_IMPORT_AFTER_PREPARE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

impl MarkdownImportOutcome {
    pub fn into_workspace(self) -> Box<WorkspaceView> {
        self.workspace
    }

    pub fn imported(&self) -> usize {
        self.imported
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn failure(&self) -> Option<MarkdownImportPartialFailure> {
        self.failure
    }

    pub fn complete(&self) -> bool {
        self.failure.is_none() && self.imported == self.total
    }
}

#[allow(clippy::too_many_arguments)]
pub fn import_markdown_collection(
    selection: &MarkdownImportSelection,
    destination: &EmptyWorkspaceTarget,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_name: &str,
    category_name: &str,
) -> Result<MarkdownImportOutcome, MarkdownImportError> {
    if selection
        .destination_is_within_source(destination)
        .map_err(|_| MarkdownImportError::SourceUnavailable)?
    {
        return Err(MarkdownImportError::SameFolder);
    }
    let prepared = selection.prepare().map_err(|error| match error {
        WorkspaceInspectError::ChangedDuringRead => MarkdownImportError::SourceChanged,
        _ => MarkdownImportError::SourceUnavailable,
    })?;
    if prepared.guides().is_empty() {
        return Err(MarkdownImportError::NothingToImport);
    }
    #[cfg(test)]
    run_markdown_import_after_prepare_hook();
    if selection
        .destination_is_within_source(destination)
        .map_err(|_| MarkdownImportError::SourceUnavailable)?
    {
        return Err(MarkdownImportError::SameFolder);
    }
    let total = prepared.guides().len();
    let mut workspace =
        initialize_workspace(destination, state_home, runtime_directory, workspace_name)
            .map_err(MarkdownImportError::Initialize)?;
    let category_id = match create_category(
        &mut workspace,
        state_home,
        runtime_directory,
        category_name,
        None,
    ) {
        Ok(outcome) => outcome.category_id(),
        Err(
            error @ CategoryCreateError::CommittedRefreshRequired {
                category_id,
                revision,
            },
        ) => {
            if refresh_workspace(&mut workspace).is_ok()
                && workspace.categories().iter().any(|loaded| {
                    loaded.category().id() == &category_id
                        && loaded.descriptor_revision() == revision
                })
            {
                category_id
            } else {
                return Ok(MarkdownImportOutcome {
                    workspace,
                    imported: 0,
                    total,
                    failure: Some(MarkdownImportPartialFailure::Category(error)),
                });
            }
        }
        Err(error) => {
            let _ = refresh_workspace(&mut workspace);
            return Ok(MarkdownImportOutcome {
                workspace,
                imported: 0,
                total,
                failure: Some(MarkdownImportPartialFailure::Category(error)),
            });
        }
    };
    let mut imported = 0usize;
    let mut failure = None;
    for (index, guide) in prepared.guides().iter().enumerate() {
        if markdown_import_guide_failure(index) {
            failure = Some(MarkdownImportPartialFailure::Guide(GuideCreateError::Io));
            break;
        }
        let summary = markdown_import_summary(guide.display_path());
        match create_guide_with_body(
            &mut workspace,
            state_home,
            runtime_directory,
            category_id,
            guide.title(),
            &summary,
            guide.body(),
        ) {
            Ok(_) => imported += 1,
            Err(error @ GuideCreateError::CommittedRefreshRequired { guide_id, revision }) => {
                imported += 1;
                if refresh_workspace(&mut workspace).is_err()
                    || !workspace.categories().iter().any(|category| {
                        category.guides().iter().any(|loaded| {
                            loaded.guide().id() == &guide_id && loaded.revision() == revision
                        })
                    })
                {
                    failure = Some(MarkdownImportPartialFailure::Guide(error));
                    break;
                }
            }
            Err(error) => {
                failure = Some(MarkdownImportPartialFailure::Guide(error));
                break;
            }
        }
    }
    if failure.is_some() {
        let _ = refresh_workspace(&mut workspace);
    }
    Ok(MarkdownImportOutcome {
        workspace,
        imported,
        total,
        failure,
    })
}

fn markdown_import_summary(display_path: &str) -> String {
    const PREFIX: &str = "Imported from ";
    let available = 500usize.saturating_sub(PREFIX.chars().count());
    let path = display_path.chars().take(available).collect::<String>();
    format!("{PREFIX}{path}")
}

pub enum GuideEdit {
    SetTitle(String),
    SetSummary(String),
    SetMetadata {
        title: String,
        summary: String,
    },
    SetDetails {
        title: String,
        summary: String,
        tags: Vec<String>,
        aliases: Vec<String>,
    },
    ReplaceTags(Vec<String>),
    ReplaceAliases(Vec<String>),
    SetContent {
        title: String,
        summary: String,
        tags: Vec<String>,
        aliases: Vec<String>,
        body: Vec<u8>,
    },
    ReplaceBody(Vec<u8>),
    UpdateAttachmentTitle {
        attachment_id: AttachmentId,
        title: String,
    },
    MarkStale,
    Archive,
    Restore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuideUpdateError {
    NotFound,
    DestinationCategoryNotFound,
    DestinationCategoryArchived,
    RevisionConflict,
    InvalidGuide,
    InvalidEdit,
    NotReady,
    UnsafeRoundTrip,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    CommittedRefreshRequired { revision: Revision },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuideTestedApprovalError {
    AlreadyTested,
    Cancelled,
    Update(GuideUpdateError),
}

impl From<GuideUpdateError> for GuideTestedApprovalError {
    fn from(error: GuideUpdateError) -> Self {
        Self::Update(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuideRemoveError {
    NotFound,
    HasAttachments,
    RevisionConflict,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    CommittedRefreshRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuideRemoveOutcome;

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

pub fn set_guide_archived(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    archived: bool,
) -> Result<GuideUpdateOutcome, GuideUpdateError> {
    update_guide(
        workspace,
        state_home,
        runtime_directory,
        guide_id,
        expected_revision,
        if archived {
            GuideEdit::Archive
        } else {
            GuideEdit::Restore
        },
    )
}

pub fn confirm_and_approve_guide_tested<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    parent: &W,
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
) -> Result<GuideUpdateOutcome, GuideUpdateError> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or(GuideUpdateError::NotFound)?;
    if loaded.revision() != expected_revision {
        return Err(GuideUpdateError::RevisionConflict);
    }
    if loaded.effective_status() == crate::domain::GuideStatus::Archived {
        return Err(GuideUpdateError::InvalidEdit);
    }
    let body =
        core::str::from_utf8(loaded.guide().body()).map_err(|_| GuideUpdateError::InvalidGuide)?;
    validate_tested_readiness(body).map_err(|error| match error {
        GuideReadinessError::MissingSteps
        | GuideReadinessError::EmptySteps
        | GuideReadinessError::DuplicateReservedHeading => GuideUpdateError::NotReady,
        GuideReadinessError::ParseLimit => GuideUpdateError::ResourceLimit,
    })?;
    let confirmed = rfd::MessageDialog::new()
        .set_parent(parent)
        .set_level(rfd::MessageLevel::Info)
        .set_title("Mark Guide Tested?")
        .set_description(guide_tested_confirmation(
            loaded.guide().title(),
            &guide_id.to_string(),
        ))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        return Ok(GuideUpdateOutcome {
            revision: expected_revision,
            changed: false,
        });
    }
    approve_guide_tested_with_context(
        workspace,
        state_home,
        runtime_directory,
        guide_id,
        expected_revision,
        new_operation_id(),
        current_timestamp(),
    )
}

pub fn validate_guide_tested_approval(
    workspace: &WorkspaceView,
    guide_id: GuideId,
    expected_revision: Revision,
) -> Result<(), GuideTestedApprovalError> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or(GuideUpdateError::NotFound)?;
    if loaded.revision() != expected_revision {
        return Err(GuideUpdateError::RevisionConflict.into());
    }
    if loaded.effective_status() == crate::domain::GuideStatus::Archived {
        return Err(GuideUpdateError::InvalidEdit.into());
    }
    if loaded.effective_status() == crate::domain::GuideStatus::Tested {
        return Err(GuideTestedApprovalError::AlreadyTested);
    }
    let body =
        core::str::from_utf8(loaded.guide().body()).map_err(|_| GuideUpdateError::InvalidGuide)?;
    validate_tested_readiness(body).map_err(|error| match error {
        GuideReadinessError::MissingSteps
        | GuideReadinessError::EmptySteps
        | GuideReadinessError::DuplicateReservedHeading => {
            GuideTestedApprovalError::Update(GuideUpdateError::NotReady)
        }
        GuideReadinessError::ParseLimit => {
            GuideTestedApprovalError::Update(GuideUpdateError::ResourceLimit)
        }
    })
}

pub fn approve_guide_tested_after_native_confirmation<C>(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    cancelled: C,
) -> Result<GuideUpdateOutcome, GuideTestedApprovalError>
where
    C: Fn() -> bool,
{
    if cancelled() {
        return Err(GuideTestedApprovalError::Cancelled);
    }
    validate_guide_tested_approval(workspace, guide_id, expected_revision)?;
    approve_guide_tested_with_context_and_cancellation(
        workspace,
        state_home,
        runtime_directory,
        guide_id,
        expected_revision,
        new_operation_id(),
        current_timestamp(),
        &cancelled,
    )
}

fn guide_tested_confirmation(title: &str, guide_id: &str) -> String {
    let safe_title = title
        .chars()
        .map(|character| {
            let value = character as u32;
            if character.is_control()
                || matches!(value, 0x061c | 0x200e | 0x200f | 0x2028..=0x202e | 0x2066..=0x2069 | 0xfeff)
                || matches!(value, 0xfdd0..=0xfdef)
                || matches!(value & 0xffff, 0xfffe | 0xffff)
            {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect::<String>();
    format!(
        "Confirm that you tried these instructions successfully.\n\nTitle: {safe_title}\nID: {guide_id}\n\nRehow will bind Tested status to the exact current content."
    )
}

#[allow(clippy::too_many_arguments)]
fn approve_guide_tested_with_context(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    operation_id: OperationId,
    at: Timestamp,
) -> Result<GuideUpdateOutcome, GuideUpdateError> {
    match approve_guide_tested_with_context_and_cancellation(
        workspace,
        state_home,
        runtime_directory,
        guide_id,
        expected_revision,
        operation_id,
        at,
        &|| false,
    ) {
        Ok(outcome) => Ok(outcome),
        Err(GuideTestedApprovalError::Update(error)) => Err(error),
        Err(GuideTestedApprovalError::AlreadyTested) => unreachable!("mutation owns no preview"),
        Err(GuideTestedApprovalError::Cancelled) => {
            unreachable!("the desktop transition never cancels after confirmation")
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn approve_guide_tested_with_context_and_cancellation<C>(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    operation_id: OperationId,
    at: Timestamp,
    cancelled: &C,
) -> Result<GuideUpdateOutcome, GuideTestedApprovalError>
where
    C: Fn() -> bool,
{
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides().iter().map(move |guide| (category, guide)))
        .find(|(_, guide)| guide.guide().id() == &guide_id)
        .ok_or(GuideUpdateError::NotFound)?;
    if loaded.1.revision() != expected_revision {
        return Err(GuideUpdateError::RevisionConflict.into());
    }
    let relative_path = loaded.1.relative_path().to_owned();
    let category_id = *loaded.0.category().id();
    let stamp = MutationStamp::from_trusted_time(at);
    let outcome = execute_document_update_cancellable(
        workspace,
        state_home,
        runtime_directory,
        &relative_path,
        expected_revision,
        operation_id,
        at,
        cancelled,
        |bytes| {
            let parsed =
                parse_guide_source(copy_bytes(bytes)?).map_err(|_| PlanError::InvalidGuide)?;
            let bound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
            if bound.record().id() != &guide_id || bound.record().category_id() != &category_id {
                return Err(PlanError::InvalidGuide);
            }
            let body =
                core::str::from_utf8(bound.record().body()).map_err(|_| PlanError::InvalidGuide)?;
            validate_tested_readiness(body).map_err(|error| match error {
                GuideReadinessError::MissingSteps
                | GuideReadinessError::EmptySteps
                | GuideReadinessError::DuplicateReservedHeading => PlanError::NotReady,
                GuideReadinessError::ParseLimit => PlanError::ResourceLimit,
            })?;
            let hash = verification_hash(&bound).map_err(|error| match error {
                VerificationFailure::Invariant => PlanError::InvalidGuide,
                VerificationFailure::ResourceLimit => PlanError::ResourceLimit,
            })?;
            apply_guide_mutation(&bound, GuideMutation::MarkTested(hash), &stamp)
        },
    )
    .map_err(map_cancellable_update_error)?;
    if outcome.changed() && refresh_workspace(workspace).is_err() {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: outcome.revision(),
        }
        .into());
    }
    Ok(GuideUpdateOutcome {
        revision: outcome.revision(),
        changed: outcome.changed(),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentImportOutcome {
    Cancelled,
    Imported {
        attachment_id: AttachmentId,
        revision: Revision,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentImportError {
    NotFound,
    RevisionConflict,
    GuideArchived,
    AttachmentLimit,
    DuplicateAttachment,
    InvalidSource,
    SourceTooLarge,
    SourceChanged,
    UnsafeRoundTrip,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    CommittedRefreshRequired {
        attachment_id: AttachmentId,
        revision: Revision,
    },
}

pub fn select_and_import_attachment<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    parent: &W,
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
) -> Result<AttachmentImportOutcome, AttachmentImportError> {
    let (category, loaded) = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides().iter().map(move |guide| (category, guide)))
        .find(|(_, guide)| guide.guide().id() == &guide_id)
        .ok_or(AttachmentImportError::NotFound)?;
    if loaded.revision() != expected_revision {
        return Err(AttachmentImportError::RevisionConflict);
    }
    if loaded.effective_status() == crate::domain::GuideStatus::Archived {
        return Err(AttachmentImportError::GuideArchived);
    }
    if loaded
        .guide()
        .attachments()
        .is_some_and(|attachments| attachments.len() >= 256)
    {
        return Err(AttachmentImportError::AttachmentLimit);
    }
    let guide_title = loaded.guide().title().to_owned();
    let category_name = category.category().name().to_owned();
    let category_id = *category.category().id();
    let guide_relative_path = loaded.relative_path().to_owned();
    let Some(path) = rfd::FileDialog::new()
        .set_parent(parent)
        .set_title("Select an Attachment")
        .pick_file()
    else {
        return Ok(AttachmentImportOutcome::Cancelled);
    };
    let filename = sanitized_attachment_filename(&path);
    let mut source = open_attachment_source(&path)?;
    let source_plan = inspect_attachment_source(&mut source)?;
    let attachment_id = new_attachment_id();
    let operation_id = new_operation_id();
    let at = current_timestamp();
    let stamp = MutationStamp::from_trusted_time(at);
    let preview = preview_attachment_import(
        workspace,
        state_home,
        runtime_directory,
        &guide_relative_path,
        guide_id,
        expected_revision,
        &filename,
        |bytes, attachment_path| {
            prepare_attachment_candidate(
                bytes,
                guide_id,
                category_id,
                attachment_id,
                &filename,
                attachment_path,
                source_plan.sha256,
                source_plan.size,
                &stamp,
            )
        },
    )
    .map_err(map_attachment_import_error)?;
    let confirmed = rfd::MessageDialog::new()
        .set_parent(parent)
        .set_level(rfd::MessageLevel::Info)
        .set_title("Add Attachment?")
        .set_description(format!(
            "Guide: {}\nCategory: {}\nGuide ID: {guide_id}\nDestination: {}\nSize: {}\n\nRehow will copy this file into the Workspace as inert content. It will not be opened or executed.",
            safe_dialog_text(&guide_title),
            safe_dialog_text(&category_name),
            preview.relative_path(),
            display_byte_size(source_plan.size)
        ))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        return Ok(AttachmentImportOutcome::Cancelled);
    }
    execute_approved_attachment_import(
        workspace,
        state_home,
        runtime_directory,
        guide_id,
        expected_revision,
        filename,
        &mut source,
        source_plan,
        preview.relative_path().to_owned(),
        preview.revision(),
        attachment_id,
        operation_id,
        at,
    )
}

#[derive(Clone, Copy)]
struct AttachmentSourcePlan {
    state: AttachmentSourceState,
    object_hash: [u8; 32],
    sha256: [u8; 32],
    size: u64,
}

fn open_attachment_source(path: &Path) -> Result<File, AttachmentImportError> {
    const IMPORT_LIMIT: u64 = 32 * 1024 * 1024;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| match error.raw_os_error() {
            Some(libc::ELOOP) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
                AttachmentImportError::InvalidSource
            }
            _ => AttachmentImportError::Io,
        })?;
    let metadata = file.metadata().map_err(|_| AttachmentImportError::Io)?;
    if !metadata.file_type().is_file() || metadata.nlink() != 1 {
        return Err(AttachmentImportError::InvalidSource);
    }
    if metadata.len() > IMPORT_LIMIT {
        return Err(AttachmentImportError::SourceTooLarge);
    }
    Ok(file)
}

fn inspect_attachment_source(
    source: &mut File,
) -> Result<AttachmentSourcePlan, AttachmentImportError> {
    use sha2::Digest;

    const IMPORT_LIMIT: u64 = 32 * 1024 * 1024;
    let before = source.metadata().map_err(|_| AttachmentImportError::Io)?;
    let state = AttachmentSourceState::capture(&before);
    if !before.file_type().is_file() || before.nlink() != 1 {
        return Err(AttachmentImportError::InvalidSource);
    }
    if state.size() > IMPORT_LIMIT {
        return Err(AttachmentImportError::SourceTooLarge);
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|_| AttachmentImportError::Io)?;
    let mut object = blake3::Hasher::new();
    let mut sha256 = sha2::Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|_| AttachmentImportError::Io)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or(AttachmentImportError::SourceTooLarge)?;
        if size > IMPORT_LIMIT {
            return Err(AttachmentImportError::SourceTooLarge);
        }
        object.update(&buffer[..read]);
        sha256.update(&buffer[..read]);
    }
    let after = source.metadata().map_err(|_| AttachmentImportError::Io)?;
    if !state.matches(&after) || size != state.size() {
        return Err(AttachmentImportError::SourceChanged);
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|_| AttachmentImportError::Io)?;
    Ok(AttachmentSourcePlan {
        state,
        object_hash: *object.finalize().as_bytes(),
        sha256: sha256.finalize().into(),
        size,
    })
}

fn safe_dialog_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            let scalar = character as u32;
            if character.is_control()
                || matches!(scalar, 0x061c | 0x200e | 0x200f | 0x2028..=0x202e | 0x2066..=0x2069 | 0xfeff)
                || matches!(scalar, 0xfdd0..=0xfdef)
                || matches!(scalar & 0xffff, 0xfffe | 0xffff)
            {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn sanitized_attachment_filename(path: &Path) -> String {
    let original = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let mut value = String::new();
    for character in original.nfc() {
        let scalar = character as u32;
        let unsafe_scalar = character.is_control()
            || matches!(character, '/' | '\\' | '\0')
            || is_unicode_16_invisible_or_format(character)
            || matches!(scalar, 0x2028..=0x2029)
            || matches!(scalar, 0xfdd0..=0xfdef)
            || matches!(scalar & 0xffff, 0xfffe | 0xffff);
        let replacement = if unsafe_scalar { '-' } else { character };
        if value.len() + replacement.len_utf8() > 200 {
            break;
        }
        value.push(replacement);
    }
    while value.contains("--") {
        value = value.replace("--", "-");
    }
    let value = value.trim_matches([' ', '-']).to_owned();
    if value.is_empty() || matches!(value.as_str(), "." | "..") {
        "attachment".to_owned()
    } else {
        value
    }
}

fn display_byte_size(size: u64) -> String {
    if size < 1_024 {
        format!("{size} bytes")
    } else if size < 1_024 * 1_024 {
        format!("{:.1} KiB", size as f64 / 1_024.0)
    } else {
        format!("{:.1} MiB", size as f64 / (1_024.0 * 1_024.0))
    }
}

#[allow(clippy::too_many_arguments)]
fn import_attachment_with_source(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    filename: String,
    source: &mut File,
    attachment_id: AttachmentId,
    operation_id: OperationId,
    at: Timestamp,
) -> Result<AttachmentImportOutcome, AttachmentImportError> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides().iter().map(move |guide| (category, guide)))
        .find(|(_, guide)| guide.guide().id() == &guide_id)
        .ok_or(AttachmentImportError::NotFound)?;
    if loaded.1.revision() != expected_revision {
        return Err(AttachmentImportError::RevisionConflict);
    }
    let guide_relative_path = loaded.1.relative_path().to_owned();
    let category_id = *loaded.0.category().id();
    let source_plan = inspect_attachment_source(source)?;
    let stamp = MutationStamp::from_trusted_time(at);
    let preview = preview_attachment_import(
        workspace,
        state_home,
        runtime_directory,
        &guide_relative_path,
        guide_id,
        expected_revision,
        &filename,
        |bytes, attachment_path| {
            prepare_attachment_candidate(
                bytes,
                guide_id,
                category_id,
                attachment_id,
                &filename,
                attachment_path,
                source_plan.sha256,
                source_plan.size,
                &stamp,
            )
        },
    )
    .map_err(map_attachment_import_error)?;
    execute_approved_attachment_import(
        workspace,
        state_home,
        runtime_directory,
        guide_id,
        expected_revision,
        filename,
        source,
        source_plan,
        preview.relative_path().to_owned(),
        preview.revision(),
        attachment_id,
        operation_id,
        at,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_approved_attachment_import(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    filename: String,
    source: &mut File,
    source_plan: AttachmentSourcePlan,
    approved_relative_path: String,
    approved_revision: Revision,
    attachment_id: AttachmentId,
    operation_id: OperationId,
    at: Timestamp,
) -> Result<AttachmentImportOutcome, AttachmentImportError> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides().iter().map(move |guide| (category, guide)))
        .find(|(_, guide)| guide.guide().id() == &guide_id)
        .ok_or(AttachmentImportError::NotFound)?;
    if loaded.1.revision() != expected_revision {
        return Err(AttachmentImportError::RevisionConflict);
    }
    let relative_path = loaded.1.relative_path().to_owned();
    let category_id = *loaded.0.category().id();
    let stamp = MutationStamp::from_trusted_time(at);
    let outcome = execute_attachment_import(
        workspace,
        state_home,
        runtime_directory,
        &relative_path,
        guide_id,
        expected_revision,
        operation_id,
        at,
        &filename,
        source,
        &source_plan.state,
        &approved_relative_path,
        approved_revision,
        source_plan.object_hash,
        source_plan.sha256,
        source_plan.size,
        |bytes, attachment_path, sha256, size| {
            prepare_attachment_candidate(
                bytes,
                guide_id,
                category_id,
                attachment_id,
                &filename,
                attachment_path,
                sha256,
                size,
                &stamp,
            )
        },
    )
    .map_err(map_attachment_import_error)?;
    if refresh_workspace(workspace).is_err()
        || !workspace
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .any(|guide| {
                guide.guide().id() == &guide_id
                    && guide.revision() == outcome.revision()
                    && guide.guide().attachments().is_some_and(|attachments| {
                        attachments
                            .iter()
                            .any(|attachment| attachment.id() == &attachment_id)
                    })
            })
    {
        return Err(AttachmentImportError::CommittedRefreshRequired {
            attachment_id,
            revision: outcome.revision(),
        });
    }
    Ok(AttachmentImportOutcome::Imported {
        attachment_id,
        revision: outcome.revision(),
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_attachment_candidate(
    bytes: &[u8],
    guide_id: GuideId,
    category_id: CategoryId,
    attachment_id: AttachmentId,
    filename: &str,
    attachment_path: &str,
    sha256: [u8; 32],
    size: u64,
    stamp: &MutationStamp,
) -> Result<Box<[u8]>, PlanError> {
    let parsed = parse_guide_source(copy_bytes(bytes)?).map_err(|_| PlanError::InvalidGuide)?;
    let bound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
    if bound.record().id() != &guide_id || bound.record().category_id() != &category_id {
        return Err(PlanError::InvalidGuide);
    }
    let attachments = bound.record().attachments().unwrap_or_default();
    if attachments.len() >= 256 {
        return Err(PlanError::AttachmentLimit);
    }
    let sha256 = sha256
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if attachments
        .iter()
        .any(|attachment| attachment.sha256() == sha256)
    {
        return Err(PlanError::DuplicateAttachment);
    }
    apply_guide_mutation(
        &bound,
        GuideMutation::AddAttachment(AttachmentInput {
            id: attachment_id,
            path: attachment_path.to_owned(),
            title: filename.to_owned(),
            sha256,
            size_bytes: size,
        }),
        stamp,
    )
}

fn map_attachment_import_error(
    error: MultiDocumentUpdateError<PlanError>,
) -> AttachmentImportError {
    match error {
        MultiDocumentUpdateError::Prepare(PlanError::DuplicateAttachment) => {
            AttachmentImportError::DuplicateAttachment
        }
        MultiDocumentUpdateError::Prepare(PlanError::AttachmentLimit) => {
            AttachmentImportError::AttachmentLimit
        }
        MultiDocumentUpdateError::Prepare(PlanError::UnsafeRoundTrip) => {
            AttachmentImportError::UnsafeRoundTrip
        }
        MultiDocumentUpdateError::Prepare(PlanError::ResourceLimit)
        | MultiDocumentUpdateError::TooLarge => AttachmentImportError::ResourceLimit,
        MultiDocumentUpdateError::Prepare(
            PlanError::InvalidGuide | PlanError::InvalidEdit | PlanError::NotReady,
        )
        | MultiDocumentUpdateError::InvalidPlan => AttachmentImportError::InvalidSource,
        MultiDocumentUpdateError::Busy => AttachmentImportError::Busy,
        MultiDocumentUpdateError::Conflict | MultiDocumentUpdateError::Cancelled => {
            AttachmentImportError::SourceChanged
        }
        MultiDocumentUpdateError::Unsafe => AttachmentImportError::UnsafeStorage,
        MultiDocumentUpdateError::Unsupported => AttachmentImportError::UnsupportedFilesystem,
        MultiDocumentUpdateError::Io => AttachmentImportError::Io,
    }
}

pub struct ManagedAttachmentCandidate {
    inner: StorageManagedAttachmentCandidate,
}

const ATTACHMENT_TEXT_PREVIEW_LIMIT: usize = 1024 * 1024;
const ATTACHMENT_EXPORT_LIMIT: usize = 64 * 1024 * 1024;
const GUIDE_SOURCE_PREVIEW_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Eq, PartialEq)]
pub struct AttachmentTextPreview {
    title: String,
    text: String,
    size_bytes: u64,
    sanitized: bool,
}

impl std::fmt::Debug for AttachmentTextPreview {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachmentTextPreview")
            .field("size_bytes", &self.size_bytes)
            .field("sanitized", &self.sanitized)
            .finish_non_exhaustive()
    }
}

impl AttachmentTextPreview {
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn sanitized(&self) -> bool {
        self.sanitized
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentTextPreviewError {
    GuideNotFound,
    AttachmentNotFound,
    RevisionConflict,
    AttachmentChanged,
    TooLarge,
    Binary,
    UnsafeStorage,
    ResourceLimit,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentExportOutcome {
    Cancelled,
    Exported { size_bytes: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentExportError {
    GuideNotFound,
    AttachmentNotFound,
    RevisionConflict,
    AttachmentChanged,
    DestinationExists,
    UnsafeDestination,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    ExportedDurabilityUnknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentRevealError {
    GuideNotFound,
    AttachmentNotFound,
    RevisionConflict,
    AttachmentChanged,
    UnsafeStorage,
    ResourceLimit,
    HandlerUnavailable,
    Io,
}

#[cfg(test)]
thread_local! {
    static ATTACHMENT_PREVIEW_AFTER_READ_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_attachment_preview_after_read_hook(hook: impl FnOnce() + 'static) {
    ATTACHMENT_PREVIEW_AFTER_READ_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_attachment_preview_after_read_hook() {
    #[cfg(test)]
    ATTACHMENT_PREVIEW_AFTER_READ_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

pub fn preview_attachment_text(
    workspace: &WorkspaceView,
    guide_id: GuideId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
) -> Result<AttachmentTextPreview, AttachmentTextPreviewError> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or(AttachmentTextPreviewError::GuideNotFound)?;
    if loaded.revision() != expected_revision {
        return Err(AttachmentTextPreviewError::RevisionConflict);
    }
    let attachment = loaded
        .guide()
        .attachments()
        .unwrap_or_default()
        .iter()
        .find(|attachment| attachment.id() == &attachment_id)
        .ok_or(AttachmentTextPreviewError::AttachmentNotFound)?;
    let expected_prefix = format!("attachments/{guide_id}/");
    if !attachment.path().starts_with(&expected_prefix) {
        return Err(AttachmentTextPreviewError::UnsafeStorage);
    }
    verify_preview_guide_revision(workspace, loaded.relative_path(), expected_revision)?;
    let bytes = read_managed_regular(
        workspace,
        attachment.path(),
        ATTACHMENT_TEXT_PREVIEW_LIMIT,
        None,
    )
    .map_err(map_attachment_preview_read_error)?;
    if attachment
        .size_bytes()
        .is_some_and(|expected| expected != bytes.len() as u64)
    {
        return Err(AttachmentTextPreviewError::AttachmentChanged);
    }
    use sha2::Digest;
    let expected_digest =
        decode_sha256(attachment.sha256()).ok_or(AttachmentTextPreviewError::UnsafeStorage)?;
    if sha2::Sha256::digest(&bytes).as_slice() != expected_digest {
        return Err(AttachmentTextPreviewError::AttachmentChanged);
    }
    let source = std::str::from_utf8(&bytes).map_err(|_| AttachmentTextPreviewError::Binary)?;
    let (text, text_sanitized) = sanitize_attachment_preview(source)?;
    let (title, title_sanitized) = sanitize_attachment_preview(attachment.title())?;
    run_attachment_preview_after_read_hook();
    verify_preview_guide_revision(workspace, loaded.relative_path(), expected_revision)?;
    Ok(AttachmentTextPreview {
        title,
        text,
        size_bytes: bytes.len() as u64,
        sanitized: text_sanitized || title_sanitized,
    })
}

pub fn select_and_export_attachment<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    parent: &W,
    workspace: &WorkspaceView,
    guide_id: GuideId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
) -> Result<AttachmentExportOutcome, AttachmentExportError> {
    let loaded = find_export_attachment(workspace, guide_id, attachment_id, expected_revision)?;
    let suggested_name = sanitized_attachment_filename(Path::new(loaded.1.path()));
    let Some(destination) = rfd::FileDialog::new()
        .set_parent(parent)
        .set_title("Export Attachment")
        .set_file_name(suggested_name)
        .save_file()
    else {
        return Ok(AttachmentExportOutcome::Cancelled);
    };
    export_attachment_to_path(
        workspace,
        guide_id,
        attachment_id,
        expected_revision,
        &destination,
    )
}

fn export_attachment_to_path(
    workspace: &WorkspaceView,
    guide_id: GuideId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
    destination: &Path,
) -> Result<AttachmentExportOutcome, AttachmentExportError> {
    let (loaded, attachment) =
        find_export_attachment(workspace, guide_id, attachment_id, expected_revision)?;
    verify_export_guide_revision(workspace, loaded.relative_path(), expected_revision)?;
    let bytes = read_managed_regular(workspace, attachment.path(), ATTACHMENT_EXPORT_LIMIT, None)
        .map_err(map_attachment_export_read_error)?;
    if attachment
        .size_bytes()
        .is_some_and(|expected| expected != bytes.len() as u64)
    {
        return Err(AttachmentExportError::AttachmentChanged);
    }
    use sha2::Digest;
    let expected_digest =
        decode_sha256(attachment.sha256()).ok_or(AttachmentExportError::UnsafeStorage)?;
    if sha2::Sha256::digest(&bytes).as_slice() != expected_digest {
        return Err(AttachmentExportError::AttachmentChanged);
    }
    let (directory, leaf) = open_export_destination(destination)?;
    if export_leaf_exists(&directory, &leaf)? {
        return Err(AttachmentExportError::DestinationExists);
    }
    let mut output = create_unnamed_export(&directory)?;
    output
        .write_all(&bytes)
        .map_err(|_| AttachmentExportError::Io)?;
    output
        .sync_all()
        .map_err(|_| AttachmentExportError::UnsupportedFilesystem)?;
    run_attachment_preview_after_read_hook();
    verify_export_guide_revision(workspace, loaded.relative_path(), expected_revision)?;
    publish_export(&directory, &leaf, &output)?;
    // The name is already published and must never be removed on an ambiguous
    // durability failure; report that exact state instead.
    if unsafe { libc::fsync(directory.as_raw_fd()) } != 0 {
        return Err(AttachmentExportError::ExportedDurabilityUnknown);
    }
    Ok(AttachmentExportOutcome::Exported {
        size_bytes: bytes.len() as u64,
    })
}

fn find_export_attachment(
    workspace: &WorkspaceView,
    guide_id: GuideId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
) -> Result<
    (
        &crate::storage::LoadedGuide,
        &crate::domain::content::guide::Attachment,
    ),
    AttachmentExportError,
> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or(AttachmentExportError::GuideNotFound)?;
    if loaded.revision() != expected_revision {
        return Err(AttachmentExportError::RevisionConflict);
    }
    let attachment = loaded
        .guide()
        .attachments()
        .unwrap_or_default()
        .iter()
        .find(|attachment| attachment.id() == &attachment_id)
        .ok_or(AttachmentExportError::AttachmentNotFound)?;
    if !attachment
        .path()
        .starts_with(&format!("attachments/{guide_id}/"))
    {
        return Err(AttachmentExportError::UnsafeStorage);
    }
    Ok((loaded, attachment))
}

fn verify_export_guide_revision(
    workspace: &WorkspaceView,
    relative_path: &str,
    expected_revision: Revision,
) -> Result<(), AttachmentExportError> {
    read_managed_regular(
        workspace,
        relative_path,
        GUIDE_SOURCE_PREVIEW_LIMIT,
        Some(expected_revision),
    )
    .map(|_| ())
    .map_err(|error| match error {
        WorkspaceInspectError::ResourceLimit => AttachmentExportError::ResourceLimit,
        WorkspaceInspectError::UnsafeRoot => AttachmentExportError::UnsafeStorage,
        WorkspaceInspectError::ChangedDuringRead
        | WorkspaceInspectError::NotFound
        | WorkspaceInspectError::NotDirectory => AttachmentExportError::RevisionConflict,
        WorkspaceInspectError::PermissionDenied | WorkspaceInspectError::Io => {
            AttachmentExportError::Io
        }
    })
}

fn map_attachment_export_read_error(error: WorkspaceInspectError) -> AttachmentExportError {
    match error {
        WorkspaceInspectError::ResourceLimit => AttachmentExportError::ResourceLimit,
        WorkspaceInspectError::UnsafeRoot => AttachmentExportError::UnsafeStorage,
        WorkspaceInspectError::ChangedDuringRead
        | WorkspaceInspectError::NotFound
        | WorkspaceInspectError::NotDirectory => AttachmentExportError::AttachmentChanged,
        WorkspaceInspectError::PermissionDenied | WorkspaceInspectError::Io => {
            AttachmentExportError::Io
        }
    }
}

fn open_export_destination(
    destination: &Path,
) -> Result<(OwnedFd, std::ffi::CString), AttachmentExportError> {
    if !destination.is_absolute() {
        return Err(AttachmentExportError::UnsafeDestination);
    }
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(AttachmentExportError::UnsafeDestination)?;
    let leaf = destination
        .file_name()
        .filter(|leaf| *leaf != "." && *leaf != ".." && !leaf.is_empty())
        .ok_or(AttachmentExportError::UnsafeDestination)?;
    let parent = std::ffi::CString::new(parent.as_os_str().as_bytes())
        .map_err(|_| AttachmentExportError::UnsafeDestination)?;
    let leaf = std::ffi::CString::new(leaf.as_bytes())
        .map_err(|_| AttachmentExportError::UnsafeDestination)?;
    // SAFETY: an all-zero open_how is a valid baseline; all defined fields
    // used by this kernel ABI are assigned immediately below.
    let mut how: libc::open_how = unsafe { std::mem::zeroed() };
    how.flags = (libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) as u64;
    how.resolve = libc::RESOLVE_NO_SYMLINKS | libc::RESOLVE_NO_MAGICLINKS;
    // SAFETY: parent is NUL-terminated and `how` has the kernel ABI layout.
    let raw = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            libc::AT_FDCWD,
            parent.as_ptr(),
            &how,
            size_of::<libc::open_how>(),
        )
    } as i32;
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ENOSYS | libc::EINVAL) => Err(AttachmentExportError::UnsupportedFilesystem),
            Some(libc::ELOOP | libc::ENOTDIR) => Err(AttachmentExportError::UnsafeDestination),
            _ => Err(AttachmentExportError::Io),
        };
    }
    // SAFETY: successful openat2 returned a newly owned descriptor.
    Ok((unsafe { OwnedFd::from_raw_fd(raw) }, leaf))
}

fn export_leaf_exists(
    directory: &OwnedFd,
    leaf: &std::ffi::CStr,
) -> Result<bool, AttachmentExportError> {
    let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: directory, leaf, and output storage are valid.
    if unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            leaf.as_ptr(),
            status.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        return Ok(true);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ENOENT) => Ok(false),
        _ => Err(AttachmentExportError::Io),
    }
}

fn create_unnamed_export(directory: &OwnedFd) -> Result<File, AttachmentExportError> {
    // SAFETY: retained directory and static dot component are valid.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c".".as_ptr(),
            libc::O_WRONLY | libc::O_TMPFILE | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    };
    if raw < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EISDIR) => {
                Err(AttachmentExportError::UnsupportedFilesystem)
            }
            _ => Err(AttachmentExportError::Io),
        };
    }
    // SAFETY: successful openat returned a newly owned descriptor.
    let file = unsafe { File::from_raw_fd(raw) };
    if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
        return Err(AttachmentExportError::Io);
    }
    Ok(file)
}

fn publish_export(
    directory: &OwnedFd,
    leaf: &std::ffi::CStr,
    output: &File,
) -> Result<(), AttachmentExportError> {
    // SAFETY: output is an unnamed inode and the target leaf is retained.
    let mut result = unsafe {
        libc::linkat(
            output.as_raw_fd(),
            c"".as_ptr(),
            directory.as_raw_fd(),
            leaf.as_ptr(),
            libc::AT_EMPTY_PATH,
        )
    };
    if result != 0
        && matches!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM | libc::EINVAL | libc::ENOENT)
        )
    {
        let source = std::ffi::CString::new(format!("/proc/self/fd/{}", output.as_raw_fd()))
            .map_err(|_| AttachmentExportError::Io)?;
        // SAFETY: source names the still-open inode and target is retained.
        result = unsafe {
            libc::linkat(
                libc::AT_FDCWD,
                source.as_ptr(),
                directory.as_raw_fd(),
                leaf.as_ptr(),
                libc::AT_SYMLINK_FOLLOW,
            )
        };
    }
    if result == 0 {
        return Ok(());
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EEXIST) => Err(AttachmentExportError::DestinationExists),
        Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EPERM) => {
            Err(AttachmentExportError::UnsupportedFilesystem)
        }
        _ => Err(AttachmentExportError::Io),
    }
}

pub struct AttachmentRevealPlan {
    target: crate::storage::ManagedRevealParent,
}

impl std::fmt::Debug for AttachmentRevealPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachmentRevealPlan")
            .field("target", &"<directory capability>")
            .finish()
    }
}

pub fn prepare_attachment_reveal(
    workspace: &WorkspaceView,
    guide_id: GuideId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
) -> Result<AttachmentRevealPlan, AttachmentRevealError> {
    prepare_attachment_reveal_parent(workspace, guide_id, attachment_id, expected_revision)
        .map(|target| AttachmentRevealPlan { target })
}

pub fn execute_attachment_reveal(plan: AttachmentRevealPlan) -> Result<(), AttachmentRevealError> {
    reveal_directory_via_portal(&plan.target)
}

fn reveal_directory_via_portal(
    target: &crate::storage::ManagedRevealParent,
) -> Result<(), AttachmentRevealError> {
    use gio::{glib::prelude::ToVariant, prelude::UnixFDListExtManual};

    let descriptor = target
        .duplicate_descriptor()
        .map_err(map_attachment_reveal_storage_error)?;
    let descriptors = gio::UnixFDList::new();
    let handle = descriptors
        .append(descriptor)
        .map_err(|_| AttachmentRevealError::HandlerUnavailable)?;
    let options = gio::glib::VariantDict::new(None);
    options.insert("writable", false);
    options.insert("ask", false);
    let parameters = ("", gio::glib::variant::Handle(handle), options.end()).to_variant();
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
        .map_err(|_| AttachmentRevealError::HandlerUnavailable)?;
    connection
        .call_with_unix_fd_list_sync(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.OpenURI",
            "OpenFile",
            Some(&parameters),
            None::<&gio::glib::VariantTy>,
            gio::DBusCallFlags::NONE,
            15_000,
            Some(&descriptors),
            None::<&gio::Cancellable>,
        )
        .map(|_| ())
        .map_err(|_| AttachmentRevealError::HandlerUnavailable)
}

fn prepare_attachment_reveal_parent(
    workspace: &WorkspaceView,
    guide_id: GuideId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
) -> Result<crate::storage::ManagedRevealParent, AttachmentRevealError> {
    let (loaded, attachment) = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or(AttachmentRevealError::GuideNotFound)
        .and_then(|loaded| {
            if loaded.revision() != expected_revision {
                Err(AttachmentRevealError::RevisionConflict)
            } else {
                let attachment = loaded
                    .guide()
                    .attachments()
                    .unwrap_or_default()
                    .iter()
                    .find(|attachment| attachment.id() == &attachment_id)
                    .ok_or(AttachmentRevealError::AttachmentNotFound)?;
                Ok((loaded, attachment))
            }
        })?;
    if !attachment
        .path()
        .starts_with(&format!("attachments/{guide_id}/"))
    {
        return Err(AttachmentRevealError::UnsafeStorage);
    }
    verify_export_guide_revision(workspace, loaded.relative_path(), expected_revision)
        .map_err(map_attachment_reveal_export_error)?;
    let bytes = read_managed_regular(workspace, attachment.path(), ATTACHMENT_EXPORT_LIMIT, None)
        .map_err(map_attachment_reveal_storage_error)?;
    if attachment
        .size_bytes()
        .is_some_and(|expected| expected != bytes.len() as u64)
    {
        return Err(AttachmentRevealError::AttachmentChanged);
    }
    use sha2::Digest;
    let expected_digest =
        decode_sha256(attachment.sha256()).ok_or(AttachmentRevealError::UnsafeStorage)?;
    if sha2::Sha256::digest(&bytes).as_slice() != expected_digest {
        return Err(AttachmentRevealError::AttachmentChanged);
    }
    run_attachment_preview_after_read_hook();
    verify_export_guide_revision(workspace, loaded.relative_path(), expected_revision)
        .map_err(map_attachment_reveal_export_error)?;
    managed_reveal_parent(workspace, attachment.path()).map_err(map_attachment_reveal_storage_error)
}

fn map_attachment_reveal_export_error(error: AttachmentExportError) -> AttachmentRevealError {
    match error {
        AttachmentExportError::RevisionConflict => AttachmentRevealError::RevisionConflict,
        AttachmentExportError::AttachmentChanged => AttachmentRevealError::AttachmentChanged,
        AttachmentExportError::UnsafeStorage => AttachmentRevealError::UnsafeStorage,
        AttachmentExportError::ResourceLimit => AttachmentRevealError::ResourceLimit,
        AttachmentExportError::Io => AttachmentRevealError::Io,
        AttachmentExportError::GuideNotFound
        | AttachmentExportError::AttachmentNotFound
        | AttachmentExportError::DestinationExists
        | AttachmentExportError::UnsafeDestination
        | AttachmentExportError::UnsupportedFilesystem
        | AttachmentExportError::ExportedDurabilityUnknown => AttachmentRevealError::Io,
    }
}

fn map_attachment_reveal_storage_error(error: WorkspaceInspectError) -> AttachmentRevealError {
    match error {
        WorkspaceInspectError::ResourceLimit => AttachmentRevealError::ResourceLimit,
        WorkspaceInspectError::UnsafeRoot => AttachmentRevealError::UnsafeStorage,
        WorkspaceInspectError::ChangedDuringRead
        | WorkspaceInspectError::NotFound
        | WorkspaceInspectError::NotDirectory => AttachmentRevealError::AttachmentChanged,
        WorkspaceInspectError::PermissionDenied | WorkspaceInspectError::Io => {
            AttachmentRevealError::Io
        }
    }
}

fn verify_preview_guide_revision(
    workspace: &WorkspaceView,
    relative_path: &str,
    expected_revision: Revision,
) -> Result<(), AttachmentTextPreviewError> {
    read_managed_regular(
        workspace,
        relative_path,
        GUIDE_SOURCE_PREVIEW_LIMIT,
        Some(expected_revision),
    )
    .map(|_| ())
    .map_err(|error| match error {
        WorkspaceInspectError::ResourceLimit => AttachmentTextPreviewError::ResourceLimit,
        WorkspaceInspectError::UnsafeRoot => AttachmentTextPreviewError::UnsafeStorage,
        WorkspaceInspectError::ChangedDuringRead
        | WorkspaceInspectError::NotFound
        | WorkspaceInspectError::NotDirectory => AttachmentTextPreviewError::RevisionConflict,
        WorkspaceInspectError::PermissionDenied | WorkspaceInspectError::Io => {
            AttachmentTextPreviewError::Io
        }
    })
}

fn map_attachment_preview_read_error(error: WorkspaceInspectError) -> AttachmentTextPreviewError {
    match error {
        WorkspaceInspectError::ResourceLimit => AttachmentTextPreviewError::TooLarge,
        WorkspaceInspectError::UnsafeRoot => AttachmentTextPreviewError::UnsafeStorage,
        WorkspaceInspectError::ChangedDuringRead
        | WorkspaceInspectError::NotFound
        | WorkspaceInspectError::NotDirectory => AttachmentTextPreviewError::AttachmentChanged,
        WorkspaceInspectError::PermissionDenied | WorkspaceInspectError::Io => {
            AttachmentTextPreviewError::Io
        }
    }
}

fn sanitize_attachment_preview(source: &str) -> Result<(String, bool), AttachmentTextPreviewError> {
    let mut text = String::new();
    text.try_reserve(source.len())
        .map_err(|_| AttachmentTextPreviewError::ResourceLimit)?;
    let mut sanitized = false;
    for character in source.chars() {
        let value = character as u32;
        let unsafe_scalar = (character.is_control() && !matches!(character, '\n' | '\t'))
            || is_unicode_16_invisible_or_format(character)
            || matches!(value, 0x2028..=0x2029 | 0xfdd0..=0xfdef)
            || matches!(value & 0xffff, 0xfffe | 0xffff);
        let displayed = if unsafe_scalar {
            sanitized = true;
            '\u{fffd}'
        } else {
            character
        };
        if text.len().saturating_add(displayed.len_utf8()) > ATTACHMENT_TEXT_PREVIEW_LIMIT {
            return Err(AttachmentTextPreviewError::TooLarge);
        }
        text.try_reserve(displayed.len_utf8())
            .map_err(|_| AttachmentTextPreviewError::ResourceLimit)?;
        text.push(displayed);
    }
    Ok((text, sanitized))
}

fn is_unicode_16_invisible_or_format(character: char) -> bool {
    // Keep this in exact parity with the approval helper's Unicode 16
    // Format/Default_Ignorable_Code_Point policy and the renderer validator.
    matches!(
        character,
        '\u{00ad}'
            | '\u{034f}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061c}'
            | '\u{06dd}'
            | '\u{070f}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08e2}'
            | '\u{115f}'..='\u{1160}'
            | '\u{17b4}'..='\u{17b5}'
            | '\u{180b}'..='\u{180f}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{3164}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{feff}'
            | '\u{ffa0}'
            | '\u{fff0}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e0fff}'
    )
}

impl std::fmt::Debug for ManagedAttachmentCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedAttachmentCandidate")
            .field("size", &self.inner.size())
            .finish_non_exhaustive()
    }
}

impl ManagedAttachmentCandidate {
    pub fn name(&self) -> &str {
        self.inner.filename()
    }

    pub fn size_bytes(&self) -> u64 {
        self.inner.size()
    }
}

pub struct ManagedAttachmentDiscovery {
    revision: Revision,
    candidates: Vec<ManagedAttachmentCandidate>,
}

impl ManagedAttachmentDiscovery {
    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn candidates(&self) -> &[ManagedAttachmentCandidate] {
        &self.candidates
    }

    pub fn into_candidates(self) -> Vec<ManagedAttachmentCandidate> {
        self.candidates
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedAttachmentDiscoveryError {
    GuideNotFound,
    GuideArchived,
    UnsafeStorage,
    ResourceLimit,
    ChangedDuringRead,
    Io,
}

pub fn list_managed_attachment_candidates(
    workspace: &WorkspaceView,
    guide_id: GuideId,
) -> Result<ManagedAttachmentDiscovery, ManagedAttachmentDiscoveryError> {
    let (_, loaded) = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides().iter().map(move |guide| (category, guide)))
        .find(|(_, guide)| guide.guide().id() == &guide_id)
        .ok_or(ManagedAttachmentDiscoveryError::GuideNotFound)?;
    if loaded.effective_status() == crate::domain::GuideStatus::Archived {
        return Err(ManagedAttachmentDiscoveryError::GuideArchived);
    }
    let declared = loaded
        .guide()
        .attachments()
        .unwrap_or_default()
        .iter()
        .map(|attachment| attachment.path().to_owned())
        .collect::<HashSet<_>>();
    let candidates = discover_managed_attachment_candidates(workspace, guide_id, &declared)
        .map_err(|error| match error {
            WorkspaceInspectError::ResourceLimit => ManagedAttachmentDiscoveryError::ResourceLimit,
            WorkspaceInspectError::ChangedDuringRead => {
                ManagedAttachmentDiscoveryError::ChangedDuringRead
            }
            WorkspaceInspectError::NotFound
            | WorkspaceInspectError::NotDirectory
            | WorkspaceInspectError::PermissionDenied
            | WorkspaceInspectError::UnsafeRoot => ManagedAttachmentDiscoveryError::UnsafeStorage,
            WorkspaceInspectError::Io => ManagedAttachmentDiscoveryError::Io,
        })?
        .into_iter()
        .map(|inner| ManagedAttachmentCandidate { inner })
        .collect();
    Ok(ManagedAttachmentDiscovery {
        revision: loaded.revision(),
        candidates,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManagedAttachmentAddOutcome {
    attachment_id: AttachmentId,
    revision: Revision,
}

impl ManagedAttachmentAddOutcome {
    pub fn attachment_id(self) -> AttachmentId {
        self.attachment_id
    }

    pub fn revision(self) -> Revision {
        self.revision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedAttachmentAddError {
    GuideNotFound,
    RevisionConflict,
    GuideArchived,
    CandidateChanged,
    AttachmentLimit,
    DuplicateAttachment,
    InvalidTitle,
    UnsafeRoundTrip,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    CommittedRefreshRequired {
        attachment_id: AttachmentId,
        revision: Revision,
    },
}

pub fn add_managed_attachment_metadata(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    candidate: ManagedAttachmentCandidate,
    title: String,
) -> Result<ManagedAttachmentAddOutcome, ManagedAttachmentAddError> {
    if !candidate.inner.matches_workspace(workspace) || candidate.inner.guide_id() != guide_id {
        return Err(ManagedAttachmentAddError::CandidateChanged);
    }
    let expected_attachment_path = format!("attachments/{guide_id}/{}", candidate.inner.filename());
    let (category, loaded) = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides().iter().map(move |guide| (category, guide)))
        .find(|(_, guide)| guide.guide().id() == &guide_id)
        .ok_or(ManagedAttachmentAddError::GuideNotFound)?;
    if loaded.revision() != expected_revision {
        return Err(ManagedAttachmentAddError::RevisionConflict);
    }
    if loaded.effective_status() == crate::domain::GuideStatus::Archived {
        return Err(ManagedAttachmentAddError::GuideArchived);
    }
    let relative_path = loaded.relative_path().to_owned();
    let category_id = *category.category().id();
    let attachment_id = new_attachment_id();
    let at = current_timestamp();
    let stamp = MutationStamp::from_trusted_time(at);
    let outcome = execute_managed_attachment_add(
        workspace,
        state_home,
        runtime_directory,
        &relative_path,
        guide_id,
        expected_revision,
        &candidate.inner,
        new_operation_id(),
        at,
        |bytes, filename, sha256, size| {
            prepare_managed_attachment_candidate(
                bytes,
                guide_id,
                category_id,
                attachment_id,
                filename,
                &title,
                sha256,
                size,
                &stamp,
            )
        },
    )
    .map_err(|error| match error {
        DocumentUpdateError::Prepare(PlanError::AttachmentLimit) => {
            ManagedAttachmentAddError::AttachmentLimit
        }
        DocumentUpdateError::Prepare(PlanError::DuplicateAttachment) => {
            ManagedAttachmentAddError::DuplicateAttachment
        }
        DocumentUpdateError::Prepare(PlanError::InvalidEdit) => {
            ManagedAttachmentAddError::InvalidTitle
        }
        DocumentUpdateError::Prepare(PlanError::UnsafeRoundTrip) => {
            ManagedAttachmentAddError::UnsafeRoundTrip
        }
        DocumentUpdateError::Prepare(PlanError::ResourceLimit) | DocumentUpdateError::TooLarge => {
            ManagedAttachmentAddError::ResourceLimit
        }
        DocumentUpdateError::Prepare(PlanError::InvalidGuide | PlanError::NotReady) => {
            ManagedAttachmentAddError::CandidateChanged
        }
        DocumentUpdateError::Conflict => ManagedAttachmentAddError::CandidateChanged,
        DocumentUpdateError::Busy => ManagedAttachmentAddError::Busy,
        DocumentUpdateError::Unsafe => ManagedAttachmentAddError::UnsafeStorage,
        DocumentUpdateError::Unsupported => ManagedAttachmentAddError::UnsupportedFilesystem,
        DocumentUpdateError::Io => ManagedAttachmentAddError::Io,
    })?;
    if refresh_workspace(workspace).is_err()
        || workspace
            .issues()
            .iter()
            .any(|issue| issue.relative_path() == expected_attachment_path)
        || !workspace
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .any(|guide| {
                guide.guide().id() == &guide_id
                    && guide.revision() == outcome.revision()
                    && guide.guide().attachments().is_some_and(|attachments| {
                        attachments.iter().any(|attachment| {
                            attachment.id() == &attachment_id
                                && attachment.path() == expected_attachment_path
                        })
                    })
            })
    {
        return Err(ManagedAttachmentAddError::CommittedRefreshRequired {
            attachment_id,
            revision: outcome.revision(),
        });
    }
    Ok(ManagedAttachmentAddOutcome {
        attachment_id,
        revision: outcome.revision(),
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_managed_attachment_candidate(
    bytes: &[u8],
    guide_id: GuideId,
    category_id: CategoryId,
    attachment_id: AttachmentId,
    filename: &str,
    title: &str,
    sha256: [u8; 32],
    size: u64,
    stamp: &MutationStamp,
) -> Result<Box<[u8]>, PlanError> {
    let path = format!("attachments/{guide_id}/{filename}");
    let parsed = parse_guide_source(copy_bytes(bytes)?).map_err(|_| PlanError::InvalidGuide)?;
    let bound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
    if bound.record().id() != &guide_id || bound.record().category_id() != &category_id {
        return Err(PlanError::InvalidGuide);
    }
    let attachments = bound.record().attachments().unwrap_or_default();
    if attachments.len() >= 256 {
        return Err(PlanError::AttachmentLimit);
    }
    let sha256 = sha256
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if attachments
        .iter()
        .any(|attachment| attachment.sha256() == sha256)
    {
        return Err(PlanError::DuplicateAttachment);
    }
    apply_guide_mutation(
        &bound,
        GuideMutation::AddAttachment(AttachmentInput {
            id: attachment_id,
            path,
            title: title.to_owned(),
            sha256,
            size_bytes: size,
        }),
        stamp,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentRemovalOutcome {
    Cancelled,
    Removed { revision: Revision },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentRemovalError {
    GuideNotFound,
    AttachmentNotFound,
    RevisionConflict,
    GuideArchived,
    AttachmentChanged,
    Cancelled,
    UnsafeRoundTrip,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
    RecoveryRequired { operation_id: OperationId },
    CommittedRefreshRequired { revision: Revision },
}

pub struct AttachmentRemovalApproval {
    guide_id: GuideId,
    category_id: CategoryId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
    guide_title: String,
    category_name: String,
    attachment_title: String,
    guide_relative_path: String,
    attachment_path: String,
    attachment_sha256: String,
    attachment_size: Option<u64>,
    at: Timestamp,
    preview: crate::storage::AttachmentRemovalPreview,
}

impl AttachmentRemovalApproval {
    pub fn guide_title(&self) -> &str {
        &self.guide_title
    }

    pub fn category_name(&self) -> &str {
        &self.category_name
    }

    pub fn attachment_title(&self) -> &str {
        &self.attachment_title
    }

    pub fn attachment_path(&self) -> &str {
        &self.attachment_path
    }

    pub fn guide_relative_path(&self) -> &str {
        &self.guide_relative_path
    }

    pub fn size(&self) -> u64 {
        self.preview.size()
    }

    pub fn effect_binding_hash(&self) -> [u8; 32] {
        self.preview.effect_binding_hash()
    }
}

pub fn validate_attachment_removal_approval(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
) -> Result<AttachmentRemovalApproval, AttachmentRemovalError> {
    let (category, loaded) = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides().iter().map(move |guide| (category, guide)))
        .find(|(_, guide)| guide.guide().id() == &guide_id)
        .ok_or(AttachmentRemovalError::GuideNotFound)?;
    if loaded.revision() != expected_revision {
        return Err(AttachmentRemovalError::RevisionConflict);
    }
    if loaded.effective_status() == crate::domain::GuideStatus::Archived {
        return Err(AttachmentRemovalError::GuideArchived);
    }
    let attachment = loaded
        .guide()
        .attachments()
        .unwrap_or_default()
        .iter()
        .find(|attachment| attachment.id() == &attachment_id)
        .ok_or(AttachmentRemovalError::AttachmentNotFound)?;
    let guide_title = loaded.guide().title().to_owned();
    let category_name = category.category().name().to_owned();
    let category_id = *category.category().id();
    let guide_relative_path = loaded.relative_path().to_owned();
    let attachment_title = attachment.title().to_owned();
    let attachment_path = attachment.path().to_owned();
    let attachment_sha256 = attachment.sha256().to_owned();
    let declared_sha256 =
        decode_sha256(&attachment_sha256).ok_or(AttachmentRemovalError::AttachmentChanged)?;
    let attachment_size = attachment.size_bytes();
    let at = current_timestamp();
    let stamp = MutationStamp::from_trusted_time(at);
    let preview = preview_attachment_removal(
        workspace,
        state_home,
        runtime_directory,
        &guide_relative_path,
        expected_revision,
        &attachment_path,
        declared_sha256,
        attachment_size,
        |bytes| {
            prepare_attachment_removal_candidate(
                bytes,
                guide_id,
                category_id,
                attachment_id,
                &attachment_path,
                &attachment_sha256,
                attachment_size,
                &stamp,
            )
        },
    )
    .map_err(|error| map_attachment_removal_error(error.into()))?;
    Ok(AttachmentRemovalApproval {
        guide_id,
        category_id,
        attachment_id,
        expected_revision,
        guide_title,
        category_name,
        attachment_title,
        guide_relative_path,
        attachment_path,
        attachment_sha256,
        attachment_size,
        at,
        preview,
    })
}

pub fn confirm_and_remove_attachment<W: HasWindowHandle + HasDisplayHandle + ?Sized>(
    parent: &W,
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
) -> Result<AttachmentRemovalOutcome, AttachmentRemovalError> {
    let approval = validate_attachment_removal_approval(
        workspace,
        state_home,
        runtime_directory,
        guide_id,
        attachment_id,
        expected_revision,
    )?;
    let confirmed = rfd::MessageDialog::new()
        .set_parent(parent)
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Remove Attachment?")
        .set_description(format!(
            "Guide: {}\nCategory: {}\nGuide ID: {guide_id}\nAttachment: {}\nAttachment ID: {attachment_id}\nPath: {}\nSize: {}\n\nRehow will remove this Attachment from the Guide and move its file to recoverable Workspace trash.",
            safe_dialog_text(approval.guide_title()),
            safe_dialog_text(approval.category_name()),
            safe_dialog_text(approval.attachment_title()),
            approval.attachment_path(),
            display_byte_size(approval.size()),
        ))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        return Ok(AttachmentRemovalOutcome::Cancelled);
    }
    approve_attachment_removal_after_native_confirmation(
        workspace,
        state_home,
        runtime_directory,
        approval,
        || false,
    )
}

pub fn approve_attachment_removal_after_native_confirmation<C>(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    approval: AttachmentRemovalApproval,
    cancelled: C,
) -> Result<AttachmentRemovalOutcome, AttachmentRemovalError>
where
    C: Fn() -> bool,
{
    if cancelled() {
        return Err(AttachmentRemovalError::Cancelled);
    }
    execute_approved_attachment_removal(
        workspace,
        state_home,
        runtime_directory,
        approval.guide_id,
        approval.category_id,
        approval.attachment_id,
        approval.expected_revision,
        &approval.attachment_path,
        &approval.attachment_sha256,
        approval.attachment_size,
        approval.at,
        &approval.preview,
        &cancelled,
        new_operation_id(),
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_approved_attachment_removal(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    category_id: CategoryId,
    attachment_id: AttachmentId,
    expected_revision: Revision,
    attachment_path: &str,
    attachment_sha256: &str,
    attachment_size: Option<u64>,
    at: Timestamp,
    preview: &crate::storage::AttachmentRemovalPreview,
    cancelled: &impl Fn() -> bool,
    operation_id: OperationId,
) -> Result<AttachmentRemovalOutcome, AttachmentRemovalError> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or(AttachmentRemovalError::GuideNotFound)?;
    if loaded.revision() != expected_revision {
        return Err(AttachmentRemovalError::RevisionConflict);
    }
    let guide_relative_path = loaded.relative_path().to_owned();
    let stamp = MutationStamp::from_trusted_time(at);
    let outcome = execute_attachment_removal(
        workspace,
        state_home,
        runtime_directory,
        &guide_relative_path,
        expected_revision,
        operation_id,
        at,
        preview,
        cancelled,
        |bytes| {
            prepare_attachment_removal_candidate(
                bytes,
                guide_id,
                category_id,
                attachment_id,
                attachment_path,
                attachment_sha256,
                attachment_size,
                &stamp,
            )
        },
    )
    .map_err(map_attachment_removal_error)?;
    if refresh_workspace(workspace).is_err()
        || !workspace
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .any(|guide| {
                guide.guide().id() == &guide_id
                    && guide.revision() == outcome.revision()
                    && !guide
                        .guide()
                        .attachments()
                        .unwrap_or_default()
                        .iter()
                        .any(|attachment| attachment.id() == &attachment_id)
            })
    {
        return Err(AttachmentRemovalError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    }
    Ok(AttachmentRemovalOutcome::Removed {
        revision: outcome.revision(),
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_attachment_removal_candidate(
    bytes: &[u8],
    guide_id: GuideId,
    category_id: CategoryId,
    attachment_id: AttachmentId,
    attachment_path: &str,
    attachment_sha256: &str,
    attachment_size: Option<u64>,
    stamp: &MutationStamp,
) -> Result<Box<[u8]>, PlanError> {
    let parsed = parse_guide_source(copy_bytes(bytes)?).map_err(|_| PlanError::InvalidGuide)?;
    let bound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
    if bound.record().id() != &guide_id || bound.record().category_id() != &category_id {
        return Err(PlanError::InvalidGuide);
    }
    let attachment = bound
        .record()
        .attachments()
        .unwrap_or_default()
        .iter()
        .find(|attachment| attachment.id() == &attachment_id)
        .ok_or(PlanError::InvalidEdit)?;
    if attachment.path() != attachment_path
        || attachment.sha256() != attachment_sha256
        || attachment.size_bytes() != attachment_size
    {
        return Err(PlanError::InvalidEdit);
    }
    apply_guide_mutation(
        &bound,
        GuideMutation::RemoveAttachment(attachment_id),
        stamp,
    )
}

fn decode_sha256(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut decoded = [0_u8; 32];
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    if !remainder.is_empty() {
        return None;
    }
    for (index, pair) in pairs.iter().enumerate() {
        let high = (pair[0] as char).to_digit(16)? as u8;
        let low = (pair[1] as char).to_digit(16)? as u8;
        decoded[index] = (high << 4) | low;
    }
    Some(decoded)
}

fn map_attachment_removal_error(
    error: AttachmentRemovalStorageError<PlanError>,
) -> AttachmentRemovalError {
    let error = match error {
        AttachmentRemovalStorageError::Mutation(error) => error,
        AttachmentRemovalStorageError::RecoveryRequired { operation_id } => {
            return AttachmentRemovalError::RecoveryRequired { operation_id };
        }
    };
    match error {
        MultiDocumentUpdateError::Prepare(PlanError::UnsafeRoundTrip) => {
            AttachmentRemovalError::UnsafeRoundTrip
        }
        MultiDocumentUpdateError::Prepare(PlanError::ResourceLimit)
        | MultiDocumentUpdateError::TooLarge => AttachmentRemovalError::ResourceLimit,
        MultiDocumentUpdateError::Prepare(
            PlanError::InvalidGuide
            | PlanError::InvalidEdit
            | PlanError::NotReady
            | PlanError::AttachmentLimit
            | PlanError::DuplicateAttachment,
        )
        | MultiDocumentUpdateError::InvalidPlan
        | MultiDocumentUpdateError::Conflict => AttachmentRemovalError::AttachmentChanged,
        MultiDocumentUpdateError::Cancelled => AttachmentRemovalError::Cancelled,
        MultiDocumentUpdateError::Busy => AttachmentRemovalError::Busy,
        MultiDocumentUpdateError::Unsafe => AttachmentRemovalError::UnsafeStorage,
        MultiDocumentUpdateError::Unsupported => AttachmentRemovalError::UnsupportedFilesystem,
        MultiDocumentUpdateError::Io => AttachmentRemovalError::Io,
    }
}

pub fn remove_guide(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
) -> Result<GuideRemoveOutcome, GuideRemoveError> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or(GuideRemoveError::NotFound)?;
    if loaded.revision() != expected_revision {
        return Err(GuideRemoveError::RevisionConflict);
    }
    if loaded
        .guide()
        .attachments()
        .is_some_and(|attachments| !attachments.is_empty())
    {
        return Err(GuideRemoveError::HasAttachments);
    }
    let relative_path = loaded.relative_path().to_owned();
    execute_document_remove(
        workspace,
        state_home,
        runtime_directory,
        &relative_path,
        expected_revision,
        new_operation_id(),
        current_timestamp(),
    )
    .map_err(map_guide_remove_error)?;
    if refresh_workspace(workspace).is_err()
        || workspace
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .any(|guide| guide.guide().id() == &guide_id)
    {
        return Err(GuideRemoveError::CommittedRefreshRequired);
    }
    Ok(GuideRemoveOutcome)
}

fn map_guide_remove_error(error: DocumentRemoveError) -> GuideRemoveError {
    match error {
        DocumentRemoveError::Busy => GuideRemoveError::Busy,
        DocumentRemoveError::Conflict => GuideRemoveError::RevisionConflict,
        DocumentRemoveError::Unsafe => GuideRemoveError::UnsafeStorage,
        DocumentRemoveError::Unsupported => GuideRemoveError::UnsupportedFilesystem,
        DocumentRemoveError::TooLarge => GuideRemoveError::ResourceLimit,
        DocumentRemoveError::Io => GuideRemoveError::Io,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_guide_details(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    category_id: CategoryId,
    title: String,
    summary: String,
    tags: Vec<String>,
    aliases: Vec<String>,
) -> Result<GuideUpdateOutcome, GuideUpdateError> {
    let (source_category_id, source_relative_path) = workspace
        .categories()
        .iter()
        .find_map(|category| {
            category
                .guides()
                .iter()
                .find(|guide| guide.guide().id() == &guide_id)
                .map(|guide| (*category.category().id(), guide.relative_path().to_owned()))
        })
        .ok_or(GuideUpdateError::NotFound)?;
    let destination = workspace
        .categories()
        .iter()
        .find(|category| category.category().id() == &category_id)
        .ok_or(GuideUpdateError::DestinationCategoryNotFound)?;
    if source_category_id == category_id {
        return update_guide(
            workspace,
            state_home,
            runtime_directory,
            guide_id,
            expected_revision,
            GuideEdit::SetDetails {
                title,
                summary,
                tags,
                aliases,
            },
        );
    }
    if destination.category().status() != CategoryStatus::Active {
        return Err(GuideUpdateError::DestinationCategoryArchived);
    }
    let destination_descriptor_path = format!("{}/category.yaml", destination.relative_directory());
    let destination_revision = destination.descriptor_revision();
    let filename = source_relative_path
        .rsplit_once('/')
        .map(|(_, filename)| filename)
        .filter(|filename| filename.ends_with(".md") && !filename.is_empty())
        .ok_or(GuideUpdateError::InvalidGuide)?;
    let destination_relative_path = format!("{}/{filename}", destination.relative_directory());
    let at = current_timestamp();
    let stamp = MutationStamp::from_trusted_time(at);
    let outcome = execute_document_move(
        workspace,
        state_home,
        runtime_directory,
        &source_relative_path,
        &destination_relative_path,
        expected_revision,
        &destination_descriptor_path,
        destination_revision,
        new_operation_id(),
        at,
        |bytes| {
            let parsed =
                parse_guide_source(copy_bytes(bytes)?).map_err(|_| PlanError::InvalidGuide)?;
            let bound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
            if bound.record().id() != &guide_id
                || bound.record().category_id() != &source_category_id
            {
                return Err(PlanError::InvalidGuide);
            }
            let mut candidate_bytes = copy_bytes(bound.source_bytes())?;
            for mutation in [
                GuideMutation::ChangeCategory(category_id),
                GuideMutation::SetTitle(title),
                GuideMutation::SetSummary(summary),
                GuideMutation::ReplaceTags(tags),
                GuideMutation::ReplaceAliases(aliases),
            ] {
                let parsed =
                    parse_guide_source(candidate_bytes).map_err(|_| PlanError::InvalidGuide)?;
                let candidate_bound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
                candidate_bytes = apply_guide_mutation(&candidate_bound, mutation, &stamp)?;
            }
            Ok(candidate_bytes)
        },
    )
    .map_err(map_move_error)?;

    if refresh_workspace(workspace).is_err() {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    }
    let mut matches = workspace.categories().iter().flat_map(|category| {
        category
            .guides()
            .iter()
            .filter(|guide| guide.guide().id() == &guide_id)
            .map(move |guide| (category, guide))
    });
    let Some((loaded_category, loaded_guide)) = matches.next() else {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    };
    if matches.next().is_some()
        || loaded_category.category().id() != &category_id
        || loaded_guide.relative_path() != destination_relative_path
        || loaded_guide.revision() != outcome.revision()
    {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    }
    Ok(GuideUpdateOutcome {
        revision: outcome.revision(),
        changed: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn change_guide_category_at_revision(
    workspace: &mut WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    guide_id: GuideId,
    expected_revision: Revision,
    category_id: CategoryId,
    expected_category_revision: Revision,
) -> Result<GuideUpdateOutcome, GuideUpdateError> {
    let (source_category_id, source_relative_path, current_revision) = workspace
        .categories()
        .iter()
        .find_map(|category| {
            category
                .guides()
                .iter()
                .find(|guide| guide.guide().id() == &guide_id)
                .map(|guide| {
                    (
                        *category.category().id(),
                        guide.relative_path().to_owned(),
                        guide.revision(),
                    )
                })
        })
        .ok_or(GuideUpdateError::NotFound)?;
    if current_revision != expected_revision {
        return Err(GuideUpdateError::RevisionConflict);
    }
    let destination = workspace
        .categories()
        .iter()
        .find(|category| category.category().id() == &category_id)
        .ok_or(GuideUpdateError::DestinationCategoryNotFound)?;
    if destination.descriptor_revision() != expected_category_revision {
        return Err(GuideUpdateError::RevisionConflict);
    }
    if destination.category().status() != CategoryStatus::Active {
        return Err(GuideUpdateError::DestinationCategoryArchived);
    }
    let filename = source_relative_path
        .rsplit_once('/')
        .map(|(_, filename)| filename)
        .filter(|filename| filename.ends_with(".md") && !filename.is_empty())
        .ok_or(GuideUpdateError::InvalidGuide)?;
    let destination_relative_path = format!("{}/{filename}", destination.relative_directory());
    let destination_descriptor_path = format!("{}/category.yaml", destination.relative_directory());
    let at = current_timestamp();
    let stamp = MutationStamp::from_trusted_time(at);
    let outcome = execute_document_move(
        workspace,
        state_home,
        runtime_directory,
        &source_relative_path,
        &destination_relative_path,
        expected_revision,
        &destination_descriptor_path,
        expected_category_revision,
        new_operation_id(),
        at,
        |bytes| {
            let parsed =
                parse_guide_source(copy_bytes(bytes)?).map_err(|_| PlanError::InvalidGuide)?;
            let bound = bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
            if bound.record().id() != &guide_id
                || bound.record().category_id() != &source_category_id
            {
                return Err(PlanError::InvalidGuide);
            }
            apply_guide_mutation(&bound, GuideMutation::ChangeCategory(category_id), &stamp)
        },
    )
    .map_err(map_move_error)?;

    if outcome.changed() && refresh_workspace(workspace).is_err() {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    }
    let mut matches = workspace.categories().iter().flat_map(|category| {
        category
            .guides()
            .iter()
            .filter(|guide| guide.guide().id() == &guide_id)
            .map(move |guide| (category, guide))
    });
    let Some((loaded_category, loaded_guide)) = matches.next() else {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    };
    if matches.next().is_some()
        || loaded_category.category().id() != &category_id
        || loaded_guide.relative_path() != destination_relative_path
        || loaded_guide.revision() != outcome.revision()
    {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    }
    Ok(GuideUpdateOutcome {
        revision: outcome.revision(),
        changed: outcome.changed(),
    })
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
                GuideEdit::SetDetails {
                    title,
                    summary,
                    tags,
                    aliases,
                } => {
                    let mut candidate_bytes = copy_bytes(bound.source_bytes())?;
                    for mutation in [
                        GuideMutation::SetTitle(title),
                        GuideMutation::SetSummary(summary),
                        GuideMutation::ReplaceTags(tags),
                        GuideMutation::ReplaceAliases(aliases),
                    ] {
                        let parsed = parse_guide_source(candidate_bytes)
                            .map_err(|_| PlanError::InvalidGuide)?;
                        let candidate_bound =
                            bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
                        candidate_bytes = apply_guide_mutation(&candidate_bound, mutation, &stamp)?;
                    }
                    Ok(candidate_bytes)
                }
                GuideEdit::ReplaceTags(values) => {
                    apply_guide_mutation(&bound, GuideMutation::ReplaceTags(values), &stamp)
                }
                GuideEdit::ReplaceAliases(values) => {
                    apply_guide_mutation(&bound, GuideMutation::ReplaceAliases(values), &stamp)
                }
                GuideEdit::SetContent {
                    title,
                    summary,
                    tags,
                    aliases,
                    body,
                } => {
                    let mut candidate_bytes = copy_bytes(bound.source_bytes())?;
                    for mutation in [
                        GuideMutation::SetTitle(title),
                        GuideMutation::SetSummary(summary),
                        GuideMutation::ReplaceTags(tags),
                        GuideMutation::ReplaceAliases(aliases),
                        GuideMutation::ReplaceBody(body),
                    ] {
                        let parsed = parse_guide_source(candidate_bytes)
                            .map_err(|_| PlanError::InvalidGuide)?;
                        let candidate_bound =
                            bind_guide(parsed).map_err(|_| PlanError::InvalidGuide)?;
                        candidate_bytes = apply_guide_mutation(&candidate_bound, mutation, &stamp)?;
                    }
                    Ok(candidate_bytes)
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
                GuideEdit::UpdateAttachmentTitle {
                    attachment_id,
                    title,
                } => apply_guide_mutation(
                    &bound,
                    GuideMutation::UpdateAttachmentTitle {
                        attachment_id,
                        title,
                    },
                    &stamp,
                ),
                GuideEdit::MarkStale => {
                    apply_guide_mutation(&bound, GuideMutation::MarkStale, &stamp)
                }
                GuideEdit::Archive => apply_guide_mutation(&bound, GuideMutation::Archive, &stamp),
                GuideEdit::Restore => apply_guide_mutation(&bound, GuideMutation::Restore, &stamp),
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
    NotReady,
    AttachmentLimit,
    DuplicateAttachment,
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
        DocumentUpdateError::Prepare(PlanError::NotReady) => GuideUpdateError::NotReady,
        DocumentUpdateError::Prepare(
            PlanError::AttachmentLimit | PlanError::DuplicateAttachment,
        ) => GuideUpdateError::InvalidEdit,
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

fn map_cancellable_update_error(
    error: CancellableDocumentUpdateError<PlanError>,
) -> GuideTestedApprovalError {
    match error {
        CancellableDocumentUpdateError::Cancelled => GuideTestedApprovalError::Cancelled,
        CancellableDocumentUpdateError::Update(error) => {
            GuideTestedApprovalError::Update(map_update_error(error))
        }
    }
}

fn map_move_error(error: MultiDocumentUpdateError<PlanError>) -> GuideUpdateError {
    match error {
        MultiDocumentUpdateError::Prepare(PlanError::InvalidGuide)
        | MultiDocumentUpdateError::InvalidPlan => GuideUpdateError::InvalidGuide,
        MultiDocumentUpdateError::Prepare(PlanError::InvalidEdit) => GuideUpdateError::InvalidEdit,
        MultiDocumentUpdateError::Prepare(PlanError::NotReady) => GuideUpdateError::NotReady,
        MultiDocumentUpdateError::Prepare(
            PlanError::AttachmentLimit | PlanError::DuplicateAttachment,
        ) => GuideUpdateError::InvalidEdit,
        MultiDocumentUpdateError::Prepare(PlanError::UnsafeRoundTrip) => {
            GuideUpdateError::UnsafeRoundTrip
        }
        MultiDocumentUpdateError::Prepare(PlanError::ResourceLimit)
        | MultiDocumentUpdateError::TooLarge => GuideUpdateError::ResourceLimit,
        MultiDocumentUpdateError::Busy => GuideUpdateError::Busy,
        MultiDocumentUpdateError::Conflict | MultiDocumentUpdateError::Cancelled => {
            GuideUpdateError::RevisionConflict
        }
        MultiDocumentUpdateError::Unsafe => GuideUpdateError::UnsafeStorage,
        MultiDocumentUpdateError::Unsupported => GuideUpdateError::UnsupportedFilesystem,
        MultiDocumentUpdateError::Io => GuideUpdateError::Io,
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
        os::unix::fs::{PermissionsExt, symlink},
        path::{Path, PathBuf},
        str::FromStr,
        sync::atomic::{AtomicU64, Ordering},
    };

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const GUIDE_ID: &str = "0199a1d7-7b0d-7d8e-a38d-f936c5735e10";
    const CATEGORY_ID: &str = "0199a1c2-d334-7d17-bd9c-f8b6ba3a5551";
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

    fn attachment_preview_fixture(
        label: &str,
        content: &[u8],
    ) -> (TestDirectory, Box<WorkspaceView>, AttachmentId, PathBuf) {
        use sha2::Digest;

        let attachment_id = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509930").unwrap();
        let sha256 = sha2::Sha256::digest(content)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let relative_path = format!("attachments/{GUIDE_ID}/preview.txt");
        let guide = GUIDE.replacen(
            "status: draft\n",
            &format!(
                "attachments:\n  - id: {attachment_id}\n    path: {relative_path}\n    title: Preview notes\n    sha256: {sha256}\n    size_bytes: {}\nstatus: draft\n",
                content.len()
            ),
            1,
        );
        let (workspace, _state, mut view) = fixture(label, &guide);
        let directory = workspace.path().join(format!("attachments/{GUIDE_ID}"));
        fs::create_dir_all(&directory).unwrap();
        fs::set_permissions(
            workspace.path().join("attachments"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let attachment_path = workspace.path().join(relative_path);
        fs::write(&attachment_path, content).unwrap();
        fs::set_permissions(&attachment_path, fs::Permissions::from_mode(0o600)).unwrap();
        refresh_workspace(&mut view).unwrap();
        (workspace, view, attachment_id, attachment_path)
    }

    #[test]
    fn attachment_text_preview_is_pathless_bounded_and_sanitized() {
        let content = "first line\nunsafe \u{1b} direction \u{202e} soft\u{00ad} \u{180e} math\u{2061} variant\u{fe0f} tag\u{e0001} text\ttail\n".as_bytes();
        let (_workspace, view, attachment_id, _path) =
            attachment_preview_fixture("attachment-text-preview", content);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let preview =
            preview_attachment_text(&view, guide_id, attachment_id, revision(&view)).unwrap();

        assert_eq!(preview.title(), "Preview notes");
        assert_eq!(preview.size_bytes(), content.len() as u64);
        assert!(preview.sanitized());
        assert_eq!(
            preview.text(),
            "first line\nunsafe � direction � soft� � math� variant� tag� text\ttail\n"
        );
        assert!(!format!("{preview:?}").contains("first line"));
    }

    #[test]
    fn attachment_text_preview_refuses_stale_binary_and_changed_content() {
        let (workspace, view, attachment_id, attachment_path) =
            attachment_preview_fixture("attachment-text-refusals", b"same");
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        fs::write(&attachment_path, b"lame").unwrap();
        assert_eq!(
            preview_attachment_text(&view, guide_id, attachment_id, expected),
            Err(AttachmentTextPreviewError::AttachmentChanged)
        );

        let (_binary_workspace, binary_view, binary_id, _binary_path) =
            attachment_preview_fixture("attachment-binary-preview", b"binary\0\xff");
        assert_eq!(
            preview_attachment_text(&binary_view, guide_id, binary_id, revision(&binary_view)),
            Err(AttachmentTextPreviewError::Binary)
        );

        let current = fs::read_to_string(workspace.path().join(GUIDE_PATH)).unwrap();
        fs::write(
            workspace.path().join(GUIDE_PATH),
            current.replace("Install a Tarball", "Install changed tarball"),
        )
        .unwrap();
        assert_eq!(
            preview_attachment_text(&view, guide_id, attachment_id, expected),
            Err(AttachmentTextPreviewError::RevisionConflict)
        );
    }

    #[test]
    fn attachment_text_preview_refuses_links_special_files_and_oversize_input() {
        let (workspace, view, attachment_id, attachment_path) =
            attachment_preview_fixture("attachment-text-unsafe", b"safe text\n");
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        let outside = workspace.path().join("outside.txt");
        fs::write(&outside, b"safe text\n").unwrap();

        fs::remove_file(&attachment_path).unwrap();
        symlink(&outside, &attachment_path).unwrap();
        assert_eq!(
            preview_attachment_text(&view, guide_id, attachment_id, expected),
            Err(AttachmentTextPreviewError::UnsafeStorage)
        );

        fs::remove_file(&attachment_path).unwrap();
        fs::write(&attachment_path, b"safe text\n").unwrap();
        let hardlink = workspace.path().join("hardlink.txt");
        fs::hard_link(&attachment_path, &hardlink).unwrap();
        assert_eq!(
            preview_attachment_text(&view, guide_id, attachment_id, expected),
            Err(AttachmentTextPreviewError::UnsafeStorage)
        );

        fs::remove_file(&hardlink).unwrap();
        fs::remove_file(&attachment_path).unwrap();
        let fifo = std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(
            attachment_path.as_os_str(),
        ))
        .unwrap();
        // SAFETY: the path is a valid NUL-free CString inside the isolated fixture.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert_eq!(
            preview_attachment_text(&view, guide_id, attachment_id, expected),
            Err(AttachmentTextPreviewError::UnsafeStorage)
        );

        let oversized = vec![b'x'; ATTACHMENT_TEXT_PREVIEW_LIMIT + 1];
        let (_large_workspace, large_view, large_id, _large_path) =
            attachment_preview_fixture("attachment-text-oversize", &oversized);
        assert_eq!(
            preview_attachment_text(&large_view, guide_id, large_id, revision(&large_view)),
            Err(AttachmentTextPreviewError::TooLarge)
        );
    }

    #[test]
    fn attachment_text_preview_rechecks_the_guide_after_reading() {
        let (workspace, view, attachment_id, _attachment_path) =
            attachment_preview_fixture("attachment-text-guide-race", b"stable\n");
        let guide_path = workspace.path().join(GUIDE_PATH);
        set_attachment_preview_after_read_hook(move || {
            let current = fs::read_to_string(&guide_path).unwrap();
            fs::write(
                &guide_path,
                current.replace("Install a Tarball", "Install changed tarball"),
            )
            .unwrap();
        });
        assert_eq!(
            preview_attachment_text(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
            ),
            Err(AttachmentTextPreviewError::RevisionConflict)
        );
    }

    #[test]
    fn attachment_export_publishes_exact_private_bytes_without_overwrite() {
        let content = b"binary\0attachment\xff";
        let (_workspace, view, attachment_id, _attachment_path) =
            attachment_preview_fixture("attachment-export", content);
        let destination = TestDirectory::new("attachment-export-destination");
        let target = destination.path().join("export.bin");
        assert_eq!(
            export_attachment_to_path(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
                &target,
            ),
            Ok(AttachmentExportOutcome::Exported {
                size_bytes: content.len() as u64,
            })
        );
        assert_eq!(fs::read(&target).unwrap(), content);
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::write(&target, b"preserve").unwrap();
        assert_eq!(
            export_attachment_to_path(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
                &target,
            ),
            Err(AttachmentExportError::DestinationExists)
        );
        assert_eq!(fs::read(&target).unwrap(), b"preserve");
    }

    #[test]
    fn attachment_export_refuses_symlinked_destination_and_source_drift() {
        let (workspace, view, attachment_id, attachment_path) =
            attachment_preview_fixture("attachment-export-refusal", b"stable");
        let destination = TestDirectory::new("attachment-export-refusal-destination");
        let linked_parent = destination.path().with_extension("link");
        symlink(destination.path(), &linked_parent).unwrap();
        assert_eq!(
            export_attachment_to_path(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
                &linked_parent.join("export.bin"),
            ),
            Err(AttachmentExportError::UnsafeDestination)
        );
        fs::remove_file(&linked_parent).unwrap();

        let outside = workspace.path().join("outside.bin");
        fs::write(&outside, b"stable").unwrap();
        fs::remove_file(&attachment_path).unwrap();
        symlink(&outside, &attachment_path).unwrap();
        let unsafe_target = destination.path().join("unsafe.bin");
        assert_eq!(
            export_attachment_to_path(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
                &unsafe_target,
            ),
            Err(AttachmentExportError::UnsafeStorage)
        );
        assert!(!unsafe_target.exists());

        fs::remove_file(&attachment_path).unwrap();
        fs::write(&attachment_path, b"stable").unwrap();
        let destination_symlink = destination.path().join("existing-link.bin");
        symlink(&outside, &destination_symlink).unwrap();
        assert_eq!(
            export_attachment_to_path(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
                &destination_symlink,
            ),
            Err(AttachmentExportError::DestinationExists)
        );
        assert!(
            fs::symlink_metadata(&destination_symlink)
                .unwrap()
                .file_type()
                .is_symlink()
        );

        fs::write(&attachment_path, b"drift!").unwrap();
        let target = destination.path().join("drift.bin");
        assert_eq!(
            export_attachment_to_path(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
                &target,
            ),
            Err(AttachmentExportError::AttachmentChanged)
        );
        assert!(!target.exists());
    }

    #[test]
    fn attachment_export_rechecks_guide_before_publication() {
        let (workspace, view, attachment_id, _attachment_path) =
            attachment_preview_fixture("attachment-export-guide-race", b"stable");
        let destination = TestDirectory::new("attachment-export-race-destination");
        let target = destination.path().join("export.bin");
        let guide_path = workspace.path().join(GUIDE_PATH);
        set_attachment_preview_after_read_hook(move || {
            let current = fs::read_to_string(&guide_path).unwrap();
            fs::write(
                &guide_path,
                current.replace("Install a Tarball", "Install changed tarball"),
            )
            .unwrap();
        });
        assert_eq!(
            export_attachment_to_path(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
                &target,
            ),
            Err(AttachmentExportError::RevisionConflict)
        );
        assert!(!target.exists());
    }

    #[test]
    fn attachment_reveal_prepares_only_the_verified_parent_directory() {
        let (workspace, view, attachment_id, _attachment_path) =
            attachment_preview_fixture("attachment-reveal", b"stable");
        let target = prepare_attachment_reveal_parent(
            &view,
            GuideId::from_str(GUIDE_ID).unwrap(),
            attachment_id,
            revision(&view),
        )
        .unwrap();
        let original = workspace.path().join(format!("attachments/{GUIDE_ID}"));
        let original_metadata = fs::metadata(&original).unwrap();
        let descriptor = fs::File::from(target.duplicate_descriptor().unwrap());
        let descriptor_metadata = descriptor.metadata().unwrap();
        assert_eq!(descriptor_metadata.dev(), original_metadata.dev());
        assert_eq!(descriptor_metadata.ino(), original_metadata.ino());

        let moved = workspace
            .path()
            .join(format!("attachments/{GUIDE_ID}-moved"));
        fs::rename(&original, &moved).unwrap();
        symlink(&moved, &original).unwrap();
        let raced_descriptor = fs::File::from(target.duplicate_descriptor().unwrap());
        let raced_metadata = raced_descriptor.metadata().unwrap();
        assert_eq!(raced_metadata.dev(), original_metadata.dev());
        assert_eq!(raced_metadata.ino(), original_metadata.ino());

        fs::remove_file(&original).unwrap();
        let outside = workspace.path().join("outside-parent");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, &original).unwrap();
        let outside_metadata = fs::metadata(&outside).unwrap();
        let post_swap_descriptor = fs::File::from(target.duplicate_descriptor().unwrap());
        let post_swap_metadata = post_swap_descriptor.metadata().unwrap();
        assert_eq!(post_swap_metadata.ino(), original_metadata.ino());
        assert_ne!(post_swap_metadata.ino(), outside_metadata.ino());
    }

    #[test]
    fn attachment_reveal_refuses_linked_source_and_guide_race() {
        let (workspace, view, attachment_id, attachment_path) =
            attachment_preview_fixture("attachment-reveal-refusal", b"stable");
        let outside = workspace.path().join("outside.bin");
        fs::write(&outside, b"stable").unwrap();
        fs::remove_file(&attachment_path).unwrap();
        symlink(&outside, &attachment_path).unwrap();
        assert!(matches!(
            prepare_attachment_reveal_parent(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
            ),
            Err(AttachmentRevealError::UnsafeStorage)
        ));

        fs::remove_file(&attachment_path).unwrap();
        fs::write(&attachment_path, b"stable").unwrap();
        let guide_path = workspace.path().join(GUIDE_PATH);
        set_attachment_preview_after_read_hook(move || {
            let current = fs::read_to_string(&guide_path).unwrap();
            fs::write(
                &guide_path,
                current.replace("Install a Tarball", "Install changed tarball"),
            )
            .unwrap();
        });
        assert!(matches!(
            prepare_attachment_reveal_parent(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
            ),
            Err(AttachmentRevealError::RevisionConflict)
        ));
    }

    #[test]
    fn attachment_reveal_refuses_raced_intermediate_symlink() {
        let (workspace, view, attachment_id, _attachment_path) =
            attachment_preview_fixture("attachment-reveal-parent-race", b"stable");
        let attachments = workspace.path().join("attachments");
        let moved = workspace.path().join("attachments-moved");
        set_attachment_preview_after_read_hook(move || {
            fs::rename(&attachments, &moved).unwrap();
            symlink(&moved, &attachments).unwrap();
        });

        assert!(matches!(
            prepare_attachment_reveal_parent(
                &view,
                GuideId::from_str(GUIDE_ID).unwrap(),
                attachment_id,
                revision(&view),
            ),
            Err(AttachmentRevealError::UnsafeStorage)
        ));
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
    fn imports_a_markdown_collection_as_draft_guides_without_mutating_sources() {
        let source = TestDirectory::new("markdown-import-source");
        let destination = TestDirectory::new("markdown-import-destination");
        let state = TestDirectory::new("markdown-import-state");
        fs::create_dir(source.path().join("linux")).unwrap();
        let first = b"\xef\xbb\xbf---\r\nstatus: tested\r\n---\r\n\r\n# Install\r\n";
        let second = b"# Configure\n";
        fs::write(source.path().join("linux/01-install.md"), first).unwrap();
        fs::write(source.path().join("configure_system.md"), second).unwrap();
        let WorkspaceInspection::OrdinaryMarkdown(source_capability) =
            inspect_workspace(source.path()).unwrap().into_parts().1
        else {
            panic!("expected ordinary Markdown");
        };
        let selection = source_capability.preview().unwrap();
        let target = empty_target(&destination);

        let outcome = import_markdown_collection(
            &selection,
            &target,
            state.path(),
            None,
            "Imported Guides",
            "Imported",
        )
        .unwrap();

        assert!(outcome.complete());
        assert_eq!(outcome.imported(), 2);
        assert_eq!(outcome.total(), 2);
        assert_eq!(outcome.failure(), None);
        let view = outcome.into_workspace();
        assert_eq!(view.manifest().name(), "Imported Guides");
        assert_eq!(view.categories().len(), 1);
        assert_eq!(view.categories()[0].category().name(), "Imported");
        assert_eq!(view.categories()[0].guides().len(), 2);
        assert!(
            view.categories()[0]
                .guides()
                .iter()
                .all(|guide| guide.guide().declared_status() == GuideStatus::Draft)
        );
        let installed = view.categories()[0]
            .guides()
            .iter()
            .find(|guide| guide.guide().title() == "01 install")
            .unwrap();
        assert_eq!(
            installed.guide().body(),
            b"---\nstatus: tested\n---\n\n# Install\n"
        );
        assert_eq!(
            installed.guide().summary(),
            "Imported from linux/01-install.md"
        );
        assert_eq!(
            fs::read(source.path().join("linux/01-install.md")).unwrap(),
            first
        );
        assert_eq!(
            fs::read(source.path().join("configure_system.md")).unwrap(),
            second
        );
    }

    #[test]
    fn later_import_failure_returns_an_exact_valid_prefix() {
        let source = TestDirectory::new("markdown-import-partial-source");
        let destination = TestDirectory::new("markdown-import-partial-destination");
        let state = TestDirectory::new("markdown-import-partial-state");
        fs::write(source.path().join("01-first.md"), b"# First\n").unwrap();
        fs::write(source.path().join("02-second.md"), b"# Second\n").unwrap();
        let WorkspaceInspection::OrdinaryMarkdown(source_capability) =
            inspect_workspace(source.path()).unwrap().into_parts().1
        else {
            panic!("expected ordinary Markdown");
        };
        let selection = source_capability.preview().unwrap();
        let target = empty_target(&destination);
        set_markdown_import_guide_failure_at(1);

        let outcome = import_markdown_collection(
            &selection,
            &target,
            state.path(),
            None,
            "Partial Import",
            "Imported",
        )
        .unwrap();

        assert_eq!(outcome.imported(), 1);
        assert_eq!(outcome.total(), 2);
        assert_eq!(
            outcome.failure(),
            Some(MarkdownImportPartialFailure::Guide(GuideCreateError::Io))
        );
        let view = outcome.into_workspace();
        assert_eq!(view.categories().len(), 1);
        assert_eq!(view.categories()[0].guides().len(), 1);
        assert_eq!(view.categories()[0].guides()[0].guide().title(), "01 first");
        assert_eq!(
            fs::read(source.path().join("01-first.md")).unwrap(),
            b"# First\n"
        );
        assert_eq!(
            fs::read(source.path().join("02-second.md")).unwrap(),
            b"# Second\n"
        );
    }

    #[test]
    fn source_drift_refuses_import_before_destination_initialization() {
        let source = TestDirectory::new("markdown-import-drift-source");
        let destination = TestDirectory::new("markdown-import-drift-destination");
        let state = TestDirectory::new("markdown-import-drift-state");
        let source_path = source.path().join("guide.md");
        fs::write(&source_path, b"# Original\n").unwrap();
        let WorkspaceInspection::OrdinaryMarkdown(source_capability) =
            inspect_workspace(source.path()).unwrap().into_parts().1
        else {
            panic!("expected ordinary Markdown");
        };
        let selection = source_capability.preview().unwrap();
        let target = empty_target(&destination);
        fs::write(source_path, b"# Changed\n").unwrap();

        assert!(matches!(
            import_markdown_collection(
                &selection,
                &target,
                state.path(),
                None,
                "Imported Guides",
                "Imported",
            ),
            Err(MarkdownImportError::SourceChanged)
        ));
        assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
    }

    #[test]
    fn apply_rejects_symlink_hardlink_and_special_file_substitution() {
        use std::os::unix::net::UnixListener;

        for kind in ["symlink", "hardlink", "socket"] {
            let source = TestDirectory::new(&format!("markdown-import-{kind}-source"));
            let destination = TestDirectory::new(&format!("markdown-import-{kind}-destination"));
            let state = TestDirectory::new(&format!("markdown-import-{kind}-state"));
            let guide = source.path().join("guide.md");
            fs::write(&guide, b"# Original\n").unwrap();
            let WorkspaceInspection::OrdinaryMarkdown(source_capability) =
                inspect_workspace(source.path()).unwrap().into_parts().1
            else {
                panic!("expected ordinary Markdown");
            };
            let selection = source_capability.preview().unwrap();
            let target = empty_target(&destination);
            fs::remove_file(&guide).unwrap();
            let _socket = match kind {
                "symlink" => {
                    symlink("missing.md", &guide).unwrap();
                    None
                }
                "hardlink" => {
                    let other = source.path().join("other.md");
                    fs::write(&other, b"# Original\n").unwrap();
                    fs::hard_link(other, &guide).unwrap();
                    None
                }
                "socket" => Some(UnixListener::bind(&guide).unwrap()),
                _ => unreachable!(),
            };

            assert!(matches!(
                import_markdown_collection(
                    &selection,
                    &target,
                    state.path(),
                    None,
                    "Must Not Exist",
                    "Imported",
                ),
                Err(MarkdownImportError::SourceChanged | MarkdownImportError::SourceUnavailable)
            ));
            assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn destination_change_after_selection_is_preserved_and_refused() {
        let source = TestDirectory::new("markdown-import-destination-change-source");
        let destination = TestDirectory::new("markdown-import-destination-change");
        let state = TestDirectory::new("markdown-import-destination-change-state");
        fs::write(source.path().join("guide.md"), b"# Source\n").unwrap();
        let WorkspaceInspection::OrdinaryMarkdown(source_capability) =
            inspect_workspace(source.path()).unwrap().into_parts().1
        else {
            panic!("expected ordinary Markdown");
        };
        let selection = source_capability.preview().unwrap();
        let target = empty_target(&destination);
        fs::write(destination.path().join("appeared.txt"), b"keep me").unwrap();

        assert!(matches!(
            import_markdown_collection(
                &selection,
                &target,
                state.path(),
                None,
                "Must Not Exist",
                "Imported",
            ),
            Err(MarkdownImportError::Initialize(
                WorkspaceInitializationError::Conflict
            ))
        ));
        assert_eq!(
            fs::read(destination.path().join("appeared.txt")).unwrap(),
            b"keep me"
        );
        assert!(!destination.path().join("rehow.yaml").exists());
    }

    #[test]
    fn destination_moved_beneath_source_during_prepare_is_rechecked() {
        let source = TestDirectory::new("markdown-import-destination-move-source");
        let destination = TestDirectory::new("markdown-import-destination-move");
        let state = TestDirectory::new("markdown-import-destination-move-state");
        fs::write(source.path().join("guide.md"), b"# Source\n").unwrap();
        let WorkspaceInspection::OrdinaryMarkdown(source_capability) =
            inspect_workspace(source.path()).unwrap().into_parts().1
        else {
            panic!("expected ordinary Markdown");
        };
        let selection = source_capability.preview().unwrap();
        let target = empty_target(&destination);
        let original = destination.path().to_owned();
        let nested = source.path().join("moved-destination");
        let nested_for_hook = nested.clone();
        set_markdown_import_after_prepare_hook(move || {
            fs::rename(original, nested_for_hook).unwrap();
        });

        assert!(matches!(
            import_markdown_collection(
                &selection,
                &target,
                state.path(),
                None,
                "Must Not Exist",
                "Imported",
            ),
            Err(MarkdownImportError::SameFolder)
        ));
        assert_eq!(fs::read_dir(&nested).unwrap().count(), 0);
    }

    #[test]
    fn nested_destination_refuses_import_without_writing_into_the_source() {
        let source = TestDirectory::new("markdown-import-nested-destination");
        let state = TestDirectory::new("markdown-import-nested-state");
        fs::write(source.path().join("guide.md"), b"# Source\n").unwrap();
        let destination_path = source.path().join("new-workspace");
        fs::create_dir(&destination_path).unwrap();
        let WorkspaceInspection::OrdinaryMarkdown(source_capability) =
            inspect_workspace(source.path()).unwrap().into_parts().1
        else {
            panic!("expected ordinary Markdown");
        };
        let selection = source_capability.preview().unwrap();
        let WorkspaceInspection::Empty(destination) =
            inspect_workspace(&destination_path).unwrap().into_parts().1
        else {
            panic!("expected empty destination");
        };

        assert!(matches!(
            import_markdown_collection(
                &selection,
                &destination,
                state.path(),
                None,
                "Must Not Exist",
                "Imported",
            ),
            Err(MarkdownImportError::SameFolder)
        ));
        assert_eq!(fs::read_dir(&destination_path).unwrap().count(), 0);
        assert_eq!(
            fs::read(source.path().join("guide.md")).unwrap(),
            b"# Source\n"
        );
    }

    #[test]
    fn manual_refresh_reconciles_external_changes_and_isolates_malformed_guides() {
        let (workspace, _state, mut view) = fixture("manual-refresh", GUIDE);
        let category = workspace.path().join("categories/linux-system--b6ba3a5551");
        let original_path = category.join("install--36c5735e10.md");
        let original_revision = revision(&view);
        fs::write(
            &original_path,
            GUIDE.replace("title: Install a Tarball", "title: Updated Externally"),
        )
        .unwrap();
        let second_id = "0199a1d7-7b0d-7d8e-a38d-f936c5735e11";
        let second_path = category.join("second--36c5735e11.md");
        fs::write(
            &second_path,
            GUIDE
                .replace(GUIDE_ID, second_id)
                .replace("title: Install a Tarball", "title: Added Externally"),
        )
        .unwrap();
        fs::set_permissions(&second_path, fs::Permissions::from_mode(0o600)).unwrap();
        let malformed_path = category.join("broken.md");
        fs::write(&malformed_path, "not Guide frontmatter\n").unwrap();
        fs::set_permissions(&malformed_path, fs::Permissions::from_mode(0o600)).unwrap();

        refresh_open_workspace(&mut view).unwrap();
        let guides = view.categories()[0].guides();
        assert_eq!(guides.len(), 2);
        assert_eq!(view.issues().len(), 1);
        let updated = guides
            .iter()
            .find(|guide| guide.guide().id() == &GuideId::from_str(GUIDE_ID).unwrap())
            .unwrap();
        assert_eq!(updated.guide().title(), "Updated Externally");
        assert_ne!(updated.revision(), original_revision);
        assert!(
            guides
                .iter()
                .any(|guide| guide.guide().title() == "Added Externally")
        );

        fs::remove_file(original_path).unwrap();
        refresh_open_workspace(&mut view).unwrap();
        assert_eq!(view.categories()[0].guides().len(), 1);
        assert_eq!(
            view.categories()[0].guides()[0].guide().title(),
            "Added Externally"
        );
        assert_eq!(view.issues().len(), 1);
    }

    #[test]
    fn failed_manual_refresh_retains_the_last_compatible_view() {
        let (workspace, _state, mut view) = fixture("manual-refresh-failure", GUIDE);
        let manifest_revision = view.manifest_revision();
        fs::remove_file(workspace.path().join("rehow.yaml")).unwrap();

        assert_eq!(
            refresh_open_workspace(&mut view),
            Err(WorkspaceRefreshError::ChangedDuringRead)
        );
        assert_eq!(view.manifest_revision(), manifest_revision);
        assert_eq!(view.categories().len(), 1);
        assert_eq!(view.categories()[0].guides().len(), 1);
        assert_eq!(
            view.categories()[0].guides()[0].guide().title(),
            "Install a Tarball"
        );
    }

    #[test]
    fn changed_attachment_refresh_retains_the_last_compatible_view() {
        let guide = GUIDE.replacen(
            "status: draft\n",
            "attachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.txt\n    title: Example text\n    sha256: 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\n    size_bytes: 5\nstatus: draft\n",
            1,
        );
        let (workspace, _state, mut view) = fixture("attachment-refresh-failure", &guide);
        let original_revision = revision(&view);
        let directory = workspace.path().join("attachments").join(GUIDE_ID);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("example.txt"), b"hello").unwrap();
        crate::storage::set_attachment_changed_fault();

        assert_eq!(
            refresh_open_workspace(&mut view),
            Err(WorkspaceRefreshError::ChangedDuringRead)
        );
        assert_eq!(revision(&view), original_revision);
        assert_eq!(
            view.categories()[0].guides()[0].guide().title(),
            "Install a Tarball"
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
    fn initialization_journal_recovers_after_categories_directory_publication() {
        let workspace = TestDirectory::new("initialize-directory-interruption");
        let state = TestDirectory::new("initialize-directory-interruption-state");
        let target = empty_target(&workspace);
        crate::storage::set_workspace_fault(crate::storage::WorkspaceFault::AfterDirectoryCreate);

        let view = initialize_workspace(&target, state.path(), None, "Recovered Rehow").unwrap();

        assert_eq!(view.manifest().name(), "Recovered Rehow");
        assert!(view.categories().is_empty());
        assert!(workspace.path().join("rehow.yaml").is_file());
        assert!(workspace.path().join("categories").is_dir());
        assert_eq!(
            fs::read_dir(workspace.path().join("categories"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn initialization_race_cannot_adopt_a_foreign_categories_directory() {
        let workspace = TestDirectory::new("initialize-directory-race");
        let state = TestDirectory::new("initialize-directory-race-state");
        let target = empty_target(&workspace);
        let categories = workspace.path().join("categories");
        crate::storage::set_before_directory_create_hook(move || {
            fs::create_dir(&categories).unwrap();
            fs::set_permissions(&categories, fs::Permissions::from_mode(0o700)).unwrap();
        });

        assert!(matches!(
            initialize_workspace(&target, state.path(), None, "Must Not Commit"),
            Err(WorkspaceInitializationError::Conflict)
        ));
        assert!(!workspace.path().join("rehow.yaml").exists());
        assert!(workspace.path().join("categories").is_dir());
    }

    #[test]
    fn initialization_reports_and_preserves_a_hostile_marker_cleanup_race() {
        let workspace = TestDirectory::new("initialize-marker-cleanup-race");
        let state = TestDirectory::new("initialize-marker-cleanup-race-state");
        let target = empty_target(&workspace);
        let marker = workspace.path().join("categories/.rehow-init");
        let marker_for_hook = marker.clone();
        crate::storage::set_before_initialization_marker_cleanup_hook(move || {
            fs::remove_file(&marker_for_hook).unwrap();
            symlink("foreign-target", &marker_for_hook).unwrap();
        });

        assert!(matches!(
            initialize_workspace(&target, state.path(), None, "Committed Rehow"),
            Err(WorkspaceInitializationError::CommittedRefreshRequired { .. })
        ));
        assert!(!marker.exists());
        let retired = fs::read_dir(workspace.path().join(".rehow/trash"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("rehow.yaml");
        assert!(
            fs::symlink_metadata(&retired)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let WorkspaceInspection::Compatible(view) =
            inspect_workspace(workspace.path()).unwrap().into_parts().1
        else {
            panic!("expected a compatible Workspace after preserving the raced marker");
        };
        assert!(view.issues().is_empty());
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
    fn updates_category_name_and_optional_summary_atomically() {
        let (workspace, state, mut view) = initialized_workspace("category-details");
        let created = create_category(&mut view, state.path(), None, "Linux", None).unwrap();
        let original_directory = view.categories()[0].relative_directory().to_owned();
        let updated = update_category_details(
            &mut view,
            state.path(),
            None,
            created.category_id(),
            created.revision(),
            "Linux System".to_owned(),
            Some("Desktop and system work".to_owned()),
        )
        .unwrap();
        assert!(updated.changed());
        let category = view.categories()[0].category();
        assert_eq!(category.name(), "Linux System");
        assert_eq!(category.summary(), Some("Desktop and system work"));
        assert_eq!(
            view.categories()[0].relative_directory(),
            original_directory
        );
        assert!(workspace.path().join(original_directory).is_dir());

        let cleared = update_category_details(
            &mut view,
            state.path(),
            None,
            created.category_id(),
            updated.revision(),
            "Linux System".to_owned(),
            None,
        )
        .unwrap();
        assert!(cleared.changed());
        assert_eq!(view.categories()[0].category().summary(), None);

        assert_eq!(
            update_category_details(
                &mut view,
                state.path(),
                None,
                created.category_id(),
                cleared.revision(),
                "Linux System".to_owned(),
                Some(" ".to_owned()),
            ),
            Err(CategoryUpdateError::InvalidSummary)
        );
    }

    #[test]
    fn reorders_complete_category_effect_set_and_preserves_content() {
        let (workspace, state, mut view) = initialized_workspace("category-reorder");
        let first = create_category(&mut view, state.path(), None, "One", None).unwrap();
        let second = create_category(&mut view, state.path(), None, "Two", None).unwrap();
        let third = create_category(&mut view, state.path(), None, "Three", None).unwrap();
        create_guide(
            &mut view,
            state.path(),
            None,
            first.category_id(),
            "First Guide",
            "Keep this Guide in place.",
        )
        .unwrap();
        let directories = view
            .categories()
            .iter()
            .map(|category| {
                (
                    *category.category().id(),
                    category.relative_directory().to_owned(),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let revision_for = |id: CategoryId, view: &WorkspaceView| {
            view.categories()
                .iter()
                .find(|category| category.category().id() == &id)
                .unwrap()
                .descriptor_revision()
        };
        let ordered = [
            CategoryOrderRevision::new(
                third.category_id(),
                revision_for(third.category_id(), &view),
            ),
            CategoryOrderRevision::new(
                first.category_id(),
                revision_for(first.category_id(), &view),
            ),
            CategoryOrderRevision::new(
                second.category_id(),
                revision_for(second.category_id(), &view),
            ),
        ];
        let reordered = reorder_categories(&mut view, state.path(), None, &ordered).unwrap();
        assert!(reordered.changed());
        assert_eq!(
            view.categories()
                .iter()
                .map(|category| category.category().name())
                .collect::<Vec<_>>(),
            ["Three", "One", "Two"]
        );
        assert_eq!(
            view.categories()
                .iter()
                .map(|category| category.category().order())
                .collect::<Vec<_>>(),
            [1_000, 2_000, 3_000]
        );
        for category in view.categories() {
            assert_eq!(
                category.relative_directory(),
                directories[category.category().id()]
            );
        }
        assert_eq!(
            view.categories()
                .iter()
                .find(|category| category.category().id() == &first.category_id())
                .unwrap()
                .guides()
                .len(),
            1
        );

        let current = view
            .categories()
            .iter()
            .map(|category| {
                CategoryOrderRevision::new(
                    *category.category().id(),
                    category.descriptor_revision(),
                )
            })
            .collect::<Vec<_>>();
        assert!(
            !reorder_categories(&mut view, state.path(), None, &current)
                .unwrap()
                .changed()
        );
        assert!(
            workspace
                .path()
                .join(directories.get(&first.category_id()).unwrap())
                .is_dir()
        );
    }

    #[test]
    fn category_reorder_rejects_stale_or_incomplete_effect_sets() {
        let (workspace, state, mut view) = initialized_workspace("category-reorder-conflict");
        let first = create_category(&mut view, state.path(), None, "One", None).unwrap();
        let second = create_category(&mut view, state.path(), None, "Two", None).unwrap();
        let stale = [
            CategoryOrderRevision::new(second.category_id(), second.revision()),
            CategoryOrderRevision::new(first.category_id(), first.revision()),
        ];
        rename_category(
            &mut view,
            state.path(),
            None,
            first.category_id(),
            first.revision(),
            "First".to_owned(),
        )
        .unwrap();
        assert_eq!(
            reorder_categories(&mut view, state.path(), None, &stale),
            Err(CategoryReorderError::RevisionConflict)
        );
        let duplicate = [
            CategoryOrderRevision::new(
                first.category_id(),
                view.categories()
                    .iter()
                    .find(|category| category.category().id() == &first.category_id())
                    .unwrap()
                    .descriptor_revision(),
            ),
            CategoryOrderRevision::new(
                first.category_id(),
                view.categories()
                    .iter()
                    .find(|category| category.category().id() == &first.category_id())
                    .unwrap()
                    .descriptor_revision(),
            ),
        ];
        assert_eq!(
            reorder_categories(&mut view, state.path(), None, &duplicate),
            Err(CategoryReorderError::InvalidEffectSet)
        );

        let current = view
            .categories()
            .iter()
            .map(|category| {
                CategoryOrderRevision::new(
                    *category.category().id(),
                    category.descriptor_revision(),
                )
            })
            .collect::<Vec<_>>();
        fs::create_dir(workspace.path().join("categories/external-residue")).unwrap();
        assert_eq!(
            reorder_categories(&mut view, state.path(), None, &current),
            Err(CategoryReorderError::RevisionConflict)
        );
    }

    #[test]
    fn category_reorder_rechecks_manifest_identity_under_the_writer_lock() {
        let (workspace, state, mut view) = initialized_workspace("category-reorder-manifest-race");
        create_category(&mut view, state.path(), None, "One", None).unwrap();
        create_category(&mut view, state.path(), None, "Two", None).unwrap();
        let expected_snapshot = category_inventory_snapshot(&view);
        let ordered = view
            .categories()
            .iter()
            .rev()
            .map(|category| {
                CategoryOrderRevision::new(
                    *category.category().id(),
                    category.descriptor_revision(),
                )
            })
            .collect::<Vec<_>>();
        let descriptors = view
            .categories()
            .iter()
            .map(|category| {
                workspace
                    .path()
                    .join(category.relative_directory())
                    .join("category.yaml")
            })
            .collect::<Vec<_>>();
        let before = descriptors
            .iter()
            .map(fs::read)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let manifest_path = workspace.path().join("rehow.yaml");
        let cancellation_checks = std::cell::Cell::new(0_u8);

        assert_eq!(
            reorder_categories_at_snapshot(
                &mut view,
                state.path(),
                None,
                expected_snapshot,
                || {
                    let check = cancellation_checks.get();
                    cancellation_checks.set(check + 1);
                    if check == 2 {
                        let mut manifest = fs::read_to_string(&manifest_path).unwrap();
                        manifest.push_str("external_key: changed\n");
                        fs::write(&manifest_path, manifest).unwrap();
                    }
                    false
                },
                &ordered,
            ),
            Err(CategoryReorderError::RevisionConflict)
        );
        assert_eq!(cancellation_checks.get(), 3);
        let after = descriptors
            .iter()
            .map(fs::read)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn category_reorder_honors_cancellation_before_writing() {
        let (workspace, state, mut view) = initialized_workspace("category-reorder-cancelled");
        create_category(&mut view, state.path(), None, "One", None).unwrap();
        create_category(&mut view, state.path(), None, "Two", None).unwrap();
        let expected_snapshot = category_inventory_snapshot(&view);
        let ordered = view
            .categories()
            .iter()
            .rev()
            .map(|category| {
                CategoryOrderRevision::new(
                    *category.category().id(),
                    category.descriptor_revision(),
                )
            })
            .collect::<Vec<_>>();
        let descriptor = workspace
            .path()
            .join(view.categories()[0].relative_directory())
            .join("category.yaml");
        let before = fs::read(&descriptor).unwrap();

        assert_eq!(
            reorder_categories_at_snapshot(
                &mut view,
                state.path(),
                None,
                expected_snapshot,
                || true,
                &ordered,
            ),
            Err(CategoryReorderError::Cancelled)
        );
        assert_eq!(fs::read(descriptor).unwrap(), before);
    }

    #[test]
    fn removes_empty_category_to_trash_without_touching_others() {
        let (workspace, state, mut view) = initialized_workspace("category-remove");
        let removed = create_category(&mut view, state.path(), None, "Remove Me", None).unwrap();
        let kept = create_category(&mut view, state.path(), None, "Keep Me", None).unwrap();
        let loaded = view
            .categories()
            .iter()
            .find(|category| category.category().id() == &removed.category_id())
            .unwrap();
        let relative_directory = loaded.relative_directory().to_owned();
        let descriptor = fs::read(
            workspace
                .path()
                .join(&relative_directory)
                .join("category.yaml"),
        )
        .unwrap();
        remove_category(
            &mut view,
            state.path(),
            None,
            removed.category_id(),
            removed.revision(),
        )
        .unwrap();
        assert!(!workspace.path().join(&relative_directory).exists());
        assert_eq!(view.categories().len(), 1);
        assert_eq!(view.categories()[0].category().id(), &kept.category_id());
        let operations = fs::read_dir(workspace.path().join(".rehow/trash"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let removed_copy = operations
            .iter()
            .map(|operation| {
                operation
                    .path()
                    .join(&relative_directory)
                    .join("category.yaml")
            })
            .find(|path| path.is_file())
            .unwrap();
        assert_eq!(fs::read(removed_copy).unwrap(), descriptor);
    }

    #[test]
    fn category_remove_honors_cancellation_and_final_manifest_recheck() {
        let (workspace, state, mut view) = initialized_workspace("category-remove-gates");
        let removed = create_category(&mut view, state.path(), None, "Remove Me", None).unwrap();
        let expected_snapshot = category_inventory_snapshot(&view);
        let relative_directory = view
            .categories()
            .iter()
            .find(|category| category.category().id() == &removed.category_id())
            .unwrap()
            .relative_directory()
            .to_owned();
        let descriptor = workspace
            .path()
            .join(&relative_directory)
            .join("category.yaml");
        let before = fs::read(&descriptor).unwrap();

        assert_eq!(
            remove_category_at_snapshot(
                &mut view,
                state.path(),
                None,
                expected_snapshot,
                || true,
                removed.category_id(),
                removed.revision(),
            ),
            Err(CategoryRemoveError::Cancelled)
        );
        assert_eq!(fs::read(&descriptor).unwrap(), before);

        let manifest_path = workspace.path().join("rehow.yaml");
        let checks = std::cell::Cell::new(0_u8);
        assert_eq!(
            remove_category_at_snapshot(
                &mut view,
                state.path(),
                None,
                expected_snapshot,
                || {
                    let check = checks.get();
                    checks.set(check + 1);
                    if check == 4 {
                        let mut manifest = fs::read_to_string(&manifest_path).unwrap();
                        manifest.push_str("external_key: changed\n");
                        fs::write(&manifest_path, manifest).unwrap();
                    }
                    false
                },
                removed.category_id(),
                removed.revision(),
            ),
            Err(CategoryRemoveError::RevisionConflict)
        );
        assert_eq!(checks.get(), 5);
        assert_eq!(fs::read(descriptor).unwrap(), before);
    }

    #[test]
    fn category_remove_reports_recovery_when_foreign_content_arrives_after_final_scan() {
        let (workspace, state, mut view) = initialized_workspace("category-remove-race");
        let removed = create_category(&mut view, state.path(), None, "Remove Me", None).unwrap();
        let expected_snapshot = category_inventory_snapshot(&view);
        let relative_directory = view.categories()[0].relative_directory().to_owned();
        let descriptor = workspace
            .path()
            .join(&relative_directory)
            .join("category.yaml");
        let foreign = workspace
            .path()
            .join(&relative_directory)
            .join("external-race.md");
        let before = fs::read(&descriptor).unwrap();
        let checks = std::cell::Cell::new(0_u8);
        let result = remove_category_at_snapshot(
            &mut view,
            state.path(),
            None,
            expected_snapshot,
            || {
                let check = checks.get();
                checks.set(check + 1);
                if check == 5 {
                    fs::write(&foreign, b"external content").unwrap();
                }
                false
            },
            removed.category_id(),
            removed.revision(),
        );
        let Err(CategoryRemoveError::RecoveryRequired { operation_id }) = result else {
            panic!("post-mutation conflict must require recovery: {result:?}");
        };
        assert_eq!(checks.get(), 6);
        assert_eq!(fs::read(&foreign).unwrap(), b"external content");
        assert!(!descriptor.exists());
        assert_eq!(
            fs::read(
                workspace
                    .path()
                    .join(".rehow/trash")
                    .join(operation_id.to_string())
                    .join(&relative_directory)
                    .join("category.yaml")
            )
            .unwrap(),
            before
        );
    }

    #[test]
    fn category_remove_refuses_nonempty_stale_and_extra_entries() {
        let (workspace, state, mut view) = initialized_workspace("category-remove-refuse");
        let nonempty = create_category(&mut view, state.path(), None, "Guides", None).unwrap();
        create_guide(
            &mut view,
            state.path(),
            None,
            nonempty.category_id(),
            "Keep Guide",
            "Removal must never cascade.",
        )
        .unwrap();
        assert_eq!(
            remove_category(
                &mut view,
                state.path(),
                None,
                nonempty.category_id(),
                nonempty.revision(),
            ),
            Err(CategoryRemoveError::NotEmpty)
        );

        let extra = create_category(&mut view, state.path(), None, "Extra", None).unwrap();
        let extra_directory = view
            .categories()
            .iter()
            .find(|category| category.category().id() == &extra.category_id())
            .unwrap()
            .relative_directory()
            .to_owned();
        fs::write(
            workspace.path().join(&extra_directory).join("unknown.txt"),
            b"keep",
        )
        .unwrap();
        assert_eq!(
            remove_category(
                &mut view,
                state.path(),
                None,
                extra.category_id(),
                extra.revision(),
            ),
            Err(CategoryRemoveError::NotEmpty)
        );
        assert_eq!(
            fs::read(workspace.path().join(extra_directory).join("unknown.txt")).unwrap(),
            b"keep"
        );

        assert_eq!(
            remove_category(
                &mut view,
                state.path(),
                None,
                extra.category_id(),
                Revision::from_str(&format!("rev:v1:{}", "22".repeat(32))).unwrap(),
            ),
            Err(CategoryRemoveError::RevisionConflict)
        );
    }

    #[test]
    fn refuses_to_remove_the_default_category() {
        let workspace = TestDirectory::new("default-category-remove");
        let state = TestDirectory::new("default-category-remove-state");
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
        let WorkspaceInspection::Compatible(mut view) =
            inspect_workspace(workspace.path()).unwrap().into_parts().1
        else {
            panic!("expected compatible Workspace");
        };
        let revision = view.categories()[0].descriptor_revision();
        assert_eq!(
            remove_category(
                &mut view,
                state.path(),
                None,
                CategoryId::from_str("0199a1c2-d334-7d17-bd9c-f8b6ba3a5551").unwrap(),
                revision,
            ),
            Err(CategoryRemoveError::DefaultCategory)
        );
        assert!(category.join("category.yaml").is_file());
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
    fn category_archive_rechecks_external_default_change_under_the_writer_lock() {
        let (workspace, state, mut view) = initialized_workspace("category-default-race");
        let created = create_category(&mut view, state.path(), None, "Linux System", None).unwrap();
        let manifest_path = workspace.path().join("rehow.yaml");
        let mut manifest = fs::read_to_string(&manifest_path).unwrap();
        manifest.push_str(&format!(
            "settings:\n  default_category_id: {}\n",
            created.category_id()
        ));
        fs::write(&manifest_path, manifest).unwrap();

        assert_eq!(
            set_category_archived(
                &mut view,
                state.path(),
                None,
                created.category_id(),
                created.revision(),
                true,
            ),
            Err(CategoryUpdateError::RevisionConflict)
        );
        let inspection = inspect_workspace(workspace.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(current) = inspection else {
            panic!("expected compatible Workspace");
        };
        assert_eq!(
            current.categories()[0].category().status(),
            CategoryStatus::Active
        );
        assert_eq!(
            current
                .manifest()
                .settings()
                .and_then(|settings| settings.default_category_id()),
            Some(&created.category_id())
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
    fn category_creation_rechecks_the_directory_limit_under_the_writer_lock() {
        let (workspace, state, mut view) = initialized_workspace("category-create-stale-limit");
        create_category(&mut view, state.path(), None, "Existing", None).unwrap();
        let expected_snapshot = category_inventory_snapshot(&view);
        let categories = workspace.path().join("categories");
        for index in 1..crate::storage::CATEGORY_LIMIT {
            fs::create_dir(categories.join(format!("external-{index:04}"))).unwrap();
        }

        assert_eq!(
            create_category_at_snapshot(
                &mut view,
                state.path(),
                None,
                expected_snapshot,
                "Must Not Exist",
                None,
            ),
            Err(CategoryCreateError::ResourceLimit)
        );
        let names = fs::read_dir(&categories)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), crate::storage::CATEGORY_LIMIT);
        assert!(names.iter().all(|name| !name.starts_with("must-not-exist")));
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
    fn imported_guide_creation_keeps_source_frontmatter_in_the_draft_body() {
        let (_workspace, state, mut view) = initialized_workspace("guide-import-body");
        let category = create_category(&mut view, state.path(), None, "Imported", None)
            .unwrap()
            .category_id();
        let body = b"---\ntitle: Untrusted source\nstatus: tested\n---\n\n# Instructions\n";

        let created = create_guide_with_body(
            &mut view,
            state.path(),
            None,
            category,
            "Source Guide",
            "Imported from source.md",
            body,
        )
        .unwrap();
        let loaded = &view.categories()[0].guides()[0];

        assert_eq!(loaded.guide().id(), &created.guide_id());
        assert_eq!(loaded.guide().declared_status(), GuideStatus::Draft);
        assert_eq!(loaded.guide().body(), body);
        assert_eq!(loaded.guide().title(), "Source Guide");
        assert_eq!(loaded.guide().summary(), "Imported from source.md");
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
    fn guide_creation_rechecks_the_global_candidate_limit_under_the_writer_lock() {
        let (workspace, state, mut view) = initialized_workspace("guide-create-stale-limit");
        let category = create_category(&mut view, state.path(), None, "Linux System", None)
            .unwrap()
            .category_id();
        let category_revision = view.categories()[0].descriptor_revision();
        let category_path = workspace
            .path()
            .join(view.categories()[0].relative_directory());
        for index in 0..crate::storage::GUIDE_COUNT_LIMIT {
            fs::write(category_path.join(format!("external-{index:05}.md")), []).unwrap();
        }

        assert_eq!(
            create_guide_at_revision(
                &mut view,
                state.path(),
                None,
                category,
                category_revision,
                "Must Not Exist",
                "The retained view is stale at the hard Guide limit.",
            ),
            Err(GuideCreateError::ResourceLimit)
        );
        let guide_names = fs::read_dir(&category_path)
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".md"))
            .collect::<Vec<_>>();
        assert_eq!(guide_names.len(), crate::storage::GUIDE_COUNT_LIMIT);
        assert!(
            guide_names
                .iter()
                .all(|name| !name.starts_with("must-not-exist"))
        );
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
    fn native_approval_binds_exact_content_and_external_change_is_effectively_stale() {
        let (workspace, state, mut view) = fixture("tested-approval", GUIDE);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let original_revision = revision(&view);
        let approved = approve_guide_tested_with_context(
            &mut view,
            state.path(),
            None,
            guide_id,
            original_revision,
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e30"),
            at(),
        )
        .unwrap();
        assert!(approved.changed());
        let loaded = &view.categories()[0].guides()[0];
        assert_eq!(loaded.guide().declared_status(), GuideStatus::Tested);
        assert_eq!(loaded.effective_status(), GuideStatus::Tested);
        let verification = loaded.guide().verification().expect("verification pair");
        assert_eq!(verification.last_verified_at(), &at());
        let approved_hash = *verification
            .verified_content_hash()
            .expect("recognized approval hash");
        assert!(
            fs::read_to_string(workspace.path().join(GUIDE_PATH))
                .unwrap()
                .contains(&approved_hash.to_string())
        );

        let no_op = approve_guide_tested_with_context(
            &mut view,
            state.path(),
            None,
            guide_id,
            approved.revision(),
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e31"),
            at(),
        )
        .unwrap();
        assert!(!no_op.changed());
        assert_eq!(no_op.revision(), approved.revision());
        assert_eq!(
            approve_guide_tested_with_context(
                &mut view,
                state.path(),
                None,
                guide_id,
                original_revision,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e32"),
                at(),
            ),
            Err(GuideUpdateError::RevisionConflict)
        );

        let path = workspace.path().join(GUIDE_PATH);
        let changed = fs::read_to_string(&path)
            .unwrap()
            .replace("Extract the archive.", "Extract a different archive.");
        fs::write(&path, changed).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let inspection = inspect_workspace(workspace.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(mut reopened) = inspection else {
            panic!("expected compatible Workspace");
        };
        let loaded = &reopened.categories()[0].guides()[0];
        assert_eq!(loaded.guide().declared_status(), GuideStatus::Tested);
        assert_eq!(loaded.effective_status(), GuideStatus::Stale);
        assert!(reopened.issues().iter().any(|issue| {
            issue.kind() == crate::storage::WorkspaceIssueKind::VerificationMismatch
        }));
        let changed_revision = loaded.revision();
        let reapproved = approve_guide_tested_with_context(
            &mut reopened,
            state.path(),
            None,
            guide_id,
            changed_revision,
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e36"),
            at(),
        )
        .unwrap();
        assert!(reapproved.changed());
        assert_eq!(
            reopened.categories()[0].guides()[0].effective_status(),
            GuideStatus::Tested
        );
    }

    #[test]
    fn adapter_tested_approval_validates_and_cancels_before_the_journal_boundary() {
        let (workspace, state, mut view) = fixture("tested-adapter-cancel", GUIDE);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        validate_guide_tested_approval(&view, guide_id, expected).unwrap();
        let before = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        let checks = std::cell::Cell::new(0_u8);
        assert_eq!(
            approve_guide_tested_after_native_confirmation(
                &mut view,
                state.path(),
                None,
                guide_id,
                expected,
                || {
                    let next = checks.get() + 1;
                    checks.set(next);
                    next >= 4
                },
            ),
            Err(GuideTestedApprovalError::Cancelled)
        );
        assert_eq!(fs::read(workspace.path().join(GUIDE_PATH)).unwrap(), before);
        assert_eq!(revision(&view), expected);

        let approved = approve_guide_tested_after_native_confirmation(
            &mut view,
            state.path(),
            None,
            guide_id,
            expected,
            || false,
        )
        .unwrap();
        assert!(approved.changed());
        assert_eq!(
            validate_guide_tested_approval(&view, guide_id, approved.revision()),
            Err(GuideTestedApprovalError::AlreadyTested)
        );
    }

    #[test]
    fn tested_approval_refuses_archived_guides() {
        let (_workspace, state, mut view) = fixture("tested-approval-archived", GUIDE);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        let archived =
            set_guide_archived(&mut view, state.path(), None, guide_id, expected, true).unwrap();
        assert_eq!(
            approve_guide_tested_with_context(
                &mut view,
                state.path(),
                None,
                guide_id,
                archived.revision(),
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e33"),
                at(),
            ),
            Err(GuideUpdateError::InvalidEdit)
        );
        assert_eq!(
            view.categories()[0].guides()[0].effective_status(),
            GuideStatus::Archived
        );
    }

    #[test]
    fn tested_approval_refuses_guides_without_ready_steps() {
        let not_ready = GUIDE.replace("## Steps", "## Notes");
        let (workspace, state, mut view) = fixture("tested-approval-not-ready", &not_ready);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        let before = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        assert_eq!(
            approve_guide_tested_with_context(
                &mut view,
                state.path(),
                None,
                guide_id,
                expected,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e34"),
                at(),
            ),
            Err(GuideUpdateError::NotReady)
        );
        assert_eq!(fs::read(workspace.path().join(GUIDE_PATH)).unwrap(), before);
    }

    #[test]
    fn future_verification_hash_loads_as_effectively_stale() {
        let future = GUIDE.replace(
            "status: draft\n",
            &format!(
                "status: tested\nlast_verified_at: \"2026-09-05T10:03:00Z\"\nverified_content_hash: verify:v2:{}\n",
                "1a".repeat(32)
            ),
        );
        let (_workspace, _state, view) = fixture("future-verification", &future);
        let loaded = &view.categories()[0].guides()[0];
        assert_eq!(loaded.guide().declared_status(), GuideStatus::Tested);
        assert_eq!(loaded.effective_status(), GuideStatus::Stale);
        assert!(
            loaded
                .guide()
                .verification()
                .is_some_and(|verification| verification.verified_content_hash().is_none())
        );
        assert!(view.issues().iter().any(|issue| {
            issue.kind() == crate::storage::WorkspaceIssueKind::VerificationMismatch
        }));
    }

    #[test]
    fn tested_approval_preserves_crlf_line_endings() {
        let crlf = GUIDE.replace('\n', "\r\n");
        let (workspace, state, mut view) = fixture("tested-approval-crlf", &crlf);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        let approved = approve_guide_tested_with_context(
            &mut view,
            state.path(),
            None,
            guide_id,
            expected,
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e35"),
            at(),
        )
        .unwrap();
        assert!(approved.changed());
        let bytes = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        assert!(bytes.windows(2).any(|window| window == b"\r\n"));
        assert!(
            !bytes
                .iter()
                .enumerate()
                .any(|(index, byte)| *byte == b'\n' && (index == 0 || bytes[index - 1] != b'\r'))
        );
    }

    #[test]
    fn stale_approval_succeeds_and_unsafe_insertion_is_atomic() {
        let stale = GUIDE.replace("status: draft", "status: stale");
        let (_workspace, state, mut view) = fixture("tested-approval-stale", &stale);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        approve_guide_tested_with_context(
            &mut view,
            state.path(),
            None,
            guide_id,
            expected,
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e37"),
            at(),
        )
        .unwrap();
        assert_eq!(
            view.categories()[0].guides()[0].effective_status(),
            GuideStatus::Tested
        );

        let commented = GUIDE.replace("created_at:", "# approval boundary\ncreated_at:");
        let (workspace, state, mut view) = fixture("tested-approval-comment", &commented);
        let expected = revision(&view);
        let before = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        assert_eq!(
            approve_guide_tested_with_context(
                &mut view,
                state.path(),
                None,
                guide_id,
                expected,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e38"),
                at(),
            ),
            Err(GuideUpdateError::UnsafeRoundTrip)
        );
        assert_eq!(fs::read(workspace.path().join(GUIDE_PATH)).unwrap(), before);
    }

    #[test]
    fn tested_confirmation_text_neutralizes_spoofing_controls() {
        assert_eq!(
            guide_tested_confirmation(
                "bad\nline\u{202e}name",
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e10"
            ),
            "Confirm that you tried these instructions successfully.\n\nTitle: bad�line�name\nID: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\n\nRehow will bind Tested status to the exact current content."
        );
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
    fn detail_update_atomically_replaces_tags_and_aliases() {
        let (workspace, state, mut view) = fixture("tag-alias-update", GUIDE);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        let category_id = CategoryId::from_str(CATEGORY_ID).unwrap();
        let outcome = update_guide_details(
            &mut view,
            state.path(),
            None,
            guide_id,
            expected,
            category_id,
            "Install a Tarball".to_owned(),
            "Install a downloaded archive.".to_owned(),
            vec!["linux".to_owned(), "Wayland".to_owned()],
            vec!["manual install".to_owned(), "tar app".to_owned()],
        )
        .unwrap();
        assert!(outcome.changed());
        let loaded = &view.categories()[0].guides()[0];
        assert_eq!(loaded.guide().tags(), ["linux", "Wayland"]);
        assert_eq!(loaded.guide().aliases(), ["manual install", "tar app"]);
        assert_eq!(loaded.guide().body(), b"## Steps\n\nExtract the archive.\n");
        assert_eq!(loaded.relative_path(), GUIDE_PATH);
        let bytes = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        assert!(
            bytes
                .windows(19)
                .any(|value| value == b"future_key: keep-me")
        );

        let before = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        assert_eq!(
            update_guide_details(
                &mut view,
                state.path(),
                None,
                guide_id,
                outcome.revision(),
                category_id,
                "Changed title".to_owned(),
                "Changed summary".to_owned(),
                vec!["Maße".to_owned(), "MASSE".to_owned()],
                vec![],
            ),
            Err(GuideUpdateError::InvalidEdit)
        );
        assert_eq!(fs::read(workspace.path().join(GUIDE_PATH)).unwrap(), before);
    }

    #[test]
    fn moves_a_guide_to_an_active_category_without_changing_its_filename_or_body() {
        let (workspace, state, mut view) = initialized_workspace("guide-category-move");
        let source_category = create_category(&mut view, state.path(), None, "Source", None)
            .unwrap()
            .category_id();
        let destination_category =
            create_category(&mut view, state.path(), None, "Destination", None)
                .unwrap()
                .category_id();
        let created = create_guide(
            &mut view,
            state.path(),
            None,
            source_category,
            "Move This Guide",
            "Keep every byte that is not organizational.",
        )
        .unwrap();
        let guide_id = created.guide_id();
        let (source_path, original_body) = view
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .find(|guide| guide.guide().id() == &guide_id)
            .map(|guide| {
                (
                    guide.relative_path().to_owned(),
                    guide.guide().body().to_owned(),
                )
            })
            .unwrap();
        let filename = source_path.rsplit_once('/').unwrap().1.to_owned();
        let destination_directory = view
            .categories()
            .iter()
            .find(|category| category.category().id() == &destination_category)
            .unwrap()
            .relative_directory()
            .to_owned();
        let destination_path = format!("{destination_directory}/{filename}");

        let moved = update_guide_details(
            &mut view,
            state.path(),
            None,
            guide_id,
            created.revision(),
            destination_category,
            "Moved Guide".to_owned(),
            "The Category and details changed together.".to_owned(),
            vec!["linux".to_owned()],
            vec!["category move".to_owned()],
        )
        .unwrap();
        assert!(moved.changed());
        assert!(!workspace.path().join(&source_path).exists());
        assert!(workspace.path().join(&destination_path).is_file());
        let matches = view
            .categories()
            .iter()
            .flat_map(|category| {
                category
                    .guides()
                    .iter()
                    .filter(|guide| guide.guide().id() == &guide_id)
                    .map(move |guide| (category, guide))
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0.category().id(), &destination_category);
        assert_eq!(matches[0].1.relative_path(), destination_path);
        assert_eq!(matches[0].1.revision(), moved.revision());
        assert_eq!(matches[0].1.guide().title(), "Moved Guide");
        assert_eq!(matches[0].1.guide().body(), original_body);
        assert_eq!(matches[0].1.guide().tags(), ["linux"]);
    }

    #[test]
    fn guide_move_refuses_inactive_missing_stale_and_occupied_destinations() {
        let (workspace, state, mut view) = initialized_workspace("guide-move-refusal");
        let source = create_category(&mut view, state.path(), None, "Source", None)
            .unwrap()
            .category_id();
        let destination = create_category(&mut view, state.path(), None, "Destination", None)
            .unwrap()
            .category_id();
        let created = create_guide(
            &mut view,
            state.path(),
            None,
            source,
            "Collision Guide",
            "The source must survive every refusal.",
        )
        .unwrap();
        let guide_id = created.guide_id();
        let source_path = view
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .find(|guide| guide.guide().id() == &guide_id)
            .unwrap()
            .relative_path()
            .to_owned();
        let details = || {
            (
                "Collision Guide".to_owned(),
                "The source must survive every refusal.".to_owned(),
                Vec::new(),
                Vec::new(),
            )
        };
        let missing = CategoryId::from_str("0199a1c2-d334-7d17-bd9c-f8b6ba3a5999").unwrap();
        let (title, summary, tags, aliases) = details();
        assert_eq!(
            update_guide_details(
                &mut view,
                state.path(),
                None,
                guide_id,
                created.revision(),
                missing,
                title,
                summary,
                tags,
                aliases,
            ),
            Err(GuideUpdateError::DestinationCategoryNotFound)
        );

        let destination_revision = view
            .categories()
            .iter()
            .find(|category| category.category().id() == &destination)
            .unwrap()
            .descriptor_revision();
        set_category_archived(
            &mut view,
            state.path(),
            None,
            destination,
            destination_revision,
            true,
        )
        .unwrap();
        let (title, summary, tags, aliases) = details();
        assert_eq!(
            update_guide_details(
                &mut view,
                state.path(),
                None,
                guide_id,
                created.revision(),
                destination,
                title,
                summary,
                tags,
                aliases,
            ),
            Err(GuideUpdateError::DestinationCategoryArchived)
        );
        let destination_revision = view
            .categories()
            .iter()
            .find(|category| category.category().id() == &destination)
            .unwrap()
            .descriptor_revision();
        set_category_archived(
            &mut view,
            state.path(),
            None,
            destination,
            destination_revision,
            false,
        )
        .unwrap();

        let destination_directory = view
            .categories()
            .iter()
            .find(|category| category.category().id() == &destination)
            .unwrap()
            .relative_directory()
            .to_owned();
        let filename = source_path.rsplit_once('/').unwrap().1;
        let occupied = workspace.path().join(destination_directory).join(filename);
        fs::write(&occupied, b"occupied").unwrap();
        fs::set_permissions(&occupied, fs::Permissions::from_mode(0o600)).unwrap();
        let (title, summary, tags, aliases) = details();
        assert_eq!(
            update_guide_details(
                &mut view,
                state.path(),
                None,
                guide_id,
                created.revision(),
                destination,
                title,
                summary,
                tags,
                aliases,
            ),
            Err(GuideUpdateError::RevisionConflict)
        );
        assert!(workspace.path().join(&source_path).is_file());

        fs::remove_file(occupied).unwrap();
        let (title, summary, tags, aliases) = details();
        assert_eq!(
            update_guide_details(
                &mut view,
                state.path(),
                None,
                guide_id,
                Revision::from_str(&format!("rev:v1:{}", "11".repeat(32))).unwrap(),
                destination,
                title,
                summary,
                tags,
                aliases,
            ),
            Err(GuideUpdateError::RevisionConflict)
        );
        assert!(workspace.path().join(source_path).is_file());
    }

    #[test]
    fn tested_guide_move_becomes_stale_and_preserves_unknown_metadata() {
        let tested = GUIDE.replacen(
            "status: draft\n",
            "status: tested\nlast_verified_at: \"2026-09-05T10:03:00Z\"\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
            1,
        );
        let (workspace, state, mut view) = fixture("tested-guide-move", &tested);
        let destination_id = CategoryId::from_str("0199a1c2-d334-7d17-bd9c-f8b6ba3a5552").unwrap();
        let destination_directory = "categories/destination--b6ba3a5552";
        fs::create_dir(workspace.path().join(destination_directory)).unwrap();
        fs::set_permissions(
            workspace.path().join(destination_directory),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let descriptor = CATEGORY
            .replace(CATEGORY_ID, &destination_id.to_string())
            .replace("name: Linux System", "name: Destination");
        fs::write(
            workspace
                .path()
                .join(destination_directory)
                .join("category.yaml"),
            descriptor,
        )
        .unwrap();
        fs::set_permissions(
            workspace
                .path()
                .join(destination_directory)
                .join("category.yaml"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        refresh_workspace(&mut view).unwrap();
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = view
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .find(|guide| guide.guide().id() == &guide_id)
            .unwrap()
            .revision();
        update_guide_details(
            &mut view,
            state.path(),
            None,
            guide_id,
            expected,
            destination_id,
            "Install a Tarball".to_owned(),
            "Install a downloaded archive.".to_owned(),
            vec!["linux".to_owned()],
            vec!["manual install".to_owned()],
        )
        .unwrap();
        let moved = view
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .find(|guide| guide.guide().id() == &guide_id)
            .unwrap();
        assert_eq!(moved.guide().declared_status(), GuideStatus::Stale);
        assert!(moved.guide().verification().is_some());
        let bytes = fs::read(workspace.path().join(moved.relative_path())).unwrap();
        assert!(
            bytes
                .windows(19)
                .any(|value| value == b"future_key: keep-me")
        );
    }

    #[test]
    fn removes_an_attachment_free_guide_to_operation_bound_trash() {
        let (workspace, state, mut view) = initialized_workspace("guide-remove");
        let category_id = create_category(&mut view, state.path(), None, "Guides", None)
            .unwrap()
            .category_id();
        let created = create_guide(
            &mut view,
            state.path(),
            None,
            category_id,
            "Remove This Guide",
            "Recoverable deletion.",
        )
        .unwrap();
        let guide_id = created.guide_id();
        let relative_path = view
            .categories()
            .iter()
            .flat_map(|category| category.guides())
            .find(|guide| guide.guide().id() == &guide_id)
            .unwrap()
            .relative_path()
            .to_owned();
        let original = fs::read(workspace.path().join(&relative_path)).unwrap();

        remove_guide(&mut view, state.path(), None, guide_id, created.revision()).unwrap();

        assert!(!workspace.path().join(&relative_path).exists());
        assert!(
            !view
                .categories()
                .iter()
                .flat_map(|category| category.guides())
                .any(|guide| guide.guide().id() == &guide_id)
        );
        let operations = fs::read_dir(workspace.path().join(".rehow/trash"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let removed_copy = operations
            .iter()
            .map(|operation| operation.path().join(&relative_path))
            .find(|path| path.is_file())
            .unwrap();
        assert_eq!(fs::read(removed_copy).unwrap(), original);
    }

    #[test]
    fn guide_remove_refuses_stale_revision_and_attachment_metadata() {
        let with_attachment = GUIDE.replacen(
            "status: draft\n",
            "attachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.desktop\n    title: Example desktop entry\n    sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nstatus: draft\n",
            1,
        );
        let (workspace, state, mut view) = fixture("guide-remove-refusal", &with_attachment);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        assert_eq!(
            remove_guide(
                &mut view,
                state.path(),
                None,
                guide_id,
                Revision::from_str(&format!("rev:v1:{}", "11".repeat(32))).unwrap(),
            ),
            Err(GuideRemoveError::RevisionConflict)
        );
        assert_eq!(
            remove_guide(&mut view, state.path(), None, guide_id, expected),
            Err(GuideRemoveError::HasAttachments)
        );
        assert!(workspace.path().join(GUIDE_PATH).is_file());
        assert!(!workspace.path().join(".rehow/trash").exists());
    }

    #[test]
    fn guide_remove_refuses_a_hardlinked_target() {
        let (workspace, state, mut view) = fixture("guide-remove-hardlink", GUIDE);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let expected = revision(&view);
        fs::hard_link(
            workspace.path().join(GUIDE_PATH),
            workspace.path().join("unexpected-hardlink"),
        )
        .unwrap();

        assert_eq!(
            remove_guide(&mut view, state.path(), None, guide_id, expected),
            Err(GuideRemoveError::UnsafeStorage)
        );
        assert!(workspace.path().join(GUIDE_PATH).is_file());
        assert!(!workspace.path().join(".rehow/trash").exists());
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

    #[test]
    fn archives_and_restores_guide_without_moving_or_losing_content() {
        let (workspace, state, mut view) = fixture("guide-archive", GUIDE);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let original_path = view.categories()[0].guides()[0].relative_path().to_owned();
        let original_body = view.categories()[0].guides()[0].guide().body().to_owned();
        let expected = revision(&view);

        let archived =
            set_guide_archived(&mut view, state.path(), None, guide_id, expected, true).unwrap();
        assert!(archived.changed());
        let loaded = &view.categories()[0].guides()[0];
        assert_eq!(loaded.guide().declared_status(), GuideStatus::Archived);
        assert_eq!(loaded.relative_path(), original_path);
        assert_eq!(loaded.guide().body(), original_body);
        let archived_bytes = fs::read(workspace.path().join(GUIDE_PATH)).unwrap();
        assert!(
            archived_bytes
                .windows(19)
                .any(|value| value == b"future_key: keep-me")
        );

        assert_eq!(
            set_guide_archived(&mut view, state.path(), None, guide_id, expected, false,),
            Err(GuideUpdateError::RevisionConflict)
        );

        let restored = set_guide_archived(
            &mut view,
            state.path(),
            None,
            guide_id,
            archived.revision(),
            false,
        )
        .unwrap();
        assert!(restored.changed());
        let loaded = &view.categories()[0].guides()[0];
        assert_eq!(loaded.guide().declared_status(), GuideStatus::Draft);
        assert_eq!(loaded.relative_path(), original_path);
        assert_eq!(loaded.guide().body(), original_body);
        assert!(workspace.path().join(GUIDE_PATH).is_file());
    }

    #[test]
    fn restoring_an_active_guide_is_an_invalid_edit() {
        let (_workspace, state, mut view) = fixture("guide-restore-active", GUIDE);
        let expected = revision(&view);
        assert_eq!(
            set_guide_archived(
                &mut view,
                state.path(),
                None,
                GuideId::from_str(GUIDE_ID).unwrap(),
                expected,
                false,
            ),
            Err(GuideUpdateError::InvalidEdit)
        );
    }

    #[test]
    fn imports_streamed_attachments_with_collision_and_duplicate_protection() {
        let tested = GUIDE.replacen(
            "status: draft\n",
            "status: tested\nlast_verified_at: \"2026-09-05T10:03:00Z\"\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
            1,
        );
        let (workspace, state, mut view) = fixture("attachment-import", &tested);
        let source_dir = TestDirectory::new("attachment-source");
        let source_path = source_dir.path().join("notes.txt");
        fs::write(&source_path, b"first attachment\n").unwrap();
        fs::set_permissions(&source_path, fs::Permissions::from_mode(0o600)).unwrap();
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let first_id = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509898").unwrap();
        let mut source = open_attachment_source(&source_path).unwrap();
        let expected = revision(&view);
        let first = import_attachment_with_source(
            &mut view,
            state.path(),
            None,
            guide_id,
            expected,
            "notes.txt".to_owned(),
            &mut source,
            first_id,
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e31"),
            at(),
        )
        .unwrap();
        let AttachmentImportOutcome::Imported {
            revision: first_revision,
            ..
        } = first
        else {
            panic!("expected import");
        };
        let first_path = workspace
            .path()
            .join(format!("attachments/{GUIDE_ID}/notes.txt"));
        assert_eq!(fs::read(&first_path).unwrap(), b"first attachment\n");
        assert_eq!(
            fs::metadata(&first_path).unwrap().permissions().mode() & 0o7777,
            0o600
        );
        assert_eq!(
            view.categories()[0].guides()[0].guide().declared_status(),
            GuideStatus::Stale
        );
        assert!(
            fs::read_to_string(workspace.path().join(GUIDE_PATH))
                .unwrap()
                .contains("future_key: keep-me")
        );

        fs::write(&source_path, b"second attachment\n").unwrap();
        let second_id = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509899").unwrap();
        let mut source = open_attachment_source(&source_path).unwrap();
        import_attachment_with_source(
            &mut view,
            state.path(),
            None,
            guide_id,
            first_revision,
            "notes.txt".to_owned(),
            &mut source,
            second_id,
            operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e32"),
            at(),
        )
        .unwrap();
        let loaded = &view.categories()[0].guides()[0];
        let attachments = loaded.guide().attachments().unwrap();
        assert_eq!(attachments.len(), 2);
        assert_eq!(
            attachments[0].path(),
            format!("attachments/{GUIDE_ID}/notes.txt")
        );
        assert_eq!(
            attachments[1].path(),
            format!("attachments/{GUIDE_ID}/notes-2.txt")
        );
        assert_eq!(
            fs::read(
                workspace
                    .path()
                    .join(format!("attachments/{GUIDE_ID}/notes-2.txt"))
            )
            .unwrap(),
            b"second attachment\n"
        );
        assert!(
            fs::read_to_string(workspace.path().join(GUIDE_PATH))
                .unwrap()
                .contains("future_key: keep-me")
        );

        let mut duplicate = open_attachment_source(&source_path).unwrap();
        let duplicate_id = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509900").unwrap();
        let current_revision = revision(&view);
        assert_eq!(
            import_attachment_with_source(
                &mut view,
                state.path(),
                None,
                guide_id,
                current_revision,
                "copy.txt".to_owned(),
                &mut duplicate,
                duplicate_id,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e33"),
                at(),
            ),
            Err(AttachmentImportError::DuplicateAttachment)
        );
        assert!(
            !workspace
                .path()
                .join(format!("attachments/{GUIDE_ID}/copy.txt"))
                .exists()
        );
    }

    #[test]
    fn attachment_import_rejects_source_changes_before_and_during_copy() {
        let (workspace, state, mut view) = fixture("attachment-source-change", GUIDE);
        let source_dir = TestDirectory::new("attachment-changing-source");
        let source_path = source_dir.path().join("changing.bin");
        fs::write(&source_path, b"original bytes").unwrap();
        fs::set_permissions(&source_path, fs::Permissions::from_mode(0o600)).unwrap();
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let category_id = CategoryId::from_str(CATEGORY_ID).unwrap();
        let attachment_id = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509910").unwrap();
        let expected = revision(&view);
        let mut source = open_attachment_source(&source_path).unwrap();
        let source_plan = inspect_attachment_source(&mut source).unwrap();
        let stamp = MutationStamp::from_trusted_time(at());
        let preview = match preview_attachment_import(
            &view,
            state.path(),
            None,
            GUIDE_PATH,
            guide_id,
            expected,
            "changing.bin",
            |bytes, attachment_path| {
                prepare_attachment_candidate(
                    bytes,
                    guide_id,
                    category_id,
                    attachment_id,
                    "changing.bin",
                    attachment_path,
                    source_plan.sha256,
                    source_plan.size,
                    &stamp,
                )
            },
        ) {
            Ok(preview) => preview,
            Err(_) => panic!("expected import preview"),
        };

        fs::write(&source_path, b"modified bytes").unwrap();
        assert_eq!(
            execute_approved_attachment_import(
                &mut view,
                state.path(),
                None,
                guide_id,
                expected,
                "changing.bin".to_owned(),
                &mut source,
                source_plan,
                preview.relative_path().to_owned(),
                preview.revision(),
                attachment_id,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e34"),
                at(),
            ),
            Err(AttachmentImportError::SourceChanged)
        );
        assert!(!workspace.path().join(preview.relative_path()).exists());

        fs::write(&source_path, b"stable source bytes").unwrap();
        let mut source = open_attachment_source(&source_path).unwrap();
        let source_plan = inspect_attachment_source(&mut source).unwrap();
        let attachment_id = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509911").unwrap();
        let preview = match preview_attachment_import(
            &view,
            state.path(),
            None,
            GUIDE_PATH,
            guide_id,
            expected,
            "changing.bin",
            |bytes, attachment_path| {
                prepare_attachment_candidate(
                    bytes,
                    guide_id,
                    category_id,
                    attachment_id,
                    "changing.bin",
                    attachment_path,
                    source_plan.sha256,
                    source_plan.size,
                    &stamp,
                )
            },
        ) {
            Ok(preview) => preview,
            Err(_) => panic!("expected import preview"),
        };
        crate::storage::set_attachment_import_source_changed_fault();
        assert_eq!(
            execute_approved_attachment_import(
                &mut view,
                state.path(),
                None,
                guide_id,
                expected,
                "changing.bin".to_owned(),
                &mut source,
                source_plan,
                preview.relative_path().to_owned(),
                preview.revision(),
                attachment_id,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e35"),
                at(),
            ),
            Err(AttachmentImportError::SourceChanged)
        );
        assert!(!workspace.path().join(preview.relative_path()).exists());
        assert!(
            view.categories()[0].guides()[0]
                .guide()
                .attachments()
                .is_none()
        );
    }

    #[test]
    fn attachment_removal_is_preview_bound_recoverable_and_preserves_metadata() {
        use sha2::Digest;

        let attachment_id = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509920").unwrap();
        let content = b"remove this attachment\n";
        let sha256 = sha2::Sha256::digest(content)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let attachment_path = format!("attachments/{GUIDE_ID}/remove.txt");
        let guide = GUIDE.replacen(
            "status: draft\n",
            &format!(
                "attachments:\n  - id: {attachment_id}\n    path: {attachment_path}\n    title: Remove me\n    sha256: {sha256}\n    size_bytes: {}\n    future_attachment: keep-until-removed\nstatus: tested\nlast_verified_at: \"2026-09-05T10:03:00Z\"\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
                content.len()
            ),
            1,
        );
        let (workspace, state, mut view) = fixture("attachment-remove", &guide);
        let attachment_directory = workspace.path().join(format!("attachments/{GUIDE_ID}"));
        fs::create_dir_all(&attachment_directory).unwrap();
        fs::set_permissions(
            workspace.path().join("attachments"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        fs::set_permissions(&attachment_directory, fs::Permissions::from_mode(0o700)).unwrap();
        let attachment_file = workspace.path().join(&attachment_path);
        let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
        let category_id = CategoryId::from_str(CATEGORY_ID).unwrap();
        let expected = revision(&view);
        let stamp = MutationStamp::from_trusted_time(at());
        let hostile_source = TestDirectory::new("attachment-removal-hostile-source");
        let outside = hostile_source.path().join("outside.txt");
        fs::write(&outside, content).unwrap();
        symlink(&outside, &attachment_file).unwrap();
        assert!(matches!(
            preview_attachment_removal(
                &view,
                state.path(),
                None,
                GUIDE_PATH,
                expected,
                &attachment_path,
                decode_sha256(&sha256).unwrap(),
                Some(content.len() as u64),
                |bytes| prepare_attachment_removal_candidate(
                    bytes,
                    guide_id,
                    category_id,
                    attachment_id,
                    &attachment_path,
                    &sha256,
                    Some(content.len() as u64),
                    &stamp,
                ),
            ),
            Err(MultiDocumentUpdateError::Unsafe)
        ));
        fs::remove_file(&attachment_file).unwrap();
        fs::write(&attachment_file, content).unwrap();
        fs::set_permissions(&attachment_file, fs::Permissions::from_mode(0o600)).unwrap();
        let hardlink = hostile_source.path().join("hardlink.txt");
        fs::hard_link(&attachment_file, &hardlink).unwrap();
        assert!(matches!(
            preview_attachment_removal(
                &view,
                state.path(),
                None,
                GUIDE_PATH,
                expected,
                &attachment_path,
                decode_sha256(&sha256).unwrap(),
                Some(content.len() as u64),
                |bytes| prepare_attachment_removal_candidate(
                    bytes,
                    guide_id,
                    category_id,
                    attachment_id,
                    &attachment_path,
                    &sha256,
                    Some(content.len() as u64),
                    &stamp,
                ),
            ),
            Err(MultiDocumentUpdateError::Unsafe)
        ));
        fs::remove_file(&hardlink).unwrap();
        refresh_workspace(&mut view).unwrap();

        let expected = revision(&view);
        let preview = match preview_attachment_removal(
            &view,
            state.path(),
            None,
            GUIDE_PATH,
            expected,
            &attachment_path,
            decode_sha256(&sha256).unwrap(),
            Some(content.len() as u64),
            |bytes| {
                prepare_attachment_removal_candidate(
                    bytes,
                    guide_id,
                    category_id,
                    attachment_id,
                    &attachment_path,
                    &sha256,
                    Some(content.len() as u64),
                    &stamp,
                )
            },
        ) {
            Ok(preview) => preview,
            Err(_) => panic!("expected removal preview"),
        };
        fs::write(&attachment_file, b"changed after preview!!!!\n").unwrap();
        assert_eq!(
            execute_approved_attachment_removal(
                &mut view,
                state.path(),
                None,
                guide_id,
                category_id,
                attachment_id,
                expected,
                &attachment_path,
                &sha256,
                Some(content.len() as u64),
                at(),
                &preview,
                &|| false,
                operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e43"),
            ),
            Err(AttachmentRemovalError::AttachmentChanged)
        );
        assert!(attachment_file.is_file());
        assert!(
            view.categories()[0].guides()[0]
                .guide()
                .attachments()
                .is_some()
        );

        fs::write(&attachment_file, content).unwrap();
        let preview = match preview_attachment_removal(
            &view,
            state.path(),
            None,
            GUIDE_PATH,
            expected,
            &attachment_path,
            decode_sha256(&sha256).unwrap(),
            Some(content.len() as u64),
            |bytes| {
                prepare_attachment_removal_candidate(
                    bytes,
                    guide_id,
                    category_id,
                    attachment_id,
                    &attachment_path,
                    &sha256,
                    Some(content.len() as u64),
                    &stamp,
                )
            },
        ) {
            Ok(preview) => preview,
            Err(_) => panic!("expected refreshed removal preview"),
        };
        let collision_operation = operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e45");
        let collision = workspace.path().join(format!(
            ".rehow/trash/{collision_operation}/{attachment_path}"
        ));
        fs::create_dir_all(collision.parent().unwrap()).unwrap();
        for directory in [
            workspace.path().join(".rehow"),
            workspace.path().join(".rehow/trash"),
            workspace
                .path()
                .join(format!(".rehow/trash/{collision_operation}")),
            workspace
                .path()
                .join(format!(".rehow/trash/{collision_operation}/attachments")),
            workspace.path().join(format!(
                ".rehow/trash/{collision_operation}/attachments/{GUIDE_ID}"
            )),
        ] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(&collision, b"do not replace").unwrap();
        fs::set_permissions(&collision, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            execute_approved_attachment_removal(
                &mut view,
                state.path(),
                None,
                guide_id,
                category_id,
                attachment_id,
                expected,
                &attachment_path,
                &sha256,
                Some(content.len() as u64),
                at(),
                &preview,
                &|| false,
                collision_operation,
            ),
            Err(AttachmentRemovalError::AttachmentChanged)
        );
        assert_eq!(fs::read(&collision).unwrap(), b"do not replace");
        assert!(attachment_file.is_file());
        assert!(
            view.categories()[0].guides()[0]
                .guide()
                .attachments()
                .is_some()
        );
        fs::remove_file(collision).unwrap();
        let operation_id = operation("0199a1d7-7b0d-7d8e-a38d-f936c5735e44");
        let removed = execute_approved_attachment_removal(
            &mut view,
            state.path(),
            None,
            guide_id,
            category_id,
            attachment_id,
            expected,
            &attachment_path,
            &sha256,
            Some(content.len() as u64),
            at(),
            &preview,
            &|| false,
            operation_id,
        )
        .unwrap();
        assert!(matches!(removed, AttachmentRemovalOutcome::Removed { .. }));
        assert!(!attachment_file.exists());
        assert_eq!(
            fs::read(
                workspace
                    .path()
                    .join(format!(".rehow/trash/{operation_id}/{attachment_path}"))
            )
            .unwrap(),
            content
        );
        let loaded = &view.categories()[0].guides()[0];
        assert!(loaded.guide().attachments().is_none());
        assert_eq!(loaded.guide().declared_status(), GuideStatus::Stale);
        let updated = fs::read_to_string(workspace.path().join(GUIDE_PATH)).unwrap();
        assert!(updated.contains("future_key: keep-me"));
        assert!(!updated.contains("future_attachment: keep-until-removed"));
        assert!(attachment_directory.is_dir());
    }

    #[test]
    fn attachment_removal_reports_recovery_when_path_is_swapped_during_displacement() {
        use sha2::Digest;

        for (label, replacement) in [
            (
                "different-inode-different-bytes",
                b"hostile replacement\n".as_slice(),
            ),
            (
                "different-inode-same-bytes",
                b"approved attachment\n".as_slice(),
            ),
        ] {
            let content = b"approved attachment\n";
            let attachment_id =
                AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509921").unwrap();
            let sha256 = sha2::Sha256::digest(content)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let attachment_path = format!("attachments/{GUIDE_ID}/race.txt");
            let guide = GUIDE.replacen(
                "status: draft\n",
                &format!(
                    "attachments:\n  - id: {attachment_id}\n    path: {attachment_path}\n    title: Race\n    sha256: {sha256}\n    size_bytes: {}\nstatus: draft\n",
                    content.len()
                ),
                1,
            );
            let (workspace, state, mut view) = fixture(label, &guide);
            let attachment_directory = workspace.path().join(format!("attachments/{GUIDE_ID}"));
            fs::create_dir_all(&attachment_directory).unwrap();
            fs::set_permissions(
                workspace.path().join("attachments"),
                fs::Permissions::from_mode(0o700),
            )
            .unwrap();
            fs::set_permissions(&attachment_directory, fs::Permissions::from_mode(0o700)).unwrap();
            let attachment_file = workspace.path().join(&attachment_path);
            fs::write(&attachment_file, content).unwrap();
            fs::set_permissions(&attachment_file, fs::Permissions::from_mode(0o600)).unwrap();
            refresh_workspace(&mut view).unwrap();
            let guide_id = GuideId::from_str(GUIDE_ID).unwrap();
            let approval = validate_attachment_removal_approval(
                &view,
                state.path(),
                None,
                guide_id,
                attachment_id,
                revision(&view),
            )
            .unwrap();
            let replacement_path = attachment_directory.join("replacement.tmp");
            let displaced_path = attachment_directory.join("externally-displaced.tmp");
            fs::write(&replacement_path, replacement).unwrap();
            fs::set_permissions(&replacement_path, fs::Permissions::from_mode(0o600)).unwrap();
            let hook_source = attachment_file.clone();
            let hook_replacement = replacement_path.clone();
            let hook_displaced = displaced_path.clone();
            crate::storage::set_attachment_before_move_hook(move || {
                fs::rename(&hook_source, &hook_displaced).unwrap();
                fs::rename(&hook_replacement, &hook_source).unwrap();
            });
            let result = approve_attachment_removal_after_native_confirmation(
                &mut view,
                state.path(),
                None,
                approval,
                || false,
            );
            let Err(AttachmentRemovalError::RecoveryRequired { operation_id }) = result else {
                panic!("expected recovery-required outcome, got {result:?}");
            };
            assert_eq!(fs::read(&displaced_path).unwrap(), content);
            let held_name = format!(
                ".rehow-held-{}",
                &blake3::hash(attachment_path.as_bytes()).to_hex()[..16]
            );
            let held = workspace.path().join(format!(
                ".rehow/trash/{operation_id}/attachments/{GUIDE_ID}/{held_name}"
            ));
            assert_eq!(fs::read(held).unwrap(), replacement);
            let guide_bytes = fs::read_to_string(workspace.path().join(GUIDE_PATH)).unwrap();
            assert!(!guide_bytes.contains(&attachment_id.to_string()));
        }
    }

    #[test]
    fn sanitizes_hostile_attachment_display_names() {
        assert_eq!(
            sanitized_attachment_filename(Path::new("/tmp/report\u{202e}.txt")),
            "report-.txt"
        );
        assert_eq!(
            sanitized_attachment_filename(Path::new("/tmp/..")),
            "attachment"
        );
    }

    #[test]
    fn attachment_source_refuses_links_and_oversize_files() {
        let source = TestDirectory::new("unsafe-attachment-source");
        let regular = source.path().join("regular.bin");
        fs::write(&regular, b"content").unwrap();
        fs::set_permissions(&regular, fs::Permissions::from_mode(0o600)).unwrap();
        let hardlink = source.path().join("hardlink.bin");
        fs::hard_link(&regular, &hardlink).unwrap();
        assert!(matches!(
            open_attachment_source(&regular),
            Err(AttachmentImportError::InvalidSource)
        ));

        fs::remove_file(&hardlink).unwrap();
        let symlink_path = source.path().join("symlink.bin");
        symlink(&regular, &symlink_path).unwrap();
        assert!(matches!(
            open_attachment_source(&symlink_path),
            Err(AttachmentImportError::InvalidSource)
        ));

        let oversize = source.path().join("oversize.bin");
        let file = fs::File::create(&oversize).unwrap();
        file.set_len(32 * 1024 * 1024 + 1).unwrap();
        fs::set_permissions(&oversize, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(
            open_attachment_source(&oversize),
            Err(AttachmentImportError::SourceTooLarge)
        ));
    }
}

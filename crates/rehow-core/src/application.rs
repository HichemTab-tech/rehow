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
            CategoryMutation, GuideMutation, MutationFailureKind, MutationField, MutationOutcome,
            MutationStamp, plan_category_mutation, plan_guide_mutation,
        },
        verification::{VerificationFailure, verification_hash},
    },
    markdown::{GuideReadinessError, validate_tested_readiness},
    parsing::{parse_guide_source, parse_yaml_document},
    protocol::{
        CategoryId, GuideId, OperationId, Revision, Timestamp, current_timestamp, new_category_id,
        new_guide_id, new_operation_id, new_workspace_id,
    },
    storage::{
        CategoryCreateStorageError, CategoryRemoveStorageError, DocumentCreateError,
        DocumentRemoveError, DocumentUpdateError, DocumentUpdateRequest, EmptyWorkspaceTarget,
        MultiDocumentUpdateError, WorkspaceInitializeError, WorkspaceInspectError, WorkspaceView,
        execute_category_create, execute_category_remove, execute_document_create,
        execute_document_move, execute_document_remove, execute_document_update,
        execute_multi_document_update, execute_workspace_initialize, open_initialized_workspace,
        refresh_workspace,
    },
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{collections::HashSet, path::Path};

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
    if !workspace.issues().is_empty() || ordered.len() != workspace.categories().len() {
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
    RevisionConflict,
    Busy,
    UnsafeStorage,
    UnsupportedFilesystem,
    ResourceLimit,
    Io,
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
        CategoryRemoveStorageError::Conflict => CategoryRemoveError::RevisionConflict,
        CategoryRemoveStorageError::NotEmpty => CategoryRemoveError::NotEmpty,
        CategoryRemoveStorageError::Unsafe => CategoryRemoveError::UnsafeStorage,
        CategoryRemoveStorageError::Unsupported => CategoryRemoveError::UnsupportedFilesystem,
        CategoryRemoveStorageError::TooLarge => CategoryRemoveError::ResourceLimit,
        CategoryRemoveStorageError::Io => CategoryRemoveError::Io,
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
            PlanError::InvalidGuide | PlanError::InvalidEdit | PlanError::NotReady => {
                Self::InvalidCategory
            }
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
    ReplaceBody(Vec<u8>),
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
    let outcome = execute_document_update(
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
    .map_err(map_update_error)?;
    if outcome.changed() && refresh_workspace(workspace).is_err() {
        return Err(GuideUpdateError::CommittedRefreshRequired {
            revision: outcome.revision(),
        });
    }
    Ok(GuideUpdateOutcome {
        revision: outcome.revision(),
        changed: outcome.changed(),
    })
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

fn map_move_error(error: MultiDocumentUpdateError<PlanError>) -> GuideUpdateError {
    match error {
        MultiDocumentUpdateError::Prepare(PlanError::InvalidGuide)
        | MultiDocumentUpdateError::InvalidPlan => GuideUpdateError::InvalidGuide,
        MultiDocumentUpdateError::Prepare(PlanError::InvalidEdit) => GuideUpdateError::InvalidEdit,
        MultiDocumentUpdateError::Prepare(PlanError::NotReady) => GuideUpdateError::NotReady,
        MultiDocumentUpdateError::Prepare(PlanError::UnsafeRoundTrip) => {
            GuideUpdateError::UnsafeRoundTrip
        }
        MultiDocumentUpdateError::Prepare(PlanError::ResourceLimit)
        | MultiDocumentUpdateError::TooLarge => GuideUpdateError::ResourceLimit,
        MultiDocumentUpdateError::Busy => GuideUpdateError::Busy,
        MultiDocumentUpdateError::Conflict => GuideUpdateError::RevisionConflict,
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
        os::unix::fs::PermissionsExt,
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
        assert_eq!(operations.len(), 1);
        assert_eq!(
            fs::read(
                operations[0]
                    .path()
                    .join(&relative_directory)
                    .join("category.yaml")
            )
            .unwrap(),
            descriptor
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
        assert_eq!(operations.len(), 1);
        assert_eq!(
            fs::read(operations[0].path().join(relative_path)).unwrap(),
            original
        );
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
}

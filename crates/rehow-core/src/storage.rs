//! Descriptor-confined Workspace inspection and private mutation primitives.
//!
//! The retained root descriptor prevents later reads or writes from being
//! redirected by renaming the path that the user selected. Mutation remains a
//! crate-private recovery boundary and is not exposed to UI or MCP adapters.

pub(crate) mod recovery;
pub(crate) use recovery::{
    CategoryCreateStorageError, CategoryCreateStorageOutcome, CategoryRemoveStorageError,
    CategoryRemoveStorageOutcome, DocumentCreateError, DocumentCreateOutcome, DocumentMoveOutcome,
    DocumentRemoveError, DocumentRemoveOutcome, DocumentUpdateError, DocumentUpdateOutcome,
    MultiDocumentUpdateError, MultiDocumentUpdateOutcome, PreparedDocument,
    WorkspaceInitializeError, WorkspaceInitializeOutcome,
};

#[cfg(not(target_os = "linux"))]
compile_error!("Rehow's confined Workspace storage currently requires Linux");

use crate::{
    domain::{Category, Guide, GuideStatus, WorkspaceManifest},
    protocol::Revision,
};
use std::{collections::HashSet, fmt, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceIssueKind {
    UnsafeEntry,
    UnsupportedEntry,
    MissingCategoryDescriptor,
    MalformedCategory,
    MalformedGuide,
    FileTooLarge,
    CategoryMismatch,
    DuplicateCategoryId,
    DuplicateGuideId,
    VerificationMismatch,
}

#[derive(Debug, Eq, PartialEq)]
pub struct WorkspaceIssue {
    relative_path: String,
    kind: WorkspaceIssueKind,
}

impl WorkspaceIssue {
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn kind(&self) -> WorkspaceIssueKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MalformedWorkspaceReason {
    Manifest,
    CategoriesMissing,
    CategoriesUnsafe,
}

pub enum WorkspaceInspection {
    Empty(EmptyWorkspaceTarget),
    Compatible(Box<WorkspaceView>),
    NewerUnsupported { version: u32 },
    OrdinaryMarkdown,
    UnrelatedNonempty,
    Malformed { reason: MalformedWorkspaceReason },
}

pub struct EmptyWorkspaceTarget {
    root: linux::DirectoryCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceFolderFacts {
    writable: bool,
    existing_git_repository: bool,
    nested_git_repository: bool,
    git_probe_complete: bool,
    unsafe_git_marker: bool,
}

impl WorkspaceFolderFacts {
    pub fn writable(self) -> bool {
        self.writable
    }

    pub fn existing_git_repository(self) -> bool {
        self.existing_git_repository
    }

    pub fn nested_git_repository(self) -> bool {
        self.nested_git_repository
    }

    pub fn git_probe_complete(self) -> bool {
        self.git_probe_complete
    }

    pub fn unsafe_git_marker(self) -> bool {
        self.unsafe_git_marker
    }
}

pub struct WorkspaceInspectionReport {
    facts: WorkspaceFolderFacts,
    classification: WorkspaceInspection,
}

impl WorkspaceInspectionReport {
    pub fn facts(&self) -> WorkspaceFolderFacts {
        self.facts
    }

    pub fn classification(&self) -> &WorkspaceInspection {
        &self.classification
    }

    pub fn into_parts(self) -> (WorkspaceFolderFacts, WorkspaceInspection) {
        (self.facts, self.classification)
    }
}

pub struct WorkspaceView {
    // Keep the root capability alive for the lifetime of every loaded record.
    _root: linux::DirectoryCapability,
    manifest: WorkspaceManifest,
    manifest_revision: Revision,
    categories: Vec<LoadedCategory>,
    issues: Vec<WorkspaceIssue>,
}

pub struct LoadedCategory {
    relative_directory: String,
    descriptor_revision: Revision,
    category: Category,
    guides: Vec<LoadedGuide>,
}

pub struct LoadedGuide {
    relative_path: String,
    revision: Revision,
    guide: Guide,
    effective_status: GuideStatus,
}

impl WorkspaceView {
    pub fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }

    pub fn manifest_revision(&self) -> Revision {
        self.manifest_revision
    }

    pub fn categories(&self) -> &[LoadedCategory] {
        &self.categories
    }

    pub fn issues(&self) -> &[WorkspaceIssue] {
        &self.issues
    }
}

impl LoadedCategory {
    pub fn relative_directory(&self) -> &str {
        &self.relative_directory
    }

    pub fn descriptor_revision(&self) -> Revision {
        self.descriptor_revision
    }

    pub fn category(&self) -> &Category {
        &self.category
    }

    pub fn guides(&self) -> &[LoadedGuide] {
        &self.guides
    }
}

impl LoadedGuide {
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn guide(&self) -> &Guide {
        &self.guide
    }

    pub fn effective_status(&self) -> GuideStatus {
        self.effective_status
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceInspectError {
    NotFound,
    NotDirectory,
    PermissionDenied,
    UnsafeRoot,
    ResourceLimit,
    ChangedDuringRead,
    Io,
}

impl fmt::Display for WorkspaceInspectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "Workspace folder was not found",
            Self::NotDirectory => "Workspace selection is not a folder",
            Self::PermissionDenied => "Workspace folder cannot be read",
            Self::UnsafeRoot => "Workspace folder is unsafe to inspect",
            Self::ResourceLimit => "Workspace exceeds a safe inspection limit",
            Self::ChangedDuringRead => "Workspace changed during inspection; try again",
            Self::Io => "Workspace inspection failed",
        })
    }
}

impl std::error::Error for WorkspaceInspectError {}

const DESCRIPTOR_LIMIT: usize = 256 * 1024;
const GUIDE_LIMIT: usize = 2 * 1024 * 1024;
const CATEGORY_LIMIT: usize = 1_000;
const GUIDE_COUNT_LIMIT: usize = 10_000;
const MANAGED_BYTES_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const GIT_ANCESTOR_LIMIT: usize = 64;

pub(crate) struct DocumentUpdateRequest {
    relative_path: String,
    expected_revision: Revision,
}

impl DocumentUpdateRequest {
    pub(crate) fn new(relative_path: String, expected_revision: Revision) -> Self {
        Self {
            relative_path,
            expected_revision,
        }
    }

    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub(crate) fn expected_revision(&self) -> Revision {
        self.expected_revision
    }
}

pub fn inspect_workspace(path: &Path) -> Result<WorkspaceInspectionReport, WorkspaceInspectError> {
    let root = linux::DirectoryCapability::open_root(path)?;
    let facts = inspect_folder_facts(&root)?;
    let classification = inspect_open_root(root)?;
    Ok(WorkspaceInspectionReport {
        facts,
        classification,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_update<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_path: &str,
    expected_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    prepare: F,
) -> Result<DocumentUpdateOutcome, DocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    recovery::execute_document_update(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_path,
        expected_revision,
        operation_id,
        created_at,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_move<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    source_relative_path: &str,
    destination_relative_path: &str,
    expected_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    prepare: F,
) -> Result<DocumentMoveOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[u8]) -> Result<Box<[u8]>, E>,
{
    recovery::execute_document_move(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        source_relative_path,
        destination_relative_path,
        expected_revision,
        operation_id,
        created_at,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_remove(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_path: &str,
    expected_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
) -> Result<DocumentRemoveOutcome, DocumentRemoveError> {
    recovery::execute_document_remove(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_path,
        expected_revision,
        operation_id,
        created_at,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_multi_document_update<E, F>(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    inventory_directory: &str,
    expected_inventory_entries: &[String],
    requests: &[DocumentUpdateRequest],
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    prepare: F,
) -> Result<MultiDocumentUpdateOutcome, MultiDocumentUpdateError<E>>
where
    F: FnOnce(&[PreparedDocument<'_>]) -> Result<Vec<Box<[u8]>>, E>,
{
    recovery::execute_multi_document_update(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        inventory_directory,
        expected_inventory_entries,
        requests,
        operation_id,
        created_at,
        prepare,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_category_remove(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_directory: &str,
    expected_revision: Revision,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
) -> Result<CategoryRemoveStorageOutcome, CategoryRemoveStorageError> {
    recovery::execute_category_remove(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_directory,
        expected_revision,
        operation_id,
        created_at,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_workspace_initialize(
    target: &EmptyWorkspaceTarget,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    workspace_id: crate::protocol::WorkspaceId,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    manifest: &[u8],
) -> Result<WorkspaceInitializeOutcome, WorkspaceInitializeError> {
    recovery::execute_workspace_initialize(
        &target.root,
        state_home,
        runtime_directory,
        workspace_id,
        operation_id,
        created_at,
        manifest,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_category_create(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_directory: &str,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    descriptor: &[u8],
) -> Result<CategoryCreateStorageOutcome, CategoryCreateStorageError> {
    recovery::execute_category_create(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_directory,
        operation_id,
        created_at,
        descriptor,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_document_create(
    workspace: &WorkspaceView,
    state_home: &Path,
    runtime_directory: Option<&Path>,
    relative_path: &str,
    operation_id: crate::protocol::OperationId,
    created_at: crate::protocol::Timestamp,
    content: &[u8],
) -> Result<DocumentCreateOutcome, DocumentCreateError> {
    recovery::execute_document_create(
        &workspace._root,
        state_home,
        runtime_directory,
        *workspace.manifest.workspace_id(),
        relative_path,
        operation_id,
        created_at,
        content,
    )
}

pub(crate) fn open_initialized_workspace(
    target: &EmptyWorkspaceTarget,
) -> Result<Box<WorkspaceView>, WorkspaceInspectError> {
    let root = target.root.duplicate().map_err(map_io)?;
    let WorkspaceInspection::Compatible(workspace) = inspect_open_root(root)? else {
        return Err(WorkspaceInspectError::ChangedDuringRead);
    };
    Ok(workspace)
}

pub(crate) fn refresh_workspace(
    workspace: &mut WorkspaceView,
) -> Result<(), WorkspaceInspectError> {
    let root = workspace._root.duplicate().map_err(map_io)?;
    let WorkspaceInspection::Compatible(refreshed) = inspect_open_root(root)? else {
        return Err(WorkspaceInspectError::ChangedDuringRead);
    };
    *workspace = *refreshed;
    Ok(())
}

fn inspect_open_root(
    root: linux::DirectoryCapability,
) -> Result<WorkspaceInspection, WorkspaceInspectError> {
    let root_entries = root.entries(20_000)?;
    if root_entries.is_empty() {
        return Ok(WorkspaceInspection::Empty(EmptyWorkspaceTarget { root }));
    }
    if !root_entries.iter().any(|name| name == "rehow.yaml") {
        let mut remaining_entries = 20_000usize.saturating_sub(root_entries.len());
        return Ok(
            if contains_safe_markdown(&root, &root_entries, 4, &mut remaining_entries)? {
                WorkspaceInspection::OrdinaryMarkdown
            } else {
                WorkspaceInspection::UnrelatedNonempty
            },
        );
    }

    let manifest_bytes = match root.read_regular("rehow.yaml", DESCRIPTOR_LIMIT) {
        Ok(bytes) => bytes,
        Err(linux::EntryReadError::Unsafe) => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest,
            });
        }
        Err(linux::EntryReadError::TooLarge) => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest,
            });
        }
        Err(linux::EntryReadError::Changed) => {
            return Err(WorkspaceInspectError::ChangedDuringRead);
        }
        Err(linux::EntryReadError::Io(error)) => return Err(map_io(error)),
    };
    let manifest_revision = document_revision("rehow.yaml", &manifest_bytes);
    let manifest_bytes_len = manifest_bytes.len();
    let manifest = match crate::parsing::parse_yaml_document(manifest_bytes.into_boxed_slice())
        .ok()
        .and_then(|parsed| crate::domain::content::bind_workspace_manifest(parsed).ok())
    {
        Some(crate::domain::content::ManifestBindingOutcome::Current(bound)) => bound.into_record(),
        Some(crate::domain::content::ManifestBindingOutcome::NewerUnsupported {
            version, ..
        }) => return Ok(WorkspaceInspection::NewerUnsupported { version }),
        None => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest,
            });
        }
    };

    let categories_dir = match root.open_directory("categories") {
        Ok(dir) => dir,
        Err(linux::OpenDirectoryError::Missing) => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::CategoriesMissing,
            });
        }
        Err(linux::OpenDirectoryError::Unsafe) => {
            return Ok(WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::CategoriesUnsafe,
            });
        }
        Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
    };

    let mut category_names = categories_dir.entries(CATEGORY_LIMIT)?;
    category_names.sort_unstable();

    let mut managed_bytes =
        u64::try_from(manifest_bytes_len).map_err(|_| WorkspaceInspectError::ResourceLimit)?;
    let mut categories = Vec::new();
    let mut issues = Vec::new();
    let mut category_ids = HashSet::new();
    let mut guide_ids = HashSet::new();
    let mut guide_candidates = 0usize;
    categories
        .try_reserve_exact(category_names.len())
        .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
    issues
        .try_reserve(category_names.len())
        .map_err(|_| WorkspaceInspectError::ResourceLimit)?;

    for category_name in category_names {
        if !valid_category_directory(&category_name) {
            push_issue(
                &mut issues,
                "categories/<unsupported>".to_owned(),
                WorkspaceIssueKind::UnsupportedEntry,
            )?;
            continue;
        }
        let category_dir = match categories_dir.open_directory(&category_name) {
            Ok(dir) => dir,
            Err(linux::OpenDirectoryError::Missing) => continue,
            Err(linux::OpenDirectoryError::Unsafe) => {
                push_issue(
                    &mut issues,
                    format!("categories/{category_name}"),
                    WorkspaceIssueKind::UnsafeEntry,
                )?;
                continue;
            }
            Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
        };
        let descriptor_path = format!("categories/{category_name}/category.yaml");
        let descriptor = match category_dir.read_regular("category.yaml", DESCRIPTOR_LIMIT) {
            Ok(bytes) => bytes,
            Err(linux::EntryReadError::Unsafe | linux::EntryReadError::TooLarge) => {
                push_issue(
                    &mut issues,
                    descriptor_path,
                    WorkspaceIssueKind::MalformedCategory,
                )?;
                continue;
            }
            Err(linux::EntryReadError::Changed) => {
                return Err(WorkspaceInspectError::ChangedDuringRead);
            }
            Err(linux::EntryReadError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                push_issue(
                    &mut issues,
                    descriptor_path,
                    WorkspaceIssueKind::MissingCategoryDescriptor,
                )?;
                continue;
            }
            Err(linux::EntryReadError::Io(error)) => return Err(map_io(error)),
        };
        add_managed_bytes(&mut managed_bytes, descriptor.len())?;
        let descriptor_revision = document_revision(&descriptor_path, &descriptor);
        let bound = match crate::parsing::parse_yaml_document(descriptor.into_boxed_slice())
            .ok()
            .and_then(|parsed| crate::domain::content::bind_category(parsed).ok())
        {
            Some(bound) => bound,
            None => {
                push_issue(
                    &mut issues,
                    descriptor_path,
                    WorkspaceIssueKind::MalformedCategory,
                )?;
                continue;
            }
        };
        category_ids
            .try_reserve(1)
            .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
        if !category_ids.insert(*bound.record().id()) {
            push_issue(
                &mut issues,
                format!("categories/{category_name}/category.yaml"),
                WorkspaceIssueKind::DuplicateCategoryId,
            )?;
            continue;
        }
        let category = bound.into_record();

        let mut guide_names = category_dir.entries(GUIDE_COUNT_LIMIT.saturating_add(1))?;
        guide_names.sort_unstable();
        let mut guides = Vec::new();
        guides
            .try_reserve(guide_names.len().min(128))
            .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
        for guide_name in guide_names {
            if guide_name == "category.yaml" {
                continue;
            }
            if !valid_guide_filename(&guide_name) {
                push_issue(
                    &mut issues,
                    format!("categories/{category_name}/<unsupported>"),
                    WorkspaceIssueKind::UnsupportedEntry,
                )?;
                continue;
            }
            if guide_candidates >= GUIDE_COUNT_LIMIT {
                return Err(WorkspaceInspectError::ResourceLimit);
            }
            guide_candidates += 1;
            let guide_path = format!("categories/{category_name}/{guide_name}");
            let bytes = match category_dir.read_regular(&guide_name, GUIDE_LIMIT) {
                Ok(bytes) => bytes,
                Err(linux::EntryReadError::Unsafe) => {
                    push_issue(&mut issues, guide_path, WorkspaceIssueKind::UnsafeEntry)?;
                    continue;
                }
                Err(linux::EntryReadError::TooLarge) => {
                    push_issue(&mut issues, guide_path, WorkspaceIssueKind::FileTooLarge)?;
                    continue;
                }
                Err(linux::EntryReadError::Changed) => {
                    return Err(WorkspaceInspectError::ChangedDuringRead);
                }
                Err(linux::EntryReadError::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound =>
                {
                    continue;
                }
                Err(linux::EntryReadError::Io(error)) => return Err(map_io(error)),
            };
            add_managed_bytes(&mut managed_bytes, bytes.len())?;
            let revision = document_revision(&guide_path, &bytes);
            let bound_guide = match crate::parsing::parse_guide_source(bytes.into_boxed_slice())
                .ok()
                .and_then(|parsed| crate::domain::content::guide::bind_guide(parsed).ok())
            {
                Some(bound) => bound,
                None => {
                    push_issue(&mut issues, guide_path, WorkspaceIssueKind::MalformedGuide)?;
                    continue;
                }
            };
            if bound_guide.record().category_id() != category.id() {
                push_issue(
                    &mut issues,
                    guide_path,
                    WorkspaceIssueKind::CategoryMismatch,
                )?;
                continue;
            }
            guide_ids
                .try_reserve(1)
                .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
            if !guide_ids.insert(*bound_guide.record().id()) {
                push_issue(
                    &mut issues,
                    guide_path.clone(),
                    WorkspaceIssueKind::DuplicateGuideId,
                )?;
                continue;
            }
            let effective_status = if bound_guide.record().declared_status() == GuideStatus::Tested
            {
                let calculated = crate::domain::content::verification::verification_hash(
                    &bound_guide,
                )
                .map_err(|error| match error {
                    crate::domain::content::verification::VerificationFailure::ResourceLimit => {
                        WorkspaceInspectError::ResourceLimit
                    }
                    crate::domain::content::verification::VerificationFailure::Invariant => {
                        WorkspaceInspectError::Io
                    }
                })?;
                if bound_guide
                    .record()
                    .verification()
                    .and_then(|verification| verification.verified_content_hash())
                    == Some(&calculated)
                {
                    GuideStatus::Tested
                } else {
                    push_issue(
                        &mut issues,
                        guide_path.clone(),
                        WorkspaceIssueKind::VerificationMismatch,
                    )?;
                    GuideStatus::Stale
                }
            } else {
                bound_guide.record().declared_status()
            };
            guides.push(LoadedGuide {
                relative_path: guide_path,
                revision,
                guide: bound_guide.into_record(),
                effective_status,
            });
        }
        guides.sort_by(|left, right| {
            left.guide()
                .title()
                .cmp(right.guide().title())
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        categories.push(LoadedCategory {
            relative_directory: format!("categories/{category_name}"),
            descriptor_revision,
            category,
            guides,
        });
    }

    categories.sort_by(|left, right| {
        left.category()
            .order()
            .cmp(&right.category().order())
            .then_with(|| left.category().name().cmp(right.category().name()))
            .then_with(|| left.relative_directory.cmp(&right.relative_directory))
    });

    Ok(WorkspaceInspection::Compatible(Box::new(WorkspaceView {
        _root: root,
        manifest,
        manifest_revision,
        categories,
        issues,
    })))
}

fn inspect_folder_facts(
    root: &linux::DirectoryCapability,
) -> Result<WorkspaceFolderFacts, WorkspaceInspectError> {
    let writable = root.writable()?;
    let mut facts = WorkspaceFolderFacts {
        writable,
        existing_git_repository: false,
        nested_git_repository: false,
        git_probe_complete: true,
        unsafe_git_marker: false,
    };
    apply_git_marker(root.git_marker(), true, &mut facts);

    let mut ancestor = match root.open_parent() {
        Ok(value) => value,
        Err(_) => {
            facts.git_probe_complete = false;
            return Ok(facts);
        }
    };
    for _ in 0..GIT_ANCESTOR_LIMIT {
        let Some(directory) = ancestor else {
            return Ok(facts);
        };
        apply_git_marker(directory.git_marker(), false, &mut facts);
        ancestor = match directory.open_parent() {
            Ok(value) => value,
            Err(_) => {
                facts.git_probe_complete = false;
                return Ok(facts);
            }
        };
    }
    if ancestor.is_some() {
        facts.git_probe_complete = false;
    }
    Ok(facts)
}

fn apply_git_marker(
    marker: Result<linux::GitMarker, std::io::Error>,
    selected_root: bool,
    facts: &mut WorkspaceFolderFacts,
) {
    match marker {
        Ok(linux::GitMarker::Missing) => {}
        Ok(linux::GitMarker::Repository) if selected_root => {
            facts.existing_git_repository = true;
        }
        Ok(linux::GitMarker::Repository) => {
            facts.nested_git_repository = true;
        }
        Ok(linux::GitMarker::Unsafe) => {
            facts.unsafe_git_marker = true;
        }
        Err(_) => {
            facts.git_probe_complete = false;
        }
    }
}

fn contains_safe_markdown(
    directory: &linux::DirectoryCapability,
    names: &[String],
    remaining_depth: usize,
    remaining_entries: &mut usize,
) -> Result<bool, WorkspaceInspectError> {
    for name in names {
        if name.ends_with(".md") {
            match directory.read_regular(name, GUIDE_LIMIT) {
                Ok(_) | Err(linux::EntryReadError::TooLarge) => return Ok(true),
                Err(linux::EntryReadError::Unsafe) => {}
                Err(linux::EntryReadError::Changed) => {
                    return Err(WorkspaceInspectError::ChangedDuringRead);
                }
                Err(linux::EntryReadError::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(linux::EntryReadError::Io(error)) => return Err(map_io(error)),
            }
        }
        if remaining_depth == 0 {
            continue;
        }
        let child = match directory.open_directory(name) {
            Ok(child) => child,
            Err(linux::OpenDirectoryError::Missing | linux::OpenDirectoryError::Unsafe) => {
                continue;
            }
            Err(linux::OpenDirectoryError::Io(error)) => return Err(map_io(error)),
        };
        if *remaining_entries == 0 {
            return Err(WorkspaceInspectError::ResourceLimit);
        }
        let children = child.entries(*remaining_entries)?;
        *remaining_entries = (*remaining_entries).saturating_sub(children.len());
        if contains_safe_markdown(&child, &children, remaining_depth - 1, remaining_entries)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn valid_category_directory(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_guide_filename(value: &str) -> bool {
    value
        .strip_suffix(".md")
        .is_some_and(valid_category_directory)
}

fn add_managed_bytes(total: &mut u64, amount: usize) -> Result<(), WorkspaceInspectError> {
    *total = total
        .checked_add(u64::try_from(amount).map_err(|_| WorkspaceInspectError::ResourceLimit)?)
        .ok_or(WorkspaceInspectError::ResourceLimit)?;
    if *total > MANAGED_BYTES_LIMIT {
        return Err(WorkspaceInspectError::ResourceLimit);
    }
    Ok(())
}

fn push_issue(
    issues: &mut Vec<WorkspaceIssue>,
    relative_path: String,
    kind: WorkspaceIssueKind,
) -> Result<(), WorkspaceInspectError> {
    issues
        .try_reserve(1)
        .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
    issues.push(WorkspaceIssue {
        relative_path,
        kind,
    });
    Ok(())
}

fn document_revision(relative_path: &str, bytes: &[u8]) -> Revision {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rehow-document-revision-v1\0");
    hash_frame(&mut hasher, relative_path.as_bytes());
    hash_frame(&mut hasher, bytes);
    Revision::from_digest_bytes(*hasher.finalize().as_bytes())
}

fn hash_frame(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn map_io(error: std::io::Error) -> WorkspaceInspectError {
    match error.kind() {
        std::io::ErrorKind::NotFound => WorkspaceInspectError::NotFound,
        std::io::ErrorKind::NotADirectory => WorkspaceInspectError::NotDirectory,
        std::io::ErrorKind::PermissionDenied => WorkspaceInspectError::PermissionDenied,
        _ => WorkspaceInspectError::Io,
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::WorkspaceInspectError;
    use std::{
        ffi::{CStr, CString, OsStr},
        fs::File,
        io::{self, Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::Path,
    };

    pub(super) struct DirectoryCapability {
        descriptor: OwnedFd,
        root_hash: [u8; 32],
        capability_hash: [u8; 32],
    }

    pub(super) enum OpenDirectoryError {
        Missing,
        Unsafe,
        Io(io::Error),
    }

    pub(super) enum EntryReadError {
        Unsafe,
        TooLarge,
        Changed,
        Io(io::Error),
    }

    pub(super) enum GitMarker {
        Missing,
        Repository,
        Unsafe,
    }

    pub(super) struct ManagedTarget {
        directory: DirectoryCapability,
        name: String,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub(super) struct RegularState {
        pub(super) bytes: Vec<u8>,
        pub(super) mode: u32,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub(super) enum ManagedState {
        Missing,
        Regular(RegularState),
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) enum MutationIoError {
        Unsafe,
        Conflict,
        Unsupported,
        TooLarge,
        Io,
    }

    #[cfg(test)]
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub(super) enum ManagedFault {
        TempAfterWrite,
        TempAfterFileSync,
        ReusedTempAfterFileSync,
    }

    #[cfg(test)]
    thread_local! {
        static MANAGED_FAULT: std::cell::Cell<Option<ManagedFault>> = const { std::cell::Cell::new(None) };
    }

    #[cfg(test)]
    pub(super) fn set_managed_fault(fault: ManagedFault) {
        MANAGED_FAULT.set(Some(fault));
    }

    #[cfg(test)]
    fn managed_fail(point: ManagedFault) -> Result<(), MutationIoError> {
        if MANAGED_FAULT.get() == Some(point) {
            MANAGED_FAULT.set(None);
            Err(MutationIoError::Unsupported)
        } else {
            Ok(())
        }
    }

    impl DirectoryCapability {
        pub(super) fn open_root(path: &Path) -> Result<Self, WorkspaceInspectError> {
            let canonical = std::fs::canonicalize(path).map_err(super::map_io)?;
            let path = CString::new(canonical.as_os_str().as_bytes())
                .map_err(|_| WorkspaceInspectError::UnsafeRoot)?;
            // SAFETY: `path` is NUL-terminated and valid for the duration of the call.
            let raw = unsafe {
                libc::open(
                    path.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return Err(super::map_io(io::Error::last_os_error()));
            }
            // SAFETY: a successful `open` returns a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            let (device, inode) =
                descriptor_identity(&descriptor).map_err(|_| WorkspaceInspectError::UnsafeRoot)?;
            let mut root_hasher = blake3::Hasher::new();
            root_hasher.update(b"rehow-canonical-root-v1\0");
            root_hasher.update(&(canonical.as_os_str().as_bytes().len() as u64).to_be_bytes());
            root_hasher.update(canonical.as_os_str().as_bytes());
            let root_hash = *root_hasher.finalize().as_bytes();
            let mut epoch = [0_u8; 32];
            fill_random(&mut epoch).map_err(|_| WorkspaceInspectError::Io)?;
            let mut capability_hasher = blake3::Hasher::new();
            capability_hasher.update(b"rehow-open-capability-v1\0");
            capability_hasher.update(&root_hash);
            capability_hasher.update(&device.to_be_bytes());
            capability_hasher.update(&inode.to_be_bytes());
            capability_hasher.update(&epoch);
            let capability_hash = *capability_hasher.finalize().as_bytes();
            Ok(Self {
                descriptor,
                root_hash,
                capability_hash,
            })
        }

        pub(super) fn entries(&self, limit: usize) -> Result<Vec<String>, WorkspaceInspectError> {
            // Reopen `.` so each scan receives an independent directory-stream
            // offset while remaining confined to the retained capability.
            // SAFETY: the descriptor and static component are valid.
            let duplicate = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c".".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if duplicate < 0 {
                return Err(super::map_io(io::Error::last_os_error()));
            }
            // SAFETY: `duplicate` is a newly owned directory descriptor; `fdopendir`
            // consumes it on success.
            let raw_stream = unsafe { libc::fdopendir(duplicate) };
            if raw_stream.is_null() {
                // SAFETY: `fdopendir` did not consume the descriptor on failure.
                unsafe { libc::close(duplicate) };
                return Err(super::map_io(io::Error::last_os_error()));
            }
            let stream = DirectoryStream(raw_stream);
            let mut names = Vec::new();
            loop {
                // SAFETY: this thread exclusively owns `stream` until `closedir`.
                unsafe { *libc::__errno_location() = 0 };
                // SAFETY: `stream` is valid and open.
                let entry = unsafe { libc::readdir(stream.0) };
                if entry.is_null() {
                    // SAFETY: Linux exposes thread-local errno through this pointer.
                    let errno = unsafe { *libc::__errno_location() };
                    if errno != 0 {
                        return Err(super::map_io(io::Error::from_raw_os_error(errno)));
                    }
                    break;
                }
                // SAFETY: `d_name` is a NUL-terminated array owned by `stream` until
                // the next `readdir`; it is copied before then.
                let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if bytes == b"." || bytes == b".." {
                    continue;
                }
                if names.len() >= limit {
                    return Err(WorkspaceInspectError::ResourceLimit);
                }
                let name =
                    std::str::from_utf8(bytes).map_err(|_| WorkspaceInspectError::UnsafeRoot)?;
                names
                    .try_reserve(1)
                    .map_err(|_| WorkspaceInspectError::ResourceLimit)?;
                names.push(name.to_owned());
            }
            Ok(names)
        }

        pub(super) fn writable(&self) -> Result<bool, WorkspaceInspectError> {
            // SAFETY: the retained descriptor is valid and `.` is a static,
            // NUL-terminated relative component.
            let result = unsafe {
                libc::faccessat(
                    self.descriptor.as_raw_fd(),
                    c".".as_ptr(),
                    libc::W_OK | libc::X_OK,
                    libc::AT_EACCESS,
                )
            };
            if result == 0 {
                return Ok(true);
            }
            let error = io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::EACCES) | Some(libc::EROFS) => Ok(false),
                _ => Err(super::map_io(error)),
            }
        }

        pub(super) fn git_marker(&self) -> Result<GitMarker, io::Error> {
            // `O_PATH | O_NOFOLLOW` obtains metadata for the directory entry
            // itself, including a symlink, without opening its target.
            // SAFETY: the descriptor is valid and `.git` is static and NUL-terminated.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c".git".as_ptr(),
                    libc::O_PATH | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(libc::ENOENT) => Ok(GitMarker::Missing),
                    _ => Err(error),
                };
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            let file = unsafe { File::from_raw_fd(raw) };
            let metadata = file.metadata()?;
            if metadata.file_type().is_dir() {
                return Ok(GitMarker::Repository);
            }
            if !metadata.file_type().is_file() || metadata.nlink() != 1 {
                return Ok(GitMarker::Unsafe);
            }
            drop(file);
            match self.read_regular(".git", 4 * 1024) {
                Ok(bytes) if valid_gitdir_file(&bytes) => Ok(GitMarker::Repository),
                Ok(_)
                | Err(
                    EntryReadError::Unsafe | EntryReadError::TooLarge | EntryReadError::Changed,
                ) => Ok(GitMarker::Unsafe),
                Err(EntryReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                    Ok(GitMarker::Missing)
                }
                Err(EntryReadError::Io(error)) => Err(error),
            }
        }

        pub(super) fn open_parent(&self) -> Result<Option<Self>, io::Error> {
            // SAFETY: the descriptor is valid and `..` is static and NUL-terminated.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c"..".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            let parent = unsafe { OwnedFd::from_raw_fd(raw) };
            let current_identity = descriptor_identity(&self.descriptor)?;
            let parent_identity = descriptor_identity(&parent)?;
            if current_identity == parent_identity {
                Ok(None)
            } else {
                Ok(Some(Self {
                    descriptor: parent,
                    root_hash: self.root_hash,
                    capability_hash: self.capability_hash,
                }))
            }
        }

        pub(super) fn open_directory(&self, name: &str) -> Result<Self, OpenDirectoryError> {
            let name = component(name).map_err(|_| OpenDirectoryError::Unsafe)?;
            // SAFETY: the directory descriptor and NUL-terminated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(libc::ENOENT) => Err(OpenDirectoryError::Missing),
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(OpenDirectoryError::Unsafe),
                    _ => Err(OpenDirectoryError::Io(error)),
                };
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            Ok(Self {
                descriptor: unsafe { OwnedFd::from_raw_fd(raw) },
                root_hash: self.root_hash,
                capability_hash: self.capability_hash,
            })
        }

        pub(super) fn read_regular(
            &self,
            name: &str,
            limit: usize,
        ) -> Result<Vec<u8>, EntryReadError> {
            self.read_regular_state(name, limit)
                .map(|state| state.bytes)
        }

        fn read_regular_state(
            &self,
            name: &str,
            limit: usize,
        ) -> Result<RegularState, EntryReadError> {
            let name = component(name).map_err(|_| EntryReadError::Unsafe)?;
            // SAFETY: the directory descriptor and NUL-terminated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(libc::ELOOP) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
                        Err(EntryReadError::Unsafe)
                    }
                    _ => Err(EntryReadError::Io(error)),
                };
            }
            // SAFETY: a successful `openat` returns a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            let metadata = file.metadata().map_err(EntryReadError::Io)?;
            if !metadata.file_type().is_file() || metadata.nlink() != 1 {
                return Err(EntryReadError::Unsafe);
            }
            if metadata.len() > limit as u64 {
                return Err(EntryReadError::TooLarge);
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(metadata.len() as usize)
                .map_err(|_| EntryReadError::TooLarge)?;
            Read::by_ref(&mut file)
                .take((limit as u64).saturating_add(1))
                .read_to_end(&mut bytes)
                .map_err(EntryReadError::Io)?;
            if bytes.len() > limit {
                return Err(EntryReadError::TooLarge);
            }
            let final_metadata = file.metadata().map_err(EntryReadError::Io)?;
            if !final_metadata.file_type().is_file() || final_metadata.nlink() != 1 {
                return Err(EntryReadError::Unsafe);
            }
            if metadata.dev() != final_metadata.dev()
                || metadata.ino() != final_metadata.ino()
                || metadata.len() != final_metadata.len()
                || metadata.mtime() != final_metadata.mtime()
                || metadata.mtime_nsec() != final_metadata.mtime_nsec()
                || metadata.ctime() != final_metadata.ctime()
                || metadata.ctime_nsec() != final_metadata.ctime_nsec()
                || metadata.mode() != final_metadata.mode()
            {
                return Err(EntryReadError::Changed);
            }
            Ok(RegularState {
                bytes,
                mode: final_metadata.mode() & 0o7777,
            })
        }

        pub(super) fn open_managed_target(
            &self,
            relative_path: &str,
        ) -> Result<ManagedTarget, MutationIoError> {
            if !super::recovery::valid_active_path(relative_path) {
                return Err(MutationIoError::Unsafe);
            }
            let mut parts = relative_path.split('/').peekable();
            let mut directory = self.duplicate().map_err(|_| MutationIoError::Io)?;
            while let Some(part) = parts.next() {
                if parts.peek().is_none() {
                    return Ok(ManagedTarget {
                        directory,
                        name: part.to_owned(),
                    });
                }
                directory = directory
                    .open_directory(part)
                    .map_err(|error| match error {
                        OpenDirectoryError::Missing => MutationIoError::Conflict,
                        OpenDirectoryError::Unsafe => MutationIoError::Unsafe,
                        OpenDirectoryError::Io(_) => MutationIoError::Io,
                    })?;
            }
            Err(MutationIoError::Unsafe)
        }

        pub(super) fn verify_empty_workspace_root(&self) -> Result<(), MutationIoError> {
            let entries = self.entries(20_000).map_err(|error| match error {
                WorkspaceInspectError::ResourceLimit => MutationIoError::TooLarge,
                WorkspaceInspectError::UnsafeRoot => MutationIoError::Unsafe,
                WorkspaceInspectError::ChangedDuringRead => MutationIoError::Conflict,
                WorkspaceInspectError::NotFound
                | WorkspaceInspectError::NotDirectory
                | WorkspaceInspectError::PermissionDenied
                | WorkspaceInspectError::Io => MutationIoError::Io,
            })?;
            if !entries.is_empty() {
                return Err(MutationIoError::Conflict);
            }
            Ok(())
        }

        pub(super) fn create_categories_directory(&self) -> Result<(), MutationIoError> {
            self.create_private_directory_exclusive("categories")?;
            Ok(())
        }

        pub(super) fn create_category_directory(
            &self,
            relative_directory: &str,
        ) -> Result<(), MutationIoError> {
            let Some(name) = relative_directory.strip_prefix("categories/") else {
                return Err(MutationIoError::Unsafe);
            };
            if !super::valid_category_directory(name) {
                return Err(MutationIoError::Unsafe);
            }
            let categories = self.open_private_directory("categories")?;
            categories.create_private_directory_exclusive(name)?;
            Ok(())
        }

        pub(super) fn remove_empty_category_directory(
            &self,
            relative_directory: &str,
        ) -> Result<(), MutationIoError> {
            let Some(name) = relative_directory.strip_prefix("categories/") else {
                return Err(MutationIoError::Unsafe);
            };
            if !super::valid_category_directory(name) {
                return Err(MutationIoError::Unsafe);
            }
            let categories = self.open_private_directory("categories")?;
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: `categories` is retained and `name` is a validated component.
            if unsafe {
                libc::unlinkat(
                    categories.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::AT_REMOVEDIR,
                )
            } != 0
            {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Ok(()),
                    Some(libc::ENOTEMPTY) | Some(libc::EEXIST) => Err(MutationIoError::Conflict),
                    Some(libc::ENOTDIR) | Some(libc::ELOOP) => Err(MutationIoError::Unsafe),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: the retained `categories` descriptor is valid.
            if unsafe { libc::fsync(categories.descriptor.as_raw_fd()) } != 0 {
                return Err(MutationIoError::Unsupported);
            }
            Ok(())
        }

        pub(super) fn category_directory_exists(
            &self,
            relative_directory: &str,
        ) -> Result<bool, MutationIoError> {
            let Some(name) = relative_directory.strip_prefix("categories/") else {
                return Err(MutationIoError::Unsafe);
            };
            if !super::valid_category_directory(name) {
                return Err(MutationIoError::Unsafe);
            }
            let categories = self.open_private_directory("categories")?;
            match categories.open_private_directory(name) {
                Ok(_) => Ok(true),
                Err(MutationIoError::Conflict) => Ok(false),
                Err(error) => Err(error),
            }
        }

        pub(super) fn open_trash_target(
            &self,
            relative_path: &str,
        ) -> Result<ManagedTarget, MutationIoError> {
            self.trash_target(relative_path, false)
        }

        pub(super) fn prepare_trash_target(
            &self,
            relative_path: &str,
        ) -> Result<ManagedTarget, MutationIoError> {
            self.trash_target(relative_path, true)
        }

        fn trash_target(
            &self,
            relative_path: &str,
            create_parents: bool,
        ) -> Result<ManagedTarget, MutationIoError> {
            if !super::recovery::valid_trash_path(relative_path) {
                return Err(MutationIoError::Unsafe);
            }
            let mut parts = relative_path.split('/').peekable();
            let mut directory = self.duplicate().map_err(|_| MutationIoError::Io)?;
            while let Some(part) = parts.next() {
                if parts.peek().is_none() {
                    return Ok(ManagedTarget {
                        directory,
                        name: part.to_owned(),
                    });
                }
                directory = if create_parents {
                    directory.ensure_private_directory(part)?
                } else {
                    directory.open_private_directory(part)?
                };
            }
            Err(MutationIoError::Unsafe)
        }

        pub(super) fn identity(&self) -> Result<(u64, u64), MutationIoError> {
            descriptor_identity(&self.descriptor).map_err(|_| MutationIoError::Io)
        }

        pub(super) fn root_hash(&self) -> [u8; 32] {
            self.root_hash
        }

        pub(super) fn capability_hash(&self) -> [u8; 32] {
            self.capability_hash
        }

        pub(super) fn duplicate(&self) -> Result<Self, io::Error> {
            // Reopen `.` to get an independent directory-stream offset while
            // remaining bound to the retained directory rather than a path.
            // SAFETY: the descriptor and static NUL-terminated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    c".".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                Err(io::Error::last_os_error())
            } else {
                // SAFETY: successful `openat` returns a newly owned descriptor.
                Ok(Self {
                    descriptor: unsafe { OwnedFd::from_raw_fd(raw) },
                    root_hash: self.root_hash,
                    capability_hash: self.capability_hash,
                })
            }
        }

        fn ensure_private_directory(&self, name: &str) -> Result<Self, MutationIoError> {
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: the retained parent and validated component are valid.
            let created =
                unsafe { libc::mkdirat(self.descriptor.as_raw_fd(), name.as_ptr(), 0o700) } == 0;
            if !created {
                match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => {}
                    _ => return Err(MutationIoError::Io),
                }
            }
            // SAFETY: the retained parent and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            if created {
                // SAFETY: descriptor is owned and valid.
                if unsafe { libc::fchmod(descriptor.as_raw_fd(), 0o700) } != 0 {
                    return Err(MutationIoError::Io);
                }
            }
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptor/status are valid.
            if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
                return Err(MutationIoError::Io);
            }
            // SAFETY: successful `fstat` initialized the structure.
            let status = unsafe { status.assume_init() };
            if status.st_uid != unsafe { libc::geteuid() }
                || status.st_mode & libc::S_IFMT != libc::S_IFDIR
                || status.st_mode & 0o7777 != 0o700
            {
                return Err(MutationIoError::Unsafe);
            }
            // SAFETY: the parent descriptor is a retained directory.
            if unsafe { libc::fsync(self.descriptor.as_raw_fd()) } != 0 {
                return Err(MutationIoError::Unsupported);
            }
            Ok(Self {
                descriptor,
                root_hash: self.root_hash,
                capability_hash: self.capability_hash,
            })
        }

        fn create_private_directory_exclusive(&self, name: &str) -> Result<Self, MutationIoError> {
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: the retained parent and validated component are valid.
            if unsafe { libc::mkdirat(self.descriptor.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => Err(MutationIoError::Conflict),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: the retained parent and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return Err(MutationIoError::Unsafe);
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            // SAFETY: descriptor is valid and owned.
            if unsafe { libc::fchmod(descriptor.as_raw_fd(), 0o700) } != 0 {
                return Err(MutationIoError::Io);
            }
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptor/status are valid.
            if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
                return Err(MutationIoError::Io);
            }
            // SAFETY: successful `fstat` initialized the structure.
            let status = unsafe { status.assume_init() };
            if status.st_uid != unsafe { libc::geteuid() }
                || status.st_mode & libc::S_IFMT != libc::S_IFDIR
                || status.st_mode & 0o7777 != 0o700
            {
                return Err(MutationIoError::Unsafe);
            }
            // SAFETY: the retained parent descriptor is valid.
            if unsafe { libc::fsync(self.descriptor.as_raw_fd()) } != 0 {
                return Err(MutationIoError::Unsupported);
            }
            Ok(Self {
                descriptor,
                root_hash: self.root_hash,
                capability_hash: self.capability_hash,
            })
        }

        pub(super) fn open_private_directory(&self, name: &str) -> Result<Self, MutationIoError> {
            let name = component(name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: the retained parent and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.descriptor.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Err(MutationIoError::Conflict),
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: descriptor/status are valid.
            if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
                return Err(MutationIoError::Io);
            }
            // SAFETY: successful `fstat` initialized the structure.
            let status = unsafe { status.assume_init() };
            if status.st_uid != unsafe { libc::geteuid() }
                || status.st_mode & libc::S_IFMT != libc::S_IFDIR
                || status.st_mode & 0o7777 != 0o700
            {
                return Err(MutationIoError::Unsafe);
            }
            Ok(Self {
                descriptor,
                root_hash: self.root_hash,
                capability_hash: self.capability_hash,
            })
        }
    }

    fn fill_random(bytes: &mut [u8]) -> Result<(), io::Error> {
        let mut filled = 0usize;
        while filled < bytes.len() {
            // SAFETY: the remaining slice is writable for the requested length.
            let result = unsafe {
                libc::getrandom(bytes[filled..].as_mut_ptr().cast(), bytes.len() - filled, 0)
            };
            if result > 0 {
                filled += result as usize;
                continue;
            }
            if result == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "system randomness returned no bytes",
                ));
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        Ok(())
    }

    impl ManagedTarget {
        pub(super) fn state(&self, limit: usize) -> Result<ManagedState, MutationIoError> {
            self.named_state(&self.name, limit)
        }

        pub(super) fn temp_state(
            &self,
            temp_name: &str,
            limit: usize,
        ) -> Result<ManagedState, MutationIoError> {
            self.named_state(temp_name, limit)
        }

        fn named_state(&self, name: &str, limit: usize) -> Result<ManagedState, MutationIoError> {
            match self.directory.read_regular_state(name, limit) {
                Ok(state) => Ok(ManagedState::Regular(state)),
                Err(EntryReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                    Ok(ManagedState::Missing)
                }
                Err(EntryReadError::Unsafe) => Err(MutationIoError::Unsafe),
                Err(EntryReadError::TooLarge) => Err(MutationIoError::TooLarge),
                Err(EntryReadError::Changed) => Err(MutationIoError::Conflict),
                Err(EntryReadError::Io(_)) => Err(MutationIoError::Io),
            }
        }

        pub(super) fn write_temp(
            &self,
            temp_name: &str,
            bytes: &[u8],
            mode: u32,
        ) -> Result<(), MutationIoError> {
            if mode > 0o777 {
                return Err(MutationIoError::Unsafe);
            }
            let temp_name = component(temp_name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: directory descriptor and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.directory.descriptor.as_raw_fd(),
                    temp_name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK,
                    mode,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EEXIST) => Err(MutationIoError::Conflict),
                    Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            // SAFETY: descriptor is valid and the mode was bounded above.
            if unsafe { libc::fchmod(file.as_raw_fd(), mode) } != 0 {
                return Err(MutationIoError::Io);
            }
            file.write_all(bytes).map_err(|_| MutationIoError::Io)?;
            #[cfg(test)]
            managed_fail(ManagedFault::TempAfterWrite)?;
            file.sync_all().map_err(|_| MutationIoError::Unsupported)?;
            #[cfg(test)]
            managed_fail(ManagedFault::TempAfterFileSync)?;
            let metadata = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !metadata.file_type().is_file()
                || metadata.nlink() != 1
                || metadata.len() != bytes.len() as u64
                || metadata.mode() & 0o7777 != mode
            {
                return Err(MutationIoError::Unsafe);
            }
            Ok(())
        }

        pub(super) fn sync_existing_temp(
            &self,
            temp_name: &str,
            expected: &[u8],
            mode: u32,
        ) -> Result<(), MutationIoError> {
            let temp_name = component(temp_name).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: directory descriptor and validated component are valid.
            let raw = unsafe {
                libc::openat(
                    self.directory.descriptor.as_raw_fd(),
                    temp_name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if raw < 0 {
                return match io::Error::last_os_error().raw_os_error() {
                    Some(libc::ENOENT) => Err(MutationIoError::Conflict),
                    Some(libc::ELOOP) | Some(libc::ENXIO) | Some(libc::ENODEV) => {
                        Err(MutationIoError::Unsafe)
                    }
                    _ => Err(MutationIoError::Io),
                };
            }
            // SAFETY: successful `openat` returns a newly owned descriptor.
            let mut file = unsafe { File::from_raw_fd(raw) };
            let before = file.metadata().map_err(|_| MutationIoError::Io)?;
            if !before.file_type().is_file() || before.nlink() != 1 {
                return Err(MutationIoError::Unsafe);
            }
            if before.len() != expected.len() as u64 || before.mode() & 0o7777 != mode {
                return Err(MutationIoError::Conflict);
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(expected.len())
                .map_err(|_| MutationIoError::TooLarge)?;
            Read::by_ref(&mut file)
                .take((expected.len() as u64).saturating_add(1))
                .read_to_end(&mut bytes)
                .map_err(|_| MutationIoError::Io)?;
            let after_read = file.metadata().map_err(|_| MutationIoError::Io)?;
            if bytes != expected
                || before.dev() != after_read.dev()
                || before.ino() != after_read.ino()
                || before.len() != after_read.len()
                || before.mtime() != after_read.mtime()
                || before.mtime_nsec() != after_read.mtime_nsec()
                || before.ctime() != after_read.ctime()
                || before.ctime_nsec() != after_read.ctime_nsec()
                || before.mode() != after_read.mode()
            {
                return Err(MutationIoError::Conflict);
            }
            file.sync_all().map_err(|_| MutationIoError::Unsupported)?;
            #[cfg(test)]
            managed_fail(ManagedFault::ReusedTempAfterFileSync)?;
            let after_sync = file.metadata().map_err(|_| MutationIoError::Io)?;
            if after_read.dev() != after_sync.dev()
                || after_read.ino() != after_sync.ino()
                || after_read.len() != after_sync.len()
                || after_read.mtime() != after_sync.mtime()
                || after_read.mtime_nsec() != after_sync.mtime_nsec()
                || after_read.ctime() != after_sync.ctime()
                || after_read.ctime_nsec() != after_sync.ctime_nsec()
                || after_read.mode() != after_sync.mode()
            {
                return Err(MutationIoError::Conflict);
            }
            Ok(())
        }

        pub(super) fn install_no_replace(&self, temp_name: &str) -> Result<(), MutationIoError> {
            self.rename(temp_name, &self.name, libc::RENAME_NOREPLACE)
        }

        pub(super) fn exchange(&self, temp_name: &str) -> Result<(), MutationIoError> {
            self.rename(temp_name, &self.name, libc::RENAME_EXCHANGE)
        }

        pub(super) fn move_to_temp(&self, temp_name: &str) -> Result<(), MutationIoError> {
            self.rename(&self.name, temp_name, libc::RENAME_NOREPLACE)
        }

        pub(super) fn move_to(&self, destination: &Self) -> Result<(), MutationIoError> {
            self.rename_between(
                &self.name,
                destination,
                &destination.name,
                libc::RENAME_NOREPLACE,
            )
        }

        pub(super) fn move_temp_to(
            &self,
            temp_name: &str,
            destination: &Self,
        ) -> Result<(), MutationIoError> {
            self.rename_between(
                temp_name,
                destination,
                &destination.name,
                libc::RENAME_NOREPLACE,
            )
        }

        fn rename(&self, from: &str, to: &str, flags: u32) -> Result<(), MutationIoError> {
            self.rename_between(from, self, to, flags)
        }

        fn rename_between(
            &self,
            from: &str,
            destination: &Self,
            to: &str,
            flags: u32,
        ) -> Result<(), MutationIoError> {
            let from = component(from).map_err(|_| MutationIoError::Unsafe)?;
            let to = component(to).map_err(|_| MutationIoError::Unsafe)?;
            // SAFETY: both names and retained directory descriptors are valid.
            let result = unsafe {
                libc::syscall(
                    libc::SYS_renameat2,
                    self.directory.descriptor.as_raw_fd(),
                    from.as_ptr(),
                    destination.directory.descriptor.as_raw_fd(),
                    to.as_ptr(),
                    flags,
                )
            };
            if result == 0 {
                return Ok(());
            }
            match io::Error::last_os_error().raw_os_error() {
                Some(libc::EEXIST) | Some(libc::ENOENT) => Err(MutationIoError::Conflict),
                Some(libc::ENOSYS)
                | Some(libc::EINVAL)
                | Some(libc::EOPNOTSUPP)
                | Some(libc::EXDEV) => Err(MutationIoError::Unsupported),
                Some(libc::ELOOP) | Some(libc::ENOTDIR) => Err(MutationIoError::Unsafe),
                _ => Err(MutationIoError::Io),
            }
        }

        pub(super) fn sync_directory(&self) -> Result<(), MutationIoError> {
            // SAFETY: descriptor is a retained directory.
            if unsafe { libc::fsync(self.directory.descriptor.as_raw_fd()) } == 0 {
                Ok(())
            } else {
                Err(MutationIoError::Unsupported)
            }
        }
    }

    fn descriptor_identity(descriptor: &OwnedFd) -> Result<(u64, u64), io::Error> {
        let mut status = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `status` points to writable storage and the descriptor is valid.
        if unsafe { libc::fstat(descriptor.as_raw_fd(), status.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful `fstat` initialized the complete structure.
        let status = unsafe { status.assume_init() };
        Ok((status.st_dev, status.st_ino))
    }

    fn valid_gitdir_file(bytes: &[u8]) -> bool {
        let line = bytes.strip_suffix(b"\n").unwrap_or(bytes);
        let Some(target) = line.strip_prefix(b"gitdir: ") else {
            return false;
        };
        !target.is_empty()
            && !target.contains(&0)
            && !target.contains(&b'\n')
            && !target.contains(&b'\r')
    }

    struct DirectoryStream(*mut libc::DIR);

    impl Drop for DirectoryStream {
        fn drop(&mut self) {
            // SAFETY: `DirectoryStream` exclusively owns a successful `fdopendir`
            // result and closes it exactly once.
            unsafe { libc::closedir(self.0) };
        }
    }

    fn component(value: &str) -> Result<CString, ()> {
        let bytes = OsStr::new(value).as_bytes();
        if bytes.is_empty()
            || bytes.len() > 255
            || bytes == b"."
            || bytes == b".."
            || bytes.contains(&b'/')
            || bytes.contains(&b'\\')
        {
            return Err(());
        }
        CString::new(bytes).map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let number = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rehow-storage-test-{}-{number}",
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

    fn manifest(version: u32) -> String {
        format!(
            "schema_version: {version}\nworkspace_id: 0199a19f-4d15-7ca7-a645-e75311caa710\nname: Test Workspace\ncreated_at: \"2026-09-05T10:00:00Z\"\nupdated_at: \"2026-09-05T10:00:00Z\"\n"
        )
    }

    const CATEGORY: &str = "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux System\norder: 1000\nstatus: active\ncreated_at: \"2026-09-05T10:01:00Z\"\nupdated_at: \"2026-09-05T10:01:00Z\"\n";
    const GUIDE: &str = "---\nschema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: Install a Tarball\nsummary: Install a downloaded archive.\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags:\n  - linux\naliases:\n  - manual install\nstatus: draft\ncreated_at: \"2026-09-05T10:03:00Z\"\nupdated_at: \"2026-09-05T10:03:00Z\"\n---\n\n## Steps\n\nExtract the archive.\n";

    fn write_compatible(root: &Path) {
        fs::write(root.join("rehow.yaml"), manifest(1)).unwrap();
        let category = root.join("categories/linux-system--b6ba3a5551");
        fs::create_dir_all(&category).unwrap();
        fs::write(category.join("category.yaml"), CATEGORY).unwrap();
        fs::write(category.join("install--36c5735e10.md"), GUIDE).unwrap();
    }

    fn classification(root: &Path) -> WorkspaceInspection {
        inspect_workspace(root).unwrap().into_parts().1
    }

    #[test]
    fn classifies_empty_and_unrelated_folders() {
        let empty = TestDirectory::new();
        assert!(matches!(
            classification(empty.path()),
            WorkspaceInspection::Empty(_)
        ));
        fs::write(empty.path().join("notes.txt"), "hello").unwrap();
        assert!(matches!(
            classification(empty.path()),
            WorkspaceInspection::UnrelatedNonempty
        ));
    }

    #[test]
    fn classifies_root_markdown_collection() {
        let root = TestDirectory::new();
        fs::write(root.path().join("notes.md"), "# Notes\n").unwrap();
        assert!(matches!(
            classification(root.path()),
            WorkspaceInspection::OrdinaryMarkdown
        ));
    }

    #[test]
    fn classifies_nested_markdown_without_following_a_symlink() {
        let root = TestDirectory::new();
        let nested = root.path().join("notes/linux");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("install.md"), "# Install\n").unwrap();
        assert!(matches!(
            classification(root.path()),
            WorkspaceInspection::OrdinaryMarkdown
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let links = TestDirectory::new();
            symlink(nested.join("install.md"), links.path().join("outside.md")).unwrap();
            assert!(matches!(
                classification(links.path()),
                WorkspaceInspection::UnrelatedNonempty
            ));
        }
    }

    #[test]
    fn loads_a_compatible_workspace_and_revisions() {
        let root = TestDirectory::new();
        write_compatible(root.path());
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert_eq!(view.manifest().name(), "Test Workspace");
        assert_eq!(view.categories().len(), 1);
        assert_eq!(view.categories()[0].guides().len(), 1);
        assert_eq!(
            view.categories()[0].guides()[0].guide().title(),
            "Install a Tarball"
        );
        assert!(view.issues().is_empty());
        assert_eq!(
            view.manifest_revision().to_string(),
            document_revision("rehow.yaml", manifest(1).as_bytes()).to_string()
        );
    }

    #[test]
    fn detects_newer_and_malformed_manifests() {
        let newer = TestDirectory::new();
        fs::write(newer.path().join("rehow.yaml"), manifest(2)).unwrap();
        assert!(matches!(
            classification(newer.path()),
            WorkspaceInspection::NewerUnsupported { version: 2 }
        ));

        let malformed = TestDirectory::new();
        fs::write(malformed.path().join("rehow.yaml"), "not: [valid").unwrap();
        assert!(matches!(
            classification(malformed.path()),
            WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest
            }
        ));

        let interrupted_initialization = TestDirectory::new();
        fs::write(
            interrupted_initialization.path().join("rehow.yaml"),
            manifest(1),
        )
        .unwrap();
        assert!(matches!(
            classification(interrupted_initialization.path()),
            WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::CategoriesMissing
            }
        ));
    }

    #[test]
    fn isolates_a_guide_in_the_wrong_category() {
        let root = TestDirectory::new();
        write_compatible(root.path());
        let path = root
            .path()
            .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md");
        let changed = GUIDE.replace(
            "0199a1c2-d334-7d17-bd9c-f8b6ba3a5551",
            "0199a1c2-d334-7d17-bd9c-f8b6ba3a5552",
        );
        fs::write(path, changed).unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(view.categories()[0].guides().is_empty());
        assert_eq!(
            view.issues()[0].kind(),
            WorkspaceIssueKind::CategoryMismatch
        );
    }

    #[test]
    fn isolates_oversize_and_duplicate_guides() {
        let root = TestDirectory::new();
        write_compatible(root.path());
        let category = root.path().join("categories/linux-system--b6ba3a5551");
        fs::write(category.join("duplicate--0000000001.md"), GUIDE).unwrap();
        fs::write(
            category.join("oversize--0000000002.md"),
            vec![b'x'; GUIDE_LIMIT + 1],
        )
        .unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert_eq!(view.categories()[0].guides().len(), 1);
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::DuplicateGuideId)
        );
        assert!(
            view.issues()
                .iter()
                .any(|issue| issue.kind() == WorkspaceIssueKind::FileTooLarge)
        );
    }

    #[test]
    fn bounded_enumeration_refuses_one_over() {
        let root = TestDirectory::new();
        fs::write(root.path().join("a"), "a").unwrap();
        fs::write(root.path().join("b"), "b").unwrap();
        let capability = linux::DirectoryCapability::open_root(root.path()).unwrap();
        assert_eq!(
            capability.entries(1).unwrap_err(),
            WorkspaceInspectError::ResourceLimit
        );
    }

    #[test]
    fn reports_writability_and_root_or_ancestor_git_markers() {
        let repository = TestDirectory::new();
        fs::create_dir(repository.path().join(".git")).unwrap();
        let root_report = inspect_workspace(repository.path()).unwrap();
        assert!(root_report.facts().writable());
        assert!(root_report.facts().existing_git_repository());
        assert!(!root_report.facts().nested_git_repository());
        assert!(root_report.facts().git_probe_complete());
        assert!(!root_report.facts().unsafe_git_marker());

        let nested = repository.path().join("notes/rehow");
        fs::create_dir_all(&nested).unwrap();
        let nested_report = inspect_workspace(&nested).unwrap();
        assert!(!nested_report.facts().existing_git_repository());
        assert!(nested_report.facts().nested_git_repository());
        assert!(nested_report.facts().git_probe_complete());

        let worktree = TestDirectory::new();
        fs::write(worktree.path().join(".git"), "gitdir: ../metadata\n").unwrap();
        assert!(
            inspect_workspace(worktree.path())
                .unwrap()
                .facts()
                .existing_git_repository()
        );
    }

    #[cfg(unix)]
    #[test]
    fn reports_non_writable_and_unsafe_git_markers_without_following_them() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let root = TestDirectory::new();
        let outside = TestDirectory::new();
        symlink(outside.path(), root.path().join(".git")).unwrap();
        let unsafe_report = inspect_workspace(root.path()).unwrap();
        assert!(!unsafe_report.facts().existing_git_repository());
        assert!(unsafe_report.facts().unsafe_git_marker());

        fs::remove_file(root.path().join(".git")).unwrap();
        fs::write(root.path().join(".git"), "ordinary file\n").unwrap();
        let invalid_file_report = inspect_workspace(root.path()).unwrap();
        assert!(!invalid_file_report.facts().existing_git_repository());
        assert!(invalid_file_report.facts().unsafe_git_marker());

        fs::remove_file(root.path().join(".git")).unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let read_only_report = inspect_workspace(root.path()).unwrap();
        assert!(!read_only_report.facts().writable());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_socket_disguised_as_a_guide_without_blocking() {
        use std::os::unix::net::UnixListener;

        let root = TestDirectory::new();
        write_compatible(root.path());
        let guide = root
            .path()
            .join("categories/linux-system--b6ba3a5551/install--36c5735e10.md");
        fs::remove_file(&guide).unwrap();
        let _listener = UnixListener::bind(&guide).unwrap();
        let WorkspaceInspection::Compatible(view) = classification(root.path()) else {
            panic!("expected compatible Workspace");
        };
        assert!(view.categories()[0].guides().is_empty());
        assert_eq!(view.issues()[0].kind(), WorkspaceIssueKind::UnsafeEntry);
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_managed_symlinks_or_accepts_hardlinks() {
        use std::os::unix::fs::symlink;

        let root = TestDirectory::new();
        let outside = TestDirectory::new();
        fs::write(outside.path().join("manifest"), manifest(1)).unwrap();
        symlink(
            outside.path().join("manifest"),
            root.path().join("rehow.yaml"),
        )
        .unwrap();
        assert!(matches!(
            classification(root.path()),
            WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest
            }
        ));

        fs::remove_file(root.path().join("rehow.yaml")).unwrap();
        fs::hard_link(
            outside.path().join("manifest"),
            root.path().join("rehow.yaml"),
        )
        .unwrap();
        assert!(matches!(
            classification(root.path()),
            WorkspaceInspection::Malformed {
                reason: MalformedWorkspaceReason::Manifest
            }
        ));
    }

    #[test]
    fn revision_is_path_and_byte_sensitive() {
        let first = document_revision("rehow.yaml", b"abc");
        let same = document_revision("rehow.yaml", b"abc");
        let moved = document_revision("other.yaml", b"abc");
        let changed = document_revision("rehow.yaml", b"abd");
        assert_eq!(first, same);
        assert_ne!(first, moved);
        assert_ne!(first, changed);
        assert_eq!(
            first.to_string(),
            "rev:v1:ec918848dad4d31267309c57c165f16c9d725f0d6eebfae93ec1d60212eea0c2"
        );
    }
}

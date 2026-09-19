pub const APPLICATION_NAME: &str = "Rehow";

use rehow_core::{
    application::{
        AttachmentImportError, AttachmentImportOutcome, AttachmentRemovalError,
        AttachmentRemovalOutcome, CategoryCreateError, CategoryOrderRevision, CategoryRemoveError,
        CategoryReorderError, CategoryUpdateError, GUIDE_BODY_INPUT_LIMIT, GuideCreateError,
        GuideEdit, GuideRemoveError, GuideUpdateError, WorkspaceInitializationError,
        WorkspaceRefreshError, confirm_and_approve_guide_tested, confirm_and_remove_attachment,
        create_category, create_guide, initialize_workspace,
        refresh_open_workspace as refresh_workspace_view, remove_category, remove_guide,
        reorder_categories, select_and_import_attachment, set_category_archived,
        set_guide_archived, update_category_details, update_guide, update_guide_details,
    },
    domain::GuideStatus,
    git::{GitReadError, GitRepositoryStatus, read_git_status},
    markdown::{SafeMarkdownDocument, parse_safe_markdown},
    protocol::{AttachmentId, CategoryId, GuideId, Revision},
    search::{SearchHit, SearchStatusScope, search_workspace_in_scope},
    storage::{
        EmptyWorkspaceTarget, WorkspaceFolderFacts, WorkspaceInspection, WorkspaceView,
        WorkspaceWatchSignal, WorkspaceWatcher, inspect_workspace,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::PathBuf,
    str::FromStr,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tauri::Emitter;

const WORKSPACE_INVALIDATED_EVENT: &str = "rehow://workspace-invalidated/v1";

#[derive(Default)]
struct DesktopState {
    workspace: Mutex<Option<DesktopWorkspace>>,
    pending_empty: Mutex<Option<PendingWorkspace>>,
    next_watch_session: AtomicU64,
}

impl DesktopState {
    fn next_watch_session(&self) -> u64 {
        self.next_watch_session
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.wrapping_add(1).max(1))
            })
            .unwrap_or(0)
            .wrapping_add(1)
            .max(1)
    }
}

struct DesktopWorkspace {
    view: Box<WorkspaceView>,
    facts: WorkspaceFolderFacts,
    watcher: Option<WorkspaceWatcher>,
    watch_status: WatchStatus,
    watch_session: u64,
}

#[derive(Clone, Copy)]
enum WatchStatus {
    Active,
    Degraded,
}

impl WatchStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Degraded => "degraded",
        }
    }
}

struct PendingWorkspace {
    target: EmptyWorkspaceTarget,
    facts: WorkspaceFolderFacts,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum OpenWorkspaceResult {
    Cancelled,
    Empty {
        facts: FolderFactsPayload,
    },
    Compatible {
        workspace: WorkspacePayload,
    },
    NewerUnsupported {
        version: u32,
        facts: FolderFactsPayload,
    },
    OrdinaryMarkdown {
        facts: FolderFactsPayload,
    },
    UnrelatedNonempty {
        facts: FolderFactsPayload,
    },
    Malformed {
        reason: &'static str,
        facts: FolderFactsPayload,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderFactsPayload {
    writable: bool,
    existing_git_repository: bool,
    nested_git_repository: bool,
    git_probe_complete: bool,
    unsafe_git_marker: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspacePayload {
    id: String,
    name: String,
    categories: Vec<CategoryPayload>,
    guides: Vec<GuidePayload>,
    issue_count: usize,
    facts: FolderFactsPayload,
    watch_status: &'static str,
    watch_session: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceWatchEventPayload {
    workspace_id: String,
    watch_session: String,
    kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitStatusPayload {
    kind: &'static str,
    version: Option<String>,
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
    failure: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CategoryPayload {
    id: String,
    name: String,
    summary: Option<String>,
    status: &'static str,
    revision: String,
    guide_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuidePayload {
    id: String,
    title: String,
    summary: String,
    category_id: String,
    category: String,
    status: &'static str,
    updated_at: String,
    tags: Vec<String>,
    aliases: Vec<String>,
    attachments: Vec<AttachmentPayload>,
    matches: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchHitPayload {
    guide_id: String,
    excerpt: String,
    excerpt_field: &'static str,
    matched_fields: Vec<&'static str>,
    highlights: Vec<SearchHighlightPayload>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchHighlightPayload {
    start: u16,
    end: u16,
}

impl From<SearchHit> for SearchHitPayload {
    fn from(hit: SearchHit) -> Self {
        Self {
            guide_id: hit.guide_id().to_string(),
            excerpt: hit.excerpt().to_owned(),
            excerpt_field: hit.excerpt_field().as_str(),
            matched_fields: hit
                .matched_fields()
                .iter()
                .map(|field| field.as_str())
                .collect(),
            highlights: hit
                .highlights()
                .iter()
                .map(|range| SearchHighlightPayload {
                    start: range.start(),
                    end: range.end(),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentPayload {
    id: String,
    title: String,
    size_bytes: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuideBodyPayload {
    id: String,
    revision: String,
    body: String,
    document: SafeMarkdownDocument,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveGuidePayload {
    workspace: WorkspacePayload,
    guide: GuideBodyPayload,
    changed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoveGuidePayload {
    workspace: WorkspacePayload,
    removed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateCategoryPayload {
    workspace: WorkspacePayload,
    category_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveCategoryPayload {
    workspace: WorkspacePayload,
    changed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CategoryOrderInput {
    id: String,
    expected_revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateGuidePayload {
    workspace: WorkspacePayload,
    guide: GuideBodyPayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeCommandError {
    code: &'static str,
    message: String,
}

impl WorkspacePayload {
    fn from_view(
        view: &WorkspaceView,
        facts: FolderFactsPayload,
        watch_status: WatchStatus,
        watch_session: u64,
    ) -> Self {
        let guide_capacity = view
            .categories()
            .iter()
            .map(|category| category.guides().len())
            .sum();
        let mut categories = Vec::with_capacity(view.categories().len());
        let mut guides = Vec::with_capacity(guide_capacity);
        for loaded_category in view.categories() {
            let category = loaded_category.category();
            categories.push(CategoryPayload {
                id: category.id().to_string(),
                name: category.name().to_owned(),
                summary: category.summary().map(str::to_owned),
                status: match category.status() {
                    rehow_core::domain::CategoryStatus::Active => "active",
                    rehow_core::domain::CategoryStatus::Archived => "archived",
                },
                revision: loaded_category.descriptor_revision().to_string(),
                guide_count: loaded_category.guides().len(),
            });
            for loaded_guide in loaded_category.guides() {
                let guide = loaded_guide.guide();
                let mut matches = guide.tags().join(" ");
                if !matches.is_empty() && !guide.aliases().is_empty() {
                    matches.push(' ');
                }
                matches.push_str(&guide.aliases().join(" "));
                guides.push(GuidePayload {
                    id: guide.id().to_string(),
                    title: guide.title().to_owned(),
                    summary: guide.summary().to_owned(),
                    category_id: category.id().to_string(),
                    category: category.name().to_owned(),
                    status: match loaded_guide.effective_status() {
                        GuideStatus::Draft => "Draft",
                        GuideStatus::Tested => "Tested",
                        GuideStatus::Stale => "Stale",
                        GuideStatus::Archived => "Archived",
                    },
                    updated_at: guide.updated_at().to_string(),
                    tags: guide.tags().to_vec(),
                    aliases: guide.aliases().to_vec(),
                    attachments: guide
                        .attachments()
                        .unwrap_or_default()
                        .iter()
                        .map(|attachment| AttachmentPayload {
                            id: attachment.id().to_string(),
                            title: attachment.title().to_owned(),
                            size_bytes: attachment.size_bytes(),
                        })
                        .collect(),
                    matches,
                });
            }
        }
        Self {
            id: view.manifest().workspace_id().to_string(),
            name: view.manifest().name().to_owned(),
            categories,
            guides,
            issue_count: view.issues().len(),
            facts,
            watch_status: watch_status.as_str(),
            watch_session: watch_session.to_string(),
        }
    }
}

impl DesktopWorkspace {
    fn new(
        view: Box<WorkspaceView>,
        facts: WorkspaceFolderFacts,
        watch_session: u64,
        app: &tauri::AppHandle,
    ) -> Self {
        let mut workspace = Self {
            view,
            facts,
            watcher: None,
            watch_status: WatchStatus::Degraded,
            watch_session,
        };
        workspace.rebuild_watcher(app);
        workspace
    }

    fn payload(&self) -> WorkspacePayload {
        WorkspacePayload::from_view(
            &self.view,
            FolderFactsPayload::from(self.facts),
            self.watch_status,
            self.watch_session,
        )
    }

    fn rebuild_watcher(&mut self, app: &tauri::AppHandle) {
        let workspace_id = self.view.manifest().workspace_id().to_string();
        let watch_session = self.watch_session.to_string();
        let app = app.clone();
        match WorkspaceWatcher::start(&self.view, move |signal| {
            let kind = match signal {
                WorkspaceWatchSignal::Changed => "changed",
                WorkspaceWatchSignal::Degraded => "degraded",
            };
            let _ = app.emit(
                WORKSPACE_INVALIDATED_EVENT,
                WorkspaceWatchEventPayload {
                    workspace_id: workspace_id.clone(),
                    watch_session: watch_session.clone(),
                    kind,
                },
            );
        }) {
            Ok(watcher) => {
                self.watcher = Some(watcher);
                self.watch_status = WatchStatus::Active;
            }
            Err(_) => self.watch_status = WatchStatus::Degraded,
        }
    }
}

fn guide_body_payload(
    workspace: &WorkspaceView,
    guide_id: &GuideId,
) -> Result<GuideBodyPayload, String> {
    let loaded = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|loaded| loaded.guide().id() == guide_id)
        .ok_or_else(|| "Guide was not found".to_owned())?;
    let guide = loaded.guide();
    let body = std::str::from_utf8(guide.body())
        .map_err(|_| "Guide Markdown is not valid UTF-8".to_owned())?;
    let document = parse_safe_markdown(body).map_err(|error| error.to_string())?;
    Ok(GuideBodyPayload {
        id: guide.id().to_string(),
        revision: loaded.revision().to_string(),
        body: body.to_owned(),
        document,
    })
}

impl From<WorkspaceFolderFacts> for FolderFactsPayload {
    fn from(facts: WorkspaceFolderFacts) -> Self {
        Self {
            writable: facts.writable(),
            existing_git_repository: facts.existing_git_repository(),
            nested_git_repository: facts.nested_git_repository(),
            git_probe_complete: facts.git_probe_complete(),
            unsafe_git_marker: facts.unsafe_git_marker(),
        }
    }
}

#[tauri::command]
fn read_open_guide(
    guide_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<GuideBodyPayload, String> {
    let guide_id = GuideId::from_str(&guide_id).map_err(|_| "Guide ID is invalid".to_owned())?;
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace session is unavailable".to_owned())?;
    let workspace = guard
        .as_ref()
        .ok_or_else(|| "No Workspace is open".to_owned())?;
    guide_body_payload(&workspace.view, &guide_id)
}

#[tauri::command]
fn search_open_workspace(
    query: String,
    scope: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<Vec<SearchHitPayload>, String> {
    let scope = search_status_scope(&scope).ok_or_else(|| "Search scope is invalid".to_owned())?;
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace session is unavailable".to_owned())?;
    let workspace = guard
        .as_ref()
        .ok_or_else(|| "No Workspace is open".to_owned())?;
    search_workspace_in_scope(&workspace.view, &query, scope, 100)
        .map(|hits| hits.into_iter().map(SearchHitPayload::from).collect())
        .map_err(|error| match error {
            rehow_core::search::SearchError::QueryTooLarge => "Search query is too long".to_owned(),
            rehow_core::search::SearchError::TooManyTerms => {
                "Search query has too many terms".to_owned()
            }
        })
}

#[tauri::command]
fn git_status_open_workspace(
    state: tauri::State<'_, DesktopState>,
) -> Result<GitStatusPayload, String> {
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace session is unavailable".to_owned())?;
    let workspace = guard
        .as_ref()
        .ok_or_else(|| "No Workspace is open".to_owned())?;
    match read_git_status(&workspace.view) {
        Ok(status) => {
            let version = status.version().map(str::to_owned);
            if !status.available() {
                return Ok(empty_git_payload("unavailable", None));
            }
            match status.repository() {
                GitRepositoryStatus::NotRepository => {
                    Ok(empty_git_payload("notRepository", version))
                }
                GitRepositoryStatus::Unsafe => Ok(empty_git_payload("unsafe", version)),
                GitRepositoryStatus::Ready(repository) => Ok(GitStatusPayload {
                    kind: "ready",
                    version,
                    branch: repository.branch().map(str::to_owned),
                    detached: repository.detached(),
                    upstream: repository.upstream().map(str::to_owned),
                    divergence_known: repository.divergence_known(),
                    ahead: repository.ahead(),
                    behind: repository.behind(),
                    changed: repository.changed(),
                    untracked: repository.untracked(),
                    conflicts: repository.conflicts(),
                    remotes: repository.remotes().to_vec(),
                    remote_count: repository.remote_count(),
                    failure: None,
                }),
            }
        }
        Err(error) => {
            let failure = match error {
                GitReadError::Spawn => "Git could not be started.",
                GitReadError::ExitStatus => "Git returned a non-success status.",
                GitReadError::Signal => "Git was terminated by a signal.",
                GitReadError::Timeout => "Git did not finish before the timeout.",
                GitReadError::OutputLimit => "Git output exceeded the safety limit.",
                GitReadError::InvalidOutput => "Git returned an unsupported status format.",
            };
            let mut payload = empty_git_payload("failed", None);
            payload.failure = Some(failure);
            Ok(payload)
        }
    }
}

fn empty_git_payload(kind: &'static str, version: Option<String>) -> GitStatusPayload {
    GitStatusPayload {
        kind,
        version,
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
        failure: None,
    }
}

fn search_status_scope(scope: &str) -> Option<SearchStatusScope> {
    match scope {
        "active" => Some(SearchStatusScope::Active),
        "tested" => Some(SearchStatusScope::Tested),
        "draft" => Some(SearchStatusScope::Draft),
        "stale" => Some(SearchStatusScope::Stale),
        "archived" => Some(SearchStatusScope::Archived),
        "all" => Some(SearchStatusScope::All),
        _ => None,
    }
}

#[tauri::command]
fn refresh_open_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
) -> Result<WorkspacePayload, NativeCommandError> {
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    refresh_workspace_view(&mut workspace.view).map_err(|error| {
        native_command_error(
            workspace_refresh_code(error),
            workspace_refresh_message(error),
        )
    })?;
    workspace.rebuild_watcher(&app);
    Ok(workspace.payload())
}

fn workspace_refresh_code(error: WorkspaceRefreshError) -> &'static str {
    match error {
        WorkspaceRefreshError::NotFound => "notFound",
        WorkspaceRefreshError::NotDirectory => "notDirectory",
        WorkspaceRefreshError::PermissionDenied => "permissionDenied",
        WorkspaceRefreshError::UnsafeStorage => "unsafeStorage",
        WorkspaceRefreshError::ResourceLimit => "resourceLimit",
        WorkspaceRefreshError::ChangedDuringRead => "changedDuringRead",
        WorkspaceRefreshError::Io => "io",
    }
}

fn workspace_refresh_message(error: WorkspaceRefreshError) -> String {
    match error {
        WorkspaceRefreshError::NotFound => {
            "The Workspace folder is no longer available. Reopen it when it returns.".to_owned()
        }
        WorkspaceRefreshError::NotDirectory => {
            "The Workspace location is no longer a folder. Choose another Workspace.".to_owned()
        }
        WorkspaceRefreshError::PermissionDenied => {
            "Rehow no longer has permission to read this Workspace.".to_owned()
        }
        WorkspaceRefreshError::UnsafeStorage => {
            "The Workspace path is unsafe or changed type. Rehow kept the previous view.".to_owned()
        }
        WorkspaceRefreshError::ResourceLimit => {
            "The Workspace exceeds Rehow’s safe refresh limits.".to_owned()
        }
        WorkspaceRefreshError::ChangedDuringRead => {
            "The Workspace changed while refreshing. Try again after file activity settles."
                .to_owned()
        }
        WorkspaceRefreshError::Io => {
            "The Workspace could not be refreshed. Check folder access and try again.".to_owned()
        }
    }
}

#[tauri::command]
fn save_open_guide_body(
    guide_id: String,
    expected_revision: String,
    body: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveGuidePayload, String> {
    if body.len() > GUIDE_BODY_INPUT_LIMIT {
        return Err("Guide content is too large. Reduce it below 2 MB and try again.".to_owned());
    }
    let guide_id = GuideId::from_str(&guide_id).map_err(|_| "Guide ID is invalid".to_owned())?;
    let expected_revision = Revision::from_str(&expected_revision)
        .map_err(|_| "Guide revision is invalid. Reopen the Guide and try again.".to_owned())?;
    let mut guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace session is unavailable".to_owned())?;
    let workspace = guard
        .as_mut()
        .ok_or_else(|| "No Workspace is open".to_owned())?;
    if !workspace.facts.writable() {
        return Err("This Workspace is read-only. Choose a writable Workspace to save.".to_owned());
    }
    let (state_home, runtime_directory) = native_recovery_locations()?;
    fs::create_dir_all(&state_home)
        .map_err(|_| "Rehow could not prepare its private recovery folder.".to_owned())?;
    let outcome = update_guide(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        guide_id,
        expected_revision,
        GuideEdit::ReplaceBody(body.into_bytes()),
    )
    .map_err(guide_update_message)?;
    Ok(SaveGuidePayload {
        workspace: workspace.payload(),
        guide: guide_body_payload(&workspace.view, &guide_id)?,
        changed: outcome.changed(),
    })
}

#[tauri::command]
#[allow(
    clippy::too_many_arguments,
    reason = "the Tauri command keeps every bounded Guide detail explicit"
)]
fn save_open_guide_details(
    guide_id: String,
    expected_revision: String,
    category_id: String,
    title: String,
    summary: String,
    tags: Vec<String>,
    aliases: Vec<String>,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveGuidePayload, NativeCommandError> {
    if guide_id.len() > 64
        || expected_revision.len() > 96
        || category_id.len() > 64
        || title.len() > 1_024
        || summary.len() > 4_096
        || tags.len() > 64
        || aliases.len() > 64
        || tags.iter().any(|value| value.len() > 256)
        || aliases.iter().any(|value| value.len() > 800)
    {
        return Err(native_command_error(
            "invalidInput",
            "Guide details are too long.".to_owned(),
        ));
    }
    let guide_id = GuideId::from_str(&guide_id)
        .map_err(|_| native_command_error("invalidGuide", "Guide ID is invalid.".to_owned()))?;
    let category_id = CategoryId::from_str(&category_id).map_err(|_| {
        native_command_error("invalidCategory", "Category ID is invalid.".to_owned())
    })?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Guide revision is invalid. Reopen the Guide and try again.".to_owned(),
        )
    })?;
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = update_guide_details(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        guide_id,
        expected_revision,
        category_id,
        title,
        summary,
        tags,
        aliases,
    )
    .map_err(|error| native_command_error(guide_update_code(error), guide_update_message(error)))?;
    Ok(SaveGuidePayload {
        workspace: workspace.payload(),
        guide: guide_body_payload(&workspace.view, &guide_id)
            .map_err(|message| native_command_error("refreshRequired", message))?,
        changed: outcome.changed(),
    })
}

#[tauri::command]
fn set_open_guide_archived(
    guide_id: String,
    expected_revision: String,
    archived: bool,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveGuidePayload, NativeCommandError> {
    if guide_id.len() > 64 || expected_revision.len() > 96 {
        return Err(native_command_error(
            "invalidInput",
            "Guide identity is too long.".to_owned(),
        ));
    }
    let guide_id = GuideId::from_str(&guide_id)
        .map_err(|_| native_command_error("invalidGuide", "Guide ID is invalid.".to_owned()))?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Guide revision is invalid. Reopen the Guide and try again.".to_owned(),
        )
    })?;
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = set_guide_archived(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        guide_id,
        expected_revision,
        archived,
    )
    .map_err(|error| native_command_error(guide_update_code(error), guide_update_message(error)))?;
    Ok(SaveGuidePayload {
        workspace: workspace.payload(),
        guide: guide_body_payload(&workspace.view, &guide_id)
            .map_err(|message| native_command_error("refreshRequired", message))?,
        changed: outcome.changed(),
    })
}

#[tauri::command]
fn approve_open_guide_tested(
    guide_id: String,
    expected_revision: String,
    window: tauri::Window,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveGuidePayload, NativeCommandError> {
    if guide_id.len() > 64 || expected_revision.len() > 96 {
        return Err(native_command_error(
            "invalidInput",
            "Guide identity is too long.".to_owned(),
        ));
    }
    let guide_id = GuideId::from_str(&guide_id)
        .map_err(|_| native_command_error("invalidGuide", "Guide ID is invalid.".to_owned()))?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Guide revision is invalid. Reopen the Guide and try again.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let loaded = workspace
        .view
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or_else(|| native_command_error("notFound", "Guide was not found.".to_owned()))?;
    if loaded.revision() != expected_revision {
        return Err(native_command_error(
            "revisionConflict",
            "This Guide changed on disk. Reopen it before marking it tested.".to_owned(),
        ));
    }
    if loaded.effective_status() == GuideStatus::Archived {
        return Err(native_command_error(
            "invalidEdit",
            "Restore this Guide before marking it tested.".to_owned(),
        ));
    }
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let outcome = confirm_and_approve_guide_tested(
        &window,
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        guide_id,
        expected_revision,
    )
    .map_err(|error| native_command_error(guide_update_code(error), guide_update_message(error)))?;
    Ok(SaveGuidePayload {
        workspace: workspace.payload(),
        guide: guide_body_payload(&workspace.view, &guide_id)
            .map_err(|message| native_command_error("refreshRequired", message))?,
        changed: outcome.changed(),
    })
}

#[tauri::command]
fn import_open_guide_attachment(
    guide_id: String,
    expected_revision: String,
    window: tauri::Window,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveGuidePayload, NativeCommandError> {
    if guide_id.len() > 64 || expected_revision.len() > 96 {
        return Err(native_command_error(
            "invalidInput",
            "Guide identity is too long.".to_owned(),
        ));
    }
    let guide_id = GuideId::from_str(&guide_id)
        .map_err(|_| native_command_error("invalidGuide", "Guide ID is invalid.".to_owned()))?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Guide revision is invalid. Reopen the Guide and try again.".to_owned(),
        )
    })?;
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = select_and_import_attachment(
        &window,
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        guide_id,
        expected_revision,
    )
    .map_err(|error| {
        native_command_error(
            attachment_import_code(error),
            attachment_import_message(error),
        )
    })?;
    Ok(SaveGuidePayload {
        workspace: workspace.payload(),
        guide: guide_body_payload(&workspace.view, &guide_id)
            .map_err(|message| native_command_error("refreshRequired", message))?,
        changed: matches!(outcome, AttachmentImportOutcome::Imported { .. }),
    })
}

fn attachment_import_code(error: AttachmentImportError) -> &'static str {
    match error {
        AttachmentImportError::NotFound => "notFound",
        AttachmentImportError::RevisionConflict | AttachmentImportError::SourceChanged => {
            "revisionConflict"
        }
        AttachmentImportError::GuideArchived => "guideArchived",
        AttachmentImportError::AttachmentLimit => "attachmentLimit",
        AttachmentImportError::DuplicateAttachment => "duplicateAttachment",
        AttachmentImportError::InvalidSource => "invalidSource",
        AttachmentImportError::SourceTooLarge => "sourceTooLarge",
        AttachmentImportError::UnsafeRoundTrip => "unsafeRoundTrip",
        AttachmentImportError::Busy => "busy",
        AttachmentImportError::UnsafeStorage => "unsafeStorage",
        AttachmentImportError::UnsupportedFilesystem => "unsupportedFilesystem",
        AttachmentImportError::ResourceLimit => "resourceLimit",
        AttachmentImportError::Io => "io",
        AttachmentImportError::CommittedRefreshRequired { .. } => "committedRefresh",
    }
}

fn attachment_import_message(error: AttachmentImportError) -> String {
    match error {
        AttachmentImportError::NotFound => "Guide was not found.".to_owned(),
        AttachmentImportError::RevisionConflict | AttachmentImportError::SourceChanged => {
            "The Guide or selected file changed. Refresh and try again.".to_owned()
        }
        AttachmentImportError::GuideArchived => {
            "Restore this Guide before adding an Attachment.".to_owned()
        }
        AttachmentImportError::AttachmentLimit => {
            "This Guide already has the maximum of 256 Attachments.".to_owned()
        }
        AttachmentImportError::DuplicateAttachment => {
            "This file is already attached to the Guide.".to_owned()
        }
        AttachmentImportError::InvalidSource => {
            "Choose a regular, single-link file.".to_owned()
        }
        AttachmentImportError::SourceTooLarge => {
            "Attachments must be 32 MiB or smaller.".to_owned()
        }
        AttachmentImportError::UnsafeRoundTrip => {
            "This Guide’s formatting cannot be updated safely. Simplify its Attachment metadata first."
                .to_owned()
        }
        AttachmentImportError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        AttachmentImportError::UnsafeStorage => {
            "Rehow refused an unsafe Attachment or Workspace path.".to_owned()
        }
        AttachmentImportError::UnsupportedFilesystem => {
            "This filesystem cannot provide the atomic durability Rehow requires.".to_owned()
        }
        AttachmentImportError::ResourceLimit => {
            "Attachment import exceeded a safe resource limit.".to_owned()
        }
        AttachmentImportError::Io => {
            "The Attachment could not be imported. Check file access and try again.".to_owned()
        }
        AttachmentImportError::CommittedRefreshRequired { .. } => {
            "The Attachment was imported, but Rehow could not refresh it. Reopen the Workspace."
                .to_owned()
        }
    }
}

#[tauri::command]
fn remove_open_guide_attachment(
    guide_id: String,
    attachment_id: String,
    expected_revision: String,
    window: tauri::Window,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveGuidePayload, NativeCommandError> {
    if guide_id.len() > 64 || attachment_id.len() > 64 || expected_revision.len() > 96 {
        return Err(native_command_error(
            "invalidInput",
            "Guide or Attachment identity is too long.".to_owned(),
        ));
    }
    let guide_id = GuideId::from_str(&guide_id)
        .map_err(|_| native_command_error("invalidGuide", "Guide ID is invalid.".to_owned()))?;
    let attachment_id = AttachmentId::from_str(&attachment_id).map_err(|_| {
        native_command_error("invalidAttachment", "Attachment ID is invalid.".to_owned())
    })?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Guide revision is invalid. Reopen the Guide and try again.".to_owned(),
        )
    })?;
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = confirm_and_remove_attachment(
        &window,
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        guide_id,
        attachment_id,
        expected_revision,
    )
    .map_err(|error| {
        native_command_error(
            attachment_removal_code(error),
            attachment_removal_message(error),
        )
    })?;
    Ok(SaveGuidePayload {
        workspace: workspace.payload(),
        guide: guide_body_payload(&workspace.view, &guide_id)
            .map_err(|message| native_command_error("refreshRequired", message))?,
        changed: matches!(outcome, AttachmentRemovalOutcome::Removed { .. }),
    })
}

fn attachment_removal_code(error: AttachmentRemovalError) -> &'static str {
    match error {
        AttachmentRemovalError::GuideNotFound => "notFound",
        AttachmentRemovalError::AttachmentNotFound => "attachmentNotFound",
        AttachmentRemovalError::RevisionConflict => "revisionConflict",
        AttachmentRemovalError::GuideArchived => "guideArchived",
        AttachmentRemovalError::AttachmentChanged => "attachmentChanged",
        AttachmentRemovalError::UnsafeRoundTrip => "unsafeRoundTrip",
        AttachmentRemovalError::Busy => "busy",
        AttachmentRemovalError::UnsafeStorage => "unsafeStorage",
        AttachmentRemovalError::UnsupportedFilesystem => "unsupportedFilesystem",
        AttachmentRemovalError::ResourceLimit => "resourceLimit",
        AttachmentRemovalError::Io => "io",
        AttachmentRemovalError::CommittedRefreshRequired { .. } => "committedRefresh",
    }
}

fn attachment_removal_message(error: AttachmentRemovalError) -> String {
    match error {
        AttachmentRemovalError::GuideNotFound => "Guide was not found.".to_owned(),
        AttachmentRemovalError::AttachmentNotFound => {
            "Attachment was not found. Refresh the Workspace and try again.".to_owned()
        }
        AttachmentRemovalError::RevisionConflict => {
            "The Guide changed. Refresh and try again.".to_owned()
        }
        AttachmentRemovalError::GuideArchived => {
            "Restore this Guide before removing an Attachment.".to_owned()
        }
        AttachmentRemovalError::AttachmentChanged => {
            "The Guide or Attachment changed. Refresh and inspect it before trying again."
                .to_owned()
        }
        AttachmentRemovalError::UnsafeRoundTrip => {
            "This Guide’s formatting cannot be updated safely. Simplify its Attachment metadata first."
                .to_owned()
        }
        AttachmentRemovalError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        AttachmentRemovalError::UnsafeStorage => {
            "Rehow refused an unsafe Attachment or Workspace path.".to_owned()
        }
        AttachmentRemovalError::UnsupportedFilesystem => {
            "This filesystem cannot provide the atomic durability Rehow requires.".to_owned()
        }
        AttachmentRemovalError::ResourceLimit => {
            "Attachment removal exceeded a safe resource limit.".to_owned()
        }
        AttachmentRemovalError::Io => {
            "The Attachment could not be removed. Check file access and try again.".to_owned()
        }
        AttachmentRemovalError::CommittedRefreshRequired { .. } => {
            "The Attachment was removed, but Rehow could not refresh it. Reopen the Workspace."
                .to_owned()
        }
    }
}

#[tauri::command]
fn remove_open_guide(
    guide_id: String,
    expected_revision: String,
    window: tauri::Window,
    state: tauri::State<'_, DesktopState>,
) -> Result<RemoveGuidePayload, NativeCommandError> {
    if guide_id.len() > 64 || expected_revision.len() > 96 {
        return Err(native_command_error(
            "invalidInput",
            "Guide identity is too long.".to_owned(),
        ));
    }
    let guide_id = GuideId::from_str(&guide_id)
        .map_err(|_| native_command_error("invalidGuide", "Guide ID is invalid.".to_owned()))?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Guide revision is invalid. Reopen the Guide and try again.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let loaded_guide = workspace
        .view
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|guide| guide.guide().id() == &guide_id)
        .ok_or_else(|| {
            native_command_error(
                "notFound",
                "Guide was not found. Reopen the Workspace.".to_owned(),
            )
        })?;
    if loaded_guide.revision() != expected_revision {
        return Err(native_command_error(
            "revisionConflict",
            "This Guide changed on disk. Reopen it before deleting it.".to_owned(),
        ));
    }
    let guide_title = loaded_guide.guide().title().to_owned();
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Delete Guide?")
        .set_description(guide_remove_confirmation(
            &guide_title,
            &guide_id.to_string(),
        ))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        return Ok(RemoveGuidePayload {
            workspace: workspace.payload(),
            removed: false,
        });
    }
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    remove_guide(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        guide_id,
        expected_revision,
    )
    .map_err(|error| native_command_error(guide_remove_code(error), guide_remove_message(error)))?;
    Ok(RemoveGuidePayload {
        workspace: workspace.payload(),
        removed: true,
    })
}

fn guide_remove_code(error: GuideRemoveError) -> &'static str {
    match error {
        GuideRemoveError::NotFound => "notFound",
        GuideRemoveError::HasAttachments => "hasAttachments",
        GuideRemoveError::RevisionConflict => "revisionConflict",
        GuideRemoveError::Busy => "busy",
        GuideRemoveError::UnsafeStorage => "unsafeStorage",
        GuideRemoveError::UnsupportedFilesystem => "unsupportedFilesystem",
        GuideRemoveError::ResourceLimit => "resourceLimit",
        GuideRemoveError::Io => "io",
        GuideRemoveError::CommittedRefreshRequired => "committedRefresh",
    }
}

fn guide_remove_confirmation(title: &str, guide_id: &str) -> String {
    let safe_title = safe_dialog_text(title);
    format!(
        "Delete this Guide?\n\nTitle: {safe_title}\nID: {guide_id}\n\nThe Guide will move to Rehow Trash."
    )
}

fn safe_dialog_text(value: &str) -> String {
    value
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
        .collect()
}

fn guide_remove_message(error: GuideRemoveError) -> String {
    match error {
        GuideRemoveError::NotFound => "Guide was not found. Reopen the Workspace.".to_owned(),
        GuideRemoveError::HasAttachments => {
            "Remove this Guide’s Attachments before deleting the Guide.".to_owned()
        }
        GuideRemoveError::RevisionConflict => {
            "This Guide changed on disk. Reopen it before deleting it.".to_owned()
        }
        GuideRemoveError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        GuideRemoveError::UnsafeStorage => {
            "The Guide path is unsafe or changed type. Rehow deleted nothing.".to_owned()
        }
        GuideRemoveError::UnsupportedFilesystem => {
            "This filesystem cannot provide the durable deletion Rehow requires.".to_owned()
        }
        GuideRemoveError::ResourceLimit => {
            "The Guide exceeds Rehow’s safe deletion limits.".to_owned()
        }
        GuideRemoveError::Io => {
            "The Guide could not be deleted. Check folder access and try again.".to_owned()
        }
        GuideRemoveError::CommittedRefreshRequired => {
            "The Guide moved to Rehow Trash, but the view could not refresh. Reopen the Workspace."
                .to_owned()
        }
    }
}

fn native_recovery_locations() -> Result<(PathBuf, Option<PathBuf>), String> {
    let state_home = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(".local/state"))
        })
        .ok_or_else(|| "Rehow cannot locate a private state folder for recovery.".to_owned())?;
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir());
    Ok((state_home, runtime))
}

fn guide_update_message(error: GuideUpdateError) -> String {
    match error {
        GuideUpdateError::NotFound => "Guide was not found. Refresh the Workspace.".to_owned(),
        GuideUpdateError::DestinationCategoryNotFound => {
            "The destination Category no longer exists. Refresh the Workspace.".to_owned()
        }
        GuideUpdateError::DestinationCategoryArchived => {
            "Choose an active destination Category before moving this Guide.".to_owned()
        }
        GuideUpdateError::RevisionConflict => {
            "This Guide changed on disk. Reopen it before saving.".to_owned()
        }
        GuideUpdateError::InvalidGuide => {
            "This Guide is malformed and cannot be edited safely.".to_owned()
        }
        GuideUpdateError::InvalidEdit => {
            "The requested Guide change is invalid. Check limits and duplicate Tags or aliases."
                .to_owned()
        }
        GuideUpdateError::NotReady => {
            "Add one nonempty “## Steps” section before marking this Guide tested.".to_owned()
        }
        GuideUpdateError::UnsafeRoundTrip => {
            "Rehow cannot preserve this Guide’s formatting safely. Edit the underlying file instead."
                .to_owned()
        }
        GuideUpdateError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        GuideUpdateError::UnsafeStorage => {
            "The Guide path is unsafe or changed type. Reopen the Workspace.".to_owned()
        }
        GuideUpdateError::UnsupportedFilesystem => {
            "This filesystem cannot provide the atomic durability Rehow requires.".to_owned()
        }
        GuideUpdateError::ResourceLimit => {
            "The Guide exceeds Rehow’s safe editing limits.".to_owned()
        }
        GuideUpdateError::Io => {
            "The Guide could not be saved. Check folder access and try again.".to_owned()
        }
        GuideUpdateError::CommittedRefreshRequired { .. } => {
            "The Guide was saved, but Rehow could not refresh it. Reopen the Workspace."
                .to_owned()
        }
    }
}

fn guide_update_code(error: GuideUpdateError) -> &'static str {
    match error {
        GuideUpdateError::NotFound => "notFound",
        GuideUpdateError::DestinationCategoryNotFound => "categoryNotFound",
        GuideUpdateError::DestinationCategoryArchived => "categoryArchived",
        GuideUpdateError::RevisionConflict => "revisionConflict",
        GuideUpdateError::InvalidGuide => "invalidGuide",
        GuideUpdateError::InvalidEdit => "invalidEdit",
        GuideUpdateError::NotReady => "notReady",
        GuideUpdateError::UnsafeRoundTrip => "unsafeRoundTrip",
        GuideUpdateError::Busy => "busy",
        GuideUpdateError::UnsafeStorage => "unsafeStorage",
        GuideUpdateError::UnsupportedFilesystem => "unsupportedFilesystem",
        GuideUpdateError::ResourceLimit => "resourceLimit",
        GuideUpdateError::Io => "io",
        GuideUpdateError::CommittedRefreshRequired { .. } => "committedRefresh",
    }
}

#[tauri::command]
fn initialize_selected_workspace(
    name: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
) -> Result<WorkspacePayload, NativeCommandError> {
    if name.len() > 1_024 {
        return Err(native_command_error(
            "invalidName",
            "Workspace name is too long.".to_owned(),
        ));
    }
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut pending = state.pending_empty.lock().map_err(|_| {
        native_command_error(
            "selectionUnavailable",
            "Workspace selection is unavailable".to_owned(),
        )
    })?;
    let selected = pending.take().ok_or_else(|| {
        native_command_error(
            "selectionUnavailable",
            "Choose an empty folder before creating a Workspace.".to_owned(),
        )
    })?;
    if !selected.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "That folder is read-only. Choose a writable folder.".to_owned(),
        ));
    }
    let view = match initialize_workspace(
        &selected.target,
        &state_home,
        runtime_directory.as_deref(),
        &name,
    ) {
        Ok(view) => view,
        Err(error) => {
            if matches!(
                error,
                WorkspaceInitializationError::InvalidName | WorkspaceInitializationError::Busy
            ) {
                *pending = Some(selected);
            }
            return Err(native_command_error(
                workspace_initialization_code(error),
                workspace_initialization_message(error),
            ));
        }
    };
    let facts = selected.facts;
    let workspace = DesktopWorkspace::new(view, facts, state.next_watch_session(), &app);
    let payload = workspace.payload();
    *state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })? = Some(workspace);
    Ok(payload)
}

fn native_command_error(code: &'static str, message: String) -> NativeCommandError {
    NativeCommandError { code, message }
}

fn workspace_initialization_code(error: WorkspaceInitializationError) -> &'static str {
    match error {
        WorkspaceInitializationError::InvalidName => "invalidName",
        WorkspaceInitializationError::Busy => "busy",
        WorkspaceInitializationError::Conflict => "conflict",
        WorkspaceInitializationError::UnsafeStorage => "unsafeStorage",
        WorkspaceInitializationError::UnsupportedFilesystem => "unsupportedFilesystem",
        WorkspaceInitializationError::ResourceLimit => "resourceLimit",
        WorkspaceInitializationError::Io => "io",
        WorkspaceInitializationError::CommittedRefreshRequired { .. } => "committedRefresh",
    }
}

fn workspace_initialization_message(error: WorkspaceInitializationError) -> String {
    match error {
        WorkspaceInitializationError::InvalidName => {
            "Enter a Workspace name between 1 and 200 characters.".to_owned()
        }
        WorkspaceInitializationError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        WorkspaceInitializationError::Conflict => {
            "That folder changed after selection. Reopen it before creating a Workspace.".to_owned()
        }
        WorkspaceInitializationError::UnsafeStorage => {
            "That folder contains an unsafe entry and was not modified.".to_owned()
        }
        WorkspaceInitializationError::UnsupportedFilesystem => {
            "This filesystem cannot provide the atomic durability Rehow requires.".to_owned()
        }
        WorkspaceInitializationError::ResourceLimit => {
            "Workspace initialization exceeded a safe resource limit.".to_owned()
        }
        WorkspaceInitializationError::Io => {
            "The Workspace could not be created. Check folder access and try again.".to_owned()
        }
        WorkspaceInitializationError::CommittedRefreshRequired { .. } => {
            "The Workspace was created, but Rehow could not open it. Choose the folder again."
                .to_owned()
        }
    }
}

#[tauri::command]
fn create_open_category(
    name: String,
    summary: Option<String>,
    state: tauri::State<'_, DesktopState>,
) -> Result<CreateCategoryPayload, NativeCommandError> {
    if name.len() > 1_024 || summary.as_ref().is_some_and(|value| value.len() > 4_096) {
        return Err(native_command_error(
            "invalidInput",
            "Category details are too long.".to_owned(),
        ));
    }
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = create_category(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        &name,
        summary.as_deref(),
    )
    .map_err(|error| {
        native_command_error(category_create_code(error), category_create_message(error))
    })?;
    Ok(CreateCategoryPayload {
        workspace: workspace.payload(),
        category_id: outcome.category_id().to_string(),
    })
}

fn category_create_code(error: CategoryCreateError) -> &'static str {
    match error {
        CategoryCreateError::InvalidName => "invalidName",
        CategoryCreateError::InvalidSummary => "invalidSummary",
        CategoryCreateError::Busy => "busy",
        CategoryCreateError::Conflict => "conflict",
        CategoryCreateError::UnsafeStorage => "unsafeStorage",
        CategoryCreateError::UnsupportedFilesystem => "unsupportedFilesystem",
        CategoryCreateError::ResourceLimit => "resourceLimit",
        CategoryCreateError::Io => "io",
        CategoryCreateError::Incomplete => "incomplete",
        CategoryCreateError::CommittedRefreshRequired { .. } => "committedRefresh",
    }
}

fn category_create_message(error: CategoryCreateError) -> String {
    match error {
        CategoryCreateError::InvalidName => {
            "Enter a Category name between 1 and 200 characters.".to_owned()
        }
        CategoryCreateError::InvalidSummary => {
            "Keep the optional Category summary below 500 characters.".to_owned()
        }
        CategoryCreateError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        CategoryCreateError::Conflict => {
            "The Category destination changed. Refresh the Workspace and try again.".to_owned()
        }
        CategoryCreateError::UnsafeStorage => {
            "The Category destination is unsafe and was not overwritten.".to_owned()
        }
        CategoryCreateError::UnsupportedFilesystem => {
            "This filesystem cannot provide the atomic durability Rehow requires.".to_owned()
        }
        CategoryCreateError::ResourceLimit => {
            "The Workspace reached a safe Category or content limit.".to_owned()
        }
        CategoryCreateError::Io => {
            "The Category could not be created. Check folder access and try again.".to_owned()
        }
        CategoryCreateError::Incomplete => {
            "Category creation was interrupted. Reopen the Workspace to inspect the empty residue."
                .to_owned()
        }
        CategoryCreateError::CommittedRefreshRequired { .. } => {
            "The Category was created, but Rehow could not refresh it. Reopen the Workspace."
                .to_owned()
        }
    }
}

#[tauri::command]
fn save_open_category_details(
    category_id: String,
    expected_revision: String,
    name: String,
    summary: Option<String>,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveCategoryPayload, NativeCommandError> {
    if category_id.len() > 64
        || expected_revision.len() > 96
        || name.len() > 1_024
        || summary.as_ref().is_some_and(|value| value.len() > 4_096)
    {
        return Err(native_command_error(
            "invalidInput",
            "Category details are too long.".to_owned(),
        ));
    }
    let category_id = CategoryId::from_str(&category_id).map_err(|_| {
        native_command_error("invalidCategory", "Category ID is invalid.".to_owned())
    })?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Category revision is invalid. Reopen the Workspace and try again.".to_owned(),
        )
    })?;
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = update_category_details(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        category_id,
        expected_revision,
        name,
        summary,
    )
    .map_err(|error| {
        native_command_error(category_update_code(error), category_update_message(error))
    })?;
    Ok(SaveCategoryPayload {
        workspace: workspace.payload(),
        changed: outcome.changed(),
    })
}

#[tauri::command]
fn set_open_category_archived(
    category_id: String,
    expected_revision: String,
    archived: bool,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveCategoryPayload, NativeCommandError> {
    if category_id.len() > 64 || expected_revision.len() > 96 {
        return Err(native_command_error(
            "invalidInput",
            "Category identity is too long.".to_owned(),
        ));
    }
    let category_id = CategoryId::from_str(&category_id).map_err(|_| {
        native_command_error("invalidCategory", "Category ID is invalid.".to_owned())
    })?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Category revision is invalid. Reopen the Workspace and try again.".to_owned(),
        )
    })?;
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = set_category_archived(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        category_id,
        expected_revision,
        archived,
    )
    .map_err(|error| {
        native_command_error(category_update_code(error), category_update_message(error))
    })?;
    Ok(SaveCategoryPayload {
        workspace: workspace.payload(),
        changed: outcome.changed(),
    })
}

#[tauri::command]
fn reorder_open_categories(
    ordered: Vec<CategoryOrderInput>,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveCategoryPayload, NativeCommandError> {
    if ordered.len() > 1_000
        || ordered
            .iter()
            .any(|item| item.id.len() > 64 || item.expected_revision.len() > 96)
    {
        return Err(native_command_error(
            "invalidEffectSet",
            "Category order is incomplete or too large. Reopen the Workspace and try again."
                .to_owned(),
        ));
    }
    let mut parsed = Vec::new();
    parsed.try_reserve_exact(ordered.len()).map_err(|_| {
        native_command_error(
            "resourceLimit",
            "The Workspace reached a safe Category limit.".to_owned(),
        )
    })?;
    for item in ordered {
        let category_id = CategoryId::from_str(&item.id).map_err(|_| {
            native_command_error(
                "invalidEffectSet",
                "Category order contains an invalid Category. Reopen the Workspace and try again."
                    .to_owned(),
            )
        })?;
        let revision = Revision::from_str(&item.expected_revision).map_err(|_| {
            native_command_error(
                "invalidEffectSet",
                "Category order contains an invalid revision. Reopen the Workspace and try again."
                    .to_owned(),
            )
        })?;
        parsed.push(CategoryOrderRevision::new(category_id, revision));
    }
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = reorder_categories(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        &parsed,
    )
    .map_err(|error| {
        native_command_error(
            category_reorder_code(error),
            category_reorder_message(error),
        )
    })?;
    Ok(SaveCategoryPayload {
        workspace: workspace.payload(),
        changed: outcome.changed(),
    })
}

fn category_reorder_code(error: CategoryReorderError) -> &'static str {
    match error {
        CategoryReorderError::InvalidEffectSet => "invalidEffectSet",
        CategoryReorderError::RevisionConflict => "revisionConflict",
        CategoryReorderError::InvalidCategory => "invalidCategory",
        CategoryReorderError::UnsafeRoundTrip => "unsafeRoundTrip",
        CategoryReorderError::Busy => "busy",
        CategoryReorderError::UnsafeStorage => "unsafeStorage",
        CategoryReorderError::UnsupportedFilesystem => "unsupportedFilesystem",
        CategoryReorderError::ResourceLimit => "resourceLimit",
        CategoryReorderError::Io => "io",
        CategoryReorderError::CommittedRefreshRequired => "committedRefresh",
    }
}

fn category_reorder_message(error: CategoryReorderError) -> String {
    match error {
        CategoryReorderError::InvalidEffectSet => {
            "Category order is incomplete. Reopen the Workspace and try again.".to_owned()
        }
        CategoryReorderError::RevisionConflict => {
            "A Category changed on disk. Reopen the Workspace before reordering.".to_owned()
        }
        CategoryReorderError::InvalidCategory => {
            "A Category is malformed and cannot be reordered safely.".to_owned()
        }
        CategoryReorderError::UnsafeRoundTrip => {
            "Rehow cannot preserve a Category’s formatting safely. Edit the underlying files instead."
                .to_owned()
        }
        CategoryReorderError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        CategoryReorderError::UnsafeStorage => {
            "A Category path is unsafe or changed type. Reopen the Workspace.".to_owned()
        }
        CategoryReorderError::UnsupportedFilesystem => {
            "This filesystem cannot provide the atomic durability Rehow requires.".to_owned()
        }
        CategoryReorderError::ResourceLimit => {
            "The Workspace reached a safe Category limit.".to_owned()
        }
        CategoryReorderError::Io => {
            "Categories could not be reordered. Check folder access and try again.".to_owned()
        }
        CategoryReorderError::CommittedRefreshRequired => {
            "Category order was saved, but Rehow could not refresh it. Reopen the Workspace."
                .to_owned()
        }
    }
}

#[tauri::command]
fn remove_open_category(
    category_id: String,
    expected_revision: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<SaveCategoryPayload, NativeCommandError> {
    if category_id.len() > 64 || expected_revision.len() > 96 {
        return Err(native_command_error(
            "invalidInput",
            "Category identity is too long.".to_owned(),
        ));
    }
    let category_id = CategoryId::from_str(&category_id).map_err(|_| {
        native_command_error("invalidCategory", "Category ID is invalid.".to_owned())
    })?;
    let expected_revision = Revision::from_str(&expected_revision).map_err(|_| {
        native_command_error(
            "invalidRevision",
            "Category revision is invalid. Reopen the Workspace and try again.".to_owned(),
        )
    })?;
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    remove_category(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        category_id,
        expected_revision,
    )
    .map_err(|error| {
        native_command_error(category_remove_code(error), category_remove_message(error))
    })?;
    Ok(SaveCategoryPayload {
        workspace: workspace.payload(),
        changed: true,
    })
}

fn category_remove_code(error: CategoryRemoveError) -> &'static str {
    match error {
        CategoryRemoveError::NotFound => "notFound",
        CategoryRemoveError::DefaultCategory => "defaultCategory",
        CategoryRemoveError::NotEmpty => "notEmpty",
        CategoryRemoveError::RevisionConflict => "revisionConflict",
        CategoryRemoveError::Busy => "busy",
        CategoryRemoveError::UnsafeStorage => "unsafeStorage",
        CategoryRemoveError::UnsupportedFilesystem => "unsupportedFilesystem",
        CategoryRemoveError::ResourceLimit => "resourceLimit",
        CategoryRemoveError::Io => "io",
        CategoryRemoveError::CommittedRefreshRequired => "committedRefresh",
    }
}

fn category_remove_message(error: CategoryRemoveError) -> String {
    match error {
        CategoryRemoveError::NotFound => {
            "Category was not found. Reopen the Workspace.".to_owned()
        }
        CategoryRemoveError::DefaultCategory => {
            "Choose a different default Category before removing this one.".to_owned()
        }
        CategoryRemoveError::NotEmpty => {
            "This Category is not empty. Move its Guides or inspect unexpected files before removing it."
                .to_owned()
        }
        CategoryRemoveError::RevisionConflict => {
            "This Category changed on disk. Reopen the Workspace before removing it.".to_owned()
        }
        CategoryRemoveError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        CategoryRemoveError::UnsafeStorage => {
            "The Category folder is unsafe or contains unexpected entries. Rehow removed nothing."
                .to_owned()
        }
        CategoryRemoveError::UnsupportedFilesystem => {
            "This filesystem cannot provide the durable removal Rehow requires.".to_owned()
        }
        CategoryRemoveError::ResourceLimit => {
            "The Category exceeds Rehow’s safe removal limits.".to_owned()
        }
        CategoryRemoveError::Io => {
            "The Category could not be removed. Check folder access and try again.".to_owned()
        }
        CategoryRemoveError::CommittedRefreshRequired => {
            "The Category moved to Rehow Trash, but the view could not refresh. Reopen the Workspace."
                .to_owned()
        }
    }
}

fn category_update_code(error: CategoryUpdateError) -> &'static str {
    match error {
        CategoryUpdateError::NotFound => "notFound",
        CategoryUpdateError::DefaultCategory => "defaultCategory",
        CategoryUpdateError::RevisionConflict => "revisionConflict",
        CategoryUpdateError::InvalidCategory => "invalidCategory",
        CategoryUpdateError::InvalidName => "invalidName",
        CategoryUpdateError::InvalidSummary => "invalidSummary",
        CategoryUpdateError::UnsafeRoundTrip => "unsafeRoundTrip",
        CategoryUpdateError::Busy => "busy",
        CategoryUpdateError::UnsafeStorage => "unsafeStorage",
        CategoryUpdateError::UnsupportedFilesystem => "unsupportedFilesystem",
        CategoryUpdateError::ResourceLimit => "resourceLimit",
        CategoryUpdateError::Io => "io",
        CategoryUpdateError::CommittedRefreshRequired { .. } => "committedRefresh",
    }
}

fn category_update_message(error: CategoryUpdateError) -> String {
    match error {
        CategoryUpdateError::NotFound => {
            "Category was not found. Refresh the Workspace.".to_owned()
        }
        CategoryUpdateError::DefaultCategory => {
            "Choose a different default Category before archiving this one.".to_owned()
        }
        CategoryUpdateError::RevisionConflict => {
            "This Category changed on disk. Reopen the Workspace before saving.".to_owned()
        }
        CategoryUpdateError::InvalidCategory => {
            "This Category is malformed and cannot be edited safely.".to_owned()
        }
        CategoryUpdateError::InvalidName => {
            "Enter a Category name between 1 and 200 characters.".to_owned()
        }
        CategoryUpdateError::InvalidSummary => {
            "Enter a Category summary of at most 500 characters.".to_owned()
        }
        CategoryUpdateError::UnsafeRoundTrip => {
            "Rehow cannot preserve this Category’s formatting safely. Edit the underlying file instead."
                .to_owned()
        }
        CategoryUpdateError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        CategoryUpdateError::UnsafeStorage => {
            "The Category path is unsafe or changed type. Reopen the Workspace.".to_owned()
        }
        CategoryUpdateError::UnsupportedFilesystem => {
            "This filesystem cannot provide the atomic durability Rehow requires.".to_owned()
        }
        CategoryUpdateError::ResourceLimit => {
            "The Category exceeds Rehow’s safe editing limits.".to_owned()
        }
        CategoryUpdateError::Io => {
            "The Category could not be saved. Check folder access and try again.".to_owned()
        }
        CategoryUpdateError::CommittedRefreshRequired { .. } => {
            "The Category was saved, but Rehow could not refresh it. Reopen the Workspace."
                .to_owned()
        }
    }
}

#[tauri::command]
fn create_open_guide(
    category_id: String,
    title: String,
    summary: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<CreateGuidePayload, NativeCommandError> {
    if category_id.len() > 64 || title.len() > 1_024 || summary.len() > 4_096 {
        return Err(native_command_error(
            "invalidInput",
            "Guide details are too long.".to_owned(),
        ));
    }
    let category_id = CategoryId::from_str(&category_id).map_err(|_| {
        native_command_error(
            "invalidCategory",
            "Choose an active Category for this Guide.".to_owned(),
        )
    })?;
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let mut guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_mut().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let outcome = create_guide(
        &mut workspace.view,
        &state_home,
        runtime_directory.as_deref(),
        category_id,
        &title,
        &summary,
    )
    .map_err(|error| native_command_error(guide_create_code(error), guide_create_message(error)))?;
    let guide_id = outcome.guide_id();
    Ok(CreateGuidePayload {
        workspace: workspace.payload(),
        guide: guide_body_payload(&workspace.view, &guide_id)
            .map_err(|message| native_command_error("refreshRequired", message))?,
    })
}

fn guide_create_code(error: GuideCreateError) -> &'static str {
    match error {
        GuideCreateError::CategoryNotFound => "categoryNotFound",
        GuideCreateError::CategoryArchived => "categoryArchived",
        GuideCreateError::InvalidTitle => "invalidTitle",
        GuideCreateError::InvalidSummary => "invalidSummary",
        GuideCreateError::Busy => "busy",
        GuideCreateError::Conflict => "conflict",
        GuideCreateError::UnsafeStorage => "unsafeStorage",
        GuideCreateError::UnsupportedFilesystem => "unsupportedFilesystem",
        GuideCreateError::ResourceLimit => "resourceLimit",
        GuideCreateError::Io => "io",
        GuideCreateError::Incomplete => "incomplete",
        GuideCreateError::CommittedRefreshRequired { .. } => "committedRefresh",
    }
}

fn guide_create_message(error: GuideCreateError) -> String {
    match error {
        GuideCreateError::CategoryNotFound => {
            "That Category no longer exists. Refresh the Workspace and choose another.".to_owned()
        }
        GuideCreateError::CategoryArchived => {
            "Choose an active Category before creating the Guide.".to_owned()
        }
        GuideCreateError::InvalidTitle => {
            "Enter a Guide title between 1 and 200 characters.".to_owned()
        }
        GuideCreateError::InvalidSummary => {
            "Enter a Guide summary between 1 and 500 characters.".to_owned()
        }
        GuideCreateError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        GuideCreateError::Conflict => {
            "The Guide destination changed. Refresh the Workspace and try again.".to_owned()
        }
        GuideCreateError::UnsafeStorage => {
            "The Guide destination is unsafe and was not overwritten.".to_owned()
        }
        GuideCreateError::UnsupportedFilesystem => {
            "This filesystem cannot provide the atomic durability Rehow requires.".to_owned()
        }
        GuideCreateError::ResourceLimit => {
            "The Workspace reached a safe Guide or content limit.".to_owned()
        }
        GuideCreateError::Io => {
            "The Guide could not be created. Check folder access and try again.".to_owned()
        }
        GuideCreateError::Incomplete => {
            "Guide creation was interrupted. Reopen the Workspace before trying again.".to_owned()
        }
        GuideCreateError::CommittedRefreshRequired { .. } => {
            "The Guide was created, but Rehow could not refresh it. Reopen the Workspace."
                .to_owned()
        }
    }
}

#[tauri::command]
fn cancel_selected_workspace_initialization(
    state: tauri::State<'_, DesktopState>,
) -> Result<(), String> {
    *state
        .pending_empty
        .lock()
        .map_err(|_| "Workspace selection is unavailable".to_owned())? = None;
    Ok(())
}

#[tauri::command]
fn choose_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
) -> Result<OpenWorkspaceResult, String> {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Open a Rehow Workspace")
        .pick_folder()
    else {
        return Ok(OpenWorkspaceResult::Cancelled);
    };

    let (report_facts, inspection) = inspect_workspace(&path)
        .map_err(|error| error.to_string())?
        .into_parts();
    let facts = FolderFactsPayload::from(report_facts);
    *state
        .pending_empty
        .lock()
        .map_err(|_| "Workspace selection is unavailable".to_owned())? = None;
    match inspection {
        WorkspaceInspection::Empty(target) => {
            *state
                .workspace
                .lock()
                .map_err(|_| "Workspace session is unavailable".to_owned())? = None;
            *state
                .pending_empty
                .lock()
                .map_err(|_| "Workspace selection is unavailable".to_owned())? =
                Some(PendingWorkspace {
                    target,
                    facts: report_facts,
                });
            Ok(OpenWorkspaceResult::Empty { facts })
        }
        WorkspaceInspection::Compatible(view) => {
            let workspace =
                DesktopWorkspace::new(view, report_facts, state.next_watch_session(), &app);
            let payload = workspace.payload();
            *state
                .workspace
                .lock()
                .map_err(|_| "Workspace session is unavailable".to_owned())? = Some(workspace);
            Ok(OpenWorkspaceResult::Compatible { workspace: payload })
        }
        WorkspaceInspection::NewerUnsupported { version } => {
            Ok(OpenWorkspaceResult::NewerUnsupported { version, facts })
        }
        WorkspaceInspection::OrdinaryMarkdown => {
            Ok(OpenWorkspaceResult::OrdinaryMarkdown { facts })
        }
        WorkspaceInspection::UnrelatedNonempty => {
            Ok(OpenWorkspaceResult::UnrelatedNonempty { facts })
        }
        WorkspaceInspection::Malformed { reason } => Ok(OpenWorkspaceResult::Malformed {
            reason: match reason {
                rehow_core::storage::MalformedWorkspaceReason::Manifest => "manifest",
                rehow_core::storage::MalformedWorkspaceReason::CategoriesMissing => {
                    "categoriesMissing"
                }
                rehow_core::storage::MalformedWorkspaceReason::CategoriesUnsafe => {
                    "categoriesUnsafe"
                }
            },
            facts,
        }),
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            approve_open_guide_tested,
            cancel_selected_workspace_initialization,
            choose_workspace,
            create_open_category,
            create_open_guide,
            initialize_selected_workspace,
            import_open_guide_attachment,
            read_open_guide,
            refresh_open_workspace,
            remove_open_category,
            remove_open_guide_attachment,
            remove_open_guide,
            reorder_open_categories,
            save_open_category_details,
            save_open_guide_body,
            save_open_guide_details,
            set_open_category_archived,
            set_open_guide_archived,
            git_status_open_workspace,
            search_open_workspace
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rehow");
}

#[cfg(test)]
mod tests {
    use super::{
        APPLICATION_NAME, CategoryPayload, FolderFactsPayload, GuideBodyPayload, GuidePayload,
        SearchHighlightPayload, SearchHitPayload, WorkspacePayload, WorkspaceWatchEventPayload,
        attachment_removal_code, attachment_removal_message, category_create_code,
        category_create_message, category_remove_code, category_remove_message,
        category_reorder_code, category_reorder_message, category_update_code,
        category_update_message, empty_git_payload, guide_create_code, guide_create_message,
        guide_remove_code, guide_remove_confirmation, guide_remove_message, guide_update_code,
        guide_update_message, native_command_error, search_status_scope,
        workspace_initialization_message, workspace_refresh_code, workspace_refresh_message,
    };
    use rehow_core::{
        application::{
            AttachmentRemovalError, CategoryCreateError, CategoryRemoveError, CategoryReorderError,
            CategoryUpdateError, GuideCreateError, GuideRemoveError, GuideUpdateError,
            WorkspaceInitializationError, WorkspaceRefreshError,
        },
        markdown::parse_safe_markdown,
    };

    #[test]
    fn application_name_is_stable() {
        assert_eq!(APPLICATION_NAME, "Rehow");
    }

    #[test]
    fn git_status_payload_is_closed_and_content_free() {
        let value =
            serde_json::to_value(empty_git_payload("unsafe", Some("2.55.0".to_owned()))).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 14);
        assert_eq!(object["kind"], "unsafe");
        assert_eq!(object["divergenceKnown"], false);
        assert_eq!(object["remotes"], serde_json::json!([]));
        assert!(!value.to_string().contains("path"));
    }

    #[test]
    fn search_status_scopes_are_closed() {
        for scope in ["active", "tested", "draft", "stale", "archived", "all"] {
            assert!(search_status_scope(scope).is_some(), "{scope}");
        }
        assert!(search_status_scope("everything").is_none());
    }

    #[test]
    fn payload_field_names_are_stable() {
        let payload = WorkspacePayload {
            id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e01".to_owned(),
            name: "Test".to_owned(),
            categories: vec![CategoryPayload {
                id: "0199a1c2-d334-7d17-bd9c-f8b6ba3a5551".to_owned(),
                name: "Linux".to_owned(),
                summary: Some("System work".to_owned()),
                status: "active",
                revision: format!("rev:v1:{}", "11".repeat(32)),
                guide_count: 1,
            }],
            guides: vec![GuidePayload {
                id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".to_owned(),
                title: "Test".to_owned(),
                summary: "Summary".to_owned(),
                category_id: "0199a1c2-d334-7d17-bd9c-f8b6ba3a5551".to_owned(),
                category: "Linux".to_owned(),
                status: "Draft",
                updated_at: "2026-09-18T00:00:00Z".to_owned(),
                tags: vec!["tag".to_owned()],
                aliases: vec!["alias".to_owned()],
                attachments: vec![],
                matches: "alias tag".to_owned(),
            }],
            issue_count: 0,
            facts: FolderFactsPayload {
                writable: true,
                existing_git_repository: false,
                nested_git_repository: false,
                git_probe_complete: true,
                unsafe_git_marker: false,
            },
            watch_status: "active",
            watch_session: "7".to_owned(),
        };
        let json = serde_json::to_value(payload).unwrap();
        assert_eq!(json["id"], "0199a1d7-7b0d-7d8e-a38d-f936c5735e01");
        assert_eq!(json["name"], "Test");
        assert_eq!(json["issueCount"], 0);
        assert_eq!(json["facts"]["writable"], true);
        assert_eq!(json["categories"][0]["status"], "active");
        assert_eq!(json["categories"][0]["summary"], "System work");
        assert_eq!(
            json["categories"][0]["revision"],
            format!("rev:v1:{}", "11".repeat(32))
        );
        assert_eq!(json["guides"][0]["tags"][0], "tag");
        assert_eq!(json["guides"][0]["aliases"][0], "alias");
        assert_eq!(
            json["guides"][0]["categoryId"],
            "0199a1c2-d334-7d17-bd9c-f8b6ba3a5551"
        );
        assert!(json["guides"][0].get("body").is_none());
        assert_eq!(json["watchStatus"], "active");
        assert_eq!(json["watchSession"], "7");
    }

    #[test]
    fn watch_event_contains_only_workspace_identity_and_health() {
        let json = serde_json::to_value(WorkspaceWatchEventPayload {
            workspace_id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e01".to_owned(),
            watch_session: "7".to_owned(),
            kind: "changed",
        })
        .unwrap();
        assert_eq!(json.as_object().unwrap().len(), 3);
        assert_eq!(json["kind"], "changed");
        assert!(json.get("path").is_none());
    }

    #[test]
    fn search_payload_is_plain_bounded_structure() {
        let json = serde_json::to_value(SearchHitPayload {
            guide_id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".to_owned(),
            excerpt: "error: permission denied".to_owned(),
            excerpt_field: "error",
            matched_fields: vec!["error", "body"],
            highlights: vec![SearchHighlightPayload { start: 7, end: 24 }],
        })
        .unwrap();
        assert_eq!(json.as_object().unwrap().len(), 5);
        assert_eq!(json["excerptField"], "error");
        assert_eq!(json["highlights"][0]["start"], 7);
        assert!(json.get("html").is_none());
        assert!(json.get("path").is_none());
    }

    #[test]
    fn guide_read_payload_has_revision_source_and_safe_document() {
        let payload = GuideBodyPayload {
            id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".to_owned(),
            revision: format!("rev:v1:{}", "11".repeat(32)),
            body: "<script>no</script>".to_owned(),
            document: parse_safe_markdown("<script>no</script>").unwrap(),
        };
        let json = serde_json::to_value(payload).unwrap();
        assert_eq!(json["body"], "<script>no</script>");
        assert_eq!(json["revision"], format!("rev:v1:{}", "11".repeat(32)));
        assert_eq!(json["document"]["version"], 1);
        assert!(
            json["document"]["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "htmlLiteral")
        );
    }

    #[test]
    fn save_errors_are_actionable_and_content_free() {
        assert_eq!(
            guide_update_code(GuideUpdateError::RevisionConflict),
            "revisionConflict"
        );
        assert_eq!(
            guide_update_message(GuideUpdateError::RevisionConflict),
            "This Guide changed on disk. Reopen it before saving."
        );
        assert_eq!(
            guide_update_message(GuideUpdateError::UnsafeStorage),
            "The Guide path is unsafe or changed type. Reopen the Workspace."
        );
        assert_eq!(
            guide_update_code(GuideUpdateError::DestinationCategoryNotFound),
            "categoryNotFound"
        );
        assert_eq!(
            guide_update_message(GuideUpdateError::DestinationCategoryArchived),
            "Choose an active destination Category before moving this Guide."
        );
        assert_eq!(guide_update_code(GuideUpdateError::NotReady), "notReady");
        assert_eq!(
            guide_update_message(GuideUpdateError::NotReady),
            "Add one nonempty “## Steps” section before marking this Guide tested."
        );
        assert_eq!(
            guide_remove_code(GuideRemoveError::HasAttachments),
            "hasAttachments"
        );
        assert_eq!(
            guide_remove_message(GuideRemoveError::RevisionConflict),
            "This Guide changed on disk. Reopen it before deleting it."
        );
        assert_eq!(
            guide_remove_confirmation(
                "bad\nline\u{202e}name “quoted”",
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e10"
            ),
            "Delete this Guide?\n\nTitle: bad�line�name “quoted”\nID: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\n\nThe Guide will move to Rehow Trash."
        );
    }

    #[test]
    fn initialization_errors_explain_retry_and_committed_outcomes() {
        assert_eq!(
            workspace_initialization_message(WorkspaceInitializationError::Conflict),
            "That folder changed after selection. Reopen it before creating a Workspace."
        );
        assert_eq!(
            workspace_initialization_message(
                WorkspaceInitializationError::CommittedRefreshRequired {
                    revision: format!("rev:v1:{}", "11".repeat(32)).parse().unwrap(),
                }
            ),
            "The Workspace was created, but Rehow could not open it. Choose the folder again."
        );
        assert_eq!(
            serde_json::to_value(native_command_error(
                "conflict",
                "Folder changed.".to_owned()
            ))
            .unwrap(),
            serde_json::json!({ "code": "conflict", "message": "Folder changed." })
        );
    }

    #[test]
    fn workspace_refresh_errors_are_stable_and_actionable() {
        assert_eq!(
            workspace_refresh_code(WorkspaceRefreshError::ChangedDuringRead),
            "changedDuringRead"
        );
        assert_eq!(
            workspace_refresh_message(WorkspaceRefreshError::UnsafeStorage),
            "The Workspace path is unsafe or changed type. Rehow kept the previous view."
        );
        assert_eq!(
            workspace_refresh_message(WorkspaceRefreshError::NotFound),
            "The Workspace folder is no longer available. Reopen it when it returns."
        );
    }

    #[test]
    fn category_create_errors_are_stable_and_actionable() {
        assert_eq!(
            category_create_code(CategoryCreateError::Conflict),
            "conflict"
        );
        assert_eq!(
            category_create_message(CategoryCreateError::Incomplete),
            "Category creation was interrupted. Reopen the Workspace to inspect the empty residue."
        );
    }

    #[test]
    fn category_update_errors_are_stable_and_actionable() {
        assert_eq!(
            category_update_code(CategoryUpdateError::RevisionConflict),
            "revisionConflict"
        );
        assert_eq!(
            category_update_message(CategoryUpdateError::InvalidName),
            "Enter a Category name between 1 and 200 characters."
        );
        assert_eq!(
            category_update_code(CategoryUpdateError::InvalidSummary),
            "invalidSummary"
        );
        assert_eq!(
            category_update_code(CategoryUpdateError::DefaultCategory),
            "defaultCategory"
        );
        assert_eq!(
            category_update_message(CategoryUpdateError::DefaultCategory),
            "Choose a different default Category before archiving this one."
        );
    }

    #[test]
    fn category_reorder_errors_are_stable_and_actionable() {
        assert_eq!(
            category_reorder_code(CategoryReorderError::InvalidEffectSet),
            "invalidEffectSet"
        );
        assert_eq!(
            category_reorder_message(CategoryReorderError::RevisionConflict),
            "A Category changed on disk. Reopen the Workspace before reordering."
        );
    }

    #[test]
    fn category_remove_errors_are_stable_and_actionable() {
        assert_eq!(
            category_remove_code(CategoryRemoveError::NotEmpty),
            "notEmpty"
        );
        assert_eq!(
            category_remove_message(CategoryRemoveError::DefaultCategory),
            "Choose a different default Category before removing this one."
        );
    }

    #[test]
    fn guide_create_errors_are_stable_and_keep_creation_draft_only() {
        assert_eq!(
            guide_create_code(GuideCreateError::CategoryArchived),
            "categoryArchived"
        );
        assert_eq!(
            guide_create_message(GuideCreateError::InvalidSummary),
            "Enter a Guide summary between 1 and 500 characters."
        );
        assert_eq!(
            guide_create_message(GuideCreateError::Incomplete),
            "Guide creation was interrupted. Reopen the Workspace before trying again."
        );
    }

    #[test]
    fn attachment_removal_errors_are_stable_and_actionable() {
        assert_eq!(
            attachment_removal_code(AttachmentRemovalError::AttachmentChanged),
            "attachmentChanged"
        );
        assert_eq!(
            attachment_removal_code(AttachmentRemovalError::AttachmentNotFound),
            "attachmentNotFound"
        );
        assert_eq!(
            attachment_removal_message(AttachmentRemovalError::GuideArchived),
            "Restore this Guide before removing an Attachment."
        );
        assert!(
            attachment_removal_message(AttachmentRemovalError::UnsafeStorage).contains("unsafe")
        );
    }
}

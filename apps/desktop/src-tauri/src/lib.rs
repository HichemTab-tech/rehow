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
    git::{
        GitCloneApplyError, GitCloneError, GitCloneMaterializeError, GitClonePlan,
        GitCloneValidation, GitCommitPlan, GitCommitPlanError, GitHttpsCredentialResourceError,
        GitHttpsCredentialResources, GitHttpsCredentialStatus, GitInitError, GitInitPlan,
        GitReadError, GitRemoteAuthentication, GitRemoteError, GitRemotePlan, GitRemoteTransport,
        GitRepositoryStatus, GitSshTransportError, GitSshTransportResources, GitSyncAncestry,
        GitSyncClassification, GitSyncError, apply_git_clone_repository_finalization,
        apply_git_clone_validation_with_authentication, apply_git_commit, apply_git_initialize,
        apply_git_push_with_authentication, apply_git_remote_connect, apply_git_sync_fast_forward,
        apply_git_sync_inspection_with_authentication, cancel_git_commit_plan,
        capture_git_https_credential_backing_provenance, capture_git_https_credential_provenance,
        capture_git_https_credential_proxy_provenance, capture_git_ssh_agent_provenance,
        capture_git_ssh_host_keys_provenance, capture_git_ssh_proxy_provenance,
        classify_git_sync_ancestry_with_authentication, git_commit_recovery_required,
        git_sync_recovery_required, plan_git_clone, plan_git_commit, plan_git_initialize,
        plan_git_remote_connect, plan_git_sync, read_git_status,
        resume_pending_git_commit_publication, resume_pending_git_sync_publication,
    },
    markdown::{SafeMarkdownDocument, parse_safe_markdown},
    protocol::{AttachmentId, CategoryId, GuideId, PlanToken, Revision},
    search::{SearchHit, SearchStatusScope, search_workspace_in_scope},
    storage::{
        EmptyWorkspaceTarget, MarkdownCollectionSource, MarkdownImportPreview,
        WorkspaceFolderFacts, WorkspaceInspection, WorkspaceView, WorkspaceWatchSignal,
        WorkspaceWatcher, inspect_workspace,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::PathBuf,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tauri::{Emitter, Manager};

const WORKSPACE_INVALIDATED_EVENT: &str = "rehow://workspace-invalidated/v1";
const GIT_COMMIT_MESSAGE_INPUT_BYTES: usize = 800;

struct DesktopState {
    https_credential_backing_provenance: rehow_core::git::GitHttpsCredentialBackingProvenance,
    https_credential_provenance: rehow_core::git::GitHttpsCredentialProvenance,
    https_credential_proxy_provenance: rehow_core::git::GitHttpsCredentialProxyProvenance,
    ssh_agent_provenance: rehow_core::git::GitSshAgentProvenance,
    ssh_host_keys_provenance: rehow_core::git::GitSshHostKeysProvenance,
    ssh_proxy_provenance: rehow_core::git::GitSshProxyProvenance,
    workspace: Mutex<Option<DesktopWorkspace>>,
    pending_empty: Mutex<Option<PendingWorkspace>>,
    pending_markdown_import: Mutex<Option<MarkdownCollectionSource>>,
    pending_git_init: Mutex<Option<(PlanToken, GitInitPlan)>>,
    pending_git_remote: Mutex<Option<(PlanToken, GitRemotePlan)>>,
    git_commit: Mutex<GitCommitLifecycle>,
    git_clone: Mutex<GitCloneLifecycle>,
    git_sync: Mutex<GitSyncLifecycle>,
    next_watch_session: AtomicU64,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            https_credential_backing_provenance: capture_git_https_credential_backing_provenance(),
            https_credential_provenance: capture_git_https_credential_provenance(),
            https_credential_proxy_provenance: capture_git_https_credential_proxy_provenance(),
            ssh_agent_provenance: capture_git_ssh_agent_provenance(),
            ssh_host_keys_provenance: capture_git_ssh_host_keys_provenance(),
            ssh_proxy_provenance: capture_git_ssh_proxy_provenance(),
            workspace: Mutex::default(),
            pending_empty: Mutex::default(),
            pending_markdown_import: Mutex::default(),
            pending_git_init: Mutex::default(),
            pending_git_remote: Mutex::default(),
            git_commit: Mutex::default(),
            git_clone: Mutex::default(),
            git_sync: Mutex::default(),
            next_watch_session: AtomicU64::default(),
        }
    }
}

enum PreparedGitRemoteAuthentication {
    Anonymous,
    Https(GitHttpsCredentialResources),
    Ssh(GitSshTransportResources),
}

impl PreparedGitRemoteAuthentication {
    fn as_borrowed(&self) -> GitRemoteAuthentication<'_> {
        match self {
            Self::Anonymous => GitRemoteAuthentication::Anonymous,
            Self::Https(resources) => GitRemoteAuthentication::Https(resources),
            Self::Ssh(resources) => GitRemoteAuthentication::Ssh(resources),
        }
    }
}

fn prepare_git_remote_authentication(
    state: &DesktopState,
    transport: GitRemoteTransport,
) -> Result<PreparedGitRemoteAuthentication, NativeCommandError> {
    match transport {
        GitRemoteTransport::Ssh => GitSshTransportResources::prepare(
            &state.ssh_agent_provenance,
            &state.ssh_host_keys_provenance,
            &state.ssh_proxy_provenance,
        )
        .map(PreparedGitRemoteAuthentication::Ssh)
        .map_err(|error| {
            native_command_error(
                git_ssh_transport_code(error),
                git_ssh_transport_message(error),
            )
        }),
        GitRemoteTransport::Https
            if state.https_credential_provenance.status() == GitHttpsCredentialStatus::Trusted =>
        {
            GitHttpsCredentialResources::prepare(
                &state.https_credential_provenance,
                &state.https_credential_proxy_provenance,
                &state.https_credential_backing_provenance,
            )
            .map(PreparedGitRemoteAuthentication::Https)
            .map_err(|error| {
                native_command_error(
                    git_https_credential_code(error),
                    git_https_credential_message(error),
                )
            })
        }
        GitRemoteTransport::Https => Ok(PreparedGitRemoteAuthentication::Anonymous),
    }
}

#[derive(Default)]
struct GitCloneLifecycle {
    pending: Option<(PlanToken, GitClonePlan)>,
    active: Option<Arc<AtomicBool>>,
    validated: Option<PendingValidatedClone>,
    revoke_on_completion: bool,
    finalizing: bool,
}

#[derive(Default)]
struct GitCommitLifecycle {
    pending: Option<(PlanToken, GitCommitPlan)>,
    active: bool,
}

#[derive(Default)]
struct GitSyncLifecycle {
    active: Option<Arc<AtomicBool>>,
    classified: Option<(PlanToken, GitSyncClassification)>,
    applying: bool,
    revoke_on_completion: bool,
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

struct PendingValidatedClone {
    selected: PendingWorkspace,
    validation: GitCloneValidation,
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
        preview: MarkdownImportPreviewPayload,
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
struct MarkdownImportPreviewPayload {
    included: usize,
    normalization_required: usize,
    excluded_non_markdown: usize,
    malformed_or_unsafe: usize,
    oversized: usize,
    aggregate_bytes: String,
    entries: Vec<MarkdownImportPreviewEntryPayload>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MarkdownImportPreviewEntryPayload {
    display_path: String,
    size: String,
    normalization_required: bool,
}

impl From<MarkdownImportPreview> for MarkdownImportPreviewPayload {
    fn from(preview: MarkdownImportPreview) -> Self {
        Self {
            included: preview.included(),
            normalization_required: preview.normalization_required(),
            excluded_non_markdown: preview.excluded_non_markdown(),
            malformed_or_unsafe: preview.malformed_or_unsafe(),
            oversized: preview.oversized(),
            aggregate_bytes: preview.aggregate_bytes().to_string(),
            entries: preview
                .entries()
                .iter()
                .map(|entry| MarkdownImportPreviewEntryPayload {
                    display_path: entry.display_path().to_owned(),
                    size: entry.size().to_string(),
                    normalization_required: entry.normalization_required(),
                })
                .collect(),
        }
    }
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
struct GitInitPlanPayload {
    plan_token: PlanToken,
    initial_branch: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitInitResultPayload {
    initialized: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitRemotePlanPayload {
    plan_token: PlanToken,
    remote_name: String,
    transport: &'static str,
    host: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitRemoteResultPayload {
    connected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitCommitPlanPayload {
    plan_token: PlanToken,
    branch: String,
    created: u32,
    updated: u32,
    deleted: u32,
    unrelated_changes: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitCommitResultPayload {
    committed: bool,
    workspace_changed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitCommitCancelResultPayload {
    cancelled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitCommitRecoveryResultPayload {
    recovered: bool,
    workspace_changed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitSyncInspectionPayload {
    plan_token: PlanToken,
    remote_name: String,
    transport: &'static str,
    host: String,
    ancestry: &'static str,
    local_changes: bool,
    workspace_matches_remote: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitSyncCancelResultPayload {
    cancellation_requested: bool,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum GitSyncApplyResultPayload {
    Declined,
    Completed {
        workspace: WorkspacePayload,
        changed: bool,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum GitPushResultPayload {
    Declined,
    Completed { pushed: bool },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum GitSyncRecoveryResultPayload {
    Declined,
    Completed {
        workspace: WorkspacePayload,
        changed: bool,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitClonePlanPayload {
    plan_token: PlanToken,
    transport: &'static str,
    host: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitCloneApplyResultPayload {
    validated: bool,
    stage: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitCloneCancelResultPayload {
    cancellation_requested: bool,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum GitCloneCompleteResultPayload {
    Declined,
    Completed { workspace: WorkspacePayload },
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
    if let Ok((state_home, runtime_directory)) = native_recovery_locations() {
        let commit_recovery = matches!(
            git_commit_recovery_required(
                &workspace.view,
                &state_home,
                runtime_directory.as_deref(),
            ),
            Ok(true)
        );
        let sync_recovery = matches!(
            git_sync_recovery_required(&workspace.view, &state_home, runtime_directory.as_deref(),),
            Ok(true)
        );
        if commit_recovery || sync_recovery {
            return Ok(empty_git_payload("recoveryRequired", None));
        }
    }
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

#[tauri::command]
fn git_initialize_plan_open_workspace(
    state: tauri::State<'_, DesktopState>,
) -> Result<GitInitPlanPayload, NativeCommandError> {
    let guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_ref().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let plan = plan_git_initialize(&workspace.view)
        .map_err(|error| native_command_error(git_init_code(error), git_init_message(error)))?;
    let payload = GitInitPlanPayload {
        plan_token: plan.token().clone(),
        initial_branch: plan.initial_branch(),
    };
    *state.pending_git_init.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git initialization plan is unavailable".to_owned(),
        )
    })? = Some((payload.plan_token.clone(), plan));
    Ok(payload)
}

#[tauri::command]
fn git_initialize_apply_open_workspace(
    window: tauri::Window,
    plan_token: PlanToken,
    state: tauri::State<'_, DesktopState>,
) -> Result<GitInitResultPayload, NativeCommandError> {
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
    let mut pending = state.pending_git_init.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git initialization plan is unavailable".to_owned(),
        )
    })?;
    let plan = take_matching_git_plan(&mut pending, &plan_token).map_err(|error| match error {
        PendingGitPlanError::Missing => native_command_error(
            "planMissing",
            "Create a fresh Git initialization plan and try again.".to_owned(),
        ),
        PendingGitPlanError::Invalid => native_command_error(
            "planInvalid",
            "The Git initialization plan does not match this request.".to_owned(),
        ),
    })?;
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Info)
        .set_title("Initialize Git?")
        .set_description(git_init_confirmation())
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        return Ok(GitInitResultPayload { initialized: false });
    }
    apply_git_initialize(&workspace.view, &plan)
        .map_err(|error| native_command_error(git_init_code(error), git_init_message(error)))?;
    workspace.facts = workspace.facts.after_git_initialize();
    Ok(GitInitResultPayload { initialized: true })
}

fn git_init_code(error: GitInitError) -> &'static str {
    match error {
        GitInitError::AlreadyRepository => "alreadyRepository",
        GitInitError::PlanStale => "planStale",
        GitInitError::ReadOnly => "readOnly",
        GitInitError::NestedRepository => "nestedRepository",
        GitInitError::Unsafe => "unsafeRepository",
        GitInitError::Unsupported => "unsupportedFilesystem",
        GitInitError::Io => "io",
    }
}

fn git_init_message(error: GitInitError) -> String {
    match error {
        GitInitError::AlreadyRepository => {
            "This Workspace already has a Git repository.".to_owned()
        }
        GitInitError::PlanStale => {
            "The Workspace or Git destination changed. Create a fresh plan and try again."
                .to_owned()
        }
        GitInitError::ReadOnly => {
            "This Workspace is read-only. Choose a writable Workspace.".to_owned()
        }
        GitInitError::NestedRepository => {
            "This Workspace is already inside another Git repository. Rehow will not create a nested repository."
                .to_owned()
        }
        GitInitError::Unsafe => {
            "The .git destination is unsafe or contains unrecognized data. Existing data was not overwritten; partial Rehow metadata may remain, so reopen or inspect the Workspace before retrying."
                .to_owned()
        }
        GitInitError::Unsupported => {
            "This filesystem cannot provide the durable Git initialization Rehow requires."
                .to_owned()
        }
        GitInitError::Io => {
            "Git could not be initialized. Check Workspace access and try again.".to_owned()
        }
    }
}

fn git_init_confirmation() -> &'static str {
    "Create private Git metadata for this Workspace with initial branch “main”?\n\nRehow will not commit, connect, or synchronize anything."
}

#[tauri::command]
fn git_remote_plan_open_workspace(
    remote_name: String,
    remote_url: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<GitRemotePlanPayload, NativeCommandError> {
    let guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_ref().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let plan = plan_git_remote_connect(&workspace.view, &remote_name, &remote_url)
        .map_err(|error| native_command_error(git_remote_code(error), git_remote_message(error)))?;
    let payload = GitRemotePlanPayload {
        plan_token: plan.token().clone(),
        remote_name: plan.remote_name().to_owned(),
        transport: plan.transport().as_str(),
        host: plan.host().to_owned(),
    };
    *state.pending_git_remote.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git remote plan is unavailable".to_owned(),
        )
    })? = Some((payload.plan_token.clone(), plan));
    Ok(payload)
}

#[tauri::command]
fn git_remote_apply_open_workspace(
    window: tauri::Window,
    plan_token: PlanToken,
    state: tauri::State<'_, DesktopState>,
) -> Result<GitRemoteResultPayload, NativeCommandError> {
    let guard = state.workspace.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Workspace session is unavailable".to_owned(),
        )
    })?;
    let workspace = guard.as_ref().ok_or_else(|| {
        native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
    })?;
    if !workspace.facts.writable() {
        return Err(native_command_error(
            "readOnly",
            "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
        ));
    }
    let mut pending = state.pending_git_remote.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git remote plan is unavailable".to_owned(),
        )
    })?;
    let plan = take_matching_git_plan(&mut pending, &plan_token).map_err(|error| match error {
        PendingGitPlanError::Missing => native_command_error(
            "planMissing",
            "Create a fresh Git remote plan and try again.".to_owned(),
        ),
        PendingGitPlanError::Invalid => native_command_error(
            "planInvalid",
            "The Git remote plan does not match this request.".to_owned(),
        ),
    })?;
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Info)
        .set_title("Connect Git remote?")
        .set_description(git_remote_confirmation(&plan))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        return Ok(GitRemoteResultPayload { connected: false });
    }
    apply_git_remote_connect(&workspace.view, &plan)
        .map_err(|error| native_command_error(git_remote_code(error), git_remote_message(error)))?;
    Ok(GitRemoteResultPayload { connected: true })
}

fn git_remote_code(error: GitRemoteError) -> &'static str {
    match error {
        GitRemoteError::InvalidName => "invalidRemoteName",
        GitRemoteError::InvalidUrl => "invalidRemoteUrl",
        GitRemoteError::NotRepository => "notRepository",
        GitRemoteError::DuplicateRemote => "duplicateRemote",
        GitRemoteError::MalformedConfig => "malformedConfig",
        GitRemoteError::RecoveryRequired => "recoveryRequired",
        GitRemoteError::PlanStale => "planStale",
        GitRemoteError::Unsafe => "unsafeRepository",
        GitRemoteError::TooLarge => "configTooLarge",
        GitRemoteError::Unsupported => "unsupportedFilesystem",
        GitRemoteError::GitUnavailable => "gitUnavailable",
        GitRemoteError::Io => "io",
    }
}

fn git_remote_message(error: GitRemoteError) -> String {
    match error {
        GitRemoteError::InvalidName => {
            "Use a lowercase remote name containing letters, numbers, dots, dashes, or underscores."
                .to_owned()
        }
        GitRemoteError::InvalidUrl => {
            "Use a secret-free HTTPS or SSH repository address without a password, token, query, fragment, space, or encoded path."
                .to_owned()
        }
        GitRemoteError::NotRepository => "Initialize Git before connecting a remote.".to_owned(),
        GitRemoteError::DuplicateRemote => {
            "A remote with that name already exists. Rehow did not change it.".to_owned()
        }
        GitRemoteError::MalformedConfig => {
            "Git config is malformed. Rehow preserved it unchanged.".to_owned()
        }
        GitRemoteError::RecoveryRequired => {
            "Git config has an unrelated lock or interrupted change. Retry the exact same remote request, or inspect .git/config.lock before continuing."
                .to_owned()
        }
        GitRemoteError::PlanStale => {
            "Git config or the Workspace changed. Review it and create a fresh plan.".to_owned()
        }
        GitRemoteError::Unsafe => {
            "Git config is not a safe owner-controlled regular file. Rehow left it unchanged."
                .to_owned()
        }
        GitRemoteError::TooLarge => {
            "Git config exceeds Rehow’s safe processing limit.".to_owned()
        }
        GitRemoteError::Unsupported => {
            "This system cannot provide the durable atomic Git config update Rehow requires."
                .to_owned()
        }
        GitRemoteError::GitUnavailable => {
            "The trusted system Git executable is unavailable.".to_owned()
        }
        GitRemoteError::Io => {
            "Git config could not be read or updated. Check access and try again.".to_owned()
        }
    }
}

fn git_remote_confirmation(plan: &GitRemotePlan) -> String {
    git_remote_confirmation_fields(
        plan.remote_name(),
        plan.transport().as_str(),
        plan.remote_url(),
    )
}

#[tauri::command]
async fn git_commit_plan_open_workspace(
    app: tauri::AppHandle,
    message: String,
) -> Result<GitCommitPlanPayload, NativeCommandError> {
    if !git_commit_message_input_allowed(&message) {
        return Err(native_command_error(
            git_commit_code(GitCommitPlanError::InvalidMessage),
            git_commit_message(GitCommitPlanError::InvalidMessage),
        ));
    }
    let (state_home, _) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    let state = app.state::<DesktopState>();
    let previous = {
        let mut lifecycle = state.git_commit.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git commit state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active {
            return Err(native_command_error(
                "commitBusy",
                "A Git commit operation is already running.".to_owned(),
            ));
        }
        lifecycle.active = true;
        lifecycle.pending.take()
    };
    let worker_app = app.clone();
    let operation = tauri::async_runtime::spawn_blocking(move || {
        if fs::create_dir_all(&state_home).is_err() {
            return Err(native_command_error(
                "stateUnavailable",
                "Rehow could not prepare its private commit staging folder.".to_owned(),
            ));
        }
        if let Some((_, plan)) = previous {
            cancel_git_commit_plan(plan).map_err(|error| {
                native_command_error(git_commit_code(error), git_commit_message(error))
            })?;
        }
        let state = worker_app.state::<DesktopState>();
        let guard = state.workspace.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable".to_owned(),
            )
        })?;
        let workspace = guard.as_ref().ok_or_else(|| {
            native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
        })?;
        if !workspace.facts.writable() {
            return Err(native_command_error(
                "readOnly",
                "This Workspace is read-only. Choose a writable Workspace.".to_owned(),
            ));
        }
        let plan = plan_git_commit(&workspace.view, &state_home, &message).map_err(|error| {
            native_command_error(git_commit_code(error), git_commit_message(error))
        })?;
        let payload = git_commit_plan_payload(&plan);
        Ok((payload, plan))
    })
    .await;
    let result = match operation {
        Ok(result) => result,
        Err(_) => Err(native_command_error(
            "sessionUnavailable",
            "Git commit planning stopped unexpectedly. Create a fresh plan and try again."
                .to_owned(),
        )),
    };
    let mut lifecycle = state.git_commit.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git commit state is unavailable".to_owned(),
        )
    })?;
    lifecycle.active = false;
    match result {
        Ok((payload, plan)) => {
            lifecycle.pending = Some((payload.plan_token.clone(), plan));
            Ok(payload)
        }
        Err(error) => Err(error),
    }
}

#[tauri::command]
async fn git_commit_cancel_open_workspace(
    app: tauri::AppHandle,
    plan_token: PlanToken,
) -> Result<GitCommitCancelResultPayload, NativeCommandError> {
    let state = app.state::<DesktopState>();
    let plan = {
        let mut lifecycle = state.git_commit.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git commit state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active {
            return Err(native_command_error(
                "commitBusy",
                "A Git commit operation is already running.".to_owned(),
            ));
        }
        let plan = take_matching_git_plan(&mut lifecycle.pending, &plan_token)
            .map_err(git_commit_pending_error)?;
        lifecycle.active = true;
        plan
    };
    let result = tauri::async_runtime::spawn_blocking(move || cancel_git_commit_plan(plan)).await;
    let mut lifecycle = state.git_commit.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git commit state is unavailable".to_owned(),
        )
    })?;
    lifecycle.active = false;
    match result {
        Ok(Ok(())) => Ok(GitCommitCancelResultPayload { cancelled: true }),
        Ok(Err(error)) => Err(native_command_error(
            git_commit_code(error),
            git_commit_message(error),
        )),
        Err(_) => Err(native_command_error(
            "sessionUnavailable",
            "Git commit cancellation stopped unexpectedly. Reopen the Workspace before retrying."
                .to_owned(),
        )),
    }
}

#[tauri::command]
async fn git_commit_apply_open_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
    plan_token: PlanToken,
) -> Result<GitCommitResultPayload, NativeCommandError> {
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    let state = app.state::<DesktopState>();
    let plan = {
        let mut lifecycle = state.git_commit.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git commit state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active {
            return Err(native_command_error(
                "commitBusy",
                "A Git commit operation is already running.".to_owned(),
            ));
        }
        let plan = take_matching_git_plan(&mut lifecycle.pending, &plan_token)
            .map_err(git_commit_pending_error)?;
        lifecycle.active = true;
        plan
    };
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Info)
        .set_title("Create Git Commit?")
        .set_description(git_commit_confirmation(&plan))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        let result =
            tauri::async_runtime::spawn_blocking(move || cancel_git_commit_plan(plan)).await;
        finish_git_commit_operation(&state)?;
        return match result {
            Ok(Ok(())) => Ok(GitCommitResultPayload {
                committed: false,
                workspace_changed: false,
            }),
            Ok(Err(error)) => Err(native_command_error(
                git_commit_code(error),
                git_commit_message(error),
            )),
            Err(_) => Err(native_command_error(
                "sessionUnavailable",
                "Git commit cancellation stopped unexpectedly. Reopen the Workspace before retrying."
                    .to_owned(),
            )),
        };
    }
    let worker_app = app.clone();
    let operation = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<DesktopState>();
        let guard = state.workspace.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable".to_owned(),
            )
        })?;
        let workspace = guard.as_ref().ok_or_else(|| {
            native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
        })?;
        apply_git_commit(
            &workspace.view,
            &state_home,
            runtime_directory.as_deref(),
            plan,
        )
        .map_err(|error| native_command_error(git_commit_code(error), git_commit_message(error)))
    })
    .await;
    finish_git_commit_operation(&state)?;
    match operation {
        Ok(Ok(outcome)) => Ok(GitCommitResultPayload {
            committed: true,
            workspace_changed: outcome.workspace_changed(),
        }),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(native_command_error(
            "sessionUnavailable",
            "Git commit publication stopped unexpectedly. Refresh Git status to recover safely."
                .to_owned(),
        )),
    }
}

#[tauri::command]
async fn git_commit_recover_open_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
) -> Result<GitCommitRecoveryResultPayload, NativeCommandError> {
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    let state = app.state::<DesktopState>();
    {
        let mut lifecycle = state.git_commit.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git commit state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active {
            return Err(native_command_error(
                "commitBusy",
                "A Git commit operation is already running.".to_owned(),
            ));
        }
        if lifecycle.pending.is_some() {
            return Err(native_command_error(
                "planPending",
                "Cancel the current Git commit plan before recovering another operation."
                    .to_owned(),
            ));
        }
        lifecycle.active = true;
    }
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Resume Git Commit?")
        .set_description(git_commit_recovery_confirmation())
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        finish_git_commit_operation(&state)?;
        return Ok(GitCommitRecoveryResultPayload {
            recovered: false,
            workspace_changed: false,
        });
    }
    let worker_app = app.clone();
    let operation = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<DesktopState>();
        let guard = state.workspace.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable".to_owned(),
            )
        })?;
        let workspace = guard.as_ref().ok_or_else(|| {
            native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
        })?;
        resume_pending_git_commit_publication(
            &workspace.view,
            &state_home,
            runtime_directory.as_deref(),
        )
        .map_err(|error| native_command_error(git_commit_code(error), git_commit_message(error)))
    })
    .await;
    finish_git_commit_operation(&state)?;
    match operation {
        Ok(Ok(outcome)) => Ok(GitCommitRecoveryResultPayload {
            recovered: true,
            workspace_changed: outcome.workspace_changed(),
        }),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(native_command_error(
            "sessionUnavailable",
            "Git commit recovery stopped unexpectedly. Refresh Git status before retrying."
                .to_owned(),
        )),
    }
}

fn finish_git_commit_operation(
    state: &tauri::State<'_, DesktopState>,
) -> Result<(), NativeCommandError> {
    state
        .git_commit
        .lock()
        .map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git commit state is unavailable".to_owned(),
            )
        })?
        .active = false;
    Ok(())
}

fn git_commit_plan_payload(plan: &GitCommitPlan) -> GitCommitPlanPayload {
    GitCommitPlanPayload {
        plan_token: plan.token().clone(),
        branch: plan.branch().to_owned(),
        created: plan.created(),
        updated: plan.updated(),
        deleted: plan.deleted(),
        unrelated_changes: plan.unrelated_changes(),
    }
}

fn git_commit_message_input_allowed(message: &str) -> bool {
    message.len() <= GIT_COMMIT_MESSAGE_INPUT_BYTES
}

fn git_commit_pending_error(error: PendingGitPlanError) -> NativeCommandError {
    match error {
        PendingGitPlanError::Missing => native_command_error(
            "planMissing",
            "Create a fresh Git commit plan and try again.".to_owned(),
        ),
        PendingGitPlanError::Invalid => native_command_error(
            "planInvalid",
            "The Git commit plan does not match this request.".to_owned(),
        ),
    }
}

fn git_commit_confirmation(plan: &GitCommitPlan) -> String {
    git_commit_confirmation_fields(
        plan.message(),
        plan.branch(),
        plan.created(),
        plan.updated(),
        plan.deleted(),
        plan.unrelated_changes(),
    )
}

fn git_commit_confirmation_fields(
    message: &str,
    branch: &str,
    created: u32,
    updated: u32,
    deleted: u32,
    unrelated_changes: u32,
) -> String {
    format!(
        "Create unsigned commit “{message}” on branch “{branch}”?\n\nManaged changes: {created} created, {updated} updated, {deleted} deleted.\nUnrelated worktree changes: {unrelated_changes}; they remain uncommitted.\n\nRehow will not push, connect to a network, run hooks, or sign this commit."
    )
}

fn git_commit_recovery_confirmation() -> &'static str {
    "Resume the interrupted managed commit?\n\nRehow will inspect its exact recovery record and either finish the approved index/branch update or preserve conflicting Git state unchanged. It will not push or connect to a network."
}

fn git_commit_code(error: GitCommitPlanError) -> &'static str {
    match error {
        GitCommitPlanError::Cancelled => "cancelled",
        GitCommitPlanError::InvalidMessage => "invalidMessage",
        GitCommitPlanError::InvalidIdentity => "invalidIdentity",
        GitCommitPlanError::WorkspaceIssues => "workspaceIssues",
        GitCommitPlanError::ReadOnly => "readOnly",
        GitCommitPlanError::NotRepository => "notRepository",
        GitCommitPlanError::UnsafeRepository => "unsafeRepository",
        GitCommitPlanError::UnsupportedRepository => "unsupportedRepository",
        GitCommitPlanError::OperationInProgress => "recoveryRequired",
        GitCommitPlanError::UnsupportedIndex => "unsupportedIndex",
        GitCommitPlanError::AmbiguousManagedHistory => "ambiguousHistory",
        GitCommitPlanError::PathCollision => "pathCollision",
        GitCommitPlanError::NothingToCommit => "nothingToCommit",
        GitCommitPlanError::ResourceLimit => "resourceLimit",
        GitCommitPlanError::PlanStale => "planStale",
        GitCommitPlanError::RecoveryNotRequired => "recoveryNotRequired",
        GitCommitPlanError::Io => "io",
    }
}

fn git_commit_message(error: GitCommitPlanError) -> String {
    match error {
        GitCommitPlanError::Cancelled => {
            "The Git commit was cancelled before publication.".to_owned()
        }
        GitCommitPlanError::InvalidMessage => {
            "Enter a one-line commit message between 1 and 200 characters.".to_owned()
        }
        GitCommitPlanError::InvalidIdentity => {
            "Set one safe user.name and user.email in this repository’s local Git config, then try again."
                .to_owned()
        }
        GitCommitPlanError::WorkspaceIssues => {
            "Resolve the Workspace validation issues before creating a commit.".to_owned()
        }
        GitCommitPlanError::ReadOnly => {
            "This Workspace is read-only. Choose a writable Workspace.".to_owned()
        }
        GitCommitPlanError::NotRepository => "Initialize Git before creating a commit.".to_owned(),
        GitCommitPlanError::UnsafeRepository => {
            "Git metadata is unsafe or ambiguous. Rehow preserved it unchanged; inspect the repository before retrying."
                .to_owned()
        }
        GitCommitPlanError::UnsupportedRepository => {
            "This repository or filesystem cannot provide the isolated, durable commit operation Rehow requires."
                .to_owned()
        }
        GitCommitPlanError::OperationInProgress => {
            "An interrupted Git commit needs recovery before another commit can start.".to_owned()
        }
        GitCommitPlanError::UnsupportedIndex => {
            "The Git index contains staged, conflicted, or advanced state. Resolve it with Git before using Rehow commit."
                .to_owned()
        }
        GitCommitPlanError::AmbiguousManagedHistory => {
            "The previous commit does not contain one complete, compatible Rehow Workspace. Review its history before retrying."
                .to_owned()
        }
        GitCommitPlanError::PathCollision => {
            "An unrelated committed path collides with current Rehow content. Resolve the collision with Git first."
                .to_owned()
        }
        GitCommitPlanError::NothingToCommit => {
            "The managed Workspace has no changes to commit.".to_owned()
        }
        GitCommitPlanError::ResourceLimit => {
            "The commit exceeded a safe time, file, object, output, or staging limit.".to_owned()
        }
        GitCommitPlanError::PlanStale => {
            "The Workspace or repository changed. Refresh Git status and create a fresh plan."
                .to_owned()
        }
        GitCommitPlanError::RecoveryNotRequired => {
            "No interrupted Git commit needs recovery. Refresh Git status.".to_owned()
        }
        GitCommitPlanError::Io => {
            "The Git commit operation could not finish. Check local access and refresh Git status."
                .to_owned()
        }
    }
}

#[tauri::command]
async fn git_sync_inspect_open_workspace(
    app: tauri::AppHandle,
) -> Result<GitSyncInspectionPayload, NativeCommandError> {
    let (state_home, _) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private sync staging folder.".to_owned(),
        )
    })?;
    let state = app.state::<DesktopState>();
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut lifecycle = state.git_sync.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git sync state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active.is_some() || lifecycle.applying {
            return Err(native_command_error(
                "syncBusy",
                "A Git sync operation is already running.".to_owned(),
            ));
        }
        lifecycle.classified = None;
        lifecycle.active = Some(Arc::clone(&cancelled));
        lifecycle.revoke_on_completion = false;
    }
    let operation_cancelled = Arc::clone(&cancelled);
    let worker_app = app.clone();
    let operation = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<DesktopState>();
        let guard = state.workspace.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable".to_owned(),
            )
        })?;
        let workspace = guard.as_ref().ok_or_else(|| {
            native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
        })?;
        let plan = plan_git_sync(&workspace.view, &state_home)
            .map_err(|error| native_command_error(git_sync_code(error), git_sync_message(error)))?;
        let authentication = prepare_git_remote_authentication(&state, plan.transport())?;
        let inspection = apply_git_sync_inspection_with_authentication(
            &workspace.view,
            &state_home,
            plan,
            &operation_cancelled,
            authentication.as_borrowed(),
        )
        .map_err(|error| native_command_error(git_sync_code(error), git_sync_message(error)))?;
        classify_git_sync_ancestry_with_authentication(
            &workspace.view,
            inspection,
            &operation_cancelled,
            authentication.as_borrowed(),
        )
        .map_err(|error| native_command_error(git_sync_code(error), git_sync_message(error)))
    })
    .await;
    let mut lifecycle = state.git_sync.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git sync state is unavailable".to_owned(),
        )
    })?;
    let owns_operation = lifecycle
        .active
        .as_ref()
        .is_some_and(|active| Arc::ptr_eq(active, &cancelled));
    if !owns_operation {
        return Err(native_command_error(
            "sessionUnavailable",
            "Git sync state changed unexpectedly. Refresh status and try again.".to_owned(),
        ));
    }
    lifecycle.active = None;
    let revoked = lifecycle.revoke_on_completion || cancelled.load(Ordering::Relaxed);
    lifecycle.revoke_on_completion = false;
    if revoked {
        lifecycle.classified = None;
        return Err(native_command_error(
            git_sync_code(GitSyncError::Cancelled),
            git_sync_message(GitSyncError::Cancelled),
        ));
    }
    match operation {
        Ok(Ok(classification)) => {
            let payload = git_sync_inspection_payload(&classification);
            lifecycle.classified = Some((payload.plan_token.clone(), classification));
            Ok(payload)
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err(native_command_error(
            "sessionUnavailable",
            "Git sync inspection stopped unexpectedly. Refresh status before retrying.".to_owned(),
        )),
    }
}

#[tauri::command]
fn git_sync_cancel_active(
    state: tauri::State<'_, DesktopState>,
) -> Result<GitSyncCancelResultPayload, NativeCommandError> {
    let cancellation_requested =
        request_git_sync_cancellation(&state.git_sync).map_err(|error| match error {
            GitSyncCancelError::Busy => native_command_error(
                "syncBusy",
                "The confirmed Git sync is being applied and cannot be cancelled.".to_owned(),
            ),
            GitSyncCancelError::Unavailable => native_command_error(
                "sessionUnavailable",
                "Git sync state is unavailable".to_owned(),
            ),
        })?;
    Ok(GitSyncCancelResultPayload {
        cancellation_requested,
    })
}

#[tauri::command]
async fn git_sync_apply_open_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
    plan_token: PlanToken,
) -> Result<GitSyncApplyResultPayload, NativeCommandError> {
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    let state = app.state::<DesktopState>();
    let classification = {
        let mut lifecycle = state.git_sync.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git sync state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active.is_some() || lifecycle.applying {
            return Err(native_command_error(
                "syncBusy",
                "A Git sync operation is already running.".to_owned(),
            ));
        }
        let classification = take_matching_git_plan(&mut lifecycle.classified, &plan_token)
            .map_err(git_sync_pending_error)?;
        if classification.ancestry() != GitSyncAncestry::RemoteFastForward
            || classification.local_changes()
        {
            return Err(native_command_error(
                "applyNotAllowed",
                "This sync result cannot be applied automatically. Review the reported Git state."
                    .to_owned(),
            ));
        }
        lifecycle.applying = true;
        classification
    };
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Apply Git Fast-forward?")
        .set_description(git_sync_confirmation(&classification))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        finish_git_sync_apply(&state)?;
        return Ok(GitSyncApplyResultPayload::Declined);
    }
    let worker_app = app.clone();
    let operation = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<DesktopState>();
        let mut guard = state.workspace.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable".to_owned(),
            )
        })?;
        let workspace = guard.as_mut().ok_or_else(|| {
            native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
        })?;
        let (fresh, outcome) = apply_git_sync_fast_forward(
            &workspace.view,
            &state_home,
            runtime_directory.as_deref(),
            classification,
        )
        .map_err(|error| native_command_error(git_sync_code(error), git_sync_message(error)))?;
        workspace.view = fresh;
        workspace.rebuild_watcher(&worker_app);
        Ok((workspace.payload(), outcome.changed()))
    })
    .await;
    finish_git_sync_apply(&state)?;
    match operation {
        Ok(Ok((workspace, changed))) => {
            Ok(GitSyncApplyResultPayload::Completed { workspace, changed })
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err(native_command_error(
            "sessionUnavailable",
            "Git sync publication stopped unexpectedly. Refresh Git status to recover safely."
                .to_owned(),
        )),
    }
}

#[tauri::command]
async fn git_push_apply_open_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
    plan_token: PlanToken,
) -> Result<GitPushResultPayload, NativeCommandError> {
    let state = app.state::<DesktopState>();
    let classification = {
        let mut lifecycle = state.git_sync.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git sync state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active.is_some() || lifecycle.applying {
            return Err(native_command_error(
                "syncBusy",
                "A Git sync operation is already running.".to_owned(),
            ));
        }
        let classification = take_matching_git_plan(&mut lifecycle.classified, &plan_token)
            .map_err(git_sync_pending_error)?;
        if !git_push_ancestry_allowed(classification.ancestry()) {
            return Err(native_command_error(
                "pushNotAllowed",
                "Only a validated local-ahead branch can be pushed safely.".to_owned(),
            ));
        }
        lifecycle.applying = true;
        classification
    };
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Push Git Commit?")
        .set_description(git_push_confirmation(&classification))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        finish_git_sync_apply(&state)?;
        return Ok(GitPushResultPayload::Declined);
    }
    let authentication = match prepare_git_remote_authentication(&state, classification.transport())
    {
        Ok(authentication) => authentication,
        Err(error) => {
            finish_git_sync_apply(&state)?;
            return Err(error);
        }
    };
    let worker_app = app.clone();
    let operation = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<DesktopState>();
        let guard = state.workspace.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable".to_owned(),
            )
        })?;
        let workspace = guard.as_ref().ok_or_else(|| {
            native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
        })?;
        apply_git_push_with_authentication(
            &workspace.view,
            classification,
            authentication.as_borrowed(),
        )
        .map_err(|error| native_command_error(git_sync_code(error), git_sync_message(error)))
    })
    .await;
    finish_git_sync_apply(&state)?;
    match operation {
        Ok(Ok(outcome)) => Ok(GitPushResultPayload::Completed {
            pushed: outcome.pushed(),
        }),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(native_command_error(
            "sessionUnavailable",
            "Git push stopped unexpectedly. Inspect the upstream again before retrying.".to_owned(),
        )),
    }
}

#[tauri::command]
async fn git_sync_recover_open_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
) -> Result<GitSyncRecoveryResultPayload, NativeCommandError> {
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    let state = app.state::<DesktopState>();
    {
        let mut lifecycle = state.git_sync.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git sync state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active.is_some() || lifecycle.applying {
            return Err(native_command_error(
                "syncBusy",
                "A Git sync operation is already running.".to_owned(),
            ));
        }
        if lifecycle.classified.is_some() {
            return Err(native_command_error(
                "planPending",
                "Cancel the current Git sync result before recovering another operation."
                    .to_owned(),
            ));
        }
        lifecycle.applying = true;
    }
    let recovery_required_result: Result<bool, NativeCommandError> = (|| {
        let guard = state.workspace.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable".to_owned(),
            )
        })?;
        let workspace = guard.as_ref().ok_or_else(|| {
            native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
        })?;
        git_sync_recovery_required(&workspace.view, &state_home, runtime_directory.as_deref())
            .map_err(|error| native_command_error(git_sync_code(error), git_sync_message(error)))
    })();
    let recovery_required = match recovery_required_result {
        Ok(required) => required,
        Err(error) => {
            finish_git_sync_apply(&state)?;
            return Err(error);
        }
    };
    if !recovery_required {
        finish_git_sync_apply(&state)?;
        return Err(native_command_error(
            "recoveryNotRequired",
            "No interrupted Git sync needs recovery. Refresh Git status.".to_owned(),
        ));
    }
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Resume Git Sync?")
        .set_description(git_sync_recovery_confirmation())
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        finish_git_sync_apply(&state)?;
        return Ok(GitSyncRecoveryResultPayload::Declined);
    }
    let worker_app = app.clone();
    let operation = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<DesktopState>();
        let mut guard = state.workspace.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable".to_owned(),
            )
        })?;
        let workspace = guard.as_mut().ok_or_else(|| {
            native_command_error("workspaceUnavailable", "No Workspace is open".to_owned())
        })?;
        let (fresh, outcome) = resume_pending_git_sync_publication(
            &workspace.view,
            &state_home,
            runtime_directory.as_deref(),
        )
        .map_err(|error| native_command_error(git_sync_code(error), git_sync_message(error)))?;
        workspace.view = fresh;
        workspace.rebuild_watcher(&worker_app);
        Ok((workspace.payload(), outcome.changed()))
    })
    .await;
    finish_git_sync_apply(&state)?;
    match operation {
        Ok(Ok((workspace, changed))) => {
            Ok(GitSyncRecoveryResultPayload::Completed { workspace, changed })
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err(native_command_error(
            "sessionUnavailable",
            "Git sync recovery stopped unexpectedly. Refresh Git status before retrying."
                .to_owned(),
        )),
    }
}

fn finish_git_sync_apply(state: &tauri::State<'_, DesktopState>) -> Result<(), NativeCommandError> {
    state
        .git_sync
        .lock()
        .map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git sync state is unavailable".to_owned(),
            )
        })?
        .applying = false;
    Ok(())
}

fn git_sync_inspection_payload(classification: &GitSyncClassification) -> GitSyncInspectionPayload {
    GitSyncInspectionPayload {
        plan_token: classification.token().clone(),
        remote_name: classification.remote_name().to_owned(),
        transport: classification.transport().as_str(),
        host: classification.host().to_owned(),
        ancestry: git_sync_ancestry(classification.ancestry()),
        local_changes: classification.local_changes(),
        workspace_matches_remote: classification.workspace_matches_remote(),
    }
}

fn git_sync_ancestry(ancestry: GitSyncAncestry) -> &'static str {
    match ancestry {
        GitSyncAncestry::UpToDate => "upToDate",
        GitSyncAncestry::LocalAhead => "localAhead",
        GitSyncAncestry::RemoteFastForward => "remoteFastForward",
        GitSyncAncestry::Diverged => "diverged",
        GitSyncAncestry::HistoryLimit => "historyLimit",
    }
}

fn git_sync_pending_error(error: PendingGitPlanError) -> NativeCommandError {
    match error {
        PendingGitPlanError::Missing => native_command_error(
            "planMissing",
            "Inspect the configured upstream before applying a Git sync.".to_owned(),
        ),
        PendingGitPlanError::Invalid => native_command_error(
            "planInvalid",
            "The Git sync result does not match this request.".to_owned(),
        ),
    }
}

fn git_sync_confirmation(classification: &GitSyncClassification) -> String {
    git_sync_confirmation_fields(
        classification.remote_name(),
        classification.transport().as_str(),
        classification.host(),
    )
}

fn git_sync_confirmation_fields(remote_name: &str, transport: &str, host: &str) -> String {
    format!(
        "Apply the validated fast-forward from remote “{remote_name}” ({}, {host})?\n\nRehow will update portable Workspace files, the Git index, and the current branch. It will not push or run Git hooks.",
        transport.to_uppercase()
    )
}

fn git_push_confirmation(classification: &GitSyncClassification) -> String {
    git_push_confirmation_fields(
        classification.remote_name(),
        classification.remote_branch(),
        classification.transport().as_str(),
        classification.host(),
    )
}

fn git_push_confirmation_fields(
    remote_name: &str,
    remote_branch: &str,
    transport: &str,
    host: &str,
) -> String {
    format!(
        "Push the reviewed local commit to “{remote_name}/{remote_branch}” ({}, {host})?\n\nRehow will recheck the exact upstream commit and request one normal fast-forward push. It will not force, push tags, run hooks, or store credentials.",
        transport.to_uppercase()
    )
}

fn git_sync_recovery_confirmation() -> &'static str {
    "Resume the interrupted Git sync?\n\nRehow will verify the durable recovery record and either finish the approved Workspace/index/branch update or preserve conflicting state unchanged. It will not push or run Git hooks."
}

fn git_sync_code(error: GitSyncError) -> &'static str {
    match error {
        GitSyncError::NotRepository => "notRepository",
        GitSyncError::NoUpstream => "noUpstream",
        GitSyncError::InvalidConfiguration => "invalidConfiguration",
        GitSyncError::UnsupportedTransport => "unsupportedTransport",
        GitSyncError::RecoveryRequired => "recoveryRequired",
        GitSyncError::ReadOnly => "readOnly",
        GitSyncError::WorkspaceIssues => "workspaceIssues",
        GitSyncError::PlanStale => "planStale",
        GitSyncError::Cancelled => "cancelled",
        GitSyncError::Timeout => "timeout",
        GitSyncError::ResourceLimit => "resourceLimit",
        GitSyncError::GitUnavailable => "gitUnavailable",
        GitSyncError::SandboxUnavailable => "sandboxUnavailable",
        GitSyncError::RemoteUnavailable => "remoteUnavailable",
        GitSyncError::AuthenticationFailed => "authenticationFailed",
        GitSyncError::HostKeyRejected => "hostKeyRejected",
        GitSyncError::RemoteChanged => "remoteChanged",
        GitSyncError::RemoteStateUncertain => "remoteStateUncertain",
        GitSyncError::InvalidRepository => "invalidRepository",
        GitSyncError::InvalidWorkspace => "invalidWorkspace",
        GitSyncError::StagingConflict => "stagingConflict",
        GitSyncError::Unsafe => "unsafeRepository",
        GitSyncError::Unsupported => "unsupported",
        GitSyncError::Io => "io",
    }
}

fn git_sync_message(error: GitSyncError) -> String {
    match error {
        GitSyncError::NotRepository => "Initialize Git before synchronizing.".to_owned(),
        GitSyncError::NoUpstream => {
            "Configure one upstream branch before synchronizing.".to_owned()
        }
        GitSyncError::InvalidConfiguration => {
            "The upstream or recovery configuration is invalid. Refresh Git status and inspect the repository."
                .to_owned()
        }
        GitSyncError::UnsupportedTransport => {
            "This upstream transport is not supported for sync.".to_owned()
        }
        GitSyncError::RecoveryRequired => {
            "An interrupted Git operation needs recovery before another sync can start.".to_owned()
        }
        GitSyncError::ReadOnly => {
            "This Workspace is read-only. Choose a writable Workspace.".to_owned()
        }
        GitSyncError::WorkspaceIssues => {
            "Resolve the Workspace validation issues before synchronizing.".to_owned()
        }
        GitSyncError::PlanStale => {
            "The Workspace or repository changed. Inspect the upstream again.".to_owned()
        }
        GitSyncError::Cancelled => "Git sync inspection was cancelled safely.".to_owned(),
        GitSyncError::Timeout => {
            "The remote operation did not finish before the safety timeout.".to_owned()
        }
        GitSyncError::ResourceLimit => {
            "The sync exceeded a safe time, file, history, object, or output limit.".to_owned()
        }
        GitSyncError::GitUnavailable => {
            "Git is unavailable at the trusted system location.".to_owned()
        }
        GitSyncError::SandboxUnavailable => {
            "The required local Git sandbox is unavailable.".to_owned()
        }
        GitSyncError::RemoteUnavailable => {
            "The configured upstream could not be reached or authenticated.".to_owned()
        }
        GitSyncError::AuthenticationFailed => {
            "Remote authentication failed. Check the credential helper or SSH agent configured for this remote."
                .to_owned()
        }
        GitSyncError::HostKeyRejected => {
            "SSH host verification failed. Check your trusted known-host entries and restart Rehow."
                .to_owned()
        }
        GitSyncError::RemoteChanged => {
            "The upstream changed after inspection. Nothing was forced; inspect it again before retrying."
                .to_owned()
        }
        GitSyncError::RemoteStateUncertain => {
            "The push may have completed, but Rehow could not verify the final upstream state. Inspect again before retrying."
                .to_owned()
        }
        GitSyncError::InvalidRepository => {
            "The upstream repository returned invalid or inconsistent Git data.".to_owned()
        }
        GitSyncError::InvalidWorkspace => {
            "The upstream does not contain this compatible Rehow Workspace.".to_owned()
        }
        GitSyncError::StagingConflict => {
            "Private sync staging changed unexpectedly. Inspect the upstream again.".to_owned()
        }
        GitSyncError::Unsafe => {
            "Git metadata is unsafe or ambiguous. Rehow preserved it unchanged.".to_owned()
        }
        GitSyncError::Unsupported => {
            "This Git state cannot be synchronized automatically. Review it with Git.".to_owned()
        }
        GitSyncError::Io => {
            "The Git sync could not finish. Check local access and refresh status.".to_owned()
        }
    }
}

#[tauri::command]
fn git_clone_plan_selected_workspace(
    remote_url: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<GitClonePlanPayload, NativeCommandError> {
    let mut lifecycle = state.git_clone.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git clone state is unavailable".to_owned(),
        )
    })?;
    if lifecycle.active.is_some() {
        return Err(native_command_error(
            "cloneBusy",
            "A Git clone validation is already running.".to_owned(),
        ));
    }
    if lifecycle.validated.is_some() {
        return Err(native_command_error(
            "completionPending",
            "Complete or cancel the validated clone before creating another plan.".to_owned(),
        ));
    }
    lifecycle.pending = None;
    let pending_workspace = state.pending_empty.lock().map_err(|_| {
        native_command_error(
            "selectionUnavailable",
            "Workspace selection is unavailable".to_owned(),
        )
    })?;
    let selected = pending_workspace.as_ref().ok_or_else(|| {
        native_command_error(
            "selectionUnavailable",
            "Choose an empty folder before cloning a Workspace.".to_owned(),
        )
    })?;
    let plan = plan_git_clone(&selected.target, &remote_url)
        .map_err(|error| native_command_error(git_clone_code(error), git_clone_message(error)))?;
    let payload = GitClonePlanPayload {
        plan_token: plan.token().clone(),
        transport: plan.transport().as_str(),
        host: plan.host().to_owned(),
    };
    lifecycle.pending = Some((payload.plan_token.clone(), plan));
    Ok(payload)
}

#[tauri::command]
async fn git_clone_apply_selected_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
    plan_token: PlanToken,
) -> Result<GitCloneApplyResultPayload, NativeCommandError> {
    let state = app.state::<DesktopState>();
    let cancelled = Arc::new(AtomicBool::new(false));
    let (plan, selected) = {
        let mut lifecycle = state.git_clone.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git clone state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active.is_some() {
            return Err(native_command_error(
                "cloneBusy",
                "A Git clone validation is already running.".to_owned(),
            ));
        }
        if lifecycle.validated.is_some() {
            return Err(native_command_error(
                "completionPending",
                "Complete or cancel the validated clone before starting another operation."
                    .to_owned(),
            ));
        }
        let plan = take_matching_git_plan(&mut lifecycle.pending, &plan_token).map_err(
            |error| match error {
                PendingGitPlanError::Missing => native_command_error(
                    "planMissing",
                    "Create a fresh Git clone plan and try again.".to_owned(),
                ),
                PendingGitPlanError::Invalid => native_command_error(
                    "planInvalid",
                    "The Git clone plan does not match this request.".to_owned(),
                ),
            },
        )?;
        let selected = state
            .pending_empty
            .lock()
            .map_err(|_| {
                native_command_error(
                    "selectionUnavailable",
                    "Workspace selection is unavailable".to_owned(),
                )
            })?
            .take()
            .ok_or_else(|| {
                native_command_error(
                    "selectionUnavailable",
                    "Choose an empty folder before cloning a Workspace.".to_owned(),
                )
            })?;
        lifecycle.active = Some(Arc::clone(&cancelled));
        lifecycle.revoke_on_completion = false;
        lifecycle.finalizing = false;
        (plan, selected)
    };
    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Info)
        .set_title("Validate Git Workspace?")
        .set_description(git_clone_confirmation(&plan))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    if !confirmed {
        restore_selected_after_clone(&state, &cancelled, selected)?;
        return Ok(GitCloneApplyResultPayload {
            validated: false,
            stage: "declined",
        });
    }
    let (state_home, _) = match native_recovery_locations() {
        Ok(locations) => locations,
        Err(message) => {
            restore_selected_after_clone(&state, &cancelled, selected)?;
            return Err(native_command_error("stateUnavailable", message));
        }
    };
    if fs::create_dir_all(&state_home).is_err() {
        restore_selected_after_clone(&state, &cancelled, selected)?;
        return Err(native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private clone staging folder.".to_owned(),
        ));
    };

    let authentication = match prepare_git_remote_authentication(&state, plan.transport()) {
        Ok(authentication) => authentication,
        Err(error) => {
            restore_selected_after_clone(&state, &cancelled, selected)?;
            return Err(error);
        }
    };

    let operation_cancelled = Arc::clone(&cancelled);
    let operation = tauri::async_runtime::spawn_blocking(move || {
        let result = apply_git_clone_validation_with_authentication(
            &selected.target,
            plan,
            &state_home,
            &operation_cancelled,
            authentication.as_borrowed(),
        );
        (selected, result)
    })
    .await;
    let (selected, result) = match operation {
        Ok(result) => result,
        Err(_) => {
            clear_active_git_clone(&state, &cancelled);
            return Err(native_command_error(
                "sessionUnavailable",
                "Git clone validation stopped unexpectedly. Choose the destination again."
                    .to_owned(),
            ));
        }
    };

    match result {
        Ok(validation) => {
            let mut lifecycle = state.git_clone.lock().map_err(|_| {
                native_command_error(
                    "sessionUnavailable",
                    "Validated Git clone state is unavailable".to_owned(),
                )
            })?;
            if !active_git_clone_matches(&lifecycle, &cancelled) {
                return Err(native_command_error(
                    "sessionUnavailable",
                    "Git clone validation state changed unexpectedly. Choose the destination again."
                        .to_owned(),
                ));
            }
            if lifecycle.revoke_on_completion {
                lifecycle.active = None;
                lifecycle.revoke_on_completion = false;
                return Err(native_command_error(
                    "cancelled",
                    "Clone validation was cancelled. The selected folder was not changed."
                        .to_owned(),
                ));
            }
            lifecycle.validated = Some(PendingValidatedClone {
                selected,
                validation,
            });
            lifecycle.active = None;
            lifecycle.revoke_on_completion = false;
            lifecycle.finalizing = false;
            Ok(GitCloneApplyResultPayload {
                validated: true,
                stage: "validated",
            })
        }
        Err(error) => {
            if error == GitCloneApplyError::DestinationChanged {
                clear_active_git_clone(&state, &cancelled);
                return Err(native_command_error(
                    git_clone_apply_code(error),
                    git_clone_apply_message(error),
                ));
            }
            restore_selected_after_clone(&state, &cancelled, selected)?;
            Err(native_command_error(
                git_clone_apply_code(error),
                git_clone_apply_message(error),
            ))
        }
    }
}

#[tauri::command]
async fn git_clone_complete_selected_workspace(
    window: tauri::Window,
    app: tauri::AppHandle,
) -> Result<GitCloneCompleteResultPayload, NativeCommandError> {
    let state = app.state::<DesktopState>();
    let (state_home, runtime_directory) = native_recovery_locations()
        .map_err(|message| native_command_error("stateUnavailable", message))?;
    fs::create_dir_all(&state_home).map_err(|_| {
        native_command_error(
            "stateUnavailable",
            "Rehow could not prepare its private recovery folder.".to_owned(),
        )
    })?;
    let active = Arc::new(AtomicBool::new(false));
    let (pending, confirmation) = {
        let mut lifecycle = state.git_clone.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git clone state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.active.is_some() {
            return Err(native_command_error(
                "cloneBusy",
                "A Git clone operation is already running.".to_owned(),
            ));
        }
        let pending = lifecycle.validated.take().ok_or_else(|| {
            native_command_error(
                "validationMissing",
                "Validate the Git Workspace before completing the clone.".to_owned(),
            )
        })?;
        let confirmation = git_clone_completion_confirmation(pending.validation.remote_url());
        lifecycle.active = Some(Arc::clone(&active));
        lifecycle.finalizing = false;
        lifecycle.revoke_on_completion = false;
        (pending, confirmation)
    };

    let confirmed = rfd::MessageDialog::new()
        .set_parent(&window)
        .set_level(rfd::MessageLevel::Info)
        .set_title("Complete Git Clone?")
        .set_description(confirmation)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes;
    {
        let mut lifecycle = state.git_clone.lock().map_err(|_| {
            native_command_error(
                "sessionUnavailable",
                "Git clone state is unavailable".to_owned(),
            )
        })?;
        if lifecycle.finalizing || !active_git_clone_matches(&lifecycle, &active) {
            return Err(native_command_error(
                "sessionUnavailable",
                "Git clone completion state changed unexpectedly. Reopen the destination to recover."
                    .to_owned(),
            ));
        }
        if !confirmed {
            lifecycle.active = None;
            lifecycle.validated = Some(pending);
            return Ok(GitCloneCompleteResultPayload::Declined);
        }
        if active.load(Ordering::Relaxed) || lifecycle.revoke_on_completion {
            lifecycle.active = None;
            lifecycle.revoke_on_completion = false;
            lifecycle.validated = Some(pending);
            return Err(native_command_error(
                "cancelled",
                "Clone completion was cancelled before the selected folder was changed.".to_owned(),
            ));
        }
        lifecycle.finalizing = true;
    }

    let pending_holder = Arc::new(Mutex::new(Some(pending)));
    let worker_pending = Arc::clone(&pending_holder);
    let operation = tauri::async_runtime::spawn_blocking(move || {
        let pending = worker_pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let pending = pending
            .as_ref()
            .expect("native clone completion retains its pending session");
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            apply_git_clone_repository_finalization(
                &pending.selected.target,
                &pending.validation,
                &state_home,
                runtime_directory.as_deref(),
            )
        }))
    })
    .await;
    let pending = pending_holder
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .ok_or_else(|| {
            native_command_error(
                "sessionUnavailable",
                "Validated Git clone state is unavailable. Reopen the destination to recover."
                    .to_owned(),
            )
        })?;
    let mut lifecycle = state.git_clone.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git clone state is unavailable".to_owned(),
        )
    })?;
    if !lifecycle.finalizing || !active_git_clone_matches(&lifecycle, &active) {
        return Err(native_command_error(
            "sessionUnavailable",
            "Git clone completion state changed unexpectedly. Reopen the destination to recover."
                .to_owned(),
        ));
    }
    lifecycle.active = None;
    lifecycle.finalizing = false;

    let result = match operation {
        Ok(result) => result,
        Err(_) => {
            lifecycle.validated = Some(pending);
            return Err(native_command_error(
                "sessionUnavailable",
                "Git clone completion stopped unexpectedly. Try completing it again.".to_owned(),
            ));
        }
    };

    let view = match result {
        Ok(Ok(view)) => view,
        Ok(Err(error)) => {
            lifecycle.validated = Some(pending);
            return Err(native_command_error(
                git_clone_materialize_code(error),
                git_clone_materialize_message(error),
            ));
        }
        Err(_) => {
            lifecycle.validated = Some(pending);
            return Err(native_command_error(
                "sessionUnavailable",
                "Git clone completion stopped unexpectedly. Try completing it again.".to_owned(),
            ));
        }
    };
    let facts = pending.selected.facts.after_git_initialize();
    let workspace = DesktopWorkspace::new(view, facts, state.next_watch_session(), &app);
    let payload = workspace.payload();
    let mut workspace_session = match state.workspace.lock() {
        Ok(session) => session,
        Err(_) => {
            lifecycle.validated = Some(pending);
            return Err(native_command_error(
                "sessionUnavailable",
                "Workspace session is unavailable. Try completing the clone again.".to_owned(),
            ));
        }
    };
    *workspace_session = Some(workspace);
    lifecycle.validated = None;
    Ok(GitCloneCompleteResultPayload::Completed { workspace: payload })
}

#[tauri::command]
fn git_clone_cancel_active(
    state: tauri::State<'_, DesktopState>,
) -> Result<GitCloneCancelResultPayload, NativeCommandError> {
    let cancellation_requested =
        request_git_clone_cancellation(&state.git_clone).map_err(|()| {
            native_command_error(
                "sessionUnavailable",
                "Git clone state is unavailable".to_owned(),
            )
        })?;
    Ok(GitCloneCancelResultPayload {
        cancellation_requested,
    })
}

fn request_git_clone_cancellation(lifecycle: &Mutex<GitCloneLifecycle>) -> Result<bool, ()> {
    let lifecycle = lifecycle.lock().map_err(|_| ())?;
    if lifecycle.finalizing {
        return Ok(false);
    }
    Ok(lifecycle.active.as_ref().is_some_and(|cancelled| {
        cancelled.store(true, Ordering::Relaxed);
        true
    }))
}

fn clear_active_git_clone(state: &DesktopState, expected: &Arc<AtomicBool>) {
    if let Ok(mut lifecycle) = state.git_clone.lock()
        && active_git_clone_matches(&lifecycle, expected)
    {
        lifecycle.active = None;
        lifecycle.revoke_on_completion = false;
        lifecycle.finalizing = false;
    }
}

fn active_git_clone_matches(lifecycle: &GitCloneLifecycle, expected: &Arc<AtomicBool>) -> bool {
    lifecycle
        .active
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, expected))
}

fn restore_selected_after_clone(
    state: &DesktopState,
    expected: &Arc<AtomicBool>,
    selected: PendingWorkspace,
) -> Result<(), NativeCommandError> {
    let mut lifecycle = state.git_clone.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git clone state is unavailable; choose the destination again.".to_owned(),
        )
    })?;
    if !active_git_clone_matches(&lifecycle, expected) {
        return Err(native_command_error(
            "sessionUnavailable",
            "Git clone state changed unexpectedly. Choose the destination again.".to_owned(),
        ));
    }
    let mut pending = state.pending_empty.lock().map_err(|_| {
        native_command_error(
            "selectionUnavailable",
            "Workspace selection is unavailable; choose the destination again.".to_owned(),
        )
    })?;
    if pending.is_some() {
        return Err(native_command_error(
            "selectionChanged",
            "The Workspace selection changed while clone validation was running.".to_owned(),
        ));
    }
    if !lifecycle.revoke_on_completion {
        *pending = Some(selected);
    }
    lifecycle.active = None;
    lifecycle.revoke_on_completion = false;
    lifecycle.finalizing = false;
    Ok(())
}

fn git_clone_confirmation(plan: &GitClonePlan) -> String {
    git_clone_confirmation_fields(plan.transport().as_str(), plan.remote_url())
}

fn git_clone_confirmation_fields(transport: &str, remote_url: &str) -> String {
    format!(
        "Download and validate this {transport} Rehow Workspace?\n\n{remote_url}\n\nRehow will use private staging only. It will not change the selected folder yet."
    )
}

fn git_clone_completion_confirmation(remote_url: &str) -> String {
    format!(
        "Rehow validated this Git Workspace:\n\n{remote_url}\n\nComplete the clone now? This writes only the validated Workspace and exact Git metadata into the selected folder. Rehow will not run Guide commands, hooks, or a checkout."
    )
}

fn git_clone_apply_code(error: GitCloneApplyError) -> &'static str {
    match error {
        GitCloneApplyError::UnsupportedTransport => "unsupportedTransport",
        GitCloneApplyError::DestinationChanged => "destinationChanged",
        GitCloneApplyError::Cancelled => "cancelled",
        GitCloneApplyError::Timeout => "timeout",
        GitCloneApplyError::ResourceLimit => "resourceLimit",
        GitCloneApplyError::GitUnavailable => "gitUnavailable",
        GitCloneApplyError::SandboxUnavailable => "sandboxUnavailable",
        GitCloneApplyError::RemoteUnavailable => "remoteUnavailable",
        GitCloneApplyError::AuthenticationFailed => "authenticationFailed",
        GitCloneApplyError::HostKeyRejected => "hostKeyRejected",
        GitCloneApplyError::InvalidRepository => "invalidRepository",
        GitCloneApplyError::InvalidWorkspace => "invalidWorkspace",
        GitCloneApplyError::WorkspaceIssues => "workspaceIssues",
        GitCloneApplyError::StagingConflict => "stagingConflict",
        GitCloneApplyError::Unsafe => "unsafeStaging",
        GitCloneApplyError::Unsupported => "unsupportedFilesystem",
        GitCloneApplyError::Io => "io",
    }
}

fn git_clone_apply_message(error: GitCloneApplyError) -> String {
    match error {
        GitCloneApplyError::UnsupportedTransport => {
            "This Git transport is not supported. Use canonical HTTPS or SSH.".to_owned()
        }
        GitCloneApplyError::DestinationChanged => {
            "The selected folder changed. Choose the empty destination again.".to_owned()
        }
        GitCloneApplyError::Cancelled => {
            "Clone validation was cancelled. The selected folder was not changed.".to_owned()
        }
        GitCloneApplyError::Timeout => {
            "The remote did not finish within Rehow’s safety timeout.".to_owned()
        }
        GitCloneApplyError::ResourceLimit => {
            "The remote repository exceeds Rehow’s safe clone limits.".to_owned()
        }
        GitCloneApplyError::GitUnavailable => {
            "The trusted system Git executable is unavailable.".to_owned()
        }
        GitCloneApplyError::SandboxUnavailable => {
            "The required Bubblewrap clone sandbox is unavailable.".to_owned()
        }
        GitCloneApplyError::RemoteUnavailable => {
            "The remote could not be reached. Check the address, network, and access.".to_owned()
        }
        GitCloneApplyError::AuthenticationFailed => {
            "Remote authentication failed. Check the credential helper or SSH agent configured for this remote."
                .to_owned()
        }
        GitCloneApplyError::HostKeyRejected => {
            "SSH host verification failed. Verify and add the host key with your normal SSH tools, then restart Rehow."
                .to_owned()
        }
        GitCloneApplyError::InvalidRepository => {
            "The remote does not contain a safe supported Git snapshot.".to_owned()
        }
        GitCloneApplyError::InvalidWorkspace => {
            "The remote snapshot is not a compatible Rehow Workspace.".to_owned()
        }
        GitCloneApplyError::WorkspaceIssues => {
            "The remote Workspace has validation issues and was not accepted.".to_owned()
        }
        GitCloneApplyError::StagingConflict => {
            "Recognized or conflicting clone staging already exists. Rehow preserved it.".to_owned()
        }
        GitCloneApplyError::Unsafe => {
            "Clone staging became unsafe. Rehow preserved it and changed no destination files."
                .to_owned()
        }
        GitCloneApplyError::Unsupported => {
            "This filesystem cannot provide the durable clone staging Rehow requires.".to_owned()
        }
        GitCloneApplyError::Io => {
            "Clone validation could not finish. Check system storage and try again.".to_owned()
        }
    }
}

fn git_ssh_transport_code(error: GitSshTransportError) -> &'static str {
    match error {
        GitSshTransportError::AgentUnavailable => "sshAgentUnavailable",
        GitSshTransportError::HostKeysUnavailable => "sshHostKeysUnavailable",
        GitSshTransportError::ProxyUnavailable => "sshHelperUnavailable",
        GitSshTransportError::SshUnavailable => "sshUnavailable",
        GitSshTransportError::ResourceLimit => "resourceLimit",
        GitSshTransportError::Io => "io",
    }
}

fn git_ssh_transport_message(error: GitSshTransportError) -> String {
    match error {
        GitSshTransportError::AgentUnavailable => {
            "No safe SSH agent was available when Rehow started. Start or unlock your normal agent, then restart Rehow."
                .to_owned()
        }
        GitSshTransportError::HostKeysUnavailable => {
            "No safe trusted SSH host-key snapshot was available when Rehow started. Verify the host with your normal SSH tools, then restart Rehow."
                .to_owned()
        }
        GitSshTransportError::ProxyUnavailable => {
            "Rehow’s authenticated Git helper is missing or unsafe. Reinstall this Rehow build."
                .to_owned()
        }
        GitSshTransportError::SshUnavailable => {
            "The trusted system SSH client is unavailable.".to_owned()
        }
        GitSshTransportError::ResourceLimit => {
            "The trusted SSH host-key files exceed Rehow’s safety limit.".to_owned()
        }
        GitSshTransportError::Io => {
            "Rehow could not prepare its sealed SSH resources. Restart Rehow and try again."
                .to_owned()
        }
    }
}

fn git_https_credential_code(error: GitHttpsCredentialResourceError) -> &'static str {
    match error {
        GitHttpsCredentialResourceError::CredentialsUnavailable => "httpsCredentialsUnavailable",
        GitHttpsCredentialResourceError::ProxyUnavailable => "httpsCredentialHelperUnavailable",
        GitHttpsCredentialResourceError::BackingUnavailable => "httpsCredentialBackingUnavailable",
        GitHttpsCredentialResourceError::Io => "io",
    }
}

fn git_https_credential_message(error: GitHttpsCredentialResourceError) -> String {
    match error {
        GitHttpsCredentialResourceError::CredentialsUnavailable => {
            "No safe HTTPS credential helper was available when Rehow started. Check your normal Git configuration, then restart Rehow."
                .to_owned()
        }
        GitHttpsCredentialResourceError::ProxyUnavailable => {
            "Rehow’s HTTPS credential proxy is missing or unsafe. Reinstall this Rehow build."
                .to_owned()
        }
        GitHttpsCredentialResourceError::BackingUnavailable => {
            "The configured GitHub CLI helper needs a private hosts.yml file that was unavailable when Rehow started. Sign in with gh, protect that file, then restart Rehow."
                .to_owned()
        }
        GitHttpsCredentialResourceError::Io => {
            "Rehow could not prepare its confined HTTPS credential resources. Restart Rehow and try again."
                .to_owned()
        }
    }
}

fn git_clone_materialize_code(error: GitCloneMaterializeError) -> &'static str {
    match error {
        GitCloneMaterializeError::DestinationChanged => "destinationChanged",
        GitCloneMaterializeError::StagingChanged => "stagingChanged",
        GitCloneMaterializeError::Busy => "busy",
        GitCloneMaterializeError::Conflict => "conflict",
        GitCloneMaterializeError::Unsafe => "unsafeState",
        GitCloneMaterializeError::Unsupported => "unsupportedFilesystem",
        GitCloneMaterializeError::ResourceLimit => "resourceLimit",
        GitCloneMaterializeError::ValidationMismatch => "validationMismatch",
        GitCloneMaterializeError::Io => "io",
    }
}

fn git_clone_materialize_message(error: GitCloneMaterializeError) -> String {
    match error {
        GitCloneMaterializeError::DestinationChanged => {
            "The clone destination changed. Reopen it before trying to recover.".to_owned()
        }
        GitCloneMaterializeError::StagingChanged => {
            "The validated clone staging data changed. Validate the remote again.".to_owned()
        }
        GitCloneMaterializeError::Busy => {
            "Another Rehow writer is active. Wait a moment and try again.".to_owned()
        }
        GitCloneMaterializeError::Conflict => {
            "The destination contains unexpected data. Nothing was overwritten.".to_owned()
        }
        GitCloneMaterializeError::Unsafe => {
            "The destination or staging area contains unsafe filesystem state.".to_owned()
        }
        GitCloneMaterializeError::Unsupported => {
            "This filesystem cannot provide the safe clone publication Rehow requires.".to_owned()
        }
        GitCloneMaterializeError::ResourceLimit => {
            "Clone completion exceeded a safe resource limit.".to_owned()
        }
        GitCloneMaterializeError::ValidationMismatch => {
            "The completed repository did not match the validated Workspace.".to_owned()
        }
        GitCloneMaterializeError::Io => {
            "Clone completion could not finish. Check folder access and try again.".to_owned()
        }
    }
}

fn git_clone_code(error: GitCloneError) -> &'static str {
    match error {
        GitCloneError::InvalidUrl => "invalidRemoteUrl",
        GitCloneError::DestinationChanged => "destinationChanged",
        GitCloneError::ReadOnly => "readOnly",
        GitCloneError::NestedRepository => "nestedRepository",
        GitCloneError::Unsafe => "unsafeDestination",
        GitCloneError::Io => "io",
    }
}

fn git_clone_message(error: GitCloneError) -> String {
    match error {
        GitCloneError::InvalidUrl => {
            "Use a secret-free HTTPS or SSH repository address without a password, token, query, fragment, space, or encoded path."
                .to_owned()
        }
        GitCloneError::DestinationChanged => {
            "That folder changed after selection. Choose the empty folder again.".to_owned()
        }
        GitCloneError::ReadOnly => {
            "That folder is read-only. Choose a writable folder.".to_owned()
        }
        GitCloneError::NestedRepository => {
            "That folder is inside another Git repository. Choose a destination outside it."
                .to_owned()
        }
        GitCloneError::Unsafe => {
            "That destination is unsafe or no longer empty. Existing content was not changed."
                .to_owned()
        }
        GitCloneError::Io => {
            "Rehow could not inspect the clone destination. Check folder access and try again."
                .to_owned()
        }
    }
}

fn git_remote_confirmation_fields(remote_name: &str, transport: &str, remote_url: &str) -> String {
    format!(
        "Add remote “{}” using this {} address?\n\n{}\n\nThis updates Git config only. Rehow will not connect, authenticate, fetch, or synchronize.",
        remote_name,
        transport.to_uppercase(),
        remote_url
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingGitPlanError {
    Missing,
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitSyncCancelError {
    Busy,
    Unavailable,
}

fn take_matching_git_plan<T>(
    pending: &mut Option<(PlanToken, T)>,
    token: &PlanToken,
) -> Result<T, PendingGitPlanError> {
    let Some((expected, _)) = pending.as_ref() else {
        return Err(PendingGitPlanError::Missing);
    };
    if expected != token {
        return Err(PendingGitPlanError::Invalid);
    }
    Ok(pending.take().expect("checked pending Git plan").1)
}

fn request_git_sync_cancellation(
    lifecycle: &Mutex<GitSyncLifecycle>,
) -> Result<bool, GitSyncCancelError> {
    let mut lifecycle = lifecycle
        .lock()
        .map_err(|_| GitSyncCancelError::Unavailable)?;
    if lifecycle.applying {
        return Err(GitSyncCancelError::Busy);
    }
    if let Some(cancelled) = lifecycle.active.as_ref() {
        cancelled.store(true, Ordering::Relaxed);
        lifecycle.revoke_on_completion = true;
        return Ok(true);
    }
    Ok(lifecycle.classified.take().is_some())
}

fn git_push_ancestry_allowed(ancestry: GitSyncAncestry) -> bool {
    ancestry == GitSyncAncestry::LocalAhead
}

fn git_sync_blocks_workspace_switch(lifecycle: &GitSyncLifecycle) -> bool {
    lifecycle.active.is_some() || lifecycle.applying
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
        AttachmentRemovalError::Cancelled => "cancelled",
        AttachmentRemovalError::UnsafeRoundTrip => "unsafeRoundTrip",
        AttachmentRemovalError::Busy => "busy",
        AttachmentRemovalError::UnsafeStorage => "unsafeStorage",
        AttachmentRemovalError::UnsupportedFilesystem => "unsupportedFilesystem",
        AttachmentRemovalError::ResourceLimit => "resourceLimit",
        AttachmentRemovalError::Io => "io",
        AttachmentRemovalError::RecoveryRequired { .. } => "recoveryRequired",
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
        AttachmentRemovalError::Cancelled => "Attachment removal was cancelled.".to_owned(),
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
        AttachmentRemovalError::RecoveryRequired { operation_id } => format!(
            "Attachment removal was interrupted after a recoverable change. Reopen the Workspace and inspect recovery operation {operation_id}."
        ),
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
    let mut lifecycle = state.git_clone.lock().map_err(|_| {
        native_command_error(
            "sessionUnavailable",
            "Git clone state is unavailable".to_owned(),
        )
    })?;
    if lifecycle.active.is_some() {
        return Err(native_command_error(
            "cloneBusy",
            "Wait for the active Git clone operation before creating a Workspace.".to_owned(),
        ));
    }
    if lifecycle.validated.is_some() {
        return Err(native_command_error(
            "completionPending",
            "Complete or cancel the validated clone before creating a Workspace.".to_owned(),
        ));
    }
    lifecycle.pending = None;
    lifecycle.validated = None;
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
    drop(lifecycle);
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
        CategoryReorderError::Cancelled => "cancelled",
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
        CategoryReorderError::Cancelled => "The Category reorder was cancelled.".to_owned(),
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
        CategoryRemoveError::Cancelled => "cancelled",
        CategoryRemoveError::RevisionConflict => "revisionConflict",
        CategoryRemoveError::Busy => "busy",
        CategoryRemoveError::UnsafeStorage => "unsafeStorage",
        CategoryRemoveError::UnsupportedFilesystem => "unsupportedFilesystem",
        CategoryRemoveError::ResourceLimit => "resourceLimit",
        CategoryRemoveError::Io => "io",
        CategoryRemoveError::RecoveryRequired { .. } => "recoveryRequired",
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
        CategoryRemoveError::Cancelled => "Category removal was cancelled before writing.".to_owned(),
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
        CategoryRemoveError::RecoveryRequired { operation_id } => format!(
            "Category removal was interrupted after its descriptor moved to Rehow Trash. Preserve the Workspace and recover operation {operation_id} before editing it."
        ),
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
    let mut lifecycle = state
        .git_clone
        .lock()
        .map_err(|_| "Git clone state is unavailable".to_owned())?;
    if lifecycle.finalizing {
        return Err(
            "Git clone completion cannot be cancelled safely. Wait for it to finish.".to_owned(),
        );
    }
    lifecycle.pending = None;
    lifecycle.validated = None;
    if lifecycle.active.as_ref().is_some_and(|cancelled| {
        cancelled.store(true, Ordering::Relaxed);
        true
    }) {
        lifecycle.revoke_on_completion = true;
        return Ok(());
    }
    *state
        .pending_empty
        .lock()
        .map_err(|_| "Workspace selection is unavailable".to_owned())? = None;
    *state
        .pending_markdown_import
        .lock()
        .map_err(|_| "Markdown import selection is unavailable".to_owned())? = None;
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

    {
        let mut lifecycle = state
            .git_sync
            .lock()
            .map_err(|_| "Git sync state is unavailable".to_owned())?;
        if git_sync_blocks_workspace_switch(&lifecycle) {
            return Err(
                "Wait for the active Git sync operation before choosing another Workspace."
                    .to_owned(),
            );
        }
        lifecycle.classified = None;
    }

    let (report_facts, inspection) = inspect_workspace(&path)
        .map_err(|error| error.to_string())?
        .into_parts();
    let facts = FolderFactsPayload::from(report_facts);
    let mut lifecycle = state
        .git_clone
        .lock()
        .map_err(|_| "Git clone state is unavailable".to_owned())?;
    if lifecycle.active.is_some() {
        return Err(
            "Wait for the active Git clone operation before choosing another folder.".to_owned(),
        );
    }
    lifecycle.pending = None;
    lifecycle.validated = None;
    *state
        .pending_empty
        .lock()
        .map_err(|_| "Workspace selection is unavailable".to_owned())? = None;
    *state
        .pending_markdown_import
        .lock()
        .map_err(|_| "Markdown import selection is unavailable".to_owned())? = None;
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
        WorkspaceInspection::OrdinaryMarkdown(source) => {
            let preview = source.preview().map_err(|error| error.to_string())?;
            *state
                .workspace
                .lock()
                .map_err(|_| "Workspace session is unavailable".to_owned())? = None;
            *state
                .pending_markdown_import
                .lock()
                .map_err(|_| "Markdown import selection is unavailable".to_owned())? = Some(source);
            Ok(OpenWorkspaceResult::OrdinaryMarkdown {
                facts,
                preview: preview.into(),
            })
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
            git_initialize_plan_open_workspace,
            git_initialize_apply_open_workspace,
            git_remote_plan_open_workspace,
            git_remote_apply_open_workspace,
            git_commit_plan_open_workspace,
            git_commit_cancel_open_workspace,
            git_commit_apply_open_workspace,
            git_commit_recover_open_workspace,
            git_sync_inspect_open_workspace,
            git_sync_cancel_active,
            git_sync_apply_open_workspace,
            git_push_apply_open_workspace,
            git_sync_recover_open_workspace,
            git_clone_plan_selected_workspace,
            git_clone_apply_selected_workspace,
            git_clone_complete_selected_workspace,
            git_clone_cancel_active,
            git_status_open_workspace,
            search_open_workspace
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rehow");
}

#[cfg(test)]
mod tests {
    use super::{
        APPLICATION_NAME, CategoryPayload, DesktopState, FolderFactsPayload,
        GitCloneApplyResultPayload, GitCloneCancelResultPayload, GitCloneCompleteResultPayload,
        GitCloneLifecycle, GitClonePlanPayload, GitCommitCancelResultPayload, GitCommitPlanPayload,
        GitCommitRecoveryResultPayload, GitCommitResultPayload, GitInitPlanPayload,
        GitInitResultPayload, GitPushResultPayload, GitRemotePlanPayload, GitRemoteResultPayload,
        GitSyncApplyResultPayload, GitSyncCancelError, GitSyncCancelResultPayload,
        GitSyncInspectionPayload, GitSyncLifecycle, GitSyncRecoveryResultPayload, GuideBodyPayload,
        GuidePayload, MarkdownImportPreviewEntryPayload, MarkdownImportPreviewPayload,
        PendingGitPlanError, SearchHighlightPayload, SearchHitPayload, WorkspacePayload,
        WorkspaceWatchEventPayload, attachment_removal_code, attachment_removal_message,
        category_create_code, category_create_message, category_remove_code,
        category_remove_message, category_reorder_code, category_reorder_message,
        category_update_code, category_update_message, empty_git_payload, git_clone_apply_code,
        git_clone_apply_message, git_clone_code, git_clone_completion_confirmation,
        git_clone_confirmation_fields, git_clone_materialize_code, git_clone_materialize_message,
        git_clone_message, git_commit_code, git_commit_confirmation_fields, git_commit_message,
        git_commit_message_input_allowed, git_commit_recovery_confirmation, git_init_code,
        git_init_confirmation, git_init_message, git_push_ancestry_allowed,
        git_push_confirmation_fields, git_remote_code, git_remote_confirmation_fields,
        git_remote_message, git_ssh_transport_code, git_ssh_transport_message, git_sync_ancestry,
        git_sync_blocks_workspace_switch, git_sync_code, git_sync_confirmation_fields,
        git_sync_message, git_sync_recovery_confirmation, guide_create_code, guide_create_message,
        guide_remove_code, guide_remove_confirmation, guide_remove_message, guide_update_code,
        guide_update_message, native_command_error, request_git_clone_cancellation,
        request_git_sync_cancellation, search_status_scope, take_matching_git_plan,
        workspace_initialization_message, workspace_refresh_code, workspace_refresh_message,
    };
    use rehow_core::{
        application::{
            AttachmentRemovalError, CategoryCreateError, CategoryRemoveError, CategoryReorderError,
            CategoryUpdateError, GuideCreateError, GuideRemoveError, GuideUpdateError,
            WorkspaceInitializationError, WorkspaceRefreshError,
        },
        git::{
            GitCloneApplyError, GitCloneError, GitCloneMaterializeError, GitCloneValidation,
            GitCommitPlanError, GitHttpsCredentialBackingStatus, GitHttpsCredentialProxyStatus,
            GitHttpsCredentialStatus, GitInitError, GitRemoteError, GitSshTransportError,
            GitSyncAncestry, GitSyncError,
        },
        markdown::parse_safe_markdown,
        protocol::{OperationId, PlanToken},
    };
    use std::{
        str::FromStr,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };

    #[test]
    fn application_name_is_stable() {
        assert_eq!(APPLICATION_NAME, "Rehow");
    }

    #[test]
    fn markdown_import_preview_payload_is_closed_and_pathless() {
        let value = serde_json::to_value(MarkdownImportPreviewPayload {
            included: 1,
            normalization_required: 1,
            excluded_non_markdown: 2,
            malformed_or_unsafe: 3,
            oversized: 4,
            aggregate_bytes: "12".to_owned(),
            entries: vec![MarkdownImportPreviewEntryPayload {
                display_path: "notes/linux.md".to_owned(),
                size: "12".to_owned(),
                normalization_required: true,
            }],
        })
        .unwrap();

        assert_eq!(value.as_object().unwrap().len(), 7);
        assert_eq!(value["entries"][0].as_object().unwrap().len(), 3);
        assert!(value.get("sourcePath").is_none());
        assert!(value.get("destinationPath").is_none());
        assert!(!value.to_string().contains("/home/"));
    }

    #[test]
    fn https_credential_provenance_is_retained_only_in_native_state() {
        let state = DesktopState::default();
        assert!(matches!(
            state.https_credential_provenance.status(),
            GitHttpsCredentialStatus::NotConfigured
                | GitHttpsCredentialStatus::Trusted
                | GitHttpsCredentialStatus::Unsafe
        ));
        let debug = format!("{:?}", state.https_credential_provenance);
        assert!(!debug.contains('/'));
        assert!(!debug.contains("helper ="));
        assert!(matches!(
            state.https_credential_proxy_provenance.status(),
            GitHttpsCredentialProxyStatus::NotConfigured
                | GitHttpsCredentialProxyStatus::Trusted
                | GitHttpsCredentialProxyStatus::Unsafe
        ));
        assert!(!format!("{:?}", state.https_credential_proxy_provenance).contains('/'));
        assert!(matches!(
            state.https_credential_backing_provenance.status(),
            GitHttpsCredentialBackingStatus::NotConfigured
                | GitHttpsCredentialBackingStatus::Trusted
                | GitHttpsCredentialBackingStatus::Unsafe
        ));
        assert!(!format!("{:?}", state.https_credential_backing_provenance).contains('/'));
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
    fn git_initialization_payload_and_errors_are_closed() {
        let token = PlanToken::from_str(&format!("plan:v1:{}", "A".repeat(43))).unwrap();
        let plan = serde_json::to_value(GitInitPlanPayload {
            plan_token: token,
            initial_branch: "main",
        })
        .unwrap();
        assert_eq!(plan.as_object().unwrap().len(), 2);
        assert_eq!(plan["initialBranch"], "main");
        assert!(plan.get("path").is_none());
        let result = serde_json::to_value(GitInitResultPayload { initialized: true }).unwrap();
        assert_eq!(result.as_object().unwrap().len(), 1);
        assert_eq!(git_init_code(GitInitError::PlanStale), "planStale");
        assert!(git_init_message(GitInitError::Unsafe).contains("not overwritten"));
        assert!(git_init_confirmation().contains("will not commit"));
    }

    #[test]
    fn git_remote_payload_and_errors_are_closed_and_url_free() {
        let token = PlanToken::from_str(&format!("plan:v1:{}", "A".repeat(43))).unwrap();
        let plan = serde_json::to_value(GitRemotePlanPayload {
            plan_token: token,
            remote_name: "origin".to_owned(),
            transport: "https",
            host: "example.invalid".to_owned(),
        })
        .unwrap();
        assert_eq!(plan.as_object().unwrap().len(), 4);
        assert!(plan.get("url").is_none());
        assert!(plan.get("path").is_none());
        assert!(!plan.to_string().contains("token="));
        let result = serde_json::to_value(GitRemoteResultPayload { connected: false }).unwrap();
        assert_eq!(result.as_object().unwrap().len(), 1);
        assert_eq!(result["connected"], false);
        assert_eq!(
            git_remote_code(GitRemoteError::InvalidUrl),
            "invalidRemoteUrl"
        );
        assert!(!git_remote_message(GitRemoteError::InvalidUrl).contains("https://"));
        assert!(!git_remote_message(GitRemoteError::PlanStale).contains("config.lock"));
        let confirmation = git_remote_confirmation_fields(
            "origin",
            "https",
            "https://example.invalid/team/rehow.git",
        );
        assert!(confirmation.contains("origin"));
        assert!(confirmation.contains("https://example.invalid/team/rehow.git"));
        assert!(confirmation.contains("will not connect"));
    }

    #[test]
    fn git_commit_payloads_confirmation_and_errors_are_closed() {
        let token = PlanToken::from_str(&format!("plan:v1:{}", "A".repeat(43))).unwrap();
        let plan = serde_json::to_value(GitCommitPlanPayload {
            plan_token: token,
            branch: "main".to_owned(),
            created: 2,
            updated: 3,
            deleted: 1,
            unrelated_changes: 4,
        })
        .unwrap();
        assert_eq!(plan.as_object().unwrap().len(), 6);
        assert_eq!(plan["branch"], "main");
        assert!(plan.get("path").is_none());
        assert!(plan.get("objectId").is_none());
        assert!(plan.get("operationId").is_none());
        let result = serde_json::to_value(GitCommitResultPayload {
            committed: true,
            workspace_changed: false,
        })
        .unwrap();
        assert_eq!(result.as_object().unwrap().len(), 2);
        let cancelled =
            serde_json::to_value(GitCommitCancelResultPayload { cancelled: true }).unwrap();
        assert_eq!(cancelled.as_object().unwrap().len(), 1);
        let recovered = serde_json::to_value(GitCommitRecoveryResultPayload {
            recovered: true,
            workspace_changed: true,
        })
        .unwrap();
        assert_eq!(recovered.as_object().unwrap().len(), 2);

        let confirmation = git_commit_confirmation_fields("Update guides", "main", 2, 3, 1, 4);
        assert!(confirmation.contains("Update guides"));
        assert!(confirmation.contains("main"));
        assert!(confirmation.contains("2 created, 3 updated, 1 deleted"));
        assert!(confirmation.contains("4; they remain uncommitted"));
        assert!(confirmation.contains("will not push"));
        assert!(confirmation.contains("sign"));
        assert!(git_commit_recovery_confirmation().contains("exact recovery record"));
        assert!(git_commit_recovery_confirmation().contains("will not push"));
        assert!(git_commit_message_input_allowed(&"x".repeat(800)));
        assert!(!git_commit_message_input_allowed(&"x".repeat(801)));

        for (error, code) in [
            (GitCommitPlanError::Cancelled, "cancelled"),
            (GitCommitPlanError::InvalidMessage, "invalidMessage"),
            (GitCommitPlanError::InvalidIdentity, "invalidIdentity"),
            (GitCommitPlanError::WorkspaceIssues, "workspaceIssues"),
            (GitCommitPlanError::ReadOnly, "readOnly"),
            (GitCommitPlanError::NotRepository, "notRepository"),
            (GitCommitPlanError::UnsafeRepository, "unsafeRepository"),
            (
                GitCommitPlanError::UnsupportedRepository,
                "unsupportedRepository",
            ),
            (GitCommitPlanError::OperationInProgress, "recoveryRequired"),
            (GitCommitPlanError::UnsupportedIndex, "unsupportedIndex"),
            (
                GitCommitPlanError::AmbiguousManagedHistory,
                "ambiguousHistory",
            ),
            (GitCommitPlanError::PathCollision, "pathCollision"),
            (GitCommitPlanError::NothingToCommit, "nothingToCommit"),
            (GitCommitPlanError::ResourceLimit, "resourceLimit"),
            (GitCommitPlanError::PlanStale, "planStale"),
            (
                GitCommitPlanError::RecoveryNotRequired,
                "recoveryNotRequired",
            ),
            (GitCommitPlanError::Io, "io"),
        ] {
            assert_eq!(git_commit_code(error), code);
            assert!(!git_commit_message(error).is_empty());
        }
    }

    #[test]
    fn git_sync_payloads_confirmation_and_errors_are_closed() {
        let token = PlanToken::from_str(&format!("plan:v1:{}", "A".repeat(43))).unwrap();
        let inspection = serde_json::to_value(GitSyncInspectionPayload {
            plan_token: token,
            remote_name: "origin".to_owned(),
            transport: "https",
            host: "example.invalid".to_owned(),
            ancestry: "remoteFastForward",
            local_changes: false,
            workspace_matches_remote: false,
        })
        .unwrap();
        assert_eq!(inspection.as_object().unwrap().len(), 7);
        assert!(inspection.get("url").is_none());
        assert!(inspection.get("path").is_none());
        assert!(inspection.get("ref").is_none());
        assert!(inspection.get("objectId").is_none());
        let cancelled = serde_json::to_value(GitSyncCancelResultPayload {
            cancellation_requested: true,
        })
        .unwrap();
        assert_eq!(
            cancelled,
            serde_json::json!({ "cancellationRequested": true })
        );
        assert_eq!(
            serde_json::to_value(GitSyncApplyResultPayload::Declined).unwrap(),
            serde_json::json!({ "kind": "declined" })
        );
        assert_eq!(
            serde_json::to_value(GitSyncRecoveryResultPayload::Declined).unwrap(),
            serde_json::json!({ "kind": "declined" })
        );
        assert_eq!(
            serde_json::to_value(GitPushResultPayload::Completed { pushed: true }).unwrap(),
            serde_json::json!({ "kind": "completed", "pushed": true })
        );
        for (ancestry, value) in [
            (GitSyncAncestry::UpToDate, "upToDate"),
            (GitSyncAncestry::LocalAhead, "localAhead"),
            (GitSyncAncestry::RemoteFastForward, "remoteFastForward"),
            (GitSyncAncestry::Diverged, "diverged"),
            (GitSyncAncestry::HistoryLimit, "historyLimit"),
        ] {
            assert_eq!(git_sync_ancestry(ancestry), value);
        }
        let confirmation = git_sync_confirmation_fields("origin", "https", "example.invalid");
        assert!(confirmation.contains("origin"));
        assert!(confirmation.contains("HTTPS"));
        assert!(confirmation.contains("example.invalid"));
        assert!(confirmation.contains("will not push"));
        assert!(!confirmation.contains("refs/"));
        let push_confirmation =
            git_push_confirmation_fields("origin", "main", "https", "example.invalid");
        assert!(push_confirmation.contains("origin/main"));
        assert!(push_confirmation.contains("normal fast-forward push"));
        assert!(push_confirmation.contains("will not force"));
        assert!(!push_confirmation.contains("refs/"));
        assert!(git_sync_recovery_confirmation().contains("durable recovery record"));
        assert!(git_sync_recovery_confirmation().contains("will not push"));

        for (error, code) in [
            (GitSyncError::NotRepository, "notRepository"),
            (GitSyncError::NoUpstream, "noUpstream"),
            (GitSyncError::InvalidConfiguration, "invalidConfiguration"),
            (GitSyncError::UnsupportedTransport, "unsupportedTransport"),
            (GitSyncError::RecoveryRequired, "recoveryRequired"),
            (GitSyncError::ReadOnly, "readOnly"),
            (GitSyncError::WorkspaceIssues, "workspaceIssues"),
            (GitSyncError::PlanStale, "planStale"),
            (GitSyncError::Cancelled, "cancelled"),
            (GitSyncError::Timeout, "timeout"),
            (GitSyncError::ResourceLimit, "resourceLimit"),
            (GitSyncError::GitUnavailable, "gitUnavailable"),
            (GitSyncError::SandboxUnavailable, "sandboxUnavailable"),
            (GitSyncError::RemoteUnavailable, "remoteUnavailable"),
            (GitSyncError::AuthenticationFailed, "authenticationFailed"),
            (GitSyncError::HostKeyRejected, "hostKeyRejected"),
            (GitSyncError::RemoteChanged, "remoteChanged"),
            (GitSyncError::RemoteStateUncertain, "remoteStateUncertain"),
            (GitSyncError::InvalidRepository, "invalidRepository"),
            (GitSyncError::InvalidWorkspace, "invalidWorkspace"),
            (GitSyncError::StagingConflict, "stagingConflict"),
            (GitSyncError::Unsafe, "unsafeRepository"),
            (GitSyncError::Unsupported, "unsupported"),
            (GitSyncError::Io, "io"),
        ] {
            assert_eq!(git_sync_code(error), code);
            let message = git_sync_message(error);
            assert!(!message.is_empty());
            assert!(!message.contains("refs/"));
            assert!(!message.contains("https://"));
        }
        let authentication = git_sync_message(GitSyncError::AuthenticationFailed);
        assert!(authentication.contains("credential helper or SSH agent"));
    }

    #[test]
    fn active_sync_cancellation_revokes_inspection_and_refuses_apply() {
        let flag = Arc::new(AtomicBool::new(false));
        let active = Mutex::new(GitSyncLifecycle {
            active: Some(Arc::clone(&flag)),
            ..GitSyncLifecycle::default()
        });
        assert_eq!(request_git_sync_cancellation(&active), Ok(true));
        assert!(flag.load(Ordering::Relaxed));
        assert!(active.lock().unwrap().revoke_on_completion);
        assert!(git_sync_blocks_workspace_switch(&active.lock().unwrap()));
        assert_eq!(request_git_sync_cancellation(&active), Ok(true));

        let inactive = Mutex::new(GitSyncLifecycle::default());
        assert_eq!(request_git_sync_cancellation(&inactive), Ok(false));
        assert!(!git_sync_blocks_workspace_switch(&inactive.lock().unwrap()));

        let applying = Mutex::new(GitSyncLifecycle {
            applying: true,
            ..GitSyncLifecycle::default()
        });
        assert_eq!(
            request_git_sync_cancellation(&applying),
            Err(GitSyncCancelError::Busy)
        );
        assert!(git_sync_blocks_workspace_switch(&applying.lock().unwrap()));
        for ancestry in [
            GitSyncAncestry::UpToDate,
            GitSyncAncestry::RemoteFastForward,
            GitSyncAncestry::Diverged,
            GitSyncAncestry::HistoryLimit,
        ] {
            assert!(!git_push_ancestry_allowed(ancestry));
        }
        assert!(git_push_ancestry_allowed(GitSyncAncestry::LocalAhead));
    }

    #[test]
    fn git_clone_plan_payload_and_errors_are_closed_and_url_free() {
        let token = PlanToken::from_str(&format!("plan:v1:{}", "A".repeat(43))).unwrap();
        let plan = serde_json::to_value(GitClonePlanPayload {
            plan_token: token,
            transport: "ssh",
            host: "example.invalid".to_owned(),
        })
        .unwrap();
        assert_eq!(plan.as_object().unwrap().len(), 3);
        assert!(plan.get("url").is_none());
        assert!(plan.get("path").is_none());
        assert_eq!(
            git_clone_code(GitCloneError::DestinationChanged),
            "destinationChanged"
        );
        assert!(!git_clone_message(GitCloneError::InvalidUrl).contains("https://"));
        assert!(git_clone_message(GitCloneError::NestedRepository).contains("inside"));

        let declined = serde_json::to_value(GitCloneApplyResultPayload {
            validated: false,
            stage: "declined",
        })
        .unwrap();
        assert_eq!(declined.as_object().unwrap().len(), 2);
        assert_eq!(declined["stage"], "declined");
        assert!(declined.get("url").is_none());
        assert!(declined.get("path").is_none());
        let cancelled = serde_json::to_value(GitCloneCancelResultPayload {
            cancellation_requested: true,
        })
        .unwrap();
        assert_eq!(cancelled.as_object().unwrap().len(), 1);
        let declined = serde_json::to_value(GitCloneCompleteResultPayload::Declined).unwrap();
        assert_eq!(declined, serde_json::json!({ "kind": "declined" }));

        let confirmation =
            git_clone_confirmation_fields("https", "https://example.invalid/team/rehow.git");
        assert!(confirmation.contains("https://example.invalid/team/rehow.git"));
        assert!(confirmation.contains("will not change the selected folder"));
        let completion =
            git_clone_completion_confirmation("https://example.invalid/team/rehow.git");
        assert!(completion.contains("https://example.invalid/team/rehow.git"));
        assert!(completion.contains("writes only the validated Workspace"));
        assert!(completion.contains("will not run Guide commands"));
        assert_eq!(
            git_clone_apply_code(GitCloneApplyError::Cancelled),
            "cancelled"
        );
        assert_eq!(
            git_clone_apply_code(GitCloneApplyError::WorkspaceIssues),
            "workspaceIssues"
        );
        assert_eq!(
            git_clone_apply_code(GitCloneApplyError::AuthenticationFailed),
            "authenticationFailed"
        );
        assert_eq!(
            git_clone_apply_code(GitCloneApplyError::HostKeyRejected),
            "hostKeyRejected"
        );
        assert_eq!(
            git_ssh_transport_code(GitSshTransportError::AgentUnavailable),
            "sshAgentUnavailable"
        );
        assert_eq!(
            git_ssh_transport_code(GitSshTransportError::SshUnavailable),
            "sshUnavailable"
        );
        assert!(
            !git_clone_apply_message(GitCloneApplyError::InvalidRepository)
                .contains("example.invalid")
        );
        assert!(git_clone_apply_message(GitCloneApplyError::Cancelled).contains("not changed"));
        assert!(!git_clone_apply_message(GitCloneApplyError::AuthenticationFailed).contains('/'));
        assert!(
            git_clone_apply_message(GitCloneApplyError::AuthenticationFailed)
                .contains("credential helper or SSH agent")
        );
        assert!(!git_ssh_transport_message(GitSshTransportError::ProxyUnavailable).contains('/'));
        assert_eq!(
            git_clone_materialize_code(GitCloneMaterializeError::StagingChanged),
            "stagingChanged"
        );
        assert_eq!(
            git_clone_materialize_code(GitCloneMaterializeError::ValidationMismatch),
            "validationMismatch"
        );
        assert!(
            git_clone_materialize_message(GitCloneMaterializeError::Conflict)
                .contains("Nothing was overwritten")
        );
        assert!(!git_clone_materialize_message(GitCloneMaterializeError::Io).contains("path"));

        fn assert_send<T: Send>() {}
        assert_send::<GitCloneValidation>();
    }

    #[test]
    fn active_clone_cancellation_is_pathless_and_idempotent() {
        let flag = Arc::new(AtomicBool::new(false));
        let active = Mutex::new(GitCloneLifecycle {
            active: Some(Arc::clone(&flag)),
            ..GitCloneLifecycle::default()
        });
        assert_eq!(request_git_clone_cancellation(&active), Ok(true));
        assert!(flag.load(Ordering::Relaxed));
        assert_eq!(request_git_clone_cancellation(&active), Ok(true));

        let inactive = Mutex::new(GitCloneLifecycle::default());
        assert_eq!(request_git_clone_cancellation(&inactive), Ok(false));

        let finalizing_flag = Arc::new(AtomicBool::new(false));
        let finalizing = Mutex::new(GitCloneLifecycle {
            active: Some(Arc::clone(&finalizing_flag)),
            finalizing: true,
            ..GitCloneLifecycle::default()
        });
        assert_eq!(request_git_clone_cancellation(&finalizing), Ok(false));
        assert!(!finalizing_flag.load(Ordering::Relaxed));
    }

    #[test]
    fn git_initialization_token_is_single_use_and_mismatch_does_not_consume() {
        let expected = PlanToken::from_str(&format!("plan:v1:{}", "A".repeat(43))).unwrap();
        let wrong = PlanToken::from_str(&format!("plan:v1:{}E", "A".repeat(42))).unwrap();
        let mut pending = Some((expected.clone(), "retained plan"));
        assert_eq!(
            take_matching_git_plan(&mut pending, &wrong),
            Err(PendingGitPlanError::Invalid)
        );
        assert!(pending.is_some());
        assert_eq!(
            take_matching_git_plan(&mut pending, &expected),
            Ok("retained plan")
        );
        assert!(pending.is_none());
        assert_eq!(
            take_matching_git_plan(&mut pending, &expected),
            Err(PendingGitPlanError::Missing)
        );
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
        assert_eq!(
            category_remove_code(CategoryRemoveError::Cancelled),
            "cancelled"
        );
        let operation_id = OperationId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e20").unwrap();
        assert_eq!(
            category_remove_code(CategoryRemoveError::RecoveryRequired { operation_id }),
            "recoveryRequired"
        );
        assert!(
            category_remove_message(CategoryRemoveError::RecoveryRequired { operation_id })
                .contains(&operation_id.to_string())
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

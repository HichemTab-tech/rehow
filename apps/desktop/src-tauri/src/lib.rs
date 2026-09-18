pub const APPLICATION_NAME: &str = "Rehow";

use rehow_core::{
    application::{GUIDE_BODY_INPUT_LIMIT, GuideEdit, GuideUpdateError, update_guide},
    domain::GuideStatus,
    markdown::{SafeMarkdownDocument, parse_safe_markdown},
    protocol::{GuideId, Revision},
    search::search_workspace,
    storage::{WorkspaceFolderFacts, WorkspaceInspection, WorkspaceView, inspect_workspace},
};
use serde::Serialize;
use std::{env, fs, path::PathBuf, str::FromStr, sync::Mutex};

#[derive(Default)]
struct DesktopState {
    workspace: Mutex<Option<DesktopWorkspace>>,
}

struct DesktopWorkspace {
    view: Box<WorkspaceView>,
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
    name: String,
    categories: Vec<CategoryPayload>,
    guides: Vec<GuidePayload>,
    issue_count: usize,
    facts: FolderFactsPayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CategoryPayload {
    id: String,
    name: String,
    guide_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuidePayload {
    id: String,
    title: String,
    summary: String,
    category: String,
    status: &'static str,
    updated_at: String,
    matches: String,
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

impl WorkspacePayload {
    fn from_view(view: &WorkspaceView, facts: FolderFactsPayload) -> Self {
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
                    category: category.name().to_owned(),
                    status: match guide.declared_status() {
                        GuideStatus::Draft => "Draft",
                        GuideStatus::Tested => "Tested",
                        GuideStatus::Stale => "Stale",
                        GuideStatus::Archived => "Archived",
                    },
                    updated_at: guide.updated_at().to_string(),
                    matches,
                });
            }
        }
        Self {
            name: view.manifest().name().to_owned(),
            categories,
            guides,
            issue_count: view.issues().len(),
            facts,
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
    state: tauri::State<'_, DesktopState>,
) -> Result<Vec<String>, String> {
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "Workspace session is unavailable".to_owned())?;
    let workspace = guard
        .as_ref()
        .ok_or_else(|| "No Workspace is open".to_owned())?;
    search_workspace(&workspace.view, &query, false, 100)
        .map(|hits| {
            hits.into_iter()
                .map(|hit| hit.guide_id().to_string())
                .collect()
        })
        .map_err(|error| match error {
            rehow_core::search::SearchError::QueryTooLarge => "Search query is too long".to_owned(),
            rehow_core::search::SearchError::TooManyTerms => {
                "Search query has too many terms".to_owned()
            }
        })
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
        workspace: WorkspacePayload::from_view(
            &workspace.view,
            FolderFactsPayload::from(workspace.facts),
        ),
        guide: guide_body_payload(&workspace.view, &guide_id)?,
        changed: outcome.changed(),
    })
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
        GuideUpdateError::RevisionConflict => {
            "This Guide changed on disk. Reopen it before saving.".to_owned()
        }
        GuideUpdateError::InvalidGuide => {
            "This Guide is malformed and cannot be edited safely.".to_owned()
        }
        GuideUpdateError::InvalidEdit => {
            "The Guide content is invalid. Fix it and try again.".to_owned()
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

#[tauri::command]
fn choose_workspace(state: tauri::State<'_, DesktopState>) -> Result<OpenWorkspaceResult, String> {
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
    match inspection {
        WorkspaceInspection::Empty => Ok(OpenWorkspaceResult::Empty { facts }),
        WorkspaceInspection::Compatible(view) => {
            let payload = WorkspacePayload::from_view(&view, facts);
            *state
                .workspace
                .lock()
                .map_err(|_| "Workspace session is unavailable".to_owned())? =
                Some(DesktopWorkspace {
                    view,
                    facts: report_facts,
                });
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
            choose_workspace,
            read_open_guide,
            save_open_guide_body,
            search_open_workspace
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rehow");
}

#[cfg(test)]
mod tests {
    use super::{
        APPLICATION_NAME, FolderFactsPayload, GuideBodyPayload, GuidePayload, WorkspacePayload,
        guide_update_message,
    };
    use rehow_core::{application::GuideUpdateError, markdown::parse_safe_markdown};

    #[test]
    fn application_name_is_stable() {
        assert_eq!(APPLICATION_NAME, "Rehow");
    }

    #[test]
    fn payload_field_names_are_stable() {
        let payload = WorkspacePayload {
            name: "Test".to_owned(),
            categories: Vec::new(),
            guides: vec![GuidePayload {
                id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".to_owned(),
                title: "Test".to_owned(),
                summary: "Summary".to_owned(),
                category: "Linux".to_owned(),
                status: "Draft",
                updated_at: "2026-09-18T00:00:00Z".to_owned(),
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
        };
        let json = serde_json::to_value(payload).unwrap();
        assert_eq!(json["name"], "Test");
        assert_eq!(json["issueCount"], 0);
        assert_eq!(json["facts"]["writable"], true);
        assert!(json["guides"][0].get("body").is_none());
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
            guide_update_message(GuideUpdateError::RevisionConflict),
            "This Guide changed on disk. Reopen it before saving."
        );
        assert_eq!(
            guide_update_message(GuideUpdateError::UnsafeStorage),
            "The Guide path is unsafe or changed type. Reopen the Workspace."
        );
    }
}

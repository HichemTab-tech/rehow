pub const APPLICATION_NAME: &str = "Rehow";

use rehow_core::{
    domain::GuideStatus,
    markdown::{SafeMarkdownDocument, parse_safe_markdown},
    protocol::GuideId,
    storage::{WorkspaceFolderFacts, WorkspaceInspection, WorkspaceView, inspect_workspace},
};
use serde::Serialize;
use std::{str::FromStr, sync::Mutex};

#[derive(Default)]
struct DesktopState {
    workspace: Mutex<Option<Box<WorkspaceView>>>,
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
    document: SafeMarkdownDocument,
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
        .as_deref()
        .ok_or_else(|| "No Workspace is open".to_owned())?;
    let guide = workspace
        .categories()
        .iter()
        .flat_map(|category| category.guides())
        .find(|loaded| loaded.guide().id() == &guide_id)
        .ok_or_else(|| "Guide was not found".to_owned())?
        .guide();
    let body = std::str::from_utf8(guide.body())
        .map_err(|_| "Guide Markdown is not valid UTF-8".to_owned())?;
    let document = parse_safe_markdown(body).map_err(|error| error.to_string())?;
    Ok(GuideBodyPayload {
        id: guide.id().to_string(),
        document,
    })
}

#[tauri::command]
fn choose_workspace(state: tauri::State<'_, DesktopState>) -> Result<OpenWorkspaceResult, String> {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Open a Rehow Workspace")
        .pick_folder()
    else {
        return Ok(OpenWorkspaceResult::Cancelled);
    };

    let (facts, inspection) = inspect_workspace(&path)
        .map_err(|error| error.to_string())?
        .into_parts();
    let facts = FolderFactsPayload::from(facts);
    match inspection {
        WorkspaceInspection::Empty => Ok(OpenWorkspaceResult::Empty { facts }),
        WorkspaceInspection::Compatible(view) => {
            let payload = WorkspacePayload::from_view(&view, facts);
            *state
                .workspace
                .lock()
                .map_err(|_| "Workspace session is unavailable".to_owned())? = Some(view);
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
        .invoke_handler(tauri::generate_handler![choose_workspace, read_open_guide])
        .run(tauri::generate_context!())
        .expect("error while running Rehow");
}

#[cfg(test)]
mod tests {
    use super::{
        APPLICATION_NAME, FolderFactsPayload, GuideBodyPayload, GuidePayload, WorkspacePayload,
    };
    use rehow_core::markdown::parse_safe_markdown;

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
    fn guide_read_payload_contains_only_the_safe_document() {
        let payload = GuideBodyPayload {
            id: "0199a1d7-7b0d-7d8e-a38d-f936c5735e10".to_owned(),
            document: parse_safe_markdown("<script>no</script>").unwrap(),
        };
        let json = serde_json::to_value(payload).unwrap();
        assert!(json.get("body").is_none());
        assert_eq!(json["document"]["version"], 1);
        assert!(
            json["document"]["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "htmlLiteral")
        );
    }
}

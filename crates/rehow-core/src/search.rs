//! Deterministic field-aware Workspace search.

use crate::{
    domain::GuideStatus,
    markdown::{SafeMarkdownContainer, SafeMarkdownEvent, parse_safe_markdown},
    protocol::GuideId,
    storage::{LoadedCategory, LoadedGuide, WorkspaceView},
};
use std::cmp::Reverse;
use unicode_normalization::UnicodeNormalization;

pub const MAX_SEARCH_QUERY_BYTES: usize = 512;
pub const MAX_SEARCH_TERMS: usize = 16;
pub const MAX_SEARCH_RESULTS: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SearchMatchField {
    Title,
    Alias,
    Command,
    Path,
    Error,
    Tag,
    Category,
    Summary,
    Heading,
    Body,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    guide_id: GuideId,
    score: u16,
    matched_fields: Vec<SearchMatchField>,
}

impl SearchHit {
    pub fn guide_id(&self) -> GuideId {
        self.guide_id
    }

    pub fn score(&self) -> u16 {
        self.score
    }

    pub fn matched_fields(&self) -> &[SearchMatchField] {
        &self.matched_fields
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchError {
    QueryTooLarge,
    TooManyTerms,
}

pub fn search_workspace(
    workspace: &WorkspaceView,
    query: &str,
    include_archived: bool,
    limit: usize,
) -> Result<Vec<SearchHit>, SearchError> {
    let parsed = SearchQuery::parse(query)?;
    let limit = limit.min(MAX_SEARCH_RESULTS);
    if limit == 0 || parsed.terms.is_empty() {
        return Ok(Vec::new());
    }
    let mut hits: Vec<(SearchHit, String)> = Vec::new();

    for category in workspace.categories() {
        for loaded in category.guides() {
            if !include_archived && loaded.effective_status() == GuideStatus::Archived {
                continue;
            }
            let fields = SearchFields::extract(category, loaded);
            if !parsed.terms.iter().all(|term| fields.contains(term)) {
                continue;
            }
            let (mut base_score, mut matched_fields) = fields.score(&parsed.normalized);
            for term in &parsed.terms {
                let (term_score, term_fields) = fields.score(term);
                base_score = base_score.max(term_score);
                matched_fields.extend(term_fields);
            }
            if base_score == 0 {
                continue;
            }
            matched_fields.sort_unstable();
            matched_fields.dedup();
            let status_tie = match loaded.effective_status() {
                GuideStatus::Tested => 3,
                GuideStatus::Draft => 2,
                GuideStatus::Stale => 1,
                GuideStatus::Archived => 0,
            };
            hits.push((
                SearchHit {
                    guide_id: *loaded.guide().id(),
                    score: base_score + status_tie,
                    matched_fields,
                },
                normalize(loaded.guide().title()),
            ));
        }
    }

    hits.sort_by(|(left, left_title), (right, right_title)| {
        Reverse(left.score)
            .cmp(&Reverse(right.score))
            .then_with(|| left_title.cmp(right_title))
            .then_with(|| left.guide_id.cmp(&right.guide_id))
    });
    hits.truncate(limit);
    Ok(hits.into_iter().map(|(hit, _)| hit).collect())
}

struct SearchQuery {
    normalized: String,
    terms: Vec<String>,
}

impl SearchQuery {
    fn parse(query: &str) -> Result<Self, SearchError> {
        if query.len() > MAX_SEARCH_QUERY_BYTES {
            return Err(SearchError::QueryTooLarge);
        }
        let normalized = normalize(query.trim());
        let mut terms = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        for character in normalized.chars() {
            if character == '"' {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
                quoted = !quoted;
            } else if character.is_whitespace() && !quoted {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            } else {
                current.push(character);
            }
            if terms.len() > MAX_SEARCH_TERMS {
                return Err(SearchError::TooManyTerms);
            }
        }
        if !current.is_empty() {
            terms.push(current);
        }
        if terms.len() > MAX_SEARCH_TERMS {
            return Err(SearchError::TooManyTerms);
        }
        let normalized = terms.join(" ");
        Ok(Self { normalized, terms })
    }
}

struct SearchFields {
    title: String,
    aliases: Vec<String>,
    commands: Vec<String>,
    paths: Vec<String>,
    errors: Vec<String>,
    tags: Vec<String>,
    category: String,
    summary: String,
    headings: Vec<String>,
    body: String,
}

impl SearchFields {
    fn extract(category: &LoadedCategory, loaded: &LoadedGuide) -> Self {
        let guide = loaded.guide();
        let body_source = std::str::from_utf8(guide.body()).unwrap_or_default();
        let mut paths = guide
            .files_affected()
            .unwrap_or_default()
            .iter()
            .map(|value| normalize(value))
            .collect::<Vec<_>>();
        let mut commands = Vec::new();
        let mut headings = Vec::new();
        extract_markdown_fields(body_source, &mut commands, &mut headings, &mut paths);
        let mut errors = Vec::new();
        for line in body_source.lines() {
            let normalized = normalize(line.trim());
            if looks_like_error(&normalized) {
                errors.push(normalized.clone());
            }
            paths.extend(
                line.split_whitespace()
                    .map(trim_path_token)
                    .filter(|token| token.contains('/') || token.starts_with('~'))
                    .map(normalize),
            );
        }
        if let Some(warnings) = guide.warnings() {
            errors.extend(
                warnings
                    .iter()
                    .map(|value| normalize(value))
                    .filter(|value| looks_like_error(value)),
            );
        }
        if let Some(attachments) = guide.attachments() {
            paths.extend(
                attachments
                    .iter()
                    .map(|attachment| normalize(attachment.path())),
            );
        }
        let mut body = normalize(body_source);
        for value in guide
            .prerequisites()
            .unwrap_or_default()
            .iter()
            .chain(guide.warnings().unwrap_or_default())
            .chain(guide.files_affected().unwrap_or_default())
        {
            append_content(&mut body, value);
        }
        if let Some(applicability) = guide.applies_to() {
            for item in applicability {
                append_content(&mut body, item.platform());
                if let Some(value) = item.versions() {
                    append_content(&mut body, value);
                }
                if let Some(value) = item.notes() {
                    append_content(&mut body, value);
                }
            }
        }
        if let Some(references) = guide.references() {
            for reference in references {
                append_content(&mut body, reference.title());
                append_content(&mut body, reference.url());
            }
        }
        if let Some(attachments) = guide.attachments() {
            for attachment in attachments {
                append_content(&mut body, attachment.title());
                append_content(&mut body, attachment.path());
                if let Some(value) = attachment.media_type() {
                    append_content(&mut body, value);
                }
            }
        }
        Self {
            title: normalize(guide.title()),
            aliases: guide
                .aliases()
                .iter()
                .map(|value| normalize(value))
                .collect(),
            commands,
            paths,
            errors,
            tags: guide.tags().iter().map(|value| normalize(value)).collect(),
            category: normalize(category.category().name()),
            summary: normalize(guide.summary()),
            headings,
            body,
        }
    }

    fn contains(&self, term: &str) -> bool {
        self.title.contains(term)
            || self.summary.contains(term)
            || self.category.contains(term)
            || self.body.contains(term)
            || [
                &self.aliases,
                &self.commands,
                &self.paths,
                &self.errors,
                &self.tags,
                &self.headings,
            ]
            .into_iter()
            .any(|values| values.iter().any(|value| value.contains(term)))
    }

    fn score(&self, query: &str) -> (u16, Vec<SearchMatchField>) {
        let mut matches = Vec::new();
        let mut score = 0;
        record(
            &mut score,
            &mut matches,
            self.title == query,
            100,
            SearchMatchField::Title,
        );
        record(
            &mut score,
            &mut matches,
            self.aliases.iter().any(|value| value == query),
            95,
            SearchMatchField::Alias,
        );
        record(
            &mut score,
            &mut matches,
            self.commands.iter().any(|value| value == query),
            75,
            SearchMatchField::Command,
        );
        record(
            &mut score,
            &mut matches,
            self.paths.iter().any(|value| value == query),
            75,
            SearchMatchField::Path,
        );
        record(
            &mut score,
            &mut matches,
            self.errors.iter().any(|value| value == query),
            75,
            SearchMatchField::Error,
        );
        record(
            &mut score,
            &mut matches,
            self.title.starts_with(query),
            70,
            SearchMatchField::Title,
        );
        record(
            &mut score,
            &mut matches,
            self.title.split_whitespace().any(|token| token == query),
            55,
            SearchMatchField::Title,
        );
        record(
            &mut score,
            &mut matches,
            self.tags.iter().any(|value| value == query),
            45,
            SearchMatchField::Tag,
        );
        record(
            &mut score,
            &mut matches,
            self.category.contains(query),
            40,
            SearchMatchField::Category,
        );
        record(
            &mut score,
            &mut matches,
            self.summary.contains(query),
            35,
            SearchMatchField::Summary,
        );
        record(
            &mut score,
            &mut matches,
            self.headings.iter().any(|value| value.contains(query)),
            25,
            SearchMatchField::Heading,
        );
        record(
            &mut score,
            &mut matches,
            self.body.contains(query),
            10,
            SearchMatchField::Body,
        );
        (score, matches)
    }
}

fn record(
    score: &mut u16,
    matches: &mut Vec<SearchMatchField>,
    matched: bool,
    boost: u16,
    field: SearchMatchField,
) {
    if matched {
        *score = (*score).max(boost);
        matches.push(field);
    }
}

fn extract_markdown_fields(
    source: &str,
    commands: &mut Vec<String>,
    headings: &mut Vec<String>,
    paths: &mut Vec<String>,
) {
    let Ok(document) = parse_safe_markdown(source) else {
        return;
    };
    let mut stack = Vec::new();
    let mut heading = String::new();
    let mut command = String::new();
    for event in document.events {
        match event {
            SafeMarkdownEvent::Start { node } => stack.push(node),
            SafeMarkdownEvent::End => {
                if let Some(node) = stack.pop() {
                    match node {
                        SafeMarkdownContainer::Heading { .. } if !heading.is_empty() => {
                            headings.push(normalize(&heading));
                            heading.clear();
                        }
                        SafeMarkdownContainer::CodeBlock { .. } if !command.is_empty() => {
                            commands.extend(
                                command
                                    .lines()
                                    .map(str::trim)
                                    .filter(|line| !line.is_empty())
                                    .map(normalize),
                            );
                            command.clear();
                        }
                        _ => {}
                    }
                }
            }
            SafeMarkdownEvent::Text { value } | SafeMarkdownEvent::InlineCode { value } => {
                if value.contains('/') || value.starts_with('~') {
                    paths.push(normalize(&value));
                }
                if stack
                    .iter()
                    .any(|node| matches!(node, SafeMarkdownContainer::Heading { .. }))
                {
                    heading.push_str(&value);
                }
                if stack
                    .iter()
                    .any(|node| matches!(node, SafeMarkdownContainer::CodeBlock { .. }))
                {
                    command.push_str(&value);
                }
            }
            SafeMarkdownEvent::SoftBreak | SafeMarkdownEvent::HardBreak => {
                if !command.is_empty() {
                    command.push('\n');
                }
            }
            SafeMarkdownEvent::HtmlLiteral { .. }
            | SafeMarkdownEvent::Rule
            | SafeMarkdownEvent::TaskMarker { .. } => {}
        }
    }
}

fn looks_like_error(value: &str) -> bool {
    [
        "error",
        "failed",
        "failure",
        "not found",
        "permission denied",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

fn trim_path_token(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(
            character,
            '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' | '.'
        )
    })
}

fn append_content(content: &mut String, value: &str) {
    content.push(' ');
    content.push_str(&normalize(value));
}

fn normalize(value: &str) -> String {
    value.nfkc().flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{WorkspaceInspection, inspect_workspace};
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
            let path = std::env::temp_dir()
                .join(format!("rehow-search-test-{}-{number}", std::process::id()));
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

    fn workspace() -> (TestDirectory, Box<WorkspaceView>) {
        let temp = TestDirectory::new();
        write_workspace(temp.path());
        let inspection = inspect_workspace(temp.path()).unwrap();
        let (_, classification) = inspection.into_parts();
        let WorkspaceInspection::Compatible(view) = classification else {
            panic!("expected compatible Workspace")
        };
        (temp, view)
    }

    fn write_workspace(root: &Path) {
        fs::create_dir(root.join("categories")).unwrap();
        fs::write(root.join("rehow.yaml"), "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Search\ncreated_at: \"2026-09-18T00:00:00Z\"\nupdated_at: \"2026-09-18T00:00:00Z\"\n").unwrap();
        for (directory, id, name, order) in [
            ("linux", "0199a1d7-7b0d-7d8e-a38d-f936c5735e11", "Linux", 1),
            (
                "desktop",
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e12",
                "Desktop",
                2,
            ),
        ] {
            fs::create_dir(root.join("categories").join(directory)).unwrap();
            fs::write(root.join("categories").join(directory).join("category.yaml"), format!("schema_version: 1\nid: {id}\nname: {name}\norder: {order}\nstatus: active\ncreated_at: \"2026-09-18T00:00:00Z\"\nupdated_at: \"2026-09-18T00:00:00Z\"\n")).unwrap();
        }
        guide(
            root,
            "linux",
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e21",
            "Install package",
            "Package setup",
            "[tar install]",
            "[arch]",
            "draft",
            "## Run\n\n```sh\ntar -xf app.tar.gz\n```\n\nEdit `/etc/example.conf`.\n",
        );
        guide(
            root,
            "linux",
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e22",
            "Fix boot",
            "Recover startup",
            "[]",
            "[]",
            "draft",
            "Error: kernel not found\n\nUse `/boot/limine.conf`.\n",
        );
        guide(
            root,
            "desktop",
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e23",
            "Tar notes",
            "Mentions install package repeatedly",
            "[]",
            "[]",
            "draft",
            "install package install package install package\n",
        );
        guide(
            root,
            "desktop",
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e24",
            "Old",
            "Archived exact body",
            "[]",
            "[]",
            "archived",
            "needle\n",
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn guide(
        root: &Path,
        directory: &str,
        id: &str,
        title: &str,
        summary: &str,
        aliases: &str,
        tags: &str,
        status: &str,
        body: &str,
    ) {
        let category_id = if directory == "linux" {
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e11"
        } else {
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e12"
        };
        let archive = if status == "archived" {
            "archived_from_status: draft\narchived_at: \"2026-09-18T00:00:00Z\"\n"
        } else {
            ""
        };
        fs::write(root.join("categories").join(directory).join(format!("{id}.md")), format!("---\nschema_version: 1\nid: {id}\ntitle: {title}\nsummary: {summary}\ncategory_id: {category_id}\ntags: {tags}\naliases: {aliases}\nstatus: {status}\ncreated_at: \"2026-09-18T00:00:00Z\"\nupdated_at: \"2026-09-18T00:00:00Z\"\n{archive}---\n\n{body}")).unwrap();
    }

    #[test]
    fn exact_title_outranks_body_repetition() {
        let (_temp, workspace) = workspace();
        let hits = search_workspace(&workspace, "install package", false, 10).unwrap();
        assert_eq!(hits[0].score(), 102);
        assert_eq!(hits[0].matched_fields()[0], SearchMatchField::Title);
        assert_eq!(hits.len(), 2);
        assert_eq!(
            search_workspace(&workspace, "package arch", false, 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn command_path_error_alias_and_unicode_queries_are_explicit() {
        let (_temp, workspace) = workspace();
        for (query, field) in [
            ("tar -xf app.tar.gz", SearchMatchField::Command),
            ("/boot/limine.conf", SearchMatchField::Path),
            ("error: kernel not found", SearchMatchField::Error),
            ("TAR INSTALL", SearchMatchField::Alias),
        ] {
            let hits = search_workspace(&workspace, query, false, 10).unwrap();
            assert!(!hits.is_empty(), "{query}");
            assert!(hits[0].matched_fields().contains(&field), "{query}");
        }
    }

    #[test]
    fn archived_scope_limits_and_quoted_terms_are_deterministic() {
        let (_temp, workspace) = workspace();
        assert!(
            search_workspace(&workspace, "needle", false, 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            search_workspace(&workspace, "needle", true, 10)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            search_workspace(&workspace, "\"kernel not found\"", false, 1)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            search_workspace(&workspace, &"x".repeat(513), false, 10),
            Err(SearchError::QueryTooLarge)
        );
    }
}

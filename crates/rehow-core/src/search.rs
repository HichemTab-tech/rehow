//! Deterministic field-aware Workspace search.

use crate::{
    domain::GuideStatus,
    markdown::{SafeMarkdownContainer, SafeMarkdownEvent, parse_safe_markdown},
    protocol::{GuideId, Revision},
    storage::{LoadedCategory, LoadedGuide, WorkspaceView},
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use unicode_normalization::UnicodeNormalization;

pub const MAX_SEARCH_QUERY_BYTES: usize = 512;
pub const MAX_SEARCH_TERMS: usize = 16;
pub const MAX_SEARCH_RESULTS: usize = 100;
pub const MAX_SEARCH_EXCERPT_CHARS: usize = 220;
pub const MAX_SEARCH_HIGHLIGHTS: usize = 32;

/// Apply the same NFKC normalization and lowercase mapping used by the search index.
pub fn normalize_search_text(value: &str) -> String {
    normalize(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchStatusScope {
    Active,
    Tested,
    Draft,
    Stale,
    Archived,
    All,
}

impl SearchStatusScope {
    fn includes(self, status: GuideStatus) -> bool {
        match self {
            Self::Active => status != GuideStatus::Archived,
            Self::Tested => status == GuideStatus::Tested,
            Self::Draft => status == GuideStatus::Draft,
            Self::Stale => status == GuideStatus::Stale,
            Self::Archived => status == GuideStatus::Archived,
            Self::All => true,
        }
    }
}

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

impl SearchMatchField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Alias => "alias",
            Self::Command => "command",
            Self::Path => "path",
            Self::Error => "error",
            Self::Tag => "tag",
            Self::Category => "category",
            Self::Summary => "summary",
            Self::Heading => "heading",
            Self::Body => "body",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchHighlight {
    start: u16,
    end: u16,
}

impl SearchHighlight {
    pub fn start(&self) -> u16 {
        self.start
    }

    pub fn end(&self) -> u16 {
        self.end
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    guide_id: GuideId,
    score: u16,
    matched_fields: Vec<SearchMatchField>,
    excerpt: String,
    excerpt_field: SearchMatchField,
    highlights: Vec<SearchHighlight>,
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

    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }

    pub fn excerpt_field(&self) -> SearchMatchField {
        self.excerpt_field
    }

    pub fn highlights(&self) -> &[SearchHighlight] {
        &self.highlights
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchError {
    QueryTooLarge,
    TooManyTerms,
}

pub(crate) struct SearchIndex {
    documents: Vec<Arc<SearchDocument>>,
}

struct SearchDocument {
    guide_id: GuideId,
    revision: Revision,
    status: GuideStatus,
    category: String,
    title: String,
    fields: Arc<SearchFields>,
}

impl SearchIndex {
    pub(crate) fn reconcile(categories: &[LoadedCategory], previous: Option<&Self>) -> Self {
        let previous = previous.map(|index| {
            index
                .documents
                .iter()
                .map(|document| (document.guide_id, document))
                .collect::<BTreeMap<_, _>>()
        });
        let mut documents = Vec::with_capacity(
            categories
                .iter()
                .map(|category| category.guides().len())
                .sum(),
        );
        for category in categories {
            let category_name = normalize(category.category().name());
            for loaded in category.guides() {
                let guide_id = *loaded.guide().id();
                let previous_document = previous
                    .as_ref()
                    .and_then(|documents| documents.get(&guide_id))
                    .filter(|document| document.revision == loaded.revision());
                if let Some(document) = previous_document {
                    if document.status == loaded.effective_status()
                        && document.category == category_name
                    {
                        documents.push(Arc::clone(document));
                    } else {
                        documents.push(Arc::new(SearchDocument {
                            guide_id,
                            revision: loaded.revision(),
                            status: loaded.effective_status(),
                            category: category_name.clone(),
                            title: document.title.clone(),
                            fields: Arc::clone(&document.fields),
                        }));
                    }
                } else {
                    documents.push(Arc::new(SearchDocument {
                        guide_id,
                        revision: loaded.revision(),
                        status: loaded.effective_status(),
                        category: category_name.clone(),
                        title: normalize(loaded.guide().title()),
                        fields: Arc::new(SearchFields::extract(loaded)),
                    }));
                }
            }
        }
        Self { documents }
    }

    fn search(
        &self,
        query: &str,
        status_scope: SearchStatusScope,
        limit: usize,
        allowed_guide_ids: Option<&BTreeSet<GuideId>>,
    ) -> Result<Vec<SearchHit>, SearchError> {
        let parsed = SearchQuery::parse(query)?;
        let limit = limit.min(MAX_SEARCH_RESULTS);
        if limit == 0 || parsed.terms.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits: Vec<(SearchHit, &str, &str, &SearchFields)> = Vec::new();

        for document in &self.documents {
            if allowed_guide_ids.is_some_and(|allowed| !allowed.contains(&document.guide_id)) {
                continue;
            }
            if !status_scope.includes(document.status) {
                continue;
            }
            if !parsed
                .terms
                .iter()
                .all(|term| document.fields.contains(term, &document.category))
            {
                continue;
            }
            let (mut base_score, mut matched_fields) = document
                .fields
                .score(&parsed.normalized, &document.category);
            for term in &parsed.terms {
                let (term_score, term_fields) = document.fields.score(term, &document.category);
                base_score = base_score.max(term_score);
                matched_fields.extend(term_fields);
            }
            if base_score == 0 {
                continue;
            }
            matched_fields.sort_unstable();
            matched_fields.dedup();
            let status_tie = match document.status {
                GuideStatus::Tested => 3,
                GuideStatus::Draft => 2,
                GuideStatus::Stale => 1,
                GuideStatus::Archived => 0,
            };
            hits.push((
                SearchHit {
                    guide_id: document.guide_id,
                    score: base_score + status_tie,
                    matched_fields,
                    excerpt: String::new(),
                    excerpt_field: SearchMatchField::Body,
                    highlights: Vec::new(),
                },
                &document.title,
                &document.category,
                &document.fields,
            ));
        }

        hits.sort_by(|(left, left_title, _, _), (right, right_title, _, _)| {
            Reverse(left.score)
                .cmp(&Reverse(right.score))
                .then_with(|| left_title.cmp(right_title))
                .then_with(|| left.guide_id.cmp(&right.guide_id))
        });
        hits.truncate(limit);
        Ok(hits
            .into_iter()
            .map(|(mut hit, _, category, fields)| {
                let (field, excerpt, highlights) =
                    fields.excerpt(&parsed, category, &hit.matched_fields);
                hit.excerpt_field = field;
                hit.excerpt = excerpt;
                hit.highlights = highlights;
                hit
            })
            .collect())
    }
}

pub fn search_workspace(
    workspace: &WorkspaceView,
    query: &str,
    include_archived: bool,
    limit: usize,
) -> Result<Vec<SearchHit>, SearchError> {
    let status_scope = if include_archived {
        SearchStatusScope::All
    } else {
        SearchStatusScope::Active
    };
    search_workspace_in_scope(workspace, query, status_scope, limit)
}

pub fn search_workspace_in_scope(
    workspace: &WorkspaceView,
    query: &str,
    status_scope: SearchStatusScope,
    limit: usize,
) -> Result<Vec<SearchHit>, SearchError> {
    workspace
        .search_index()
        .search(query, status_scope, limit, None)
}

/// Searches only Guides in the caller-provided allowed set, applying that filter before ranking
/// and the public result cap. The set carries no filesystem authority.
pub fn search_workspace_filtered(
    workspace: &WorkspaceView,
    query: &str,
    include_archived: bool,
    limit: usize,
    allowed_guide_ids: &BTreeSet<GuideId>,
) -> Result<Vec<SearchHit>, SearchError> {
    let status_scope = if include_archived {
        SearchStatusScope::All
    } else {
        SearchStatusScope::Active
    };
    workspace
        .search_index()
        .search(query, status_scope, limit, Some(allowed_guide_ids))
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
    summary: String,
    headings: Vec<String>,
    body: String,
}

impl SearchFields {
    fn extract(loaded: &LoadedGuide) -> Self {
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
            summary: normalize(guide.summary()),
            headings,
            body,
        }
    }

    fn contains(&self, term: &str, category: &str) -> bool {
        self.title.contains(term)
            || self.summary.contains(term)
            || category.contains(term)
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

    fn score(&self, query: &str, category: &str) -> (u16, Vec<SearchMatchField>) {
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
            category.contains(query),
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

    fn excerpt(
        &self,
        query: &SearchQuery,
        category: &str,
        matched_fields: &[SearchMatchField],
    ) -> (SearchMatchField, String, Vec<SearchHighlight>) {
        let (field, value) = matched_fields
            .iter()
            .find_map(|field| {
                self.excerpt_candidate(*field, query, category)
                    .map(|value| (*field, value))
            })
            .unwrap_or((SearchMatchField::Body, &self.body));
        let excerpt = excerpt_window(value, &query.terms);
        let highlights = highlight_ranges(&excerpt, &query.terms);
        (field, excerpt, highlights)
    }

    fn excerpt_candidate<'a>(
        &'a self,
        field: SearchMatchField,
        query: &SearchQuery,
        category: &'a str,
    ) -> Option<&'a str> {
        let values: &[String] = match field {
            SearchMatchField::Title => std::slice::from_ref(&self.title),
            SearchMatchField::Alias => &self.aliases,
            SearchMatchField::Command => &self.commands,
            SearchMatchField::Path => &self.paths,
            SearchMatchField::Error => &self.errors,
            SearchMatchField::Tag => &self.tags,
            SearchMatchField::Category => {
                return matches_query(category, query).then_some(category);
            }
            SearchMatchField::Summary => std::slice::from_ref(&self.summary),
            SearchMatchField::Heading => &self.headings,
            SearchMatchField::Body => std::slice::from_ref(&self.body),
        };
        values
            .iter()
            .find(|value| matches_query(value, query))
            .map(String::as_str)
    }
}

fn matches_query(value: &str, query: &SearchQuery) -> bool {
    value.contains(&query.normalized) || query.terms.iter().any(|term| value.contains(term))
}

fn excerpt_window(value: &str, terms: &[String]) -> String {
    const BEFORE_MATCH: usize = 60;
    let first_match = terms
        .iter()
        .filter_map(|term| value.find(term))
        .min()
        .unwrap_or(0);
    if value.is_ascii() {
        let start = first_match.saturating_sub(BEFORE_MATCH);
        let end = start
            .saturating_add(MAX_SEARCH_EXCERPT_CHARS)
            .min(value.len());
        let mut excerpt = String::with_capacity(end.saturating_sub(start) + 6);
        if start > 0 {
            excerpt.push('…');
        }
        excerpt.push_str(&value[start..end]);
        if end < value.len() {
            excerpt.push('…');
        }
        return excerpt;
    }
    let match_character = value[..first_match].chars().count();
    let start_character = match_character.saturating_sub(BEFORE_MATCH);
    let end_character = start_character.saturating_add(MAX_SEARCH_EXCERPT_CHARS);
    let mut start_byte = value.len();
    let mut end_byte = value.len();
    let mut character_count = 0usize;
    for (byte, _) in value.char_indices() {
        if character_count == start_character {
            start_byte = byte;
        }
        if character_count == end_character {
            end_byte = byte;
            break;
        }
        character_count += 1;
    }
    if start_character == character_count {
        start_byte = value.len();
    }
    let leading = start_byte > 0;
    let trailing = end_byte < value.len();
    let mut excerpt = String::with_capacity(end_byte.saturating_sub(start_byte) + 6);
    if leading {
        excerpt.push('…');
    }
    excerpt.push_str(&value[start_byte..end_byte]);
    if trailing {
        excerpt.push('…');
    }
    excerpt
}

fn highlight_ranges(value: &str, terms: &[String]) -> Vec<SearchHighlight> {
    let mut ranges = Vec::new();
    for term in terms {
        let mut offset = 0usize;
        while ranges.len() < MAX_SEARCH_HIGHLIGHTS {
            let Some(relative) = value[offset..].find(term) else {
                break;
            };
            let start_byte = offset + relative;
            let end_byte = start_byte + term.len();
            let (start, end) = if value.is_ascii() {
                (start_byte, end_byte)
            } else {
                let start = value[..start_byte].chars().count();
                (start, start + value[start_byte..end_byte].chars().count())
            };
            ranges.push(SearchHighlight {
                start: u16::try_from(start).expect("bounded excerpt start"),
                end: u16::try_from(end).expect("bounded excerpt end"),
            });
            offset = end_byte;
        }
    }
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    let mut merged: Vec<SearchHighlight> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
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
    fn status_scope_filters_before_the_result_cap() {
        let temp = TestDirectory::new();
        write_workspace(temp.path());
        for index in 0..101_u64 {
            let id = format!("0199a1d7-7b0d-7d8e-a38d-{index:012x}");
            guide(
                temp.path(),
                "desktop",
                &id,
                &format!("Needle {index:03}"),
                "Active result",
                "[]",
                "[]",
                "draft",
                "needle\n",
            );
        }
        let inspection = inspect_workspace(temp.path()).unwrap();
        let (_, WorkspaceInspection::Compatible(workspace)) = inspection.into_parts() else {
            panic!("expected compatible Workspace")
        };

        let all =
            search_workspace_in_scope(&workspace, "needle", SearchStatusScope::All, 100).unwrap();
        assert_eq!(all.len(), 100);
        assert!(
            !all.iter().any(|hit| {
                hit.guide_id().to_string() == "0199a1d7-7b0d-7d8e-a38d-f936c5735e24"
            })
        );

        let archived =
            search_workspace_in_scope(&workspace, "needle", SearchStatusScope::Archived, 100)
                .unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(
            archived[0].guide_id().to_string(),
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e24"
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
            assert!(!hits[0].excerpt().is_empty(), "{query}");
            assert!(!hits[0].highlights().is_empty(), "{query}");
            assert!(
                hits[0].excerpt().chars().count() <= MAX_SEARCH_EXCERPT_CHARS + 2,
                "{query}"
            );
        }
    }

    #[test]
    fn excerpts_are_bounded_and_highlights_are_merged_unicode_scalar_ranges() {
        let source = format!(
            "{}permission denied{}",
            "before ".repeat(80),
            " after".repeat(80)
        );
        let excerpt = excerpt_window(&source, &["permission denied".to_owned()]);
        assert!(excerpt.starts_with('…'));
        assert!(excerpt.ends_with('…'));
        assert!(excerpt.chars().count() <= MAX_SEARCH_EXCERPT_CHARS + 2);
        let ranges = highlight_ranges(
            "café permission denied",
            &[
                "café".to_owned(),
                "permission".to_owned(),
                "permission denied".to_owned(),
            ],
        );
        assert_eq!(
            ranges,
            vec![
                SearchHighlight { start: 0, end: 4 },
                SearchHighlight { start: 5, end: 22 }
            ]
        );
        assert!(ranges.len() <= MAX_SEARCH_HIGHLIGHTS);
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

    #[test]
    fn disposable_index_rebuild_tracks_external_changes_and_ignores_malformed_guides() {
        let (temp, mut workspace) = workspace();
        let changed_id: GuideId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e21".parse().unwrap();
        let unchanged_id: GuideId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e22".parse().unwrap();
        let changed_before = indexed_document(workspace.search_index(), changed_id);
        let unchanged_before = indexed_document(workspace.search_index(), unchanged_id);
        assert!(
            search_workspace(&workspace, "new external token", false, 10)
                .unwrap()
                .is_empty()
        );
        let guide_path = temp
            .path()
            .join("categories/linux/0199a1d7-7b0d-7d8e-a38d-f936c5735e21.md");
        let source = fs::read_to_string(&guide_path).unwrap();
        fs::write(
            &guide_path,
            source.replace("Edit `/etc/example.conf`.", "New external token."),
        )
        .unwrap();
        fs::write(
            temp.path().join("categories/linux/malformed.md"),
            "---\ntitle: [not valid\n---\n",
        )
        .unwrap();
        assert!(
            search_workspace(&workspace, "new external token", false, 10)
                .unwrap()
                .is_empty(),
            "the retained index must not observe disk changes before a successful refresh"
        );

        crate::storage::refresh_workspace(&mut workspace).unwrap();

        assert_eq!(
            search_workspace(&workspace, "new external token", false, 10)
                .unwrap()
                .len(),
            1
        );
        assert!(!workspace.issues().is_empty());
        assert_eq!(workspace.search_index().documents.len(), 4);
        let changed_after = indexed_document(workspace.search_index(), changed_id);
        assert!(!Arc::ptr_eq(&changed_before, &changed_after));
        assert!(!Arc::ptr_eq(&changed_before.fields, &changed_after.fields));
        assert!(Arc::ptr_eq(
            &unchanged_before,
            &indexed_document(workspace.search_index(), unchanged_id)
        ));
    }

    #[test]
    fn incremental_reconcile_adds_removes_and_rebuilds_category_dependents() {
        let (temp, mut workspace) = workspace();
        let stable_desktop_id: GuideId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e24".parse().unwrap();
        let linux_id: GuideId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e22".parse().unwrap();
        let stable_desktop = indexed_document(workspace.search_index(), stable_desktop_id);
        let linux_before = indexed_document(workspace.search_index(), linux_id);

        fs::remove_file(
            temp.path()
                .join("categories/desktop/0199a1d7-7b0d-7d8e-a38d-f936c5735e23.md"),
        )
        .unwrap();
        crate::storage::refresh_workspace(&mut workspace).unwrap();
        assert_eq!(workspace.search_index().documents.len(), 3);
        assert!(Arc::ptr_eq(
            &stable_desktop,
            &indexed_document(workspace.search_index(), stable_desktop_id)
        ));
        assert!(
            search_workspace(&workspace, "tar notes", false, 10)
                .unwrap()
                .is_empty()
        );

        guide(
            temp.path(),
            "desktop",
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e25",
            "New indexed guide",
            "Added externally",
            "[]",
            "[]",
            "draft",
            "A newly indexed token.\n",
        );
        crate::storage::refresh_workspace(&mut workspace).unwrap();
        assert_eq!(workspace.search_index().documents.len(), 4);
        assert!(Arc::ptr_eq(
            &stable_desktop,
            &indexed_document(workspace.search_index(), stable_desktop_id)
        ));
        assert_eq!(
            search_workspace(&workspace, "newly indexed token", false, 10)
                .unwrap()
                .len(),
            1
        );

        let category_path = temp.path().join("categories/linux/category.yaml");
        let category = fs::read_to_string(&category_path).unwrap();
        fs::write(
            &category_path,
            category.replace("name: Linux", "name: Servers"),
        )
        .unwrap();
        crate::storage::refresh_workspace(&mut workspace).unwrap();
        assert_eq!(
            search_workspace(&workspace, "servers", false, 10)
                .unwrap()
                .len(),
            2
        );
        let linux_after = indexed_document(workspace.search_index(), linux_id);
        assert!(!Arc::ptr_eq(&linux_before, &linux_after));
        assert!(Arc::ptr_eq(&linux_before.fields, &linux_after.fields));
        assert!(Arc::ptr_eq(
            &stable_desktop,
            &indexed_document(workspace.search_index(), stable_desktop_id)
        ));
    }

    #[test]
    fn incremental_reconcile_applies_external_archive_transition() {
        let (temp, mut workspace) = workspace();
        let archived_id: GuideId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e21".parse().unwrap();
        let unchanged_id: GuideId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e22".parse().unwrap();
        let archived_before = indexed_document(workspace.search_index(), archived_id);
        let unchanged_before = indexed_document(workspace.search_index(), unchanged_id);
        let guide_path = temp
            .path()
            .join("categories/linux/0199a1d7-7b0d-7d8e-a38d-f936c5735e21.md");
        let source = fs::read_to_string(&guide_path).unwrap();
        let source = source
            .replace("status: draft\n", "status: archived\n")
            .replace(
                "updated_at: \"2026-09-18T00:00:00Z\"\n---",
                "updated_at: \"2026-09-19T00:00:00Z\"\narchived_from_status: draft\narchived_at: \"2026-09-19T00:00:00Z\"\n---",
            );
        fs::write(guide_path, source).unwrap();

        crate::storage::refresh_workspace(&mut workspace).unwrap();

        assert!(
            search_workspace(&workspace, "install package", false, 10)
                .unwrap()
                .iter()
                .all(|hit| hit.guide_id() != archived_id)
        );
        assert!(
            search_workspace(&workspace, "install package", true, 10)
                .unwrap()
                .iter()
                .any(|hit| hit.guide_id() == archived_id)
        );
        let archived_after = indexed_document(workspace.search_index(), archived_id);
        assert!(!Arc::ptr_eq(&archived_before, &archived_after));
        assert!(!Arc::ptr_eq(
            &archived_before.fields,
            &archived_after.fields
        ));
        assert!(Arc::ptr_eq(
            &unchanged_before,
            &indexed_document(workspace.search_index(), unchanged_id)
        ));
    }

    #[test]
    fn failed_refresh_retains_the_exact_prior_index() {
        let (temp, mut workspace) = workspace();
        let guide_id: GuideId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e21".parse().unwrap();
        let before = indexed_document(workspace.search_index(), guide_id);
        fs::write(temp.path().join("rehow.yaml"), "not: [valid").unwrap();

        assert!(crate::storage::refresh_workspace(&mut workspace).is_err());

        assert!(Arc::ptr_eq(
            &before,
            &indexed_document(workspace.search_index(), guide_id)
        ));
        assert_eq!(
            search_workspace(&workspace, "install package", false, 10)
                .unwrap()
                .first()
                .map(SearchHit::guide_id),
            Some(guide_id)
        );
    }

    #[test]
    #[ignore = "production performance gate; run explicitly with Cargo --release"]
    fn single_guide_incremental_reindex_meets_production_budget() {
        let temp = TestDirectory::new();
        write_performance_workspace(temp.path());
        let inspection = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(workspace) = inspection else {
            panic!("expected compatible performance Workspace");
        };
        let changed_path = temp.path().join("categories/benchmark/guide-02500.md");
        let changed = fs::read_to_string(&changed_path).unwrap();
        fs::write(
            &changed_path,
            changed.replace(
                "Error: permission denied synthetic failure 02500.",
                "Error: permission denied externally changed 02500.",
            ),
        )
        .unwrap();
        let refreshed = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(refreshed) = refreshed else {
            panic!("expected refreshed performance Workspace");
        };

        let started = std::time::Instant::now();
        let reconciled =
            SearchIndex::reconcile(refreshed.categories(), Some(workspace.search_index()));
        let elapsed = started.elapsed();
        let previous = workspace
            .search_index()
            .documents
            .iter()
            .map(|document| (document.guide_id, document))
            .collect::<BTreeMap<_, _>>();
        let reused = reconciled
            .documents
            .iter()
            .filter(|document| {
                previous
                    .get(&document.guide_id)
                    .is_some_and(|prior| Arc::ptr_eq(document, prior))
            })
            .count();

        println!(
            "single_guide_incremental_reindex_ms={:.3}",
            elapsed.as_secs_f64() * 1_000.0
        );
        assert_eq!(reconciled.documents.len(), 5_000);
        assert_eq!(reused, 4_999);
        assert!(
            elapsed <= std::time::Duration::from_millis(150),
            "single-Guide incremental reindex exceeded 150 ms"
        );
    }

    fn write_performance_workspace(root: &Path) {
        const GUIDE_COUNT: usize = 5_000;
        const BODY_BYTES: usize = 10_128;

        fs::create_dir(root.join("categories")).unwrap();
        fs::write(
            root.join("rehow.yaml"),
            "schema_version: 1\nworkspace_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\nname: Incremental search benchmark\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
        )
        .unwrap();
        let category = root.join("categories/benchmark");
        fs::create_dir(&category).unwrap();
        fs::write(
            category.join("category.yaml"),
            "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e11\nname: Benchmark\norder: 1\nstatus: active\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n",
        )
        .unwrap();
        let filler = "rehow-benchmark-token ".repeat(BODY_BYTES / 22);
        for index in 0..GUIDE_COUNT {
            let id = format!("0199a1d7-7b0d-7d8e-a38d-{index:012x}");
            let source = format!(
                "---\nschema_version: 1\nid: {id}\ntitle: Guide {index:05}\nsummary: Synthetic benchmark guide {index:05}\ncategory_id: 0199a1d7-7b0d-7d8e-a38d-f936c5735e11\ntags: [benchmark]\naliases: []\nstatus: draft\ncreated_at: \"2026-09-19T00:00:00Z\"\nupdated_at: \"2026-09-19T00:00:00Z\"\n---\n\n## Steps\n\n```sh\necho guide-{index:05}\n```\n\nEdit `/etc/rehow/{index:05}.conf`.\n\nError: permission denied synthetic failure {index:05}.\n\n{filler}\n"
            );
            fs::write(category.join(format!("guide-{index:05}.md")), source).unwrap();
        }
    }

    fn indexed_document(index: &SearchIndex, guide_id: GuideId) -> Arc<SearchDocument> {
        Arc::clone(
            index
                .documents
                .iter()
                .find(|document| document.guide_id == guide_id)
                .unwrap(),
        )
    }
}

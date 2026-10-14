//! Deterministic field-aware Workspace search.

use crate::{
    domain::{Guide, GuideStatus},
    markdown::{SafeMarkdownContainer, SafeMarkdownEvent, parse_safe_markdown},
    protocol::{GuideId, Revision},
    storage::WorkspaceView,
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::File,
    io::Write,
    os::unix::fs::FileExt,
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
    IndexUnavailable,
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
    record: SearchRecord,
}

struct SearchBacking {
    file: File,
    length: u64,
}

#[derive(Clone)]
struct SearchRecord {
    backing: Arc<SearchBacking>,
    offset: u64,
    length: u32,
}

enum PendingRecord {
    Existing(SearchRecord),
    New { offset: u64, length: u32 },
}

struct PendingDocument {
    guide_id: GuideId,
    revision: Revision,
    status: GuideStatus,
    category: String,
    title: String,
    record: PendingRecord,
}

enum PendingSearchDocument {
    Existing(Arc<SearchDocument>),
    New(PendingDocument),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SearchIndexBuildError {
    ResourceLimit,
    Io,
}

pub(crate) struct SearchIndexBuilder<'a> {
    previous: BTreeMap<GuideId, &'a Arc<SearchDocument>>,
    file: File,
    offset: u64,
    retained_backing_bytes: u64,
    documents: Vec<PendingSearchDocument>,
    encoded: Vec<u8>,
    compact: bool,
}

const SEARCH_BACKING_GENERATION_LIMIT: usize = 8;
const MAX_SEARCH_RECORD_BYTES: usize = 64 * 1024 * 1024;
const MAX_SEARCH_LIVE_BACKING_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_SEARCH_COMPACTION_SOURCE_BYTES: u64 = MAX_SEARCH_LIVE_BACKING_BYTES / 2;
const MAX_SEARCH_VALUES_PER_FIELD: usize = 65_536;

impl<'a> SearchIndexBuilder<'a> {
    pub(crate) fn new(previous: Option<&'a SearchIndex>) -> Result<Self, SearchIndexBuildError> {
        let (compact, retained_backing_bytes) = previous.map_or((false, 0), |index| {
            let (generations, bytes) = index.backing_stats();
            let compact = generations >= SEARCH_BACKING_GENERATION_LIMIT
                && bytes <= MAX_SEARCH_COMPACTION_SOURCE_BYTES;
            (compact, bytes)
        });
        let previous = previous
            .map(|index| {
                index
                    .documents
                    .iter()
                    .map(|document| (document.guide_id, document))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            previous,
            file: tempfile::tempfile().map_err(|_| SearchIndexBuildError::Io)?,
            offset: 0,
            retained_backing_bytes,
            documents: Vec::new(),
            encoded: Vec::new(),
            compact,
        })
    }

    pub(crate) fn add(
        &mut self,
        guide: &Guide,
        revision: Revision,
        status: GuideStatus,
        category: &str,
    ) -> Result<(), SearchIndexBuildError> {
        self.documents
            .try_reserve(1)
            .map_err(|_| SearchIndexBuildError::ResourceLimit)?;
        let guide_id = *guide.id();
        let category = normalize(category);
        let title = normalize(guide.title());
        if !self.compact
            && let Some(previous) = self.previous.get(&guide_id).filter(|document| {
                document.revision == revision
                    && document.status == status
                    && document.category == category
                    && document.title == title
            })
        {
            self.documents
                .push(PendingSearchDocument::Existing(Arc::clone(previous)));
            return Ok(());
        }
        let record = if let Some(previous) = self
            .previous
            .get(&guide_id)
            .filter(|document| document.revision == revision)
        {
            if self.compact {
                previous
                    .record
                    .read_bytes(&mut self.encoded)
                    .map_err(|_| SearchIndexBuildError::Io)?;
                self.write_encoded_record()?
            } else {
                PendingRecord::Existing(previous.record.clone())
            }
        } else {
            self.encoded.clear();
            SearchFields::extract(guide).encode(&mut self.encoded)?;
            self.write_encoded_record()?
        };
        self.documents
            .push(PendingSearchDocument::New(PendingDocument {
                guide_id,
                revision,
                status,
                category,
                title,
                record,
            }));
        Ok(())
    }

    fn write_encoded_record(&mut self) -> Result<PendingRecord, SearchIndexBuildError> {
        self.write_encoded_record_with_limits(
            MAX_SEARCH_RECORD_BYTES,
            MAX_SEARCH_LIVE_BACKING_BYTES,
        )
    }

    fn write_encoded_record_with_limits(
        &mut self,
        record_limit: usize,
        live_backing_limit: u64,
    ) -> Result<PendingRecord, SearchIndexBuildError> {
        if self.encoded.len() > record_limit {
            return Err(SearchIndexBuildError::ResourceLimit);
        }
        let length =
            u32::try_from(self.encoded.len()).map_err(|_| SearchIndexBuildError::ResourceLimit)?;
        let next_offset = self
            .offset
            .checked_add(u64::from(length))
            .ok_or(SearchIndexBuildError::ResourceLimit)?;
        let total_live_bytes = self
            .retained_backing_bytes
            .checked_add(next_offset)
            .ok_or(SearchIndexBuildError::ResourceLimit)?;
        if total_live_bytes > live_backing_limit {
            return Err(SearchIndexBuildError::ResourceLimit);
        }
        self.file
            .write_all(&self.encoded)
            .map_err(|_| SearchIndexBuildError::Io)?;
        let offset = self.offset;
        self.offset = next_offset;
        Ok(PendingRecord::New { offset, length })
    }

    pub(crate) fn finish(mut self) -> Result<SearchIndex, SearchIndexBuildError> {
        self.file.flush().map_err(|_| SearchIndexBuildError::Io)?;
        let backing = Arc::new(SearchBacking {
            file: self.file,
            length: self.offset,
        });
        let documents = self
            .documents
            .into_iter()
            .map(|document| match document {
                PendingSearchDocument::Existing(document) => document,
                PendingSearchDocument::New(document) => Arc::new(SearchDocument {
                    guide_id: document.guide_id,
                    revision: document.revision,
                    status: document.status,
                    category: document.category,
                    title: document.title,
                    record: match document.record {
                        PendingRecord::Existing(record) => record,
                        PendingRecord::New { offset, length } => SearchRecord {
                            backing: Arc::clone(&backing),
                            offset,
                            length,
                        },
                    },
                }),
            })
            .collect();
        Ok(SearchIndex { documents })
    }
}

impl SearchRecord {
    fn read_bytes(&self, buffer: &mut Vec<u8>) -> Result<(), SearchError> {
        let length = usize::try_from(self.length).map_err(|_| SearchError::IndexUnavailable)?;
        let end = self
            .offset
            .checked_add(u64::from(self.length))
            .ok_or(SearchError::IndexUnavailable)?;
        if length > MAX_SEARCH_RECORD_BYTES || end > self.backing.length {
            return Err(SearchError::IndexUnavailable);
        }
        buffer.clear();
        buffer
            .try_reserve_exact(length)
            .map_err(|_| SearchError::IndexUnavailable)?;
        buffer.resize(length, 0);
        self.backing
            .file
            .read_exact_at(buffer, self.offset)
            .map_err(|_| SearchError::IndexUnavailable)
    }

    fn read(&self, buffer: &mut Vec<u8>) -> Result<SearchFields, SearchError> {
        self.read_bytes(buffer)?;
        SearchFields::decode(buffer).ok_or(SearchError::IndexUnavailable)
    }
}

impl SearchIndex {
    fn backing_stats(&self) -> (usize, u64) {
        let mut seen = HashSet::new();
        let mut bytes = 0u64;
        for document in &self.documents {
            let identity = Arc::as_ptr(&document.record.backing) as usize;
            if seen.insert(identity) {
                bytes = bytes.saturating_add(document.record.backing.length);
            }
        }
        (seen.len(), bytes)
    }

    #[cfg(test)]
    fn reconcile_prebuilt(refreshed: &Self, previous: &Self) -> Self {
        let previous = previous
            .documents
            .iter()
            .map(|document| (document.guide_id, document))
            .collect::<BTreeMap<_, _>>();
        let documents = refreshed
            .documents
            .iter()
            .map(|document| {
                previous
                    .get(&document.guide_id)
                    .filter(|prior| {
                        prior.revision == document.revision
                            && prior.status == document.status
                            && prior.category == document.category
                            && prior.title == document.title
                    })
                    .map_or_else(|| Arc::clone(document), |prior| Arc::clone(prior))
            })
            .collect();
        Self { documents }
    }

    fn search(
        &self,
        query: &str,
        status_scope: SearchStatusScope,
        limit: usize,
        allowed_guide_ids: Option<&BTreeSet<GuideId>>,
    ) -> Result<Vec<SearchHit>, SearchError> {
        self.search_with_stats(
            query,
            status_scope,
            limit,
            allowed_guide_ids,
            &mut SearchStats::default(),
        )
    }

    fn search_with_stats(
        &self,
        query: &str,
        status_scope: SearchStatusScope,
        limit: usize,
        allowed_guide_ids: Option<&BTreeSet<GuideId>>,
        stats: &mut SearchStats,
    ) -> Result<Vec<SearchHit>, SearchError> {
        let parsed = SearchQuery::parse(query)?;
        let limit = limit.min(MAX_SEARCH_RESULTS);
        if limit == 0 || parsed.terms.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits: Vec<(SearchHit, usize, Vec<String>)> = Vec::new();
        let mut buffer = Vec::new();
        let mut scratch = FuzzyScratch::default();

        for (document_index, document) in self.documents.iter().enumerate() {
            if allowed_guide_ids.is_some_and(|allowed| !allowed.contains(&document.guide_id)) {
                continue;
            }
            if !status_scope.includes(document.status) {
                continue;
            }
            let fields = document.record.read(&mut buffer)?;
            let Some(evaluation) =
                fields.evaluate(&parsed, &document.category, &mut scratch, stats)
            else {
                continue;
            };
            let status_tie = match document.status {
                GuideStatus::Tested => 3,
                GuideStatus::Draft => 2,
                GuideStatus::Stale => 1,
                GuideStatus::Archived => 0,
            };
            let score =
                u16::try_from(evaluation.score.saturating_add(status_tie)).unwrap_or(u16::MAX);
            hits.push((
                SearchHit {
                    guide_id: document.guide_id,
                    score,
                    matched_fields: evaluation.matched_fields,
                    excerpt: String::new(),
                    excerpt_field: SearchMatchField::Body,
                    highlights: Vec::new(),
                },
                document_index,
                evaluation.fuzzy_tokens,
            ));
        }

        hits.sort_by(|(left, left_index, _), (right, right_index, _)| {
            Reverse(left.score)
                .cmp(&Reverse(right.score))
                .then_with(|| {
                    self.documents[*left_index]
                        .title
                        .cmp(&self.documents[*right_index].title)
                })
                .then_with(|| left.guide_id.cmp(&right.guide_id))
        });
        hits.truncate(limit);
        let mut completed = Vec::new();
        completed
            .try_reserve_exact(hits.len())
            .map_err(|_| SearchError::IndexUnavailable)?;
        for (mut hit, document_index, fuzzy_tokens) in hits {
            let document = &self.documents[document_index];
            let fields = document.record.read(&mut buffer)?;
            let mut needles = parsed.terms.clone();
            needles.extend(fuzzy_tokens);
            let (field, excerpt, highlights) = fields.excerpt(
                &parsed.normalized,
                &needles,
                &document.category,
                &hit.matched_fields,
            );
            hit.excerpt_field = field;
            hit.excerpt = excerpt;
            hit.highlights = highlights;
            completed.push(hit);
        }
        Ok(completed)
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

/// Work counters for one query. They make the fuzzy-matching bound observable to tests.
#[derive(Debug, Default)]
struct SearchStats {
    fuzzy_tokens_visited: u64,
    fuzzy_comparisons: u64,
}

struct SearchQuery {
    normalized: String,
    terms: Vec<String>,
    /// Typo-tolerance parameters for each term; `None` when the term must match exactly
    /// (quoted, too short, too long, or containing non-alphanumeric characters).
    fuzzy: Vec<Option<FuzzyTerm>>,
}

struct FuzzyTerm {
    characters: Vec<char>,
    max_distance: usize,
}

impl FuzzyTerm {
    fn for_term(term: &str, quoted: bool) -> Option<Self> {
        // Numbers, versions, identifiers with punctuation, and quoted phrases stay exact: a
        // one-edit neighbor of "04999" or "v1.2" is a different thing, not a typo.
        if quoted || !term.chars().all(char::is_alphabetic) {
            return None;
        }
        let characters = term.chars().collect::<Vec<_>>();
        if !(MIN_FUZZY_TERM_CHARS..=MAX_FUZZY_TERM_CHARS).contains(&characters.len()) {
            return None;
        }
        let max_distance = if characters.len() < 8 { 1 } else { 2 };
        Some(Self {
            characters,
            max_distance,
        })
    }
}

impl SearchQuery {
    fn parse(query: &str) -> Result<Self, SearchError> {
        if query.len() > MAX_SEARCH_QUERY_BYTES {
            return Err(SearchError::QueryTooLarge);
        }
        let normalized = normalize(query.trim());
        let mut terms = Vec::new();
        let mut quoted_terms = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        for character in normalized.chars() {
            if character == '"' {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                    quoted_terms.push(quoted);
                }
                quoted = !quoted;
            } else if character.is_whitespace() && !quoted {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                    quoted_terms.push(false);
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
            quoted_terms.push(quoted);
        }
        if terms.len() > MAX_SEARCH_TERMS {
            return Err(SearchError::TooManyTerms);
        }
        let fuzzy = terms
            .iter()
            .zip(&quoted_terms)
            .map(|(term, quoted)| FuzzyTerm::for_term(term, *quoted))
            .collect();
        let normalized = terms.join(" ");
        Ok(Self {
            normalized,
            terms,
            fuzzy,
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct SearchFields {
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
    pub(crate) fn extract(guide: &Guide) -> Self {
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

    fn encode(&self, output: &mut Vec<u8>) -> Result<(), SearchIndexBuildError> {
        output
            .try_reserve(4)
            .map_err(|_| SearchIndexBuildError::ResourceLimit)?;
        output.extend_from_slice(b"RHS1");
        encode_string(output, &self.title)?;
        encode_strings(output, &self.aliases)?;
        encode_strings(output, &self.commands)?;
        encode_strings(output, &self.paths)?;
        encode_strings(output, &self.errors)?;
        encode_strings(output, &self.tags)?;
        encode_string(output, &self.summary)?;
        encode_strings(output, &self.headings)?;
        encode_string(output, &self.body)?;
        Ok(())
    }

    fn decode(input: &[u8]) -> Option<Self> {
        let mut decoder = SearchRecordDecoder::new(input);
        if decoder.take(4)? != b"RHS1" {
            return None;
        }
        let value = Self {
            title: decoder.string()?,
            aliases: decoder.strings()?,
            commands: decoder.strings()?,
            paths: decoder.strings()?,
            errors: decoder.strings()?,
            tags: decoder.strings()?,
            summary: decoder.string()?,
            headings: decoder.strings()?,
            body: decoder.string()?,
        };
        (decoder.remaining() == 0).then_some(value)
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

    /// The normalized values stored for one field, in a fixed order.
    fn values<'a>(
        &'a self,
        field: SearchMatchField,
        category: &'a str,
    ) -> impl Iterator<Item = &'a str> + 'a {
        let (single, many): (Option<&'a str>, &'a [String]) = match field {
            SearchMatchField::Title => (Some(&self.title), &[]),
            SearchMatchField::Alias => (None, &self.aliases),
            SearchMatchField::Command => (None, &self.commands),
            SearchMatchField::Path => (None, &self.paths),
            SearchMatchField::Error => (None, &self.errors),
            SearchMatchField::Tag => (None, &self.tags),
            SearchMatchField::Category => (Some(category), &[]),
            SearchMatchField::Summary => (Some(&self.summary), &[]),
            SearchMatchField::Heading => (None, &self.headings),
            SearchMatchField::Body => (Some(&self.body), &[]),
        };
        single.into_iter().chain(many.iter().map(String::as_str))
    }

    /// Scores one Guide against a parsed query. Every term must match, exactly or, when a
    /// term has no exact occurrence anywhere in the Guide, through bounded typo tolerance.
    /// The total is the sum of each term's best field score plus small multi-field and
    /// phrase bonuses, so Guides that match more strongly across terms rank higher.
    fn evaluate(
        &self,
        query: &SearchQuery,
        category: &str,
        scratch: &mut FuzzyScratch,
        stats: &mut SearchStats,
    ) -> Option<SearchEvaluation> {
        // Cheap exact containment first: reject before any fuzzy work when a term that cannot
        // be fuzzy-matched is absent.
        let mut missing = 0u32;
        for (index, term) in query.terms.iter().enumerate() {
            if !self.contains(term, category) {
                query.fuzzy[index].as_ref()?;
                missing |= 1 << index;
            }
        }
        let mut total = 0u32;
        let mut mask = 0u16;
        let mut title_terms = 0usize;
        let mut fuzzy_tokens = Vec::new();
        // Fuzzy terms run first so a failed typo match rejects the Guide before exact scoring.
        for (index, fuzzy) in query.fuzzy.iter().enumerate() {
            if missing & (1 << index) == 0 {
                continue;
            }
            let matched = self.fuzzy_term_score(fuzzy.as_ref()?, category, scratch, stats)?;
            total = total.saturating_add(matched.score);
            mask |= matched.mask;
            if matched.mask & field_bit(SearchMatchField::Title) != 0 {
                title_terms += 1;
            }
            fuzzy_tokens.extend(matched.tokens);
        }
        for (index, term) in query.terms.iter().enumerate() {
            if missing & (1 << index) != 0 {
                continue;
            }
            let (score, term_mask) = self.exact_term_score(term, category);
            if score == 0 {
                return None;
            }
            total = total.saturating_add(score);
            mask |= term_mask;
            if term_mask & field_bit(SearchMatchField::Title) != 0 {
                title_terms += 1;
            }
        }
        if query.terms.len() > 1 {
            total = total.saturating_add(self.phrase_bonus(&query.normalized));
            if title_terms == query.terms.len() {
                total = total.saturating_add(ALL_TERMS_IN_TITLE_BONUS);
            }
        }
        let extra_fields = mask.count_ones().saturating_sub(1).min(MAX_BONUS_FIELDS);
        total = total.saturating_add(extra_fields * MULTI_FIELD_BONUS);
        let matched_fields = ALL_FIELDS
            .into_iter()
            .filter(|field| mask & field_bit(*field) != 0)
            .collect();
        Some(SearchEvaluation {
            score: total,
            matched_fields,
            fuzzy_tokens,
        })
    }

    /// Best exact score for one term across fields, plus the mask of every field it matched.
    fn exact_term_score(&self, term: &str, category: &str) -> (u32, u16) {
        let mut best = 0;
        let mut mask = 0;
        for weight in EXACT_FIELD_WEIGHTS {
            let mut field_best = 0;
            for value in self.values(weight.field, category) {
                let score = if value == term {
                    weight.whole_value
                } else if let Some(percent) = occurrence_percent(value, term) {
                    weight.token * percent / 100
                } else {
                    continue;
                };
                field_best = field_best.max(score);
                if field_best >= weight.whole_value {
                    break;
                }
            }
            if field_best > 0 {
                mask |= field_bit(weight.field);
                best = best.max(field_best);
            }
        }
        (best, mask)
    }

    /// Bounded typo-tolerant match of one term against word tokens of high-value fields.
    /// At most `MAX_FUZZY_TOKENS_PER_TERM` tokens and `MAX_FUZZY_BYTES_PER_TERM` bytes are
    /// examined per Guide and term, so the cost per Guide is constant regardless of field
    /// sizes.
    fn fuzzy_term_score(
        &self,
        term: &FuzzyTerm,
        category: &str,
        scratch: &mut FuzzyScratch,
        stats: &mut SearchStats,
    ) -> Option<FuzzyMatch> {
        let term_length = term.characters.len();
        let max_distance = term.max_distance;
        let mut remaining_tokens = MAX_FUZZY_TOKENS_PER_TERM;
        let mut remaining_bytes = MAX_FUZZY_BYTES_PER_TERM;
        let mut best = 0;
        let mut mask = 0;
        let mut tokens: Vec<String> = Vec::new();
        'fields: for weight in FUZZY_FIELD_WEIGHTS {
            // Fields are ordered by weight; a lower field can no longer beat the best match.
            if best >= weight.token * FUZZY_ONE_EDIT_PERCENT / 100 {
                break;
            }
            for value in self.values(weight.field, category) {
                if remaining_bytes == 0 {
                    break 'fields;
                }
                let scanned = truncate_at_char_boundary(value, remaining_bytes);
                remaining_bytes -= scanned.len();
                for token in scanned
                    .split(|character: char| !character.is_alphanumeric())
                    .filter(|token| !token.is_empty())
                {
                    if remaining_tokens == 0 {
                        break 'fields;
                    }
                    remaining_tokens -= 1;
                    stats.fuzzy_tokens_visited += 1;
                    // Byte length bounds the character count from both sides (1..=4 bytes).
                    if token.len() + max_distance < term_length
                        || token.len() > (term_length + max_distance) * 4
                    {
                        continue;
                    }
                    scratch.token.clear();
                    scratch.token.extend(token.chars());
                    if scratch.token.len().abs_diff(term_length) > max_distance {
                        continue;
                    }
                    stats.fuzzy_comparisons += 1;
                    let Some(distance) = bounded_osa_distance(
                        &term.characters,
                        &scratch.token,
                        max_distance,
                        &mut scratch.rows,
                    ) else {
                        continue;
                    };
                    let percent = if distance <= 1 {
                        FUZZY_ONE_EDIT_PERCENT
                    } else {
                        FUZZY_TWO_EDIT_PERCENT
                    };
                    best = best.max(weight.token * percent / 100);
                    mask |= field_bit(weight.field);
                    if tokens.len() < MAX_FUZZY_TOKENS_HIGHLIGHTED
                        && !tokens.iter().any(|existing| existing == token)
                    {
                        tokens.push(token.to_owned());
                    }
                }
            }
        }
        (best > 0).then_some(FuzzyMatch {
            score: best,
            mask,
            tokens,
        })
    }

    fn phrase_bonus(&self, phrase: &str) -> u32 {
        if self.title == phrase {
            EXACT_TITLE_PHRASE_BONUS
        } else if self.aliases.iter().any(|alias| alias == phrase) {
            EXACT_ALIAS_PHRASE_BONUS
        } else if self.title.contains(phrase) {
            TITLE_PHRASE_BONUS
        } else if self.aliases.iter().any(|alias| alias.contains(phrase)) {
            ALIAS_PHRASE_BONUS
        } else {
            0
        }
    }

    fn excerpt(
        &self,
        phrase: &str,
        needles: &[String],
        category: &str,
        matched_fields: &[SearchMatchField],
    ) -> (SearchMatchField, String, Vec<SearchHighlight>) {
        let (field, value) = matched_fields
            .iter()
            .find_map(|field| {
                self.values(*field, category)
                    .find(|value| matches_needles(value, phrase, needles))
                    .map(|value| (*field, value))
            })
            .unwrap_or((SearchMatchField::Body, self.body.as_str()));
        let excerpt = excerpt_window(value, needles);
        let highlights = highlight_ranges(&excerpt, needles);
        (field, excerpt, highlights)
    }
}

#[derive(Clone, Copy)]
struct FieldWeight {
    field: SearchMatchField,
    /// Score for the term as a whole word inside the field; word-prefix, mid-word, and fuzzy
    /// matches earn a percentage of it.
    token: u32,
    /// Score when the entire field value equals the term.
    whole_value: u32,
}

const fn weight(field: SearchMatchField, token: u32, whole_value: u32) -> FieldWeight {
    FieldWeight {
        field,
        token,
        whole_value,
    }
}

const EXACT_FIELD_WEIGHTS: [FieldWeight; 10] = [
    weight(SearchMatchField::Title, 70, 100),
    weight(SearchMatchField::Alias, 65, 95),
    weight(SearchMatchField::Command, 50, 75),
    weight(SearchMatchField::Path, 50, 75),
    weight(SearchMatchField::Error, 50, 75),
    weight(SearchMatchField::Tag, 50, 60),
    weight(SearchMatchField::Category, 45, 55),
    weight(SearchMatchField::Summary, 35, 35),
    weight(SearchMatchField::Heading, 30, 35),
    weight(SearchMatchField::Body, 12, 12),
];

/// High-value fields eligible for typo tolerance, ordered by descending token weight.
const FUZZY_FIELD_WEIGHTS: [FieldWeight; 6] = [
    weight(SearchMatchField::Title, 70, 0),
    weight(SearchMatchField::Alias, 65, 0),
    weight(SearchMatchField::Command, 50, 0),
    weight(SearchMatchField::Tag, 50, 0),
    weight(SearchMatchField::Category, 45, 0),
    weight(SearchMatchField::Heading, 30, 0),
];

const ALL_FIELDS: [SearchMatchField; 10] = [
    SearchMatchField::Title,
    SearchMatchField::Alias,
    SearchMatchField::Command,
    SearchMatchField::Path,
    SearchMatchField::Error,
    SearchMatchField::Tag,
    SearchMatchField::Category,
    SearchMatchField::Summary,
    SearchMatchField::Heading,
    SearchMatchField::Body,
];

const WHOLE_WORD_PERCENT: u32 = 100;
const WORD_PREFIX_PERCENT: u32 = 85;
const MID_WORD_PERCENT: u32 = 50;
const FUZZY_ONE_EDIT_PERCENT: u32 = 60;
const FUZZY_TWO_EDIT_PERCENT: u32 = 45;
const EXACT_TITLE_PHRASE_BONUS: u32 = 100;
const EXACT_ALIAS_PHRASE_BONUS: u32 = 90;
const TITLE_PHRASE_BONUS: u32 = 25;
const ALIAS_PHRASE_BONUS: u32 = 20;
const ALL_TERMS_IN_TITLE_BONUS: u32 = 15;
const MULTI_FIELD_BONUS: u32 = 2;
const MAX_BONUS_FIELDS: u32 = 4;
const MAX_RANKED_OCCURRENCES: usize = 64;
const MIN_FUZZY_TERM_CHARS: usize = 4;
const MAX_FUZZY_TERM_CHARS: usize = 32;
const MAX_FUZZY_TOKENS_PER_TERM: usize = 256;
const MAX_FUZZY_BYTES_PER_TERM: usize = 16 * 1024;
const MAX_FUZZY_TOKENS_HIGHLIGHTED: usize = 4;

struct SearchEvaluation {
    score: u32,
    matched_fields: Vec<SearchMatchField>,
    fuzzy_tokens: Vec<String>,
}

struct FuzzyMatch {
    score: u32,
    mask: u16,
    tokens: Vec<String>,
}

#[derive(Default)]
struct FuzzyScratch {
    token: Vec<char>,
    rows: OsaRows,
}

#[derive(Default)]
struct OsaRows {
    two_back: Vec<usize>,
    previous: Vec<usize>,
    current: Vec<usize>,
}

fn field_bit(field: SearchMatchField) -> u16 {
    1 << (field as u16)
}

/// Percentage of a field's token weight earned by the best occurrence of `term` in `value`:
/// a whole word, a word prefix, or a mid-word substring. Only a bounded number of
/// occurrences is ranked.
fn occurrence_percent(value: &str, term: &str) -> Option<u32> {
    let term_starts_word = term.chars().next().is_some_and(char::is_alphanumeric);
    let term_ends_word = term.chars().next_back().is_some_and(char::is_alphanumeric);
    let mut best: Option<u32> = None;
    for (start, _) in value.match_indices(term).take(MAX_RANKED_OCCURRENCES) {
        let end = start + term.len();
        let at_word_start = !term_starts_word
            || !value[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric);
        let at_word_end = !term_ends_word
            || !value[end..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric);
        let percent = match (at_word_start, at_word_end) {
            (true, true) => WHOLE_WORD_PERCENT,
            (true, false) => WORD_PREFIX_PERCENT,
            _ => MID_WORD_PERCENT,
        };
        best = Some(best.map_or(percent, |current| current.max(percent)));
        if percent == WHOLE_WORD_PERCENT {
            break;
        }
    }
    best
}

fn truncate_at_char_boundary(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

/// Damerau-Levenshtein distance (optimal string alignment) with an upper bound. Returns
/// `None` as soon as every alignment in a row exceeds `max_distance`; row minima never
/// decrease, so the early exit is exact.
fn bounded_osa_distance(
    left: &[char],
    right: &[char],
    max_distance: usize,
    rows: &mut OsaRows,
) -> Option<usize> {
    if left.len().abs_diff(right.len()) > max_distance {
        return None;
    }
    let width = right.len() + 1;
    let OsaRows {
        two_back,
        previous,
        current,
    } = rows;
    for row in [&mut *two_back, &mut *previous, &mut *current] {
        row.clear();
        row.resize(width, 0);
    }
    for (column, cell) in previous.iter_mut().enumerate() {
        *cell = column;
    }
    for row in 1..=left.len() {
        current[0] = row;
        let mut row_minimum = row;
        for column in 1..width {
            let substitution = usize::from(left[row - 1] != right[column - 1]);
            let mut distance = (previous[column] + 1)
                .min(current[column - 1] + 1)
                .min(previous[column - 1] + substitution);
            if row > 1
                && column > 1
                && left[row - 1] == right[column - 2]
                && left[row - 2] == right[column - 1]
            {
                distance = distance.min(two_back[column - 2] + 1);
            }
            current[column] = distance;
            row_minimum = row_minimum.min(distance);
        }
        if row_minimum > max_distance {
            return None;
        }
        std::mem::swap(two_back, previous);
        std::mem::swap(previous, current);
    }
    let distance = previous[right.len()];
    (distance <= max_distance).then_some(distance)
}

fn encode_u32(output: &mut Vec<u8>, value: usize) -> Result<(), SearchIndexBuildError> {
    let value = u32::try_from(value).map_err(|_| SearchIndexBuildError::ResourceLimit)?;
    output
        .try_reserve(4)
        .map_err(|_| SearchIndexBuildError::ResourceLimit)?;
    output.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn encode_string(output: &mut Vec<u8>, value: &str) -> Result<(), SearchIndexBuildError> {
    encode_u32(output, value.len())?;
    output
        .try_reserve(value.len())
        .map_err(|_| SearchIndexBuildError::ResourceLimit)?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn encode_strings(output: &mut Vec<u8>, values: &[String]) -> Result<(), SearchIndexBuildError> {
    if values.len() > MAX_SEARCH_VALUES_PER_FIELD {
        return Err(SearchIndexBuildError::ResourceLimit);
    }
    encode_u32(output, values.len())?;
    for value in values {
        encode_string(output, value)?;
    }
    Ok(())
}

struct SearchRecordDecoder<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> SearchRecordDecoder<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.offset)
    }

    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(length)?;
        let value = self.input.get(self.offset..end)?;
        self.offset = end;
        Some(value)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let length = usize::try_from(self.u32()?).ok()?;
        let value = std::str::from_utf8(self.take(length)?).ok()?;
        let mut owned = String::new();
        owned.try_reserve_exact(length).ok()?;
        owned.push_str(value);
        Some(owned)
    }

    fn strings(&mut self) -> Option<Vec<String>> {
        let count = usize::try_from(self.u32()?).ok()?;
        if count > MAX_SEARCH_VALUES_PER_FIELD || count > self.remaining() / 4 {
            return None;
        }
        let mut values = Vec::new();
        values.try_reserve_exact(count).ok()?;
        for _ in 0..count {
            values.push(self.string()?);
        }
        Some(values)
    }
}

fn matches_needles(value: &str, phrase: &str, needles: &[String]) -> bool {
    value.contains(phrase) || needles.iter().any(|needle| value.contains(needle))
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

    #[test]
    fn disk_record_round_trips_and_rejects_every_truncation() {
        let fields = SearchFields {
            title: "configure café".to_owned(),
            aliases: vec!["setup\0alias".to_owned(), "second".to_owned()],
            commands: vec!["printf '✓'".to_owned()],
            paths: vec!["~/.config/rehow".to_owned()],
            errors: vec!["error: permission denied".to_owned()],
            tags: vec!["linux".to_owned(), "wayland".to_owned()],
            summary: "bounded summary".to_owned(),
            headings: vec!["verification".to_owned()],
            body: "body with unicode λ and embedded \0 bytes".to_owned(),
        };
        let mut encoded = Vec::new();
        fields.encode(&mut encoded).unwrap();
        assert_eq!(SearchFields::decode(&encoded), Some(fields));
        for end in 0..encoded.len() {
            assert!(SearchFields::decode(&encoded[..end]).is_none(), "{end}");
        }
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(SearchFields::decode(&trailing).is_none());
        let mut bad_magic = encoded;
        bad_magic[0] ^= 0xff;
        assert!(SearchFields::decode(&bad_magic).is_none());

        let mut invalid_utf8 = b"RHS1".to_vec();
        invalid_utf8.extend_from_slice(&1u32.to_le_bytes());
        invalid_utf8.push(0xff);
        assert!(SearchFields::decode(&invalid_utf8).is_none());

        let mut oversized_string = b"RHS1".to_vec();
        oversized_string.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(SearchFields::decode(&oversized_string).is_none());

        let mut oversized_count = b"RHS1".to_vec();
        oversized_count.extend_from_slice(&0u32.to_le_bytes());
        oversized_count.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(SearchFields::decode(&oversized_count).is_none());
    }

    #[test]
    fn disk_record_limits_fail_before_writing_or_allocating() {
        let mut record_limited = SearchIndexBuilder::new(None).unwrap();
        record_limited.encoded.extend_from_slice(b"12345");
        assert!(matches!(
            record_limited.write_encoded_record_with_limits(4, 100),
            Err(SearchIndexBuildError::ResourceLimit)
        ));
        assert_eq!(record_limited.file.metadata().unwrap().len(), 0);

        let mut live_backing_limited = SearchIndexBuilder::new(None).unwrap();
        live_backing_limited.retained_backing_bytes = 3;
        live_backing_limited.compact = true;
        live_backing_limited.encoded.extend_from_slice(b"12");
        assert!(matches!(
            live_backing_limited.write_encoded_record_with_limits(10, 4),
            Err(SearchIndexBuildError::ResourceLimit)
        ));
        assert_eq!(live_backing_limited.file.metadata().unwrap().len(), 0);

        let mut count = Vec::new();
        count.extend_from_slice(
            &u32::try_from(MAX_SEARCH_VALUES_PER_FIELD + 1)
                .unwrap()
                .to_le_bytes(),
        );
        count.resize(4 + (MAX_SEARCH_VALUES_PER_FIELD + 1) * 4, 0);
        let mut decoder = SearchRecordDecoder::new(&count);
        assert!(decoder.strings().is_none());
    }

    #[test]
    fn disk_record_reads_reject_invalid_positions_and_lengths() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"RHS1").unwrap();
        let backing = Arc::new(SearchBacking { file, length: 4 });
        let mut buffer = Vec::new();
        let beyond_end = SearchRecord {
            backing: Arc::clone(&backing),
            offset: 1,
            length: 4,
        };
        assert_eq!(
            beyond_end.read_bytes(&mut buffer),
            Err(SearchError::IndexUnavailable)
        );
        let oversized = SearchRecord {
            backing,
            offset: 0,
            length: u32::try_from(MAX_SEARCH_RECORD_BYTES + 1).unwrap(),
        };
        assert_eq!(
            oversized.read_bytes(&mut buffer),
            Err(SearchError::IndexUnavailable)
        );
    }

    #[test]
    fn incremental_backings_compact_at_the_generation_limit() {
        let temp = TestDirectory::new();
        write_workspace(temp.path());
        for generation in 0..SEARCH_BACKING_GENERATION_LIMIT {
            let id = format!("0199a1d7-7b0d-7d8e-a38d-f936c5735e3{generation}");
            guide(
                temp.path(),
                "linux",
                &id,
                &format!("Compaction {generation}"),
                "Compaction fixture",
                "[]",
                "[]",
                "draft",
                "Initial body.\n",
            );
        }
        let inspection = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(mut workspace) = inspection else {
            panic!("expected compatible Workspace")
        };
        for generation in 0..SEARCH_BACKING_GENERATION_LIMIT {
            let id = format!("0199a1d7-7b0d-7d8e-a38d-f936c5735e3{generation}");
            let guide_path = temp.path().join(format!("categories/linux/{id}.md"));
            let mut source = fs::read_to_string(&guide_path).unwrap();
            source.push_str(&format!("\nGeneration token {generation}.\n"));
            fs::write(&guide_path, source).unwrap();
            crate::storage::refresh_workspace(&mut workspace).unwrap();
        }
        assert_eq!(workspace.search_index().backing_stats().0, 1);
        assert_eq!(
            search_workspace(
                &workspace,
                &format!("generation token {}", SEARCH_BACKING_GENERATION_LIMIT - 1),
                false,
                10,
            )
            .unwrap()
            .len(),
            1
        );
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
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0].guide_id().to_string(),
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e21"
        );
        assert_eq!(hits[0].matched_fields()[0], SearchMatchField::Title);
        // The exact-title phrase bonus keeps the exact title well above repeated body text.
        assert!(hits[0].score() >= hits[1].score() + EXACT_TITLE_PHRASE_BONUS as u16);
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
        assert!(!Arc::ptr_eq(
            &changed_before.record.backing,
            &changed_after.record.backing
        ));
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
        assert!(Arc::ptr_eq(
            &linux_before.record.backing,
            &linux_after.record.backing
        ));
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
            &archived_before.record.backing,
            &archived_after.record.backing
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

    fn ranking_workspace() -> (TestDirectory, Box<WorkspaceView>) {
        let temp = TestDirectory::new();
        write_workspace(temp.path());
        for (id, title, tags, body) in [
            (
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e41",
                "Docker prune images",
                "[docker, cleanup]",
                "## Prune\n\n```sh\ndocker system prune\n```\n",
            ),
            (
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e42",
                "WireGuard VPN setup",
                "[vpn]",
                "Configure the tunnel.\n",
            ),
            (
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e43",
                "Network cabling",
                "[]",
                "Run a wire to the switch.\n",
            ),
            (
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e44",
                "Docker volume cleanup",
                "[]",
                "Remove dangling volumes.\n",
            ),
            (
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e45",
                "Docker networking",
                "[]",
                "Each volume mounts here.\n",
            ),
        ] {
            guide(
                temp.path(),
                "desktop",
                id,
                title,
                "Ranking fixture",
                "[]",
                tags,
                "draft",
                body,
            );
        }
        let inspection = inspect_workspace(temp.path()).unwrap().into_parts().1;
        let WorkspaceInspection::Compatible(view) = inspection else {
            panic!("expected compatible Workspace")
        };
        (temp, view)
    }

    fn hit_ids(hits: &[SearchHit]) -> Vec<String> {
        hits.iter().map(|hit| hit.guide_id().to_string()).collect()
    }

    fn highlighted(hit: &SearchHit) -> Vec<String> {
        let characters = hit.excerpt().chars().collect::<Vec<_>>();
        hit.highlights()
            .iter()
            .map(|range| {
                characters[usize::from(range.start())..usize::from(range.end())]
                    .iter()
                    .collect()
            })
            .collect()
    }

    #[test]
    fn bounded_osa_distance_counts_transpositions_and_exits_early() {
        let mut rows = OsaRows::default();
        let distance = |left: &str, right: &str, max: usize, rows: &mut OsaRows| {
            let left = left.chars().collect::<Vec<_>>();
            let right = right.chars().collect::<Vec<_>>();
            bounded_osa_distance(&left, &right, max, rows)
        };
        assert_eq!(distance("dokcer", "docker", 1, &mut rows), Some(1));
        assert_eq!(distance("wiregaurd", "wireguard", 2, &mut rows), Some(1));
        assert_eq!(distance("dockr", "docker", 1, &mut rows), Some(1));
        assert_eq!(distance("dcoker", "docker", 1, &mut rows), Some(1));
        assert_eq!(distance("dockerr", "docker", 1, &mut rows), Some(1));
        assert_eq!(distance("dokcre", "docker", 2, &mut rows), Some(2));
        assert_eq!(distance("abcdef", "uvwxyz", 2, &mut rows), None);
        assert_eq!(distance("docker", "doc", 2, &mut rows), None);
        assert_eq!(distance("café", "cafe", 1, &mut rows), Some(1));
    }

    #[test]
    fn typo_queries_match_high_value_fields_and_highlight_the_matched_token() {
        let (_temp, workspace) = ranking_workspace();

        let hits = search_workspace(&workspace, "dokcer prune", false, 10).unwrap();
        assert_eq!(
            hit_ids(&hits),
            vec!["0199a1d7-7b0d-7d8e-a38d-f936c5735e41".to_owned()]
        );
        assert!(hits[0].matched_fields().contains(&SearchMatchField::Title));
        assert_eq!(hits[0].excerpt_field(), SearchMatchField::Title);
        assert_eq!(hits[0].excerpt(), "docker prune images");
        assert_eq!(highlighted(&hits[0]), vec!["docker", "prune"]);

        let hits = search_workspace(&workspace, "wiregaurd", false, 10).unwrap();
        assert_eq!(
            hit_ids(&hits),
            vec!["0199a1d7-7b0d-7d8e-a38d-f936c5735e42".to_owned()]
        );
        assert_eq!(highlighted(&hits[0]), vec!["wireguard"]);

        // A typo scores below the same Guide's exact match.
        let typo = search_workspace(&workspace, "dokcer prune", false, 10).unwrap();
        let exact = search_workspace(&workspace, "docker prune", false, 10).unwrap();
        assert_eq!(exact[0].guide_id(), typo[0].guide_id());
        assert!(exact[0].score() > typo[0].score());
    }

    fn hits_for(workspace: &WorkspaceView, query: &str) -> Vec<GuideId> {
        search_workspace(workspace, query, false, 10)
            .unwrap()
            .iter()
            .map(SearchHit::guide_id)
            .collect()
    }

    #[test]
    fn quoted_and_short_terms_never_match_fuzzily() {
        let (_temp, workspace) = ranking_workspace();
        for query in [
            "\"dokcer\"",
            "\"dokcer prune\"",
            "\"wiregaurd\"",
            "vpm",
            "dok",
            "prune \"dokcer\"",
        ] {
            assert!(hits_for(&workspace, query).is_empty(), "{query}");
        }
        // Unquoted terms of four or more characters are typo tolerant.
        assert_eq!(hits_for(&workspace, "dokcer").len(), 3);
        // Terms with punctuation or digits are exact-only.
        assert!(hits_for(&workspace, "dokcer-prune").is_empty());
        assert!(SearchQuery::parse("guide 04998").unwrap().fuzzy[1].is_none());
        assert!(SearchQuery::parse("dokcer2").unwrap().fuzzy[0].is_none());
        let parsed = SearchQuery::parse("wiregaurd dokcer").unwrap();
        assert_eq!(
            parsed
                .fuzzy
                .iter()
                .map(|term| term.as_ref().map(|term| term.max_distance))
                .collect::<Vec<_>>(),
            vec![Some(2), Some(1)]
        );
    }

    #[test]
    fn word_prefix_in_title_outranks_whole_word_in_body() {
        let (_temp, workspace) = ranking_workspace();
        let hits = search_workspace(&workspace, "wire", false, 10).unwrap();
        assert_eq!(
            hit_ids(&hits),
            vec![
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e42".to_owned(),
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e43".to_owned(),
            ]
        );
        assert!(hits[0].score() > hits[1].score());
        assert_eq!(hits[0].excerpt_field(), SearchMatchField::Title);
        assert_eq!(hits[1].excerpt_field(), SearchMatchField::Body);

        assert_eq!(occurrence_percent("wireguard vpn", "wire"), Some(85));
        assert_eq!(occurrence_percent("rewire it", "wire"), Some(50));
        assert_eq!(occurrence_percent("rewire a wire", "wire"), Some(100));
        assert_eq!(occurrence_percent("/etc/wire.conf", "/etc/wire"), Some(100));
        assert_eq!(occurrence_percent("cable", "wire"), None);
    }

    #[test]
    fn multi_term_scores_accumulate_across_terms() {
        let (_temp, workspace) = ranking_workspace();
        let hits = search_workspace(&workspace, "docker volume", false, 10).unwrap();
        assert_eq!(
            hit_ids(&hits),
            vec![
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e44".to_owned(),
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e45".to_owned(),
            ]
        );
        assert!(hits[0].score() > hits[1].score());
        // Without accumulation both would tie on their best single-field score.
        let single = search_workspace(&workspace, "docker", false, 10).unwrap();
        assert_eq!(single.len(), 3);
        assert!(hits[1].score() > single.iter().map(SearchHit::score).max().unwrap());

        // The Guide matching "docker" in title, tag, and command ranks first; equal scores
        // remain ordered by title, then Guide ID.
        assert_eq!(
            hit_ids(&single),
            vec![
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e41".to_owned(),
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e45".to_owned(),
                "0199a1d7-7b0d-7d8e-a38d-f936c5735e44".to_owned(),
            ]
        );
        assert_eq!(single[1].score(), single[2].score());
    }

    #[test]
    fn fuzzy_matching_work_is_bounded_per_guide() {
        const GUIDES: usize = 5_000;
        let crowded_heading = "worker ".repeat(600);
        let documents = (0..GUIDES)
            .map(|index| SearchFields {
                title: if index % 50 == 0 {
                    format!("docker host {index:05}")
                } else {
                    format!("service host {index:05}")
                },
                aliases: Vec::new(),
                commands: vec!["worker ".repeat(2_000)],
                paths: Vec::new(),
                errors: Vec::new(),
                tags: vec!["synthetic".to_owned()],
                summary: "synthetic guide".to_owned(),
                headings: vec![crowded_heading.clone()],
                body: "x".repeat(4_096),
            })
            .collect::<Vec<_>>();
        let query = SearchQuery::parse("dokcer").unwrap();
        let mut scratch = FuzzyScratch::default();
        let mut stats = SearchStats::default();
        let started = std::time::Instant::now();
        let matched = documents
            .iter()
            .filter(|fields| {
                fields
                    .evaluate(&query, "benchmark", &mut scratch, &mut stats)
                    .is_some()
            })
            .count();
        let elapsed = started.elapsed();
        println!(
            "fuzzy_guides={GUIDES} tokens={} comparisons={} elapsed_ms={:.1}",
            stats.fuzzy_tokens_visited,
            stats.fuzzy_comparisons,
            elapsed.as_secs_f64() * 1_000.0
        );
        assert_eq!(matched, GUIDES / 50);
        let bound = u64::try_from(GUIDES * MAX_FUZZY_TOKENS_PER_TERM).unwrap();
        assert!(stats.fuzzy_tokens_visited <= bound);
        assert!(stats.fuzzy_comparisons <= stats.fuzzy_tokens_visited);
        // Non-matching Guides exhaust the per-term token budget instead of scanning every token.
        assert!(stats.fuzzy_tokens_visited >= bound / 2);
        assert!(elapsed < std::time::Duration::from_secs(60));
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
        let previous_documents = workspace
            .search_index()
            .documents
            .iter()
            .map(|document| (document.guide_id, Arc::clone(document)))
            .collect::<BTreeMap<_, _>>();
        let started = std::time::Instant::now();
        let reconciled =
            SearchIndex::reconcile_prebuilt(refreshed.search_index(), workspace.search_index());
        let elapsed = started.elapsed();
        let reused = reconciled
            .documents
            .iter()
            .filter(|document| {
                previous_documents
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

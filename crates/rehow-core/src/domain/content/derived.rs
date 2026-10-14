//! Metadata derivation for hand-written Markdown Guides.
//!
//! A Guide file whose frontmatter is absent or incomplete is still listed and
//! readable. Its missing metadata is derived from the body, the filename, and
//! the file modification time, and is never written until the user explicitly
//! saves the Guide (adoption). Derivation is pure: it reads only the bytes and
//! context it is given.

use crate::{
    parsing::{ParseFailureKind, SourceLineEnding, parse_guide_source, scan_guide_envelope},
    protocol::{CategoryId, GuideId, Timestamp, WorkspaceId, derived_guide_id},
};

use super::{
    GuideDiagnostic, GuideDiagnosticCode, GuideMetadata,
    guide::{
        BoundGuide, Guide, GuideDefaults, bind_guide, bind_guide_with_defaults, guide_from_parts,
        missing_required_fields,
    },
};

const BOM: &[u8] = b"\xef\xbb\xbf";
const TITLE_LIMIT: usize = 200;
const SUMMARY_LIMIT: usize = 500;
const FALLBACK_TITLE: &str = "Untitled Guide";

/// Whether a listed Guide is fully managed or was derived from a
/// hand-written file that Rehow has not yet written to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum GuideOrigin {
    /// The file has complete, valid Rehow frontmatter.
    Managed,
    /// Some or all metadata is derived; the file is adopted on first save.
    Unadopted(UnadoptedReasons),
}

impl GuideOrigin {
    pub fn is_managed(self) -> bool {
        matches!(self, Self::Managed)
    }
}

/// Why a Guide is unadopted. More than one reason is never set today, but the
/// set is a bitset so adapters can treat it uniformly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct UnadoptedReasons(u8);

impl UnadoptedReasons {
    const NO_FRONTMATTER: u8 = 1;
    const UNTERMINATED_FRONTMATTER: u8 = 1 << 1;
    const NON_MAPPING_FRONTMATTER: u8 = 1 << 2;
    const MISSING_FIELDS: u8 = 1 << 3;

    /// The file does not start with a `---` line.
    pub fn no_frontmatter(self) -> bool {
        self.0 & Self::NO_FRONTMATTER != 0
    }

    /// The file starts with `---` but has no closing `---` line; the whole
    /// file is treated as body.
    pub fn unterminated_frontmatter(self) -> bool {
        self.0 & Self::UNTERMINATED_FRONTMATTER != 0
    }

    /// The `---` block is not a YAML mapping; the whole file is body.
    pub fn non_mapping_frontmatter(self) -> bool {
        self.0 & Self::NON_MAPPING_FRONTMATTER != 0
    }

    /// The frontmatter is a valid mapping that lacks required Rehow fields.
    pub fn missing_fields(self) -> bool {
        self.0 & Self::MISSING_FIELDS != 0
    }

    /// Whether the whole file (minus a leading BOM) is the body.
    pub(crate) fn whole_file_body(self) -> bool {
        !self.missing_fields()
    }
}

pub(crate) struct DeriveContext<'a> {
    pub(crate) workspace_id: &'a WorkspaceId,
    pub(crate) relative_path: &'a str,
    pub(crate) category_id: CategoryId,
    pub(crate) modified_at: Timestamp,
}

pub(crate) enum DerivedGuide {
    Managed(Box<BoundGuide>),
    Unadopted(UnadoptedGuide),
    Rejected(GuideDiagnostic),
}

pub(crate) struct UnadoptedGuide {
    reasons: UnadoptedReasons,
    source: UnadoptedSource,
}

enum UnadoptedSource {
    /// No usable frontmatter: the body is the whole file after any BOM.
    WholeFile { bom: bool, guide: Box<Guide> },
    /// A frontmatter block holding only blank lines and comments. Fields are
    /// inserted before `insert_at` (the closing delimiter).
    EmptyFrontmatter {
        insert_at: usize,
        line_ending: SourceLineEnding,
        guide: Box<Guide>,
    },
    /// A valid mapping with absent required fields, bound with defaults.
    Partial(Box<BoundGuide>),
}

impl UnadoptedGuide {
    pub(crate) fn reasons(&self) -> UnadoptedReasons {
        self.reasons
    }

    pub(crate) fn guide(&self) -> &Guide {
        match &self.source {
            UnadoptedSource::WholeFile { guide, .. }
            | UnadoptedSource::EmptyFrontmatter { guide, .. } => guide,
            UnadoptedSource::Partial(bound) => bound.record(),
        }
    }

    pub(crate) fn into_guide(self) -> Guide {
        match self.source {
            UnadoptedSource::WholeFile { guide, .. }
            | UnadoptedSource::EmptyFrontmatter { guide, .. } => *guide,
            UnadoptedSource::Partial(bound) => (*bound).into_record(),
        }
    }

    /// For a whole-file body, whether the file starts with a BOM.
    pub(crate) fn whole_file_bom(&self) -> Option<bool> {
        match &self.source {
            UnadoptedSource::WholeFile { bom, .. } => Some(*bom),
            UnadoptedSource::Partial(_) | UnadoptedSource::EmptyFrontmatter { .. } => None,
        }
    }

    /// For partial frontmatter, the bound source with derived defaults.
    pub(crate) fn partial(&self) -> Option<&BoundGuide> {
        match &self.source {
            UnadoptedSource::Partial(bound) => Some(bound),
            UnadoptedSource::WholeFile { .. } | UnadoptedSource::EmptyFrontmatter { .. } => None,
        }
    }

    /// For a comment-only frontmatter block, where to insert fields and the
    /// frontmatter line ending.
    pub(crate) fn empty_frontmatter(&self) -> Option<(usize, SourceLineEnding)> {
        match &self.source {
            UnadoptedSource::EmptyFrontmatter {
                insert_at,
                line_ending,
                ..
            } => Some((*insert_at, *line_ending)),
            UnadoptedSource::WholeFile { .. } | UnadoptedSource::Partial(_) => None,
        }
    }
}

/// Classify and bind one Guide file.
///
/// * strict parse and bind succeed: `Managed`;
/// * no opening `---`, no closing `---`, or a non-mapping block: `Unadopted`
///   with the whole file (minus BOM) as body;
/// * a valid mapping missing only required fields: `Unadopted` using every
///   present field, each validated strictly;
/// * anything else (invalid UTF-8, YAML syntax, an invalid Rehow field such
///   as `status: tested` without verification): `Rejected`.
pub(crate) fn derive_guide(source: Box<[u8]>, context: &DeriveContext<'_>) -> DerivedGuide {
    let Ok(text) = core::str::from_utf8(&source) else {
        return DerivedGuide::Rejected(GuideDiagnostic::new(
            GuideDiagnosticCode::InvalidUtf8,
            None,
            None,
        ));
    };
    let bom = source.starts_with(BOM);
    let content = &text[if bom { BOM.len() } else { 0 }..];
    let opened = content.starts_with("---\n") || content.starts_with("---\r\n");
    if !opened {
        return whole_file(&source, bom, UnadoptedReasons::NO_FRONTMATTER, context);
    }
    let after_opening = content.split_once('\n').map_or("", |(_, rest)| rest);
    if !has_closing_line(after_opening) {
        return whole_file(
            &source,
            bom,
            UnadoptedReasons::UNTERMINATED_FRONTMATTER,
            context,
        );
    }

    // The YAML parser has no document for a block holding only blank lines
    // and comments; such a block is an empty mapping with every field absent.
    if let Ok(envelope) = scan_guide_envelope(&source) {
        let yaml_range = envelope.yaml_file_range();
        let yaml = &text[yaml_range.start().get() as usize..yaml_range.end().get() as usize];
        if yaml
            .lines()
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
        {
            let body_range = envelope.body_file_range();
            let body = &source[body_range.start().get() as usize..body_range.end().get() as usize];
            let body_text = core::str::from_utf8(body).unwrap_or_default();
            let mut copy = Vec::new();
            if copy.try_reserve_exact(body.len()).is_err() {
                return resource_limit();
            }
            copy.extend_from_slice(body);
            let metadata = GuideMetadata::derived(
                derived_id(context),
                derive_title(body_text, context.relative_path),
                derive_summary(body_text),
                context.category_id,
                context.modified_at,
            );
            return DerivedGuide::Unadopted(UnadoptedGuide {
                reasons: UnadoptedReasons(UnadoptedReasons::MISSING_FIELDS),
                source: UnadoptedSource::EmptyFrontmatter {
                    insert_at: envelope.closing_delimiter().start().get() as usize,
                    line_ending: envelope.yaml_line_ending(),
                    guide: Box::new(guide_from_parts(metadata, copy.into_boxed_slice())),
                },
            });
        }
    }

    let mut copy = Vec::new();
    if copy.try_reserve_exact(source.len()).is_err() {
        return resource_limit();
    }
    copy.extend_from_slice(&source);
    let parsed = match parse_guide_source(source) {
        Ok(parsed) => parsed,
        Err(error) if error.kind() == ParseFailureKind::RootMustBeMapping => {
            return whole_file(
                &copy,
                bom,
                UnadoptedReasons::NON_MAPPING_FRONTMATTER,
                context,
            );
        }
        Err(error) => {
            return DerivedGuide::Rejected(GuideDiagnostic::from_guide_parse_failure(&error));
        }
    };
    if !missing_required_fields(&parsed) {
        return match bind_guide(parsed) {
            Ok(bound) => DerivedGuide::Managed(Box::new(bound)),
            Err(error) => DerivedGuide::Rejected(GuideDiagnostic::from_guide_bind_failure(&error)),
        };
    }
    let body = core::str::from_utf8(parsed.body_bytes()).unwrap_or_default();
    let defaults = GuideDefaults {
        id: derived_id(context),
        title: derive_title(body, context.relative_path),
        summary: derive_summary(body),
        category_id: context.category_id,
        modified_at: context.modified_at,
    };
    match bind_guide_with_defaults(parsed, &defaults) {
        Ok(bound) => DerivedGuide::Unadopted(UnadoptedGuide {
            reasons: UnadoptedReasons(UnadoptedReasons::MISSING_FIELDS),
            source: UnadoptedSource::Partial(Box::new(bound)),
        }),
        Err(error) => DerivedGuide::Rejected(GuideDiagnostic::from_guide_bind_failure(&error)),
    }
}

fn whole_file(source: &[u8], bom: bool, reason: u8, context: &DeriveContext<'_>) -> DerivedGuide {
    let body = &source[if bom { BOM.len() } else { 0 }..];
    let text = core::str::from_utf8(body).unwrap_or_default();
    let mut copy = Vec::new();
    if copy.try_reserve_exact(body.len()).is_err() {
        return resource_limit();
    }
    copy.extend_from_slice(body);
    let metadata = GuideMetadata::derived(
        derived_id(context),
        derive_title(text, context.relative_path),
        derive_summary(text),
        context.category_id,
        context.modified_at,
    );
    DerivedGuide::Unadopted(UnadoptedGuide {
        reasons: UnadoptedReasons(reason),
        source: UnadoptedSource::WholeFile {
            bom,
            guide: Box::new(guide_from_parts(metadata, copy.into_boxed_slice())),
        },
    })
}

fn resource_limit() -> DerivedGuide {
    DerivedGuide::Rejected(GuideDiagnostic::new(
        GuideDiagnosticCode::ResourceLimit,
        None,
        None,
    ))
}

/// Whether any later line is exactly `---` (LF, CRLF, or at end of file).
fn has_closing_line(text: &str) -> bool {
    text.split('\n')
        .any(|line| line.strip_suffix('\r').unwrap_or(line) == "---")
}

/// The stable id of an unadopted Guide without an `id` field. It depends on
/// the Workspace and the relative path only, so a rename changes it. It is
/// never written to disk: adoption mints a fresh UUIDv7.
pub(crate) fn derived_id(context: &DeriveContext<'_>) -> GuideId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"rehow-derived-guide-v1\0");
    hasher.update(context.workspace_id.to_string().as_bytes());
    hasher.update(context.relative_path.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    derived_guide_id(bytes)
}

/// The first level-one ATX heading outside fenced code, else the humanized
/// filename, else a fixed fallback. Always a valid Guide title.
pub(crate) fn derive_title(body: &str, relative_path: &str) -> String {
    let filename = relative_path.rsplit('/').next().unwrap_or(relative_path);
    first_heading(body)
        .map(|heading| collapse_text(heading, false, TITLE_LIMIT))
        .filter(|title| super::valid_text(title, TITLE_LIMIT))
        .or_else(|| humanized_filename_title(filename))
        .unwrap_or_else(|| FALLBACK_TITLE.to_owned())
}

/// The first paragraph outside headings and fenced code, collapsed to one
/// line and capped at 500 scalars. May be empty.
pub(crate) fn derive_summary(body: &str) -> String {
    let mut fence = None;
    let mut paragraph = String::new();
    for line in body.lines() {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        if let Some((marker, length)) = fence {
            if indent <= 3 && closes_fence(trimmed, marker, length) {
                fence = None;
            }
            continue;
        }
        if line.trim().is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        if indent <= 3 {
            if let Some(opened) = opens_fence(trimmed) {
                if !paragraph.is_empty() {
                    break;
                }
                fence = Some(opened);
                continue;
            }
            if trimmed.starts_with('#') || thematic_break(trimmed) {
                if !paragraph.is_empty() {
                    break;
                }
                continue;
            }
        }
        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(line);
        if paragraph.len() > SUMMARY_LIMIT * 8 {
            break;
        }
    }
    let summary = collapse_text(&paragraph, false, SUMMARY_LIMIT);
    if summary.is_empty() || super::valid_text(&summary, SUMMARY_LIMIT) {
        summary
    } else {
        String::new()
    }
}

fn first_heading(body: &str) -> Option<&str> {
    let mut fence = None;
    for line in body.lines() {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        if let Some((marker, length)) = fence {
            if indent <= 3 && closes_fence(trimmed, marker, length) {
                fence = None;
            }
            continue;
        }
        if indent > 3 {
            continue;
        }
        if let Some(opened) = opens_fence(trimmed) {
            fence = Some(opened);
            continue;
        }
        let Some(rest) = trimmed.strip_prefix('#') else {
            continue;
        };
        if rest.starts_with('#') {
            continue;
        }
        let content = if rest.is_empty() {
            rest
        } else if let Some(content) = rest.strip_prefix([' ', '\t']) {
            content
        } else {
            continue;
        };
        let content = content.trim_end_matches(['\r', ' ', '\t']);
        // An optional closing sequence of `#` preceded by a space.
        let without_closing = content.trim_end_matches('#');
        let content = if without_closing.len() < content.len()
            && (without_closing.is_empty() || without_closing.ends_with([' ', '\t']))
        {
            without_closing
        } else {
            content
        };
        let content = content.trim();
        if !content.is_empty() {
            return Some(content);
        }
    }
    None
}

fn opens_fence(line: &str) -> Option<(char, usize)> {
    let marker = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let length = line.chars().take_while(|c| *c == marker).count();
    if length < 3 {
        return None;
    }
    if marker == '`' && line[length..].contains('`') {
        return None;
    }
    Some((marker, length))
}

fn closes_fence(line: &str, marker: char, length: usize) -> bool {
    let count = line.chars().take_while(|c| *c == marker).count();
    count >= length && line[count * marker.len_utf8()..].trim().is_empty()
}

fn thematic_break(line: &str) -> bool {
    let compact = line
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    compact.len() >= 3
        && ['-', '*', '_']
            .iter()
            .any(|marker| compact.chars().all(|c| c == *marker))
}

/// A humanized filename title: the stem with `-`, `_`, whitespace,
/// controls, and invisible or spoofing scalars collapsed to single spaces,
/// capped at 200 scalars. `None` when nothing printable remains.
pub(crate) fn humanized_filename_title(filename: &str) -> Option<String> {
    let stem = filename.strip_suffix(".md").unwrap_or(filename);
    let title = collapse_text(stem, true, TITLE_LIMIT);
    super::valid_text(&title, TITLE_LIMIT).then_some(title)
}

fn collapse_text(value: &str, filename: bool, limit: usize) -> String {
    let mut output = String::new();
    let mut separator = false;
    let mut length = 0usize;
    for character in value.chars() {
        if (filename && (character == '-' || character == '_'))
            || character.is_whitespace()
            || character.is_control()
            || unsafe_title_scalar(character)
        {
            separator = !output.is_empty();
        } else {
            if separator && length < limit {
                output.push(' ');
                length += 1;
            }
            separator = false;
            if length < limit {
                output.push(character);
                length += 1;
            }
        }
    }
    while output.ends_with(' ') {
        output.pop();
    }
    output
}

pub(crate) fn unsafe_title_scalar(character: char) -> bool {
    let scalar = character as u32;
    matches!(
        scalar,
        0x00ad
            | 0x034f
            | 0x061c
            | 0x115f..=0x1160
            | 0x17b4..=0x17b5
            | 0x180b..=0x180f
            | 0x200b..=0x200f
            | 0x202a..=0x202e
            | 0x2060..=0x206f
            | 0x3164
            | 0xfe00..=0xfe0f
            | 0xfeff
            | 0xffa0
            | 0xfff9..=0xfffb
            | 0x1bca0..=0x1bca3
            | 0x1d173..=0x1d17a
            | 0xe0001
            | 0xe0020..=0xe007f
            | 0xe0100..=0xe01ef
            | 0xfdd0..=0xfdef
    ) || matches!(scalar & 0xffff, 0xfffe | 0xffff)
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::*;
    use crate::domain::GuideStatus;

    const VALID: &str = "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: T\nsummary: S\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags: []\naliases: []\nstatus: draft\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n";

    fn workspace() -> WorkspaceId {
        WorkspaceId::from_str("0199a19f-4d15-7ca7-a645-e75311caa710").unwrap()
    }

    fn category() -> CategoryId {
        CategoryId::from_str("0199a1c2-d334-7d17-bd9c-f8b6ba3a5551").unwrap()
    }

    fn mtime() -> Timestamp {
        Timestamp::from_str("2026-09-01T08:00:00Z").unwrap()
    }

    fn derive_at(source: &str, path: &str) -> DerivedGuide {
        let workspace = workspace();
        derive_guide(
            source.as_bytes().to_vec().into_boxed_slice(),
            &DeriveContext {
                workspace_id: &workspace,
                relative_path: path,
                category_id: category(),
                modified_at: mtime(),
            },
        )
    }

    fn derive(source: &str) -> DerivedGuide {
        derive_at(source, "categories/linux/my-notes.md")
    }

    fn unadopted(source: &str) -> UnadoptedGuide {
        match derive(source) {
            DerivedGuide::Unadopted(value) => value,
            DerivedGuide::Managed(_) => panic!("managed"),
            DerivedGuide::Rejected(diagnostic) => panic!("rejected {diagnostic:?}"),
        }
    }

    fn rejected(source: &str) -> GuideDiagnostic {
        match derive(source) {
            DerivedGuide::Rejected(value) => value,
            _ => panic!("not rejected"),
        }
    }

    #[test]
    fn complete_frontmatter_is_managed() {
        assert!(matches!(
            derive(&format!("---\n{VALID}---\n\nBody\n")),
            DerivedGuide::Managed(_)
        ));
        assert!(matches!(
            derive(&format!("---\n{VALID}---\nBody\n")),
            DerivedGuide::Managed(_)
        ));
    }

    #[test]
    fn plain_markdown_derives_draft_metadata_from_body_and_mtime() {
        let guide = unadopted(
            "Intro text\n\n# Install `tool`  #\n\nRun the\n  installer.\n\n```sh\n# not a title\n```\n",
        );
        assert!(guide.reasons().no_frontmatter());
        assert!(guide.reasons().whole_file_body());
        assert_eq!(guide.whole_file_bom(), Some(false));
        let record = guide.guide();
        assert_eq!(record.title(), "Install `tool`");
        assert_eq!(record.summary(), "Intro text");
        assert_eq!(record.declared_status(), GuideStatus::Draft);
        assert!(record.tags().is_empty() && record.aliases().is_empty());
        assert_eq!(record.created_at(), &mtime());
        assert_eq!(record.updated_at(), &mtime());
        assert_eq!(record.category_id(), &category());
        assert!(record.body().starts_with(b"Intro text\n"));
    }

    #[test]
    fn heading_inside_fence_is_ignored_and_filename_is_the_fallback() {
        let guide = unadopted("```\n# fenced\n```\n\n## Second level\n\nText.\n");
        assert_eq!(guide.guide().title(), "my notes");
        assert_eq!(guide.guide().summary(), "Text.");
        let empty = unadopted("");
        assert_eq!(empty.guide().summary(), "");
        assert_eq!(empty.guide().title(), "my notes");
        let DerivedGuide::Unadopted(spoofed) = derive_at("x\n", "categories/l/\u{202e}\u{200b}.md")
        else {
            panic!("unadopted")
        };
        assert_eq!(spoofed.guide().title(), "Untitled Guide");
        let long = unadopted(&format!("# {}\n\n{}\n", "t".repeat(300), "s".repeat(600)));
        assert_eq!(long.guide().title().chars().count(), 200);
        assert_eq!(long.guide().summary().chars().count(), 500);
    }

    #[test]
    fn bom_is_excluded_from_whole_file_body() {
        let guide = unadopted("\u{feff}# Title\r\n\r\nText\r\n");
        assert_eq!(guide.whole_file_bom(), Some(true));
        assert_eq!(guide.guide().body(), b"# Title\r\n\r\nText\r\n");
        assert_eq!(guide.guide().title(), "Title");
    }

    #[test]
    fn unterminated_and_non_mapping_blocks_become_whole_file_bodies() {
        let unterminated = unadopted("---\ntitle: x\n\n# Heading\n");
        assert!(unterminated.reasons().unterminated_frontmatter());
        assert_eq!(unterminated.guide().body(), b"---\ntitle: x\n\n# Heading\n");
        let list = unadopted("---\n- a\n- b\n---\n# Heading\n");
        assert!(list.reasons().non_mapping_frontmatter());
        assert_eq!(list.guide().body(), b"---\n- a\n- b\n---\n# Heading\n");
        assert_eq!(list.guide().title(), "Heading");
    }

    #[test]
    fn partial_frontmatter_keeps_present_fields_and_derives_the_rest() {
        let guide = unadopted(
            "---\ntitle: Kept title\ntags: [linux]\ncreated_at: 2026-08-01T00:00:00Z\nlayout: post\n---\n# Body title\n\nFirst para.\n",
        );
        assert!(guide.reasons().missing_fields());
        assert!(!guide.reasons().whole_file_body());
        assert!(guide.partial().is_some());
        let record = guide.guide();
        assert_eq!(record.title(), "Kept title");
        assert_eq!(record.summary(), "First para.");
        assert_eq!(record.tags(), ["linux".to_owned()]);
        assert_eq!(
            record.created_at(),
            &Timestamp::from_str("2026-08-01T00:00:00Z").unwrap()
        );
        assert_eq!(record.updated_at(), &mtime());
        assert_eq!(record.body(), b"# Body title\n\nFirst para.\n");

        let with_id = unadopted("---\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\n---\nBody\n");
        assert_eq!(
            with_id.guide().id(),
            &GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap()
        );
    }

    #[test]
    fn comment_only_frontmatter_is_an_empty_mapping() {
        for source in ["---\n---\nBody\n", "---\n# note\n\n---\n\nBody\n"] {
            let guide = unadopted(source);
            assert!(guide.reasons().missing_fields());
            assert!(guide.empty_frontmatter().is_some());
            assert_eq!(guide.guide().body(), b"Body\n");
        }
    }

    #[test]
    fn invalid_reserved_fields_and_syntax_are_rejected_with_diagnostics() {
        let status = rejected("---\nstatus: tested\n---\nBody\n");
        assert_eq!(status.code(), GuideDiagnosticCode::InvalidFieldValue);
        let tags = rejected("---\ntags: linux\n---\nBody\n");
        assert_eq!(tags.code(), GuideDiagnosticCode::InvalidFieldType);
        assert_eq!(
            tags.field(),
            Some(crate::domain::GuideDiagnosticField::Tags)
        );
        let id = rejected("---\nid: not-an-id\n---\nBody\n");
        assert_eq!(id.code(), GuideDiagnosticCode::InvalidFieldValue);
        let syntax = rejected("---\ntitle: [x\n---\nBody\n");
        assert_eq!(syntax.code(), GuideDiagnosticCode::YamlSyntax);
        let mut invalid = b"# T\n".to_vec();
        invalid.push(0xff);
        let workspace = workspace();
        assert!(matches!(
            derive_guide(
                invalid.into_boxed_slice(),
                &DeriveContext {
                    workspace_id: &workspace,
                    relative_path: "categories/l/a.md",
                    category_id: category(),
                    modified_at: mtime(),
                }
            ),
            DerivedGuide::Rejected(diagnostic)
                if diagnostic.code() == GuideDiagnosticCode::InvalidUtf8
        ));
    }

    #[test]
    fn derived_ids_are_stable_uuid_v7_shaped_and_path_bound() {
        let first = unadopted("# A\n").guide().id().to_string();
        let again = unadopted("# Changed body\n").guide().id().to_string();
        assert_eq!(first, again);
        assert!(first.starts_with("00000000-0000-7"));
        assert!(GuideId::from_str(&first).is_ok());
        let DerivedGuide::Unadopted(renamed) = derive_at("# A\n", "categories/linux/other.md")
        else {
            panic!("unadopted")
        };
        assert_ne!(renamed.guide().id().to_string(), first);
    }
}

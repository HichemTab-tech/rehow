use core::fmt::{self, Write as _};

use crate::{
    parsing::{
        CollectionPresentationStyle, ParseFailure, ParseFailureKind, SourceLineEnding,
        parse_guide_source, parse_yaml_document,
    },
    protocol::{CategoryId, GuideId, Timestamp, WorkspaceId},
};

use super::{
    BoundCategory, BoundWorkspaceManifest, CategoryStatus, GuideStatus, ManifestBindingOutcome,
    bind_category, bind_workspace_manifest,
    derived::UnadoptedGuide,
    guide::{BoundGuide, bind_guide},
};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum EmitFailureKind {
    InvalidValue,
    ResourceLimit,
    CandidateMismatch,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum EmitField {
    Name,
    Summary,
    Title,
    Body,
    Document,
}

pub(crate) struct EmitFailure {
    kind: EmitFailureKind,
    field: EmitField,
}

impl EmitFailure {
    pub(crate) fn kind(&self) -> EmitFailureKind {
        self.kind
    }

    pub(crate) fn field(&self) -> EmitField {
        self.field
    }
}

pub(super) fn emit_yaml_string(value: &str) -> Result<Box<[u8]>, EmitFailure> {
    let mut output = EmitBuffer::new();
    output.quoted(value)?;
    Ok(output.finish())
}

pub(super) fn emit_yaml_display(value: impl fmt::Display) -> Result<Box<[u8]>, EmitFailure> {
    let mut output = EmitBuffer::new();
    output.quoted_display(value)?;
    Ok(output.finish())
}

pub(super) fn emit_plain_display(value: impl fmt::Display) -> Result<Box<[u8]>, EmitFailure> {
    let mut output = EmitBuffer::new();
    output.display(value)?;
    Ok(output.finish())
}

pub(crate) fn emit_new_workspace_manifest(
    workspace_id: &WorkspaceId,
    name: &str,
    timestamp: &Timestamp,
    default_category_id: Option<&CategoryId>,
) -> Result<BoundWorkspaceManifest, EmitFailure> {
    if !super::valid_text(name, 200) {
        return Err(invalid(EmitField::Name));
    }

    let mut output = EmitBuffer::new();
    output.raw("schema_version: 1\n")?;
    output.field_display("workspace_id", workspace_id)?;
    output.field_string("name", name)?;
    output.field_display("created_at", timestamp)?;
    output.field_display("updated_at", timestamp)?;
    if let Some(category_id) = default_category_id {
        output.raw("settings:\n  default_category_id: ")?;
        output.quoted_display(category_id)?;
        output.raw("\n")?;
    }

    let source = output.finish();
    let parsed = parse_yaml_document(source).map_err(map_parse_failure)?;
    let outcome = bind_workspace_manifest(parsed).map_err(|_| mismatch(EmitField::Document))?;
    let ManifestBindingOutcome::Current(bound) = outcome else {
        return Err(mismatch(EmitField::Document));
    };
    let record = bound.record();
    if record.workspace_id() != workspace_id
        || record.name() != name
        || record.created_at() != timestamp
        || record.updated_at() != timestamp
        || record
            .settings()
            .and_then(|settings| settings.default_category_id())
            != default_category_id
        || !bound.unknown_root_entries().is_empty()
        || !bound.unknown_settings_entries().is_empty()
    {
        return Err(mismatch(EmitField::Document));
    }
    Ok(bound)
}

pub(crate) fn emit_new_category(
    id: &CategoryId,
    name: &str,
    summary: Option<&str>,
    order: i64,
    timestamp: &Timestamp,
) -> Result<BoundCategory, EmitFailure> {
    if !super::valid_text(name, 200) {
        return Err(invalid(EmitField::Name));
    }
    if summary.is_some_and(|value| !super::valid_text(value, 500)) {
        return Err(invalid(EmitField::Summary));
    }

    let mut output = EmitBuffer::new();
    output.raw("schema_version: 1\n")?;
    output.field_display("id", id)?;
    output.field_string("name", name)?;
    if let Some(value) = summary {
        output.field_string("summary", value)?;
    }
    output.raw("order: ")?;
    output.display(order)?;
    output.raw("\nstatus: active\n")?;
    output.field_display("created_at", timestamp)?;
    output.field_display("updated_at", timestamp)?;

    let source = output.finish();
    let parsed = parse_yaml_document(source).map_err(map_parse_failure)?;
    let bound = bind_category(parsed).map_err(|_| mismatch(EmitField::Document))?;
    let record = bound.record();
    if record.id() != id
        || record.name() != name
        || record.summary() != summary
        || record.order() != order
        || record.status() != CategoryStatus::Active
        || record.created_at() != timestamp
        || record.updated_at() != timestamp
        || !bound.unknown_root_entries().is_empty()
    {
        return Err(mismatch(EmitField::Document));
    }
    Ok(bound)
}

pub(crate) fn emit_new_guide(
    id: &GuideId,
    title: &str,
    summary: &str,
    category_id: &CategoryId,
    timestamp: &Timestamp,
    body: &[u8],
) -> Result<BoundGuide, EmitFailure> {
    if !super::valid_text(title, 200) {
        return Err(invalid(EmitField::Title));
    }
    if !super::valid_text(summary, 500) {
        return Err(invalid(EmitField::Summary));
    }
    let body_text = canonical_body(body)?;

    let mut output = EmitBuffer::new();
    output.raw("---\nschema_version: 1\n")?;
    output.field_display("id", id)?;
    output.field_string("title", title)?;
    output.field_string("summary", summary)?;
    output.field_display("category_id", category_id)?;
    output.raw("tags: []\naliases: []\nstatus: draft\n")?;
    output.field_display("created_at", timestamp)?;
    output.field_display("updated_at", timestamp)?;
    output.raw("---\n\n")?;
    output.raw(body_text)?;

    let source = output.finish();
    let parsed = parse_guide_source(source).map_err(map_parse_failure)?;
    let bound = bind_guide(parsed).map_err(|_| mismatch(EmitField::Document))?;
    let record = bound.record();
    if record.id() != id
        || record.title() != title
        || record.summary() != summary
        || record.category_id() != category_id
        || !record.tags().is_empty()
        || !record.aliases().is_empty()
        || record.declared_status() != GuideStatus::Draft
        || record.created_at() != timestamp
        || record.updated_at() != timestamp
        || record.body() != body
        || record.verification().is_some()
        || record.archive().is_some()
        || record.applies_to().is_some()
        || record.prerequisites().is_some()
        || record.warnings().is_some()
        || record.files_affected().is_some()
        || record.related_guide_ids().is_some()
        || record.references().is_some()
        || record.attachments().is_some()
        || !bound.unknown_partition().root().is_empty()
    {
        return Err(mismatch(EmitField::Document));
    }
    Ok(bound)
}

/// Values supplied by the application for adopting a hand-written Guide.
pub(crate) struct AdoptionInput<'a> {
    /// A freshly minted UUIDv7, used only when the file has no `id`.
    pub(crate) fresh_id: &'a GuideId,
    /// The file modification time in whole seconds.
    pub(crate) modified_at: &'a Timestamp,
    /// The trusted current time.
    pub(crate) now: &'a Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdoptionFailure {
    /// The frontmatter presentation cannot be extended safely (flow-style or
    /// indented root mapping).
    Unsupported,
    /// The adopted candidate did not rebind to exactly the expected values.
    Mismatch,
    ResourceLimit,
}

/// Emit the adopted bytes for an unadopted Guide, then strictly rebind them.
///
/// * Whole-file body: keep a leading BOM, emit canonical frontmatter and one
///   blank line, then the original body bytes unchanged (CRLF, trailing
///   newlines, and any leading `---` text included).
/// * Partial frontmatter: keep every existing YAML byte and insert only the
///   missing required keys immediately before the closing `---`, using the
///   frontmatter line ending. Flow-style or indented root mappings are
///   refused. The body and separator layout are untouched.
///
/// The derived id is never written: a missing `id` becomes `fresh_id`.
/// `created_at` is `min(mtime, now)` (and never after a present
/// `updated_at`); a missing `updated_at` is `now` (and never before
/// `created_at`). An empty derived summary is replaced by the title.
pub(crate) fn emit_adopted_guide(
    unadopted: &UnadoptedGuide,
    source: &[u8],
    input: &AdoptionInput<'_>,
) -> Result<BoundGuide, AdoptionFailure> {
    let derived = unadopted.guide();
    let summary = if derived.summary().is_empty() {
        derived.title()
    } else {
        derived.summary()
    };
    let created_floor = core::cmp::min(*input.modified_at, *input.now);
    let candidate = if let Some(bom) = unadopted.whole_file_bom() {
        let body_start = if bom { 3 } else { 0 };
        let body = source.get(body_start..).ok_or(AdoptionFailure::Mismatch)?;
        if body != derived.body() {
            return Err(AdoptionFailure::Mismatch);
        }
        let mut fields = EmitBuffer::new();
        fields.raw("---\n")?;
        emit_adoption_fields(
            &mut fields,
            &AdoptionFields {
                missing: &|_| true,
                id: input.fresh_id,
                title: derived.title(),
                summary,
                category_id: derived.category_id(),
                created_at: &created_floor,
                updated_at: input.now,
            },
        )?;
        fields.raw("---\n\n")?;
        let fields = fields.finish();
        let mut output = Vec::new();
        output
            .try_reserve_exact(body_start + fields.len() + body.len())
            .map_err(|_| AdoptionFailure::ResourceLimit)?;
        if bom {
            output.extend_from_slice(&source[..3]);
        }
        output.extend_from_slice(&fields);
        output.extend_from_slice(body);
        output.into_boxed_slice()
    } else if let Some((insert_at, line_ending)) = unadopted.empty_frontmatter() {
        let mut fields = EmitBuffer::new();
        emit_adoption_fields(
            &mut fields,
            &AdoptionFields {
                missing: &|_| true,
                id: input.fresh_id,
                title: derived.title(),
                summary,
                category_id: derived.category_id(),
                created_at: &created_floor,
                updated_at: input.now,
            },
        )?;
        splice_fields(source, insert_at, &fields.finish(), line_ending)?
    } else {
        let partial = unadopted.partial().ok_or(AdoptionFailure::Mismatch)?;
        if partial.source_bytes() != source {
            return Err(AdoptionFailure::Mismatch);
        }
        insert_missing_fields(partial, input, summary, &created_floor)?
    };

    let parsed = parse_guide_source(candidate).map_err(|error| {
        if map_parse_failure(error).kind() == EmitFailureKind::ResourceLimit {
            AdoptionFailure::ResourceLimit
        } else {
            AdoptionFailure::Mismatch
        }
    })?;
    let bound = bind_guide(parsed).map_err(|_| AdoptionFailure::Mismatch)?;
    let record = bound.record();
    let id_matches = match unadopted.partial() {
        Some(partial) if root_key_present(partial, "id") => record.id() == derived.id(),
        _ => record.id() == input.fresh_id,
    };
    if !id_matches
        || record.title() != derived.title()
        || (record.summary() != derived.summary() && !derived.summary().is_empty())
        || record.category_id() != derived.category_id()
        || record.tags() != derived.tags()
        || record.aliases() != derived.aliases()
        || record.declared_status() != derived.declared_status()
        || record.body() != derived.body()
        || record.created_at() > record.updated_at()
    {
        return Err(AdoptionFailure::Mismatch);
    }
    Ok(bound)
}

struct AdoptionFields<'a> {
    missing: &'a dyn Fn(&str) -> bool,
    id: &'a GuideId,
    title: &'a str,
    summary: &'a str,
    category_id: &'a CategoryId,
    created_at: &'a Timestamp,
    updated_at: &'a Timestamp,
}

fn emit_adoption_fields(
    output: &mut EmitBuffer,
    fields: &AdoptionFields<'_>,
) -> Result<(), AdoptionFailure> {
    let missing = fields.missing;
    if missing("schema_version") {
        output.raw("schema_version: 1\n")?;
    }
    if missing("id") {
        output.field_display("id", fields.id)?;
    }
    if missing("title") {
        output.field_string("title", fields.title)?;
    }
    if missing("summary") {
        output.field_string("summary", fields.summary)?;
    }
    if missing("category_id") {
        output.field_display("category_id", fields.category_id)?;
    }
    if missing("tags") {
        output.raw("tags: []\n")?;
    }
    if missing("aliases") {
        output.raw("aliases: []\n")?;
    }
    if missing("status") {
        output.raw("status: draft\n")?;
    }
    if missing("created_at") {
        output.field_display("created_at", fields.created_at)?;
    }
    if missing("updated_at") {
        output.field_display("updated_at", fields.updated_at)?;
    }
    Ok(())
}

fn root_key_present(bound: &BoundGuide, key: &str) -> bool {
    bound
        .parsed()
        .yaml()
        .semantic()
        .entries()
        .iter()
        .any(|entry| entry.key() == key)
}

fn insert_missing_fields(
    partial: &BoundGuide,
    input: &AdoptionInput<'_>,
    summary: &str,
    created_floor: &Timestamp,
) -> Result<Box<[u8]>, AdoptionFailure> {
    let source = partial.source_bytes();
    let parsed = partial.parsed();
    let artifact = parsed.yaml();
    let envelope = parsed.envelope();
    let yaml_start = envelope.yaml_file_range().start().get() as usize;
    let close = envelope.closing_delimiter().start().get() as usize;

    // Only a block mapping whose root entries all start at column zero can be
    // extended by appending column-zero keys without changing its meaning.
    let root_entries = artifact
        .entries()
        .iter()
        .filter(|entry| entry.parent() == artifact.root_id())
        .collect::<Vec<_>>();
    if root_entries.is_empty() {
        let yaml = core::str::from_utf8(source.get(yaml_start..close).unwrap_or_default())
            .map_err(|_| AdoptionFailure::Mismatch)?;
        if !yaml
            .lines()
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
        {
            return Err(AdoptionFailure::Unsupported);
        }
    } else if artifact
        .node(artifact.root_id())
        .and_then(|node| node.collection_style())
        != Some(CollectionPresentationStyle::Block)
        || root_entries.iter().any(|entry| {
            let start = entry.file_range().start().get() as usize;
            entry.collection_style() != CollectionPresentationStyle::Block
                || start < yaml_start
                || source.get(start.wrapping_sub(1)) != Some(&b'\n')
        })
    {
        return Err(AdoptionFailure::Unsupported);
    }
    if close < yaml_start || (close > 0 && source.get(close - 1) != Some(&b'\n')) {
        return Err(AdoptionFailure::Unsupported);
    }

    let derived = partial.record();
    let missing = |key: &str| !root_key_present(partial, key);
    let created_at = if missing("created_at") {
        if missing("updated_at") {
            *created_floor
        } else {
            core::cmp::min(*created_floor, *derived.updated_at())
        }
    } else {
        *derived.created_at()
    };
    let updated_at = if missing("updated_at") {
        core::cmp::max(*input.now, created_at)
    } else {
        *derived.updated_at()
    };
    let mut fields = EmitBuffer::new();
    emit_adoption_fields(
        &mut fields,
        &AdoptionFields {
            missing: &missing,
            id: input.fresh_id,
            title: derived.title(),
            summary,
            category_id: derived.category_id(),
            created_at: &created_at,
            updated_at: &updated_at,
        },
    )?;
    splice_fields(source, close, &fields.finish(), envelope.yaml_line_ending())
}

/// Insert LF-emitted field lines at `at`, converting to CRLF when the
/// frontmatter uses CRLF. `at` must start a line.
fn splice_fields(
    source: &[u8],
    at: usize,
    fields: &[u8],
    line_ending: SourceLineEnding,
) -> Result<Box<[u8]>, AdoptionFailure> {
    if at > source.len() || (at > 0 && source.get(at - 1) != Some(&b'\n')) {
        return Err(AdoptionFailure::Unsupported);
    }
    let crlf = line_ending == SourceLineEnding::CrLf;
    let mut output = Vec::new();
    output
        .try_reserve_exact(source.len() + fields.len() * 2)
        .map_err(|_| AdoptionFailure::ResourceLimit)?;
    output.extend_from_slice(&source[..at]);
    for byte in fields {
        if *byte == b'\n' && crlf {
            output.push(b'\r');
        }
        output.push(*byte);
    }
    output.extend_from_slice(&source[at..]);
    Ok(output.into_boxed_slice())
}

impl From<EmitFailure> for AdoptionFailure {
    fn from(error: EmitFailure) -> Self {
        match error.kind() {
            EmitFailureKind::ResourceLimit => Self::ResourceLimit,
            EmitFailureKind::InvalidValue | EmitFailureKind::CandidateMismatch => Self::Mismatch,
        }
    }
}

fn canonical_body(body: &[u8]) -> Result<&str, EmitFailure> {
    let text = core::str::from_utf8(body).map_err(|_| invalid(EmitField::Body))?;
    if text.is_empty()
        || body.starts_with(&[0xef, 0xbb, 0xbf])
        || text.contains('\r')
        || !text.ends_with('\n')
        || text.ends_with("\n\n")
    {
        Err(invalid(EmitField::Body))
    } else {
        Ok(text)
    }
}

struct EmitBuffer {
    value: String,
}

impl EmitBuffer {
    fn new() -> Self {
        Self {
            value: String::new(),
        }
    }

    fn raw(&mut self, value: &str) -> Result<(), EmitFailure> {
        self.write_str(value)
            .map_err(|_| resource(EmitField::Document))
    }

    fn display(&mut self, value: impl fmt::Display) -> Result<(), EmitFailure> {
        write!(self, "{value}").map_err(|_| resource(EmitField::Document))
    }

    fn field_display(&mut self, key: &str, value: impl fmt::Display) -> Result<(), EmitFailure> {
        self.raw(key)?;
        self.raw(": ")?;
        self.quoted_display(value)?;
        self.raw("\n")
    }

    fn field_string(&mut self, key: &str, value: &str) -> Result<(), EmitFailure> {
        self.raw(key)?;
        self.raw(": ")?;
        self.quoted(value)?;
        self.raw("\n")
    }

    fn quoted_display(&mut self, value: impl fmt::Display) -> Result<(), EmitFailure> {
        self.raw("\"")?;
        self.display(value)?;
        self.raw("\"")
    }

    fn quoted(&mut self, value: &str) -> Result<(), EmitFailure> {
        self.raw("\"")?;
        for character in value.chars() {
            match character {
                '"' => self.raw("\\\"")?,
                '\\' => self.raw("\\\\")?,
                value if escaped_scalar(value) => {
                    write!(self, "\\u{:04X}", value as u32)
                        .map_err(|_| resource(EmitField::Document))?;
                }
                value => {
                    let mut encoded = [0_u8; 4];
                    self.raw(value.encode_utf8(&mut encoded))?;
                }
            }
        }
        self.raw("\"")
    }

    fn finish(self) -> Box<[u8]> {
        self.value.into_bytes().into_boxed_slice()
    }
}

impl fmt::Write for EmitBuffer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.value
            .try_reserve_exact(value.len())
            .map_err(|_| fmt::Error)?;
        self.value.push_str(value);
        Ok(())
    }
}

fn escaped_scalar(value: char) -> bool {
    matches!(
        value as u32,
        0x0000..=0x001f
            | 0x007f..=0x009f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x2028..=0x202e
            | 0x2066..=0x2069
            | 0xfeff
    )
}

fn invalid(field: EmitField) -> EmitFailure {
    EmitFailure {
        kind: EmitFailureKind::InvalidValue,
        field,
    }
}

fn resource(field: EmitField) -> EmitFailure {
    EmitFailure {
        kind: EmitFailureKind::ResourceLimit,
        field,
    }
}

fn mismatch(field: EmitField) -> EmitFailure {
    EmitFailure {
        kind: EmitFailureKind::CandidateMismatch,
        field,
    }
}

pub(super) fn map_parse_failure(error: ParseFailure) -> EmitFailure {
    match error.kind() {
        ParseFailureKind::SourceTooLarge
        | ParseFailureKind::FrontmatterTooLarge
        | ParseFailureKind::DepthLimit
        | ParseFailureKind::MappingEntryLimit
        | ParseFailureKind::SequenceEntryLimit
        | ParseFailureKind::NodeLimit
        | ParseFailureKind::RawScalarLimit
        | ParseFailureKind::DecodedScalarLimit
        | ParseFailureKind::CanonicalScalarLimit
        | ParseFailureKind::NumericWorkLimit
        | ParseFailureKind::NumericDigitLimit
        | ParseFailureKind::SemanticEncodingLimit
        | ParseFailureKind::SemanticComparisonLimit
        | ParseFailureKind::MeterArithmeticOverflow => resource(EmitField::Document),
        _ => mismatch(EmitField::Document),
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::*;

    fn workspace_id() -> WorkspaceId {
        WorkspaceId::from_str("0199a19f-4d15-7ca7-a645-e75311caa710").unwrap()
    }

    fn category_id() -> CategoryId {
        CategoryId::from_str("0199a1c2-d334-7d17-bd9c-f8b6ba3a5551").unwrap()
    }

    fn guide_id() -> GuideId {
        GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap()
    }

    fn timestamp() -> Timestamp {
        Timestamp::from_str("2026-09-05T10:00:00Z").unwrap()
    }

    #[test]
    fn workspace_golden_with_and_without_settings() {
        let id = workspace_id();
        let category = category_id();
        let at = timestamp();
        let plain = emit_new_workspace_manifest(&id, "Hichem's Rehow", &at, None)
            .unwrap_or_else(|_| panic!("emit"));
        assert_eq!(
            plain.source_bytes(),
            b"schema_version: 1\nworkspace_id: \"0199a19f-4d15-7ca7-a645-e75311caa710\"\nname: \"Hichem's Rehow\"\ncreated_at: \"2026-09-05T10:00:00Z\"\nupdated_at: \"2026-09-05T10:00:00Z\"\n"
        );
        let configured = emit_new_workspace_manifest(&id, "Rehow", &at, Some(&category))
            .unwrap_or_else(|_| panic!("emit"));
        assert!(configured.source_bytes().ends_with(
            b"settings:\n  default_category_id: \"0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\"\n"
        ));
    }

    #[test]
    fn category_golden_and_integer_extremes() {
        let id = category_id();
        let at = timestamp();
        let category = emit_new_category(
            &id,
            "Linux \"System\"",
            Some("Paths \\ packages"),
            i64::MIN,
            &at,
        )
        .unwrap_or_else(|_| panic!("emit"));
        assert_eq!(
            category.source_bytes(),
            b"schema_version: 1\nid: \"0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\"\nname: \"Linux \\\"System\\\"\"\nsummary: \"Paths \\\\ packages\"\norder: -9223372036854775808\nstatus: active\ncreated_at: \"2026-09-05T10:00:00Z\"\nupdated_at: \"2026-09-05T10:00:00Z\"\n"
        );
        assert!(emit_new_category(&id, "Linux", None, i64::MAX, &at).is_ok());
    }

    #[test]
    fn guide_golden_is_draft_and_rebinds() {
        let guide = emit_new_guide(
            &guide_id(),
            "Install",
            "Install the package.",
            &category_id(),
            &timestamp(),
            b"## Steps\n\nDo it.\n",
        )
        .unwrap_or_else(|_| panic!("emit"));
        assert_eq!(
            guide.source_bytes(),
            b"---\nschema_version: 1\nid: \"0199a1d7-7b0d-7d8e-a38d-f936c5735e10\"\ntitle: \"Install\"\nsummary: \"Install the package.\"\ncategory_id: \"0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\"\ntags: []\naliases: []\nstatus: draft\ncreated_at: \"2026-09-05T10:00:00Z\"\nupdated_at: \"2026-09-05T10:00:00Z\"\n---\n\n## Steps\n\nDo it.\n"
        );
    }

    #[test]
    fn exact_escape_policy_is_deterministic() {
        let id = workspace_id();
        let at = timestamp();
        let value = "a\u{0001}\u{007f}\u{061c}\u{200e}\u{202e}\u{2069}\u{feff}\\\"é";
        let first =
            emit_new_workspace_manifest(&id, value, &at, None).unwrap_or_else(|_| panic!("emit"));
        let second =
            emit_new_workspace_manifest(&id, value, &at, None).unwrap_or_else(|_| panic!("emit"));
        assert_eq!(first.source_bytes(), second.source_bytes());
        assert!(
            core::str::from_utf8(first.source_bytes())
                .unwrap()
                .contains("name: \"a\\u0001\\u007F\\u061C\\u200E\\u202E\\u2069\\uFEFF\\\\\\\"é\"")
        );
    }

    fn adopt(source: &[u8]) -> Result<BoundGuide, AdoptionFailure> {
        use super::super::derived::{DeriveContext, DerivedGuide, derive_guide};
        let workspace = workspace_id();
        let modified = Timestamp::from_str("2026-09-01T08:00:00Z").unwrap();
        let derived = derive_guide(
            source.to_vec().into_boxed_slice(),
            &DeriveContext {
                workspace_id: &workspace,
                relative_path: "categories/linux/My Notes.md",
                category_id: category_id(),
                modified_at: modified,
            },
        );
        let unadopted = match derived {
            DerivedGuide::Unadopted(unadopted) => unadopted,
            DerivedGuide::Managed(_) => panic!("managed"),
            DerivedGuide::Rejected(diagnostic) => {
                panic!("rejected {diagnostic:?} {:?}", core::str::from_utf8(source))
            }
        };
        emit_adopted_guide(
            &unadopted,
            source,
            &AdoptionInput {
                fresh_id: &guide_id(),
                modified_at: &modified,
                now: &timestamp(),
            },
        )
    }

    #[test]
    fn adoption_prepends_canonical_frontmatter_and_keeps_body_bytes() {
        let body = b"# Deploy\r\n\r\nRun it.\r\n\r\n\r\n";
        let adopted = adopt(body).unwrap_or_else(|_| panic!("adopt"));
        assert_eq!(
            adopted.source_bytes(),
            [
                b"---\nschema_version: 1\nid: \"0199a1d7-7b0d-7d8e-a38d-f936c5735e10\"\ntitle: \"Deploy\"\nsummary: \"Run it.\"\ncategory_id: \"0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\"\ntags: []\naliases: []\nstatus: draft\ncreated_at: \"2026-09-01T08:00:00Z\"\nupdated_at: \"2026-09-05T10:00:00Z\"\n---\n\n".as_slice(),
                body.as_slice()
            ]
            .concat()
        );
        assert_eq!(adopted.record().body(), body);

        let mut with_bom = b"\xef\xbb\xbf".to_vec();
        with_bom.extend_from_slice(b"Only text, no heading");
        let adopted = adopt(&with_bom).unwrap_or_else(|_| panic!("adopt"));
        assert!(adopted.source_bytes().starts_with(b"\xef\xbb\xbf---\n"));
        assert!(
            adopted
                .source_bytes()
                .ends_with(b"---\n\nOnly text, no heading")
        );
        assert_eq!(adopted.record().title(), "My Notes");
        assert_eq!(adopted.record().summary(), "Only text, no heading");

        let empty = adopt(b"").unwrap_or_else(|_| panic!("adopt"));
        assert_eq!(empty.record().summary(), "My Notes");
        assert_eq!(empty.record().body(), b"");

        let unterminated = adopt(b"---\ntitle: x\n").unwrap_or_else(|_| panic!("adopt"));
        assert_eq!(unterminated.record().body(), b"---\ntitle: x\n");
    }

    #[test]
    fn adoption_inserts_only_missing_keys_into_partial_frontmatter() {
        let source =
            b"---\r\ntitle: Kept # note\r\nlayout: post\r\ntags:\r\n- linux\r\n---\r\nBody\r\n";
        let adopted = adopt(source).unwrap_or_else(|_| panic!("adopt"));
        let text = core::str::from_utf8(adopted.source_bytes()).unwrap();
        assert!(text.starts_with("---\r\ntitle: Kept # note\r\nlayout: post\r\ntags:\r\n- linux\r\nschema_version: 1\r\nid: \"0199a1d7-7b0d-7d8e-a38d-f936c5735e10\"\r\nsummary: \"Body\"\r\ncategory_id: "));
        assert!(text.ends_with("updated_at: \"2026-09-05T10:00:00Z\"\r\n---\r\nBody\r\n"));
        assert!(!text.contains("\ntitle: \""));
        assert_eq!(adopted.record().title(), "Kept");
        assert_eq!(adopted.record().tags(), ["linux".to_owned()]);
        assert_eq!(adopted.unknown_partition().root().len(), 1);

        let kept_id = adopt(
            b"---\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e99\ncreated_at: 2026-09-03T00:00:00Z\n---\nBody\n",
        )
        .unwrap_or_else(|_| panic!("adopt"));
        assert_eq!(
            kept_id.record().id().to_string(),
            "0199a1d7-7b0d-7d8e-a38d-f936c5735e99"
        );
        assert_eq!(
            kept_id.record().created_at(),
            &Timestamp::from_str("2026-09-03T00:00:00Z").unwrap()
        );

        let empty =
            adopt(b"---\n# just a comment\n---\nBody\n").unwrap_or_else(|_| panic!("adopt"));
        assert!(
            empty
                .source_bytes()
                .starts_with(b"---\n# just a comment\nschema_version: 1\n")
        );
        assert!(empty.source_bytes().ends_with(b"\n---\nBody\n"));
        assert_eq!(empty.record().body(), b"Body\n");
    }

    #[test]
    fn adoption_refuses_flow_and_indented_root_mappings() {
        assert_eq!(
            adopt(b"---\n{title: Flow}\n---\nBody\n").err(),
            Some(AdoptionFailure::Unsupported)
        );
        assert_eq!(
            adopt(b"---\n  title: Indented\n  layout: x\n---\nBody\n").err(),
            Some(AdoptionFailure::Unsupported)
        );
    }

    #[test]
    fn invalid_text_and_body_are_rejected_without_content_errors() {
        let id = workspace_id();
        let category = category_id();
        let guide = guide_id();
        let at = timestamp();
        for name in ["", " spaced", "trailing ", "\u{fdd0}"] {
            let error = emit_new_workspace_manifest(&id, name, &at, None)
                .err()
                .unwrap();
            assert!(matches!(error.kind(), EmitFailureKind::InvalidValue));
            assert!(matches!(error.field(), EmitField::Name));
        }
        for body in [b"".as_slice(), b"no newline", b"bad\r\n", b"two\n\n"] {
            let error = emit_new_guide(&guide, "Title", "Summary", &category, &at, body)
                .err()
                .unwrap();
            assert!(matches!(error.field(), EmitField::Body));
        }
        assert!(
            emit_new_guide(&guide, "Title", "Summary", &category, &at, &[0xff, b'\n']).is_err()
        );

        let mut oversized = vec![b'a'; 2 * 1024 * 1024];
        *oversized.last_mut().unwrap() = b'\n';
        let error = emit_new_guide(&guide, "Title", "Summary", &category, &at, &oversized)
            .err()
            .unwrap();
        assert!(matches!(error.kind(), EmitFailureKind::ResourceLimit));
    }
}

#![allow(
    dead_code,
    reason = "Phase 1D-C emitters are connected to storage/application in Phase 1E"
)]

use core::fmt::{self, Write as _};

use crate::{
    parsing::{ParseFailure, ParseFailureKind, parse_guide_source, parse_yaml_document},
    protocol::{CategoryId, GuideId, Timestamp, WorkspaceId},
};

use super::{
    BoundCategory, BoundWorkspaceManifest, CategoryStatus, GuideStatus, ManifestBindingOutcome,
    bind_category, bind_workspace_manifest,
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

fn map_parse_failure(error: ParseFailure) -> EmitFailure {
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

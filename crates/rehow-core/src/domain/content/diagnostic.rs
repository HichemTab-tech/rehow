//! Closed, content-free diagnostics for Guide files that could not be bound.
//!
//! A diagnostic names only a fixed failure class, an optional known field,
//! and an optional one-based source position. It never carries file
//! content, key names from the file, or scalar values.

use crate::parsing::{ParseFailure, ParseFailureKind, SourcePosition};

use super::guide::{GuideBindFailure, GuideBindFailureKind, GuideField};

/// Why a Guide file could not be read as a managed Guide.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum GuideDiagnosticCode {
    /// The file is larger than the Guide size limit.
    FileTooLarge,
    /// The frontmatter is larger than the frontmatter size limit.
    FrontmatterTooLarge,
    /// The file is not valid UTF-8.
    InvalidUtf8,
    /// The frontmatter contains a bare carriage return or mixed line endings.
    UnsupportedLineEndings,
    /// A byte-order mark appears somewhere other than the start of the file.
    MisplacedByteOrderMark,
    /// The file does not start with a `---` frontmatter line.
    MissingFrontmatter,
    /// The frontmatter has no closing `---` line.
    MissingClosingDelimiter,
    /// The frontmatter is not well-formed YAML.
    YamlSyntax,
    /// The frontmatter uses a YAML feature Rehow refuses (anchors, aliases,
    /// tags, directives, merge keys, or several documents).
    UnsupportedYamlFeature,
    /// The frontmatter contains a disallowed Unicode scalar.
    InvalidUnicode,
    /// The frontmatter root is not a mapping.
    FrontmatterNotMapping,
    /// A top-level frontmatter key is not a string.
    NonStringKey,
    /// A frontmatter mapping repeats a key.
    DuplicateKey,
    /// A required Rehow field is absent.
    MissingRequiredField,
    /// A Rehow field has the wrong YAML type.
    InvalidFieldType,
    /// A Rehow field has an invalid value.
    InvalidFieldValue,
    /// A Rehow list field repeats an equivalent value.
    DuplicateFieldValue,
    /// Parsing or binding exceeded a resource limit.
    ResourceLimit,
    /// An internal parser or binder invariant failed.
    Internal,
}

/// A known Rehow frontmatter field named by a diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum GuideDiagnosticField {
    SchemaVersion,
    Id,
    Title,
    Summary,
    CategoryId,
    Tags,
    Aliases,
    Status,
    CreatedAt,
    UpdatedAt,
    LastVerifiedAt,
    VerifiedContentHash,
    ArchivedFromStatus,
    ArchivedAt,
    AppliesTo,
    Prerequisites,
    Warnings,
    FilesAffected,
    RelatedGuideIds,
    References,
    Attachments,
}

impl GuideDiagnosticField {
    /// The frontmatter key for this field.
    pub fn key(self) -> &'static str {
        match self {
            Self::SchemaVersion => "schema_version",
            Self::Id => "id",
            Self::Title => "title",
            Self::Summary => "summary",
            Self::CategoryId => "category_id",
            Self::Tags => "tags",
            Self::Aliases => "aliases",
            Self::Status => "status",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
            Self::LastVerifiedAt => "last_verified_at",
            Self::VerifiedContentHash => "verified_content_hash",
            Self::ArchivedFromStatus => "archived_from_status",
            Self::ArchivedAt => "archived_at",
            Self::AppliesTo => "applies_to",
            Self::Prerequisites => "prerequisites",
            Self::Warnings => "warnings",
            Self::FilesAffected => "files_affected",
            Self::RelatedGuideIds => "related_guide_ids",
            Self::References => "references",
            Self::Attachments => "attachments",
        }
    }
}

/// A one-based line and column (in Unicode scalars) within the Guide file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GuideDiagnosticPosition {
    line: u32,
    column: u32,
}

impl GuideDiagnosticPosition {
    pub fn line(self) -> u32 {
        self.line
    }

    pub fn column(self) -> u32 {
        self.column
    }
}

/// A content-free description of why a Guide file was not bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GuideDiagnostic {
    code: GuideDiagnosticCode,
    field: Option<GuideDiagnosticField>,
    position: Option<GuideDiagnosticPosition>,
}

impl GuideDiagnostic {
    pub fn code(self) -> GuideDiagnosticCode {
        self.code
    }

    pub fn field(self) -> Option<GuideDiagnosticField> {
        self.field
    }

    pub fn position(self) -> Option<GuideDiagnosticPosition> {
        self.position
    }

    pub(crate) fn new(
        code: GuideDiagnosticCode,
        field: Option<GuideDiagnosticField>,
        position: Option<GuideDiagnosticPosition>,
    ) -> Self {
        Self {
            code,
            field,
            position,
        }
    }

    /// Map a failure from `parse_guide_source`. Parser positions after the
    /// envelope scan are relative to the frontmatter text, which always
    /// starts on file line 2 (the opening `---` occupies line 1).
    pub(crate) fn from_guide_parse_failure(failure: &ParseFailure) -> Self {
        let position = failure.position().and_then(|position| {
            Some(GuideDiagnosticPosition {
                line: position.line().checked_add(1)?,
                column: position.column(),
            })
        });
        Self {
            code: parse_code(failure.kind()),
            field: None,
            position,
        }
    }

    pub(crate) fn from_guide_bind_failure(failure: &GuideBindFailure) -> Self {
        Self {
            code: match failure.kind() {
                GuideBindFailureKind::MissingRequiredField => {
                    GuideDiagnosticCode::MissingRequiredField
                }
                GuideBindFailureKind::InvalidFieldType => GuideDiagnosticCode::InvalidFieldType,
                GuideBindFailureKind::InvalidFieldValue => GuideDiagnosticCode::InvalidFieldValue,
                GuideBindFailureKind::DuplicateField => GuideDiagnosticCode::DuplicateFieldValue,
                GuideBindFailureKind::ResourceLimit => GuideDiagnosticCode::ResourceLimit,
                GuideBindFailureKind::BindingInvariant => GuideDiagnosticCode::Internal,
            },
            field: Some(diagnostic_field(failure.field())),
            position: failure.position().map(file_position),
        }
    }
}

fn file_position(position: SourcePosition) -> GuideDiagnosticPosition {
    GuideDiagnosticPosition {
        line: position.line(),
        column: position.column(),
    }
}

pub(crate) fn diagnostic_field(field: GuideField) -> GuideDiagnosticField {
    match field {
        GuideField::SchemaVersion => GuideDiagnosticField::SchemaVersion,
        GuideField::Id => GuideDiagnosticField::Id,
        GuideField::Title => GuideDiagnosticField::Title,
        GuideField::Summary => GuideDiagnosticField::Summary,
        GuideField::CategoryId => GuideDiagnosticField::CategoryId,
        GuideField::Tags => GuideDiagnosticField::Tags,
        GuideField::Aliases => GuideDiagnosticField::Aliases,
        GuideField::Status => GuideDiagnosticField::Status,
        GuideField::CreatedAt => GuideDiagnosticField::CreatedAt,
        GuideField::UpdatedAt => GuideDiagnosticField::UpdatedAt,
        GuideField::LastVerifiedAt => GuideDiagnosticField::LastVerifiedAt,
        GuideField::VerifiedContentHash => GuideDiagnosticField::VerifiedContentHash,
        GuideField::ArchivedFromStatus => GuideDiagnosticField::ArchivedFromStatus,
        GuideField::ArchivedAt => GuideDiagnosticField::ArchivedAt,
        GuideField::AppliesTo => GuideDiagnosticField::AppliesTo,
        GuideField::Prerequisites => GuideDiagnosticField::Prerequisites,
        GuideField::Warnings => GuideDiagnosticField::Warnings,
        GuideField::FilesAffected => GuideDiagnosticField::FilesAffected,
        GuideField::RelatedGuideIds => GuideDiagnosticField::RelatedGuideIds,
        GuideField::References => GuideDiagnosticField::References,
        GuideField::Attachments => GuideDiagnosticField::Attachments,
    }
}

fn parse_code(kind: ParseFailureKind) -> GuideDiagnosticCode {
    use GuideDiagnosticCode as C;
    use ParseFailureKind as P;
    match kind {
        P::SourceTooLarge => C::FileTooLarge,
        P::FrontmatterTooLarge => C::FrontmatterTooLarge,
        P::InvalidUtf8 => C::InvalidUtf8,
        P::RawBomDenied => C::MisplacedByteOrderMark,
        P::BareCarriageReturn | P::MixedYamlLineEndings => C::UnsupportedLineEndings,
        P::MissingGuideOpeningDelimiter => C::MissingFrontmatter,
        P::MissingGuideClosingDelimiter | P::MissingGuideSeparator => C::MissingClosingDelimiter,
        P::PresentationSyntax
        | P::PresentationCoverage
        | P::PresentationShape
        | P::SemanticSyntax
        | P::ParserTagDisagreement => C::YamlSyntax,
        P::DirectiveDenied
        | P::DocumentMarkerDenied
        | P::AnchorDenied
        | P::AliasDenied
        | P::ExplicitTagDenied
        | P::ExplicitTagKindMismatch
        | P::ExplicitTagValueMismatch
        | P::MergeKeyDenied
        | P::MultipleDocuments => C::UnsupportedYamlFeature,
        P::InvalidUnicodeScalar => C::InvalidUnicode,
        P::RootMustBeMapping => C::FrontmatterNotMapping,
        P::TopLevelKeyMustBeString => C::NonStringKey,
        P::DuplicateMappingKey => C::DuplicateKey,
        P::DepthLimit
        | P::MappingEntryLimit
        | P::SequenceEntryLimit
        | P::NodeLimit
        | P::RawScalarLimit
        | P::DecodedScalarLimit
        | P::CanonicalScalarLimit
        | P::NumericWorkLimit
        | P::NumericDigitLimit
        | P::SemanticEncodingLimit
        | P::SemanticComparisonLimit
        | P::MeterArithmeticOverflow => C::ResourceLimit,
        P::SemanticStreamOrder | P::SemanticBuilderInvariant => C::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::content::guide::bind_guide, parsing::parse_guide_source};

    fn diagnose(source: &str) -> GuideDiagnostic {
        match parse_guide_source(source.as_bytes().to_vec().into_boxed_slice()) {
            Err(error) => GuideDiagnostic::from_guide_parse_failure(&error),
            Ok(parsed) => match bind_guide(parsed) {
                Err(error) => GuideDiagnostic::from_guide_bind_failure(&error),
                Ok(_) => panic!("valid Guide"),
            },
        }
    }

    const VALID: &str = "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: T\nsummary: S\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags: []\naliases: []\nstatus: draft\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n";

    #[test]
    fn envelope_and_yaml_failures_map_to_closed_codes() {
        assert_eq!(
            diagnose("# Hello\n").code(),
            GuideDiagnosticCode::MissingFrontmatter
        );
        assert_eq!(
            diagnose("---\ntitle: x\n").code(),
            GuideDiagnosticCode::MissingClosingDelimiter
        );
        assert_eq!(
            diagnose("---\n- a\n---\n").code(),
            GuideDiagnosticCode::FrontmatterNotMapping
        );
        let duplicate = diagnose("---\ntitle: a\ntitle: b\n---\n");
        assert_eq!(duplicate.code(), GuideDiagnosticCode::DuplicateKey);
        assert_eq!(duplicate.field(), None);
        let anchor = diagnose("---\na: &x 1\nb: *x\n---\n");
        assert_eq!(anchor.code(), GuideDiagnosticCode::UnsupportedYamlFeature);
        let syntax = diagnose("---\ntitle: a\nbad: [\n---\n");
        assert_eq!(syntax.code(), GuideDiagnosticCode::YamlSyntax);
    }

    #[test]
    fn parse_positions_are_file_relative() {
        let anchor = diagnose("---\ntitle: a\nb: &x 1\n---\n");
        let position = anchor.position().unwrap();
        assert_eq!((position.line(), position.column()), (3, 4));
    }

    #[test]
    fn bind_failures_name_known_fields_and_file_positions() {
        let missing = diagnose(&format!("---\n{}---\n", VALID.replace("title: T\n", "")));
        assert_eq!(missing.code(), GuideDiagnosticCode::MissingRequiredField);
        assert_eq!(missing.field(), Some(GuideDiagnosticField::Title));
        assert_eq!(missing.position(), None);

        let invalid = diagnose(&format!(
            "---\n{}---\n",
            VALID.replace("status: draft", "status: done")
        ));
        assert_eq!(invalid.code(), GuideDiagnosticCode::InvalidFieldValue);
        assert_eq!(invalid.field(), Some(GuideDiagnosticField::Status));
        let position = invalid.position().unwrap();
        assert_eq!((position.line(), position.column()), (9, 9));
        assert_eq!(GuideDiagnosticField::Status.key(), "status");

        let typed = diagnose(&format!(
            "---\n{}---\n",
            VALID.replace("tags: []", "tags: nope")
        ));
        assert_eq!(typed.code(), GuideDiagnosticCode::InvalidFieldType);
        assert_eq!(typed.field(), Some(GuideDiagnosticField::Tags));
    }
}

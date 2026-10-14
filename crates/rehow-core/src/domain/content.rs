#![allow(
    dead_code,
    reason = "Phase 1D-A binding is connected to storage/application in Phase 1E"
)]

pub(crate) mod derived;
pub(crate) mod diagnostic;
pub(crate) mod emission;
pub(crate) mod guide;
pub(crate) mod mutation;
pub(crate) mod verification;

pub use derived::{GuideOrigin, UnadoptedReasons};
pub use diagnostic::{
    GuideDiagnostic, GuideDiagnosticCode, GuideDiagnosticField, GuideDiagnosticPosition,
};
pub use guide::{
    Applicability, ArchiveMetadata, Attachment, Guide, GuideMetadata, GuideReference, GuideStatus,
    RestorableGuideStatus, VerificationMetadata,
};

use core::str::FromStr;

use crate::{
    parsing::{NodePresentation, ParsedYamlDocument, ReconciledEntryKind, SourcePosition},
    protocol::{CategoryId, Timestamp, WorkspaceId},
};

use super::{CanonicalYamlInteger, UnknownMetadataDocument, UnknownYamlValue};

const SCHEMA_VERSION: &str = "schema_version";
const WORKSPACE_ID: &str = "workspace_id";
const NAME: &str = "name";
const CREATED_AT: &str = "created_at";
const UPDATED_AT: &str = "updated_at";
const SETTINGS: &str = "settings";
const DEFAULT_CATEGORY_ID: &str = "default_category_id";
const ID: &str = "id";
const SUMMARY: &str = "summary";
const ORDER: &str = "order";
const STATUS: &str = "status";

pub struct WorkspaceSettings {
    default_category_id: Option<CategoryId>,
}

pub struct WorkspaceManifest {
    schema_version: u32,
    workspace_id: WorkspaceId,
    name: String,
    created_at: Timestamp,
    updated_at: Timestamp,
    settings: Option<WorkspaceSettings>,
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum CategoryStatus {
    Active,
    Archived,
}

impl core::fmt::Debug for CategoryStatus {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Active => "CategoryStatus::Active",
            Self::Archived => "CategoryStatus::Archived",
        })
    }
}

pub struct Category {
    schema_version: u32,
    id: CategoryId,
    name: String,
    summary: Option<String>,
    order: i64,
    status: CategoryStatus,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl WorkspaceSettings {
    pub fn default_category_id(&self) -> Option<&CategoryId> {
        self.default_category_id.as_ref()
    }
}

impl WorkspaceManifest {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn workspace_id(&self) -> &WorkspaceId {
        &self.workspace_id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }
    pub fn updated_at(&self) -> &Timestamp {
        &self.updated_at
    }
    pub fn settings(&self) -> Option<&WorkspaceSettings> {
        self.settings.as_ref()
    }
}

impl Category {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn id(&self) -> &CategoryId {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }
    pub fn order(&self) -> i64 {
        self.order
    }
    pub fn status(&self) -> CategoryStatus {
        self.status
    }
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }
    pub fn updated_at(&self) -> &Timestamp {
        &self.updated_at
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum DocumentKind {
    WorkspaceManifest,
    Category,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum BindFailureKind {
    MissingRequiredField,
    InvalidFieldType,
    InvalidFieldValue,
    BindingInvariant,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum KnownFieldPath {
    SchemaVersion,
    WorkspaceId,
    Name,
    CreatedAt,
    UpdatedAt,
    Settings,
    DefaultCategoryId,
    Id,
    Summary,
    Order,
    Status,
}

pub(crate) struct BindFailure {
    document_kind: DocumentKind,
    kind: BindFailureKind,
    field: KnownFieldPath,
    position: Option<SourcePosition>,
    parsed_category_id: Option<CategoryId>,
}

impl BindFailure {
    fn new(document_kind: DocumentKind, kind: BindFailureKind, field: KnownFieldPath) -> Self {
        Self {
            document_kind,
            kind,
            field,
            position: None,
            parsed_category_id: None,
        }
    }

    pub(crate) fn document_kind(&self) -> DocumentKind {
        self.document_kind
    }
    pub(crate) fn kind(&self) -> BindFailureKind {
        self.kind
    }
    pub(crate) fn field(&self) -> KnownFieldPath {
        self.field
    }
    pub(crate) fn position(&self) -> Option<SourcePosition> {
        self.position
    }
    pub(crate) fn parsed_category_id(&self) -> Option<&CategoryId> {
        self.parsed_category_id.as_ref()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ManifestSchemaClass {
    Current,
    NewerUnsupported(u32),
}

pub(crate) enum ManifestBindingOutcome {
    Current(BoundWorkspaceManifest),
    NewerUnsupported {
        version: u32,
        parsed: ParsedYamlDocument,
    },
}

impl ManifestBindingOutcome {
    pub(crate) fn newer_version(&self) -> Option<u32> {
        match self {
            Self::Current(_) => None,
            Self::NewerUnsupported { version, .. } => Some(*version),
        }
    }

    pub(crate) fn source_bytes(&self) -> &[u8] {
        match self {
            Self::Current(bound) => bound.source_bytes(),
            Self::NewerUnsupported { parsed, .. } => parsed.source_bytes(),
        }
    }
}

pub(crate) struct BoundWorkspaceManifest {
    parsed: ParsedYamlDocument,
    record: WorkspaceManifest,
    unknown_root_entries: Box<[u32]>,
    unknown_settings_entries: Box<[u32]>,
}

pub(crate) struct BoundCategory {
    parsed: ParsedYamlDocument,
    record: Category,
    unknown_root_entries: Box<[u32]>,
}

impl BoundWorkspaceManifest {
    pub(crate) fn record(&self) -> &WorkspaceManifest {
        &self.record
    }
    pub(crate) fn source_bytes(&self) -> &[u8] {
        self.parsed.source_bytes()
    }
    pub(crate) fn unknown_root_entries(&self) -> &[u32] {
        &self.unknown_root_entries
    }
    pub(crate) fn unknown_settings_entries(&self) -> &[u32] {
        &self.unknown_settings_entries
    }
    pub(crate) fn into_record(self) -> WorkspaceManifest {
        self.record
    }
}

impl BoundCategory {
    pub(crate) fn record(&self) -> &Category {
        &self.record
    }
    pub(crate) fn source_bytes(&self) -> &[u8] {
        self.parsed.source_bytes()
    }
    pub(crate) fn unknown_root_entries(&self) -> &[u32] {
        &self.unknown_root_entries
    }
    pub(crate) fn into_record(self) -> Category {
        self.record
    }
}

pub(crate) fn inspect_manifest_schema(
    parsed: &ParsedYamlDocument,
) -> Result<ManifestSchemaClass, BindFailure> {
    let entry = root_entry(parsed.yaml().semantic(), SCHEMA_VERSION).ok_or_else(|| {
        BindFailure::new(
            DocumentKind::WorkspaceManifest,
            BindFailureKind::MissingRequiredField,
            KnownFieldPath::SchemaVersion,
        )
    })?;
    let version = parse_u32(entry.value()).ok_or_else(|| {
        BindFailure::new(
            DocumentKind::WorkspaceManifest,
            value_failure_kind(entry.value()),
            KnownFieldPath::SchemaVersion,
        )
    })?;
    if version == 0 {
        return Err(BindFailure::new(
            DocumentKind::WorkspaceManifest,
            BindFailureKind::InvalidFieldValue,
            KnownFieldPath::SchemaVersion,
        ));
    }
    Ok(if version == 1 {
        ManifestSchemaClass::Current
    } else {
        ManifestSchemaClass::NewerUnsupported(version)
    })
}

pub(crate) fn bind_workspace_manifest(
    parsed: ParsedYamlDocument,
) -> Result<ManifestBindingOutcome, BindFailure> {
    let positions = known_positions(&parsed);
    match inspect_manifest_schema(&parsed)? {
        ManifestSchemaClass::Current => bind_current_workspace_manifest(parsed)
            .map(ManifestBindingOutcome::Current)
            .map_err(|error| with_position(error, &positions)),
        ManifestSchemaClass::NewerUnsupported(version) => {
            Ok(ManifestBindingOutcome::NewerUnsupported { version, parsed })
        }
    }
}

fn bind_current_workspace_manifest(
    parsed: ParsedYamlDocument,
) -> Result<BoundWorkspaceManifest, BindFailure> {
    let document = parsed.yaml().semantic();
    let workspace_id = required_string(
        document,
        WORKSPACE_ID,
        DocumentKind::WorkspaceManifest,
        KnownFieldPath::WorkspaceId,
    )
    .and_then(|value| {
        WorkspaceId::from_str(value)
            .map_err(|_| invalid(DocumentKind::WorkspaceManifest, KnownFieldPath::WorkspaceId))
    })?;
    let name = required_text(
        document,
        NAME,
        DocumentKind::WorkspaceManifest,
        KnownFieldPath::Name,
        200,
    )?;
    let created_at = required_string(
        document,
        CREATED_AT,
        DocumentKind::WorkspaceManifest,
        KnownFieldPath::CreatedAt,
    )
    .and_then(|value| {
        Timestamp::from_str(value)
            .map_err(|_| invalid(DocumentKind::WorkspaceManifest, KnownFieldPath::CreatedAt))
    })?;
    let updated_at = required_string(
        document,
        UPDATED_AT,
        DocumentKind::WorkspaceManifest,
        KnownFieldPath::UpdatedAt,
    )
    .and_then(|value| {
        Timestamp::from_str(value)
            .map_err(|_| invalid(DocumentKind::WorkspaceManifest, KnownFieldPath::UpdatedAt))
    })?;
    if updated_at < created_at {
        return Err(invalid(
            DocumentKind::WorkspaceManifest,
            KnownFieldPath::UpdatedAt,
        ));
    }

    let (settings, unknown_settings_entries) = match root_entry(document, SETTINGS) {
        None => (None, Vec::new()),
        Some(entry) => {
            let mapping = entry.value().as_mapping().ok_or_else(|| {
                type_error(DocumentKind::WorkspaceManifest, KnownFieldPath::Settings)
            })?;
            let mut default_category_id = None;
            let mut unknown = Vec::new();
            unknown.try_reserve_exact(mapping.len()).map_err(|_| {
                invariant(DocumentKind::WorkspaceManifest, KnownFieldPath::Settings)
            })?;
            for (index, member) in mapping.iter().enumerate() {
                let key = string_value(member.key()).ok_or_else(|| {
                    type_error(DocumentKind::WorkspaceManifest, KnownFieldPath::Settings)
                })?;
                if key == DEFAULT_CATEGORY_ID {
                    let raw = string_value(member.value()).ok_or_else(|| {
                        type_error(
                            DocumentKind::WorkspaceManifest,
                            KnownFieldPath::DefaultCategoryId,
                        )
                    })?;
                    default_category_id = Some(CategoryId::from_str(raw).map_err(|_| {
                        invalid(
                            DocumentKind::WorkspaceManifest,
                            KnownFieldPath::DefaultCategoryId,
                        )
                    })?);
                } else {
                    unknown.push(u32::try_from(index).map_err(|_| {
                        invariant(DocumentKind::WorkspaceManifest, KnownFieldPath::Settings)
                    })?);
                }
            }
            (
                Some(WorkspaceSettings {
                    default_category_id,
                }),
                unknown,
            )
        }
    };

    let unknown_root_entries = unknown_root_indices(
        document,
        &[
            SCHEMA_VERSION,
            WORKSPACE_ID,
            NAME,
            CREATED_AT,
            UPDATED_AT,
            SETTINGS,
        ],
        DocumentKind::WorkspaceManifest,
    )?;
    Ok(BoundWorkspaceManifest {
        parsed,
        record: WorkspaceManifest {
            schema_version: 1,
            workspace_id,
            name,
            created_at,
            updated_at,
            settings,
        },
        unknown_root_entries: unknown_root_entries.into_boxed_slice(),
        unknown_settings_entries: unknown_settings_entries.into_boxed_slice(),
    })
}

pub(crate) fn bind_category(parsed: ParsedYamlDocument) -> Result<BoundCategory, BindFailure> {
    let positions = known_positions(&parsed);
    bind_category_inner(parsed).map_err(|error| with_position(error, &positions))
}

fn bind_category_inner(parsed: ParsedYamlDocument) -> Result<BoundCategory, BindFailure> {
    let document = parsed.yaml().semantic();
    let version = required_value(
        document,
        SCHEMA_VERSION,
        DocumentKind::Category,
        KnownFieldPath::SchemaVersion,
    )
    .and_then(|value| {
        parse_u32(value).ok_or_else(|| {
            BindFailure::new(
                DocumentKind::Category,
                value_failure_kind(value),
                KnownFieldPath::SchemaVersion,
            )
        })
    })?;
    if version != 1 {
        return Err(invalid(
            DocumentKind::Category,
            KnownFieldPath::SchemaVersion,
        ));
    }
    let id = required_string(document, ID, DocumentKind::Category, KnownFieldPath::Id).and_then(
        |value| {
            CategoryId::from_str(value)
                .map_err(|_| invalid(DocumentKind::Category, KnownFieldPath::Id))
        },
    )?;
    let name = required_text(
        document,
        NAME,
        DocumentKind::Category,
        KnownFieldPath::Name,
        200,
    )?;
    let summary = optional_text(
        document,
        SUMMARY,
        DocumentKind::Category,
        KnownFieldPath::Summary,
        500,
    )?;
    let order_value = required_value(
        document,
        ORDER,
        DocumentKind::Category,
        KnownFieldPath::Order,
    )?;
    let order = parse_i64(order_value).ok_or_else(|| {
        BindFailure::new(
            DocumentKind::Category,
            value_failure_kind(order_value),
            KnownFieldPath::Order,
        )
    })?;
    let status = match required_string(
        document,
        STATUS,
        DocumentKind::Category,
        KnownFieldPath::Status,
    )? {
        "active" => CategoryStatus::Active,
        "archived" => CategoryStatus::Archived,
        _ => return Err(invalid(DocumentKind::Category, KnownFieldPath::Status)),
    };
    let created_at = required_string(
        document,
        CREATED_AT,
        DocumentKind::Category,
        KnownFieldPath::CreatedAt,
    )
    .and_then(|value| {
        Timestamp::from_str(value)
            .map_err(|_| invalid(DocumentKind::Category, KnownFieldPath::CreatedAt))
    })?;
    let updated_at = required_string(
        document,
        UPDATED_AT,
        DocumentKind::Category,
        KnownFieldPath::UpdatedAt,
    )
    .and_then(|value| {
        Timestamp::from_str(value)
            .map_err(|_| invalid(DocumentKind::Category, KnownFieldPath::UpdatedAt))
    })?;
    if updated_at < created_at {
        return Err(invalid(DocumentKind::Category, KnownFieldPath::UpdatedAt));
    }
    let unknown_root_entries = unknown_root_indices(
        document,
        &[
            SCHEMA_VERSION,
            ID,
            NAME,
            SUMMARY,
            ORDER,
            STATUS,
            CREATED_AT,
            UPDATED_AT,
        ],
        DocumentKind::Category,
    )?;
    Ok(BoundCategory {
        parsed,
        record: Category {
            schema_version: version,
            id,
            name,
            summary,
            order,
            status,
            created_at,
            updated_at,
        },
        unknown_root_entries: unknown_root_entries.into_boxed_slice(),
    })
}

fn root_entry<'a>(
    document: &'a UnknownMetadataDocument,
    key: &str,
) -> Option<&'a super::UnknownMetadataEntry> {
    document.entries().iter().find(|entry| entry.key() == key)
}

fn required_value<'a>(
    document: &'a UnknownMetadataDocument,
    key: &str,
    kind: DocumentKind,
    field: KnownFieldPath,
) -> Result<&'a UnknownYamlValue, BindFailure> {
    root_entry(document, key)
        .map(super::UnknownMetadataEntry::value)
        .ok_or_else(|| BindFailure::new(kind, BindFailureKind::MissingRequiredField, field))
}

fn required_string<'a>(
    document: &'a UnknownMetadataDocument,
    key: &str,
    kind: DocumentKind,
    field: KnownFieldPath,
) -> Result<&'a str, BindFailure> {
    let value = required_value(document, key, kind, field)?;
    string_value(value).ok_or_else(|| type_error(kind, field))
}

fn required_text(
    document: &UnknownMetadataDocument,
    key: &str,
    kind: DocumentKind,
    field: KnownFieldPath,
    max_scalars: usize,
) -> Result<String, BindFailure> {
    let value = required_string(document, key, kind, field)?;
    if !valid_text(value, max_scalars) {
        return Err(invalid(kind, field));
    }
    copy_text(value, kind, field)
}

fn optional_text(
    document: &UnknownMetadataDocument,
    key: &str,
    kind: DocumentKind,
    field: KnownFieldPath,
    max_scalars: usize,
) -> Result<Option<String>, BindFailure> {
    let Some(entry) = root_entry(document, key) else {
        return Ok(None);
    };
    let value = string_value(entry.value()).ok_or_else(|| type_error(kind, field))?;
    if !valid_text(value, max_scalars) {
        return Err(invalid(kind, field));
    }
    copy_text(value, kind, field).map(Some)
}

fn string_value(value: &UnknownYamlValue) -> Option<&str> {
    value.as_scalar()?.as_str()
}

fn integer_value(value: &UnknownYamlValue) -> Option<&CanonicalYamlInteger> {
    value.as_scalar()?.as_integer()
}

fn parse_u32(value: &UnknownYamlValue) -> Option<u32> {
    let integer = integer_value(value)?;
    if integer.is_negative() {
        return None;
    }
    integer.digits().parse().ok()
}

fn parse_i64(value: &UnknownYamlValue) -> Option<i64> {
    let integer = integer_value(value)?;
    let magnitude = integer.digits().parse::<u64>().ok()?;
    if integer.is_negative() {
        if magnitude == (i64::MAX as u64) + 1 {
            Some(i64::MIN)
        } else {
            i64::try_from(magnitude).ok().map(|value| -value)
        }
    } else {
        i64::try_from(magnitude).ok()
    }
}

fn value_failure_kind(value: &UnknownYamlValue) -> BindFailureKind {
    if integer_value(value).is_some() {
        BindFailureKind::InvalidFieldValue
    } else {
        BindFailureKind::InvalidFieldType
    }
}

fn valid_text(value: &str, max_scalars: usize) -> bool {
    let count = value.chars().count();
    count > 0
        && count <= max_scalars
        && value.chars().next().is_some_and(|c| !is_white_space(c))
        && value.chars().last().is_some_and(|c| !is_white_space(c))
        && !value.chars().any(is_noncharacter)
}

fn copy_text(
    value: &str,
    kind: DocumentKind,
    field: KnownFieldPath,
) -> Result<String, BindFailure> {
    let mut result = String::new();
    result
        .try_reserve_exact(value.len())
        .map_err(|_| invariant(kind, field))?;
    result.push_str(value);
    Ok(result)
}

fn is_white_space(value: char) -> bool {
    matches!(
        value as u32,
        0x0009..=0x000d
            | 0x0020
            | 0x0085
            | 0x00a0
            | 0x1680
            | 0x2000..=0x200a
            | 0x2028
            | 0x2029
            | 0x202f
            | 0x205f
            | 0x3000
    )
}

fn is_noncharacter(value: char) -> bool {
    let scalar = value as u32;
    (0xfdd0..=0xfdef).contains(&scalar) || scalar & 0xfffe == 0xfffe
}

fn unknown_root_indices(
    document: &UnknownMetadataDocument,
    known: &[&str],
    kind: DocumentKind,
) -> Result<Vec<u32>, BindFailure> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(document.entries().len())
        .map_err(|_| invariant(kind, KnownFieldPath::SchemaVersion))?;
    for (index, entry) in document.entries().iter().enumerate() {
        if !known.contains(&entry.key()) {
            result.push(
                u32::try_from(index).map_err(|_| invariant(kind, KnownFieldPath::SchemaVersion))?,
            );
        }
    }
    Ok(result)
}

fn type_error(kind: DocumentKind, field: KnownFieldPath) -> BindFailure {
    BindFailure::new(kind, BindFailureKind::InvalidFieldType, field)
}

fn invalid(kind: DocumentKind, field: KnownFieldPath) -> BindFailure {
    BindFailure::new(kind, BindFailureKind::InvalidFieldValue, field)
}

fn invariant(kind: DocumentKind, field: KnownFieldPath) -> BindFailure {
    BindFailure::new(kind, BindFailureKind::BindingInvariant, field)
}

fn with_position(
    mut error: BindFailure,
    positions: &[(KnownFieldPath, SourcePosition)],
) -> BindFailure {
    if error.position.is_none() {
        error.position = positions
            .iter()
            .find_map(|(field, position)| (*field == error.field).then_some(*position));
    }
    error
}

fn known_positions(parsed: &ParsedYamlDocument) -> Vec<(KnownFieldPath, SourcePosition)> {
    const FIELDS: &[(&str, KnownFieldPath)] = &[
        (SCHEMA_VERSION, KnownFieldPath::SchemaVersion),
        (WORKSPACE_ID, KnownFieldPath::WorkspaceId),
        (NAME, KnownFieldPath::Name),
        (CREATED_AT, KnownFieldPath::CreatedAt),
        (UPDATED_AT, KnownFieldPath::UpdatedAt),
        (SETTINGS, KnownFieldPath::Settings),
        (ID, KnownFieldPath::Id),
        (SUMMARY, KnownFieldPath::Summary),
        (ORDER, KnownFieldPath::Order),
        (STATUS, KnownFieldPath::Status),
    ];
    let artifact = parsed.yaml();
    let mut positions = Vec::new();
    if positions.try_reserve_exact(FIELDS.len()).is_err() {
        return positions;
    }
    for entry in artifact.entries() {
        let ReconciledEntryKind::Mapping { key, value } = entry.kind() else {
            continue;
        };
        let Some(key_text) = artifact.node(key).and_then(|node| node.decoded_scalar()) else {
            continue;
        };
        let Some((_, field)) = FIELDS.iter().find(|(name, _)| *name == key_text) else {
            continue;
        };
        let Some(value_node) = artifact.node(value) else {
            continue;
        };
        let NodePresentation::Concrete {
            whole_file_range, ..
        } = value_node.presentation()
        else {
            continue;
        };
        if let Some(position) = parsed.position_at_file_offset(whole_file_range.start()) {
            positions.push((*field, position));
        }
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::parse_yaml_document;

    fn parse(source: &str) -> ParsedYamlDocument {
        parse_yaml_document(source.as_bytes().into()).expect("fixture parses")
    }

    #[test]
    fn binds_manifest_and_preserves_unknown_partition() {
        let result = bind_workspace_manifest(parse(
            "schema_version: 1\nworkspace_id: 0199a19f-4d15-7ca7-a645-e75311caa710\nname: Rehow\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:01Z\nextension: {complex: [1, 2]}\nsettings:\n  default_category_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\n  future: true\n",
        ));
        let Ok(ManifestBindingOutcome::Current(bound)) = result else {
            panic!("valid manifest must bind");
        };
        assert_eq!(bound.record().schema_version(), 1);
        assert_eq!(bound.record().name(), "Rehow");
        assert!(
            bound
                .record()
                .settings()
                .unwrap()
                .default_category_id()
                .is_some()
        );
        assert_eq!(bound.unknown_root_entries(), &[5]);
        assert_eq!(bound.unknown_settings_entries(), &[1]);
        assert!(bound.source_bytes().starts_with(b"schema_version"));
    }

    #[test]
    fn classifies_newer_manifest_without_binding_it() {
        let parsed = parse("schema_version: 2\n");
        assert!(matches!(
            inspect_manifest_schema(&parsed),
            Ok(ManifestSchemaClass::NewerUnsupported(2))
        ));
        let Ok(outcome) = bind_workspace_manifest(parsed) else {
            panic!("classification must succeed");
        };
        assert_eq!(outcome.newer_version(), Some(2));
        assert!(outcome.source_bytes().starts_with(b"schema_version"));
    }

    #[test]
    fn binds_category_with_full_i64_order() {
        let result = bind_category(parse(
            "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux System\nsummary: Host setup\norder: -9223372036854775808\nstatus: archived\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\nfuture: null\n",
        ));
        let Ok(bound) = result else {
            panic!("valid Category must bind");
        };
        assert_eq!(bound.record().order(), i64::MIN);
        assert_eq!(bound.record().status(), CategoryStatus::Archived);
        assert_eq!(bound.record().summary(), Some("Host setup"));
        assert_eq!(bound.unknown_root_entries(), &[8]);
    }

    #[test]
    fn rejects_null_trimmed_and_time_reversal() {
        let null_name = bind_category(parse(
            "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: null\norder: 0\nstatus: active\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n",
        ));
        assert!(
            matches!(null_name, Err(error) if error.kind() == BindFailureKind::InvalidFieldType && error.field() == KnownFieldPath::Name)
        );

        let spaced = bind_category(parse(
            "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: ' Linux'\norder: 0\nstatus: active\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n",
        ));
        assert!(matches!(spaced, Err(error) if error.kind() == BindFailureKind::InvalidFieldValue));

        let reversed = bind_category(parse(
            "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux\norder: 0\nstatus: active\ncreated_at: 2026-09-05T10:00:01Z\nupdated_at: 2026-09-05T10:00:00Z\n",
        ));
        assert!(matches!(reversed, Err(error) if error.field() == KnownFieldPath::UpdatedAt));
    }

    #[test]
    fn enforces_text_scalar_boundaries() {
        let exact = "x".repeat(200);
        let over = "x".repeat(201);
        let make = |name: &str| {
            format!(
                "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: {name}\norder: 0\nstatus: active\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n"
            )
        };
        assert!(bind_category(parse(&make(&exact))).is_ok());
        assert!(
            matches!(bind_category(parse(&make(&over))), Err(error) if error.field() == KnownFieldPath::Name)
        );
    }

    #[test]
    fn failure_is_content_free_and_typed() {
        let error = bind_category(parse("schema_version: 1\n")).err().unwrap();
        assert!(error.document_kind() == DocumentKind::Category);
        assert!(error.kind() == BindFailureKind::MissingRequiredField);
        assert!(error.field() == KnownFieldPath::Id);
        assert!(error.position().is_none());
        assert!(error.parsed_category_id().is_none());
    }

    #[test]
    fn settings_absence_empty_unknown_and_null_are_distinct() {
        let prefix = "schema_version: 1\nworkspace_id: 0199a19f-4d15-7ca7-a645-e75311caa710\nname: Rehow\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n";
        let Ok(ManifestBindingOutcome::Current(absent)) = bind_workspace_manifest(parse(prefix))
        else {
            panic!("absent settings must bind");
        };
        assert!(absent.record().settings().is_none());

        let Ok(ManifestBindingOutcome::Current(empty)) =
            bind_workspace_manifest(parse(&format!("{prefix}settings: {{}}\n")))
        else {
            panic!("empty settings must bind");
        };
        assert!(empty.record().settings().is_some());
        assert!(empty.unknown_settings_entries().is_empty());

        let Ok(ManifestBindingOutcome::Current(unknown)) =
            bind_workspace_manifest(parse(&format!("{prefix}settings:\n  future: true\n")))
        else {
            panic!("unknown-only settings must bind");
        };
        assert_eq!(unknown.unknown_settings_entries(), &[0]);

        for suffix in [
            "settings: null\n",
            "settings:\n  default_category_id: null\n",
        ] {
            let result = bind_workspace_manifest(parse(&format!("{prefix}{suffix}")));
            assert!(
                matches!(result, Err(error) if error.kind() == BindFailureKind::InvalidFieldType)
            );
        }
    }

    #[test]
    fn category_required_field_and_scalar_matrix() {
        let valid = [
            "schema_version: 1",
            "id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551",
            "name: Linux",
            "order: 0",
            "status: active",
            "created_at: 2026-09-05T10:00:00Z",
            "updated_at: 2026-09-05T10:00:00Z",
        ];
        for missing in 0..valid.len() {
            let source = valid
                .iter()
                .enumerate()
                .filter_map(|(index, line)| (index != missing).then_some(*line))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            assert!(
                matches!(bind_category(parse(&source)), Err(error) if error.kind() == BindFailureKind::MissingRequiredField)
            );
        }

        for replacement in [
            "id: 0199a1c2-d334-6d17-bd9c-f8b6ba3a5551",
            "id: 0199A1C2-D334-7D17-BD9C-F8B6BA3A5551",
            "created_at: 2026-09-05T10:00:00+00:00",
            "order: 9223372036854775808",
            "order: -9223372036854775809",
            "status: ACTIVE",
        ] {
            let index = if replacement.starts_with("id:") {
                1
            } else if replacement.starts_with("created_at:") {
                5
            } else if replacement.starts_with("order:") {
                3
            } else {
                4
            };
            let mut rows = valid;
            rows[index] = replacement;
            assert!(bind_category(parse(&(rows.join("\n") + "\n"))).is_err());
        }
    }

    #[test]
    fn summary_boundaries_and_read_only_artifact_bind() {
        let make = |summary: &str| {
            format!(
                "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux\nsummary: {summary}\norder: 0\nstatus: active\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n"
            )
        };
        assert!(bind_category(parse(&make(&"s".repeat(500)))).is_ok());
        assert!(bind_category(parse(&make(&"s".repeat(501)))).is_err());

        let mut parsed = parse(
            "schema_version: 1\n# boundary comment\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux\norder: 0\nstatus: active\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n",
        );
        parsed.force_read_only_for_content_test();
        assert!(!parsed.yaml().round_trip_status().is_proven());
        assert!(bind_category(parsed).is_ok());
    }

    #[test]
    fn malformed_values_carry_parser_proven_positions() {
        let error = bind_category(parse(
            "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux\norder: 0\nstatus: unknown\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n",
        ))
        .err()
        .unwrap();
        let position = error.position().expect("known value has a position");
        assert_eq!(position.line(), 5);
        assert_eq!(position.column(), 9);
    }
}

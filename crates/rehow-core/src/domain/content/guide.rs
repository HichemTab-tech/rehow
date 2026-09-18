#![allow(
    dead_code,
    reason = "Phase 1D-B Guide binding is connected to storage/application in Phase 1E"
)]

use core::{
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use focaccia::CaseFold;
use unicode_normalization::UnicodeNormalization;
use url::{Host, Url};

use crate::{
    domain::{
        CanonicalYamlInteger, UnknownMetadataDocument, UnknownYamlMappingEntry, UnknownYamlValue,
    },
    parsing::{NodePresentation, ParsedGuideSource, ReconciledEntryKind, SourcePosition},
    protocol::{AttachmentId, CategoryId, GuideId, Timestamp, VerificationHash},
};

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum GuideStatus {
    Draft,
    Tested,
    Stale,
    Archived,
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RestorableGuideStatus {
    Draft,
    Tested,
    Stale,
}

macro_rules! status_debug {
    ($type:ty, {$($variant:ident),+ $(,)?}) => {
        impl core::fmt::Debug for $type {
            fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str(match self { $(Self::$variant => concat!(stringify!($type), "::", stringify!($variant))),+ })
            }
        }
    };
}
status_debug!(GuideStatus, {Draft, Tested, Stale, Archived});
status_debug!(RestorableGuideStatus, {Draft, Tested, Stale});

pub struct Applicability {
    platform: String,
    versions: Option<String>,
    notes: Option<String>,
}
pub struct GuideReference {
    title: String,
    url: String,
}
pub struct Attachment {
    id: AttachmentId,
    path: String,
    title: String,
    media_type: Option<String>,
    sha256: String,
    size_bytes: Option<u64>,
}
pub struct VerificationMetadata {
    last_verified_at: Timestamp,
    verified_content_hash: VerificationHash,
}
pub struct ArchiveMetadata {
    archived_from_status: RestorableGuideStatus,
    archived_at: Timestamp,
}
pub struct Guide {
    schema_version: u32,
    id: GuideId,
    title: String,
    summary: String,
    category_id: CategoryId,
    tags: Vec<String>,
    aliases: Vec<String>,
    declared_status: GuideStatus,
    created_at: Timestamp,
    updated_at: Timestamp,
    verification: Option<VerificationMetadata>,
    archive: Option<ArchiveMetadata>,
    applies_to: Option<Vec<Applicability>>,
    prerequisites: Option<Vec<String>>,
    warnings: Option<Vec<String>>,
    files_affected: Option<Vec<String>>,
    related_guide_ids: Option<Vec<GuideId>>,
    references: Option<Vec<GuideReference>>,
    attachments: Option<Vec<Attachment>>,
    body: Box<[u8]>,
}

impl Applicability {
    pub fn platform(&self) -> &str {
        &self.platform
    }
    pub fn versions(&self) -> Option<&str> {
        self.versions.as_deref()
    }
    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }
}
impl GuideReference {
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn url(&self) -> &str {
        &self.url
    }
}
impl Attachment {
    pub fn id(&self) -> &AttachmentId {
        &self.id
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn size_bytes(&self) -> Option<u64> {
        self.size_bytes
    }
}
impl VerificationMetadata {
    pub fn last_verified_at(&self) -> &Timestamp {
        &self.last_verified_at
    }
    pub fn verified_content_hash(&self) -> &VerificationHash {
        &self.verified_content_hash
    }
}
impl ArchiveMetadata {
    pub fn archived_from_status(&self) -> RestorableGuideStatus {
        self.archived_from_status
    }
    pub fn archived_at(&self) -> &Timestamp {
        &self.archived_at
    }
}
impl Guide {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn id(&self) -> &GuideId {
        &self.id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn summary(&self) -> &str {
        &self.summary
    }
    pub fn category_id(&self) -> &CategoryId {
        &self.category_id
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
    pub fn declared_status(&self) -> GuideStatus {
        self.declared_status
    }
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }
    pub fn updated_at(&self) -> &Timestamp {
        &self.updated_at
    }
    pub fn verification(&self) -> Option<&VerificationMetadata> {
        self.verification.as_ref()
    }
    pub fn archive(&self) -> Option<&ArchiveMetadata> {
        self.archive.as_ref()
    }
    pub fn applies_to(&self) -> Option<&[Applicability]> {
        self.applies_to.as_deref()
    }
    pub fn prerequisites(&self) -> Option<&[String]> {
        self.prerequisites.as_deref()
    }
    pub fn warnings(&self) -> Option<&[String]> {
        self.warnings.as_deref()
    }
    pub fn files_affected(&self) -> Option<&[String]> {
        self.files_affected.as_deref()
    }
    pub fn related_guide_ids(&self) -> Option<&[GuideId]> {
        self.related_guide_ids.as_deref()
    }
    pub fn references(&self) -> Option<&[GuideReference]> {
        self.references.as_deref()
    }
    pub fn attachments(&self) -> Option<&[Attachment]> {
        self.attachments.as_deref()
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum GuideBindFailureKind {
    MissingRequiredField,
    InvalidFieldType,
    InvalidFieldValue,
    DuplicateField,
    ResourceLimit,
    BindingInvariant,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum GuideField {
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
pub(crate) struct GuideBindFailure {
    kind: GuideBindFailureKind,
    field: GuideField,
    position: Option<SourcePosition>,
    parsed_guide_id: Option<GuideId>,
}
impl GuideBindFailure {
    fn new(kind: GuideBindFailureKind, field: GuideField) -> Self {
        Self {
            kind,
            field,
            position: None,
            parsed_guide_id: None,
        }
    }
    pub(crate) fn kind(&self) -> GuideBindFailureKind {
        self.kind
    }
    pub(crate) fn field(&self) -> GuideField {
        self.field
    }
    pub(crate) fn position(&self) -> Option<SourcePosition> {
        self.position
    }
    pub(crate) fn parsed_guide_id(&self) -> Option<&GuideId> {
        self.parsed_guide_id.as_ref()
    }
}

pub(crate) struct GuideUnknownPartition {
    root: Box<[u32]>,
    applicability: Box<[Box<[u32]>]>,
    references: Box<[Box<[u32]>]>,
    attachments: Box<[Box<[u32]>]>,
}
pub(crate) struct BoundGuide {
    parsed: ParsedGuideSource,
    record: Guide,
    unknown: GuideUnknownPartition,
}
type UnknownChildPartitions = Vec<Box<[u32]>>;
type BoundNestedCollection<T> = Result<(Option<Vec<T>>, UnknownChildPartitions), GuideBindFailure>;
impl BoundGuide {
    pub(crate) fn record(&self) -> &Guide {
        &self.record
    }
    pub(crate) fn source_bytes(&self) -> &[u8] {
        self.parsed.source_bytes()
    }
    pub(crate) fn unknown_partition(&self) -> &GuideUnknownPartition {
        &self.unknown
    }
    pub(crate) fn parsed(&self) -> &ParsedGuideSource {
        &self.parsed
    }
    pub(crate) fn into_record(self) -> Guide {
        self.record
    }

    #[cfg(test)]
    pub(crate) fn parsed_mut_for_content_test(&mut self) -> &mut ParsedGuideSource {
        &mut self.parsed
    }
}
impl GuideUnknownPartition {
    pub(crate) fn root(&self) -> &[u32] {
        &self.root
    }
    pub(crate) fn applicability(&self) -> &[Box<[u32]>] {
        &self.applicability
    }
    pub(crate) fn references(&self) -> &[Box<[u32]>] {
        &self.references
    }
    pub(crate) fn attachments(&self) -> &[Box<[u32]>] {
        &self.attachments
    }
}

const ROOT_FIELDS: &[(&str, GuideField)] = &[
    ("schema_version", GuideField::SchemaVersion),
    ("id", GuideField::Id),
    ("title", GuideField::Title),
    ("summary", GuideField::Summary),
    ("category_id", GuideField::CategoryId),
    ("tags", GuideField::Tags),
    ("aliases", GuideField::Aliases),
    ("status", GuideField::Status),
    ("created_at", GuideField::CreatedAt),
    ("updated_at", GuideField::UpdatedAt),
    ("last_verified_at", GuideField::LastVerifiedAt),
    ("verified_content_hash", GuideField::VerifiedContentHash),
    ("archived_from_status", GuideField::ArchivedFromStatus),
    ("archived_at", GuideField::ArchivedAt),
    ("applies_to", GuideField::AppliesTo),
    ("prerequisites", GuideField::Prerequisites),
    ("warnings", GuideField::Warnings),
    ("files_affected", GuideField::FilesAffected),
    ("related_guide_ids", GuideField::RelatedGuideIds),
    ("references", GuideField::References),
    ("attachments", GuideField::Attachments),
];

pub(crate) fn bind_guide(parsed: ParsedGuideSource) -> Result<BoundGuide, GuideBindFailure> {
    let positions = known_positions(&parsed);
    let parsed_id = root_entry(parsed.yaml().semantic(), "id")
        .and_then(|entry| string_value(entry.value()))
        .and_then(|value| GuideId::from_str(value).ok());
    bind_guide_inner(parsed).map_err(|mut error| {
        error.parsed_guide_id = parsed_id;
        error.position = positions
            .iter()
            .find_map(|(field, position)| (*field == error.field).then_some(*position));
        error
    })
}

fn bind_guide_inner(parsed: ParsedGuideSource) -> Result<BoundGuide, GuideBindFailure> {
    let document = parsed.yaml().semantic();
    let schema_value = required(document, "schema_version", GuideField::SchemaVersion)?;
    let schema_integer =
        integer_value(schema_value).ok_or_else(|| type_error(GuideField::SchemaVersion))?;
    if parse_u32_integer(schema_integer) != Some(1) {
        return Err(invalid(GuideField::SchemaVersion));
    }
    let id = parse_id::<GuideId>(document, "id", GuideField::Id)?;
    let title = text(document, "title", GuideField::Title, 200)?;
    let summary = text(document, "summary", GuideField::Summary, 500)?;
    let category_id = parse_id::<CategoryId>(document, "category_id", GuideField::CategoryId)?;
    let tags = required_strings(document, "tags", GuideField::Tags, 64, 64, None)?;
    unique_folded(&tags, GuideField::Tags)?;
    let aliases = required_strings(document, "aliases", GuideField::Aliases, 64, 200, None)?;
    unique_folded(&aliases, GuideField::Aliases)?;
    let declared_status = match required_string(document, "status", GuideField::Status)? {
        "draft" => GuideStatus::Draft,
        "tested" => GuideStatus::Tested,
        "stale" => GuideStatus::Stale,
        "archived" => GuideStatus::Archived,
        _ => return Err(invalid(GuideField::Status)),
    };
    let created_at = timestamp(document, "created_at", GuideField::CreatedAt)?;
    let updated_at = timestamp(document, "updated_at", GuideField::UpdatedAt)?;
    if updated_at < created_at {
        return Err(invalid(GuideField::UpdatedAt));
    }

    let last_verified_at =
        optional_timestamp(document, "last_verified_at", GuideField::LastVerifiedAt)?;
    let verified_content_hash = optional_string(
        document,
        "verified_content_hash",
        GuideField::VerifiedContentHash,
    )?
    .map(|value| {
        VerificationHash::from_str(value).map_err(|_| invalid(GuideField::VerifiedContentHash))
    })
    .transpose()?;
    let verification = match (last_verified_at, verified_content_hash) {
        (None, None) => None,
        (Some(at), Some(hash)) if at >= created_at && at <= updated_at => {
            Some(VerificationMetadata {
                last_verified_at: at,
                verified_content_hash: hash,
            })
        }
        _ => return Err(invalid(GuideField::LastVerifiedAt)),
    };

    let archived_from_status = optional_string(
        document,
        "archived_from_status",
        GuideField::ArchivedFromStatus,
    )?
    .map(|value| match value {
        "draft" => Ok(RestorableGuideStatus::Draft),
        "tested" => Ok(RestorableGuideStatus::Tested),
        "stale" => Ok(RestorableGuideStatus::Stale),
        _ => Err(invalid(GuideField::ArchivedFromStatus)),
    })
    .transpose()?;
    let archived_at = optional_timestamp(document, "archived_at", GuideField::ArchivedAt)?;
    let archive = match (archived_from_status, archived_at) {
        (None, None) => None,
        (Some(from), Some(at)) if at >= created_at && at <= updated_at => Some(ArchiveMetadata {
            archived_from_status: from,
            archived_at: at,
        }),
        (None, Some(_)) => return Err(invalid(GuideField::ArchivedFromStatus)),
        (Some(_), None) | (Some(_), Some(_)) => return Err(invalid(GuideField::ArchivedAt)),
    };
    validate_status_companions(declared_status, verification.as_ref(), archive.as_ref())?;

    let (applies_to, applicability_unknown) = bind_applicability(document)?;
    let prerequisites = optional_strings(
        document,
        "prerequisites",
        GuideField::Prerequisites,
        128,
        1_000,
        None,
    )?;
    let warnings = optional_strings(document, "warnings", GuideField::Warnings, 128, 1_000, None)?;
    let files_affected = optional_strings(
        document,
        "files_affected",
        GuideField::FilesAffected,
        256,
        1_024,
        Some(4_096),
    )?;
    let related_guide_ids = optional_ids(
        document,
        "related_guide_ids",
        GuideField::RelatedGuideIds,
        256,
        &id,
    )?;
    let (references, reference_unknown) = bind_references(document)?;
    let (attachments, attachment_unknown) = bind_attachments(document, &id)?;
    let root_unknown = unknown_indices(
        document.entries(),
        |key| ROOT_FIELDS.iter().any(|(known, _)| *known == key),
        GuideField::SchemaVersion,
    )?;

    let body = copy_bytes(parsed.body_bytes(), GuideField::SchemaVersion)?;
    Ok(BoundGuide {
        parsed,
        record: Guide {
            schema_version: 1,
            id,
            title,
            summary,
            category_id,
            tags,
            aliases,
            declared_status,
            created_at,
            updated_at,
            verification,
            archive,
            applies_to,
            prerequisites,
            warnings,
            files_affected,
            related_guide_ids,
            references,
            attachments,
            body,
        },
        unknown: GuideUnknownPartition {
            root: root_unknown.into_boxed_slice(),
            applicability: applicability_unknown.into_boxed_slice(),
            references: reference_unknown.into_boxed_slice(),
            attachments: attachment_unknown.into_boxed_slice(),
        },
    })
}

fn validate_status_companions(
    status: GuideStatus,
    verification: Option<&VerificationMetadata>,
    archive: Option<&ArchiveMetadata>,
) -> Result<(), GuideBindFailure> {
    match status {
        GuideStatus::Draft if verification.is_none() && archive.is_none() => Ok(()),
        GuideStatus::Tested if verification.is_some() && archive.is_none() => Ok(()),
        GuideStatus::Stale if archive.is_none() => Ok(()),
        GuideStatus::Archived => {
            let Some(archive) = archive else {
                return Err(invalid(GuideField::ArchivedFromStatus));
            };
            match archive.archived_from_status {
                RestorableGuideStatus::Draft if verification.is_none() => Ok(()),
                RestorableGuideStatus::Tested if verification.is_some() => Ok(()),
                RestorableGuideStatus::Stale => Ok(()),
                _ => Err(invalid(GuideField::LastVerifiedAt)),
            }
        }
        GuideStatus::Draft | GuideStatus::Tested if archive.is_some() => {
            Err(invalid(GuideField::ArchivedFromStatus))
        }
        GuideStatus::Draft | GuideStatus::Tested => Err(invalid(GuideField::LastVerifiedAt)),
        GuideStatus::Stale => Err(invalid(GuideField::ArchivedFromStatus)),
    }
}

fn bind_applicability(document: &UnknownMetadataDocument) -> BoundNestedCollection<Applicability> {
    let Some(value) = optional_value(document, "applies_to") else {
        return Ok((None, Vec::new()));
    };
    let sequence = value
        .as_sequence()
        .ok_or_else(|| type_error(GuideField::AppliesTo))?;
    if sequence.len() > 32 {
        return Err(invalid(GuideField::AppliesTo));
    }
    let mut values = reserve_vec(sequence.len(), GuideField::AppliesTo)?;
    let mut unknowns = reserve_vec(sequence.len(), GuideField::AppliesTo)?;
    for item in sequence {
        let mapping = item
            .as_mapping()
            .ok_or_else(|| type_error(GuideField::AppliesTo))?;
        ensure_string_keys(mapping, GuideField::AppliesTo)?;
        values.push(Applicability {
            platform: mapping_text(mapping, "platform", GuideField::AppliesTo, 100, true)?.unwrap(),
            versions: mapping_text(mapping, "versions", GuideField::AppliesTo, 200, false)?,
            notes: mapping_text(mapping, "notes", GuideField::AppliesTo, 500, false)?,
        });
        unknowns.push(
            unknown_mapping_indices(
                mapping,
                &["platform", "versions", "notes"],
                GuideField::AppliesTo,
            )?
            .into_boxed_slice(),
        );
    }
    Ok((Some(values), unknowns))
}

fn bind_references(document: &UnknownMetadataDocument) -> BoundNestedCollection<GuideReference> {
    let Some(value) = optional_value(document, "references") else {
        return Ok((None, Vec::new()));
    };
    let sequence = value
        .as_sequence()
        .ok_or_else(|| type_error(GuideField::References))?;
    if sequence.len() > 128 {
        return Err(invalid(GuideField::References));
    }
    let mut values = reserve_vec(sequence.len(), GuideField::References)?;
    let mut unknowns = reserve_vec(sequence.len(), GuideField::References)?;
    for item in sequence {
        let mapping = item
            .as_mapping()
            .ok_or_else(|| type_error(GuideField::References))?;
        ensure_string_keys(mapping, GuideField::References)?;
        let title = mapping_text(mapping, "title", GuideField::References, 200, true)?.unwrap();
        let raw_url = mapping_text(mapping, "url", GuideField::References, 2_048, true)?.unwrap();
        if raw_url.len() > 2_048 || !valid_reference_url(&raw_url) {
            return Err(invalid(GuideField::References));
        }
        values.push(GuideReference {
            title,
            url: raw_url,
        });
        unknowns.push(
            unknown_mapping_indices(mapping, &["title", "url"], GuideField::References)?
                .into_boxed_slice(),
        );
    }
    Ok((Some(values), unknowns))
}

fn bind_attachments(
    document: &UnknownMetadataDocument,
    guide_id: &GuideId,
) -> BoundNestedCollection<Attachment> {
    let Some(value) = optional_value(document, "attachments") else {
        return Ok((None, Vec::new()));
    };
    let sequence = value
        .as_sequence()
        .ok_or_else(|| type_error(GuideField::Attachments))?;
    if sequence.len() > 256 {
        return Err(invalid(GuideField::Attachments));
    }
    let mut values = reserve_vec(sequence.len(), GuideField::Attachments)?;
    let mut unknowns = reserve_vec(sequence.len(), GuideField::Attachments)?;
    for item in sequence {
        let mapping = item
            .as_mapping()
            .ok_or_else(|| type_error(GuideField::Attachments))?;
        ensure_string_keys(mapping, GuideField::Attachments)?;
        let id =
            mapping_required_string(mapping, "id", GuideField::Attachments).and_then(|value| {
                AttachmentId::from_str(value).map_err(|_| invalid(GuideField::Attachments))
            })?;
        let path = mapping_text(mapping, "path", GuideField::Attachments, 1_024, true)?.unwrap();
        if path.len() > 1_024 || !valid_attachment_path(&path, guide_id) {
            return Err(invalid(GuideField::Attachments));
        }
        let title = mapping_text(mapping, "title", GuideField::Attachments, 200, true)?.unwrap();
        if title.len() > 800 {
            return Err(invalid(GuideField::Attachments));
        }
        let media_type = mapping_text(mapping, "media_type", GuideField::Attachments, 127, false)?;
        if media_type
            .as_deref()
            .is_some_and(|value| !valid_media_type(value))
        {
            return Err(invalid(GuideField::Attachments));
        }
        let sha256 = mapping_required_string(mapping, "sha256", GuideField::Attachments)?;
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(invalid(GuideField::Attachments));
        }
        let size_bytes = mapping_optional_value(mapping, "size_bytes")
            .map(|value| {
                let integer =
                    integer_value(value).ok_or_else(|| type_error(GuideField::Attachments))?;
                parse_u64_integer(integer).ok_or_else(|| invalid(GuideField::Attachments))
            })
            .transpose()?;
        if values
            .iter()
            .any(|current: &Attachment| current.id == id || current.path == path)
        {
            return Err(invalid(GuideField::Attachments));
        }
        values.push(Attachment {
            id,
            path,
            title,
            media_type,
            sha256: copy_string(sha256, GuideField::Attachments)?,
            size_bytes,
        });
        unknowns.push(
            unknown_mapping_indices(
                mapping,
                &["id", "path", "title", "media_type", "sha256", "size_bytes"],
                GuideField::Attachments,
            )?
            .into_boxed_slice(),
        );
    }
    Ok((Some(values), unknowns))
}

fn required<'a>(
    document: &'a UnknownMetadataDocument,
    key: &str,
    field: GuideField,
) -> Result<&'a UnknownYamlValue, GuideBindFailure> {
    root_entry(document, key)
        .map(crate::domain::UnknownMetadataEntry::value)
        .ok_or_else(|| missing(field))
}
fn optional_value<'a>(
    document: &'a UnknownMetadataDocument,
    key: &str,
) -> Option<&'a UnknownYamlValue> {
    root_entry(document, key).map(crate::domain::UnknownMetadataEntry::value)
}
fn root_entry<'a>(
    document: &'a UnknownMetadataDocument,
    key: &str,
) -> Option<&'a crate::domain::UnknownMetadataEntry> {
    document.entries().iter().find(|entry| entry.key() == key)
}
fn string_value(value: &UnknownYamlValue) -> Option<&str> {
    value.as_scalar()?.as_str()
}
fn integer_value(value: &UnknownYamlValue) -> Option<&CanonicalYamlInteger> {
    value.as_scalar()?.as_integer()
}
fn required_string<'a>(
    document: &'a UnknownMetadataDocument,
    key: &str,
    field: GuideField,
) -> Result<&'a str, GuideBindFailure> {
    string_value(required(document, key, field)?).ok_or_else(|| type_error(field))
}
fn optional_string<'a>(
    document: &'a UnknownMetadataDocument,
    key: &str,
    field: GuideField,
) -> Result<Option<&'a str>, GuideBindFailure> {
    optional_value(document, key)
        .map(|value| string_value(value).ok_or_else(|| type_error(field)))
        .transpose()
}
fn text(
    document: &UnknownMetadataDocument,
    key: &str,
    field: GuideField,
    max: usize,
) -> Result<String, GuideBindFailure> {
    let value = required_string(document, key, field)?;
    validate_text(value, max, None, field)?;
    copy_string(value, field)
}
fn timestamp(
    document: &UnknownMetadataDocument,
    key: &str,
    field: GuideField,
) -> Result<Timestamp, GuideBindFailure> {
    Timestamp::from_str(required_string(document, key, field)?).map_err(|_| invalid(field))
}
fn optional_timestamp(
    document: &UnknownMetadataDocument,
    key: &str,
    field: GuideField,
) -> Result<Option<Timestamp>, GuideBindFailure> {
    optional_string(document, key, field)?
        .map(|value| Timestamp::from_str(value).map_err(|_| invalid(field)))
        .transpose()
}
fn parse_id<T: FromStr>(
    document: &UnknownMetadataDocument,
    key: &str,
    field: GuideField,
) -> Result<T, GuideBindFailure> {
    required_string(document, key, field)?
        .parse()
        .map_err(|_| invalid(field))
}
fn parse_u32_integer(value: &CanonicalYamlInteger) -> Option<u32> {
    (!value.is_negative())
        .then(|| value.digits().parse().ok())
        .flatten()
}
fn parse_u64_integer(value: &CanonicalYamlInteger) -> Option<u64> {
    (!value.is_negative())
        .then(|| value.digits().parse().ok())
        .flatten()
}

fn required_strings(
    document: &UnknownMetadataDocument,
    key: &str,
    field: GuideField,
    max_items: usize,
    max_scalars: usize,
    max_bytes: Option<usize>,
) -> Result<Vec<String>, GuideBindFailure> {
    sequence_strings(
        required(document, key, field)?,
        field,
        max_items,
        max_scalars,
        max_bytes,
    )
}
fn optional_strings(
    document: &UnknownMetadataDocument,
    key: &str,
    field: GuideField,
    max_items: usize,
    max_scalars: usize,
    max_bytes: Option<usize>,
) -> Result<Option<Vec<String>>, GuideBindFailure> {
    optional_value(document, key)
        .map(|value| sequence_strings(value, field, max_items, max_scalars, max_bytes))
        .transpose()
}
fn sequence_strings(
    value: &UnknownYamlValue,
    field: GuideField,
    max_items: usize,
    max_scalars: usize,
    max_bytes: Option<usize>,
) -> Result<Vec<String>, GuideBindFailure> {
    let sequence = value.as_sequence().ok_or_else(|| type_error(field))?;
    if sequence.len() > max_items {
        return Err(invalid(field));
    }
    let mut result = reserve_vec(sequence.len(), field)?;
    for value in sequence {
        let text = string_value(value).ok_or_else(|| type_error(field))?;
        validate_text(text, max_scalars, max_bytes, field)?;
        result.push(copy_string(text, field)?);
    }
    Ok(result)
}
fn optional_ids(
    document: &UnknownMetadataDocument,
    key: &str,
    field: GuideField,
    max: usize,
    self_id: &GuideId,
) -> Result<Option<Vec<GuideId>>, GuideBindFailure> {
    let Some(value) = optional_value(document, key) else {
        return Ok(None);
    };
    let sequence = value.as_sequence().ok_or_else(|| type_error(field))?;
    if sequence.len() > max {
        return Err(invalid(field));
    }
    let mut result = reserve_vec(sequence.len(), field)?;
    for value in sequence {
        let id = GuideId::from_str(string_value(value).ok_or_else(|| type_error(field))?)
            .map_err(|_| invalid(field))?;
        if &id == self_id || result.contains(&id) {
            return Err(invalid(field));
        }
        result.push(id);
    }
    Ok(Some(result))
}
fn unique_folded(values: &[String], field: GuideField) -> Result<(), GuideBindFailure> {
    let mut indexes: Vec<usize> = reserve_vec(values.len(), field)?;
    indexes.extend(0..values.len());
    indexes.sort_unstable_by(|left, right| {
        CaseFold::Full
            .casecmp(&values[*left], &values[*right])
            .then_with(|| left.cmp(right))
    });
    if indexes
        .windows(2)
        .any(|pair| CaseFold::Full.case_eq(&values[pair[0]], &values[pair[1]]))
    {
        return Err(invalid(field));
    }
    Ok(())
}

fn mapping_entry<'a>(
    mapping: &'a [UnknownYamlMappingEntry],
    key: &str,
) -> Option<&'a UnknownYamlMappingEntry> {
    mapping
        .iter()
        .find(|entry| string_value(entry.key()) == Some(key))
}
fn mapping_required_string<'a>(
    mapping: &'a [UnknownYamlMappingEntry],
    key: &str,
    field: GuideField,
) -> Result<&'a str, GuideBindFailure> {
    mapping_entry(mapping, key)
        .ok_or_else(|| missing(field))
        .and_then(|entry| string_value(entry.value()).ok_or_else(|| type_error(field)))
}
fn mapping_optional_value<'a>(
    mapping: &'a [UnknownYamlMappingEntry],
    key: &str,
) -> Option<&'a UnknownYamlValue> {
    mapping_entry(mapping, key).map(UnknownYamlMappingEntry::value)
}
fn mapping_text(
    mapping: &[UnknownYamlMappingEntry],
    key: &str,
    field: GuideField,
    max: usize,
    required: bool,
) -> Result<Option<String>, GuideBindFailure> {
    let Some(entry) = mapping_entry(mapping, key) else {
        return if required {
            Err(missing(field))
        } else {
            Ok(None)
        };
    };
    let value = string_value(entry.value()).ok_or_else(|| type_error(field))?;
    validate_text(value, max, None, field)?;
    copy_string(value, field).map(Some)
}
fn ensure_string_keys(
    mapping: &[UnknownYamlMappingEntry],
    field: GuideField,
) -> Result<(), GuideBindFailure> {
    if mapping
        .iter()
        .all(|entry| string_value(entry.key()).is_some())
    {
        Ok(())
    } else {
        Err(type_error(field))
    }
}
fn unknown_mapping_indices(
    mapping: &[UnknownYamlMappingEntry],
    known: &[&str],
    field: GuideField,
) -> Result<Vec<u32>, GuideBindFailure> {
    unknown_indices(mapping, |key| known.contains(&key), field)
}
fn unknown_indices<T>(
    entries: &[T],
    is_known: impl Fn(&str) -> bool,
    field: GuideField,
) -> Result<Vec<u32>, GuideBindFailure>
where
    T: MappingKey,
{
    let mut result = reserve_vec(entries.len(), field)?;
    for (index, entry) in entries.iter().enumerate() {
        if !is_known(entry.mapping_key()) {
            result.push(u32::try_from(index).map_err(|_| invariant(field))?);
        }
    }
    Ok(result)
}
trait MappingKey {
    fn mapping_key(&self) -> &str;
}
impl MappingKey for crate::domain::UnknownMetadataEntry {
    fn mapping_key(&self) -> &str {
        self.key()
    }
}
impl MappingKey for UnknownYamlMappingEntry {
    fn mapping_key(&self) -> &str {
        string_value(self.key()).unwrap_or("")
    }
}

pub(crate) fn valid_reference_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 2_048
        || !value.is_ascii()
        || value.contains('\\')
        || !valid_percent_escapes(value.as_bytes())
    {
        return false;
    }
    let Ok(parsed) = Url::parse(value) else {
        return false;
    };
    match parsed.scheme() {
        "http" | "https" => valid_http_url(value, &parsed),
        "mailto" => {
            if !value.starts_with("mailto:") {
                return false;
            }
            let suffix = &value["mailto:".len()..];
            !suffix.starts_with("//")
                && !suffix.contains('#')
                && !suffix.split('?').next().unwrap_or("").is_empty()
                && parsed.cannot_be_a_base()
        }
        _ => false,
    }
}
fn valid_http_url(value: &str, parsed: &Url) -> bool {
    let prefix = if value.starts_with("http://") {
        "http://"
    } else if value.starts_with("https://") {
        "https://"
    } else {
        return false;
    };
    let tail = &value[prefix.len()..];
    let end = tail.find(['/', '?', '#']).unwrap_or(tail.len());
    let authority = &tail[..end];
    if authority.is_empty() || authority.contains('@') || authority.contains('%') {
        return false;
    }
    let (host, port) = if authority.starts_with('[') {
        let Some(close) = authority.find(']') else {
            return false;
        };
        let host = &authority[..=close];
        let rest = &authority[close + 1..];
        if !rest.is_empty() && !rest.starts_with(':') {
            return false;
        }
        (host, rest.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
                (host, Some(port))
            }
            _ => (authority, None),
        }
    };
    if parsed.username() != ""
        || parsed.password().is_some()
        || !valid_canonical_host(host, parsed.host())
    {
        return false;
    }
    if let Some(port) = port {
        if port.starts_with('0') || port.parse::<u16>().ok().filter(|p| *p > 0).is_none() {
            return false;
        }
        if (parsed.scheme() == "http" && port == "80")
            || (parsed.scheme() == "https" && port == "443")
        {
            return false;
        }
    }
    true
}
fn valid_canonical_host(raw: &str, parsed: Option<Host<&str>>) -> bool {
    match parsed {
        Some(Host::Ipv4(parsed)) => Ipv4Addr::from_str(raw).ok().is_some_and(|address| {
            address == parsed
                && address.to_string() == raw
                && raw
                    .split('.')
                    .all(|part| part == "0" || !part.starts_with('0'))
        }),
        Some(Host::Ipv6(parsed)) => raw
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .and_then(|inner| {
                Ipv6Addr::from_str(inner)
                    .ok()
                    .map(|address| (inner, address))
            })
            .is_some_and(|(inner, address)| address == parsed && address.to_string() == inner),
        Some(Host::Domain(parsed)) => {
            raw == parsed
                && raw.len() <= 253
                && !raw.ends_with('.')
                && raw.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label
                            .bytes()
                            .next()
                            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                        && label
                            .bytes()
                            .last()
                            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                        && label
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                })
        }
        None => false,
    }
}
fn valid_percent_escapes(bytes: &[u8]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !matches!(bytes[index+1], b'0'..=b'9'|b'A'..=b'F')
                || !matches!(bytes[index+2], b'0'..=b'9'|b'A'..=b'F')
            {
                return false;
            }
            let high = hex(bytes[index + 1]);
            let low = hex(bytes[index + 2]);
            let decoded = high * 16 + low;
            if decoded <= 0x1f || (0x7f..=0x9f).contains(&decoded) || decoded == b'\\' {
                return false;
            }
            index += 3;
        } else {
            if bytes[index] <= 0x1f || (0x7f..=0x9f).contains(&bytes[index]) {
                return false;
            }
            index += 1;
        }
    }
    true
}
fn hex(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}
fn valid_attachment_path(path: &str, guide_id: &GuideId) -> bool {
    let mut parts = path.split('/');
    if parts.next() != Some("attachments")
        || parts
            .next()
            .and_then(|part| GuideId::from_str(part).ok())
            .as_ref()
            != Some(guide_id)
    {
        return false;
    }
    let Some(filename) = parts.next() else {
        return false;
    };
    parts.next().is_none() && valid_filename(filename)
}
fn valid_filename(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', '\0'])
        && value.nfc().eq(value.chars())
        && !value
            .chars()
            .any(|c| prohibited_filename_scalar(c) || super::is_noncharacter(c))
}
fn prohibited_filename_scalar(c: char) -> bool {
    matches!(c as u32, 0x00..=0x1f|0x7f..=0x9f|0x061c|0x200e|0x200f|0x2028..=0x202e|0x2066..=0x2069|0xfeff)
}
fn valid_media_type(value: &str) -> bool {
    (3..=127).contains(&value.len())
        && !value.contains(';')
        && value.split('/').count() == 2
        && value.split('/').all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b"!#$&^_.+-".contains(&b)
                })
        })
}

fn validate_text(
    value: &str,
    max_scalars: usize,
    max_bytes: Option<usize>,
    field: GuideField,
) -> Result<(), GuideBindFailure> {
    let count = value.chars().count();
    if count == 0
        || count > max_scalars
        || max_bytes.is_some_and(|max| value.len() > max)
        || value.chars().next().is_some_and(super::is_white_space)
        || value.chars().last().is_some_and(super::is_white_space)
        || value.chars().any(super::is_noncharacter)
    {
        Err(invalid(field))
    } else {
        Ok(())
    }
}
fn copy_string(value: &str, field: GuideField) -> Result<String, GuideBindFailure> {
    let mut result = String::new();
    result
        .try_reserve_exact(value.len())
        .map_err(|_| resource_limit(field))?;
    result.push_str(value);
    Ok(result)
}
fn copy_bytes(value: &[u8], field: GuideField) -> Result<Box<[u8]>, GuideBindFailure> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(value.len())
        .map_err(|_| resource_limit(field))?;
    result.extend_from_slice(value);
    Ok(result.into_boxed_slice())
}
fn reserve_vec<T>(capacity: usize, field: GuideField) -> Result<Vec<T>, GuideBindFailure> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(capacity)
        .map_err(|_| resource_limit(field))?;
    Ok(result)
}
fn missing(field: GuideField) -> GuideBindFailure {
    GuideBindFailure::new(GuideBindFailureKind::MissingRequiredField, field)
}
fn type_error(field: GuideField) -> GuideBindFailure {
    GuideBindFailure::new(GuideBindFailureKind::InvalidFieldType, field)
}
fn invalid(field: GuideField) -> GuideBindFailure {
    GuideBindFailure::new(GuideBindFailureKind::InvalidFieldValue, field)
}
fn invariant(field: GuideField) -> GuideBindFailure {
    GuideBindFailure::new(GuideBindFailureKind::BindingInvariant, field)
}
fn resource_limit(field: GuideField) -> GuideBindFailure {
    GuideBindFailure::new(GuideBindFailureKind::ResourceLimit, field)
}

fn known_positions(parsed: &ParsedGuideSource) -> Vec<(GuideField, SourcePosition)> {
    let artifact = parsed.yaml();
    let mut positions = Vec::new();
    if positions.try_reserve_exact(ROOT_FIELDS.len()).is_err() {
        return positions;
    }
    for entry in artifact.entries() {
        if entry.parent() != artifact.root_id() {
            continue;
        }
        let ReconciledEntryKind::Mapping { key, value } = entry.kind() else {
            continue;
        };
        let Some(key_text) = artifact.node(key).and_then(|node| node.decoded_scalar()) else {
            continue;
        };
        let Some((_, field)) = ROOT_FIELDS.iter().find(|(name, _)| *name == key_text) else {
            continue;
        };
        let Some(node) = artifact.node(value) else {
            continue;
        };
        let NodePresentation::Concrete {
            whole_file_range, ..
        } = node.presentation()
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
    use crate::parsing::parse_guide_source;

    fn parse(frontmatter: &str, body: &str) -> ParsedGuideSource {
        parse_guide_source(
            format!("---\n{frontmatter}---\n\n{body}")
                .into_bytes()
                .into_boxed_slice(),
        )
        .unwrap()
    }
    fn base(extra: &str) -> String {
        format!(
            "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: Install\nsummary: Do it\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags: []\naliases: []\nstatus: draft\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n{extra}"
        )
    }

    #[test]
    fn binds_draft_and_preserves_body_and_unknowns() {
        let bound = bind_guide(parse(
            &base("future: {nested: [1]}\n"),
            "## Steps\n\nDo it.\n",
        ))
        .unwrap_or_else(|_| panic!("bind"));
        assert_eq!(bound.record().title(), "Install");
        assert_eq!(bound.record().body(), b"## Steps\n\nDo it.\n");
        assert_eq!(bound.unknown_partition().root(), &[10]);
    }
    #[test]
    fn rejects_fold_duplicates_without_normalization() {
        let duplicate = base("tags: [Maße, MASSE]\n").replacen("tags: []\n", "", 1);
        assert!(bind_guide(parse(&duplicate, "")).is_err());
        let distinct = base("tags: [é, é]\n").replacen("tags: []\n", "", 1);
        assert!(bind_guide(parse(&distinct, "")).is_ok());
    }
    #[test]
    fn status_matrix_enforces_companions() {
        assert!(bind_guide(parse(&base(""), "")).is_ok());
        let tested=base("last_verified_at: 2026-09-05T10:00:00Z\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n").replacen("status: draft","status: tested",1);
        assert!(bind_guide(parse(&tested, "")).is_ok());
        assert!(
            bind_guide(parse(
                &base("").replacen("status: draft", "status: tested", 1),
                ""
            ))
            .is_err()
        );
        let archived = base("archived_from_status: draft\narchived_at: 2026-09-05T10:00:00Z\n")
            .replacen("status: draft", "status: archived", 1);
        assert!(bind_guide(parse(&archived, "")).is_ok());
    }
    #[test]
    fn validates_references_and_attachments() {
        let extra = "references:\n  - title: Upstream\n    url: https://example.com\n    future: true\nattachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/example.desktop\n    title: Desktop file\n    media_type: application/x-desktop\n    sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n    size_bytes: 42\n    future: true\n";
        let bound = bind_guide(parse(&base(extra), "")).unwrap_or_else(|_| panic!("bind"));
        assert_eq!(
            bound.record().references().unwrap()[0].url(),
            "https://example.com"
        );
        assert_eq!(
            bound.record().attachments().unwrap()[0].size_bytes(),
            Some(42)
        );
        assert_eq!(bound.unknown_partition().references()[0].as_ref(), &[2]);
        assert_eq!(bound.unknown_partition().attachments()[0].as_ref(), &[6]);
    }
    #[test]
    fn hostile_urls_and_attachment_paths_fail() {
        for url in [
            "HTTPS://example.com",
            "MAILTO:user@example.com",
            "https://user@example.com",
            "https://127.1",
            "https://0x7f.0.0.1",
            "https://0177.0.0.1",
            "https://example.com:080",
            "javascript:alert(1)",
        ] {
            let extra = format!("references:\n  - title: bad\n    url: '{url}'\n");
            assert!(bind_guide(parse(&base(&extra), "")).is_err());
        }
        let extra = "attachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/../secret\n    title: bad\n    sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
        assert!(bind_guide(parse(&base(extra), "")).is_err());
    }
    #[test]
    fn self_related_and_read_only_behavior() {
        let extra = "related_guide_ids: [0199a1d7-7b0d-7d8e-a38d-f936c5735e10]\n";
        assert!(bind_guide(parse(&base(extra), "")).is_err());
        let mut parsed = parse(&base(""), "");
        parsed.force_read_only_for_content_test();
        assert!(bind_guide(parsed).is_ok());
    }

    #[test]
    fn enforces_text_and_collection_bounds() {
        let title_200 = "a".repeat(200);
        let title_201 = "a".repeat(201);
        assert!(
            bind_guide(parse(
                &base("").replacen("title: Install", &format!("title: {title_200}"), 1),
                ""
            ))
            .is_ok()
        );
        assert!(
            bind_guide(parse(
                &base("").replacen("title: Install", &format!("title: {title_201}"), 1),
                ""
            ))
            .is_err()
        );

        let tags_64 = (0..64)
            .map(|index| format!("tag-{index}"))
            .collect::<Vec<_>>();
        let valid = base("").replacen("tags: []", &format!("tags: [{}]", tags_64.join(", ")), 1);
        assert!(bind_guide(parse(&valid, "")).is_ok());
        let mut tags_65 = tags_64;
        tags_65.push("overflow".to_owned());
        let invalid = base("").replacen("tags: []", &format!("tags: [{}]", tags_65.join(", ")), 1);
        assert!(bind_guide(parse(&invalid, "")).is_err());
    }

    #[test]
    fn rejects_nulls_wrong_kinds_and_duplicate_related_ids() {
        for replacement in ["tags: null", "tags: {}", "aliases: false", "title: null"] {
            let original = replacement.split(':').next().unwrap();
            let source = match original {
                "tags" => base("").replacen("tags: []", replacement, 1),
                "aliases" => base("").replacen("aliases: []", replacement, 1),
                "title" => base("").replacen("title: Install", replacement, 1),
                _ => unreachable!(),
            };
            assert!(bind_guide(parse(&source, "")).is_err());
        }
        let duplicate = "related_guide_ids: [0199a264-ef5f-73b4-a238-a71ba1509898, 0199a264-ef5f-73b4-a238-a71ba1509898]\n";
        assert!(bind_guide(parse(&base(duplicate), "")).is_err());

        let schema = base("").replacen("schema_version: 1", "schema_version: '1'", 1);
        let error = bind_guide(parse(&schema, "")).err().unwrap();
        assert!(matches!(
            error.kind(),
            GuideBindFailureKind::InvalidFieldType
        ));

        let attachment = "attachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/file\n    title: file\n    sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n    size_bytes: '1'\n";
        let error = bind_guide(parse(&base(attachment), "")).err().unwrap();
        assert!(matches!(
            error.kind(),
            GuideBindFailureKind::InvalidFieldType
        ));
    }

    #[test]
    fn preserves_optional_empty_and_nested_unknown_partitions() {
        let extra = "applies_to:\n  - platform: Linux\n    future: {complex: [1, true]}\nreferences: []\nattachments: []\n";
        let bound = bind_guide(parse(&base(extra), "")).unwrap_or_else(|_| panic!("bind"));
        assert!(bound.record().references().unwrap().is_empty());
        assert!(bound.record().attachments().unwrap().is_empty());
        assert_eq!(bound.unknown_partition().applicability()[0].as_ref(), &[1]);
    }

    #[test]
    fn reports_content_free_root_position_and_parsed_identity() {
        let source = base("").replacen(
            "title: Install",
            "extension:\n  title: nested\ntitle: ' trailing '",
            1,
        );
        let error = bind_guide(parse(&source, "secret body"))
            .err()
            .unwrap_or_else(|| panic!("expected failure"));
        assert!(matches!(
            error.kind(),
            GuideBindFailureKind::InvalidFieldValue
        ));
        assert!(matches!(error.field(), GuideField::Title));
        let position = error.position().unwrap();
        assert_eq!(position.line(), 6);
        assert!(error.parsed_guide_id().is_some());
    }

    #[test]
    fn rejects_noncanonical_attachment_metadata() {
        let template = |path: &str, media_type: &str, sha256: &str, size: &str| {
            format!(
                "attachments:\n  - id: 0199a264-ef5f-73b4-a238-a71ba1509898\n    path: {path}\n    title: file\n    media_type: {media_type}\n    sha256: {sha256}\n    size_bytes: {size}\n"
            )
        };
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let prefix = "attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10";
        for extra in [
            template(&format!("{prefix}/é.txt"), "text/plain", hash, "0"),
            template(&format!("{prefix}/file"), "Text/plain", hash, "0"),
            template(
                &format!("{prefix}/file"),
                "text/plain",
                &hash.to_uppercase(),
                "0",
            ),
            template(&format!("{prefix}/file"), "text/plain", hash, "-1"),
        ] {
            assert!(bind_guide(parse(&base(&extra), "")).is_err());
        }
    }
}

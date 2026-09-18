#![allow(
    dead_code,
    reason = "Phase 1D-C mutation candidates are connected to storage/application in Phase 1E"
)]

use core::cmp::max;

use crate::{
    parsing::{
        CollectionPresentationStyle, CommentOwner, NodePresentation, ParsedYamlArtifact,
        ReconciledEntryKind, RoundTripStatus, parse_guide_source, parse_yaml_document,
    },
    protocol::{CategoryId, Timestamp},
};

use super::{
    BoundCategory, BoundWorkspaceManifest, CategoryStatus, GuideStatus, ManifestBindingOutcome,
    RestorableGuideStatus, bind_category, bind_workspace_manifest,
    emission::{
        EmitFailure, EmitFailureKind, emit_plain_display, emit_yaml_display, emit_yaml_string,
        map_parse_failure,
    },
    guide::{BoundGuide, bind_guide},
};

const MAX_EDITS: usize = 8;

pub(crate) struct MutationStamp {
    at: Timestamp,
}

impl MutationStamp {
    pub(crate) fn from_trusted_time(at: Timestamp) -> Self {
        Self { at }
    }
}

pub(crate) enum WorkspaceMutation {
    SetName(String),
}

pub(crate) enum CategoryMutation {
    SetName(String),
    SetSummary(Option<String>),
    SetOrder(i64),
    Archive,
    Restore,
}

pub(crate) enum GuideMutation {
    SetTitle(String),
    SetSummary(String),
    ChangeCategory(CategoryId),
    ReplaceBody(Vec<u8>),
    MarkStale,
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum MutationEffect {
    Name,
    Order,
    Status,
    Title,
    Summary,
    CategoryId,
    UpdatedAt,
    ArchivedFromStatus,
    Body,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum MutationFailureKind {
    InvalidValue,
    InvalidTransition,
    ReadOnly,
    CommentUnsafe,
    PresentationUnsafe,
    ResourceLimit,
    CandidateMismatch,
    BindingInvariant,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum MutationField {
    Name,
    Order,
    Status,
    Title,
    Summary,
    CategoryId,
    UpdatedAt,
    ArchivedFromStatus,
    Body,
    Document,
}

pub(crate) struct MutationFailure {
    kind: MutationFailureKind,
    field: MutationField,
}

impl MutationFailure {
    pub(crate) fn kind(&self) -> MutationFailureKind {
        self.kind
    }

    pub(crate) fn field(&self) -> MutationField {
        self.field
    }
}

pub(crate) enum MutationOutcome<T> {
    NoOp,
    Candidate(MutationCandidate<T>),
}

pub(crate) struct MutationCandidate<T> {
    bound: T,
    effects: Box<[MutationEffect]>,
}

impl<T> MutationCandidate<T> {
    pub(crate) fn bound(&self) -> &T {
        &self.bound
    }

    pub(crate) fn effects(&self) -> &[MutationEffect] {
        &self.effects
    }

    pub(crate) fn into_bound(self) -> T {
        self.bound
    }
}

pub(crate) fn plan_workspace_mutation(
    bound: &BoundWorkspaceManifest,
    mutation: WorkspaceMutation,
    stamp: &MutationStamp,
) -> Result<MutationOutcome<BoundWorkspaceManifest>, MutationFailure> {
    let record = bound.record();
    let WorkspaceMutation::SetName(name) = mutation;
    if !super::valid_text(&name, 200) {
        return Err(failure(
            MutationFailureKind::InvalidValue,
            MutationField::Name,
        ));
    }
    if name == record.name() {
        return Ok(MutationOutcome::NoOp);
    }
    require_proven(bound.parsed.yaml().round_trip_status(), MutationField::Name)?;

    let updated_at = max(*record.updated_at(), stamp.at);
    let mut edits = Vec::new();
    reserve(&mut edits, 2, MutationField::Document)?;
    replace_root_value(
        bound.parsed.yaml(),
        bound.source_bytes(),
        "name",
        emit_yaml_string(&name).map_err(|error| map_emit(error, MutationField::Name))?,
        MutationEffect::Name,
        MutationField::Name,
        &mut edits,
    )?;
    replace_root_value(
        bound.parsed.yaml(),
        bound.source_bytes(),
        "updated_at",
        emit_yaml_display(updated_at).map_err(|error| map_emit(error, MutationField::UpdatedAt))?,
        MutationEffect::UpdatedAt,
        MutationField::UpdatedAt,
        &mut edits,
    )?;
    let effects = effect_set(&[MutationEffect::Name, MutationEffect::UpdatedAt])?;
    let candidate_bytes = apply_edits(bound.source_bytes(), edits)?;
    let parsed = parse_yaml_document(candidate_bytes).map_err(map_parse)?;
    let outcome =
        bind_workspace_manifest(parsed).map_err(|_| candidate_mismatch(MutationField::Document))?;
    let ManifestBindingOutcome::Current(candidate) = outcome else {
        return Err(candidate_mismatch(MutationField::Document));
    };
    let candidate_record = candidate.record();
    if candidate_record.workspace_id() != record.workspace_id()
        || candidate_record.name() != name
        || candidate_record.created_at() != record.created_at()
        || candidate_record.updated_at() != &updated_at
        || candidate_record
            .settings()
            .and_then(|value| value.default_category_id())
            != record
                .settings()
                .and_then(|value| value.default_category_id())
        || candidate.unknown_root_entries() != bound.unknown_root_entries()
        || candidate.unknown_settings_entries() != bound.unknown_settings_entries()
    {
        return Err(candidate_mismatch(MutationField::Document));
    }
    Ok(MutationOutcome::Candidate(MutationCandidate {
        bound: candidate,
        effects,
    }))
}

pub(crate) fn plan_category_mutation(
    bound: &BoundCategory,
    mutation: CategoryMutation,
    stamp: &MutationStamp,
) -> Result<MutationOutcome<BoundCategory>, MutationFailure> {
    let record = bound.record();
    let mut expected_name = record.name().to_owned();
    let mut expected_summary = record.summary().map(str::to_owned);
    let mut expected_order = record.order();
    let mut expected_status = record.status();
    let (field, effect, edit) = match mutation {
        CategoryMutation::SetName(name) => {
            if !super::valid_text(&name, 200) {
                return Err(failure(
                    MutationFailureKind::InvalidValue,
                    MutationField::Name,
                ));
            }
            if name == record.name() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_name = name.clone();
            let replacement =
                emit_yaml_string(&name).map_err(|error| map_emit(error, MutationField::Name))?;
            (
                MutationField::Name,
                MutationEffect::Name,
                CategoryEdit::Replace("name", replacement),
            )
        }
        CategoryMutation::SetSummary(summary) => {
            if summary
                .as_deref()
                .is_some_and(|value| !super::valid_text(value, 500))
            {
                return Err(failure(
                    MutationFailureKind::InvalidValue,
                    MutationField::Summary,
                ));
            }
            if summary.as_deref() == record.summary() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_summary = summary.clone();
            (
                MutationField::Summary,
                MutationEffect::Summary,
                CategoryEdit::OptionalSummary(summary),
            )
        }
        CategoryMutation::SetOrder(order) => {
            if order == record.order() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_order = order;
            (
                MutationField::Order,
                MutationEffect::Order,
                CategoryEdit::Replace(
                    "order",
                    emit_plain_display(order)
                        .map_err(|error| map_emit(error, MutationField::Order))?,
                ),
            )
        }
        CategoryMutation::Archive => {
            if record.status() == CategoryStatus::Archived {
                return Ok(MutationOutcome::NoOp);
            }
            expected_status = CategoryStatus::Archived;
            (
                MutationField::Status,
                MutationEffect::Status,
                CategoryEdit::Replace("status", Box::<[u8]>::from(b"archived".as_slice())),
            )
        }
        CategoryMutation::Restore => {
            if record.status() == CategoryStatus::Active {
                return Ok(MutationOutcome::NoOp);
            }
            expected_status = CategoryStatus::Active;
            (
                MutationField::Status,
                MutationEffect::Status,
                CategoryEdit::Replace("status", Box::<[u8]>::from(b"active".as_slice())),
            )
        }
    };
    require_proven(bound.parsed.yaml().round_trip_status(), field)?;

    let updated_at = max(*record.updated_at(), stamp.at);
    let mut edits = Vec::new();
    reserve(&mut edits, 2, MutationField::Document)?;
    match edit {
        CategoryEdit::Replace(key, replacement) => replace_root_value(
            bound.parsed.yaml(),
            bound.source_bytes(),
            key,
            replacement,
            effect,
            field,
            &mut edits,
        )?,
        CategoryEdit::OptionalSummary(Some(value)) => {
            let replacement = emit_yaml_string(&value)
                .map_err(|error| map_emit(error, MutationField::Summary))?;
            if record.summary().is_some() {
                replace_root_value(
                    bound.parsed.yaml(),
                    bound.source_bytes(),
                    "summary",
                    replacement,
                    effect,
                    field,
                    &mut edits,
                )?;
            } else {
                insert_root_value_before(
                    bound.parsed.yaml(),
                    bound.source_bytes(),
                    "order",
                    "summary",
                    replacement,
                    effect,
                    field,
                    &mut edits,
                )?;
            }
        }
        CategoryEdit::OptionalSummary(None) => remove_root_entry(
            bound.parsed.yaml(),
            bound.source_bytes(),
            "summary",
            effect,
            field,
            &mut edits,
        )?,
    }
    replace_root_value(
        bound.parsed.yaml(),
        bound.source_bytes(),
        "updated_at",
        emit_yaml_display(updated_at).map_err(|error| map_emit(error, MutationField::UpdatedAt))?,
        MutationEffect::UpdatedAt,
        MutationField::UpdatedAt,
        &mut edits,
    )?;
    let effects = effect_set(&[effect, MutationEffect::UpdatedAt])?;
    let candidate_bytes = apply_edits(bound.source_bytes(), edits)?;
    let parsed = parse_yaml_document(candidate_bytes).map_err(map_parse)?;
    let candidate =
        bind_category(parsed).map_err(|_| candidate_mismatch(MutationField::Document))?;
    let candidate_record = candidate.record();
    if candidate_record.id() != record.id()
        || candidate_record.name() != expected_name
        || candidate_record.summary() != expected_summary.as_deref()
        || candidate_record.order() != expected_order
        || candidate_record.status() != expected_status
        || candidate_record.created_at() != record.created_at()
        || candidate_record.updated_at() != &updated_at
        || candidate.unknown_root_entries() != bound.unknown_root_entries()
    {
        return Err(candidate_mismatch(MutationField::Document));
    }
    Ok(MutationOutcome::Candidate(MutationCandidate {
        bound: candidate,
        effects,
    }))
}

enum CategoryEdit {
    Replace(&'static str, Box<[u8]>),
    OptionalSummary(Option<String>),
}

pub(crate) fn plan_guide_mutation(
    bound: &BoundGuide,
    mutation: GuideMutation,
    stamp: &MutationStamp,
) -> Result<MutationOutcome<BoundGuide>, MutationFailure> {
    let record = bound.record();
    let mut target_edit: Option<(&str, Box<[u8]>, MutationEffect, MutationField)> = None;
    let mut body_replacement: Option<Box<[u8]>> = None;
    let mut expected_title = record.title();
    let mut expected_summary = record.summary();
    let mut expected_category = record.category_id();
    let mut expected_body = record.body();
    let explicit_mark_stale;

    match &mutation {
        GuideMutation::SetTitle(value) => {
            if !super::valid_text(value, 200) {
                return Err(failure(
                    MutationFailureKind::InvalidValue,
                    MutationField::Title,
                ));
            }
            if value == record.title() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_title = value;
            target_edit = Some((
                "title",
                emit_yaml_string(value).map_err(|error| map_emit(error, MutationField::Title))?,
                MutationEffect::Title,
                MutationField::Title,
            ));
            explicit_mark_stale = false;
        }
        GuideMutation::SetSummary(value) => {
            if !super::valid_text(value, 500) {
                return Err(failure(
                    MutationFailureKind::InvalidValue,
                    MutationField::Summary,
                ));
            }
            if value == record.summary() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_summary = value;
            target_edit = Some((
                "summary",
                emit_yaml_string(value).map_err(|error| map_emit(error, MutationField::Summary))?,
                MutationEffect::Summary,
                MutationField::Summary,
            ));
            explicit_mark_stale = false;
        }
        GuideMutation::ChangeCategory(value) => {
            if value == record.category_id() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_category = value;
            target_edit = Some((
                "category_id",
                emit_yaml_display(value)
                    .map_err(|error| map_emit(error, MutationField::CategoryId))?,
                MutationEffect::CategoryId,
                MutationField::CategoryId,
            ));
            explicit_mark_stale = false;
        }
        GuideMutation::ReplaceBody(value) => {
            core::str::from_utf8(value)
                .map_err(|_| failure(MutationFailureKind::InvalidValue, MutationField::Body))?;
            if value.as_slice() == record.body() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_body = value;
            let mut copy = Vec::new();
            copy.try_reserve_exact(value.len())
                .map_err(|_| failure(MutationFailureKind::ResourceLimit, MutationField::Body))?;
            copy.extend_from_slice(value);
            body_replacement = Some(copy.into_boxed_slice());
            explicit_mark_stale = false;
        }
        GuideMutation::MarkStale => {
            let already_stale = match record.declared_status() {
                GuideStatus::Stale => true,
                GuideStatus::Archived => record.archive().is_some_and(|archive| {
                    archive.archived_from_status() == RestorableGuideStatus::Stale
                }),
                GuideStatus::Draft | GuideStatus::Tested => false,
            };
            if already_stale {
                return Ok(MutationOutcome::NoOp);
            }
            explicit_mark_stale = true;
        }
    }

    let target_field = if explicit_mark_stale {
        MutationField::Status
    } else {
        target_edit
            .as_ref()
            .map_or(MutationField::Body, |value| value.3)
    };
    require_proven(bound.parsed().yaml().round_trip_status(), target_field)?;

    let updated_at = max(*record.updated_at(), stamp.at);
    let mut effects = Vec::new();
    reserve(&mut effects, 4, MutationField::Document)?;
    let mut edits = Vec::new();
    reserve(&mut edits, 4, MutationField::Document)?;

    if let Some((key, replacement, effect, field)) = target_edit {
        replace_root_value(
            bound.parsed().yaml(),
            bound.source_bytes(),
            key,
            replacement,
            effect,
            field,
            &mut edits,
        )?;
        effects.push(effect);
    }
    if let Some(replacement) = body_replacement {
        let range = bound.parsed().envelope().body_file_range();
        push_edit(
            bound.source_bytes(),
            range.start().get() as usize,
            range.end().get() as usize,
            replacement,
            MutationEffect::Body,
            MutationField::Body,
            &mut edits,
        )?;
        effects.push(MutationEffect::Body);
    }

    let (expected_status, expected_archived_from) =
        stale_guide_if_required(bound, explicit_mark_stale, &mut edits, &mut effects)?;

    replace_root_value(
        bound.parsed().yaml(),
        bound.source_bytes(),
        "updated_at",
        emit_yaml_display(updated_at).map_err(|error| map_emit(error, MutationField::UpdatedAt))?,
        MutationEffect::UpdatedAt,
        MutationField::UpdatedAt,
        &mut edits,
    )?;
    effects.push(MutationEffect::UpdatedAt);
    let effects = effect_set(&effects)?;
    let candidate_bytes = apply_edits(bound.source_bytes(), edits)?;
    let parsed = parse_guide_source(candidate_bytes).map_err(map_parse)?;
    let candidate = bind_guide(parsed).map_err(|_| candidate_mismatch(MutationField::Document))?;
    let candidate_record = candidate.record();
    if candidate_record.id() != record.id()
        || candidate_record.title() != expected_title
        || candidate_record.summary() != expected_summary
        || candidate_record.category_id() != expected_category
        || candidate_record.tags() != record.tags()
        || candidate_record.aliases() != record.aliases()
        || candidate_record.declared_status() != expected_status
        || candidate_record.created_at() != record.created_at()
        || candidate_record.updated_at() != &updated_at
        || candidate_record.body() != expected_body
        || !same_verification(candidate_record, record)
        || !same_archive(candidate_record, record, expected_archived_from)
        || !same_nested(candidate_record, record)
        || candidate.unknown_partition().root() != bound.unknown_partition().root()
        || candidate.unknown_partition().applicability()
            != bound.unknown_partition().applicability()
        || candidate.unknown_partition().references() != bound.unknown_partition().references()
        || candidate.unknown_partition().attachments() != bound.unknown_partition().attachments()
    {
        return Err(candidate_mismatch(MutationField::Document));
    }
    Ok(MutationOutcome::Candidate(MutationCandidate {
        bound: candidate,
        effects,
    }))
}

fn stale_guide_if_required(
    bound: &BoundGuide,
    explicit_mark_stale: bool,
    edits: &mut Vec<Edit>,
    effects: &mut Vec<MutationEffect>,
) -> Result<(GuideStatus, Option<RestorableGuideStatus>), MutationFailure> {
    let record = bound.record();
    let archived_from = record.archive().map(|value| value.archived_from_status());
    match record.declared_status() {
        GuideStatus::Draft if !explicit_mark_stale => Ok((GuideStatus::Draft, None)),
        GuideStatus::Draft | GuideStatus::Tested => {
            replace_root_value(
                bound.parsed().yaml(),
                bound.source_bytes(),
                "status",
                Box::<[u8]>::from(b"stale".as_slice()),
                MutationEffect::Status,
                MutationField::Status,
                edits,
            )?;
            effects.push(MutationEffect::Status);
            Ok((GuideStatus::Stale, None))
        }
        GuideStatus::Stale => Ok((GuideStatus::Stale, None)),
        GuideStatus::Archived => match archived_from {
            Some(RestorableGuideStatus::Draft) if !explicit_mark_stale => {
                Ok((GuideStatus::Archived, Some(RestorableGuideStatus::Draft)))
            }
            Some(RestorableGuideStatus::Draft | RestorableGuideStatus::Tested) => {
                replace_root_value(
                    bound.parsed().yaml(),
                    bound.source_bytes(),
                    "archived_from_status",
                    Box::<[u8]>::from(b"stale".as_slice()),
                    MutationEffect::ArchivedFromStatus,
                    MutationField::ArchivedFromStatus,
                    edits,
                )?;
                effects.push(MutationEffect::ArchivedFromStatus);
                Ok((GuideStatus::Archived, Some(RestorableGuideStatus::Stale)))
            }
            Some(RestorableGuideStatus::Stale) => {
                Ok((GuideStatus::Archived, Some(RestorableGuideStatus::Stale)))
            }
            None => Err(failure(
                MutationFailureKind::BindingInvariant,
                MutationField::ArchivedFromStatus,
            )),
        },
    }
}

fn same_verification(left: &super::Guide, right: &super::Guide) -> bool {
    match (left.verification(), right.verification()) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.last_verified_at() == right.last_verified_at()
                && left.verified_content_hash() == right.verified_content_hash()
        }
        _ => false,
    }
}

fn same_archive(
    left: &super::Guide,
    right: &super::Guide,
    expected_from: Option<RestorableGuideStatus>,
) -> bool {
    match (left.archive(), right.archive(), expected_from) {
        (None, None, None) => true,
        (Some(left), Some(right), Some(expected)) => {
            left.archived_from_status() == expected && left.archived_at() == right.archived_at()
        }
        _ => false,
    }
}

fn same_nested(left: &super::Guide, right: &super::Guide) -> bool {
    same_applicability(left, right)
        && left.prerequisites() == right.prerequisites()
        && left.warnings() == right.warnings()
        && left.files_affected() == right.files_affected()
        && left.related_guide_ids() == right.related_guide_ids()
        && same_references(left, right)
        && same_attachments(left, right)
}

fn same_applicability(left: &super::Guide, right: &super::Guide) -> bool {
    match (left.applies_to(), right.applies_to()) {
        (None, None) => true,
        (Some(left), Some(right)) if left.len() == right.len() => {
            left.iter().zip(right).all(|(left, right)| {
                left.platform() == right.platform()
                    && left.versions() == right.versions()
                    && left.notes() == right.notes()
            })
        }
        _ => false,
    }
}

fn same_references(left: &super::Guide, right: &super::Guide) -> bool {
    match (left.references(), right.references()) {
        (None, None) => true,
        (Some(left), Some(right)) if left.len() == right.len() => left
            .iter()
            .zip(right)
            .all(|(left, right)| left.title() == right.title() && left.url() == right.url()),
        _ => false,
    }
}

fn same_attachments(left: &super::Guide, right: &super::Guide) -> bool {
    match (left.attachments(), right.attachments()) {
        (None, None) => true,
        (Some(left), Some(right)) if left.len() == right.len() => {
            left.iter().zip(right).all(|(left, right)| {
                left.id() == right.id()
                    && left.path() == right.path()
                    && left.title() == right.title()
                    && left.media_type() == right.media_type()
                    && left.sha256() == right.sha256()
                    && left.size_bytes() == right.size_bytes()
            })
        }
        _ => false,
    }
}

struct Edit {
    start: usize,
    end: usize,
    replacement: Box<[u8]>,
    effect: MutationEffect,
}

fn replace_root_value(
    artifact: &ParsedYamlArtifact,
    source: &[u8],
    key_name: &str,
    replacement: Box<[u8]>,
    effect: MutationEffect,
    field: MutationField,
    edits: &mut Vec<Edit>,
) -> Result<(), MutationFailure> {
    let entry = artifact
        .entries()
        .iter()
        .find(|entry| {
            if entry.parent() != artifact.root_id() {
                return false;
            }
            let ReconciledEntryKind::Mapping { key, .. } = entry.kind() else {
                return false;
            };
            artifact.node(key).and_then(|node| node.decoded_scalar()) == Some(key_name)
        })
        .ok_or_else(|| failure(MutationFailureKind::BindingInvariant, field))?;
    let ReconciledEntryKind::Mapping { value, .. } = entry.kind() else {
        return Err(failure(MutationFailureKind::BindingInvariant, field));
    };
    let node = artifact
        .node(value)
        .ok_or_else(|| failure(MutationFailureKind::BindingInvariant, field))?;
    let NodePresentation::Concrete {
        whole_file_range, ..
    } = node.presentation()
    else {
        return Err(failure(MutationFailureKind::PresentationUnsafe, field));
    };
    let start = whole_file_range.start().get() as usize;
    let end = whole_file_range.end().get() as usize;
    if artifact.comments().iter().any(|comment| {
        let range = comment.file_range();
        ranges_overlap(
            start,
            end,
            range.start().get() as usize,
            range.end().get() as usize,
        )
    }) {
        return Err(failure(MutationFailureKind::CommentUnsafe, field));
    }
    push_edit(source, start, end, replacement, effect, field, edits)
}

#[allow(clippy::too_many_arguments)]
fn insert_root_value_before(
    artifact: &ParsedYamlArtifact,
    source: &[u8],
    anchor_name: &str,
    key_name: &str,
    value: Box<[u8]>,
    effect: MutationEffect,
    field: MutationField,
    edits: &mut Vec<Edit>,
) -> Result<(), MutationFailure> {
    let anchor = artifact
        .entries()
        .iter()
        .find(|entry| {
            if entry.parent() != artifact.root_id() {
                return false;
            }
            let ReconciledEntryKind::Mapping { key, .. } = entry.kind() else {
                return false;
            };
            artifact.node(key).and_then(|node| node.decoded_scalar()) == Some(anchor_name)
        })
        .ok_or_else(|| failure(MutationFailureKind::BindingInvariant, field))?;
    if anchor.collection_style() != CollectionPresentationStyle::Block {
        return Err(failure(MutationFailureKind::PresentationUnsafe, field));
    }
    let start = anchor.file_range().start().get() as usize;
    if start > source.len() || (start != 0 && source.get(start.wrapping_sub(1)) != Some(&b'\n')) {
        return Err(failure(MutationFailureKind::PresentationUnsafe, field));
    }
    let gap_start = artifact
        .entries()
        .iter()
        .filter(|entry| entry.parent() == artifact.root_id() && entry.index() < anchor.index())
        .max_by_key(|entry| entry.index())
        .map_or(0, |entry| entry.file_range().end().get() as usize);
    if gap_start > start
        || artifact.comments().iter().any(|comment| {
            let range = comment.file_range();
            (range.start().get() as usize) < start && (range.end().get() as usize) > gap_start
        })
    {
        return Err(failure(MutationFailureKind::CommentUnsafe, field));
    }
    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(key_name.len() + value.len() + 3)
        .map_err(|_| failure(MutationFailureKind::ResourceLimit, field))?;
    replacement.extend_from_slice(key_name.as_bytes());
    replacement.extend_from_slice(b": ");
    replacement.extend_from_slice(&value);
    replacement.push(b'\n');
    push_edit(
        source,
        start,
        start,
        replacement.into_boxed_slice(),
        effect,
        field,
        edits,
    )
}

fn remove_root_entry(
    artifact: &ParsedYamlArtifact,
    source: &[u8],
    key_name: &str,
    effect: MutationEffect,
    field: MutationField,
    edits: &mut Vec<Edit>,
) -> Result<(), MutationFailure> {
    let entry = artifact
        .entries()
        .iter()
        .find(|entry| {
            if entry.parent() != artifact.root_id() {
                return false;
            }
            let ReconciledEntryKind::Mapping { key, .. } = entry.kind() else {
                return false;
            };
            artifact.node(key).and_then(|node| node.decoded_scalar()) == Some(key_name)
        })
        .ok_or_else(|| failure(MutationFailureKind::BindingInvariant, field))?;
    let ReconciledEntryKind::Mapping { key, value } = entry.kind() else {
        return Err(failure(MutationFailureKind::BindingInvariant, field));
    };
    if entry.collection_style() != CollectionPresentationStyle::Block {
        return Err(failure(MutationFailureKind::PresentationUnsafe, field));
    }
    let start = entry.file_range().start().get() as usize;
    let entry_end = entry.file_range().end().get() as usize;
    if start > entry_end
        || entry_end > source.len()
        || (start != 0 && source.get(start.wrapping_sub(1)) != Some(&b'\n'))
    {
        return Err(failure(MutationFailureKind::PresentationUnsafe, field));
    }
    let end = if source.get(entry_end.wrapping_sub(1)) == Some(&b'\n') {
        entry_end
    } else {
        source[entry_end..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(source.len(), |offset| entry_end + offset + 1)
    };
    if source[entry_end..end]
        .iter()
        .any(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        return Err(failure(MutationFailureKind::PresentationUnsafe, field));
    }
    if artifact.comments().iter().any(|comment| {
        let range = comment.file_range();
        let overlaps = ranges_overlap(
            start,
            end,
            range.start().get() as usize,
            range.end().get() as usize,
        );
        let owned = match comment.owner() {
            CommentOwner::Entry(owner) => owner == entry.id(),
            CommentOwner::Node(owner) => owner == key || owner == value,
            CommentOwner::Boundary { before, after, .. } => {
                before == Some(entry.id()) || after == Some(entry.id())
            }
            CommentOwner::Ambiguous => true,
        };
        overlaps || owned
    }) {
        return Err(failure(MutationFailureKind::CommentUnsafe, field));
    }
    push_edit(
        source,
        start,
        end,
        Box::<[u8]>::default(),
        effect,
        field,
        edits,
    )
}

fn push_edit(
    source: &[u8],
    start: usize,
    end: usize,
    replacement: Box<[u8]>,
    effect: MutationEffect,
    field: MutationField,
    edits: &mut Vec<Edit>,
) -> Result<(), MutationFailure> {
    let text = core::str::from_utf8(source)
        .map_err(|_| failure(MutationFailureKind::BindingInvariant, field))?;
    if start > end
        || end > source.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return Err(failure(MutationFailureKind::BindingInvariant, field));
    }
    if source[start..end] != *replacement {
        edits.push(Edit {
            start,
            end,
            replacement,
            effect,
        });
    }
    Ok(())
}

fn apply_edits(source: &[u8], mut edits: Vec<Edit>) -> Result<Box<[u8]>, MutationFailure> {
    if edits.is_empty() || edits.len() > MAX_EDITS {
        return Err(failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Document,
        ));
    }
    edits.sort_unstable_by_key(|edit| edit.start);
    let mut previous_end = 0usize;
    let mut final_len = source.len();
    for edit in &edits {
        if edit.start < previous_end || edit.end > source.len() {
            return Err(failure(
                MutationFailureKind::BindingInvariant,
                MutationField::Document,
            ));
        }
        previous_end = edit.end;
        final_len = final_len
            .checked_sub(edit.end - edit.start)
            .and_then(|value| value.checked_add(edit.replacement.len()))
            .ok_or_else(|| failure(MutationFailureKind::ResourceLimit, MutationField::Document))?;
    }
    let mut candidate = Vec::new();
    candidate
        .try_reserve_exact(final_len)
        .map_err(|_| failure(MutationFailureKind::ResourceLimit, MutationField::Document))?;
    let mut cursor = 0usize;
    for edit in edits {
        candidate.extend_from_slice(&source[cursor..edit.start]);
        candidate.extend_from_slice(&edit.replacement);
        cursor = edit.end;
        let _ = edit.effect;
    }
    candidate.extend_from_slice(&source[cursor..]);
    if candidate.len() != final_len {
        return Err(failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Document,
        ));
    }
    Ok(candidate.into_boxed_slice())
}

fn effect_set(values: &[MutationEffect]) -> Result<Box<[MutationEffect]>, MutationFailure> {
    let mut effects = Vec::new();
    reserve(&mut effects, values.len(), MutationField::Document)?;
    effects.extend_from_slice(values);
    effects.sort_unstable();
    effects.dedup();
    if effects.len() > MAX_EDITS {
        return Err(failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Document,
        ));
    }
    Ok(effects.into_boxed_slice())
}

fn require_proven(status: &RoundTripStatus, field: MutationField) -> Result<(), MutationFailure> {
    if status.is_proven() {
        Ok(())
    } else {
        Err(failure(MutationFailureKind::ReadOnly, field))
    }
}

fn reserve<T>(
    values: &mut Vec<T>,
    count: usize,
    field: MutationField,
) -> Result<(), MutationFailure> {
    values
        .try_reserve_exact(count)
        .map_err(|_| failure(MutationFailureKind::ResourceLimit, field))
}

fn ranges_overlap(
    left_start: usize,
    left_end: usize,
    right_start: usize,
    right_end: usize,
) -> bool {
    left_start < right_end && right_start < left_end
}

fn map_emit(error: EmitFailure, field: MutationField) -> MutationFailure {
    match error.kind() {
        EmitFailureKind::ResourceLimit => failure(MutationFailureKind::ResourceLimit, field),
        EmitFailureKind::InvalidValue => failure(MutationFailureKind::InvalidValue, field),
        EmitFailureKind::CandidateMismatch => failure(MutationFailureKind::BindingInvariant, field),
    }
}

fn map_parse(error: crate::parsing::ParseFailure) -> MutationFailure {
    let error = map_parse_failure(error);
    match error.kind() {
        EmitFailureKind::ResourceLimit => {
            failure(MutationFailureKind::ResourceLimit, MutationField::Document)
        }
        _ => candidate_mismatch(MutationField::Document),
    }
}

fn failure(kind: MutationFailureKind, field: MutationField) -> MutationFailure {
    MutationFailure { kind, field }
}

fn candidate_mismatch(field: MutationField) -> MutationFailure {
    failure(MutationFailureKind::CandidateMismatch, field)
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::*;
    use crate::parsing::{parse_guide_source, parse_yaml_document};

    fn timestamp(value: &str) -> Timestamp {
        Timestamp::from_str(value).unwrap()
    }

    fn workspace(source: &str) -> BoundWorkspaceManifest {
        let parsed = parse_yaml_document(source.as_bytes().to_vec().into_boxed_slice()).unwrap();
        match bind_workspace_manifest(parsed).unwrap_or_else(|_| panic!("bind")) {
            ManifestBindingOutcome::Current(value) => value,
            ManifestBindingOutcome::NewerUnsupported { .. } => panic!("newer"),
        }
    }

    fn category(source: &str) -> BoundCategory {
        bind_category(parse_yaml_document(source.as_bytes().to_vec().into_boxed_slice()).unwrap())
            .unwrap_or_else(|_| panic!("bind"))
    }

    fn guide(frontmatter: &str, body: &str) -> BoundGuide {
        let source = format!("---\n{frontmatter}---\n\n{body}");
        bind_guide(parse_guide_source(source.into_bytes().into_boxed_slice()).unwrap())
            .unwrap_or_else(|_| panic!("bind"))
    }

    fn guide_base(status: &str, extra: &str) -> String {
        format!(
            "schema_version: 1\nid: 0199a1d7-7b0d-7d8e-a38d-f936c5735e10\ntitle: 'Old title'\nsummary: Old summary\ncategory_id: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\ntags: []\naliases: []\nstatus: {status}\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n{extra}future: {{nested: [1]}}\n"
        )
    }

    #[test]
    fn workspace_name_preserves_unknown_comment_and_uses_stamp_max() {
        let bound = workspace(
            "schema_version: 1\nworkspace_id: 0199a19f-4d15-7ca7-a645-e75311caa710\nname: 'Old' # keep\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\nfuture: {nested: [1]}\n",
        );
        let outcome = plan_workspace_mutation(
            &bound,
            WorkspaceMutation::SetName("New".into()),
            &MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z")),
        )
        .unwrap_or_else(|_| panic!("plan"));
        let MutationOutcome::Candidate(candidate) = outcome else {
            panic!("candidate")
        };
        let source = core::str::from_utf8(candidate.bound().source_bytes()).unwrap();
        assert!(source.contains("name: \"New\" # keep"));
        assert!(source.contains("future: {nested: [1]}"));
        assert!(source.contains("updated_at: \"2026-09-05T11:00:00Z\""));
        assert!(candidate.effects() == [MutationEffect::Name, MutationEffect::UpdatedAt]);
    }

    #[test]
    fn no_op_precedes_read_only_but_actual_change_refuses() {
        let mut bound = workspace(
            "schema_version: 1\nworkspace_id: 0199a19f-4d15-7ca7-a645-e75311caa710\nname: Old\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n",
        );
        bound.parsed.force_read_only_for_content_test();
        assert!(matches!(
            plan_workspace_mutation(
                &bound,
                WorkspaceMutation::SetName("Old".into()),
                &MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"))
            ),
            Ok(MutationOutcome::NoOp)
        ));
        let error = plan_workspace_mutation(
            &bound,
            WorkspaceMutation::SetName("New".into()),
            &MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z")),
        )
        .err()
        .unwrap();
        assert!(matches!(error.kind(), MutationFailureKind::ReadOnly));
    }

    #[test]
    fn category_status_and_order_transitions_rebind() {
        let source = "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux\norder: 1\nstatus: active\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\nfuture: keep\n";
        let bound = category(source);
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        let MutationOutcome::Candidate(archived) =
            plan_category_mutation(&bound, CategoryMutation::Archive, &stamp)
                .unwrap_or_else(|_| panic!("archive"))
        else {
            panic!("candidate")
        };
        assert_eq!(archived.bound().record().status(), CategoryStatus::Archived);
        let MutationOutcome::Candidate(restored) =
            plan_category_mutation(archived.bound(), CategoryMutation::Restore, &stamp)
                .unwrap_or_else(|_| panic!("restore"))
        else {
            panic!("candidate")
        };
        assert_eq!(restored.bound().record().status(), CategoryStatus::Active);
        let MutationOutcome::Candidate(ordered) =
            plan_category_mutation(&bound, CategoryMutation::SetOrder(i64::MIN), &stamp)
                .unwrap_or_else(|_| panic!("order"))
        else {
            panic!("candidate")
        };
        assert_eq!(ordered.bound().record().order(), i64::MIN);
        assert!(
            core::str::from_utf8(ordered.bound().source_bytes())
                .unwrap()
                .contains("future: keep")
        );
    }

    #[test]
    fn category_optional_summary_inserts_replaces_and_removes() {
        let source = "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux\nfuture: {nested: [1]}\norder: 1\nstatus: active\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n";
        let bound = category(source);
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        let MutationOutcome::Candidate(inserted) = plan_category_mutation(
            &bound,
            CategoryMutation::SetSummary(Some("System work".into())),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("insert summary")) else {
            panic!("candidate")
        };
        assert_eq!(inserted.bound().record().summary(), Some("System work"));
        assert!(
            core::str::from_utf8(inserted.bound().source_bytes())
                .unwrap()
                .contains("future: {nested: [1]}\nsummary: \"System work\"\norder: 1")
        );

        let MutationOutcome::Candidate(replaced) = plan_category_mutation(
            inserted.bound(),
            CategoryMutation::SetSummary(Some("Desktop work".into())),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("replace summary")) else {
            panic!("candidate")
        };
        assert_eq!(replaced.bound().record().summary(), Some("Desktop work"));

        let Ok(MutationOutcome::Candidate(removed)) =
            plan_category_mutation(replaced.bound(), CategoryMutation::SetSummary(None), &stamp)
        else {
            panic!("remove summary")
        };
        assert_eq!(removed.bound().record().summary(), None);
        let removed_source = core::str::from_utf8(removed.bound().source_bytes()).unwrap();
        assert!(!removed_source.contains("summary:"));
        assert!(removed_source.contains("future: {nested: [1]}"));
    }

    #[test]
    fn category_summary_removal_refuses_owned_comment() {
        let source = "schema_version: 1\nid: 0199a1c2-d334-7d17-bd9c-f8b6ba3a5551\nname: Linux\nsummary: Keep this # context\norder: 1\nstatus: active\ncreated_at: 2026-09-05T10:00:00Z\nupdated_at: 2026-09-05T10:00:00Z\n";
        let error = plan_category_mutation(
            &category(source),
            CategoryMutation::SetSummary(None),
            &MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z")),
        )
        .err()
        .unwrap();
        assert!(matches!(error.kind(), MutationFailureKind::CommentUnsafe));
        assert!(matches!(error.field(), MutationField::Summary));
    }

    #[test]
    fn tested_and_archived_tested_edits_become_stale() {
        let verification = "last_verified_at: 2026-09-05T10:00:00Z\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
        let tested = guide(&guide_base("tested", verification), "Body\n");
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        let MutationOutcome::Candidate(candidate) =
            plan_guide_mutation(&tested, GuideMutation::SetTitle("New title".into()), &stamp)
                .unwrap_or_else(|_| panic!("plan"))
        else {
            panic!("candidate")
        };
        assert_eq!(
            candidate.bound().record().declared_status(),
            GuideStatus::Stale
        );
        assert!(candidate.bound().record().verification().is_some());

        let archived_extra = format!(
            "{verification}archived_from_status: tested\narchived_at: 2026-09-05T10:00:00Z\n"
        );
        let archived = guide(&guide_base("archived", &archived_extra), "Body\n");
        let MutationOutcome::Candidate(candidate) = plan_guide_mutation(
            &archived,
            GuideMutation::SetSummary("New summary".into()),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("plan")) else {
            panic!("candidate")
        };
        assert_eq!(
            candidate
                .bound()
                .record()
                .archive()
                .unwrap()
                .archived_from_status(),
            RestorableGuideStatus::Stale
        );
    }

    #[test]
    fn body_change_preserves_frontmatter_and_mark_stale_matrix() {
        let draft = guide(&guide_base("draft", ""), "Old body\n");
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T09:00:00Z"));
        let MutationOutcome::Candidate(candidate) = plan_guide_mutation(
            &draft,
            GuideMutation::ReplaceBody(b"New body\n".to_vec()),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("body")) else {
            panic!("candidate")
        };
        assert_eq!(candidate.bound().record().body(), b"New body\n");
        assert_eq!(
            candidate.bound().record().updated_at(),
            draft.record().updated_at()
        );
        assert!(
            str::from_utf8(candidate.bound().source_bytes())
                .unwrap()
                .contains("future: {nested: [1]}")
        );

        let MutationOutcome::Candidate(stale) = plan_guide_mutation(
            &draft,
            GuideMutation::MarkStale,
            &MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z")),
        )
        .unwrap_or_else(|_| panic!("stale")) else {
            panic!("candidate")
        };
        assert_eq!(stale.bound().record().declared_status(), GuideStatus::Stale);
        assert!(matches!(
            plan_guide_mutation(
                stale.bound(),
                GuideMutation::MarkStale,
                &MutationStamp::from_trusted_time(timestamp("2026-09-05T12:00:00Z"))
            ),
            Ok(MutationOutcome::NoOp)
        ));
    }

    #[test]
    fn guide_no_op_precedes_read_only_and_invalid_payload_precedes_no_op() {
        let mut bound = guide(&guide_base("draft", ""), "Body\n");
        bound
            .parsed_mut_for_content_test()
            .force_read_only_for_content_test();
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        assert!(matches!(
            plan_guide_mutation(&bound, GuideMutation::SetTitle("Old title".into()), &stamp),
            Ok(MutationOutcome::NoOp)
        ));
        let error = plan_guide_mutation(&bound, GuideMutation::SetTitle(" changed".into()), &stamp)
            .err()
            .unwrap();
        assert!(matches!(error.kind(), MutationFailureKind::InvalidValue));
        assert!(matches!(error.field(), MutationField::Title));
    }
}

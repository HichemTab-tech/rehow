#![allow(
    dead_code,
    reason = "Phase 1D-C mutation candidates are connected to storage/application in Phase 1E"
)]

use core::cmp::max;
use focaccia::CaseFold;

use crate::{
    parsing::{
        CollectionPresentationStyle, CommentOwner, NodePresentation, ParsedYamlArtifact,
        ReconciledEntryKind, RoundTripStatus, SourceLineEnding, parse_guide_source,
        parse_yaml_document,
    },
    protocol::{AttachmentId, CategoryId, Timestamp, VerificationHash},
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
    ReplaceTags(Vec<String>),
    ReplaceAliases(Vec<String>),
    ReplaceBody(Vec<u8>),
    AddAttachment(AttachmentInput),
    UpdateAttachmentTitle {
        attachment_id: AttachmentId,
        title: String,
    },
    RemoveAttachment(AttachmentId),
    MarkTested(VerificationHash),
    MarkStale,
    Archive,
    Restore,
}

pub(crate) struct AttachmentInput {
    pub(crate) id: AttachmentId,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) sha256: String,
    pub(crate) size_bytes: u64,
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum MutationEffect {
    Name,
    Order,
    Status,
    Title,
    Summary,
    CategoryId,
    Tags,
    Aliases,
    UpdatedAt,
    LastVerifiedAt,
    VerifiedContentHash,
    ArchivedFromStatus,
    ArchivedAt,
    Body,
    Attachments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MutationField {
    Name,
    Order,
    Status,
    Title,
    Summary,
    CategoryId,
    Tags,
    Aliases,
    UpdatedAt,
    LastVerifiedAt,
    VerifiedContentHash,
    ArchivedFromStatus,
    ArchivedAt,
    Body,
    Attachments,
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
    let mutation = match mutation {
        GuideMutation::AddAttachment(attachment) => {
            return plan_add_attachment(bound, attachment, stamp);
        }
        GuideMutation::UpdateAttachmentTitle {
            attachment_id,
            title,
        } => {
            return plan_update_attachment_title(bound, attachment_id, title, stamp);
        }
        GuideMutation::RemoveAttachment(attachment_id) => {
            return plan_remove_attachment(bound, attachment_id, stamp);
        }
        mutation => mutation,
    };
    let record = bound.record();
    let mut target_edit: Option<(&str, Box<[u8]>, MutationEffect, MutationField)> = None;
    let mut sequence_edit: Option<(&str, &[String], MutationEffect, MutationField)> = None;
    let mut body_replacement: Option<Box<[u8]>> = None;
    let mut expected_title = record.title();
    let mut expected_summary = record.summary();
    let mut expected_category = record.category_id();
    let mut expected_tags = record.tags();
    let mut expected_aliases = record.aliases();
    let mut expected_body = record.body();
    let explicit_mark_stale;
    let lifecycle_edit;

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
            lifecycle_edit = GuideLifecycleEdit::None;
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
            lifecycle_edit = GuideLifecycleEdit::None;
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
            lifecycle_edit = GuideLifecycleEdit::None;
        }
        GuideMutation::ReplaceTags(values) => {
            validate_string_list(values, 64, 64, MutationField::Tags)?;
            if values.as_slice() == record.tags() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_tags = values;
            sequence_edit = Some(("tags", values, MutationEffect::Tags, MutationField::Tags));
            explicit_mark_stale = false;
            lifecycle_edit = GuideLifecycleEdit::None;
        }
        GuideMutation::ReplaceAliases(values) => {
            validate_string_list(values, 64, 200, MutationField::Aliases)?;
            if values.as_slice() == record.aliases() {
                return Ok(MutationOutcome::NoOp);
            }
            expected_aliases = values;
            sequence_edit = Some((
                "aliases",
                values,
                MutationEffect::Aliases,
                MutationField::Aliases,
            ));
            explicit_mark_stale = false;
            lifecycle_edit = GuideLifecycleEdit::None;
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
            lifecycle_edit = GuideLifecycleEdit::None;
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
            lifecycle_edit = GuideLifecycleEdit::None;
        }
        GuideMutation::MarkTested(hash) => {
            if record.declared_status() == GuideStatus::Archived {
                return Err(failure(
                    MutationFailureKind::InvalidTransition,
                    MutationField::Status,
                ));
            }
            if record.declared_status() == GuideStatus::Tested
                && record
                    .verification()
                    .and_then(|verification| verification.verified_content_hash())
                    == Some(hash)
            {
                return Ok(MutationOutcome::NoOp);
            }
            explicit_mark_stale = false;
            lifecycle_edit = GuideLifecycleEdit::Tested(*hash);
        }
        GuideMutation::Archive => {
            let archived_from = match record.declared_status() {
                GuideStatus::Draft => RestorableGuideStatus::Draft,
                GuideStatus::Tested | GuideStatus::Stale => RestorableGuideStatus::Stale,
                GuideStatus::Archived => return Ok(MutationOutcome::NoOp),
            };
            explicit_mark_stale = false;
            lifecycle_edit = GuideLifecycleEdit::Archive(archived_from);
        }
        GuideMutation::Restore => {
            let Some(archive) = record.archive() else {
                return Err(failure(
                    MutationFailureKind::InvalidTransition,
                    MutationField::Status,
                ));
            };
            let restored_status = match archive.archived_from_status() {
                RestorableGuideStatus::Draft => GuideStatus::Draft,
                RestorableGuideStatus::Tested | RestorableGuideStatus::Stale => GuideStatus::Stale,
            };
            explicit_mark_stale = false;
            lifecycle_edit = GuideLifecycleEdit::Restore(restored_status);
        }
        GuideMutation::AddAttachment(_) => unreachable!("handled before the mutation match"),
        GuideMutation::UpdateAttachmentTitle { .. } => {
            unreachable!("handled before the mutation match")
        }
        GuideMutation::RemoveAttachment(_) => unreachable!("handled before the mutation match"),
    }

    let target_field = if explicit_mark_stale || !matches!(lifecycle_edit, GuideLifecycleEdit::None)
    {
        MutationField::Status
    } else {
        target_edit
            .as_ref()
            .map(|value| value.3)
            .or_else(|| sequence_edit.as_ref().map(|value| value.3))
            .unwrap_or(MutationField::Body)
    };
    require_proven(bound.parsed().yaml().round_trip_status(), target_field)?;

    let updated_at = max(*record.updated_at(), stamp.at);
    let mut effects = Vec::new();
    reserve(&mut effects, 8, MutationField::Document)?;
    let mut edits = Vec::new();
    reserve(&mut edits, 8, MutationField::Document)?;

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
    if let Some((key, values, effect, field)) = sequence_edit {
        replace_root_string_sequence(
            bound.parsed().yaml(),
            bound.source_bytes(),
            key,
            values,
            effect,
            field,
            &mut edits,
        )?;
        effects.push(effect);
    }
    if let Some(mut replacement) = body_replacement {
        let envelope = bound.parsed().envelope();
        let range = envelope.body_file_range();
        // A hand-written Guide may end at its closing `---` with no line
        // terminator. A non-empty body then needs exactly one terminator so
        // the delimiter stays a line of its own; no blank line is inserted.
        let closing = envelope.closing_delimiter();
        if !replacement.is_empty()
            && range.is_empty()
            && closing.end() == range.start()
            && !bound.source_bytes()[..closing.end().get() as usize].ends_with(b"\n")
        {
            let newline: &[u8] = match envelope.yaml_line_ending() {
                SourceLineEnding::CrLf => b"\r\n",
                SourceLineEnding::Lf | SourceLineEnding::None => b"\n",
            };
            let mut prefixed = Vec::new();
            prefixed
                .try_reserve_exact(newline.len() + replacement.len())
                .map_err(|_| failure(MutationFailureKind::ResourceLimit, MutationField::Body))?;
            prefixed.extend_from_slice(newline);
            prefixed.extend_from_slice(&replacement);
            replacement = prefixed.into_boxed_slice();
        }
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

    let mut expected_verification = record.verification().map(|verification| {
        (
            *verification.last_verified_at(),
            verification.verified_content_hash().copied(),
        )
    });
    let (expected_status, expected_archived_from, expected_archived_at) = match lifecycle_edit {
        GuideLifecycleEdit::None => {
            let (status, archived_from) =
                stale_guide_if_required(bound, explicit_mark_stale, &mut edits, &mut effects)?;
            (
                status,
                archived_from,
                record.archive().map(|archive| *archive.archived_at()),
            )
        }
        GuideLifecycleEdit::Archive(archived_from) => {
            replace_root_value(
                bound.parsed().yaml(),
                bound.source_bytes(),
                "status",
                Box::<[u8]>::from(b"archived".as_slice()),
                MutationEffect::Status,
                MutationField::Status,
                &mut edits,
            )?;
            effects.push(MutationEffect::Status);
            insert_archive_metadata_before(
                bound.parsed().yaml(),
                bound.source_bytes(),
                archived_from,
                updated_at,
                &mut edits,
            )?;
            effects.push(MutationEffect::ArchivedFromStatus);
            effects.push(MutationEffect::ArchivedAt);
            (GuideStatus::Archived, Some(archived_from), Some(updated_at))
        }
        GuideLifecycleEdit::Tested(hash) => {
            replace_root_value(
                bound.parsed().yaml(),
                bound.source_bytes(),
                "status",
                Box::<[u8]>::from(b"tested".as_slice()),
                MutationEffect::Status,
                MutationField::Status,
                &mut edits,
            )?;
            effects.push(MutationEffect::Status);
            if record.verification().is_some() {
                replace_root_value(
                    bound.parsed().yaml(),
                    bound.source_bytes(),
                    "last_verified_at",
                    emit_yaml_display(updated_at)
                        .map_err(|error| map_emit(error, MutationField::LastVerifiedAt))?,
                    MutationEffect::LastVerifiedAt,
                    MutationField::LastVerifiedAt,
                    &mut edits,
                )?;
                replace_root_value(
                    bound.parsed().yaml(),
                    bound.source_bytes(),
                    "verified_content_hash",
                    emit_yaml_display(hash)
                        .map_err(|error| map_emit(error, MutationField::VerifiedContentHash))?,
                    MutationEffect::VerifiedContentHash,
                    MutationField::VerifiedContentHash,
                    &mut edits,
                )?;
            } else {
                insert_verification_metadata_before(
                    bound.parsed().yaml(),
                    bound.source_bytes(),
                    updated_at,
                    &hash,
                    &mut edits,
                )?;
            }
            effects.push(MutationEffect::LastVerifiedAt);
            effects.push(MutationEffect::VerifiedContentHash);
            expected_verification = Some((updated_at, Some(hash)));
            (GuideStatus::Tested, None, None)
        }
        GuideLifecycleEdit::Restore(restored_status) => {
            let status = match restored_status {
                GuideStatus::Draft => b"draft".as_slice(),
                GuideStatus::Stale => b"stale".as_slice(),
                GuideStatus::Tested | GuideStatus::Archived => {
                    return Err(failure(
                        MutationFailureKind::BindingInvariant,
                        MutationField::Status,
                    ));
                }
            };
            replace_root_value(
                bound.parsed().yaml(),
                bound.source_bytes(),
                "status",
                Box::<[u8]>::from(status),
                MutationEffect::Status,
                MutationField::Status,
                &mut edits,
            )?;
            remove_root_entry(
                bound.parsed().yaml(),
                bound.source_bytes(),
                "archived_from_status",
                MutationEffect::ArchivedFromStatus,
                MutationField::ArchivedFromStatus,
                &mut edits,
            )?;
            remove_root_entry(
                bound.parsed().yaml(),
                bound.source_bytes(),
                "archived_at",
                MutationEffect::ArchivedAt,
                MutationField::ArchivedAt,
                &mut edits,
            )?;
            effects.push(MutationEffect::Status);
            effects.push(MutationEffect::ArchivedFromStatus);
            effects.push(MutationEffect::ArchivedAt);
            (restored_status, None, None)
        }
    };

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
        || candidate_record.tags() != expected_tags
        || candidate_record.aliases() != expected_aliases
        || candidate_record.declared_status() != expected_status
        || candidate_record.created_at() != record.created_at()
        || candidate_record.updated_at() != &updated_at
        || candidate_record.body() != expected_body
        || !same_expected_verification(candidate_record, expected_verification.as_ref())
        || !same_archive(
            candidate_record,
            expected_archived_from,
            expected_archived_at,
        )
        || !same_nested(candidate_record, record)
        || !same_unknown_root(&candidate, bound)
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

fn plan_add_attachment(
    bound: &BoundGuide,
    attachment: AttachmentInput,
    stamp: &MutationStamp,
) -> Result<MutationOutcome<BoundGuide>, MutationFailure> {
    let record = bound.record();
    if record.declared_status() == GuideStatus::Archived {
        return Err(failure(
            MutationFailureKind::InvalidTransition,
            MutationField::Attachments,
        ));
    }
    let existing = record.attachments().unwrap_or_default();
    if existing.len() >= 256
        || existing.iter().any(|value| {
            value.id() == &attachment.id
                || value.path() == attachment.path
                || value.sha256() == attachment.sha256
        })
    {
        return Err(failure(
            MutationFailureKind::InvalidValue,
            MutationField::Attachments,
        ));
    }
    require_proven(
        bound.parsed().yaml().round_trip_status(),
        MutationField::Attachments,
    )?;

    let newline: &[u8] = bound
        .source_bytes()
        .iter()
        .position(|byte| *byte == b'\n')
        .filter(|offset| *offset > 0 && bound.source_bytes()[*offset - 1] == b'\r')
        .map_or(b"\n", |_| b"\r\n");
    let item = emit_attachment_item(&attachment, newline)?;
    let mut edits = Vec::new();
    reserve(&mut edits, 4, MutationField::Document)?;
    append_attachment_entry(bound, item, newline, &mut edits)?;

    let mut effects = Vec::new();
    reserve(&mut effects, 4, MutationField::Document)?;
    effects.push(MutationEffect::Attachments);
    let (expected_status, expected_archived_from) =
        stale_guide_if_required(bound, false, &mut edits, &mut effects)?;
    let updated_at = max(*record.updated_at(), stamp.at);
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

    let candidate_bytes = apply_edits(bound.source_bytes(), edits)?;
    let parsed = parse_guide_source(candidate_bytes).map_err(map_parse)?;
    let candidate = bind_guide(parsed).map_err(|_| candidate_mismatch(MutationField::Document))?;
    let actual = candidate.record();
    let expected_archived_at = record.archive().map(|archive| *archive.archived_at());
    if actual.id() != record.id()
        || actual.title() != record.title()
        || actual.summary() != record.summary()
        || actual.category_id() != record.category_id()
        || actual.tags() != record.tags()
        || actual.aliases() != record.aliases()
        || actual.declared_status() != expected_status
        || actual.created_at() != record.created_at()
        || actual.updated_at() != &updated_at
        || actual.body() != record.body()
        || !same_verification(actual, record)
        || !same_archive(actual, expected_archived_from, expected_archived_at)
        || !same_nested_except_attachments(actual, record)
        || !attachment_was_appended(actual, record, &attachment)
        || !same_unknown_root(&candidate, bound)
        || candidate.unknown_partition().applicability()
            != bound.unknown_partition().applicability()
        || candidate.unknown_partition().references() != bound.unknown_partition().references()
        || candidate.unknown_partition().attachments().len()
            != bound.unknown_partition().attachments().len() + 1
        || candidate.unknown_partition().attachments()[..existing.len()]
            != bound.unknown_partition().attachments()[..]
        || !candidate.unknown_partition().attachments()[existing.len()].is_empty()
    {
        return Err(candidate_mismatch(MutationField::Document));
    }
    Ok(MutationOutcome::Candidate(MutationCandidate {
        bound: candidate,
        effects: effect_set(&effects)?,
    }))
}

fn emit_attachment_item(
    attachment: &AttachmentInput,
    newline: &[u8],
) -> Result<Box<[u8]>, MutationFailure> {
    let path = emit_yaml_string(&attachment.path)
        .map_err(|error| map_emit(error, MutationField::Attachments))?;
    let title = emit_yaml_string(&attachment.title)
        .map_err(|error| map_emit(error, MutationField::Attachments))?;
    let sha256 = emit_yaml_string(&attachment.sha256)
        .map_err(|error| map_emit(error, MutationField::Attachments))?;
    let text = format!(
        "  - id: {}{nl}    path: {}{nl}    title: {}{nl}    sha256: {}{nl}    size_bytes: {}",
        attachment.id,
        String::from_utf8_lossy(&path),
        String::from_utf8_lossy(&title),
        String::from_utf8_lossy(&sha256),
        attachment.size_bytes,
        nl = core::str::from_utf8(newline).unwrap_or("\n"),
    );
    if text.len() > 2_048 {
        return Err(failure(
            MutationFailureKind::ResourceLimit,
            MutationField::Attachments,
        ));
    }
    Ok(text.into_bytes().into_boxed_slice())
}

fn append_attachment_entry(
    bound: &BoundGuide,
    item: Box<[u8]>,
    newline: &[u8],
    edits: &mut Vec<Edit>,
) -> Result<(), MutationFailure> {
    let artifact = bound.parsed().yaml();
    let source = bound.source_bytes();
    let entry = artifact.entries().iter().find(|entry| {
        if entry.parent() != artifact.root_id() {
            return false;
        }
        let ReconciledEntryKind::Mapping { key, .. } = entry.kind() else {
            return false;
        };
        artifact.node(key).and_then(|node| node.decoded_scalar()) == Some("attachments")
    });
    let Some(entry) = entry else {
        let mut raw = Vec::new();
        raw.try_reserve_exact(13 + newline.len() + item.len() + newline.len())
            .map_err(|_| {
                failure(
                    MutationFailureKind::ResourceLimit,
                    MutationField::Attachments,
                )
            })?;
        raw.extend_from_slice(b"attachments:");
        raw.extend_from_slice(newline);
        raw.extend_from_slice(&item);
        raw.extend_from_slice(newline);
        return insert_raw_root_entries_before(
            artifact,
            source,
            "created_at",
            raw.into_boxed_slice(),
            MutationEffect::Attachments,
            MutationField::Attachments,
            edits,
        );
    };
    let ReconciledEntryKind::Mapping { value, .. } = entry.kind() else {
        return Err(failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        ));
    };
    let node = artifact.node(value).ok_or_else(|| {
        failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        )
    })?;
    let NodePresentation::Concrete {
        whole_file_range,
        tag_file_range,
        ..
    } = node.presentation()
    else {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::Attachments,
        ));
    };
    let start = whole_file_range.start().get() as usize;
    let end = whole_file_range.end().get() as usize;
    let mut replacement = Vec::new();
    match node.collection_style() {
        Some(CollectionPresentationStyle::Flow)
            if node.entries().is_empty()
                && tag_file_range.is_none()
                && source.get(start..end) == Some(b"[]") =>
        {
            replacement.extend_from_slice(newline);
            replacement.extend_from_slice(&item);
            replacement.extend_from_slice(newline);
        }
        Some(CollectionPresentationStyle::Block) => {
            replacement.extend_from_slice(source.get(start..end).ok_or_else(|| {
                failure(
                    MutationFailureKind::PresentationUnsafe,
                    MutationField::Attachments,
                )
            })?);
            if !replacement.ends_with(b"\n") {
                replacement.extend_from_slice(newline);
            }
            replacement.extend_from_slice(&item);
            replacement.extend_from_slice(newline);
        }
        _ => {
            return Err(failure(
                MutationFailureKind::PresentationUnsafe,
                MutationField::Attachments,
            ));
        }
    }
    push_edit(
        source,
        start,
        end,
        replacement.into_boxed_slice(),
        MutationEffect::Attachments,
        MutationField::Attachments,
        edits,
    )
}

fn same_nested_except_attachments(left: &super::Guide, right: &super::Guide) -> bool {
    same_applicability(left, right)
        && left.prerequisites() == right.prerequisites()
        && left.warnings() == right.warnings()
        && left.files_affected() == right.files_affected()
        && left.related_guide_ids() == right.related_guide_ids()
        && same_references(left, right)
}

fn attachment_was_appended(
    actual: &super::Guide,
    previous: &super::Guide,
    expected: &AttachmentInput,
) -> bool {
    let actual = actual.attachments().unwrap_or_default();
    let previous = previous.attachments().unwrap_or_default();
    actual.len() == previous.len() + 1
        && actual[..previous.len()]
            .iter()
            .zip(previous)
            .all(|(left, right)| {
                left.id() == right.id()
                    && left.path() == right.path()
                    && left.title() == right.title()
                    && left.media_type() == right.media_type()
                    && left.sha256() == right.sha256()
                    && left.size_bytes() == right.size_bytes()
            })
        && actual.last().is_some_and(|value| {
            value.id() == &expected.id
                && value.path() == expected.path
                && value.title() == expected.title
                && value.media_type().is_none()
                && value.sha256() == expected.sha256
                && value.size_bytes() == Some(expected.size_bytes)
        })
}

fn plan_update_attachment_title(
    bound: &BoundGuide,
    attachment_id: AttachmentId,
    title: String,
    stamp: &MutationStamp,
) -> Result<MutationOutcome<BoundGuide>, MutationFailure> {
    let record = bound.record();
    if record.declared_status() == GuideStatus::Archived {
        return Err(failure(
            MutationFailureKind::InvalidTransition,
            MutationField::Attachments,
        ));
    }
    if !super::valid_text(&title, 200) {
        return Err(failure(
            MutationFailureKind::InvalidValue,
            MutationField::Attachments,
        ));
    }
    let existing = record.attachments().unwrap_or_default();
    let index = existing
        .iter()
        .position(|attachment| attachment.id() == &attachment_id)
        .ok_or_else(|| {
            failure(
                MutationFailureKind::InvalidValue,
                MutationField::Attachments,
            )
        })?;
    if existing[index].title() == title {
        return Ok(MutationOutcome::NoOp);
    }
    require_proven(
        bound.parsed().yaml().round_trip_status(),
        MutationField::Attachments,
    )?;

    let mut edits = Vec::new();
    reserve(&mut edits, 4, MutationField::Document)?;
    replace_attachment_mapping_value(
        bound,
        index,
        existing.len(),
        "title",
        emit_yaml_string(&title).map_err(|error| map_emit(error, MutationField::Attachments))?,
        &mut edits,
    )?;
    let mut effects = Vec::new();
    reserve(&mut effects, 4, MutationField::Document)?;
    effects.push(MutationEffect::Attachments);
    let (expected_status, expected_archived_from) =
        stale_guide_if_required(bound, false, &mut edits, &mut effects)?;
    let updated_at = max(*record.updated_at(), stamp.at);
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

    let candidate_bytes = apply_edits(bound.source_bytes(), edits)?;
    let parsed = parse_guide_source(candidate_bytes).map_err(map_parse)?;
    let candidate = bind_guide(parsed).map_err(|_| candidate_mismatch(MutationField::Document))?;
    let actual = candidate.record();
    let expected_archived_at = record.archive().map(|archive| *archive.archived_at());
    if actual.id() != record.id()
        || actual.title() != record.title()
        || actual.summary() != record.summary()
        || actual.category_id() != record.category_id()
        || actual.tags() != record.tags()
        || actual.aliases() != record.aliases()
        || actual.declared_status() != expected_status
        || actual.created_at() != record.created_at()
        || actual.updated_at() != &updated_at
        || actual.body() != record.body()
        || !same_verification(actual, record)
        || !same_archive(actual, expected_archived_from, expected_archived_at)
        || !same_nested_except_attachments(actual, record)
        || !attachment_title_was_updated(actual, record, index, &title)
        || !same_unknown_root(&candidate, bound)
        || candidate.unknown_partition().applicability()
            != bound.unknown_partition().applicability()
        || candidate.unknown_partition().references() != bound.unknown_partition().references()
        || candidate.unknown_partition().attachments() != bound.unknown_partition().attachments()
    {
        return Err(candidate_mismatch(MutationField::Document));
    }
    Ok(MutationOutcome::Candidate(MutationCandidate {
        bound: candidate,
        effects: effect_set(&effects)?,
    }))
}

fn replace_attachment_mapping_value(
    bound: &BoundGuide,
    index: usize,
    count: usize,
    key_name: &str,
    replacement: Box<[u8]>,
    edits: &mut Vec<Edit>,
) -> Result<(), MutationFailure> {
    let artifact = bound.parsed().yaml();
    let source = bound.source_bytes();
    let root_entry = artifact
        .entries()
        .iter()
        .find(|entry| {
            if entry.parent() != artifact.root_id() {
                return false;
            }
            let ReconciledEntryKind::Mapping { key, .. } = entry.kind() else {
                return false;
            };
            artifact.node(key).and_then(|node| node.decoded_scalar()) == Some("attachments")
        })
        .ok_or_else(|| {
            failure(
                MutationFailureKind::BindingInvariant,
                MutationField::Attachments,
            )
        })?;
    let ReconciledEntryKind::Mapping { value, .. } = root_entry.kind() else {
        return Err(failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        ));
    };
    let sequence = artifact.node(value).ok_or_else(|| {
        failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        )
    })?;
    if sequence.collection_style() != Some(CollectionPresentationStyle::Block)
        || sequence.entries().len() != count
    {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::Attachments,
        ));
    }
    let item_entry = sequence
        .entries()
        .get(index)
        .and_then(|entry_id| artifact.entry(*entry_id))
        .ok_or_else(|| {
            failure(
                MutationFailureKind::BindingInvariant,
                MutationField::Attachments,
            )
        })?;
    let ReconciledEntryKind::Sequence { value } = item_entry.kind() else {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::Attachments,
        ));
    };
    let mapping = artifact.node(value).ok_or_else(|| {
        failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        )
    })?;
    if mapping.collection_style() != Some(CollectionPresentationStyle::Block) {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::Attachments,
        ));
    }
    let entry = mapping
        .entries()
        .iter()
        .filter_map(|entry_id| artifact.entry(*entry_id))
        .find(|entry| {
            let ReconciledEntryKind::Mapping { key, .. } = entry.kind() else {
                return false;
            };
            artifact.node(key).and_then(|node| node.decoded_scalar()) == Some(key_name)
        })
        .ok_or_else(|| {
            failure(
                MutationFailureKind::BindingInvariant,
                MutationField::Attachments,
            )
        })?;
    let ReconciledEntryKind::Mapping { value, .. } = entry.kind() else {
        return Err(failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        ));
    };
    let node = artifact.node(value).ok_or_else(|| {
        failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        )
    })?;
    let NodePresentation::Concrete {
        whole_file_range, ..
    } = node.presentation()
    else {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::Attachments,
        ));
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
        return Err(failure(
            MutationFailureKind::CommentUnsafe,
            MutationField::Attachments,
        ));
    }
    push_edit(
        source,
        start,
        end,
        replacement,
        MutationEffect::Attachments,
        MutationField::Attachments,
        edits,
    )
}

fn attachment_title_was_updated(
    actual: &super::Guide,
    previous: &super::Guide,
    changed: usize,
    expected_title: &str,
) -> bool {
    let actual = actual.attachments().unwrap_or_default();
    let previous = previous.attachments().unwrap_or_default();
    actual.len() == previous.len()
        && actual
            .iter()
            .zip(previous)
            .enumerate()
            .all(|(index, (left, right))| {
                left.id() == right.id()
                    && left.path() == right.path()
                    && left.title()
                        == if index == changed {
                            expected_title
                        } else {
                            right.title()
                        }
                    && left.media_type() == right.media_type()
                    && left.sha256() == right.sha256()
                    && left.size_bytes() == right.size_bytes()
            })
}

fn plan_remove_attachment(
    bound: &BoundGuide,
    attachment_id: AttachmentId,
    stamp: &MutationStamp,
) -> Result<MutationOutcome<BoundGuide>, MutationFailure> {
    let record = bound.record();
    if record.declared_status() == GuideStatus::Archived {
        return Err(failure(
            MutationFailureKind::InvalidTransition,
            MutationField::Attachments,
        ));
    }
    let existing = record.attachments().unwrap_or_default();
    let index = existing
        .iter()
        .position(|attachment| attachment.id() == &attachment_id)
        .ok_or_else(|| {
            failure(
                MutationFailureKind::InvalidValue,
                MutationField::Attachments,
            )
        })?;
    require_proven(
        bound.parsed().yaml().round_trip_status(),
        MutationField::Attachments,
    )?;

    let mut edits = Vec::new();
    reserve(&mut edits, 4, MutationField::Document)?;
    remove_attachment_entry(bound, index, existing.len(), &mut edits)?;
    let mut effects = Vec::new();
    reserve(&mut effects, 4, MutationField::Document)?;
    effects.push(MutationEffect::Attachments);
    let (expected_status, expected_archived_from) =
        stale_guide_if_required(bound, false, &mut edits, &mut effects)?;
    let updated_at = max(*record.updated_at(), stamp.at);
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

    let candidate_bytes = apply_edits(bound.source_bytes(), edits)?;
    let parsed = parse_guide_source(candidate_bytes).map_err(map_parse)?;
    let candidate = bind_guide(parsed).map_err(|_| candidate_mismatch(MutationField::Document))?;
    let actual = candidate.record();
    let expected_archived_at = record.archive().map(|archive| *archive.archived_at());
    if actual.id() != record.id()
        || actual.title() != record.title()
        || actual.summary() != record.summary()
        || actual.category_id() != record.category_id()
        || actual.tags() != record.tags()
        || actual.aliases() != record.aliases()
        || actual.declared_status() != expected_status
        || actual.created_at() != record.created_at()
        || actual.updated_at() != &updated_at
        || actual.body() != record.body()
        || !same_verification(actual, record)
        || !same_archive(actual, expected_archived_from, expected_archived_at)
        || !same_nested_except_attachments(actual, record)
        || !attachment_was_removed(actual, record, index)
        || !same_unknown_root(&candidate, bound)
        || candidate.unknown_partition().applicability()
            != bound.unknown_partition().applicability()
        || candidate.unknown_partition().references() != bound.unknown_partition().references()
        || !attachment_unknowns_removed_at(&candidate, bound, index)
    {
        return Err(candidate_mismatch(MutationField::Document));
    }
    Ok(MutationOutcome::Candidate(MutationCandidate {
        bound: candidate,
        effects: effect_set(&effects)?,
    }))
}

fn remove_attachment_entry(
    bound: &BoundGuide,
    index: usize,
    count: usize,
    edits: &mut Vec<Edit>,
) -> Result<(), MutationFailure> {
    let artifact = bound.parsed().yaml();
    let source = bound.source_bytes();
    if count == 1 {
        return remove_root_entry(
            artifact,
            source,
            "attachments",
            MutationEffect::Attachments,
            MutationField::Attachments,
            edits,
        );
    }
    let root_entry = artifact
        .entries()
        .iter()
        .find(|entry| {
            if entry.parent() != artifact.root_id() {
                return false;
            }
            let ReconciledEntryKind::Mapping { key, .. } = entry.kind() else {
                return false;
            };
            artifact.node(key).and_then(|node| node.decoded_scalar()) == Some("attachments")
        })
        .ok_or_else(|| {
            failure(
                MutationFailureKind::BindingInvariant,
                MutationField::Attachments,
            )
        })?;
    let ReconciledEntryKind::Mapping { value, .. } = root_entry.kind() else {
        return Err(failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        ));
    };
    let sequence = artifact.node(value).ok_or_else(|| {
        failure(
            MutationFailureKind::BindingInvariant,
            MutationField::Attachments,
        )
    })?;
    if sequence.collection_style() != Some(CollectionPresentationStyle::Block)
        || sequence.entries().len() != count
    {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::Attachments,
        ));
    }
    let entry = sequence
        .entries()
        .get(index)
        .and_then(|entry_id| artifact.entry(*entry_id))
        .ok_or_else(|| {
            failure(
                MutationFailureKind::BindingInvariant,
                MutationField::Attachments,
            )
        })?;
    if entry.collection_style() != CollectionPresentationStyle::Block
        || !matches!(entry.kind(), ReconciledEntryKind::Sequence { .. })
    {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::Attachments,
        ));
    }
    let entry_start = entry.file_range().start().get() as usize;
    let entry_end = entry.file_range().end().get() as usize;
    let start = source[..entry_start]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |offset| offset + 1);
    if entry_start > entry_end
        || entry_end > source.len()
        || source[start..entry_start]
            .iter()
            .any(|byte| !matches!(byte, b' ' | b'\t'))
    {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::Attachments,
        ));
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
        || artifact.comments().iter().any(|comment| {
            let range = comment.file_range();
            let overlaps = ranges_overlap(
                start,
                end,
                range.start().get() as usize,
                range.end().get() as usize,
            );
            let owned = match comment.owner() {
                CommentOwner::Entry(owner) => owner == entry.id(),
                CommentOwner::Node(owner) => match entry.kind() {
                    ReconciledEntryKind::Sequence { value } => owner == value,
                    ReconciledEntryKind::Mapping { .. } => false,
                },
                CommentOwner::Boundary { before, after, .. } => {
                    before == Some(entry.id()) || after == Some(entry.id())
                }
                CommentOwner::Ambiguous => true,
            };
            overlaps || owned
        })
    {
        return Err(failure(
            MutationFailureKind::CommentUnsafe,
            MutationField::Attachments,
        ));
    }
    push_edit(
        source,
        start,
        end,
        Box::<[u8]>::default(),
        MutationEffect::Attachments,
        MutationField::Attachments,
        edits,
    )
}

fn attachment_was_removed(actual: &super::Guide, previous: &super::Guide, removed: usize) -> bool {
    let actual = actual.attachments().unwrap_or_default();
    let previous = previous.attachments().unwrap_or_default();
    actual.len() + 1 == previous.len()
        && actual.iter().enumerate().all(|(index, left)| {
            let right = &previous[if index < removed { index } else { index + 1 }];
            left.id() == right.id()
                && left.path() == right.path()
                && left.title() == right.title()
                && left.media_type() == right.media_type()
                && left.sha256() == right.sha256()
                && left.size_bytes() == right.size_bytes()
        })
}

fn attachment_unknowns_removed_at(left: &BoundGuide, right: &BoundGuide, removed: usize) -> bool {
    let left = left.unknown_partition().attachments();
    let right = right.unknown_partition().attachments();
    left.len() + 1 == right.len()
        && left
            .iter()
            .enumerate()
            .all(|(index, value)| value == &right[if index < removed { index } else { index + 1 }])
}

#[derive(Clone, Copy)]
enum GuideLifecycleEdit {
    None,
    Archive(RestorableGuideStatus),
    Restore(GuideStatus),
    Tested(VerificationHash),
}

fn same_expected_verification(
    guide: &super::Guide,
    expected: Option<&(Timestamp, Option<VerificationHash>)>,
) -> bool {
    match (guide.verification(), expected) {
        (None, None) => true,
        (Some(actual), Some((at, hash))) => {
            actual.last_verified_at() == at && actual.verified_content_hash().copied() == *hash
        }
        _ => false,
    }
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
    guide: &super::Guide,
    expected_from: Option<RestorableGuideStatus>,
    expected_at: Option<Timestamp>,
) -> bool {
    match (guide.archive(), expected_from, expected_at) {
        (None, None, None) => true,
        (Some(archive), Some(expected_from), Some(expected_at)) => {
            archive.archived_from_status() == expected_from && archive.archived_at() == &expected_at
        }
        _ => false,
    }
}

fn same_unknown_root(left: &BoundGuide, right: &BoundGuide) -> bool {
    let left_entries = left.parsed().yaml().semantic().entries();
    let right_entries = right.parsed().yaml().semantic().entries();
    let left_unknown = left.unknown_partition().root();
    let right_unknown = right.unknown_partition().root();
    left_unknown.len() == right_unknown.len()
        && left_unknown
            .iter()
            .zip(right_unknown)
            .all(|(left_index, right_index)| {
                let Some(left_entry) = left_entries.get(*left_index as usize) else {
                    return false;
                };
                let Some(right_entry) = right_entries.get(*right_index as usize) else {
                    return false;
                };
                left_entry.key() == right_entry.key()
                    && same_unknown_value(left_entry.value(), right_entry.value())
            })
}

fn same_unknown_value(
    left: &crate::domain::UnknownYamlValue,
    right: &crate::domain::UnknownYamlValue,
) -> bool {
    use crate::domain::UnknownYamlScalarKind;

    match (
        left.as_scalar(),
        right.as_scalar(),
        left.as_sequence(),
        right.as_sequence(),
        left.as_mapping(),
        right.as_mapping(),
    ) {
        (Some(left), Some(right), _, _, _, _) if left.kind() == right.kind() => match left.kind() {
            UnknownYamlScalarKind::Null => true,
            UnknownYamlScalarKind::Boolean => left.as_bool() == right.as_bool(),
            UnknownYamlScalarKind::Integer => left.as_integer() == right.as_integer(),
            UnknownYamlScalarKind::Float => left.as_float() == right.as_float(),
            UnknownYamlScalarKind::String => left.as_str() == right.as_str(),
        },
        (_, _, Some(left), Some(right), _, _) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| same_unknown_value(left, right))
        }
        (_, _, _, _, Some(left), Some(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    same_unknown_value(left.key(), right.key())
                        && same_unknown_value(left.value(), right.value())
                })
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

fn validate_string_list(
    values: &[String],
    max_items: usize,
    max_scalars: usize,
    field: MutationField,
) -> Result<(), MutationFailure> {
    if values.len() > max_items
        || values
            .iter()
            .any(|value| !super::valid_text(value, max_scalars))
    {
        return Err(failure(MutationFailureKind::InvalidValue, field));
    }
    for (index, value) in values.iter().enumerate() {
        if values[index + 1..]
            .iter()
            .any(|other| CaseFold::Full.case_eq(value, other))
        {
            return Err(failure(MutationFailureKind::InvalidValue, field));
        }
    }
    Ok(())
}

struct Edit {
    start: usize,
    end: usize,
    replacement: Box<[u8]>,
    effect: MutationEffect,
}

#[allow(clippy::too_many_arguments)]
fn replace_root_string_sequence(
    artifact: &ParsedYamlArtifact,
    source: &[u8],
    key_name: &str,
    values: &[String],
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
    let node = artifact
        .node(value)
        .ok_or_else(|| failure(MutationFailureKind::BindingInvariant, field))?;
    let NodePresentation::Concrete {
        whole_file_range,
        tag_file_range,
        ..
    } = node.presentation()
    else {
        return Err(failure(MutationFailureKind::PresentationUnsafe, field));
    };
    match node.collection_style() {
        Some(CollectionPresentationStyle::Block) => {}
        Some(CollectionPresentationStyle::Flow)
            if node.entries().is_empty()
                && tag_file_range.is_none()
                && source.get(
                    whole_file_range.start().get() as usize..whole_file_range.end().get() as usize,
                ) == Some(b"[]") => {}
        _ => return Err(failure(MutationFailureKind::PresentationUnsafe, field)),
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

    let newline: &[u8] = source
        .iter()
        .position(|byte| *byte == b'\n')
        .filter(|offset| *offset > 0 && source[*offset - 1] == b'\r')
        .map_or(b"\n", |_| b"\r\n");
    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(key_name.len() + 4)
        .map_err(|_| failure(MutationFailureKind::ResourceLimit, field))?;
    replacement.extend_from_slice(key_name.as_bytes());
    replacement.push(b':');
    if values.is_empty() {
        replacement.extend_from_slice(b" []");
        replacement.extend_from_slice(newline);
    } else {
        replacement.extend_from_slice(newline);
        for value in values {
            let encoded = emit_yaml_string(value).map_err(|error| map_emit(error, field))?;
            replacement
                .try_reserve(4 + encoded.len() + newline.len())
                .map_err(|_| failure(MutationFailureKind::ResourceLimit, field))?;
            replacement.extend_from_slice(b"  - ");
            replacement.extend_from_slice(&encoded);
            replacement.extend_from_slice(newline);
        }
    }
    push_edit(
        source,
        start,
        end,
        replacement.into_boxed_slice(),
        effect,
        field,
        edits,
    )
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

fn insert_archive_metadata_before(
    artifact: &ParsedYamlArtifact,
    source: &[u8],
    archived_from: RestorableGuideStatus,
    archived_at: Timestamp,
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
            artifact.node(key).and_then(|node| node.decoded_scalar()) == Some("created_at")
        })
        .ok_or_else(|| {
            failure(
                MutationFailureKind::BindingInvariant,
                MutationField::ArchivedAt,
            )
        })?;
    if anchor.collection_style() != CollectionPresentationStyle::Block {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::ArchivedAt,
        ));
    }
    let start = anchor.file_range().start().get() as usize;
    if start > source.len() || (start != 0 && source.get(start.wrapping_sub(1)) != Some(&b'\n')) {
        return Err(failure(
            MutationFailureKind::PresentationUnsafe,
            MutationField::ArchivedAt,
        ));
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
        return Err(failure(
            MutationFailureKind::CommentUnsafe,
            MutationField::ArchivedAt,
        ));
    }
    let archived_from = match archived_from {
        RestorableGuideStatus::Draft => "draft",
        RestorableGuideStatus::Tested => "tested",
        RestorableGuideStatus::Stale => "stale",
    };
    let timestamp = emit_yaml_display(archived_at)
        .map_err(|error| map_emit(error, MutationField::ArchivedAt))?;
    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(55 + archived_from.len() + timestamp.len())
        .map_err(|_| {
            failure(
                MutationFailureKind::ResourceLimit,
                MutationField::ArchivedAt,
            )
        })?;
    replacement.extend_from_slice(b"archived_from_status: ");
    replacement.extend_from_slice(archived_from.as_bytes());
    replacement.extend_from_slice(b"\narchived_at: ");
    replacement.extend_from_slice(&timestamp);
    replacement.push(b'\n');
    push_edit(
        source,
        start,
        start,
        replacement.into_boxed_slice(),
        MutationEffect::ArchivedAt,
        MutationField::ArchivedAt,
        edits,
    )
}

fn insert_verification_metadata_before(
    artifact: &ParsedYamlArtifact,
    source: &[u8],
    verified_at: Timestamp,
    hash: &VerificationHash,
    edits: &mut Vec<Edit>,
) -> Result<(), MutationFailure> {
    let timestamp = emit_yaml_display(verified_at)
        .map_err(|error| map_emit(error, MutationField::LastVerifiedAt))?;
    let hash = emit_yaml_display(hash)
        .map_err(|error| map_emit(error, MutationField::VerifiedContentHash))?;
    let newline: &[u8] = source
        .iter()
        .position(|byte| *byte == b'\n')
        .filter(|offset| *offset > 0 && source[*offset - 1] == b'\r')
        .map_or(b"\n", |_| b"\r\n");
    let mut value = Vec::new();
    value
        .try_reserve_exact(47 + timestamp.len() + hash.len() + newline.len() * 2)
        .map_err(|_| {
            failure(
                MutationFailureKind::ResourceLimit,
                MutationField::VerifiedContentHash,
            )
        })?;
    value.extend_from_slice(b"last_verified_at: ");
    value.extend_from_slice(&timestamp);
    value.extend_from_slice(newline);
    value.extend_from_slice(b"verified_content_hash: ");
    value.extend_from_slice(&hash);
    value.extend_from_slice(newline);
    insert_raw_root_entries_before(
        artifact,
        source,
        "created_at",
        value.into_boxed_slice(),
        MutationEffect::VerifiedContentHash,
        MutationField::VerifiedContentHash,
        edits,
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_raw_root_entries_before(
    artifact: &ParsedYamlArtifact,
    source: &[u8],
    anchor_name: &str,
    replacement: Box<[u8]>,
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
    if start > source.len() || (start != 0 && source.get(start - 1) != Some(&b'\n')) {
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
    push_edit(source, start, start, replacement, effect, field, edits)
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
    fn separator_less_layouts_survive_edits_without_inserted_blank_lines() {
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        let front = guide_base("draft", "");
        for (layout, body) in [("---\n", "# Body\n"), ("---\n", ""), ("---", "")] {
            let source = format!("---\n{front}{layout}{body}");
            let bound = bind_guide(
                parse_guide_source(source.clone().into_bytes().into_boxed_slice()).unwrap(),
            )
            .unwrap_or_else(|_| panic!("bind"));
            assert_eq!(bound.record().body(), body.as_bytes());
            let hash = super::super::verification::verification_hash(&bound).unwrap();
            let blank = format!("---\n{front}---\n\n{body}");
            let blank_bound =
                bind_guide(parse_guide_source(blank.into_bytes().into_boxed_slice()).unwrap())
                    .unwrap_or_else(|_| panic!("bind"));
            assert_eq!(
                super::super::verification::verification_hash(&blank_bound).unwrap(),
                hash
            );

            let MutationOutcome::Candidate(titled) =
                plan_guide_mutation(&bound, GuideMutation::SetTitle("New title".into()), &stamp)
                    .unwrap_or_else(|_| panic!("title"))
            else {
                panic!("candidate")
            };
            let titled_source = core::str::from_utf8(titled.bound().source_bytes()).unwrap();
            assert!(titled_source.ends_with(&format!("{layout}{body}")));
            assert!(!titled_source.contains("---\n\n"));
            assert_eq!(titled.bound().record().body(), body.as_bytes());

            let MutationOutcome::Candidate(rebodied) = plan_guide_mutation(
                &bound,
                GuideMutation::ReplaceBody(b"New body\n".to_vec()),
                &stamp,
            )
            .unwrap_or_else(|_| panic!("body")) else {
                panic!("candidate")
            };
            assert_eq!(rebodied.bound().record().body(), b"New body\n");
            let rebodied_source = core::str::from_utf8(rebodied.bound().source_bytes()).unwrap();
            assert!(rebodied_source.ends_with("future: {nested: [1]}\n---\nNew body\n"));
            assert!(!rebodied_source.contains("---\n\n"));
        }
    }

    #[test]
    fn guide_archive_restore_follows_the_closed_status_matrix() {
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        for (status, extra, expected_from, restored) in [
            (
                "draft",
                "",
                RestorableGuideStatus::Draft,
                GuideStatus::Draft,
            ),
            (
                "stale",
                "",
                RestorableGuideStatus::Stale,
                GuideStatus::Stale,
            ),
            (
                "tested",
                "last_verified_at: 2026-09-05T10:00:00Z\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
                RestorableGuideStatus::Stale,
                GuideStatus::Stale,
            ),
        ] {
            let original = guide(&guide_base(status, extra), "Body\n");
            let MutationOutcome::Candidate(archived) =
                plan_guide_mutation(&original, GuideMutation::Archive, &stamp).unwrap_or_else(
                    |error| {
                        panic!(
                            "archive {status}: kind={} field={}",
                            error.kind() as u8,
                            error.field() as u8
                        )
                    },
                )
            else {
                panic!("archive candidate")
            };
            let archive = archived.bound().record().archive().unwrap();
            assert_eq!(
                archived.bound().record().declared_status(),
                GuideStatus::Archived
            );
            assert_eq!(archive.archived_from_status(), expected_from);
            assert_eq!(archive.archived_at(), &timestamp("2026-09-05T11:00:00Z"));
            assert_eq!(archived.bound().record().body(), b"Body\n");
            assert!(same_unknown_root(archived.bound(), &original));
            assert!(matches!(
                plan_guide_mutation(archived.bound(), GuideMutation::Archive, &stamp),
                Ok(MutationOutcome::NoOp)
            ));

            let MutationOutcome::Candidate(restored_candidate) = plan_guide_mutation(
                archived.bound(),
                GuideMutation::Restore,
                &MutationStamp::from_trusted_time(timestamp("2026-09-05T12:00:00Z")),
            )
            .unwrap_or_else(|_| panic!("restore {status}")) else {
                panic!("restore candidate")
            };
            assert_eq!(
                restored_candidate.bound().record().declared_status(),
                restored
            );
            assert!(restored_candidate.bound().record().archive().is_none());
            assert_eq!(restored_candidate.bound().record().body(), b"Body\n");
        }

        let draft = guide(&guide_base("draft", ""), "Body\n");
        let error = plan_guide_mutation(&draft, GuideMutation::Restore, &stamp)
            .err()
            .unwrap();
        assert!(matches!(
            error.kind(),
            MutationFailureKind::InvalidTransition
        ));
        assert!(matches!(error.field(), MutationField::Status));
    }

    #[test]
    fn guide_tags_and_aliases_are_bounded_unique_and_round_trip_safe() {
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        let original = guide(&guide_base("draft", ""), "Body\n");
        let MutationOutcome::Candidate(tagged) = plan_guide_mutation(
            &original,
            GuideMutation::ReplaceTags(vec!["linux".into(), "Wayland".into()]),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("tags")) else {
            panic!("tag candidate")
        };
        assert_eq!(tagged.bound().record().tags(), ["linux", "Wayland"]);
        let tagged_source = core::str::from_utf8(tagged.bound().source_bytes()).unwrap();
        assert!(tagged_source.contains("tags:\n  - \"linux\"\n  - \"Wayland\"\n"));
        assert!(tagged_source.contains("future: {nested: [1]}"));

        let MutationOutcome::Candidate(aliased) = plan_guide_mutation(
            tagged.bound(),
            GuideMutation::ReplaceAliases(vec!["manual install".into(), "tar app".into()]),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("aliases")) else {
            panic!("alias candidate")
        };
        assert_eq!(
            aliased.bound().record().aliases(),
            ["manual install", "tar app"]
        );
        assert_eq!(aliased.bound().record().tags(), ["linux", "Wayland"]);
        assert!(matches!(
            plan_guide_mutation(
                aliased.bound(),
                GuideMutation::ReplaceAliases(vec!["manual install".into(), "tar app".into()]),
                &stamp,
            ),
            Ok(MutationOutcome::NoOp)
        ));

        for values in [
            vec!["Maße".to_owned(), "MASSE".to_owned()],
            vec![" leading".to_owned()],
            vec!["x".repeat(65)],
            (0..65).map(|index| format!("tag-{index}")).collect(),
        ] {
            let error = plan_guide_mutation(&original, GuideMutation::ReplaceTags(values), &stamp)
                .err()
                .unwrap();
            assert!(matches!(error.kind(), MutationFailureKind::InvalidValue));
            assert!(matches!(error.field(), MutationField::Tags));
        }

        let flow = guide(
            &guide_base("draft", "").replacen("tags: []", "tags: [linux]", 1),
            "Body\n",
        );
        let error = plan_guide_mutation(
            &flow,
            GuideMutation::ReplaceTags(vec!["linux".into(), "wayland".into()]),
            &stamp,
        )
        .err()
        .unwrap();
        assert!(matches!(
            error.kind(),
            MutationFailureKind::PresentationUnsafe
        ));

        let commented = guide(
            &guide_base("draft", "").replacen(
                "tags: []",
                "tags:\n  - linux # keep why this exists",
                1,
            ),
            "Body\n",
        );
        let error = plan_guide_mutation(
            &commented,
            GuideMutation::ReplaceTags(vec!["wayland".into()]),
            &stamp,
        )
        .err()
        .unwrap();
        assert!(matches!(error.kind(), MutationFailureKind::CommentUnsafe));

        let decomposed = plan_guide_mutation(
            &original,
            GuideMutation::ReplaceTags(vec!["é".into(), "é".into()]),
            &stamp,
        );
        assert!(matches!(decomposed, Ok(MutationOutcome::Candidate(_))));

        let crlf_frontmatter = guide_base("draft", "").replace('\n', "\r\n");
        let crlf_source = format!("---\r\n{crlf_frontmatter}---\r\n\r\nBody\r\n");
        let crlf =
            bind_guide(parse_guide_source(crlf_source.into_bytes().into_boxed_slice()).unwrap())
                .unwrap_or_else(|_| panic!("bind crlf"));
        let MutationOutcome::Candidate(crlf_candidate) = plan_guide_mutation(
            &crlf,
            GuideMutation::ReplaceAliases(vec!["manual install".into()]),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("crlf aliases")) else {
            panic!("crlf candidate")
        };
        assert!(
            core::str::from_utf8(crlf_candidate.bound().source_bytes())
                .unwrap()
                .contains("aliases:\r\n  - \"manual install\"\r\n")
        );

        let tested_extra = "last_verified_at: 2026-09-05T10:00:00Z\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
        let tested = guide(&guide_base("tested", tested_extra), "Body\n");
        let MutationOutcome::Candidate(stale) = plan_guide_mutation(
            &tested,
            GuideMutation::ReplaceAliases(vec!["install tarball".into()]),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("tested alias")) else {
            panic!("stale candidate")
        };
        assert_eq!(stale.bound().record().declared_status(), GuideStatus::Stale);
        assert!(stale.bound().record().verification().is_some());
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

    #[test]
    fn attachment_removal_preserves_remaining_unknowns_and_stales_tested_guides() {
        let first = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509898").unwrap();
        let second = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509899").unwrap();
        let extra = format!(
            "attachments:\n  - id: {first}\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/first.txt\n    title: First\n    sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n    size_bytes: 5\n    future_attachment: remove-me\n  - id: {second}\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/second.txt\n    title: Second\n    sha256: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n    size_bytes: 6\n    future_attachment: keep-me\nlast_verified_at: 2026-09-05T10:00:00Z\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n"
        );
        let original = guide(&guide_base("tested", &extra), "Body\n");
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        let MutationOutcome::Candidate(candidate) =
            plan_guide_mutation(&original, GuideMutation::RemoveAttachment(first), &stamp)
                .unwrap_or_else(|error| {
                    panic!(
                        "remove first Attachment: {:?}/{:?}",
                        error.kind(),
                        error.field()
                    )
                })
        else {
            panic!("candidate")
        };
        let attachments = candidate.bound().record().attachments().unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].id(), &second);
        assert_eq!(
            candidate.bound().record().declared_status(),
            GuideStatus::Stale
        );
        let source = core::str::from_utf8(candidate.bound().source_bytes()).unwrap();
        assert!(!source.contains("future_attachment: remove-me"));
        assert!(source.contains("future_attachment: keep-me"));
        assert!(source.contains("future: {nested: [1]}"));

        let MutationOutcome::Candidate(empty) = plan_guide_mutation(
            candidate.bound(),
            GuideMutation::RemoveAttachment(second),
            &stamp,
        )
        .unwrap_or_else(|_| panic!("remove final Attachment")) else {
            panic!("candidate")
        };
        assert!(empty.bound().record().attachments().is_none());
        assert!(
            !core::str::from_utf8(empty.bound().source_bytes())
                .unwrap()
                .contains("attachments:")
        );
    }

    #[test]
    fn attachment_title_update_is_narrow_preserves_unknowns_and_stales_tested_guides() {
        let attachment_id = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509898").unwrap();
        let extra = format!(
            "attachments:\n  - id: {attachment_id}\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/file.txt\n    title: Old title\n    media_type: text/plain\n    sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n    size_bytes: 5\n    future_attachment: keep-me\nlast_verified_at: 2026-09-05T10:00:00Z\nverified_content_hash: verify:v1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n"
        );
        let original = guide(&guide_base("tested", &extra), "Body\n");
        let stamp = MutationStamp::from_trusted_time(timestamp("2026-09-05T11:00:00Z"));
        let MutationOutcome::Candidate(candidate) = plan_guide_mutation(
            &original,
            GuideMutation::UpdateAttachmentTitle {
                attachment_id,
                title: "Useful notes".into(),
            },
            &stamp,
        )
        .unwrap_or_else(|_| panic!("update Attachment title")) else {
            panic!("candidate")
        };
        let attachment = &candidate.bound().record().attachments().unwrap()[0];
        assert_eq!(attachment.title(), "Useful notes");
        assert_eq!(
            attachment.path(),
            original.record().attachments().unwrap()[0].path()
        );
        assert_eq!(
            attachment.sha256(),
            original.record().attachments().unwrap()[0].sha256()
        );
        assert_eq!(
            candidate.bound().record().declared_status(),
            GuideStatus::Stale
        );
        let source = core::str::from_utf8(candidate.bound().source_bytes()).unwrap();
        assert!(source.contains("future_attachment: keep-me"));
        assert!(source.contains("future: {nested: [1]}"));
        assert!(matches!(
            plan_guide_mutation(
                candidate.bound(),
                GuideMutation::UpdateAttachmentTitle {
                    attachment_id,
                    title: "Useful notes".into(),
                },
                &stamp,
            ),
            Ok(MutationOutcome::NoOp)
        ));
        assert!(
            plan_guide_mutation(
                &original,
                GuideMutation::UpdateAttachmentTitle {
                    attachment_id,
                    title: " leading".into(),
                },
                &stamp,
            )
            .is_err()
        );

        let missing = AttachmentId::from_str("0199a264-ef5f-73b4-a238-a71ba1509899").unwrap();
        assert!(
            plan_guide_mutation(
                &original,
                GuideMutation::UpdateAttachmentTitle {
                    attachment_id: missing,
                    title: "Missing".into(),
                },
                &stamp,
            )
            .is_err()
        );

        let archived_extra = format!(
            "attachments:\n  - id: {attachment_id}\n    path: attachments/0199a1d7-7b0d-7d8e-a38d-f936c5735e10/file.txt\n    title: Old title\n    sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\narchived_from_status: draft\narchived_at: 2026-09-05T10:00:00Z\n"
        );
        let archived = guide(&guide_base("archived", &archived_extra), "Body\n");
        assert!(
            plan_guide_mutation(
                &archived,
                GuideMutation::UpdateAttachmentTitle {
                    attachment_id,
                    title: "Blocked".into(),
                },
                &stamp,
            )
            .is_err()
        );
    }
}

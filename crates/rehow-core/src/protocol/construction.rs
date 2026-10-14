#![allow(dead_code)]
use super::*;

pub(super) struct ErrorConstructionError {
    _private: (),
}
macro_rules! catalog {($name:ident{$($variant:ident=>$wire:literal),+$(,)?})=>{pub(super) enum $name{$($variant),+}impl $name{fn wire(self)->&'static str{match self{$(Self::$variant=>$wire),+}}}}}
catalog!(Field{SchemaVersion=>"/schema_version",WorkspaceId=>"/workspace_id",Name=>"/name",CreatedAt=>"/created_at",UpdatedAt=>"/updated_at",Settings=>"/settings",DefaultCategoryId=>"/settings/default_category_id",Id=>"/id",Title=>"/title",Summary=>"/summary",Order=>"/order",Status=>"/status",CategoryId=>"/category_id",Tags=>"/tags",Aliases=>"/aliases",LastVerifiedAt=>"/last_verified_at",VerifiedContentHash=>"/verified_content_hash",ArchivedFromStatus=>"/archived_from_status",ArchivedAt=>"/archived_at",AppliesTo=>"/applies_to",Prerequisites=>"/prerequisites",Warnings=>"/warnings",FilesAffected=>"/files_affected",RelatedGuideIds=>"/related_guide_ids",References=>"/references",Attachments=>"/attachments",Version=>"/version",Code=>"/code",Message=>"/message",Retryable=>"/retryable",RequestId=>"/requestId",Details=>"/details",Recovery=>"/recovery",ExpectedRevision=>"/expectedRevision",ExpectedSnapshot=>"/expectedSnapshot",PlanToken=>"/planToken",Operation=>"/operation",Limit=>"/limit",Cursor=>"/cursor"});
catalog!(Reason{InvalidFormat=>"invalid_format",Required=>"required",OutOfRange=>"out_of_range",UnsupportedValue=>"unsupported_value",CapabilityMissing=>"capability_missing",CapabilityExpired=>"capability_expired",WorkspaceRootMissing=>"workspace_root_missing",WorkspaceRootChanged=>"workspace_root_changed",WorkspaceNotWritable=>"workspace_not_writable",PathAbsolute=>"path_absolute",PathParentComponent=>"path_parent_component",PathInvalidComponent=>"path_invalid_component",PathOutsideWorkspace=>"path_outside_workspace",RoundTripUnproven=>"round_trip_unproven",PresentationMismatch=>"presentation_mismatch",CommentOwnershipAmbiguous=>"comment_ownership_ambiguous",PlanExpired=>"plan_expired",PlanReplayed=>"plan_replayed",PlanStateChanged=>"plan_state_changed",PlanScopeMismatch=>"plan_scope_mismatch",RepositoryOwnerUntrusted=>"repository_owner_untrusted",RepositoryLayoutUnsupported=>"repository_layout_unsupported",JournalInvalid=>"journal_invalid",JournalStateAmbiguous=>"journal_state_ambiguous",ExternalChangeDetected=>"external_change_detected"});
catalog!(Diagnostic{MissingRequiredField=>"missing_required_field",InvalidFieldType=>"invalid_field_type",InvalidFieldValue=>"invalid_field_value",DuplicateField=>"duplicate_field",YamlSyntax=>"yaml_syntax",YamlDepthLimit=>"yaml_depth_limit",YamlMappingEntryLimit=>"yaml_mapping_entry_limit",YamlSequenceEntryLimit=>"yaml_sequence_entry_limit",YamlNodeLimit=>"yaml_node_limit",YamlScalarLimit=>"yaml_scalar_limit",YamlNumericWorkLimit=>"yaml_numeric_work_limit",YamlSemanticWorkLimit=>"yaml_semantic_work_limit",YamlDuplicateKey=>"yaml_duplicate_key",YamlUnsupportedConstruct=>"yaml_unsupported_construct",YamlParserDisagreement=>"yaml_parser_disagreement",InvalidFrontmatterEnvelope=>"invalid_frontmatter_envelope",InvalidUtf8=>"invalid_utf8"});
catalog!(Operation{WorkspaceOpen=>"workspace_open",WorkspaceInitialize=>"workspace_initialize",WorkspaceMigrate=>"workspace_migrate",WorkspaceRecover=>"workspace_recover",CategoryCreate=>"category_create",CategoryUpdate=>"category_update",CategoryReorder=>"category_reorder",CategoryArchive=>"category_archive",CategoryRestore=>"category_restore",CategoryRemove=>"category_remove",GuideCreate=>"guide_create",GuideUpdate=>"guide_update",GuideChangeCategory=>"guide_change_category",GuideChangeStatus=>"guide_change_status",GuideArchive=>"guide_archive",GuideRestore=>"guide_restore",GuideDelete=>"guide_delete",AttachmentAdd=>"attachment_add",AttachmentRemove=>"attachment_remove",SearchRebuild=>"search_rebuild",GitCommit=>"git_commit",GitSync=>"git_sync"});
catalog!(GitOperation{Status=>"git_status",Init=>"git_init",Clone=>"git_clone",SetRemote=>"git_set_remote",Commit=>"git_commit",Sync=>"git_sync"});
fn field(v: Field) -> SchemaFieldPath {
    SchemaFieldPath(v.wire().into())
}
fn reason(v: Reason) -> ReasonCode {
    ReasonCode(v.wire().into())
}
fn diagnostic(v: Diagnostic) -> DiagnosticCode {
    DiagnosticCode(v.wire().into())
}
fn operation(v: Operation) -> OperationCode {
    OperationCode(v.wire().into())
}
fn git_operation(v: GitOperation) -> GitOperationCode {
    GitOperationCode(v.wire().into())
}
const COPY: [(&str, &str); 36] = [
    (
        "The request contains an invalid value.",
        "Correct the named field and try again.",
    ),
    (
        "The recent Workspace could not be found.",
        "Choose the Workspace again or remove it from recent Workspaces.",
    ),
    (
        "The Workspace is temporarily unavailable.",
        "Restore access to the Workspace and retry.",
    ),
    (
        "The Workspace is read-only for this operation.",
        "Reopen it with the required capability or choose a writable Workspace.",
    ),
    (
        "The Workspace exceeds a safety limit.",
        "Reduce the named scope before retrying the operation.",
    ),
    (
        "This Workspace schema version is not supported.",
        "Open it read-only or use a Rehow version that supports this schema.",
    ),
    (
        "The Workspace manifest is invalid.",
        "Correct the reported manifest field and inspect the Workspace again.",
    ),
    (
        "The Category descriptor is invalid.",
        "Correct the reported Category field and refresh the Workspace.",
    ),
    (
        "The Guide is invalid.",
        "Correct the reported Guide field and refresh the Workspace.",
    ),
    (
        "Rehow cannot prove that this edit preserves the document.",
        "Edit the file externally or simplify the reported field, then refresh.",
    ),
    (
        "More than one item uses the same identifier.",
        "Give each reported item a distinct identifier before retrying.",
    ),
    (
        "The Category could not be found.",
        "Refresh the Workspace and choose an existing Category.",
    ),
    (
        "The Guide could not be found.",
        "Refresh the Workspace and choose an existing Guide.",
    ),
    (
        "The Attachment could not be found.",
        "Refresh the Guide and choose an existing Attachment.",
    ),
    (
        "The item changed since it was opened.",
        "Read it again, compare changes, and retry with its new revision.",
    ),
    (
        "The Workspace changed after the operation was planned.",
        "Refresh the Workspace, review the changes, and create a new plan.",
    ),
    (
        "A managed path would leave the Workspace.",
        "Correct the path and keep it within the opened Workspace.",
    ),
    (
        "A managed path contains a symbolic link.",
        "Replace the symbolic link with a confined regular file and refresh.",
    ),
    (
        "A managed file has more than one hard link.",
        "Copy it to a new regular file with one link and refresh.",
    ),
    (
        "The operation was denied by filesystem permissions.",
        "Restore the required permission or choose another Workspace.",
    ),
    (
        "The file exceeds the safety limit.",
        "Reduce the file size before retrying.",
    ),
    (
        "The operation could not finish while the resource was busy.",
        "Wait for the other operation to finish and retry.",
    ),
    (
        "The search index is not ready.",
        "Wait for indexing to finish or rebuild the index.",
    ),
    (
        "The operation plan is stale.",
        "Refresh the Workspace and create a new plan.",
    ),
    (
        "This operation requires confirmation.",
        "Review the exact plan in the native confirmation dialog.",
    ),
    (
        "The confirmation is invalid or no longer usable.",
        "Refresh the Workspace and request a new confirmation.",
    ),
    (
        "Marking a Guide tested requires native user approval.",
        "Try the exact Guide revision and approve it in the native app.",
    ),
    (
        "Git is unavailable.",
        "Install or configure a trusted Git executable, then retry.",
    ),
    (
        "Rehow cannot use this Git repository safely.",
        "Correct the reported repository condition or use Git outside Rehow.",
    ),
    (
        "Git authentication is required.",
        "Restore the reported credential source and retry.",
    ),
    (
        "The local and remote Git histories have diverged.",
        "Resolve the divergence with Git tooling, then refresh Rehow.",
    ),
    (
        "The Git operation has conflicts.",
        "Resolve the reported conflicts with Git tooling, then refresh Rehow.",
    ),
    (
        "Git could not complete the operation.",
        "Correct the reported Git condition and retry.",
    ),
    (
        "The Workspace must be migrated before it can be changed.",
        "Review the migration plan and verified backup before continuing.",
    ),
    (
        "Rehow cannot safely complete or roll back an interrupted operation.",
        "Keep the Workspace read-only and follow the recovery guidance.",
    ),
    (
        "Rehow encountered an internal error.",
        "Retry once; if it happens again, report the incident identifier.",
    ),
];
fn make(request_id: RequestId, kind: ErrorKind) -> Result<ErrorDto, ErrorConstructionError> {
    let code = match &kind {
        ErrorKind::InvalidArgument(_) => 0,
        ErrorKind::WorkspaceNotFound(_) => 1,
        ErrorKind::WorkspaceUnavailable(_) => 2,
        ErrorKind::WorkspaceReadOnly(_) => 3,
        ErrorKind::WorkspaceLimitExceeded(_) => 4,
        ErrorKind::SchemaUnsupported(_) => 5,
        ErrorKind::InvalidManifest(_) => 6,
        ErrorKind::InvalidCategory(_) => 7,
        ErrorKind::InvalidGuide(_) => 8,
        ErrorKind::UnsafeRoundTrip(_) => 9,
        ErrorKind::DuplicateId(_) => 10,
        ErrorKind::CategoryNotFound(_) => 11,
        ErrorKind::GuideNotFound(_) => 12,
        ErrorKind::AttachmentNotFound(_) => 13,
        ErrorKind::RevisionConflict(_) => 14,
        ErrorKind::SnapshotConflict(_) => 15,
        ErrorKind::PathEscape(_) => 16,
        ErrorKind::SymlinkDenied(_) => 17,
        ErrorKind::HardlinkDenied(_) => 18,
        ErrorKind::PermissionDenied(_) => 19,
        ErrorKind::FileTooLarge(_) => 20,
        ErrorKind::IoBusy(_) => 21,
        ErrorKind::IndexNotReady(_) => 22,
        ErrorKind::PlanStale(_) => 23,
        ErrorKind::ConfirmationRequired(_) => 24,
        ErrorKind::ConfirmationInvalid(_) => 25,
        ErrorKind::TestApprovalRequired(_) => 26,
        ErrorKind::GitUnavailable(_) => 27,
        ErrorKind::GitUnsafeRepository(_) => 28,
        ErrorKind::GitAuthRequired(_) => 29,
        ErrorKind::GitDiverged(_) => 30,
        ErrorKind::GitConflict(_) => 31,
        ErrorKind::GitFailed(_) => 32,
        ErrorKind::MigrationRequired(_) => 33,
        ErrorKind::RecoveryRequired(_) => 34,
        ErrorKind::Internal(_) => 35,
    };
    let retryable = matches!(code, 1 | 2 | 21 | 22 | 29);
    let (message, recovery) = COPY[code];
    let dto = ErrorDto {
        version: 1,
        kind,
        message: PublicText(message.into()),
        retryable,
        request_id,
        recovery: PublicText(recovery.into()),
    };
    dto.validate()
        .map_err(|_| ErrorConstructionError { _private: () })?;
    Ok(dto)
}
macro_rules! factory {($name:ident($request:ident $(,$arg:ident:$ty:ty)*)=>$variant:ident:$details:ident{$($field:ident:$value:expr),*$(,)?})=>{pub(super) fn $name($request:RequestId,$($arg:$ty),*)->Result<ErrorDto,ErrorConstructionError>{make($request,ErrorKind::$variant($details{$($field:$value),*}))}}}
factory!(invalid_argument(r,f:Field,x:Reason)=>InvalidArgument:InvalidArgumentData{field:field(f),reason:reason(x)});
factory!(workspace_not_found(r,x:RecentWorkspaceId)=>WorkspaceNotFound:WorkspaceNotFoundData{recent_workspace_id:x});
factory!(workspace_unavailable(r,x:Reason)=>WorkspaceUnavailable:WorkspaceUnavailableData{reason:reason(x)});
factory!(workspace_read_only(r,x:Reason,c:Option<RequiredCapability>)=>WorkspaceReadOnly:WorkspaceReadOnlyData{reason:reason(x),required_capability:c});
factory!(workspace_limit_exceeded(r,l:U64Decimal,o:U64Decimal,s:WorkspaceLimitScope)=>WorkspaceLimitExceeded:WorkspaceLimitExceededData{limit:l,observed_at_least:o,scope:s});
factory!(schema_unsupported(r,f:u32,n:u32,m:u32)=>SchemaUnsupported:SchemaUnsupportedData{found_version:f,minimum_version:n,maximum_version:m});
factory!(invalid_manifest(r,p:SafeRelativePath,d:Diagnostic,f:Option<Field>,l:Option<SourceLocation>)=>InvalidManifest:InvalidManifestData{relative_path:p,diagnostic_code:diagnostic(d),field:f.map(field),line:l.as_ref().map(|x|x.line),column:l.as_ref().map(|x|x.column)});
factory!(invalid_category(r,p:SafeRelativePath,d:Diagnostic,f:Option<Field>,l:Option<SourceLocation>,c:Option<CategoryId>)=>InvalidCategory:InvalidCategoryData{relative_path:p,diagnostic_code:diagnostic(d),field:f.map(field),line:l.as_ref().map(|x|x.line),column:l.as_ref().map(|x|x.column),category_id:c});
factory!(invalid_guide(r,p:SafeRelativePath,d:Diagnostic,f:Option<Field>,l:Option<SourceLocation>,g:Option<GuideId>)=>InvalidGuide:InvalidGuideData{relative_path:p,diagnostic_code:diagnostic(d),field:f.map(field),line:l.as_ref().map(|x|x.line),column:l.as_ref().map(|x|x.column),guide_id:g});
/// INVALID_GUIDE for a hand-written Guide that Rehow has listed but not adopted
/// (ADR 0063). The detail shape is the closed INVALID_GUIDE shape; only the
/// fixed human copy is more specific.
pub(super) fn unadopted_guide(
    r: RequestId,
    p: SafeRelativePath,
    g: GuideId,
) -> Result<ErrorDto, ErrorConstructionError> {
    let mut dto = invalid_guide(r, p, Diagnostic::MissingRequiredField, None, None, Some(g))?;
    dto.message =
        PublicText("This Guide is a plain Markdown file Rehow hasn't adopted yet.".into());
    dto.recovery = PublicText("Open and save it once in the Rehow app first.".into());
    dto.validate()
        .map_err(|_| ErrorConstructionError { _private: () })?;
    Ok(dto)
}
factory!(unsafe_round_trip(r,p:SafeRelativePath,x:Reason,f:Option<Field>)=>UnsafeRoundTrip:UnsafeRoundTripData{relative_path:p,reason:reason(x),field:f.map(field)});
factory!(duplicate_id(r,e:PortableEntityId,p:BoundedList<SafeRelativePath>)=>DuplicateId:DuplicateIdData{entity:e,relative_paths:p});
factory!(category_not_found(r,c:CategoryId)=>CategoryNotFound:CategoryNotFoundData{category_id:c});
factory!(guide_not_found(r,g:GuideId)=>GuideNotFound:GuideNotFoundData{guide_id:g});
factory!(attachment_not_found(r,a:AttachmentId,g:GuideId)=>AttachmentNotFound:AttachmentNotFoundData{attachment_id:a,guide_id:g});
factory!(revision_conflict(r,e:PortableEntityId,x:Revision,a:Revision)=>RevisionConflict:RevisionConflictData{entity:e,expected_revision:x,actual_revision:a});
factory!(snapshot_conflict(r,e:Snapshot,a:Snapshot,c:BoundedList<PortableEntityId>)=>SnapshotConflict:SnapshotConflictData{expected_snapshot:e,actual_snapshot:a,changed_entity_ids:c});
factory!(path_escape(r,x:Reason,p:Option<SafeRelativePath>)=>PathEscape:PathEscapeData{reason:reason(x),relative_path:p});
factory!(symlink_denied(r,p:SafeRelativePath)=>SymlinkDenied:SymlinkDeniedData{relative_path:p});
factory!(hardlink_denied(r,p:SafeRelativePath,c:U64Decimal)=>HardlinkDenied:HardlinkDeniedData{relative_path:p,link_count:c});
factory!(permission_denied(r,o:Operation,p:Option<SafeRelativePath>)=>PermissionDenied:PermissionDeniedData{operation:operation(o),relative_path:p});
factory!(file_too_large(r,p:SafeRelativePath,l:U64Decimal,o:U64Decimal)=>FileTooLarge:FileTooLargeData{relative_path:p,limit_bytes:l,observed_at_least_bytes:o});
factory!(io_busy(r,o:Operation,t:u32)=>IoBusy:IoBusyData{operation:operation(o),timeout_ms:t});
factory!(index_not_ready(r,g:U64Decimal,c:u32,t:Option<u32>)=>IndexNotReady:IndexNotReadyData{index_generation:g,progress_completed:c,progress_total:t});
factory!(plan_stale(r,o:Operation,c:BoundedList<PortableEntityId>)=>PlanStale:PlanStaleData{operation:operation(o),changed_entity_ids:c});
factory!(confirmation_required(r,o:Operation,p:PlanToken,e:Timestamp)=>ConfirmationRequired:ConfirmationRequiredData{operation:operation(o),plan_token:p,expires_at:e});
factory!(confirmation_invalid(r,o:Operation,x:Reason)=>ConfirmationInvalid:ConfirmationInvalidData{operation:operation(o),reason:reason(x)});
factory!(test_approval_required(r,g:GuideId,v:Revision,a:ApprovalRequestId)=>TestApprovalRequired:TestApprovalRequiredData{guide_id:g,guide_revision:v,approval_request_id:a});
factory!(git_unavailable(r,s:BoundedList<GitSearchSourceKind>)=>GitUnavailable:GitUnavailableData{search_source_kinds:s});
factory!(git_unsafe_repository(r,x:Reason)=>GitUnsafeRepository:GitUnsafeRepositoryData{reason:reason(x)});
factory!(git_auth_required(r,n:SafeGitDisplay,h:SafeHost,c:GitCredentialKind)=>GitAuthRequired:GitAuthRequiredData{remote_name:n,host:h,credential_kind:c});
factory!(git_diverged(r,a:U64Decimal,b:U64Decimal,n:SafeGitDisplay)=>GitDiverged:GitDivergedData{ahead:a,behind:b,branch:n});
factory!(git_conflict(r,c:GitConflictKind,p:BoundedList<SafeRelativePath>)=>GitConflict:GitConflictData{conflict_kind:c,relative_paths:p});
pub(super) fn git_failed(
    r: RequestId,
    o: GitOperation,
    e: GitExitKind,
) -> Result<ErrorDto, ErrorConstructionError> {
    let text = match e {
        GitExitKind::Spawn => "Git could not be started.",
        GitExitKind::ExitStatus => "Git returned a non-success status.",
        GitExitKind::Signal => "Git was terminated by a signal.",
        GitExitKind::Timeout => "Git did not finish before the timeout.",
        GitExitKind::OutputLimit => "Git output exceeded the safety limit.",
    };
    make(
        r,
        ErrorKind::GitFailed(GitFailedData {
            operation: git_operation(o),
            exit_kind: e,
            sanitized_reason: PublicText(text.into()),
        }),
    )
}

pub(super) fn git_remote_state_uncertain(r: RequestId) -> Result<ErrorDto, ErrorConstructionError> {
    make(
        r,
        ErrorKind::GitFailed(GitFailedData {
            operation: git_operation(GitOperation::Sync),
            exit_kind: GitExitKind::ExitStatus,
            sanitized_reason: PublicText(
                "The remote may already contain the push; inspect remote state before retrying."
                    .into(),
            ),
        }),
    )
}
factory!(migration_required(r,f:u32,t:u32)=>MigrationRequired:MigrationRequiredData{found_version:f,target_version:t});
factory!(recovery_required(r,o:OperationId,x:Reason,p:BoundedList<SafeRelativePath>)=>RecoveryRequired:RecoveryRequiredData{operation_id:o,reason:reason(x),affected_relative_paths:p});
factory!(internal(r,i:IncidentId)=>Internal:InternalData{incident_id:i});

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    fn id() -> RequestId {
        RequestId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap()
    }
    fn path() -> SafeRelativePath {
        SafeRelativePath::parse_wire("guides/a.md").unwrap()
    }
    fn rev(byte: char) -> Revision {
        Revision::from_str(&format!("rev:v1:{}", byte.to_string().repeat(64))).unwrap()
    }
    fn snap(byte: char) -> Snapshot {
        Snapshot::from_str(&format!("snapshot:v1:{}", byte.to_string().repeat(64))).unwrap()
    }
    fn one_path() -> BoundedList<SafeRelativePath> {
        BoundedList::try_new(vec![path()], 1).unwrap()
    }
    fn entity() -> PortableEntityId {
        PortableEntityId::guide(GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap())
    }
    fn ok(v: Result<ErrorDto, ErrorConstructionError>) -> ErrorDto {
        match v {
            Ok(x) => x,
            Err(_) => panic!("valid fixture rejected"),
        }
    }
    #[test]
    fn unadopted_guide_refusal_is_invalid_guide_with_specific_copy() {
        let guide = GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap();
        let dto = unadopted_guide_error(id(), "categories/general/deploy notes.md", guide);
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["code"], "INVALID_GUIDE");
        assert_eq!(value["retryable"], false);
        assert_eq!(value["details"]["diagnosticCode"], "missing_required_field");
        assert_eq!(
            value["details"]["relativePath"],
            "categories/general/deploy notes.md"
        );
        assert_eq!(value["details"]["guideId"], guide.to_string());
        assert_eq!(
            value["message"],
            "This Guide is a plain Markdown file Rehow hasn't adopted yet."
        );
        let hostile = unadopted_guide_error(id(), "categories/general/a\u{202e}b\u{7}.md", guide);
        let hostile = serde_json::to_value(&hostile).unwrap();
        let path = hostile["details"]["relativePath"]
            .as_str()
            .unwrap_or_default();
        assert!(!path.contains('\u{202e}') && !path.contains('\u{7}'));
    }
    #[test]
    fn every_factory_produces_correlated_fixed_copy() {
        let r = id();
        let recent =
            RecentWorkspaceId::from_str("recent:v1:0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap();
        let mut all = vec![
            invalid_argument(r, Field::Title, Reason::Required),
            workspace_not_found(r, recent),
            workspace_unavailable(r, Reason::WorkspaceRootMissing),
            workspace_read_only(
                r,
                Reason::WorkspaceNotWritable,
                Some(RequiredCapability::Write),
            ),
            workspace_limit_exceeded(
                r,
                U64Decimal::new(1),
                U64Decimal::new(2),
                WorkspaceLimitScope::Guides,
            ),
            schema_unsupported(r, 9, 1, 2),
            invalid_manifest(
                r,
                path(),
                Diagnostic::InvalidFieldValue,
                Some(Field::Title),
                Some(SourceLocation { line: 1, column: 2 }),
            ),
            invalid_category(
                r,
                path(),
                Diagnostic::InvalidFieldValue,
                None,
                None,
                Some(CategoryId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap()),
            ),
            invalid_guide(
                r,
                path(),
                Diagnostic::InvalidFieldValue,
                None,
                None,
                Some(GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap()),
            ),
            unsafe_round_trip(r, path(), Reason::RoundTripUnproven, Some(Field::Title)),
            duplicate_id(
                r,
                entity(),
                BoundedList::try_new(
                    vec![path(), SafeRelativePath::parse_wire("guides/b.md").unwrap()],
                    2,
                )
                .unwrap(),
            ),
            category_not_found(
                r,
                CategoryId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap(),
            ),
            guide_not_found(
                r,
                GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap(),
            ),
            attachment_not_found(
                r,
                AttachmentId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap(),
                GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap(),
            ),
            revision_conflict(r, entity(), rev('0'), rev('1')),
            snapshot_conflict(
                r,
                snap('0'),
                snap('1'),
                BoundedList::try_new(Vec::new(), 0).unwrap(),
            ),
            path_escape(r, Reason::PathOutsideWorkspace, Some(path())),
            symlink_denied(r, path()),
            hardlink_denied(r, path(), U64Decimal::new(2)),
            permission_denied(r, Operation::WorkspaceOpen, Some(path())),
            file_too_large(r, path(), U64Decimal::new(1), U64Decimal::new(2)),
            io_busy(r, Operation::WorkspaceOpen, 1),
            index_not_ready(r, U64Decimal::new(1), 1, Some(1)),
            plan_stale(
                r,
                Operation::GuideUpdate,
                BoundedList::try_new(Vec::new(), 0).unwrap(),
            ),
            confirmation_required(
                r,
                Operation::GuideDelete,
                PlanToken::from_str(&format!("plan:v1:{}", "A".repeat(43))).unwrap(),
                Timestamp::from_str("2026-09-06T00:00:00Z").unwrap(),
            ),
            confirmation_invalid(r, Operation::GuideDelete, Reason::PlanExpired),
            test_approval_required(
                r,
                GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap(),
                rev('0'),
                ApprovalRequestId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap(),
            ),
            git_unavailable(
                r,
                BoundedList::try_new(vec![GitSearchSourceKind::System], 1).unwrap(),
            ),
            git_unsafe_repository(r, Reason::RepositoryOwnerUntrusted),
            git_auth_required(
                r,
                SafeGitDisplay::parse_wire("origin").unwrap(),
                SafeHost::parse_wire("example.com").unwrap(),
                GitCredentialKind::SshAgent,
            ),
            git_diverged(
                r,
                U64Decimal::new(1),
                U64Decimal::new(1),
                SafeGitDisplay::parse_wire("main").unwrap(),
            ),
            git_conflict(r, GitConflictKind::Merge, one_path()),
            git_failed(r, GitOperation::Sync, GitExitKind::Timeout),
            migration_required(r, 1, 2),
            recovery_required(
                r,
                OperationId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap(),
                Reason::JournalInvalid,
                one_path(),
            ),
            internal(
                r,
                IncidentId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap(),
            ),
        ];
        let root = format!(
            "{}/tests/fixtures/error-contract/v1",
            env!("CARGO_MANIFEST_DIR")
        );
        let mut fixtures = std::fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        fixtures.sort();
        assert_eq!(all.len(), 36);
        assert_eq!(fixtures.len(), 36);
        for (index, (result, path)) in all.drain(..).zip(fixtures).enumerate() {
            let value = serde_json::to_value(ok(result)).unwrap();
            let golden: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(value, golden);
            assert_eq!(value["message"], COPY[index].0);
            assert_eq!(value["recovery"], COPY[index].1);
            assert_eq!(value.as_object().unwrap().len(), 7);
        }
    }
    #[test]
    fn git_reasons_are_fixed() {
        let reasons = [
            (GitExitKind::Spawn, "Git could not be started."),
            (
                GitExitKind::ExitStatus,
                "Git returned a non-success status.",
            ),
            (GitExitKind::Signal, "Git was terminated by a signal."),
            (
                GitExitKind::Timeout,
                "Git did not finish before the timeout.",
            ),
            (
                GitExitKind::OutputLimit,
                "Git output exceeded the safety limit.",
            ),
        ];
        for (kind, text) in reasons {
            let value =
                serde_json::to_value(ok(git_failed(id(), GitOperation::Status, kind))).unwrap();
            assert_eq!(value["details"]["sanitizedReason"], text)
        }
    }

    #[test]
    fn every_factory_owned_correlation_and_count_invariant_rejects() {
        let r = id();
        let empty_paths = || BoundedList::try_new(Vec::new(), 0).unwrap();
        let invalid = vec![
            workspace_limit_exceeded(
                r,
                U64Decimal::new(2),
                U64Decimal::new(2),
                WorkspaceLimitScope::Guides,
            ),
            workspace_limit_exceeded(
                r,
                U64Decimal::new(2),
                U64Decimal::new(1),
                WorkspaceLimitScope::Guides,
            ),
            schema_unsupported(r, 2, 1, 3),
            schema_unsupported(r, 3, 0, 2),
            schema_unsupported(r, 3, 1, 0),
            schema_unsupported(r, 3, 2, 1),
            schema_unsupported(r, 1, 1, 2),
            schema_unsupported(r, 2, 1, 2),
            duplicate_id(r, entity(), empty_paths()),
            revision_conflict(r, entity(), rev('0'), rev('0')),
            snapshot_conflict(
                r,
                snap('0'),
                snap('0'),
                BoundedList::try_new(Vec::new(), 0).unwrap(),
            ),
            hardlink_denied(r, path(), U64Decimal::new(1)),
            file_too_large(r, path(), U64Decimal::new(2), U64Decimal::new(2)),
            file_too_large(r, path(), U64Decimal::new(2), U64Decimal::new(1)),
            io_busy(r, Operation::WorkspaceOpen, 0),
            index_not_ready(r, U64Decimal::new(1), 2, Some(1)),
            git_unavailable(r, BoundedList::try_new(Vec::new(), 0).unwrap()),
            git_diverged(
                r,
                U64Decimal::new(0),
                U64Decimal::new(1),
                SafeGitDisplay::parse_wire("main").unwrap(),
            ),
            git_diverged(
                r,
                U64Decimal::new(1),
                U64Decimal::new(0),
                SafeGitDisplay::parse_wire("main").unwrap(),
            ),
            git_conflict(r, GitConflictKind::Merge, empty_paths()),
            migration_required(r, 0, 1),
            migration_required(r, 2, 2),
            migration_required(r, 2, 1),
        ];
        assert_eq!(invalid.len(), 23);
        for result in invalid {
            assert!(result.is_err());
        }

        for result in [
            invalid_manifest(
                r,
                path(),
                Diagnostic::InvalidFieldValue,
                None,
                Some(SourceLocation { line: 0, column: 1 }),
            ),
            invalid_category(
                r,
                path(),
                Diagnostic::InvalidFieldValue,
                None,
                Some(SourceLocation { line: 1, column: 0 }),
                None,
            ),
            invalid_guide(
                r,
                path(),
                Diagnostic::InvalidFieldValue,
                None,
                Some(SourceLocation { line: 0, column: 0 }),
                None,
            ),
        ] {
            assert!(result.is_err());
        }
    }
}

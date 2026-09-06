use rehow_core::protocol::*;
use ts_rs::{Config, TS};

struct One;
struct True;
struct False;
macro_rules! literal_ts {
    ($ty:ty,$text:literal) => {
        impl TS for $ty {
            type WithoutGenerics = Self;
            type OptionInnerType = Self;
            fn name(_: &Config) -> String {
                $text.into()
            }
            fn inline(_: &Config) -> String {
                $text.into()
            }
            fn decl(_: &Config) -> String {
                String::new()
            }
            fn decl_concrete(_: &Config) -> String {
                String::new()
            }
        }
    };
}
literal_ts!(One, "1");
literal_ts!(True, "true");
literal_ts!(False, "false");

#[expect(dead_code, reason = "private ts-rs contract marker")]
#[derive(TS)]
#[ts(rename = "ErrorEnvelope", rename_all = "camelCase")]
struct ErrorEnvelopeContract {
    version: One,
    message: PublicText,
    request_id: RequestId,
    recovery: PublicText,
}

#[expect(dead_code, reason = "private exhaustive ts-rs contract marker")]
#[derive(TS)]
#[ts(
    rename = "ErrorVariant",
    tag = "code",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
enum ErrorVariantContract {
    InvalidArgument {
        retryable: False,
        details: InvalidArgumentDetails,
    },
    WorkspaceNotFound {
        retryable: True,
        details: WorkspaceNotFoundDetails,
    },
    WorkspaceUnavailable {
        retryable: True,
        details: WorkspaceUnavailableDetails,
    },
    WorkspaceReadOnly {
        retryable: False,
        details: WorkspaceReadOnlyDetails,
    },
    WorkspaceLimitExceeded {
        retryable: False,
        details: WorkspaceLimitExceededDetails,
    },
    SchemaUnsupported {
        retryable: False,
        details: SchemaUnsupportedDetails,
    },
    InvalidManifest {
        retryable: False,
        details: InvalidManifestDetails,
    },
    InvalidCategory {
        retryable: False,
        details: InvalidCategoryDetails,
    },
    InvalidGuide {
        retryable: False,
        details: InvalidGuideDetails,
    },
    UnsafeRoundTrip {
        retryable: False,
        details: UnsafeRoundTripDetails,
    },
    DuplicateId {
        retryable: False,
        details: DuplicateIdDetails,
    },
    CategoryNotFound {
        retryable: False,
        details: CategoryNotFoundDetails,
    },
    GuideNotFound {
        retryable: False,
        details: GuideNotFoundDetails,
    },
    AttachmentNotFound {
        retryable: False,
        details: AttachmentNotFoundDetails,
    },
    RevisionConflict {
        retryable: False,
        details: RevisionConflictDetails,
    },
    SnapshotConflict {
        retryable: False,
        details: SnapshotConflictDetails,
    },
    PathEscape {
        retryable: False,
        details: PathEscapeDetails,
    },
    SymlinkDenied {
        retryable: False,
        details: SymlinkDeniedDetails,
    },
    HardlinkDenied {
        retryable: False,
        details: HardlinkDeniedDetails,
    },
    PermissionDenied {
        retryable: False,
        details: PermissionDeniedDetails,
    },
    FileTooLarge {
        retryable: False,
        details: FileTooLargeDetails,
    },
    IoBusy {
        retryable: True,
        details: IoBusyDetails,
    },
    IndexNotReady {
        retryable: True,
        details: IndexNotReadyDetails,
    },
    PlanStale {
        retryable: False,
        details: PlanStaleDetails,
    },
    ConfirmationRequired {
        retryable: False,
        details: ConfirmationRequiredDetails,
    },
    ConfirmationInvalid {
        retryable: False,
        details: ConfirmationInvalidDetails,
    },
    TestApprovalRequired {
        retryable: False,
        details: TestApprovalRequiredDetails,
    },
    GitUnavailable {
        retryable: False,
        details: GitUnavailableDetails,
    },
    GitUnsafeRepository {
        retryable: False,
        details: GitUnsafeRepositoryDetails,
    },
    GitAuthRequired {
        retryable: True,
        details: GitAuthRequiredDetails,
    },
    GitDiverged {
        retryable: False,
        details: GitDivergedDetails,
    },
    GitConflict {
        retryable: False,
        details: GitConflictDetails,
    },
    GitFailed {
        retryable: False,
        details: GitFailedDetails,
    },
    MigrationRequired {
        retryable: False,
        details: MigrationRequiredDetails,
    },
    RecoveryRequired {
        retryable: False,
        details: RecoveryRequiredDetails,
    },
    Internal {
        retryable: False,
        details: InternalDetails,
    },
}

macro_rules! emit { ($cfg:ident;$($ty:ty),+$(,)?) => {$({ println!("export {}",<$ty as TS>::decl(&$cfg)); })+}; }
fn main() {
    let config = Config::default();
    println!("// This file is generated. Do not edit.");
    emit!(config;WorkspaceId,CategoryId,GuideId,AttachmentId,RequestId,ApprovalRequestId,OperationId,IncidentId,RecentWorkspaceId,Timestamp,U64Decimal,SignedInt64Decimal,Revision,Snapshot,VerificationHash,PlanToken,SchemaFieldPath,ReasonCode,DiagnosticCode,OperationCode,GitOperationCode,SafeRelativePath,SafeGitDisplay,SafeHost,PublicText,RequiredCapability,WorkspaceLimitScope,GitSearchSourceKind,GitCredentialKind,GitExitKind,GitConflictKind,ErrorCode,PortableEntityId,BoundedList<String>,InvalidArgumentDetails,WorkspaceNotFoundDetails,WorkspaceUnavailableDetails,WorkspaceReadOnlyDetails,WorkspaceLimitExceededDetails,SchemaUnsupportedDetails,InvalidManifestDetails,InvalidCategoryDetails,InvalidGuideDetails,UnsafeRoundTripDetails,DuplicateIdDetails,CategoryNotFoundDetails,GuideNotFoundDetails,AttachmentNotFoundDetails,RevisionConflictDetails,SnapshotConflictDetails,PathEscapeDetails,SymlinkDeniedDetails,HardlinkDeniedDetails,PermissionDeniedDetails,FileTooLargeDetails,IoBusyDetails,IndexNotReadyDetails,PlanStaleDetails,ConfirmationRequiredDetails,ConfirmationInvalidDetails,TestApprovalRequiredDetails,GitUnavailableDetails,GitUnsafeRepositoryDetails,GitAuthRequiredDetails,GitDivergedDetails,GitConflictDetails,GitFailedDetails,MigrationRequiredDetails,RecoveryRequiredDetails,InternalDetails,ErrorEnvelopeContract,ErrorVariantContract,ErrorDto);
}

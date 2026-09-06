use rehow_core::protocol::*;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
const ID: &str = "0199a1d7-7b0d-7d8e-a38d-f936c5735e10";
#[test]
fn canonical_protocol_values_round_trip() {
    assert_eq!(WorkspaceId::from_str(ID).unwrap().to_string(), ID);
    assert!(WorkspaceId::from_str(&ID.to_uppercase()).is_err());
    assert_eq!(
        Timestamp::from_str("2026-09-06T00:00:00.000000001Z")
            .unwrap()
            .to_string(),
        "2026-09-06T00:00:00.000000001Z"
    );
    assert!(Timestamp::from_str("2026-09-06T00:00:00.10Z").is_err());
    assert_eq!(
        U64Decimal::from_str("18446744073709551615")
            .unwrap()
            .to_string(),
        "18446744073709551615"
    );
    assert!(SignedInt64Decimal::from_str("-0").is_err());
    let r = format!("rev:v1:{}", "0".repeat(64));
    assert_eq!(Revision::from_str(&r).unwrap().to_string(), r);
    assert!(SafeRelativePath::parse_wire("../guide.md").is_err());
    assert!(SafeHost::parse_wire("::ffff:192.0.2.128").is_ok());
    assert!(SafeHost::parse_wire("::ffff:c000:280").is_err());
}
#[test]
fn bounded_lists_require_complete_prefix() {
    assert!(BoundedList::<u8>::try_new(vec![], 0).is_ok());
    assert!(!BoundedList::try_new(vec![0_u8; 2], 2).unwrap().truncated());
    assert!(
        !BoundedList::try_new(vec![0_u8; 32], 32)
            .unwrap()
            .truncated()
    );
    assert!(
        BoundedList::try_new(vec![0_u8; 32], 33)
            .unwrap()
            .truncated()
    );
    assert!(
        BoundedList::try_new(vec![0_u8; 32], 40)
            .unwrap()
            .truncated()
    );
    assert!(BoundedList::try_new(vec![0_u8; 31], 33).is_err());
    assert!(BoundedList::try_new(vec![0_u8; 33], 33).is_err());
    assert!(
        serde_json::from_str::<BoundedList<u8>>(r#"{"items":[1],"totalCount":1,"truncated":true}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<BoundedList<u8>>(
            r#"{"items":[],"totalCount":0,"truncated":false,"extra":1}"#
        )
        .is_err()
    );
}

#[test]
fn bounded_list_wire_boundaries_and_deserialization_cutoff() {
    for (count, retained) in [(0, 0), (32, 32), (33, 32), (40, 32)] {
        let wire = format!(
            r#"{{"items":[{}],"totalCount":{count},"truncated":{}}}"#,
            vec!["0"; retained].join(","),
            count > 32
        );
        let list: BoundedList<u8> = serde_json::from_str(&wire).unwrap();
        assert_eq!(list.items().len(), retained);
        assert_eq!(list.total_count(), count);
        assert_eq!(list.truncated(), count > 32);
    }

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    struct Bomb;
    impl<'de> Deserialize<'de> for Bomb {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            CALLS.fetch_add(1, Ordering::SeqCst);
            let _ = serde::de::IgnoredAny::deserialize(deserializer)?;
            Ok(Bomb)
        }
    }
    CALLS.store(0, Ordering::SeqCst);
    let wire = format!(
        r#"{{"items":[{}],"totalCount":33,"truncated":true}}"#,
        vec!["0"; 33].join(",")
    );
    assert!(serde_json::from_str::<BoundedList<Bomb>>(&wire).is_err());
    assert_eq!(CALLS.load(Ordering::SeqCst), 32);
}
#[test]
fn every_id_wrapper_is_strict_string_serde() {
    macro_rules! check {
        ($t:ty) => {{
            let value = <$t>::from_str(ID).unwrap();
            assert_eq!(serde_json::to_string(&value).unwrap(), format!("\"{ID}\""));
            assert!(serde_json::from_str::<$t>("7").is_err());
            assert!(<$t>::from_str("0199a1d7-7b0d-6d8e-a38d-f936c5735e10").is_err());
            assert!(<$t>::from_str("0199a1d7-7b0d-7d8e-738d-f936c5735e10").is_err());
        }};
    }
    check!(WorkspaceId);
    check!(CategoryId);
    check!(GuideId);
    check!(AttachmentId);
    check!(RequestId);
    check!(ApprovalRequestId);
    check!(OperationId);
    check!(IncidentId);
    let recent = RecentWorkspaceId::from_str(&format!("recent:v1:{ID}")).unwrap();
    assert_eq!(recent.to_string(), format!("recent:v1:{ID}"));
    let recent_wire = format!(r#""recent:v1:{ID}""#);
    assert_eq!(
        serde_json::from_str::<RecentWorkspaceId>(&recent_wire)
            .unwrap()
            .to_string(),
        format!("recent:v1:{ID}")
    );
    for invalid in [
        format!(r#""{ID}""#),
        format!(r#""recent:v2:{ID}""#),
        format!(r#""recent:v1:recent:v1:{ID}""#),
        "123456789".into(),
    ] {
        assert!(serde_json::from_str::<RecentWorkspaceId>(&invalid).is_err());
    }
}
#[test]
fn timestamps_validate_calendar_and_order() {
    for valid in [
        "0001-01-01T00:00:00Z",
        "2000-02-29T23:59:59Z",
        "9999-12-31T23:59:59.999999999Z",
    ] {
        assert_eq!(Timestamp::from_str(valid).unwrap().to_string(), valid)
    }
    for invalid in [
        "0000-01-01T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2024-02-30T00:00:00Z",
        "2024-01-01T24:00:00Z",
        "2024-01-01T00:00:60Z",
        "2024-01-01t00:00:00z",
        "2024-01-01T00:00:00+00:00",
        "2024-01-01T00:00:00.0Z",
        "2024-01-01T00:00:00.1234567890Z",
    ] {
        assert!(Timestamp::from_str(invalid).is_err(), "{invalid}")
    }
    assert!(
        Timestamp::from_str("2024-01-01T00:00:00.000000001Z").unwrap()
            > Timestamp::from_str("2024-01-01T00:00:00Z").unwrap()
    );
    assert!(serde_json::from_str::<Timestamp>("1").is_err());
}
#[test]
fn decimals_digests_and_plan_tokens_are_canonical() {
    for x in ["0", "18446744073709551615"] {
        assert_eq!(U64Decimal::from_str(x).unwrap().to_string(), x)
    }
    for x in ["", "+1", "01", "-1", "18446744073709551616"] {
        assert!(U64Decimal::from_str(x).is_err())
    }
    for x in ["0", "18446744073709551615"] {
        let wire = format!(r#""{x}""#);
        assert_eq!(
            serde_json::from_str::<U64Decimal>(&wire)
                .unwrap()
                .to_string(),
            x
        );
    }
    assert!(serde_json::from_str::<U64Decimal>("18446744073709551615").is_err());
    for x in ["-9223372036854775808", "0", "9223372036854775807"] {
        assert_eq!(SignedInt64Decimal::from_str(x).unwrap().to_string(), x)
    }
    for x in ["-0", "01", "9223372036854775808"] {
        assert!(SignedInt64Decimal::from_str(x).is_err())
    }
    for x in ["-9223372036854775808", "0", "9223372036854775807"] {
        let wire = format!(r#""{x}""#);
        assert_eq!(
            serde_json::from_str::<SignedInt64Decimal>(&wire)
                .unwrap()
                .to_string(),
            x
        );
    }
    for (prefix, kind) in [("rev:v1:", 0_u8), ("snapshot:v1:", 1), ("verify:v1:", 2)] {
        let s = format!("{prefix}{}", "a".repeat(64));
        let wire = format!(r#""{s}""#);
        let output = match kind {
            0 => serde_json::from_str::<Revision>(&wire).unwrap().to_string(),
            1 => serde_json::from_str::<Snapshot>(&wire).unwrap().to_string(),
            _ => serde_json::from_str::<VerificationHash>(&wire)
                .unwrap()
                .to_string(),
        };
        assert_eq!(output, s);
        for invalid in [
            format!("other:v1:{}", "a".repeat(64)),
            format!("{prefix}{}", "a".repeat(63)),
            format!("{prefix}{}", "a".repeat(65)),
            format!("{prefix}{}", "A".repeat(64)),
            format!("{prefix}{}g", "a".repeat(63)),
        ] {
            let bad_wire = format!(r#""{invalid}""#);
            let rejected = match kind {
                0 => serde_json::from_str::<Revision>(&bad_wire).is_err(),
                1 => serde_json::from_str::<Snapshot>(&bad_wire).is_err(),
                _ => serde_json::from_str::<VerificationHash>(&bad_wire).is_err(),
            };
            assert!(rejected, "{invalid}");
        }
    }
    for final_char in "AEIMQUYcgkosw048".chars() {
        let token = format!("plan:v1:{}{}", "A".repeat(42), final_char);
        assert!(PlanToken::from_str(&token).is_ok(), "{final_char}")
    }
    assert!(PlanToken::from_str(&format!("plan:v1:{}B", "A".repeat(42))).is_err());
    assert!(PlanToken::from_str(&format!("plan:v1:{}=", "A".repeat(42))).is_err());
    for invalid in [
        format!("plan:v1:{}", "A".repeat(42)),
        format!("plan:v1:{}", "A".repeat(44)),
        format!("plan:v1:{}+", "A".repeat(42)),
        format!("plan:v1:{}/", "A".repeat(42)),
        format!("plan:v1:{}=", "A".repeat(42)),
        format!("plan:v1:{}B", "A".repeat(42)),
    ] {
        assert!(PlanToken::from_str(&invalid).is_err(), "{invalid}");
    }
}
#[test]
fn safe_display_and_host_canonicality() {
    for valid in ["guides/a.md", "name%25", "bad%FF", "%E2%80%AE%FF", "%25%FF"] {
        assert!(SafeRelativePath::parse_wire(valid).is_ok(), "{valid}")
    }
    for invalid in [
        "", "/root", "a//b", ".", "..", "a/../b", "%41", "%2F", "%ff", "%", "a\n",
    ] {
        assert!(SafeRelativePath::parse_wire(invalid).is_err(), "{invalid}")
    }
    assert!(SafeRelativePath::parse_wire(&"a".repeat(1024)).is_ok());
    assert!(SafeRelativePath::parse_wire(&"a".repeat(1025)).is_err());
    let escaped_limit = "%FF".repeat(1024);
    assert_eq!(escaped_limit.len(), 3072);
    assert!(SafeRelativePath::parse_wire(&escaped_limit).is_ok());
    assert!(SafeRelativePath::parse_wire(&(escaped_limit + "a")).is_err());
    for encoded in ["%2E", "%2F", "a%2Fb", "%2E%2E"] {
        assert!(SafeRelativePath::parse_wire(encoded).is_err(), "{encoded}");
    }
    for prohibited in [
        '\u{0000}',
        '\u{001f}',
        '\u{007f}',
        '\u{009f}',
        '\u{061c}',
        '\u{200e}',
        '\u{200f}',
        '\u{2028}',
        '\u{202e}',
        '\u{2066}',
        '\u{2069}',
        '\u{feff}',
        '\u{fdd0}',
        '\u{fdef}',
        '\u{fffe}',
        '\u{ffff}',
        '\u{1fffe}',
        '\u{10ffff}',
    ] {
        assert!(SafeRelativePath::parse_wire(&prohibited.to_string()).is_err());
        let escaped = prohibited
            .to_string()
            .as_bytes()
            .iter()
            .map(|byte| format!("%{byte:02X}"))
            .collect::<String>();
        assert!(SafeRelativePath::parse_wire(&escaped).is_ok(), "{escaped}");
    }
    assert!(SafeGitDisplay::parse_wire(&"🦀".repeat(768)).is_ok());
    assert!(SafeGitDisplay::parse_wire(&"🦀".repeat(769)).is_err());
    for valid in [
        "unknown",
        "example.com",
        "127.0.0.1",
        "2001:db8::1",
        "::ffff:192.0.2.128",
    ] {
        assert!(SafeHost::parse_wire(valid).is_ok(), "{valid}")
    }
    for invalid in [
        "EXAMPLE.com",
        "example.com.",
        "user@example.com",
        "[::1]",
        "::1%eth0",
        "127.00.0.1",
        "123",
        "123.456",
        "127.1",
        "2130706433",
        "017700000001",
        "0x7f000001",
        "0x7f.1",
        "256.0.0.1",
        "::ffff:c000:280",
    ] {
        assert!(SafeHost::parse_wire(invalid).is_err(), "{invalid}")
    }
    assert!(SafeHost::parse_wire("0.0.0.0").is_ok());
    assert!(SafeHost::parse_wire("255.255.255.255").is_ok());
    assert!(SafeHost::parse_wire(&format!("{}.example", "a".repeat(63))).is_ok());
    assert!(SafeHost::parse_wire(&format!("{}.example", "a".repeat(64))).is_err());
    let host_253 = format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    );
    assert_eq!(host_253.len(), 253);
    assert!(SafeHost::parse_wire(&host_253).is_ok());
    assert!(SafeHost::parse_wire(&(host_253 + "e")).is_err());
}

#[test]
fn public_deserialization_errors_never_include_input_content() {
    for error in [
        serde_json::from_str::<WorkspaceId>(r#""secret-workspace-value""#)
            .err()
            .unwrap()
            .to_string(),
        serde_json::from_str::<WorkspaceId>("987654321")
            .err()
            .unwrap()
            .to_string(),
        serde_json::from_str::<RequiredCapability>(r#""secret-capability""#)
            .err()
            .unwrap()
            .to_string(),
        serde_json::from_str::<PortableEntityId>(r#"{"kind":"guide","id":987654321}"#)
            .err()
            .unwrap()
            .to_string(),
        serde_json::from_str::<BoundedList<WorkspaceId>>(
            r#"{"items":["secret-nested-value"],"totalCount":1,"truncated":false}"#,
        )
        .err()
        .unwrap()
        .to_string(),
    ] {
        assert!(error.starts_with("invalid canonical value"), "{error}");
        for secret in ["secret", "987654321"] {
            assert!(!error.contains(secret), "{error}");
        }
    }
}

#[test]
fn uuid_v7_timestamp_and_variant_boundaries_are_lexical() {
    for valid in [
        "00000000-0000-7000-8000-000000000000",
        "ffffffff-ffff-7fff-bfff-ffffffffffff",
    ] {
        assert_eq!(WorkspaceId::from_str(valid).unwrap().to_string(), valid);
    }
    for invalid in [
        "00000000-0000-7000-7fff-ffffffffffff",
        "00000000-0000-7000-c000-000000000000",
        "00000000-0000-6000-8000-000000000000",
        "{00000000-0000-7000-8000-000000000000}",
        "urn:uuid:00000000-0000-7000-8000-000000000000",
        "00000000000070008000000000000000",
    ] {
        assert!(WorkspaceId::from_str(invalid).is_err(), "{invalid}");
    }
    // Every UUID wrapper shares this exact parser through `uuid_id!`; the
    // wrapper matrix above separately proves each wrapper's strict Serde.
    for version in "012345689abcdef".chars() {
        let invalid = format!("00000000-0000-{version}000-8000-000000000000");
        assert!(
            WorkspaceId::from_str(&invalid).is_err(),
            "version {version}"
        );
    }
}

#[test]
fn consumer_token_catalogs_and_grammars_are_exhaustive() {
    const SCHEMA_PATHS: &[&str] = &[
        "/schema_version",
        "/workspace_id",
        "/name",
        "/created_at",
        "/updated_at",
        "/settings",
        "/settings/default_category_id",
        "/id",
        "/title",
        "/summary",
        "/order",
        "/status",
        "/category_id",
        "/tags",
        "/aliases",
        "/last_verified_at",
        "/verified_content_hash",
        "/archived_from_status",
        "/archived_at",
        "/applies_to",
        "/prerequisites",
        "/warnings",
        "/files_affected",
        "/related_guide_ids",
        "/references",
        "/attachments",
        "/version",
        "/code",
        "/message",
        "/retryable",
        "/requestId",
        "/details",
        "/recovery",
        "/expectedRevision",
        "/expectedSnapshot",
        "/planToken",
        "/operation",
        "/limit",
        "/cursor",
    ];
    assert_eq!(SCHEMA_PATHS.len(), 39);
    for spelling in SCHEMA_PATHS {
        let wire = serde_json::to_string(spelling).unwrap();
        let value: SchemaFieldPath = serde_json::from_str(&wire).unwrap();
        assert_eq!(value.as_str(), *spelling);
        assert_eq!(serde_json::to_string(&value).unwrap(), wire);
    }
    for wire in [r#""/unknown""#, r#""title""#, "987654321"] {
        assert!(serde_json::from_str::<SchemaFieldPath>(wire).is_err());
    }

    macro_rules! grammar {
        ($ty:ty) => {{
            for spelling in ["a", "a0_b", &"a".repeat(64)] {
                let wire = serde_json::to_string(spelling).unwrap();
                let value: $ty = serde_json::from_str(&wire).unwrap();
                assert_eq!(value.as_str(), spelling);
                assert_eq!(serde_json::to_string(&value).unwrap(), wire);
            }
            for spelling in [
                "".into(),
                "Upper".into(),
                "1starts_with_digit".into(),
                "trailing_".into(),
                "double__underscore".into(),
                "has-hyphen".into(),
                "a".repeat(65),
            ] {
                let wire = serde_json::to_string(&spelling).unwrap();
                assert!(serde_json::from_str::<$ty>(&wire).is_err(), "{spelling}");
            }
            assert!(serde_json::from_str::<$ty>("987654321").is_err());
        }};
    }
    grammar!(ReasonCode);
    grammar!(DiagnosticCode);
    grammar!(OperationCode);
    grammar!(GitOperationCode);
}

#[test]
fn every_closed_discriminant_and_error_code_has_exact_string_serde() {
    macro_rules! closed {
        ($ty:ty; $( $value:expr => $wire:literal ),+ $(,)?) => {{
            $(
                assert_eq!(serde_json::to_string(&$value).unwrap(), concat!("\"", $wire, "\""));
                let parsed: $ty = serde_json::from_str(concat!("\"", $wire, "\"")).unwrap();
                assert_eq!(serde_json::to_string(&parsed).unwrap(), concat!("\"", $wire, "\""));
            )+
            assert!(serde_json::from_str::<$ty>(r#""unknown-wire-value""#).is_err());
            assert!(serde_json::from_str::<$ty>("987654321").is_err());
        }};
    }
    closed!(RequiredCapability;
        RequiredCapability::Write => "write",
        RequiredCapability::Broad => "broad",
        RequiredCapability::GitWrite => "gitWrite",
    );
    closed!(WorkspaceLimitScope;
        WorkspaceLimitScope::Categories => "categories",
        WorkspaceLimitScope::Guides => "guides",
        WorkspaceLimitScope::ManagedDocumentBytes => "managedDocumentBytes",
        WorkspaceLimitScope::AttachmentCandidates => "attachmentCandidates",
        WorkspaceLimitScope::AttachmentBytes => "attachmentBytes",
    );
    closed!(GitSearchSourceKind;
        GitSearchSourceKind::Bundled => "bundled",
        GitSearchSourceKind::System => "system",
        GitSearchSourceKind::GuiPath => "guiPath",
    );
    closed!(GitCredentialKind;
        GitCredentialKind::SshAgent => "sshAgent",
        GitCredentialKind::CredentialHelper => "credentialHelper",
        GitCredentialKind::Unknown => "unknown",
    );
    closed!(GitExitKind;
        GitExitKind::Spawn => "spawn",
        GitExitKind::ExitStatus => "exitStatus",
        GitExitKind::Signal => "signal",
        GitExitKind::Timeout => "timeout",
        GitExitKind::OutputLimit => "outputLimit",
    );
    closed!(GitConflictKind;
        GitConflictKind::Merge => "merge",
        GitConflictKind::Rebase => "rebase",
        GitConflictKind::CherryPick => "cherryPick",
        GitConflictKind::Revert => "revert",
        GitConflictKind::ApplyMailbox => "applyMailbox",
        GitConflictKind::Index => "index",
        GitConflictKind::Unknown => "unknown",
    );
    closed!(ErrorCode;
        ErrorCode::InvalidArgument => "INVALID_ARGUMENT",
        ErrorCode::WorkspaceNotFound => "WORKSPACE_NOT_FOUND",
        ErrorCode::WorkspaceUnavailable => "WORKSPACE_UNAVAILABLE",
        ErrorCode::WorkspaceReadOnly => "WORKSPACE_READ_ONLY",
        ErrorCode::WorkspaceLimitExceeded => "WORKSPACE_LIMIT_EXCEEDED",
        ErrorCode::SchemaUnsupported => "SCHEMA_UNSUPPORTED",
        ErrorCode::InvalidManifest => "INVALID_MANIFEST",
        ErrorCode::InvalidCategory => "INVALID_CATEGORY",
        ErrorCode::InvalidGuide => "INVALID_GUIDE",
        ErrorCode::UnsafeRoundTrip => "UNSAFE_ROUND_TRIP",
        ErrorCode::DuplicateId => "DUPLICATE_ID",
        ErrorCode::CategoryNotFound => "CATEGORY_NOT_FOUND",
        ErrorCode::GuideNotFound => "GUIDE_NOT_FOUND",
        ErrorCode::AttachmentNotFound => "ATTACHMENT_NOT_FOUND",
        ErrorCode::RevisionConflict => "REVISION_CONFLICT",
        ErrorCode::SnapshotConflict => "SNAPSHOT_CONFLICT",
        ErrorCode::PathEscape => "PATH_ESCAPE",
        ErrorCode::SymlinkDenied => "SYMLINK_DENIED",
        ErrorCode::HardlinkDenied => "HARDLINK_DENIED",
        ErrorCode::PermissionDenied => "PERMISSION_DENIED",
        ErrorCode::FileTooLarge => "FILE_TOO_LARGE",
        ErrorCode::IoBusy => "IO_BUSY",
        ErrorCode::IndexNotReady => "INDEX_NOT_READY",
        ErrorCode::PlanStale => "PLAN_STALE",
        ErrorCode::ConfirmationRequired => "CONFIRMATION_REQUIRED",
        ErrorCode::ConfirmationInvalid => "CONFIRMATION_INVALID",
        ErrorCode::TestApprovalRequired => "TEST_APPROVAL_REQUIRED",
        ErrorCode::GitUnavailable => "GIT_UNAVAILABLE",
        ErrorCode::GitUnsafeRepository => "GIT_UNSAFE_REPOSITORY",
        ErrorCode::GitAuthRequired => "GIT_AUTH_REQUIRED",
        ErrorCode::GitDiverged => "GIT_DIVERGED",
        ErrorCode::GitConflict => "GIT_CONFLICT",
        ErrorCode::GitFailed => "GIT_FAILED",
        ErrorCode::MigrationRequired => "MIGRATION_REQUIRED",
        ErrorCode::RecoveryRequired => "RECOVERY_REQUIRED",
        ErrorCode::Internal => "INTERNAL",
    );
}

#[test]
fn every_fraction_width_and_month_boundary_is_checked() {
    for width in 1..=9 {
        let value = format!("2024-02-29T00:00:00.{}1Z", "0".repeat(width - 1));
        assert_eq!(Timestamp::from_str(&value).unwrap().to_string(), value);
    }
    for invalid in [
        "2023-02-29T00:00:00Z",
        "2024-04-31T00:00:00Z",
        "2024-06-31T00:00:00Z",
        "2024-09-31T00:00:00Z",
        "2024-11-31T00:00:00Z",
        "2024-12-32T00:00:00Z",
    ] {
        assert!(Timestamp::from_str(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn bounded_list_rejects_every_inconsistent_wire_shape() {
    for wire in [
        r#"{"items":[],"totalCount":1,"truncated":false}"#,
        r#"{"items":[],"totalCount":33,"truncated":true}"#,
        r#"{"items":[0],"totalCount":0,"truncated":false}"#,
        r#"{"items":[0],"totalCount":1,"truncated":true}"#,
        r#"{"items":[],"totalCount":32,"truncated":false}"#,
        r#"{"items":[],"totalCount":33,"truncated":false}"#,
        r#"{"items":[],"totalCount":4294967296,"truncated":true}"#,
        r#"{"items":[],"items":[],"totalCount":0,"truncated":false}"#,
        r#"{"items":[],"totalCount":0,"totalCount":0,"truncated":false}"#,
        r#"{"items":[],"totalCount":0,"truncated":false,"truncated":false}"#,
    ] {
        assert!(
            serde_json::from_str::<BoundedList<u8>>(wire).is_err(),
            "{wire}"
        );
    }
    let over_retained = format!(
        r#"{{"items":[{}],"totalCount":33,"truncated":true}}"#,
        vec!["0"; 33].join(",")
    );
    assert!(serde_json::from_str::<BoundedList<u8>>(&over_retained).is_err());
}

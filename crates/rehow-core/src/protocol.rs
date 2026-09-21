mod construction;

use crate::domain::ValueConstructionError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::{
    fmt,
    net::{Ipv4Addr, Ipv6Addr},
    str::FromStr,
};
use time::{UtcOffset, format_description::well_known::Rfc3339};
use ts_rs::TS;
use uuid::{Uuid, Variant, Version};

macro_rules! string_ts {
    ($name:ident) => {
        impl TS for $name {
            type WithoutGenerics = Self;
            type OptionInnerType = Self;
            fn name(_: &ts_rs::Config) -> String {
                stringify!($name).into()
            }
            fn inline(_: &ts_rs::Config) -> String {
                "string".into()
            }
            fn decl(_: &ts_rs::Config) -> String {
                format!("type {} = string;", stringify!($name))
            }
            fn decl_concrete(c: &ts_rs::Config) -> String {
                Self::decl(c)
            }
        }
    };
}
macro_rules! string_serde {
    ($name:ident) => {
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_string())
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct StringVisitor;
                impl<'de> serde::de::Visitor<'de> for StringVisitor {
                    type Value = $name;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a canonical string value")
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<$name, E> {
                        value.parse().map_err(E::custom)
                    }
                    fn visit_borrowed_str<E: serde::de::Error>(
                        self,
                        value: &'de str,
                    ) -> Result<$name, E> {
                        self.visit_str(value)
                    }
                }
                d.deserialize_str(StringVisitor)
                    .map_err(|_| D::Error::custom(ValueConstructionError))
            }
        }
    };
}
macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(Uuid);
        string_ts!($name);
        impl FromStr for $name {
            type Err = ValueConstructionError;
            fn from_str(v: &str) -> Result<Self, Self::Err> {
                parse_uuid(v).map(Self)
            }
        }
        impl TryFrom<&str> for $name {
            type Error = ValueConstructionError;
            fn try_from(v: &str) -> Result<Self, Self::Error> {
                v.parse()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.hyphenated())
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
        string_serde!($name);
    };
}
uuid_id!(WorkspaceId);
uuid_id!(CategoryId);
uuid_id!(GuideId);
uuid_id!(AttachmentId);
uuid_id!(RequestId);
uuid_id!(ApprovalRequestId);
uuid_id!(OperationId);
uuid_id!(IncidentId);
fn parse_uuid(v: &str) -> Result<Uuid, ValueConstructionError> {
    if v.len() != 36
        || !v
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f' | b'-'))
    {
        return Err(ValueConstructionError);
    }
    let id = Uuid::parse_str(v).map_err(|_| ValueConstructionError)?;
    if id.get_version() != Some(Version::SortRand)
        || id.get_variant() != Variant::RFC4122
        || id.hyphenated().to_string() != v
    {
        return Err(ValueConstructionError);
    }
    Ok(id)
}

pub(crate) fn new_operation_id() -> OperationId {
    OperationId(Uuid::now_v7())
}

pub fn new_request_id() -> RequestId {
    RequestId(Uuid::now_v7())
}

fn constructed_error(value: Result<ErrorDto, construction::ErrorConstructionError>) -> ErrorDto {
    match value {
        Ok(value) => value,
        Err(_) => unreachable!("fixed public error construction must remain valid"),
    }
}

pub fn workspace_write_required_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_read_only(
        request_id,
        construction::Reason::CapabilityMissing,
        Some(RequiredCapability::Write),
    ))
}

pub fn workspace_broad_required_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_read_only(
        request_id,
        construction::Reason::CapabilityMissing,
        Some(RequiredCapability::Broad),
    ))
}

pub fn guide_not_found_error(request_id: RequestId, guide_id: GuideId) -> ErrorDto {
    constructed_error(construction::guide_not_found(request_id, guide_id))
}

pub fn guide_revision_conflict_error(
    request_id: RequestId,
    guide_id: GuideId,
    expected_revision: Revision,
    actual_revision: Revision,
) -> ErrorDto {
    constructed_error(construction::revision_conflict(
        request_id,
        PortableEntityId::guide(guide_id),
        expected_revision,
        actual_revision,
    ))
}

pub fn invalid_guide_update_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Details,
        construction::Reason::InvalidFormat,
    ))
}

pub fn guide_update_body_limit_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Details,
        construction::Reason::OutOfRange,
    ))
}

pub fn invalid_guide_tags_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Tags,
        construction::Reason::InvalidFormat,
    ))
}

pub fn invalid_guide_aliases_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Aliases,
        construction::Reason::InvalidFormat,
    ))
}

pub fn invalid_guide_category_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::CategoryId,
        construction::Reason::UnsupportedValue,
    ))
}

pub fn guide_change_category_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::GuideChangeCategory,
        2_000,
    ))
}

pub fn guide_update_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::GuideUpdate,
        2_000,
    ))
}

pub fn guide_update_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub fn invalid_guide_lifecycle_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Status,
        construction::Reason::UnsupportedValue,
    ))
}

pub fn guide_archive_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::GuideArchive,
        2_000,
    ))
}

pub fn guide_restore_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::GuideRestore,
        2_000,
    ))
}

pub fn guide_mark_stale_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::GuideChangeStatus,
        2_000,
    ))
}

pub fn guide_lifecycle_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub fn guide_tested_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::GuideChangeStatus,
        2_000,
    ))
}

pub fn guide_tested_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub fn guide_tested_plan_stale_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::plan_stale(
        request_id,
        construction::Operation::GuideChangeStatus,
        BoundedList::try_new(Vec::new(), 0).expect("the fixed empty changed-entity list is valid"),
    ))
}

pub fn guide_tested_plan_expired_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::GuideChangeStatus,
        construction::Reason::PlanExpired,
    ))
}

pub fn guide_tested_plan_replayed_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::GuideChangeStatus,
        construction::Reason::PlanReplayed,
    ))
}

pub fn guide_tested_plan_scope_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::GuideChangeStatus,
        construction::Reason::PlanScopeMismatch,
    ))
}

pub fn guide_tested_cancelled_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::GuideChangeStatus,
        construction::Reason::PlanStateChanged,
    ))
}

pub fn guide_create_category_not_found_error(
    request_id: RequestId,
    category_id: CategoryId,
) -> ErrorDto {
    constructed_error(construction::category_not_found(request_id, category_id))
}

pub fn guide_create_category_revision_conflict_error(
    request_id: RequestId,
    category_id: CategoryId,
    expected_revision: Revision,
    actual_revision: Revision,
) -> ErrorDto {
    constructed_error(construction::revision_conflict(
        request_id,
        PortableEntityId::category(category_id),
        expected_revision,
        actual_revision,
    ))
}

pub fn guide_create_archived_category_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::CategoryId,
        construction::Reason::UnsupportedValue,
    ))
}

pub fn invalid_guide_create_title_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Title,
        construction::Reason::InvalidFormat,
    ))
}

pub fn invalid_guide_create_summary_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Summary,
        construction::Reason::InvalidFormat,
    ))
}

pub fn guide_create_limit_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Details,
        construction::Reason::OutOfRange,
    ))
}

pub fn guide_create_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::GuideCreate,
        2_000,
    ))
}

pub fn guide_create_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub fn category_create_snapshot_conflict_error(
    request_id: RequestId,
    expected_snapshot: Snapshot,
    actual_snapshot: Snapshot,
) -> ErrorDto {
    category_inventory_snapshot_conflict_error(request_id, expected_snapshot, actual_snapshot)
}

pub fn category_inventory_snapshot_conflict_error(
    request_id: RequestId,
    expected_snapshot: Snapshot,
    actual_snapshot: Snapshot,
) -> ErrorDto {
    constructed_error(construction::snapshot_conflict(
        request_id,
        expected_snapshot,
        actual_snapshot,
        BoundedList::try_new(Vec::new(), 0).expect("the fixed empty changed-entity list is valid"),
    ))
}

pub fn invalid_category_create_name_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Name,
        construction::Reason::InvalidFormat,
    ))
}

pub fn invalid_category_create_summary_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Summary,
        construction::Reason::InvalidFormat,
    ))
}

pub fn category_create_limit_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_limit_exceeded(
        request_id,
        U64Decimal::new(1_000),
        U64Decimal::new(1_001),
        WorkspaceLimitScope::Categories,
    ))
}

pub fn category_create_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::CategoryCreate,
        2_000,
    ))
}

pub fn category_create_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub fn category_reorder_invalid_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Details,
        construction::Reason::InvalidFormat,
    ))
}

pub fn category_reorder_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::CategoryReorder,
        2_000,
    ))
}

pub fn category_reorder_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub fn category_reorder_plan_stale_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::plan_stale(
        request_id,
        construction::Operation::CategoryReorder,
        BoundedList::try_new(Vec::new(), 0).expect("the fixed empty changed-entity list is valid"),
    ))
}

pub fn category_reorder_plan_expired_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::CategoryReorder,
        construction::Reason::PlanExpired,
    ))
}

pub fn category_reorder_plan_replayed_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::CategoryReorder,
        construction::Reason::PlanReplayed,
    ))
}

pub fn category_reorder_plan_scope_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::CategoryReorder,
        construction::Reason::PlanScopeMismatch,
    ))
}

pub fn category_reorder_cancelled_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::CategoryReorder,
        construction::Reason::PlanStateChanged,
    ))
}

pub fn category_remove_invalid_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::CategoryId,
        construction::Reason::UnsupportedValue,
    ))
}

pub fn category_remove_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::CategoryRemove,
        2_000,
    ))
}

pub fn category_remove_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub fn category_remove_plan_stale_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::plan_stale(
        request_id,
        construction::Operation::CategoryRemove,
        BoundedList::try_new(Vec::new(), 0).expect("the fixed empty changed-entity list is valid"),
    ))
}

pub fn category_remove_plan_expired_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::CategoryRemove,
        construction::Reason::PlanExpired,
    ))
}

pub fn category_remove_plan_replayed_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::CategoryRemove,
        construction::Reason::PlanReplayed,
    ))
}

pub fn category_remove_plan_scope_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::CategoryRemove,
        construction::Reason::PlanScopeMismatch,
    ))
}

pub fn category_remove_cancelled_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::confirmation_invalid(
        request_id,
        construction::Operation::CategoryRemove,
        construction::Reason::PlanStateChanged,
    ))
}

pub fn category_remove_recovery_required_error(
    request_id: RequestId,
    operation_id: OperationId,
    relative_path: SafeRelativePath,
) -> ErrorDto {
    constructed_error(construction::recovery_required(
        request_id,
        operation_id,
        construction::Reason::JournalStateAmbiguous,
        BoundedList::try_new(vec![relative_path], 1)
            .expect("the fixed single affected path is bounded"),
    ))
}

pub fn category_update_not_found_error(request_id: RequestId, category_id: CategoryId) -> ErrorDto {
    constructed_error(construction::category_not_found(request_id, category_id))
}

pub fn category_update_revision_conflict_error(
    request_id: RequestId,
    category_id: CategoryId,
    expected_revision: Revision,
    actual_revision: Revision,
) -> ErrorDto {
    constructed_error(construction::revision_conflict(
        request_id,
        PortableEntityId::category(category_id),
        expected_revision,
        actual_revision,
    ))
}

pub fn invalid_category_update_name_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Name,
        construction::Reason::InvalidFormat,
    ))
}

pub fn invalid_category_update_summary_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Summary,
        construction::Reason::InvalidFormat,
    ))
}

pub fn invalid_category_update_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Details,
        construction::Reason::InvalidFormat,
    ))
}

pub fn category_update_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::CategoryUpdate,
        2_000,
    ))
}

pub fn category_update_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub fn category_lifecycle_not_found_error(
    request_id: RequestId,
    category_id: CategoryId,
) -> ErrorDto {
    constructed_error(construction::category_not_found(request_id, category_id))
}

pub fn category_lifecycle_revision_conflict_error(
    request_id: RequestId,
    category_id: CategoryId,
    expected_revision: Revision,
    actual_revision: Revision,
) -> ErrorDto {
    constructed_error(construction::revision_conflict(
        request_id,
        PortableEntityId::category(category_id),
        expected_revision,
        actual_revision,
    ))
}

pub fn category_archive_default_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::CategoryId,
        construction::Reason::UnsupportedValue,
    ))
}

pub fn invalid_category_lifecycle_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::invalid_argument(
        request_id,
        construction::Field::Status,
        construction::Reason::UnsupportedValue,
    ))
}

pub fn category_archive_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::CategoryArchive,
        2_000,
    ))
}

pub fn category_restore_busy_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::io_busy(
        request_id,
        construction::Operation::CategoryRestore,
        2_000,
    ))
}

pub fn category_lifecycle_unavailable_error(request_id: RequestId) -> ErrorDto {
    constructed_error(construction::workspace_unavailable(
        request_id,
        construction::Reason::WorkspaceRootChanged,
    ))
}

pub(crate) fn new_workspace_id() -> WorkspaceId {
    WorkspaceId(Uuid::now_v7())
}

pub(crate) fn new_category_id() -> CategoryId {
    CategoryId(Uuid::now_v7())
}

pub(crate) fn new_guide_id() -> GuideId {
    GuideId(Uuid::now_v7())
}

pub(crate) fn new_attachment_id() -> AttachmentId {
    AttachmentId(Uuid::now_v7())
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct RecentWorkspaceId(Uuid);
string_ts!(RecentWorkspaceId);
impl FromStr for RecentWorkspaceId {
    type Err = ValueConstructionError;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        v.strip_prefix("recent:v1:")
            .ok_or(ValueConstructionError)
            .and_then(parse_uuid)
            .map(Self)
    }
}
impl TryFrom<&str> for RecentWorkspaceId {
    type Error = ValueConstructionError;
    fn try_from(v: &str) -> Result<Self, Self::Error> {
        v.parse()
    }
}
impl fmt::Display for RecentWorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "recent:v1:{}", self.0.hyphenated())
    }
}
impl fmt::Debug for RecentWorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecentWorkspaceId(<redacted>)")
    }
}
string_serde!(RecentWorkspaceId);

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Timestamp(time::OffsetDateTime);
string_ts!(Timestamp);
pub(crate) fn current_timestamp() -> Timestamp {
    Timestamp(time::OffsetDateTime::now_utc())
}
impl FromStr for Timestamp {
    type Err = ValueConstructionError;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        let bytes = v.as_bytes();
        if !(20..=30).contains(&bytes.len())
            || v.as_bytes().get(4) != Some(&b'-')
            || v.as_bytes().get(7) != Some(&b'-')
            || v.as_bytes().get(10) != Some(&b'T')
            || v.as_bytes().get(13) != Some(&b':')
            || v.as_bytes().get(16) != Some(&b':')
            || !v.ends_with('Z')
            || !(bytes[0..4]
                .iter()
                .chain(&bytes[5..7])
                .chain(&bytes[8..10])
                .chain(&bytes[11..13])
                .chain(&bytes[14..16])
                .chain(&bytes[17..19]))
            .all(u8::is_ascii_digit)
            || (bytes.len() > 20
                && (bytes[19] != b'.'
                    || !(1..=9).contains(&(bytes.len() - 21))
                    || !bytes[20..bytes.len() - 1].iter().all(u8::is_ascii_digit)
                    || !matches!(bytes[bytes.len() - 2], b'1'..=b'9')))
        {
            return Err(ValueConstructionError);
        }
        if &bytes[0..4] == b"0000" {
            return Err(ValueConstructionError);
        }
        let parsed =
            time::OffsetDateTime::parse(v, &Rfc3339).map_err(|_| ValueConstructionError)?;
        let result = Self(parsed);
        if result.0.offset() != UtcOffset::UTC || result.to_string() != v {
            return Err(ValueConstructionError);
        }
        Ok(result)
    }
}
impl TryFrom<&str> for Timestamp {
    type Error = ValueConstructionError;
    fn try_from(v: &str) -> Result<Self, Self::Error> {
        v.parse()
    }
}
impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.0.date().to_calendar_date();
        let t = self.0.time();
        write!(
            f,
            "{y:04}-{:02}-{d:02}T{:02}:{:02}:{:02}",
            m as u8,
            t.hour(),
            t.minute(),
            t.second()
        )?;
        if t.nanosecond() != 0 {
            let x = format!("{:09}", t.nanosecond());
            write!(f, ".{}", x.trim_end_matches('0'))?
        }
        f.write_str("Z")
    }
}
impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Timestamp(<redacted>)")
    }
}
string_serde!(Timestamp);

macro_rules! decimal {
    ($name:ident,$inner:ty,$signed:expr) => {
        #[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name($inner);
        string_ts!($name);
        impl $name {
            pub fn new(v: $inner) -> Self {
                Self(v)
            }
        }
        impl FromStr for $name {
            type Err = ValueConstructionError;
            fn from_str(v: &str) -> Result<Self, Self::Err> {
                if v.is_empty()
                    || v.starts_with('+')
                    || v.trim_start_matches('-').starts_with('0')
                        && v.trim_start_matches('-').len() > 1
                    || (!$signed && v.starts_with('-'))
                    || v == "-0"
                    || !v
                        .trim_start_matches('-')
                        .bytes()
                        .all(|b| b.is_ascii_digit())
                {
                    return Err(ValueConstructionError);
                }
                v.parse().map(Self).map_err(|_| ValueConstructionError)
            }
        }
        impl TryFrom<&str> for $name {
            type Error = ValueConstructionError;
            fn try_from(v: &str) -> Result<Self, Self::Error> {
                v.parse()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
        string_serde!($name);
    };
}
decimal!(U64Decimal, u64, false);
decimal!(SignedInt64Decimal, i64, true);
fn nibble(b: u8) -> Result<u8, ValueConstructionError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(ValueConstructionError),
    }
}
macro_rules! digest {
    ($name:ident,$prefix:literal) => {
        #[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name([u8; 32]);
        string_ts!($name);
        impl FromStr for $name {
            type Err = ValueConstructionError;
            fn from_str(v: &str) -> Result<Self, Self::Err> {
                let h = v.strip_prefix($prefix).ok_or(ValueConstructionError)?;
                if h.len() != 64 {
                    return Err(ValueConstructionError);
                }
                let mut out = [0; 32];
                for (i, p) in h.as_bytes().chunks_exact(2).enumerate() {
                    out[i] = (nibble(p[0])? << 4) | nibble(p[1])?
                }
                Ok(Self(out))
            }
        }
        impl TryFrom<&str> for $name {
            type Error = ValueConstructionError;
            fn try_from(v: &str) -> Result<Self, Self::Error> {
                v.parse()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($prefix)?;
                for b in self.0 {
                    write!(f, "{b:02x}")?
                }
                Ok(())
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
        string_serde!($name);
    };
}
digest!(Revision, "rev:v1:");
digest!(Snapshot, "snapshot:v1:");
digest!(VerificationHash, "verify:v1:");

impl Revision {
    pub(crate) fn from_digest_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Snapshot {
    pub(crate) fn from_digest_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl VerificationHash {
    pub(crate) fn from_digest_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct PlanToken([u8; 32]);
string_ts!(PlanToken);
impl FromStr for PlanToken {
    type Err = ValueConstructionError;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        let p = v.strip_prefix("plan:v1:").ok_or(ValueConstructionError)?;
        if p.len() != 43
            || !p
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !matches!(
                p.as_bytes()[42],
                b'A' | b'E'
                    | b'I'
                    | b'M'
                    | b'Q'
                    | b'U'
                    | b'Y'
                    | b'c'
                    | b'g'
                    | b'k'
                    | b'o'
                    | b's'
                    | b'w'
                    | b'0'
                    | b'4'
                    | b'8'
            )
        {
            return Err(ValueConstructionError);
        }
        let mut bytes = [0_u8; 32];
        let decoded = URL_SAFE_NO_PAD
            .decode_slice(p, &mut bytes)
            .map_err(|_| ValueConstructionError)?;
        let mut canonical = [0_u8; 43];
        let encoded = URL_SAFE_NO_PAD
            .encode_slice(bytes, &mut canonical)
            .map_err(|_| ValueConstructionError)?;
        if decoded != bytes.len() || encoded != canonical.len() || canonical != p.as_bytes() {
            return Err(ValueConstructionError);
        }
        Ok(Self(bytes))
    }
}
impl PlanToken {
    pub(crate) fn from_random_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}
impl TryFrom<&str> for PlanToken {
    type Error = ValueConstructionError;
    fn try_from(v: &str) -> Result<Self, Self::Error> {
        v.parse()
    }
}
impl fmt::Debug for PlanToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlanToken(<redacted>)")
    }
}
impl Serialize for PlanToken {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut payload = [0_u8; 43];
        let written = URL_SAFE_NO_PAD
            .encode_slice(self.0, &mut payload)
            .map_err(serde::ser::Error::custom)?;
        debug_assert_eq!(written, payload.len());
        let payload = std::str::from_utf8(&payload).map_err(serde::ser::Error::custom)?;
        let mut wire = String::with_capacity(51);
        wire.push_str("plan:v1:");
        wire.push_str(payload);
        s.serialize_str(&wire)
    }
}
impl<'de> Deserialize<'de> for PlanToken {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct TokenVisitor;
        impl<'de> serde::de::Visitor<'de> for TokenVisitor {
            type Value = PlanToken;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a canonical plan token")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<PlanToken, E> {
                value.parse().map_err(E::custom)
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<PlanToken, E> {
                self.visit_str(value)
            }
        }
        d.deserialize_str(TokenVisitor)
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}

macro_rules! token {
    ($name:ident,$validator:expr) => {
        #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(String);
        string_ts!($name);
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct TokenVisitor;
                impl<'de> serde::de::Visitor<'de> for TokenVisitor {
                    type Value = $name;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a canonical protocol token")
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<$name, E> {
                        if ($validator)(value) {
                            Ok($name(value.into()))
                        } else {
                            Err(E::custom(ValueConstructionError))
                        }
                    }
                    fn visit_borrowed_str<E: serde::de::Error>(
                        self,
                        value: &'de str,
                    ) -> Result<$name, E> {
                        self.visit_str(value)
                    }
                }
                d.deserialize_str(TokenVisitor)
                    .map_err(|_| D::Error::custom(ValueConstructionError))
            }
        }
    };
}
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
fn valid_token(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && v.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && !v.ends_with('_')
        && !v.contains("__")
}
token!(SchemaFieldPath, |v: &str| SCHEMA_PATHS.contains(&v));
token!(ReasonCode, valid_token);
token!(DiagnosticCode, valid_token);
token!(OperationCode, valid_token);
token!(GitOperationCode, valid_token);

fn prohibited(c: char) -> bool {
    let n = c as u32;
    n <= 0x1f
        || (0x7f..=0x9f).contains(&n)
        || matches!(n,0x061c|0x200e|0x200f|0x2028..=0x202e|0x2066..=0x2069|0xfeff)
        || (0xfdd0..=0xfdef).contains(&n)
        || n & 0xfffe == 0xfffe
}
fn decode_display(
    v: &str,
    scalar_limit: usize,
    byte_limit: usize,
) -> Result<Vec<u8>, ValueConstructionError> {
    if v.chars().count() > scalar_limit {
        return Err(ValueConstructionError);
    }
    let mut out = Vec::new();
    let b = v.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return Err(ValueConstructionError);
            }
            let h = |x| match x {
                b'0'..=b'9' => Some(x - b'0'),
                b'A'..=b'F' => Some(x - b'A' + 10),
                _ => None,
            };
            if out.len() == byte_limit {
                return Err(ValueConstructionError);
            }
            out.push(
                (h(b[i + 1]).ok_or(ValueConstructionError)? << 4)
                    | h(b[i + 2]).ok_or(ValueConstructionError)?,
            );
            i += 3
        } else {
            let c = v[i..].chars().next().ok_or(ValueConstructionError)?;
            if prohibited(c) {
                return Err(ValueConstructionError);
            }
            let mut buf = [0; 4];
            let encoded = c.encode_utf8(&mut buf).as_bytes();
            if encoded.len() > byte_limit.saturating_sub(out.len()) {
                return Err(ValueConstructionError);
            }
            out.extend_from_slice(encoded);
            i += c.len_utf8()
        }
    }
    Ok(out)
}
fn encode_display(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        match std::str::from_utf8(&bytes[i..]) {
            Ok(s) => {
                for c in s.chars() {
                    if c == '%' || prohibited(c) {
                        for b in c.to_string().bytes() {
                            out.push_str(&format!("%{b:02X}"))
                        }
                    } else {
                        out.push(c)
                    }
                }
                break;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                if valid > 0 {
                    for c in std::str::from_utf8(&bytes[i..i + valid])
                        .expect("valid prefix")
                        .chars()
                    {
                        if c == '%' || prohibited(c) {
                            for b in c.to_string().bytes() {
                                out.push_str(&format!("%{b:02X}"));
                            }
                        } else {
                            out.push(c);
                        }
                    }
                    i += valid
                } else {
                    out.push_str(&format!("%{:02X}", bytes[i]));
                    i += 1
                }
            }
        }
    }
    out
}
macro_rules! safe_display {
    ($name:ident,$validator:expr) => {
        #[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(String);
        string_ts!($name);
        impl $name {
            pub fn parse_wire(v: &str) -> Result<Self, ValueConstructionError> {
                ($validator)(v).map(Self)
            }
        }
        impl FromStr for $name {
            type Err = ValueConstructionError;
            fn from_str(v: &str) -> Result<Self, Self::Err> {
                Self::parse_wire(v)
            }
        }
        impl TryFrom<&str> for $name {
            type Error = ValueConstructionError;
            fn try_from(v: &str) -> Result<Self, Self::Error> {
                Self::parse_wire(v)
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
        string_serde!($name);
    };
}
fn relative(v: &str) -> Result<String, ValueConstructionError> {
    let b = decode_display(v, 3072, 1024)?;
    if b.is_empty()
        || b.split(|x| *x == b'/')
            .any(|s| s.is_empty() || s == b"." || s == b"..")
        || encode_display(&b) != v
    {
        Err(ValueConstructionError)
    } else {
        Ok(v.into())
    }
}
fn git_display(v: &str) -> Result<String, ValueConstructionError> {
    let b = decode_display(v, 768, 3072)?;
    if b.is_empty() || encode_display(&b) != v {
        Err(ValueConstructionError)
    } else {
        Ok(v.into())
    }
}
safe_display!(SafeRelativePath, relative);
safe_display!(SafeGitDisplay, git_display);

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SafeHost(String);
string_ts!(SafeHost);
impl SafeHost {
    pub fn parse_wire(v: &str) -> Result<Self, ValueConstructionError> {
        if v == "unknown" {
            return Ok(Self(v.into()));
        }
        if let Ok(ip) = v.parse::<Ipv4Addr>() {
            return if ip.to_string() == v {
                Ok(Self(v.into()))
            } else {
                Err(ValueConstructionError)
            };
        }
        if let Ok(ip) = v.parse::<Ipv6Addr>() {
            return if ip.to_string() == v {
                Ok(Self(v.into()))
            } else {
                Err(ValueConstructionError)
            };
        }
        if v.split('.').all(|label| {
            (!label.is_empty() && label.bytes().all(|b| b.is_ascii_digit()))
                || label.strip_prefix("0x").is_some_and(|hex| {
                    !hex.is_empty()
                        && hex
                            .bytes()
                            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
                })
        }) {
            return Err(ValueConstructionError);
        }
        if v.len() > 253
            || v.is_empty()
            || v.ends_with('.')
            || v.split('.').any(|l| {
                l.is_empty()
                    || l.len() > 63
                    || !l
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    || l.starts_with('-')
                    || l.ends_with('-')
            })
        {
            Err(ValueConstructionError)
        } else {
            Ok(Self(v.into()))
        }
    }
}
impl FromStr for SafeHost {
    type Err = ValueConstructionError;
    fn from_str(v: &str) -> Result<Self, Self::Err> {
        Self::parse_wire(v)
    }
}
impl TryFrom<&str> for SafeHost {
    type Error = ValueConstructionError;
    fn try_from(v: &str) -> Result<Self, Self::Error> {
        Self::parse_wire(v)
    }
}
impl fmt::Display for SafeHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl fmt::Debug for SafeHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SafeHost(<redacted>)")
    }
}
string_serde!(SafeHost);

#[derive(Clone, Eq, PartialEq)]
pub struct PublicText(String);
string_ts!(PublicText);
impl PublicText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for PublicText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PublicText(<redacted>)")
    }
}
impl Serialize for PublicText {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

macro_rules! closed {
    ($name:ident{$($variant:ident=>$wire:literal),+$(,)?}) => {
        #[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, TS)]
        #[serde(rename_all = "camelCase")]
        #[ts(rename_all = "camelCase")]
        pub enum $name { $($variant),+ }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> serde::de::Visitor<'de> for V {
                    type Value = $name;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a closed protocol value")
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<$name, E> {
                        match value {
                            $($wire => Ok($name::$variant),)+
                            _ => Err(E::custom(ValueConstructionError)),
                        }
                    }
                    fn visit_borrowed_str<E: serde::de::Error>(
                        self,
                        value: &'de str,
                    ) -> Result<$name, E> {
                        self.visit_str(value)
                    }
                }
                d.deserialize_str(V)
                    .map_err(|_| D::Error::custom(ValueConstructionError))
            }
        }
    };
}
closed!(RequiredCapability {
    Write => "write",
    Broad => "broad",
    GitWrite => "gitWrite"
});
closed!(WorkspaceLimitScope {
    Categories => "categories",
    Guides => "guides",
    ManagedDocumentBytes => "managedDocumentBytes",
    AttachmentCandidates => "attachmentCandidates",
    AttachmentBytes => "attachmentBytes"
});
closed!(GitSearchSourceKind {
    Bundled => "bundled",
    System => "system",
    GuiPath => "guiPath"
});
closed!(GitCredentialKind {
    SshAgent => "sshAgent",
    CredentialHelper => "credentialHelper",
    Unknown => "unknown"
});
closed!(GitExitKind {
    Spawn => "spawn",
    ExitStatus => "exitStatus",
    Signal => "signal",
    Timeout => "timeout",
    OutputLimit => "outputLimit"
});
closed!(GitConflictKind {
    Merge => "merge",
    Rebase => "rebase",
    CherryPick => "cherryPick",
    Revert => "revert",
    ApplyMailbox => "applyMailbox",
    Index => "index",
    Unknown => "unknown"
});

#[derive(Clone, Eq, PartialEq)]
enum PortableEntityRepresentation {
    Workspace(WorkspaceId),
    Category(CategoryId),
    Guide(GuideId),
    Attachment(AttachmentId),
}
#[derive(Clone, Eq, PartialEq)]
pub struct PortableEntityId(PortableEntityRepresentation);
#[expect(dead_code, reason = "private ts-rs correlation marker")]
#[derive(TS)]
#[ts(tag = "kind", content = "id", rename_all = "camelCase")]
enum PortableEntityContract {
    Workspace(WorkspaceId),
    Category(CategoryId),
    Guide(GuideId),
    Attachment(AttachmentId),
}
impl TS for PortableEntityId {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        "PortableEntityId".into()
    }
    fn inline(config: &ts_rs::Config) -> String {
        PortableEntityContract::inline(config)
    }
    fn decl(c: &ts_rs::Config) -> String {
        format!("type PortableEntityId = {};", Self::inline(c))
    }
    fn decl_concrete(c: &ts_rs::Config) -> String {
        Self::decl(c)
    }
}
impl PortableEntityId {
    pub fn workspace(v: WorkspaceId) -> Self {
        Self(PortableEntityRepresentation::Workspace(v))
    }
    pub fn category(v: CategoryId) -> Self {
        Self(PortableEntityRepresentation::Category(v))
    }
    pub fn guide(v: GuideId) -> Self {
        Self(PortableEntityRepresentation::Guide(v))
    }
    pub fn attachment(v: AttachmentId) -> Self {
        Self(PortableEntityRepresentation::Attachment(v))
    }
}
#[derive(Serialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "camelCase",
    deny_unknown_fields
)]
enum PortableEntityWire {
    Workspace(WorkspaceId),
    Category(CategoryId),
    Guide(GuideId),
    Attachment(AttachmentId),
}
impl Serialize for PortableEntityId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            PortableEntityRepresentation::Workspace(v) => PortableEntityWire::Workspace(*v),
            PortableEntityRepresentation::Category(v) => PortableEntityWire::Category(*v),
            PortableEntityRepresentation::Guide(v) => PortableEntityWire::Guide(*v),
            PortableEntityRepresentation::Attachment(v) => PortableEntityWire::Attachment(*v),
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for PortableEntityId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        enum Field {
            Kind,
            Id,
        }
        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl serde::de::Visitor<'_> for V {
                    type Value = Field;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a portable entity field")
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Field, E> {
                        match value {
                            "kind" => Ok(Field::Kind),
                            "id" => Ok(Field::Id),
                            _ => Err(E::custom(ValueConstructionError)),
                        }
                    }
                }
                d.deserialize_identifier(V)
            }
        }
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = PortableEntityId;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a portable entity")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut kind = None;
                let mut id = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Kind if kind.is_none() => kind = Some(map.next_value()?),
                        Field::Id if id.is_none() => id = Some(map.next_value()?),
                        Field::Kind => return Err(A::Error::duplicate_field("kind")),
                        Field::Id => return Err(A::Error::duplicate_field("id")),
                    }
                }
                entity(
                    kind.ok_or_else(|| A::Error::missing_field("kind"))?,
                    id.ok_or_else(|| A::Error::missing_field("id"))?,
                )
                .map_err(A::Error::custom)
            }
        }
        d.deserialize_map(V)
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}
#[derive(Clone, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct BoundedList<T> {
    items: Vec<T>,
    total_count: u32,
    truncated: bool,
}
impl<T> BoundedList<T> {
    pub fn try_new(items: Vec<T>, total_count: u32) -> Result<Self, ValueConstructionError> {
        if items.len() != usize::try_from(total_count).unwrap_or(usize::MAX).min(32) {
            return Err(ValueConstructionError);
        }
        Ok(Self {
            truncated: total_count > items.len() as u32,
            items,
            total_count,
        })
    }
    pub fn items(&self) -> &[T] {
        &self.items
    }
    pub fn total_count(&self) -> u32 {
        self.total_count
    }
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for BoundedList<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct RetainedItems<T>(Vec<T>);
        impl<'de, T: Deserialize<'de>> Deserialize<'de> for RetainedItems<T> {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct ItemsVisitor<T>(core::marker::PhantomData<T>);
                impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ItemsVisitor<T> {
                    type Value = RetainedItems<T>;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("at most 32 retained items")
                    }
                    fn visit_seq<A: serde::de::SeqAccess<'de>>(
                        self,
                        mut seq: A,
                    ) -> Result<Self::Value, A::Error> {
                        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(32));
                        while items.len() < 32 {
                            let Some(item) = seq.next_element()? else {
                                return Ok(RetainedItems(items));
                            };
                            items.push(item);
                        }
                        struct RejectAny;
                        impl<'de> serde::de::DeserializeSeed<'de> for RejectAny {
                            type Value = ();
                            fn deserialize<D: Deserializer<'de>>(
                                self,
                                _: D,
                            ) -> Result<(), D::Error> {
                                Err(D::Error::custom("more than 32 retained items"))
                            }
                        }
                        if seq.next_element_seed(RejectAny)?.is_some() {
                            unreachable!()
                        }
                        Ok(RetainedItems(items))
                    }
                }
                d.deserialize_seq(ItemsVisitor(core::marker::PhantomData))
            }
        }
        struct ListVisitor<T>(core::marker::PhantomData<T>);
        enum ListField {
            Items,
            TotalCount,
            Truncated,
        }
        impl<'de> Deserialize<'de> for ListField {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct FieldVisitor;
                impl serde::de::Visitor<'_> for FieldVisitor {
                    type Value = ListField;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a bounded-list field")
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<ListField, E> {
                        match value {
                            "items" => Ok(ListField::Items),
                            "totalCount" => Ok(ListField::TotalCount),
                            "truncated" => Ok(ListField::Truncated),
                            _ => Err(E::custom(ValueConstructionError)),
                        }
                    }
                    fn visit_borrowed_str<E: serde::de::Error>(
                        self,
                        value: &str,
                    ) -> Result<ListField, E> {
                        self.visit_str(value)
                    }
                }
                d.deserialize_identifier(FieldVisitor)
            }
        }
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ListVisitor<T> {
            type Value = BoundedList<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a bounded-list object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let (mut items, mut total, mut truncated) = (None, None, None);
                while let Some(key) = map.next_key::<ListField>()? {
                    match key {
                        ListField::Items if items.is_none() => {
                            items = Some(map.next_value::<RetainedItems<T>>()?.0)
                        }
                        ListField::TotalCount if total.is_none() => {
                            total = Some(map.next_value::<u32>()?)
                        }
                        ListField::Truncated if truncated.is_none() => {
                            truncated = Some(map.next_value::<bool>()?)
                        }
                        ListField::Items => {
                            return Err(serde::de::Error::duplicate_field("items"));
                        }
                        ListField::TotalCount => {
                            return Err(serde::de::Error::duplicate_field("totalCount"));
                        }
                        ListField::Truncated => {
                            return Err(serde::de::Error::duplicate_field("truncated"));
                        }
                    }
                }
                let total = total.ok_or_else(|| serde::de::Error::missing_field("totalCount"))?;
                let expected_truncated = total > 32;
                let value = BoundedList::try_new(
                    items.ok_or_else(|| serde::de::Error::missing_field("items"))?,
                    total,
                )
                .map_err(A::Error::custom)?;
                if truncated.ok_or_else(|| serde::de::Error::missing_field("truncated"))?
                    != expected_truncated
                {
                    return Err(A::Error::custom(ValueConstructionError));
                }
                Ok(value)
            }
        }
        d.deserialize_map(ListVisitor(core::marker::PhantomData))
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}
pub struct SourceLocation {
    line: u32,
    column: u32,
}

macro_rules! details { ($($name:ident,$data:ident{$($field:ident:$ty:ty),*$(,)?}),*$(,)?) => {$ (
    #[expect(dead_code, reason="public TypeScript contract marker has intentionally private fields")]
    #[derive(TS)]
    #[ts(rename_all="camelCase")]
    pub struct $name { $( $field:$ty ),* }
    #[derive(Clone,Eq,PartialEq,Serialize)]
    #[serde(rename_all="camelCase")]
    struct $data { $( $field:$ty ),* }
)*}; }
details!(
InvalidArgumentDetails,InvalidArgumentData{field:SchemaFieldPath,reason:ReasonCode},WorkspaceNotFoundDetails,WorkspaceNotFoundData{recent_workspace_id:RecentWorkspaceId},WorkspaceUnavailableDetails,WorkspaceUnavailableData{reason:ReasonCode},WorkspaceLimitExceededDetails,WorkspaceLimitExceededData{limit:U64Decimal,observed_at_least:U64Decimal,scope:WorkspaceLimitScope},SchemaUnsupportedDetails,SchemaUnsupportedData{found_version:u32,minimum_version:u32,maximum_version:u32},CategoryNotFoundDetails,CategoryNotFoundData{category_id:CategoryId},GuideNotFoundDetails,GuideNotFoundData{guide_id:GuideId},AttachmentNotFoundDetails,AttachmentNotFoundData{attachment_id:AttachmentId,guide_id:GuideId},SnapshotConflictDetails,SnapshotConflictData{expected_snapshot:Snapshot,actual_snapshot:Snapshot,changed_entity_ids:BoundedList<PortableEntityId>},SymlinkDeniedDetails,SymlinkDeniedData{relative_path:SafeRelativePath},HardlinkDeniedDetails,HardlinkDeniedData{relative_path:SafeRelativePath,link_count:U64Decimal},FileTooLargeDetails,FileTooLargeData{relative_path:SafeRelativePath,limit_bytes:U64Decimal,observed_at_least_bytes:U64Decimal},IoBusyDetails,IoBusyData{operation:OperationCode,timeout_ms:u32},PlanStaleDetails,PlanStaleData{operation:OperationCode,changed_entity_ids:BoundedList<PortableEntityId>},ConfirmationRequiredDetails,ConfirmationRequiredData{operation:OperationCode,plan_token:PlanToken,expires_at:Timestamp},ConfirmationInvalidDetails,ConfirmationInvalidData{operation:OperationCode,reason:ReasonCode},TestApprovalRequiredDetails,TestApprovalRequiredData{guide_id:GuideId,guide_revision:Revision,approval_request_id:ApprovalRequestId},GitUnavailableDetails,GitUnavailableData{search_source_kinds:BoundedList<GitSearchSourceKind>},GitUnsafeRepositoryDetails,GitUnsafeRepositoryData{reason:ReasonCode},GitAuthRequiredDetails,GitAuthRequiredData{remote_name:SafeGitDisplay,host:SafeHost,credential_kind:GitCredentialKind},GitDivergedDetails,GitDivergedData{ahead:U64Decimal,behind:U64Decimal,branch:SafeGitDisplay},GitConflictDetails,GitConflictData{conflict_kind:GitConflictKind,relative_paths:BoundedList<SafeRelativePath>},MigrationRequiredDetails,MigrationRequiredData{found_version:u32,target_version:u32},RecoveryRequiredDetails,RecoveryRequiredData{operation_id:OperationId,reason:ReasonCode,affected_relative_paths:BoundedList<SafeRelativePath>},InternalDetails,InternalData{incident_id:IncidentId});
macro_rules! optional_details { ($name:ident,$data:ident{$($field:ident:$ty:ty),*};{$($optional:ident:$oty:ty),*}) => {
    #[expect(dead_code, reason="public TypeScript contract marker has intentionally private fields")]
    #[derive(TS)]
    #[ts(rename_all="camelCase")]
    pub struct $name { $( $field:$ty, )* $( #[ts(optional)] $optional:Option<$oty>, )* }
    #[derive(Clone,Eq,PartialEq,Serialize)]
    #[serde(rename_all="camelCase")]
    struct $data { $( $field:$ty, )* $( #[serde(default,skip_serializing_if="Option::is_none")] $optional:Option<$oty>, )* }
}; }
optional_details!(WorkspaceReadOnlyDetails,WorkspaceReadOnlyData{reason:ReasonCode};{required_capability:RequiredCapability});
optional_details!(UnsafeRoundTripDetails,UnsafeRoundTripData{relative_path:SafeRelativePath,reason:ReasonCode};{field:SchemaFieldPath});
optional_details!(PathEscapeDetails,PathEscapeData{reason:ReasonCode};{relative_path:SafeRelativePath});
optional_details!(PermissionDeniedDetails,PermissionDeniedData{operation:OperationCode};{relative_path:SafeRelativePath});
optional_details!(IndexNotReadyDetails,IndexNotReadyData{index_generation:U64Decimal,progress_completed:u32};{progress_total:u32});
macro_rules! located_details {
    ($name:ident,$data:ident,$marker:ident,$extra:ident:$extra_ty:ty) => {
        #[expect(
            dead_code,
            reason = "public TypeScript contract marker has intentionally private fields"
        )]
        pub struct $name {
            relative_path: SafeRelativePath,
            diagnostic_code: DiagnosticCode,
            field: Option<SchemaFieldPath>,
            line: Option<u32>,
            column: Option<u32>,
            $extra: Option<$extra_ty>,
        }
        #[expect(dead_code, reason = "private ts-rs correlation marker")]
        #[derive(TS)]
        #[ts(rename_all = "camelCase")]
        struct $marker {
            relative_path: SafeRelativePath,
            diagnostic_code: DiagnosticCode,
            #[ts(optional)]
            field: Option<SchemaFieldPath>,
            #[ts(optional)]
            $extra: Option<$extra_ty>,
            #[ts(flatten)]
            location: LocationContract,
        }
        #[derive(Clone, Eq, PartialEq, Serialize)]
        #[serde(rename_all = "camelCase")]
        struct $data {
            relative_path: SafeRelativePath,
            diagnostic_code: DiagnosticCode,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            field: Option<SchemaFieldPath>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            line: Option<u32>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            column: Option<u32>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            $extra: Option<$extra_ty>,
        }
        impl TS for $name {
            type WithoutGenerics = Self;
            type OptionInnerType = Self;
            fn name(_: &ts_rs::Config) -> String {
                stringify!($name).into()
            }
            fn inline(config: &ts_rs::Config) -> String {
                $marker::inline(config)
            }
            fn decl(c: &ts_rs::Config) -> String {
                format!("type {} = {};", stringify!($name), Self::inline(c))
            }
            fn decl_concrete(c: &ts_rs::Config) -> String {
                Self::decl(c)
            }
        }
    };
}

struct Never;
impl TS for Never {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        "never".into()
    }
    fn inline(_: &ts_rs::Config) -> String {
        "never".into()
    }
    fn decl(_: &ts_rs::Config) -> String {
        String::new()
    }
    fn decl_concrete(_: &ts_rs::Config) -> String {
        String::new()
    }
}
#[expect(dead_code, reason = "private ts-rs correlation marker")]
#[derive(TS)]
#[ts(untagged)]
enum LocationContract {
    Located {
        line: u32,
        column: u32,
    },
    Unlocated {
        #[ts(optional)]
        line: Option<Never>,
        #[ts(optional)]
        column: Option<Never>,
    },
}

#[expect(
    dead_code,
    reason = "public TypeScript contract marker has intentionally private fields"
)]
pub struct InvalidManifestDetails {
    relative_path: SafeRelativePath,
    diagnostic_code: DiagnosticCode,
    field: Option<SchemaFieldPath>,
    line: Option<u32>,
    column: Option<u32>,
}
#[expect(dead_code, reason = "private ts-rs correlation marker")]
#[derive(TS)]
#[ts(rename_all = "camelCase")]
struct InvalidManifestContract {
    relative_path: SafeRelativePath,
    diagnostic_code: DiagnosticCode,
    #[ts(optional)]
    field: Option<SchemaFieldPath>,
    #[ts(flatten)]
    location: LocationContract,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvalidManifestData {
    relative_path: SafeRelativePath,
    diagnostic_code: DiagnosticCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    field: Option<SchemaFieldPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    column: Option<u32>,
}
impl TS for InvalidManifestDetails {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        "InvalidManifestDetails".into()
    }
    fn inline(config: &ts_rs::Config) -> String {
        InvalidManifestContract::inline(config)
    }
    fn decl(c: &ts_rs::Config) -> String {
        format!("type InvalidManifestDetails = {};", Self::inline(c))
    }
    fn decl_concrete(c: &ts_rs::Config) -> String {
        Self::decl(c)
    }
}
located_details!(InvalidCategoryDetails,InvalidCategoryData,InvalidCategoryContract,category_id:CategoryId);
located_details!(InvalidGuideDetails,InvalidGuideData,InvalidGuideContract,guide_id:GuideId);

#[expect(
    dead_code,
    reason = "public TypeScript contract marker has intentionally private fields"
)]
pub struct DuplicateIdDetails {
    entity: PortableEntityId,
    relative_paths: BoundedList<SafeRelativePath>,
}
#[expect(dead_code, reason = "private ts-rs correlation marker")]
#[derive(TS)]
#[ts(tag = "entityKind", content = "entityId", rename_all = "camelCase")]
enum EntityDetailsContract {
    Workspace(WorkspaceId),
    Category(CategoryId),
    Guide(GuideId),
    Attachment(AttachmentId),
}
#[expect(dead_code, reason = "private ts-rs correlation marker")]
#[derive(TS)]
#[ts(rename_all = "camelCase")]
struct DuplicateIdContract {
    relative_paths: BoundedList<SafeRelativePath>,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateIdData {
    #[serde(flatten)]
    entity: PortableEntityId,
    relative_paths: BoundedList<SafeRelativePath>,
}
impl TS for DuplicateIdDetails {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        "DuplicateIdDetails".into()
    }
    fn inline(config: &ts_rs::Config) -> String {
        format!(
            "({}) & {}",
            EntityDetailsContract::inline(config),
            DuplicateIdContract::inline(config)
        )
    }
    fn decl(c: &ts_rs::Config) -> String {
        format!("type DuplicateIdDetails = {};", Self::inline(c))
    }
    fn decl_concrete(c: &ts_rs::Config) -> String {
        Self::decl(c)
    }
}
#[expect(
    dead_code,
    reason = "public TypeScript contract marker has intentionally private fields"
)]
pub struct RevisionConflictDetails {
    entity: PortableEntityId,
    expected_revision: Revision,
    actual_revision: Revision,
}
#[expect(dead_code, reason = "private ts-rs correlation marker")]
#[derive(TS)]
#[ts(rename_all = "camelCase")]
struct RevisionConflictContract {
    expected_revision: Revision,
    actual_revision: Revision,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RevisionConflictData {
    #[serde(flatten)]
    entity: PortableEntityId,
    expected_revision: Revision,
    actual_revision: Revision,
}
impl TS for RevisionConflictDetails {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        "RevisionConflictDetails".into()
    }
    fn inline(config: &ts_rs::Config) -> String {
        format!(
            "({}) & {}",
            EntityDetailsContract::inline(config),
            RevisionConflictContract::inline(config)
        )
    }
    fn decl(c: &ts_rs::Config) -> String {
        format!("type RevisionConflictDetails = {};", Self::inline(c))
    }
    fn decl_concrete(c: &ts_rs::Config) -> String {
        Self::decl(c)
    }
}
#[expect(
    dead_code,
    reason = "public TypeScript contract marker has intentionally private fields"
)]
#[derive(TS)]
#[ts(rename_all = "camelCase")]
pub struct GitFailedDetails {
    operation: GitOperationCode,
    exit_kind: GitExitKind,
    sanitized_reason: PublicText,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitFailedData {
    operation: GitOperationCode,
    exit_kind: GitExitKind,
    sanitized_reason: PublicText,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidArgument,
    WorkspaceNotFound,
    WorkspaceUnavailable,
    WorkspaceReadOnly,
    WorkspaceLimitExceeded,
    SchemaUnsupported,
    InvalidManifest,
    InvalidCategory,
    InvalidGuide,
    UnsafeRoundTrip,
    DuplicateId,
    CategoryNotFound,
    GuideNotFound,
    AttachmentNotFound,
    RevisionConflict,
    SnapshotConflict,
    PathEscape,
    SymlinkDenied,
    HardlinkDenied,
    PermissionDenied,
    FileTooLarge,
    IoBusy,
    IndexNotReady,
    PlanStale,
    ConfirmationRequired,
    ConfirmationInvalid,
    TestApprovalRequired,
    GitUnavailable,
    GitUnsafeRepository,
    GitAuthRequired,
    GitDiverged,
    GitConflict,
    GitFailed,
    MigrationRequired,
    RecoveryRequired,
    Internal,
}
impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = ErrorCode;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an error code")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<ErrorCode, E> {
                match value {
                    "INVALID_ARGUMENT" => Ok(ErrorCode::InvalidArgument),
                    "WORKSPACE_NOT_FOUND" => Ok(ErrorCode::WorkspaceNotFound),
                    "WORKSPACE_UNAVAILABLE" => Ok(ErrorCode::WorkspaceUnavailable),
                    "WORKSPACE_READ_ONLY" => Ok(ErrorCode::WorkspaceReadOnly),
                    "WORKSPACE_LIMIT_EXCEEDED" => Ok(ErrorCode::WorkspaceLimitExceeded),
                    "SCHEMA_UNSUPPORTED" => Ok(ErrorCode::SchemaUnsupported),
                    "INVALID_MANIFEST" => Ok(ErrorCode::InvalidManifest),
                    "INVALID_CATEGORY" => Ok(ErrorCode::InvalidCategory),
                    "INVALID_GUIDE" => Ok(ErrorCode::InvalidGuide),
                    "UNSAFE_ROUND_TRIP" => Ok(ErrorCode::UnsafeRoundTrip),
                    "DUPLICATE_ID" => Ok(ErrorCode::DuplicateId),
                    "CATEGORY_NOT_FOUND" => Ok(ErrorCode::CategoryNotFound),
                    "GUIDE_NOT_FOUND" => Ok(ErrorCode::GuideNotFound),
                    "ATTACHMENT_NOT_FOUND" => Ok(ErrorCode::AttachmentNotFound),
                    "REVISION_CONFLICT" => Ok(ErrorCode::RevisionConflict),
                    "SNAPSHOT_CONFLICT" => Ok(ErrorCode::SnapshotConflict),
                    "PATH_ESCAPE" => Ok(ErrorCode::PathEscape),
                    "SYMLINK_DENIED" => Ok(ErrorCode::SymlinkDenied),
                    "HARDLINK_DENIED" => Ok(ErrorCode::HardlinkDenied),
                    "PERMISSION_DENIED" => Ok(ErrorCode::PermissionDenied),
                    "FILE_TOO_LARGE" => Ok(ErrorCode::FileTooLarge),
                    "IO_BUSY" => Ok(ErrorCode::IoBusy),
                    "INDEX_NOT_READY" => Ok(ErrorCode::IndexNotReady),
                    "PLAN_STALE" => Ok(ErrorCode::PlanStale),
                    "CONFIRMATION_REQUIRED" => Ok(ErrorCode::ConfirmationRequired),
                    "CONFIRMATION_INVALID" => Ok(ErrorCode::ConfirmationInvalid),
                    "TEST_APPROVAL_REQUIRED" => Ok(ErrorCode::TestApprovalRequired),
                    "GIT_UNAVAILABLE" => Ok(ErrorCode::GitUnavailable),
                    "GIT_UNSAFE_REPOSITORY" => Ok(ErrorCode::GitUnsafeRepository),
                    "GIT_AUTH_REQUIRED" => Ok(ErrorCode::GitAuthRequired),
                    "GIT_DIVERGED" => Ok(ErrorCode::GitDiverged),
                    "GIT_CONFLICT" => Ok(ErrorCode::GitConflict),
                    "GIT_FAILED" => Ok(ErrorCode::GitFailed),
                    "MIGRATION_REQUIRED" => Ok(ErrorCode::MigrationRequired),
                    "RECOVERY_REQUIRED" => Ok(ErrorCode::RecoveryRequired),
                    "INTERNAL" => Ok(ErrorCode::Internal),
                    _ => Err(E::custom(ValueConstructionError)),
                }
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<ErrorCode, E> {
                self.visit_str(value)
            }
        }
        d.deserialize_str(V)
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}
#[derive(Clone, Eq, PartialEq)]
enum ErrorKind {
    InvalidArgument(InvalidArgumentData),
    WorkspaceNotFound(WorkspaceNotFoundData),
    WorkspaceUnavailable(WorkspaceUnavailableData),
    WorkspaceReadOnly(WorkspaceReadOnlyData),
    WorkspaceLimitExceeded(WorkspaceLimitExceededData),
    SchemaUnsupported(SchemaUnsupportedData),
    InvalidManifest(InvalidManifestData),
    InvalidCategory(InvalidCategoryData),
    InvalidGuide(InvalidGuideData),
    UnsafeRoundTrip(UnsafeRoundTripData),
    DuplicateId(DuplicateIdData),
    CategoryNotFound(CategoryNotFoundData),
    GuideNotFound(GuideNotFoundData),
    AttachmentNotFound(AttachmentNotFoundData),
    RevisionConflict(RevisionConflictData),
    SnapshotConflict(SnapshotConflictData),
    PathEscape(PathEscapeData),
    SymlinkDenied(SymlinkDeniedData),
    HardlinkDenied(HardlinkDeniedData),
    PermissionDenied(PermissionDeniedData),
    FileTooLarge(FileTooLargeData),
    IoBusy(IoBusyData),
    IndexNotReady(IndexNotReadyData),
    PlanStale(PlanStaleData),
    ConfirmationRequired(ConfirmationRequiredData),
    ConfirmationInvalid(ConfirmationInvalidData),
    TestApprovalRequired(TestApprovalRequiredData),
    GitUnavailable(GitUnavailableData),
    GitUnsafeRepository(GitUnsafeRepositoryData),
    GitAuthRequired(GitAuthRequiredData),
    GitDiverged(GitDivergedData),
    GitConflict(GitConflictData),
    GitFailed(GitFailedData),
    MigrationRequired(MigrationRequiredData),
    RecoveryRequired(RecoveryRequiredData),
    Internal(InternalData),
}
#[derive(Clone, Eq, PartialEq)]
pub struct ErrorDto {
    version: u8,
    kind: ErrorKind,
    message: PublicText,
    retryable: bool,
    request_id: RequestId,
    recovery: PublicText,
}
impl TS for ErrorDto {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        "ErrorDto".into()
    }
    fn inline(_: &ts_rs::Config) -> String {
        "ErrorEnvelope & ErrorVariant".into()
    }
    fn decl(c: &ts_rs::Config) -> String {
        format!("type ErrorDto = {};", Self::inline(c))
    }
    fn decl_concrete(c: &ts_rs::Config) -> String {
        Self::decl(c)
    }
}
impl fmt::Debug for ErrorDto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ErrorDto {{ code: {}, request_id: {:?} }}",
            self.code().wire(),
            self.request_id
        )
    }
}
impl ErrorCode {
    fn wire(self) -> &'static str {
        match self {
            Self::InvalidArgument => "INVALID_ARGUMENT",
            Self::WorkspaceNotFound => "WORKSPACE_NOT_FOUND",
            Self::WorkspaceUnavailable => "WORKSPACE_UNAVAILABLE",
            Self::WorkspaceReadOnly => "WORKSPACE_READ_ONLY",
            Self::WorkspaceLimitExceeded => "WORKSPACE_LIMIT_EXCEEDED",
            Self::SchemaUnsupported => "SCHEMA_UNSUPPORTED",
            Self::InvalidManifest => "INVALID_MANIFEST",
            Self::InvalidCategory => "INVALID_CATEGORY",
            Self::InvalidGuide => "INVALID_GUIDE",
            Self::UnsafeRoundTrip => "UNSAFE_ROUND_TRIP",
            Self::DuplicateId => "DUPLICATE_ID",
            Self::CategoryNotFound => "CATEGORY_NOT_FOUND",
            Self::GuideNotFound => "GUIDE_NOT_FOUND",
            Self::AttachmentNotFound => "ATTACHMENT_NOT_FOUND",
            Self::RevisionConflict => "REVISION_CONFLICT",
            Self::SnapshotConflict => "SNAPSHOT_CONFLICT",
            Self::PathEscape => "PATH_ESCAPE",
            Self::SymlinkDenied => "SYMLINK_DENIED",
            Self::HardlinkDenied => "HARDLINK_DENIED",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::FileTooLarge => "FILE_TOO_LARGE",
            Self::IoBusy => "IO_BUSY",
            Self::IndexNotReady => "INDEX_NOT_READY",
            Self::PlanStale => "PLAN_STALE",
            Self::ConfirmationRequired => "CONFIRMATION_REQUIRED",
            Self::ConfirmationInvalid => "CONFIRMATION_INVALID",
            Self::TestApprovalRequired => "TEST_APPROVAL_REQUIRED",
            Self::GitUnavailable => "GIT_UNAVAILABLE",
            Self::GitUnsafeRepository => "GIT_UNSAFE_REPOSITORY",
            Self::GitAuthRequired => "GIT_AUTH_REQUIRED",
            Self::GitDiverged => "GIT_DIVERGED",
            Self::GitConflict => "GIT_CONFLICT",
            Self::GitFailed => "GIT_FAILED",
            Self::MigrationRequired => "MIGRATION_REQUIRED",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
            Self::Internal => "INTERNAL",
        }
    }
}
impl ErrorDto {
    fn code(&self) -> ErrorCode {
        match self.kind {
            ErrorKind::InvalidArgument(_) => ErrorCode::InvalidArgument,
            ErrorKind::WorkspaceNotFound(_) => ErrorCode::WorkspaceNotFound,
            ErrorKind::WorkspaceUnavailable(_) => ErrorCode::WorkspaceUnavailable,
            ErrorKind::WorkspaceReadOnly(_) => ErrorCode::WorkspaceReadOnly,
            ErrorKind::WorkspaceLimitExceeded(_) => ErrorCode::WorkspaceLimitExceeded,
            ErrorKind::SchemaUnsupported(_) => ErrorCode::SchemaUnsupported,
            ErrorKind::InvalidManifest(_) => ErrorCode::InvalidManifest,
            ErrorKind::InvalidCategory(_) => ErrorCode::InvalidCategory,
            ErrorKind::InvalidGuide(_) => ErrorCode::InvalidGuide,
            ErrorKind::UnsafeRoundTrip(_) => ErrorCode::UnsafeRoundTrip,
            ErrorKind::DuplicateId(_) => ErrorCode::DuplicateId,
            ErrorKind::CategoryNotFound(_) => ErrorCode::CategoryNotFound,
            ErrorKind::GuideNotFound(_) => ErrorCode::GuideNotFound,
            ErrorKind::AttachmentNotFound(_) => ErrorCode::AttachmentNotFound,
            ErrorKind::RevisionConflict(_) => ErrorCode::RevisionConflict,
            ErrorKind::SnapshotConflict(_) => ErrorCode::SnapshotConflict,
            ErrorKind::PathEscape(_) => ErrorCode::PathEscape,
            ErrorKind::SymlinkDenied(_) => ErrorCode::SymlinkDenied,
            ErrorKind::HardlinkDenied(_) => ErrorCode::HardlinkDenied,
            ErrorKind::PermissionDenied(_) => ErrorCode::PermissionDenied,
            ErrorKind::FileTooLarge(_) => ErrorCode::FileTooLarge,
            ErrorKind::IoBusy(_) => ErrorCode::IoBusy,
            ErrorKind::IndexNotReady(_) => ErrorCode::IndexNotReady,
            ErrorKind::PlanStale(_) => ErrorCode::PlanStale,
            ErrorKind::ConfirmationRequired(_) => ErrorCode::ConfirmationRequired,
            ErrorKind::ConfirmationInvalid(_) => ErrorCode::ConfirmationInvalid,
            ErrorKind::TestApprovalRequired(_) => ErrorCode::TestApprovalRequired,
            ErrorKind::GitUnavailable(_) => ErrorCode::GitUnavailable,
            ErrorKind::GitUnsafeRepository(_) => ErrorCode::GitUnsafeRepository,
            ErrorKind::GitAuthRequired(_) => ErrorCode::GitAuthRequired,
            ErrorKind::GitDiverged(_) => ErrorCode::GitDiverged,
            ErrorKind::GitConflict(_) => ErrorCode::GitConflict,
            ErrorKind::GitFailed(_) => ErrorCode::GitFailed,
            ErrorKind::MigrationRequired(_) => ErrorCode::MigrationRequired,
            ErrorKind::RecoveryRequired(_) => ErrorCode::RecoveryRequired,
            ErrorKind::Internal(_) => ErrorCode::Internal,
        }
    }
}

struct WireOperationToken(String);
impl WireOperationToken {
    fn operation(self) -> OperationCode {
        OperationCode(self.0)
    }
    fn git_operation(self) -> GitOperationCode {
        GitOperationCode(self.0)
    }
}
impl<'de> Deserialize<'de> for WireOperationToken {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = WireOperationToken;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a canonical operation token")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if valid_token(value) {
                    Ok(WireOperationToken(value.into()))
                } else {
                    Err(E::custom(ValueConstructionError))
                }
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<Self::Value, E> {
                self.visit_str(value)
            }
        }
        d.deserialize_str(V)
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}

enum WireEntityKind {
    Workspace,
    Category,
    Guide,
    Attachment,
}
impl<'de> Deserialize<'de> for WireEntityKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = WireEntityKind;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a portable entity kind")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "workspace" => Ok(WireEntityKind::Workspace),
                    "category" => Ok(WireEntityKind::Category),
                    "guide" => Ok(WireEntityKind::Guide),
                    "attachment" => Ok(WireEntityKind::Attachment),
                    _ => Err(E::custom(ValueConstructionError)),
                }
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<Self::Value, E> {
                self.visit_str(value)
            }
        }
        d.deserialize_str(V)
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}

struct WireEntityId(String);
impl<'de> Deserialize<'de> for WireEntityId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = WireEntityId;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a canonical entity identifier")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.len() == 36 {
                    Ok(WireEntityId(value.into()))
                } else {
                    Err(E::custom(ValueConstructionError))
                }
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<Self::Value, E> {
                self.visit_str(value)
            }
        }
        d.deserialize_str(V)
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}

macro_rules! raw_detail_catalog {
    ($(($slot:ident,$variant:ident,$wire:literal,$ty:ty,$bit:expr)),+ $(,)?) => {
        #[derive(Default)]
        struct RawDetails { seen:u64, $($slot:Option<$ty>),+ }
        enum DetailField { $($variant),+, Unknown }
        impl<'de> Deserialize<'de> for DetailField {
            fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{
                struct V;
                impl<'de> serde::de::Visitor<'de> for V { type Value=DetailField;
                    fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("a detail field")}
                    fn visit_str<E:serde::de::Error>(self,v:&str)->Result<Self::Value,E>{Ok(match v{$($wire=>DetailField::$variant,)+ _=>DetailField::Unknown})}
                    fn visit_borrowed_str<E:serde::de::Error>(self,v:&'de str)->Result<Self::Value,E>{self.visit_str(v)}
                }
                d.deserialize_identifier(V)
            }
        }
        impl<'de> Deserialize<'de> for RawDetails {
            fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{
                struct V;
                impl<'de> serde::de::Visitor<'de> for V { type Value=RawDetails;
                    fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("an error details object")}
                    fn visit_map<A:serde::de::MapAccess<'de>>(self,mut map:A)->Result<RawDetails,A::Error>{
                        let mut raw=RawDetails::default();
                        while let Some(field)=map.next_key::<DetailField>()? { match field {
                            DetailField::Unknown=>{map.next_value::<serde::de::IgnoredAny>()?; continue},
                            $(DetailField::$variant=>{let flag=1u64<<$bit; if raw.seen&flag!=0{return Err(serde::de::Error::duplicate_field($wire))} raw.seen|=flag; raw.$slot=Some(map.next_value()?);})+
                        }} Ok(raw)
                    }
                } d.deserialize_map(V).map_err(|_| D::Error::custom(ValueConstructionError))
            }
        }
    }
}
struct ConsumerText(PublicText);
impl<'de> Deserialize<'de> for ConsumerText {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = ConsumerText;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("safe public text")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.is_empty() || value.chars().count() > 256 || value.chars().any(prohibited)
                {
                    Err(E::custom(ValueConstructionError))
                } else {
                    Ok(ConsumerText(PublicText(value.into())))
                }
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<Self::Value, E> {
                self.visit_str(value)
            }
        }
        d.deserialize_str(V)
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}
raw_detail_catalog!(
(field,Field,"field",SchemaFieldPath,0),(reason,Reason,"reason",ReasonCode,1),(recent_workspace_id,RecentWorkspaceId,"recentWorkspaceId",RecentWorkspaceId,2),(required_capability,RequiredCapability,"requiredCapability",RequiredCapability,3),(limit,Limit,"limit",U64Decimal,4),(observed_at_least,ObservedAtLeast,"observedAtLeast",U64Decimal,5),(scope,Scope,"scope",WorkspaceLimitScope,6),(found_version,FoundVersion,"foundVersion",u32,7),(minimum_version,MinimumVersion,"minimumVersion",u32,8),(maximum_version,MaximumVersion,"maximumVersion",u32,9),(relative_path,RelativePath,"relativePath",SafeRelativePath,10),(diagnostic_code,DiagnosticCode,"diagnosticCode",DiagnosticCode,11),(line,Line,"line",u32,12),(column,Column,"column",u32,13),(category_id,CategoryId,"categoryId",CategoryId,14),(guide_id,GuideId,"guideId",GuideId,15),(entity_kind,EntityKind,"entityKind",WireEntityKind,16),(entity_id,EntityId,"entityId",WireEntityId,17),(relative_paths,RelativePaths,"relativePaths",BoundedList<SafeRelativePath>,18),(attachment_id,AttachmentId,"attachmentId",AttachmentId,19),(expected_revision,ExpectedRevision,"expectedRevision",Revision,20),(actual_revision,ActualRevision,"actualRevision",Revision,21),(expected_snapshot,ExpectedSnapshot,"expectedSnapshot",Snapshot,22),(actual_snapshot,ActualSnapshot,"actualSnapshot",Snapshot,23),(changed_entity_ids,ChangedEntityIds,"changedEntityIds",BoundedList<PortableEntityId>,24),(link_count,LinkCount,"linkCount",U64Decimal,25),(operation,Operation,"operation",WireOperationToken,26),(limit_bytes,LimitBytes,"limitBytes",U64Decimal,27),(observed_at_least_bytes,ObservedAtLeastBytes,"observedAtLeastBytes",U64Decimal,28),(timeout_ms,TimeoutMs,"timeoutMs",u32,29),(index_generation,IndexGeneration,"indexGeneration",U64Decimal,30),(progress_completed,ProgressCompleted,"progressCompleted",u32,31),(progress_total,ProgressTotal,"progressTotal",u32,32),(plan_token,PlanToken,"planToken",PlanToken,33),(expires_at,ExpiresAt,"expiresAt",Timestamp,34),(guide_revision,GuideRevision,"guideRevision",Revision,35),(approval_request_id,ApprovalRequestId,"approvalRequestId",ApprovalRequestId,36),(search_source_kinds,SearchSourceKinds,"searchSourceKinds",BoundedList<GitSearchSourceKind>,37),(remote_name,RemoteName,"remoteName",SafeGitDisplay,38),(host,Host,"host",SafeHost,39),(credential_kind,CredentialKind,"credentialKind",GitCredentialKind,40),(ahead,Ahead,"ahead",U64Decimal,41),(behind,Behind,"behind",U64Decimal,42),(branch,Branch,"branch",SafeGitDisplay,43),(conflict_kind,ConflictKind,"conflictKind",GitConflictKind,44),(exit_kind,ExitKind,"exitKind",GitExitKind,45),(sanitized_reason,SanitizedReason,"sanitizedReason",ConsumerText,46),(target_version,TargetVersion,"targetVersion",u32,47),(operation_id,OperationId,"operationId",OperationId,48),(affected_relative_paths,AffectedRelativePaths,"affectedRelativePaths",BoundedList<SafeRelativePath>,49),(incident_id,IncidentId,"incidentId",IncidentId,50));

macro_rules! mask {($($bit:expr),*$(,)?)=>{0u64$(|(1u64<<$bit))*}}
fn exact_mask(seen: u64, required: u64, optional: u64) -> Result<(), ValueConstructionError> {
    if seen & required == required && seen & !(required | optional) == 0 {
        Ok(())
    } else {
        Err(ValueConstructionError)
    }
}
fn entity(
    kind: WireEntityKind,
    id: WireEntityId,
) -> Result<PortableEntityId, ValueConstructionError> {
    match kind {
        WireEntityKind::Workspace => Ok(PortableEntityId::workspace(id.0.parse()?)),
        WireEntityKind::Category => Ok(PortableEntityId::category(id.0.parse()?)),
        WireEntityKind::Guide => Ok(PortableEntityId::guide(id.0.parse()?)),
        WireEntityKind::Attachment => Ok(PortableEntityId::attachment(id.0.parse()?)),
    }
}
impl RawDetails {
    fn into_kind(mut self, code: ErrorCode) -> Result<ErrorKind, ValueConstructionError> {
        macro_rules! req {
            ($f:ident) => {
                self.$f.take().ok_or(ValueConstructionError)?
            };
        }
        macro_rules! ex {
            ($r:expr,$o:expr) => {
                exact_mask(self.seen, $r, $o)?
            };
        }
        Ok(match code {
            ErrorCode::InvalidArgument => {
                ex!(mask!(0, 1), 0);
                ErrorKind::InvalidArgument(InvalidArgumentData {
                    field: req!(field),
                    reason: req!(reason),
                })
            }
            ErrorCode::WorkspaceNotFound => {
                ex!(mask!(2), 0);
                ErrorKind::WorkspaceNotFound(WorkspaceNotFoundData {
                    recent_workspace_id: req!(recent_workspace_id),
                })
            }
            ErrorCode::WorkspaceUnavailable => {
                ex!(mask!(1), 0);
                ErrorKind::WorkspaceUnavailable(WorkspaceUnavailableData {
                    reason: req!(reason),
                })
            }
            ErrorCode::WorkspaceReadOnly => {
                ex!(mask!(1), mask!(3));
                ErrorKind::WorkspaceReadOnly(WorkspaceReadOnlyData {
                    reason: req!(reason),
                    required_capability: self.required_capability.take(),
                })
            }
            ErrorCode::WorkspaceLimitExceeded => {
                ex!(mask!(4, 5, 6), 0);
                ErrorKind::WorkspaceLimitExceeded(WorkspaceLimitExceededData {
                    limit: req!(limit),
                    observed_at_least: req!(observed_at_least),
                    scope: req!(scope),
                })
            }
            ErrorCode::SchemaUnsupported => {
                ex!(mask!(7, 8, 9), 0);
                ErrorKind::SchemaUnsupported(SchemaUnsupportedData {
                    found_version: req!(found_version),
                    minimum_version: req!(minimum_version),
                    maximum_version: req!(maximum_version),
                })
            }
            ErrorCode::InvalidManifest => {
                ex!(mask!(10, 11), mask!(0, 12, 13));
                ErrorKind::InvalidManifest(InvalidManifestData {
                    relative_path: req!(relative_path),
                    diagnostic_code: req!(diagnostic_code),
                    field: self.field.take(),
                    line: self.line.take(),
                    column: self.column.take(),
                })
            }
            ErrorCode::InvalidCategory => {
                ex!(mask!(10, 11), mask!(0, 12, 13, 14));
                ErrorKind::InvalidCategory(InvalidCategoryData {
                    relative_path: req!(relative_path),
                    diagnostic_code: req!(diagnostic_code),
                    field: self.field.take(),
                    line: self.line.take(),
                    column: self.column.take(),
                    category_id: self.category_id.take(),
                })
            }
            ErrorCode::InvalidGuide => {
                ex!(mask!(10, 11), mask!(0, 12, 13, 15));
                ErrorKind::InvalidGuide(InvalidGuideData {
                    relative_path: req!(relative_path),
                    diagnostic_code: req!(diagnostic_code),
                    field: self.field.take(),
                    line: self.line.take(),
                    column: self.column.take(),
                    guide_id: self.guide_id.take(),
                })
            }
            ErrorCode::UnsafeRoundTrip => {
                ex!(mask!(10, 1), mask!(0));
                ErrorKind::UnsafeRoundTrip(UnsafeRoundTripData {
                    relative_path: req!(relative_path),
                    reason: req!(reason),
                    field: self.field.take(),
                })
            }
            ErrorCode::DuplicateId => {
                ex!(mask!(16, 17, 18), 0);
                let entity = entity(req!(entity_kind), req!(entity_id))?;
                ErrorKind::DuplicateId(DuplicateIdData {
                    entity,
                    relative_paths: req!(relative_paths),
                })
            }
            ErrorCode::CategoryNotFound => {
                ex!(mask!(14), 0);
                ErrorKind::CategoryNotFound(CategoryNotFoundData {
                    category_id: req!(category_id),
                })
            }
            ErrorCode::GuideNotFound => {
                ex!(mask!(15), 0);
                ErrorKind::GuideNotFound(GuideNotFoundData {
                    guide_id: req!(guide_id),
                })
            }
            ErrorCode::AttachmentNotFound => {
                ex!(mask!(19, 15), 0);
                ErrorKind::AttachmentNotFound(AttachmentNotFoundData {
                    attachment_id: req!(attachment_id),
                    guide_id: req!(guide_id),
                })
            }
            ErrorCode::RevisionConflict => {
                ex!(mask!(16, 17, 20, 21), 0);
                let entity = entity(req!(entity_kind), req!(entity_id))?;
                ErrorKind::RevisionConflict(RevisionConflictData {
                    entity,
                    expected_revision: req!(expected_revision),
                    actual_revision: req!(actual_revision),
                })
            }
            ErrorCode::SnapshotConflict => {
                ex!(mask!(22, 23, 24), 0);
                ErrorKind::SnapshotConflict(SnapshotConflictData {
                    expected_snapshot: req!(expected_snapshot),
                    actual_snapshot: req!(actual_snapshot),
                    changed_entity_ids: req!(changed_entity_ids),
                })
            }
            ErrorCode::PathEscape => {
                ex!(mask!(1), mask!(10));
                ErrorKind::PathEscape(PathEscapeData {
                    reason: req!(reason),
                    relative_path: self.relative_path.take(),
                })
            }
            ErrorCode::SymlinkDenied => {
                ex!(mask!(10), 0);
                ErrorKind::SymlinkDenied(SymlinkDeniedData {
                    relative_path: req!(relative_path),
                })
            }
            ErrorCode::HardlinkDenied => {
                ex!(mask!(10, 25), 0);
                ErrorKind::HardlinkDenied(HardlinkDeniedData {
                    relative_path: req!(relative_path),
                    link_count: req!(link_count),
                })
            }
            ErrorCode::PermissionDenied => {
                ex!(mask!(26), mask!(10));
                ErrorKind::PermissionDenied(PermissionDeniedData {
                    operation: req!(operation).operation(),
                    relative_path: self.relative_path.take(),
                })
            }
            ErrorCode::FileTooLarge => {
                ex!(mask!(10, 27, 28), 0);
                ErrorKind::FileTooLarge(FileTooLargeData {
                    relative_path: req!(relative_path),
                    limit_bytes: req!(limit_bytes),
                    observed_at_least_bytes: req!(observed_at_least_bytes),
                })
            }
            ErrorCode::IoBusy => {
                ex!(mask!(26, 29), 0);
                ErrorKind::IoBusy(IoBusyData {
                    operation: req!(operation).operation(),
                    timeout_ms: req!(timeout_ms),
                })
            }
            ErrorCode::IndexNotReady => {
                ex!(mask!(30, 31), mask!(32));
                ErrorKind::IndexNotReady(IndexNotReadyData {
                    index_generation: req!(index_generation),
                    progress_completed: req!(progress_completed),
                    progress_total: self.progress_total.take(),
                })
            }
            ErrorCode::PlanStale => {
                ex!(mask!(26, 24), 0);
                ErrorKind::PlanStale(PlanStaleData {
                    operation: req!(operation).operation(),
                    changed_entity_ids: req!(changed_entity_ids),
                })
            }
            ErrorCode::ConfirmationRequired => {
                ex!(mask!(26, 33, 34), 0);
                ErrorKind::ConfirmationRequired(ConfirmationRequiredData {
                    operation: req!(operation).operation(),
                    plan_token: req!(plan_token),
                    expires_at: req!(expires_at),
                })
            }
            ErrorCode::ConfirmationInvalid => {
                ex!(mask!(26, 1), 0);
                ErrorKind::ConfirmationInvalid(ConfirmationInvalidData {
                    operation: req!(operation).operation(),
                    reason: req!(reason),
                })
            }
            ErrorCode::TestApprovalRequired => {
                ex!(mask!(15, 35, 36), 0);
                ErrorKind::TestApprovalRequired(TestApprovalRequiredData {
                    guide_id: req!(guide_id),
                    guide_revision: req!(guide_revision),
                    approval_request_id: req!(approval_request_id),
                })
            }
            ErrorCode::GitUnavailable => {
                ex!(mask!(37), 0);
                ErrorKind::GitUnavailable(GitUnavailableData {
                    search_source_kinds: req!(search_source_kinds),
                })
            }
            ErrorCode::GitUnsafeRepository => {
                ex!(mask!(1), 0);
                ErrorKind::GitUnsafeRepository(GitUnsafeRepositoryData {
                    reason: req!(reason),
                })
            }
            ErrorCode::GitAuthRequired => {
                ex!(mask!(38, 39, 40), 0);
                ErrorKind::GitAuthRequired(GitAuthRequiredData {
                    remote_name: req!(remote_name),
                    host: req!(host),
                    credential_kind: req!(credential_kind),
                })
            }
            ErrorCode::GitDiverged => {
                ex!(mask!(41, 42, 43), 0);
                ErrorKind::GitDiverged(GitDivergedData {
                    ahead: req!(ahead),
                    behind: req!(behind),
                    branch: req!(branch),
                })
            }
            ErrorCode::GitConflict => {
                ex!(mask!(44, 18), 0);
                ErrorKind::GitConflict(GitConflictData {
                    conflict_kind: req!(conflict_kind),
                    relative_paths: req!(relative_paths),
                })
            }
            ErrorCode::GitFailed => {
                ex!(mask!(26, 45, 46), 0);
                ErrorKind::GitFailed(GitFailedData {
                    operation: req!(operation).git_operation(),
                    exit_kind: req!(exit_kind),
                    sanitized_reason: req!(sanitized_reason).0,
                })
            }
            ErrorCode::MigrationRequired => {
                ex!(mask!(7, 47), 0);
                ErrorKind::MigrationRequired(MigrationRequiredData {
                    found_version: req!(found_version),
                    target_version: req!(target_version),
                })
            }
            ErrorCode::RecoveryRequired => {
                ex!(mask!(48, 1, 49), 0);
                ErrorKind::RecoveryRequired(RecoveryRequiredData {
                    operation_id: req!(operation_id),
                    reason: req!(reason),
                    affected_relative_paths: req!(affected_relative_paths),
                })
            }
            ErrorCode::Internal => {
                ex!(mask!(50), 0);
                ErrorKind::Internal(InternalData {
                    incident_id: req!(incident_id),
                })
            }
        })
    }
}

impl<'de> Deserialize<'de> for ErrorDto {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        enum TopField {
            Version,
            Code,
            Message,
            Retryable,
            RequestId,
            Details,
            Recovery,
        }
        impl<'de> Deserialize<'de> for TopField {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct FieldVisitor;
                impl serde::de::Visitor<'_> for FieldVisitor {
                    type Value = TopField;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("an ErrorDto field")
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<TopField, E> {
                        match value {
                            "version" => Ok(TopField::Version),
                            "code" => Ok(TopField::Code),
                            "message" => Ok(TopField::Message),
                            "retryable" => Ok(TopField::Retryable),
                            "requestId" => Ok(TopField::RequestId),
                            "details" => Ok(TopField::Details),
                            "recovery" => Ok(TopField::Recovery),
                            _ => Err(E::custom(ValueConstructionError)),
                        }
                    }
                    fn visit_borrowed_str<E: serde::de::Error>(
                        self,
                        value: &str,
                    ) -> Result<TopField, E> {
                        self.visit_str(value)
                    }
                }
                d.deserialize_identifier(FieldVisitor)
            }
        }
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = ErrorDto;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an ErrorDto object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<ErrorDto, A::Error> {
                let mut version: Option<u8> = None;
                let mut code: Option<ErrorCode> = None;
                let mut message: Option<ConsumerText> = None;
                let mut retryable: Option<bool> = None;
                let mut request_id: Option<RequestId> = None;
                let mut details: Option<RawDetails> = None;
                let mut recovery: Option<ConsumerText> = None;
                while let Some(field) = a.next_key::<TopField>()? {
                    match field {
                        TopField::Version if version.is_none() => version = Some(a.next_value()?),
                        TopField::Code if code.is_none() => code = Some(a.next_value()?),
                        TopField::Message if message.is_none() => message = Some(a.next_value()?),
                        TopField::Retryable if retryable.is_none() => {
                            retryable = Some(a.next_value()?)
                        }
                        TopField::RequestId if request_id.is_none() => {
                            request_id = Some(a.next_value()?)
                        }
                        TopField::Details if details.is_none() => details = Some(a.next_value()?),
                        TopField::Recovery if recovery.is_none() => {
                            recovery = Some(a.next_value()?)
                        }
                        TopField::Version => {
                            return Err(serde::de::Error::duplicate_field("version"));
                        }
                        TopField::Code => return Err(serde::de::Error::duplicate_field("code")),
                        TopField::Message => {
                            return Err(serde::de::Error::duplicate_field("message"));
                        }
                        TopField::Retryable => {
                            return Err(serde::de::Error::duplicate_field("retryable"));
                        }
                        TopField::RequestId => {
                            return Err(serde::de::Error::duplicate_field("requestId"));
                        }
                        TopField::Details => {
                            return Err(serde::de::Error::duplicate_field("details"));
                        }
                        TopField::Recovery => {
                            return Err(serde::de::Error::duplicate_field("recovery"));
                        }
                    }
                }
                let version: u8 =
                    version.ok_or_else(|| serde::de::Error::missing_field("version"))?;
                if version != 1 {
                    return Err(serde::de::Error::custom("unsupported error version"));
                }
                let code = code.ok_or_else(|| serde::de::Error::missing_field("code"))?;
                let kind = details
                    .ok_or_else(|| serde::de::Error::missing_field("details"))?
                    .into_kind(code)
                    .map_err(serde::de::Error::custom)?;
                let dto = ErrorDto {
                    version,
                    kind,
                    message: message
                        .ok_or_else(|| serde::de::Error::missing_field("message"))?
                        .0,
                    retryable: retryable
                        .ok_or_else(|| serde::de::Error::missing_field("retryable"))?,
                    request_id: request_id
                        .ok_or_else(|| serde::de::Error::missing_field("requestId"))?,
                    recovery: recovery
                        .ok_or_else(|| serde::de::Error::missing_field("recovery"))?
                        .0,
                };
                dto.validate().map_err(serde::de::Error::custom)?;
                Ok(dto)
            }
        }
        d.deserialize_map(V)
            .map_err(|_| D::Error::custom(ValueConstructionError))
    }
}

struct DetailsRef<'a>(&'a ErrorKind);
fn serialize_entity_fields<M: serde::ser::SerializeMap>(
    m: &mut M,
    entity: &PortableEntityId,
) -> Result<(), M::Error> {
    match &entity.0 {
        PortableEntityRepresentation::Workspace(id) => {
            m.serialize_entry("entityKind", "workspace")?;
            m.serialize_entry("entityId", id)
        }
        PortableEntityRepresentation::Category(id) => {
            m.serialize_entry("entityKind", "category")?;
            m.serialize_entry("entityId", id)
        }
        PortableEntityRepresentation::Guide(id) => {
            m.serialize_entry("entityKind", "guide")?;
            m.serialize_entry("entityId", id)
        }
        PortableEntityRepresentation::Attachment(id) => {
            m.serialize_entry("entityKind", "attachment")?;
            m.serialize_entry("entityId", id)
        }
    }
}
impl Serialize for DetailsRef<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self.0 {
            ErrorKind::DuplicateId(d) => {
                let mut m = s.serialize_map(Some(3))?;
                serialize_entity_fields(&mut m, &d.entity)?;
                m.serialize_entry("relativePaths", &d.relative_paths)?;
                m.end()
            }
            ErrorKind::RevisionConflict(d) => {
                let mut m = s.serialize_map(Some(4))?;
                serialize_entity_fields(&mut m, &d.entity)?;
                m.serialize_entry("expectedRevision", &d.expected_revision)?;
                m.serialize_entry("actualRevision", &d.actual_revision)?;
                m.end()
            }
            ErrorKind::InvalidArgument(d) => d.serialize(s),
            ErrorKind::WorkspaceNotFound(d) => d.serialize(s),
            ErrorKind::WorkspaceUnavailable(d) => d.serialize(s),
            ErrorKind::WorkspaceReadOnly(d) => d.serialize(s),
            ErrorKind::WorkspaceLimitExceeded(d) => d.serialize(s),
            ErrorKind::SchemaUnsupported(d) => d.serialize(s),
            ErrorKind::InvalidManifest(d) => d.serialize(s),
            ErrorKind::InvalidCategory(d) => d.serialize(s),
            ErrorKind::InvalidGuide(d) => d.serialize(s),
            ErrorKind::UnsafeRoundTrip(d) => d.serialize(s),
            ErrorKind::CategoryNotFound(d) => d.serialize(s),
            ErrorKind::GuideNotFound(d) => d.serialize(s),
            ErrorKind::AttachmentNotFound(d) => d.serialize(s),
            ErrorKind::SnapshotConflict(d) => d.serialize(s),
            ErrorKind::PathEscape(d) => d.serialize(s),
            ErrorKind::SymlinkDenied(d) => d.serialize(s),
            ErrorKind::HardlinkDenied(d) => d.serialize(s),
            ErrorKind::PermissionDenied(d) => d.serialize(s),
            ErrorKind::FileTooLarge(d) => d.serialize(s),
            ErrorKind::IoBusy(d) => d.serialize(s),
            ErrorKind::IndexNotReady(d) => d.serialize(s),
            ErrorKind::PlanStale(d) => d.serialize(s),
            ErrorKind::ConfirmationRequired(d) => d.serialize(s),
            ErrorKind::ConfirmationInvalid(d) => d.serialize(s),
            ErrorKind::TestApprovalRequired(d) => d.serialize(s),
            ErrorKind::GitUnavailable(d) => d.serialize(s),
            ErrorKind::GitUnsafeRepository(d) => d.serialize(s),
            ErrorKind::GitAuthRequired(d) => d.serialize(s),
            ErrorKind::GitDiverged(d) => d.serialize(s),
            ErrorKind::GitConflict(d) => d.serialize(s),
            ErrorKind::GitFailed(d) => d.serialize(s),
            ErrorKind::MigrationRequired(d) => d.serialize(s),
            ErrorKind::RecoveryRequired(d) => d.serialize(s),
            ErrorKind::Internal(d) => d.serialize(s),
        }
    }
}
impl Serialize for ErrorDto {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(7))?;
        m.serialize_entry("version", &1_u8)?;
        m.serialize_entry("code", &self.code())?;
        m.serialize_entry("message", &self.message)?;
        m.serialize_entry("retryable", &self.retryable)?;
        m.serialize_entry("requestId", &self.request_id)?;
        m.serialize_entry("details", &DetailsRef(&self.kind))?;
        m.serialize_entry("recovery", &self.recovery)?;
        m.end()
    }
}
impl ErrorDto {
    fn validate(&self) -> Result<(), ValueConstructionError> {
        let expected = matches!(
            self.kind,
            ErrorKind::WorkspaceNotFound(_)
                | ErrorKind::WorkspaceUnavailable(_)
                | ErrorKind::IoBusy(_)
                | ErrorKind::IndexNotReady(_)
                | ErrorKind::GitAuthRequired(_)
        );
        if self.retryable != expected {
            return Err(ValueConstructionError);
        }
        match &self.kind {
            ErrorKind::WorkspaceLimitExceeded(d) if d.observed_at_least.0 <= d.limit.0 => {
                Err(ValueConstructionError)
            }
            ErrorKind::SchemaUnsupported(d)
                if d.minimum_version == 0
                    || d.maximum_version == 0
                    || d.minimum_version > d.maximum_version
                    || (d.found_version >= d.minimum_version
                        && d.found_version <= d.maximum_version) =>
            {
                Err(ValueConstructionError)
            }
            ErrorKind::InvalidManifest(d) if invalid_location(d.line, d.column) => {
                Err(ValueConstructionError)
            }
            ErrorKind::InvalidCategory(d) if invalid_location(d.line, d.column) => {
                Err(ValueConstructionError)
            }
            ErrorKind::InvalidGuide(d) if invalid_location(d.line, d.column) => {
                Err(ValueConstructionError)
            }
            ErrorKind::DuplicateId(d) if d.relative_paths.total_count < 2 => {
                Err(ValueConstructionError)
            }
            ErrorKind::RevisionConflict(d) if d.expected_revision == d.actual_revision => {
                Err(ValueConstructionError)
            }
            ErrorKind::SnapshotConflict(d) if d.expected_snapshot == d.actual_snapshot => {
                Err(ValueConstructionError)
            }
            ErrorKind::HardlinkDenied(d) if d.link_count.0 <= 1 => Err(ValueConstructionError),
            ErrorKind::FileTooLarge(d) if d.observed_at_least_bytes.0 <= d.limit_bytes.0 => {
                Err(ValueConstructionError)
            }
            ErrorKind::IoBusy(d) if d.timeout_ms == 0 => Err(ValueConstructionError),
            ErrorKind::IndexNotReady(d)
                if d.progress_total.is_some_and(|t| t < d.progress_completed) =>
            {
                Err(ValueConstructionError)
            }
            ErrorKind::GitUnavailable(d) if d.search_source_kinds.total_count == 0 => {
                Err(ValueConstructionError)
            }
            ErrorKind::GitDiverged(d) if d.ahead.0 == 0 || d.behind.0 == 0 => {
                Err(ValueConstructionError)
            }
            ErrorKind::GitConflict(d) if d.relative_paths.total_count == 0 => {
                Err(ValueConstructionError)
            }
            ErrorKind::MigrationRequired(d)
                if d.found_version == 0 || d.target_version <= d.found_version =>
            {
                Err(ValueConstructionError)
            }
            _ => Ok(()),
        }
    }
}
fn invalid_location(line: Option<u32>, column: Option<u32>) -> bool {
    line.is_some() != column.is_some() || line == Some(0) || column == Some(0)
}

use rehow_core::{
    domain::{
        CanonicalYamlFloat, CanonicalYamlInteger, Category, CategoryStatus,
        UnknownYamlDocumentBuilder, UnknownYamlEvent, WorkspaceManifest, WorkspaceSettings,
    },
    protocol::{
        BoundedList, ErrorDto, GuideId, PlanToken, PortableEntityId, SafeRelativePath, Timestamp,
        U64Decimal, WorkspaceId,
    },
};
use std::str::FromStr;
use ts_rs::TS;

fn main() {
    fn inspect_manifest(value: &WorkspaceManifest) {
        let _ = (
            value.schema_version(),
            value.workspace_id(),
            value.name(),
            value.created_at(),
            value.updated_at(),
            value.settings(),
        );
    }
    fn inspect_settings(value: &WorkspaceSettings) {
        let _ = value.default_category_id();
    }
    fn inspect_category(value: &Category) {
        let _: CategoryStatus = value.status();
        let _ = (
            value.schema_version(),
            value.id(),
            value.name(),
            value.summary(),
            value.order(),
            value.created_at(),
            value.updated_at(),
        );
    }
    let _ = (inspect_manifest, inspect_settings, inspect_category);
    let id = WorkspaceId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap();
    let _: WorkspaceId = "0199a1d7-7b0d-7d8e-a38d-f936c5735e10"
        .try_into()
        .unwrap();
    let wire = serde_json::to_string(&id).unwrap();
    let _: WorkspaceId = serde_json::from_str(&wire).unwrap();
    let _ = id.to_string();

    let guide = GuideId::from_str("0199a1d7-7b0d-7d8e-a38d-f936c5735e10").unwrap();
    let _ = PortableEntityId::guide(guide);
    let _ = Timestamp::from_str("2026-09-06T00:00:00Z").unwrap();
    let _ = U64Decimal::new(1);
    let token = PlanToken::from_str("plan:v1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap();
    let _ = serde_json::to_string(&token).unwrap();
    let _ = SafeRelativePath::parse_wire("guides/a.md").unwrap();

    let list: BoundedList<u8> = BoundedList::try_new(vec![1], 1).unwrap();
    let _ = (list.items(), list.total_count(), list.truncated());

    let integer = CanonicalYamlInteger::new(false, "1".into()).unwrap();
    let float = CanonicalYamlFloat::finite(
        false,
        "1".into(),
        CanonicalYamlInteger::new(false, "0".into()).unwrap(),
    )
    .unwrap();
    let _ = (integer.is_negative(), integer.digits());
    let _ = (
        float.kind(),
        float.is_negative(),
        float.coefficient_digits(),
        float.exponent10(),
    );
    let mut builder = UnknownYamlDocumentBuilder::new();
    for event in [
        UnknownYamlEvent::BeginMapping,
        UnknownYamlEvent::String("key".into()),
        UnknownYamlEvent::BeginSequence,
        UnknownYamlEvent::Integer(integer),
        UnknownYamlEvent::Float(float),
        UnknownYamlEvent::EndSequence,
        UnknownYamlEvent::EndMapping,
    ] {
        builder.push_event(event).unwrap();
    }
    let document = builder.finish_document().unwrap();
    let entry = &document.entries()[0];
    let value = entry.value();
    let scalar = value.as_sequence().unwrap()[0].as_scalar().unwrap();
    let _ = (
        entry.key(),
        value.kind(),
        value.as_scalar(),
        value.as_mapping(),
        scalar.kind(),
        scalar.as_bool(),
        scalar.as_integer(),
        scalar.as_float(),
        scalar.as_str(),
    );

    let error_wire = r#"{"version":1,"code":"INVALID_ARGUMENT","message":"m","retryable":false,"requestId":"0199a1d7-7b0d-7d8e-a38d-f936c5735e10","details":{"field":"/title","reason":"required"},"recovery":"r"}"#;
    let error: ErrorDto = serde_json::from_str(error_wire).unwrap();
    let _ = serde_json::to_string(&error).unwrap();
    let _ = ErrorDto::decl(&ts_rs::Config::default());
}

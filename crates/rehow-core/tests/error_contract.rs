use rehow_core::protocol::ErrorDto;
use serde_json::{Value, json};
use std::fs;

const REQUEST: &str = "0199a1d7-7b0d-7d8e-a38d-f936c5735e10";
fn invalid_argument() -> Value {
    json!({"version":1,"code":"INVALID_ARGUMENT","message":"The request contains an invalid value.","retryable":false,"requestId":REQUEST,"details":{"field":"/title","reason":"required"},"recovery":"Correct the named field and try again."})
}
fn golden(number: &str) -> Value {
    let root = format!(
        "{}/tests/fixtures/error-contract/v1",
        env!("CARGO_MANIFEST_DIR")
    );
    let path = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_string_lossy().starts_with(number))
        .unwrap();
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
#[test]
fn strict_envelope_and_consumer_copy() {
    let value = invalid_argument();
    let dto: ErrorDto = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&dto).unwrap(), value);
    let mut improved = value.clone();
    improved["message"] = json!("Improved safe wording.");
    let dto: ErrorDto = serde_json::from_value(improved.clone()).unwrap();
    assert_eq!(serde_json::to_value(dto).unwrap(), improved);
    for key in [
        "version",
        "code",
        "message",
        "retryable",
        "requestId",
        "details",
        "recovery",
    ] {
        let mut x = value.clone();
        x.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<ErrorDto>(x).is_err())
    }
    let mut x = value.clone();
    x["extra"] = json!(1);
    assert!(serde_json::from_value::<ErrorDto>(x).is_err());
}
#[test]
fn correlations_and_unknown_detail_policy() {
    let mut x = invalid_argument();
    x["retryable"] = json!(true);
    assert!(serde_json::from_value::<ErrorDto>(x).is_err());
    let mut x = invalid_argument();
    x["details"]["categoryId"] = json!(REQUEST);
    assert!(serde_json::from_value::<ErrorDto>(x).is_err());
    let mut x = invalid_argument();
    x["details"]["futureSafeField"] = json!({"anything":null});
    let dto: ErrorDto = serde_json::from_value(x).unwrap();
    assert!(
        serde_json::to_value(dto).unwrap()["details"]
            .get("futureSafeField")
            .is_none()
    );
    let mut x = invalid_argument();
    x["message"] = json!("bad\ntext");
    assert!(serde_json::from_value::<ErrorDto>(x).is_err());
}
#[test]
fn duplicate_and_null_known_fields_fail() {
    let duplicate = format!(
        r#"{{"version":1,"version":1,"code":"INVALID_ARGUMENT","message":"x","retryable":false,"requestId":"{REQUEST}","details":{{"field":"/title","reason":"required"}},"recovery":"x"}}"#
    );
    assert!(serde_json::from_str::<ErrorDto>(&duplicate).is_err());
    let mut x = invalid_argument();
    x["details"]["field"] = Value::Null;
    assert!(serde_json::from_value::<ErrorDto>(x).is_err());
    let duplicate_detail = format!(
        r#"{{"version":1,"code":"INVALID_ARGUMENT","message":"x","retryable":false,"requestId":"{REQUEST}","details":{{"field":"/title","reason":"required","reason":"required"}},"recovery":"x"}}"#
    );
    assert!(serde_json::from_str::<ErrorDto>(&duplicate_detail).is_err());
}
#[test]
fn correlated_entity_shape_round_trips() {
    let value = json!({"version":1,"code":"DUPLICATE_ID","message":"x","retryable":false,"requestId":REQUEST,"details":{"entityKind":"guide","entityId":REQUEST,"relativePaths":{"items":["guides/a.md","guides/b.md"],"totalCount":2,"truncated":false}},"recovery":"x"});
    let dto: ErrorDto = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(dto).unwrap(), value);
    let mut future = value.clone();
    future["details"]["futureSafeField"] = json!(1);
    let dto: ErrorDto = serde_json::from_value(future).unwrap();
    assert_eq!(serde_json::to_value(dto).unwrap(), value);
}
#[test]
fn all_golden_fixtures_are_exact_and_correlated() {
    let root = format!(
        "{}/tests/fixtures/error-contract/v1",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut files = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files.len(), 36);
    for path in files {
        let bytes = fs::read(path).unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 7);
        let dto: ErrorDto = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(dto).unwrap(), value);
        for key in [
            "version",
            "code",
            "message",
            "retryable",
            "requestId",
            "details",
            "recovery",
        ] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<ErrorDto>(missing).is_err());
            let mut null = value.clone();
            null[key] = Value::Null;
            assert!(serde_json::from_value::<ErrorDto>(null).is_err())
        }
        let mut mismatch = value.clone();
        mismatch["retryable"] = json!(!value["retryable"].as_bool().unwrap());
        assert!(serde_json::from_value::<ErrorDto>(mismatch).is_err());
        let mut unknown = value.clone();
        unknown["unknown"] = json!(1);
        assert!(serde_json::from_value::<ErrorDto>(unknown).is_err());
    }
}
#[test]
fn debug_is_redacted() {
    let dto: ErrorDto = serde_json::from_value(invalid_argument()).unwrap();
    let debug = format!("{dto:?}");
    assert!(debug.contains("INVALID_ARGUMENT"));
    assert!(debug.contains("RequestId(<redacted>)"));
    assert!(!debug.contains("invalid value"));
    assert!(!debug.contains("/title"));
}
#[test]
fn every_additional_invariant_is_enforced() {
    let cases = [
        ("05", vec![("/details/observedAtLeast", json!("1"))]),
        ("06", vec![("/details/foundVersion", json!(1))]),
        ("07", vec![("/details/line", json!(0))]),
        (
            "11",
            vec![(
                "/details/relativePaths",
                json!({"items":["guides/a.md"],"totalCount":1,"truncated":false}),
            )],
        ),
        (
            "15",
            vec![(
                "/details/actualRevision",
                json!(format!("rev:v1:{}", "0".repeat(64))),
            )],
        ),
        (
            "16",
            vec![(
                "/details/actualSnapshot",
                json!(format!("snapshot:v1:{}", "0".repeat(64))),
            )],
        ),
        ("19", vec![("/details/linkCount", json!("1"))]),
        ("21", vec![("/details/observedAtLeastBytes", json!("1"))]),
        ("22", vec![("/details/timeoutMs", json!(0))]),
        ("23", vec![("/details/progressTotal", json!(0))]),
        (
            "28",
            vec![(
                "/details/searchSourceKinds",
                json!({"items":[],"totalCount":0,"truncated":false}),
            )],
        ),
        ("31", vec![("/details/ahead", json!("0"))]),
        (
            "32",
            vec![(
                "/details/relativePaths",
                json!({"items":[],"totalCount":0,"truncated":false}),
            )],
        ),
        ("34", vec![("/details/targetVersion", json!(1))]),
    ];
    for (number, changes) in cases {
        let mut value = golden(number);
        for (pointer, replacement) in changes {
            *value.pointer_mut(pointer).unwrap() = replacement
        }
        assert!(
            serde_json::from_value::<ErrorDto>(value).is_err(),
            "fixture {number}"
        )
    }
}
#[test]
fn source_locations_are_paired() {
    let mut value = golden("07");
    value["details"].as_object_mut().unwrap().remove("column");
    assert!(serde_json::from_value::<ErrorDto>(value).is_err());
}
#[test]
fn optional_null_and_nested_unknown_are_rejected() {
    let mut value = golden("17");
    value["details"]["relativePath"] = Value::Null;
    assert!(serde_json::from_value::<ErrorDto>(value).is_err());
    let mut value = golden("11");
    value["details"]["relativePaths"]["unknown"] = json!(1);
    assert!(serde_json::from_value::<ErrorDto>(value).is_err());
}

#[test]
fn envelope_and_details_are_field_order_independent() {
    let wire = format!(
        r#"{{"recovery":"r","details":{{"reason":"required","field":"/title"}},"requestId":"{REQUEST}","retryable":false,"message":"m","code":"INVALID_ARGUMENT","version":1}}"#
    );
    let dto: ErrorDto = serde_json::from_str(&wire).unwrap();
    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        json!({
            "version": 1,
            "code": "INVALID_ARGUMENT",
            "message": "m",
            "retryable": false,
            "requestId": REQUEST,
            "details": {"field": "/title", "reason": "required"},
            "recovery": "r"
        })
    );
}

#[test]
fn every_fixture_rejects_wrong_typed_and_cross_code_detail_fields() {
    for number in 1..=36 {
        let prefix = format!("{number:02}");
        let value = golden(&prefix);
        let first_key = value["details"]
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        let mut wrong_type = value.clone();
        wrong_type["details"][&first_key] = json!({});
        assert!(
            serde_json::from_value::<ErrorDto>(wrong_type).is_err(),
            "wrong detail type accepted for {prefix}/{first_key}"
        );

        let mut cross_code = value;
        let foreign = if number == 36 { "field" } else { "incidentId" };
        cross_code["details"][foreign] = json!(REQUEST);
        assert!(
            serde_json::from_value::<ErrorDto>(cross_code).is_err(),
            "cross-code detail accepted for {prefix}/{foreign}"
        );
    }
}

#[test]
fn genuinely_unknown_immediate_details_are_discarded_even_when_repeated() {
    let large_unknown = serde_json::to_string(&"x".repeat(1_000_000)).unwrap();
    let wire = format!(
        r#"{{"version":1,"code":"INVALID_ARGUMENT","message":"m","retryable":false,"requestId":"{REQUEST}","details":{{"future":{large_unknown},"field":"/title","future":[[[[[null]]]]],"reason":"required"}},"recovery":"r"}}"#,
    );
    let dto: ErrorDto = serde_json::from_str(&wire).unwrap();
    let value = serde_json::to_value(dto).unwrap();
    assert_eq!(
        value["details"],
        json!({"field":"/title","reason":"required"})
    );
}

#[test]
fn public_copy_validation_covers_boundaries_and_prohibited_scalars() {
    let mut base = invalid_argument();
    base["message"] = json!("a".repeat(256));
    assert!(serde_json::from_value::<ErrorDto>(base.clone()).is_ok());
    base["message"] = json!("a".repeat(257));
    assert!(serde_json::from_value::<ErrorDto>(base).is_err());
    for rejected in [
        "",
        "x\u{0}y",
        "x\u{7f}y",
        "x\u{061c}y",
        "x\u{200e}y",
        "x\u{202e}y",
        "x\u{2066}y",
        "x\u{feff}y",
        "x\u{fdd0}y",
        "x\u{10ffff}y",
    ] {
        let mut value = invalid_argument();
        value["message"] = json!(rejected);
        assert!(serde_json::from_value::<ErrorDto>(value).is_err());
    }
}

#[test]
fn bounded_list_nested_duplicates_and_wire_numbers_are_rejected() {
    let duplicate = format!(
        r#"{{"version":1,"code":"GIT_CONFLICT","message":"m","retryable":false,"requestId":"{REQUEST}","details":{{"conflictKind":"merge","relativePaths":{{"items":[],"items":[],"totalCount":0,"truncated":false}}}},"recovery":"r"}}"#
    );
    assert!(serde_json::from_str::<ErrorDto>(&duplicate).is_err());

    let mut size = golden("21");
    size["details"]["limitBytes"] = json!(1);
    assert!(serde_json::from_value::<ErrorDto>(size).is_err());
    let mut entity = golden("11");
    entity["details"]["entityKind"] = json!("workspace");
    entity["details"]["entityId"] = json!("not-a-workspace-id");
    assert!(serde_json::from_value::<ErrorDto>(entity).is_err());
}

#[test]
fn serde_failures_do_not_echo_hostile_field_or_value_text() {
    let mut unknown_code = invalid_argument();
    unknown_code["code"] = json!("SECRET_UNKNOWN_CODE");
    let error = serde_json::from_value::<ErrorDto>(unknown_code)
        .expect_err("unknown code must fail")
        .to_string();
    assert!(!error.contains("SECRET_UNKNOWN_CODE"));

    let wire = format!(
        r#"{{"version":1,"code":"INVALID_ARGUMENT","message":"m","retryable":false,"requestId":"{REQUEST}","SECRET_UNKNOWN_FIELD":1,"details":{{"field":"/title","reason":"required"}},"recovery":"r"}}"#
    );
    let error = serde_json::from_str::<ErrorDto>(&wire)
        .expect_err("unknown top field must fail")
        .to_string();
    assert!(!error.contains("SECRET_UNKNOWN_FIELD"));

    let mut invalid_enum = golden("29");
    invalid_enum["details"]["credentialKind"] = json!("SECRET_CREDENTIAL_KIND");
    let error = serde_json::from_value::<ErrorDto>(invalid_enum)
        .expect_err("unknown closed value must fail")
        .to_string();
    assert!(!error.contains("SECRET_CREDENTIAL_KIND"));
}

fn duplicate_object_field(value: &Value, object_key: Option<&str>, field: &str) -> String {
    let duplicate_value = if let Some(object_key) = object_key {
        value[object_key][field].clone()
    } else {
        value[field].clone()
    };
    let wire = serde_json::to_string(value).unwrap();
    let needle = object_key.map_or_else(|| "{".to_string(), |key| format!(r#""{key}":{{"#));
    let insertion = object_key.map_or_else(
        || {
            format!(
                r#"{{"{field}":{},"#,
                serde_json::to_string(&duplicate_value).unwrap()
            )
        },
        |key| {
            format!(
                r#""{key}":{{"{field}":{},"#,
                serde_json::to_string(&duplicate_value).unwrap()
            )
        },
    );
    wire.replacen(&needle, &insertion, 1)
}

#[test]
fn every_documented_detail_key_has_required_and_optional_wire_evidence() {
    const KEYS: [(&[&str], &[&str]); 36] = [
        (&["field", "reason"], &[]),
        (&["recentWorkspaceId"], &[]),
        (&["reason"], &[]),
        (&["reason"], &["requiredCapability"]),
        (&["limit", "observedAtLeast", "scope"], &[]),
        (&["foundVersion", "minimumVersion", "maximumVersion"], &[]),
        (
            &["relativePath", "diagnosticCode"],
            &["field", "line", "column"],
        ),
        (
            &["relativePath", "diagnosticCode"],
            &["field", "line", "column", "categoryId"],
        ),
        (
            &["relativePath", "diagnosticCode"],
            &["field", "line", "column", "guideId"],
        ),
        (&["relativePath", "reason"], &["field"]),
        (&["entityKind", "entityId", "relativePaths"], &[]),
        (&["categoryId"], &[]),
        (&["guideId"], &[]),
        (&["attachmentId", "guideId"], &[]),
        (
            &[
                "entityKind",
                "entityId",
                "expectedRevision",
                "actualRevision",
            ],
            &[],
        ),
        (
            &["expectedSnapshot", "actualSnapshot", "changedEntityIds"],
            &[],
        ),
        (&["reason"], &["relativePath"]),
        (&["relativePath"], &[]),
        (&["relativePath", "linkCount"], &[]),
        (&["operation"], &["relativePath"]),
        (&["relativePath", "limitBytes", "observedAtLeastBytes"], &[]),
        (&["operation", "timeoutMs"], &[]),
        (
            &["indexGeneration", "progressCompleted"],
            &["progressTotal"],
        ),
        (&["operation", "changedEntityIds"], &[]),
        (&["operation", "planToken", "expiresAt"], &[]),
        (&["operation", "reason"], &[]),
        (&["guideId", "guideRevision", "approvalRequestId"], &[]),
        (&["searchSourceKinds"], &[]),
        (&["reason"], &[]),
        (&["remoteName", "host", "credentialKind"], &[]),
        (&["ahead", "behind", "branch"], &[]),
        (&["conflictKind", "relativePaths"], &[]),
        (&["operation", "exitKind", "sanitizedReason"], &[]),
        (&["foundVersion", "targetVersion"], &[]),
        (&["operationId", "reason", "affectedRelativePaths"], &[]),
        (&["incidentId"], &[]),
    ];
    fn optional_value(key: &str) -> Value {
        match key {
            "requiredCapability" => json!("write"),
            "field" => json!("/title"),
            "line" | "column" => json!(1),
            "categoryId" | "guideId" => json!(REQUEST),
            "relativePath" => json!("guides/a.md"),
            "progressTotal" => json!(100),
            _ => unreachable!(),
        }
    }

    for (index, (required, optional)) in KEYS.iter().enumerate() {
        let number = format!("{:02}", index + 1);
        let value = golden(&number);
        for &key in *required {
            let mut missing = value.clone();
            missing["details"].as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<ErrorDto>(missing).is_err(),
                "{number}/{key} missing"
            );
            let mut null = value.clone();
            null["details"][key] = Value::Null;
            assert!(
                serde_json::from_value::<ErrorDto>(null).is_err(),
                "{number}/{key} null"
            );
            let mut wrong = value.clone();
            wrong["details"][key] = json!({"secret-structural-value": 987654321});
            assert!(
                serde_json::from_value::<ErrorDto>(wrong).is_err(),
                "{number}/{key} type"
            );
            let duplicate = duplicate_object_field(&value, Some("details"), key);
            assert!(
                serde_json::from_str::<ErrorDto>(&duplicate).is_err(),
                "{number}/{key} duplicate"
            );
        }

        for &key in *optional {
            let mut omitted = value.clone();
            omitted["details"].as_object_mut().unwrap().remove(key);
            if matches!(key, "line" | "column") {
                omitted["details"].as_object_mut().unwrap().remove("line");
                omitted["details"].as_object_mut().unwrap().remove("column");
            }
            assert!(
                serde_json::from_value::<ErrorDto>(omitted).is_ok(),
                "{number}/{key} omitted"
            );

            let mut present = value.clone();
            present["details"][key] = optional_value(key);
            if matches!(key, "line" | "column") {
                present["details"]["line"] = json!(1);
                present["details"]["column"] = json!(1);
            }
            assert!(
                serde_json::from_value::<ErrorDto>(present.clone()).is_ok(),
                "{number}/{key} present"
            );
            let mut null = present.clone();
            null["details"][key] = Value::Null;
            assert!(
                serde_json::from_value::<ErrorDto>(null).is_err(),
                "{number}/{key} null"
            );
            let duplicate = duplicate_object_field(&present, Some("details"), key);
            assert!(
                serde_json::from_str::<ErrorDto>(&duplicate).is_err(),
                "{number}/{key} duplicate"
            );
        }

        for key in [
            "version",
            "code",
            "message",
            "retryable",
            "requestId",
            "details",
            "recovery",
        ] {
            let duplicate = duplicate_object_field(&value, None, key);
            assert!(
                serde_json::from_str::<ErrorDto>(&duplicate).is_err(),
                "{number}/{key} duplicate"
            );
        }
        for key in [
            "version",
            "code",
            "message",
            "retryable",
            "requestId",
            "details",
            "recovery",
        ] {
            let mut wrong = value.clone();
            wrong[key] = json!({"secret-structural-value": 987654321});
            assert!(
                serde_json::from_value::<ErrorDto>(wrong).is_err(),
                "{number}/{key} structural type"
            );
        }
    }
}

#[test]
fn malformed_public_error_wires_are_disclosure_safe_at_every_layer() {
    for wire in [
        format!(
            r#"{{"version":987654321,"code":"INVALID_ARGUMENT","message":"m","retryable":false,"requestId":"{REQUEST}","details":{{"field":"/title","reason":"required"}},"recovery":"r"}}"#
        ),
        format!(
            r#"{{"version":1,"code":"INVALID_ARGUMENT","message":"secret-envelope-value","retryable":false,"requestId":"{REQUEST}","details":987654321,"recovery":"r"}}"#
        ),
        format!(
            r#"{{"version":1,"code":"INVALID_ARGUMENT","message":"m","retryable":false,"requestId":"{REQUEST}","details":{{"field":987654321,"reason":"required"}},"recovery":"r"}}"#
        ),
        format!(
            r#"{{"version":1,"code":"GIT_CONFLICT","message":"m","retryable":false,"requestId":"{REQUEST}","details":{{"conflictKind":"merge","relativePaths":{{"items":["%secret-nested-value"],"totalCount":1,"truncated":false}}}},"recovery":"r"}}"#
        ),
    ] {
        let error = serde_json::from_str::<ErrorDto>(&wire)
            .err()
            .unwrap()
            .to_string();
        assert!(error.starts_with("invalid canonical value"), "{error}");
        for content in ["secret", "987654321"] {
            assert!(!error.contains(content), "{error}");
        }
    }
}

#[test]
fn noncatalog_consumer_copy_is_preserved_exactly() {
    let mut value = invalid_argument();
    value["message"] = json!("Safe consumer message.");
    value["recovery"] = json!("Safe consumer recovery.");
    let dto: ErrorDto = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(dto).unwrap(), value);

    let mut git = golden("33");
    git["details"]["sanitizedReason"] = json!("Safe noncatalog Git reason.");
    let dto: ErrorDto = serde_json::from_value(git.clone()).unwrap();
    assert_eq!(serde_json::to_value(dto).unwrap(), git);
}

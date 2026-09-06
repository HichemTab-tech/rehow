use rehow_core::domain::*;
#[test]
fn constructs_complete_mapping_and_preserves_order() {
    let mut b = UnknownYamlDocumentBuilder::new();
    for event in [
        UnknownYamlEvent::BeginMapping,
        UnknownYamlEvent::String("first".into()),
        UnknownYamlEvent::Boolean(true),
        UnknownYamlEvent::String("second".into()),
        UnknownYamlEvent::BeginSequence,
        UnknownYamlEvent::Null,
        UnknownYamlEvent::EndSequence,
        UnknownYamlEvent::EndMapping,
    ] {
        b.push_event(event).unwrap()
    }
    let d = b.finish_document().unwrap();
    assert_eq!(d.entries()[0].key(), "first");
    assert_eq!(d.entries()[1].key(), "second");
    assert_eq!(d.entries()[1].value().as_sequence().unwrap().len(), 1);
}

#[test]
fn every_value_and_scalar_accessor_reports_only_its_own_kind() {
    let integer = CanonicalYamlInteger::new(true, "7".into()).unwrap();
    let float = CanonicalYamlFloat::finite(
        false,
        "15".into(),
        CanonicalYamlInteger::new(true, "1".into()).unwrap(),
    )
    .unwrap();
    let mut builder = open_root();
    for (key, event) in [
        ("null", UnknownYamlEvent::Null),
        ("bool", UnknownYamlEvent::Boolean(true)),
        ("int", UnknownYamlEvent::Integer(integer.clone())),
        ("float", UnknownYamlEvent::Float(float.clone())),
        ("string", UnknownYamlEvent::String("value".into())),
    ] {
        builder
            .push_event(UnknownYamlEvent::String(key.into()))
            .unwrap();
        builder.push_event(event).unwrap();
    }
    builder
        .push_event(UnknownYamlEvent::String("sequence".into()))
        .unwrap();
    builder.push_event(UnknownYamlEvent::BeginSequence).unwrap();
    builder.push_event(UnknownYamlEvent::Null).unwrap();
    builder.push_event(UnknownYamlEvent::EndSequence).unwrap();
    builder
        .push_event(UnknownYamlEvent::String("mapping".into()))
        .unwrap();
    builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
    builder
        .push_event(UnknownYamlEvent::String("inner".into()))
        .unwrap();
    builder.push_event(UnknownYamlEvent::Null).unwrap();
    builder.push_event(UnknownYamlEvent::EndMapping).unwrap();
    builder.push_event(UnknownYamlEvent::EndMapping).unwrap();
    let document = builder.finish_document().unwrap();
    let values = document
        .entries()
        .iter()
        .map(UnknownMetadataEntry::value)
        .collect::<Vec<_>>();

    for value in &values[..5] {
        assert!(value.kind() == UnknownYamlValueKind::Scalar);
        assert!(value.as_scalar().is_some());
        assert!(value.as_sequence().is_none());
        assert!(value.as_mapping().is_none());
    }
    let scalars = values[..5]
        .iter()
        .map(|value| value.as_scalar().unwrap())
        .collect::<Vec<_>>();
    assert!(scalars[0].kind() == UnknownYamlScalarKind::Null);
    assert!(
        scalars[0].as_bool().is_none()
            && scalars[0].as_integer().is_none()
            && scalars[0].as_float().is_none()
            && scalars[0].as_str().is_none()
    );
    assert!(scalars[1].kind() == UnknownYamlScalarKind::Boolean);
    assert_eq!(scalars[1].as_bool(), Some(true));
    assert!(
        scalars[1].as_integer().is_none()
            && scalars[1].as_float().is_none()
            && scalars[1].as_str().is_none()
    );
    assert!(scalars[2].kind() == UnknownYamlScalarKind::Integer);
    assert_eq!(scalars[2].as_integer().unwrap().digits(), integer.digits());
    assert!(
        scalars[2].as_bool().is_none()
            && scalars[2].as_float().is_none()
            && scalars[2].as_str().is_none()
    );
    assert!(scalars[3].kind() == UnknownYamlScalarKind::Float);
    assert_eq!(
        scalars[3].as_float().unwrap().coefficient_digits(),
        float.coefficient_digits()
    );
    assert!(
        scalars[3].as_bool().is_none()
            && scalars[3].as_integer().is_none()
            && scalars[3].as_str().is_none()
    );
    assert!(scalars[4].kind() == UnknownYamlScalarKind::String);
    assert_eq!(scalars[4].as_str(), Some("value"));
    assert!(
        scalars[4].as_bool().is_none()
            && scalars[4].as_integer().is_none()
            && scalars[4].as_float().is_none()
    );

    assert!(values[5].kind() == UnknownYamlValueKind::Sequence);
    assert_eq!(values[5].as_sequence().unwrap().len(), 1);
    assert!(values[5].as_scalar().is_none() && values[5].as_mapping().is_none());
    assert!(values[6].kind() == UnknownYamlValueKind::Mapping);
    let mapping = values[6].as_mapping().unwrap();
    assert_eq!(mapping.len(), 1);
    assert_eq!(
        mapping[0].key().as_scalar().unwrap().as_str(),
        Some("inner")
    );
    assert!(mapping[0].value().as_scalar().is_some());
    assert!(values[6].as_scalar().is_none() && values[6].as_sequence().is_none());
}
#[test]
fn failures_poison_the_builder() {
    let mut b = UnknownYamlDocumentBuilder::new();
    assert!(
        b.push_event(UnknownYamlEvent::Null).unwrap_err().kind()
            == UnknownYamlBuildErrorKind::RootMustBeMapping
    );
    assert!(
        b.push_event(UnknownYamlEvent::BeginMapping)
            .unwrap_err()
            .kind()
            == UnknownYamlBuildErrorKind::Poisoned
    );
}
#[test]
fn canonical_numbers_validate() {
    assert!(
        !CanonicalYamlInteger::new(true, "0".into())
            .unwrap()
            .is_negative()
    );
    assert!(CanonicalYamlInteger::new(false, "01".into()).is_err());
    let zero = CanonicalYamlFloat::finite(
        true,
        "0".into(),
        CanonicalYamlInteger::new(false, "0".into()).unwrap(),
    )
    .unwrap();
    assert_eq!(zero.is_negative(), Some(true));
    assert!(
        CanonicalYamlFloat::positive_infinity().kind() == CanonicalYamlFloatKind::PositiveInfinity
    );
    assert_eq!(
        CanonicalYamlFloat::negative_infinity().is_negative(),
        Some(true)
    );
    assert_eq!(CanonicalYamlFloat::nan().is_negative(), None);
    assert!(
        CanonicalYamlFloat::finite(
            false,
            "10".into(),
            CanonicalYamlInteger::new(false, "0".into()).unwrap()
        )
        .is_err()
    );
}
fn open_root() -> UnknownYamlDocumentBuilder {
    let mut b = UnknownYamlDocumentBuilder::new();
    b.push_event(UnknownYamlEvent::BeginMapping).unwrap();
    b
}
#[test]
fn structural_errors_and_finish_precedence() {
    let b = UnknownYamlDocumentBuilder::new();
    assert!(b.finish_document().is_err());
    let mut b = UnknownYamlDocumentBuilder::new();
    assert!(
        b.push_event(UnknownYamlEvent::EndMapping)
            .expect_err("unmatched end")
            .kind()
            == UnknownYamlBuildErrorKind::UnexpectedEnd
    );
    let mut b = open_root();
    assert!(
        b.push_event(UnknownYamlEvent::Boolean(true))
            .expect_err("root key type")
            .kind()
            == UnknownYamlBuildErrorKind::TopLevelKeyMustBeString
    );
    let mut b = open_root();
    b.push_event(UnknownYamlEvent::String("key".into()))
        .unwrap();
    assert!(
        b.finish_document().err().unwrap().kind()
            == UnknownYamlBuildErrorKind::IncompleteMappingEntry
    );
    let mut b = open_root();
    assert!(
        b.push_event(UnknownYamlEvent::EndSequence)
            .unwrap_err()
            .kind()
            == UnknownYamlBuildErrorKind::MismatchedEnd
    );
    let mut b = open_root();
    b.push_event(UnknownYamlEvent::EndMapping).unwrap();
    assert!(
        b.push_event(UnknownYamlEvent::Null).unwrap_err().kind()
            == UnknownYamlBuildErrorKind::MultipleRoots
    );
}
#[test]
fn exact_collection_and_depth_limits() {
    let mut b = open_root();
    for i in 0..256 {
        b.push_event(UnknownYamlEvent::String(format!("k{i}")))
            .unwrap();
        b.push_event(UnknownYamlEvent::Null).unwrap()
    }
    b.push_event(UnknownYamlEvent::EndMapping).unwrap();
    assert_eq!(b.finish_document().unwrap().entries().len(), 256);
    let mut b = open_root();
    for i in 0..257 {
        let result = b
            .push_event(UnknownYamlEvent::String(format!("k{i}")))
            .and_then(|_| b.push_event(UnknownYamlEvent::Null));
        if i == 256 {
            assert!(result.unwrap_err().kind() == UnknownYamlBuildErrorKind::MappingEntryLimit);
            break;
        } else {
            result.unwrap()
        }
    }
    let mut b = open_root();
    b.push_event(UnknownYamlEvent::String("s".into())).unwrap();
    b.push_event(UnknownYamlEvent::BeginSequence).unwrap();
    for _ in 0..2048 {
        b.push_event(UnknownYamlEvent::Null).unwrap()
    }
    assert!(
        b.push_event(UnknownYamlEvent::Null).unwrap_err().kind()
            == UnknownYamlBuildErrorKind::SequenceEntryLimit
    );
    let mut b = open_root();
    b.push_event(UnknownYamlEvent::String("d".into())).unwrap();
    for _ in 0..15 {
        b.push_event(UnknownYamlEvent::BeginSequence).unwrap()
    }
    assert!(
        b.push_event(UnknownYamlEvent::BeginSequence)
            .unwrap_err()
            .kind()
            == UnknownYamlBuildErrorKind::DepthLimit
    );
}

#[test]
fn exact_sequence_and_depth_limits_close_and_finish_successfully() {
    let mut sequence = open_root();
    sequence
        .push_event(UnknownYamlEvent::String("sequence".into()))
        .unwrap();
    sequence
        .push_event(UnknownYamlEvent::BeginSequence)
        .unwrap();
    for _ in 0..2048 {
        sequence.push_event(UnknownYamlEvent::Null).unwrap();
    }
    sequence.push_event(UnknownYamlEvent::EndSequence).unwrap();
    sequence.push_event(UnknownYamlEvent::EndMapping).unwrap();
    assert_eq!(sequence.finish_document().unwrap().entries().len(), 1);

    let mut depth = open_root();
    depth
        .push_event(UnknownYamlEvent::String("depth".into()))
        .unwrap();
    for _ in 0..15 {
        depth.push_event(UnknownYamlEvent::BeginSequence).unwrap();
    }
    depth.push_event(UnknownYamlEvent::Null).unwrap();
    for _ in 0..15 {
        depth.push_event(UnknownYamlEvent::EndSequence).unwrap();
    }
    depth.push_event(UnknownYamlEvent::EndMapping).unwrap();
    assert_eq!(depth.finish_document().unwrap().entries().len(), 1);
}

#[test]
fn cloned_numeric_occurrences_are_each_charged() {
    let number = CanonicalYamlInteger::new(false, "7".repeat(40_960)).unwrap();
    let mut builder = open_root();
    builder
        .push_event(UnknownYamlEvent::String("numbers".into()))
        .unwrap();
    builder.push_event(UnknownYamlEvent::BeginSequence).unwrap();
    builder
        .push_event(UnknownYamlEvent::Integer(number.clone()))
        .unwrap();
    builder
        .push_event(UnknownYamlEvent::Integer(number))
        .unwrap();
    assert!(
        builder
            .push_event(UnknownYamlEvent::Integer(
                CanonicalYamlInteger::new(false, "1".into()).unwrap()
            ))
            .unwrap_err()
            .kind()
            == UnknownYamlBuildErrorKind::NumericDigitLimit
    );
}
#[test]
fn scalar_numeric_and_noncharacter_limits() {
    let mut b = open_root();
    b.push_event(UnknownYamlEvent::String("a".repeat(65_536)))
        .unwrap();
    b.push_event(UnknownYamlEvent::Null).unwrap();
    b.push_event(UnknownYamlEvent::EndMapping).unwrap();
    assert!(b.finish_document().is_ok());
    let mut b = open_root();
    assert!(
        b.push_event(UnknownYamlEvent::String("a".repeat(65_537)))
            .unwrap_err()
            .kind()
            == UnknownYamlBuildErrorKind::ScalarLimit
    );
    let mut b = open_root();
    assert!(
        b.push_event(UnknownYamlEvent::String("\u{fdd0}".into()))
            .unwrap_err()
            .kind()
            == UnknownYamlBuildErrorKind::InvalidString
    );
    let mut b = open_root();
    b.push_event(UnknownYamlEvent::String("n".into())).unwrap();
    b.push_event(UnknownYamlEvent::BeginSequence).unwrap();
    b.push_event(UnknownYamlEvent::Integer(
        CanonicalYamlInteger::new(false, "1".repeat(65_536)).unwrap(),
    ))
    .unwrap();
    b.push_event(UnknownYamlEvent::Integer(
        CanonicalYamlInteger::new(false, "2".repeat(16_384)).unwrap(),
    ))
    .unwrap();
    assert!(
        b.push_event(UnknownYamlEvent::Integer(
            CanonicalYamlInteger::new(false, "3".into()).unwrap()
        ))
        .unwrap_err()
        .kind()
            == UnknownYamlBuildErrorKind::NumericDigitLimit
    );
}
#[test]
fn duplicate_semantics_cover_order_zero_nan_and_sequence() {
    fn duplicate(k1: Vec<UnknownYamlEvent>, k2: Vec<UnknownYamlEvent>) -> bool {
        let mut b = open_root();
        b.push_event(UnknownYamlEvent::String("nested".into()))
            .unwrap();
        b.push_event(UnknownYamlEvent::BeginMapping).unwrap();
        for e in k1 {
            b.push_event(e).unwrap()
        }
        b.push_event(UnknownYamlEvent::Null).unwrap();
        for e in k2 {
            b.push_event(e).unwrap()
        }
        b.push_event(UnknownYamlEvent::Null).unwrap();
        b.push_event(UnknownYamlEvent::EndMapping).is_err()
    }
    let z = CanonicalYamlInteger::new(false, "0".into()).unwrap();
    assert!(duplicate(
        vec![UnknownYamlEvent::Null],
        vec![UnknownYamlEvent::Null]
    ));
    assert!(duplicate(
        vec![UnknownYamlEvent::Boolean(true)],
        vec![UnknownYamlEvent::Boolean(true)]
    ));
    assert!(!duplicate(
        vec![UnknownYamlEvent::Boolean(false)],
        vec![UnknownYamlEvent::Boolean(true)]
    ));
    assert!(duplicate(
        vec![UnknownYamlEvent::String("same".into())],
        vec![UnknownYamlEvent::String("same".into())]
    ));
    assert!(!duplicate(
        vec![UnknownYamlEvent::String("left".into())],
        vec![UnknownYamlEvent::String("right".into())]
    ));
    assert!(duplicate(
        vec![UnknownYamlEvent::Integer(
            CanonicalYamlInteger::new(true, "1".into()).unwrap()
        )],
        vec![UnknownYamlEvent::Integer(
            CanonicalYamlInteger::new(true, "1".into()).unwrap()
        )]
    ));
    assert!(duplicate(
        vec![UnknownYamlEvent::Float(
            CanonicalYamlFloat::positive_infinity()
        )],
        vec![UnknownYamlEvent::Float(
            CanonicalYamlFloat::positive_infinity()
        )]
    ));
    assert!(!duplicate(
        vec![UnknownYamlEvent::Float(
            CanonicalYamlFloat::positive_infinity()
        )],
        vec![UnknownYamlEvent::Float(
            CanonicalYamlFloat::negative_infinity()
        )]
    ));
    assert!(duplicate(
        vec![UnknownYamlEvent::Float(
            CanonicalYamlFloat::finite(false, "0".into(), z.clone()).unwrap()
        )],
        vec![UnknownYamlEvent::Float(
            CanonicalYamlFloat::finite(true, "0".into(), z).unwrap()
        )]
    ));
    assert!(duplicate(
        vec![UnknownYamlEvent::Float(CanonicalYamlFloat::nan())],
        vec![UnknownYamlEvent::Float(CanonicalYamlFloat::nan())]
    ));
    assert!(!duplicate(
        vec![UnknownYamlEvent::Integer(
            CanonicalYamlInteger::new(false, "1".into()).unwrap()
        )],
        vec![UnknownYamlEvent::Float(
            CanonicalYamlFloat::finite(
                false,
                "1".into(),
                CanonicalYamlInteger::new(false, "0".into()).unwrap()
            )
            .unwrap()
        )]
    ));
}

#[test]
fn complex_key_mapping_order_is_semantic_but_sequence_order_is_not() {
    let mut b = open_root();
    b.push_event(UnknownYamlEvent::String("nested".into()))
        .unwrap();
    b.push_event(UnknownYamlEvent::BeginMapping).unwrap();
    for events in [
        vec![
            UnknownYamlEvent::BeginMapping,
            UnknownYamlEvent::String("a".into()),
            UnknownYamlEvent::Boolean(true),
            UnknownYamlEvent::String("b".into()),
            UnknownYamlEvent::Boolean(false),
            UnknownYamlEvent::EndMapping,
        ],
        vec![
            UnknownYamlEvent::BeginMapping,
            UnknownYamlEvent::String("b".into()),
            UnknownYamlEvent::Boolean(false),
            UnknownYamlEvent::String("a".into()),
            UnknownYamlEvent::Boolean(true),
            UnknownYamlEvent::EndMapping,
        ],
    ] {
        for e in events {
            b.push_event(e).unwrap()
        }
        b.push_event(UnknownYamlEvent::Null).unwrap()
    }
    assert!(
        b.push_event(UnknownYamlEvent::EndMapping)
            .expect_err("duplicate maps")
            .kind()
            == UnknownYamlBuildErrorKind::DuplicateMappingKey
    );
    let mut b = open_root();
    b.push_event(UnknownYamlEvent::String("nested".into()))
        .unwrap();
    b.push_event(UnknownYamlEvent::BeginMapping).unwrap();
    for order in [[true, false], [false, true]] {
        b.push_event(UnknownYamlEvent::BeginSequence).unwrap();
        for v in order {
            b.push_event(UnknownYamlEvent::Boolean(v)).unwrap()
        }
        b.push_event(UnknownYamlEvent::EndSequence).unwrap();
        b.push_event(UnknownYamlEvent::Null).unwrap()
    }
    b.push_event(UnknownYamlEvent::EndMapping).unwrap();
    b.push_event(UnknownYamlEvent::EndMapping).unwrap();
    assert!(b.finish_document().is_ok());
}

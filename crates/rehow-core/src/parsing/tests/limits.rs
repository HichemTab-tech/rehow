use super::super::ParseLimitScope;
use super::super::presentation;
use super::super::reconcile::ScalarPresentationStyle as Style;
use super::super::semantic::{ReducerTestEvent as Event, reduce_events_for_test};
use super::{ParseFailureKind, failure};

#[test]
fn checked_in_limit_descriptors_materialize_exact_and_one_over_natural_sources() {
    let yaml_exact = [
        include_str!("fixtures/accepted/whole-limit-exact.recipe"),
        include_str!("fixtures/accepted/raw-scalar-limit-exact.recipe"),
        include_str!("fixtures/accepted/decoded-scalar-limit-exact.recipe"),
        include_str!("fixtures/accepted/canonical-scalar-limit-exact.recipe"),
        include_str!("fixtures/accepted/numeric-digit-limit-exact.recipe"),
        include_str!("fixtures/accepted/numeric-work-limit-exact.recipe"),
        include_str!("fixtures/accepted/depth-limit-exact.recipe"),
        include_str!("fixtures/accepted/mapping-entry-limit-exact.recipe"),
        include_str!("fixtures/accepted/sequence-entry-limit-exact.recipe"),
        include_str!("fixtures/accepted/node-limit-exact.recipe"),
    ];
    for (index, descriptor) in yaml_exact.into_iter().enumerate() {
        let recipe = super::materialize_fixture_recipe(descriptor);
        assert_eq!(recipe.fact("outcome"), Some("ok"));
        assert!(
            super::parse_yaml_document(recipe.bytes).is_ok(),
            "exact YAML descriptor {index} failed"
        );
    }
    for descriptor in [
        include_str!("fixtures/accepted/guide-limit-exact.recipe"),
        include_str!("fixtures/accepted/frontmatter-limit-exact.recipe"),
    ] {
        let recipe = super::materialize_fixture_recipe(descriptor);
        assert_eq!(recipe.fact("outcome"), Some("ok"));
        assert!(super::parse_guide_source(recipe.bytes).is_ok());
    }

    let yaml_over = [
        (
            include_str!("fixtures/malformed/whole-limit-one-over.recipe"),
            ParseFailureKind::SourceTooLarge,
            ParseLimitScope::WholeYamlBytes,
        ),
        (
            include_str!("fixtures/malformed/raw-scalar-limit-one-over.recipe"),
            ParseFailureKind::RawScalarLimit,
            ParseLimitScope::RawScalarBytes,
        ),
        (
            include_str!("fixtures/malformed/decoded-scalar-limit-one-over.recipe"),
            ParseFailureKind::DecodedScalarLimit,
            ParseLimitScope::DecodedScalarBytes,
        ),
        (
            include_str!("fixtures/malformed/canonical-scalar-limit-one-over.recipe"),
            ParseFailureKind::CanonicalScalarLimit,
            ParseLimitScope::CanonicalScalarBytes,
        ),
        (
            include_str!("fixtures/malformed/numeric-digit-limit-one-over.recipe"),
            ParseFailureKind::NumericDigitLimit,
            ParseLimitScope::NumericDigits,
        ),
        (
            include_str!("fixtures/malformed/numeric-work-limit-one-over.recipe"),
            ParseFailureKind::NumericWorkLimit,
            ParseLimitScope::NumericWork,
        ),
        (
            include_str!("fixtures/malformed/depth-limit-one-over.recipe"),
            ParseFailureKind::DepthLimit,
            ParseLimitScope::Depth,
        ),
        (
            include_str!("fixtures/malformed/mapping-entry-limit-one-over.recipe"),
            ParseFailureKind::MappingEntryLimit,
            ParseLimitScope::MappingEntries,
        ),
        (
            include_str!("fixtures/malformed/sequence-entry-limit-one-over.recipe"),
            ParseFailureKind::SequenceEntryLimit,
            ParseLimitScope::SequenceEntries,
        ),
        (
            include_str!("fixtures/malformed/node-limit-one-over.recipe"),
            ParseFailureKind::NodeLimit,
            ParseLimitScope::Nodes,
        ),
    ];
    for (descriptor, kind, scope) in yaml_over {
        assert_descriptor_failure(descriptor, kind, scope, false);
    }
    for (descriptor, kind, scope) in [
        (
            include_str!("fixtures/malformed/guide-limit-one-over.recipe"),
            ParseFailureKind::SourceTooLarge,
            ParseLimitScope::GuideBytes,
        ),
        (
            include_str!("fixtures/malformed/frontmatter-limit-one-over.recipe"),
            ParseFailureKind::FrontmatterTooLarge,
            ParseLimitScope::FrontmatterBytes,
        ),
    ] {
        assert_descriptor_failure(descriptor, kind, scope, true);
    }
}

fn assert_descriptor_failure(
    descriptor: &'static str,
    kind: ParseFailureKind,
    scope: ParseLimitScope,
    guide: bool,
) {
    let recipe = super::materialize_fixture_recipe(descriptor);
    let expected_limit = recipe.fact("limit").unwrap().parse::<u64>().unwrap();
    let expected_observed = recipe.fact("observed").unwrap().parse::<u64>().unwrap();
    let result = if guide {
        super::parse_guide_source(recipe.bytes).map(|_| ())
    } else {
        super::parse_yaml_document(recipe.bytes).map(|_| ())
    };
    let error = result.expect_err("one-over descriptor must fail");
    assert_eq!(error.kind(), kind);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), scope);
    assert_eq!(facts.limit(), expected_limit);
    assert_eq!(facts.observed_at_least(), expected_observed);
}

#[test]
fn byte_and_raw_scalar_limits_have_exact_facts() {
    let error = failure(&vec![b'a'; 256 * 1024 + 1]);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::WholeYamlBytes);
    assert_eq!(facts.limit(), 256 * 1024);
    assert_eq!(facts.observed_at_least(), 256 * 1024 + 1);
    let mut bytes = b"key: ".to_vec();
    bytes.extend(vec![b'a'; 65_537]);
    bytes.push(b'\n');
    assert_eq!(failure(&bytes).kind(), ParseFailureKind::RawScalarLimit);
}

#[test]
fn raw_scalar_limit_accepts_equality_and_refuses_one_over() {
    let mut exact = b"key: ".to_vec();
    exact.extend(core::iter::repeat_n(b'a', 65_536));
    exact.push(b'\n');
    assert!(super::parse_yaml_document(exact.into_boxed_slice()).is_ok());

    let mut over = b"key: ".to_vec();
    over.extend(core::iter::repeat_n(b'a', 65_537));
    over.push(b'\n');
    let error = failure(&over);
    let facts = error.limit().unwrap();
    assert_eq!(error.kind(), ParseFailureKind::RawScalarLimit);
    assert_eq!(facts.scope(), ParseLimitScope::RawScalarBytes);
    assert_eq!(facts.limit(), 65_536);
    assert_eq!(facts.observed_at_least(), 65_537);

    let mut far_over = b"key: ".to_vec();
    far_over.extend(core::iter::repeat_n(b'a', 70_000));
    far_over.push(b'\n');
    let error = failure(&far_over);
    let facts = error.limit().unwrap();
    assert_eq!(error.kind(), ParseFailureKind::RawScalarLimit);
    assert_eq!(facts.scope(), ParseLimitScope::RawScalarBytes);
    assert_eq!(facts.limit(), 65_536);
    assert_eq!(facts.observed_at_least(), 65_537);
}

#[test]
fn whole_yaml_byte_limit_accepts_equality_and_reports_first_refused() {
    use super::super::source::{MAX_YAML_BYTES, scan_whole};

    assert!(scan_whole(&vec![b'a'; MAX_YAML_BYTES]).is_ok());
    let error = match scan_whole(&vec![b'a'; MAX_YAML_BYTES + 1]) {
        Err(error) => error,
        Ok(_) => panic!("oversized whole YAML accepted"),
    };
    let facts = error.limit().unwrap();
    assert_eq!(error.kind(), ParseFailureKind::SourceTooLarge);
    assert_eq!(facts.scope(), ParseLimitScope::WholeYamlBytes);
    assert_eq!(facts.limit(), MAX_YAML_BYTES as u64);
    assert_eq!(facts.observed_at_least(), (MAX_YAML_BYTES + 1) as u64);

    let error = match scan_whole(&vec![b'a'; MAX_YAML_BYTES + 8_192]) {
        Err(error) => error,
        Ok(_) => panic!("far-oversized whole YAML accepted"),
    };
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::WholeYamlBytes);
    assert_eq!(facts.limit(), MAX_YAML_BYTES as u64);
    assert_eq!(facts.observed_at_least(), (MAX_YAML_BYTES + 1) as u64);
}

#[test]
fn canonical_scalar_limit_accepts_equality_and_refuses_one_over() {
    let mut exact = b"value: .".to_vec();
    exact.extend(core::iter::repeat_n(b'1', 65_531));
    exact.push(b'\n');
    assert!(super::parse_yaml_document(exact.into_boxed_slice()).is_ok());

    let mut over = b"value: .".to_vec();
    over.extend(core::iter::repeat_n(b'1', 65_532));
    over.push(b'\n');
    let error = failure(&over);
    assert_eq!(error.kind(), ParseFailureKind::CanonicalScalarLimit);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::CanonicalScalarBytes);
    assert_eq!(facts.limit(), 65_536);
    assert_eq!(facts.observed_at_least(), 65_537);
}

#[test]
fn aggregate_numeric_digits_accept_equality_and_refuse_one_over() {
    let mut exact = b"large: .".to_vec();
    exact.extend(core::iter::repeat_n(b'1', 65_531));
    exact.extend_from_slice(b"\nrest: ");
    exact.extend(core::iter::repeat_n(b'1', 16_384));
    exact.push(b'\n');
    assert!(super::parse_yaml_document(exact.into_boxed_slice()).is_ok());

    let mut over = b"large: .".to_vec();
    over.extend(core::iter::repeat_n(b'1', 65_531));
    over.extend_from_slice(b"\nrest: ");
    over.extend(core::iter::repeat_n(b'1', 16_385));
    over.push(b'\n');
    let error = failure(&over);
    assert_eq!(error.kind(), ParseFailureKind::NumericDigitLimit);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::NumericDigits);
    assert_eq!(facts.limit(), 80 * 1024);
    assert_eq!(facts.observed_at_least(), 80 * 1024 + 1);
}

#[test]
fn natural_numeric_work_accepts_exact_equality_and_refuses_one_more_source_digit() {
    let source = |decimal_digits: usize| {
        let mut bytes = b"radix: 0x".to_vec();
        bytes.extend(core::iter::repeat_n(b'F', 4_030));
        bytes.extend_from_slice(b"\ndecimal: ");
        bytes.extend(core::iter::repeat_n(b'1', decimal_digits));
        bytes.push(b'\n');
        bytes
    };
    assert!(super::parse_yaml_document(source(62_598).into_boxed_slice()).is_ok());
    let error = failure(&source(62_599));
    assert_eq!(error.kind(), ParseFailureKind::NumericWorkLimit);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::NumericWork);
    assert_eq!(facts.limit(), 16 * 1024 * 1024);
    assert_eq!(facts.observed_at_least(), 16 * 1024 * 1024 + 8);
}

#[test]
fn huge_radix_top_level_key_is_rejected_before_numeric_work() {
    let source = "x: value\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let scalar = |text: String| Event::Scalar {
        text,
        style: Style::Plain,
        tag: None,
        invalid_tag: false,
    };
    let events = vec![
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::MappingStart,
        scalar(format!("0x{}", "f".repeat(4_096))),
        scalar("value".to_owned()),
        Event::MappingEnd,
        Event::DocumentEnd,
        Event::StreamEnd,
    ];
    assert_eq!(
        reduce_events_for_test(5_000, &parsed, events, None, false).failure,
        Some(ParseFailureKind::TopLevelKeyMustBeString)
    );
}

#[test]
fn semantic_depth_and_collection_limits_accept_equality_and_refuse_one_over() {
    let exact_depth = format!("value: {}x{}\n", "[".repeat(15), "]".repeat(15));
    assert!(super::parse_yaml_document(exact_depth.into_bytes().into_boxed_slice()).is_ok());
    let over_depth = format!("value: {}x{}\n", "[".repeat(16), "]".repeat(16));
    let error = failure(over_depth.as_bytes());
    assert_eq!(error.kind(), ParseFailureKind::DepthLimit);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::Depth);
    assert_eq!(facts.limit(), 16);
    assert_eq!(facts.observed_at_least(), 17);

    let mut exact_mapping = String::new();
    for index in 0..256 {
        exact_mapping.push_str(&format!("k{index}: x\n"));
    }
    assert!(super::parse_yaml_document(exact_mapping.into_bytes().into_boxed_slice()).is_ok());
    let mut over_mapping = String::new();
    for index in 0..257 {
        over_mapping.push_str(&format!("k{index}: x\n"));
    }
    let error = failure(over_mapping.as_bytes());
    assert_eq!(error.kind(), ParseFailureKind::MappingEntryLimit);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::MappingEntries);
    assert_eq!(facts.limit(), 256);
    assert_eq!(facts.observed_at_least(), 257);

    let mut exact_sequence = String::from("values:\n");
    for _ in 0..2_048 {
        exact_sequence.push_str("  - x\n");
    }
    assert!(super::parse_yaml_document(exact_sequence.into_bytes().into_boxed_slice()).is_ok());
    let mut over_sequence = String::from("values:\n");
    for _ in 0..2_049 {
        over_sequence.push_str("  - x\n");
    }
    let error = failure(over_sequence.as_bytes());
    assert_eq!(error.kind(), ParseFailureKind::SequenceEntryLimit);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::SequenceEntries);
    assert_eq!(facts.limit(), 2_048);
    assert_eq!(facts.observed_at_least(), 2_049);
}

fn node_limit_source(last_group: usize) -> Vec<u8> {
    let mut source = String::from("values:\n");
    for group in 0..10 {
        source.push_str("  -\n");
        let items = if group == 9 { last_group } else { 2_000 };
        for _ in 0..items {
            source.push_str("    - x\n");
        }
    }
    source.into_bytes()
}

#[test]
fn semantic_node_limit_accepts_equality_and_refuses_one_over() {
    let exact = node_limit_source(1_987);
    assert!(super::parse_yaml_document(exact.into_boxed_slice()).is_ok());
    let over = node_limit_source(1_988);
    let error = failure(&over);
    assert_eq!(error.kind(), ParseFailureKind::NodeLimit);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::Nodes);
    assert_eq!(facts.limit(), 20_000);
    assert_eq!(facts.observed_at_least(), 20_001);
}

#[test]
fn decoded_scalar_limit_accepts_equality_and_refuses_one_over() {
    let mut exact = b"value: \"".to_vec();
    for _ in 0..21_845 {
        exact.extend_from_slice(b"\\L");
    }
    exact.extend_from_slice(b"a\"\n");
    assert!(super::parse_yaml_document(exact.into_boxed_slice()).is_ok());

    let mut over = b"value: \"".to_vec();
    for _ in 0..21_845 {
        over.extend_from_slice(b"\\L");
    }
    over.extend_from_slice(b"aa\"\n");
    let error = failure(&over);
    assert_eq!(error.kind(), ParseFailureKind::DecodedScalarLimit);
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::DecodedScalarBytes);
    assert_eq!(facts.limit(), 65_536);
    assert_eq!(facts.observed_at_least(), 65_537);
}

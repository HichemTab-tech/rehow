use super::super::presentation;
use super::super::reconcile::{ScalarPresentationStyle as Style, SourceTag};
use super::super::semantic::{
    ReducerTestEvent as Event, borrowed_meter_for_test, borrowed_retention_inventory_for_test,
    borrowed_text_limit_for_test, builder_overflow_scope_for_test,
    fold_scalar_index_fault_for_test, force_event_call_bound_for_test,
    forced_borrowed_limit_failure_for_test, map_builder_for_test, mirror_builder_mismatch_for_test,
    production_builder_limit_for_test, production_builder_overflow_for_test,
    production_copy_inventory_for_test, reduce_events_for_test,
};
use super::super::{ParseFailureKind, ParseLimitScope};
use crate::domain::UnknownYamlBuildErrorKind as BuilderKind;

fn scalar(text: impl Into<String>) -> Event {
    Event::Scalar {
        text: text.into(),
        style: Style::Plain,
        tag: None,
        invalid_tag: false,
    }
}

fn tagged_scalar(text: impl Into<String>, tag: SourceTag) -> Event {
    Event::Scalar {
        text: text.into(),
        style: Style::Plain,
        tag: Some(tag),
        invalid_tag: false,
    }
}

fn mapping(key: impl Into<String>, value: impl Into<String>) -> Vec<Event> {
    vec![
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::MappingStart,
        scalar(key),
        scalar(value),
        Event::MappingEnd,
        Event::DocumentEnd,
        Event::StreamEnd,
    ]
}

fn reduce(source: &str, events: Vec<Event>) -> Option<ParseFailureKind> {
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    reduce_events_for_test(source.len(), &parsed, events, None, false).failure
}

fn tail_guard_case(source_key: &str, event_key: &str) -> Option<ParseFailureKind> {
    let mut source = String::from("a:\n");
    for _ in 0..20_000 {
        source.push_str("  - x\n");
    }
    source.push_str("  -\n    ");
    source.push_str(source_key);
    source.push_str(": v\n");
    let parsed = presentation::inspect(&source, source.as_bytes(), 0).unwrap();
    let mut events = Vec::with_capacity(20_014);
    events.extend([
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::MappingStart,
        scalar("a"),
        Event::SequenceStart,
    ]);
    events.extend((0..20_000).map(|_| scalar("x")));
    events.extend([
        Event::MappingStart,
        scalar(event_key),
        scalar("v"),
        Event::MappingEnd,
        Event::SequenceEnd,
        Event::MappingEnd,
        Event::DocumentEnd,
        Event::StreamEnd,
    ]);
    reduce_events_for_test(source.len(), &parsed, events, None, false).failure
}

fn tail_tag_case(source_tagged: bool, event_tagged: bool) -> Option<ParseFailureKind> {
    let mut source = String::from("a:\n");
    for _ in 0..20_000 {
        source.push_str("  - x\n");
    }
    source.push_str(if source_tagged {
        "  -\n    x: !!str v\n"
    } else {
        "  -\n    x: v\n"
    });
    let parsed = presentation::inspect(&source, source.as_bytes(), 0).unwrap();
    let mut events = Vec::with_capacity(20_014);
    events.extend([
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::MappingStart,
        scalar("a"),
        Event::SequenceStart,
    ]);
    events.extend((0..20_000).map(|_| scalar("x")));
    events.extend([
        Event::MappingStart,
        scalar("x"),
        if event_tagged {
            tagged_scalar("v", SourceTag::String)
        } else {
            scalar("v")
        },
        Event::MappingEnd,
        Event::SequenceEnd,
        Event::MappingEnd,
        Event::DocumentEnd,
        Event::StreamEnd,
    ]);
    reduce_events_for_test(source.len(), &parsed, events, None, false).failure
}

#[test]
fn tail_descriptor_layout_is_fixed() {
    assert_eq!(presentation::tail_descriptor_layout(), (16, 4));
}

#[test]
fn production_fold_attributes_all_builder_overflows_to_the_injected_active_meter() {
    let expected = [
        ParseLimitScope::Nodes,
        ParseLimitScope::NumericDigits,
        ParseLimitScope::SemanticEncodingBytes,
        ParseLimitScope::SemanticComparisonBytes,
    ];
    for (scope, expected) in expected.into_iter().enumerate() {
        assert_eq!(
            production_builder_overflow_for_test(scope as u8),
            (ParseFailureKind::MeterArithmeticOverflow, Some(expected))
        );
    }
}

#[test]
fn production_fold_exercises_exact_and_one_over_for_all_four_active_meters() {
    let expected = [
        (ParseFailureKind::NodeLimit, ParseLimitScope::Nodes),
        (
            ParseFailureKind::NumericDigitLimit,
            ParseLimitScope::NumericDigits,
        ),
        (
            ParseFailureKind::SemanticEncodingLimit,
            ParseLimitScope::SemanticEncodingBytes,
        ),
        (
            ParseFailureKind::SemanticComparisonLimit,
            ParseLimitScope::SemanticComparisonBytes,
        ),
    ];
    for (scope, (kind, limit_scope)) in expected.into_iter().enumerate() {
        assert_eq!(
            production_builder_limit_for_test(scope as u8),
            (kind, Some(limit_scope), true),
            "meter {scope} must accept equality, reject the next real charge, and poison"
        );
    }
}

#[test]
fn guard_source_none_event_merge() {
    assert_eq!(
        reduce("x: v\n", mapping("<<", "v")),
        Some(ParseFailureKind::ParserTagDisagreement)
    );
    assert_eq!(
        tail_guard_case("x", "<<"),
        Some(ParseFailureKind::ParserTagDisagreement)
    );
}

#[test]
fn guard_source_merge_event_none() {
    assert_eq!(
        reduce("<<: v\n", mapping("x", "v")),
        Some(ParseFailureKind::ParserTagDisagreement)
    );
    assert_eq!(
        tail_guard_case("<<", "x"),
        Some(ParseFailureKind::ParserTagDisagreement)
    );
}

#[test]
fn guard_shifted_saphyr_only_merge() {
    let events = vec![
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::MappingStart,
        scalar("a"),
        Event::MappingStart,
        scalar("<<"),
        scalar("v"),
        Event::MappingEnd,
        Event::MappingEnd,
        Event::DocumentEnd,
        Event::StreamEnd,
    ];
    assert_eq!(
        reduce("a: [x]\n", events),
        Some(ParseFailureKind::ParserTagDisagreement)
    );
}

#[test]
fn guard_shifted_saphyr_only_merge_after_node_20000() {
    let mut source = String::from("a:\n");
    for _ in 0..20_000 {
        source.push_str("  - x\n");
    }
    source.push_str("  - {x: v}\n");
    let parsed = presentation::inspect(&source, source.as_bytes(), 0).unwrap();

    let mut events = Vec::with_capacity(20_016);
    events.extend([
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::MappingStart,
        scalar("a"),
        Event::SequenceStart,
    ]);
    events.extend((0..20_000).map(|_| scalar("x")));
    events.extend([
        Event::MappingStart,
        Event::MappingStart,
        scalar("<<"),
        scalar("v"),
        Event::MappingEnd,
        scalar("outer-value"),
        Event::MappingEnd,
        Event::SequenceEnd,
        Event::MappingEnd,
        Event::DocumentEnd,
        Event::StreamEnd,
    ]);
    let result = reduce_events_for_test(source.len(), &parsed, events, None, false);
    assert_eq!(
        result.failure,
        Some(ParseFailureKind::ParserTagDisagreement)
    );
}

#[test]
fn explicit_tag_asymmetry_is_detected_before_and_after_node_20000() {
    let mut before = mapping("x", "v");
    before[4] = tagged_scalar("v", SourceTag::String);
    assert_eq!(
        reduce("x: v\n", before),
        Some(ParseFailureKind::ParserTagDisagreement)
    );
    assert_eq!(
        tail_tag_case(false, true),
        Some(ParseFailureKind::ParserTagDisagreement)
    );
    assert_eq!(
        tail_tag_case(true, false),
        Some(ParseFailureKind::ParserTagDisagreement)
    );
}

#[test]
fn tail_borrowed_text_repeated_scalar_maxima_at_limit() {
    let first = "a".repeat(65_536);
    let second = "b".repeat(65_536);
    assert_eq!(
        borrowed_meter_for_test(131_072, 0, &[&first, &second]),
        (131_072, 131_072, false)
    );
}

#[test]
fn expansion_heavy_production_path_keeps_exactly_two_decoder_copies() {
    let source = format!("value: \"{}a\"\n", "\\L".repeat(21_845));
    let parsed = presentation::inspect(&source, source.as_bytes(), 0).unwrap();
    let (presentation_decoded, semantic_decoded, raw, visited, borrowed_limit) =
        production_copy_inventory_for_test(&source, &parsed);
    assert_eq!(visited, semantic_decoded as u64);
    assert!(visited <= borrowed_limit);
    let nodes = parsed.nodes.len();
    assert!(
        presentation_decoded
            .checked_add(semantic_decoded)
            .is_some_and(|copies| copies <= 3 * source.len() + 2 * nodes)
    );
    assert!(
        presentation_decoded
            .checked_add(semantic_decoded)
            .and_then(|copies| copies.checked_add(raw))
            .is_some_and(|copies| copies <= 4 * source.len() + 2 * nodes)
    );
}

#[test]
fn borrowed_retention_copies_during_the_only_instrumented_source_traversal() {
    let decoded = format!("{}a", "\u{2028}".repeat(21_845));
    let source = format!("value: \"{}a\"\n", "\\L".repeat(21_845));
    let parsed = presentation::inspect(&source, source.as_bytes(), 0).unwrap();
    let expected = u64::try_from("value".len() + decoded.len()).unwrap();
    let (failure, source_visits, inspected, retained, copy_attempts, copy_bytes) =
        borrowed_retention_inventory_for_test(
            source.len(),
            &parsed,
            "value",
            &decoded,
            None,
            false,
        );
    assert_eq!(failure, None);
    assert_eq!(source_visits, expected);
    assert_eq!(inspected, expected);
    assert_eq!(retained, expected as usize);
    assert_eq!(copy_attempts, 2);
    assert_eq!(copy_bytes, expected as usize);
}

#[test]
fn borrowed_one_over_refuses_current_scalar_before_retention_allocation() {
    let value = "expanded";
    let source = "key: expanded\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let limit = borrowed_text_limit_for_test(source.len(), parsed.descriptor_count()).unwrap();
    let seed = limit - "key".len() as u64 - value.len() as u64 + 1;
    let (failure, visits, inspected, retained, copy_attempts, copy_bytes) =
        borrowed_retention_inventory_for_test(
            source.len(),
            &parsed,
            "key",
            value,
            Some(seed),
            false,
        );
    assert_eq!(failure, Some(ParseFailureKind::SemanticBuilderInvariant));
    assert_eq!(visits, "key".len() as u64);
    assert_eq!(inspected, "key".len() as u64);
    assert_eq!(retained, 0);
    assert_eq!(copy_attempts, 1);
    assert_eq!(copy_bytes, "key".len());
}

#[test]
fn decoded_scalar_one_over_is_inspected_without_retention_allocation() {
    let value = "x".repeat(65_537);
    let source = "key: value\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let (failure, visits, inspected, retained, copy_attempts, copy_bytes) =
        borrowed_retention_inventory_for_test(70_000, &parsed, "key", &value, None, false);
    assert_eq!(failure, Some(ParseFailureKind::DecodedScalarLimit));
    assert_eq!(visits, ("key".len() + value.len()) as u64);
    assert_eq!(inspected, visits);
    assert_eq!(retained, 0);
    assert_eq!(copy_attempts, 1);
    assert_eq!(copy_bytes, "key".len());
}

#[test]
fn forced_borrowed_reservation_failure_disables_current_and_later_visits() {
    let source = "key: value\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let (failure, visits, _inspected, retained, copy_attempts, copy_bytes) =
        borrowed_retention_inventory_for_test(source.len(), &parsed, "key", "value", None, true);
    assert_eq!(failure, Some(ParseFailureKind::SemanticBuilderInvariant));
    assert_eq!(visits, 0);
    assert_eq!(retained, 0);
    assert_eq!(copy_attempts, 1);
    assert_eq!(copy_bytes, "key".len());
}

#[test]
fn expansion_heavy_production_reducer_accepts_borrowed_equality_and_refuses_one_over() {
    let decoded = format!("{}a", "\u{2028}".repeat(21_845));
    let source = format!("value: \"{}\"\n", "\\L".repeat(21_845));
    let parsed = presentation::inspect(&source, source.as_bytes(), 0).unwrap();
    let limit = borrowed_text_limit_for_test(source.len(), parsed.descriptor_count()).unwrap();
    let decoded_total = u64::try_from("value".len() + decoded.len()).unwrap();
    let equality_seed = limit.checked_sub(decoded_total).unwrap();
    let equality = reduce_events_for_test(
        source.len(),
        &parsed,
        mapping("value", decoded.clone()),
        Some(equality_seed),
        false,
    );
    assert_eq!(equality.failure, None);
    assert_eq!(equality.borrowed_used, limit);
    assert_eq!(equality.inspected, decoded_total);

    let one_over = reduce_events_for_test(
        source.len(),
        &parsed,
        mapping("value", decoded),
        Some(equality_seed + 1),
        false,
    );
    assert_eq!(
        one_over.failure,
        Some(ParseFailureKind::SemanticBuilderInvariant)
    );
    assert!(one_over.disabled);
    assert_eq!(one_over.inspected, "value".len() as u64);
}

#[test]
fn tail_borrowed_text_one_over_is_invariant() {
    let text = "a".repeat(65_537);
    assert_eq!(borrowed_meter_for_test(65_536, 0, &[&text]), (0, 0, true));
}

#[test]
fn tail_borrowed_text_counter_overflow_is_invariant() {
    assert_eq!(
        borrowed_meter_for_test(u64::MAX, u64::MAX, &["x"]),
        (u64::MAX, 0, true)
    );
}

#[test]
fn tail_borrowed_text_limit_arithmetic_overflow_is_invariant() {
    assert_eq!(borrowed_text_limit_for_test(usize::MAX, usize::MAX), None);
}

#[test]
fn tail_borrowed_text_disable_stops_all_later_scans() {
    let result = borrowed_meter_for_test(1, 0, &["xx", "later", "values"]);
    assert_eq!(result, (0, 0, true));
}

#[test]
fn tail_borrowed_text_invariant_then_scan_error() {
    let source = "a: v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let result = reduce_events_for_test(
        source.len(),
        &parsed,
        mapping("a", "v"),
        Some(u64::MAX),
        true,
    );
    assert_eq!(result.failure, Some(ParseFailureKind::SemanticSyntax));
    assert!(result.disabled);
}

#[test]
fn tail_borrowed_text_invariant_then_stream_error() {
    let source = "a\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let events = vec![
        Event::StreamStart,
        Event::DocumentStart(false),
        scalar("a"),
        Event::DocumentStart(false),
    ];
    let result = reduce_events_for_test(source.len(), &parsed, events, Some(u64::MAX), false);
    assert_eq!(result.failure, Some(ParseFailureKind::MultipleDocuments));
}

#[test]
fn tail_borrowed_text_invariant_then_exact_tag_disagreement() {
    let source = "a: v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let mut events = mapping("a", "v");
    events[4] = tagged_scalar("v", SourceTag::String);
    let result = reduce_events_for_test(source.len(), &parsed, events, Some(u64::MAX), false);
    assert_eq!(
        result.failure,
        Some(ParseFailureKind::ParserTagDisagreement)
    );
}

#[test]
fn tail_borrowed_text_invariant_then_shifted_source_guard() {
    let source = "a: !!str v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let events = vec![
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::SequenceStart,
        scalar("x"),
        Event::SequenceEnd,
        Event::DocumentEnd,
        Event::StreamEnd,
    ];
    let result = reduce_events_for_test(source.len(), &parsed, events, Some(u64::MAX), false);
    assert_eq!(
        result.failure,
        Some(ParseFailureKind::ParserTagDisagreement)
    );
}

#[test]
fn tail_borrowed_text_invariant_then_uninspectable_merge() {
    let source = "a: v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let result = reduce_events_for_test(
        source.len(),
        &parsed,
        mapping("a", "v"),
        Some(u64::MAX),
        false,
    );
    assert_eq!(
        result.failure,
        Some(ParseFailureKind::SemanticBuilderInvariant)
    );
    assert_eq!(result.inspected, 0);
}

#[test]
fn builder_arithmetic_overflow_uses_the_active_meter_scope() {
    assert_eq!(builder_overflow_scope_for_test(0), ParseLimitScope::Nodes);
    assert_eq!(
        builder_overflow_scope_for_test(1),
        ParseLimitScope::NumericDigits
    );
    assert_eq!(
        builder_overflow_scope_for_test(2),
        ParseLimitScope::SemanticEncodingBytes
    );
    assert_eq!(
        builder_overflow_scope_for_test(3),
        ParseLimitScope::SemanticComparisonBytes
    );
}

#[test]
fn every_builder_error_kind_maps_to_the_normative_failure() {
    use BuilderKind as B;
    use ParseFailureKind as P;
    let cases = [
        (B::Poisoned, P::SemanticBuilderInvariant, None),
        (B::RootMustBeMapping, P::RootMustBeMapping, None),
        (B::MultipleRoots, P::MultipleDocuments, None),
        (B::UnexpectedEnd, P::SemanticStreamOrder, None),
        (B::MismatchedEnd, P::SemanticStreamOrder, None),
        (B::IncompleteMappingEntry, P::SemanticStreamOrder, None),
        (B::IncompleteDocument, P::SemanticStreamOrder, None),
        (B::TopLevelKeyMustBeString, P::TopLevelKeyMustBeString, None),
        (B::DuplicateMappingKey, P::DuplicateMappingKey, None),
        (B::InvalidString, P::InvalidUnicodeScalar, None),
        (B::DepthLimit, P::DepthLimit, Some(ParseLimitScope::Depth)),
        (
            B::MappingEntryLimit,
            P::MappingEntryLimit,
            Some(ParseLimitScope::MappingEntries),
        ),
        (
            B::SequenceEntryLimit,
            P::SequenceEntryLimit,
            Some(ParseLimitScope::SequenceEntries),
        ),
        (B::NodeLimit, P::NodeLimit, Some(ParseLimitScope::Nodes)),
        (
            B::ScalarLimit,
            P::CanonicalScalarLimit,
            Some(ParseLimitScope::CanonicalScalarBytes),
        ),
        (
            B::NumericDigitLimit,
            P::NumericDigitLimit,
            Some(ParseLimitScope::NumericDigits),
        ),
        (
            B::SemanticEncodingLimit,
            P::SemanticEncodingLimit,
            Some(ParseLimitScope::SemanticEncodingBytes),
        ),
        (
            B::SemanticComparisonLimit,
            P::SemanticComparisonLimit,
            Some(ParseLimitScope::SemanticComparisonBytes),
        ),
    ];
    for (builder, expected_kind, expected_scope) in cases {
        assert_eq!(
            map_builder_for_test(builder),
            (expected_kind, expected_scope)
        );
    }
    assert_eq!(
        map_builder_for_test(B::ArithmeticOverflow).0,
        P::SemanticBuilderInvariant
    );
}

#[test]
fn event_call_bound_is_a_sanitized_builder_invariant() {
    let source = "a: v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    assert_eq!(
        force_event_call_bound_for_test(source.len(), &parsed, mapping("a", "v")),
        ParseFailureKind::SemanticBuilderInvariant
    );
}

#[test]
fn event_call_bound_mid_document_does_not_synthesize_stream_order() {
    let source = "a: v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    assert_eq!(
        force_event_call_bound_for_test(
            source.len(),
            &parsed,
            vec![
                Event::StreamStart,
                Event::DocumentStart(false),
                Event::MappingStart,
            ],
        ),
        ParseFailureKind::SemanticBuilderInvariant
    );
}

#[test]
fn event_call_stop_does_not_infer_unconsumed_tag_or_merge_disagreement() {
    let tagged = "a: !!str value\n";
    let tagged_presentation = presentation::inspect(tagged, tagged.as_bytes(), 0).unwrap();
    assert_eq!(
        force_event_call_bound_for_test(
            tagged.len(),
            &tagged_presentation,
            vec![
                Event::StreamStart,
                Event::DocumentStart(false),
                Event::MappingStart,
                scalar("a"),
            ],
        ),
        ParseFailureKind::SemanticBuilderInvariant
    );

    let merged = "a:\n  <<: value\n";
    let merged_presentation = presentation::inspect(merged, merged.as_bytes(), 0).unwrap();
    assert_eq!(
        force_event_call_bound_for_test(
            merged.len(),
            &merged_presentation,
            vec![
                Event::StreamStart,
                Event::DocumentStart(false),
                Event::MappingStart,
                scalar("a"),
                Event::MappingStart,
            ],
        ),
        ParseFailureKind::SemanticBuilderInvariant
    );
}

#[test]
fn initial_document_start_without_stream_start_is_stream_order() {
    let source = "a: v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let mut events = mapping("a", "v");
    events.remove(0);
    assert_eq!(
        reduce_events_for_test(source.len(), &parsed, events, None, false).failure,
        Some(ParseFailureKind::SemanticStreamOrder)
    );
}

#[test]
fn emitted_event_cap_is_checked_before_any_vector_growth() {
    let source = "{}\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let mut events = vec![
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::MappingStart,
    ];
    events.extend((0..8).map(|index| scalar(format!("s{index}"))));
    events.extend([Event::MappingEnd, Event::DocumentEnd, Event::StreamEnd]);
    let result = reduce_events_for_test(source.len(), &parsed, events, None, false);
    assert_eq!(
        result.failure,
        Some(ParseFailureKind::SemanticBuilderInvariant)
    );
    assert_eq!(result.retained_node_capacity, 7);
    assert!(result.retained_nodes <= 7);
}

#[test]
fn source_builtin_tag_still_disagrees_with_uninspectable_tagless_event() {
    let source = "x: !!str v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let limit = borrowed_text_limit_for_test(source.len(), parsed.descriptor_count()).unwrap();
    let result = reduce_events_for_test(
        source.len(),
        &parsed,
        mapping("x", "v"),
        Some(limit - 1),
        false,
    );
    assert_eq!(
        result.failure,
        Some(ParseFailureKind::ParserTagDisagreement)
    );
}

#[test]
fn borrowed_limit_arithmetic_failure_drains_and_later_scan_wins() {
    let source = "a: v\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    assert_eq!(
        forced_borrowed_limit_failure_for_test(&parsed, mapping("a", "v"), true),
        ParseFailureKind::SemanticSyntax
    );
}

#[test]
fn mirror_builder_mismatch_is_a_sanitized_builder_invariant() {
    assert_eq!(
        mirror_builder_mismatch_for_test(),
        (ParseFailureKind::SemanticBuilderInvariant, 1, 2)
    );
}

#[test]
fn fold_rejects_repeated_missing_reversed_and_out_of_range_scalar_indexes() {
    for case in 0..4 {
        assert_eq!(
            fold_scalar_index_fault_for_test(case),
            ParseFailureKind::SemanticBuilderInvariant
        );
    }
}

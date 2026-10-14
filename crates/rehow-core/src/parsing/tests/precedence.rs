use super::super::combined_stage_8_and_9_for_test;
use super::super::presentation;
use super::super::reconcile::ScalarPresentationStyle as Style;
use super::super::semantic::{
    ReducerTestEvent as Event, adjacent_semantic_precedence_for_test,
    collection_key_depth_precedence_for_test, reduce_events_for_test,
};
use super::{ParseFailureKind, failure, parse_guide_source, parse_yaml_document};

#[test]
fn byte_policy_precedes_parser_failures() {
    assert_eq!(
        failure(b"a: [\r").kind(),
        ParseFailureKind::BareCarriageReturn
    );
    assert_eq!(
        failure(b"a: \xef\xbb\xbf[").kind(),
        ParseFailureKind::RawBomDenied
    );
}

#[test]
fn all_41_failure_kinds_have_one_reachability_class() {
    use ParseFailureKind as P;
    let source = [
        P::SourceTooLarge,
        P::FrontmatterTooLarge,
        P::InvalidUtf8,
        P::RawBomDenied,
        P::BareCarriageReturn,
        P::MixedYamlLineEndings,
        P::MissingGuideOpeningDelimiter,
        P::MissingGuideClosingDelimiter,
        P::PresentationSyntax,
        P::PresentationShape,
        P::DirectiveDenied,
        P::DocumentMarkerDenied,
        P::AnchorDenied,
        P::AliasDenied,
        P::ExplicitTagDenied,
        P::ExplicitTagKindMismatch,
        P::ExplicitTagValueMismatch,
        P::MergeKeyDenied,
        P::InvalidUnicodeScalar,
        P::RootMustBeMapping,
        P::TopLevelKeyMustBeString,
        P::DepthLimit,
        P::MappingEntryLimit,
        P::SequenceEntryLimit,
        P::NodeLimit,
        P::RawScalarLimit,
        P::DecodedScalarLimit,
        P::CanonicalScalarLimit,
        P::NumericWorkLimit,
        P::NumericDigitLimit,
        P::DuplicateMappingKey,
    ];
    let differential = [
        P::PresentationCoverage,
        P::SemanticSyntax,
        P::ParserTagDisagreement,
    ];
    // A closing delimiter may now be followed directly by the body or EOF, so
    // a missing blank separator is accepted; the kind remains only as a
    // defensive scanner invariant (mismatched delimiter endings are reported
    // first as mixed line endings).
    let fault = [
        P::MissingGuideSeparator,
        P::SemanticStreamOrder,
        P::MultipleDocuments,
        P::SemanticEncodingLimit,
        P::SemanticComparisonLimit,
        P::MeterArithmeticOverflow,
        P::SemanticBuilderInvariant,
    ];
    let mut seen = [false; 41];
    for kind in source.into_iter().chain(differential).chain(fault) {
        let slot = &mut seen[kind as usize];
        assert!(
            !*slot,
            "failure kind assigned to multiple reachability classes"
        );
        *slot = true;
    }
    assert!(seen.into_iter().all(|assigned| assigned));
}

#[test]
fn later_scan_failure_wins_an_earlier_semantic_root_failure() {
    let source = "value\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    let events = vec![
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::Scalar {
            text: "value".to_owned(),
            style: Style::Plain,
            tag: None,
            invalid_tag: false,
        },
    ];
    let result = reduce_events_for_test(source.len(), &parsed, events, None, true);
    assert_eq!(result.failure, Some(ParseFailureKind::SemanticSyntax));
}

#[test]
fn later_matched_merge_wins_an_earlier_collection_limit() {
    let mut source = String::from("values:\n");
    for _ in 0..2_049 {
        source.push_str("  - x\n");
    }
    source.push_str("  -\n    <<: value\n");
    assert_eq!(
        failure(source.as_bytes()).kind(),
        ParseFailureKind::MergeKeyDenied
    );
}

#[test]
fn guarded_tail_preserves_or_overrides_the_earlier_node_limit() {
    let mut valid = String::from("values:\n");
    for _ in 0..10 {
        valid.push_str("  -\n");
        for _ in 0..2_000 {
            valid.push_str("    - x\n");
        }
    }
    valid.push_str("  - !!str value\n");
    assert_eq!(
        failure(valid.as_bytes()).kind(),
        ParseFailureKind::NodeLimit
    );

    let mut invalid = String::from("values:\n");
    for _ in 0..10 {
        invalid.push_str("  -\n");
        for _ in 0..2_000 {
            invalid.push_str("    - x\n");
        }
    }
    invalid.push_str("  - !!int nope\n");
    assert_eq!(
        failure(invalid.as_bytes()).kind(),
        ParseFailureKind::ExplicitTagValueMismatch
    );
}

#[test]
fn collection_top_level_key_precedes_same_event_depth_failure() {
    let source = "a: value\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    for mapping in [false, true] {
        assert_eq!(
            collection_key_depth_precedence_for_test(&parsed, mapping),
            ParseFailureKind::TopLevelKeyMustBeString
        );
    }
}

#[test]
fn presentation_syntax_and_shape_have_natural_production_witnesses() {
    assert_eq!(
        presentation::syntax_over_coverage_after_dependency_for_test("a: [\n"),
        ParseFailureKind::PresentationSyntax
    );
    assert_eq!(
        failure(b"a: b: c\n").kind(),
        ParseFailureKind::PresentationShape
    );
}

#[test]
fn all_32_natural_failure_classes_have_production_parse_witnesses() {
    use ParseFailureKind as P;
    let ordinary: &[(&[u8], P)] = &[
        (&[0xff], P::InvalidUtf8),
        (b"a: \xef\xbb\xbf\n", P::RawBomDenied),
        (b"a: b\r", P::BareCarriageReturn),
        (b"a: b\r\nc: d\n", P::MixedYamlLineEndings),
        (
            include_bytes!("fixtures/malformed/presentation-syntax.yaml"),
            P::PresentationSyntax,
        ),
        (
            include_bytes!("fixtures/malformed/presentation-shape.yaml"),
            P::PresentationShape,
        ),
        (
            include_bytes!("fixtures/malformed/directive.yaml"),
            P::DirectiveDenied,
        ),
        (
            include_bytes!("fixtures/malformed/document-marker.yaml"),
            P::DocumentMarkerDenied,
        ),
        (
            include_bytes!("fixtures/malformed/anchor.yaml"),
            P::AnchorDenied,
        ),
        (
            include_bytes!("fixtures/malformed/alias.yaml"),
            P::AliasDenied,
        ),
        (
            include_bytes!("fixtures/malformed/denied-tag.yaml"),
            P::ExplicitTagDenied,
        ),
        (
            include_bytes!("fixtures/malformed/tag-kind.yaml"),
            P::ExplicitTagKindMismatch,
        ),
        (
            include_bytes!("fixtures/malformed/tag-value.yaml"),
            P::ExplicitTagValueMismatch,
        ),
        (
            include_bytes!("fixtures/malformed/merge.yaml"),
            P::MergeKeyDenied,
        ),
        (
            include_bytes!("fixtures/malformed/noncharacter.yaml"),
            P::InvalidUnicodeScalar,
        ),
        (
            include_bytes!("fixtures/malformed/root-scalar.yaml"),
            P::RootMustBeMapping,
        ),
        (
            include_bytes!("fixtures/malformed/top-level-key.yaml"),
            P::TopLevelKeyMustBeString,
        ),
        (
            include_bytes!("fixtures/malformed/duplicate-key.yaml"),
            P::DuplicateMappingKey,
        ),
    ];
    for (source, expected) in ordinary {
        assert_eq!(failure(source).kind(), *expected);
    }

    let guide_cases: &[(&[u8], P)] = &[
        (
            include_bytes!("fixtures/malformed/guide-missing-opening.md"),
            P::MissingGuideOpeningDelimiter,
        ),
        (
            include_bytes!("fixtures/malformed/guide-missing-closing.md"),
            P::MissingGuideClosingDelimiter,
        ),
    ];
    for (source, expected) in guide_cases {
        let error = match parse_guide_source(source.to_vec().into_boxed_slice()) {
            Err(error) => error,
            Ok(_) => panic!("malformed Guide witness accepted"),
        };
        assert_eq!(error.kind(), *expected);
    }

    assert_eq!(
        failure(&vec![b'a'; 256 * 1024 + 1]).kind(),
        P::SourceTooLarge
    );
    let mut frontmatter = b"---\na: ".to_vec();
    frontmatter.extend(core::iter::repeat_n(b'x', 256 * 1024 - 12));
    frontmatter.extend_from_slice(b"\n---\n\n");
    let error = match parse_guide_source(frontmatter.into_boxed_slice()) {
        Err(error) => error,
        Ok(_) => panic!("oversized frontmatter witness accepted"),
    };
    assert_eq!(error.kind(), P::FrontmatterTooLarge);

    let depth = format!("a: {}x{}\n", "[".repeat(16), "]".repeat(16));
    assert_eq!(failure(depth.as_bytes()).kind(), P::DepthLimit);
    let mapping = (0..257)
        .map(|index| format!("k{index}: x\n"))
        .collect::<String>();
    assert_eq!(failure(mapping.as_bytes()).kind(), P::MappingEntryLimit);
    let mut sequence = String::from("a:\n");
    for _ in 0..2_049 {
        sequence.push_str("  - x\n");
    }
    assert_eq!(failure(sequence.as_bytes()).kind(), P::SequenceEntryLimit);
    let mut nodes = String::from("a:\n");
    for _ in 0..10 {
        nodes.push_str("  -\n");
        for _ in 0..2_000 {
            nodes.push_str("    - x\n");
        }
    }
    assert_eq!(failure(nodes.as_bytes()).kind(), P::NodeLimit);

    let mut raw = b"a: ".to_vec();
    raw.extend(core::iter::repeat_n(b'x', 65_537));
    raw.push(b'\n');
    assert_eq!(failure(&raw).kind(), P::RawScalarLimit);
    let mut decoded = b"a: \"".to_vec();
    for _ in 0..21_845 {
        decoded.extend_from_slice(b"\\L");
    }
    decoded.extend_from_slice(b"aa\"\n");
    assert_eq!(failure(&decoded).kind(), P::DecodedScalarLimit);
    let mut canonical = b"a: .".to_vec();
    canonical.extend(core::iter::repeat_n(b'1', 65_532));
    canonical.push(b'\n');
    assert_eq!(failure(&canonical).kind(), P::CanonicalScalarLimit);

    let mut numeric_work = b"radix: 0x".to_vec();
    numeric_work.extend(core::iter::repeat_n(b'F', 4_030));
    numeric_work.extend_from_slice(b"\ndecimal: ");
    numeric_work.extend(core::iter::repeat_n(b'1', 62_599));
    numeric_work.push(b'\n');
    assert_eq!(failure(&numeric_work).kind(), P::NumericWorkLimit);
    let mut digits = b"large: .".to_vec();
    digits.extend(core::iter::repeat_n(b'1', 65_531));
    digits.extend_from_slice(b"\nrest: ");
    digits.extend(core::iter::repeat_n(b'1', 16_385));
    digits.push(b'\n');
    assert_eq!(failure(&digits).kind(), P::NumericDigitLimit);

    assert!(parse_yaml_document(b"a: b\n".to_vec().into_boxed_slice()).is_ok());
    assert!(
        parse_yaml_document(
            include_bytes!("fixtures/accepted/styles-tags-comments.yaml")
                .to_vec()
                .into_boxed_slice()
        )
        .is_ok()
    );
    assert_eq!(
        failure(include_bytes!("fixtures/malformed/root-sequence.yaml")).kind(),
        P::RootMustBeMapping
    );
}

#[test]
fn every_adjacent_global_precedence_boundary_from_stage_5_through_14_is_exact() {
    assert_eq!(
        failure(b"a: [\n").kind(),
        ParseFailureKind::PresentationSyntax
    );
    assert_eq!(
        presentation::adjacent_pending_precedence_for_test(6),
        ParseFailureKind::PresentationCoverage
    );
    assert_eq!(
        presentation::adjacent_pending_precedence_for_test(7),
        ParseFailureKind::DirectiveDenied
    );
    assert_eq!(
        presentation::adjacent_pending_precedence_for_test(8),
        ParseFailureKind::PresentationShape
    );
    assert_eq!(
        combined_stage_8_and_9_for_test("a: b\n"),
        ParseFailureKind::PresentationShape
    );
    let source = "a: b\n";
    let parsed = presentation::inspect(source, source.as_bytes(), 0).unwrap();
    for (stage, expected) in [
        (9, ParseFailureKind::SemanticSyntax),
        (10, ParseFailureKind::SemanticStreamOrder),
        (11, ParseFailureKind::ParserTagDisagreement),
        (12, ParseFailureKind::MergeKeyDenied),
        (13, ParseFailureKind::RootMustBeMapping),
    ] {
        assert_eq!(
            adjacent_semantic_precedence_for_test(&parsed, stage),
            expected
        );
    }
}

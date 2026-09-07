use super::super::{ReconciledNodeKind, RoundTripStatus};
use super::{ParseFailureKind, failure, parse_yaml_document};

#[test]
fn all_50_syntax_kinds_hit_both_exhaustive_arms() {
    use yaml_edit::SyntaxKind;

    let kinds = [
        SyntaxKind::ROOT,
        SyntaxKind::DOCUMENT,
        SyntaxKind::SEQUENCE,
        SyntaxKind::MAPPING,
        SyntaxKind::SCALAR,
        SyntaxKind::ALIAS,
        SyntaxKind::TAGGED_NODE,
        SyntaxKind::ERROR,
        SyntaxKind::DASH,
        SyntaxKind::PLUS,
        SyntaxKind::COLON,
        SyntaxKind::QUESTION,
        SyntaxKind::LEFT_BRACKET,
        SyntaxKind::RIGHT_BRACKET,
        SyntaxKind::LEFT_BRACE,
        SyntaxKind::RIGHT_BRACE,
        SyntaxKind::COMMA,
        SyntaxKind::PIPE,
        SyntaxKind::GREATER,
        SyntaxKind::AMPERSAND,
        SyntaxKind::ASTERISK,
        SyntaxKind::EXCLAMATION,
        SyntaxKind::PERCENT,
        SyntaxKind::AT,
        SyntaxKind::BACKTICK,
        SyntaxKind::QUOTE,
        SyntaxKind::SINGLE_QUOTE,
        SyntaxKind::DOC_START,
        SyntaxKind::DOC_END,
        SyntaxKind::KEY,
        SyntaxKind::VALUE,
        SyntaxKind::MAPPING_ENTRY,
        SyntaxKind::SEQUENCE_ENTRY,
        SyntaxKind::STRING,
        SyntaxKind::UNTERMINATED_STRING,
        SyntaxKind::INT,
        SyntaxKind::FLOAT,
        SyntaxKind::BOOL,
        SyntaxKind::NULL,
        SyntaxKind::TAG,
        SyntaxKind::ANCHOR,
        SyntaxKind::REFERENCE,
        SyntaxKind::MERGE_KEY,
        SyntaxKind::DIRECTIVE,
        SyntaxKind::WHITESPACE,
        SyntaxKind::NEWLINE,
        SyntaxKind::INDENT,
        SyntaxKind::COMMENT,
        SyntaxKind::BOM,
        SyntaxKind::EOF,
    ];
    let expected_nodes = [
        0, 0, 0, 0, 0, 1, 0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 0,
        0, 0, 0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    ];
    let expected_tokens = [
        0, 0, 0, 0, 0, 0, 0, 0, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 3, 4, 7, 7, 7, 7, 7, 7, 2, 2, 0,
        0, 0, 0, 7, 6, 7, 7, 7, 7, 7, 3, 4, 7, 1, 7, 7, 7, 7, 5, 7,
    ];
    assert_eq!(kinds.len(), 50);
    for (index, kind) in kinds.into_iter().enumerate() {
        assert_eq!(
            super::super::presentation::exhaustive_kind_outcomes(kind),
            (expected_nodes[index], expected_tokens[index]),
            "wrong exhaustive classifications at index {index}"
        );
    }
}

#[test]
fn positioned_error_selection_uses_valid_ranges_and_exact_ties() {
    use yaml_edit::ParseErrorKind;

    let selected = super::super::presentation::positioned_error_for_test(
        "a\nbc\n",
        &[
            (5, 4, ParseErrorKind::UnclosedFlowSequence),
            (2, 4, ParseErrorKind::Other),
            (2, 3, ParseErrorKind::UnterminatedString),
            (2, 3, ParseErrorKind::UnclosedFlowMapping),
        ],
    );
    assert_eq!(selected, Some((2, 3, 1, 3)));
    assert_eq!(
        super::super::presentation::positioned_error_for_test(
            "abc",
            &[
                (4, 4, ParseErrorKind::Other),
                (3, 2, ParseErrorKind::UnclosedFlowSequence),
            ],
        ),
        None
    );
    assert_eq!(
        super::super::presentation::positioned_error_for_test(
            "abc",
            &[
                (1, 2, ParseErrorKind::Other),
                (4, 4, ParseErrorKind::UnclosedFlowSequence),
            ],
        ),
        Some((1, 2, 3, 0))
    );
}

#[test]
fn class_first_pending_precedence_and_stage_seven_win() {
    let presentation = super::super::presentation::pending_kind_for_test(
        None,
        Some((100, ParseFailureKind::PresentationShape)),
        Some((1, ParseFailureKind::ExplicitTagKindMismatch)),
        Some((0, ParseFailureKind::RawScalarLimit)),
    );
    assert_eq!(presentation, Some(ParseFailureKind::PresentationShape));
    let tag_before_raw = super::super::presentation::pending_kind_for_test(
        None,
        None,
        Some((100, ParseFailureKind::ExplicitTagKindMismatch)),
        Some((0, ParseFailureKind::RawScalarLimit)),
    );
    assert_eq!(
        tag_before_raw,
        Some(ParseFailureKind::ExplicitTagKindMismatch)
    );
    assert_eq!(
        super::super::presentation::pending_kind_for_test(
            Some((100, ParseFailureKind::DirectiveDenied)),
            Some((0, ParseFailureKind::PresentationShape)),
            None,
            None,
        ),
        Some(ParseFailureKind::DirectiveDenied)
    );
    assert_eq!(
        super::super::presentation::structural_token_vs_directive_for_test(),
        ParseFailureKind::DirectiveDenied
    );
    assert_eq!(
        super::super::presentation::lowest_bucket_offset_for_test(&[90, 12, 44]),
        Some(12)
    );
}

#[test]
fn strict_leaf_machine_rejects_coverage_differentials() {
    use yaml_edit::SyntaxKind;

    type Leaf<'a> = (
        SyntaxKind,
        usize,
        usize,
        &'a str,
        SyntaxKind,
        bool,
        u32,
        bool,
        Option<SyntaxKind>,
    );
    let scalar = |kind, start, end, dependency: &'static str| -> Leaf<'static> {
        (
            kind,
            start,
            end,
            dependency,
            SyntaxKind::SCALAR,
            false,
            0,
            false,
            Some(SyntaxKind::VALUE),
        )
    };
    for (source, leaves) in [
        ("abc", vec![scalar(SyntaxKind::STRING, 1, 3, "bc")]),
        (
            "abc",
            vec![
                scalar(SyntaxKind::STRING, 0, 2, "ab"),
                scalar(SyntaxKind::STRING, 1, 3, "bc"),
            ],
        ),
        ("abc", vec![scalar(SyntaxKind::STRING, 0, 3, "abd")]),
        (
            "a",
            vec![
                scalar(SyntaxKind::STRING, 0, 1, "a"),
                scalar(SyntaxKind::NULL, 1, 1, ""),
            ],
        ),
    ] {
        assert_eq!(
            super::super::presentation::leaf_sequence_kind_for_test(source, &leaves),
            Some(ParseFailureKind::PresentationCoverage)
        );
    }

    let extra_empty = [
        (
            SyntaxKind::NULL,
            0,
            0,
            "",
            SyntaxKind::SCALAR,
            true,
            0,
            false,
            Some(SyntaxKind::VALUE),
        ),
        (
            SyntaxKind::NULL,
            0,
            0,
            "",
            SyntaxKind::SCALAR,
            true,
            0,
            false,
            Some(SyntaxKind::VALUE),
        ),
    ];
    assert_eq!(
        super::super::presentation::leaf_sequence_kind_for_test("", &extra_empty),
        Some(ParseFailureKind::PresentationCoverage)
    );

    let after_eof = [
        (
            SyntaxKind::STRING,
            0,
            1,
            "a",
            SyntaxKind::DOCUMENT,
            false,
            0,
            true,
            Some(SyntaxKind::ROOT),
        ),
        (
            SyntaxKind::EOF,
            1,
            1,
            "",
            SyntaxKind::DOCUMENT,
            false,
            0,
            true,
            Some(SyntaxKind::ROOT),
        ),
        (
            SyntaxKind::EOF,
            1,
            1,
            "",
            SyntaxKind::DOCUMENT,
            false,
            0,
            true,
            Some(SyntaxKind::ROOT),
        ),
    ];
    assert_eq!(
        super::super::presentation::leaf_sequence_kind_for_test("a", &after_eof),
        Some(ParseFailureKind::PresentationCoverage)
    );
}

#[test]
fn invalid_token_contexts_and_checked_growth_are_rejected() {
    use yaml_edit::SyntaxKind;

    assert!(!super::super::presentation::token_context_allowed_for_test(
        SyntaxKind::ROOT,
        SyntaxKind::COLON,
        false
    ));
    assert!(!super::super::presentation::token_context_allowed_for_test(
        SyntaxKind::TAGGED_NODE,
        SyntaxKind::COMMA,
        false
    ));
    assert!(super::super::presentation::token_context_allowed_for_test(
        SyntaxKind::SCALAR,
        SyntaxKind::AMPERSAND,
        true
    ));
    assert_eq!(
        super::super::presentation::capped_push_kind_for_test(2, 3),
        Some(ParseFailureKind::PresentationCoverage)
    );
    assert_eq!(
        super::super::presentation::comment_line_counts_for_test(b"\n\n\n").unwrap(),
        (4, 4)
    );
}

#[test]
fn normalized_contexts_accept_block_flow_complex_and_implicit_forms() {
    for source in [
        b"a: b\n".as_slice(),
        b"a: [x, y]\n",
        b"a: {b: c}\n",
        b"? [a, b]\n: c\n",
        b"a:\n",
        b"a: [x, ]\n",
        b"a: {}\n",
        b"a: []\n",
    ] {
        let text = core::str::from_utf8(source).unwrap();
        super::super::presentation::inspect(text, source, 0)
            .unwrap_or_else(|error| panic!("normalized form rejected: {}", error.kind() as u8));
    }
}

#[test]
fn block_scalar_punctuation_is_content_not_source_syntax() {
    for source in [
        b"value: |\n  & * % ! --- ... # ' \" [ ] { }\n".as_slice(),
        b"value: >\n  & * % ! --- ... # ' \" [ ] { }\n",
    ] {
        let parsed = parse_yaml_document(source.to_vec().into_boxed_slice())
            .expect("block scalar punctuation must remain scalar content");
        assert!(matches!(
            parsed.yaml().round_trip_status(),
            RoundTripStatus::Proven
        ));
    }
}

#[test]
fn proven_shape_omitted_value_is_implicit_empty() {
    let parsed = parse_yaml_document(b"a:\n".to_vec().into_boxed_slice()).unwrap();
    let value = &parsed.yaml().nodes()[2];
    assert_eq!(value.kind(), ReconciledNodeKind::ImplicitEmpty);
    assert!(matches!(
        parsed.yaml().round_trip_status(),
        RoundTripStatus::Proven
    ));
}

#[test]
fn presentation_keeps_only_rich_prefix_but_describes_every_candidate() {
    let item_count = 20_020usize;
    let mut source = String::from("root:\n");
    for _ in 0..item_count {
        source.push_str("  - value\n");
    }
    source.push_str("  - !!str tail\n");
    let presentation = super::super::presentation::inspect(&source, source.as_bytes(), 0)
        .expect("bounded candidate tail must normalize");
    assert_eq!(presentation.nodes.len(), 20_000);
    assert_eq!(presentation.descriptor_count(), item_count + 4);
    let tail = presentation.descriptor_facts(20_000).unwrap();
    assert_eq!(tail.0, 20_000);
    assert_eq!(tail.5, 0);
    let last = presentation
        .descriptor_facts(presentation.descriptor_count() - 1)
        .unwrap();
    assert_eq!(last.0 as usize, presentation.descriptor_count() - 1);
    assert_eq!(last.5, 6);
    for ordinal in 19_995..presentation.descriptor_count() {
        assert_eq!(
            presentation.descriptor_facts(ordinal).unwrap().0 as usize,
            ordinal
        );
    }
}

#[test]
fn raw_scalar_limit_is_enforced_in_descriptor_only_tail() {
    use super::super::ParseLimitScope;

    let mut source = String::from("root: [");
    for _ in 0..20_000 {
        source.push_str("x,");
    }
    source.extend(core::iter::repeat_n('z', 65_537));
    source.push_str("]\n");
    assert!(source.len() <= 256 * 1024);
    let error = match super::super::presentation::inspect(&source, source.as_bytes(), 0) {
        Err(error) => error,
        Ok(_) => panic!("oversized tail scalar accepted"),
    };
    let facts = error.limit().unwrap();
    assert_eq!(error.kind(), ParseFailureKind::RawScalarLimit);
    assert_eq!(facts.scope(), ParseLimitScope::RawScalarBytes);
    assert_eq!(facts.limit(), 65_536);
    assert_eq!(facts.observed_at_least(), 65_537);
}

#[test]
fn source_guards_are_derived_once_from_exact_source_surface() {
    let source = "tagged: !!str 12\nquoted: \"<<\"\n<<: value\n";
    let presentation = super::super::presentation::inspect(source, source.as_bytes(), 0)
        .unwrap_or_else(|error| panic!("guard fixture rejected: {}", error.kind() as u8));
    assert_eq!(presentation.guard_code(2), Some(6));
    assert_eq!(presentation.guard_code(4), Some(0));
    assert_eq!(presentation.guard_code(5), Some(1));
}

#[test]
fn source_constructs_are_denied() {
    for (bytes, kind) in [
        (
            b"%YAML 1.2\na: b\n".as_slice(),
            ParseFailureKind::DirectiveDenied,
        ),
        (
            b"---\na: b\n".as_slice(),
            ParseFailureKind::DocumentMarkerDenied,
        ),
        (b"a: &x b\n".as_slice(), ParseFailureKind::AnchorDenied),
        (b"a: *x\n".as_slice(), ParseFailureKind::AliasDenied),
        (
            b"a: !custom b\n".as_slice(),
            ParseFailureKind::ExplicitTagDenied,
        ),
        (b"<<: value\n".as_slice(), ParseFailureKind::MergeKeyDenied),
    ] {
        let actual = failure(bytes).kind();
        assert!(
            actual == kind,
            "actual={} expected={}",
            actual as u8,
            kind as u8
        );
    }
}

#[test]
fn literal_core_tags_parse() {
    for bytes in [
        b"n: !!null null\n".as_slice(),
        b"b: !!bool true\n",
        b"i: !!int 12\n",
        b"f: !!float 1.0\n",
        b"s: !!str 12\n",
        b"q: !!seq [a]\n",
        b"m: !!map {a: b}\n",
    ] {
        if let Err(error) = parse_yaml_document(bytes.to_vec().into_boxed_slice()) {
            panic!(
                "tag case failed kind={} bytes={bytes:?}",
                error.kind() as u8
            );
        }
    }
}

#[test]
fn literal_core_tags_reject_kind_and_value_mismatches() {
    for (bytes, expected) in [
        (
            b"value: !!seq scalar\n".as_slice(),
            ParseFailureKind::ExplicitTagKindMismatch,
        ),
        (
            b"value: !!int nope\n".as_slice(),
            ParseFailureKind::ExplicitTagValueMismatch,
        ),
    ] {
        assert!(failure(bytes).kind() == expected);
    }
}

#[test]
fn decoded_noncharacters_are_rejected() {
    assert!(failure(b"value: \"\\uFFFF\"\n").kind() == ParseFailureKind::InvalidUnicodeScalar);
}

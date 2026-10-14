use super::{ParseFailureKind, failure, parse_guide_source, parse_yaml_document};

#[test]
fn named_checked_in_accepted_corpus_parses_through_production_entries() {
    for source in [
        include_bytes!("fixtures/accepted/allowed-built-in-tags.yaml").as_slice(),
        include_bytes!("fixtures/accepted/basic.yaml").as_slice(),
        include_bytes!("fixtures/accepted/comments-all-placements.yaml").as_slice(),
        include_bytes!("fixtures/accepted/complex-keys.yaml").as_slice(),
        include_bytes!("fixtures/accepted/implicit-empties.yaml").as_slice(),
        include_bytes!("fixtures/accepted/mixed-flow-block-nesting.yaml").as_slice(),
        include_bytes!("fixtures/accepted/scalar-numeric-vectors.yaml").as_slice(),
        include_bytes!("fixtures/accepted/styles-tags-comments.yaml").as_slice(),
        include_bytes!("fixtures/accepted/whole-bom-lf-final-newline.yaml").as_slice(),
        include_bytes!("fixtures/accepted/whole-lf-final-newline.yaml").as_slice(),
    ] {
        assert!(parse_yaml_document(Box::<[u8]>::from(source)).is_ok());
    }
    for source in [
        include_bytes!("fixtures/accepted/guide-bom-lf-final-newline.md").as_slice(),
        include_bytes!("fixtures/accepted/guide-close-at-eof-newline.md").as_slice(),
        include_bytes!("fixtures/accepted/guide-lf.md").as_slice(),
    ] {
        assert!(parse_guide_source(Box::<[u8]>::from(source)).is_ok());
    }
}

#[test]
fn checked_in_byte_descriptors_materialize_endings_and_invalid_bytes_exactly() {
    use super::super::SourceLineEnding;

    for (descriptor, guide, bom, ending, final_newline) in [
        (
            include_str!("fixtures/accepted/whole-crlf-final-newline.recipe"),
            false,
            false,
            SourceLineEnding::CrLf,
            true,
        ),
        (
            include_str!("fixtures/accepted/whole-lf-no-final-newline.recipe"),
            false,
            false,
            SourceLineEnding::None,
            false,
        ),
        (
            include_str!("fixtures/accepted/guide-crlf-no-final-newline.recipe"),
            true,
            false,
            SourceLineEnding::CrLf,
            false,
        ),
    ] {
        let recipe = super::materialize_fixture_recipe(descriptor);
        assert_eq!(
            recipe.fact("final"),
            Some(if final_newline { "true" } else { "false" })
        );
        if guide {
            let parsed = parse_guide_source(recipe.bytes).unwrap();
            assert_eq!(parsed.envelope().bom_present(), bom);
            assert!(parsed.envelope().yaml_line_ending() == ending);
            assert_eq!(parsed.envelope().final_newline(), final_newline);
        } else {
            let parsed = parse_yaml_document(recipe.bytes).unwrap();
            assert_eq!(parsed.envelope().bom_present(), bom);
            assert!(parsed.envelope().line_ending() == ending);
            assert_eq!(parsed.envelope().final_newline(), final_newline);
        }
    }

    for (descriptor, kind) in [
        (
            include_str!("fixtures/malformed/invalid-utf8-at-start.recipe"),
            ParseFailureKind::InvalidUtf8,
        ),
        (
            include_str!("fixtures/malformed/bare-carriage-return.recipe"),
            ParseFailureKind::BareCarriageReturn,
        ),
        (
            include_str!("fixtures/malformed/mixed-line-endings.recipe"),
            ParseFailureKind::MixedYamlLineEndings,
        ),
    ] {
        let recipe = super::materialize_fixture_recipe(descriptor);
        assert_eq!(failure(&recipe.bytes).kind(), kind);
    }
}

#[test]
fn whole_envelope_bom_endings_and_positions() {
    let parsed =
        parse_yaml_document(b"\xef\xbb\xbfkey: value\r\n".to_vec().into_boxed_slice()).unwrap();
    assert!(parsed.envelope().bom_present());
    assert_eq!(parsed.yaml_bytes(), b"key: value\r\n");
    assert!(parsed.envelope().final_newline());
    assert_eq!(
        failure(b"key: value\rnext: value\n").kind(),
        ParseFailureKind::BareCarriageReturn
    );
    assert_eq!(
        failure(b"a: b\r\nc: d\n").kind(),
        ParseFailureKind::MixedYamlLineEndings
    );
    assert_eq!(
        failure(b"a: \xef\xbb\xbf\n").kind(),
        ParseFailureKind::RawBomDenied
    );
}

#[test]
fn whole_and_guide_lf_crlf_bom_and_final_newline_matrix() {
    use super::super::SourceLineEnding;

    for (bytes, bom, ending, final_newline) in [
        (b"a: b\n".as_slice(), false, SourceLineEnding::Lf, true),
        (b"a: b".as_slice(), false, SourceLineEnding::None, false),
        (b"a: b\r\n".as_slice(), false, SourceLineEnding::CrLf, true),
        (
            b"\xef\xbb\xbfa: b\n".as_slice(),
            true,
            SourceLineEnding::Lf,
            true,
        ),
    ] {
        let parsed = parse_yaml_document(bytes.to_vec().into_boxed_slice()).unwrap();
        assert_eq!(parsed.envelope().bom_present(), bom);
        assert!(parsed.envelope().line_ending() == ending);
        assert_eq!(parsed.envelope().final_newline(), final_newline);
    }
    for (bytes, bom, ending, final_newline) in [
        (
            b"---\na: b\n---\n\nbody\n".as_slice(),
            false,
            SourceLineEnding::Lf,
            true,
        ),
        (
            b"---\r\na: b\r\n---\r\n\r\nbody".as_slice(),
            false,
            SourceLineEnding::CrLf,
            false,
        ),
        (
            b"\xef\xbb\xbf---\na: b\n---\n\nbody".as_slice(),
            true,
            SourceLineEnding::Lf,
            false,
        ),
    ] {
        let parsed = parse_guide_source(bytes.to_vec().into_boxed_slice()).unwrap();
        assert_eq!(parsed.envelope().bom_present(), bom);
        assert!(parsed.envelope().yaml_line_ending() == ending);
        assert_eq!(parsed.envelope().final_newline(), final_newline);
    }
}

#[test]
fn guide_envelope_failures_are_distinct() {
    for (bytes, kind) in [
        (
            b"key: value\n".as_slice(),
            ParseFailureKind::MissingGuideOpeningDelimiter,
        ),
        (
            b"---\nkey: value\n".as_slice(),
            ParseFailureKind::MissingGuideClosingDelimiter,
        ),
    ] {
        let error = match parse_guide_source(bytes.to_vec().into_boxed_slice()) {
            Err(e) => e,
            Ok(_) => panic!("bad Guide accepted"),
        };
        assert_eq!(error.kind(), kind);
    }
}

#[test]
fn invalid_utf8_is_complete_file_and_never_positioned() {
    for bytes in [
        vec![0xff],
        b"a: b\n".iter().copied().chain([0xff]).collect(),
        b"a: b\r\n".iter().copied().chain([0x80]).collect(),
        vec![b'a', b':', b' ', 0xe2, 0x82],
    ] {
        let error = match parse_yaml_document(bytes.into_boxed_slice()) {
            Err(error) => error,
            Ok(_) => panic!("invalid UTF-8 accepted"),
        };
        assert_eq!(error.kind(), ParseFailureKind::InvalidUtf8);
        assert_eq!(error.position(), None);
    }
    let mut guide = b"---\na: b\n---\n\nbody".to_vec();
    guide.push(0xff);
    let error = match parse_guide_source(guide.into_boxed_slice()) {
        Err(error) => error,
        Ok(_) => panic!("invalid UTF-8 Guide accepted"),
    };
    assert_eq!(error.kind(), ParseFailureKind::InvalidUtf8);
    assert_eq!(error.position(), None);
}

#[test]
fn guide_body_is_opaque_to_yaml_policy_and_delimiters() {
    let source = b"---\ntitle: Rehow\n---\n\n\xef\xbb\xbf\r\n& * % ! #\n---\n...\n";
    let parsed = parse_guide_source(source.to_vec().into_boxed_slice())
        .expect("valid UTF-8 Guide body must remain opaque");
    assert_eq!(
        parsed.body_bytes(),
        b"\xef\xbb\xbf\r\n& * % ! #\n---\n...\n"
    );
}

#[test]
fn guide_comment_scan_input_stops_at_separator() {
    use super::super::source::{scan_guide, slice};

    let source = b"---\n# frontmatter\na: b\n---\n\n# body\n# body again\n";
    let envelope = scan_guide(source).unwrap();
    let yaml = slice(source, envelope.yaml_file_range());
    let text = core::str::from_utf8(yaml).unwrap();
    let region_end = envelope.separator_line().end().get() as usize;
    let presentation = super::super::presentation::inspect(
        text,
        &source[..region_end],
        envelope.yaml_file_range().start().get() as usize,
    )
    .unwrap();
    assert_eq!(presentation.comment_region_len as usize, region_end);
    assert_eq!(presentation.comments.len(), 1);
    assert_eq!(presentation.comment_line_starts.len(), 6);
    assert_eq!(presentation.comment_line_blank.len(), 6);
    assert_eq!(presentation.yaml_region_offset, 4);
}

#[test]
fn scanner_selects_global_source_and_envelope_precedence() {
    let cases: &[(&[u8], ParseFailureKind)] = &[
        (
            b"not-open\xef\xbb\xbf\r\nx\n",
            ParseFailureKind::RawBomDenied,
        ),
        (b"not-open\rx\n", ParseFailureKind::BareCarriageReturn),
        (b"not-open\r\nx\n", ParseFailureKind::MixedYamlLineEndings),
        (
            b"not-open\n",
            ParseFailureKind::MissingGuideOpeningDelimiter,
        ),
        (
            b"---\na: b\n",
            ParseFailureKind::MissingGuideClosingDelimiter,
        ),
        (
            b"---\na: b\n---\r\nbody\n",
            ParseFailureKind::MixedYamlLineEndings,
        ),
    ];
    for (source, expected) in cases {
        let error = match parse_guide_source(source.to_vec().into_boxed_slice()) {
            Err(error) => error,
            Ok(_) => panic!("malformed Guide accepted"),
        };
        assert_eq!(error.kind(), *expected);
    }
}

#[test]
fn whole_scanner_selects_every_adjacent_fixed_fact_precedence() {
    let mut oversized_invalid = vec![b'a'; 256 * 1024 + 1];
    oversized_invalid[256 * 1024] = 0xff;
    let error = match parse_yaml_document(oversized_invalid.into_boxed_slice()) {
        Err(error) => error,
        Ok(_) => panic!("oversized source accepted"),
    };
    assert_eq!(error.kind(), ParseFailureKind::SourceTooLarge);

    for (source, expected) in [
        (
            b"a: \xff\xef\xbb\xbf\r\nx\n".as_slice(),
            ParseFailureKind::InvalidUtf8,
        ),
        (
            b"a: \xef\xbb\xbf\r\nx\n".as_slice(),
            ParseFailureKind::RawBomDenied,
        ),
        (
            b"a: b\rx: y\r\nz: q\n".as_slice(),
            ParseFailureKind::BareCarriageReturn,
        ),
        (
            b"a: [\r\nx: y\n".as_slice(),
            ParseFailureKind::MixedYamlLineEndings,
        ),
    ] {
        let error = match parse_yaml_document(source.to_vec().into_boxed_slice()) {
            Err(error) => error,
            Ok(_) => panic!("malformed source accepted"),
        };
        assert_eq!(error.kind(), expected);
    }
}

#[test]
fn frontmatter_and_guide_byte_caps_report_exact_facts() {
    use super::super::ParseLimitScope;
    use super::super::source::{MAX_GUIDE_BYTES, MAX_YAML_BYTES, scan_guide};

    fn frontmatter(total: usize) -> Vec<u8> {
        let scalar_len = total - 9 - 4;
        let mut bytes = b"---\na: ".to_vec();
        bytes.extend(core::iter::repeat_n(b'x', scalar_len));
        bytes.extend_from_slice(b"\n---\n\n");
        assert_eq!(bytes.len(), total);
        bytes
    }

    assert!(scan_guide(&frontmatter(MAX_YAML_BYTES)).is_ok());
    let error = match scan_guide(&frontmatter(MAX_YAML_BYTES + 1)) {
        Err(error) => error,
        Ok(_) => panic!("oversized frontmatter accepted"),
    };
    let facts = error.limit().unwrap();
    assert_eq!(error.kind(), ParseFailureKind::FrontmatterTooLarge);
    assert_eq!(facts.scope(), ParseLimitScope::FrontmatterBytes);
    assert_eq!(facts.limit(), MAX_YAML_BYTES as u64);
    assert_eq!(facts.observed_at_least(), (MAX_YAML_BYTES + 1) as u64);

    let error = match scan_guide(&frontmatter(MAX_YAML_BYTES + 8_192)) {
        Err(error) => error,
        Ok(_) => panic!("far-oversized frontmatter accepted"),
    };
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::FrontmatterBytes);
    assert_eq!(facts.limit(), MAX_YAML_BYTES as u64);
    assert_eq!(facts.observed_at_least(), (MAX_YAML_BYTES + 1) as u64);

    let mut largest = b"---\na: b\n---\n\n".to_vec();
    largest.resize(MAX_GUIDE_BYTES, b'x');
    assert!(scan_guide(&largest).is_ok());
    largest.push(b'x');
    let error = match scan_guide(&largest) {
        Err(error) => error,
        Ok(_) => panic!("oversized Guide accepted"),
    };
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::GuideBytes);
    assert_eq!(facts.limit(), MAX_GUIDE_BYTES as u64);
    assert_eq!(facts.observed_at_least(), (MAX_GUIDE_BYTES + 1) as u64);

    largest.extend(core::iter::repeat_n(b'x', 8_192));
    let error = match scan_guide(&largest) {
        Err(error) => error,
        Ok(_) => panic!("far-oversized Guide accepted"),
    };
    let facts = error.limit().unwrap();
    assert_eq!(facts.scope(), ParseLimitScope::GuideBytes);
    assert_eq!(facts.limit(), MAX_GUIDE_BYTES as u64);
    assert_eq!(facts.observed_at_least(), (MAX_GUIDE_BYTES + 1) as u64);
}

#[test]
fn guide_separator_blank_line_is_optional_and_layout_is_exact() {
    for (source, separator_len, body) in [
        (
            b"---\na: b\n---\n\nbody\n".as_slice(),
            1,
            b"body\n".as_slice(),
        ),
        (
            b"---\na: b\n---\nbody\n".as_slice(),
            0,
            b"body\n".as_slice(),
        ),
        (b"---\na: b\n---\n".as_slice(), 0, b"".as_slice()),
        (b"---\na: b\n---".as_slice(), 0, b"".as_slice()),
        (
            b"---\r\na: b\r\n---\r\n# T\r\n".as_slice(),
            0,
            b"# T\r\n".as_slice(),
        ),
        (
            b"---\r\na: b\r\n---\r\n\r\n# T".as_slice(),
            2,
            b"# T".as_slice(),
        ),
        (
            b"\xef\xbb\xbf---\na: b\n---\n# T\n".as_slice(),
            0,
            b"# T\n".as_slice(),
        ),
        (
            b"---\na: b\n---\n\n\nbody\n".as_slice(),
            1,
            b"\nbody\n".as_slice(),
        ),
    ] {
        let parsed = parse_guide_source(source.to_vec().into_boxed_slice()).unwrap();
        assert_eq!(parsed.envelope().separator_line().len(), separator_len);
        assert_eq!(parsed.body_bytes(), body);
        let envelope = parsed.envelope();
        assert_eq!(
            envelope.separator_line().end(),
            envelope.body_file_range().start()
        );
        assert_eq!(
            envelope.body_file_range().end().get() as usize,
            source.len()
        );
    }
    for source in [
        b"---\na: b\n---x\n".as_slice(),
        b"---\na: b\n--- \n".as_slice(),
        b"---\na: b\n----".as_slice(),
    ] {
        let error = match parse_guide_source(source.to_vec().into_boxed_slice()) {
            Err(error) => error,
            Ok(_) => panic!("unterminated frontmatter accepted"),
        };
        assert_eq!(error.kind(), ParseFailureKind::MissingGuideClosingDelimiter);
    }
}

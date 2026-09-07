use super::super::reconcile::{
    ReconcileFault, reconcile_fault_for_test, reconciliation_work_for_test,
    reset_pipeline_observation_for_test,
};
use super::super::{CommentOwner, CommentPlacement, RoundTripReason};
use super::parse_yaml_document;

#[test]
fn comments_retain_ranges_without_text_copies() {
    let parsed =
        parse_yaml_document(b"# lead\na: b # inline\n".to_vec().into_boxed_slice()).unwrap();
    assert_eq!(parsed.yaml().comments().len(), 2);
    for comment in parsed.yaml().comments() {
        let range = comment.yaml_range();
        assert_eq!(
            parsed.yaml_bytes()[range.start().get() as usize..range.end().get() as usize].first(),
            Some(&b'#')
        );
    }
}

#[test]
fn literal_and_folded_header_comments_are_natural_nested_scalar_comments() {
    for source in [
        b"value: | # literal header\n  body\n".as_slice(),
        b"value: > # folded header\n  body\n".as_slice(),
    ] {
        let parsed = parse_yaml_document(source.to_vec().into_boxed_slice()).unwrap();
        let comments = parsed.yaml().comments();
        assert_eq!(comments.len(), 1);
        let comment = &comments[0];
        assert_eq!(comment.placement(), CommentPlacement::Nested);
        assert!(matches!(comment.owner(), CommentOwner::Node(_)));
        let range = comment.yaml_range();
        let slice = &parsed.yaml_bytes()[range.start().get() as usize..range.end().get() as usize];
        assert!(slice.starts_with(b"#"));
        assert!(slice.ends_with(b"header"));
    }
}

#[test]
fn document_inline_inter_entry_trailing_and_flow_matrix() {
    let parsed = parse_yaml_document(
        b"# document\na: b # inline\n# between\nc: d\n# trailing\n"
            .to_vec()
            .into_boxed_slice(),
    )
    .unwrap();
    let comments = parsed.yaml().comments();
    assert_eq!(comments.len(), 4);
    assert_eq!(comments[0].placement(), CommentPlacement::DocumentOnly);
    assert_eq!(comments[1].placement(), CommentPlacement::Inline);
    assert_eq!(comments[2].placement(), CommentPlacement::InterEntry);
    assert_eq!(comments[3].placement(), CommentPlacement::Trailing);
    assert!(matches!(comments[1].owner(), CommentOwner::Entry(_)));
    assert!(matches!(comments[2].owner(), CommentOwner::Boundary { .. }));

    let flow =
        parse_yaml_document(b"root: {a: b, # flow\n c: d}\n".to_vec().into_boxed_slice()).unwrap();
    assert_eq!(
        flow.yaml().comments()[0].placement(),
        CommentPlacement::Flow
    );
}

#[test]
fn crossed_dropped_and_unreconciled_comment_provenance_is_ambiguous() {
    for (source, fault) in [
        (
            b"a: b # one\nc: d # two\n".as_slice(),
            ReconcileFault::CommentCross,
        ),
        (
            b"a: b # one\nc: d # two\n".as_slice(),
            ReconcileFault::CommentReverse,
        ),
        (
            b"a: b # one\n".as_slice(),
            ReconcileFault::CommentDroppedEntry,
        ),
        (
            b"a: b # one\nc: d\n".as_slice(),
            ReconcileFault::CommentConflictingEntry,
        ),
        (
            b"a: b # one\n".as_slice(),
            ReconcileFault::CommentUnreconciledNode,
        ),
        (
            b"a: b # one\n".as_slice(),
            ReconcileFault::CommentScalarOverlap { node: 2 },
        ),
        (b"a: b # one\n".as_slice(), ReconcileFault::RebaseOverflow),
    ] {
        let artifact = reconcile_fault_for_test(source, fault).unwrap();
        let report = artifact.round_trip_status().report().unwrap();
        assert!(
            report
                .issues()
                .iter()
                .any(|issue| issue.reason() == RoundTripReason::CommentOwnershipAmbiguous)
        );
        assert!(artifact.comments().iter().any(|comment| {
            comment.placement() == CommentPlacement::Unresolved
                && comment.owner() == CommentOwner::Ambiguous
                && source[comment.yaml_range().start().get() as usize
                    ..comment.yaml_range().end().get() as usize]
                    .starts_with(b"#")
        }));
    }
}

#[test]
fn scalar_nested_fault_uses_deepest_cst_candidate_before_entry_owner() {
    let artifact = reconcile_fault_for_test(
        b"a: b # nested\n",
        ReconcileFault::CommentScalarNested { node: 2 },
    )
    .unwrap();
    let comment = &artifact.comments()[0];
    assert_eq!(comment.placement(), CommentPlacement::Nested);
    assert!(matches!(comment.owner(), CommentOwner::Node(_)));
}

#[test]
fn dense_newline_region_hits_exact_table_bound_and_linear_work_is_bounded() {
    let dense = vec![b'\n'; 262_144];
    assert_eq!(
        super::super::presentation::comment_line_counts_for_test(&dense).unwrap(),
        (262_145, 262_145)
    );

    reset_pipeline_observation_for_test();
    let parsed =
        parse_yaml_document(b"a: b\n\n# separated\n\nc: d\n".to_vec().into_boxed_slice()).unwrap();
    let (work, bound) = reconciliation_work_for_test();
    assert!(work <= bound);
    assert_eq!(
        parsed.yaml().comments()[0].placement(),
        CommentPlacement::Leading
    );
    assert!(parsed.yaml().round_trip_status().is_proven());
}

#[test]
fn dense_comment_population_stays_inside_production_linear_bound() {
    let mut source = Vec::new();
    for _ in 0..4_096 {
        source.extend_from_slice(b"#\n");
    }
    source.extend_from_slice(b"a: b\n");
    reset_pipeline_observation_for_test();
    let parsed = parse_yaml_document(source.into_boxed_slice()).unwrap();
    assert_eq!(parsed.yaml().comments().len(), 4_096);
    let (work, bound) = reconciliation_work_for_test();
    assert!(work <= bound);
}

#[test]
fn maximum_bounded_comment_region_reconciles_with_exact_population() {
    let mut source = Vec::new();
    source.try_reserve_exact(262_144).unwrap();
    source.extend_from_slice(b"#x\n");
    for _ in 0..131_068 {
        source.extend_from_slice(b"#\n");
    }
    source.extend_from_slice(b"a: b\n");
    assert_eq!(source.len(), 262_144);
    reset_pipeline_observation_for_test();
    let parsed = parse_yaml_document(source.into_boxed_slice()).unwrap();
    assert_eq!(parsed.yaml().comments().len(), 131_069);
    let (work, bound) = reconciliation_work_for_test();
    assert!(work <= bound);
}

#[test]
fn deepest_accepted_nesting_and_mixed_flow_block_comments_meet_work_bound() {
    let nested = format!("value: {}x{} # deepest\n", "[".repeat(15), "]".repeat(15));
    for source in [
        nested.into_bytes(),
        b"# document\nroot:\n  # leading\n  a: b # inline\n  # between\n  c: {d: e, # flow\n      f: g}\n  # trailing\n"
            .to_vec(),
    ] {
        reset_pipeline_observation_for_test();
        let parsed = parse_yaml_document(source.into_boxed_slice()).unwrap();
        assert!(!parsed.yaml().comments().is_empty());
        let (work, bound) = reconciliation_work_for_test();
        assert!(work <= bound);
    }
}

#[test]
fn adjacent_boundary_comment_run_shares_one_entry_pair() {
    let parsed = parse_yaml_document(
        b"a: b\n# first\n# second\nc: d\n"
            .to_vec()
            .into_boxed_slice(),
    )
    .unwrap();
    let comments = parsed.yaml().comments();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].placement(), CommentPlacement::InterEntry);
    assert_eq!(comments[1].placement(), CommentPlacement::InterEntry);
    assert_eq!(comments[0].owner(), comments[1].owner());
    assert!(matches!(
        comments[0].owner(),
        CommentOwner::Boundary {
            before: Some(_),
            after: Some(_),
            ..
        }
    ));
}

#[test]
fn blank_line_splits_boundary_runs_into_trailing_and_leading() {
    let parsed = parse_yaml_document(
        b"a: b\n# trailing\n\n# leading\nc: d\n"
            .to_vec()
            .into_boxed_slice(),
    )
    .unwrap();
    let comments = parsed.yaml().comments();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].placement(), CommentPlacement::Trailing);
    assert_eq!(comments[1].placement(), CommentPlacement::Leading);
    assert!(matches!(
        comments[0].owner(),
        CommentOwner::Boundary {
            before: Some(_),
            after: None,
            ..
        }
    ));
    assert!(matches!(
        comments[1].owner(),
        CommentOwner::Boundary {
            before: None,
            after: Some(_),
            ..
        }
    ));
}

#[test]
fn long_inline_comment_lines_remain_inside_closed_form_work_bound() {
    let mut source = Vec::new();
    for index in 0..32 {
        source.extend_from_slice(format!("key{index}: ").as_bytes());
        source.extend(core::iter::repeat_n(b'x', 1_024));
        source.extend_from_slice(b" # inline\n");
    }
    reset_pipeline_observation_for_test();
    let parsed = parse_yaml_document(source.into_boxed_slice())
        .unwrap_or_else(|failure| panic!("long inline fixture failed: {:?}", failure.kind()));
    assert_eq!(parsed.yaml().comments().len(), 32);
    assert!(
        parsed
            .yaml()
            .comments()
            .iter()
            .all(|comment| comment.placement() == CommentPlacement::Inline)
    );
    let (work, bound) = reconciliation_work_for_test();
    assert!(work <= bound);
}

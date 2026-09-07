use super::super::reconcile::{
    NodeMismatchDimension, ReconcileFault, draft_allocation_overflow_for_test,
    entry_ordinal_for_test, issue_total_overflow_for_test, pipeline_observation_for_test,
    reconcile_fault_for_test, reconciliation_work_for_test, reset_pipeline_observation_for_test,
};
use super::super::{CommentOwner, NodePresentation, NodeRole, ParsedYamlArtifact};
use super::super::{ParseFailureKind, RoundTripReason};
use super::{failure, parse_yaml_document};

fn assert_all_entry_ids_resolve(artifact: &ParsedYamlArtifact) {
    for node in artifact.nodes() {
        for id in node.entries() {
            assert!(artifact.entry(*id).is_some());
        }
    }
    for comment in artifact.comments() {
        match comment.owner() {
            CommentOwner::Entry(id) => assert!(artifact.entry(id).is_some()),
            CommentOwner::Boundary { before, after, .. } => {
                assert!(before.is_none_or(|id| artifact.entry(id).is_some()));
                assert!(after.is_none_or(|id| artifact.entry(id).is_some()));
            }
            CommentOwner::Node(_) | CommentOwner::Ambiguous => {}
        }
    }
    if let Some(report) = artifact.round_trip_status().report() {
        for issue in report.issues() {
            assert!(issue.entry().is_none_or(|id| artifact.entry(id).is_some()));
        }
    }
}

#[test]
fn reconciled_ids_ranges_and_roles_are_consistent() {
    let parsed = parse_yaml_document(b"a: [b, c]\n".to_vec().into_boxed_slice()).unwrap();
    let yaml = parsed.yaml();
    assert_eq!(yaml.root_id(), yaml.nodes()[0].id());
    for (index, node) in yaml.nodes().iter().enumerate() {
        assert_eq!(yaml.node(node.id()).unwrap().id(), node.id());
        if index == 0 {
            assert_eq!(node.role(), NodeRole::Root);
        }
        assert!(!matches!(
            node.presentation(),
            NodePresentation::Unreconciled
        ));
    }
    assert!(yaml.round_trip_status().is_proven());
}

#[test]
fn all_eight_node_dimensions_select_the_first_ordered_reason() {
    for (first, expected) in [
        (
            NodeMismatchDimension::Parent,
            RoundTripReason::ParentOrArityMismatch,
        ),
        (
            NodeMismatchDimension::Kind,
            RoundTripReason::NodeCorrespondenceMismatch,
        ),
        (
            NodeMismatchDimension::Arity,
            RoundTripReason::ParentOrArityMismatch,
        ),
        (
            NodeMismatchDimension::Tag,
            RoundTripReason::NodeCorrespondenceMismatch,
        ),
        (
            NodeMismatchDimension::Style,
            RoundTripReason::ScalarStyleMismatch,
        ),
        (
            NodeMismatchDimension::Decoded,
            RoundTripReason::DecodedScalarMismatch,
        ),
        (
            NodeMismatchDimension::Raw,
            RoundTripReason::ScalarSourceMismatch,
        ),
        (
            NodeMismatchDimension::Range,
            RoundTripReason::InvalidCandidateRange,
        ),
    ] {
        let artifact = reconcile_fault_for_test(
            b"a: b\n",
            ReconcileFault::OrderedNodeDimensions { node: 2, first },
        )
        .unwrap();
        let issue = artifact
            .round_trip_status()
            .report()
            .unwrap()
            .primary_issue()
            .unwrap();
        assert_eq!(issue.reason(), expected);
        assert_eq!(issue.node(), Some(artifact.nodes()[2].id()));
        assert!(issue.entry().is_none());
        assert!(issue.comment().is_none());
    }
}

#[test]
fn all_ten_read_only_reasons_are_reported_by_production_reconciliation() {
    let cases = [
        (
            include_bytes!("fixtures/read-only/node-correspondence-mismatch.yaml").as_slice(),
            ReconcileFault::Kind { node: 1 },
            RoundTripReason::NodeCorrespondenceMismatch,
        ),
        (
            include_bytes!("fixtures/read-only/parent-or-arity-mismatch.yaml").as_slice(),
            ReconcileFault::Parent { node: 1 },
            RoundTripReason::ParentOrArityMismatch,
        ),
        (
            include_bytes!("fixtures/read-only/scalar-style-mismatch.yaml").as_slice(),
            ReconcileFault::ScalarStyle { node: 2 },
            RoundTripReason::ScalarStyleMismatch,
        ),
        (
            include_bytes!("fixtures/read-only/decoded-scalar-mismatch.yaml").as_slice(),
            ReconcileFault::Decoded { node: 2 },
            RoundTripReason::DecodedScalarMismatch,
        ),
        (
            include_bytes!("fixtures/read-only/scalar-source-mismatch.yaml").as_slice(),
            ReconcileFault::Raw { node: 2 },
            RoundTripReason::ScalarSourceMismatch,
        ),
        (
            include_bytes!("fixtures/read-only/invalid-candidate-range.yaml").as_slice(),
            ReconcileFault::WholeOutside { node: 1 },
            RoundTripReason::InvalidCandidateRange,
        ),
        (
            include_bytes!("fixtures/read-only/overlapping-sibling-ranges.yaml").as_slice(),
            ReconcileFault::SiblingOverlap { node: 2 },
            RoundTripReason::OverlappingSiblingRanges,
        ),
        (
            include_bytes!("fixtures/read-only/no-op-render-mismatch.yaml").as_slice(),
            ReconcileFault::NoOpRender,
            RoundTripReason::NoOpRenderMismatch,
        ),
        (
            include_bytes!("fixtures/read-only/unsupported-presentation.yaml").as_slice(),
            ReconcileFault::UnsupportedPresentation,
            RoundTripReason::UnsupportedPresentation,
        ),
        (
            include_bytes!("fixtures/read-only/comment-ownership-ambiguous.yaml").as_slice(),
            ReconcileFault::CommentUnreconciledNode,
            RoundTripReason::CommentOwnershipAmbiguous,
        ),
    ];
    for (source, fault, reason) in cases {
        let artifact = reconcile_fault_for_test(source, fault).unwrap();
        let report = artifact.round_trip_status().report().unwrap();
        assert!(report.issues().iter().any(|issue| issue.reason() == reason));
    }
}

#[test]
fn deferred_fold_drops_presentation_before_full_production_builder_peak() {
    reset_pipeline_observation_for_test();
    let source = format!("value: \"{}a\"\n", "\\L".repeat(21_845));
    let parsed = parse_yaml_document(source.clone().into_bytes().into_boxed_slice()).unwrap();
    let (
        prepare_decoded,
        prepare_all,
        barrier,
        fold_copies,
        fold_peak,
        identity_count,
        identity_matches,
    ) = pipeline_observation_for_test();
    assert!(barrier);
    assert_eq!(fold_copies, 2);
    assert_eq!(identity_count, 2);
    assert_eq!(identity_matches, identity_count);
    assert!(prepare_decoded <= 3 * source.len() + 2 * parsed.yaml().nodes().len());
    assert!(prepare_all <= 4 * source.len() + 2 * parsed.yaml().nodes().len());
    assert!(fold_peak <= 3 * source.len() + 2 * parsed.yaml().nodes().len());
}

#[test]
fn reconciliation_draft_source_is_statically_text_free() {
    let source = include_str!("../reconcile.rs");
    let start = source.find("struct DraftNode").unwrap();
    let end = source.find("pub(super) fn reconcile").unwrap();
    let draft = &source[start..end];
    for forbidden in ["String", "Box<str>", "Cow<", "&[u8]", "PresentationNode"] {
        assert!(
            !draft.contains(forbidden),
            "draft retained forbidden text owner"
        );
    }
    let permit_start = source.find("pub(super) struct FoldPermit").unwrap();
    let permit = &source[permit_start..end];
    for forbidden in ["bool", "Presentation", "String", "Box<str>", "Cow<", "&"] {
        assert!(
            !permit.contains(forbidden),
            "fold permit retained bypass or owner"
        );
    }
    let semantic = include_str!("../semantic.rs");
    let fold_start = semantic.find("pub(super) fn fold(").unwrap();
    let fold_end = semantic[fold_start..].find("let build").unwrap() + fold_start;
    let signature = &semantic[fold_start..fold_end];
    assert!(signature.contains("FoldPermit"));
    assert!(!signature.contains("bool"));
    assert!(!signature.contains("Presentation"));
}

#[test]
fn unreconciled_suffix_still_moves_original_saphyr_decoded_facts() {
    let artifact =
        reconcile_fault_for_test(b"a: expanded\n", ReconcileFault::Kind { node: 1 }).unwrap();
    let suffix = artifact.nodes().last().unwrap();
    assert!(matches!(
        suffix.presentation(),
        NodePresentation::Unreconciled
    ));
    assert_eq!(suffix.decoded_scalar(), Some("expanded"));
}

#[test]
fn expansion_one_over_fails_before_prepare_or_fold_authority() {
    reset_pipeline_observation_for_test();
    let mut source = b"value: \"".to_vec();
    for _ in 0..21_845 {
        source.extend_from_slice(b"\\L");
    }
    source.extend_from_slice(b"aa\"\n");
    assert_eq!(
        failure(&source).kind(),
        ParseFailureKind::DecodedScalarLimit
    );
    assert_eq!(pipeline_observation_for_test(), (0, 0, false, 0, 0, 0, 0));
}

#[test]
fn semantic_failure_remains_err_when_reconciliation_control_would_mismatch() {
    let error = reconcile_fault_for_test(b"value\n", ReconcileFault::Decoded { node: 0 })
        .err()
        .unwrap();
    assert_eq!(error.kind(), ParseFailureKind::RootMustBeMapping);
}

#[test]
fn draft_capacity_arithmetic_is_sanitized() {
    assert_eq!(
        draft_allocation_overflow_for_test(),
        ParseFailureKind::SemanticBuilderInvariant
    );
    assert_eq!(
        issue_total_overflow_for_test(),
        ParseFailureKind::SemanticBuilderInvariant
    );
}

#[test]
fn issue_report_retains_exact_32_prefix_and_checked_total() {
    let mut source = Vec::new();
    for _ in 0..33 {
        source.extend_from_slice(b"# issue\n");
    }
    source.extend_from_slice(b"a: b\n");
    let artifact =
        reconcile_fault_for_test(&source, ReconcileFault::AllCommentsUnreconciled).unwrap();
    let report = artifact.round_trip_status().report().unwrap();
    assert_eq!(report.issues().len(), 32);
    assert_eq!(report.total_issues(), 33);
    assert!(report.is_truncated());
    assert!(
        report
            .issues()
            .iter()
            .enumerate()
            .all(|(index, issue)| issue.comment() == Some(artifact.comments()[index].id()))
    );
}

#[test]
fn preorder_zip_breaks_once_at_root_key_value_and_nested_child() {
    let source = b"root:\n  child:\n    leaf: value\nnext: tail\n";
    for (fault, first) in [
        (ReconcileFault::Arity { node: 0 }, 0usize),
        (ReconcileFault::Parent { node: 1 }, 1),
        (ReconcileFault::Kind { node: 2 }, 2),
        (ReconcileFault::Parent { node: 5 }, 5),
    ] {
        let artifact = reconcile_fault_for_test(source, fault).unwrap();
        let report = artifact.round_trip_status().report().unwrap();
        assert_eq!(
            report.issues()[0].node(),
            Some(artifact.nodes()[first].id())
        );
        assert!(artifact.nodes()[first..].iter().all(|node| {
            matches!(node.presentation(), NodePresentation::Unreconciled)
                && node.collection_style().is_none()
        }));
        assert!(artifact.entries().iter().enumerate().all(
            |(index, entry)| entry_ordinal_for_test(entry.id()) == index as u32
                && artifact.entry(entry.id()).is_some()
        ));
    }
}

#[test]
fn entry_wrapper_failure_cuts_off_ids_without_resuming() {
    let artifact = reconcile_fault_for_test(
        b"a: b\nc: d\ne: f\n",
        ReconcileFault::EntryOutside { entry: 1 },
    )
    .unwrap();
    assert_eq!(artifact.entries().len(), 1);
    assert_eq!(
        artifact.entries()[0].id(),
        artifact.entry(artifact.entries()[0].id()).unwrap().id()
    );
    assert!(
        artifact
            .nodes()
            .iter()
            .skip(3)
            .all(|node| node.entries().is_empty())
    );
    let issue = artifact
        .round_trip_status()
        .report()
        .unwrap()
        .primary_issue()
        .unwrap();
    assert!(issue.node().is_none());
    assert!(issue.entry().is_none());
    assert!(issue.comment().is_none());
    assert!(issue.yaml_offset().is_some());
}

#[test]
fn structural_break_with_large_entry_tail_never_mints_suffix_ids() {
    let mut source = Vec::new();
    for index in 0..256 {
        source.extend_from_slice(format!("k{index}: value\n").as_bytes());
    }
    let artifact =
        reconcile_fault_for_test(&source, ReconcileFault::EntryOutside { entry: 1 }).unwrap();
    assert_eq!(artifact.entries().len(), 1);
    assert!(
        artifact
            .nodes()
            .iter()
            .flat_map(|node| node.entries())
            .all(|id| artifact.entry(*id).is_some())
    );
    let issue = artifact
        .round_trip_status()
        .report()
        .unwrap()
        .primary_issue()
        .unwrap();
    assert!(issue.entry().is_none());
}

#[test]
fn node_and_entry_range_faults_are_read_only_without_panics() {
    let node_source = b"outer:\n  inner: value\nnext: tail\n";
    for (fault, reason, node, offset, retained_entries) in [
        (
            ReconcileFault::WholeOutside { node: 1 },
            RoundTripReason::InvalidCandidateRange,
            1usize,
            0u32,
            0usize,
        ),
        (
            ReconcileFault::PayloadOutside { node: 1 },
            RoundTripReason::ScalarSourceMismatch,
            1,
            node_source.len() as u32,
            0,
        ),
        (
            ReconcileFault::AncestorEscape { node: 3 },
            RoundTripReason::InvalidCandidateRange,
            3,
            0,
            1,
        ),
        (
            ReconcileFault::SiblingOverlap { node: 2 },
            RoundTripReason::OverlappingSiblingRanges,
            2,
            0,
            0,
        ),
        (
            ReconcileFault::RebaseOverflow,
            RoundTripReason::InvalidCandidateRange,
            0,
            0,
            0,
        ),
    ] {
        let artifact = reconcile_fault_for_test(node_source, fault).unwrap();
        let issue = artifact
            .round_trip_status()
            .report()
            .unwrap()
            .primary_issue()
            .unwrap();
        assert_eq!(issue.reason(), reason);
        assert_eq!(issue.node(), Some(artifact.nodes()[node].id()));
        assert!(issue.entry().is_none());
        assert!(issue.comment().is_none());
        assert_eq!(issue.yaml_offset().unwrap().get(), offset);
        assert!(matches!(
            artifact.nodes()[node].presentation(),
            NodePresentation::Unreconciled
        ));
        match fault {
            ReconcileFault::WholeOutside { .. }
            | ReconcileFault::PayloadOutside { .. }
            | ReconcileFault::AncestorEscape { .. } => {
                assert!(artifact.nodes().iter().enumerate().all(|(index, value)| {
                    (index == node)
                        == matches!(value.presentation(), NodePresentation::Unreconciled)
                }))
            }
            ReconcileFault::SiblingOverlap { .. } => {
                assert!(
                    artifact.nodes()[..2].iter().all(|value| !matches!(
                        value.presentation(),
                        NodePresentation::Unreconciled
                    ))
                );
                assert!(
                    artifact.nodes()[2..5].iter().all(|value| matches!(
                        value.presentation(),
                        NodePresentation::Unreconciled
                    ))
                );
                assert!(
                    artifact.nodes()[5..].iter().all(|value| !matches!(
                        value.presentation(),
                        NodePresentation::Unreconciled
                    ))
                );
            }
            ReconcileFault::RebaseOverflow => {
                assert!(artifact.nodes().iter().all(|value| {
                    matches!(value.presentation(), NodePresentation::Unreconciled)
                }))
            }
            _ => unreachable!("table contains only node-range faults"),
        }
        assert_eq!(artifact.entries().len(), retained_entries);
        assert_all_entry_ids_resolve(&artifact);
    }
    let utf8 = reconcile_fault_for_test(
        "a: é\n".as_bytes(),
        ReconcileFault::Utf8Boundary { node: 2, offset: 4 },
    )
    .unwrap();
    let utf8_issue = utf8
        .round_trip_status()
        .report()
        .unwrap()
        .primary_issue()
        .unwrap();
    assert_eq!(utf8_issue.reason(), RoundTripReason::ScalarSourceMismatch);
    assert_eq!(utf8_issue.node(), Some(utf8.nodes()[2].id()));
    assert!(utf8_issue.entry().is_none());
    assert!(utf8_issue.comment().is_none());
    assert!(utf8_issue.yaml_offset().is_none());
    assert!(matches!(
        utf8.nodes()[2].presentation(),
        NodePresentation::Unreconciled
    ));
    assert_eq!(utf8.entries().len(), 0);
    assert_all_entry_ids_resolve(&utf8);

    for (fault, expected_offset) in [
        (ReconcileFault::TagOutside { node: 2 }, 3u32),
        (ReconcileFault::TagPayloadOverlap { node: 2 }, 3u32),
    ] {
        let tag = reconcile_fault_for_test(b"a: !!str b\n", fault).unwrap();
        let issue = tag
            .round_trip_status()
            .report()
            .unwrap()
            .primary_issue()
            .unwrap();
        assert_eq!(issue.reason(), RoundTripReason::InvalidCandidateRange);
        assert_eq!(issue.node(), Some(tag.nodes()[2].id()));
        assert!(issue.entry().is_none());
        assert!(issue.comment().is_none());
        assert_eq!(issue.yaml_offset().unwrap().get(), expected_offset);
        assert!(matches!(
            tag.nodes()[2].presentation(),
            NodePresentation::Unreconciled
        ));
        assert_eq!(tag.entries().len(), 0);
        assert_all_entry_ids_resolve(&tag);
    }
    for (fault, reason, offset, retained_entries) in [
        (
            ReconcileFault::EntryOutside { entry: 1 },
            RoundTripReason::InvalidCandidateRange,
            0u32,
            1usize,
        ),
        (
            ReconcileFault::EntryOverlap { entry: 1 },
            RoundTripReason::OverlappingSiblingRanges,
            0,
            1,
        ),
        (
            ReconcileFault::EntryReverse,
            RoundTripReason::InvalidCandidateRange,
            5,
            0,
        ),
    ] {
        let artifact = reconcile_fault_for_test(b"a: b\nc: d\n", fault).unwrap();
        let issue = artifact
            .round_trip_status()
            .report()
            .unwrap()
            .primary_issue()
            .unwrap();
        assert_eq!(issue.reason(), reason);
        assert!(issue.node().is_none());
        assert!(issue.entry().is_none());
        assert!(issue.comment().is_none());
        assert_eq!(issue.yaml_offset().unwrap().get(), offset);
        assert_eq!(artifact.entries().len(), retained_entries);
        assert!(
            artifact
                .nodes()
                .iter()
                .all(|node| !matches!(node.presentation(), NodePresentation::Unreconciled))
        );
        assert_all_entry_ids_resolve(&artifact);
    }
}

#[test]
fn reversed_node_entry_and_comment_order_is_reported_without_forged_ids() {
    let node = reconcile_fault_for_test(b"a: one\nb: two\n", ReconcileFault::NodeReverse).unwrap();
    let node_issue = node
        .round_trip_status()
        .report()
        .unwrap()
        .primary_issue()
        .unwrap();
    assert_eq!(node_issue.reason(), RoundTripReason::InvalidCandidateRange);
    assert_eq!(node_issue.node(), Some(node.nodes()[2].id()));
    assert!(node_issue.entry().is_none());
    assert!(node_issue.comment().is_none());
    assert!(node_issue.yaml_offset().is_some());

    let entry =
        reconcile_fault_for_test(b"a: one\nb: two\n", ReconcileFault::EntryReverse).unwrap();
    let entry_issue = entry
        .round_trip_status()
        .report()
        .unwrap()
        .primary_issue()
        .unwrap();
    assert_eq!(entry_issue.reason(), RoundTripReason::InvalidCandidateRange);
    assert!(entry_issue.node().is_none());
    assert!(entry_issue.entry().is_none());
    assert!(entry_issue.comment().is_none());
    assert!(entry_issue.yaml_offset().is_some());

    let source = b"a: one # first\nb: two # second\n";
    let comments = reconcile_fault_for_test(source, ReconcileFault::CommentReverse).unwrap();
    assert!(comments.comments().iter().any(|comment| {
        comment.placement() == super::super::CommentPlacement::Unresolved
            && source[comment.yaml_range().start().get() as usize
                ..comment.yaml_range().end().get() as usize]
                .starts_with(b"#")
    }));
}

#[test]
fn production_work_counter_is_observed_and_within_closed_form_bound() {
    reset_pipeline_observation_for_test();
    let parsed = parse_yaml_document(
        b"# document\na: [b, c]\n# middle\nd: e # inline\n"
            .to_vec()
            .into_boxed_slice(),
    )
    .unwrap();
    let (work, bound) = reconciliation_work_for_test();
    assert!(work > parsed.yaml_bytes().len() as u64);
    assert_eq!(work, 547);
    assert_eq!(bound, 1_036);
    assert!(work <= bound);
}

#[test]
fn production_work_one_over_is_sanitized_before_artifact_authority() {
    reset_pipeline_observation_for_test();
    let failure = reconcile_fault_for_test(b"a: b\n", ReconcileFault::WorkOneOver)
        .err()
        .expect("the real final node visit must exceed the closed-form bound");
    assert_eq!(failure.kind(), ParseFailureKind::SemanticBuilderInvariant);
}

#[test]
fn reconciliation_source_has_no_search_sort_or_repeated_parent_walk() {
    let source = include_str!("../reconcile.rs");
    for forbidden in [
        ".iter().find(",
        ".rposition(",
        ".sort",
        ".parent()",
        "inline_before_comment",
        "yaml.get(yaml_line_start..end)",
    ] {
        assert!(
            !source.contains(forbidden),
            "forbidden reconciliation search"
        );
    }
    assert!(source.contains("entry_direct_tables"));
    assert!(source.contains("assign_boundary_runs"));
    assert!(source.contains("boundary_run"));
    assert!(source.contains("work.bytes_equal"));
    assert!(source.contains("seed_one_over_before_final_visit"));
    assert_eq!(source.matches("work.metered_copy").count(), 5);
    assert_eq!(source.matches(".resize(").count(), 1);
    assert_eq!(source.matches(".into_boxed_slice()").count(), 1);
    assert!(source.contains("fn metered_resize"));
    assert!(source.contains("fn metered_box"));
    assert!(source.contains("tables.work.comment()?"));
    let presentation = include_str!("../presentation.rs");
    assert!(presentation.contains("work_total: u64"));
    assert!(presentation.contains("metered_equal"));
    assert_eq!(presentation.matches(".into_boxed_slice()").count(), 1);
    let line_cursor = &presentation[presentation.find("fn line_for_offset").unwrap()
        ..presentation.find("fn handle_token").unwrap()];
    assert!(line_cursor.contains("charge_work(work, 1)?"));
    assert!(!presentation.contains("!= Some(text.as_bytes())"));
}

#[test]
fn temporary_provenance_and_final_draft_inventory_are_bounded_and_text_free() {
    use super::super::presentation::CommentCandidate;
    assert_eq!(core::mem::size_of::<CommentCandidate>(), 40);
    let presentation = include_str!("../presentation.rs");
    let candidate_start = presentation
        .find("pub(super) struct CommentCandidate")
        .unwrap();
    let candidate_end = presentation[candidate_start..]
        .find("impl Presentation")
        .unwrap()
        + candidate_start;
    let candidate = &presentation[candidate_start..candidate_end];
    for forbidden in ["String", "Box<str>", "Cow<", "SyntaxNode", "SyntaxToken"] {
        assert!(!candidate.contains(forbidden));
    }
    let reconciliation = include_str!("../reconcile.rs");
    let artifact_start = reconciliation
        .find("pub(crate) struct ParsedYamlArtifact")
        .unwrap();
    let artifact_end = reconciliation[artifact_start..]
        .find("impl ReconciledNode")
        .unwrap()
        + artifact_start;
    let artifact = &reconciliation[artifact_start..artifact_end];
    for dropped_scratch in [
        "comment_line_starts",
        "comment_line_blank",
        "EntrySlots",
        "blank_prefix",
        "SemanticTrace",
        "Presentation",
    ] {
        assert!(!artifact.contains(dropped_scratch));
    }
}

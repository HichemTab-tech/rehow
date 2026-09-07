use crate::domain::{UnknownYamlValue, UnknownYamlValueKind};

use super::super::presentation;
use super::super::{
    CommentOwner, NodePresentation, NodeRole, ParsedGuideSource, ParsedYamlArtifact,
    ParsedYamlDocument, ReconciledEntryKind, RoundTripReason, YamlByteRange,
};
use super::{parse_guide_source, parse_yaml_document};

fn contains(range: YamlByteRange, inner: YamlByteRange) -> bool {
    range.start() <= inner.start() && inner.end() <= range.end()
}

fn assert_semantic_traversal_terminates(artifact: &ParsedYamlArtifact) {
    let mut pending: Vec<&UnknownYamlValue> = artifact
        .semantic()
        .entries()
        .iter()
        .map(|entry| entry.value())
        .collect();
    let mut visited = 0usize;
    while let Some(value) = pending.pop() {
        visited = visited.checked_add(1).unwrap();
        assert!(visited <= 20_000);
        match value.kind() {
            UnknownYamlValueKind::Scalar => assert!(value.as_scalar().is_some()),
            UnknownYamlValueKind::Sequence => pending.extend(value.as_sequence().unwrap()),
            UnknownYamlValueKind::Mapping => {
                for entry in value.as_mapping().unwrap() {
                    pending.push(entry.key());
                    pending.push(entry.value());
                }
            }
        }
    }
}

fn assert_artifact_invariants(artifact: &ParsedYamlArtifact, yaml: &[u8], yaml_base: u32) {
    let yaml_text = core::str::from_utf8(yaml).unwrap();
    let inspected = presentation::inspect(yaml_text, yaml, 0).unwrap();
    assert!(!artifact.nodes().is_empty());
    assert!(artifact.nodes().len() <= 20_000);
    let emitted_node_cap = yaml.len().checked_mul(2).unwrap().checked_add(1).unwrap();
    assert!(artifact.nodes().len() <= emitted_node_cap);
    assert!(artifact.entries().len() <= 19_999);
    assert!(artifact.entries().len() <= artifact.nodes().len().saturating_sub(1));
    assert!(artifact.comments().len() <= yaml.len());
    assert_eq!(artifact.root_id(), artifact.nodes()[0].id());
    let has_node_issue = |reason, id| {
        artifact.round_trip_status().report().is_some_and(|report| {
            report
                .issues()
                .iter()
                .any(|issue| issue.reason() == reason && issue.node() == Some(id))
        })
    };

    let mut structural_suffix = false;
    let mut decoded_bytes = 0usize;
    let mut child_references = vec![0usize; artifact.nodes().len()];
    for (ordinal, node) in artifact.nodes().iter().enumerate() {
        assert!(core::ptr::eq(artifact.node(node.id()).unwrap(), node));
        decoded_bytes = decoded_bytes
            .checked_add(node.decoded_scalar().map_or(0, str::len))
            .unwrap();
        if ordinal == 0 {
            assert_eq!(node.parent(), None);
            assert_eq!(node.role(), NodeRole::Root);
        } else {
            let parent = artifact.node(node.parent().unwrap()).unwrap();
            assert!(parent.children().contains(&node.id()));
            assert!(!matches!(node.role(), NodeRole::Root));
        }
        let expected = inspected.nodes.get(ordinal);
        if let Some(expected) = expected {
            assert_eq!(expected.ordinal as usize, ordinal);
        } else {
            assert!(matches!(
                node.presentation(),
                NodePresentation::Unreconciled
            ));
        }
        if !matches!(node.presentation(), NodePresentation::Unreconciled) {
            let expected = expected.expect("reconciled prefix requires presentation node");
            assert_eq!(node.role(), expected.role);
            assert_eq!(node.kind(), expected.kind);
            assert!(
                node.decoded_scalar() == expected.decoded.as_deref()
                    || has_node_issue(RoundTripReason::DecodedScalarMismatch, node.id())
            );
            assert!(
                node.scalar_style() == expected.scalar_style
                    || has_node_issue(RoundTripReason::ScalarStyleMismatch, node.id())
            );
            assert_eq!(node.collection_style(), expected.collection_style);
            assert!(
                node.source_tag() == expected.tag
                    || has_node_issue(RoundTripReason::NodeCorrespondenceMismatch, node.id())
            );
            if let Some(raw) = expected.raw.as_deref() {
                let start = expected.payload.start().get() as usize;
                let end = expected.payload.end().get() as usize;
                assert!(
                    yaml.get(start..end) == Some(raw.as_bytes())
                        || has_node_issue(RoundTripReason::ScalarSourceMismatch, node.id())
                );
            }
            assert_eq!(node.children().len(), expected.child_count as usize);
            assert_eq!(
                node.parent().map(|id| {
                    artifact
                        .nodes()
                        .iter()
                        .position(|candidate| candidate.id() == id)
                        .unwrap() as u32
                }),
                expected.parent
            );
        }
        let mut previous_end = None;
        for child_id in node.children() {
            let child = artifact.node(*child_id).unwrap();
            let child_ordinal = artifact
                .nodes()
                .iter()
                .position(|candidate| candidate.id() == *child_id)
                .unwrap();
            child_references[child_ordinal] += 1;
            assert_eq!(child.parent(), Some(node.id()));
            if let NodePresentation::Concrete {
                whole_yaml_range, ..
            } = child.presentation()
            {
                if let Some(end) = previous_end {
                    assert!(end <= whole_yaml_range.start());
                }
                previous_end = Some(whole_yaml_range.end());
            }
        }
        match node.presentation() {
            NodePresentation::Concrete {
                whole_yaml_range,
                whole_file_range,
                payload_yaml_range,
                payload_file_range,
                tag_yaml_range,
                tag_file_range,
            } => {
                let expected = expected.expect("concrete node requires presentation node");
                assert!(!structural_suffix);
                assert!(contains(whole_yaml_range, payload_yaml_range));
                assert!(whole_yaml_range.end().get() as usize <= yaml.len());
                assert!(yaml_text.is_char_boundary(whole_yaml_range.start().get() as usize));
                assert!(yaml_text.is_char_boundary(whole_yaml_range.end().get() as usize));
                assert_eq!(
                    whole_file_range.start().get(),
                    yaml_base + whole_yaml_range.start().get()
                );
                assert_eq!(
                    whole_file_range.end().get(),
                    yaml_base + whole_yaml_range.end().get()
                );
                assert_eq!(
                    payload_file_range.start().get(),
                    yaml_base + payload_yaml_range.start().get()
                );
                assert_eq!(
                    payload_file_range.end().get(),
                    yaml_base + payload_yaml_range.end().get()
                );
                match (tag_yaml_range, tag_file_range) {
                    (Some(tag), Some(file)) => {
                        assert!(contains(whole_yaml_range, tag));
                        assert!(
                            tag.end() <= payload_yaml_range.start()
                                || payload_yaml_range.end() <= tag.start()
                        );
                        assert_eq!(file.start().get(), yaml_base + tag.start().get());
                        assert_eq!(file.end().get(), yaml_base + tag.end().get());
                    }
                    (None, None) => {}
                    _ => panic!("tag coordinate representations disagree"),
                }
                assert_eq!(whole_yaml_range, expected.whole);
                assert_eq!(payload_yaml_range, expected.payload);
                assert_eq!(tag_yaml_range, expected.tag_range);
                if let Some(parent) = node.parent().and_then(|id| artifact.node(id))
                    && let NodePresentation::Concrete {
                        whole_yaml_range: parent_range,
                        ..
                    } = parent.presentation()
                {
                    assert!(contains(parent_range, whole_yaml_range));
                }
            }
            NodePresentation::ImplicitEmpty {
                yaml_site,
                file_site,
            } => {
                assert!(!structural_suffix);
                assert!(yaml_site.get() as usize <= yaml.len());
                assert_eq!(file_site.get(), yaml_base + yaml_site.get());
            }
            NodePresentation::Unreconciled => structural_suffix = true,
        }
    }
    assert_eq!(child_references.first(), Some(&0));
    assert!(child_references.iter().skip(1).all(|count| *count == 1));
    assert_eq!(
        artifact
            .nodes()
            .iter()
            .map(|node| node.children().len())
            .sum::<usize>(),
        artifact.nodes().len().saturating_sub(1)
    );
    assert!(decoded_bytes <= 3 * yaml.len() + 2 * artifact.nodes().len());

    let mut previous_by_parent = vec![None; artifact.nodes().len()];
    let mut entry_references = vec![0usize; artifact.entries().len()];
    for (entry_ordinal, entry) in artifact.entries().iter().enumerate() {
        assert!(core::ptr::eq(artifact.entry(entry.id()).unwrap(), entry));
        let parent = artifact.node(entry.parent()).unwrap();
        assert_eq!(
            parent.entries().get(entry.index() as usize),
            Some(&entry.id())
        );
        entry_references[entry_ordinal] += 1;
        let expected = &inspected.entries[entry_ordinal];
        assert_eq!(
            expected.parent as usize,
            parent_slot_for(artifact, entry.parent())
        );
        assert_eq!(expected.index, entry.index());
        assert_eq!(expected.range, entry.yaml_range());
        assert_eq!(expected.style, entry.collection_style());
        assert!(entry.yaml_range().end().get() as usize <= yaml.len());
        assert_eq!(
            entry.file_range().start().get(),
            yaml_base + entry.yaml_range().start().get()
        );
        assert_eq!(
            entry.file_range().end().get(),
            yaml_base + entry.yaml_range().end().get()
        );
        let parent_slot = artifact
            .nodes()
            .iter()
            .position(|node| node.id() == entry.parent())
            .unwrap();
        if let Some(end) = previous_by_parent[parent_slot] {
            assert!(end <= entry.yaml_range().start());
        }
        previous_by_parent[parent_slot] = Some(entry.yaml_range().end());
        match entry.kind() {
            ReconciledEntryKind::Mapping { key, value } => {
                let key = artifact.node(key).unwrap();
                let value = artifact.node(value).unwrap();
                assert_eq!(key.parent(), Some(entry.parent()));
                assert_eq!(value.parent(), Some(entry.parent()));
                assert_eq!(
                    key.role(),
                    NodeRole::MappingKey {
                        index: entry.index()
                    }
                );
                assert_eq!(
                    value.role(),
                    NodeRole::MappingValue {
                        index: entry.index()
                    }
                );
            }
            ReconciledEntryKind::Sequence { value } => {
                let value = artifact.node(value).unwrap();
                assert_eq!(value.parent(), Some(entry.parent()));
                assert_eq!(
                    value.role(),
                    NodeRole::SequenceItem {
                        index: entry.index()
                    }
                );
            }
        }
        if let NodePresentation::Concrete {
            whole_yaml_range, ..
        } = parent.presentation()
        {
            assert!(contains(whole_yaml_range, entry.yaml_range()));
        }
    }
    assert!(entry_references.iter().all(|count| *count == 1));
    assert_eq!(
        artifact
            .nodes()
            .iter()
            .map(|node| node.entries().len())
            .sum::<usize>(),
        artifact.entries().len()
    );

    let mut previous_comment_end = None;
    for comment in artifact.comments() {
        assert!(core::ptr::eq(
            artifact.comment(comment.id()).unwrap(),
            comment
        ));
        assert!(comment.yaml_range().end().get() as usize <= yaml.len());
        assert!(yaml_text.is_char_boundary(comment.yaml_range().start().get() as usize));
        assert!(yaml_text.is_char_boundary(comment.yaml_range().end().get() as usize));
        assert_eq!(
            comment.file_range().start().get(),
            yaml_base + comment.yaml_range().start().get()
        );
        assert_eq!(
            comment.file_range().end().get(),
            yaml_base + comment.yaml_range().end().get()
        );
        if let Some(end) = previous_comment_end {
            assert!(end <= comment.yaml_range().start());
        }
        previous_comment_end = Some(comment.yaml_range().end());
        match comment.owner() {
            CommentOwner::Entry(id) => assert!(artifact.entry(id).is_some()),
            CommentOwner::Node(id) => assert!(artifact.node(id).is_some()),
            CommentOwner::Boundary {
                parent,
                before,
                after,
            } => {
                assert!(parent.is_none_or(|id| artifact.node(id).is_some()));
                assert!(before.is_none_or(|id| artifact.entry(id).is_some()));
                assert!(after.is_none_or(|id| artifact.entry(id).is_some()));
            }
            CommentOwner::Ambiguous => {}
        }
    }
    if let Some(report) = artifact.round_trip_status().report() {
        assert!(report.issues().len() <= 32);
        assert!(report.total_issues() as usize >= report.issues().len());
        assert_eq!(
            report.is_truncated(),
            report.total_issues() as usize > report.issues().len()
        );
        assert!(core::ptr::eq(
            report.primary_issue().unwrap(),
            &report.issues()[0]
        ));
        let mut previous_order = None;
        for issue in report.issues() {
            assert!(issue.node().is_none_or(|id| artifact.node(id).is_some()));
            assert!(issue.entry().is_none_or(|id| artifact.entry(id).is_some()));
            assert!(
                issue
                    .comment()
                    .is_none_or(|id| artifact.comment(id).is_some())
            );
            let order = issue_order_key(artifact, issue);
            assert!(previous_order.is_none_or(|previous| previous <= order));
            previous_order = Some(order);
            assert!(
                issue
                    .yaml_offset()
                    .is_none_or(|offset| offset.get() as usize <= yaml.len())
            );
        }
    } else {
        assert!(artifact.round_trip_status().is_proven());
        assert!(
            artifact
                .nodes()
                .iter()
                .all(|node| !matches!(node.presentation(), NodePresentation::Unreconciled))
        );
    }
    assert_semantic_traversal_terminates(artifact);
}

fn issue_order_key(
    artifact: &ParsedYamlArtifact,
    issue: &super::super::RoundTripIssue,
) -> (u8, usize, usize) {
    let reason = issue.reason() as usize;
    if let Some(id) = issue.node() {
        return (0, parent_slot_for(artifact, id), reason);
    }
    if let Some(id) = issue.entry() {
        let ordinal = artifact
            .entries()
            .iter()
            .position(|entry| entry.id() == id)
            .unwrap();
        return (1, ordinal, reason);
    }
    if let Some(id) = issue.comment() {
        let ordinal = artifact
            .comments()
            .iter()
            .position(|comment| comment.id() == id)
            .unwrap();
        return (2, ordinal, reason);
    }
    match issue.reason() {
        RoundTripReason::InvalidCandidateRange | RoundTripReason::OverlappingSiblingRanges => {
            (1, usize::MAX, reason)
        }
        RoundTripReason::CommentOwnershipAmbiguous => (2, usize::MAX, reason),
        RoundTripReason::NoOpRenderMismatch => (3, 0, reason),
        RoundTripReason::UnsupportedPresentation => (4, 0, reason),
        _ => (0, usize::MAX, reason),
    }
}

fn assert_accepted_artifact_invariants(parsed: &ParsedYamlDocument) {
    assert_artifact_invariants(
        parsed.yaml(),
        parsed.yaml_bytes(),
        parsed.envelope().yaml_file_range().start().get(),
    );
}

fn assert_accepted_guide_invariants(parsed: &ParsedGuideSource) {
    assert_artifact_invariants(
        parsed.yaml(),
        parsed.yaml_bytes(),
        parsed.envelope().yaml_file_range().start().get(),
    );
}

fn parent_slot_for(artifact: &ParsedYamlArtifact, id: super::super::NodeId) -> usize {
    artifact
        .nodes()
        .iter()
        .position(|node| node.id() == id)
        .unwrap()
}

fn consume_result(result: Result<ParsedYamlDocument, super::super::ParseFailure>) {
    match result {
        Ok(parsed) => assert_accepted_artifact_invariants(&parsed),
        Err(error) => {
            assert!((error.kind() as usize) < 41);
            assert!(!format!("{error:?}").is_empty());
        }
    }
}

fn consume_guide_result(result: Result<ParsedGuideSource, super::super::ParseFailure>) {
    match result {
        Ok(parsed) => assert_accepted_guide_invariants(&parsed),
        Err(error) => {
            assert!((error.kind() as usize) < 41);
            assert!(!format!("{error:?}").is_empty());
        }
    }
}

fn accepted_mutation_corpus() -> Vec<(bool, Box<[u8]>)> {
    let mut corpus = Vec::new();
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
        corpus.push((false, Box::<[u8]>::from(source)));
    }
    for source in [
        include_bytes!("fixtures/accepted/guide-bom-lf-final-newline.md").as_slice(),
        include_bytes!("fixtures/accepted/guide-lf.md").as_slice(),
    ] {
        corpus.push((true, Box::<[u8]>::from(source)));
    }
    for (guide, descriptor) in [
        (
            false,
            include_str!("fixtures/accepted/canonical-scalar-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/decoded-scalar-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/depth-limit-exact.recipe"),
        ),
        (
            true,
            include_str!("fixtures/accepted/frontmatter-limit-exact.recipe"),
        ),
        (
            true,
            include_str!("fixtures/accepted/guide-crlf-no-final-newline.recipe"),
        ),
        (
            true,
            include_str!("fixtures/accepted/guide-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/mapping-entry-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/node-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/numeric-digit-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/numeric-work-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/raw-scalar-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/sequence-entry-limit-exact.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/whole-crlf-final-newline.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/whole-lf-no-final-newline.recipe"),
        ),
        (
            false,
            include_str!("fixtures/accepted/whole-limit-exact.recipe"),
        ),
    ] {
        corpus.push((guide, super::materialize_fixture_recipe(descriptor).bytes));
    }
    corpus
}

#[test]
fn fixed_seed_arbitrary_and_mutation_harness_4096_each() {
    let mut state = 0x5eed_cafe_f00d_beefu64;
    for _ in 0..4096 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let len = (state as usize) % 64;
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            bytes.push(state as u8);
        }
        consume_result(parse_yaml_document(bytes.into_boxed_slice()));
    }
    let accepted = accepted_mutation_corpus();
    assert_eq!(accepted.len(), 27);
    for (guide, source) in &accepted {
        if *guide {
            assert_accepted_guide_invariants(
                &parse_guide_source(source.clone()).expect("accepted Guide fixture"),
            );
        } else {
            assert_accepted_artifact_invariants(
                &parse_yaml_document(source.clone()).expect("accepted YAML fixture"),
            );
        }
    }
    for index in 0..4096 {
        let corpus_index = if index < accepted.len() {
            index
        } else {
            index % 12
        };
        let (guide, source) = &accepted[corpus_index];
        let mut bytes = source.to_vec();
        let at = index % bytes.len();
        bytes[at] ^= (index as u8).wrapping_mul(31) | 1;
        if *guide {
            consume_guide_result(parse_guide_source(bytes.into_boxed_slice()));
        } else {
            consume_result(parse_yaml_document(bytes.into_boxed_slice()));
        }
    }
}

use rowan::ast::AstNode;
use rowan::{NodeOrToken, SyntaxNode};
use yaml_edit::{Lang, ParseErrorKind, Scalar, SyntaxKind, YamlFile};

use super::reconcile::{
    CollectionPresentationStyle, NodeRole, ReconciledNodeKind, ScalarPresentationStyle, SourceTag,
};
use super::source::{
    ParseFailure, ParseFailureKind, ParseLimitScope, YamlByteOffset, YamlByteRange, position,
    yaml_range,
};

const MAX_RAW_SCALAR: usize = 65_536;
const MAX_RICH_NODES: usize = 20_000;
const MAX_RICH_ENTRIES: usize = MAX_RICH_NODES - 1;

#[repr(C)]
struct TailDescriptor {
    ordinal: u32,
    parent_ordinal: u32,
    index: u32,
    role: u8,
    kind: u8,
    guard: u8,
    reserved: u8,
}
const _: [(); 16] = [(); core::mem::size_of::<TailDescriptor>()];
const _: [(); 4] = [(); core::mem::align_of::<TailDescriptor>()];

#[cfg(test)]
pub(super) fn tail_descriptor_layout() -> (usize, usize) {
    (
        core::mem::size_of::<TailDescriptor>(),
        core::mem::align_of::<TailDescriptor>(),
    )
}

pub(super) struct Presentation {
    pub(super) nodes: Vec<PresentationNode>,
    pub(super) entries: Vec<PresentationEntry>,
    pub(super) comments: Vec<CommentCandidate>,
    pub(super) no_op_matches: bool,
    pub(super) unsupported: bool,
    descriptors: Box<[TailDescriptor]>,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "comment line geometry is asserted only by presentation tests"
        )
    )]
    pub(super) comment_line_starts: Box<[u32]>,
    pub(super) comment_line_blank: Box<[u8]>,
    pub(super) comment_region_len: u32,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "comment line geometry is asserted only by presentation tests"
        )
    )]
    pub(super) yaml_region_offset: u32,
    pub(super) syntax_elements: u32,
    pub(super) work_total: u64,
    #[cfg(test)]
    pub(super) force_work_one_over: bool,
}

#[derive(Clone, Copy)]
pub(super) struct CommentCandidate {
    pub(super) range: YamlByteRange,
    pub(super) line: u32,
    pub(super) deepest_node: Option<u32>,
    pub(super) deepest_entry: Option<u32>,
    pub(super) deepest_collection: Option<u32>,
    pub(super) inline_before: bool,
}

impl Presentation {
    #[cfg(test)]
    pub(super) fn guard_code(&self, index: usize) -> Option<u8> {
        self.descriptors.get(index).map(|value| value.guard)
    }

    pub(super) fn descriptor_count(&self) -> usize {
        self.descriptors.len()
    }

    pub(super) fn descriptor_facts(&self, index: usize) -> Option<(u32, u32, u32, u8, u8, u8)> {
        self.descriptors.get(index).map(|value| {
            (
                value.ordinal,
                value.parent_ordinal,
                value.index,
                value.role,
                value.kind,
                value.guard,
            )
        })
    }
}

pub(super) struct PresentationNode {
    pub(super) ordinal: u32,
    pub(super) parent: Option<u32>,
    pub(super) role: NodeRole,
    pub(super) kind: ReconciledNodeKind,
    pub(super) whole: YamlByteRange,
    pub(super) payload: YamlByteRange,
    pub(super) tag_range: Option<YamlByteRange>,
    pub(super) tag: SourceTag,
    pub(super) decoded: Option<String>,
    pub(super) raw: Option<String>,
    pub(super) scalar_style: Option<ScalarPresentationStyle>,
    pub(super) collection_style: Option<CollectionPresentationStyle>,
    pub(super) child_count: u32,
    pub(super) start_line: u32,
    pub(super) end_line: u32,
}

pub(super) struct PresentationEntry {
    pub(super) parent: u32,
    pub(super) index: u32,
    pub(super) range: YamlByteRange,
    pub(super) sequence: bool,
    pub(super) style: CollectionPresentationStyle,
    pub(super) end_line: u32,
}

#[derive(Clone, Copy)]
struct TagInfo {
    tag: SourceTag,
    tag_range: YamlByteRange,
    whole: YamlByteRange,
}

struct Frame {
    kind: SyntaxKind,
    range: YamlByteRange,
    semantic: Option<u32>,
    semantic_parent: Option<u32>,
    payload_role: NodeRole,
    eof_context: bool,
    entry_index: Option<u32>,
    tag: Option<TagInfo>,
    collection_style: Option<CollectionPresentationStyle>,
    direct_nodes: u32,
    first_node: Option<SyntaxKind>,
    second_node: Option<SyntaxKind>,
    direct_tokens: u32,
    zero_null_tokens: u32,
    tag_tokens: u32,
    left_delimiters: u32,
    right_delimiters: u32,
    colons: u32,
    questions: u32,
    dashes: u32,
    commas: u32,
    entry_count: u32,
    deepest_node: Option<u32>,
    deepest_entry: Option<u32>,
    deepest_collection: Option<u32>,
    presentation_entry: Option<u32>,
}

struct FrameSeed {
    kind: SyntaxKind,
    range: YamlByteRange,
    semantic: Option<u32>,
    semantic_parent: Option<u32>,
    payload_role: NodeRole,
    eof_context: bool,
    entry_index: Option<u32>,
    collection_style: Option<CollectionPresentationStyle>,
    deepest_node: Option<u32>,
    deepest_entry: Option<u32>,
    deepest_collection: Option<u32>,
    presentation_entry: Option<u32>,
}

impl Frame {
    fn new(seed: FrameSeed) -> Self {
        Self {
            kind: seed.kind,
            range: seed.range,
            semantic: seed.semantic,
            semantic_parent: seed.semantic_parent,
            payload_role: seed.payload_role,
            eof_context: seed.eof_context,
            entry_index: seed.entry_index,
            tag: None,
            collection_style: seed.collection_style,
            direct_nodes: 0,
            first_node: None,
            second_node: None,
            direct_tokens: 0,
            zero_null_tokens: 0,
            tag_tokens: 0,
            left_delimiters: 0,
            right_delimiters: 0,
            colons: 0,
            questions: 0,
            dashes: 0,
            commas: 0,
            entry_count: 0,
            deepest_node: seed.deepest_node,
            deepest_entry: seed.deepest_entry,
            deepest_collection: seed.deepest_collection,
            presentation_entry: seed.presentation_entry,
        }
    }
}

#[derive(Clone)]
struct PendingFailure {
    offset: usize,
    rank: u8,
    failure: ParseFailure,
}

struct Pending {
    coverage: Option<PendingFailure>,
    source: Option<PendingFailure>,
    shape: Option<PendingFailure>,
    tag_kind: Option<PendingFailure>,
    raw_scalar: Option<PendingFailure>,
}

struct LeafState {
    cursor: usize,
    eof_seen: bool,
}

struct LeafElement<'a> {
    kind: SyntaxKind,
    range: YamlByteRange,
    dependency_text: &'a [u8],
}

struct LeafContext {
    frame_kind: SyntaxKind,
    frame_empty: bool,
    direct_nodes: u32,
    direct_tokens: u32,
    eof_context: bool,
    parent_kind: Option<SyntaxKind>,
}

struct CommentLineTable {
    starts: Box<[u32]>,
    blank: Box<[u8]>,
}

struct Traversal<'a> {
    descriptor_limit: usize,
    context_limit: usize,
    line_starts: &'a [u32],
    yaml_region_offset: usize,
    line_cursor: usize,
    last_content_line: Option<u32>,
    work_total: u64,
}

impl Traversal<'_> {
    fn line(&mut self, yaml_offset: YamlByteOffset) -> Result<u32, ParseFailure> {
        let file_offset = self
            .yaml_region_offset
            .checked_add(yaml_offset.get() as usize)
            .ok_or_else(invariant)?;
        line_for_offset(
            self.line_starts,
            file_offset,
            &mut self.line_cursor,
            &mut self.work_total,
        )
    }

    fn note_content(&mut self, range: YamlByteRange) -> Result<(), ParseFailure> {
        let last = usize::try_from(range.end().get().checked_sub(1).ok_or_else(invariant)?)
            .map_err(|_| invariant())?;
        self.last_content_line = Some(self.line(yaml_range(last, last)?.start())?);
        Ok(())
    }

    fn charge(&mut self, amount: usize) -> Result<(), ParseFailure> {
        charge_work(&mut self.work_total, amount)
    }
}

struct CopyBudget {
    decoded_total: usize,
    raw_total: usize,
    decoded_base_bound: usize,
}

struct BuildState {
    nodes: Vec<PresentationNode>,
    entries: Vec<PresentationEntry>,
    comments: Vec<CommentCandidate>,
    descriptors: Vec<TailDescriptor>,
    pending: Pending,
    copies: CopyBudget,
}

#[derive(Clone, Copy)]
struct ScalarContext {
    offset: usize,
    decoded_bound: usize,
}

#[derive(Clone, Copy)]
enum NodeClass {
    Normal,
    Alias,
    Shape,
}

#[derive(Clone, Copy)]
enum TokenClass {
    Structural,
    Directive,
    DocumentMarker,
    Anchor,
    Alias,
    RawBom,
    Unterminated,
    Contextual,
}

pub(super) fn inspect(
    text: &str,
    comment_region: &[u8],
    yaml_region_offset: usize,
) -> Result<Presentation, ParseFailure> {
    let parse = YamlFile::parse(text);
    if !parse.positioned_errors().is_empty() {
        return Err(ParseFailure::at(
            ParseFailureKind::PresentationSyntax,
            select_positioned_error(
                text,
                parse
                    .positioned_errors()
                    .iter()
                    .enumerate()
                    .map(|(index, error)| {
                        (
                            index,
                            error.range.start,
                            error.range.end,
                            error.kind.clone(),
                        )
                    }),
            ),
        ));
    }

    let yaml_end = yaml_region_offset
        .checked_add(text.len())
        .ok_or_else(invariant)?;
    let mut work_total = 0u64;
    let exact_source = if yaml_end > comment_region.len() {
        false
    } else {
        metered_equal(
            comment_region
                .get(yaml_region_offset..yaml_end)
                .ok_or_else(invariant)?,
            text.as_bytes(),
            &mut work_total,
        )?
    };
    if !exact_source {
        return Err(invariant());
    }
    let comment_lines = scan_comment_lines(comment_region, &mut work_total)?;
    let root = parse.tree();
    let root_syntax = root.syntax().clone();
    let descriptor_limit = text
        .len()
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(invariant)?;
    descriptor_limit
        .checked_mul(core::mem::size_of::<TailDescriptor>())
        .ok_or_else(invariant)?;

    let rich_capacity = descriptor_limit.min(MAX_RICH_NODES);
    let rich_entry_capacity = rich_capacity.checked_sub(1).ok_or_else(invariant)?;
    let context_capacity = text.len().checked_add(1).ok_or_else(invariant)?;
    let mut nodes = Vec::new();
    reserve(&mut nodes, rich_capacity)?;
    let mut entries = Vec::new();
    reserve(&mut entries, rich_entry_capacity.min(MAX_RICH_ENTRIES))?;
    let mut comments = Vec::new();
    reserve(&mut comments, text.len())?;
    let mut descriptors = Vec::new();
    reserve(&mut descriptors, descriptor_limit)?;
    let mut contexts = Vec::<Frame>::new();
    reserve(&mut contexts, descriptor_limit.min(context_capacity))?;

    let mut leaf = LeafState {
        cursor: 0,
        eof_seen: false,
    };
    let decoded_base_bound = text
        .len()
        .checked_mul(3)
        .and_then(|value| value.checked_add(1))
        .map(|value| value / 2)
        .ok_or_else(invariant)?;
    let element_limit = text
        .len()
        .checked_mul(8)
        .and_then(|value| value.checked_add(64))
        .ok_or_else(invariant)?;
    let mut build = BuildState {
        nodes,
        entries,
        comments,
        descriptors,
        pending: Pending {
            coverage: None,
            source: None,
            shape: None,
            tag_kind: None,
            raw_scalar: None,
        },
        copies: CopyBudget {
            decoded_total: 0,
            raw_total: 0,
            decoded_base_bound,
        },
    };

    let mut visited = 0usize;
    let mut traversal = Traversal {
        descriptor_limit,
        context_limit: element_limit,
        line_starts: &comment_lines.starts,
        yaml_region_offset,
        line_cursor: 0,
        last_content_line: None,
        work_total,
    };
    let mut current = Some(NodeOrToken::Node(root_syntax.clone()));
    while let Some(element) = current.take() {
        traversal.charge(1)?;
        visited = visited.checked_add(1).ok_or_else(invariant)?;
        if visited > element_limit {
            note_plain(
                &mut build.pending.coverage,
                0,
                0,
                ParseFailureKind::PresentationCoverage,
            );
            break;
        }
        match &element {
            NodeOrToken::Node(node) => enter_node(
                node,
                text,
                &mut contexts,
                &mut build,
                leaf.eof_seen,
                &mut traversal,
            )?,
            NodeOrToken::Token(token) => handle_token(
                token,
                text,
                &mut contexts,
                &mut build.comments,
                &mut build.pending,
                &mut leaf,
                &mut traversal,
            )?,
        }
        if let NodeOrToken::Node(node) = &element
            && let Some(child) = node.first_child_or_token()
        {
            current = Some(child);
            continue;
        }
        let mut completed = element;
        loop {
            traversal.charge(1)?;
            if let NodeOrToken::Node(node) = &completed {
                exit_node(node, text, &mut contexts, &mut build, &mut traversal)?;
            }
            if let Some(sibling) = completed.next_sibling_or_token() {
                current = Some(sibling);
                break;
            }
            let Some(parent) = completed.parent() else {
                break;
            };
            completed = NodeOrToken::Node(parent);
        }
    }

    if !contexts.is_empty() && build.pending.coverage.is_none() {
        return Err(invariant());
    }
    if leaf.cursor != text.len() {
        note_plain(
            &mut build.pending.coverage,
            leaf.cursor,
            0,
            ParseFailureKind::PresentationCoverage,
        );
    }
    if let Some(failure) = selected_pending(&build.pending) {
        return Err(with_position(failure.clone(), text));
    }

    traversal.charge(text.len())?;
    let no_op_matches = root_syntax.text() == text;
    let mut work_total = traversal.work_total;

    let descriptors = metered_box(build.descriptors, &mut work_total)?;
    Ok(Presentation {
        nodes: build.nodes,
        entries: build.entries,
        comments: build.comments,
        no_op_matches,
        unsupported: false,
        descriptors,
        comment_line_starts: comment_lines.starts,
        comment_line_blank: comment_lines.blank,
        comment_region_len: u32::try_from(comment_region.len()).map_err(|_| invariant())?,
        yaml_region_offset: u32::try_from(yaml_region_offset).map_err(|_| invariant())?,
        syntax_elements: u32::try_from(visited).map_err(|_| invariant())?,
        work_total,
        #[cfg(test)]
        #[cfg(test)]
        force_work_one_over: false,
    })
}

fn enter_node(
    node: &SyntaxNode<Lang>,
    text: &str,
    contexts: &mut Vec<Frame>,
    build: &mut BuildState,
    eof_seen: bool,
    traversal: &mut Traversal<'_>,
) -> Result<(), ParseFailure> {
    let BuildState {
        nodes,
        entries,
        descriptors,
        pending,
        copies,
        ..
    } = build;
    let node_class = classify_node_kind(node.kind());
    let range = node_range(node, text)?;
    let offset = range.start().get() as usize;
    if eof_seen {
        note_plain(
            &mut pending.coverage,
            offset,
            0,
            ParseFailureKind::PresentationCoverage,
        );
    }
    if let Some(parent) = contexts.last_mut() {
        record_direct_node(parent, node.kind(), offset, &mut pending.shape)?;
    } else if node.kind() != SyntaxKind::ROOT {
        note_plain(
            &mut pending.shape,
            offset,
            0,
            ParseFailureKind::PresentationShape,
        );
    }
    match node_class {
        NodeClass::Alias => note_plain(
            &mut pending.source,
            offset,
            3,
            ParseFailureKind::AliasDenied,
        ),
        NodeClass::Shape => note_plain(
            &mut pending.shape,
            offset,
            0,
            ParseFailureKind::PresentationShape,
        ),
        NodeClass::Normal => {}
    }

    let inherited_semantic = contexts
        .last()
        .and_then(|frame| frame.semantic.or(frame.semantic_parent));
    let inherited_deepest_node = contexts.last().and_then(|frame| frame.deepest_node);
    let inherited_deepest_entry = contexts.last().and_then(|frame| frame.deepest_entry);
    let inherited_deepest_collection = contexts.last().and_then(|frame| frame.deepest_collection);
    let inherited_role = contexts
        .last()
        .map_or(NodeRole::Root, |frame| frame.payload_role);
    let mut entry_index = None;
    let mut presentation_entry = None;
    let mut deepest_entry = inherited_deepest_entry;
    if matches!(
        node.kind(),
        SyntaxKind::MAPPING_ENTRY | SyntaxKind::SEQUENCE_ENTRY
    ) {
        if let Some(parent_frame) = contexts.last_mut()
            && matches!(
                parent_frame.kind,
                SyntaxKind::MAPPING | SyntaxKind::SEQUENCE
            )
            && let Some(parent) = parent_frame.semantic
        {
            let index = parent_frame.entry_count;
            parent_frame.entry_count = parent_frame
                .entry_count
                .checked_add(1)
                .ok_or_else(invariant)?;
            entry_index = Some(index);
            if entries.len() < MAX_RICH_ENTRIES && (parent as usize) < MAX_RICH_NODES {
                let ordinal = u32::try_from(entries.len()).map_err(|_| invariant())?;
                let start_line = traversal.line(range.start())?;
                capped_push(
                    entries,
                    MAX_RICH_ENTRIES,
                    PresentationEntry {
                        parent,
                        index,
                        range,
                        sequence: node.kind() == SyntaxKind::SEQUENCE_ENTRY,
                        style: parent_frame
                            .collection_style
                            .unwrap_or(CollectionPresentationStyle::Block),
                        end_line: start_line,
                    },
                    ParseFailureKind::SemanticBuilderInvariant,
                )?;
                deepest_entry = Some(ordinal);
                presentation_entry = Some(ordinal);
            }
        } else {
            note_plain(
                &mut pending.shape,
                offset,
                0,
                ParseFailureKind::PresentationShape,
            );
        }
    }

    let collection_style = match node.kind() {
        SyntaxKind::SEQUENCE | SyntaxKind::MAPPING => Some(CollectionPresentationStyle::Block),
        _ => None,
    };
    let semantic_kind = match node.kind() {
        SyntaxKind::SCALAR if range.is_empty() => Some(ReconciledNodeKind::ImplicitEmpty),
        SyntaxKind::SCALAR => Some(ReconciledNodeKind::Scalar),
        SyntaxKind::SEQUENCE => Some(ReconciledNodeKind::Sequence),
        SyntaxKind::MAPPING => Some(ReconciledNodeKind::Mapping),
        _ => None,
    };

    let mut semantic = None;
    let role = match node.kind() {
        SyntaxKind::KEY => entry_index_from_parent(contexts)
            .map_or(inherited_role, |index| NodeRole::MappingKey { index }),
        SyntaxKind::VALUE => entry_index_from_parent(contexts)
            .map_or(inherited_role, |index| NodeRole::MappingValue { index }),
        SyntaxKind::SEQUENCE_ENTRY => {
            entry_index.map_or(inherited_role, |index| NodeRole::SequenceItem { index })
        }
        _ => inherited_role,
    };
    if let Some(kind) = semantic_kind {
        if descriptors.len() >= traversal.descriptor_limit {
            note_plain(
                &mut pending.coverage,
                offset,
                0,
                ParseFailureKind::PresentationCoverage,
            );
        } else {
            let ordinal = u32::try_from(descriptors.len()).map_err(|_| invariant())?;
            let ordinal_index = usize::try_from(ordinal).map_err(|_| invariant())?;
            semantic = Some(ordinal);
            let inherited_tag = contexts.last().and_then(|frame| frame.tag);
            let source_tag = inherited_tag.map_or(SourceTag::Implicit, |value| value.tag);
            let payload_bytes = text
                .as_bytes()
                .get(range.start().get() as usize..range.end().get() as usize)
                .ok_or_else(invariant)?;
            let scalar_style =
                (node.kind() == SyntaxKind::SCALAR).then(|| scalar_style_from_bytes(payload_bytes));
            if node.kind() == SyntaxKind::SCALAR && payload_bytes.len() > MAX_RAW_SCALAR {
                note_failure(
                    &mut pending.raw_scalar,
                    offset,
                    0,
                    ParseFailure::limited(
                        ParseFailureKind::RawScalarLimit,
                        ParseLimitScope::RawScalarBytes,
                        65_536,
                        65_537,
                    ),
                );
            }
            let (role_code, index) = role_code(role);
            capped_push(
                descriptors,
                traversal.descriptor_limit,
                TailDescriptor {
                    ordinal,
                    parent_ordinal: inherited_semantic.unwrap_or(u32::MAX),
                    index,
                    role: role_code,
                    kind: kind_code(kind),
                    guard: 0,
                    reserved: 0,
                },
                ParseFailureKind::PresentationCoverage,
            )?;

            if source_tag != SourceTag::Implicit && !tag_matches_kind(source_tag, kind) {
                note_plain(
                    &mut pending.tag_kind,
                    inherited_tag
                        .map(|value| value.tag_range.start().get() as usize)
                        .unwrap_or(offset),
                    0,
                    ParseFailureKind::ExplicitTagKindMismatch,
                );
            }

            if ordinal_index < MAX_RICH_NODES {
                let start_line = traversal.line(range.start())?;
                let rich_count = ordinal_index.checked_add(1).ok_or_else(invariant)?;
                let decoded_bound = copies
                    .decoded_base_bound
                    .checked_add(rich_count)
                    .ok_or_else(invariant)?;
                let (decoded, raw) =
                    if node.kind() == SyntaxKind::SCALAR && payload_bytes.len() <= MAX_RAW_SCALAR {
                        decode_scalar(
                            node,
                            text,
                            payload_bytes,
                            pending,
                            copies,
                            &mut traversal.work_total,
                            ScalarContext {
                                offset,
                                decoded_bound,
                            },
                        )?
                    } else {
                        (None, None)
                    };
                capped_push(
                    nodes,
                    MAX_RICH_NODES,
                    PresentationNode {
                        ordinal,
                        parent: inherited_semantic,
                        role,
                        kind,
                        whole: inherited_tag.map_or(range, |value| value.whole),
                        payload: range,
                        tag_range: inherited_tag.map(|value| value.tag_range),
                        tag: source_tag,
                        decoded,
                        raw,
                        scalar_style,
                        collection_style,
                        child_count: 0,
                        start_line,
                        end_line: start_line,
                    },
                    ParseFailureKind::SemanticBuilderInvariant,
                )?;
            }
        }
    }
    let descendant_semantic = semantic.or(inherited_semantic);
    let deepest_node = semantic.or(inherited_deepest_node);
    let deepest_collection = if matches!(
        semantic_kind,
        Some(ReconciledNodeKind::Sequence | ReconciledNodeKind::Mapping)
    ) {
        semantic
    } else {
        inherited_deepest_collection
    };
    let eof_context = matches!(node.kind(), SyntaxKind::ROOT | SyntaxKind::DOCUMENT)
        && contexts.last().is_none_or(|parent| parent.eof_context);
    capped_push(
        contexts,
        traversal.context_limit,
        Frame::new(FrameSeed {
            kind: node.kind(),
            range,
            semantic,
            semantic_parent: descendant_semantic,
            payload_role: role,
            eof_context,
            entry_index,
            collection_style,
            deepest_node,
            deepest_entry,
            deepest_collection,
            presentation_entry,
        }),
        ParseFailureKind::PresentationCoverage,
    )?;
    Ok(())
}

fn decode_scalar(
    node: &SyntaxNode<Lang>,
    text: &str,
    payload_bytes: &[u8],
    pending: &mut Pending,
    copies: &mut CopyBudget,
    work: &mut u64,
    context: ScalarContext,
) -> Result<(Option<String>, Option<String>), ParseFailure> {
    if payload_bytes.is_empty() {
        copies.decoded_total = copies.decoded_total.checked_add(1).ok_or_else(invariant)?;
        if copies.decoded_total > context.decoded_bound {
            return Err(invariant());
        }
        return Ok((Some(String::from("~")), Some(String::new())));
    }
    let Some(scalar) = Scalar::cast(node.clone()) else {
        note_plain(
            &mut pending.shape,
            context.offset,
            0,
            ParseFailureKind::PresentationShape,
        );
        return Ok((None, None));
    };
    let next_raw = copies
        .raw_total
        .checked_add(payload_bytes.len())
        .ok_or_else(invariant)?;
    if next_raw > text.len() {
        return Err(invariant());
    }
    let worst_decoded = payload_bytes
        .len()
        .checked_mul(3)
        .and_then(|value| value.checked_add(1))
        .map(|value| value / 2)
        .and_then(|value| copies.decoded_total.checked_add(value))
        .and_then(|value| value.checked_add(1))
        .ok_or_else(invariant)?;
    if worst_decoded > context.decoded_bound {
        return Err(invariant());
    }
    let raw_value = scalar.value();
    let decoded_value = scalar.as_string();
    drop(scalar);
    copies.raw_total = next_raw;
    copies.decoded_total = copies
        .decoded_total
        .checked_add(decoded_value.len())
        .ok_or_else(invariant)?;
    if copies.decoded_total > context.decoded_bound
        || !metered_equal(raw_value.as_bytes(), payload_bytes, work)?
    {
        return Err(invariant());
    }
    Ok((Some(decoded_value), Some(raw_value)))
}

fn exit_node(
    node: &SyntaxNode<Lang>,
    text: &str,
    contexts: &mut Vec<Frame>,
    build: &mut BuildState,
    traversal: &mut Traversal<'_>,
) -> Result<(), ParseFailure> {
    let Some(frame) = contexts.pop() else {
        return Err(invariant());
    };
    if frame.kind != node.kind() {
        return Err(invariant());
    }
    if let Some(entry) = frame.presentation_entry {
        let end_line = traversal.line(frame.range.end())?;
        build
            .entries
            .get_mut(usize::try_from(entry).map_err(|_| invariant())?)
            .ok_or_else(invariant)?
            .end_line = end_line;
    }
    let offset = frame.range.start().get() as usize;
    let valid = match frame.kind {
        SyntaxKind::ROOT => {
            frame.direct_nodes == 0 || frame.first_node == Some(SyntaxKind::DOCUMENT)
        }
        SyntaxKind::DOCUMENT => frame.direct_nodes == 1 && is_payload(frame.first_node),
        SyntaxKind::MAPPING => {
            (frame.direct_nodes == 0 || frame.first_node == Some(SyntaxKind::MAPPING_ENTRY))
                && delimiter_shape(&frame)
        }
        SyntaxKind::MAPPING_ENTRY => {
            let punctuation = match contexts.last().and_then(|parent| parent.collection_style) {
                Some(CollectionPresentationStyle::Flow) => frame.commas <= 1,
                Some(CollectionPresentationStyle::Block) => frame.commas == 0,
                None => false,
            };
            frame.direct_nodes == 2
                && frame.first_node == Some(SyntaxKind::KEY)
                && frame.second_node == Some(SyntaxKind::VALUE)
                && frame.colons == 1
                && frame.questions <= 1
                && punctuation
        }
        SyntaxKind::KEY | SyntaxKind::VALUE => {
            frame.direct_nodes == 1 && is_payload(frame.first_node)
        }
        SyntaxKind::SEQUENCE => {
            (frame.direct_nodes == 0 || frame.first_node == Some(SyntaxKind::SEQUENCE_ENTRY))
                && delimiter_shape(&frame)
        }
        SyntaxKind::SEQUENCE_ENTRY => {
            let punctuation = match contexts.last().and_then(|parent| parent.collection_style) {
                Some(CollectionPresentationStyle::Flow) => frame.dashes == 0 && frame.commas <= 1,
                Some(CollectionPresentationStyle::Block) => frame.dashes == 1 && frame.commas == 0,
                None => false,
            };
            frame.direct_nodes == 1 && is_payload(frame.first_node) && punctuation
        }
        SyntaxKind::TAGGED_NODE => {
            frame.direct_nodes == 1
                && matches!(
                    frame.first_node,
                    Some(SyntaxKind::SCALAR | SyntaxKind::SEQUENCE | SyntaxKind::MAPPING)
                )
                && frame.tag_tokens == 1
        }
        SyntaxKind::SCALAR if frame.range.is_empty() => {
            frame.direct_nodes == 0
                && frame.direct_tokens == 1
                && frame.zero_null_tokens == 1
                && contexts.last().is_some_and(|parent| {
                    matches!(
                        parent.kind,
                        SyntaxKind::KEY | SyntaxKind::VALUE | SyntaxKind::SEQUENCE_ENTRY
                    )
                })
        }
        SyntaxKind::SCALAR => {
            frame.direct_nodes == 0 && frame.direct_tokens > 0 && frame.zero_null_tokens == 0
        }
        SyntaxKind::ALIAS | SyntaxKind::ERROR => false,
        _ => false,
    };
    if !valid {
        note_plain(
            &mut build.pending.shape,
            offset,
            0,
            ParseFailureKind::PresentationShape,
        );
    }
    if let Some(ordinal) = frame.semantic {
        let kind = match frame.kind {
            SyntaxKind::SCALAR if frame.range.is_empty() => ReconciledNodeKind::ImplicitEmpty,
            SyntaxKind::SCALAR => ReconciledNodeKind::Scalar,
            SyntaxKind::SEQUENCE => ReconciledNodeKind::Sequence,
            SyntaxKind::MAPPING => ReconciledNodeKind::Mapping,
            _ => return Err(invariant()),
        };
        let payload = text
            .as_bytes()
            .get(frame.range.start().get() as usize..frame.range.end().get() as usize)
            .ok_or_else(invariant)?;
        let style = (frame.kind == SyntaxKind::SCALAR).then(|| scalar_style_from_bytes(payload));
        let source_tag = contexts
            .last()
            .and_then(|parent| parent.tag)
            .map_or(SourceTag::Implicit, |tag| tag.tag);
        let descriptor = build
            .descriptors
            .get_mut(ordinal as usize)
            .ok_or_else(invariant)?;
        let (guard, visits) = guard_code(source_tag, kind, frame.payload_role, style, payload);
        traversal.charge(visits)?;
        descriptor.guard = guard;
    }
    if let Some(ordinal) = frame.semantic
        && let Some(rich) = build.nodes.get_mut(ordinal as usize)
        && matches!(
            rich.kind,
            ReconciledNodeKind::Sequence | ReconciledNodeKind::Mapping
        )
    {
        rich.collection_style = frame.collection_style;
        rich.child_count = if rich.kind == ReconciledNodeKind::Mapping {
            frame.entry_count.checked_mul(2).ok_or_else(invariant)?
        } else {
            frame.entry_count
        };
    }
    if let Some(ordinal) = frame.semantic {
        let end_line = traversal.line(frame.range.end())?;
        if let Some(node) = build
            .nodes
            .get_mut(usize::try_from(ordinal).map_err(|_| invariant())?)
        {
            node.end_line = end_line;
        }
    }
    Ok(())
}

fn line_for_offset(
    starts: &[u32],
    offset: usize,
    cursor: &mut usize,
    work: &mut u64,
) -> Result<u32, ParseFailure> {
    while cursor
        .checked_add(1)
        .and_then(|next| starts.get(next).map(|start| (next, *start as usize)))
        .is_some_and(|(_, start)| start <= offset)
    {
        charge_work(work, 1)?;
        *cursor = cursor.checked_add(1).ok_or_else(invariant)?;
    }
    u32::try_from(*cursor).map_err(|_| invariant())
}

fn handle_token(
    token: &rowan::SyntaxToken<Lang>,
    text: &str,
    contexts: &mut [Frame],
    comments: &mut Vec<CommentCandidate>,
    pending: &mut Pending,
    leaf: &mut LeafState,
    traversal: &mut Traversal<'_>,
) -> Result<(), ParseFailure> {
    let token_class = classify_token_kind(token.kind());
    let range = text_range(token.text_range(), text)?;
    let start = range.start().get() as usize;
    let parent_kind = contexts
        .len()
        .checked_sub(2)
        .and_then(|index| contexts.get(index))
        .map(|frame| frame.kind);
    let Some(frame_index) = contexts.len().checked_sub(1) else {
        note_plain(
            &mut pending.shape,
            start,
            0,
            ParseFailureKind::PresentationShape,
        );
        return Ok(());
    };
    let frame = contexts.get(frame_index).ok_or_else(invariant)?;
    let exact_leaf = check_leaf_coverage(
        text,
        LeafElement {
            kind: token.kind(),
            range,
            dependency_text: token.text().as_bytes(),
        },
        LeafContext {
            frame_kind: frame.kind,
            frame_empty: frame.range.is_empty(),
            direct_nodes: frame.direct_nodes,
            direct_tokens: frame.direct_tokens,
            eof_context: frame.eof_context,
            parent_kind,
        },
        leaf,
        &mut pending.coverage,
        &mut traversal.work_total,
    )?;
    let line = traversal.line(range.start())?;
    let candidate = CommentCandidate {
        range,
        line,
        deepest_node: frame.deepest_node,
        deepest_entry: frame.deepest_entry,
        deepest_collection: frame.deepest_collection,
        inline_before: traversal.last_content_line == Some(line),
    };
    let frame = contexts.get_mut(frame_index).ok_or_else(invariant)?;
    frame.direct_tokens = frame.direct_tokens.checked_add(1).ok_or_else(invariant)?;
    if token.kind() == SyntaxKind::NULL && range.is_empty() {
        frame.zero_null_tokens = frame
            .zero_null_tokens
            .checked_add(1)
            .ok_or_else(invariant)?;
    }
    let block_scalar = frame.kind == SyntaxKind::SCALAR
        && text
            .as_bytes()
            .get(frame.range.start().get() as usize)
            .is_some_and(|byte| matches!(byte, b'|' | b'>'));
    if !block_scalar {
        record_source_token(
            token_class,
            token.kind(),
            start,
            frame.kind,
            exact_leaf,
            pending,
        );
    } else if exact_leaf && matches!(token_class, TokenClass::Structural) {
        note_plain(
            &mut pending.shape,
            start,
            0,
            ParseFailureKind::PresentationShape,
        );
    } else if exact_leaf && matches!(token_class, TokenClass::RawBom) {
        note_plain(
            &mut pending.source,
            start,
            5,
            ParseFailureKind::RawBomDenied,
        );
    }
    if token.kind() == SyntaxKind::COMMENT && exact_leaf && !range.is_empty() {
        capped_push(
            comments,
            text.len(),
            candidate,
            ParseFailureKind::PresentationCoverage,
        )?;
    }
    if exact_leaf
        && !range.is_empty()
        && !matches!(
            token.kind(),
            SyntaxKind::WHITESPACE
                | SyntaxKind::NEWLINE
                | SyntaxKind::INDENT
                | SyntaxKind::COMMENT
                | SyntaxKind::EOF
        )
    {
        traversal.note_content(range)?;
    }
    if !token_allowed(frame.kind, token.kind(), block_scalar) {
        note_plain(
            &mut pending.shape,
            start,
            0,
            ParseFailureKind::PresentationShape,
        );
    }
    match token.kind() {
        SyntaxKind::TAG => {
            frame.tag_tokens = checked_inc(frame.tag_tokens)?;
            if exact_leaf && frame.kind == SyntaxKind::TAGGED_NODE {
                let tag_bytes = text
                    .as_bytes()
                    .get(range.start().get() as usize..range.end().get() as usize)
                    .ok_or_else(invariant)?;
                traversal.charge(tag_bytes.len())?;
                if let Some(tag) = classify_source_tag(tag_bytes) {
                    if frame.tag.is_none() {
                        frame.tag = Some(TagInfo {
                            tag,
                            tag_range: range,
                            whole: frame.range,
                        });
                    }
                } else {
                    note_plain(
                        &mut pending.source,
                        start,
                        4,
                        ParseFailureKind::ExplicitTagDenied,
                    );
                }
            }
        }
        SyntaxKind::LEFT_BRACKET | SyntaxKind::LEFT_BRACE => {
            frame.left_delimiters = checked_inc(frame.left_delimiters)?;
            if frame.direct_nodes != 0 || frame.right_delimiters != 0 {
                note_plain(
                    &mut pending.shape,
                    start,
                    0,
                    ParseFailureKind::PresentationShape,
                );
            }
            frame.collection_style = Some(CollectionPresentationStyle::Flow);
        }
        SyntaxKind::RIGHT_BRACKET | SyntaxKind::RIGHT_BRACE => {
            frame.right_delimiters = checked_inc(frame.right_delimiters)?;
            if frame.left_delimiters != 1 {
                note_plain(
                    &mut pending.shape,
                    start,
                    0,
                    ParseFailureKind::PresentationShape,
                );
            }
        }
        SyntaxKind::COLON => frame.colons = checked_inc(frame.colons)?,
        SyntaxKind::QUESTION => frame.questions = checked_inc(frame.questions)?,
        SyntaxKind::DASH => frame.dashes = checked_inc(frame.dashes)?,
        SyntaxKind::COMMA => frame.commas = checked_inc(frame.commas)?,
        _ => {}
    }
    Ok(())
}

fn record_source_token(
    class: TokenClass,
    kind: SyntaxKind,
    offset: usize,
    parent: SyntaxKind,
    exact_leaf: bool,
    pending: &mut Pending,
) {
    if !exact_leaf {
        return;
    }
    match class {
        TokenClass::Directive => {
            note_plain(
                &mut pending.source,
                offset,
                0,
                ParseFailureKind::DirectiveDenied,
            );
        }
        TokenClass::DocumentMarker => {
            note_plain(
                &mut pending.source,
                offset,
                1,
                ParseFailureKind::DocumentMarkerDenied,
            );
        }
        TokenClass::Anchor => {
            note_plain(
                &mut pending.source,
                offset,
                2,
                ParseFailureKind::AnchorDenied,
            );
        }
        TokenClass::Alias => {
            note_plain(
                &mut pending.source,
                offset,
                3,
                ParseFailureKind::AliasDenied,
            );
        }
        TokenClass::Contextual => {
            if (kind == SyntaxKind::TAG && parent != SyntaxKind::TAGGED_NODE)
                || (kind == SyntaxKind::EXCLAMATION && parent != SyntaxKind::SCALAR)
            {
                note_plain(
                    &mut pending.source,
                    offset,
                    4,
                    ParseFailureKind::ExplicitTagDenied,
                );
            }
        }
        TokenClass::RawBom => note_plain(
            &mut pending.source,
            offset,
            5,
            ParseFailureKind::RawBomDenied,
        ),
        TokenClass::Unterminated => {
            note_plain(
                &mut pending.source,
                offset,
                6,
                ParseFailureKind::PresentationSyntax,
            );
        }
        TokenClass::Structural => {
            note_plain(
                &mut pending.shape,
                offset,
                0,
                ParseFailureKind::PresentationShape,
            );
        }
    }
}

fn token_allowed(parent: SyntaxKind, kind: SyntaxKind, block_scalar: bool) -> bool {
    if block_scalar {
        return !matches!(
            kind,
            SyntaxKind::ROOT
                | SyntaxKind::DOCUMENT
                | SyntaxKind::SEQUENCE
                | SyntaxKind::MAPPING
                | SyntaxKind::SCALAR
                | SyntaxKind::ALIAS
                | SyntaxKind::TAGGED_NODE
                | SyntaxKind::ERROR
                | SyntaxKind::KEY
                | SyntaxKind::VALUE
                | SyntaxKind::MAPPING_ENTRY
                | SyntaxKind::SEQUENCE_ENTRY
                | SyntaxKind::BOM
                | SyntaxKind::EOF
        );
    }
    let trivia = matches!(
        kind,
        SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::INDENT | SyntaxKind::COMMENT
    );
    match parent {
        SyntaxKind::ROOT | SyntaxKind::DOCUMENT => trivia || kind == SyntaxKind::EOF,
        SyntaxKind::MAPPING => {
            trivia || matches!(kind, SyntaxKind::LEFT_BRACE | SyntaxKind::RIGHT_BRACE)
        }
        SyntaxKind::SEQUENCE => {
            trivia || matches!(kind, SyntaxKind::LEFT_BRACKET | SyntaxKind::RIGHT_BRACKET)
        }
        SyntaxKind::MAPPING_ENTRY => {
            trivia
                || matches!(
                    kind,
                    SyntaxKind::COLON | SyntaxKind::QUESTION | SyntaxKind::COMMA
                )
        }
        SyntaxKind::SEQUENCE_ENTRY => {
            trivia || matches!(kind, SyntaxKind::DASH | SyntaxKind::COMMA)
        }
        SyntaxKind::KEY | SyntaxKind::VALUE => trivia,
        SyntaxKind::TAGGED_NODE => trivia || kind == SyntaxKind::TAG,
        SyntaxKind::SCALAR => matches!(
            kind,
            SyntaxKind::DASH
                | SyntaxKind::PLUS
                | SyntaxKind::PIPE
                | SyntaxKind::GREATER
                | SyntaxKind::QUOTE
                | SyntaxKind::SINGLE_QUOTE
                | SyntaxKind::STRING
                | SyntaxKind::INT
                | SyntaxKind::FLOAT
                | SyntaxKind::BOOL
                | SyntaxKind::NULL
                | SyntaxKind::MERGE_KEY
                | SyntaxKind::WHITESPACE
                | SyntaxKind::NEWLINE
                | SyntaxKind::INDENT
        ),
        _ => false,
    }
}

fn record_direct_node(
    parent: &mut Frame,
    child: SyntaxKind,
    offset: usize,
    pending: &mut Option<PendingFailure>,
) -> Result<(), ParseFailure> {
    parent.direct_nodes = checked_inc(parent.direct_nodes)?;
    if parent.first_node.is_none() {
        parent.first_node = Some(child);
    } else if parent.second_node.is_none() {
        parent.second_node = Some(child);
    }
    let allowed = match parent.kind {
        SyntaxKind::ROOT => child == SyntaxKind::DOCUMENT,
        SyntaxKind::DOCUMENT => is_payload(Some(child)),
        SyntaxKind::MAPPING => child == SyntaxKind::MAPPING_ENTRY,
        SyntaxKind::MAPPING_ENTRY => matches!(child, SyntaxKind::KEY | SyntaxKind::VALUE),
        SyntaxKind::KEY | SyntaxKind::VALUE | SyntaxKind::SEQUENCE_ENTRY => is_payload(Some(child)),
        SyntaxKind::SEQUENCE => child == SyntaxKind::SEQUENCE_ENTRY,
        SyntaxKind::TAGGED_NODE => matches!(
            child,
            SyntaxKind::SCALAR | SyntaxKind::SEQUENCE | SyntaxKind::MAPPING
        ),
        _ => false,
    };
    let ordered = match parent.kind {
        SyntaxKind::TAGGED_NODE => parent.tag_tokens == 1,
        SyntaxKind::MAPPING_ENTRY if child == SyntaxKind::KEY => {
            parent.direct_nodes == 1 && parent.colons == 0
        }
        SyntaxKind::MAPPING_ENTRY if child == SyntaxKind::VALUE => {
            parent.direct_nodes == 2
                && parent.first_node == Some(SyntaxKind::KEY)
                && parent.colons == 1
        }
        SyntaxKind::MAPPING | SyntaxKind::SEQUENCE => parent.right_delimiters == 0,
        _ => true,
    };
    if !allowed || !ordered {
        note_plain(pending, offset, 0, ParseFailureKind::PresentationShape);
    }
    Ok(())
}

fn entry_index_from_parent(contexts: &[Frame]) -> Option<u32> {
    contexts.last().and_then(|frame| frame.entry_index)
}

fn classify_source_tag(bytes: &[u8]) -> Option<SourceTag> {
    match bytes {
        b"!!null" => Some(SourceTag::Null),
        b"!!bool" => Some(SourceTag::Boolean),
        b"!!int" => Some(SourceTag::Integer),
        b"!!float" => Some(SourceTag::Float),
        b"!!str" => Some(SourceTag::String),
        b"!!seq" => Some(SourceTag::Sequence),
        b"!!map" => Some(SourceTag::Mapping),
        _ => None,
    }
}

fn charge_work(total: &mut u64, amount: usize) -> Result<(), ParseFailure> {
    *total = total
        .checked_add(u64::try_from(amount).map_err(|_| invariant())?)
        .ok_or_else(invariant)?;
    Ok(())
}

fn metered_box<T>(values: Vec<T>, work: &mut u64) -> Result<Box<[T]>, ParseFailure> {
    charge_work(work, values.len())?;
    Ok(values.into_boxed_slice())
}

fn metered_equal(left: &[u8], right: &[u8], work: &mut u64) -> Result<bool, ParseFailure> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left.iter().zip(right) {
        charge_work(work, 1)?;
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
}

fn scan_comment_lines(bytes: &[u8], work: &mut u64) -> Result<CommentLineTable, ParseFailure> {
    let capacity = bytes.len().checked_add(1).ok_or_else(invariant)?;
    capacity
        .checked_mul(core::mem::size_of::<u32>())
        .and_then(|value| value.checked_add(capacity))
        .ok_or_else(invariant)?;
    let mut starts = Vec::new();
    reserve(&mut starts, capacity)?;
    let mut blank = Vec::new();
    reserve(&mut blank, capacity)?;
    capped_push(
        &mut starts,
        capacity,
        0,
        ParseFailureKind::SemanticBuilderInvariant,
    )?;
    let mut line_blank = true;
    for (index, byte) in bytes.iter().copied().enumerate() {
        charge_work(work, 1)?;
        match byte {
            b'\n' => {
                capped_push(
                    &mut blank,
                    capacity,
                    u8::from(line_blank),
                    ParseFailureKind::SemanticBuilderInvariant,
                )?;
                capped_push(
                    &mut starts,
                    capacity,
                    u32::try_from(index + 1).map_err(|_| invariant())?,
                    ParseFailureKind::SemanticBuilderInvariant,
                )?;
                line_blank = true;
            }
            b'\r' | b' ' | b'\t' => {}
            _ => line_blank = false,
        }
    }
    if starts.len() > blank.len() {
        capped_push(
            &mut blank,
            capacity,
            u8::from(line_blank),
            ParseFailureKind::SemanticBuilderInvariant,
        )?;
    }
    Ok(CommentLineTable {
        starts: metered_box(starts, work)?,
        blank: metered_box(blank, work)?,
    })
}

fn delimiter_shape(frame: &Frame) -> bool {
    match frame.collection_style {
        Some(CollectionPresentationStyle::Flow) => {
            frame.left_delimiters == 1 && frame.right_delimiters == 1
        }
        Some(CollectionPresentationStyle::Block) => {
            frame.left_delimiters == 0 && frame.right_delimiters == 0
        }
        None => false,
    }
}

fn is_payload(kind: Option<SyntaxKind>) -> bool {
    matches!(
        kind,
        Some(
            SyntaxKind::SCALAR
                | SyntaxKind::SEQUENCE
                | SyntaxKind::MAPPING
                | SyntaxKind::TAGGED_NODE
        )
    )
}

fn scalar_style_from_bytes(raw: &[u8]) -> ScalarPresentationStyle {
    if raw.first() == Some(&b'\'') && raw.last() == Some(&b'\'') {
        ScalarPresentationStyle::SingleQuoted
    } else if raw.first() == Some(&b'"') && raw.last() == Some(&b'"') {
        ScalarPresentationStyle::DoubleQuoted
    } else if raw.first() == Some(&b'|') {
        ScalarPresentationStyle::Literal
    } else if raw.first() == Some(&b'>') {
        ScalarPresentationStyle::Folded
    } else {
        ScalarPresentationStyle::Plain
    }
}

fn tag_matches_kind(tag: SourceTag, kind: ReconciledNodeKind) -> bool {
    match tag {
        SourceTag::Null
        | SourceTag::Boolean
        | SourceTag::Integer
        | SourceTag::Float
        | SourceTag::String => kind == ReconciledNodeKind::Scalar,
        SourceTag::Sequence => kind == ReconciledNodeKind::Sequence,
        SourceTag::Mapping => kind == ReconciledNodeKind::Mapping,
        SourceTag::Implicit => true,
    }
}

fn guard_code(
    tag: SourceTag,
    kind: ReconciledNodeKind,
    role: NodeRole,
    style: Option<ScalarPresentationStyle>,
    raw: &[u8],
) -> (u8, usize) {
    let code = match tag {
        SourceTag::Null => 2,
        SourceTag::Boolean => 3,
        SourceTag::Integer => 4,
        SourceTag::Float => 5,
        SourceTag::String => 6,
        SourceTag::Sequence => 7,
        SourceTag::Mapping => 8,
        SourceTag::Implicit
            if kind == ReconciledNodeKind::Scalar
                && style == Some(ScalarPresentationStyle::Plain)
                && matches!(role, NodeRole::MappingKey { .. })
                && raw == b"<<" =>
        {
            1
        }
        SourceTag::Implicit => 0,
    };
    let visits = usize::from(
        tag == SourceTag::Implicit
            && kind == ReconciledNodeKind::Scalar
            && style == Some(ScalarPresentationStyle::Plain)
            && matches!(role, NodeRole::MappingKey { .. })
            && raw.len() == 2,
    ) * 2;
    (code, visits)
}

fn role_code(role: NodeRole) -> (u8, u32) {
    match role {
        NodeRole::Root => (0, 0),
        NodeRole::SequenceItem { index } => (1, index),
        NodeRole::MappingKey { index } => (2, index),
        NodeRole::MappingValue { index } => (3, index),
    }
}

fn kind_code(kind: ReconciledNodeKind) -> u8 {
    match kind {
        ReconciledNodeKind::Scalar => 0,
        ReconciledNodeKind::Sequence => 1,
        ReconciledNodeKind::Mapping => 2,
        ReconciledNodeKind::ImplicitEmpty => 3,
    }
}

fn node_range(node: &SyntaxNode<Lang>, text: &str) -> Result<YamlByteRange, ParseFailure> {
    text_range(node.text_range(), text)
}

fn text_range(range: rowan::TextRange, text: &str) -> Result<YamlByteRange, ParseFailure> {
    let start = u32::from(range.start()) as usize;
    let end = u32::from(range.end()) as usize;
    if start > end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return Err(ParseFailure::plain(ParseFailureKind::PresentationCoverage));
    }
    yaml_range(start, end)
}

fn check_leaf_coverage(
    text: &str,
    element: LeafElement<'_>,
    context: LeafContext,
    state: &mut LeafState,
    pending: &mut Option<PendingFailure>,
    work: &mut u64,
) -> Result<bool, ParseFailure> {
    let start = element.range.start().get() as usize;
    let end = element.range.end().get() as usize;
    let exact = if state.eof_seen {
        false
    } else if end > start {
        element.kind != SyntaxKind::EOF
            && start == state.cursor
            && match text.as_bytes().get(start..end) {
                Some(bytes) => metered_equal(bytes, element.dependency_text, work)?,
                None => false,
            }
    } else if element.kind == SyntaxKind::EOF {
        element.dependency_text.is_empty()
            && start == text.len()
            && start == state.cursor
            && context.eof_context
            && matches!(context.frame_kind, SyntaxKind::ROOT | SyntaxKind::DOCUMENT)
    } else {
        element.kind == SyntaxKind::NULL
            && element.dependency_text.is_empty()
            && start == state.cursor
            && context.frame_kind == SyntaxKind::SCALAR
            && context.frame_empty
            && context.direct_nodes == 0
            && context.direct_tokens == 0
            && matches!(
                context.parent_kind,
                Some(SyntaxKind::KEY | SyntaxKind::VALUE | SyntaxKind::SEQUENCE_ENTRY)
            )
    };
    if exact {
        if end > start {
            state.cursor = end;
        } else if element.kind == SyntaxKind::EOF {
            state.eof_seen = true;
        }
    } else {
        note_plain(pending, start, 0, ParseFailureKind::PresentationCoverage);
    }
    Ok(exact)
}

fn select_positioned_error(
    text: &str,
    errors: impl Iterator<Item = (usize, u32, u32, ParseErrorKind)>,
) -> Option<super::source::SourcePosition> {
    select_positioned_error_key(text, errors)
        .and_then(|value| usize::try_from(value.0).ok())
        .and_then(|offset| position(text, offset))
}

fn select_positioned_error_key(
    text: &str,
    errors: impl Iterator<Item = (usize, u32, u32, ParseErrorKind)>,
) -> Option<(u32, u32, u8, usize)> {
    let mut best: Option<(u32, u32, u8, usize)> = None;
    for (index, start, end, kind) in errors {
        let Ok(start_usize) = usize::try_from(start) else {
            continue;
        };
        let Ok(end_usize) = usize::try_from(end) else {
            continue;
        };
        let valid = start <= end
            && end_usize <= text.len()
            && text.is_char_boundary(start_usize)
            && text.is_char_boundary(end_usize);
        if valid {
            let rank = match kind {
                ParseErrorKind::UnclosedFlowSequence => 0,
                ParseErrorKind::UnclosedFlowMapping => 1,
                ParseErrorKind::UnterminatedString => 2,
                ParseErrorKind::Other => 3,
            };
            let key = (start, end, rank, index);
            if best.is_none_or(|old| key < old) {
                best = Some(key);
            }
        }
    }
    best
}

fn note_plain(
    target: &mut Option<PendingFailure>,
    offset: usize,
    rank: u8,
    kind: ParseFailureKind,
) {
    note(
        target,
        PendingFailure {
            offset,
            rank,
            failure: ParseFailure::plain(kind),
        },
    );
}

fn note_failure(
    target: &mut Option<PendingFailure>,
    offset: usize,
    rank: u8,
    failure: ParseFailure,
) {
    note(
        target,
        PendingFailure {
            offset,
            rank,
            failure,
        },
    );
}

fn note(target: &mut Option<PendingFailure>, candidate: PendingFailure) {
    if target
        .as_ref()
        .is_none_or(|current| (candidate.offset, candidate.rank) < (current.offset, current.rank))
    {
        *target = Some(candidate);
    }
}

fn selected_pending(pending: &Pending) -> Option<&PendingFailure> {
    pending
        .coverage
        .as_ref()
        .or(pending.source.as_ref())
        .or(pending.shape.as_ref())
        .or(pending.tag_kind.as_ref())
        .or(pending.raw_scalar.as_ref())
}

fn with_position(mut pending: PendingFailure, text: &str) -> ParseFailure {
    if pending.failure.limit().is_none() {
        pending.failure = ParseFailure::at(pending.failure.kind(), position(text, pending.offset));
    }
    pending.failure
}

fn checked_inc(value: u32) -> Result<u32, ParseFailure> {
    value.checked_add(1).ok_or_else(invariant)
}

fn reserve<T>(values: &mut Vec<T>, capacity: usize) -> Result<(), ParseFailure> {
    capacity
        .checked_mul(core::mem::size_of::<T>())
        .ok_or_else(invariant)?;
    values.try_reserve_exact(capacity).map_err(|_| invariant())
}

fn capped_push<T>(
    values: &mut Vec<T>,
    limit: usize,
    value: T,
    failure: ParseFailureKind,
) -> Result<(), ParseFailure> {
    if values.len() >= limit {
        return Err(ParseFailure::plain(failure));
    }
    if values.len() == values.capacity() {
        values
            .try_reserve_exact(1)
            .map_err(|_| ParseFailure::plain(failure))?;
    }
    values.push(value);
    Ok(())
}

fn invariant() -> ParseFailure {
    ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant)
}

fn classify_node_kind(kind: SyntaxKind) -> NodeClass {
    match kind {
        SyntaxKind::ROOT
        | SyntaxKind::DOCUMENT
        | SyntaxKind::SEQUENCE
        | SyntaxKind::MAPPING
        | SyntaxKind::SCALAR
        | SyntaxKind::TAGGED_NODE
        | SyntaxKind::KEY
        | SyntaxKind::VALUE
        | SyntaxKind::MAPPING_ENTRY
        | SyntaxKind::SEQUENCE_ENTRY => NodeClass::Normal,
        SyntaxKind::ALIAS => NodeClass::Alias,
        SyntaxKind::ERROR
        | SyntaxKind::DASH
        | SyntaxKind::PLUS
        | SyntaxKind::COLON
        | SyntaxKind::QUESTION
        | SyntaxKind::LEFT_BRACKET
        | SyntaxKind::RIGHT_BRACKET
        | SyntaxKind::LEFT_BRACE
        | SyntaxKind::RIGHT_BRACE
        | SyntaxKind::COMMA
        | SyntaxKind::PIPE
        | SyntaxKind::GREATER
        | SyntaxKind::AMPERSAND
        | SyntaxKind::ASTERISK
        | SyntaxKind::EXCLAMATION
        | SyntaxKind::PERCENT
        | SyntaxKind::AT
        | SyntaxKind::BACKTICK
        | SyntaxKind::QUOTE
        | SyntaxKind::SINGLE_QUOTE
        | SyntaxKind::DOC_START
        | SyntaxKind::DOC_END
        | SyntaxKind::STRING
        | SyntaxKind::UNTERMINATED_STRING
        | SyntaxKind::INT
        | SyntaxKind::FLOAT
        | SyntaxKind::BOOL
        | SyntaxKind::NULL
        | SyntaxKind::TAG
        | SyntaxKind::ANCHOR
        | SyntaxKind::REFERENCE
        | SyntaxKind::MERGE_KEY
        | SyntaxKind::DIRECTIVE
        | SyntaxKind::WHITESPACE
        | SyntaxKind::NEWLINE
        | SyntaxKind::INDENT
        | SyntaxKind::COMMENT
        | SyntaxKind::BOM
        | SyntaxKind::EOF => NodeClass::Shape,
    }
}

fn classify_token_kind(kind: SyntaxKind) -> TokenClass {
    match kind {
        SyntaxKind::ROOT
        | SyntaxKind::DOCUMENT
        | SyntaxKind::SEQUENCE
        | SyntaxKind::MAPPING
        | SyntaxKind::SCALAR
        | SyntaxKind::ALIAS
        | SyntaxKind::TAGGED_NODE
        | SyntaxKind::ERROR
        | SyntaxKind::KEY
        | SyntaxKind::VALUE
        | SyntaxKind::MAPPING_ENTRY
        | SyntaxKind::SEQUENCE_ENTRY => TokenClass::Structural,
        SyntaxKind::DIRECTIVE => TokenClass::Directive,
        SyntaxKind::DOC_START | SyntaxKind::DOC_END => TokenClass::DocumentMarker,
        SyntaxKind::AMPERSAND | SyntaxKind::ANCHOR => TokenClass::Anchor,
        SyntaxKind::ASTERISK | SyntaxKind::REFERENCE => TokenClass::Alias,
        SyntaxKind::BOM => TokenClass::RawBom,
        SyntaxKind::UNTERMINATED_STRING => TokenClass::Unterminated,
        SyntaxKind::DASH
        | SyntaxKind::PLUS
        | SyntaxKind::COLON
        | SyntaxKind::QUESTION
        | SyntaxKind::LEFT_BRACKET
        | SyntaxKind::RIGHT_BRACKET
        | SyntaxKind::LEFT_BRACE
        | SyntaxKind::RIGHT_BRACE
        | SyntaxKind::COMMA
        | SyntaxKind::PIPE
        | SyntaxKind::GREATER
        | SyntaxKind::EXCLAMATION
        | SyntaxKind::PERCENT
        | SyntaxKind::AT
        | SyntaxKind::BACKTICK
        | SyntaxKind::QUOTE
        | SyntaxKind::SINGLE_QUOTE
        | SyntaxKind::STRING
        | SyntaxKind::INT
        | SyntaxKind::FLOAT
        | SyntaxKind::BOOL
        | SyntaxKind::NULL
        | SyntaxKind::TAG
        | SyntaxKind::MERGE_KEY
        | SyntaxKind::WHITESPACE
        | SyntaxKind::NEWLINE
        | SyntaxKind::INDENT
        | SyntaxKind::COMMENT
        | SyntaxKind::EOF => TokenClass::Contextual,
    }
}

#[cfg(test)]
pub(super) fn exhaustive_kind_outcomes(kind: SyntaxKind) -> (u8, u8) {
    let node = match classify_node_kind(kind) {
        NodeClass::Normal => 0,
        NodeClass::Alias => 1,
        NodeClass::Shape => 2,
    };
    let token = match classify_token_kind(kind) {
        TokenClass::Structural => 0,
        TokenClass::Directive => 1,
        TokenClass::DocumentMarker => 2,
        TokenClass::Anchor => 3,
        TokenClass::Alias => 4,
        TokenClass::RawBom => 5,
        TokenClass::Unterminated => 6,
        TokenClass::Contextual => 7,
    };
    (node, token)
}

#[cfg(test)]
pub(super) fn positioned_error_for_test(
    text: &str,
    errors: &[(u32, u32, ParseErrorKind)],
) -> Option<(u32, u32, u8, usize)> {
    select_positioned_error_key(
        text,
        errors
            .iter()
            .enumerate()
            .map(|(index, (start, end, kind))| (index, *start, *end, kind.clone())),
    )
}

#[cfg(test)]
pub(super) fn pending_kind_for_test(
    source: Option<(usize, ParseFailureKind)>,
    shape: Option<(usize, ParseFailureKind)>,
    tag_kind: Option<(usize, ParseFailureKind)>,
    raw_scalar: Option<(usize, ParseFailureKind)>,
) -> Option<ParseFailureKind> {
    let mut pending = Pending {
        coverage: None,
        source: None,
        shape: None,
        tag_kind: None,
        raw_scalar: None,
    };
    for (target, value) in [
        (&mut pending.source, source),
        (&mut pending.shape, shape),
        (&mut pending.tag_kind, tag_kind),
        (&mut pending.raw_scalar, raw_scalar),
    ] {
        if let Some((offset, kind)) = value {
            note_plain(target, offset, 0, kind);
        }
    }
    selected_pending(&pending).map(|failure| failure.failure.kind())
}

#[cfg(test)]
pub(super) fn structural_token_vs_directive_for_test() -> ParseFailureKind {
    let mut pending = Pending {
        coverage: None,
        source: None,
        shape: None,
        tag_kind: None,
        raw_scalar: None,
    };
    record_source_token(
        TokenClass::Structural,
        SyntaxKind::ROOT,
        0,
        SyntaxKind::ROOT,
        true,
        &mut pending,
    );
    record_source_token(
        TokenClass::Directive,
        SyntaxKind::DIRECTIVE,
        10,
        SyntaxKind::ROOT,
        true,
        &mut pending,
    );
    selected_pending(&pending)
        .map(|failure| failure.failure.kind())
        .unwrap_or(ParseFailureKind::SemanticBuilderInvariant)
}

#[cfg(test)]
pub(super) fn lowest_bucket_offset_for_test(offsets: &[usize]) -> Option<usize> {
    let mut bucket = None;
    for &offset in offsets {
        note_plain(&mut bucket, offset, 0, ParseFailureKind::PresentationShape);
    }
    bucket.map(|failure| failure.offset)
}

#[cfg(test)]
pub(super) fn adjacent_pending_precedence_for_test(stage: u8) -> ParseFailureKind {
    let mut pending = Pending {
        coverage: None,
        source: None,
        shape: None,
        tag_kind: None,
        raw_scalar: None,
    };
    match stage {
        6 => {
            note_plain(
                &mut pending.coverage,
                9,
                0,
                ParseFailureKind::PresentationCoverage,
            );
            note_plain(&mut pending.source, 1, 0, ParseFailureKind::DirectiveDenied);
        }
        7 => {
            note_plain(&mut pending.source, 9, 0, ParseFailureKind::DirectiveDenied);
            note_plain(
                &mut pending.shape,
                1,
                0,
                ParseFailureKind::PresentationShape,
            );
        }
        _ => {
            note_plain(
                &mut pending.shape,
                9,
                0,
                ParseFailureKind::PresentationShape,
            );
            note_plain(
                &mut pending.tag_kind,
                1,
                0,
                ParseFailureKind::ExplicitTagKindMismatch,
            );
        }
    }
    selected_pending(&pending)
        .expect("test inserts adjacent pending failures")
        .failure
        .kind()
}

#[cfg(test)]
pub(super) type TestLeaf<'a> = (
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

#[cfg(test)]
pub(super) fn leaf_sequence_kind_for_test(
    text: &str,
    leaves: &[TestLeaf<'_>],
) -> Option<ParseFailureKind> {
    let mut state = LeafState {
        cursor: 0,
        eof_seen: false,
    };
    let mut pending = None;
    let mut work = 0u64;
    let mut direct_tokens = 0u32;
    for &(
        kind,
        start,
        end,
        dependency_text,
        frame,
        frame_empty,
        direct_nodes,
        eof_context,
        parent,
    ) in leaves
    {
        let Ok(range) = yaml_range(start, end) else {
            return Some(ParseFailureKind::PresentationCoverage);
        };
        if check_leaf_coverage(
            text,
            LeafElement {
                kind,
                range,
                dependency_text: dependency_text.as_bytes(),
            },
            LeafContext {
                frame_kind: frame,
                frame_empty,
                direct_nodes,
                direct_tokens,
                eof_context,
                parent_kind: parent,
            },
            &mut state,
            &mut pending,
            &mut work,
        )
        .is_err()
        {
            return Some(ParseFailureKind::SemanticBuilderInvariant);
        }
        direct_tokens = direct_tokens.saturating_add(1);
    }
    if state.cursor != text.len() {
        note_plain(
            &mut pending,
            state.cursor,
            0,
            ParseFailureKind::PresentationCoverage,
        );
    }
    pending.map(|failure| failure.failure.kind())
}

#[cfg(test)]
pub(super) fn token_context_allowed_for_test(
    parent: SyntaxKind,
    kind: SyntaxKind,
    block_scalar: bool,
) -> bool {
    token_allowed(parent, kind, block_scalar)
}

#[cfg(test)]
pub(super) fn capped_push_kind_for_test(limit: usize, pushes: usize) -> Option<ParseFailureKind> {
    let mut values = Vec::new();
    if reserve(&mut values, limit).is_err() {
        return Some(ParseFailureKind::SemanticBuilderInvariant);
    }
    for value in 0..pushes {
        if let Err(failure) = capped_push(
            &mut values,
            limit,
            value,
            ParseFailureKind::PresentationCoverage,
        ) {
            return Some(failure.kind());
        }
    }
    None
}

#[cfg(test)]
pub(super) fn comment_line_counts_for_test(bytes: &[u8]) -> Result<(usize, usize), ParseFailure> {
    scan_comment_lines(bytes, &mut 0).map(|table| (table.starts.len(), table.blank.len()))
}

#[cfg(test)]
pub(super) fn syntax_over_coverage_after_dependency_for_test(text: &str) -> ParseFailureKind {
    let parse = YamlFile::parse(text);
    let syntax = (!parse.positioned_errors().is_empty()).then(|| {
        ParseFailure::at(
            ParseFailureKind::PresentationSyntax,
            select_positioned_error(
                text,
                parse
                    .positioned_errors()
                    .iter()
                    .enumerate()
                    .map(|(index, error)| {
                        (
                            index,
                            error.range.start,
                            error.range.end,
                            error.kind.clone(),
                        )
                    }),
            ),
        )
    });
    let mut copied_pending = Pending {
        coverage: None,
        source: None,
        shape: None,
        tag_kind: None,
        raw_scalar: None,
    };
    note_plain(
        &mut copied_pending.coverage,
        0,
        0,
        ParseFailureKind::PresentationCoverage,
    );
    syntax
        .or_else(|| {
            selected_pending(&copied_pending)
                .map(|failure| ParseFailure::plain(failure.failure.kind()))
        })
        .expect("syntax and coverage test facts")
        .kind()
}

#[cfg(test)]
pub(super) fn shape_fault_after_dependency_for_test(text: &str) -> ParseFailure {
    let parse = YamlFile::parse(text);
    assert!(parse.positioned_errors().is_empty());
    let _tree = parse.tree();
    let mut copied_pending = Pending {
        coverage: None,
        source: None,
        shape: None,
        tag_kind: None,
        raw_scalar: None,
    };
    note_plain(
        &mut copied_pending.shape,
        0,
        0,
        ParseFailureKind::PresentationShape,
    );
    ParseFailure::plain(
        selected_pending(&copied_pending)
            .expect("shape fault inserted")
            .failure
            .kind(),
    )
}

use core::fmt;
#[cfg(test)]
use std::cell::{Cell, RefCell};

use crate::domain::UnknownMetadataDocument;

use super::presentation::{Presentation, PresentationEntry, PresentationNode};
use super::semantic::SemanticTrace;
use super::source::{
    FileByteOffset, FileByteRange, GuideEnvelope, ParseFailure, ParseFailureKind,
    WholeYamlEnvelope, YamlByteOffset, YamlByteRange, file_offset, rebase, slice,
};

#[cfg(test)]
thread_local! {
    static PREPARE_PEAK: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    static TRACE_IDENTITIES: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
    static FINISH_IDENTITIES: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    static WORK_OBSERVATION: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

macro_rules! value_debug {
    ($ty:ty, $name:literal) => {
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($name)
            }
        }
    };
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct NodeId(u32);
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct EntryId(u32);
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct CommentId(u32);
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum ReconciledNodeKind {
    Scalar,
    Sequence,
    Mapping,
    ImplicitEmpty,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum NodeRole {
    Root,
    SequenceItem { index: u32 },
    MappingKey { index: u32 },
    MappingValue { index: u32 },
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum ScalarPresentationStyle {
    Plain,
    SingleQuoted,
    DoubleQuoted,
    Literal,
    Folded,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum CollectionPresentationStyle {
    Block,
    Flow,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum SourceTag {
    Implicit,
    Null,
    Boolean,
    Integer,
    Float,
    String,
    Sequence,
    Mapping,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum ReconciledEntryKind {
    Mapping { key: NodeId, value: NodeId },
    Sequence { value: NodeId },
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum NodePresentation {
    Concrete {
        whole_yaml_range: YamlByteRange,
        whole_file_range: FileByteRange,
        payload_yaml_range: YamlByteRange,
        payload_file_range: FileByteRange,
        tag_yaml_range: Option<YamlByteRange>,
        tag_file_range: Option<FileByteRange>,
    },
    ImplicitEmpty {
        yaml_site: YamlByteOffset,
        file_site: FileByteOffset,
    },
    Unreconciled,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum CommentPlacement {
    Inline,
    Leading,
    Trailing,
    InterEntry,
    Nested,
    Flow,
    DocumentOnly,
    Unresolved,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum CommentOwner {
    Entry(EntryId),
    Node(NodeId),
    Boundary {
        parent: Option<NodeId>,
        before: Option<EntryId>,
        after: Option<EntryId>,
    },
    Ambiguous,
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum RoundTripReason {
    NodeCorrespondenceMismatch,
    ParentOrArityMismatch,
    ScalarStyleMismatch,
    DecodedScalarMismatch,
    ScalarSourceMismatch,
    InvalidCandidateRange,
    OverlappingSiblingRanges,
    NoOpRenderMismatch,
    UnsupportedPresentation,
    CommentOwnershipAmbiguous,
}
value_debug!(NodeId, "NodeId");
value_debug!(EntryId, "EntryId");
value_debug!(CommentId, "CommentId");
value_debug!(ReconciledNodeKind, "ReconciledNodeKind");
value_debug!(NodeRole, "NodeRole");
value_debug!(ScalarPresentationStyle, "ScalarPresentationStyle");
value_debug!(CollectionPresentationStyle, "CollectionPresentationStyle");
value_debug!(SourceTag, "SourceTag");
value_debug!(ReconciledEntryKind, "ReconciledEntryKind");
value_debug!(NodePresentation, "NodePresentation");
value_debug!(CommentPlacement, "CommentPlacement");
value_debug!(CommentOwner, "CommentOwner");
value_debug!(RoundTripReason, "RoundTripReason");

pub(crate) struct ReconciledNode {
    id: NodeId,
    parent: Option<NodeId>,
    role: NodeRole,
    kind: ReconciledNodeKind,
    children: Box<[NodeId]>,
    entries: Box<[EntryId]>,
    decoded_scalar: Option<Box<str>>,
    scalar_style: Option<ScalarPresentationStyle>,
    collection_style: Option<CollectionPresentationStyle>,
    source_tag: SourceTag,
    presentation: NodePresentation,
}
pub(crate) struct ReconciledEntry {
    id: EntryId,
    parent: NodeId,
    index: u32,
    yaml_range: YamlByteRange,
    file_range: FileByteRange,
    collection_style: CollectionPresentationStyle,
    kind: ReconciledEntryKind,
}
pub(crate) struct CommentRecord {
    id: CommentId,
    yaml_range: YamlByteRange,
    file_range: FileByteRange,
    placement: CommentPlacement,
    owner: CommentOwner,
}
pub(crate) struct RoundTripIssue {
    reason: RoundTripReason,
    node: Option<NodeId>,
    entry: Option<EntryId>,
    comment: Option<CommentId>,
    yaml_offset: Option<YamlByteOffset>,
}
pub(crate) struct RoundTripReport {
    issues: Box<[RoundTripIssue]>,
    total_issues: u32,
    truncated: bool,
}
pub(crate) enum RoundTripStatus {
    Proven,
    ReadOnly(RoundTripReport),
}
pub(crate) struct ParsedYamlArtifact {
    semantic: UnknownMetadataDocument,
    nodes: Box<[ReconciledNode]>,
    entries: Box<[ReconciledEntry]>,
    comments: Box<[CommentRecord]>,
    round_trip: RoundTripStatus,
}
pub(crate) struct ParsedYamlDocument {
    source: Box<[u8]>,
    envelope: WholeYamlEnvelope,
    yaml: ParsedYamlArtifact,
}
pub(crate) struct ParsedGuideSource {
    source: Box<[u8]>,
    envelope: GuideEnvelope,
    yaml: ParsedYamlArtifact,
}

impl ReconciledNode {
    pub(crate) fn id(&self) -> NodeId {
        self.id
    }
    pub(crate) fn parent(&self) -> Option<NodeId> {
        self.parent
    }
    pub(crate) fn role(&self) -> NodeRole {
        self.role
    }
    pub(crate) fn kind(&self) -> ReconciledNodeKind {
        self.kind
    }
    pub(crate) fn children(&self) -> &[NodeId] {
        &self.children
    }
    pub(crate) fn entries(&self) -> &[EntryId] {
        &self.entries
    }
    pub(crate) fn decoded_scalar(&self) -> Option<&str> {
        self.decoded_scalar.as_deref()
    }
    pub(crate) fn scalar_style(&self) -> Option<ScalarPresentationStyle> {
        self.scalar_style
    }
    pub(crate) fn collection_style(&self) -> Option<CollectionPresentationStyle> {
        self.collection_style
    }
    pub(crate) fn source_tag(&self) -> SourceTag {
        self.source_tag
    }
    pub(crate) fn presentation(&self) -> NodePresentation {
        self.presentation
    }
}
impl ReconciledEntry {
    pub(crate) fn id(&self) -> EntryId {
        self.id
    }
    pub(crate) fn parent(&self) -> NodeId {
        self.parent
    }
    pub(crate) fn index(&self) -> u32 {
        self.index
    }
    pub(crate) fn yaml_range(&self) -> YamlByteRange {
        self.yaml_range
    }
    pub(crate) fn file_range(&self) -> FileByteRange {
        self.file_range
    }
    pub(crate) fn collection_style(&self) -> CollectionPresentationStyle {
        self.collection_style
    }
    pub(crate) fn kind(&self) -> ReconciledEntryKind {
        self.kind
    }
}
impl CommentRecord {
    pub(crate) fn id(&self) -> CommentId {
        self.id
    }
    pub(crate) fn yaml_range(&self) -> YamlByteRange {
        self.yaml_range
    }
    pub(crate) fn file_range(&self) -> FileByteRange {
        self.file_range
    }
    pub(crate) fn placement(&self) -> CommentPlacement {
        self.placement
    }
    pub(crate) fn owner(&self) -> CommentOwner {
        self.owner
    }
}
impl RoundTripStatus {
    pub(crate) fn is_proven(&self) -> bool {
        matches!(self, Self::Proven)
    }
    pub(crate) fn report(&self) -> Option<&RoundTripReport> {
        if let Self::ReadOnly(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
impl RoundTripReport {
    pub(crate) fn issues(&self) -> &[RoundTripIssue] {
        &self.issues
    }
    pub(crate) fn total_issues(&self) -> u32 {
        self.total_issues
    }
    pub(crate) fn is_truncated(&self) -> bool {
        self.truncated
    }
    pub(crate) fn primary_issue(&self) -> Option<&RoundTripIssue> {
        self.issues.first()
    }
}
impl RoundTripIssue {
    pub(crate) fn reason(&self) -> RoundTripReason {
        self.reason
    }
    pub(crate) fn node(&self) -> Option<NodeId> {
        self.node
    }
    pub(crate) fn entry(&self) -> Option<EntryId> {
        self.entry
    }
    pub(crate) fn comment(&self) -> Option<CommentId> {
        self.comment
    }
    pub(crate) fn yaml_offset(&self) -> Option<YamlByteOffset> {
        self.yaml_offset
    }
}
impl ParsedYamlArtifact {
    pub(crate) fn semantic(&self) -> &UnknownMetadataDocument {
        &self.semantic
    }
    pub(crate) fn root_id(&self) -> NodeId {
        NodeId(0)
    }
    pub(crate) fn nodes(&self) -> &[ReconciledNode] {
        &self.nodes
    }
    pub(crate) fn node(&self, id: NodeId) -> Option<&ReconciledNode> {
        usize::try_from(id.0).ok().and_then(|v| self.nodes.get(v))
    }
    pub(crate) fn entries(&self) -> &[ReconciledEntry] {
        &self.entries
    }
    pub(crate) fn entry(&self, id: EntryId) -> Option<&ReconciledEntry> {
        usize::try_from(id.0).ok().and_then(|v| self.entries.get(v))
    }
    pub(crate) fn comments(&self) -> &[CommentRecord] {
        &self.comments
    }
    pub(crate) fn comment(&self, id: CommentId) -> Option<&CommentRecord> {
        usize::try_from(id.0)
            .ok()
            .and_then(|v| self.comments.get(v))
    }
    pub(crate) fn round_trip_status(&self) -> &RoundTripStatus {
        &self.round_trip
    }
}
impl ParsedYamlDocument {
    pub(super) fn new(
        source: Box<[u8]>,
        envelope: WholeYamlEnvelope,
        yaml: ParsedYamlArtifact,
    ) -> Self {
        Self {
            source,
            envelope,
            yaml,
        }
    }
    pub(crate) fn source_bytes(&self) -> &[u8] {
        &self.source
    }
    pub(crate) fn yaml_bytes(&self) -> &[u8] {
        slice(&self.source, self.envelope.yaml_file_range())
    }
    pub(crate) fn envelope(&self) -> &WholeYamlEnvelope {
        &self.envelope
    }
    pub(crate) fn yaml(&self) -> &ParsedYamlArtifact {
        &self.yaml
    }
}
impl ParsedGuideSource {
    pub(super) fn new(
        source: Box<[u8]>,
        envelope: GuideEnvelope,
        yaml: ParsedYamlArtifact,
    ) -> Self {
        Self {
            source,
            envelope,
            yaml,
        }
    }
    pub(crate) fn source_bytes(&self) -> &[u8] {
        &self.source
    }
    pub(crate) fn yaml_bytes(&self) -> &[u8] {
        slice(&self.source, self.envelope.yaml_file_range())
    }
    pub(crate) fn body_bytes(&self) -> &[u8] {
        slice(&self.source, self.envelope.body_file_range())
    }
    pub(crate) fn envelope(&self) -> &GuideEnvelope {
        &self.envelope
    }
    pub(crate) fn yaml(&self) -> &ParsedYamlArtifact {
        &self.yaml
    }
}

struct Issues {
    values: Vec<RoundTripIssue>,
    total: u32,
}

fn exact_vec<T>(capacity: usize) -> Result<Vec<T>, ParseFailure> {
    capacity
        .checked_mul(core::mem::size_of::<T>())
        .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
    Ok(values)
}

fn metered_resize<T: Clone>(
    values: &mut Vec<T>,
    new_len: usize,
    value: T,
    work: &mut ReconciliationWork,
) -> Result<(), ParseFailure> {
    let added = new_len.checked_sub(values.len()).ok_or_else(invariant)?;
    work.charge(added)?;
    values.resize(new_len, value);
    Ok(())
}

fn metered_box<T>(values: Vec<T>, work: &mut ReconciliationWork) -> Result<Box<[T]>, ParseFailure> {
    work.charge(values.len())?;
    Ok(values.into_boxed_slice())
}

impl Issues {
    fn new() -> Result<Self, ParseFailure> {
        Ok(Self {
            values: exact_vec(32)?,
            total: 0,
        })
    }
    fn add(
        &mut self,
        reason: RoundTripReason,
        node: Option<NodeId>,
        entry: Option<EntryId>,
        comment: Option<CommentId>,
        offset: Option<YamlByteOffset>,
    ) -> Result<(), ParseFailure> {
        self.total = self
            .total
            .checked_add(1)
            .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
        if self.values.len() < 32 {
            self.values.push(RoundTripIssue {
                reason,
                node,
                entry,
                comment,
                yaml_offset: offset,
            });
        }
        Ok(())
    }
    fn finish(self, work: &mut ReconciliationWork) -> Result<RoundTripStatus, ParseFailure> {
        if self.total == 0 {
            Ok(RoundTripStatus::Proven)
        } else {
            let truncated = self.total as usize > self.values.len();
            Ok(RoundTripStatus::ReadOnly(RoundTripReport {
                issues: metered_box(self.values, work)?,
                total_issues: self.total,
                truncated,
            }))
        }
    }
}

struct DraftNode {
    ordinal: u32,
    parent: Option<u32>,
    role: NodeRole,
    kind: ReconciledNodeKind,
    children: Box<[NodeId]>,
    entries: Box<[EntryId]>,
    scalar_style: Option<ScalarPresentationStyle>,
    collection_style: Option<CollectionPresentationStyle>,
    source_tag: SourceTag,
    presentation: NodePresentation,
}

struct ReconciliationDraft {
    nodes: Vec<DraftNode>,
    entries: Vec<ReconciledEntry>,
    comments: Vec<CommentRecord>,
    issues: Issues,
    work: ReconciliationWork,
}

pub(super) struct FoldPermit {
    sealed: (),
}

pub(super) fn reconcile(
    mut semantic: SemanticTrace,
    presentation: Presentation,
    yaml: &[u8],
    file_base: FileByteOffset,
) -> Result<ParsedYamlArtifact, ParseFailure> {
    #[cfg(test)]
    {
        let presentation_decoded = presentation
            .nodes
            .iter()
            .try_fold(0usize, |total, node| {
                total.checked_add(node.decoded.as_ref().map_or(0, String::len))
            })
            .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
        let presentation_raw = presentation
            .nodes
            .iter()
            .try_fold(0usize, |total, node| {
                total.checked_add(node.raw.as_ref().map_or(0, String::len))
            })
            .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
        let semantic_decoded = semantic
            .nodes
            .iter()
            .try_fold(0usize, |total, node| {
                total.checked_add(node.decoded.as_ref().map_or(0, |value| value.len()))
            })
            .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
        let decoded = presentation_decoded
            .checked_add(semantic_decoded)
            .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
        let all = decoded
            .checked_add(presentation_raw)
            .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
        PREPARE_PEAK.with(|peak| peak.set((decoded, all)));
    }
    #[cfg(test)]
    TRACE_IDENTITIES.with(|identities| {
        let mut identities = identities.borrow_mut();
        identities.clear();
        identities.extend(semantic.nodes.iter().filter_map(|node| {
            node.decoded
                .as_ref()
                .map(|value| (value.as_ptr() as usize, value.len()))
        }));
    });
    let (draft, permit) = prepare(&semantic, presentation, yaml, file_base)?;
    let document = semantic.fold(permit)?;
    finish(semantic, draft, document)
}

#[cfg(test)]
pub(super) fn reset_pipeline_observation_for_test() {
    PREPARE_PEAK.with(|peak| peak.set((0, 0)));
    TRACE_IDENTITIES.with(|identities| identities.borrow_mut().clear());
    FINISH_IDENTITIES.with(|identities| identities.set((0, 0)));
    WORK_OBSERVATION.with(|observation| observation.set((0, 0)));
    super::semantic::reset_fold_observation_for_test();
}

#[cfg(test)]
pub(super) fn reconciliation_work_for_test() -> (u64, u64) {
    WORK_OBSERVATION.with(Cell::get)
}

#[cfg(test)]
pub(super) fn entry_ordinal_for_test(id: EntryId) -> u32 {
    id.0
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) enum ReconcileFault {
    ScalarStyle {
        node: usize,
    },
    Decoded {
        node: usize,
    },
    Raw {
        node: usize,
    },
    UnsupportedPresentation,
    NoOpRender,
    Parent {
        node: usize,
    },
    Kind {
        node: usize,
    },
    Arity {
        node: usize,
    },
    WholeOutside {
        node: usize,
    },
    PayloadOutside {
        node: usize,
    },
    Utf8Boundary {
        node: usize,
        offset: usize,
    },
    TagOutside {
        node: usize,
    },
    TagPayloadOverlap {
        node: usize,
    },
    AncestorEscape {
        node: usize,
    },
    SiblingOverlap {
        node: usize,
    },
    NodeReverse,
    EntryOutside {
        entry: usize,
    },
    EntryOverlap {
        entry: usize,
    },
    EntryReverse,
    CommentCross,
    CommentReverse,
    CommentDroppedEntry,
    CommentConflictingEntry,
    CommentUnreconciledNode,
    CommentScalarNested {
        node: usize,
    },
    CommentScalarOverlap {
        node: usize,
    },
    AllCommentsUnreconciled,
    RebaseOverflow,
    WorkOneOver,
    OrderedNodeDimensions {
        node: usize,
        first: NodeMismatchDimension,
    },
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) enum NodeMismatchDimension {
    Parent,
    Kind,
    Arity,
    Tag,
    Style,
    Decoded,
    Raw,
    Range,
}

#[cfg(test)]
pub(super) fn reconcile_fault_for_test(
    yaml: &[u8],
    fault: ReconcileFault,
) -> Result<ParsedYamlArtifact, ParseFailure> {
    let text = core::str::from_utf8(yaml).map_err(|_| invariant())?;
    let mut presentation = super::presentation::inspect(text, yaml, 0)?;
    let semantic = super::semantic::parse(text, &presentation)?;
    let mut file_base = file_offset(0);
    match fault {
        ReconcileFault::ScalarStyle { node } => {
            presentation
                .nodes
                .get_mut(node)
                .ok_or_else(invariant)?
                .scalar_style = Some(ScalarPresentationStyle::Folded);
        }
        ReconcileFault::Decoded { node } => {
            presentation
                .nodes
                .get_mut(node)
                .ok_or_else(invariant)?
                .decoded = Some(String::from("different"));
        }
        ReconcileFault::Raw { node } => {
            presentation.nodes.get_mut(node).ok_or_else(invariant)?.raw =
                Some(String::from("different"));
        }
        ReconcileFault::UnsupportedPresentation => presentation.unsupported = true,
        ReconcileFault::NoOpRender => presentation.no_op_matches = false,
        ReconcileFault::Parent { node } => {
            presentation
                .nodes
                .get_mut(node)
                .ok_or_else(invariant)?
                .parent = None;
        }
        ReconcileFault::Kind { node } => {
            let value = presentation.nodes.get_mut(node).ok_or_else(invariant)?;
            value.kind = if value.kind == ReconciledNodeKind::Scalar {
                ReconciledNodeKind::Sequence
            } else {
                ReconciledNodeKind::Scalar
            };
        }
        ReconcileFault::Arity { node } => {
            let value = presentation.nodes.get_mut(node).ok_or_else(invariant)?;
            value.child_count = value.child_count.checked_add(1).ok_or_else(invariant)?;
        }
        ReconcileFault::WholeOutside { node } => {
            presentation
                .nodes
                .get_mut(node)
                .ok_or_else(invariant)?
                .whole =
                super::source::yaml_range(0, yaml.len().checked_add(1).ok_or_else(invariant)?)?;
        }
        ReconcileFault::PayloadOutside { node } => {
            presentation
                .nodes
                .get_mut(node)
                .ok_or_else(invariant)?
                .payload = super::source::yaml_range(yaml.len(), yaml.len())?;
        }
        ReconcileFault::Utf8Boundary { node, offset } => {
            let value = presentation.nodes.get_mut(node).ok_or_else(invariant)?;
            value.payload = super::source::yaml_range(offset, value.payload.end().get() as usize)?;
        }
        ReconcileFault::TagOutside { node } => {
            presentation
                .nodes
                .get_mut(node)
                .ok_or_else(invariant)?
                .tag_range = Some(super::source::yaml_range(
                yaml.len(),
                yaml.len().checked_add(1).ok_or_else(invariant)?,
            )?);
        }
        ReconcileFault::TagPayloadOverlap { node } => {
            let value = presentation.nodes.get_mut(node).ok_or_else(invariant)?;
            value.tag_range = Some(value.payload);
        }
        ReconcileFault::AncestorEscape { node } => {
            let value = presentation.nodes.get_mut(node).ok_or_else(invariant)?;
            value.whole = super::source::yaml_range(0, value.whole.end().get() as usize)?;
        }
        ReconcileFault::SiblingOverlap { node } => {
            let start = presentation
                .nodes
                .get(node.checked_sub(1).ok_or_else(invariant)?)
                .ok_or_else(invariant)?
                .whole
                .start()
                .get() as usize;
            let end = presentation
                .nodes
                .get(node)
                .ok_or_else(invariant)?
                .whole
                .end()
                .get() as usize;
            presentation
                .nodes
                .get_mut(node)
                .ok_or_else(invariant)?
                .whole = super::source::yaml_range(start, end)?;
        }
        ReconcileFault::NodeReverse => {
            let first = presentation.nodes.get(2).ok_or_else(invariant)?.whole;
            let second = presentation.nodes.get(4).ok_or_else(invariant)?.whole;
            presentation.nodes.get_mut(2).ok_or_else(invariant)?.whole = second;
            presentation.nodes.get_mut(4).ok_or_else(invariant)?.whole = first;
        }
        ReconcileFault::EntryOutside { entry } => {
            presentation
                .entries
                .get_mut(entry)
                .ok_or_else(invariant)?
                .range =
                super::source::yaml_range(0, yaml.len().checked_add(1).ok_or_else(invariant)?)?;
        }
        ReconcileFault::EntryOverlap { entry } => {
            let start = presentation
                .entries
                .get(entry.checked_sub(1).ok_or_else(invariant)?)
                .ok_or_else(invariant)?
                .range
                .start()
                .get() as usize;
            let end = presentation
                .entries
                .get(entry)
                .ok_or_else(invariant)?
                .range
                .end()
                .get() as usize;
            presentation
                .entries
                .get_mut(entry)
                .ok_or_else(invariant)?
                .range = super::source::yaml_range(start, end)?;
        }
        ReconcileFault::EntryReverse => {
            let first = presentation.entries.first().ok_or_else(invariant)?.range;
            let second = presentation.entries.get(1).ok_or_else(invariant)?.range;
            presentation
                .entries
                .first_mut()
                .ok_or_else(invariant)?
                .range = second;
            presentation.entries.get_mut(1).ok_or_else(invariant)?.range = first;
        }
        ReconcileFault::CommentCross => {
            let first = presentation.comments.first().ok_or_else(invariant)?.range;
            presentation
                .comments
                .get_mut(1)
                .ok_or_else(invariant)?
                .range = first;
        }
        ReconcileFault::CommentReverse => {
            let first = presentation.comments.first().ok_or_else(invariant)?.range;
            let second = presentation.comments.get(1).ok_or_else(invariant)?.range;
            presentation
                .comments
                .first_mut()
                .ok_or_else(invariant)?
                .range = second;
            presentation
                .comments
                .get_mut(1)
                .ok_or_else(invariant)?
                .range = first;
        }
        ReconcileFault::CommentDroppedEntry => {
            presentation
                .comments
                .first_mut()
                .ok_or_else(invariant)?
                .deepest_entry = Some(u32::MAX);
        }
        ReconcileFault::CommentConflictingEntry => {
            presentation
                .comments
                .first_mut()
                .ok_or_else(invariant)?
                .deepest_entry = Some(1);
        }
        ReconcileFault::CommentUnreconciledNode => {
            presentation
                .comments
                .first_mut()
                .ok_or_else(invariant)?
                .deepest_node = Some(u32::MAX);
        }
        ReconcileFault::CommentScalarNested { node } => {
            let comment = presentation.comments.first().ok_or_else(invariant)?.range;
            let value = presentation.nodes.get_mut(node).ok_or_else(invariant)?;
            value.whole = super::source::yaml_range(
                value.whole.start().get() as usize,
                comment.end().get() as usize,
            )?;
            presentation
                .comments
                .first_mut()
                .ok_or_else(invariant)?
                .deepest_node = Some(u32::try_from(node).map_err(|_| invariant())?);
        }
        ReconcileFault::CommentScalarOverlap { node } => {
            let comment = presentation.comments.first().ok_or_else(invariant)?.range;
            let value = presentation.nodes.get_mut(node).ok_or_else(invariant)?;
            value.whole = super::source::yaml_range(
                value.whole.start().get() as usize,
                comment.end().get() as usize,
            )?;
        }
        ReconcileFault::AllCommentsUnreconciled => {
            for comment in &mut presentation.comments {
                comment.deepest_node = Some(u32::MAX);
            }
        }
        ReconcileFault::RebaseOverflow => file_base = file_offset(u32::MAX),
        ReconcileFault::WorkOneOver => presentation.force_work_one_over = true,
        ReconcileFault::OrderedNodeDimensions { node, first } => {
            let value = presentation.nodes.get_mut(node).ok_or_else(invariant)?;
            let rank = first as u8;
            if rank <= NodeMismatchDimension::Parent as u8 {
                value.parent = None;
            }
            if rank <= NodeMismatchDimension::Kind as u8 {
                value.kind = ReconciledNodeKind::Sequence;
            }
            if rank <= NodeMismatchDimension::Arity as u8 {
                value.child_count = value.child_count.checked_add(1).ok_or_else(invariant)?;
            }
            if rank <= NodeMismatchDimension::Tag as u8 {
                value.tag = SourceTag::String;
            }
            if rank <= NodeMismatchDimension::Style as u8 {
                value.scalar_style = Some(ScalarPresentationStyle::DoubleQuoted);
            }
            if rank <= NodeMismatchDimension::Decoded as u8 {
                value.decoded = Some(String::from("changed"));
            }
            if rank <= NodeMismatchDimension::Raw as u8 {
                value.raw = Some(String::from("changed"));
            }
            if rank <= NodeMismatchDimension::Range as u8 {
                value.whole =
                    super::source::yaml_range(0, yaml.len().checked_add(1).ok_or_else(invariant)?)?;
            }
        }
    }
    reconcile(semantic, presentation, yaml, file_base)
}

#[cfg(test)]
pub(super) fn pipeline_observation_for_test() -> (usize, usize, bool, usize, usize, usize, usize) {
    let (prepare_decoded, prepare_all) = PREPARE_PEAK.with(Cell::get);
    let (barrier, copies, fold_peak) = super::semantic::fold_observation_for_test();
    let (identity_count, identity_matches) = FINISH_IDENTITIES.with(Cell::get);
    (
        prepare_decoded,
        prepare_all,
        barrier,
        copies,
        fold_peak,
        identity_count,
        identity_matches,
    )
}

#[cfg(test)]
pub(super) fn draft_allocation_overflow_for_test() -> ParseFailureKind {
    match exact_vec::<DraftNode>(usize::MAX) {
        Err(failure) => failure.kind(),
        Ok(_) => panic!("impossible draft allocation unexpectedly succeeded"),
    }
}

#[cfg(test)]
pub(super) fn issue_total_overflow_for_test() -> ParseFailureKind {
    let mut issues = Issues {
        values: exact_vec(32).expect("fixed issue capacity must allocate in test"),
        total: u32::MAX,
    };
    issues
        .add(
            RoundTripReason::InvalidCandidateRange,
            None,
            None,
            None,
            None,
        )
        .expect_err("issue total overflow must fail")
        .kind()
}

fn prepare(
    semantic: &SemanticTrace,
    presentation: Presentation,
    yaml: &[u8],
    file_base: FileByteOffset,
) -> Result<(ReconciliationDraft, FoldPermit), ParseFailure> {
    let draft = prepare_borrowed(semantic, &presentation, yaml, file_base)?;
    drop(presentation);
    Ok((draft, FoldPermit { sealed: () }))
}

fn prepare_borrowed(
    semantic: &SemanticTrace,
    presentation: &Presentation,
    yaml: &[u8],
    file_base: FileByteOffset,
) -> Result<ReconciliationDraft, ParseFailure> {
    let mut work = ReconciliationWork::new(presentation, semantic, yaml)?;
    let mut issues = Issues::new()?;
    let node_count = semantic.nodes.len();
    let zip_len = node_count.min(presentation.nodes.len());
    let mut matched = zip_len;
    let mut structural_break = false;
    let mut node_presentations = exact_vec(node_count)?;
    let mut node_proven = exact_vec(node_count)?;
    let mut collection_styles = exact_vec(node_count)?;
    let mut last_child_end = exact_vec(node_count)?;
    for _ in 0..node_count {
        work.node()?;
        node_presentations.push(NodePresentation::Unreconciled);
        node_proven.push(false);
        collection_styles.push(None);
        last_child_end.push(None::<YamlByteRange>);
    }

    for index in 0..zip_len {
        work.node()?;
        let s = semantic.nodes.get(index).ok_or_else(invariant)?;
        let p = presentation.nodes.get(index).ok_or_else(invariant)?;
        let id = node_id(index)?;
        let offset = safe_offset(p.payload, yaml);
        if s.ordinal != u32::try_from(index).map_err(|_| invariant())?
            || p.ordinal != s.ordinal
            || s.parent != p.parent
            || s.role != p.role
        {
            issues.add(
                RoundTripReason::ParentOrArityMismatch,
                Some(id),
                None,
                None,
                offset,
            )?;
            matched = index;
            structural_break = true;
            break;
        }
        if s.kind != p.kind {
            issues.add(
                RoundTripReason::NodeCorrespondenceMismatch,
                Some(id),
                None,
                None,
                offset,
            )?;
            matched = index;
            structural_break = true;
            break;
        }
        if s.child_count != p.child_count {
            issues.add(
                RoundTripReason::ParentOrArityMismatch,
                Some(id),
                None,
                None,
                offset,
            )?;
            matched = index;
            structural_break = true;
            break;
        }
        if s.source_tag != p.tag {
            issues.add(
                RoundTripReason::NodeCorrespondenceMismatch,
                Some(id),
                None,
                None,
                offset,
            )?;
        }
        if s.scalar_style != p.scalar_style {
            issues.add(
                RoundTripReason::ScalarStyleMismatch,
                Some(id),
                None,
                None,
                offset,
            )?;
        }
        let decoded_matches = match (s.decoded.as_deref(), p.decoded.as_deref()) {
            (Some(left), Some(right)) => work.bytes_equal(left.as_bytes(), right.as_bytes())?,
            (None, None) => true,
            _ => false,
        };
        if !decoded_matches {
            issues.add(
                RoundTripReason::DecodedScalarMismatch,
                Some(id),
                None,
                None,
                offset,
            )?;
        }
        if let Some(raw) = &p.raw {
            let raw_matches = match checked_yaml_slice(yaml, p.payload) {
                Some(source) => work.bytes_equal(source, raw.as_bytes())?,
                None => false,
            };
            if !raw_matches {
                issues.add(
                    RoundTripReason::ScalarSourceMismatch,
                    Some(id),
                    None,
                    None,
                    offset,
                )?;
            }
        }

        let (node_presentation, valid) = prove_node_range(
            index,
            s.kind,
            p,
            &mut RangeProof {
                yaml,
                file_base,
                prior: &node_presentations,
                last_child_end: &mut last_child_end,
                issues: &mut issues,
            },
        )?;
        *node_presentations.get_mut(index).ok_or_else(invariant)? = node_presentation;
        *node_proven.get_mut(index).ok_or_else(invariant)? = valid;
        *collection_styles.get_mut(index).ok_or_else(invariant)? = p.collection_style;
    }
    if !structural_break && node_count != presentation.nodes.len() {
        let index = zip_len;
        issues.add(
            RoundTripReason::NodeCorrespondenceMismatch,
            (index < node_count)
                .then(|| u32::try_from(index).ok().map(NodeId))
                .flatten(),
            None,
            None,
            presentation
                .nodes
                .get(index)
                .and_then(|node| safe_offset(node.payload, yaml)),
        )?;
        matched = index;
    }

    let mut child_lists = exact_vec(node_count)?;
    let mut entry_lists = exact_vec(node_count)?;
    for node in &semantic.nodes {
        work.node()?;
        let child_capacity = usize::try_from(node.child_count).map_err(|_| invariant())?;
        child_lists.push(exact_vec(child_capacity)?);
        entry_lists.push(exact_vec(child_capacity)?);
    }
    for s in semantic.nodes.iter().skip(1) {
        work.node()?;
        let parent = s
            .parent
            .and_then(|value| usize::try_from(value).ok())
            .and_then(|value| child_lists.get_mut(value))
            .ok_or_else(invariant)?;
        parent.push(NodeId(s.ordinal));
    }

    let (entry_slots, mut entry_refs) = entry_direct_tables(semantic, presentation, &mut work)?;
    for s in semantic.nodes.iter().skip(1) {
        work.node()?;
        let Some(parent) = s.parent.and_then(|value| usize::try_from(value).ok()) else {
            return Err(invariant());
        };
        let Some(slot) = entry_slot_for_role(parent, s.role, &entry_slots) else {
            continue;
        };
        let reference = entry_refs.get_mut(slot).ok_or_else(invariant)?;
        match s.role {
            NodeRole::SequenceItem { .. } | NodeRole::MappingValue { .. } => {
                reference.value = Some(NodeId(s.ordinal));
            }
            NodeRole::MappingKey { .. } => reference.key = Some(NodeId(s.ordinal)),
            NodeRole::Root => return Err(invariant()),
        }
    }

    let mut entries = exact_vec(presentation.entries.len())?;
    let mut entry_lines = exact_vec(presentation.entries.len())?;
    let mut last_entry_end = exact_vec(node_count)?;
    for _ in 0..node_count {
        work.node()?;
        last_entry_end.push(None::<YamlByteRange>);
    }
    for (ordinal, pentry) in presentation.entries.iter().enumerate() {
        work.entry()?;
        let Some(reference) = entry_refs.get(ordinal).copied() else {
            return Err(invariant());
        };
        let parent_index = usize::try_from(pentry.parent).map_err(|_| invariant())?;
        let reason = validate_entry(
            pentry,
            entry_slots.local.get(ordinal).copied(),
            reference,
            &mut EntryValidationContext {
                matched,
                node_proven: &node_proven,
                semantic,
                node_presentations: &node_presentations,
                collection_styles: &collection_styles,
                yaml,
                last_end: &mut last_entry_end,
            },
        );
        let Some(kind) = reason.kind else {
            issues.add(
                reason.issue,
                None,
                None,
                None,
                reason.offset.or_else(|| safe_offset(pentry.range, yaml)),
            )?;
            break;
        };
        let Some(file_range) = rebase(pentry.range, file_base) else {
            issues.add(
                RoundTripReason::InvalidCandidateRange,
                None,
                None,
                None,
                safe_offset(pentry.range, yaml),
            )?;
            break;
        };
        let expected_id = entry_id(entries.len())?;
        let entry = ReconciledEntry {
            id: expected_id,
            parent: NodeId(pentry.parent),
            index: pentry.index,
            yaml_range: pentry.range,
            file_range,
            collection_style: pentry.style,
            kind,
        };
        let payload_lines = entry_payload_lines(kind, &presentation.nodes)?;
        entry_lists
            .get_mut(parent_index)
            .ok_or_else(invariant)?
            .push(expected_id);
        entries.push(entry);
        entry_lines.push(payload_lines);
    }

    let comments = classify_comments(
        presentation,
        &mut CommentContext {
            yaml,
            file_base,
            matched,
            semantic,
            node_presentations: &node_presentations,
            collection_styles: &collection_styles,
            entries: &entries,
            entry_lines: &entry_lines,
            issues: &mut issues,
            work: &mut work,
        },
    )?;

    let mut nodes = exact_vec(node_count)?;
    for (index, s) in semantic.nodes.iter().enumerate() {
        work.node()?;
        let children = metered_box(
            core::mem::take(child_lists.get_mut(index).ok_or_else(invariant)?),
            &mut work,
        )?;
        let node_entries = metered_box(
            core::mem::take(entry_lists.get_mut(index).ok_or_else(invariant)?),
            &mut work,
        )?;
        let suffix = index >= matched;
        nodes.push(DraftNode {
            ordinal: s.ordinal,
            parent: s.parent,
            role: s.role,
            kind: s.kind,
            children,
            entries: node_entries,
            scalar_style: s.scalar_style,
            collection_style: if suffix {
                None
            } else {
                *collection_styles.get(index).ok_or_else(invariant)?
            },
            source_tag: s.source_tag,
            presentation: if suffix {
                NodePresentation::Unreconciled
            } else {
                *node_presentations.get(index).ok_or_else(invariant)?
            },
        });
    }
    if !presentation.no_op_matches {
        issues.add(RoundTripReason::NoOpRenderMismatch, None, None, None, None)?;
    }
    if presentation.unsupported {
        issues.add(
            RoundTripReason::UnsupportedPresentation,
            None,
            None,
            None,
            None,
        )?;
    }
    Ok(ReconciliationDraft {
        nodes,
        entries,
        comments,
        issues,
        work,
    })
}

fn invariant() -> ParseFailure {
    ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant)
}

struct ReconciliationWork {
    total: u64,
    source: u32,
    syntax: u32,
    #[cfg(test)]
    force_one_over: bool,
}

impl ReconciliationWork {
    fn new(
        presentation: &Presentation,
        semantic: &SemanticTrace,
        yaml: &[u8],
    ) -> Result<Self, ParseFailure> {
        if usize::try_from(presentation.comment_region_len).map_err(|_| invariant())? < yaml.len()
            || semantic.nodes.len() > 20_000
        {
            return Err(invariant());
        }
        let source = presentation.comment_region_len;
        let syntax = presentation.syntax_elements;
        Ok(Self {
            total: presentation.work_total,
            source,
            syntax,
            #[cfg(test)]
            force_one_over: presentation.force_work_one_over,
        })
    }

    fn charge(&mut self, amount: usize) -> Result<(), ParseFailure> {
        self.total = self
            .total
            .checked_add(u64::try_from(amount).map_err(|_| invariant())?)
            .ok_or_else(invariant)?;
        Ok(())
    }

    fn node(&mut self) -> Result<(), ParseFailure> {
        self.charge(1)
    }

    fn entry(&mut self) -> Result<(), ParseFailure> {
        self.charge(1)
    }

    fn comment(&mut self) -> Result<(), ParseFailure> {
        self.charge(1)
    }

    fn bytes_equal(&mut self, left: &[u8], right: &[u8]) -> Result<bool, ParseFailure> {
        if left.len() != right.len() {
            return Ok(false);
        }
        for (left, right) in left.iter().zip(right) {
            self.charge(1)?;
            if left != right {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn metered_copy<T: Copy>(&mut self, values: &[T]) -> Result<Vec<T>, ParseFailure> {
        let mut copy = exact_vec(values.len())?;
        self.charge(values.len())?;
        copy.extend_from_slice(values);
        Ok(copy)
    }

    fn bound(&self, nodes: usize, entries: usize, comments: usize) -> Result<u64, ParseFailure> {
        u64::from(self.source)
            .checked_mul(12)
            .and_then(|value| {
                u64::from(self.syntax)
                    .checked_mul(8)
                    .and_then(|v| value.checked_add(v))
            })
            .and_then(|value| {
                u64::try_from(nodes)
                    .ok()?
                    .checked_mul(12)
                    .and_then(|v| value.checked_add(v))
            })
            .and_then(|value| {
                u64::try_from(entries)
                    .ok()?
                    .checked_mul(8)
                    .and_then(|v| value.checked_add(v))
            })
            .and_then(|value| {
                u64::try_from(comments)
                    .ok()?
                    .checked_mul(8)
                    .and_then(|v| value.checked_add(v))
            })
            .and_then(|value| value.checked_add(64))
            .ok_or_else(invariant)
    }

    #[cfg(test)]
    fn seed_one_over_before_final_visit(
        &mut self,
        nodes: usize,
        entries: usize,
        comments: usize,
    ) -> Result<(), ParseFailure> {
        if self.force_one_over {
            self.total = self.bound(nodes, entries, comments)?;
        }
        Ok(())
    }

    fn finish(&self, nodes: usize, entries: usize, comments: usize) -> Result<(), ParseFailure> {
        let bound = self.bound(nodes, entries, comments)?;
        if self.total > bound {
            return Err(invariant());
        }
        #[cfg(test)]
        WORK_OBSERVATION.with(|observation| observation.set((self.total, bound)));
        Ok(())
    }
}

fn node_id(index: usize) -> Result<NodeId, ParseFailure> {
    Ok(NodeId(u32::try_from(index).map_err(|_| invariant())?))
}

fn entry_id(index: usize) -> Result<EntryId, ParseFailure> {
    Ok(EntryId(u32::try_from(index).map_err(|_| invariant())?))
}

fn safe_offset(range: YamlByteRange, yaml: &[u8]) -> Option<YamlByteOffset> {
    let start = usize::try_from(range.start().get()).ok()?;
    utf8_boundary(yaml, start).then_some(range.start())
}

fn utf8_boundary(yaml: &[u8], offset: usize) -> bool {
    offset <= yaml.len()
        && (offset == yaml.len() || yaml.get(offset).is_some_and(|byte| byte & 0xc0 != 0x80))
}

fn valid_range(range: YamlByteRange, yaml: &[u8]) -> bool {
    let start = range.start().get() as usize;
    let end = range.end().get() as usize;
    start <= end && end <= yaml.len() && utf8_boundary(yaml, start) && utf8_boundary(yaml, end)
}

fn contains(outer: YamlByteRange, inner: YamlByteRange) -> bool {
    outer.start() <= inner.start() && inner.end() <= outer.end()
}

fn checked_yaml_slice(yaml: &[u8], range: YamlByteRange) -> Option<&[u8]> {
    valid_range(range, yaml)
        .then(|| yaml.get(range.start().get() as usize..range.end().get() as usize))?
}

fn presentation_range(value: NodePresentation) -> Option<YamlByteRange> {
    match value {
        NodePresentation::Concrete {
            whole_yaml_range, ..
        } => Some(whole_yaml_range),
        NodePresentation::ImplicitEmpty { yaml_site, .. } => {
            super::source::yaml_range(yaml_site.get() as usize, yaml_site.get() as usize).ok()
        }
        NodePresentation::Unreconciled => None,
    }
}

struct RangeProof<'a> {
    yaml: &'a [u8],
    file_base: FileByteOffset,
    prior: &'a [NodePresentation],
    last_child_end: &'a mut [Option<YamlByteRange>],
    issues: &'a mut Issues,
}

fn prove_node_range(
    index: usize,
    kind: ReconciledNodeKind,
    node: &PresentationNode,
    proof: &mut RangeProof<'_>,
) -> Result<(NodePresentation, bool), ParseFailure> {
    let yaml = proof.yaml;
    let id = node_id(index)?;
    let offset = safe_offset(node.whole, yaml);
    let parent_range = match node.parent {
        Some(parent) => usize::try_from(parent)
            .ok()
            .and_then(|parent| proof.prior.get(parent).copied())
            .and_then(presentation_range),
        None => None,
    };
    let basic = valid_range(node.whole, yaml)
        && valid_range(node.payload, yaml)
        && contains(node.whole, node.payload)
        && node.tag_range.is_none_or(|tag| {
            valid_range(tag, yaml) && contains(node.whole, tag) && tag.end() <= node.payload.start()
        })
        && (node.parent.is_none() || parent_range.is_some_and(|range| contains(range, node.whole)))
        && ((node.tag == SourceTag::Implicit && node.tag_range.is_none())
            || (node.tag != SourceTag::Implicit && node.tag_range.is_some()));
    if !basic {
        proof.issues.add(
            RoundTripReason::InvalidCandidateRange,
            Some(id),
            None,
            None,
            offset,
        )?;
        return Ok((NodePresentation::Unreconciled, false));
    }
    if let Some(parent) = node.parent.and_then(|value| usize::try_from(value).ok()) {
        let Some(previous) = proof.last_child_end.get_mut(parent) else {
            proof.issues.add(
                RoundTripReason::InvalidCandidateRange,
                Some(id),
                None,
                None,
                offset,
            )?;
            return Ok((NodePresentation::Unreconciled, false));
        };
        if previous.is_some_and(|range| range.end() > node.whole.start()) {
            let overlap_offset = previous
                .map(|range| range.start().min(node.whole.start()))
                .or(offset);
            proof.issues.add(
                RoundTripReason::OverlappingSiblingRanges,
                Some(id),
                None,
                None,
                overlap_offset,
            )?;
            return Ok((NodePresentation::Unreconciled, false));
        }
        *previous = Some(node.whole);
    }
    if kind == ReconciledNodeKind::ImplicitEmpty {
        if !node.whole.is_empty() || !node.payload.is_empty() || node.tag_range.is_some() {
            proof.issues.add(
                RoundTripReason::InvalidCandidateRange,
                Some(id),
                None,
                None,
                offset,
            )?;
            return Ok((NodePresentation::Unreconciled, false));
        }
        let Some(file) = proof
            .file_base
            .get()
            .checked_add(node.payload.start().get())
        else {
            proof.issues.add(
                RoundTripReason::InvalidCandidateRange,
                Some(id),
                None,
                None,
                offset,
            )?;
            return Ok((NodePresentation::Unreconciled, false));
        };
        return Ok((
            NodePresentation::ImplicitEmpty {
                yaml_site: node.payload.start(),
                file_site: file_offset(file),
            },
            true,
        ));
    }
    let (Some(whole_file), Some(payload_file)) = (
        rebase(node.whole, proof.file_base),
        rebase(node.payload, proof.file_base),
    ) else {
        proof.issues.add(
            RoundTripReason::InvalidCandidateRange,
            Some(id),
            None,
            None,
            offset,
        )?;
        return Ok((NodePresentation::Unreconciled, false));
    };
    let tag_file = match node.tag_range {
        Some(tag) => {
            let Some(file) = rebase(tag, proof.file_base) else {
                proof.issues.add(
                    RoundTripReason::InvalidCandidateRange,
                    Some(id),
                    None,
                    None,
                    offset,
                )?;
                return Ok((NodePresentation::Unreconciled, false));
            };
            Some(file)
        }
        None => None,
    };
    Ok((
        NodePresentation::Concrete {
            whole_yaml_range: node.whole,
            whole_file_range: whole_file,
            payload_yaml_range: node.payload,
            payload_file_range: payload_file,
            tag_yaml_range: node.tag_range,
            tag_file_range: tag_file,
        },
        true,
    ))
}

#[derive(Clone, Copy, Default)]
struct EntryRefs {
    key: Option<NodeId>,
    value: Option<NodeId>,
}

struct EntrySlots {
    offsets: Box<[usize]>,
    order: Box<[usize]>,
    local: Box<[u32]>,
}

fn entry_direct_tables(
    semantic: &SemanticTrace,
    presentation: &Presentation,
    work: &mut ReconciliationWork,
) -> Result<(EntrySlots, Vec<EntryRefs>), ParseFailure> {
    let node_count = semantic.nodes.len();
    let mut counts = exact_vec(node_count)?;
    metered_resize(&mut counts, node_count, 0usize, work)?;
    for entry in &presentation.entries {
        work.entry()?;
        if let Some(count) = usize::try_from(entry.parent)
            .ok()
            .and_then(|parent| counts.get_mut(parent))
        {
            *count = count.checked_add(1).ok_or_else(invariant)?;
        }
    }
    let mut offsets = exact_vec(node_count.checked_add(1).ok_or_else(invariant)?)?;
    offsets.push(0usize);
    for count in counts {
        work.node()?;
        let next = offsets
            .last()
            .copied()
            .ok_or_else(invariant)?
            .checked_add(count)
            .ok_or_else(invariant)?;
        offsets.push(next);
    }
    let mut order = exact_vec(presentation.entries.len())?;
    let mut local = exact_vec(presentation.entries.len())?;
    let mut cursor = work.metered_copy(&offsets[..node_count])?;
    metered_resize(&mut order, presentation.entries.len(), usize::MAX, work)?;
    metered_resize(&mut local, presentation.entries.len(), u32::MAX, work)?;
    for (ordinal, entry) in presentation.entries.iter().enumerate() {
        work.entry()?;
        let Some(parent) = usize::try_from(entry.parent)
            .ok()
            .filter(|v| *v < node_count)
        else {
            continue;
        };
        let slot = *cursor.get(parent).ok_or_else(invariant)?;
        *order.get_mut(slot).ok_or_else(invariant)? = ordinal;
        let parent_offset = *offsets.get(parent).ok_or_else(invariant)?;
        *local.get_mut(ordinal).ok_or_else(invariant)? =
            u32::try_from(slot.checked_sub(parent_offset).ok_or_else(invariant)?)
                .map_err(|_| invariant())?;
        *cursor.get_mut(parent).ok_or_else(invariant)? =
            slot.checked_add(1).ok_or_else(invariant)?;
    }
    let mut refs = exact_vec(presentation.entries.len())?;
    metered_resize(
        &mut refs,
        presentation.entries.len(),
        EntryRefs::default(),
        work,
    )?;
    let offsets = metered_box(offsets, work)?;
    let order = metered_box(order, work)?;
    let local = metered_box(local, work)?;
    Ok((
        EntrySlots {
            offsets,
            order,
            local,
        },
        refs,
    ))
}

fn entry_slot_for_role(parent: usize, role: NodeRole, slots: &EntrySlots) -> Option<usize> {
    let index = match role {
        NodeRole::SequenceItem { index }
        | NodeRole::MappingKey { index }
        | NodeRole::MappingValue { index } => usize::try_from(index).ok()?,
        NodeRole::Root => return None,
    };
    let start = *slots.offsets.get(parent)?;
    let end = *slots.offsets.get(parent.checked_add(1)?)?;
    let bucket = start.checked_add(index)?;
    (bucket < end)
        .then(|| slots.order.get(bucket).copied())
        .flatten()
        .filter(|ordinal| *ordinal != usize::MAX)
}

struct EntryValidation {
    kind: Option<ReconciledEntryKind>,
    issue: RoundTripReason,
    offset: Option<YamlByteOffset>,
}

fn entry_payload_lines(
    kind: ReconciledEntryKind,
    nodes: &[PresentationNode],
) -> Result<(u32, u32), ParseFailure> {
    match kind {
        ReconciledEntryKind::Sequence { value } => {
            let node = nodes
                .get(usize::try_from(value.0).map_err(|_| invariant())?)
                .ok_or_else(invariant)?;
            Ok((node.start_line, node.end_line))
        }
        ReconciledEntryKind::Mapping { key, value } => {
            let key = nodes
                .get(usize::try_from(key.0).map_err(|_| invariant())?)
                .ok_or_else(invariant)?;
            let value = nodes
                .get(usize::try_from(value.0).map_err(|_| invariant())?)
                .ok_or_else(invariant)?;
            Ok((
                key.start_line.min(value.start_line),
                key.end_line.max(value.end_line),
            ))
        }
    }
}

struct EntryValidationContext<'a> {
    matched: usize,
    node_proven: &'a [bool],
    semantic: &'a SemanticTrace,
    node_presentations: &'a [NodePresentation],
    collection_styles: &'a [Option<CollectionPresentationStyle>],
    yaml: &'a [u8],
    last_end: &'a mut [Option<YamlByteRange>],
}

fn validate_entry(
    entry: &PresentationEntry,
    normalized_index: Option<u32>,
    refs: EntryRefs,
    context: &mut EntryValidationContext<'_>,
) -> EntryValidation {
    let Some(parent) = usize::try_from(entry.parent)
        .ok()
        .filter(|v| *v < context.matched)
    else {
        return EntryValidation {
            kind: None,
            issue: RoundTripReason::ParentOrArityMismatch,
            offset: safe_offset(entry.range, context.yaml),
        };
    };
    if normalized_index != Some(entry.index)
        || !context.node_proven.get(parent).copied().unwrap_or(false)
        || context.collection_styles.get(parent).copied().flatten() != Some(entry.style)
        || context.semantic.nodes.get(parent).is_none_or(|node| {
            (entry.sequence && node.kind != ReconciledNodeKind::Sequence)
                || (!entry.sequence && node.kind != ReconciledNodeKind::Mapping)
        })
        || !valid_range(entry.range, context.yaml)
    {
        return EntryValidation {
            kind: None,
            issue: RoundTripReason::InvalidCandidateRange,
            offset: safe_offset(entry.range, context.yaml),
        };
    }
    let Some(parent_range) = context
        .node_presentations
        .get(parent)
        .copied()
        .and_then(presentation_range)
    else {
        return EntryValidation {
            kind: None,
            issue: RoundTripReason::InvalidCandidateRange,
            offset: safe_offset(entry.range, context.yaml),
        };
    };
    let kind = if entry.sequence {
        let Some(value) = refs.value.filter(|id| {
            usize::try_from(id.0)
                .ok()
                .is_some_and(|v| v < context.matched && context.node_proven.get(v) == Some(&true))
        }) else {
            return EntryValidation {
                kind: None,
                issue: RoundTripReason::ParentOrArityMismatch,
                offset: safe_offset(entry.range, context.yaml),
            };
        };
        ReconciledEntryKind::Sequence { value }
    } else {
        let (Some(key), Some(value)) = (refs.key, refs.value) else {
            return EntryValidation {
                kind: None,
                issue: RoundTripReason::ParentOrArityMismatch,
                offset: safe_offset(entry.range, context.yaml),
            };
        };
        if [key, value].iter().any(|id| {
            usize::try_from(id.0)
                .ok()
                .is_none_or(|v| v >= context.matched || context.node_proven.get(v) != Some(&true))
        }) {
            return EntryValidation {
                kind: None,
                issue: RoundTripReason::ParentOrArityMismatch,
                offset: safe_offset(entry.range, context.yaml),
            };
        }
        ReconciledEntryKind::Mapping { key, value }
    };
    let referenced = match kind {
        ReconciledEntryKind::Mapping { key, value } => [Some(key), Some(value)],
        ReconciledEntryKind::Sequence { value } => [Some(value), None],
    };
    if !contains(parent_range, entry.range)
        || referenced.into_iter().flatten().any(|id| {
            usize::try_from(id.0)
                .ok()
                .and_then(|index| context.node_presentations.get(index).copied())
                .and_then(presentation_range)
                .is_none_or(|range| !contains(entry.range, range))
        })
    {
        return EntryValidation {
            kind: None,
            issue: RoundTripReason::InvalidCandidateRange,
            offset: safe_offset(entry.range, context.yaml),
        };
    }
    let Some(previous) = context.last_end.get_mut(parent) else {
        return EntryValidation {
            kind: None,
            issue: RoundTripReason::InvalidCandidateRange,
            offset: safe_offset(entry.range, context.yaml),
        };
    };
    if previous.is_some_and(|range| range.end() > entry.range.start()) {
        return EntryValidation {
            kind: None,
            issue: RoundTripReason::OverlappingSiblingRanges,
            offset: previous
                .map(|range| range.start().min(entry.range.start()))
                .or_else(|| safe_offset(entry.range, context.yaml)),
        };
    }
    *previous = Some(entry.range);
    EntryValidation {
        kind: Some(kind),
        issue: RoundTripReason::ParentOrArityMismatch,
        offset: None,
    }
}

struct CommentContext<'a> {
    yaml: &'a [u8],
    file_base: FileByteOffset,
    matched: usize,
    semantic: &'a SemanticTrace,
    node_presentations: &'a [NodePresentation],
    collection_styles: &'a [Option<CollectionPresentationStyle>],
    entries: &'a [ReconciledEntry],
    entry_lines: &'a [(u32, u32)],
    issues: &'a mut Issues,
    work: &'a mut ReconciliationWork,
}

fn classify_comments(
    presentation: &Presentation,
    context: &mut CommentContext<'_>,
) -> Result<Vec<CommentRecord>, ParseFailure> {
    let CommentContext {
        yaml,
        file_base,
        matched,
        semantic,
        node_presentations,
        collection_styles,
        entries,
        entry_lines,
        issues,
        work,
    } = context;
    let yaml = *yaml;
    let file_base = *file_base;
    let matched = *matched;
    let semantic = *semantic;
    let node_presentations = *node_presentations;
    let collection_styles = *collection_styles;
    let entries = *entries;
    let entry_lines = *entry_lines;
    let issues = &mut **issues;
    let work = &mut **work;
    let node_count = semantic.nodes.len();
    let mut counts = exact_vec(node_count)?;
    metered_resize(&mut counts, node_count, 0usize, work)?;
    for entry in entries {
        work.entry()?;
        let parent = usize::try_from(entry.parent.0).map_err(|_| invariant())?;
        let count = counts.get_mut(parent).ok_or_else(invariant)?;
        *count = count.checked_add(1).ok_or_else(invariant)?;
    }
    let mut offsets = exact_vec(node_count.checked_add(1).ok_or_else(invariant)?)?;
    offsets.push(0usize);
    for count in counts {
        work.node()?;
        offsets.push(
            offsets
                .last()
                .copied()
                .ok_or_else(invariant)?
                .checked_add(count)
                .ok_or_else(invariant)?,
        );
    }
    let mut bucketed = exact_vec(entries.len())?;
    metered_resize(&mut bucketed, entries.len(), EntryId(u32::MAX), work)?;
    let mut write = work.metered_copy(&offsets[..node_count])?;
    for entry in entries {
        work.entry()?;
        let parent = usize::try_from(entry.parent.0).map_err(|_| invariant())?;
        let slot = *write.get(parent).ok_or_else(invariant)?;
        *bucketed.get_mut(slot).ok_or_else(invariant)? = entry.id;
        *write.get_mut(parent).ok_or_else(invariant)? =
            slot.checked_add(1).ok_or_else(invariant)?;
    }
    let mut cursors = work.metered_copy(&offsets[..node_count])?;
    let blank_capacity = presentation
        .comment_line_blank
        .len()
        .checked_add(1)
        .ok_or_else(invariant)?;
    let mut blank_prefix = exact_vec(blank_capacity)?;
    blank_prefix.push(0u32);
    for value in &presentation.comment_line_blank {
        work.charge(1)?;
        let next = blank_prefix
            .last()
            .copied()
            .ok_or_else(invariant)?
            .checked_add(u32::from(*value != 0))
            .ok_or_else(invariant)?;
        blank_prefix.push(next);
    }
    let mut comment_counts = exact_vec(node_count)?;
    metered_resize(&mut comment_counts, node_count, 0usize, work)?;
    for candidate in &presentation.comments {
        work.comment()?;
        if let Some(parent) = candidate
            .deepest_collection
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < matched)
        {
            let count = comment_counts.get_mut(parent).ok_or_else(invariant)?;
            *count = count.checked_add(1).ok_or_else(invariant)?;
        }
    }
    let mut comment_offsets = exact_vec(node_count.checked_add(1).ok_or_else(invariant)?)?;
    comment_offsets.push(0usize);
    for count in comment_counts {
        work.node()?;
        comment_offsets.push(
            comment_offsets
                .last()
                .copied()
                .ok_or_else(invariant)?
                .checked_add(count)
                .ok_or_else(invariant)?,
        );
    }
    let bucketed_comment_count = *comment_offsets.last().ok_or_else(invariant)?;
    let mut bucketed_comments = exact_vec(bucketed_comment_count)?;
    metered_resize(
        &mut bucketed_comments,
        bucketed_comment_count,
        usize::MAX,
        work,
    )?;
    let mut comment_write = work.metered_copy(&comment_offsets[..node_count])?;
    for (ordinal, candidate) in presentation.comments.iter().enumerate() {
        work.comment()?;
        if let Some(parent) = candidate
            .deepest_collection
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < matched)
        {
            let slot = *comment_write.get(parent).ok_or_else(invariant)?;
            *bucketed_comments.get_mut(slot).ok_or_else(invariant)? = ordinal;
            *comment_write.get_mut(parent).ok_or_else(invariant)? =
                slot.checked_add(1).ok_or_else(invariant)?;
        }
    }
    let mut comment_cursors = work.metered_copy(&comment_offsets[..node_count])?;
    let mut scalar_nodes = exact_vec(node_count)?;
    for (index, node) in semantic.nodes.iter().enumerate() {
        work.node()?;
        if matches!(
            node.kind,
            ReconciledNodeKind::Scalar | ReconciledNodeKind::ImplicitEmpty
        ) && node_presentations
            .get(index)
            .copied()
            .and_then(presentation_range)
            .is_some()
        {
            scalar_nodes.push(index);
        }
    }
    let mut scalar_cursor = 0usize;
    let mut comments = exact_vec(presentation.comments.len())?;
    let mut boundary_parents = exact_vec(presentation.comments.len())?;
    let mut prior_comment_end = None::<YamlByteOffset>;
    for (index, candidate) in presentation.comments.iter().copied().enumerate() {
        work.comment()?;
        let id = CommentId(u32::try_from(index).map_err(|_| invariant())?);
        let mut range = candidate.range;
        let crossed = prior_comment_end.is_some_and(|end| end > range.start());
        let valid = valid_range(range, yaml) && !range.is_empty();
        if valid {
            prior_comment_end =
                Some(prior_comment_end.map_or(range.end(), |end| end.max(range.end())));
        } else {
            range = super::source::yaml_range(0, 0)?;
        }
        let mut ambiguous = !valid || crossed;
        let file_range = if let Some(value) = rebase(range, file_base) {
            value
        } else {
            ambiguous = true;
            rebase(super::source::yaml_range(0, 0)?, file_base).ok_or_else(invariant)?
        };
        let deepest_node = candidate
            .deepest_node
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < matched)
            .filter(|value| {
                node_presentations
                    .get(*value)
                    .copied()
                    .and_then(presentation_range)
                    .is_some_and(|owner| contains(owner, range))
            });
        if candidate.deepest_node.is_some() && deepest_node.is_none() {
            ambiguous = true;
        }
        let deepest_entry = candidate
            .deepest_entry
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < entries.len())
            .filter(|value| {
                entries
                    .get(*value)
                    .is_some_and(|entry| contains(entry.yaml_range, range))
            });
        if candidate.deepest_entry.is_some() && deepest_entry.is_none() {
            ambiguous = true;
        }
        let parent = candidate
            .deepest_collection
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < matched)
            .filter(|value| {
                node_presentations
                    .get(*value)
                    .copied()
                    .and_then(presentation_range)
                    .is_some_and(|owner| contains(owner, range))
            });
        if candidate.deepest_collection.is_some() && parent.is_none() {
            ambiguous = true;
        }
        while let Some(index) = scalar_nodes.get(scalar_cursor).copied() {
            let scalar_range = node_presentations
                .get(index)
                .copied()
                .and_then(presentation_range)
                .ok_or_else(invariant)?;
            work.node()?;
            if scalar_range.end() > range.start() {
                break;
            }
            scalar_cursor = scalar_cursor.checked_add(1).ok_or_else(invariant)?;
        }
        if let Some(index) = scalar_nodes.get(scalar_cursor).copied() {
            let scalar_range = node_presentations
                .get(index)
                .copied()
                .and_then(presentation_range)
                .ok_or_else(invariant)?;
            if scalar_range.start() < range.end()
                && deepest_node != Some(index)
                && !range.is_empty()
            {
                ambiguous = true;
            }
        }
        if let Some(parent) = parent {
            let cursor = comment_cursors.get_mut(parent).ok_or_else(invariant)?;
            if bucketed_comments.get(*cursor).copied() != Some(index) {
                return Err(invariant());
            }
            *cursor = cursor.checked_add(1).ok_or_else(invariant)?;
            work.comment()?;
        }

        let inline = valid && candidate.inline_before;
        let flow = parent.is_some_and(|value| {
            collection_styles.get(value).copied().flatten()
                == Some(CollectionPresentationStyle::Flow)
        });
        let nested_scalar = deepest_node.is_some_and(|value| {
            semantic.nodes.get(value).is_some_and(|node| {
                matches!(
                    node.kind,
                    ReconciledNodeKind::Scalar | ReconciledNodeKind::ImplicitEmpty
                )
            })
        });

        let mut boundary_parent = None;
        let (mut placement, owner) = if ambiguous {
            (CommentPlacement::Unresolved, CommentOwner::Ambiguous)
        } else if nested_scalar {
            (
                CommentPlacement::Nested,
                CommentOwner::Node(node_id(deepest_node.ok_or_else(invariant)?)?),
            )
        } else if inline {
            if let Some(entry) = deepest_entry.and_then(|value| entries.get(value)) {
                (CommentPlacement::Inline, CommentOwner::Entry(entry.id))
            } else if let Some(node) = deepest_node {
                (CommentPlacement::Inline, CommentOwner::Node(node_id(node)?))
            } else {
                (CommentPlacement::Unresolved, CommentOwner::Ambiguous)
            }
        } else if let Some(parent) = parent {
            boundary_parent = Some(parent);
            (
                CommentPlacement::Leading,
                CommentOwner::Boundary {
                    parent: Some(node_id(parent)?),
                    before: None,
                    after: None,
                },
            )
        } else {
            (
                CommentPlacement::DocumentOnly,
                CommentOwner::Boundary {
                    parent: None,
                    before: None,
                    after: None,
                },
            )
        };
        if flow && !matches!(placement, CommentPlacement::Unresolved) {
            placement = CommentPlacement::Flow;
        }
        if matches!(owner, CommentOwner::Ambiguous) {
            placement = CommentPlacement::Unresolved;
            issues.add(
                RoundTripReason::CommentOwnershipAmbiguous,
                None,
                None,
                Some(id),
                safe_offset(candidate.range, yaml),
            )?;
        }
        comments.push(CommentRecord {
            id,
            yaml_range: range,
            file_range,
            placement,
            owner,
        });
        boundary_parents.push(boundary_parent);
    }
    assign_boundary_runs(BoundaryAssignment {
        presentation,
        boundary_parents: &boundary_parents,
        comments: &mut comments,
        entry_offsets: &offsets,
        bucketed_entries: &bucketed,
        entry_cursors: &mut cursors,
        comment_offsets: &comment_offsets,
        bucketed_comments: &bucketed_comments,
        entries,
        entry_lines,
        collection_styles,
        blank_prefix: &blank_prefix,
        work,
    })?;
    Ok(comments)
}

struct BoundaryTables<'a> {
    entry_offsets: &'a [usize],
    bucketed_entries: &'a [EntryId],
    entry_cursors: &'a mut [usize],
    entries: &'a [ReconciledEntry],
    entry_lines: &'a [(u32, u32)],
    blank_prefix: &'a [u32],
    work: &'a mut ReconciliationWork,
}

struct BoundaryAssignment<'a> {
    presentation: &'a Presentation,
    boundary_parents: &'a [Option<usize>],
    comments: &'a mut [CommentRecord],
    entry_offsets: &'a [usize],
    bucketed_entries: &'a [EntryId],
    entry_cursors: &'a mut [usize],
    comment_offsets: &'a [usize],
    bucketed_comments: &'a [usize],
    entries: &'a [ReconciledEntry],
    entry_lines: &'a [(u32, u32)],
    collection_styles: &'a [Option<CollectionPresentationStyle>],
    blank_prefix: &'a [u32],
    work: &'a mut ReconciliationWork,
}

fn assign_boundary_runs(context: BoundaryAssignment<'_>) -> Result<(), ParseFailure> {
    let BoundaryAssignment {
        presentation,
        boundary_parents,
        comments,
        entry_offsets,
        bucketed_entries,
        entry_cursors,
        comment_offsets,
        bucketed_comments,
        entries,
        entry_lines,
        collection_styles,
        blank_prefix,
        work,
    } = context;
    let node_count = entry_cursors.len();
    let mut tables = BoundaryTables {
        entry_offsets,
        bucketed_entries,
        entry_cursors,
        entries,
        entry_lines,
        blank_prefix,
        work,
    };
    for parent in 0..node_count {
        tables.work.node()?;
        let mut cursor = *comment_offsets.get(parent).ok_or_else(invariant)?;
        let end = *comment_offsets
            .get(parent.checked_add(1).ok_or_else(invariant)?)
            .ok_or_else(invariant)?;
        while cursor < end {
            tables.work.comment()?;
            let index = *bucketed_comments.get(cursor).ok_or_else(invariant)?;
            if boundary_parents.get(index).copied().flatten() != Some(parent) {
                cursor = cursor.checked_add(1).ok_or_else(invariant)?;
                continue;
            }
            let run_start = cursor;
            cursor = cursor.checked_add(1).ok_or_else(invariant)?;
            while cursor < end {
                tables.work.comment()?;
                let previous = *bucketed_comments
                    .get(cursor.checked_sub(1).ok_or_else(invariant)?)
                    .ok_or_else(invariant)?;
                let next = *bucketed_comments.get(cursor).ok_or_else(invariant)?;
                if boundary_parents.get(next).copied().flatten() != Some(parent)
                    || !adjacent_comment_lines(
                        *presentation.comments.get(previous).ok_or_else(invariant)?,
                        *presentation.comments.get(next).ok_or_else(invariant)?,
                        blank_prefix,
                    )?
                {
                    break;
                }
                cursor = cursor.checked_add(1).ok_or_else(invariant)?;
            }
            let first_index = *bucketed_comments.get(run_start).ok_or_else(invariant)?;
            let last_index = *bucketed_comments
                .get(cursor.checked_sub(1).ok_or_else(invariant)?)
                .ok_or_else(invariant)?;
            let first = *presentation
                .comments
                .get(first_index)
                .ok_or_else(invariant)?;
            let last = *presentation
                .comments
                .get(last_index)
                .ok_or_else(invariant)?;
            let (placement, owner) = boundary_run(parent, first, last, &mut tables)?;
            let flow = collection_styles.get(parent).copied().flatten()
                == Some(CollectionPresentationStyle::Flow);
            for slot in run_start..cursor {
                let index = *bucketed_comments.get(slot).ok_or_else(invariant)?;
                let comment = comments.get_mut(index).ok_or_else(invariant)?;
                comment.placement = if flow {
                    CommentPlacement::Flow
                } else {
                    placement
                };
                comment.owner = owner;
                tables.work.comment()?;
            }
        }
    }
    Ok(())
}

fn adjacent_comment_lines(
    previous: super::presentation::CommentCandidate,
    next: super::presentation::CommentCandidate,
    blank_prefix: &[u32],
) -> Result<bool, ParseFailure> {
    if next.line != previous.line.checked_add(1).ok_or_else(invariant)? {
        return Ok(false);
    }
    let low = usize::try_from(previous.line.saturating_add(1)).map_err(|_| invariant())?;
    let high = usize::try_from(next.line).map_err(|_| invariant())?;
    let before = *blank_prefix.get(low).ok_or_else(invariant)?;
    let after = *blank_prefix.get(high).ok_or_else(invariant)?;
    Ok(before == after)
}

fn boundary_run(
    parent: usize,
    first: super::presentation::CommentCandidate,
    last: super::presentation::CommentCandidate,
    context: &mut BoundaryTables<'_>,
) -> Result<(CommentPlacement, CommentOwner), ParseFailure> {
    let start = *context.entry_offsets.get(parent).ok_or_else(invariant)?;
    let end = *context
        .entry_offsets
        .get(parent.checked_add(1).ok_or_else(invariant)?)
        .ok_or_else(invariant)?;
    let cursor = context
        .entry_cursors
        .get_mut(parent)
        .ok_or_else(invariant)?;
    if *cursor < start || *cursor > end {
        return Err(invariant());
    }
    while *cursor < end {
        let id = *context
            .bucketed_entries
            .get(*cursor)
            .ok_or_else(invariant)?;
        let entry = context
            .entries
            .get(usize::try_from(id.0).map_err(|_| invariant())?)
            .ok_or_else(invariant)?;
        context.work.entry()?;
        if entry.yaml_range.end() > first.range.start() {
            break;
        }
        *cursor = cursor.checked_add(1).ok_or_else(invariant)?;
    }
    let before = cursor
        .checked_sub(1)
        .filter(|value| *value >= start)
        .and_then(|value| context.bucketed_entries.get(value).copied());
    let after = (*cursor < end)
        .then(|| context.bucketed_entries.get(*cursor).copied())
        .flatten();
    let before = eligible_entry(
        before,
        first.line,
        false,
        context.entries,
        context.entry_lines,
        context.blank_prefix,
    )?;
    let after = eligible_entry(
        after,
        last.line,
        true,
        context.entries,
        context.entry_lines,
        context.blank_prefix,
    )?;
    let parent = Some(node_id(parent)?);
    Ok(match (before, after) {
        (Some(before), Some(after)) => (
            CommentPlacement::InterEntry,
            CommentOwner::Boundary {
                parent,
                before: Some(before),
                after: Some(after),
            },
        ),
        (None, Some(after)) => (
            CommentPlacement::Leading,
            CommentOwner::Boundary {
                parent,
                before: None,
                after: Some(after),
            },
        ),
        (Some(before), None) => (
            CommentPlacement::Trailing,
            CommentOwner::Boundary {
                parent,
                before: Some(before),
                after: None,
            },
        ),
        (None, None) => (
            CommentPlacement::Leading,
            CommentOwner::Boundary {
                parent,
                before: None,
                after: None,
            },
        ),
    })
}

fn eligible_entry(
    id: Option<EntryId>,
    comment_line: u32,
    following: bool,
    entries: &[ReconciledEntry],
    entry_lines: &[(u32, u32)],
    blank_prefix: &[u32],
) -> Result<Option<EntryId>, ParseFailure> {
    let Some(id) = id else { return Ok(None) };
    let index = usize::try_from(id.0).map_err(|_| invariant())?;
    let _entry = entries.get(index).ok_or_else(invariant)?;
    let (start, end) = *entry_lines.get(index).ok_or_else(invariant)?;
    let (low, high) = if following {
        (comment_line, start)
    } else {
        (end, comment_line)
    };
    if low > high {
        return Ok(None);
    }
    let scan_start = usize::try_from(low.saturating_add(1)).map_err(|_| invariant())?;
    let scan_end = usize::try_from(high).map_err(|_| invariant())?;
    if scan_start >= scan_end {
        return Ok(Some(id));
    }
    let before = *blank_prefix.get(scan_start).ok_or_else(invariant)?;
    let after = *blank_prefix.get(scan_end).ok_or_else(invariant)?;
    Ok((before == after).then_some(id))
}

fn finish(
    semantic: SemanticTrace,
    draft: ReconciliationDraft,
    document: UnknownMetadataDocument,
) -> Result<ParsedYamlArtifact, ParseFailure> {
    let ReconciliationDraft {
        nodes: draft_nodes,
        entries,
        comments,
        issues,
        mut work,
    } = draft;
    if semantic.nodes.len() != draft_nodes.len() {
        return Err(ParseFailure::plain(
            ParseFailureKind::SemanticBuilderInvariant,
        ));
    }
    semantic
        .nodes
        .len()
        .checked_mul(core::mem::size_of::<ReconciledNode>())
        .ok_or_else(|| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?;
    let mut nodes = exact_vec(semantic.nodes.len())?;
    #[cfg(test)]
    work.seed_one_over_before_final_visit(semantic.nodes.len(), entries.len(), comments.len())?;
    for (index, (source, prepared)) in semantic.nodes.into_iter().zip(draft_nodes).enumerate() {
        work.node()?;
        if source.ordinal != prepared.ordinal
            || source.ordinal
                != u32::try_from(index)
                    .map_err(|_| ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant))?
            || source.parent != prepared.parent
            || source.role != prepared.role
            || source.kind != prepared.kind
            || source.scalar_style != prepared.scalar_style
            || source.source_tag != prepared.source_tag
        {
            return Err(ParseFailure::plain(
                ParseFailureKind::SemanticBuilderInvariant,
            ));
        }
        nodes.push(ReconciledNode {
            id: NodeId(source.ordinal),
            parent: source.parent.map(NodeId),
            role: source.role,
            kind: source.kind,
            children: prepared.children,
            entries: prepared.entries,
            decoded_scalar: source.decoded,
            scalar_style: source.scalar_style,
            collection_style: prepared.collection_style,
            source_tag: source.source_tag,
            presentation: prepared.presentation,
        });
    }
    #[cfg(test)]
    TRACE_IDENTITIES.with(|expected| {
        let expected = expected.borrow();
        let mut actual = nodes.iter().filter_map(|node| {
            node.decoded_scalar
                .as_ref()
                .map(|value| (value.as_ptr() as usize, value.len()))
        });
        let matches = expected
            .iter()
            .filter(|identity| actual.next() == Some(**identity))
            .count();
        let complete = actual.next().is_none();
        FINISH_IDENTITIES.with(|observation| {
            observation.set((expected.len(), if complete { matches } else { 0 }));
        });
    });
    for (index, entry) in entries.iter().enumerate() {
        work.entry()?;
        if entry.id != entry_id(index)? {
            return Err(invariant());
        }
    }
    for (index, comment) in comments.iter().enumerate() {
        work.comment()?;
        if comment.id != CommentId(u32::try_from(index).map_err(|_| invariant())?)
            || !comment_owner_resolves(comment.owner, entries.len())
        {
            return Err(invariant());
        }
    }
    for issue in &issues.values {
        work.charge(1)?;
        if issue
            .entry
            .is_some_and(|id| !entry_resolves(id, entries.len()))
            || issue
                .comment
                .is_some_and(|id| usize::try_from(id.0).map_or(true, |v| v >= comments.len()))
        {
            return Err(invariant());
        }
    }
    let node_count = nodes.len();
    let entry_count = entries.len();
    let comment_count = comments.len();
    let round_trip = issues.finish(&mut work)?;
    let nodes = metered_box(nodes, &mut work)?;
    let entries = metered_box(entries, &mut work)?;
    let comments = metered_box(comments, &mut work)?;
    work.finish(node_count, entry_count, comment_count)?;
    Ok(ParsedYamlArtifact {
        semantic: document,
        nodes,
        entries,
        comments,
        round_trip,
    })
}

fn entry_resolves(id: EntryId, entry_count: usize) -> bool {
    usize::try_from(id.0).is_ok_and(|value| value < entry_count)
}

fn comment_owner_resolves(owner: CommentOwner, entry_count: usize) -> bool {
    match owner {
        CommentOwner::Entry(id) => entry_resolves(id, entry_count),
        CommentOwner::Boundary { before, after, .. } => {
            before.is_none_or(|id| entry_resolves(id, entry_count))
                && after.is_none_or(|id| entry_resolves(id, entry_count))
        }
        CommentOwner::Node(_) | CommentOwner::Ambiguous => true,
    }
}

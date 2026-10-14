use std::borrow::Cow;
#[cfg(test)]
use std::cell::Cell;

use saphyr_parser::{Event, Parser, ScalarStyle as SaphyrStyle, Tag};

#[cfg(test)]
use crate::domain::UnknownYamlTestMeter;
use crate::domain::{
    UnknownMetadataDocument, UnknownYamlBuildErrorKind, UnknownYamlDocumentBuilder,
    UnknownYamlEvent,
};

use super::presentation::Presentation;
use super::reconcile::FoldPermit;
use super::reconcile::{NodeRole, ReconciledNodeKind, ScalarPresentationStyle, SourceTag};
use super::scalar::{self, NumericMeter};
use super::source::{ParseFailure, ParseFailureKind, ParseLimitScope};

const MAX_DEPTH: usize = 16;
const MAX_NODES: usize = 20_000;
const MAX_MAPPING_ENTRIES: usize = 256;
const MAX_SEQUENCE_ENTRIES: usize = 2_048;
const MAX_SCALAR: usize = 65_536;
const MAX_NUMERIC_DIGITS: u64 = 80 * 1024;
const MAX_SEMANTIC_ENCODING: u64 = 8 * 1024 * 1024;
const MAX_SEMANTIC_COMPARISON: u64 = 32 * 1024 * 1024;

#[cfg(test)]
thread_local! {
    static FOLD_OBSERVATION: Cell<(bool, usize, usize)> = const { Cell::new((false, 0, 0)) };
    static MIRROR_CORRUPTION: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    static METER_EQUALITY_ACCEPTED: Cell<bool> = const { Cell::new(false) };
}

pub(super) struct SemanticTrace {
    pub(super) nodes: Vec<SemanticNode>,
    build: Vec<BuildEvent>,
}

pub(super) struct SemanticNode {
    pub(super) ordinal: u32,
    pub(super) parent: Option<u32>,
    pub(super) role: NodeRole,
    pub(super) kind: ReconciledNodeKind,
    pub(super) decoded: Option<Box<str>>,
    pub(super) scalar_style: Option<ScalarPresentationStyle>,
    pub(super) source_tag: SourceTag,
    pub(super) child_count: u32,
}

enum BuildEvent {
    BeginSequence,
    EndSequence,
    BeginMapping,
    EndMapping,
    Scalar {
        node_index: usize,
        style: ScalarPresentationStyle,
        tag: SourceTag,
    },
}

#[repr(C)]
struct Frame {
    ordinal: u32,
    count: u32,
    kind: u8,
    mapping_value: u8,
    reserved: [u8; 6],
}

const _: [(); 16] = [(); core::mem::size_of::<Frame>()];
const _: [(); 4] = [(); core::mem::align_of::<Frame>()];

#[derive(Clone, Copy, Eq, PartialEq)]
enum StreamState {
    ExpectStreamStart,
    ExpectDocumentStart,
    InDocument,
    ExpectDocumentEnd,
    ExpectStreamEnd,
    ExpectNone,
    Broken,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Guard {
    None,
    Merge,
    BuiltIn(SourceTag),
    Uninspectable,
    InvalidTag,
}

struct EventFailure {
    ordinal: u64,
    rank: u8,
    failure: ParseFailure,
}

struct Pending {
    scan: Option<ParseFailure>,
    stream: Option<EventFailure>,
    guard: Option<EventFailure>,
    restriction: Option<EventFailure>,
    semantic: Option<EventFailure>,
    tail_invariant: bool,
}

enum Inspection {
    Available(u64),
    Disabled,
}

struct BorrowedTextMeter {
    limit: u64,
    used: u64,
    inspected: u64,
    disabled: bool,
}

impl BorrowedTextMeter {
    fn new(yaml_len: usize, descriptors: usize) -> (Self, bool) {
        let (limit, failed) = match borrowed_text_limit(yaml_len, descriptors) {
            Ok(limit) => (limit, false),
            Err(()) => (0, true),
        };
        (
            Self {
                limit,
                used: 0,
                inspected: 0,
                disabled: failed,
            },
            failed,
        )
    }

    fn inspect(&mut self, text: &str) -> Inspection {
        if self.disabled {
            return Inspection::Disabled;
        }
        let Ok(amount) = u64::try_from(text.len()) else {
            self.disabled = true;
            return Inspection::Disabled;
        };
        let Some(next) = self.used.checked_add(amount) else {
            self.disabled = true;
            return Inspection::Disabled;
        };
        if next > self.limit {
            self.disabled = true;
            return Inspection::Disabled;
        }
        let Some(inspected) = self.inspected.checked_add(amount) else {
            self.disabled = true;
            return Inspection::Disabled;
        };
        self.used = next;
        self.inspected = inspected;
        Inspection::Available(amount)
    }
}

fn borrowed_text_limit(yaml_len: usize, descriptors: usize) -> Result<u64, ()> {
    let yaml_len = u64::try_from(yaml_len).map_err(|_| ())?;
    let descriptors = u64::try_from(descriptors).map_err(|_| ())?;
    yaml_len
        .checked_mul(3)
        .and_then(|value| value.checked_add(1))
        .map(|value| value / 2)
        .and_then(|value| value.checked_add(descriptors))
        .ok_or(())
}

struct Reducer<'a> {
    presentation: &'a Presentation,
    event_call_limit: u64,
    event_calls: u64,
    event_ordinal: u64,
    node_ordinal: u32,
    source_cursor: usize,
    prefix_proven: bool,
    source_tail_visited: bool,
    tail_work: u64,
    source_byte_visits: u64,
    #[cfg(test)]
    retained_copy_attempts: usize,
    #[cfg(test)]
    retained_copy_bytes: usize,
    #[cfg(test)]
    force_retained_reservation_failure: bool,
    text_meter: BorrowedTextMeter,
    stream_state: StreamState,
    stream_records: u8,
    document_starts: u8,
    stopped_by_event_invariant: bool,
    root_complete: bool,
    saw_root: bool,
    structural_limit: usize,
    frames: Vec<Frame>,
    nodes: Vec<SemanticNode>,
    build: Vec<BuildEvent>,
    retained_node_events: usize,
    retained_end_events: usize,
    semantic_decoded: usize,
    semantic_decoded_base: usize,
    pending: Pending,
}

impl<'a> Reducer<'a> {
    fn new(text_len: usize, presentation: &'a Presentation) -> Result<Self, ParseFailure> {
        let text_len_u64 = u64::try_from(text_len).map_err(|_| invariant())?;
        let event_call_limit = text_len_u64
            .checked_mul(8)
            .and_then(|value| value.checked_add(32))
            .ok_or_else(invariant)?;
        let descriptor_count = presentation.descriptor_count();
        let structural_limit = text_len
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(invariant)?;
        structural_limit
            .checked_mul(core::mem::size_of::<Frame>())
            .ok_or_else(invariant)?;
        let decoded_base = text_len
            .checked_mul(3)
            .and_then(|value| value.checked_add(1))
            .map(|value| value / 2)
            .ok_or_else(invariant)?;
        let (text_meter, borrowed_limit_failed) =
            BorrowedTextMeter::new(text_len, descriptor_count);

        let mut frames = Vec::new();
        frames
            .try_reserve_exact(structural_limit)
            .map_err(|_| invariant())?;
        let retained_node_limit = MAX_NODES.min(structural_limit);
        retained_node_limit
            .checked_mul(core::mem::size_of::<SemanticNode>())
            .ok_or_else(invariant)?;
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(retained_node_limit)
            .map_err(|_| invariant())?;
        let mut build = Vec::new();
        let build_limit = MAX_NODES
            .checked_mul(2)
            .and_then(|value| value.checked_add(4))
            .ok_or_else(invariant)?;
        build_limit
            .checked_mul(core::mem::size_of::<BuildEvent>())
            .ok_or_else(invariant)?;
        build
            .try_reserve_exact(build_limit)
            .map_err(|_| invariant())?;

        Ok(Self {
            presentation,
            event_call_limit,
            event_calls: 0,
            event_ordinal: 0,
            node_ordinal: 0,
            source_cursor: 0,
            prefix_proven: true,
            source_tail_visited: false,
            tail_work: u64::try_from(descriptor_count).map_err(|_| invariant())?,
            source_byte_visits: 0,
            #[cfg(test)]
            retained_copy_attempts: 0,
            #[cfg(test)]
            retained_copy_bytes: 0,
            #[cfg(test)]
            force_retained_reservation_failure: false,
            text_meter,
            stream_state: StreamState::ExpectStreamStart,
            stream_records: 0,
            document_starts: 0,
            stopped_by_event_invariant: false,
            root_complete: false,
            saw_root: false,
            structural_limit,
            frames,
            nodes,
            build,
            retained_node_events: 0,
            retained_end_events: 0,
            semantic_decoded: 0,
            semantic_decoded_base: decoded_base,
            pending: Pending {
                scan: None,
                stream: None,
                guard: None,
                restriction: None,
                semantic: None,
                tail_invariant: borrowed_limit_failed,
            },
        })
    }

    fn before_call(&mut self) -> bool {
        let Some(next) = self.event_calls.checked_add(1) else {
            self.stopped_by_event_invariant = true;
            self.poison_tail();
            return false;
        };
        self.event_calls = next;
        if next > self.event_call_limit {
            self.stopped_by_event_invariant = true;
            self.poison_tail();
            return false;
        }
        self.charge_tail(1);
        true
    }

    fn accept(&mut self, event: Event<'_>) {
        let ordinal = self.event_ordinal;
        let Some(next_ordinal) = self.event_ordinal.checked_add(1) else {
            self.poison_tail();
            return;
        };
        self.event_ordinal = next_ordinal;
        match event {
            Event::StreamStart => self.stream_start(ordinal),
            Event::DocumentStart(explicit) => self.document_start(explicit, ordinal),
            Event::DocumentEnd => self.document_end(ordinal),
            Event::StreamEnd => self.stream_end(ordinal),
            Event::Nothing => self.note_stream(ordinal, 1, ParseFailureKind::SemanticStreamOrder),
            Event::Alias(_) => {
                self.note_stream_position(ordinal);
                self.note_restriction(ordinal, 0, ParseFailureKind::AliasDenied);
            }
            Event::Scalar(text, style, anchor, tag) => {
                self.scalar(ordinal, text, style, anchor, tag)
            }
            Event::SequenceStart(anchor, tag) => self.collection_start(ordinal, false, anchor, tag),
            Event::MappingStart(anchor, tag) => self.collection_start(ordinal, true, anchor, tag),
            Event::SequenceEnd => self.collection_end(ordinal, false),
            Event::MappingEnd => self.collection_end(ordinal, true),
        }
    }

    fn finish_drain(mut self) -> Result<(Vec<SemanticNode>, Vec<BuildEvent>), ParseFailure> {
        if !self.stopped_by_event_invariant && self.stream_state != StreamState::ExpectNone {
            self.note_stream(self.event_ordinal, 1, ParseFailureKind::SemanticStreamOrder);
        } else {
            self.stream_state = StreamState::ExpectNone;
        }
        if !self.saw_root {
            self.note_semantic(
                self.event_ordinal,
                0,
                ParseFailure::plain(ParseFailureKind::RootMustBeMapping),
            );
        }
        if !self.stopped_by_event_invariant
            && self.prefix_proven
            && self.source_cursor != self.presentation.descriptor_count()
        {
            self.break_prefix_and_visit_source();
        }
        let descriptor_count =
            u64::try_from(self.presentation.descriptor_count()).map_err(|_| invariant())?;
        let tail_bound = descriptor_count
            .checked_mul(2)
            .and_then(|value| value.checked_add(self.event_call_limit))
            .and_then(|value| value.checked_add(self.text_meter.limit))
            .ok_or_else(invariant)?;
        if self.tail_work > tail_bound || self.text_meter.inspected > self.text_meter.limit {
            self.poison_tail();
        }

        if let Some(error) = self.pending.scan {
            return Err(error);
        }
        if let Some(error) = self.pending.stream {
            return Err(error.failure);
        }
        if let Some(error) = self.pending.guard {
            return Err(error.failure);
        }
        if let Some(error) = self.pending.restriction {
            return Err(error.failure);
        }
        if self.pending.tail_invariant {
            return Err(invariant());
        }
        if let Some(error) = self.pending.semantic {
            return Err(error.failure);
        }
        if self.retained_node_events > MAX_NODES
            || self.retained_end_events > MAX_NODES
            || self.stream_records > 4
            || self.nodes.len() != self.retained_node_events
        {
            return Err(invariant());
        }
        Ok((self.nodes, self.build))
    }

    fn scan_error(&mut self) {
        self.pending.scan = Some(ParseFailure::plain(ParseFailureKind::SemanticSyntax));
        self.clear_success();
    }

    fn stream_start(&mut self, ordinal: u64) {
        self.stream_records = self.stream_records.saturating_add(1);
        if self.stream_state == StreamState::ExpectStreamStart {
            self.stream_state = StreamState::ExpectDocumentStart;
        } else {
            self.note_stream(ordinal, 1, ParseFailureKind::SemanticStreamOrder);
        }
    }

    fn document_start(&mut self, explicit: bool, ordinal: u64) {
        self.stream_records = self.stream_records.saturating_add(1);
        let previous = self.document_starts;
        self.document_starts = self.document_starts.saturating_add(1);
        if previous != 0 {
            self.note_stream(ordinal, 0, ParseFailureKind::MultipleDocuments);
        } else if self.stream_state != StreamState::ExpectDocumentStart || explicit {
            self.note_stream(ordinal, 1, ParseFailureKind::SemanticStreamOrder);
        } else {
            self.stream_state = StreamState::InDocument;
        }
    }

    fn document_end(&mut self, ordinal: u64) {
        self.stream_records = self.stream_records.saturating_add(1);
        if self.stream_state == StreamState::ExpectDocumentEnd && self.frames.is_empty() {
            self.stream_state = StreamState::ExpectStreamEnd;
        } else {
            self.note_stream(ordinal, 1, ParseFailureKind::SemanticStreamOrder);
        }
    }

    fn stream_end(&mut self, ordinal: u64) {
        self.stream_records = self.stream_records.saturating_add(1);
        if self.stream_state == StreamState::ExpectStreamEnd {
            self.stream_state = StreamState::ExpectNone;
        } else {
            self.note_stream(ordinal, 1, ParseFailureKind::SemanticStreamOrder);
        }
    }

    fn note_stream_position(&mut self, ordinal: u64) {
        if self.stream_state != StreamState::InDocument || self.root_complete {
            self.note_stream(ordinal, 1, ParseFailureKind::SemanticStreamOrder);
        }
    }

    fn scalar(
        &mut self,
        event_ordinal: u64,
        text: Cow<'_, str>,
        style: SaphyrStyle,
        anchor: usize,
        tag: Option<Cow<'_, Tag>>,
    ) {
        self.note_stream_position(event_ordinal);
        let mapped_style = map_style(style);
        let (parent, role, parent_ordinal, role_code, index) = self.next_role();
        let expected_kind = if self.prefix_proven {
            self.presentation
                .descriptor_facts(self.source_cursor)
                .map(|facts| facts.4)
        } else {
            None
        };
        let inspection = self.text_meter.inspect(text.as_ref());
        let text_available = matches!(inspection, Inspection::Available(_));
        let decoded_oversize = text_available && text.len() > MAX_SCALAR;
        let mut allocation_failed = false;
        let mut retained_borrowed = if matches!(&inspection, Inspection::Available(_))
            && self.can_retain()
            && !decoded_oversize
        {
            match &text {
                Cow::Borrowed(borrowed) => {
                    #[cfg(test)]
                    {
                        self.retained_copy_attempts = self.retained_copy_attempts.saturating_add(1);
                        self.retained_copy_bytes =
                            self.retained_copy_bytes.saturating_add(borrowed.len());
                    }
                    #[cfg(test)]
                    let forced_failure = self.force_retained_reservation_failure;
                    #[cfg(not(test))]
                    let forced_failure = false;
                    let mut copy = String::new();
                    if forced_failure || copy.try_reserve_exact(borrowed.len()).is_err() {
                        allocation_failed = true;
                        self.poison_tail();
                        None
                    } else {
                        Some(copy)
                    }
                }
                Cow::Owned(_) => None,
            }
        } else {
            None
        };
        let analysis = if let Inspection::Available(amount) = inspection
            && !allocation_failed
        {
            match scalar::BorrowedScalarAnalysis::inspect(
                text.as_ref(),
                retained_borrowed.as_mut(),
                &mut self.source_byte_visits,
            ) {
                Ok(analysis) if analysis.visited_bytes() == amount => {
                    self.charge_tail(amount);
                    Some(analysis)
                }
                Ok(_) | Err(_) => {
                    self.poison_tail();
                    None
                }
            }
        } else {
            self.poison_tail();
            None
        };
        let implicit_empty = analysis
            .as_ref()
            .is_some_and(|analysis| analysis.is_tilde())
            && expected_kind == Some(3)
            && style == SaphyrStyle::Plain
            && tag.is_none();
        let kind = if implicit_empty { 3 } else { 0 };
        let event_guard = event_guard(
            tag.as_deref(),
            true,
            mapped_style,
            role_code,
            analysis.as_ref(),
        );
        let source_tag = guard_source_tag(event_guard);
        let matched_guard = self.compare_tuple_and_guards(
            (self.node_ordinal, parent_ordinal, index, role_code, kind),
            event_guard,
            event_ordinal,
        );

        if anchor != 0 {
            self.note_restriction(event_ordinal, 0, ParseFailureKind::AnchorDenied);
        }
        if decoded_oversize {
            self.note_semantic(
                event_ordinal,
                1,
                ParseFailure::limited(
                    ParseFailureKind::DecodedScalarLimit,
                    ParseLimitScope::DecodedScalarBytes,
                    MAX_SCALAR as u64,
                    (MAX_SCALAR + 1) as u64,
                ),
            );
        }
        if let Some(analysis) = analysis.as_ref() {
            self.inspect_scalar_restrictions(
                event_ordinal,
                matched_guard,
                analysis,
                mapped_style,
                source_tag,
            );
            if parent == Some(0)
                && role_code == 2
                && !analysis.resolves_to_string(mapped_style, source_tag)
            {
                self.note_semantic(
                    event_ordinal,
                    0,
                    ParseFailure::plain(ParseFailureKind::TopLevelKeyMustBeString),
                );
            }
        }
        self.accept_node_limit(event_ordinal, parent.is_none(), kind);

        if self.can_retain() && text_available {
            let next_decoded = self.semantic_decoded.checked_add(text.len());
            let decoded_limit = self
                .retained_node_events
                .checked_add(1)
                .and_then(|count| self.semantic_decoded_base.checked_add(count));
            if let (Some(next_decoded), Some(decoded_limit)) = (next_decoded, decoded_limit)
                && next_decoded <= decoded_limit
            {
                self.semantic_decoded = next_decoded;
                let decoded = match text {
                    Cow::Owned(decoded) => Some(decoded),
                    Cow::Borrowed(_) => retained_borrowed,
                };
                if let Some(decoded) = decoded {
                    let node_index = self.nodes.len();
                    self.nodes.push(SemanticNode {
                        ordinal: self.node_ordinal,
                        parent,
                        role,
                        kind: if implicit_empty {
                            ReconciledNodeKind::ImplicitEmpty
                        } else {
                            ReconciledNodeKind::Scalar
                        },
                        decoded: Some(decoded.into_boxed_str()),
                        scalar_style: Some(mapped_style),
                        source_tag,
                        child_count: 0,
                    });
                    self.build.push(BuildEvent::Scalar {
                        node_index,
                        style: mapped_style,
                        tag: source_tag,
                    });
                    self.retained_node_events += 1;
                }
            } else {
                self.poison_tail();
            }
        }
        self.advance_node_ordinal();
        self.complete_child(event_ordinal);
        self.finish_root_if_closed();
    }

    fn collection_start(
        &mut self,
        event_ordinal: u64,
        mapping: bool,
        anchor: usize,
        tag: Option<Cow<'_, Tag>>,
    ) {
        self.note_stream_position(event_ordinal);
        let (parent, role, parent_ordinal, role_code, index) = self.next_role();
        let kind = if mapping { 2 } else { 1 };
        let event_guard = event_guard(
            tag.as_deref(),
            false,
            ScalarPresentationStyle::Plain,
            role_code,
            None,
        );
        let source_tag = guard_source_tag(event_guard);
        self.compare_tuple_and_guards(
            (self.node_ordinal, parent_ordinal, index, role_code, kind),
            event_guard,
            event_ordinal,
        );
        if anchor != 0 {
            self.note_restriction(event_ordinal, 0, ParseFailureKind::AnchorDenied);
        }
        if parent == Some(0) && role_code == 2 {
            self.note_semantic(
                event_ordinal,
                0,
                ParseFailure::plain(ParseFailureKind::TopLevelKeyMustBeString),
            );
        }
        let next_depth = self.frames.len().checked_add(1);
        if next_depth.is_none_or(|depth| depth > MAX_DEPTH) {
            self.note_semantic(
                event_ordinal,
                2,
                ParseFailure::limited(
                    ParseFailureKind::DepthLimit,
                    ParseLimitScope::Depth,
                    MAX_DEPTH as u64,
                    (MAX_DEPTH + 1) as u64,
                ),
            );
        }
        self.accept_node_limit(event_ordinal, parent.is_none(), kind);
        if self.can_retain() {
            self.nodes.push(SemanticNode {
                ordinal: self.node_ordinal,
                parent,
                role,
                kind: if mapping {
                    ReconciledNodeKind::Mapping
                } else {
                    ReconciledNodeKind::Sequence
                },
                decoded: None,
                scalar_style: None,
                source_tag,
                child_count: 0,
            });
            self.build.push(if mapping {
                BuildEvent::BeginMapping
            } else {
                BuildEvent::BeginSequence
            });
            self.retained_node_events += 1;
        }
        if self.frames.len() >= self.structural_limit {
            self.poison_tail();
            return;
        }
        self.frames.push(Frame {
            ordinal: self.node_ordinal,
            count: 0,
            kind,
            mapping_value: 0,
            reserved: [0; 6],
        });
        self.advance_node_ordinal();
    }

    fn collection_end(&mut self, event_ordinal: u64, mapping: bool) {
        let expected = if mapping { 2 } else { 1 };
        let Some(frame) = self.frames.pop() else {
            self.note_stream(event_ordinal, 1, ParseFailureKind::SemanticStreamOrder);
            return;
        };
        if frame.kind != expected || (mapping && frame.mapping_value != 0) {
            self.note_stream(event_ordinal, 1, ParseFailureKind::SemanticStreamOrder);
        }
        let child_count = if mapping {
            frame.count.checked_mul(2)
        } else {
            Some(frame.count)
        };
        if child_count.is_none() {
            self.poison_tail();
        }
        if self.can_retain() {
            if let Some(node) = self.nodes.get_mut(frame.ordinal as usize) {
                node.child_count = child_count.expect("checked above");
                self.build.push(if mapping {
                    BuildEvent::EndMapping
                } else {
                    BuildEvent::EndSequence
                });
                self.retained_end_events += 1;
            } else {
                self.poison_tail();
            }
        }
        self.complete_child(event_ordinal);
        self.finish_root_if_closed();
    }

    fn finish_root_if_closed(&mut self) {
        if self.frames.is_empty() && self.saw_root {
            self.root_complete = true;
            if self.stream_state == StreamState::InDocument {
                self.stream_state = StreamState::ExpectDocumentEnd;
            }
        }
    }

    fn accept_node_limit(&mut self, ordinal: u64, root: bool, kind: u8) {
        if root {
            self.saw_root = true;
        }
        if self.node_ordinal as usize >= MAX_NODES {
            self.note_semantic(
                ordinal,
                3,
                ParseFailure::limited(
                    ParseFailureKind::NodeLimit,
                    ParseLimitScope::Nodes,
                    MAX_NODES as u64,
                    (MAX_NODES + 1) as u64,
                ),
            );
        }
        if self.node_ordinal as usize >= self.structural_limit {
            self.poison_tail();
        }
        if root && kind != 2 {
            self.note_semantic(
                ordinal,
                0,
                ParseFailure::plain(ParseFailureKind::RootMustBeMapping),
            );
        }
    }

    fn complete_child(&mut self, ordinal: u64) {
        let mut failure = None;
        if let Some(parent) = self.frames.last_mut() {
            if parent.kind == 1 {
                match parent.count.checked_add(1) {
                    Some(next) => {
                        parent.count = next;
                        if next as usize > MAX_SEQUENCE_ENTRIES {
                            failure = Some(ParseFailure::limited(
                                ParseFailureKind::SequenceEntryLimit,
                                ParseLimitScope::SequenceEntries,
                                MAX_SEQUENCE_ENTRIES as u64,
                                (MAX_SEQUENCE_ENTRIES + 1) as u64,
                            ));
                        }
                    }
                    _ => {
                        self.pending.tail_invariant = true;
                    }
                }
            } else if parent.mapping_value != 0 {
                parent.mapping_value = 0;
                match parent.count.checked_add(1) {
                    Some(next) => {
                        parent.count = next;
                        if next as usize > MAX_MAPPING_ENTRIES {
                            failure = Some(ParseFailure::limited(
                                ParseFailureKind::MappingEntryLimit,
                                ParseLimitScope::MappingEntries,
                                MAX_MAPPING_ENTRIES as u64,
                                (MAX_MAPPING_ENTRIES + 1) as u64,
                            ));
                        }
                    }
                    _ => {
                        self.pending.tail_invariant = true;
                    }
                }
            } else {
                parent.mapping_value = 1;
            }
        }
        if let Some(failure) = failure {
            self.note_semantic(ordinal, 4, failure);
        }
    }

    fn next_role(&self) -> (Option<u32>, NodeRole, u32, u8, u32) {
        let Some(parent) = self.frames.last() else {
            return (None, NodeRole::Root, u32::MAX, 0, 0);
        };
        if parent.kind == 1 {
            (
                Some(parent.ordinal),
                NodeRole::SequenceItem {
                    index: parent.count,
                },
                parent.ordinal,
                1,
                parent.count,
            )
        } else if parent.mapping_value != 0 {
            (
                Some(parent.ordinal),
                NodeRole::MappingValue {
                    index: parent.count,
                },
                parent.ordinal,
                3,
                parent.count,
            )
        } else {
            (
                Some(parent.ordinal),
                NodeRole::MappingKey {
                    index: parent.count,
                },
                parent.ordinal,
                2,
                parent.count,
            )
        }
    }

    fn compare_tuple_and_guards(
        &mut self,
        tuple: (u32, u32, u32, u8, u8),
        event_guard: Guard,
        event_ordinal: u64,
    ) -> Option<Guard> {
        if !self.prefix_proven {
            self.note_unpaired_event_guard(event_guard, event_ordinal);
            return None;
        }
        let Some(source) = self.presentation.descriptor_facts(self.source_cursor) else {
            self.break_prefix_and_visit_source();
            self.note_unpaired_event_guard(event_guard, event_ordinal);
            return None;
        };
        self.source_cursor += 1;
        self.charge_tail(1);
        if (source.0, source.1, source.2, source.3, source.4) != tuple {
            self.prefix_proven = false;
            self.note_unpaired_source_guard(guard_from_code(source.5), event_ordinal);
            self.visit_source_tail();
            self.note_unpaired_event_guard(event_guard, event_ordinal);
            return None;
        }
        let source_guard = guard_from_code(source.5);
        if source_guard == event_guard {
            Some(source_guard)
        } else if matches!(event_guard, Guard::Uninspectable)
            && matches!(source_guard, Guard::BuiltIn(_))
        {
            self.note_guard(event_ordinal);
            None
        } else if matches!(source_guard, Guard::Uninspectable)
            || matches!(event_guard, Guard::Uninspectable)
        {
            self.poison_tail();
            None
        } else {
            self.note_guard(event_ordinal);
            None
        }
    }

    fn inspect_scalar_restrictions(
        &mut self,
        ordinal: u64,
        matched_guard: Option<Guard>,
        analysis: &scalar::BorrowedScalarAnalysis,
        _style: ScalarPresentationStyle,
        _tag: SourceTag,
    ) {
        match matched_guard {
            Some(Guard::Merge) => {
                self.note_restriction(ordinal, 1, ParseFailureKind::MergeKeyDenied)
            }
            Some(Guard::BuiltIn(source_tag)) if !analysis.form_valid_for(source_tag) => {
                self.note_restriction(ordinal, 0, ParseFailureKind::ExplicitTagValueMismatch);
            }
            Some(Guard::BuiltIn(SourceTag::String)) | Some(Guard::None) | None
                if analysis.contains_noncharacter() =>
            {
                self.note_restriction(ordinal, 2, ParseFailureKind::InvalidUnicodeScalar);
            }
            _ => {}
        }
    }

    fn note_unpaired_source_guard(&mut self, guard: Guard, ordinal: u64) {
        if !matches!(guard, Guard::None) {
            self.note_guard(ordinal);
        }
    }

    fn note_unpaired_event_guard(&mut self, guard: Guard, ordinal: u64) {
        match guard {
            Guard::None => {}
            Guard::Uninspectable => self.poison_tail(),
            Guard::Merge | Guard::BuiltIn(_) | Guard::InvalidTag => self.note_guard(ordinal),
        }
    }

    fn break_prefix_and_visit_source(&mut self) {
        self.prefix_proven = false;
        self.visit_source_tail();
    }

    fn visit_source_tail(&mut self) {
        if self.source_tail_visited {
            return;
        }
        while let Some(source) = self.presentation.descriptor_facts(self.source_cursor) {
            self.source_cursor += 1;
            self.charge_tail(1);
            self.note_unpaired_source_guard(guard_from_code(source.5), self.event_ordinal);
        }
        self.source_tail_visited = true;
    }

    fn note_stream(&mut self, ordinal: u64, rank: u8, kind: ParseFailureKind) {
        note_event(
            &mut self.pending.stream,
            ordinal,
            rank,
            ParseFailure::plain(kind),
        );
        self.stream_state = StreamState::Broken;
        self.clear_success();
    }

    fn note_guard(&mut self, ordinal: u64) {
        note_event(
            &mut self.pending.guard,
            ordinal,
            0,
            ParseFailure::plain(ParseFailureKind::ParserTagDisagreement),
        );
        self.clear_success();
    }

    fn note_restriction(&mut self, ordinal: u64, rank: u8, kind: ParseFailureKind) {
        note_event(
            &mut self.pending.restriction,
            ordinal,
            rank,
            ParseFailure::plain(kind),
        );
        self.clear_success();
    }

    fn note_semantic(&mut self, ordinal: u64, rank: u8, failure: ParseFailure) {
        note_event(&mut self.pending.semantic, ordinal, rank, failure);
        self.clear_success();
    }

    fn poison_tail(&mut self) {
        self.pending.tail_invariant = true;
        self.text_meter.disabled = true;
        self.clear_success();
    }

    fn clear_success(&mut self) {
        self.nodes.clear();
        self.build.clear();
        self.retained_node_events = 0;
        self.retained_end_events = 0;
        self.semantic_decoded = 0;
    }

    fn can_retain(&self) -> bool {
        self.pending.scan.is_none()
            && self.pending.stream.is_none()
            && self.pending.guard.is_none()
            && self.pending.restriction.is_none()
            && self.pending.semantic.is_none()
            && !self.pending.tail_invariant
    }

    fn advance_node_ordinal(&mut self) {
        if let Some(next) = self.node_ordinal.checked_add(1) {
            self.node_ordinal = next;
        } else {
            self.poison_tail();
        }
    }

    fn charge_tail(&mut self, amount: u64) {
        if let Some(next) = self.tail_work.checked_add(amount) {
            self.tail_work = next;
        } else {
            self.poison_tail();
        }
    }
}

fn note_event(slot: &mut Option<EventFailure>, ordinal: u64, rank: u8, failure: ParseFailure) {
    if slot
        .as_ref()
        .is_none_or(|current| (ordinal, rank) < (current.ordinal, current.rank))
    {
        *slot = Some(EventFailure {
            ordinal,
            rank,
            failure,
        });
    }
}

pub(super) fn parse(
    text: &str,
    presentation: &Presentation,
) -> Result<SemanticTrace, ParseFailure> {
    let mut reducer = Reducer::new(text.len(), presentation)?;
    let mut parser = Parser::new_from_str(text).keep_tags(false);
    loop {
        if !reducer.before_call() {
            break;
        }
        let Some(next) = parser.next_event() else {
            break;
        };
        match next {
            Ok((event, _)) => reducer.accept(event),
            Err(_) => {
                reducer.scan_error();
                break;
            }
        }
    }
    drop(parser);
    let (nodes, build) = reducer.finish_drain()?;
    Ok(SemanticTrace { nodes, build })
}

impl SemanticTrace {
    pub(super) fn fold(
        &mut self,
        _permit: FoldPermit,
    ) -> Result<UnknownMetadataDocument, ParseFailure> {
        let build = core::mem::take(&mut self.build);
        #[cfg(test)]
        {
            fold_inner(&self.nodes, build, FoldControl::production())
        }
        #[cfg(not(test))]
        {
            fold_inner(&self.nodes, build)
        }
    }
}

#[cfg(test)]
fn fold_for_test(
    nodes: &[SemanticNode],
    build: Vec<BuildEvent>,
) -> Result<UnknownMetadataDocument, ParseFailure> {
    fold_inner(nodes, build, FoldControl::test())
}

#[cfg(test)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum FoldMode {
    Production,
    Test,
}

#[cfg(test)]
struct FoldFault {
    index: usize,
    meter: ActiveMeter,
    used: usize,
}

#[cfg(test)]
struct FoldControl {
    corrupt_mirror_after: Option<usize>,
    meter_fault: Option<FoldFault>,
    mode: FoldMode,
}

#[cfg(test)]
impl FoldControl {
    fn production() -> Self {
        Self {
            corrupt_mirror_after: None,
            meter_fault: None,
            mode: FoldMode::Production,
        }
    }

    fn test() -> Self {
        Self {
            corrupt_mirror_after: None,
            meter_fault: None,
            mode: FoldMode::Test,
        }
    }
}

fn fold_inner(
    nodes: &[SemanticNode],
    build: Vec<BuildEvent>,
    #[cfg(test)] control: FoldControl,
) -> Result<UnknownMetadataDocument, ParseFailure> {
    let mut builder = UnknownYamlDocumentBuilder::new();
    let mut numeric = NumericMeter::new();
    let mut mirror = FoldMirror::new()?;
    let mut scalar_cursor = 0usize;
    #[cfg(test)]
    let semantic_bytes = nodes
        .iter()
        .try_fold(0usize, |total, node| {
            total.checked_add(node.decoded.as_ref().map_or(0, |value| value.len()))
        })
        .ok_or_else(invariant)?;
    #[cfg(test)]
    let mut builder_bytes = 0usize;
    #[cfg(test)]
    let mut checked_copies = 0usize;
    #[cfg(test)]
    let mut first_builder_push = true;
    for (index, retained) in build.into_iter().enumerate() {
        let event = match retained {
            BuildEvent::BeginSequence => UnknownYamlEvent::BeginSequence,
            BuildEvent::EndSequence => UnknownYamlEvent::EndSequence,
            BuildEvent::BeginMapping => UnknownYamlEvent::BeginMapping,
            BuildEvent::EndMapping => UnknownYamlEvent::EndMapping,
            BuildEvent::Scalar {
                node_index,
                style,
                tag,
            } => {
                while nodes
                    .get(scalar_cursor)
                    .is_some_and(|node| node.decoded.is_none())
                {
                    scalar_cursor = scalar_cursor.checked_add(1).ok_or_else(invariant)?;
                }
                if node_index != scalar_cursor {
                    return Err(invariant());
                }
                let source = nodes
                    .get(node_index)
                    .and_then(|node| node.decoded.as_ref())
                    .ok_or_else(invariant)?;
                let mut text = String::new();
                text.try_reserve_exact(source.len())
                    .map_err(|_| invariant())?;
                text.push_str(source);
                #[cfg(test)]
                {
                    checked_copies = checked_copies.checked_add(1).ok_or_else(invariant)?;
                    let peak = semantic_bytes
                        .checked_add(builder_bytes)
                        .and_then(|value| value.checked_add(text.len()))
                        .ok_or_else(invariant)?;
                    FOLD_OBSERVATION.with(|observation| {
                        let (_, copies, current_peak) = observation.get();
                        observation.set((
                            control.mode == FoldMode::Production,
                            copies,
                            current_peak.max(peak),
                        ));
                    });
                }
                scalar_cursor = scalar_cursor.checked_add(1).ok_or_else(invariant)?;
                scalar::resolve(text, style, tag, &mut numeric)?
            }
        };
        mirror.accept(&event)?;
        #[cfg(test)]
        let active = if control
            .meter_fault
            .as_ref()
            .is_some_and(|fault| fault.index == index)
        {
            let fault = control.meter_fault.as_ref().expect("matched above");
            builder
                .set_test_meter(test_meter(fault.meter), fault.used)
                .map_err(|_| invariant())?;
            Some(fault.meter)
        } else {
            None
        };
        #[cfg(not(test))]
        let active = None;
        #[cfg(test)]
        if first_builder_push {
            FOLD_OBSERVATION.with(|observation| {
                let (_, copies, peak) = observation.get();
                observation.set((control.mode == FoldMode::Production, copies, peak));
            });
            first_builder_push = false;
        }
        #[cfg(test)]
        let event_string_len = match &event {
            UnknownYamlEvent::String(value) => value.len(),
            _ => 0,
        };
        if let Err(error) = builder.push_event(event) {
            let kind = error.kind();
            #[cfg(test)]
            if active.is_some() {
                let follow_up = builder
                    .push_event(UnknownYamlEvent::Null)
                    .expect_err("builder meter failure must poison")
                    .kind();
                if follow_up != UnknownYamlBuildErrorKind::Poisoned {
                    return Err(invariant());
                }
            }
            return Err(map_builder(kind, active));
        }
        #[cfg(test)]
        if active.is_some() {
            METER_EQUALITY_ACCEPTED.with(|observation| observation.set(true));
        }
        #[cfg(test)]
        {
            builder_bytes = builder_bytes
                .checked_add(event_string_len)
                .ok_or_else(invariant)?;
        }
        #[cfg(test)]
        if control.corrupt_mirror_after == Some(index) {
            let _before = mirror.events;
            mirror.events = mirror.events.checked_add(1).ok_or_else(invariant)?;
            MIRROR_CORRUPTION.with(|observation| observation.set((_before, mirror.events)));
        }
        let expected_events = index.checked_add(1).ok_or_else(invariant)?;
        if mirror.events != expected_events {
            return Err(invariant());
        }
    }
    if nodes
        .get(scalar_cursor..)
        .ok_or_else(invariant)?
        .iter()
        .any(|node| node.decoded.is_some())
    {
        return Err(invariant());
    }
    if !mirror.frames.is_empty() {
        return Err(invariant());
    }
    builder
        .finish_document()
        .map_err(|error| map_builder(error.kind(), None))
        .inspect(|_| {
            #[cfg(test)]
            FOLD_OBSERVATION.with(|observation| {
                let (barrier, _, peak) = observation.get();
                observation.set((barrier, checked_copies, peak));
            });
        })
}

#[cfg(test)]
pub(super) fn reset_fold_observation_for_test() {
    FOLD_OBSERVATION.with(|observation| observation.set((false, 0, 0)));
}

#[cfg(test)]
pub(super) fn fold_observation_for_test() -> (bool, usize, usize) {
    FOLD_OBSERVATION.with(Cell::get)
}

#[cfg(test)]
fn test_meter(meter: ActiveMeter) -> UnknownYamlTestMeter {
    match meter {
        ActiveMeter::Nodes => UnknownYamlTestMeter::Nodes,
        ActiveMeter::NumericDigits => UnknownYamlTestMeter::NumericDigits,
        ActiveMeter::SemanticEncoding => UnknownYamlTestMeter::SemanticEncoding,
        ActiveMeter::SemanticComparison => UnknownYamlTestMeter::SemanticComparison,
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "production folds never activate a fault meter; only fold fault-injection tests construct these"
    )
)]
enum ActiveMeter {
    Nodes,
    NumericDigits,
    SemanticEncoding,
    SemanticComparison,
}

struct FoldFrame {
    mapping: bool,
    count: usize,
    mapping_value: bool,
}

struct FoldMirror {
    events: usize,
    nodes: usize,
    numeric_digits: usize,
    frames: Vec<FoldFrame>,
}

impl FoldMirror {
    fn new() -> Result<Self, ParseFailure> {
        let mut frames = Vec::new();
        MAX_DEPTH
            .checked_mul(core::mem::size_of::<FoldFrame>())
            .ok_or_else(invariant)?;
        frames
            .try_reserve_exact(MAX_DEPTH)
            .map_err(|_| invariant())?;
        Ok(Self {
            events: 0,
            nodes: 0,
            numeric_digits: 0,
            frames,
        })
    }

    fn accept(&mut self, event: &UnknownYamlEvent) -> Result<(), ParseFailure> {
        self.events = self.events.checked_add(1).ok_or_else(invariant)?;
        match event {
            UnknownYamlEvent::BeginSequence | UnknownYamlEvent::BeginMapping => {
                self.nodes = self.nodes.checked_add(1).ok_or_else(invariant)?;
                let next_depth = self.frames.len().checked_add(1).ok_or_else(invariant)?;
                if self.nodes > MAX_NODES || next_depth > MAX_DEPTH {
                    return Err(invariant());
                }
                self.frames.push(FoldFrame {
                    mapping: matches!(event, UnknownYamlEvent::BeginMapping),
                    count: 0,
                    mapping_value: false,
                });
                Ok(())
            }
            UnknownYamlEvent::EndSequence | UnknownYamlEvent::EndMapping => {
                let mapping = matches!(event, UnknownYamlEvent::EndMapping);
                let frame = self.frames.pop().ok_or_else(invariant)?;
                if frame.mapping != mapping || (mapping && frame.mapping_value) {
                    return Err(invariant());
                }
                self.complete_child()?;
                Ok(())
            }
            UnknownYamlEvent::Integer(value) => {
                self.scalar(value.digits().len())?;
                Ok(())
            }
            UnknownYamlEvent::Float(value) => {
                let digits = value
                    .coefficient_digits()
                    .map_or(0, str::len)
                    .checked_add(value.exponent10().map_or(0, |value| value.digits().len()))
                    .ok_or_else(invariant)?;
                self.scalar(digits)?;
                Ok(())
            }
            UnknownYamlEvent::Null | UnknownYamlEvent::Boolean(_) | UnknownYamlEvent::String(_) => {
                self.scalar(0)?;
                Ok(())
            }
        }
    }

    fn scalar(&mut self, digits: usize) -> Result<(), ParseFailure> {
        self.nodes = self.nodes.checked_add(1).ok_or_else(invariant)?;
        self.numeric_digits = self
            .numeric_digits
            .checked_add(digits)
            .ok_or_else(invariant)?;
        if self.nodes > MAX_NODES || self.numeric_digits > MAX_NUMERIC_DIGITS as usize {
            return Err(invariant());
        }
        self.complete_child()
    }

    fn complete_child(&mut self) -> Result<(), ParseFailure> {
        let Some(frame) = self.frames.last_mut() else {
            return Ok(());
        };
        if !frame.mapping {
            frame.count = frame.count.checked_add(1).ok_or_else(invariant)?;
            if frame.count > MAX_SEQUENCE_ENTRIES {
                return Err(invariant());
            }
        } else if frame.mapping_value {
            frame.mapping_value = false;
            frame.count = frame.count.checked_add(1).ok_or_else(invariant)?;
            if frame.count > MAX_MAPPING_ENTRIES {
                return Err(invariant());
            }
        } else {
            frame.mapping_value = true;
        }
        Ok(())
    }
}

fn map_builder(kind: UnknownYamlBuildErrorKind, active: Option<ActiveMeter>) -> ParseFailure {
    use ParseFailureKind as P;
    use ParseLimitScope as S;
    use UnknownYamlBuildErrorKind as B;
    match kind {
        B::Poisoned => ParseFailure::plain(P::SemanticBuilderInvariant),
        B::RootMustBeMapping => ParseFailure::plain(P::RootMustBeMapping),
        B::MultipleRoots => ParseFailure::plain(P::MultipleDocuments),
        B::UnexpectedEnd | B::MismatchedEnd | B::IncompleteMappingEntry | B::IncompleteDocument => {
            ParseFailure::plain(P::SemanticStreamOrder)
        }
        B::TopLevelKeyMustBeString => ParseFailure::plain(P::TopLevelKeyMustBeString),
        B::DuplicateMappingKey => ParseFailure::plain(P::DuplicateMappingKey),
        B::InvalidString => ParseFailure::plain(P::InvalidUnicodeScalar),
        B::DepthLimit => limited(P::DepthLimit, S::Depth, MAX_DEPTH as u64),
        B::MappingEntryLimit => limited(
            P::MappingEntryLimit,
            S::MappingEntries,
            MAX_MAPPING_ENTRIES as u64,
        ),
        B::SequenceEntryLimit => limited(
            P::SequenceEntryLimit,
            S::SequenceEntries,
            MAX_SEQUENCE_ENTRIES as u64,
        ),
        B::NodeLimit => limited(P::NodeLimit, S::Nodes, MAX_NODES as u64),
        B::ScalarLimit => limited(
            P::CanonicalScalarLimit,
            S::CanonicalScalarBytes,
            MAX_SCALAR as u64,
        ),
        B::NumericDigitLimit => limited(P::NumericDigitLimit, S::NumericDigits, MAX_NUMERIC_DIGITS),
        B::SemanticEncodingLimit => limited(
            P::SemanticEncodingLimit,
            S::SemanticEncodingBytes,
            MAX_SEMANTIC_ENCODING,
        ),
        B::SemanticComparisonLimit => limited(
            P::SemanticComparisonLimit,
            S::SemanticComparisonBytes,
            MAX_SEMANTIC_COMPARISON,
        ),
        B::ArithmeticOverflow => match active {
            Some(ActiveMeter::Nodes) => overflow(S::Nodes, MAX_NODES as u64),
            Some(ActiveMeter::NumericDigits) => overflow(S::NumericDigits, MAX_NUMERIC_DIGITS),
            Some(ActiveMeter::SemanticEncoding) => {
                overflow(S::SemanticEncodingBytes, MAX_SEMANTIC_ENCODING)
            }
            Some(ActiveMeter::SemanticComparison) => {
                overflow(S::SemanticComparisonBytes, MAX_SEMANTIC_COMPARISON)
            }
            None => ParseFailure::plain(P::SemanticBuilderInvariant),
        },
    }
}

fn limited(kind: ParseFailureKind, scope: ParseLimitScope, limit: u64) -> ParseFailure {
    ParseFailure::limited(kind, scope, limit, limit + 1)
}

fn overflow(scope: ParseLimitScope, limit: u64) -> ParseFailure {
    ParseFailure::limited(
        ParseFailureKind::MeterArithmeticOverflow,
        scope,
        limit,
        u64::MAX,
    )
}

fn event_guard(
    tag: Option<&Tag>,
    scalar: bool,
    style: ScalarPresentationStyle,
    role: u8,
    analysis: Option<&scalar::BorrowedScalarAnalysis>,
) -> Guard {
    if let Some(tag) = tag {
        return exact_event_tag(tag).map_or(Guard::InvalidTag, Guard::BuiltIn);
    }
    if scalar && role == 2 && style == ScalarPresentationStyle::Plain {
        if analysis.is_none() {
            Guard::Uninspectable
        } else if analysis.is_some_and(scalar::BorrowedScalarAnalysis::is_merge) {
            Guard::Merge
        } else {
            Guard::None
        }
    } else {
        Guard::None
    }
}

fn exact_event_tag(tag: &Tag) -> Option<SourceTag> {
    const HANDLE: &[u8] = b"tag:yaml.org,2002:";
    if tag.handle.len() != HANDLE.len() || tag.handle.as_bytes() != HANDLE {
        return None;
    }
    match tag.suffix.as_bytes() {
        b"null" => Some(SourceTag::Null),
        b"bool" => Some(SourceTag::Boolean),
        b"int" => Some(SourceTag::Integer),
        b"float" => Some(SourceTag::Float),
        b"str" => Some(SourceTag::String),
        b"seq" => Some(SourceTag::Sequence),
        b"map" => Some(SourceTag::Mapping),
        _ => None,
    }
}

fn guard_from_code(code: u8) -> Guard {
    match code {
        0 => Guard::None,
        1 => Guard::Merge,
        2 => Guard::BuiltIn(SourceTag::Null),
        3 => Guard::BuiltIn(SourceTag::Boolean),
        4 => Guard::BuiltIn(SourceTag::Integer),
        5 => Guard::BuiltIn(SourceTag::Float),
        6 => Guard::BuiltIn(SourceTag::String),
        7 => Guard::BuiltIn(SourceTag::Sequence),
        8 => Guard::BuiltIn(SourceTag::Mapping),
        _ => Guard::InvalidTag,
    }
}

fn guard_source_tag(guard: Guard) -> SourceTag {
    match guard {
        Guard::BuiltIn(tag) => tag,
        Guard::None | Guard::Merge | Guard::Uninspectable | Guard::InvalidTag => {
            SourceTag::Implicit
        }
    }
}

fn map_style(style: SaphyrStyle) -> ScalarPresentationStyle {
    match style {
        SaphyrStyle::Plain => ScalarPresentationStyle::Plain,
        SaphyrStyle::SingleQuoted => ScalarPresentationStyle::SingleQuoted,
        SaphyrStyle::DoubleQuoted => ScalarPresentationStyle::DoubleQuoted,
        SaphyrStyle::Literal => ScalarPresentationStyle::Literal,
        SaphyrStyle::Folded => ScalarPresentationStyle::Folded,
    }
}

fn invariant() -> ParseFailure {
    ParseFailure::plain(ParseFailureKind::SemanticBuilderInvariant)
}

#[cfg(test)]
pub(super) fn borrowed_text_limit_for_test(yaml_len: usize, descriptors: usize) -> Option<u64> {
    borrowed_text_limit(yaml_len, descriptors).ok()
}

#[cfg(test)]
pub(super) fn borrowed_meter_for_test(limit: u64, seed: u64, texts: &[&str]) -> (u64, u64, bool) {
    let mut meter = BorrowedTextMeter {
        limit,
        used: seed,
        inspected: 0,
        disabled: false,
    };
    for text in texts {
        let _ = meter.inspect(text);
    }
    (meter.used, meter.inspected, meter.disabled)
}

#[cfg(test)]
pub(super) fn builder_overflow_scope_for_test(scope: u8) -> ParseLimitScope {
    let active = match scope {
        0 => ActiveMeter::Nodes,
        1 => ActiveMeter::NumericDigits,
        2 => ActiveMeter::SemanticEncoding,
        _ => ActiveMeter::SemanticComparison,
    };
    map_builder(UnknownYamlBuildErrorKind::ArithmeticOverflow, Some(active))
        .limit()
        .expect("overflow has limit facts")
        .scope()
}

#[cfg(test)]
pub(super) fn production_builder_overflow_for_test(
    scope: u8,
) -> (ParseFailureKind, Option<ParseLimitScope>) {
    let meter = match scope {
        0 => ActiveMeter::Nodes,
        1 => ActiveMeter::NumericDigits,
        2 => ActiveMeter::SemanticEncoding,
        _ => ActiveMeter::SemanticComparison,
    };
    let mut nodes = Vec::new();
    let (build, index) = match meter {
        ActiveMeter::Nodes => (vec![BuildEvent::BeginMapping], 0),
        ActiveMeter::SemanticEncoding => {
            nodes.push(test_scalar_node("key"));
            (
                vec![
                    BuildEvent::BeginMapping,
                    BuildEvent::Scalar {
                        node_index: 0,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::String,
                    },
                ],
                1,
            )
        }
        ActiveMeter::NumericDigits => {
            nodes.push(test_scalar_node("key"));
            nodes.push(test_scalar_node("1"));
            (
                vec![
                    BuildEvent::BeginMapping,
                    BuildEvent::Scalar {
                        node_index: 0,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::String,
                    },
                    BuildEvent::Scalar {
                        node_index: 1,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::Integer,
                    },
                ],
                2,
            )
        }
        ActiveMeter::SemanticComparison => {
            for text in ["a", "v", "b", "w"] {
                nodes.push(test_scalar_node(text));
            }
            let mut build = vec![BuildEvent::BeginMapping];
            for node_index in 0..4 {
                build.push(BuildEvent::Scalar {
                    node_index,
                    style: ScalarPresentationStyle::Plain,
                    tag: SourceTag::String,
                });
            }
            build.push(BuildEvent::EndMapping);
            (build, 5)
        }
    };
    let failure = match fold_inner(
        &nodes,
        build,
        FoldControl {
            corrupt_mirror_after: None,
            meter_fault: Some(FoldFault {
                index,
                meter,
                used: usize::MAX,
            }),
            mode: FoldMode::Test,
        },
    ) {
        Err(failure) => failure,
        Ok(_) => panic!("injected real builder meter unexpectedly accepted"),
    };
    (failure.kind(), failure.limit().map(|facts| facts.scope()))
}

#[cfg(test)]
pub(super) fn production_builder_limit_for_test(
    scope: u8,
) -> (ParseFailureKind, Option<ParseLimitScope>, bool) {
    let meter = match scope {
        0 => ActiveMeter::Nodes,
        1 => ActiveMeter::NumericDigits,
        2 => ActiveMeter::SemanticEncoding,
        _ => ActiveMeter::SemanticComparison,
    };
    let mut nodes = Vec::new();
    let (build, index, used) = match meter {
        ActiveMeter::Nodes => {
            nodes.push(test_scalar_node("a"));
            (
                vec![
                    BuildEvent::BeginMapping,
                    BuildEvent::Scalar {
                        node_index: 0,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::String,
                    },
                ],
                0,
                MAX_NODES - 1,
            )
        }
        ActiveMeter::NumericDigits => {
            for text in ["a", "1", "b", "1"] {
                nodes.push(test_scalar_node(text));
            }
            (
                vec![
                    BuildEvent::BeginMapping,
                    BuildEvent::Scalar {
                        node_index: 0,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::String,
                    },
                    BuildEvent::Scalar {
                        node_index: 1,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::Integer,
                    },
                    BuildEvent::Scalar {
                        node_index: 2,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::String,
                    },
                    BuildEvent::Scalar {
                        node_index: 3,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::Integer,
                    },
                ],
                2,
                MAX_NUMERIC_DIGITS as usize - 1,
            )
        }
        ActiveMeter::SemanticEncoding => {
            for text in ["a", "b"] {
                nodes.push(test_scalar_node(text));
            }
            (
                vec![
                    BuildEvent::BeginMapping,
                    BuildEvent::Scalar {
                        node_index: 0,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::String,
                    },
                    BuildEvent::Scalar {
                        node_index: 1,
                        style: ScalarPresentationStyle::Plain,
                        tag: SourceTag::String,
                    },
                ],
                1,
                MAX_SEMANTIC_ENCODING as usize - 10,
            )
        }
        ActiveMeter::SemanticComparison => {
            for text in [
                "first", "a", "null", "b", "null", "second", "a", "null", "b", "null",
            ] {
                nodes.push(test_scalar_node(text));
            }
            let mut build = vec![
                BuildEvent::BeginMapping,
                BuildEvent::Scalar {
                    node_index: 0,
                    style: ScalarPresentationStyle::Plain,
                    tag: SourceTag::String,
                },
                BuildEvent::BeginMapping,
            ];
            for node_index in 1..5 {
                build.push(BuildEvent::Scalar {
                    node_index,
                    style: ScalarPresentationStyle::Plain,
                    tag: if (node_index - 1) % 2 == 0 {
                        SourceTag::String
                    } else {
                        SourceTag::Null
                    },
                });
            }
            build.push(BuildEvent::EndMapping);
            build.push(BuildEvent::Scalar {
                node_index: 5,
                style: ScalarPresentationStyle::Plain,
                tag: SourceTag::String,
            });
            build.push(BuildEvent::BeginMapping);
            for node_index in 6..10 {
                build.push(BuildEvent::Scalar {
                    node_index,
                    style: ScalarPresentationStyle::Plain,
                    tag: if (node_index - 6) % 2 == 0 {
                        SourceTag::String
                    } else {
                        SourceTag::Null
                    },
                });
            }
            build.push(BuildEvent::EndMapping);
            (build, 7, MAX_SEMANTIC_COMPARISON as usize - 40)
        }
    };
    METER_EQUALITY_ACCEPTED.with(|observation| observation.set(false));
    let failure = match fold_inner(
        &nodes,
        build,
        FoldControl {
            corrupt_mirror_after: None,
            meter_fault: Some(FoldFault { index, meter, used }),
            mode: FoldMode::Test,
        },
    ) {
        Err(failure) => failure,
        Ok(_) => panic!("real builder meter one-over was unexpectedly accepted"),
    };
    (
        failure.kind(),
        failure.limit().map(|facts| facts.scope()),
        METER_EQUALITY_ACCEPTED.with(Cell::get),
    )
}

#[cfg(test)]
fn test_scalar_node(text: &str) -> SemanticNode {
    SemanticNode {
        ordinal: 0,
        parent: Some(0),
        role: NodeRole::MappingKey { index: 0 },
        kind: ReconciledNodeKind::Scalar,
        decoded: Some(Box::<str>::from(text)),
        scalar_style: Some(ScalarPresentationStyle::Plain),
        source_tag: SourceTag::String,
        child_count: 0,
    }
}

#[cfg(test)]
pub(super) fn fold_scalar_index_fault_for_test(case: u8) -> ParseFailureKind {
    let nodes = vec![
        SemanticNode {
            ordinal: 0,
            parent: None,
            role: NodeRole::Root,
            kind: ReconciledNodeKind::Mapping,
            decoded: None,
            scalar_style: None,
            source_tag: SourceTag::Implicit,
            child_count: 2,
        },
        SemanticNode {
            ordinal: 1,
            ..test_scalar_node("key")
        },
        SemanticNode {
            ordinal: 2,
            role: NodeRole::MappingValue { index: 0 },
            ..test_scalar_node("value")
        },
    ];
    let scalar = |node_index| BuildEvent::Scalar {
        node_index,
        style: ScalarPresentationStyle::Plain,
        tag: SourceTag::String,
    };
    let build = match case {
        0 => vec![
            BuildEvent::BeginMapping,
            scalar(1),
            scalar(1),
            BuildEvent::EndMapping,
        ],
        1 => vec![BuildEvent::BeginMapping, scalar(1), BuildEvent::EndMapping],
        2 => vec![
            BuildEvent::BeginMapping,
            scalar(2),
            scalar(1),
            BuildEvent::EndMapping,
        ],
        _ => vec![
            BuildEvent::BeginMapping,
            scalar(99),
            scalar(2),
            BuildEvent::EndMapping,
        ],
    };
    match fold_inner(&nodes, build, FoldControl::test()) {
        Err(failure) => failure.kind(),
        Ok(_) => panic!("invalid scalar index trace unexpectedly folded"),
    }
}

#[cfg(test)]
pub(super) fn map_builder_for_test(
    kind: UnknownYamlBuildErrorKind,
) -> (ParseFailureKind, Option<ParseLimitScope>) {
    let active = (kind != UnknownYamlBuildErrorKind::ArithmeticOverflow)
        .then_some(ActiveMeter::SemanticEncoding);
    let failure = map_builder(kind, active);
    (failure.kind(), failure.limit().map(|facts| facts.scope()))
}

#[cfg(test)]
pub(super) fn mirror_builder_mismatch_for_test() -> (ParseFailureKind, usize, usize) {
    MIRROR_CORRUPTION.with(|observation| observation.set((0, 0)));
    let kind = match fold_inner(
        &[],
        vec![BuildEvent::BeginMapping],
        FoldControl {
            corrupt_mirror_after: Some(0),
            meter_fault: None,
            mode: FoldMode::Test,
        },
    ) {
        Err(failure) => failure.kind(),
        Ok(_) => panic!("forced mirror mismatch unexpectedly accepted"),
    };
    let (before, after) = MIRROR_CORRUPTION.with(Cell::get);
    (kind, before, after)
}

#[cfg(test)]
pub(super) enum ReducerTestEvent {
    StreamStart,
    DocumentStart(bool),
    DocumentEnd,
    StreamEnd,
    Scalar {
        text: String,
        style: ScalarPresentationStyle,
        tag: Option<SourceTag>,
        invalid_tag: bool,
    },
    SequenceStart,
    SequenceEnd,
    MappingStart,
    MappingEnd,
}

#[cfg(test)]
pub(super) struct ReducerTestResult {
    pub(super) failure: Option<ParseFailureKind>,
    pub(super) borrowed_used: u64,
    pub(super) inspected: u64,
    pub(super) disabled: bool,
    pub(super) retained_node_capacity: usize,
    pub(super) retained_nodes: usize,
}

#[cfg(test)]
pub(super) fn reduce_events_for_test(
    text_len: usize,
    presentation: &Presentation,
    events: Vec<ReducerTestEvent>,
    borrowed_seed: Option<u64>,
    scan_after_events: bool,
) -> ReducerTestResult {
    let mut reducer = Reducer::new(text_len, presentation).expect("test bounds are valid");
    if let Some(seed) = borrowed_seed {
        reducer.text_meter.used = seed;
    }
    for event in events {
        assert!(reducer.before_call(), "test event-call budget");
        reducer.accept(test_event(event));
    }
    if scan_after_events {
        assert!(reducer.before_call(), "test scan call budget");
        reducer.scan_error();
    } else {
        assert!(reducer.before_call(), "test final None call budget");
    }
    let borrowed_used = reducer.text_meter.used;
    let inspected = reducer.text_meter.inspected;
    let disabled = reducer.text_meter.disabled;
    let retained_node_capacity = reducer.nodes.capacity();
    let retained_nodes = reducer.nodes.len();
    let result = reducer
        .finish_drain()
        .and_then(|(nodes, build)| fold_for_test(&nodes, build));
    ReducerTestResult {
        failure: result.err().map(|failure| failure.kind()),
        borrowed_used,
        inspected,
        disabled,
        retained_node_capacity,
        retained_nodes,
    }
}

#[cfg(test)]
pub(super) fn borrowed_retention_inventory_for_test(
    text_len: usize,
    presentation: &Presentation,
    key: &str,
    value: &str,
    borrowed_seed: Option<u64>,
    force_reservation_failure: bool,
) -> (Option<ParseFailureKind>, u64, u64, usize, usize, usize) {
    let mut reducer = Reducer::new(text_len, presentation).expect("test bounds are valid");
    reducer.force_retained_reservation_failure = force_reservation_failure;
    if let Some(seed) = borrowed_seed {
        reducer.text_meter.used = seed;
    }
    for event in [
        Event::StreamStart,
        Event::DocumentStart(false),
        Event::MappingStart(0, None),
        Event::Scalar(Cow::Borrowed(key), SaphyrStyle::Plain, 0, None),
        Event::Scalar(Cow::Borrowed(value), SaphyrStyle::Plain, 0, None),
        Event::MappingEnd,
        Event::DocumentEnd,
        Event::StreamEnd,
    ] {
        assert!(reducer.before_call(), "test event-call budget");
        reducer.accept(event);
    }
    assert!(reducer.before_call(), "test final None call budget");
    let visits = reducer.source_byte_visits;
    let inspected = reducer.text_meter.inspected;
    let retained = reducer.semantic_decoded;
    let copy_attempts = reducer.retained_copy_attempts;
    let copy_bytes = reducer.retained_copy_bytes;
    let result = reducer
        .finish_drain()
        .and_then(|(nodes, build)| fold_for_test(&nodes, build));
    (
        result.err().map(|failure| failure.kind()),
        visits,
        inspected,
        retained,
        copy_attempts,
        copy_bytes,
    )
}

#[cfg(test)]
pub(super) fn forced_borrowed_limit_failure_for_test(
    presentation: &Presentation,
    events: Vec<ReducerTestEvent>,
    scan_after_events: bool,
) -> ParseFailureKind {
    let mut reducer = Reducer::new(0, presentation).expect("zero-sized test bounds are valid");
    reducer.text_meter.disabled = true;
    reducer.pending.tail_invariant = true;
    for event in events {
        if !reducer.before_call() {
            break;
        }
        reducer.accept(test_event(event));
    }
    if scan_after_events && reducer.before_call() {
        reducer.scan_error();
    }
    match reducer.finish_drain() {
        Err(failure) => failure.kind(),
        Ok(_) => panic!("forced borrowed-limit arithmetic failure unexpectedly accepted"),
    }
}

#[cfg(test)]
pub(super) fn production_copy_inventory_for_test(
    text: &str,
    presentation: &Presentation,
) -> (usize, usize, usize, u64, u64) {
    let presentation_decoded = presentation
        .nodes
        .iter()
        .filter_map(|node| node.decoded.as_ref())
        .map(String::len)
        .sum();
    let presentation_raw = presentation
        .nodes
        .iter()
        .filter_map(|node| node.raw.as_ref())
        .map(String::len)
        .sum();
    let mut reducer = Reducer::new(text.len(), presentation).expect("production bounds are valid");
    let mut parser = Parser::new_from_str(text).keep_tags(false);
    while reducer.before_call() {
        let Some(next) = parser.next_event() else {
            break;
        };
        match next {
            Ok((event, _)) => reducer.accept(event),
            Err(_) => {
                reducer.scan_error();
                break;
            }
        }
    }
    let semantic_decoded = reducer.semantic_decoded;
    let source_byte_visits = reducer.source_byte_visits;
    let borrowed_limit = reducer.text_meter.limit;
    let (nodes, build) = reducer
        .finish_drain()
        .expect("inventory source must reduce");
    fold_for_test(&nodes, build).expect("inventory source must fold");
    (
        presentation_decoded,
        semantic_decoded,
        presentation_raw,
        source_byte_visits,
        borrowed_limit,
    )
}

#[cfg(test)]
pub(super) fn force_event_call_bound_for_test(
    text_len: usize,
    presentation: &Presentation,
    events: Vec<ReducerTestEvent>,
) -> ParseFailureKind {
    let mut reducer = Reducer::new(text_len, presentation).expect("test bounds are valid");
    for event in events {
        assert!(reducer.before_call(), "test event-call budget");
        reducer.accept(test_event(event));
    }
    reducer.event_call_limit = reducer.event_calls;
    assert!(
        !reducer.before_call(),
        "forced event-call bound must stop calls"
    );
    match reducer.finish_drain() {
        Err(failure) => failure.kind(),
        Ok(_) => panic!("event-call invariant unexpectedly accepted"),
    }
}

#[cfg(test)]
pub(super) fn collection_key_depth_precedence_for_test(
    presentation: &Presentation,
    mapping: bool,
) -> ParseFailureKind {
    let mut reducer = Reducer::new(8, presentation).expect("test bounds are valid");
    reducer.stream_state = StreamState::InDocument;
    reducer.saw_root = true;
    reducer.prefix_proven = false;
    reducer.source_cursor = presentation.descriptor_count();
    reducer.source_tail_visited = true;
    for _ in 0..MAX_DEPTH {
        reducer.frames.push(Frame {
            ordinal: 0,
            count: 0,
            kind: 2,
            mapping_value: 0,
            reserved: [0; 6],
        });
    }
    reducer.collection_start(0, mapping, 0, None);
    reducer
        .pending
        .semantic
        .expect("combined top-key/depth fault must be recorded")
        .failure
        .kind()
}

#[cfg(test)]
pub(super) fn adjacent_semantic_precedence_for_test(
    presentation: &Presentation,
    stage: u8,
) -> ParseFailureKind {
    let mut reducer = Reducer::new(8, presentation).expect("test bounds are valid");
    reducer.stream_state = StreamState::ExpectNone;
    reducer.saw_root = true;
    reducer.prefix_proven = false;
    reducer.source_cursor = presentation.descriptor_count();
    reducer.source_tail_visited = true;
    match stage {
        9 => {
            reducer.scan_error();
            reducer.note_stream(1, 0, ParseFailureKind::SemanticStreamOrder);
        }
        10 => {
            reducer.note_stream(1, 0, ParseFailureKind::SemanticStreamOrder);
            reducer.note_guard(0);
        }
        11 => {
            reducer.note_guard(1);
            reducer.note_restriction(0, 0, ParseFailureKind::MergeKeyDenied);
        }
        12 => {
            reducer.note_restriction(1, 0, ParseFailureKind::MergeKeyDenied);
            reducer.note_semantic(0, 0, ParseFailure::plain(ParseFailureKind::DepthLimit));
        }
        _ => {
            reducer.note_semantic(
                0,
                0,
                ParseFailure::plain(ParseFailureKind::RootMustBeMapping),
            );
            reducer.build.push(BuildEvent::BeginMapping);
        }
    }
    match reducer
        .finish_drain()
        .and_then(|(nodes, build)| fold_for_test(&nodes, build))
    {
        Err(error) => error.kind(),
        Ok(_) => panic!("adjacent precedence fixture must fail"),
    }
}

#[cfg(test)]
pub(super) fn scan_fault_after_dependency_for_test(
    text: &str,
    presentation: &Presentation,
) -> ParseFailure {
    let _accepted = parse(text, presentation).expect("control source must parse before injection");
    let mut copied_reducer = Reducer::new(text.len(), presentation).expect("test bounds are valid");
    copied_reducer.scan_error();
    match copied_reducer.finish_drain() {
        Err(error) => error,
        Ok(_) => panic!("injected scan failure unexpectedly accepted"),
    }
}

#[cfg(test)]
fn test_event(event: ReducerTestEvent) -> Event<'static> {
    use ReducerTestEvent as T;
    match event {
        T::StreamStart => Event::StreamStart,
        T::DocumentStart(explicit) => Event::DocumentStart(explicit),
        T::DocumentEnd => Event::DocumentEnd,
        T::StreamEnd => Event::StreamEnd,
        T::SequenceStart => Event::SequenceStart(0, None),
        T::SequenceEnd => Event::SequenceEnd,
        T::MappingStart => Event::MappingStart(0, None),
        T::MappingEnd => Event::MappingEnd,
        T::Scalar {
            text,
            style,
            tag,
            invalid_tag,
        } => Event::Scalar(
            Cow::Owned(text),
            test_style(style),
            0,
            if invalid_tag {
                Some(Cow::Owned(Tag {
                    handle: "!".to_owned(),
                    suffix: "hostile".to_owned(),
                }))
            } else {
                tag.map(|tag| {
                    Cow::Owned(Tag {
                        handle: "tag:yaml.org,2002:".to_owned(),
                        suffix: test_tag_suffix(tag).to_owned(),
                    })
                })
            },
        ),
    }
}

#[cfg(test)]
fn test_style(style: ScalarPresentationStyle) -> SaphyrStyle {
    match style {
        ScalarPresentationStyle::Plain => SaphyrStyle::Plain,
        ScalarPresentationStyle::SingleQuoted => SaphyrStyle::SingleQuoted,
        ScalarPresentationStyle::DoubleQuoted => SaphyrStyle::DoubleQuoted,
        ScalarPresentationStyle::Literal => SaphyrStyle::Literal,
        ScalarPresentationStyle::Folded => SaphyrStyle::Folded,
    }
}

#[cfg(test)]
fn test_tag_suffix(tag: SourceTag) -> &'static str {
    match tag {
        SourceTag::Null => "null",
        SourceTag::Boolean => "bool",
        SourceTag::Integer => "int",
        SourceTag::Float => "float",
        SourceTag::String => "str",
        SourceTag::Sequence => "seq",
        SourceTag::Mapping => "map",
        SourceTag::Implicit => "invalid",
    }
}

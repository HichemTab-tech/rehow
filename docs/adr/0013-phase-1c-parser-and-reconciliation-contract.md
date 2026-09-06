# ADR 0013: Phase 1C Parser and Reconciliation Contract

- Status: Accepted — Architect/xhigh, Reviewer/high, Security Reviewer/xhigh, and feasibility closure passed 2026-09-06
- Date: 2026-09-06

## Context

ADR 0011 fixes the dual-parser strategy, source limits, accepted YAML subset,
semantic meters, and the rule that only a later typed binder may authorize an
edit. It intentionally does not define enough Rust API or parser-state detail
for a low-reasoning Phase 1C implementation packet. In particular, a Worker
must not decide artifact ownership, dependency-event interpretation, diagnostic
precedence, concrete-syntax classification, comment correspondence, or how a
parser disagreement is retained without granting mutation authority.

This ADR closes those gaps without reopening ADR 0011 or ADR 0012. Phase 1C is
still a generic, read-only content-format proof. It has no Workspace path,
filesystem, Git, Markdown-body parser, product-field binder, fixed-schema
emitter, span-edit planner, protocol `ErrorDto` factory, TypeScript contract, or
mutation authority.

Implementation is gated on a signed Phase 1B checkpoint whose actual diff has
fresh Reviewer/high and Security Reviewer/xhigh passes. If the accepted Phase
1B surface differs from ADR 0012, this proposal must be reconciled and reviewed
again before dispatch.

## Decision

### Dependencies and allowed dependency surface

Phase 1C adds exactly the three runtime dependencies already approved by ADR
0011:

```toml
saphyr-parser = "=0.0.12"
yaml-edit = { version = "=0.3.1", default-features = false }
rowan = "=0.16.1"
```

No feature is enabled on `saphyr-parser` or `rowan`. In particular,
`saphyr-parser/debug_prints` and `yaml-edit/binary` are absent. Phase 1C adds no
other runtime or development dependency. `saphyr-parser` 0.0.12 is `MIT OR
Apache-2.0` with Rust 1.85; `yaml-edit` 0.3.1 is `Apache-2.0` with Rust 1.70;
both are compatible with the project Rust 1.98 baseline. `yaml-edit` contains a
localized unsafe conversion from rowan's raw syntax kind to its closed
`SyntaxKind`; that unsafe and rowan's internal unsafe surface are dependency
risk, not Rehow authority, and are included in the pinned-source security
review. Phase 1C's own production modules contain no unsafe block; dependency
unsafe remains behind the parser boundary.

The `yaml-edit` call allowlist remains exactly ADR 0011's: bounded
`YamlFile::parse`, `Parse::positioned_errors`, `Parse::tree`,
`rowan::ast::AstNode::syntax`, read-only syntax node/token kind, text, range,
parent, child, and sibling traversal, plus `AstNode::cast`, `Scalar::value`, and
`Scalar::as_string` after an exact `SCALAR` kind check. No validator, path,
file, relaxed-parse, conversion, high-level navigation, builder, serializer,
mutation, green-node, splice, or scalar-set API is used. Only
`Parser::new_from_str`, `Parser::keep_tags(false)`, and `next_event` are used
from `saphyr-parser`; `load`, `peek`, custom inputs, receivers, scanners, and
marker/span slicing are forbidden.

The implementation evidence records the exact lockfile checksums, complete
normal and feature trees, duplicate versions, licenses, MSRV, crate-source
revisions, unsafe occurrences, current RustSec/OSV results, and before/after
release sizes. A yanked package, unresolved advisory, feature drift, or
different `yaml-edit`/rowan resolution blocks acceptance.

### Module boundary and exact entry API

The crate root declares this temporary, narrowly located lint allowance because
the Phase 1C sibling surface has no production consumer until Phase 1D:

```rust
#[allow(dead_code, reason = "Phase 1C parser is consumed beginning in Phase 1D")]
pub(crate) mod parsing;
```

No item inside `parsing` receives its own `dead_code` allowance and no other
lint is allowed. The module is not `pub`, is not re-exported by `rehow_core`,
and is unavailable to the desktop and MCP crates. The allowance must be
removed in the first Phase 1D commit that adds the typed binder; a source check
for its exact reason string is a Phase 1D gate. Its internal files are fixed
for Phase 1C:

```text
crates/rehow-core/src/parsing/mod.rs
crates/rehow-core/src/parsing/source.rs
crates/rehow-core/src/parsing/presentation.rs
crates/rehow-core/src/parsing/semantic.rs
crates/rehow-core/src/parsing/scalar.rs
crates/rehow-core/src/parsing/reconcile.rs
```

`presentation`, `semantic`, `scalar`, and `reconcile` are private child
modules. `parsing` re-exports to core siblings only the types and functions
listed in this ADR. There is no public parser trait, configurable parser,
dependency wrapper, or generic source-kind argument.

The only production entry points are:

```rust
pub(crate) fn parse_yaml_document(
    source: Box<[u8]>,
) -> Result<ParsedYamlDocument, ParseFailure>;

pub(crate) fn parse_guide_source(
    source: Box<[u8]>,
) -> Result<ParsedGuideSource, ParseFailure>;
```

The first entry point is shared by the later Manifest and Category binders. It
does not know which product record will consume it. The second recognizes only
the Guide envelope and returns the body as retained bytes; it does not parse
Markdown. Taking `Box<[u8]>` is deliberate: an accepted artifact owns the
caller's one bounded source allocation without a second full-file copy. Every
length cap is checked before UTF-8 conversion or either dependency allocation;
an oversized box is dropped immediately. Phase 1E must still bound reads before
allocating the box.

There is no borrowing lifetime in a result, no `unsafe` self-reference, and no
retained Saphyr, `yaml-edit`, rowan, `Cow`, `GreenNode`, `SyntaxNode`, or
`SyntaxToken`. Every dependency handle is dropped before either entry point
returns.

### Coordinate and source-envelope types

All offsets are checked UTF-8 byte offsets. Saphyr markers are never converted
to these types.

```rust
pub(crate) struct FileByteOffset(u32);
pub(crate) struct FileByteRange { start: FileByteOffset, end: FileByteOffset }
pub(crate) struct YamlByteOffset(u32);
pub(crate) struct YamlByteRange { start: YamlByteOffset, end: YamlByteOffset }
pub(crate) struct SourcePosition { line: u32, column: u32 }

pub(crate) enum SourceLineEnding { None, Lf, CrLf }

pub(crate) struct WholeYamlEnvelope {
    bom_present: bool,
    yaml_file_range: FileByteRange,
    line_ending: SourceLineEnding,
    final_newline: bool,
}

pub(crate) struct GuideEnvelope {
    bom_present: bool,
    opening_delimiter: FileByteRange,
    yaml_file_range: FileByteRange,
    closing_delimiter: FileByteRange,
    separator_line: FileByteRange,
    body_file_range: FileByteRange,
    yaml_line_ending: SourceLineEnding,
    final_newline: bool,
}
```

`SourceLineEnding::None` is possible only for a whole-YAML document with no
line break. A valid Guide envelope necessarily selects `Lf` or `CrLf`.
`final_newline` describes the complete retained file, not merely the YAML
slice. The Guide body begins immediately after the one consumed blank
separator line. Further blank lines are body bytes, so an empty or
blank-leading body remains unambiguous.

`FileByteOffset`, `YamlByteOffset`, both ranges, `SourcePosition`, and
`SourceLineEnding` implement exactly `Clone`, `Copy`, `Eq`, `PartialEq`, `Ord`,
`PartialOrd`, and `Hash`, plus a manual non-content `Debug`; they implement no
`Display`, `Default`, conversion, Serde, or TS trait. Their constructors and
fields are private. Their exact read surface is:

```rust
impl FileByteOffset { pub(crate) fn get(self) -> u32; }
impl YamlByteOffset { pub(crate) fn get(self) -> u32; }
impl FileByteRange {
    pub(crate) fn start(self) -> FileByteOffset;
    pub(crate) fn end(self) -> FileByteOffset;
    pub(crate) fn len(self) -> u32;
    pub(crate) fn is_empty(self) -> bool;
}
impl YamlByteRange {
    pub(crate) fn start(self) -> YamlByteOffset;
    pub(crate) fn end(self) -> YamlByteOffset;
    pub(crate) fn len(self) -> u32;
    pub(crate) fn is_empty(self) -> bool;
}
impl SourcePosition {
    pub(crate) fn line(self) -> u32;
    pub(crate) fn column(self) -> u32;
}
impl WholeYamlEnvelope {
    pub(crate) fn bom_present(&self) -> bool;
    pub(crate) fn yaml_file_range(&self) -> FileByteRange;
    pub(crate) fn line_ending(&self) -> SourceLineEnding;
    pub(crate) fn final_newline(&self) -> bool;
}
impl GuideEnvelope {
    pub(crate) fn bom_present(&self) -> bool;
    pub(crate) fn opening_delimiter(&self) -> FileByteRange;
    pub(crate) fn yaml_file_range(&self) -> FileByteRange;
    pub(crate) fn closing_delimiter(&self) -> FileByteRange;
    pub(crate) fn separator_line(&self) -> FileByteRange;
    pub(crate) fn body_file_range(&self) -> FileByteRange;
    pub(crate) fn yaml_line_ending(&self) -> SourceLineEnding;
    pub(crate) fn final_newline(&self) -> bool;
}
```

The two envelope structs implement exactly `Clone`, `Eq`, and `PartialEq` and
no `Copy`, `Debug`, `Display`, `Default`, hashing, ordering, Serde, or TS trait.

`SourcePosition` is one-based. It is computed only after the complete input has
validated as UTF-8: CRLF counts as one newline and each Unicode scalar,
including a tab, advances the column by one. `InvalidUtf8` always has
`position() == None`; `Utf8Error::valid_up_to`, dependency line/column values,
and Saphyr markers are never converted into a position. A dependency range
that is out of bounds, reversed, or not on UTF-8 boundaries also yields no
position. Byte fixtures cover invalid leading bytes, invalid bytes after LF
and CRLF, invalid continuation bytes, and an incomplete final sequence and
assert `None` for all of them.

### Exact immutable parse artifact

The two entry results own one exact source and one generic YAML artifact:

```rust
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

pub(crate) struct ParsedYamlArtifact {
    semantic: UnknownMetadataDocument,
    nodes: Box<[ReconciledNode]>,
    entries: Box<[ReconciledEntry]>,
    comments: Box<[CommentRecord]>,
    round_trip: RoundTripStatus,
}
```

The exact result accessors are:

```rust
impl ParsedYamlDocument {
    pub(crate) fn source_bytes(&self) -> &[u8];
    pub(crate) fn yaml_bytes(&self) -> &[u8];
    pub(crate) fn envelope(&self) -> &WholeYamlEnvelope;
    pub(crate) fn yaml(&self) -> &ParsedYamlArtifact;
}
impl ParsedGuideSource {
    pub(crate) fn source_bytes(&self) -> &[u8];
    pub(crate) fn yaml_bytes(&self) -> &[u8];
    pub(crate) fn body_bytes(&self) -> &[u8];
    pub(crate) fn envelope(&self) -> &GuideEnvelope;
    pub(crate) fn yaml(&self) -> &ParsedYamlArtifact;
}
impl ParsedYamlArtifact {
    pub(crate) fn semantic(&self) -> &UnknownMetadataDocument;
    pub(crate) fn root_id(&self) -> NodeId;
    pub(crate) fn nodes(&self) -> &[ReconciledNode];
    pub(crate) fn node(&self, id: NodeId) -> Option<&ReconciledNode>;
    pub(crate) fn entries(&self) -> &[ReconciledEntry];
    pub(crate) fn entry(&self, id: EntryId) -> Option<&ReconciledEntry>;
    pub(crate) fn comments(&self) -> &[CommentRecord];
    pub(crate) fn comment(&self, id: CommentId) -> Option<&CommentRecord>;
    pub(crate) fn round_trip_status(&self) -> &RoundTripStatus;
}
```

A successful parse always has exactly one root node and `root_id()` is the
privately constructed `NodeId(0)`. Slice accessors borrow the retained source
and cannot fail because envelope ranges were checked before construction. ID
lookups use checked conversion to `usize` and return `None` when absent.

The complete input is retained exactly once: at most 256 KiB for whole YAML or
2 MiB for a Guide, of which at most 256 KiB is YAML. Final artifact heap
inventory is bounded as follows; no line table, dependency object, or temporary
trace survives the return:

| Retained final allocation | Exact bound |
| --- | --- |
| source bytes, including a Guide body | 262,144 whole-YAML bytes or 2,097,152 Guide bytes |
| semantic values/collection storage | at most 20,000 values/collection nodes; total sequence items plus mapping key/value records is at most 20,000; mapping semantic-order indexes are at most the mapping-entry total |
| semantic `String` UTF-8 payloads | at most `ceil(3 * yaml_len / 2) + node_count` bytes in aggregate, derived from the maximum accepted YAML escape expansion plus synthetic empties |
| canonical integer/finite-float digit strings | at most 81,920 ASCII digits in aggregate and 65,536 for one canonical scalar |
| Phase 1B per-value canonical key-encoding vectors | at most 8,388,608 payload bytes in aggregate, as charged by the semantic-encoding meter |
| `ReconciledNode` records and child-ID arrays | at most 20,000 records and at most 19,999 child IDs |
| reconciled decoded scalar strings | at most `ceil(3 * yaml_len / 2) + node_count` bytes in aggregate |
| proven `ReconciledEntry` records/entry-ID arrays | at most 19,999 records and at most 19,999 entry IDs |
| `CommentRecord` records | at most `yaml_len` records, because each source comment has a nonempty `#` token |
| round-trip diagnostics | at most 32 retained `RoundTripIssue` records regardless of the checked total |

Semantic strings, numeric digit strings, cached canonical key encodings, and
reconciled decoded strings are distinct retained allocations and are not
described as one copy. Raw node/comment text is read by checked source ranges.
Record/vector allocator overhead is bounded by the stated counts and checked
`usize` multiplication before allocation; exact platform layout is asserted
with `size_of` evidence rather than treated as a portable byte constant. The
temporary two-decoder-copy/raw-value bounds are defined separately below.

`ParsedYamlDocument`, `ParsedGuideSource`, `ParsedYamlArtifact`,
`ReconciledNode`, `ReconciledEntry`, `CommentRecord`, `RoundTripIssue`,
`RoundTripReport`, and `RoundTripStatus` implement no `Clone`, `Copy`, `Debug`,
`Display`, equality, ordering, hashing, `Default`, Serde, TS, or owning
iterator. Immutability is by private fields and shared-reference accessors.

The small identifiers and vocabularies are crate-private:

```rust
pub(crate) struct NodeId(u32);
pub(crate) struct EntryId(u32);
pub(crate) struct CommentId(u32);

pub(crate) enum ReconciledNodeKind { Scalar, Sequence, Mapping, ImplicitEmpty }
pub(crate) enum NodeRole {
    Root,
    SequenceItem { index: u32 },
    MappingKey { index: u32 },
    MappingValue { index: u32 },
}
pub(crate) enum ScalarPresentationStyle {
    Plain, SingleQuoted, DoubleQuoted, Literal, Folded,
}
pub(crate) enum CollectionPresentationStyle { Block, Flow }
pub(crate) enum SourceTag {
    Implicit, Null, Boolean, Integer, Float, String, Sequence, Mapping,
}

pub(crate) enum ReconciledEntryKind {
    Mapping { key: NodeId, value: NodeId },
    Sequence { value: NodeId },
}
```

IDs are indexes into their respective boxed slices. Nodes are numbered in
Saphyr semantic encounter preorder: collection start is a node, followed by
each sequence child or mapping key then value recursively; a scalar is one
node. Explicit `TAGGED_NODE`, `DOCUMENT`, `KEY`, `VALUE`, and entry wrappers do
not consume a `NodeId`. Every node after the root has exactly one parent and
one `NodeRole`; each index is the zero-based encounter index in that parent.

The node and proven-entry records have these exact fields:

```rust
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
```

For an untagged concrete node, whole and payload ranges are equal. For an
explicit built-in tag, the whole range is the collapsed `TAGGED_NODE` and the
payload range is its scalar or collection child. An implicit empty has a site
but deliberately has no replacement range. `Unreconciled` is possible only in
an accepted read-only artifact at and after the first structural disagreement.
`decoded_scalar` is present only for a scalar/implicit-empty node and retains
Saphyr's decoded pre-resolution text; style fields are present only for their
matching node kind. `children` always uses semantic preorder IDs. `entries`
contains only proven presentation-entry IDs and can be a strict prefix or
empty on a collection in a read-only artifact.

`ReconciledEntry` exists only when its parent and every referenced child were
matched before the first structural disagreement. Entries are numbered
contiguously from zero in normalized presentation preorder. No later
presentation entry receives an `EntryId`, and no semantic `NodeId` is guessed
for it. It is correspondence evidence, not a removal range, insertion site, or
permission to edit.

```rust
impl ReconciledNode {
    pub(crate) fn id(&self) -> NodeId;
    pub(crate) fn parent(&self) -> Option<NodeId>;
    pub(crate) fn role(&self) -> NodeRole;
    pub(crate) fn kind(&self) -> ReconciledNodeKind;
    pub(crate) fn children(&self) -> &[NodeId];
    pub(crate) fn entries(&self) -> &[EntryId];
    pub(crate) fn decoded_scalar(&self) -> Option<&str>;
    pub(crate) fn scalar_style(&self) -> Option<ScalarPresentationStyle>;
    pub(crate) fn collection_style(&self) -> Option<CollectionPresentationStyle>;
    pub(crate) fn source_tag(&self) -> SourceTag;
    pub(crate) fn presentation(&self) -> NodePresentation;
}
impl ReconciledEntry {
    pub(crate) fn id(&self) -> EntryId;
    pub(crate) fn parent(&self) -> NodeId;
    pub(crate) fn index(&self) -> u32;
    pub(crate) fn yaml_range(&self) -> YamlByteRange;
    pub(crate) fn file_range(&self) -> FileByteRange;
    pub(crate) fn collection_style(&self) -> CollectionPresentationStyle;
    pub(crate) fn kind(&self) -> ReconciledEntryKind;
}
```

All IDs, `ReconciledNodeKind`, `NodeRole`, both presentation-style enums,
`SourceTag`, `ReconciledEntryKind`, and `NodePresentation` implement exactly
`Clone`, `Copy`, `Eq`, `PartialEq`, `Ord`, `PartialOrd`, and `Hash`, plus manual
non-content `Debug`. They have no struct constructor and implement no
`Display`, `Default`, conversion, Serde, or TS trait. No type in this section is
re-exported outside `rehow-core`.

### Byte scanner and envelope classification

The byte scanner operates before UTF-8 conversion and retains only fixed-size
offset/count/envelope facts; it allocates no content-proportional table.
Reporting precedence is fixed separately below. The temporary line table used
for comment ownership is built later, after a bounded YAML/envelope region is
known.

For whole YAML, reject length 262,145 and above before either parser. Consume
one optional exact UTF-8 BOM (`EF BB BF`) only at byte zero. The YAML parser
slice is every remaining byte. A raw BOM sequence anywhere in that slice is
recorded by the byte scanner and reported as `RawBomDenied` before either
parser. The slice must contain either only LF, only CRLF, or no newline; bare
CR and mixed LF/CRLF are malformed. `---` and `...` document marker lines and
all directives are later rejected from CST tokens.

For a Guide, reject length 2,097,153 and above. Validate the complete file as
UTF-8, but apply YAML newline and raw-BOM policy only from byte zero through the
separator line; Markdown-body line-ending policy belongs to the Markdown
phase. After the optional byte-zero BOM:

1. the opening line is exactly `---` plus LF or CRLF;
2. the first following column-zero line exactly equal to `---` is the closing
   delimiter; indented occurrences are YAML content;
3. byte zero through the required blank separator line, including BOM,
   delimiters, and line endings, may be at most 262,144 bytes;
4. the closing delimiter uses the selected line ending and is followed by one
   empty line using the same ending;
5. the YAML slice starts after the opening line and ends at the first byte of
   the closing delimiter; the body starts after the separator line.

The envelope scan records a raw BOM found before the body boundary and reports
`RawBomDenied` ahead of delimiter/newline failures according to the precedence
below. A raw U+FEFF encoded wholly in the Markdown body is not a Phase 1C YAML
failure. If no valid separator exists, there is no body boundary and the scan
ends at the frontmatter or whole-file cap.

If no closing delimiter occurs before the frontmatter cap and enough bytes
exist to cross it, return `FrontmatterTooLarge`, not
`MissingGuideClosingDelimiter`.
If EOF arrives below the cap, return `MissingGuideClosingDelimiter`. A closing
line without its following line ending or blank separator is
`MissingGuideSeparator`. An empty body is valid at this generic phase. The YAML
slice itself must still parse as exactly one mapping.

### Presentation adapter and exhaustive CST vocabulary

`YamlFile::parse` runs only after the envelope and UTF-8 gates. Positioned
errors are collected without retaining `message` or `code`. If any exist, the
tree is not examined semantically. A range is valid only when
`start <= end <= yaml_len` and both endpoints are UTF-8 boundaries. If at least
one error has a valid range, select the smallest `range.start`, then
`range.end`, then `ParseErrorKind` rank `UnclosedFlowSequence`,
`UnclosedFlowMapping`, `UnterminatedString`, `Other`, then original slice
index. Invalid-range errors cannot shadow a valid-range error. If every range
is invalid, select original slice index zero and return the same sanitized
`PresentationSyntax` with no position. Direct adapter tests cover multiple
valid ties, invalid-before-valid, valid-before-invalid, and multiple invalid
ranges. No error message or code participates.

The adapter then performs one iterative depth-first walk. Rowan recursion is
not used in Rehow code. The only accepted node kinds and normalized meanings
are:

| `yaml_edit::SyntaxKind` node | Requirement and normalized meaning |
| --- | --- |
| `ROOT` | zero or more `DOCUMENT` nodes; zero remains classifiable as a missing/non-mapping root, and multiple remain classifiable so document-marker/source and Saphyr stream precedence select the final failure |
| `DOCUMENT` | exactly one scalar, sequence, or mapping semantic payload after trivia, possibly through one `TAGGED_NODE`; root-kind validity belongs to the semantic builder, so a scalar or sequence reaches `RootMustBeMapping` |
| `MAPPING` | block or matched `{`/`}` flow form; semantic children are `MAPPING_ENTRY` in encounter order |
| `MAPPING_ENTRY` | exactly one `KEY` and one `VALUE`; punctuation/trivia may surround them according to block/flow form |
| `KEY`, `VALUE` | exactly one semantic payload, possibly a zero-width implicit empty |
| `SEQUENCE` | block or matched `[`/`]` flow form; semantic children are `SEQUENCE_ENTRY` in encounter order |
| `SEQUENCE_ENTRY` | exactly one scalar/collection payload, including the implicit-empty form |
| `TAGGED_NODE` | exactly one direct `TAG` token and exactly one scalar/sequence/mapping payload; nesting another tag wrapper is rejected |
| `SCALAR` | no semantic-node descendant; raw range and decoder checks below |
| `ALIAS` | `AliasDenied`; `ERROR` is `PresentationShape` |

Any missing/extra wrapper or accepted kind in a context outside this table is
`PresentationShape`. Both rowan element arms use an exhaustive match with no
wildcard. The node arm explicitly accepts/classifies the twelve possible node
kinds and explicitly maps all thirty-eight lexical kinds to
`PresentationShape`:

```text
node kinds:
ROOT DOCUMENT SEQUENCE MAPPING SCALAR ALIAS TAGGED_NODE ERROR
KEY VALUE MAPPING_ENTRY SEQUENCE_ENTRY

lexical-as-node => PresentationShape:
DASH PLUS COLON QUESTION LEFT_BRACKET RIGHT_BRACKET LEFT_BRACE RIGHT_BRACE
COMMA PIPE GREATER AMPERSAND ASTERISK EXCLAMATION PERCENT AT BACKTICK QUOTE
SINGLE_QUOTE DOC_START DOC_END STRING UNTERMINATED_STRING INT FLOAT BOOL NULL
TAG ANCHOR REFERENCE MERGE_KEY DIRECTIVE WHITESPACE NEWLINE INDENT COMMENT BOM
EOF
```

The token arm explicitly maps the same closed vocabulary, again without a
wildcard:

```text
structural-as-token => PresentationShape:
ROOT DOCUMENT SEQUENCE MAPPING SCALAR ALIAS TAGGED_NODE ERROR
KEY VALUE MAPPING_ENTRY SEQUENCE_ENTRY

always source-denied:
DIRECTIVE => DirectiveDenied
DOC_START | DOC_END => DocumentMarkerDenied
AMPERSAND | ANCHOR => AnchorDenied
ASTERISK | REFERENCE => AliasDenied
BOM => RawBomDenied
UNTERMINATED_STRING => PresentationSyntax

context-classified:
DASH PLUS COLON QUESTION LEFT_BRACKET RIGHT_BRACKET LEFT_BRACE RIGHT_BRACE
COMMA PIPE GREATER EXCLAMATION PERCENT AT BACKTICK QUOTE SINGLE_QUOTE STRING
INT FLOAT BOOL NULL TAG MERGE_KEY WHITESPACE NEWLINE INDENT COMMENT EOF
```

`AMPERSAND` and `ASTERISK` are never scalar punctuation: quoted occurrences
are part of quoted scalar text, while these standalone lexer kinds are rejected
as anchor/alias syntax. `WHITESPACE`, `NEWLINE`, and `INDENT` are trivia;
`COMMENT` is trivia plus a record. `TAG` is allowed only as the one direct tag
child described below. `EOF` may appear zero or one time and, if present, is an
empty final root/document token at `yaml_len`. All remaining
context-classified punctuation is accepted only under its exact
scalar/collection/entry role; `MERGE_KEY` may be scalar content but receives
the separate mapping-key check below. A new `SyntaxKind` variant makes both
matches fail to compile.

Leaf coverage is independent of semantic classification. Starting at cursor
zero, each token range must be ordered, within the parser slice, on UTF-8
boundaries, and have text bytes exactly equal to source bytes at that range.
Every nonempty token must start at the cursor and advances it to its end. The
only zero-width leaves allowed are an optional single final `EOF` and the exact
implicit empty `NULL` described below. The final cursor must equal the
parser-slice length. A gap, overlap, duplicate nonempty byte, token-text
mismatch, or extra zero-width token is malformed `PresentationCoverage`.

Before calling `Scalar::value` or `Scalar::as_string`, check that a `SCALAR`
range is at most 65,536 source bytes. Cast only that exact node, copy both
decoder results into the temporary owned presentation record, and immediately
drop the wrapper. Scalar style derives from original source surface: leading
and trailing `'` is `SingleQuoted`, leading and trailing `"` is
`DoubleQuoted`, leading `|` is `Literal`, leading `>` is `Folded`, otherwise
`Plain`. Missing quote termination was already a positioned error. The style is
later compared to Saphyr; yaml-edit's scalar classification helpers are never
called.

The sole implicit-empty presentation is exactly one zero-width `SCALAR`
containing exactly one zero-width `NULL` token with empty text at the sole
payload position of a `KEY`, `VALUE`, or `SEQUENCE_ENTRY`. It has no tag,
anchor, alias, comment, or other scalar leaf. Explicit `~`, `null`, `""`,
`''`, and tagged omitted payloads are never implicit empty.

The adapter verifies `root.syntax().text()` equals the exact parser slice as a
separate no-op check. Leaf-coverage failure is malformed; a no-op text mismatch
after valid leaf coverage is an accepted read-only reason because source bytes
remain available and semantic acceptance is independent.

### Source-only policy and event-dependent policy

The presentation pass first performs source-only checks over original token
bytes. It does not call a Saphyr tag helper and does not use a decoded scalar
to make these decisions. Directives of every spelling/version, document
markers, anchors, aliases, local/verbatim/custom tags, custom handles, the
standalone `!` tag, explicit merge tags, and remapped `!!` handles are
malformed. Raw BOM was already rejected by the byte scanner.

The only lexically allowed explicit tag token texts are exact ASCII
`!!null`, `!!bool`, `!!int`, `!!float`, `!!str`, `!!seq`, and `!!map`, with no
presentation variation. CST shape proves that `!!seq` directly wraps a
sequence, `!!map` a mapping, and the five scalar tags a concrete non-implicit
scalar; a different wrapper is `ExplicitTagKindMismatch`. At this stage the
adapter records a `TaggedCandidate` containing only candidate index, exact
source tag, and checked tag/payload ranges. It does not yet accept the tag's
decoded value.

Likewise, an untagged plain scalar in mapping-key role whose exact raw payload
bytes are `<<` becomes a `MergeCandidate`; no decoder decides this. Quoted
`"<<"`, `'<<'`, and explicit `!!str <<` are ordinary strings. A source-only
raw-scalar length violation is recorded independently. Candidate records are
split into a full range-bearing prefix needed for a potentially accepted
20,000-node artifact and this range/content-free descriptor retained through
the bounded tail:

```rust
#[repr(C)]
struct TailDescriptor {
    ordinal: u32,
    parent_ordinal: u32, // u32::MAX only for the root
    index: u32,          // zero for root; otherwise parent entry/item index
    role: u8,            // 0 root, 1 item, 2 mapping key, 3 mapping value
    kind: u8,            // 0 scalar, 1 sequence, 2 mapping, 3 implicit empty
    guard: u8,           // 0 none, 1 merge, 2..=8 Null..Mapping SourceTag
    reserved: u8,        // always zero
}
```

The encoded `guard` is the closed private vocabulary
`Guard::{None, Merge, BuiltIn(SourceTag)}`; the seven `SourceTag` variants use
codes 2 through 8 in their declaration order. For every presentation semantic
node, `SourceGuard` is computed exactly once: an accepted explicit built-in tag
is `BuiltIn`, an untagged `Plain` mapping-key scalar whose raw payload is exact
ASCII `<<` is `Merge`, and every other node is `None`. No other source fact is
represented as a guard.

Compile-time assertions require size 16 and alignment 4. At most
`2 * yaml_len + 1` descriptors are accepted (524,289 at the source cap), a
conservative bound because every semantic candidate is a collection or scalar
payload or a punctuation-backed implicit empty. Checked allocation is at most
8,388,624 payload bytes. Exceeding this derived bound is
`PresentationCoverage`; it is not a configurable content limit. Descriptors
contain no text, tag range, or artifact `NodeId`.

During the bounded Saphyr pass, the adapter computes an `EventGuard` for every
semantic node before guard comparison. If an event tag is present, exact
`handle == "tag:yaml.org,2002:"` plus one of the seven lowercase suffixes maps
to `BuiltIn(SourceTag)`; every other present tag records
`ParserTagDisagreement`. Only when the tag is absent, a scalar with Saphyr
style `Plain` in mapping-key role and borrowed decoded text exactly `<<` maps
to `Merge`; every other tagless node maps to `None`. Thus a tagged `!!str <<`
is `BuiltIn(String)`, never `Merge`. Tag helpers, `Display`, custom handle
resolution, markers, and spans are not used.

Tags and merge candidates are validated only while node correspondence remains
a proven ordinal prefix. Source and event guards must be exactly equal. A
matched built-in guard additionally requires the matching
scalar/sequence/mapping event kind, zero anchor, and decoded scalar grammar
below. A matched merge is necessarily an untagged `Plain` mapping-key scalar
with decoded text exactly `<<`. Missing, additional, or different guards are
`ParserTagDisagreement`.

Once node kind/parent/role correspondence first differs, no later ordinal is
associated. Every remaining source and event guard is still computed, but the
two streams are never paired again; the sole exception is the explicit
`Uninspectable` terminal state after `TailBorrowedTextInvariant` described
below. Any proved non-`None` guard on either side makes the document malformed
`ParserTagDisagreement`; matching by a shifted ordinal is forbidden. This rule
covers source-none/event-merge,
source-merge/event-none, and a shifted Saphyr-only merge as well as explicit
tags. Only when every remaining guard on both sides is `None` may the
structural difference become accepted read-only. Fixtures cover those three
asymmetric merge cases, a shift before an otherwise matching tag, a shift
before a merge candidate, an escaped explicitly tagged number, a merge
decoder differential, and an extra/missing Saphyr tag, both before and after
node 20,000.

After a matched pair, invalid decoded content for an otherwise matched built-in
tag is `ExplicitTagValueMismatch`; a matched merge is `MergeKeyDenied`. A
resolved string containing a Unicode noncharacter is `InvalidUnicodeScalar`.
U+FEFF remains valid string data only when escaped in YAML source. These are
event-dependent checks and occur after any Saphyr scan/stream failure, as fixed
by the precedence section; they are not mislabeled as source-only checks.

### Exact Saphyr stream and event state machine

The accepted event sequence is exactly:

```text
StreamStart
DocumentStart(false)
<one mapping-root node>
DocumentEnd
StreamEnd
<next_event returns None>
```

`DocumentStart(true)`, missing/end-after-end events, `Nothing`, or any event
outside this state is `SemanticStreamOrder`. A second `DocumentStart` is
`MultipleDocuments`. Ordinary source containing `---` or `...` was already
rejected as `DocumentMarkerDenied`; these stream kinds therefore cover a
dependency differential or direct state fault, not a second source spelling.
An empty stream has no root and maps to `RootMustBeMapping`. A scalar or
sequence root is allowed through presentation classification, passed as the
first semantic builder event, and maps exactly to `RootMustBeMapping`.

For `Scalar(text, style, anchor_id, tag)`, `anchor_id` must be zero. The five
Saphyr styles map one-for-one to Rehow's five scalar styles. For
`SequenceStart(anchor_id, tag)` and `MappingStart(anchor_id, tag)`, the anchor
must be zero and the optional tag is validated as above. `Alias(_)` is always
rejected. Saphyr `Span` and `Marker` values are ignored completely, including
for diagnostics.

Each scalar/collection start is one semantic node. A collection start pushes
one Rehow frame and its matching end closes it; mapping frames alternate key
then value and sequence frames accept values. A complete child increments its
parent entry count. Rehow uses an iterative state machine. Before accepting a
node event it applies this same-event order:

1. stream/parent position, end kind, alias, anchor, and tag validity;
2. scalar decoded-byte and string-scalar validity;
3. collection depth;
4. total node count;
5. completed parent collection count;
6. numeric source-work precharge, conversion, result charge, per-canonical-
   scalar cap, and aggregate canonical digit budget;
7. Phase 1B builder semantic-encoding/comparison meters;
8. duplicate-key rejection.

The Saphyr adapter is a bounded drain followed by a semantic fold; it does not
return on the first semantic failure and thereby hide a later parser failure:

1. Compute `event_call_limit = 8 * yaml_len + 32` with checked `u64`
   arithmetic. This is at most 2,097,184 for the 256 KiB YAML cap. Call
   `next_event` no more than that many times, including the final `None` call.
   Exceeding the bound is invariant-only `SemanticBuilderInvariant` and stops
   dependency calls.
2. Drain events once. A Saphyr `Err` records sanitized `SemanticSyntax` with no
   dependency text/marker and ends the drain because the parser cannot be
   resumed reliably. Successful events pass through the stream reducer and
   source/event correspondence reducer. These retain separate pending buckets:
   scan error; stream/multiple-document error; parser tag/merge differential;
   event-dependent source restriction; semantic structural/limit error.
3. Retain an owned trace only while no semantic structural/limit error has made
   success impossible. The trace holds at most 20,000 node events, at most
   20,000 collection-end events, and four stream/document events: 40,004
   records. Each retained scalar owns at most 65,536 decoded bytes; one-over is
   `DecodedScalarLimit`. Across the temporary presentation and semantic traces,
   aggregate copied decoded text is checked against
   `3 * yaml_len + 2 * node_count` bytes (at most 826,432). YAML's largest
   accepted UTF-8 expansion is a two-byte double-quoted escape such as `\L` or
   `\P` decoding to a three-byte scalar, so each of the two decoder copies is
   at most `3/2` of its disjoint raw scalar bytes; the `2 * node_count` term
   conservatively covers both copies of zero-width synthetic empties. The
   separately copied raw `Scalar::value` text totals at most `yaml_len`, making
   all temporary scalar text at most `4 * yaml_len + 2 * node_count`
   (1,088,576 bytes). Exceeding a derived bound is invariant/fault-only
   `SemanticBuilderInvariant`, not a new product limit. A record contains only
   Rehow enums, owned decoded text, anchor-zero fact, and a matched
   source-candidate index—no marker, span, dependency error, or dependency
   object.
4. On the first pending semantic structural/limit failure, clear the retained
   success trace and stop decoded-text copying, numeric conversion, and builder
   work. Continue draining with a lightweight collection-kind/key/value stack
   and a monotonic cursor over `TailDescriptor`. The event side constructs the
   same ordinal/parent/role/kind tuple on the stack, capped to
   `2 * yaml_len + 1` frames/descriptors, and computes the exact `EventGuard`
   defined above for every node while text inspection remains enabled. After
   the terminal invariant it computes every non-text-dependent guard exactly
   and uses only the specified `Uninspectable` state. While the prefix is
   intact, compare
   `SourceGuard` and `EventGuard` exactly and perform matched built-in content,
   merge, and noncharacter checks. After a tuple mismatch, do not compare
   ordinals, but continue computing both guard streams; any non-`None` guard on
   either side records `ParserTagDisagreement`.
5. A tuple mismatch permanently breaks the prefix. No later source/event tuple
   is paired. The cursor still visits every remaining source descriptor once
   and the drain inspects every later event once. With an intact prefix, a
   valid matched tag adds no failure and preserves the earlier semantic limit,
   a differing/missing/additional tag or merge guard is
   `ParserTagDisagreement`, invalid matched tag content is
   `ExplicitTagValueMismatch`, and a matched merge is `MergeKeyDenied`.
6. The lightweight stack uses the same 16-byte layout/bound as descriptors, so
   each descriptor vector or worst-case stack is at most 8,388,624 payload
   bytes. Let `D` be the retained source-descriptor count, checked-convert
   `yaml_len` and `D` to `u64` as `Y` and `D64`, and compute
   `borrowed_text_limit = ceil(3 * yaml_len / 2) + D` with checked `u64`
   arithmetic, specifically
   `((Y.checked_mul(3)?.checked_add(1)?) / 2).checked_add(D64)?`.
   Failure of any conversion or limit arithmetic sets
   `TailBorrowedTextInvariant` and disables text inspection before the next
   scalar; no wrapped or saturated value is used.
   Before inspecting each Saphyr scalar's borrowed decoded text, obtain
   `text.len()` in O(1), convert it to `u64`, and checked-add it to the one
   monotonic `borrowed_text_used` counter. Equality is accepted. Conversion or
   addition overflow, or a sum above `borrowed_text_limit`, records the
   sanitized internal pending state `TailBorrowedTextInvariant`, exposed only
   as `SemanticBuilderInvariant` with no position or dependency text. It then
   sets `text_inspection_disabled` permanently. The current scalar and every
   later scalar are neither scanned nor copied; their `len()` is not read
   again.

   Once text inspection is disabled, dependency calls and O(1) structural and
   tag-state reduction continue so later scan/stream failures and exact
   explicit-tag disagreements remain detectable. A present event tag still
   yields an exact built-in guard or `ParserTagDisagreement` without consulting
   scalar text: compare its two fields by O(1) length checks followed by at
   most the fixed 23-byte longest core-tag URI length, never an unbounded tag
   scan. A
   tagless non-`Plain` scalar, a tagless scalar outside
   mapping-key role, and every tagless collection still yield `None`. A tagless
   `Plain` mapping-key scalar instead yields the private state
   `Uninspectable`, never guessed as `None` or `Merge`; likewise, matched
   built-in scalar content and Unicode validity become uninspectable. A later
   source guard after a broken prefix or an exact present event tag can still
   prove `ParserTagDisagreement`; any outcome that would require unavailable
   scalar text leaves `TailBorrowedTextInvariant` as the conservative result.

   Tail work is charged once per descriptor build, descriptor comparison,
   event, and permitted borrowed decoded byte. Its checked bound is
   `2 * D + event_call_limit + borrowed_text_limit`; no text byte, descriptor,
   or event is visited twice, and no rescan or search is permitted.
7. After the drain, choose the highest-priority nonempty bucket according to
   the global precedence below. Only when every failure bucket is empty is the
   retained trace folded into scalar values and a fresh
   `UnknownYamlDocumentBuilder`.

This bounded trace is why a later Saphyr scan error can outrank an earlier node,
depth, decoded-scalar, root, numeric, or builder failure without retaining an
unbounded event stream. Expensive numeric work never starts until the parser
has drained successfully, the source/event restrictions have passed, and the
closed-form numeric precharge fits. Combined-failure fixtures include a later
scan error after an earlier non-mapping root, node/depth/decoded limit, and
numeric-looking scalar, plus a later guarded tag/merge after an earlier
semantic limit. For each earlier semantic limit kind, the matrix appends: a
valid matched built-in tag that preserves that limit; a differing, missing, and
additional tag that yields `ParserTagDisagreement`; matched invalid tag content
that yields `ExplicitTagValueMismatch`; and a matched merge that yields
`MergeKeyDenied`. Separate cases shift before each guard and place each case
after node 20,000 to exercise the descriptor-only tail. It also covers
source-none/event-merge, source-merge/event-none, and shifted Saphyr-only merge
before and after node 20,000. Borrowed-text meter tests cover repeated
65,536-byte decoded scalars in the private copied-event reducer through exact
budget equality, a one-over copied event, checked limit/counter arithmetic
overflow, and proof that no current or later text is scanned after
disablement. Precedence tests then append a later scan error, stream error,
exact explicit-tag mismatch, source guard after a shift, and tagless plain
mapping-key `Uninspectable` event; the first four select the applicable
higher-priority failure and the last selects `SemanticBuilderInvariant`
without guessing a merge result.

The adapter mirrors depth/node/entry and numeric-digit counters only to reject
before expensive conversion. The Phase 1B builder remains construction
authority and independently checks every folded event. Exact limit equality is
accepted. A mirror/builder mismatch is `SemanticBuilderInvariant`.

The presentation candidate sequence collapses document/key/value/entry/tag
wrappers into semantic preorder. Candidates are paired only through the first
exact parent/role/kind match. Missing or structurally different unguarded
candidates after that prefix become read-only; the explicit-tag/merge rule in
the prior section applies when a guarded candidate remains.

The Saphyr scalar `"~"` with `Plain`, zero anchor, no tag, and the structurally
corresponding exact zero-width presentation candidate becomes an
`ImplicitEmpty` null. A concrete source `~` has a nonzero range and is an
ordinary scalar null. Any other empty-event/candidate combination is either an
explicit-tag/content failure or a read-only decoder disagreement; it never
gains a replacement range.

Resolved values are pushed one-for-one to the builder: collection starts/ends,
null, Boolean, canonical integer/float, and owned string. `finish_document` is
called only after the single implicit Saphyr document and root close. No
partial value, trace, or builder is retained on failure.

### Literal YAML 1.2.2 core scalar resolution

Rehow implements ASCII byte predicates directly; it adds no regex or bigint
dependency. Only a plain scalar without an explicit source tag is implicitly
resolved. First match wins in this exact order:

| Type | Exact accepted grammar |
| --- | --- |
| null | empty, `null`, `Null`, `NULL`, or `~` |
| Boolean | `true`, `True`, `TRUE`, `false`, `False`, or `FALSE` |
| decimal integer | `[-+]?[0-9]+` |
| octal integer | `0o[0-7]+` |
| hexadecimal integer | `0x[0-9a-fA-F]+` |
| finite float | `[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?` |
| infinity | `[-+]?(\.inf|\.Inf|\.INF)` |
| NaN | `.nan`, `.NaN`, or `.NAN` |
| string | every other accepted plain scalar |

The integer rows precede the finite-float row, so a plain `12` is an integer.
Octal and hexadecimal forms have no sign under the YAML 1.2.2 core grammar.
YAML 1.1 underscores, sexagesimal forms, and legacy Boolean words are strings.
Timestamp-like text is a string. Untagged single-quoted, double-quoted, literal,
and folded scalars are always strings.

An explicit scalar tag selects its named row rather than first-match
resolution. Its decoded text must match that complete row: `!!str` accepts any
valid decoded string; `!!null`, `!!bool`, `!!int`, and `!!float` accept only
the exact corresponding grammar above. `!!float` accepts the finite,
infinity, or NaN row. No prefix match or surrounding whitespace is allowed.

Integer normalization is arbitrary precision. Decimal removes a leading sign
and magnitude zeroes; every zero becomes positive canonical `0`. Octal and
hexadecimal use the ADR 0011 left-to-right multiply/add decimal-vector
conversion after the quadratic precharge. No machine integer or generic
`from_str_radix` receives the complete value.

For a finite float, remove the optional sign; concatenate integer and
fractional mantissa digits, treating the absent integer part as empty; parse
the optional exponent as an arbitrary-precision signed decimal; subtract the
fractional digit count; remove coefficient leading zeroes; then remove each
coefficient trailing zero while adding one to the exponent. A nonzero result
has neither leading nor trailing coefficient zero. A zero coefficient forces a
zero exponent, preserves a negative float sign, and otherwise becomes positive
zero. Exponent negative zero normalizes positive. Infinities retain only their
sign, and all accepted NaN spellings become the single NaN value.

For numeric work, `S`, `C`, `R`, the checked charge order, 16 Mi work budget,
and 80 Ki retained canonical digit budget are exactly ADR 0011. `S` counts all
mantissa and exponent digits for a finite float. `C` counts final coefficient
and exponent magnitude digits, including their canonical single zero digits.
The pre-conversion `R + 4*S` check occurs before radix conversion; `4*C` is
charged before the canonical result is retained. Decimal and float conversion
are linear in `S + C`.

### Builder failures and sanitized parse taxonomy

The only returned error is a non-content-bearing crate-private value:

```rust
pub(crate) struct ParseFailure {
    kind: ParseFailureKind,
    position: Option<SourcePosition>,
    limit: Option<ParseLimitFacts>,
}

pub(crate) enum ParseLimitScope {
    WholeYamlBytes, GuideBytes, FrontmatterBytes, RawScalarBytes,
    DecodedScalarBytes, CanonicalScalarBytes, Depth, MappingEntries,
    SequenceEntries, Nodes, NumericWork, NumericDigits, SemanticEncodingBytes,
    SemanticComparisonBytes,
}

pub(crate) struct ParseLimitFacts {
    scope: ParseLimitScope,
    limit: u64,
    observed_at_least: u64,
}

pub(crate) enum ParseFailureKind {
    SourceTooLarge, FrontmatterTooLarge, InvalidUtf8, RawBomDenied,
    BareCarriageReturn, MixedYamlLineEndings,
    MissingGuideOpeningDelimiter, MissingGuideClosingDelimiter,
    MissingGuideSeparator, PresentationSyntax, PresentationCoverage,
    PresentationShape, DirectiveDenied, DocumentMarkerDenied, AnchorDenied,
    AliasDenied, ExplicitTagDenied, ExplicitTagKindMismatch,
    ExplicitTagValueMismatch, MergeKeyDenied, InvalidUnicodeScalar,
    SemanticSyntax, SemanticStreamOrder, MultipleDocuments,
    RootMustBeMapping, TopLevelKeyMustBeString, DepthLimit,
    MappingEntryLimit, SequenceEntryLimit, NodeLimit, RawScalarLimit,
    DecodedScalarLimit, CanonicalScalarLimit, NumericWorkLimit, NumericDigitLimit,
    SemanticEncodingLimit, SemanticComparisonLimit, DuplicateMappingKey,
    ParserTagDisagreement, MeterArithmeticOverflow, SemanticBuilderInvariant,
}
```

The exact read surface is:

```rust
impl ParseFailure {
    pub(crate) fn kind(&self) -> ParseFailureKind;
    pub(crate) fn position(&self) -> Option<SourcePosition>;
    pub(crate) fn limit(&self) -> Option<&ParseLimitFacts>;
}
impl ParseLimitFacts {
    pub(crate) fn scope(&self) -> ParseLimitScope;
    pub(crate) fn limit(&self) -> u64;
    pub(crate) fn observed_at_least(&self) -> u64;
}
```

`ParseFailure` and `ParseLimitFacts` implement exactly `Clone`, `Eq`, and
`PartialEq`, plus manual non-content `Debug`; `ParseFailure` also implements
the exact `Display` text `invalid Rehow YAML source` and `std::error::Error`
with `source() == None`. Neither is `Copy`, ordered, hashed, defaulted, or
serialized. `ParseFailureKind` and `ParseLimitScope` implement exactly `Clone`,
`Copy`, `Eq`, `PartialEq`, `Ord`, `PartialOrd`, and `Hash`, plus manual
non-content `Debug`; they have no display/conversion/serialization traits.

Limit facts are present only for the thirteen named limit kinds and
`MeterArithmeticOverflow`: source/frontmatter, raw/decoded scalar, depth,
mapping/sequence entries, nodes, numeric work/digits, and semantic
encoding/comparison. The scope identifies the applicable counter; `limit` is
the fixed accepted maximum and `observed_at_least` is the first refused value
or checked lower bound. Every other kind has `limit() == None`. Dependency
messages/codes, event debug output, rejected text, bytes, paths, absolute
locations, and parser objects are never stored, logged, or returned.

The Phase 1B builder kinds map exactly as follows:

| `UnknownYamlBuildErrorKind` | Phase 1C failure |
| --- | --- |
| `RootMustBeMapping` | `RootMustBeMapping` |
| `TopLevelKeyMustBeString` | `TopLevelKeyMustBeString` |
| `DuplicateMappingKey` | `DuplicateMappingKey` |
| `InvalidString` | `InvalidUnicodeScalar` |
| `DepthLimit` | `DepthLimit` |
| `MappingEntryLimit` | `MappingEntryLimit` |
| `SequenceEntryLimit` | `SequenceEntryLimit` |
| `NodeLimit` | `NodeLimit` |
| `ScalarLimit` | `CanonicalScalarLimit` |
| `NumericDigitLimit` | `NumericDigitLimit` |
| `SemanticEncodingLimit` | `SemanticEncodingLimit` |
| `SemanticComparisonLimit` | `SemanticComparisonLimit` |
| `ArithmeticOverflow` | `MeterArithmeticOverflow` with the active meter scope |
| `MultipleRoots` | `MultipleDocuments` |
| `UnexpectedEnd`, `MismatchedEnd`, `IncompleteMappingEntry`, `IncompleteDocument` | `SemanticStreamOrder` |
| `Poisoned` | `SemanticBuilderInvariant` |

Every failure kind has one and only one reachability class:

| Reachability | Exhaustive kinds | Required evidence |
| --- | --- | --- |
| Source-reachable | `SourceTooLarge`, `FrontmatterTooLarge`, `InvalidUtf8`, `RawBomDenied`, `BareCarriageReturn`, `MixedYamlLineEndings`, `MissingGuideOpeningDelimiter`, `MissingGuideClosingDelimiter`, `MissingGuideSeparator`, `PresentationSyntax`, `PresentationShape`, `DirectiveDenied`, `DocumentMarkerDenied`, `AnchorDenied`, `AliasDenied`, `ExplicitTagDenied`, `ExplicitTagKindMismatch`, `ExplicitTagValueMismatch`, `MergeKeyDenied`, `InvalidUnicodeScalar`, `RootMustBeMapping`, `TopLevelKeyMustBeString`, `DepthLimit`, `MappingEntryLimit`, `SequenceEntryLimit`, `NodeLimit`, `RawScalarLimit`, `DecodedScalarLimit`, `CanonicalScalarLimit`, `NumericWorkLimit`, `NumericDigitLimit`, `DuplicateMappingKey` | one deterministic natural byte case per kind; exact/one-over generated source tests for limits; scalar and sequence roots explicitly assert `RootMustBeMapping` |
| Parser-differential | `PresentationCoverage`, `SemanticSyntax`, `ParserTagDisagreement` | direct sanitized-adapter tests plus the private copied-record/range/event fault seam; retain a natural pinned-parser regression fixture only if one is actually found |
| Invariant/fault-only | `SemanticStreamOrder`, `MultipleDocuments`, `SemanticEncodingLimit`, `SemanticComparisonLimit`, `MeterArithmeticOverflow`, `SemanticBuilderInvariant` | direct event-state/builder-meter/error-mapper tests and an exact private fault control; no natural-source fixture claim |

An ordinary multi-document source contains a document marker and therefore
returns `DocumentMarkerDenied`; `MultipleDocuments` is reserved for a Saphyr
stream that contradicts the already accepted source trace. `SemanticSyntax`
means Saphyr rejected a slice for which yaml-edit produced a complete accepted
CST. `DecodedScalarLimit` is source-reachable: 21,845 repeated `\L` escapes plus
one ASCII byte decode to exactly 65,536 UTF-8 bytes from a 43,693-byte quoted
raw scalar; adding a second ASCII byte yields the one-over fixture while the
raw scalar remains below its cap. `CanonicalScalarLimit` is also
source-reachable, but not through radix expansion: a finite float `.` followed
by 65,531 nonzero fractional digits stores a 65,531-digit coefficient plus the
five-digit exponent magnitude `65531`, exactly 65,536 canonical digits, while
65,532 fractional digits produce 65,537 canonical digits from a 65,533-byte raw
scalar. Its numeric-work charge remains below 16 MiB. Octal/hex inputs large
enough to cross the canonical-scalar cap are instead preempted by `S * S`.
`CanonicalScalarLimit` remains distinct from the 80 KiB document aggregate
`NumericDigitLimit`.
No reviewed generator reaches the 8 MiB semantic-encoding or 32 MiB
semantic-comparison meter through a parser slice capped at 256 KiB; the Phase
1B natural vectors that reach them require more source-equivalent content.
Phase 1C therefore makes no false natural-fixture claim: both defensive builder
outcomes remain exhaustively mapped, and their exact/one-over/overflow evidence
uses the `cfg(test)` builder-meter bridge below. Discovering a <=256 KiB natural
witness changes the reachability row and requires Architect/Security review,
but does not change the production failure mapping.
`SemanticBuilderInvariant` includes a mirror/builder disagreement,
unexpected builder poisoning, event-call-bound violation, or impossible
retained-trace accounting result, including `TailBorrowedTextInvariant`.

`ParseFailureKind` is not a protocol token. A later application mapper owns
the safe relative path and request ID and performs an exhaustive mapping. It
may map source/frontmatter size failures to `FILE_TOO_LARGE`; all other generic
failures map to the appropriate `INVALID_MANIFEST`, `INVALID_CATEGORY`, or
`INVALID_GUIDE` only after constructing a confined `SafeRelativePath`. Its
diagnostic token mapping is fixed here:

| Phase 1C failure group | Existing `DiagnosticCode` spelling |
| --- | --- |
| `InvalidUtf8` | `invalid_utf8` |
| delimiter/BOM/newline envelope failures | `invalid_frontmatter_envelope` |
| presentation/Saphyr syntax, shape, coverage, stream order, multiple documents, builder invariant | `yaml_syntax` |
| directive/marker/anchor/alias/tag/merge/noncharacter/root/top-key restrictions | `yaml_unsupported_construct` |
| depth/map/sequence/node/raw, decoded, or canonical scalar limits | corresponding `yaml_*_limit` token from ADR 0012 |
| numeric work, numeric digits, or numeric arithmetic overflow | `yaml_numeric_work_limit` |
| semantic encoding/comparison or their arithmetic overflow | `yaml_semantic_work_limit` |
| duplicate mapping key | `yaml_duplicate_key` |
| parser tag disagreement | `yaml_parser_disagreement` |

Using `yaml_numeric_work_limit` as the public bucket for the separately retained
80 KiB digit failure is intentional protocol-v1 compatibility; the internal
kind and limit scope remain exact. Source-size failures have no diagnostic code
because they become `FILE_TOO_LARGE`. Phase 1C imports neither `ErrorDto` nor
`SafeRelativePath`, does not construct `RequestId`, and adds no protocol
factory.

### Deterministic malformed precedence

Only one `ParseFailure` is returned. The pipeline order is normative even when
an input violates more than one rule:

1. whole-source byte cap;
2. complete-file UTF-8 validity, always without a position;
3. byte policy: raw BOM outside the optional byte-zero prefix, then bare CR,
   then mixed YAML/envelope line endings;
4. Guide envelope: opening delimiter, closing delimiter/frontmatter cap, then
   separator;
5. first selected yaml-edit positioned error;
6. leaf coverage;
7. source-token violations, ordered by lowest YAML byte offset and ties by
   directive, document marker, anchor, alias, denied tag, raw BOM, unterminated
   token;
8. CST shape, then explicit-tag kind, then raw-scalar limit, each at the lowest
   YAML byte offset;
9. a Saphyr scan error anywhere in the bounded drain;
10. stream-state failure, with `MultipleDocuments` before other stream failures
    at the same event ordinal;
11. guarded tag/merge parser disagreement;
12. event-dependent restrictions, ordered by event ordinal and then explicit
    tag value, merge key, Unicode noncharacter;
13. semantic event/build failure, ordered by event ordinal and the same-event
    order above;
14. builder `finish_document` failure.

The scanner and adapters may collect later-stage facts early but report only by
this list. A lower numbered stage wins globally, not merely by observation
time. Within an offset-ordered stage the lowest safe file offset wins; within
an event stage the lowest ordinal wins. ADR 0012 precedence breaks builder ties.
`TailBorrowedTextInvariant` is a validation-poisoning substate of stage 13:
once set, it replaces every pending stage 13 or 14 result regardless of event
ordinal because later text-dependent guards and restrictions can no longer be
proved. It never replaces a stage 9 through 12 result already proved. A later
scan error, stream-state failure, exact present-tag disagreement, or source
guard after a broken prefix is still evaluated without scalar text and
replaces it under stages 9 through 11. No text-dependent stage 11 or 12 result
is guessed after disablement; absent such an independently proved result, the
returned kind is `SemanticBuilderInvariant`. This is the sole exception to
event-ordinal ordering within stage 13.
Thus a later Saphyr scan error wins over an earlier root/limit failure; a
validated merge restriction wins over an earlier numeric limit; node limit
wins before numeric conversion on node 20,001; and a mapping-entry limit wins
before duplicate detection on entry 257. No dependency string/marker or unsafe
ordinal guess participates. Pairwise combined-failure tests cover every
adjacent stage boundary from 5 through 14 plus the cross-stage examples above.

### Dual-tree reconciliation and first disagreement

The semantic adapter retains a temporary Rehow-owned trace containing node
kind, parent/role, child arity when closed, decoded pre-resolution scalar text,
Saphyr scalar style, and validated explicit tag. The presentation adapter
retains the corresponding normalized temporary trace plus checked candidate
ranges, yaml-edit raw `value`, decoded `as_string`, collection style, entry
records, and comments. Both traces are iterative and bounded; dependency
objects have already been dropped.

Reconciliation zips semantic and presentation node preorder without searching
or resynchronizing. For each ordinal it compares, in this order:

1. parent `NodeId` and `NodeRole`;
2. node kind, treating only the exact implicit-empty pair as
   `ImplicitEmpty`;
3. closed direct-child arity and mapping key/value or sequence item roles;
4. validated source-tag surface;
5. scalar style;
6. yaml-edit `as_string` versus Saphyr pre-resolution text;
7. yaml-edit `value` versus exact source bytes at the payload range;
8. candidate range validity and containment.

An extra node in either trace is compared at the first missing ordinal. After
the first parent/kind/arity mismatch, Rehow does not attempt to align later
nodes: remaining semantic nodes receive `NodePresentation::Unreconciled`.
Their semantic values and semantic parent/child IDs remain readable. Only
entries whose parent and all referenced nodes were proven before that ordinal
are retained; every other temporary presentation entry is dropped without an
ID. Any comment whose deepest possible owner or adjacent boundary uses a
dropped entry or unreconciled node is retained by source range but receives
`Unresolved`/`Ambiguous`. A source-tag or merge mismatch was already malformed
and cannot enter this state. Tests shift the first key, value, nested child, and
entry wrapper and assert no post-shift entry ID or semantic correspondence is
invented.

Every concrete range must be within the parser slice, on UTF-8 boundaries,
contained by its reconciled parent, and disjoint from semantic siblings.
Containment is allowed only along the ancestor chain. Entry wrapper ranges must
contain their direct key/value or item payloads and be disjoint from sibling
entries. All yaml ranges are rebased to file ranges exactly once with checked
addition of `yaml_file_range.start`; overflow or disagreement is read-only, not
an unchecked coordinate.

### Comments and correspondence ownership

Every yaml-edit `COMMENT` token becomes one `CommentRecord`, numbered by source
range. It stores yaml/file ranges, `CommentPlacement`, and `CommentOwner`, but
not copied text:

```rust
pub(crate) enum CommentPlacement {
    Inline, Leading, Trailing, InterEntry, Nested, Flow, DocumentOnly,
    Unresolved,
}
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

pub(crate) struct CommentRecord {
    id: CommentId,
    yaml_range: YamlByteRange,
    file_range: FileByteRange,
    placement: CommentPlacement,
    owner: CommentOwner,
}

impl CommentRecord {
    pub(crate) fn id(&self) -> CommentId;
    pub(crate) fn yaml_range(&self) -> YamlByteRange;
    pub(crate) fn file_range(&self) -> FileByteRange;
    pub(crate) fn placement(&self) -> CommentPlacement;
    pub(crate) fn owner(&self) -> CommentOwner;
}
```

`CommentPlacement` and `CommentOwner` implement exactly the read-only value
traits assigned to IDs, including manual non-content `Debug`, and no others.
`CommentRecord` has exactly the no-trait surface assigned to content-bearing
records above. Comment IDs are contiguous source-order indexes.

Ownership is byte-correspondence, not an interpretation of prose, and uses
only source-ordered sweeps:

1. After envelope classification, the presentation pass scans only the bounded
   whole-YAML bytes or Guide bytes from byte zero through the separator. It
   builds one temporary `Box<[u32]>` of line starts and one parallel
   `Box<[u8]>` of blank-line flags. For region length `R <= 262_144`, each has
   at most `R + 1 <= 262_145` entries, for at most 1,048,580 + 262,145 =
   1,310,725 payload bytes. Checked `R + 1`/capacity arithmetic precedes
   allocation. Both boxes are dropped before return, and the Guide body is not
   scanned for YAML comments. A dense-newline fixture at `R == 262_144` proves
   the entry/byte maximum and single scan.
2. One iterative CST enter/leave traversal visits each syntax node and token
   once, maintains an ancestor stack, and records the deepest candidate node,
   entry, and collection for each comment. The stack is bounded by the number
   of visited syntax elements. Traversal stops with `PresentationCoverage` if
   syntax-element visits exceed checked `8 * yaml_len + 64`; this is a
   parser-differential guard, not a configurable product limit.
3. After reconciliation, two counting/prefix-sum passes bucket direct proven
   entries and comments by parent without hashing or all-pairs comparison.
   Within each parent both slices are already source ordered. One monotonic
   two-cursor merge identifies the immediately preceding/following proven
   entry for each maximal adjacent comment-only run; a blank-line bit stops
   eligibility.
4. A same-line preceding nontrivia token makes the comment `Inline` and assigns
   the deepest proven containing entry, otherwise the deepest proven node. A
   comment lexed inside a proven scalar wrapper is `Nested` on that node. A
   comment-only run with both eligible block-entry neighbors is `InterEntry`;
   only following is `Leading`; only preceding is `Trailing`; neither becomes
   a collection boundary. At document scope it is `DocumentOnly`.
5. A comment in a flow collection is `Flow` while retaining the same proven
   owner/boundary. A crossed range, two eligible deepest owners, missing range,
   dropped entry, unreconciled node, or scalar-overlap without CST descent is
   `Unresolved` with `CommentOwner::Ambiguous` and adds
   `CommentOwnershipAmbiguous`.

No step scans all entries for one comment, calls `parent()` repeatedly from a
leaf, sorts by a content comparator, or revisits a whole source/token slice per
node. With `S` source bytes, `T` syntax elements, `N` semantic nodes, `E`
proven entries, and `C` comments, Rehow-owned presentation/reconciliation work
is `O(S + T + N + E + C)` and memory is `O(T + N + E + C)`, each bounded by
the source and stated guards. A test-only counter charges one per source-byte,
syntax-element, reconciliation-node, entry-cursor, and comment-cursor visit and
asserts `work <= 12*S + 8*T + 12*N + 8*E + 8*C + 64`. Max-size fixtures use
dense comments, deepest allowed nesting, flow/block mixtures, and comments at
every boundary. Repeated full scans or superlinear correspondence fail the
counter gate.

Concrete boundary ownership lets Phase 1D refuse only edits whose value,
entry, or insertion boundary would disturb a comment. Merely having a leading,
trailing, or inter-entry comment does not make the whole document read-only.
Phase 1C still grants no edit range.

### Accepted read-only classification

An input returns `Ok` only after source policy, both parses, semantic limits,
root constraints, duplicate detection, and complete semantic construction
succeed. It is `RoundTripStatus::Proven` only when reconciliation, ranges,
comments, and no-op rendering all succeed. Otherwise it is semantic-readable
but globally non-round-trippable:

```rust
pub(crate) enum RoundTripStatus {
    Proven,
    ReadOnly(RoundTripReport),
}

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

impl RoundTripStatus {
    pub(crate) fn is_proven(&self) -> bool;
    pub(crate) fn report(&self) -> Option<&RoundTripReport>;
}
impl RoundTripReport {
    pub(crate) fn issues(&self) -> &[RoundTripIssue];
    pub(crate) fn total_issues(&self) -> u32;
    pub(crate) fn is_truncated(&self) -> bool;
    pub(crate) fn primary_issue(&self) -> Option<&RoundTripIssue>;
}
impl RoundTripIssue {
    pub(crate) fn reason(&self) -> RoundTripReason;
    pub(crate) fn node(&self) -> Option<NodeId>;
    pub(crate) fn entry(&self) -> Option<EntryId>;
    pub(crate) fn comment(&self) -> Option<CommentId>;
    pub(crate) fn yaml_offset(&self) -> Option<YamlByteOffset>;
}
```

`RoundTripReason` implements exactly the read-only value traits assigned to
IDs, including manual non-content `Debug`, and no others. Status/report/issues
have exactly the no-trait content-bearing surface declared earlier and private
fields/constructors. The report retains the first 32 issues, exact checked
`u32` total, and truncation flag. Detection order is node preorder, then proven
entry order, then comment range, then no-op rendering. At one ordinal/offset,
reason order is enum declaration order. No issue retains text or a range; an
unavailable or unsafe location is `None`. A checked total overflow is
invariant-only `SemanticBuilderInvariant` rather than wraparound.

Node count/kind/missing-candidate faults emit `NodeCorrespondenceMismatch`;
parent/role/direct-child/entry arity emits `ParentOrArityMismatch`; style,
decoded-text, and raw-value/source checks emit their corresponding scalar
reason; invalid/rebase failure emits `InvalidCandidateRange`; sibling/entry
overlap emits `OverlappingSiblingRanges`; and the last three names map
one-for-one to no-op, unsupported, and comment checks. A node issue sets only
`node`, an entry-only range issue sets only `entry`, a comment issue sets only
`comment`, and a document-wide no-op issue sets none; `yaml_offset` is the
lowest validated involved offset. The first structural mismatch emits one
structural issue, marks the suffix unreconciled, and does not emit fabricated
per-node suffix issues.

The exhaustive malformed/read-only split is:

| Condition | Result |
| --- | --- |
| input/envelope/UTF-8 failure, dependency syntax error, incomplete leaf coverage, unclassifiable CST | `Err(ParseFailure)` |
| directive, document marker, anchor/alias, denied/mismatched tag, merge key, raw BOM, invalid string scalar | `Err(ParseFailure)` |
| Saphyr state/scan failure, root/top-key rule, any resource limit, duplicate semantic key, builder failure | `Err(ParseFailure)` |
| semantic/presentation node, parent, role, arity, or style disagreement after both accepted parses | accepted `ReadOnly` |
| yaml-edit decoded text differs from Saphyr text, or yaml-edit raw value differs from its checked source range | accepted `ReadOnly` |
| missing, invalid, overlapping, or unrebasable semantic candidate/entry range after complete leaf coverage | accepted `ReadOnly` |
| exact implicit-empty pair | accepted and potentially `Proven`, with site but no replacement range |
| any other zero-width semantic candidate | malformed presentation shape |
| no-op tree text differs from source after complete leaf coverage | accepted `ReadOnly` |
| classified but unsupported presentation correspondence | accepted `ReadOnly` |
| unique comment boundary/owner cannot be proven | accepted `ReadOnly` |

For later `UNSAFE_ROUND_TRIP`, node/style/decoder/raw/no-op disagreement maps to
reason `presentation_mismatch`; range or unsupported-presentation issues map to
`round_trip_unproven`; ambiguous comments map to
`comment_ownership_ambiguous`. Phase 1D/application owns that mapping and the
safe path/request context.

### Test-only seams and property strategy

Production has fixed limits and no injection object. Under `#[cfg(test)]` only,
private child test modules may call a private `parse_with_test_control` whose
unnameable control has one exact variant for each of these copied-record or
state-reducer faults:

- presentation coverage gap/overlap/extra zero-width token;
- drop candidate, change candidate kind, parent, or arity, and shift before a
  later explicit tag or merge candidate;
- missing/invalid/overlapping candidate or entry range;
- scalar style, presentation-decoder text, raw-value/source, unsupported
  presentation, and no-op-render mismatch;
- detach/cross a comment owner boundary;
- replace a copied Saphyr tag, scalar style/text, stream event, or return with a
  sanitized scan error; replace either copied guard independently, including
  source-none/event-merge, source-merge/event-none, and a shifted Saphyr-only
  merge; force the event-call bound;
- seed the parser-owned tail borrowed-text `u64` counter immediately before one
  real nonempty `text.len()` charge; expose its private scan-count snapshot to
  the child test only, so exact equality, one-over, checked overflow, and
  permanent no-scan-after-disable behavior are asserted;
- seed parser-owned numeric work/digit counters immediately before one charge
  or force their checked arithmetic branch; and
- force a mirror/builder counter mismatch after a builder call succeeds.

These controls provide one parse-path test for all ten `RoundTripReason`
variants, every parser-differential failure, `SemanticBuilderInvariant`, and
guarded-candidate behavior. Private reducers also accept synthetic closed test
enums for every stream state, positioned-error range ordering, and the
exhaustive builder-error mapping; those are direct unit tests, not claims that
impossible source bytes cause the state.

Injection occurs only in Rehow-owned temporary records and meters after
dependency calls. It cannot construct `UnknownYamlValue`, bypass the Phase 1B
builder, change production constants, supply YAML fragments, create an
artifact directly, or survive a non-test build. There is no cargo feature,
environment variable, global mutable hook, trait object, or non-test/public
symbol. External trybuild negatives prove the controls and artifact
constructors are absent.

Phase 1C authorizes one narrow bridge to Phase 1B's builder-owned meters because
private sibling helpers are otherwise inaccessible:

```rust
#[cfg(test)]
pub(crate) enum UnknownYamlTestMeter {
    Nodes,
    NumericDigits,
    SemanticEncoding,
    SemanticComparison,
}

#[cfg(test)]
impl UnknownYamlDocumentBuilder {
    pub(crate) fn set_test_meter(
        &mut self,
        meter: UnknownYamlTestMeter,
        used: usize,
    ) -> Result<(), UnknownYamlBuildErrorKind>;
}
```

`domain::yaml_value` defines these items and `domain` re-exports the enum as
`pub(crate)` only under `cfg(test)`. The method matches the actual Phase 1B
`BuilderState::Active` `usize` fields exactly: `nodes`, `numeric_digits`,
`encoding_bytes`, and `comparison_units`. It directly sets only the selected
field. On `BuilderState::Poisoned` it returns
`Err(UnknownYamlBuildErrorKind::Poisoned)` without mutation. It performs no
integer conversion, changes no fixed maximum, and cannot skip a real charge.

Overflow tests first construct the required valid builder state, set exactly
one selected field to `usize::MAX` immediately before a named real nonzero
charge, and then call `push_event`: `BeginMapping` charges `Nodes`; a canonical
integer value charges `NumericDigits`; attaching a scalar charges
`SemanticEncoding`; closing a two-entry mapping charges
`SemanticComparison`. Each must return `ArithmeticOverflow`, after which a
second real event must return `Poisoned`. Exact-limit/one-over tests set each
field near its production maximum and assert the ordinary named limit. All
four meters and poisoned-helper behavior are covered.

The bridge is absent from normal and external dependency builds. No other
Phase 1B visibility changes. A normal `cargo check --lib`, rustdoc absence
check, and external compile-fail fixture prove absence without `cfg(test)`.

Phase 1C adds no property/fuzz dependency. A deterministic test-only xorshift64
generator with fixed checked-in seeds performs at least 4,096 bounded arbitrary
byte cases and 4,096 mutations of accepted fixtures. Normal test completion
proves no panic; accepted cases also prove semantic traversal termination and
range/source invariants. Natural/generated source exercises every
source-reachable production limit at equality and one over; differential and
invariant-only limits use only the classified private controls/direct reducers,
never reduced production constants. A cargo-fuzz
target may be proposed later with a separately reviewed development dependency;
it is not silently introduced here.

### Required API, fixture, and evidence gates

Phase 1C tests live inside the private parser module so no production API is
made public for integration tests. Stable fixture roots are:

```text
crates/rehow-core/src/parsing/tests/fixtures/accepted/
crates/rehow-core/src/parsing/tests/fixtures/malformed/
crates/rehow-core/src/parsing/tests/fixtures/read-only/
```

The accepted set has named fixtures for whole/Guide LF, CRLF, BOM, final/no
final newline, every scalar and collection style, every literal built-in tag,
arbitrary integers/floats/special floats, complex keys, comments in every
placement, implicit keys/values/items, flow/block nesting, and exact limits.
The malformed set names every source-reachable `ParseFailureKind` and every
source-reachable one-over limit, directive/tag/anchor/alias/merge form, invalid
UTF-8/BOM/newline and Guide envelope combination, non-mapping root, and
semantic duplicate form. Parser-differential and invariant/fault-only kinds use
the exact direct-state/fault evidence in the reachability table and are not
misrepresented as natural fixtures. The read-only set plus private controls
covers every `RoundTripReason`; any naturally discovered pinned-parser
disagreement is added as a regression fixture without changing its class.

The guarded-tail closure tests have stable names and run through the private
copied-event/state reducers:

```text
guard_source_none_event_merge
guard_source_merge_event_none
guard_shifted_saphyr_only_merge
guard_shifted_saphyr_only_merge_after_node_20000
tail_borrowed_text_repeated_scalar_maxima_at_limit
tail_borrowed_text_one_over_is_invariant
tail_borrowed_text_counter_overflow_is_invariant
tail_borrowed_text_limit_arithmetic_overflow_is_invariant
tail_borrowed_text_disable_stops_all_later_scans
tail_borrowed_text_invariant_then_scan_error
tail_borrowed_text_invariant_then_stream_error
tail_borrowed_text_invariant_then_exact_tag_disagreement
tail_borrowed_text_invariant_then_shifted_source_guard
tail_borrowed_text_invariant_then_uninspectable_merge
```

The first three also run with the guarded event beyond node 20,000. The later
error tests assert the exact precedence result described above, not only that
parsing fails.

Trybuild negatives are exactly:

```text
phase1c_parser_module_is_private.rs
phase1c_parse_entries_are_private.rs
phase1c_artifact_fields_are_private.rs
phase1c_coordinate_constructors_are_private.rs
phase1c_round_trip_report_cannot_be_forged.rs
phase1c_test_controls_absent.rs
```

The public API positive snapshot remains the Phase 1B API only. A dependency
surface check compiles one adapter fixture using every allowed yaml-edit/rowan
and Saphyr call and scans production parser imports/calls for the forbidden API
catalog in this ADR. `cargo doc --no-deps` plus trybuild proves no `parsing`
module or parser/artifact symbol is public.

`packages/contracts/src/generated.ts` must remain byte-identical after contract
generation. It must not contain `ParsedYamlDocument`, `ParsedGuideSource`,
`ParsedYamlArtifact`, `ReconciledNode`, `ReconciledEntry`, `ParseFailure`, any
coordinate/comment/round-trip type, or any `UnknownYaml*` type. There is no
Phase 1C TypeScript fixture other than this explicit absence assertion.

The phase writes these evidence files only after Phase 1B's evidence is
checkpointed:

```text
docs/evidence/phase-1c-dependencies.md
docs/evidence/phase-1c-parser-matrix.md
```

The dependency evidence contains lock checksums, `cargo tree` normal/features
and inverse trees for all three pins, advisory/yank/license/MSRV/source/unsafe
review, exact allowed API audit, and release `rehow-core` rlib plus desktop/MCP
binary sizes before and after. The parser matrix maps every failure kind to its
reachability class and named natural/direct/fault test, and maps every
round-trip reason, ADR 0011 vector, property seed/count, combined-precedence
pair, complexity counter, and fault seam to a named passing test. It records
exact test counts and commands.

Acceptance commands are the Phase 1B/full workspace gates plus focused private
parser tests, trybuild, contract byte-drift/absence checks, the dependency
surface check, release-size evidence, `cargo tree` evidence, bootstrap check,
and `git diff --check`. Tests use only checked-in byte fixtures and in-memory
boxes; Phase 1C opens no Workspace or arbitrary path.

## Consequences

- Phase 1D receives a stable crate-private, owned semantic/source/presentation
  artifact without exposing parser dependencies or mutation capability across
  a crate boundary.
- Accepted source bytes are retained exactly and bounded; rejected content
  cannot appear in dependency error text or public diagnostics.
- A presentation disagreement does not erase safely parsed semantics, while no
  later metadata write can proceed from an unproved artifact.
- Comment correspondence is conservative and local. It preserves unrelated
  edit potential without pretending comments have YAML semantic ownership.
- The two parsers, normalization trace, source retention, and adversarial
  matrix increase memory and implementation cost within the fixed 256 KiB YAML
  slice. This is accepted to prove byte preservation before product editing.
- The parser remains internal Rust behavior. Generated TypeScript and adapter
  contracts do not grow in Phase 1C.

## Rejected alternatives

- Make parser results public or serializable: it would freeze dependency-shaped
  internals and invite adapters to treat ranges as authority.
- Borrow source into a self-referential artifact or retain rowan handles: it
  complicates safety and retains an interior-mutable dependency object.
- Accept `&[u8]` and copy the complete accepted Guide: avoidable 2 MiB source
  duplication.
- Trust yaml-edit scalar types, Saphyr tag helpers, markers, messages, or line
  numbers: none is Rehow's policy or safe byte coordinate.
- Use one parser as a fallback when the other fails: malformed input would gain
  ambiguous semantics or presentation authority.
- Treat parser disagreement as writable or silently normalize it: violates
  byte preservation and unknown metadata trust.
- Treat every inter-entry comment as globally ambiguous: unnecessarily disables
  unrelated edits; exact boundary correspondence is sufficient.
- Add regex, bigint, property-test, snapshot, or fuzz dependencies: the fixed
  grammars, arbitrary-decimal operations, and deterministic harness do not
  require them in this phase.
- Add configurable production limits or a public fault injector: creates a
  limit-bypass and artifact-forgery surface.
- Map parser failures directly to `ErrorDto`: Phase 1C does not own a confined
  path, request ID, product entity kind, or public factory authority.

## Review triggers

Architect/xhigh and Security Reviewer/xhigh review is mandatory before:

- changing either entry signature, ownership model, module visibility, retained
  artifact field, coordinate system, or malformed/read-only split;
- changing a source/semantic meter, precedence rule, scalar grammar,
  normalization algorithm, implicit-empty rule, comment algorithm, or
  dependency call allowlist;
- enabling a dependency feature, changing any of the three pins, introducing a
  parser/property/fuzz/regex/bigint dependency, or retaining a dependency
  object in an artifact;
- exposing a parser/artifact type through Rust public API, Serde, TS, Tauri, or
  MCP;
- adding filesystem/path handling, ErrorDto construction, product known-field
  classification, YAML emission, span edits, or mutation to Phase 1C; or
- weakening a failure to accepted read-only or accepted writable behavior.

Reviewer/high closure is also required for API minimality, fixture completeness,
trait surface, deterministic precedence, and evidence accuracy. Any unresolved
parser panic, unbounded allocation/work, dependency advisory, or inability to
prove complete leaf coverage is a phase blocker, not a deferral.

## Sources

- [YAML 1.2.2 core schema and tag resolution](https://yaml.org/spec/1.2.2/#103-core-schema)
- [`saphyr-parser` 0.0.12](https://crates.io/crates/saphyr-parser/0.0.12), packaged source revision `5c45acd365c711e3d92f0ed4a0ceabe349cde514`
- [`yaml-edit` 0.3.1](https://crates.io/crates/yaml-edit/0.3.1), packaged source revision `4cbd1d452f0bc5bbb0f92d558c30191180754c2a`
- [`rowan` 0.16.1](https://crates.io/crates/rowan/0.16.1)
- [RustSec advisory database](https://rustsec.org/advisories/)

# Phase 1C parser matrix evidence

Date: 2026-09-07 (Africa/Algiers)

Status: Boundary 4 candidate gate passed; independent final review remains the
Orchestrator's acceptance step.

## Failure reachability

`all_41_failure_kinds_have_one_reachability_class` exhaustively assigns the
closed 41-kind enum to 32 natural, three dependency-differential, and six
invariant/fault-only classes. `all_32_natural_failure_classes_have_production_parse_witnesses`
then exercises every natural class through a production entry.

| Class | Failure kinds | Concrete evidence |
| --- | --- | --- |
| Natural source/envelope | `SourceTooLarge`, `FrontmatterTooLarge`, `InvalidUtf8`, `RawBomDenied`, `BareCarriageReturn`, `MixedYamlLineEndings`, `MissingGuideOpeningDelimiter`, `MissingGuideClosingDelimiter`, `MissingGuideSeparator` | `checked_in_byte_descriptors_materialize_endings_and_invalid_bytes_exactly`, `frontmatter_and_guide_byte_caps_report_exact_facts`, `guide_envelope_failures_are_distinct` |
| Natural presentation policy | `PresentationSyntax`, `PresentationShape`, `DirectiveDenied`, `DocumentMarkerDenied`, `AnchorDenied`, `AliasDenied`, `ExplicitTagDenied`, `ExplicitTagKindMismatch`, `ExplicitTagValueMismatch`, `MergeKeyDenied`, `InvalidUnicodeScalar` | `presentation_syntax_and_shape_have_natural_production_witnesses`, `source_constructs_are_denied`, `literal_core_tags_reject_kind_and_value_mismatches`, `decoded_noncharacters_are_rejected` |
| Natural semantic structure | `RootMustBeMapping`, `TopLevelKeyMustBeString`, `DepthLimit`, `MappingEntryLimit`, `SequenceEntryLimit`, `NodeLimit`, `DuplicateMappingKey` | `mapping_root_and_string_top_keys_are_required`, `semantic_depth_and_collection_limits_accept_equality_and_refuse_one_over`, `semantic_node_limit_accepts_equality_and_refuses_one_over`, `duplicate_semantic_keys_are_rejected` |
| Natural scalar/resource | `RawScalarLimit`, `DecodedScalarLimit`, `CanonicalScalarLimit`, `NumericWorkLimit`, `NumericDigitLimit` | `checked_in_limit_descriptors_materialize_exact_and_one_over_natural_sources` plus the five focused equality/one-over tests in `limits` |
| Dependency differential | `PresentationCoverage`, `SemanticSyntax`, `ParserTagDisagreement` | `strict_leaf_machine_rejects_coverage_differentials`, `later_scan_failure_wins_an_earlier_semantic_root_failure`, guarded-tail tests |
| Invariant/fault-only | `SemanticStreamOrder`, `MultipleDocuments`, `SemanticEncodingLimit`, `SemanticComparisonLimit`, `MeterArithmeticOverflow`, `SemanticBuilderInvariant` | stream/event tests; `production_fold_exercises_exact_and_one_over_for_all_four_active_meters`; `production_fold_attributes_all_builder_overflows_to_the_injected_active_meter`; mirror/index/event-cap tests |

## Named corpus and limits

The checked-in corpus contains 27 files under `accepted`, 35 under `malformed`,
and 11 under `read-only` (including its README). Thirty narrowly named textual
recipes describe byte-sensitive or large inputs. The private deterministic
loader materializes only bounded in-memory `Box<[u8]>` values from `hex`,
`repeat`, and named structural operations. Tests assert descriptor metadata,
terminal-newline/line-ending facts, and exact failure scope/limit/first-refused
facts. No generated blob is checked in.

- `named_checked_in_accepted_corpus_parses_through_production_entries` covers
  both entry APIs, BOM, Guide/whole YAML, styles, built-in tags, complex keys,
  comments, implicit empties, mixed flow/block nesting, scalar and numeric
  vectors.
- `checked_in_byte_descriptors_materialize_endings_and_invalid_bytes_exactly`
  covers CRLF, no-final-newline, and lossless invalid-byte placements.
- `checked_in_limit_descriptors_materialize_exact_and_one_over_natural_sources`
  covers whole YAML, Guide, frontmatter, raw/decoded/canonical scalar, numeric
  work/digits, depth, mapping entries, sequence entries, and nodes. Equality is
  accepted and one-over reports the exact fixed facts.
- Builder encoding/comparison cannot be reached naturally beneath the source
  cap; their exact/one-over and overflow cases use only the Phase 1B meter
  bridge on real production fold events.

## Ordering, tail, and scalar evidence

`every_adjacent_global_precedence_boundary_from_stage_5_through_14_is_exact`
uses production selectors for every adjacent pair, including combined 5>6 and
8>9. Stages 1–4 are covered by
`whole_scanner_selects_every_adjacent_fixed_fact_precedence`; cross-stage cases
are `byte_policy_precedes_parser_failures`,
`later_scan_failure_wins_an_earlier_semantic_root_failure`,
`later_matched_merge_wins_an_earlier_collection_limit`,
`guarded_tail_preserves_or_overrides_the_earlier_node_limit`, and
`collection_top_level_key_precedes_same_event_depth_failure`.

The required stable production-reducer tests are present verbatim:

- `guard_source_none_event_merge`
- `guard_source_merge_event_none`
- `guard_shifted_saphyr_only_merge`
- `guard_shifted_saphyr_only_merge_after_node_20000`
- `tail_borrowed_text_repeated_scalar_maxima_at_limit`
- `tail_borrowed_text_one_over_is_invariant`
- `tail_borrowed_text_counter_overflow_is_invariant`
- `tail_borrowed_text_limit_arithmetic_overflow_is_invariant`
- `tail_borrowed_text_disable_stops_all_later_scans`
- `tail_borrowed_text_invariant_then_scan_error`
- `tail_borrowed_text_invariant_then_stream_error`
- `tail_borrowed_text_invariant_then_exact_tag_disagreement`
- `tail_borrowed_text_invariant_then_shifted_source_guard`
- `tail_borrowed_text_invariant_then_uninspectable_merge`

Beyond-20,000 guard/tag cases, rich-prefix/descriptor-tail capacity,
event-call stop, copied-event excess, one-pass borrowed retention, refused
allocation, and expansion-copy peaks have separate passing tests in `faults`
and `presentation`; this includes the additional
`explicit_tag_asymmetry_is_detected_before_and_after_node_20000` regression.
Scalar grammar and conversion are covered by
`yaml_12_core_scalar_resolution_matrix`,
`literal_core_grammar_accepts_only_the_normative_rows`,
`arbitrary_radix_and_signed_zero_normalize`, and the numeric precharge tests.

## Reconciliation and comments

`all_eight_node_dimensions_select_the_first_ordered_reason` proves the fixed
preorder comparison order. Root/key/value/nested breaks, entry-wrapper cutoff,
large rejected tails, semantic-only suffixes, and original decoded-allocation
moves are covered by the named `reconcile` tests. The exact range suite covers
whole/payload/tag/UTF-8/ancestor/sibling/rebase and entry outside/overlap/reverse
faults with no forged IDs.

`all_ten_read_only_reasons_are_reported_by_production_reconciliation` consumes
one named fixture for each of `NodeCorrespondenceMismatch`,
`ParentOrArityMismatch`, `ScalarStyleMismatch`, `DecodedScalarMismatch`,
`ScalarSourceMismatch`, `InvalidCandidateRange`,
`OverlappingSiblingRanges`, `NoOpRenderMismatch`,
`UnsupportedPresentation`, and `CommentOwnershipAmbiguous`.

Comment evidence covers Inline, Leading, Trailing, InterEntry, Nested, Flow,
DocumentOnly, Unresolved, crossed/ambiguous, dropped/unreconciled owners,
literal/folded scalar headers, adjacent runs and blank-line splits. Dense
newline `R=262144`, maximum comment population, long inline lines, deepest
nesting, and mixed flow/block fixtures all remain within the production bound
`12*S + 8*T + 12*N + 8*E + 8*C + 64`; `WorkOneOver` is refused before artifact
authority.

## Properties, allocations, and API

`fixed_seed_arbitrary_and_mutation_harness_4096_each` uses xorshift64 seed
`0x5eed_cafe_f00d_beef`, 4,096 arbitrary byte inputs and 4,096 mutations derived
from all 27 checked-in accepted materializations: ten direct YAML payloads, two
direct Guide payloads, and fifteen deterministic recipe payloads. Every source
is selected at least once; the remaining mutations cycle across the twelve
small direct payloads so the exact-limit recipes do not dominate runtime. Both
YAML and Guide entry APIs are exercised. Every `Ok` proves root and ID
resolution, the contiguous eight-dimension prefix (including raw value versus
the exact payload source bytes or `ScalarSourceMismatch`), parent/ancestor/
sibling and entry containment, exact roles and entry relations, unique and
aggregate child/entry formulas, comment ranges/owners, report primary/count/
truncation and the normative node-to-entry-to-comment-to-no-op total issue
order with same-ordinal enum order, semantic termination, and retained
count/copy bounds. No result is ignored.

`production_dependency_imports_and_calls_match_the_frozen_allowlist` exactly
enumerates imports and associated qualified calls, rejects unexpected direct
dependency namespaces, and freezes the complete rowan 0.16.1 public
structural/navigation/conversion method inventory for its typed and raw
node/token/element, cursor, `NodeOrToken`, offset/walk, and AST-pointer types.
Across all six production parser modules it rejects both dot and
qualified/UFCS spellings for every forbidden method, rejects qualified forms
of dot-approved calls, and independently counts generic associated suffixes
and their exact permitted qualifiers, including the one pinned internal
`semantic::parse` occurrence alongside `YamlFile::parse`. Addition and
replacement probes cover aliases and `<Type>`/UFCS forms. The scalar audit crosses its inline
test-meter `cfg` seams through the final production helper;
`production_parser_source_rejects_the_complete_forbidden_surface` and
`retained_records_have_no_dependency_handles_and_dead_code_reason_is_exact`
cover forbidden APIs/types, unsafe, pseudo lexers, and the one lint reason.
The private adapter exercises every allowed yaml-edit/rowan/Saphyr operation.
Normal rustdoc and generated TypeScript exact-name searches found no parser,
artifact, failure, coordinate, comment, round-trip, unknown-YAML, or test-control
surface. The public API test passed its Phase 1B positive and all 24 negatives,
including the six Phase 1C cases.

## Final commands and counts

All commands exited 0 on 2026-09-07:

- focused parser: 130 passed, 0 failed, 0 ignored, 13 filtered;
- full `rehow-core` library in the Workspace gate: 143 passed;
- Rust integration suites: 18 error-contract, 13 protocol-value, 11
  unknown-YAML, one public API/24 trybuild cases; desktop and MCP each one;
- Vitest: one file and one test passed;
- `cargo fmt --all --check`, locked Workspace Clippy with `-D warnings`, locked
  Workspace tests, core normal check, release Workspace build, Tauri no-bundle,
  frozen contracts generation/check, frozen pnpm materialization, pnpm check,
  unit/integration tests and build, bootstrap validation, and allowed-path
  whitespace audit.

The only warning was the pre-existing Tauri recommendation about an identifier
ending in `.app`; it does not affect this parser-only phase.

# Phase 1D-B review evidence

Date: 2026-09-18 (Africa/Algiers)

One combined independent correctness/security review inspected the actual Guide-binding diff and initially blocked acceptance. It confirmed the dependency identities and absence of filesystem, process, network, logging, Tauri, Git, or MCP authority, then reported these material issues:

- uppercase `MAILTO` and legacy hexadecimal/octal numeric hosts could pass URL checks;
- schema/Attachment integer wrong-kind classification and root-position lookup were incorrect;
- ADR 0014's proposed indexed diagnostics and separate binding meter conflicted with the already accepted root-only protocol and the small schema caps;
- the filename control-character policy was stricter than the proposed text;
- adversarial URL, type, root-shadowing, boundary, and public-surface tests needed expansion.

Resolution:

- URL validation now requires the exact lowercase `mailto:` prefix and matches the raw authority against `url::Host`, accepting IP literals only in canonical spelling. Tests include short, hexadecimal, octal, uppercase-scheme, userinfo, and zero-padded/default-port cases.
- Semantic integers distinguish wrong YAML kind from invalid range. Diagnostic position lookup is restricted to entries whose reconciled parent is the document root, with a nested-key shadowing regression test.
- Allocation refusal has the closed `ResourceLimit` kind. Output allocations reserve fallibly; temporary unknown-key collection allocation was removed; folded uniqueness uses an allocation-free deterministic sort. ADR 0014 now relies on existing parser work caps plus fixed schema ceilings, including at most 32,640 comparisons in either 256-item uniqueness scan.
- ADR 0014 retains ADR 0012's root-only schema-path catalog and documents collection-root reporting for nested failures. It also explicitly documents the filename-specific control/bidi restriction.
- Guide and nested aggregate accessors, private construction/mutation/Serde, Unicode fold behavior, exact body retention, optional-empty preservation, status companions, hostile URLs, Attachment metadata, wrong kinds/nulls, source positions, and collection/scalar boundaries have production-backed tests.

The correction gate is recorded in `PROGRESS.md`; Phase 1D-C mutation/emitter evidence remains out of scope and is not implied by this acceptance.

The same independent Security Reviewer performed a narrow closure inspection of the current corrected files and returned `PASS`: none of its three prior high findings remains.

# Phase 5D2A bounded sync ancestry evidence

Core now consumes the opaque Phase 5D1 inspection and proves ancestry using bounded, configuration-free history reads. It deepens only the exact validated remote OID in private staging to 4,096 commits, confirms the validated remote tree did not change, and never grants the network child access to the active Workspace or repository.

The classifier reports remote-fast-forward or local-ahead only when one bounded history contains the other head. It reports divergence only when both histories are complete; shallow or capped histories produce `historyLimit`. Strict parsing rejects malformed, duplicate, wrong-format, or wrong-head output. Local and staged shallow state is identity/content/metadata bound around history reads, local plan binding is checked before and after the operation, and cancellation is checked immediately before every successful return. Deepening is constrained by verified remaining staging capacity, conservative per-file limits, metadata headroom, and a final aggregate rescan.

Verification covers the classification matrix, strict history and shallow-state parsing, and the exact `--depth=4096` fetch boundary. The locked all-feature core gate passed 399 tests with 2 explicit network/performance ignores, plus 18 error-contract tests, 13 protocol tests, the public-API/trybuild suite, and 11 unknown-YAML tests. Strict all-target/all-feature Clippy, formatting, bootstrap validation, and whitespace checks also passed.

The single reused Security Reviewer identified and then verified corrections for terminal cancellation, shallow-state races, and transient deepening capacity. Final verdict: **PASS**.

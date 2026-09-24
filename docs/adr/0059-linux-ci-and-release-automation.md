# ADR 0059: Linux CI and Existing-Release Asset Publication

Status: accepted

Date: 2026-09-24

## Context

Rehow needs repeatable pull-request verification and release builds without overstating platform support or exposing a publication credential to repository code. The current application intentionally depends on Linux descriptor, process, sandbox, portal, and packaging primitives. Omarchy uses Arch Linux, while the currently qualified packages are Debian and RPM formats.

## Decision

- CI runs on Ubuntu 24.04 for pull requests, pushes to `master`, and manual dispatch. It installs the repository's exact Node, pnpm, and Rust toolchains, then runs formatting, strict Clippy, contracts, unit/integration tests, the production Tauri build, both package smoke tests, ABI checks, and durable bootstrap validation.
- Every external GitHub Action is pinned to an immutable full commit. CI has only `contents: read` permission and no publication credential.
- Release automation runs when an existing GitHub Release is published and may be retried manually only for an existing published tag. It never creates or pushes a tag, creates a release, or selects a different commit.
- The release tag must be a `v`-prefixed semantic version that exactly matches `tauri.conf.json`. The checked-out tag must identify the built commit.
- The build job has read-only repository permission. It produces exactly the `.deb`, `.rpm`, and SHA-256 checksum file and transfers them through a short-lived workflow artifact.
- A separate publish job has `contents: write`, checks out and executes no repository code, validates the exact three-file inventory, names, checksum grammar, and hashes, confirms the GitHub Release is published, and uploads only those explicit filenames.
- Version `0.1.x` publishes Linux x86_64 `.deb` and `.rpm` assets. Omarchy needs the same application code but a future reviewed AUR/PKGBUILD or independently qualified AppImage. Windows and macOS remain unsupported until their filesystem/process/security adapters and native packaging are deliberately implemented and tested.
- Checksums are not publisher signatures. Package signing, attestations, updater metadata, AUR publication, and other credentials remain separate human-approved decisions.

## Consequences

Pull requests exercise the real native packaging path rather than only browser or compile checks. Publishing a GitHub Release deliberately becomes the trigger for asset construction, so a new release page may exist briefly before assets arrive and a failed build leaves the release without new binaries. A manual retry is safe and replaces only the same three Rehow assets.

The workflow is truthful about platform support. Omarchy users cannot install the current `.deb` or `.rpm` natively, and adding Windows/macOS to a matrix is not accepted as platform support.

## Evidence

Local validation parses both workflow files, proves all eight action references are immutable 40-hex commits, exercises accepted tag/version matching, verifies exact artifact inventory and checksum behavior against the real `0.1.0` packages, and passes the complete local CI command sequence. The first remote GitHub Actions run remains authoritative for hosted-runner dependency availability and workflow-context behavior.

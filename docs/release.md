# Release process

Status: Linux x86_64 automation implemented and `v0.1.0` published; signing remains a deliberate maintainer decision
Last updated: 2026-09-24

## Supported release platform

Rehow `0.1.x` releases target Linux x86_64 and attach these native packages:

- Debian/Ubuntu: `Rehow_<version>_amd64.deb`
- Fedora/RPM-family: `Rehow-<version>-1.x86_64.rpm`
- Arch/Omarchy: `rehow-<version>-1-x86_64.pkg.tar.zst`

Omarchy is based on Arch Linux and uses the Arch package; it does not need a separate Omarchy build. The Arch package is created by a reviewed `makepkg` recipe inside an immutable-digest Arch `base-devel` container with networking disabled. Its `/usr` payload is byte-identical to the already inspected Debian payload, and disposable `pacman` install/remove coverage is required. Do not tell users to convert or force-install another distribution's package.

Windows and macOS are not release targets. The current core and adapters deliberately use Linux/Unix filesystem descriptors, `memfd`, XDG portals, `/usr/bin/bwrap`, and Unix process controls. Supporting another operating system requires a designed security-portability phase, native integration tests, packaging, and the platform's signing requirements—not merely another CI matrix row.

## Pull-request CI

`.github/workflows/ci.yml` runs for every pull request, every push to `master`, and manual `workflow_dispatch`. It uses Ubuntu 24.04 and the repository's exact Node, pnpm, and Rust versions, then runs:

1. Cargo formatting and strict all-target/all-feature Clippy.
2. Type, lint, contract, unit, and integration checks.
3. The production Tauri build for `.deb`/`.rpm` and native Arch packaging from the same payload.
4. Bundle layout, cross-format payload identity, Arch metadata, embedded helper digest, MIT license, desktop entry, icon, and glibc 2.39 ABI checks.
5. Durable bootstrap validation and a clean generated-file diff.

The workflow has read-only repository permission, does not receive release permission, and pins every external action to an immutable commit.

## Publishing a GitHub Release

The repository does not create tags or releases automatically. To publish version `0.1.0`:

1. Ensure the version in `apps/desktop/src-tauri/tauri.conf.json`, Cargo metadata, package metadata, documentation, and expected package paths is `0.1.0`.
2. Ensure CI passes for the exact signed commit intended for release.
3. Create the Git tag `v0.1.0` at that commit and push it deliberately.
4. Create a GitHub Release for the existing `v0.1.0` tag, write the release notes, and publish it.
5. The `Release` workflow verifies that the tag is a semantic version and exactly matches the Tauri version, rebuilds and retests the source, and uploads the `.deb`, `.rpm`, `.pkg.tar.zst`, and `SHA256SUMS` file to that already-published release.

The `published` event intentionally covers both stable releases and pre-releases. Assets appear after the build finishes, not at the instant the release page is published.

To retry a failed asset build, run the `Release` workflow manually from GitHub Actions and supply the existing published tag. The workflow revalidates the release and replaces only same-named Rehow assets.

Do not move an already published tag to adopt newer release automation. A tag predating a packaging change must be amended only from its existing released payload through an explicitly reviewed maintainer procedure, or superseded by a new version.

## Release security boundary

The release workflow separates building from publication:

- The build job has `contents: read` only and runs repository code.
- It transfers the three packages and checksums through a short-lived private workflow artifact.
- The publish job has `contents: write`, does not check out or execute repository code, verifies artifact names and SHA-256 checksums, confirms the release is not a draft, and uploads with GitHub CLI.

Checksums detect corruption but are not cryptographic publisher signatures. Package signing, artifact attestation, and updater metadata remain disabled until their credentials, key custody, rotation, and recovery policy are explicitly approved. The workflow never pushes code, creates a tag, creates a release, or stores a long-lived credential.

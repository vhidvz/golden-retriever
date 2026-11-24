---
name: release-direction
description: Phase 9 Linux implementation complete and CI-verified 2026-11-24; all five formats, SHA-256/SBOM, optional GPG signing, pinned Actions and download-only upgrade; Windows unsupported, tag publish/AppImage UX open
metadata:
  type: project
---

Recorded 2026-11-23; the Phase 9 Linux implementation was completed and locally verified
on 2026-11-24 at the owner's request. The durable record is
`docs/release-direction.md` and the binding decisions are in `docs/decisions.md`.

- Artifacts: `.deb`, `.rpm`, Arch `.pkg.tar.zst`, AppImage, generic `.tar.zst`, plus
  `SHA256SUMS` and a CycloneDX 1.5 SBOM. Windows is **explicitly unsupported** for this
  release line: a native port of the Unix-socket/systemd/desktop-bridge daemon is a
  separate project, so no MSI/EXE is produced.
- CI: tag/manual GitHub Actions workflow; every action is pinned by commit SHA; the
  default token is read-only and only `publish` gets `contents: write`; each package job
  runs `scripts/verify-package.sh`; the metadata job builds one checksum manifest and the
  SBOM and signs only if a GPG key secret is configured. Manual run 45322812499 passed
  tests, all five package jobs and metadata; the tag-only publish/upload path is still
  unexercised.
- All five Linux formats (tar, DEB, RPM, Arch, AppImage) were built and payload-verified
  locally. Packaging fixes: system packages install under `/usr`; the binary now finds
  `/usr/share/buddy/renderer` and `/usr/share/buddy/perception` from `/usr/bin/buddy`,
  and `buddy-ui` resolves its UI relative to itself. The AppImage does not bundle Godot
  or Ollama; they are supplied separately on first run. Arch uses a project-generated
  PKGBUILD with a real source checksum.
- Updates: `buddy version` embeds the git tag/commit/dirty state via
  `crates/buddy/build.rs`; `buddy update check`/`update tools` only report. Signing is
  unsigned SHA-256 manifests by default, with an optional detached GPG signature.
  `buddy upgrade` downloads the portable user-local artifact, verifies it against the
  release's `SHA256SUMS`, and prints manual replacement commands; it refuses system
  packages/checkouts and never installs.
- Package ownership: system packages own `/usr`; user data stays in
  `~/.local/share/buddy`; no automatic migration.

**Why:** release automation changes distribution, permissions and supply-chain trust; it
must not be smuggled into feature work or claim completion without a tagged CI run.

**How to apply:** do not add Windows packaging until a daemon port exists; do not add
silent self-update. Optional GPG signing was tested locally; verify the workflow in
GitHub Actions and the AppImage first-run path before calling Phase 9 complete. Related: [[self-contained-ai]], [[control-panel]],
[[merge-completed-tasks-into-main]].

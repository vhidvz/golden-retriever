---
name: release-direction
description: Phase 9 started 2026-11-24; Linux release pipeline hardened with relocatable packages, SHA-256 manifests, SBOM, pinned Actions and download-only upgrade; Windows explicitly unsupported, workflow/signing still open
metadata:
  type: project
---

Recorded 2026-11-23; Phase 9 started 2026-11-24 at the owner's request. The durable
record is `docs/release-direction.md` and the binding decisions are in
`docs/decisions.md`.

- Artifacts: `.deb`, `.rpm`, Arch `.pkg.tar.zst`, AppImage, generic `.tar.zst`, plus
  `SHA256SUMS` and a CycloneDX 1.5 SBOM. Windows is **explicitly unsupported** for this
  release line: a native port of the Unix-socket/systemd/desktop-bridge daemon is a
  separate project, so no MSI/EXE is produced.
- CI: tag/manual GitHub Actions workflow; every action is pinned by commit SHA; the
  default token is read-only and only `publish` gets `contents: write`; each package job
  runs `scripts/verify-package.sh`; publish regenerates one checksum manifest and the
  SBOM. The workflow has still not run in Actions, so it is not proven end to end.
- Packaging fixes: system packages install under `/usr`; the binary now finds
  `/usr/share/buddy/renderer` and `/usr/share/buddy/perception` from `/usr/bin/buddy`,
  and `buddy-ui` resolves its UI relative to itself. The AppImage does not bundle Godot
  or Ollama; they are supplied separately on first run. Arch uses a project-generated
  PKGBUILD with a real source checksum.
- Updates: `buddy version` embeds the git tag/commit/dirty state via
  `crates/buddy/build.rs`; `buddy update check`/`update tools` only report. Signing is
  unsigned SHA-256 manifests for now. `buddy upgrade` downloads the portable user-local
  artifact, verifies it against the release's `SHA256SUMS`, and prints manual replacement
  commands; it refuses system packages/checkouts and never installs.
- Package ownership: system packages own `/usr`; user data stays in
  `~/.local/share/buddy`; no automatic migration.

**Why:** release automation changes distribution, permissions and supply-chain trust; it
must not be smuggled into feature work or claim completion without a tagged CI run.

**How to apply:** do not add Windows packaging until a daemon port exists; do not add
silent self-update. Verify the workflow in Actions and the AppImage first-run/signing
work before calling Phase 9 complete. Related: [[self-contained-ai]], [[control-panel]],
[[merge-completed-tasks-into-main]].

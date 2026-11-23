---
name: release-direction
description: Phase 9 requirements for GitHub Actions release artifacts and opt-in updates for Buddy and its managed tools; not yet approved
metadata:
  type: project
---

Recorded 2026-11-23 at the owner's request. Phase 9 now has a **Linux implementation
foundation**; Windows and signing remain open, and the GitHub workflow has not yet run in
Actions. The durable record is `docs/release-direction.md`.

- Target artifacts: `.deb`, `.rpm`, Arch package, AppImage, generic Linux tarball and a
  Windows installer. Windows is blocked on a daemon port because Unix sockets, systemd,
  the GNOME extension and the managed Ollama service are Linux-specific.
- CI: tag/manual GitHub Actions release workflow, test/package matrix, pinned actions,
  least privilege, checksums, SBOM and optional signatures; no release secrets on PRs.
- Updates: `buddy version` and an opt-in `buddy update check`/guided upgrade; managed
  Godot, Ollama and model digests get checks via the existing runtime manifest and
  `buddy ai doctor`/`ai status`. No background polling or silent updates.
- Package ownership of the current user-local data directory and the AppImage's
  Godot/Ollama bundling policy are open decisions.
- Implemented foundation: `scripts/package.sh` (tar/DEB/RPM/Arch/AppImage),
  `.github/workflows/release.yml` (tag or manual, tests + package + release upload), and
  `buddy version`/`update check`/`update tools`. No automatic update.

**Why:** release automation and an updater affect security, signing, permissions and
installation semantics; they must not be smuggled into a feature branch.

**How to apply:** extend the Linux foundation first; do not add a Windows package until
the daemon port exists, and do not add silent self-update. Verify the workflow in Actions
before calling Phase 9 complete.
Related: [[self-contained-ai]], [[control-panel]], [[merge-completed-tasks-into-main]].

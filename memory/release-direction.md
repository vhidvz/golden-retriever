---
name: release-direction
description: Phase 9 requirements for GitHub Actions release artifacts and opt-in updates for Buddy and its managed tools; not yet approved
metadata:
  type: project
---

Recorded 2026-11-23 at the owner's request. Phase 9 is **requirements only, not approved
and not implemented**. The durable record is `docs/release-direction.md`.

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
- The project rule still applies: spike, written plan and explicit approval before
  implementation. `docs/decisions.md` records the same status.

**Why:** release automation and an updater affect security, signing, permissions and
installation semantics; they must not be smuggled into a feature branch.

**How to apply:** use `docs/release-direction.md` as the input to the approval plan; do
not add workflows, packaging scripts or update network calls until that plan is approved.
Related: [[self-contained-ai]], [[control-panel]], [[merge-completed-tasks-into-main]].

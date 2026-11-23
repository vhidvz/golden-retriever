---
name: helper-registry
description: Phase 7 Stage 1 named helper scripts are implemented with argv-only execution, dry-run, approved cwd, timeout and always-ask permission
metadata:
  type: project
---

Implemented 2026-11-23 as the first safe slice of Phase 7.

- Manifests live in `$BUDDY_DATA_DIR/helpers/*.json`; `crates/buddy/src/helpers.rs` loads
  and validates them. Commands are argv arrays, never shell strings. `{arg}` placeholders
  are substituted with separate values; cwd must be inside the helper folder or an
  approved `buddy folders` directory; timeout is 1–600 s.
- CLI: `buddy helpers`, `buddy helper dir|show|dry-run|run NAME [--arg k=v]`.
- `run_helper` is a sensitive skill, so every execution asks first and cannot be set to
  allow. `helper dry-run` executes nothing. Output is captured/trimmed and the journal
  records the name/outcome, not the full output.
- Not implemented: model-driven helper selection, a true sandbox, and synthetic input.
  The model tool list deliberately omits `run_helper` until a helper catalogue is attached.
- Tests cover rendering, dry-run, no-shell argument handling, validation and missing args.
  Phase 7’s remaining work is a separate slice. See `docs/helpers.md` and
  `docs/automation-direction.md`.

**Why:** deterministic named helpers are safer and more reliable than letting a model
generate shell commands; the owner's helper code still requires explicit approval.

**How to apply:** extend the manifest/registry only with argv-safe execution and explicit
permissions. Do not add synthetic input or raw command strings without a new decision.
Related: [[phase5-completion]], [[release-direction]], [[context-direction]].

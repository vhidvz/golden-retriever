---
name: phase5-completion
description: Phase 5 is complete — accessible text entry, mixed application/inside-app workflows, assisted manual steps for no-tree apps, and abort/continue controls
metadata:
  type: project
---

Completed 2026-11-23. Phase 5's remaining workflow gaps are implemented:

- **Text entry:** a `type_text` step locates one uniquely identified editable AT-SPI
  field, clears it, replaces it with text supplied on stdin, and verifies an insertion
  event. Password fields, terminals, shells and ambiguous fields are refused. The text
  is stored in the workflow and export; docs warn not to use secrets. Commands:
  `buddy workflow type-step "TEXT" --delay 5`, `buddy workflow capture-step`.
- **Mixed sequences:** replay walks application, accessible-action, text and manual
  steps in order, opening the expected app and waiting for focus before each helper.
  Capture inserts an explicit app step before a new app. Legacy app-only and action-only
  workflows still replay.
- **Apps without an accessibility tree:** explicit assisted `manual` steps. Buddy brings
  the app forward and waits for the user to do the work, then `buddy workflow continue`;
  it never blind-clicks, sends keys or uses coordinates. `buddy workflow abort` releases
  a stuck replay. Manual steps do not require the accessibility sense; accessible and
  text steps do.
- **Evidence:** 110 Rust tests; isolated daemon runs exercised text+mixed replay and two
  consecutive manual steps; the GTK panel smoke path passes. Durable decisions are in
  `docs/decisions.md` "Phase 5 completion and names"; commands and limits are in
  `README.md` and `docs/development.md`.

**Why:** finish the roadmap's Phase 5 without weakening the rule that Buddy never
injects blind input into applications.

**How to apply:** extend the step sequence model in `crates/buddy-core/src/workflow.rs`
and the daemon state machine; keep every accessible helper semantic and verified, and
keep no-tree apps on the explicit manual path. Related: [[workflow-recording]],
[[control-panel]], [[name-recognition]].

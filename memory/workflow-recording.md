---
name: workflow-recording
description: "Application-level workflow recording and verified replay are implemented; inside-application accessible actions are researched and planned"
metadata:
  node_type: memory
  type: project
  originSessionId: 5b57146d-145d-465d-8b74-d74014873801
  modified: 2026-11-23T00:00:00.000Z
---

On 2026-11-23 the next roadmap step was chosen: the remaining Phase 5 item,
recording and replaying workflows with per-step verification. Milestone 1 is
implemented and committed in `2853733`:

- `crates/buddy-core/src/workflow.rs` owns `Workflows`, `Recording`, `WorkflowStep`
  foundations and `ReplayState`; `Mind.workflows` persists it.
- `Sim` records focus events while a recording is active.
- The daemon exposes `workflow_record`/`workflows`/`workflow`/`workflow_delete` and a
  permissioned `replay_workflow` skill; it opens each application and advances only when
  a real focus event matches the expected step. Timeouts record failures.
- CLI: `buddy workflows`, `buddy workflow record/stop/cancel/show/replay/delete`.
- The skill count is now eight; `replay_workflow` is `Risk::Changes` and asks first.

Milestone 2 research is in `spikes/workflow-a11y/`. AT-SPI is present and accessible
actions work (a GTK4 button action changed a label), but a programmatic action emitted no
dedicated action event, so inside-app recording must be **guided capture**, not raw
event/keystroke logging. Some apps expose no tree. No text entry, password fields,
terminals or coordinate replay are in scope. This design is recorded durably in
`docs/decisions.md` and `docs/development.md`; no separate plan file is kept as a project
artifact.

**Why:** the roadmap's next item, and the owner asked for research to mature the plan
before implementation. The accessibility path is viable but needs a permissioned sense.

**How to apply:** keep replay permissioned and focus-verified; never fall back to
coordinates or synthetic keys. Read `docs/decisions.md` and
`spikes/workflow-a11y/RESULTS.md` before implementing Milestone 2. Related:
[[buddy-additional-requirements]], [[self-contained-ai]],
[[tracked-docs-no-untracked-refs]].

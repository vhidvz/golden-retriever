# Workflow Recording and Replay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Buddy record a short routine of applications the user works through, replay it on request, and verify that each application actually came to the front.

**Architecture:** A new `workflow` module in `buddy-core` owns the workflow and recording state; `Sim` records focus events while a recording is active and persists workflows in `Mind`. The daemon opens each step with the existing `open_app` skill, tracks the application currently in front from the desktop sense, and advances a replay only when that application matches the expected step.

**Tech Stack:** Rust, serde, existing SQLite store, existing Unix-socket CLI, existing GNOME Shell focus events.

**Spec:** `README.md` Roadmap Phase 5 and `docs/decisions.md` additional requirements 1–3 and 5. This plan is **Milestone 1**: application-level routines and per-step verification. Inside-application keystrokes and clicks need AT-SPI/input work and are specified separately in `2026-11-23-workflow-accessibility.md`.

**Status:** Milestone 1 Tasks 1–4 are implemented in the working tree; the commit steps await the owner's explicit commit request. Milestone 2 is researched and specified, not implemented.

## Global Constraints

- Local first: recording and replay never depend on Ollama or the network.
- Replay is a permissioned action: it asks first, supports `--always`/`--never`, and is journaled.
- The daemon is the only decision-maker; `workflow` is pure data and `Sim` is pure simulation.
- Every replay step is verified against a real focus event; a timeout is a failed step, not a silent success.
- No coordinate replay, no synthetic keystrokes and no clicks in this milestone.
- English and Persian commands are equal; all user-facing text goes through the existing CLI.
- No new dependency is required.

## Review Focus

- A user starts recording while already in an application: the first focus change must not lose the starting app, and stopping must include the app currently in front.
- An application is already focused when a replay step starts: the step must pass without waiting for a focus event that will never arrive.
- A replay step opens the wrong application or the user switches away: the step must time out or fail, and the workflow must record a failure rather than claiming success.
- Recording is cancelled, the daemon restarts, or the database is opened by an older version: no half-written workflow may appear, and existing `Mind` JSON must still load.
- Replay is refused, lapses, or is interrupted by privacy mode/quiet mode: no application should be opened after the refusal.

## Research findings (2026-11-23)

The accessibility spike in `spikes/workflow-a11y/` tested this GNOME/Wayland session:

- The AT-SPI bus and `Atspi 2.0` Python bindings are present; a GTK4 probe exposed a full tree even with `toolkit-accessibility=false`.
- A button's accessible action (`do_action(0)`) worked and changed the label from `ready` to `pressed`.
- Editable text could be set, and `object:text-changed` events arrived.
- A programmatic action did **not** emit a dedicated action/pressed event; only the label's accessible-name change was observed. Raw event recording is therefore unreliable.
- Some applications expose no accessible tree (Telegram Desktop had zero children).
- `xdotool` cannot see native Wayland windows; `ydotool`/`wtype` are absent. `/dev/uinput` is user-accessible, so synthetic input is possible but is not the chosen path.

Consequence: inside-app recording must be **guided capture** (the user marks the focused control), not blind click/keystroke logging. Replay can locate by app, frame title, role and accessible name, invoke the action, and verify a semantic event or state change. Text entry is a separate, later spike. The detailed Milestone 2 plan is `2026-11-23-workflow-accessibility.md`.

---

### Task 1: Workflow and recording data model

**Files:**
- Create: `crates/buddy-core/src/workflow.rs`
- Modify: `crates/buddy-core/src/lib.rs` (add `pub mod workflow;`)
- Modify: `crates/buddy-core/src/sim.rs` (`Mind.workflows`, `Sim.recording`, methods, focus hook)
- Test: tests inside `workflow.rs` and `sim.rs`

**Interfaces:**
- Produces:
  - `pub struct Workflow { pub id: u64, pub name: String, pub apps: Vec<String>, pub created_at: f64, pub last_run: Option<f64>, pub runs: u32, pub successes: u32, pub failures: u32, pub standing_yes: bool, pub never: bool }`
  - `pub struct Workflows { pub items: Vec<Workflow>, next_id: u64 }` with `record(&mut self, name: &str, apps: &[String], now: f64) -> Result<Workflow>`, `list(&self) -> &[Workflow]`, `get(&self, id: u64) -> Option<&Workflow>`, `find(&self, name: &str) -> Option<&Workflow>`, `get_mut(&mut self, id: u64) -> Option<&mut Workflow>`, `remove(&mut self, id: u64) -> bool`, `outcome(&mut self, id: u64, ok: bool, now: f64)`.
  - `pub struct Recording { pub name: String, pub started_at: f64, apps: Vec<String>, pending: Option<(String, f64)> }` with `new`, `observe(&mut self, app: &str, now: f64)`, `finish(self, now: f64) -> Vec<String>`, `cancel`.
  - `Sim::start_workflow_recording(&mut self, name: String, now: f64) -> Result<()>`, `Sim::stop_workflow_recording(&mut self, now: f64) -> Result<Workflow>`, `Sim::cancel_workflow_recording(&mut self) -> bool`, `Sim::workflows(&self) -> &[Workflow]`, `Sim::workflow_mut(&mut self, id: u64) -> Option<&mut Workflow>`.
  - `Mind.workflows: Workflows` (serde default, so old `mind` JSON still loads).

- [x] **Step 1: Write the failing workflow tests**

```rust
#[test]
fn a_recording_keeps_the_apps_the_user_stayed_on() {
    let mut recording = Recording::new("morning", 100.0);
    recording.observe("terminal", 100.0);
    recording.observe("editor", 101.0);   // terminal stayed only 1s: a fly-by
    recording.observe("browser", 130.0);  // editor stayed 29s
    assert_eq!(recording.finish(140.0), vec!["editor", "browser"]);
}

#[test]
fn a_workflow_needs_a_name_and_at_least_two_apps() {
    let mut workflows = Workflows::default();
    assert!(workflows.record("", &["a".into(), "b".into()], 0.0).is_err());
    assert!(workflows.record("x", &["a".into()], 0.0).is_err());
    let saved = workflows.record("x", &["a".into(), "b".into()], 0.0).unwrap();
    assert_eq!(saved.id, 1);
    assert_eq!(workflows.list().len(), 1);
}

#[test]
fn outcomes_are_counted_and_survive_json() {
    let mut workflows = Workflows::default();
    let id = workflows.record("x", &["a".into(), "b".into()], 0.0).unwrap().id;
    workflows.outcome(id, true, 10.0);
    workflows.outcome(id, false, 20.0);
    let restored: Workflows = serde_json::from_str(&serde_json::to_string(&workflows).unwrap()).unwrap();
    let saved = restored.get(id).unwrap();
    assert_eq!((saved.runs, saved.successes, saved.failures), (2, 1, 1));
    assert_eq!(saved.last_run, Some(20.0));
}

#[test]
fn focus_events_are_recorded_while_a_recording_is_active() {
    let mut s = sim(0);
    s.start_workflow_recording("morning".into(), 0.0).unwrap();
    s.sense(Stimulus::Focus { app: "terminal".into() }, 0.0);
    s.sense(Stimulus::Focus { app: "editor".into() }, 30.0);
    let workflow = s.stop_workflow_recording(40.0).unwrap();
    assert_eq!(workflow.apps, vec!["terminal", "editor"]);
    assert_eq!(s.workflows().len(), 1);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p buddy-core workflow`
Expected: FAIL with unresolved `Recording`/`Workflows` and missing `start_workflow_recording`.

- [x] **Step 3: Implement the module and the `Sim` hooks**

`workflow.rs`: validate a workflow with 2–8 non-empty application identifiers, drop consecutive duplicates, assign ids from `next_id`, and keep `list` in insertion order. `Recording::observe` mirrors `Patterns::focus`: a previous application becomes a step only after `MIN_DWELL_SECS` (2.0) and a different application comes to the front; `finish` appends the pending application and returns the steps. Use `anyhow::{bail, Result}`.

`Mind` gains `#[serde(default)] pub workflows: Workflows`. `Sim` gains a non-persisted `recording: Option<Recording>`; `Sim::new` and `Sim::from_mind` initialize it to `None`. In `Sim::sense`’s `Stimulus::Focus` arm, call `self.recording.as_mut().map(|r| r.observe(&app, now));` before the existing `patterns.focus` call. `stop_workflow_recording` takes the recording, finishes it, records it in `self.workflows`, and returns the saved workflow. `cancel_workflow_recording` clears it and returns whether one was active. `Sim::explain` adds `"workflows": self.workflows.list()`.

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p buddy-core workflow`
Expected: PASS (4 tests).

- [x] **Step 5: Run the full core suite**

Run: `cargo test -p buddy-core`
Expected: all tests pass; the old `mind` JSON with no `workflows` still loads.

- [ ] **Step 6: Commit**

```bash
git add crates/buddy-core/src/workflow.rs crates/buddy-core/src/lib.rs crates/buddy-core/src/sim.rs
git commit -m "feat: record application workflows"
```

### Task 2: Daemon commands and CLI

**Files:**
- Modify: `crates/buddy/src/daemon.rs` (workflow commands; `current_app` is added in Task 3)
- Modify: `crates/buddy/src/main.rs` (usage and routes)
- Modify: `crates/buddy/src/client.rs` (list/show output)
- Modify: `crates/buddy/src/daemon/assist.rs` (`replay_workflow` skill; added in Task 3)

**Interfaces:**
- Consumes: `Sim::start_workflow_recording`, `stop_workflow_recording`, `cancel_workflow_recording`, `workflows`, `workflow_mut`.
- Produces daemon commands:
  - `"workflow_record"` with `{ "action": "start"|"stop"|"cancel", "name": "..." }` → `Value`; `stop` returns the saved workflow; `start` returns `{ "recording": name }`.
  - `"workflows"` → `{ "workflows": [...] }`.
  - `"workflow"` with `{ "id": u64 }` or `{ "name": "..." }` → one workflow.
  - `"workflow_delete"` with `{ "id": u64 }` → `{ "deleted": id }`.
- CLI:
  - `buddy workflows`
  - `buddy workflow record NAME` / `buddy workflow stop` / `buddy workflow cancel`
  - `buddy workflow show NAME|ID`
  - `buddy workflow delete NAME|ID`
  - `buddy workflow replay NAME|ID` (wired in Task 3)

- [x] **Step 1: Write a failing client/output test**

Add a unit test for a pure formatter in `client.rs`:

```rust
#[test]
fn a_workflow_line_names_the_steps_and_the_record() {
    let value = serde_json::json!({
        "id": 1, "name": "morning", "apps": ["terminal", "editor"],
        "runs": 2, "successes": 1, "failures": 1
    });
    assert_eq!(workflow_line(&value), "1  morning  terminal > editor  (2 runs, 1 ok, 1 failed)");
}
```

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p buddy a_workflow_line`
Expected: FAIL with `workflow_line` not found.

- [x] **Step 3: Implement the daemon commands and CLI**

Add a `workflow_lines`/`workflow_line` helper in `client.rs` and a `workflows()` printer. In `main.rs`, route the commands above to `client::run`/`client::act`. In `daemon.rs`’s `command` match, implement the four immediate commands. `workflow_record start` errors if a recording is already active or the name is empty; `stop` errors if none is active; `cancel` returns whether one was cancelled and journals nothing. `workflows` and `workflow` read `sim.workflows()`. `workflow_delete` removes by id or name and persists.

- [x] **Step 4: Run the test to verify it passes**

Run: `cargo test -p buddy a_workflow_line`
Expected: PASS.

- [x] **Step 5: Manual check with a running daemon**

Run the daemon in a sandbox with `BUDDY_DATA_DIR` and `BUDDY_SOCKET`, then:

```bash
cargo run -- ai status        # daemon up, unrelated
cargo run -- workflow record morning
cargo run -- workflow stop
cargo run -- workflows
```

Expected: a workflow with the applications focused during the recording, at least two; `workflows` lists it; `workflow show id` prints it.

- [ ] **Step 6: Commit**

```bash
git add crates/buddy/src/daemon.rs crates/buddy/src/main.rs crates/buddy/src/client.rs
git commit -m "feat: record workflows from the CLI"
```

### Task 3: Permissioned replay with per-step focus verification

**Files:**
- Modify: `crates/buddy/src/skills.rs` (add `replay_workflow` skill and `describe`)
- Modify: `crates/buddy/src/daemon/assist.rs` (special-case replay in `carry_out`)
- Modify: `crates/buddy/src/daemon.rs` (`current_app`, `Replay`, `advance_replay`, command handler, tick loop)
- Test: `crates/buddy-core/src/workflow.rs` (pure `ReplayState`) and `crates/buddy/src/skills.rs` (skill policy)

**Interfaces:**
- `pub struct ReplayState { pub apps: Vec<String>, pub index: usize, pub started: f64 }`
  - `pub fn new(apps: Vec<String>, now: f64) -> Self`
  - `pub fn expected(&self) -> Option<&str>`
  - `pub fn check(&mut self, front: Option<&str>, now: f64, timeout: f64) -> ReplayEvent`
  - `pub enum ReplayEvent { Waiting, StepOk, Done, TimedOut }`
- `Skill { name: "replay_workflow", what: "Replay a saved workflow and check each step.", risk: Risk::Changes, arguments: &["workflow", "id"] }`
- `Daemon.current_app: Option<String>` set from `Stimulus::Focus` in `perceive`.
- `Daemon.replay: Option<Replay>` where `Replay { requester, workflow_id, name, state, args, step_started }`.
- `buddy workflow replay NAME|ID` → `client::act("replay_workflow", { "workflow": name, "id": id })`.

- [x] **Step 1: Write the failing replay-state tests**

```rust
#[test]
fn a_step_passes_only_when_the_expected_app_is_in_front() {
    let mut state = ReplayState::new(vec!["terminal".into(), "editor".into()], 0.0);
    assert_eq!(state.check(Some("browser"), 1.0, 20.0), ReplayEvent::Waiting);
    assert_eq!(state.check(Some("terminal"), 2.0, 20.0), ReplayEvent::StepOk);
    assert_eq!(state.expected(), Some("editor"));
    assert_eq!(state.check(Some("editor"), 3.0, 20.0), ReplayEvent::Done);
}

#[test]
fn a_step_that_never_appears_times_out() {
    let mut state = ReplayState::new(vec!["terminal".into()], 0.0);
    assert_eq!(state.check(None, 5.0, 20.0), ReplayEvent::Waiting);
    assert_eq!(state.check(None, 21.0, 20.0), ReplayEvent::TimedOut);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p buddy-core a_step_`
Expected: FAIL with unresolved `ReplayState`.

- [x] **Step 3: Implement `ReplayState` and the replay skill**

`ReplayState::check` returns `StepOk` and advances when `front == expected`; returns `Done` after the last step; returns `TimedOut` when `now - started > timeout`; otherwise `Waiting`. `started` resets on each successful step.

In `skills.rs`, add the eighth skill and `describe("replay_workflow")` → `replay the workflow "name" (N steps)`. In `assist.rs`, `carry_out` returns without a `Did` for `replay_workflow`; it calls `Daemon::start_replay(requester, args, at)`. `start_replay` looks up the workflow by id/name, checks `never`, sets `self.replay = Some(Replay { ... })`, and opens the first app only if `self.current_app` is not already that app. Add `advance_replay(at)` to the daemon loop after `sim.tick`: call `state.check(current_app, now, 20.0)`; on `StepOk`, open the next app and reset `step_started`; on `Done`, call `self.did(..., Ok((summary, detail, None)))`; on `TimedOut`, call `self.did(..., Err("... did not come to the front"))`. Record success/failure with `sim.workflows.outcome`. `perceive` sets `current_app` on every `Stimulus::Focus` before `sim.sense`.

- [x] **Step 4: Run the unit tests to verify they pass**

Run: `cargo test -p buddy-core a_step_` and `cargo test -p buddy replay_workflow`
Expected: PASS.

- [x] **Step 5: Manual replay test**

In a sandbox daemon, create a workflow with two applications, then:

```bash
cargo run -- workflow replay 1
cargo run -- approve
cargo run -- waiting
```

Expected: the first app opens; after it comes to the front, the next opens; `buddy result` reports each step; switching away or failing to open an app times out and records a failure.

- [ ] **Step 6: Commit**

```bash
git add crates/buddy/src/skills.rs crates/buddy/src/daemon/assist.rs crates/buddy/src/daemon.rs crates/buddy-core/src/workflow.rs
git commit -m "feat: replay workflows with focus verification"
```

### Task 4: Documentation and visible behavior

**Files:**
- Modify: `README.md` (commands, roadmap, known gaps, skills table)
- Modify: `CLAUDE.md` (commands and module map)
- Modify: `docs/development.md` (tests and manual check)
- Modify: `docs/decisions.md` or `memory/` only if the behavior changes a standing decision

- [x] **Step 1: Update the command list and skill table**

Add the `buddy workflow` commands and change “seven skills” to “eight skills” if the new skill is exposed. State clearly that this milestone records **application order only**, not keystrokes or clicks; inside-application actions remain open and require the AT-SPI/input plan.

- [x] **Step 2: Update the roadmap and known gaps**

Phase 5’s remaining line becomes: “application-level workflow recording and verified replay are done; inside-application recording is next.” Keep the existing gap about no key/click control, now scoped to inside applications.

- [x] **Step 3: Run the full suite and the manual sandbox check**

Run: `cargo test`, `sh -n scripts/install.sh`, and one sandbox record/replay. Expected: all pass; the manual replay reports a verified step for each app.

- [ ] **Step 4: Commit**

```bash
git add README.md CLAUDE.md docs/development.md
git commit -m "docs: describe workflow recording and replay"
```

## Milestone 2

Milestone 2 is specified in `docs/superpowers/plans/2026-11-23-workflow-accessibility.md`. It is not part of this plan's implementation: it adds a permissioned accessibility sense, guided capture, semantic locators, and accessible-action replay. The research above is its evidence base.

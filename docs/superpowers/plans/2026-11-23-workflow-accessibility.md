# Workflow Accessibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a permissioned accessibility adapter that lets Buddy capture a focused control as a workflow step and replay it by invoking the control’s accessible action, verifying a real semantic change.

**Architecture:** A new `accessibility` sense starts a Python AT-SPI helper only while the sense is allowed and privacy mode is off. A short-lived `a11y_act.py` locates an accessible object by a semantic locator and invokes its action; the daemon verifies the change from the event stream or a state query. Workflows gain a typed step model; guided capture records only an explicitly marked focused control, never a raw click or keystroke.

**Tech Stack:** Rust, serde, existing Unix-socket daemon and `Helper` process pattern, PyGObject `Atspi 2.0`, GNOME Shell extension for an optional capture shortcut.

**Spec:** `README.md` Roadmap Phase 5, `docs/decisions.md` additional requirements 1–3 and 5, `spikes/workflow-a11y/RESULTS.md`, and `docs/superpowers/plans/2026-11-23-workflow-recording.md` (Milestone 1).

## Global Constraints

- The `accessibility` sense is **off by default**, has its own permission, and is suspended by privacy mode.
- No raw keystrokes, no mouse coordinates, no clipboard contents and no screen frames.
- No text entry in this milestone. Password fields, terminals, shells and apps without an accessibility tree are refused.
- A workflow can only use applications the user has explicitly allowed; no implicit app allowlist.
- Replay asks first, is journaled with metadata only, stops on the first failed verification, and can be cancelled.
- The daemon remains the only decision-maker; Python helpers only observe or perform one named accessible action.
- Local only: the accessibility adapter never calls a model or a network.
- No new Rust dependency is required; PyGObject/Atspi is already installed and is invoked as a helper.
- English and Persian user-facing text follows the existing CLI conventions.

## Review Focus

- An app exposes no accessible tree or a control disappears between capture and replay: report “unsupported” or “not found” and stop; never fall back to coordinates.
- A focused control is a password field, a terminal or a shell: refuse to capture or activate it, even if the role/name looks ordinary.
- The user switches to another window while a replay step waits: the step must fail on the wrong window/app, not activate an identically named control elsewhere.
- The accessibility sense is turned off or privacy mode is enabled mid-replay: the helper must stop, the replay must fail cleanly, and no action may be sent afterwards.
- A control is activated but emits no semantic event: the step must time out and record a failure, not report success because the action call returned.
- An old `Mind` document with `Workflows` from Milestone 1 must still load after the step model changes.

---

### Task 1: Add the accessibility sense

**Files:**
- Modify: `crates/buddy-core/src/perception.rs` (`Senses.accessibility`, `SOURCES`)
- Modify: `crates/buddy/src/daemon.rs` (a third `Helper` for `a11y_watch.py`)
- Modify: `crates/buddy/src/client.rs` / `README.md` (sense command and docs)
- Test: `crates/buddy-core/src/perception.rs`

**Interfaces:**
- `Senses` gains `pub accessibility: bool`, default `false`.
- `SOURCES` becomes `["system", "desktop", "window_titles", "screen", "camera", "microphone", "accessibility"]`.
- `Helper::new("accessibility", "a11y_watch.py", Vec::new())` is polled with `open && senses.accessibility`.
- `buddy sense accessibility on|off` works through the existing command.

- [ ] **Step 1: Write the failing sense tests**

```rust
#[test]
fn accessibility_is_off_by_default_and_has_its_own_switch() {
    let mut senses = Senses::default();
    assert!(!senses.accessibility);
    assert!(senses.set("accessibility", true));
    assert!(senses.accessibility);
    assert!(SOURCES.contains(&"accessibility"));
}
```

- [ ] **Step 2: Run it and verify it fails**

Run: `cargo test -p buddy-core accessibility_is_off`
Expected: FAIL with no field/source.

- [ ] **Step 3: Implement the field, source and helper**

Add the field and match arms in `Senses::get`/`set`, update the `buddy senses` text and the daemon’s helper array. Keep privacy mode and the existing `sense` command behavior unchanged.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p buddy-core perception` and `cargo test -p buddy`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/buddy-core/src/perception.rs crates/buddy/src/daemon.rs crates/buddy/src/client.rs README.md
git commit -m "feat: add a permissioned accessibility sense"
```

### Task 2: Semantic event helper

**Files:**
- Create: `perception/a11y_watch.py`
- Test: run it against `spikes/workflow-a11y/probe_app.py`

**Interfaces:**
- One JSON line per event: `{"kind": "accessible_event", "data": {"app": "...", "frame": "...", "role": "...", "name": "...", "event": "object:property-change:accessible-name", "detail1": 0, "detail2": 0}}`.
- `{"kind": "unavailable", "data": {"why": "..."}}` when Atspi/GTK bindings or the session bus are missing.
- Only semantic events; never text contents, key codes, coordinates or clipboard.

- [ ] **Step 1: Write the helper**

Use `Atspi.EventListener.new(callback)` and register only:
`object:state-changed:focused`, `object:state-changed:checked`, `object:state-changed:selected`, `object:property-change:accessible-name`, `object:text-changed:insert`, `object:text-changed:delete`, `window:activate`.
Resolve the source’s application and frame title, drop empty/duplicate events, and print one JSON line per event. Suppress any source whose role is `password text`. Exit with `unavailable` if `gi`/`Atspi` cannot load.

- [ ] **Step 2: Run it against the probe**

Start `spikes/workflow-a11y/probe_app.py`, run `perception/a11y_watch.py`, then run `probe_action.py`.
Expected: name/text event lines arrive; no raw text or key codes; the event for the label name change is present.

- [ ] **Step 3: Commit**

```bash
git add perception/a11y_watch.py
git commit -m "feat: stream semantic accessibility events"
```

### Task 3: Guided capture of a focused control

**Files:**
- Modify: `crates/buddy-core/src/workflow.rs` (step model)
- Modify: `crates/buddy/src/daemon.rs` (capture command)
- Modify: `crates/buddy/src/main.rs` / `client.rs` (`buddy workflow capture-step`)
- Modify: `shell-extension/buddy@buddy.local/extension.js` (optional shortcut; can follow after the CLI path)
- Test: `workflow.rs`

**Interfaces:**
- `pub enum WorkflowStep { OpenApp { app: String }, Activate { app: String, frame: String, role: String, name: String }, WaitFor { app: String, frame: String, role: String, name: String, event: String } }`
- `Workflow.apps` stays for Milestone 1 compatibility; new workflows store `steps: Vec<WorkflowStep>`.
- `buddy workflow capture-step` asks the daemon for the currently focused accessible object and appends an `Activate` step to the active recording.
- The daemon asks the helper for the current focus through a short-lived `a11y_focus.py` (same semantic locator) or through the last event; no synthetic input.

- [ ] **Step 1: Write the failing step-model tests**

```rust
#[test]
fn a_step_round_trips_and_old_workflows_still_load() {
    let step = WorkflowStep::Activate {
        app: "local.buddy.A11yProbe".into(),
        frame: "Buddy A11y Probe".into(),
        role: "button".into(),
        name: "Press me".into(),
    };
    let json = serde_json::to_string(&step).unwrap();
    assert_eq!(serde_json::from_str::<WorkflowStep>(&json).unwrap(), step);
    let old: Workflow = serde_json::from_str(r#"{"id":1,"name":"x","apps":["a","b"],"created_at":0}"#).unwrap();
    assert!(old.steps.is_empty());
}
```

- [ ] **Step 2: Run it and verify it fails**

Run: `cargo test -p buddy-core workflow_step`
Expected: FAIL with no `WorkflowStep`.

- [ ] **Step 3: Implement the step model and capture command**

Add `#[serde(default)] pub steps: Vec<WorkflowStep>` to `Workflow`; keep `apps` populated from `OpenApp` steps for Milestone 1 display. `capture-step` fails if no recording is active, if the focused role is `password text`, if the app is a terminal/shell, or if the sense is off. It appends the step and journals only the semantic locator.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p buddy-core workflow` and `cargo test -p buddy`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/buddy-core/src/workflow.rs crates/buddy/src/daemon.rs crates/buddy/src/main.rs crates/buddy/src/client.rs
git commit -m "feat: capture focused accessible controls"
```

### Task 4: Accessible action helper

**Files:**
- Create: `perception/a11y_act.py`
- Test: `spikes/workflow-a11y/probe_action.py` and a shell test

**Interfaces:**
- CLI: `a11y_act.py --app APP --frame FRAME --role ROLE --name NAME --action click [--verify-event EVENT]`
- Prints one JSON line: `{"ok": bool, "verified": bool, "why": "..."}`.
- Finds the application and frame by exact name, then the control by role and name (first match; report ambiguity if more than one). Refuses `password text`, terminal/shell apps, and any control whose action name is not in the allowlist `["click", "press", "activate", "toggle", "select"]`.
- Invokes `do_action(0)` or the named action; waits up to 2 s for a semantic event if `--verify-event` is given; exits non-zero on failure.

- [ ] **Step 1: Write the helper**

Use the verified PyGObject pattern from `spikes/workflow-a11y/probe_action.py`: locate by role/name, inspect `get_n_actions()`/`get_action_name()`, call `do_action(index)`, and observe an optional event. Never call `generate_keyboard_event`, `generate_mouse_event`, or set coordinates.

- [ ] **Step 2: Verify against the probe**

Start `probe_app.py`, run:

```sh
python3 -I perception/a11y_act.py --app python3 --frame 'Buddy A11y Probe' --role button --name 'Press me' --action click --verify-event object:property-change:accessible-name
```

Expected: `{"ok": true, "verified": true, ...}` and the label changes to `pressed`.

- [ ] **Step 3: Commit**

```bash
git add perception/a11y_act.py
git commit -m "feat: invoke accessible actions"
```

### Task 5: Replay accessible steps

**Files:**
- Modify: `crates/buddy/src/daemon.rs` (step dispatch)
- Modify: `crates/buddy/src/daemon/assist.rs` (replay skill)
- Test: `crates/buddy-core/src/workflow.rs` (state machine) and the probe app

**Interfaces:**
- `ReplayState` operates on `WorkflowStep`; `OpenApp` waits for `Stimulus::Focus`, `Activate` waits for `a11y_act.py`’s verified result, `WaitFor` waits for the named event.
- Each step has a 30 s timeout; a failed step stops the replay and records a failure.
- The daemon refuses to start if the accessibility sense is off and the workflow contains an accessibility step.

- [ ] **Step 1: Write the failing replay-step tests**

```rust
#[test]
fn an_accessibility_step_cannot_run_without_the_sense() {
    let workflow = Workflow { steps: vec![WorkflowStep::Activate { app: "a".into(), frame: "f".into(), role: "button".into(), name: "b".into() }], ..Default::default() };
    assert!(workflow.requires_accessibility());
}
```

- [ ] **Step 2: Run it and verify it fails**

Run: `cargo test -p buddy-core requires_accessibility`
Expected: FAIL.

- [ ] **Step 3: Implement the dispatch and safety checks**

OpenApp uses the existing `open_app` path. Activate spawns `a11y_act.py` through the helper and parses its JSON; it only counts as success when `ok && verified`. WaitFor uses the watcher event with matching app/frame/role/name. Stop on the first failure; journal only locators and outcomes.

- [ ] **Step 4: Verify with the probe**

Capture a workflow with an `Activate` step on the probe button; replay it; confirm the label changes and `buddy result` reports the verified step. Then disable the accessibility sense mid-replay and confirm the replay stops without invoking the action.

- [ ] **Step 5: Commit**

```bash
git add crates/buddy-core/src/workflow.rs crates/buddy/src/daemon.rs crates/buddy/src/daemon/assist.rs
git commit -m "feat: replay accessible workflow steps"
```

### Task 6: Documentation and limits

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `docs/development.md`
- Modify: `spikes/workflow-a11y/README.md` if the interface changes

- [ ] **Step 1: Document the sense, guided capture, supported actions and refusals**

State clearly: accessibility is off by default; only apps with an accessible tree are supported; no text entry, password fields, terminals, coordinates or synthetic input; a failed verification stops the replay.

- [ ] **Step 2: Run the full suite and the probe workflow**

Run: `cargo test`, `python3 -I spikes/workflow-a11y/probe_action.py`, and one recorded probe workflow. Expected: all pass; the replay reports a verified step.

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md docs/development.md
git commit -m "docs: describe accessible workflow capture and replay"
```

## Out of scope

- Typing text into applications.
- Password fields, terminals, shells, banking apps and anything without an accessibility tree.
- Mouse coordinates, synthetic keystrokes, clipboard access and screen frames.
- Automatic recording of raw clicks or keystrokes; capture is guided.
- Cloud models or network calls of any kind.

# Workflow accessibility spike

This spike asks what Buddy can actually see and do inside another application on this
GNOME/Wayland desktop, without synthetic mouse or keyboard input. It is research only:
nothing here is started by Buddy and no data is installed.

## Run it

Start the probe window in the graphical session:

```sh
python3 -I spikes/workflow-a11y/probe_app.py &
```

Then, while it is open:

```sh
python3 -I spikes/workflow-a11y/probe_tree.py      # list apps and the probe's tree
python3 -I spikes/workflow-a11y/probe_action.py    # invoke the button, set text
python3 -I spikes/workflow-a11y/probe_events.py    # listen for semantic events
```

`probe_events.py` runs for four seconds; start it before `probe_action.py` to see the
events caused by the action.

## What this machine provides

- Python GObject introspection and `Atspi 2.0` are installed (`python-gobject`).
- The AT-SPI bus is present at `unix:path=/run/user/1000/at-spi/bus`.
- `at-spi2-registryd` is running.
- `toolkit-accessibility` is `false`, yet the GTK4 probe still exposed a full tree.
- `xdotool` is installed but `getactivewindow` returns nothing under Wayland; it can
  only reach X11/XWayland windows.
- `ydotool` and `wtype` are absent. `/dev/uinput` has an ACL for the user and the user
  is in the `input` group, so a synthetic-input adapter is technically possible later,
  but it is not the default and is not used here.
- Some applications expose an empty tree (for example Telegram Desktop had zero
  children), so an accessibility-based feature must report "unsupported app" rather
  than silently replaying coordinates.

## What works

- Finding an application, frame, button and text field by role and accessible name.
- Invoking the button's single accessible action (`click`): the label changed from
  `ready` to `pressed`.
- Setting text through the accessible editable-text interface; this produced
  `object:text-changed:insert` events.
- Listening for `object:property-change:accessible-name`,
  `object:text-changed:insert`, focus/state and window events.

## What does not

- A programmatic button action did **not** emit a dedicated action or `pressed` event
  in the probe; only the label's accessible-name change was seen. Recording arbitrary
  inside-app actions from raw AT-SPI events is therefore not reliable.
- PyGObject's text-reading call is awkward/broken through the convenience wrapper; use
  semantic events for verification, or a raw D-Bus/`pyatspi` adapter in a later spike.
- Some applications expose no accessibility tree at all.

## Consequence for the plan

Milestone 2 should use **guided capture**, not blind event recording:

1. The user focuses a control.
2. A Buddy command or shell shortcut captures that control as a step.
3. Replay locates it by app, frame title, role and accessible name, invokes its action,
   and verifies a semantic event or state change.

Text entry is a later, separate spike. No coordinate replay and no synthetic keystrokes
are part of this design.

## Later implementation (2026-11-23)

Phase 5 later implemented accessible text entry, mixed workflow replay and explicit
assisted manual steps for apps with no tree. This directory remains the research
record; the implementation is in `perception/a11y_text.py`, `crates/buddy-core/src/workflow.rs`
and `crates/buddy/src/daemon.rs`.

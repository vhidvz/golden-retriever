# Workflow accessibility spike results

Measured on 2026-11-23 on the GNOME/Wayland development session.

## Probe

`probe_app.py` is a GTK4 application with a frame titled "Buddy A11y Probe", a label,
a button labelled "Press me" and an empty text field. The probes use PyGObject's
`Atspi 2.0` bindings and never send synthetic input.

| Check | Result |
|---|---|
| AT-SPI bus | present (`unix:path=/run/user/1000/at-spi/bus`) |
| Python bindings | `gi` and `Atspi 2.0` available; `pyatspi` missing |
| Probe tree | `frame 'Buddy A11y Probe'` → panel → `label 'ready'`, `button 'Press me'`, `text ''` |
| Button action | `do_action(0)` invoked; label changed to `pressed` |
| Editable text | `set_text_contents()` accepted; `object:text-changed:insert` observed |
| Event listener | name, text, focus/state and window events are subscribable |
| Dedicated action event | **not emitted** for the programmatic button action |
| Other apps | 13 AT-SPI applications; some, such as Telegram Desktop, exposed 0 children |
| `toolkit-accessibility` | `false`, yet GTK4 still exposed the tree |
| `xdotool` | installed; `getactivewindow` empty under Wayland |
| `ydotool` / `wtype` | missing; `/dev/uinput` is user-accessible via ACL |
| GNOME Shell D-Bus | `org.gnome.Shell.FocusApp` is present; `Eval` exists but was not used |

## Interpretation

- Accessible **actions** are a viable replay primitive, and semantic events are a
  viable verification signal.
- Accessible **recording from raw events** is not reliable: the action itself did not
  produce a stable event. A guided capture step is required.
- Applications without an accessibility tree must be marked unsupported; falling back
  to coordinates would be a different, riskier feature.
- Typing text is possible at the interface level but reading it back through the
  PyGObject convenience API is unreliable; text entry and verification belong in a
  separate spike.

## Decision

Keep Milestone 1 (application routines) as the shipped foundation. Build Milestone 2
around AT-SPI with:

- a new permissioned `accessibility` sense, default off;
- guided capture of a focused control;
- semantic locators (app, frame title, role, accessible name);
- action invocation plus event/state verification;
- no text entry, no password fields, no terminal/shell apps, no coordinate replay;
- a clear "unsupported application" state.

This keeps the roadmap moving without turning Buddy into an unconstrained input bot.

## Post-spike update (2026-11-23)

This was the research stage. The later Phase 5 implementation added the safe
conclusions above, not a fallback to blind input:

- text entry through `Atspi.EditableText`, with text on stdin and insertion-event
  verification (`perception/a11y_text.py`);
- mixed application/inside-application sequences;
- explicit assisted manual steps for applications that expose no tree; Buddy never
  clicks coordinates or sends synthetic keys.

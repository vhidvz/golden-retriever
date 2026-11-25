# Context accessibility spike results

Measured on 2026-11-25 on the GNOME/Wayland development session, with
`gnome-text-editor` 50.1 and `at-spi2-core` 2.60.7. `probe.py` is the measurement
harness; it is not part of the runtime.

## What was measured

`probe.py` finds the largest editable text control in a running
`gnome-text-editor`, focuses it, registers `object:text-changed` and
`object:state-changed:focused`, and changes the text 40–100 times through the
accessibility `EditableText` interface. The application and AT-SPI stack emit the
events; the probe does not synthesize keystrokes or read the clipboard. The
programmatic replacement is deliberate: it makes the number of changes exact and
avoids relying on a Wayland keyboard-injection tool.

| Check | Result |
|---|---|
| Accessible text control | one focused text object, 820,080 px² at 1139×720 |
| Second text object | same frame, not focused, 67,680 px²; it emits duplicate events |
| Event names observed | `object:text-changed:insert`, `object:text-changed:delete` |
| Events per change, focused object | 1 insert + 1 delete (the clear and the insert) |
| Events per change, both objects | 2 inserts + 2 deletes before the focus filter |
| Captured changes at 80 ms apart | 40/40 inserts, no gaps |
| Captured changes at 10 ms apart | 100/100 inserts, no gaps |
| Captured changes at 250 ms apart | 60/60 inserts, no gaps |
| Listener CPU while idle 8 s | 0.0003 s user+system, 0.0–0.01 % of one core |
| Focus events during programmatic replacement | 0, so text-change is the useful signal |
| Frame name available on the event | yes: `note.txt (~/.cache/buddy-spikes/context) - Text Editor` |
| App name available on the event | yes: `gnome-text-editor` |

Raw result (40 turns, 80 ms):

```json
{"ok": true, "app": "gnome-text-editor", "turns": 40, "pause_secs": 0.08,
 "events": 160, "focused_events": 80, "inserts": 40, "deletes": 40,
 "focus_events": 0, "inserts_per_turn": 1.0, "duplicate_insert_ratio": 0.0,
 "frames": ["note.txt (~/.cache/buddy-spikes/context) - Text Editor"],
 "roles": ["text"], "idle_cpu_secs": 0.0003, "idle_secs": 8.0,
 "cpu_percent_idle": 0.0}
```

## Consequences for the implementation

- Subscribe to `object:text-changed` only. Filter the event source to the
  **FOCUSED** state; otherwise applications that expose a second text object (as
  `gnome-text-editor` does) report every change twice.
- Debounce after the last event rather than reporting each one. A burst of typing
  should become one bounded summary, not a key log.
- The event source provides the application name and the containing frame title.
  The helper must still refuse password roles, terminals, shells, login/lock and
  consent applications before reading any text.
- Reading the focused text with `Atspi.Text.get_text(source, 0, count)` works; the
  older `Accessible.get_text()` binding is deprecated and returns the object.
- The listener's idle cost is negligible compared with the local model; the
  expensive part is only the debounced summary computation, which is bounded.
- This spike did not exercise a real hardware keyboard or a password dialog.
  Those are deliberate refusals in the implementation, not captured data. The
  event contract is what the implementation relies on.

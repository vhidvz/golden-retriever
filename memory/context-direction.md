---
name: context-direction
description: Phase 10 implemented safely: semantic text context, bounded glances and warmth; not a keylogger
metadata:
  type: project
---

Recorded 2026-11-23; implemented 2026-11-25 after the owner asked for Phase 10 to be
completed. The durable record is `docs/context-direction.md`.

- Existing context: focus, fullscreen, overview, pointer position, optional window titles,
  one-shot screen/camera vision, microphone speech and accessibility.
- Implemented: pointer clicks at app/coarse-region level (headless Shell harness verifies
  privacy suppression); the off-by-default `context` sense (AT-SPI semantic text changes,
  debounced, app/kind/word/char counts only, two-minute in-memory note, clearable);
  the off-by-default `glances` sense (requires `screen`, at most one portal frame every
  20 minutes while active, five-minute local summary); quiet company while typing, one
  look-up after a burst, and at most one routine greeting per 45 minutes.
- Explicitly not proposed: raw global keycodes, clipboard capture, hidden continuous
  screenshots, password/terminal/consent/login observation, cloud processing or
  unbounded retention. A separate raw-input mode would be a different high-risk decision.
- Every layer is off by default, local-only, visible, privacy-mode suspended and
  permissioned. The spike is `spikes/context-a11y/RESULTS.md`; it measured the real
  semantic events, duplicate text objects, capture gaps and idle CPU before the
  implementation.
- The owner specifically wants more warmth and context; the point is better timing and
  companionship, not a transcript of the day.

**Why:** richer observation can improve help, but keyboard/screen data is the most
sensitive data on the machine and must not become a keylogger by accident.

**How to apply:** a new context or input sense still needs its own spike, written design
and explicit approval; do not widen `context` into raw keys or clipboard. Keep dog
behaviour, permission gates and local-first rules intact. Related:
[[phase5-completion]], [[camera-look]], [[release-direction]].

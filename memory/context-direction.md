---
name: context-direction
description: Phase 10 requirements for richer desktop context and family-dog warmth; semantic observation, not raw keylogging; not yet approved
metadata:
  type: project
---

Recorded 2026-11-23 at the owner's request: more observation should make Buddy a better
helper and feel more like a warm family dog. Phase 10 is **requirements only**, not
approved or implemented. The durable record is `docs/context-direction.md`.

- Existing context: focus, fullscreen, overview, pointer position, optional window titles,
  one-shot screen/camera vision, microphone speech and accessibility.
- Proposed safe layer: pointer clicks at app/coarse-region level; semantic text changes
  in the focused accessible control; bounded optional screen glances; routines-aware
  greetings and quiet company.
- Explicitly not proposed: raw global keycodes, clipboard capture, hidden continuous
  screenshots, password/terminal/consent/login observation, cloud processing or
  unbounded retention. A separate raw-input mode would be a different high-risk decision.
- Every layer is off by default, local-only, visible, privacy-mode suspended and
  permissioned. A spike must measure real semantic events, false positives, CPU/battery
  cost and retention before implementation.
- The owner specifically wants more warmth and context; the point is better timing and
  companionship, not a transcript of the day.

**Why:** richer observation can improve help, but keyboard/screen data is the most
sensitive data on the machine and must not become a keylogger by accident.

**How to apply:** write the spike/plan and get approval before adding a new context or
input sense. Keep dog behaviour, permission gates and local-first rules intact. Related:
[[phase5-completion]], [[camera-look]], [[release-direction]].

---
name: buddy-product-decisions
description: "Binding product decisions for Buddy: no paid assets, no human voice, English and Persian from day one, staged camera/mic, explicit approval before new phases"
metadata:
  node_type: memory
  type: project
  originSessionId: 2544f5d0-0fc6-455b-8724-10d57b4433cb
  modified: 2026-11-22T18:45:00.978Z
---

The project owner's binding decisions, given 2026-11-12 after the research stage. They
override earlier suggestions (a paid model, an optional spoken voice).

1. **No money at all.** No paid models, assets, services or commercial tools. Free and
   openly licensed material only. Build or substitute with free tools when a
   professional asset is needed.
2. **No human voice, ever.** Buddy communicates only as a dog: barks, whines, growls,
   tail, ears, posture, gaze, animation. No text-to-speech. Optional text conversation
   would live in the UI, never as a human-like character.
3. **English and Persian from day one.** Commands, training and learned words must work
   in both. Multilingual by architecture, not English-first.
4. **Camera and microphone are staged.** Absent now, added for final testing. Presence,
   wake word, speech and voice stay in scope, degrade cleanly when hardware is missing,
   and activate later without architectural change.
5. **Staging and approval.** No main implementation before the owner has seen spike
   results and a detailed plan. Persistent changes begin only after an explicit commit
   request. Phases 1-5 have passed this gate; keep the habit for new phases.
6. **Non-commercial motion data is optional and separate.** CC BY-NC material (for
   example the AI4Animation dog mocap set) is offered as a separate download; never
   bundled in the MIT repository, never a hard dependency.

Scope is a realistic, autonomous, trainable, bilingual virtual dog, not a humanoid robot
and not a voice assistant.

The full text and application notes are recorded in the repository at
`docs/decisions.md`.

**Why:** these decisions come from the owner and override recommendations made during
research; violating them changes the product.

**How to apply:** never propose paid assets or TTS as a default; evaluate every language
component for Persian; keep camera/mic providers in every plan with an "unavailable"
state; do not let scope drift towards a general assistant. Related:
[[buddy-additional-requirements]], [[user-bash-only-access]].

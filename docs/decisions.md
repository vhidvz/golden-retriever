# Product decisions and standing requirements

These are the project owner's binding decisions and standing requirements, recorded in
November 2026. They cannot be derived from the code, and they override earlier
suggestions made during research. `README.md` describes what Buddy does; this note
records what he must remain.

## Binding product decisions (given 2026-11-12)

1. **No money at all.** No paid models, assets, services or commercial tools. Only
   open-source or free assets with usable licences. If a professional asset is needed,
   build it with free tools, substitute it, or start with a simple replaceable
   placeholder. A paid model must never be a default or a core dependency.
2. **No human voice, ever.** Buddy communicates only as a dog: barks, whines, growls,
   tail, ears, posture, gaze and animation. There is no text-to-speech. If text
   conversation is ever added, it is optional and lives in the UI; it never turns the dog
   into a human-like character.
3. **English and Persian from day one.** Commands, training and learned names or words
   must work in both languages. The design is multilingual from the start, not
   English-first with Persian added later. Persian quality may be weaker initially, but
   the architecture must allow it to improve.
4. **Camera and microphone are staged, not cancelled.** They are absent now but will be
   added for final testing. Presence detection, wake word, speech and voice interaction
   stay in scope from the start, must degrade cleanly when the hardware is missing, and
   must activate later without an architectural change.
5. **Staging and approval.** No main implementation before the owner has seen spike
   results (overlay and click-through on GNOME, fur/render performance with a
   placeholder, local-model CPU benchmark, Claude Code connectivity) and a detailed
   implementation plan. The original brief also says persistent project changes begin
   only after an explicit commit request. Phases 1-5 have passed this gate; keep the same
   habit for new phases.
6. **Non-commercial motion data is optional and separate.** CC BY-NC material, such as
   the AI4Animation dog mocap set, may be offered to the user as a separate download. It
   is never bundled in the MIT repository and never a hard dependency.

**Scope.** A realistic, autonomous, trainable, bilingual virtual dog, not a humanoid
robot and not a human voice assistant.

**How to apply.** Never propose paid assets or TTS speech as a default. Evaluate every
language component for Persian support. Keep camera and microphone providers in every
plan, each with an "unavailable" state. Do not let scope drift towards a general-purpose
assistant.

## Additional requirements (given 2026-11-12, during Phase 2)

A second requirements document extends the decisions above; it does not replace them.
It covers proactive assistance, learning, local AI and natural interaction.

1. **Proactive assistance from behavioural patterns.** Notice recurring workflows (apps,
   menus, click or shortcut sequences, file operations) and offer help. The current
   implementation notices application order only; recording what happens inside an
   application is still open.
2. **Learned workflows.** Capture context, preconditions, intended outcome, action
   sequence and variations. Prefer accessibility APIs and structured tools over
   coordinate replay. Verify the outcome.
3. **Playful invitation, not a notification.** When he spots a pattern he gets excited,
   bounces, wags, looks at the user and gives short natural dog sounds. The user can
   accept, decline or ignore; refusals are learned. This must work with no cloud AI.
4. **Feedback with distinct consequences.** Correction, disabling assistance per context
   or task, approval, a persistent preference ("remember this method"), and positive
   reinforcement must be different things. Temporary versus permanent, and one-time
   versus ongoing authorisation, must be distinguished.
5. **Error recovery and control.** Confidence, confirmation before consequential actions,
   checkpoints, verification, history and rollback; an immediate pause or cancel.
   Feedback must never silently widen permissions.
6. **Positive reinforcement.** Petting, virtual treats, praise and play affect emotion,
   preferences and learned associations. They never override verification or permissions.
7. **Natural eye contact.** He looks towards the person in front of the monitor without
   needing a webcam. Occasional, varied, never constant staring. It is an independent part
   of the animation and personality system.
8. **Ollama as a first-class local provider.** Per-capability model selection. It is never
   a dependency of the autonomous engine.
9. **First model to evaluate: `gemma4:e4b`.** Measure CPU/GPU/RAM/VRAM, latency, Persian
   and English quality, tool calling, image understanding, and whether audio input
   actually works through Ollama (do not assume it does from the model family).
10. **Recommend other models for this hardware**, possibly different ones per capability.
11. **Ollama and Claude Code complement each other.** Provider choice depends on task,
    availability, quota, privacy, latency and preference.
12. **Priority rule.** Unfinished or already approved work is completed and verified
    first; these additions come after it.

**Why.** This widens Phases 3-5: perception must feed pattern discovery; skills need a
workflow recorder and verifier; a provider layer must treat Ollama and Claude Code as
peers.

**How to apply.** When planning Phase 3 onward, include pattern recognition, workflow
learning, the invitation behaviour and the feedback vocabulary. Keep "no human voice":
invitations and replies are dog behaviour and dog sounds only. Eye contact belongs in the
animation and personality layer.

## Where this stands now

- Habit detection lives in `crates/buddy-core/src/patterns.rs`; offers and their
  permission gate live in `crates/buddy/src/daemon/assist.rs`.
- Ollama is the local provider in `crates/buddy/src/ai.rs`; Claude Code is in
  `crates/buddy/src/claude.rs`.
- Still open: recording and replaying workflows inside applications with per-step
  verification, step-by-step approval for Claude Code, toys and fetch, and the remaining
  gaps listed in README "Known gaps".

The measurements behind the design, including the `gemma4:e4b` evaluation, are in
`spikes/RESULTS.md`.

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
   placeholder, local-model CPU benchmark) and a detailed
   implementation plan. The original brief also says persistent project changes begin
   only after an explicit commit request. Phases 1-5 have passed this gate; keep the same
   habit for new phases.
6. **Non-commercial motion data is optional and separate.** CC BY-NC material, such as
   the AI4Animation dog mocap set, may be offered to the user as a separate download. It
   is never bundled in the MIT repository and never a hard dependency.
   **Amended 2026-11-22:** it stays optional and separate and is never a hard dependency
   of the daemon. If a motion feature adopts it, it becomes a required data component of
   that feature, gated by licence acceptance. See `docs/self-contained-ai-decision.md`
   D8.

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
    **Superseded 2026-11-22:** Ollama is the sole AI runtime; the Claude Code
    integration is removed. See `docs/self-contained-ai-decision.md` D1.
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
- Ollama is the local provider in `crates/buddy/src/ai.rs`. Claude Code was removed in
  Phase 1 of the approved self-contained-AI plan on 2026-11-22.
- Application-level workflow recording and verified replay live in
  `crates/buddy-core/src/workflow.rs`, `crates/buddy/src/daemon.rs` and the `buddy
  workflow` commands; they were implemented in commit `2853733`.
- Inside-application actions are implemented for focused controls that have an
  accessible action: `buddy sense accessibility on`, `buddy workflow capture NAME`,
  `buddy workflow capture-step`, then `buddy workflow replay NAME|ID`. Replay locates
  the control by app, frame title, role and accessible name, invokes its accessible
  action and verifies a semantic event. It never uses mouse coordinates, synthetic keys
  or the clipboard, and refuses text entry, password fields, terminals and shells.
  Mixed application-plus-inside-application workflows are rejected for now. The
  `accessibility` sense is off by default and privacy mode suspends it. Evidence:
  `spikes/workflow-a11y/RESULTS.md`.
- Phase 5 is complete: accessible text entry, mixed application/inside-application
  sequences, and assisted manual steps for applications without an accessibility tree
  are implemented and verified. Still open: toys and fetch, and the remaining gaps
  listed in README "Known gaps".

The measurements behind the design, including the `gemma4:e4b` evaluation, are in
`spikes/RESULTS.md`.

## Amendments (approved 2026-11-22)

- **Self-contained AI runtime.** Decision record:
  `docs/self-contained-ai-decision.md`. Implementation plan:
  `docs/self-contained-ai-plan.md`. Approved by the project owner on 2026-11-22. It
  amends decision 6 (AI4Animation remains optional and separate; it becomes required
  only for a motion feature that consumes it, never for the daemon) and supersedes
  additional requirement 11 (Ollama becomes the sole AI runtime; Claude Code is
  removed). Phases 0–3 are implemented and Phase 4 is a completed research spike; no
  commit has been made. Status and deferred items are in the plan.

## Control panel (approved 2026-11-23)

- **A native control panel ships with the daemon.** A GTK4/libadwaita application,
  launched as **Buddy** from the application list or as `buddy-ui`, gives one place to
  see his state and to configure him without memorising the CLI. Its layout is
  status-first: a sidebar, a live home page with needs and waiting requests, a model
  picker with capability badges, and settings grouped by what they affect. The
  inspiration is LM Studio's clear sidebar-and-dashboard arrangement, not its code.
- **The daemon remains the single decision-maker.** The panel is a socket client like
  the CLI. It stores no simulation state, never opens the database, and sends every
  change through the same commands. Closing it does not stop Buddy; if it cannot reach
  the daemon it shows an offline state and a retry rather than inventing one.
- **It covers the whole surface, with long network operations left to the CLI.** Home,
  Talk, Train (words, memories, praise), Senses and privacy, Local AI (models, voice,
  capabilities, doctor), Skills and folders, Workflows (record, guided capture, replay,
  delete), Journal and export, and Settings are in the window. `buddy ai setup`,
  `repair` and `prune` are shown as copyable terminal commands because they are long,
  network-facing operations; `doctor` and model choices are interactive.
- **It is optional and free.** The panel uses the system Python, PyGObject, GTK 4 and
  libadwaita; no new paid service, web runtime or bundled asset is added. The daemon,
  CLI and renderer keep working when the panel's stack is absent. The installer warns
  when the panel's dependencies are missing.
- **Desktop identity is part of the deliverable.** `ui/buddy.svg` is the application
  icon (a golden retriever face in the project's forest-and-amber palette); the
  installer renders the hicolor sizes, installs `buddy.desktop`, and refreshes the
  icon cache. The window class is `buddy-ui` so the launcher groups correctly.

## Automation direction (researched 2026-11-23; not decided)

The owner asked for the research on virtual input versus shell/script automation to be
kept as part of the completion roadmap, for later implementation. The evidence, the
three options and the recommendation are in `docs/automation-direction.md`.

- The recommendation is named helper scripts first (deterministic, parameterized,
  verified, sandboxed), with `gemma4:e4b` only choosing the helper and its arguments.
- Virtual mouse and keyboard would come later, only as replay of user-recorded
  sequences through the RemoteDesktop/libei portal, with preconditions, per-step
  verification, an immediate stop and audit. Never model-generated clicks or keys.
- No option is approved. The existing preference for accessibility APIs and structured
  tools over coordinate replay still stands (additional requirement 2), and any
  implementation still needs a spike, a written plan and explicit approval.

## Body realism (researched 2026-11-23; not decided)

The owner asked for the research on making Buddy pass for a real Golden Retriever, in
image and in motion, to be kept as part of the completion roadmap for later
implementation. The evidence, options and recommendation are in
`docs/realism-direction.md`.

- Keep the offline sprite bake as the runtime architecture: the expensive work happens
  while the user is away, and showing the pictures costs almost nothing. The open work
  is the body, the coat, the light and the motion, not the renderer design.
- 3D Gaussian Splatting and neural animal avatars are research-grade: they need
  24-48 GB of VRAM to train, their template dependencies (BITE, D-SMAL) are
  non-commercial, and the Godot splat viewers need Forward+/compute rather than the
  `gl_compatibility` renderer. They are not a Buddy path on 2-4 GB of VRAM.
- The only real dog motion found is the AI4Animation/MANN set, CC BY-NC. It stays
  optional and separate under decision 6 and is never a hard dependency of the daemon.
- No open, photoreal, rigged Golden Retriever asset was found. The closest is a CC BY
  model that is rigged but low-detail and unanimated.
- No option is approved. Any implementation needs the usual spike, written plan and
  explicit approval. The target on-screen size for "indistinguishable" is still open.

## Phase 5 completion and names (approved 2026-11-23)

- **Phase 5 is complete when the remaining workflow gaps are safe.** Accessible text
  entry uses the AT-SPI editable-text interface, locates one uniquely identified
  field by app/frame/role/name, clears then replaces its contents, and verifies an
  insertion event. Text reaches the helper on stdin, not in the process list. Password
  fields, terminals, shells and ambiguous fields are refused. The text is stored as
  part of the workflow (and in exports); the CLI, GUI and docs warn that it must not
  contain passwords or secrets.
- **Mixed sequences are first-class.** A replay walks application, accessible-action,
  text and assisted steps in order. It opens each expected application, waits for
  focus, runs the helper, verifies the semantic event, and only then advances. A
  captured sequence inserts an explicit application step before a new app. The old
  app-only workflows and action-only workflows remain loadable and replayable.
- **Apps without a tree are supported honestly, not blindly.** They can be included as
  explicit assisted manual steps: Buddy brings the app forward, tells the user what to
  do, and waits for `buddy workflow continue`. It never falls back to coordinates,
  synthetic keys or the clipboard. The user's confirmation is the verification for
  that step; a timeout and `buddy workflow abort` release a stuck replay.
- **Names are user data, not vocabulary.** `buddy name NAME` (or Rename in Train and
  Settings) sets the name he answers to. `language::addresses` and `Heard::Called`
  recognize it in typed and spoken speech in English and Persian; the English/Persian
  forms of "Buddy" remain aliases only while that is the configured name. The first
  call gets a bright greeting, later calls are a recall to the middle of the screen;
  `Life.name_calls` preserves familiarity across restarts and is shown in `status` and
  the panel. No user name is hard-coded into `lexicon.json`.
- **Verification.** The suite is 105 Rust tests; the GUI smoke path and an isolated
  daemon test exercised text replay, mixed text+manual replay, two consecutive manual
  steps, and the name/call commands.

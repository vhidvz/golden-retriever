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
- **Verification.** The suite is 112 Rust tests; the GUI smoke path and an isolated
  daemon test exercised text replay, mixed text+manual replay, two consecutive manual
  steps, and the name/call commands.

## Camera one-shot vision (2026-11-23)

- **The camera sense stays presence-only.** `camera_watch.py` continues to read a
  64×48 greyscale motion signal and never sends frames to a model. This keeps continuous
  observation cheap and private.
- **A one-shot camera look is explicit.** With the camera sense on and privacy mode off,
  `buddy look --camera` (or **Look through the camera** in Senses) pauses the presence
  helper, runs `perception/camera_still.py` for one frame, sends that frame to the
  selected local vision model, and deletes it immediately after reading. The result is
  a short summary in memory; no image is kept, uploaded or put in the journal.
- **The model is local and capability-checked.** The path uses the same
  `ask_about_image`/`gemma4:e4b` vision route as `buddy look --file`; cloud models are
  refused. The camera prompt asks only whether a person is visible and what is happening,
  and explicitly avoids identifying people or inferring sensitive traits.
- **No continuous camera vision is promised.** If the camera sense is off, the command
  refuses rather than bypassing the permission. The presence helper is restarted after
  the look; it is never left competing for the device.
- **Test seam:** `BUDDY_CAMERA_PIPELINE` can replace the GStreamer capture pipeline with
  a synthetic source for tests. The real hardware path uses `v4l2src`; the synthetic path
  is not used for a user's camera.


## Release engineering and updates (Phase 9 Linux implementation, 2026-11-24)

- **The Linux implementation is complete and CI-verified.** Manual GitHub Actions run
  45322812499 passed tests, tar, DEB, RPM, Arch, AppImage and the merged metadata job;
  the tag-only publish job was skipped because no tag was created. Packages find their
  assets under `/usr/share/buddy`,
  the UI launcher is relocatable, `scripts/verify-package.sh` checks the complete
  payload, `scripts/checksums.sh` writes SHA-256 manifests, `scripts/sbom.py` writes a
  CycloneDX 1.5 SBOM, and every GitHub Action is pinned by commit SHA with a read-only
  default token. All five Linux formats were built and payload-verified locally.
- **Windows is explicitly unsupported for this release line.** Unix sockets, systemd,
  the GNOME/KDE bridges and the managed Ollama service are Linux-specific. An MSI/EXE
  cannot make the current daemon work; a native port is a separate project.
- **AppImage does not bundle managed runtimes.** It carries the daemon, UI, renderer
  project, helpers and tools, but not Godot or Ollama. The existing installer or
  `BUDDY_GODOT` supplies those on first run; checksums and the unavailable state stay
  authoritative.
- **Signing is optional and starts unsigned.** `SHA256SUMS` is generated and attached; no
  paid certificate or signing service is assumed. `scripts/sign-release.sh` adds a
  detached GPG signature when a key is configured; otherwise the release is
  checksum-only. CI uses `BUDDY_SIGNING_PRIVATE_KEY`, `BUDDY_SIGNING_KEY` and optional
  `BUDDY_SIGNING_PASSPHRASE` secrets; missing secrets keep the release unsigned.
- **Arch is a project-generated PKGBUILD with a real source checksum**, not an AUR or
  official-repository package yet.
- **Stable channel only.** No telemetry, background polling, silent self-update or model
  auto-update. `buddy update check`/`update tools` report only. `buddy upgrade`
  downloads a user-local artifact, verifies it against the release's `SHA256SUMS`, and
  prints manual replacement commands; it refuses system packages and checkouts and never
  installs or overwrites anything itself. The download path verifies SHA-256; detached
  signatures are optional and not yet checked by `buddy upgrade`.
- **Package ownership is split by design.** System packages own `/usr`; user data stays
  in `~/.local/share/buddy`. Package-managed and user-local installs must not overwrite
  each other, and no automatic data migration is attempted.
- **Build identity is embedded.** `crates/buddy/build.rs` records the git
  tag/commit/dirty state (`unknown` outside a checkout); `buddy version` prints it.
- **Still open:** the tag-only publish/upload path has not been exercised; the manual
  workflow skips it by design. The AppImage first-run UX beyond the env/status path
  remains. Windows is not an open Phase 9 task because this release line explicitly does
  not target it. Guided upgrade exists as download/verify only; detached signatures are
  optional and not yet checked by it.


## Context and companionship (pointer/click foundation, 2026-11-23)

- **Pointer/click foundation implemented.** The GNOME Shell desktop sense reports a click
  by button, current application and coarse region only. It stores no click history and
  sends nothing in privacy mode; the headless Shell harness covers it. Semantic text,
  bounded screen glances and routines-aware warmth remain unimplemented.
- **Phase 10 is recorded, not approved.** The owner wants Buddy to observe more of the
  environment so he can be a better helper and feel more like a family dog. The safe
  requirements and limits are in `docs/context-direction.md`.
- **No raw global keylogger.** The proposed keyboard layer is opt-in semantic text-change
  observation through AT-SPI in the focused accessible control, with password fields,
  terminals, shells, login/lock/consent surfaces and applications without a usable tree
  refused. Raw keycodes, clipboard capture and hidden continuous screenshots are not
  proposed. More context must remain local, privacy-gated and bounded.
- **Warmth is a behavior goal, not surveillance.** Routines-aware greetings, quiet
  company, looking up and well-timed offers matter more than logging. The existing dog
  behaviour remains the interface. A spike, written plan and explicit approval are still
  required before any implementation.


## Tray icon and custom notifications (2026-11-24)

- **OS tray first, custom UI second.** The icon is a standard StatusNotifierItem hosted
  by the desktop (GNOME AppIndicator extension / KDE), so the OS decides where it lives.
  The panel opened from it is Buddy's own GTK4 UI; no `org.freedesktop.Notifications`
  daemon or native toast is used.
- **Messages come from his state, not a second brain.** His existing `waiting` offers
  (including habit/pattern offers) are shown as interactive cards; hunger, thirst,
  boredom, company and tiredness cross thresholds with per-need cooldowns. A returning
  user gets an information-only "You're back" card that closes itself; quiet mode keeps
  cards but does not auto-open the panel.
- **Every action is a daemon command.** Feed, water, pet, sleep, approve/deny and the new
  `play` command go through the same socket and permission rules as the CLI. `play` is a
  short bow that settles boredom and lifts mood.
- **Presentation stays non-intrusive.** Interactive cards stay until answered; a small
  badge/attention state marks unread messages; the panel has a close button and Escape,
  and a notice-only panel closes itself after a short time. The tray also exports the
  standard DBusMenu expected by GNOME's AppIndicator extension, so a single click has a
  useful result even though GNOME reserves double-click for activation. The tray is a
  normal client and can die without affecting his life.
- **Host dependency is explicit.** GNOME requires the AppIndicator extension; KDE has
  the host natively. The tray service retries registration if the watcher starts later.
  `scripts/install.sh` installs/enables it by default; `--no-tray` opts out.
- **No global input or screen capture was added.** The tray reads `status`/`waiting`
  over the same socket and never touches raw keyboard or pointer content.

## Named helpers (Phase 7 Stage 1, 2026-11-23)

- **Owner-registered, argv-only helpers are implemented.** Manifests live in the Buddy
  data directory; commands are arrays, arguments are separate, placeholders are explicit,
  cwd is confined, timeouts are enforced, and dry-run executes nothing. Every run asks
  through the sensitive `run_helper` skill; no shell or model-generated command runs.
- **Model selection implemented 2026-11-24.** The local model receives one tool per
  helper with its declared arguments; a selected helper becomes the always-ask
  `run_helper` skill. Optional `bwrap` sandboxing and a post-run `verify` argv are
  implemented.
- **Phase 7 is complete on option B.** Named helpers are the adopted automation path.
  Synthetic mouse/keyboard input (options A/C) is not adopted: it needs a heavy portal
  consent, can reach password dialogs and terminals, and GNOME 50 has input-grab
  lifecycle risks. This is a deliberate decision, not deferred work. See `docs/helpers.md`
  and `docs/automation-direction.md`.


## Phase 6 body refinement (started 2026-11-24)

- **The measured skeleton is not being re-guessed.** The first visual pass only changes
  the cheek/upper-lip volume, brow softness, ear width and tail fullness in
  `tools/build_dog.py`. `tools/measure_dog.py` still passes every adult and puppy
  reference check; head and limb landmarks did not move.
- **The body change invalidates sprite sets by design.** The baker will rebuild the
  pictures while the user is away; the live 3D body and tests keep working meanwhile.
- **KDE Plasma sense added 2026-11-24.** KWin 6 has no readable active-window D-Bus
  method, so a KWin script reports focused application, pointer position and overview
  state to a session D-Bus bridge in the daemon. The bridge was tested with real `gdbus`
  Focus/Pointer/Overview calls. Overview has no signal, so the script polls
  `workspace.isEffectActive("overview")` with a `QTimer`.
- **KDE native click effect source added 2026-11-24.** `kde/kwin-effect` is a C++ KWin
  effect using `KWIN_EFFECT_FACTORY`, `startMousePolling` and
  `EffectsHandler::mouseChanged`; it calls the existing `Click` bridge. The installer
  builds/installs it only when KWin headers, CMake and KDE tools are present. This host
  has no Plasma/KWin development stack and Docker Hub is blocked, so the effect was not
  compiled or runtime-tested here; it is not claimed as verified.
- **KDE window titles implemented 2026-11-24.** The KWin script sends
  `Focus(app, caption)` through the same bridge. The daemon keeps the title only when
  `window_titles` is allowed, drops it when the sense is off or privacy is on, and never
  journals it or sends it to a model. `buddy senses` and the panel show the last title
  while the sense is on. The bridge was tested with `gdbus` Focus calls both off and on.
- **Live body refinement (2026-11-24).** The head was broadened at the skull and
  muzzle, the ears/eyes/nose were enlarged and the coat gained chest, leg and tail
  feathering. The live renderer now uses a procedural iris/pupil/limbal shader instead
  of a black pupil ball, and the coat gets high-frequency lighting variation. All adult
  and puppy proportion checks still pass. This is an artistic improvement, not a
  photoreal claim; the measured no-go in Phase 8 stands.
- **Motion smoothing (2026-11-24).** The renderer caps how far the window and leg phase
  may travel in one frame, so a new activity or a large `x_frac` change cannot advance
  the stride by a whole cycle. Sprite pace selection has hysteresis at the walk/trot and
  trot/gallop boundaries, and the 3D body is hidden once the sprite fade has enough
  weight, avoiding a ghost/doll overlap.
- **Still open for Phase 6:** KDE native click runtime verification and the remaining
  fur/model craft (surface detail, coat response, puppy-specific polish).


## Phase 8 realism baseline (started 2026-11-24)

- **The first Phase 8 step is a measured spike, not a photoreal rewrite.** The current
  offline bake costs about 72 s per stand frame at 150k strands/24 samples on CPU
  (28–33 s at 60k/12). A real Cycles CUDA run of the same frame took 25.4 s and peaked
  at 1027 MiB. The committed stills in `spikes/realism/` show the quality ceiling:
  better than the live body, still visibly CG at the edges/head/paws.
- **No photoreal claim.** The 800 px close-up still reads as CG at 1M strands/64 samples,
  so the acceptance bar is not met. Denser groom and more samples did not fix the eye
  shape, muzzle or lip line; 500k/32 peaked at 3725 MiB of 4 GB. A blind A/B and the
  MANN walk/foot-slide test remain unrun, but they cannot rescue a body that fails the
  close-up test.

## Phase 8 photoreal decision (2026-11-24)

- **No-go for photoreal on this hardware.** The measured production bake is 72.2 s/frame
  on CPU and 25.4 s on the GTX 1650 CUDA path; the 1M/64 800 px close-up still reads as
  CG. Neural/splat avatars need 24–48 GB VRAM and non-commercial dependencies; the photo
  puppet is limited to its source activities and share-alike terms. The current sprite
  bake remains the product path.
- **Phase 8 is closed as a decision, not as a photoreal success.** Optional future
  experiments (photo puppet, diffusion refinement, a larger GPU) require a new owner
  request; no runtime or data dependency was added.

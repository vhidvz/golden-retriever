# Development notes

This note records the machine, tools and working agreements that are not obvious from
the repository. It complements `README.md` (product behaviour) and `docs/decisions.md`
(binding product decisions).

## Primary development machine

The measurements in `spikes/RESULTS.md` were taken on 2026-11-12 on this baseline:

- Manjaro Linux, GNOME Shell 50.5 on Wayland, XWayland 24.1.
- GeForce GT 1030 with 2 GB VRAM (driver 580.178.04), Intel i5-11400F, 32 GB RAM.
- One 3440x1440 monitor at 100 Hz, scale 1.0.
- VRAM is the scarce resource: the desktop alone used 888-1506 MB of the 2048 MB during
  testing, which is why the daemon eases off under load and the renderer is deliberately
  cheap.

On 2026-11-22 the machine used for the self-contained-AI work reported a GeForce GTX
1650 Mobile (4 GB), an i5-10300H and 31.2 GB RAM. The measurements above were taken on
the older GT 1030 baseline; re-run a speed comparison before treating a model result as
portable between the two.

Toolchain checked on 2026-11-22:

- rustc and cargo 1.97.1, Python 3.14.7.
- Blender 5.2.2 LTS at `/usr/bin/blender` (installed with pacman).
- GNOME Shell 50.5.
- Godot is managed by the installer: a pinned 4.7.2 build lives under
  `$BUDDY_DATA_DIR/godot/4.7.2`, and `BUDDY_GODOT` is written to `~/.config/buddy/env`.
  A system `godot`/`godot4` or an explicit `BUDDY_GODOT` takes precedence. The daemon is
  designed to live without a renderer, so a missing Godot is not a reason to fake one.

Before writing a substitute for an asset or a piece of tooling, check what is installed
with `command -v` and `pacman -Q`. This project already made that mistake once: a
hand-written Python mesh was produced while Blender was installed, and the result did not
look like a Golden Retriever. The body is built in Blender for that reason.

## Scratch space

`/tmp` is tmpfs on this machine and is wiped between sessions. Work that must survive a
restart belongs under `~/.cache/buddy-spikes/`:

- `work/` — bake outputs, test renders and helper scripts. `shot.sh` takes a self-test
  picture with retries; `sheet.py` tiles pictures on a light background.
- `realism/` — side-by-side realism comparisons and the first hair proof of concept.
- `reference/` — CC-licensed Golden Retriever reference photographs (for example a clean
  side profile from Wikimedia Commons).

Create the directory when needed; it is not part of the repository.

## Working agreements

- **The owner acts through bash only.** Give every step they must take as a
  copy-pasteable bash command. If something genuinely requires a click in the graphical
  session (a portal consent dialog, a graphical login, a GNOME extension approval), say
  so explicitly and offer a terminal-only alternative where one exists. Do not design a
  verification step that waits on GUI interaction. They can see the screen at least some
  of the time, so a visual check is acceptable when there is no terminal equivalent.
- **Use installed tools.** Blender, Python, GNOME Shell and Ollama are present on the
  development machine. Prefer them over new dependencies, especially for anything the
  project must keep free and redistributable.
- **Commit finished tasks and merge them; never push unasked.** Work on a branch. When
  a task the owner asked for is finished and verified, commit it and merge the branch
  into `main` (fast-forward where possible); push only when explicitly asked. Do not
  leave finished work on a side branch. New phases are still staged behind spike
  results, a written plan and explicit approval. This repository uses the owner's own
  commit messages and history; do not rewrite them.
- **Local first.** Never make Buddy's life depend on Ollama or the network. The model
  provider must fail on its own and be reported in plain words.
- **He is a dog.** No text-to-speech, no human voice, no chatbot persona. Invitations,
  replies and feedback are dog behaviour and dog sounds.
- **English and Persian by architecture.** Vocabulary is data, not code. Add new phrases
  to `crates/buddy-core/data/lexicon.json` and check both languages.
- **Senses are permissioned.** Every source has an "unavailable" state. Privacy mode
  suspends all of them. Do not add a perception path that bypasses the daemon's policy.
- **Durable memory lives in the repository.** `memory/MEMORY.md` indexes the standing
  decisions and working agreements for Claude Code sessions; it is tracked in git. The
  per-project memory directory under `~/.claude/projects/` only points there, so the
  repository copy is authoritative.
- **No paid assets or services.** CC BY-NC material is offered as a separate optional
  download, never bundled. See `docs/decisions.md`.
- **Tracked documents never cite untracked scratch.** Do not point a git-tracked file at
  `.superpowers/`, `~/.cache/...`, a session workspace, an absolute home path, or another
  temporary artifact as a source or as the only home of a fact. Copy the durable summary
  into a tracked document and cite that instead. Runtime/product paths and the designated
  scratch directory as an instruction are allowed, but never as evidence.
- **The control panel is a client, never a second brain.** `ui/buddy_ui.py` speaks the
  same newline-delimited JSON socket as `crates/buddy/src/client.rs`; it stores no state
  and never opens the database. Any behaviour it exposes must exist in the daemon first.
- **A plan is process, not a project artifact.** Do not copy a task-by-task plan or
  session ledger into the repository. Extract its durable decisions, interfaces,
  constraints and evidence into the existing docs, spikes or memory, then delete the
  plan copy. A future session should not need a separate plans tree.

## Testing and runtime traps

- `cargo test` currently runs 129 tests in under 30 seconds. Unit tests sit beside their
  modules; the daemon lifecycle test is `crates/buddy/tests/lifecycle.rs`.
- **Wall-clock time:** tests must not depend on the hour. The lifecycle sandbox sets
  `BUDDY_HOUR=12` because a sleeping dog answers differently; a test that passes at noon
  and fails at night is this bug.
- **Isolation:** `BUDDY_DATA_DIR` and `BUDDY_SOCKET` give a test its own database and
  socket. A second daemon on the same socket refuses to start; the lifecycle test relies
  on that. `BUDDY_UPDATE_REPO`/`BUDDY_UPDATE_API` point update and upgrade tests at a
  local HTTP server instead of GitHub.
- **Renderer is disposable:** it can crash, be hidden or be missing. The daemon restarts
  it with backoff and keeps simulating. Do not put life-critical logic in Godot.
- **Sprite bakes are expensive:** measured 72.2 s/frame on CPU and 25.4 s on the GTX
  1650 CUDA path at 150k strands/24 samples, with about 1,700 frames per growth stage.
  The daemon runs them only while the user is away, pauses them when the user returns,
  and resumes rather than restarting. Use `--only` for a targeted test.
- **Cycles GPU needs an explicit device, not just `scene.cycles.device = "GPU"`.** Set
  `compute_device_type` (`CUDA` or `OPTIX`) and enable that device, or Blender silently
  renders on CPU; `nvidia-smi` peaking at ~126 MiB is the tell. `spikes/realism/run.sh
  --device GPU` does this. Production 150k peaks at 1027 MiB; a 500k-strand close-up
  peaks at 3725 MiB of the 4 GB card, so dense full-body bakes have no headroom.
- **Desktop sense test:** `tests/shell-sense/run.sh` starts an isolated headless GNOME
  Shell in its own D-Bus session. It needs `gnome-shell`, `dbus-run-session` and a
  writable runtime directory. `BUDDY_TRACE_SENSES=1` makes the daemon log what it hears.
- **Workflows:** `buddy workflow record NAME`, `stop`, `replay`, `workflows` for
  application-level routines. Inside applications, turn on the `accessibility` sense,
  run `buddy workflow capture NAME`, focus a control, press `Super+Shift+B` (or run
  `buddy workflow capture-step`) for each action. Text uses
  `buddy workflow type-step "TEXT" --delay 5`; the helper receives text on stdin, and
  the text is stored in the workflow (warn the user: not for secrets). Apps without a
  tree use `buddy workflow manual-step "NOTE" --app APP`; replay brings the app forward,
  waits for `buddy workflow continue`, and never blind-clicks or sends keys. Stop with
  `buddy workflow stop`; replay with `buddy workflow replay NAME`. Replay walks mixed
  application/accessible/text/manual sequences, opening each app and verifying each
  accessible step. The shortcut is registered by the GNOME extension from its compiled
  schema; `scripts/install.sh` compiles it. Password fields, terminals, shells and
  coordinate/key replay remain refused. Text is stored in the workflow and export; do
  not use it for secrets. `buddy workflow abort` releases a waiting replay. Evidence:
  `spikes/workflow-a11y/RESULTS.md`; durable decisions are in `docs/decisions.md`. No
  separate plan file is kept as a project artifact.
- **Name recognition:** `Identity.name` is the configured name; `daemon`/CLI use
  `language::addresses` and `Heard::Called`. In English and Persian, the name is
  stripped from a sentence before cue matching. The English/Persian forms of "Buddy"
  are aliases only while that is the configured name. `Life.name_calls` records calls;
  the first is a greeting, later ones are a recall. The GUI exposes Rename and Call on
  Train and Settings. Do not hard-code a user's name in vocabulary data.
- **Ollama:** `scripts/install.sh` installs a pinned runtime under
  `$BUDDY_DATA_DIR/ollama/<version>` and starts `buddy-ollama.service` on
  `127.0.0.1:11435` with its own model store. A system Ollama on 11434 is left untouched.
  `BUDDY_OLLAMA_URL` overrides the endpoint for tests. `buddy ai status` and
  `buddy ai doctor` report the state; `buddy ai setup` recommends, pulls and verifies a
  model. Every caller copes with the runtime being absent.
  `spikes/local-llm/ollama_eval.py` reproduces the measurements except the audio test.
- **Model catalogue:** `crates/buddy/data/ai_models.json` is data, not code. Add a model
  only after evaluating it with `spikes/local-llm/ollama_eval.py`; mark `verified` only
  when the project has measurements.
- **Claude Code:** removed on 2026-11-22. The historical spike remains under
  `spikes/claude-code/` for the record; it is not part of Buddy's runtime.
- **Tray client:** `ui/buddy_tray.py` registers a StatusNotifierItem on the session bus
  and opens its own GTK panel from `Activate`/the native `com.canonical.dbusmenu` item.
  GNOME uses the project's Shell extension top-bar button (direct single click);
  KDE/other hosts use SNI. It polls `status` and
  `waiting` every 1.25 s, never uses `org.freedesktop.Notifications`, and sends every
  action through the daemon socket. `scripts/install.sh` installs
  `dist/buddy-tray.service`; use `--no-tray` to skip it. Test the registration with
  `gdbus call --session --dest org.kde.StatusNotifierWatcher --object-path
  /StatusNotifierWatcher --method org.freedesktop.DBus.Properties.Get
  org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems` and then call
  `Activate` on the `org.kde.StatusNotifierItem-<pid>-1` name.
- **Control panel:** `ui/buddy_ui.py` is a GTK4/libadwaita client installed as
  `buddy-ui`, with `dist/buddy-ui.desktop` and `ui/buddy.svg` for the application list.
  It needs Python 3, PyGObject, GTK 4 and libadwaita; the daemon and CLI do not. Check it
  with `python3 -m py_compile ui/buddy_ui.py`, `desktop-file-validate
  dist/buddy-ui.desktop`, and a launch under X11 (`GDK_BACKEND=x11 BUDDY_UI_PAGE=home
  python3 ui/buddy_ui.py`). `BUDDY_UI_PAGE` selects a page for support checks; `BUDDY_SOCKET`
  pointing at a missing socket exercises the offline state. The panel is one client of
  the same socket as the CLI; it cannot change state except by asking the daemon. The
  installer copies it to `$BUDDY_DATA_DIR/ui`, installs the launcher in `~/.local/bin`,
  the desktop file in the user applications directory, and SVG/PNG icons in hicolor.
  `buddy snapshot` still saves only Buddy's window, not the control panel or the desktop.
- **Screenshots:** `buddy snapshot` saves only Buddy's window, not the desktop. Use it for
  drawing bugs; do not capture the user's screen for diagnosis.

## Evidence and limits

- `spikes/RESULTS.md` — overlay, fur performance, local-model benchmarks and long-run
  simulation results. The Claude Code section is historical. The design rests on these
  measurements.
- `spikes/ai4animation/` — the MANN dog BVH research spike. It is licence-gated and is
  not part of the runtime or the installer.
- Verified on 2026-11-23: the real Ollama 0.40.17 install on 11435 and `gemma4:e4b`
  (6.6 GB) are active; the system Ollama on 11434 was left untouched.
- `spikes/workflow-a11y/` — the AT-SPI accessibility spike for inside-application
  workflows. The GTK4 probe and event/action probes are run manually; they are not part
  of the installer or the test suite.
- `spikes/context-a11y/` — the Phase 10 semantic text-change probe for
  `gnome-text-editor`; it measures event names, duplicate text objects, capture gaps and
  listener CPU. It is run manually against a real app and is not part of the installer
  or the test suite.
- `docs/proportions.md` — how Buddy's body was measured against breed standards,
  photographs and skeletal studies, and what changed as a result.
- `README.md` "Known gaps" — the current, maintained list of what is not done. Prefer it
  over older notes when they disagree.

- **Context sense:** `buddy sense context on` starts `perception/a11y_context.py` only
  while privacy is off. It subscribes to AT-SPI `object:text-changed`, filters the
  FOCUSED editable text object, debounces 1.2 s, and prints one summary line; it never
  prints text. Check with `buddy context` (the one in-memory note, two-minute retention)
  and `buddy context clear`. Password fields, terminals, shells, password managers,
  login/lock and consent/portal surfaces are refused by the helper and again by the
  daemon; `buddy senses` shows whether it is reporting. The probe in
  `spikes/context-a11y/` measures the event path with `gnome-text-editor`; it is not
  part of the test suite and does not use a hardware keyboard.
- **Screen glances:** `buddy sense screen on` then `buddy sense glances on`. The first
  automatic look waits five minutes, then runs at most every twenty minutes while the
  user is present and not locked/fullscreen/overview. It uses the same portal path as
  `buddy look`; a first-run consent prompt is expected. Only a local one-line summary is
  kept, for five minutes, and `buddy context clear` erases it. The portal's sharing
  indicator is the visible sign of the moment of capture.
- **Camera one-shot:** `perception/camera_still.py` captures one frame for
  `buddy look --camera`. It requires the camera sense and privacy off; the daemon pauses
  the continuous presence helper while it runs, reads and deletes the JPEG, and sends it
  only to the local vision model. `BUDDY_CAMERA_PIPELINE` substitutes a GStreamer test
  pipeline (`{out}` is the output path) for hardware-free tests. The continuous camera
  helper remains `camera_watch.py` and presence-only.

- **Release artifacts:** `scripts/package.sh FORMAT` builds tar/deb/rpm/arch/AppImage
  from the release binary, the renderer project, perception helpers, UI, systemd user
  unit and desktop/icon files. It runs `scripts/verify-package.sh` and writes a
  `SHA256SUMS` before returning. `scripts/checksums.sh DIR` regenerates the manifest,
  `scripts/sbom.py` writes a CycloneDX SBOM from `Cargo.lock`, and
  `scripts/sign-release.sh DIR` optionally adds a detached GPG signature when
  `BUDDY_SIGNING_KEY` is set. `scripts/release-rehearsal.sh` builds every locally
  available format and generates the same metadata; use it before a tag. The release workflow pins
  every action by commit SHA, gives only the publish job `contents: write`, verifies
  each package, and attaches one checksum manifest plus the SBOM. A manual run on
  2026-11-24 passed tests, all five package jobs and metadata; the tag-only publish path
  is still unexercised. Windows is not built
  because the daemon is Unix-specific; this release line explicitly does not support it.
  `buddy version` includes the git describe string from `crates/buddy/build.rs`;
  `buddy update check` and `buddy update tools` are network-only-on-request checks; no
  updater or installer runs automatically. `buddy upgrade` downloads a user-local
  artifact and verifies it against `SHA256SUMS`, then prints manual replacement commands;
  it refuses system packages and checkouts and never installs.

- **Helpers:** manifests live in `$BUDDY_DATA_DIR/helpers/*.json`; `crates/buddy/src/helpers.rs`
  resolves argv arrays, placeholders, approved cwd and timeouts. `run_helper` is a
  sensitive skill and always asks. `buddy helper dry-run` executes nothing. Model-driven
  helper selection is implemented by exposing each helper as a model tool; optional
  `bwrap` sandboxing and verification are implemented. Synthetic input is deliberately
  not adopted. See `docs/helpers.md`.

- **KDE sense:** KWin 6 exposes active window and cursor position to scripts but no
  readable active-window D-Bus method. `crates/buddy/src/kde.rs` serves
  `local.buddy.Sense` over the session bus; `kde/kwin/buddy` is installed into
  `~/.local/share/kwin/scripts/buddy-sense` and enabled in `kwinrc` when KDE tools exist.
  Test the bridge with `gdbus call ... local.buddy.Sense.Focus APP`, `.Pointer X Y` and
  `.Overview true|false`. Overview is polled by the KWin script with `QTimer` because
  KWin has no overview signal. Normal KWin scripts cannot report clicks; the native C++
  effect source in `kde/kwin-effect` uses `startMousePolling` and
  `EffectsHandler::mouseChanged`. Build it only where KWin headers/CMake exist with
  `scripts/build-kwin-effect.sh`; it was not compiled or runtime-tested on this GNOME
  machine. The KWin script also sends `window.caption` with focus; the daemon drops it
  unless the `window_titles` sense is on, and never journals it. Test both states with
  `gdbus ... Focus APP TITLE` and `buddy senses`.

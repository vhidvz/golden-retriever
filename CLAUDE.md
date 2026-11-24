# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

Buddy is a Golden Retriever who lives on a Linux desktop: a long-running Rust daemon
simulates his life, a Godot overlay draws him, and an optional local model helps him
understand language and act on the computer. **Phase 5 (skills and helpers) is
complete**; Phase 6 (refinement) continues with his body. `README.md` is the user-facing
specification and describes every command and behaviour. `docs/decisions.md` records the
project owner's binding decisions; `docs/development.md` records the machine, tools and
working agreements. `memory/MEMORY.md` is the durable, git-tracked project memory that
indexes what future Claude Code sessions should recall.

The invariants that matter most:

- **Local first.** His life never depends on a language model or a network. The local
  Ollama runtime is an optional helper that fails independently; Claude Code was removed
  on 2026-11-22.
- **He is a dog.** Barks, whines, posture, gaze and motion; never text-to-speech or a
  human voice.
- **English and Persian from day one.** Language data lives in
  `crates/buddy-core/data/lexicon.json`; never hard-code one language.
- **No paid assets or services.** Only free, openly licensed material. CC BY-NC
  material is an optional separate download, never bundled in this MIT repository.
- **Every sense is optional and permissioned.** Each source has an "unavailable" state;
  privacy mode suspends all of them at once.

## Commands

### Build, test, run

```sh
cargo build --release
cargo test                                    # 118 tests, under 30 s on the dev machine
cargo test -p buddy-core <test_name>          # one unit test
cargo test -p buddy --test lifecycle          # daemon persistence/lifecycle integration test
cargo fmt --check                             # rustfmt.toml: max_width 150 (currently reports pre-existing diffs)
cargo run -- daemon                           # run from the checkout
cargo run -- daemon --no-renderer             # headless (no Godot window)
cargo run -- status                           # CLI client against a running daemon
scripts/install.sh                            # install for this user and start with the session
scripts/install.sh --remove                   # stop and remove (keeps ~/.local/share/buddy)
buddy-ui                                      # optional GTK4/libadwaita control panel
python3 -m py_compile ui/buddy_ui.py          # check the panel
buddy version | update check | update tools   # versions + git describe; network only on request
buddy upgrade                                  # download/verify user-local artifact; no install
buddy helpers | helper dry-run|run NAME       # owner-registered argv helpers; runs ask
scripts/package.sh tar|deb|rpm|arch|appimage  # verified release artifacts + SHA256SUMS
scripts/release-rehearsal.sh [FORMAT...]          # build all local formats, SBOM, checksums, optional sign
scripts/verify-package.sh dist/release/FILE       # check the full installed payload
scripts/checksums.sh dist/release                 # regenerate SHA256SUMS
python3 scripts/sbom.py                           # CycloneDX SBOM from Cargo.lock
BUDDY_SIGNING_KEY=KEY scripts/sign-release.sh dist/release  # optional detached GPG signature
buddy name "پشمک"                              # choose the name he answers to
buddy call                                    # call him by that name
```

`cargo fmt --check` currently reports pre-existing formatting differences on this
toolchain; do not reformat unrelated files unless the task is formatting.

`buddy` is both the daemon and its CLI; the CLI talks to the daemon over one Unix
socket. `README.md` lists the full command set. The diagnostic commands worth knowing:
`buddy why` (current decision and alternatives), `buddy status`, `buddy senses`,
`buddy providers`, `buddy ai status` and `buddy ai doctor`, `buddy journal`,
`buddy snapshot` (only Buddy's window, for drawing
bugs), and `buddy simulate DAYS --user absent|attentive|office` (long-run behaviour
check).

### Renderer (Godot)

The renderer is a Godot 4.7 project using the GL Compatibility renderer. It is launched
with `--display-driver x11` on purpose: under XWayland the unfocusable window is
override-redirect, which is what makes it always on top, on every workspace, out of the
taskbar, and never focused. It needs an X11 or XWayland display.

```sh
godot --path renderer --display-driver x11 -- --selftest /tmp/buddy.png \
      [--activity sit] [--variant 1] [--growth 0.2] [--sprites DIR] [--no-sprites]
godot --headless --path renderer -s res://export_poses.gd -- \
      --growth 0.02 --size 420 --out poses.json [--only stand,walk]
```

The installer downloads a pinned Godot 4.7.2 under `$BUDDY_DATA_DIR/godot` and writes
`BUDDY_GODOT` into `~/.config/buddy/env`; a system `godot`/`godot4` or an explicit
`BUDDY_GODOT` takes precedence. `BUDDY_RENDERER_DIR` points at a renderer project when it
is not next to the binary. Do not fake a render or substitute a different renderer: the
daemon is designed to keep living without one.

### Body and pre-drawn pictures (Blender)

Blender 5.2.2 is installed at `/usr/bin/blender` and is the intended tool. Run it
headless with `--factory-startup`:

```sh
blender -b --factory-startup --python tools/build_dog.py
blender -b --factory-startup --python tools/build_dog.py -- --preview "$HOME/.cache/buddy-spikes/work/dog"
python3 tools/measure_dog.py [--age puppy] [--picture out.png]
blender -b --factory-startup --python tools/bake_sprites.py -- \
        POSES.json OUT_DIR [--samples 24] [--strands 150000] [--only stand,walk]
DEVICE=GPU COMPUTE=CUDA blender -b --factory-startup --python tools/bake_sprites.py -- ...
spikes/realism/run.sh --device GPU [--focus 0.4 --size 2400x1600 --crop 800]
```

`tools/build_dog.py` writes `renderer/dog/retriever.json`; that file is committed, so
Blender is needed only to change the body. `tools/bake_sprites.py` is normally driven by
the daemon's `baker.rs` while the user is away; a full bake is expensive (72 s/frame CPU,
25 s/frame CUDA measured, roughly 1,700 frames per growth stage). Setting
`scene.cycles.device="GPU"` alone is not enough: `compute_device_type` must be `CUDA` or
`OPTIX` and the device enabled, or Blender silently uses the CPU. Changing the body, the
bake script or the pose recorder invalidates existing sprite sets; changing only motion
code invalidates the affected clips through the fingerprints in `manifest.json`.

### Senses and evidence harnesses

```sh
tests/shell-sense/run.sh          # headless GNOME Shell + extension + daemon; exit 0 on success
spikes/overlay-gnome/run.sh       # overlay / click-through / focus harness
spikes/local-llm/ollama_eval.py MODEL [speed|intent|tools|vision|persian]
spikes/claude-code/run.sh         # historical: the Claude Code boundary was removed
```

### Test and runtime isolation

- `BUDDY_DATA_DIR` — data directory (default `~/.local/share/buddy`).
- `BUDDY_SOCKET` — control socket (default `$XDG_RUNTIME_DIR/buddy/buddy.sock`).
- `BUDDY_HOUR` — fixes the wall-clock hour. Tests that depend on his sleep must set
  `BUDDY_HOUR=12`; otherwise they can fail at night. The lifecycle sandbox already does.
- `BUDDY_GODOT`, `BUDDY_RENDERER_DIR`, `BUDDY_BLENDER`, `BUDDY_PERCEPTION_DIR` — tool
  locations.
- `BUDDY_CAMERA_PIPELINE` — a full GStreamer pipeline with `{out}`, for hardware-free
  camera-look tests. `buddy look --camera` pauses the presence helper and deletes the frame.
- `BUDDY_UPDATE_REPO` / `BUDDY_UPDATE_API` — override the release repository and API base
  for update/upgrade tests; point them at a local HTTP server, never at production.

## Architecture

Two crates, one long-running process, a replaceable renderer, and an optional native control panel.

- `crates/buddy-core` — the brain. Pure simulation: `Sim` advances `Mind` (identity,
  life, needs, mood, personality, training, habits, senses); `behavior` scores his
  repertoire and samples one; `workflow` records application routines and checks replay
  steps; `store` persists named JSON documents, the journal and behaviour records in
  SQLite; `proto` defines the wire messages. No graphics, no
  network, no language model, no filesystem beyond SQLite.
- `crates/buddy` — the daemon and CLI. `daemon.rs` owns `Sim`, ticks at 50 ms while he
  moves and 250 ms at rest, saves every 30 s, and serves one newline-delimited JSON Unix
  socket. `client.rs` is the CLI half. `renderer.rs` supervises Godot and restarts it
  with backoff. `skills.rs` is the only code that touches files or processes; `helpers.rs`
  loads owner-registered argv-only helper manifests and enforces cwd/timeout, with every
  run gated by the sensitive `run_helper` skill.
  `daemon.rs` also tracks the application in front and advances permissioned workflow
  replays; inside applications it uses `perception/a11y_focus.py`, `a11y_act.py` and
  `a11y_text.py` through the off-by-default `accessibility` sense. It can replay a
  mixed sequence of application, accessible, text and assisted manual steps; a manual
  step waits for `buddy workflow continue` and never blind-clicks. `daemon/assist.rs`
  resolves requests,
  gates them on permission, and handles his own offers. `ai.rs` is the Ollama client; `ai_runtime.rs` owns the managed runtime
  manifest, endpoint and port checks; `ai_hardware.rs`, `ai_catalog.rs` and `ai_setup.rs`
  implement the first-run hardware assessment and model choice. Claude Code was removed.
  `baker.rs` runs the offline sprite bake only while the user is away. `system.rs` polls
  idle time, lock and load.
- `ui/` — the optional GTK4/libadwaita control panel (`buddy-ui`). It is a socket client
  like the CLI: it stores no state, never touches the database, and its actions go through
  the daemon. `ui/buddy.svg` is the installed application icon; `dist/buddy-ui.desktop`
  is the launcher. `scripts/install.sh` installs the launcher, icons and desktop entry.
- `renderer/` — the Godot overlay. `main.gd` connects to the socket and draws the state
  it is sent. `dog.gd` is the older procedural stand-in; `retriever.gd` loads the skinned
  model from `renderer/dog/retriever.json` and extends it. `sprites.gd` uses pre-drawn
  pictures when a set matches his age and activity. `voice.gd` plays his sounds.
  `export_poses.gd` records poses for the Blender bake.
- `kde/kwin/buddy/` — an optional KWin script; it calls the daemon's session D-Bus sense
  bridge (`crates/buddy/src/kde.rs`) for focused application, pointer position and polled
  overview state. The KWin script sends `window.caption`; the daemon keeps it only while
  `window_titles` is allowed and never journals it. `kde/kwin-effect` is a native C++
  click effect source, built only where KWin headers exist; it is not runtime-tested here.
- `shell-extension/` and `perception/` — his senses. The GNOME Shell extension also
  registers the guided-capture shortcut (`Super+Shift+B` by default) from its compiled
  schema, and reports
  application focus, fullscreen, overview, pointer, clicks and notifications; Python helpers for
  camera, microphone and screen are started only while their sense is allowed. Privacy
  mode is enforced in the daemon, not in the helpers.
- Data: `~/.local/share/buddy/buddy.db` (SQLite; migrations are the `MIGRATIONS` array in
  `store.rs`; a daily backup sits beside it), `.../sprites/`, and `.../notes.md`.

The daemon is the single decision-maker. The renderer, CLI, control panel and sense
providers are clients; any of them can die without stopping his life. `proto::StateMsg`
is the renderer contract, so adding a field there means updating both the daemon and the
Godot side.

## Working agreements from the project owner

- **Bash only.** The user acts on the machine through a terminal. Give every step they
  must take as a copy-pasteable bash command. When something genuinely needs a GUI click
  (a portal consent dialog, a graphical login), say so plainly and offer a terminal-only
  alternative where one exists. Do not design a verification that waits on a GUI.
- **Use what is installed.** Check `command -v` and `pacman -Q` before writing a
  from-scratch substitute. Blender in particular was expected to build the body; a
  hand-written mesh substitute was a mistake.
- **Scratch outside `/tmp`.** `/tmp` is tmpfs and is wiped between sessions. Keep
  renders, bakes and helper scripts under `~/.cache/buddy-spikes/` (`work/`, `realism/`,
  `reference/`).
- **Commit finished tasks and merge them; never push unasked.** Work on a branch. When
  a task the owner asked for is finished and verified, commit it and merge the branch
  into `main` (fast-forward where possible); push only when explicitly asked. Do not
  leave finished work on a side branch. New phases are still staged behind spikes, a
  written plan and explicit approval.
- **Tracked documents never cite untracked scratch.** A file in git must not point at
  `.superpowers/`, `~/.cache/...`, a session workspace, an absolute home path, or another
  temporary artifact as a source or as the only home of durable knowledge. Copy the
  durable summary into a tracked file and cite that. Product/runtime paths and the
  designated scratch directory as a working instruction are allowed, but never as
  evidence.
- **A plan is process, not a project artifact.** Do not copy a task-by-task plan,
  session ledger or plugin workspace into the repository. When a plan produced durable
  knowledge, move the decisions, interfaces, constraints and evidence into the existing
  artifacts (`README.md`, `docs/decisions.md`, `docs/development.md`,
  `spikes/*/RESULTS.md`, `memory/`) and delete the plan copy. Do not create a parallel
  `docs/plans/`, `docs/superpowers/` or similar tree.

Full detail: `docs/decisions.md` and `docs/development.md`. The versioned session memory
lives in `memory/`; start at `memory/MEMORY.md`.

## Where to look first

- Behaviour, needs, mood, personality: `crates/buddy-core/src/sim.rs`, `behavior.rs`,
  `needs.rs`, `mood.rs`, `personality.rs`.
- Name recognition and bilingual speech: `crates/buddy-core/src/language.rs`
  (`Heard::Called`, `addresses`), `identity.name`, `Life.name_calls`.
- Daemon, CLI, protocol: `crates/buddy/src/daemon.rs`, `client.rs`,
  `crates/buddy-core/src/proto.rs`.
- Skills, permissions, offers: `crates/buddy/src/skills.rs`, `daemon/assist.rs`.
- Named helpers: `crates/buddy/src/helpers.rs`, `buddy helper` commands, `docs/helpers.md`.
- Workflow recording and replay: `crates/buddy-core/src/workflow.rs`, `daemon.rs`,
  `buddy workflow` commands; inside-app capture uses `perception/a11y_focus.py`,
  `a11y_act.py` and `a11y_text.py` behind the `accessibility` sense. Manual steps cover
  apps without a tree without coordinates or synthetic keys. One-shot camera vision uses
  `perception/camera_still.py`; the continuous camera sense remains presence-only.
  Evidence:
  `spikes/workflow-a11y/`.
- Optional provider: `crates/buddy/src/ai.rs` (Ollama), managed by `ai_runtime.rs` on
  `127.0.0.1:11435` with its own model store; first-run choice in `ai_setup.rs`.
- Renderer and body: `renderer/main.gd`, `retriever.gd`, `sprites.gd`,
  `renderer/dog/retriever.json`, `tools/build_dog.py`, `tools/bake_sprites.py`.
- Senses: `shell-extension/`, `perception/`, `crates/buddy/src/system.rs`.
- Control panel and desktop identity: `ui/buddy_ui.py`, `ui/buddy.css`, `ui/buddy.svg`,
  `dist/buddy-ui.desktop`, `scripts/install.sh`.
- Evidence and limits: `spikes/RESULTS.md`, `docs/proportions.md`, README "Known gaps".
- Project memory and standing decisions: `memory/MEMORY.md`, `docs/decisions.md`.

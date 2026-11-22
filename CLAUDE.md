# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

Buddy is a Golden Retriever who lives on a Linux desktop: a long-running Rust daemon
simulates his life, a Godot overlay draws him, and optional local or remote models help
him understand language and act on the computer. The project is at **Phase 5 (skills and
helpers)**; Phase 6 (refinement) has begun with his body. `README.md` is the user-facing
specification and describes every command and behaviour. `docs/decisions.md` records the
project owner's binding decisions; `docs/development.md` records the machine, tools and
working agreements. `memory/MEMORY.md` is the durable, git-tracked project memory that
indexes what future Claude Code sessions should recall.

The invariants that matter most:

- **Local first.** His life never depends on a language model or a network. Ollama and
  Claude Code are optional helpers that fail independently.
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
cargo test                                    # 68 tests, under 30 s on the dev machine
cargo test -p buddy-core <test_name>          # one unit test
cargo test -p buddy --test lifecycle          # daemon persistence/lifecycle integration test
cargo fmt --check                             # rustfmt.toml: max_width 150 (currently reports pre-existing diffs)
cargo run -- daemon                           # run from the checkout
cargo run -- daemon --no-renderer             # headless (no Godot window)
cargo run -- status                           # CLI client against a running daemon
scripts/install.sh                            # install for this user and start with the session
scripts/install.sh --remove                   # stop and remove (keeps ~/.local/share/buddy)
```

`cargo fmt --check` currently reports pre-existing formatting differences on this
toolchain; do not reformat unrelated files unless the task is formatting.

`buddy` is both the daemon and its CLI; the CLI talks to the daemon over one Unix
socket. `README.md` lists the full command set. The diagnostic commands worth knowing:
`buddy why` (current decision and alternatives), `buddy status`, `buddy senses`,
`buddy providers`, `buddy journal`, `buddy snapshot` (only Buddy's window, for drawing
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

Godot is not currently on `PATH` in this environment (`godot`, `godot4` and the Flatpak
are absent). Check before assuming it is available; set `BUDDY_GODOT` or
`BUDDY_RENDERER_DIR` when it is installed elsewhere. Do not fake a render or substitute
a different renderer: the daemon is designed to keep living without one.

### Body and pre-drawn pictures (Blender)

Blender 5.2.2 is installed at `/usr/bin/blender` and is the intended tool. Run it
headless with `--factory-startup`:

```sh
blender -b --factory-startup --python tools/build_dog.py
blender -b --factory-startup --python tools/build_dog.py -- --preview /tmp/dog
python3 tools/measure_dog.py [--age puppy] [--picture out.png]
blender -b --factory-startup --python tools/bake_sprites.py -- \
        POSES.json OUT_DIR [--samples 24] [--strands 150000] [--only stand,walk]
```

`tools/build_dog.py` writes `renderer/dog/retriever.json`; that file is committed, so
Blender is needed only to change the body. `tools/bake_sprites.py` is normally driven by
the daemon's `baker.rs` while the user is away; a full bake is expensive (seconds per
frame, roughly 1,700 frames per growth stage). Changing the body, the bake script or the
pose recorder invalidates existing sprite sets; changing only motion code invalidates
the affected clips through the fingerprints in `manifest.json`.

### Senses and evidence harnesses

```sh
tests/shell-sense/run.sh          # headless GNOME Shell + extension + daemon; exit 0 on success
spikes/overlay-gnome/run.sh       # overlay / click-through / focus harness
spikes/local-llm/ollama_eval.py MODEL [speed|intent|tools|vision|persian]
spikes/claude-code/run.sh         # Claude Code boundary; uses the user's own login
```

### Test and runtime isolation

- `BUDDY_DATA_DIR` — data directory (default `~/.local/share/buddy`).
- `BUDDY_SOCKET` — control socket (default `$XDG_RUNTIME_DIR/buddy/buddy.sock`).
- `BUDDY_HOUR` — fixes the wall-clock hour. Tests that depend on his sleep must set
  `BUDDY_HOUR=12`; otherwise they can fail at night. The lifecycle sandbox already does.
- `BUDDY_GODOT`, `BUDDY_RENDERER_DIR`, `BUDDY_BLENDER`, `BUDDY_PERCEPTION_DIR` — tool
  locations.

## Architecture

Two crates, one long-running process, and a replaceable renderer.

- `crates/buddy-core` — the brain. Pure simulation: `Sim` advances `Mind` (identity,
  life, needs, mood, personality, training, habits, senses); `behavior` scores his
  repertoire and samples one; `store` persists named JSON documents, the journal and
  behaviour records in SQLite; `proto` defines the wire messages. No graphics, no
  network, no language model, no filesystem beyond SQLite.
- `crates/buddy` — the daemon and CLI. `daemon.rs` owns `Sim`, ticks at 50 ms while he
  moves and 250 ms at rest, saves every 30 s, and serves one newline-delimited JSON Unix
  socket. `client.rs` is the CLI half. `renderer.rs` supervises Godot and restarts it
  with backoff. `skills.rs` is the only code that touches files or processes.
  `daemon/assist.rs` resolves requests, gates them on permission, and handles his own
  offers. `ai.rs` is the Ollama client; `claude.rs` is the optional Claude Code helper.
  `baker.rs` runs the offline sprite bake only while the user is away. `system.rs` polls
  idle time, lock and load.
- `renderer/` — the Godot overlay. `main.gd` connects to the socket and draws the state
  it is sent. `dog.gd` is the older procedural stand-in; `retriever.gd` loads the skinned
  model from `renderer/dog/retriever.json` and extends it. `sprites.gd` uses pre-drawn
  pictures when a set matches his age and activity. `voice.gd` plays his sounds.
  `export_poses.gd` records poses for the Blender bake.
- `shell-extension/` and `perception/` — his senses. A GNOME Shell extension reports
  application focus, fullscreen, overview, pointer and notifications; Python helpers for
  camera, microphone and screen are started only while their sense is allowed. Privacy
  mode is enforced in the daemon, not in the helpers.
- Data: `~/.local/share/buddy/buddy.db` (SQLite; migrations are the `MIGRATIONS` array in
  `store.rs`; a daily backup sits beside it), `.../sprites/`, and `.../notes.md`.

The daemon is the single decision-maker. The renderer, CLI and sense providers are
clients; any of them can die without stopping his life. `proto::StateMsg` is the renderer
contract, so adding a field there means updating both the daemon and the Godot side.

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
- **Do not commit or push unless asked.** New phases and large persistent changes were
  staged behind spikes, a written plan and explicit approval; keep that habit.

Full detail: `docs/decisions.md` and `docs/development.md`. The versioned session memory
lives in `memory/`; start at `memory/MEMORY.md`.

## Where to look first

- Behaviour, needs, mood, personality: `crates/buddy-core/src/sim.rs`, `behavior.rs`,
  `needs.rs`, `mood.rs`, `personality.rs`.
- Daemon, CLI, protocol: `crates/buddy/src/daemon.rs`, `client.rs`,
  `crates/buddy-core/src/proto.rs`.
- Skills, permissions, offers: `crates/buddy/src/skills.rs`, `daemon/assist.rs`.
- Optional providers: `crates/buddy/src/ai.rs` (Ollama), `claude.rs` (Claude Code).
- Renderer and body: `renderer/main.gd`, `retriever.gd`, `sprites.gd`,
  `renderer/dog/retriever.json`, `tools/build_dog.py`, `tools/bake_sprites.py`.
- Senses: `shell-extension/`, `perception/`, `crates/buddy/src/system.rs`.
- Evidence and limits: `spikes/RESULTS.md`, `docs/proportions.md`, README "Known gaps".
- Project memory and standing decisions: `memory/MEMORY.md`, `docs/decisions.md`.

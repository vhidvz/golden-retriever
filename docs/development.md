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

Toolchain checked on 2026-11-22:

- rustc and cargo 1.97.1, Python 3.14.7.
- Blender 5.2.2 LTS at `/usr/bin/blender` (installed with pacman).
- Claude Code 2.1.323, logged in by the user.
- GNOME Shell 50.5.
- Godot is **not** on `PATH` here: no `godot`, no `godot4`, no Flatpak. The project needs
  Godot 4.6 or newer (the project file targets 4.7). When it is available, point
  `BUDDY_GODOT` at the binary or `BUDDY_RENDERER_DIR` at the project. The daemon is
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
- **Use installed tools.** Blender, Python, GNOME Shell, Ollama and Claude Code are
  present on the development machine. Prefer them over new dependencies, especially for
  anything the project must keep free and redistributable.
- **Do not commit or push unless asked.** New phases and large persistent changes were
  staged behind spike results, a written plan and explicit approval. Keep that habit.
  This repository uses the owner's own commit messages and history; do not rewrite them.
- **Local first.** Never make Buddy's life depend on Ollama, Claude Code or the network.
  Each provider must fail on its own and be reported in plain words.
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

## Testing and runtime traps

- `cargo test` currently runs 68 tests in under 30 seconds. Unit tests sit beside their
  modules; the daemon lifecycle test is `crates/buddy/tests/lifecycle.rs`.
- **Wall-clock time:** tests must not depend on the hour. The lifecycle sandbox sets
  `BUDDY_HOUR=12` because a sleeping dog answers differently; a test that passes at noon
  and fails at night is this bug.
- **Isolation:** `BUDDY_DATA_DIR` and `BUDDY_SOCKET` give a test its own database and
  socket. A second daemon on the same socket refuses to start; the lifecycle test relies
  on that.
- **Renderer is disposable:** it can crash, be hidden or be missing. The daemon restarts
  it with backoff and keeps simulating. Do not put life-critical logic in Godot.
- **Sprite bakes are expensive:** roughly 10 seconds per frame and about 1,700 frames per
  growth stage. The daemon runs them only while the user is away, pauses them when the
  user returns, and resumes rather than restarting. Use `--only` for a targeted test.
- **Desktop sense test:** `tests/shell-sense/run.sh` starts an isolated headless GNOME
  Shell in its own D-Bus session. It needs `gnome-shell`, `dbus-run-session` and a
  writable runtime directory. `BUDDY_TRACE_SENSES=1` makes the daemon log what it hears.
- **Claude Code:** `crates/buddy/src/claude.rs` clears the environment before running the
  user's `claude` and removes API keys, so work is not silently billed to a key. It is
  read-only unless `--edit`; it always asks first. The spike in
  `spikes/claude-code/run.sh` makes real calls on the user's subscription.
- **Ollama:** the daemon expects it at `127.0.0.1:11434`. It is optional; every caller
  copes with it being absent. `spikes/local-llm/ollama_eval.py` reproduces the
  measurements except the audio test.
- **Screenshots:** `buddy snapshot` saves only Buddy's window, not the desktop. Use it for
  drawing bugs; do not capture the user's screen for diagnosis.

## Evidence and limits

- `spikes/RESULTS.md` — overlay, fur performance, local-model benchmarks, Claude Code
  behaviour and long-run simulation results. The design rests on these measurements.
- `docs/proportions.md` — how Buddy's body was measured against breed standards,
  photographs and skeletal studies, and what changed as a result.
- `README.md` "Known gaps" — the current, maintained list of what is not done. Prefer it
  over older notes when they disagree.

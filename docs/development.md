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
- **A plan is process, not a project artifact.** Do not copy a task-by-task plan or
  session ledger into the repository. Extract its durable decisions, interfaces,
  constraints and evidence into the existing docs, spikes or memory, then delete the
  plan copy. A future session should not need a separate plans tree.

## Testing and runtime traps

- `cargo test` currently runs 100 tests in under 30 seconds. Unit tests sit beside their
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
- **Workflows:** `buddy workflow record NAME`, `stop`, `replay`, `workflows` for
  application-level routines. Inside applications, turn on the `accessibility` sense,
  run `buddy workflow capture NAME`, focus a control, press `Super+Shift+B` (or run
  `buddy workflow capture-step`) for each action, then `buddy workflow stop` and
  `buddy workflow replay NAME`. The shortcut is registered by the GNOME extension from
  its compiled schema; `scripts/install.sh` compiles it. Replay
  locates the control by app/frame/role/name, invokes its accessible action, and waits
  for a semantic event; it sends no keys, coordinates or clipboard data. Text entry,
  password fields, terminals, shells and apps without an accessibility tree are refused;
  mixed app-plus-inside workflows are rejected for now. Evidence:
  `spikes/workflow-a11y/RESULTS.md`; durable decisions are in `docs/decisions.md`. No
  separate plan file is kept as a project artifact.
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
- `docs/proportions.md` — how Buddy's body was measured against breed standards,
  photographs and skeletal studies, and what changed as a result.
- `README.md` "Known gaps" — the current, maintained list of what is not done. Prefer it
  over older notes when they disagree.

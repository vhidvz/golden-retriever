# Automation direction: helpers, synthetic input, and the local model

**Status:** research record, 2026-11-23. This is not an approved decision. The owner
asked for the research to be kept as part of Buddy's completion roadmap; the choice
between the options below is still open. Binding decisions live in `docs/decisions.md`.

## The question

Should Buddy be given a virtual mouse and keyboard, shell/script automation skills, or
both? Can `gemma4:e4b` drive either reliably?

## What exists today

- `run_command` is already a skill: `Risk::Sensitive`, always asks, an exact command
  allowlist, and a 60-second timeout (`crates/buddy/src/skills.rs`).
- Workflows record application-level routines and, with the `accessibility` sense on,
  focused accessible actions inside applications. Replay locates the control semantically,
  invokes its accessible action and verifies a semantic event. It never sends keys,
  coordinates or clipboard data (`crates/buddy-core/src/workflow.rs`,
  `crates/buddy/src/daemon.rs`).
- The owner's standing requirement prefers accessibility APIs and structured tools over
  coordinate replay, and requires outcome verification (`docs/decisions.md`, additional
  requirement 2).
- This machine: Wayland, GNOME Shell 50.5, `xdg-desktop-portal` 1.22.1,
  `xdg-desktop-portal-gnome` 50.0. The `RemoteDesktop` and `InputCapture` portal
  interfaces are present. `ydotool`/`wtype` are not installed; `xdotool` sees only X11
  windows.

## Findings

### 1. Synthetic input on Wayland

`libei` through the `RemoteDesktop` portal (`ConnectToEIS`) is the supported,
cross-desktop path. It has been integrated since xdg-desktop-portal 1.17 (2023), GNOME
supports it from 45, and session persistence (`restore_token`) arrived in 1.21. Xwayland
routes XTEST through the same portal. Raw `/dev/uinput` tooling works below the
compositor but was already rejected in `spikes/RESULTS.md` as the chosen path.

The costs are real:

- A portal consent dialog titled "Remote Desktop" appears even for a keyboard-only
  session. It is a heavy permission to ask a desktop pet to hold.
- GNOME 50 has session-lifecycle bugs; upstream fixes (including input-only sessions)
  landed in GNOME 51. Real clients have left input grabs stuck, freezing the local
  keyboard and mouse until the process was killed.
- Synthetic input can reach anything: password dialogs, terminals, consent prompts. It
  carries no semantic context and gives no verification that the intended control was
  the one acted on.

### 2. GUI agents are still below human reliability

- OSWorld: human 72.36%; the best baseline at release reached 12.24% with an
  accessibility tree and about 5% from screenshots alone (NeurIPS 2024).
- By 2025-2026 frontier agents reached roughly 60-70% on OSWorld, still below human,
  taking 2.7-4.3 times more steps; planning dominated task time (OSWorld-Human).
- Windows Agent Arena: 19.5% agent versus 74.5% human.
- The observation medium matters more than any other design choice. ComponentBench
  found the same model shifting by more than 30 points between an accessibility tree and
  pixels (GPT-5 mini: 83.1% versus 48.9%), with the gap widening on harder tasks.
  Weaker models gained 9-37 points from the accessibility tree in Greptile's OSWorld
  experiment; the strongest models saw no significant difference. Hybrid observation
  (screenshot plus tree, element-ID clicks) is the emerging consensus.

The coordinate-and-pixel path that virtual mouse control implies is the weakest path,
and it is the one the project deliberately avoided.

### 3. Shell automation

- Terminal-Bench 2.0: the best agent scored 49.6% at launch (November 2025); the paper
  reports frontier agents below 65% and smaller models around 15%. The 2026 leaderboard
  has reached 60-78%, still failing a substantial share.
- The largest single failure class was "command not found / not in PATH" (24.1%), which
  deterministic scripts and packaged helpers remove.
- Security is the dominant cost: indirect prompt injection from files, pages or PDFs has
  produced real RCE (`sudo rm -rf` via a poisoned document); "safe" command allowlists
  are bypassable through argument injection into otherwise allowed binaries
  (`git show --output=`, `rg --pre`, `fd -x`, `go test -exec`). The documented mitigation
  is architectural: sandboxing, least privilege, argument separation, egress control and
  human gates, not model-level refusal or an allowlist alone.

### 4. What `gemma4:e4b` can and cannot do

Project measurements (`spikes/RESULTS.md`):

| Work | Result |
|---|---|
| Choose one of eight tools with sensible arguments, English and Persian | 5/5 |
| Schema-constrained intent, English / Persian | 10/10 and 10/10, 20/20 valid JSON |
| Speed and memory | 13.8 tok/s generation on CPU, 7.2 GB resident, no VRAM |

Independent measurements for the same model class (Gemma 4 E4B, 8B total / 4.5B
effective, Apache-2.0):

| Benchmark | Result |
|---|---|
| τ²-bench | 42.2% lab average (thinking), 20.8% independent |
| Terminal-Bench 2.1 / Hard | 1.9% / 8.3% |
| Context | 128K, though Buddy uses 8K |

The small-model literature agrees: single-turn, schema-constrained tool calling is
viable; multi-turn and long-horizon planning degrade sharply. Multi-turn function
calling collapsed for a 4B model from 88% to 16.9% in one study, and tool-call thrashing
and format drift are the common failure modes at 4-8B.

**Interpretation:** `gemma4:e4b` is a good one-shot router: which skill or helper, with
which arguments. It is not a multi-step GUI or terminal agent. The two roles should not
be conflated.

## Options

**A. Virtual mouse and keyboard first.** Works everywhere, including applications with
no accessibility tree. But it contradicts the accessibility-first decision, needs a
heavy portal grant, is fragile on GNOME 50, cannot be verified semantically, and the
current model cannot generate the sequences. Only the "applications without a tree"
niche benefits.

**B. Shell/script helpers first.** Fits Phase 5 and the existing skills contract,
is deterministic and testable, has a journal and undo, and matches the owner's
terminal-first workflow. Shell risk is contained by named parameterized scripts plus a
sandbox, not by free-form commands.

**C. Both, staged.** B now; A later only as replay of user-recorded sequences with
preconditions, verification, an immediate stop and audit.

## Recommendation (not yet approved)

**Option C, in the order B then A.**

### Stage 1: named helpers (recommended first)

- A helper is a named script with declared arguments, a dry run, an expected-result
  check and a timeout. It lives in a folder the owner has approved.
- The model chooses the helper and fills its arguments; deterministic code executes,
  verifies and records the outcome.
- This extends the existing skills contract: the tool list becomes dynamic (today
  `SKILLS` is a static array), helpers run sandboxed, and the journal records what ran.
  `run_command` remains the always-ask emergency door.
- Candidate first helpers: search and open files, inspect a project, run build or test
  commands with a known working directory, organize downloads, back up a folder.

### Stage 2: synthetic input, only if the niche is real

- A separate sense, off by default, with its own permission and explicit portal
  consent; privacy mode suspends it.
- Replay of sequences the user recorded, never clicks or keys generated by
  `gemma4:e4b`.
- Preconditions (application, window, focus), per-step verification, an immediate stop
  shortcut, and an audit journal.
- Requires a spike on GNOME 50.5 first; input-only session support is reported fixed in
  GNOME 51.

### The model's role

- `gemma4:e4b` stays the one-shot router and argument filler.
- No model-driven multi-step loop for either GUI or shell.
- A dedicated GUI grounding model (for example UI-TARS-1.5-7B, Apache-2.0, OSWorld
  27.5) is a research spike, not a product path on 2-4 GB of VRAM; it is also far below
  human reliability.

## Decision triggers

- Choose **B** if the need is files, commands, builds, backups and repetitive local
  tasks.
- Choose **A/C** only if the blocking need is applications that expose no accessibility
  tree.
- Any implementation still needs the established sequence: spike results, a written
  plan, and explicit owner approval. This record is the starting evidence, not the
  approval.

## Sources

- XDG Desktop Portal, RemoteDesktop:
  <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop>
- xdg-desktop-portal-gnome NEWS: <https://github.com/GNOME/xdg-desktop-portal-gnome/blob/main/NEWS>
- wayland-devel, synthetic input via libei:
  <https://lists.freedesktop.org/archives/wayland-devel/2025-May/044241.html>
- Deskflow, GNOME 50 input-session bug: <https://github.com/deskflow/deskflow/issues/10248>
- OSWorld: <https://arxiv.org/abs/2404.07972>
- OSWorld-Human: <https://arxiv.org/html/2506.16042v1>
- ComponentBench:
  <https://proceedings.colm.cc/paper_files/paper/2026/file/0ef6ffcb85a2d238fc4761860c31ded4-Paper-Conference.pdf>
- Greptile, how should an LLM see a computer:
  <https://www.greptile.com/blog/trex-computer-use>
- Terminal-Bench 2.0: <https://epoch.ai/benchmarks/terminal-bench>,
  <https://github.com/harbor-framework/terminal-bench-2>
- Gemma 4 E4B: <https://huggingface.co/google/gemma-4-E4B>,
  <https://modelmenu.dev/models/gemma-4-e4b>
- Small Language Models for Agentic Systems: <https://arxiv.org/abs/2510.03847>
- TinyLLM: <https://arxiv.org/abs/2511.22138>
- Trail of Bits, prompt injection to RCE:
  <https://blog.trailofbits.com/2025/10/22/prompt-injection-to-rce-in-ai-agents/>
- HiddenLayer, indirect prompt injection of computer use:
  <https://www.hiddenlayer.com/research/indirect-prompt-injection-of-claude-computer-use>
- UI-TARS-1.5-7B: <https://huggingface.co/ByteDance-Seed/UI-TARS-1.5-7B>

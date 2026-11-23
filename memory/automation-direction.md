---
name: automation-direction
description: "Research (2026-11-23) on virtual input versus shell/script automation and what gemma4:e4b can drive; recommendation is named helpers first, synthetic input later as a fallback"
metadata:
  type: project
---

On 2026-11-23 the owner asked whether Buddy should get a virtual mouse and keyboard,
shell/script automation skills, or both, and whether `gemma4:e4b` can drive either.
The research was saved as part of the completion roadmap; **no option is approved yet**.
The full record is `docs/automation-direction.md`.

The durable conclusions:

- `gemma4:e4b` is a good one-shot router: the project measured 5/5 tool calls, 20/20
  valid JSON and 10/10 intent in English and Persian at 13.8 tok/s on CPU. Independent
  agentic benchmarks are weak (τ²-bench 20.8-42.2%, Terminal-Bench 2.1 1.9%), and the
  small-model literature shows multi-turn planning collapsing at 4-8B. It must not be
  given a multi-step GUI or terminal loop.
- Shell automation is more reliable and verifiable than GUI replay, but is not
  autonomous-safe: frontier terminal agents still fail a large share of tasks, and
  prompt injection to RCE plus argument-injection bypasses of command allowlists are
  documented. The mitigation is sandboxing and least privilege, not a model refusal.
- Synthetic input on Wayland goes through libei and the RemoteDesktop portal
  (`ConnectToEIS`); portal 1.22.1 and GNOME 50.0 are installed here, but GNOME 50 has
  input-session lifecycle bugs fixed in GNOME 51, and the grant is a heavy consent.
- Accessibility-tree observation remains the most reliable structured channel; the
  project's standing preference for accessible actions over coordinate replay still
  holds.

Recommended shape (proposal, not decision): named helper scripts with declared
arguments, a dry run, an expected-result check, a timeout and a sandbox, with the model
only choosing the helper and filling arguments; `run_command` stays the always-ask
emergency door. Virtual input, if ever, only replays user-recorded sequences through
the portal, with preconditions, per-step verification, an immediate stop and audit.

**Why:** the owner wants the research kept for later implementation without redoing it,
and the choice changes the Phase 5/6 shape.

**How to apply:** read `docs/automation-direction.md` before implementing automation.
Keep the model a one-shot router, keep every action permissioned and verified, and do
not add model-generated clicks or keys. Related: [[workflow-recording]],
[[buddy-additional-requirements]], [[tracked-docs-no-untracked-refs]].

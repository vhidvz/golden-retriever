---
name: self-contained-ai
description: "Approved direction: managed Ollama as Buddy's sole AI runtime, Claude Code removed, AI4Animation deferred until a consumer exists"
metadata:
  node_type: memory
  type: project
  originSessionId: 5b57146d-145d-465d-8b74-d74014873801
  modified: 2026-11-22T00:00:00.000Z
---

On 2026-11-22 the owner asked for Buddy's AI capabilities to become self-contained: no
Claude Code subscription, Ollama as the sole managed runtime, AI4Animation data in the
installation lifecycle, and a hardware-aware model choice. The decision record
is `docs/self-contained-ai-decision.md`; the implementation plan is
`docs/self-contained-ai-plan.md`. Status: **approved 2026-11-22**. Phases 0–3 are
implemented and Phase 4 is a completed research spike. No commit or push has been made.
Deferred: the daemon's 30-second health loop, `buddy ai update`, AMD/Vulkan detection, a
per-request keep-alive policy, `/api/ps` placement, and the retarget/render part of the
AI4Animation spike.

The main decisions: Claude Code removal is complete (`claude.rs`, the `ask_claude` skill,
the CLI command, provider status and tests are gone; seven skills remained, and a later workflow replay skill makes eight); the managed
runtime is a pinned user-local Ollama on port 11435 with its own model store; AI failure
never blocks Buddy's life; AI4Animation has no consumer in the current code and stays
optional/separate (CC BY-NC 4.0), which amends decision 6 and supersedes additional
requirement 11.

**Why:** the owner's direction and the evidence from the 2026-11-22 assessment; a future
session must not implement ahead of approval or reintroduce Claude Code as a peer. The
owner approved D1–D11 with the recommended answers: AI4Animation deferred until a motion
feature exists, CC BY-NC acceptance before any future download, linger offered, no local
coding agent now, permissive model licences by default, ~12 GB default disk budget, and
Linux/systemd only.

**How to apply:** read the decision record before changing AI code; keep local-first and
the seven remaining skills; never make AI4Animation a daemon dependency. Related:
[[buddy-product-decisions]], [[buddy-additional-requirements]].

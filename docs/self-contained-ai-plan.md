# Self-contained AI runtime — implementation plan

**Status: approved 2026-11-22; Phases 0–3 implemented and the Phase 4 spike completed.**
No commit or push has been made. Phase 4 integration remains conditional on an approved
motion feature and CC BY-NC 4.0 acceptance.

**Date:** 2026-11-22

**Decision record:** `docs/self-contained-ai-decision.md`

**Scope:** installation, managed Ollama runtime, hardware-aware model selection, removal
of Claude Code, and the conditional AI4Animation data lifecycle. Unrelated features,
UI redesign, cloud models and a local coding agent are out of scope.

## Principles

1. **Local first.** Buddy's life never depends on the runtime, a model or the network.
   AI failure disables AI features, not the daemon.
2. **One owner per thing.** systemd owns the runtime process; Buddy owns configuration,
   model choice and readiness state; the installer orchestrates, it does not duplicate
   runtime logic.
3. **Smallest coherent change.** Remove Claude Code, add the managed runtime, then add
   model selection. AI4Animation is a separate, conditional phase because it has no
   consumer yet.
4. **Reversible.** Every phase has a rollback that leaves Buddy alive. Runtime versions
   and models are pinned; updates are explicit.
5. **Evidence before claims.** Each phase has a verification gate. A feature is not
   "done" because it compiles; it is done when the gate passes.
6. **No silent permission widening.** The local model still only chooses a skill;
   `skills.rs` remains the only code that touches files and processes.

## Target architecture

| Component | Responsibility | State owner |
| --- | --- | --- |
| `buddy-ollama.service` | Runs the pinned `ollama serve` on loopback; restarts on failure | systemd user manager |
| `buddy.service` | Buddy's daemon; unchanged graphical-session lifecycle | systemd user manager |
| `ai_runtime.rs` (new) | Reads the runtime manifest, health-checks the endpoint, chooses/report ports | read-only |
| `ai.rs` | HTTP client to the configured endpoint; request-level `num_ctx` and `keep_alive` | — |
| `buddy ai …` (new CLI group) | Setup, status, model, doctor, repair, prune; a Rust `update` command is deferred | runtime manifest + SQLite settings |
| `runtime.json` | Pinned runtime version, tarball SHA-256, port, store path, service name, model digest | installer / CLI |
| SQLite `Settings` | User choices: language model, vision model; audio and keep-alive settings are deferred | daemon |
| `scripts/install.sh` | Preflight, install Buddy, install/verify runtime, print the next `buddy ai setup` step | — |
| `dist/buddy-ollama.service.in` | Unit template rendered by the installer with the resolved data dir and private HOME | — |

### Topology

```text
buddy.service (graphical-session.target)
        │  HTTP 127.0.0.1:<port>
        ▼
buddy-ollama.service (default.target)  ──OLLAMA_MODELS──▶  $BUDDY_DATA_DIR/ollama/models
        │
        └─ $BUDDY_DATA_DIR/ollama/<version>/{bin,lib}
```

The AI runtime does not sit behind the renderer or the Buddy window. Hiding Buddy,
restarting the daemon or losing the renderer does not stop it.

## Work breakdown

### Phase 0 — Decisions and records

**Goal.** Make the direction explicit and resolve the two conflicts with existing
binding decisions before touching code.

**Tasks.**

- [x] Owner approves, amends or rejects D1–D11 in the decision record.
- [x] Owner answers the open decisions: AI4Animation consumer, CC BY-NC acceptance,
      lingering, `ask_claude` replacement, model licence policy, disk budget.
- [x] Add an amendments pointer to `docs/decisions.md` and a binding memory entry.
- [x] Confirm the supported platform is Linux with systemd user services.
- [x] Confirm no commit/push is expected in this phase.

**Exit criteria.** The decision record is marked approved (or amended) and the open
decisions have answers.

**Result.** Complete on 2026-11-22. The answers are recorded in
`docs/self-contained-ai-decision.md` under "Approval". No code was changed.

**Estimate.** One owner review; no code.

### Phase 1 — Remove Claude Code

**Goal.** Make Ollama the sole AI runtime with no subscription assumptions left in code,
configuration, CLI, tests or documentation.

**Changes.**

| File | Change |
| --- | --- |
| `crates/buddy/src/claude.rs` | Delete the module and its tests. |
| `crates/buddy/src/main.rs` | Remove `mod claude`, the usage lines and the `ask-claude` route. |
| `crates/buddy/src/daemon.rs` | Remove the `ask_claude` command path. |
| `crates/buddy/src/daemon/assist.rs` | Remove the `ask_claude` branch, `ask_claude()` function and the Claude entry in `providers`. |
| `crates/buddy/src/client.rs` | Remove the Claude lines from `providers`. |
| `crates/buddy/src/skills.rs` | Remove the `ask_claude` skill; `SKILLS` becomes seven; update `describe()` and tests. |
| `crates/buddy/src/ai.rs` | Remove the "Use ask_claude…" instruction from the `plan` prompt. |
| `README.md`, `docs/`, `spikes/`, `memory/` | Remove or mark historical; update the skills table and provider description. |

**Tasks.**

- [x] Delete runtime code and tests.
- [x] Update the skill list, usage text and provider output.
- [x] Search the repository for `claude`, `ask_claude`, `ask-claude`, `ANTHROPIC` and
      `subscription`; every remaining hit is historical or a deliberate note.
- [x] Unknown skill names are denied safely by `Permissions::policy`; old persisted
      policies need no migration.
- [x] Keep old journal entries readable. Do not rewrite history.
- [x] Update the "Known gaps" and helpers sections in `README.md`.

**Verification.**

- `cargo test` passes.
- `cargo fmt --check` shows no new diffs (the toolchain already reports pre-existing
  differences; do not reformat unrelated files).
- `buddy skills` listed seven skills at that point; the later workflow replay skill makes eight.
- `buddy providers` has no Claude line.
- A request that previously routed to `ask_claude` now produces a clean "no skill fits"
  or local-model result; no panic, no dangling command.
- A grep of `crates/` finds no Claude Code reference.

**Rollback.** Revert the phase's commit. Old data is untouched.

**Result.** Complete on 2026-11-22. `claude.rs`, the skill, the command, provider status,
tests and docs are gone; `cargo test` passed (66 at that point; 88 after all phases); a grep of `crates/` finds only
the regression assertion that `ask_claude` is not a skill. `buddy providers` shows only
the local runtime.

**Estimate.** 0.5–1 day.

### Phase 2 — Managed Ollama runtime

**Goal.** Install, start and supervise a pinned Ollama runtime owned by Buddy, on a
dedicated port and model store, without touching a system Ollama.

**Changes.**

| Area | Change |
| --- | --- |
| `crates/buddy/src/ai_runtime.rs` (new) | Runtime manifest (read/write), port selection, endpoint resolution, health checks (`/api/version`, `/api/tags`, `/api/ps`). |
| `crates/buddy-core/src/paths.rs` | Add `ollama_dir()`, `ollama_models_dir()`, `runtime_manifest_path()` and `ai4animation_dir()` using the existing data-dir rules. |
| `crates/buddy/src/ai.rs` | Remove the hard-coded URL; read the managed endpoint. Add request-level `num_ctx`. |
| `crates/buddy/src/main.rs` | Add the `buddy ai` command group (`status`, `doctor`, `setup`, `model`, `repair`, `prune`). |
| `crates/buddy/src/daemon/assist.rs` | `providers` reports runtime version and endpoint; placement is not yet reported. |
| `dist/buddy-ollama.service.in` (new) | User unit template with the environment and hardening described in D4/D6. |
| `scripts/install.sh` | Preflight; download the pinned tarball with retry/resume; verify SHA-256; extract atomically; render the unit with the resolved data dir; `enable --now`; health check; offer linger. |
| `docs/development.md` | Replace "Ollama is expected at 11434" with the managed runtime description; update the hardware baseline. |
| `README.md` | Describe the managed runtime, `buddy ai` commands and the removal of Claude Code. |

**Tasks.**

- [x] Define `runtime.json`:

  ```json
  {
    "schema": 1,
    "ollama_version": "0.40.17",
    "tarball_sha256": "…",
    "endpoint": "http://127.0.0.1:11435",
    "service": "buddy-ollama.service",
    "models_dir": "/home/user/.local/share/buddy/ollama/models",
    "model": null,
    "model_digest": null
  }
  ```

- [x] Implement port selection 11435–11445 with a persisted choice; never kill the
      occupant.
- [x] Implement download: `curl` or Rust HTTP with `--retry`, resume, progress, SHA-256
      verification against the release `sha256sum.txt`; extract to a temporary directory
      and rename atomically. The 1.44 GiB download was run end-to-end on 2026-11-23.
- [x] Render the systemd user unit with the actual data dir, not a hard-coded
      `~/.local/share/buddy`; respect `XDG_DATA_HOME`.
- [x] `systemctl --user daemon-reload`, `enable --now`, health check; if 11434 is in use,
      record it and leave it alone.
- [ ] Health loop: version/tags every 30 s; expose state as `not-installed`, `stopped`,
      `starting`, `ready`, `model-missing`, `unhealthy`. (Deferred: `ai status` and
      `ai doctor` probe on demand instead of running a daemon loop.)
- [x] Restart policy: systemd `Restart=always` is in the unit. Buddy does not yet request
      a bounded restart itself; systemd owns recovery.
- [x] `buddy ai doctor` reports port, store, service, system Ollama, disk, model digest.
- [ ] `buddy ai update` downloads a new version, switches `current`, restarts, checks,
      keeps the previous version for rollback. (Deferred: `OLLAMA_VERSION` on a rerun is
      the current manual path.)
- [x] `scripts/install.sh --remove` stops/disables the unit and removes it; the runtime
      and models are kept for a later reinstall. A Rust `buddy ai uninstall --purge` is
      not implemented.
- [x] No fallback to 11434 by default. An explicit `--external` escape hatch may be
      added later; it is not part of this phase.

**Verification.**

- The installer and rendered unit are inspected; there is no `--dry-run`. The real
  install, including the 1.44 GiB download, was run end-to-end on 2026-11-23.
- `systemd-analyze verify` accepts the rendered unit.
- Managed service is active on 11435; `curl 127.0.0.1:11435/api/version` answers.
- The system Ollama on 11434 is still active and its models are unchanged.
- `systemctl --user stop buddy-ollama` makes Buddy report "runtime stopped"; the daemon
  stays alive and `buddy status` still works.
- `systemctl --user start buddy-ollama` returns the state to ready.
- `kill` the Ollama process; systemd restarts it; Buddy recovers.
- Reboot or logout/login: the unit starts with the user manager; with linger, it starts
  before login.
- A truncated tarball download resumes and verifies; a corrupted tarball is rejected and
  the previous runtime remains.
- `buddy providers` shows runtime version, endpoint, model and placement.

**Rollback.** Disable/stop the unit, remove `$BUDDY_DATA_DIR/ollama`, and point
`ai.rs` at the previous endpoint via configuration. The code change can be reverted
independently of the data.

**Estimate.** 3–5 days.

**Result.** Implemented on 2026-11-22. The user-local `bin/../lib/ollama` layout was run
on 127.0.0.1:11435 with a separate model store; the system Ollama on 11434 stayed
untouched. `buddy ai status` and `buddy ai doctor` were verified against that runtime.
The installer is syntax-checked and its unit is accepted by `systemd-analyze verify`; a
transient user unit with the unit's `ProtectHome=read-only` and a private `HOME` under
the runtime directory answered `/api/version`. On 2026-11-23 the real 1.44 GiB download,
SHA-256 check, extraction and user-service install were run end-to-end; the managed
runtime answered `/api/version` as 0.40.17. The 30-second health loop, bounded restart request, `buddy ai update` and
`/api/ps` placement remain deferred.

### Phase 3 — Hardware-aware model selection

**Goal.** A first-run flow that detects the machine, recommends a model with trade-offs,
lets the user choose, pulls and verifies it, and persists the choice.

**Changes.**

| Area | Change |
| --- | --- |
| `crates/buddy/src/ai_hardware.rs` (new) | CPU, RAM, NVIDIA, disk and existing-Ollama probes; best-effort with timeouts; fixture-friendly parsing. AMD/Vulkan detection is not implemented. |
| `crates/buddy/src/ai_catalog.rs` (new) | Model catalogue as data: name, size, capabilities, licence, minimum memory, notes; cloud models excluded. |
| `crates/buddy/src/ai_setup.rs` (new) | Scoring, recommendation, alternatives, interactive choice, pull, verify, persist. |
| `crates/buddy/src/ai.rs` | Use the selected language/vision model; pass `num_ctx` and `keep_alive` (the latter is a 5m constant). |
| `crates/buddy-core/src/model.rs` | Keeps `language_model` and `vision_model`; no separate `audio_model` or `keep_alive` setting was added. |
| `crates/buddy/src/main.rs` | `buddy ai setup`, `buddy ai model`, `buddy ai repair`, `buddy ai prune`. |
| `spikes/local-llm/ollama_eval.py` | Not re-run for the new candidates; only `gemma4:e4b` has project measurements. |
| `docs/development.md`, `README.md` | Document the flow, defaults and how to change the model. |

**Tasks.**

- [x] Hardware probe:
  - CPU: `/proc/cpuinfo`, AVX flags; `lscpu` was not needed.
  - RAM: `/proc/meminfo`, total and available.
  - NVIDIA: `nvidia-smi --query-gpu=name,memory.total,driver_version,compute_cap`.
  - [ ] AMD: `rocm-smi` / `rocminfo`; ROCm v7. Not implemented; an AMD machine falls back to CPU recommendations.
  - [ ] Intel/Vulkan: `lspci`, `vulkaninfo --summary`. Not implemented.
  - [x] Disk: `df -Pk` on the data dir.
  - [x] Existing Ollama: a loopback connection to 11434; the system service is left untouched.
  - [x] Every probe has a timeout and a "not available" result; none blocks installation.
- [x] Catalogue initial entries: `gemma4:e4b`, `gemma4:e2b`, `lfm3:1.7b`,
      `lfm3-vl:2b`, `lfm3-omni:2b`, `bonsai-2` 4B/9B, `qwen3-vl:8b`,
      `gemma4:12b/26b`. Mark which are project-verified and which need evaluation.
- [x] Capability filter: text + tools + JSON + English/Persian for the default; `--vision`
      and `--audio` add the corresponding requirement. It does not yet read the senses.
- [x] Fit heuristic (simplified): the catalogue carries `min_ram_gb` and `min_vram_gb`;
      `cpu_friendly` gates CPU-only machines; quality, verified and download size score the
      candidates. The 1.15/KV-cache estimate and desktop-VRAM measurement are not implemented.
- [x] Recommendation output: default plus alternatives, with name, size, minimum RAM and
      an evaluation marker. Speed and licence are not printed yet. The user's choice is final.
- [x] Pull:
  - `POST /api/pull` with `stream: true`;
  - show `status`, `digest`, `total`, `completed`;
  - retry on network errors; resume by re-running the pull;
  - Ollama verifies SHA-256; Buddy records the digest from `/api/tags`;
  - if the model is missing after pull, fail with a clear message.
- [x] Persist the choice in settings and the digest in `runtime.json`.
- [x] `buddy ai model` lists installed models and changes the choice; `buddy ai repair`
      re-pulls/re-verifies; `buddy ai prune` removes unused models after checking the running daemon and `--yes`.
- [ ] `keep_alive`: 5m is set in the service unit; 0/1m on RAM-constrained machines and a
      per-request policy are deferred.
- [ ] `buddy providers` shows the runtime version, endpoint and models; `/api/ps` placement
      is deferred.

**Verification.**

- Scoring fixtures for 8 GB RAM / no GPU, 16 GB RAM / 6 GB VRAM, 31 GB RAM / 4 GB VRAM.
- On the current machine, `gemma4:e4b` is recommended; `ollama ps` reported it on the
  GPU during a real chat, and the system Ollama was not used.
- On an 8 GB profile, a small model is recommended and the reason is explained.
- The user can override the recommendation; the choice survives a daemon restart and a
  reboot.
- An interrupted pull resumes and verifies; a digest mismatch is reported.
- `spikes/local-llm/ollama_eval.py` passes the project's quality bar for the recommended
  model: English/Persian intent, valid JSON, tool calls; vision/audio only if the chosen
  model advertises them.
- `num_ctx` is explicit in requests; `ollama ps` shows the expected context and placement.

**Rollback.** Change the model in `buddy ai model`; the previous model files can stay.
Settings are versioned with the existing store.

**Estimate.** 4–6 days, including evaluation.

**Result.** Implemented on 2026-11-22. The hardware probe, catalogue, fit heuristic,
interactive/`--yes` setup, pull stream, digest verification, manifest and settings
persistence, `ai model`, `ai repair` and `ai prune` are in place. On the current machine
the default recommendation is `gemma4:e4b`; a synthetic 8 GB profile chooses a small
model. `buddy ai setup --yes` was exercised against a mock pull stream and then against the
real runtime: `gemma4:e4b` (6.6 GB) was installed, selected for language and vision,
recorded with its digest, and answered a tiny local chat request. Only `gemma4:e4b` is project-verified;
a review pass added disk and available-RAM fit checks, cloud-model guards, daemon-aware
pruning and corrupt-manifest reporting.
AMD/Vulkan detection, the per-request keep-alive policy, `/api/ps` placement and a fresh
evaluation of the other catalogue entries remain deferred.

### Phase 4 — AI4Animation data (conditional)

**Goal.** Determine whether the MANN dog data improves Buddy enough to justify the
licence and integration cost, then build only the lifecycle the consuming feature needs.

**Gate.** Do not start this phase until the owner names a consumer and accepts the
CC BY-NC 4.0 terms (or decides the data stays a research-only separate download).

**Gate update 2026-11-22.** The owner asked to start Phase 4. It was executed as a
research spike only: no consumer was named, no data was installed into Buddy, and the
run script requires `--accept-license` before downloading. D8 still defers mandatory
integration.

**Spike tasks.**

- [x] Download the MANN archive; verify SHA-256
      `0ecd909b673cfca5e20c3f568376dcf27d7da3bd153957c2b034d95af366132a`.
- [x] List and validate 51 BVH files; confirm the 21-joint skeleton and frame rate.
- [x] Convert one clip to NPZ or parse BVH directly. A standard-library BVH parser was
      written; AI4AnimationPy was not needed or added.
- [x] Map the 21 joints to Buddy's 25 bones as a design sketch (see the decision record
      and `spikes/ai4animation/README.md`). The muzzle, jaw, ears, hind paws and `tail3`
      stay procedural.
- [ ] Retarget one clip; check foot contacts, root motion and scale. Not done.
- [ ] Render it in the Godot renderer or bake a short sprite clip; compare with the
      procedural version. Not done.
- [x] Measure: archive size, skeleton, frame count, frame rate and uncompressed size. A
      visual/frame-cost comparison was not done.
- [x] Write the result and a go/no-go recommendation in
      `spikes/ai4animation/RESULTS.md`.

**If go:** define the data manifest (version, URL, SHA-256, file count, extracted size,
licence), the installer's retry/resume/atomic extraction, the readiness state, the
licence acceptance and the side-load path. Make the data required for the motion feature,
not for the daemon.

**If no-go:** record the result; keep the data out of the installation lifecycle and
remove the decision's deferral note only when a future feature reopens it.

**Verification.** Visual evidence, a measured cost, a licence record, and a clean
fallback to the procedural body when the data is absent.

**Estimate.** 2–3 days for the spike; the optional component is separately estimated and
approved.

**Result.** The spike is complete on 2026-11-22. `spikes/ai4animation/` contains a
licence-gated downloader, a standard-library BVH parser and the findings. The archive is
31.9 MiB, holds 51 clips, and the inspected clip has 21 joints, 66 channels, 1,531 frames
at 60 fps. Nineteen MANN joints map naturally to Buddy bones; `LeftArm` and `RightArm` do
not, and seven Buddy bones have no source. Recommendation: **no-go for mandatory
installation now**; retargeting and a visual comparison are the next step only if a
motion feature is approved.

## Cross-cutting work

### Documentation

- `README.md`: AI runtime, model selection, removal of Claude Code, new `buddy ai`
  commands, known gaps.
- `docs/development.md`: managed runtime, port, store, service, hardware baseline,
  evaluation commands.
- `docs/decisions.md`: the amendments pointer is added; decision 6 is marked amended and
  additional requirement 11 superseded.
- `memory/`: a concise durable entry for future sessions.
- `spikes/RESULTS.md`: model evaluation results and the AI4Animation spike.

### Tests

| Level | What |
| --- | --- |
| Unit | Port selection, manifest read/write/atomic save, memory/CPU/NVIDIA/df parsers, catalogue scoring (RAM, disk, GPU, capabilities), cloud-name guard, pull-progress parser, prune protection. |
| Integration | Manual only: a mock Ollama HTTP server for `/api/version`, `/api/tags` and `/api/pull`; a daemon cloud-model `set` check; `systemd-analyze verify`; a user-local Ollama layout on 11435. These are not in `cargo test`. |
| Manual | Install alongside the system Ollama; stop/restart/kill the runtime; reboot/login; linger; interrupted pull; corrupted tarball; model change; disk-full. |
| Regression | `cargo test`; `tests/shell-sense/run.sh`; `buddy simulate`; renderer self-test. |
| Evaluation | `spikes/local-llm/ollama_eval.py` on the recommended model and one alternative. |

### Security and privacy

- Loopback only; no `0.0.0.0`.
- SHA-256 for the runtime tarball and model digests.
- File permissions under the Buddy data directory.
- No cloud models, no telemetry, no new outbound requests except the pinned downloads.
- The runtime's API has no authentication; treat the loopback boundary as the security
  boundary and harden the unit.

### Compatibility and migration

- Old SQLite settings keep working; new fields have defaults.
- An old `ask_claude` policy is ignored; no migration is needed if unknown skills are
  handled safely.
- Old journal entries remain readable; do not rewrite them.
- A user with a system Ollama can keep it; Buddy's runtime is separate. If they want
  Buddy to use the system one, that is an explicit future escape hatch, not the default.

## Rollout

1. Land Phase 1 alone; verify and commit only when the owner asks.
2. Land Phase 2 with the installer enabling the runtime by default, but the daemon must
   start and live without it. `--no-ai` installs Buddy without a runtime or model.
3. Land Phase 3 in the same flow; the default recommendation is data-driven and can be
   overridden.
4. Phase 4 only after a separate go decision.

## Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| CC BY-NC conflict | Commercial use restricted; decision 6 conflict | Optional until a consumer; licence gate; owner decision |
| Loss of agentic coding | No Claude Code hand-off | Explicit removal; separate future feature |
| 1.44 GiB runtime + 7 GB model | Slow install, disk pressure | Preflight, progress, resume, smaller model, prune |
| Network at install | Partial install | Retry/resume, SHA-256, side-load, clear recovery |
| User service before login | Runtime unavailable until login | Offer `enable-linger`; document the choice |
| VRAM contention with renderer | Dropped frames | CPU-first on small VRAM; max one loaded model; measure |
| Persian/tool quality | Poor local responses | Eval harness; verified-only catalogue defaults |
| Ollama has no auth | Exposure if bound wrongly | Loopback only; unit hardening; doctor warning |
| Runtime/model drift | Behaviour changes | Pin version and digest; explicit update; rollback |
| Stale hardware docs | Wrong recommendations | Update `docs/development.md`; probe at install |

## Definition of done

- `docs/decisions.md` reflects the approved amendments.
- Claude Code is gone from runtime code, CLI, skills, tests and docs.
- The managed runtime installs, starts, restarts, reports health and leaves the system
  Ollama untouched.
- Buddy starts and lives when the runtime is stopped, missing or broken.
- The first-run flow recommends a model with trade-offs, lets the user choose, pulls and
  verifies it, and persists the choice.
- The selected model passes the project's evaluation for the capabilities it claims.
- `buddy ai status`, `doctor`, `repair` and `providers` explain the state in plain words.
- AI4Animation is absent by default and cannot break Buddy's life; its decision is
  recorded.
- All tests pass; no unrelated features were added.

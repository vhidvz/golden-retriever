# Self-contained AI runtime — decision record

**Status: approved and implemented 2026-11-22.** The project owner approved D1–D11 and
the recommended answers to the open decisions. Phases 0–3 of the plan are implemented and
Phase 4 is a completed research spike; no commit or push has been made. Deferred items are
listed in `docs/self-contained-ai-plan.md`.

**Date:** 2026-11-22

**Related:** `docs/decisions.md` (binding decisions), `docs/development.md` (machine and
tools), `docs/self-contained-ai-plan.md` (implementation plan), `spikes/RESULTS.md`
(measurements).

## Context

The owner asked for Buddy to become self-contained in its AI capabilities:

- no dependence on a Claude Code subscription or on an externally managed Claude Code
  installation;
- Ollama as the sole AI runtime, installed and managed by Buddy;
- a post-install lifecycle for the required AI4Animation data;
- a first-run, hardware-aware model recommendation with the user keeping the final
  choice;
- a background runtime that is independent of Buddy's graphical interface.

The evidence behind this record is summarized in this section and expanded into the
verification gates in `docs/self-contained-ai-plan.md`. Its main findings:

- Buddy's runtime has **no consumer** for AI4Animation today. The only mention is
  `docs/decisions.md` decision 6, which records AI4Animation as optional and separate.
- Claude Code is a small, localised integration: one module (`claude.rs`), one skill, a
  CLI command, provider status, tests and documentation. Removing it removes the
  agentic-coding capability, not Buddy's life.
- Ollama is already the only local model provider (`ai.rs`), but it is an external
  prerequisite at `127.0.0.1:11434`. The official Linux tarball supports a user-local
  `bin/../lib/ollama` layout, so a no-root managed runtime is practical.
- The project has measured `gemma4:e4b` on CPU: 7.2 GB resident, 13.8 tok/s, 10/10
  English/Persian intent, 20/20 valid JSON, 5/5 tool calls, with vision and audio input
  working through Ollama. Smaller current models have not been evaluated by this project.

## Approval

Approved by the project owner on 2026-11-22. The owner's approval is recorded as
agreement with D1–D11 and with these answers to the open decisions:

- **AI4Animation:** D8 as written. It is not installed now; it becomes a required part of
  a motion feature only when that feature is approved. CC BY-NC 4.0 acceptance is
  required before any future download.
- **Lingering:** the installer offers `loginctl enable-linger`; the default is a user
  service that starts at login. The difference is explained, not hidden.
- **`ask_claude`:** removed; no local coding agent in this phase.
- **Model licences:** permissive licences by default; every offered model shows its
  licence.
- **Disk budget:** about 12 GB free for the default setup; about 6 GB for the
  small-model profile.
- **Platform:** Linux with systemd user services.
- **Commit/push:** not requested. None happens without a separate explicit request.

## Decisions

### D1 — Ollama is the sole AI runtime; Claude Code is removed

**Decision.** Buddy has one AI runtime and one model provider: the Ollama instance it
manages. The Claude Code integration is removed: `crates/buddy/src/claude.rs`, the
`ask_claude` skill, the `buddy ask-claude` command, the Claude provider status, the
Claude-specific tests and the corresponding documentation are deleted or updated.

**Why.** The owner's direction is to remove subscription-dependent execution,
configuration and operational assumptions. Keeping Claude Code as an optional helper
would keep those assumptions alive and contradict "Ollama as the sole runtime".

**Consequence.** Buddy loses the ability to hand a coding or analysis task to an agent
with read/edit tools. The other seven skills remained; a later workflow replay skill makes eight. A local coding agent is **not** part
of this change; if wanted, it is a separate feature with its own permissions, sandbox,
verification and undo.

### D2 — Ollama is a Buddy-managed, user-local runtime

**Decision.** Buddy installs and owns a pinned Ollama release under its data directory:

```text
$BUDDY_DATA_DIR/ollama/<version>/bin/ollama
$BUDDY_DATA_DIR/ollama/<version>/lib/ollama
$BUDDY_DATA_DIR/ollama/current -> <version>
$BUDDY_DATA_DIR/ollama/models
```

The official release tarball is downloaded, verified against the release's SHA-256, and
extracted atomically. The runtime is never installed into `/usr`, never uses the
distribution package as its primary source, and never modifies a separately installed
Ollama. If the runtime cannot be installed, AI is unavailable; Buddy still lives.

**Why.** A user-local runtime needs no root, gives Buddy a single owner, makes version
pinning and rollback possible, and keeps the system Ollama (and its models) untouched.
Ollama's own development documentation puts the native payload at `../lib/ollama`
relative to `bin/ollama`, so the layout works without `OLLAMA_LIBRARY_PATH`.

**Consequence.** The download is about 1.44 GiB for the current linux-amd64 release
(1,545,470,208 bytes for v0.40.17). Extracted runtime size must be measured in the
spike; budget 2–3 GiB as a placeholder. The installer must handle retry, resume,
integrity failure and insufficient disk space.

### D3 — A dedicated endpoint and separate model store

**Decision.** The managed runtime listens on `127.0.0.1:11435` by default. The port is
chosen once, persisted in the runtime manifest, and reused across restarts. If 11435 is
occupied, the installer probes the next free port in a bounded range and records it. The
model store is `$BUDDY_DATA_DIR/ollama/models`, separate from any system Ollama store.
The runtime binds only to loopback. Buddy never kills or reconfigures another Ollama.

**Why.** The brief requires a dedicated port and isolation from independently managed
Ollama instances. Ollama has no authentication in its normal local mode, so binding to
anything except loopback is unsafe.

**Consequence.** Models are duplicated if the user already has the same model in a system
Ollama; that is the price of isolation. `buddy ai doctor` must report the chosen port,
the store path, and any existing service on 11434 without touching it.

### D4 — The runtime starts with the user manager, not the Buddy UI

**Decision.** `buddy-ollama.service` is a systemd **user** unit:

- `WantedBy=default.target`
- `Restart=always`, `RestartSec=3`
- `Environment=OLLAMA_HOST=127.0.0.1:<chosen port>`
- `Environment=OLLAMA_MODELS=<Buddy data dir>/ollama/models`
- `Environment=OLLAMA_MAX_LOADED_MODELS=1`
- `Environment=OLLAMA_NUM_PARALLEL=1`
- `Environment=OLLAMA_KEEP_ALIVE=5m`

Buddy's own daemon remains a `graphical-session.target` service. The AI runtime is
independent of the renderer and of the daemon: hiding, restarting or crashing Buddy does
not stop Ollama.

**Why.** The brief asks for a background service that is available independently of the
graphical interface. A user unit under `default.target` starts with the user manager,
before and independently of the graphical session.

**Consequence.** By default a user service starts at login, not at boot. Starting before
login requires `loginctl enable-linger $USER` (which may need authorisation). The
installer offers it and explains the difference rather than silently requiring root. On
the current test machine lingering is already enabled.

### D5 — Hardware-aware, user-confirmed model selection

**Decision.** First run performs a best-effort hardware assessment:

- CPU model, cores, AVX2/AVX-512;
- total and available RAM;
- NVIDIA GPU name, VRAM, driver and compute capability;
- AMD GPU and ROCm, Intel/Vulkan as available;
- free disk space;
- any existing Ollama service or port on 11434.

It filters models by the capabilities Buddy actually needs, estimates memory, scores
fit, speed, quality and licence, and presents a default recommendation plus one or two
alternatives with their trade-offs. The user makes the final choice. The chosen model is
persisted and is pulled, verified and reported by Buddy. Cloud-only models (names with
`:cloud`) are excluded.

**Why.** Buddy's default senses are `system` and `desktop`; `screen`, `camera` and
`microphone` are off by default. The default model therefore needs text, tool calling,
JSON and English/Persian — not necessarily vision or audio. A fixed `gemma4:e4b` is a good
default on a 16 GB+ machine but not on an 8 GB machine.

**Consequence.** The model catalogue becomes project data that must be updated as Ollama
releases change. A recommendation is not a promise: the eval harness
(`spikes/local-llm/ollama_eval.py`) must validate every model added to the catalogue.
The user can change the model later with `buddy ai model`.

### D6 — Resource policy: fit first, CPU first on small GPUs

**Decision.** Defaults for Buddy's managed runtime:

- `OLLAMA_MAX_LOADED_MODELS=1`
- `OLLAMA_NUM_PARALLEL=1`
- `OLLAMA_KEEP_ALIVE=5m`; on machines with limited RAM, 0 or 1m
- explicit `num_ctx` of 8192 or 16384 in Buddy's requests
- prefer CPU inference when VRAM is small (for example 4 GB) or when the desktop/renderer
  already uses much of it; allow GPU when the card has headroom

**Why.** Buddy's model calls are short and intermittent. A model that fits in system RAM
but competes with the renderer for VRAM can cost more in dropped frames than it gains in
latency. Ollama's default context can be sized from VRAM (4k, 32k or 256k), so Buddy must
set a small, explicit context instead of inheriting a large one.

**Consequence.** Cold loads can take seconds (8.2 s measured for `gemma4:e4b`). The
runtime should report placement (`/api/ps`: CPU, GPU or split) and expose it in
`buddy providers`. "Stay loaded" is an opt-in for machines with headroom, not a default.

### D7 — AI failure never blocks Buddy's life

**Decision.** If the runtime is missing, stopped, unhealthy or the model is absent:

- the daemon still starts and keeps simulating;
- AI-dependent features report "unavailable" in plain words;
- `buddy ai status` and `buddy providers` explain what is missing;
- systemd restarts the runtime; Buddy may request a bounded restart with backoff;
- Buddy never starts a second `ollama serve` of its own.

A "setup incomplete" state may disable AI features. It must not disable the dog.

**Why.** The project's first invariant is local-first: his life never depends on a model
or a network. The brief's "fully operational" requirement is satisfied by defining it per
feature, not by blocking the daemon.

**Consequence.** Installation can finish with AI incomplete and be repaired later. Tests
must cover the absence of the runtime, a stopped runtime and a missing model.

### D8 — AI4Animation: required for the motion feature, never for the dog

**Decision.** No AI4Animation asset is a dependency of Buddy's autonomous life. The
candidate subset is the MANN dog BVH archive (51 BVH clips, 31.9 MiB compressed,
83.2 MB extracted) or the AI4AnimationPy dog assets if the spike chooses the NPZ format;
no other AI4Animation datasets are in scope.

Because no Buddy code consumes this data today, mandatory acquisition is **deferred**
until a consuming motion feature is approved. When that feature is approved:

- the data becomes a required component **of that feature**, not of the daemon;
- Buddy's installer acquires, verifies, versions and reports readiness for it;
- the feature does not report ready without valid data;
- the data is never bundled in the MIT repository;
- the user must accept the licence before download;
- a side-load path exists for offline installation.

**Why.** The assessment found no loader, retargeting pipeline or renderer integration for
BVH/NPZ. Making the data mandatory now would add a dead dependency and a licence conflict
without a consumer. The MANN skeleton has 21 joints; Buddy has 25 bones, so a retargeting
pipeline is new work, not an installer flag.

**Consequence.** This **amends** `docs/decisions.md` decision 6: the data remains optional
and separate, never bundled, and never a hard dependency of the daemon. If the owner
wants the data mandatory immediately, the decision must say so explicitly and accept the
CC BY-NC 4.0 consequences; even then, "mandatory" should mean "the motion feature is not
ready", not "Buddy cannot live".

### D9 — Licensing and privacy

**Decision.** Buddy's core stays MIT. Downloads are verified by SHA-256. Model licences
are shown in the recommendation; permissive licences are the default. Cloud models are
excluded. No telemetry is added. Ollama binds to loopback. The AI runtime's files are
readable only by the user. CC BY-NC 4.0 material is never bundled, and its users are told
that it is for non-commercial use.

**Why.** The project's standing rule is "no paid assets or services" and open licences.
The AI4Animation repository states that it is "only for research or education purposes,
and not freely available for commercial use or redistribution"; AI4AnimationPy is
CC BY-NC 4.0. Treating that as a hard dependency of an MIT product would change the
product's licensing posture.

**Consequence.** The installer needs a licence-acceptance step for non-permissive data.
Model catalogue entries carry a licence field. The project should seek legal advice before
shipping any configuration that makes NC data mandatory.

### D10 — Supported platform: Linux with systemd user services

**Decision.** This phase targets Linux with systemd user services, the platform Buddy
already supports. Windows and macOS are out of scope. The installer remains user-local
and root-free.

**Why.** The current service, installation script and renderer assumptions are Linux.
Designing for other operating systems now would add branches without a supported product.

**Consequence.** The decision record and plan must say "Linux" rather than "every OS".
A future port must revisit the service manager, paths and GPU detection.

### D11 — Observability and documentation are part of the feature

**Decision.** The following commands and records are part of the deliverable:

- `buddy ai status` — runtime version, endpoint, health, model, placement;
- `buddy ai setup` — first-run hardware assessment, recommendation, pull;
- `buddy ai model` — inspect or change the selected model;
- `buddy ai doctor` — port, store, service, system Ollama, disk, model digest;
- `buddy ai repair` — re-pull, re-verify, restart;
- `buddy ai update` — explicit runtime update with rollback (deferred; rerun
  `scripts/install.sh` with `OLLAMA_VERSION` for now);
- `buddy ai prune` — remove unused models after checking the running daemon; old
  runtime versions are kept for rollback;
- `buddy providers` — one line for the runtime and one for the model, no Claude Code.

`README.md`, `docs/development.md`, `docs/decisions.md` and `memory/` are updated in the
same change that alters behaviour.

**Why.** A managed runtime that cannot explain its state is worse than an external one.
The project's diagnostic habit (`buddy why`, `buddy senses`, `buddy providers`) applies.

**Consequence.** Each command needs a test or a documented manual check. Status output
must distinguish "not installed", "installed but stopped", "running but model missing",
"model loading" and "ready".

## Relationship to existing decisions

| Existing decision | Effect of this record |
| --- | --- |
| `docs/decisions.md` 1 (no money) | Retained. The runtime and models are free; no paid service is introduced. |
| 2 (no human voice) | Retained. No TTS is added. |
| 3 (English and Persian) | Retained. Model evaluation must test both; Persian quality is a selection criterion. |
| 4 (camera/microphone staged) | Retained. Vision and audio models remain optional capabilities. |
| 5 (staging and approval) | Retained. This record and the plan are the gate; no code before approval. |
| **6 (non-commercial motion data optional/separate)** | **Amended by D8.** It remains optional and separate; it becomes required only for a motion feature that consumes it, never for the daemon. |
| 9 (`gemma4:e4b` first evaluation) | Retained and extended by D5/D6. |
| 10 (recommend other models) | Retained; D5 makes it a first-run flow. |
| **Additional requirement 11 (Ollama and Claude Code are peers)** | **Superseded by D1.** Ollama is the sole runtime; Claude Code is removed. |
| 12 (priority rule) | Retained. Existing approved work is finished first. |

## Alternatives considered

| Decision | Alternative | Why not |
| --- | --- | --- |
| Runtime ownership | Use the system Ollama or distro package | Requires root or shares a port/store; cannot guarantee version, isolation or lifecycle. |
| Runtime packaging | Container | Extra runtime, GPU passthrough and complexity for no gain on a user desktop. |
| Claude Code | Keep it as an optional helper | Keeps subscription assumptions, configuration and UI; contradicts "sole runtime". |
| Claude Code | Replace it now with a local coding agent | New, larger feature with its own permissions and verification; out of scope. |
| Model choice | Fixed `gemma4:e4b` | Fails on 8 GB machines; ignores GPU/VRAM and capability needs. |
| Model choice | Automatic choice without asking | Violates the owner's requirement that the user keeps the final choice. |
| AI4Animation | Mandatory now | No consumer, CC BY-NC conflict, new retargeting pipeline; would add a dead dependency. |
| AI4Animation | Bundle in the repository | Violates decision 6 and the upstream non-commercial/redistribution terms. |

## Open decisions (answered at approval)

The answers are recorded in Approval above and summarized here.

1. **AI4Animation consumer.** No consumer exists today. D8 keeps it deferred; it becomes
   required for a motion feature only when that feature is approved.
2. **CC BY-NC acceptance.** Accepted in principle for a future motion feature, before any
   download. No data is installed now, and the core stays MIT.
3. **Lingering.** The installer offers `loginctl enable-linger`; the default starts at
   login.
4. **`ask_claude` replacement.** Removed; no local coding agent in this phase.
5. **Model licence policy.** Permissive by default; the licence is shown for every model.
6. **Disk budget.** About 12 GB for the default setup; about 6 GB for the small-model
   profile.

## Consequences of approval

- Phase 1 removed Claude Code and reduced the skills from eight to seven.
- Phase 2 added a managed runtime, service unit, manifest, health checks and diagnostics.
- Phase 3 added hardware detection, a model catalogue, recommendation and verified pulls.
- Phase 4 is a completed research spike; integration remains conditional on a consumer
  and licence approval.
- `README.md`, `docs/development.md`, `docs/decisions.md` and `memory/` are updated.
- No commit or push happens without a separate explicit request.

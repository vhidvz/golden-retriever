# Spike results

Measured on 2026-11-12 on the primary test machine: Manjaro, GNOME Shell 50.5
(Wayland), XWayland 24.1, GeForce GT 1030 2 GB (driver 580.178.04), i5-11400F,
32 GB RAM, one 3440×1440 monitor at 100 Hz and scale 1.0.

## Overlay on GNOME (`overlay-gnome/`)

An isolated headless gnome-shell drives three overlay windows with a virtual
pointer over an ordinary window.

| Behaviour | X11, managed | X11, override-redirect | Wayland + shell extension |
|---|---|---|---|
| Above other windows, all workspaces, out of the taskbar | yes | yes | yes |
| Moved 50 times a second | 60/60 exact | not tested | 60/60 exact |
| Per-pixel transparency | yes | yes | yes |
| Click beside the dog reaches the window underneath | yes | yes | yes |
| Click on the dog reaches the dog | yes | yes | **no** |
| Keyboard focus stays where it was | yes | yes | **no** |
| Stays above a focused fullscreen window | yes | yes | yes |

Decision: X11 through XWayland, no extension needed for the overlay itself.
Godot's unfocusable window is override-redirect, which is the middle column.
Why the Wayland variant loses the click was not investigated.

On the real session a transparent GL window was confirmed by eye. Not yet
checked there: clicks, the overview, the lock screen, and the tiling and panel
extensions. Because he stays above fullscreen windows, he will need to hide
himself; detecting fullscreen needs a small shell extension.

The headless shell does not present GL from X11 clients on this driver (not
even opaque ones), so GL appearance cannot be tested there.

## Fur rendering (`overlay-gnome/furtest.c`, and the real renderer)

Raw OpenGL stand-in, about 600 px tall, 182k opaque pixels, back faces culled:

| Shells | 4x MSAA + alpha-to-coverage | No MSAA |
|---|---|---|
| 16 | 3.0 ms (19% GPU at 50 fps) | 1.2 ms (10%) |
| 24 | 4.5 ms (27%) | — |
| 32 | 5.8 ms (36%) | 2.2 ms (17%) |
| 48 | 8.1 ms (41%) | — |

At rest with 16 shells: 6% GPU at 12 fps, 2% at 4 fps. CPU 1–4% of one core.

The Godot renderer (20 shells, 4x MSAA, Compatibility renderer, adult at 420 px)
measures 2.7–4.5 ms GPU per frame depending on pose, 3–5% of one core, 227 MB RSS.

VRAM is the scarce resource: the desktop alone went from 888 MB to 1506 MB of
2048 MB when a virtual machine started.

## Local language model (`local-llm/`)

llama.cpp b12317 (CPU build), Qwen3.5-2B Q4_K_M:

| Threads | Prompt | Generation |
|---|---|---|
| 4 | 221 tok/s | 24.3 tok/s |
| 5 | 249 tok/s | 25.8 tok/s |
| 6 | 299 tok/s | 26.3 tok/s |

The same build with Qwen3.5-4B Q4_K_M generates 13.4 tok/s on 5 threads and 13.8 on 6
(prompt 113 and 132 tok/s).

Intent routing with schema-constrained output, 10 utterances per language:

| Model | English | Persian | Valid JSON | Median per utterance | Resident memory |
|---|---|---|---|---|---|
| Qwen3.5-2B | 9/10 | 9/10 | 20/20 | 1.35 s | 2.05 GB |
| Qwen3.5-4B | 9/10 | 10/10 | 20/20 | 2.46 s | 4.54 GB |

The 2B model read "lie down" as "sleep" in both languages; the 4B model's one
miss was an open question classed as unknown. Both load in under 2 s from the
page cache.

### `gemma4:e4b` through Ollama 0.35.1

Ollama lists its capabilities as completion, vision, audio, tools and thinking
(7.5B parameters, Q4_K_M, Apache-2.0). Measured with thinking off:

| What | Result |
|---|---|
| Where it runs | 100% CPU; no VRAM used, so it does not compete with the renderer |
| Memory | 7.2 GB resident |
| Load | 8.2 s cold |
| Speed | 13.8 tok/s generation, 76 tok/s prompt |
| Intent routing with a JSON schema | English 10/10, Persian 10/10, valid JSON 20/20, median 2.1 s |
| Tool calling | 5/5: the right tool with sensible arguments in both languages, and no call for "Good boy" |
| Image input | Read "4721" and "red" off a synthetic image in 2.9 s (134 prompt tokens) |
| Audio input | Works through the OpenAI-compatible endpoint (`input_audio`, WAV): a 9 s English clip was transcribed word for word in 3.5 s, and a question about the speaker answered |
| Persian reply | Natural, correct, friendly, 11 tok/s |

Not tested: Persian speech (no sample or microphone), long sessions, and
streaming latency for spoken commands. `spikes/local-llm/ollama_eval.py`
reproduces everything except the audio test.

## Claude Code (`claude-code/`) — historical

Claude Code was removed from Buddy on 2026-11-22; these measurements are kept for the
record and are not part of the current runtime.

With the installed CLI 2.1.322 and a subscription login, from a clean environment:

- Schema-constrained output for a Persian utterance: correct, about 4 s.
- Image input through the Read tool, and a session resumed by a second process: work.
- `stream-json` emits a `rate_limit_event` with status, utilisation and `resetsAt`.
- Network down: exit 1 and `is_error: true`, but `subtype` is still `success`.
- An `ANTHROPIC_API_KEY` in the environment switches auth to the key; `claude auth status` shows it.
- stdin must be closed or each run waits 3 s.

## Long-run behaviour (`buddy simulate`)

Ninety simulated days each, with a stand-in user who pets, praises, feeds and
waters on a routine. These are the Phase 2 exit measurements.

| Scenario | Activities used | Next-activity uncertainty | Longest loop | Done exactly as the time before | Sleep per day | Bids for attention per day |
|---|---|---|---|---|---|---|
| Adult, someone around all day | 19 of 19 | 2.93 bits | 3.0 rounds of a 3-cycle | 1.3% | 10.9 h | 2.4 |
| Puppy from 20 days old | 19 of 19 | 2.89 bits | 3.5 rounds of a 2-cycle | 1.3% | 13.3 h | 2.7 |
| Adult, computer off overnight | 19 of 19 | 2.92 bits | 4.3 rounds of a 3-cycle | 1.2% | 11.6 h of the time it is on | 0.9 |
| Adult, nobody ever interacts | 15 of 19 | 2.78 bits | 3.0 rounds of a 2-cycle | 1.2% | 10.8 h | 3.0 |

"Uncertainty" is how many bits it takes to guess his next activity from his
current one: 0 would be a fixed routine. A "loop" is the same short cycle of
activities repeated back to back. In the unattended scenario his mood never
fell below -0.08 on a scale of -1 to 1, and he made at most one loud sound in
his busiest hour. He looks up at the viewer about 40 times per waking hour,
which adds up to 2.3% of his waking time.

## Managed Ollama and model selection (self-contained AI, measured 2026-11-22)

- A user-local Ollama layout (`bin/ollama` with `../lib/ollama`) was run from a temporary
  directory with `OLLAMA_HOST=127.0.0.1:11435` and a separate models directory on Ollama
  0.40.16. `/api/version` answered; the system Ollama on 11434 stayed active and was not
  touched.
- `buddy ai status` and `buddy ai doctor` reported the manifest, endpoint, version,
  service, store, disk, system Ollama and selected model correctly against that runtime.
- `buddy ai setup --yes` was exercised against a mock `/api/pull` stream on the same
  endpoint: it detected the machine, chose `gemma4:e4b`, parsed progress, verified the
  digest, wrote `runtime.json`, and selected the model through a running daemon.
- The 1.44 GiB official Ollama tarball download was **not** run end-to-end; the installer
  is syntax-checked and its layout and checksum parsing are verified separately.
- Only `gemma4:e4b` has project measurements. The other catalogue entries are marked
  unevaluated.

## Skills and helpers (Phase 5, measured 2026-11-14)

- A request placed by vocabulary alone ("find my tax report", «گزارش مالیات رو پیدا کن»)
  is answered at once, with no model running.
- A request only the local model could place ("Could you put down for me that I
  must call the dentist tomorrow?", and its Persian equivalent) became a
  `make_note` proposal with a sensible argument; two model calls, a few seconds.
- A read-only hand-off to Claude Code ("Is there a bug in geometry.py?") came back
  in 10 s with the right answer, left the file untouched, and reported 21% of the
  five-hour allowance used. The same request for a folder he had not been given
  was refused before Claude Code was started. (Historical: the integration was
  removed on 2026-11-22.)
- With a habit seeded (Terminal, then code, then Calculator), bringing Terminal and
  then code to the front made him invite within seconds; "تو این کار کمکم نکن"
  ended it and marked the habit never to be offered again.

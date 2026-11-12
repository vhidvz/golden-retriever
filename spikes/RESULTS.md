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

Intent routing with schema-constrained output, 10 utterances per language:
English 9/10, Persian 9/10, valid JSON 20/20, median 1.35 s per utterance,
2.05 GB resident, 1.3 s to load from the page cache. Both misses were "lie down"
read as "sleep". The 4B model was not benchmarked (download incomplete).

## Claude Code (`claude-code/`)

With the installed CLI 2.1.322 and a subscription login, from a clean environment:

- Schema-constrained output for a Persian utterance: correct, about 4 s.
- Image input through the Read tool, and a session resumed by a second process: work.
- `stream-json` emits a `rate_limit_event` with status, utilisation and `resetsAt`.
- Network down: exit 1 and `is_error: true`, but `subtype` is still `success`.
- An `ANTHROPIC_API_KEY` in the environment switches auth to the key; `claude auth status` shows it.
- stdin must be closed or each run waits 3 s.

# Buddy

A Golden Retriever who lives on a Linux desktop. He is born as a puppy, grows
with real time, and keeps living when every AI service is unavailable.

This repository is at **Phase 1 (foundation)**: he exists, persists, is drawn
as an overlay and can be controlled. He does not yet perceive anything, and his
behaviour is a simple placeholder.

## What is here

| Part | Where | Role |
|---|---|---|
| Brain | `crates/buddy-core` | Identity, age and experience, locomotion, persistence (SQLite). No graphics, no models. |
| Daemon and CLI | `crates/buddy` | Runs the brain, serves a Unix socket, supervises the renderer. |
| Renderer | `renderer/` | Godot 4 project: a transparent, click-through overlay window with a fur-covered placeholder dog. |
| Service | `dist/buddy.service`, `scripts/install.sh` | Starts him with the graphical session, as the user, without root. |
| Evidence | `spikes/` | The measurements the design rests on, with the harnesses that produced them. |

The daemon owns everything about him. The renderer only draws the state it is
sent, so it can crash, be hidden or be replaced without him noticing.

## Run it

Requirements: Rust, Godot 4.6 or newer on `PATH` (or `BUDDY_GODOT`), an X11 or
XWayland display. Developed on Manjaro with GNOME 50.

```sh
cargo test                      # brain and daemon tests
cargo run -- daemon             # run from the checkout
scripts/install.sh              # install for this user and start with the session
scripts/install.sh --remove     # stop and remove (his memory is kept)
```

```sh
buddy status            # how he is, what is running, what is being observed
buddy hide [MINUTES]    # stops the renderer entirely; `buddy show` brings him back
buddy sleep | wake
buddy privacy on|off    # suspends every perception source (none exist yet)
buddy set adult_days 30 # re-time growth without resetting him
buddy journal | export  # what happened; everything he knows, as JSON
```

On the dog: click to pet, drag to move, right-click for a menu.

His memory lives in `~/.local/share/buddy/buddy.db`, with a daily backup beside
it that is restored automatically if the database is ever damaged.

## Ground rules

- **Local first.** His life never depends on a language model or a network.
- **No paid assets or services.** Only free, openly licensed material. Anything
  under a non-commercial licence is offered as a separate optional download and
  never bundled here.
- **He is a dog.** He barks, whines, wags and looks; he never speaks with a
  human voice.
- **English and Persian from the start.** Nothing language-specific is hard-coded.
- **Camera and microphone are optional.** Every perception source has an
  "unavailable" state and its own permission.

## Replacing the dog

`renderer/dog.gd` is a stand-in built from ellipsoids. A real model replaces it
by providing the same methods the renderer calls: `apply_growth`,
`set_activity`, `set_facing`, `set_excitement`, `advance` and `get_reach`.

## Known gaps in Phase 1

- He stays above fullscreen windows, and his behaviour over the overview and
  lock screen is unverified; hiding in those cases needs shell signals (Phase 3).
- Petting, dragging and the right-click menu are untested with a real pointer.
- Suspend is handled by time accounting only; there is no lock or sleep signal yet.
- The renderer needs Godot installed; it is not yet exported as a standalone binary.

## Roadmap

1. Foundation — this phase.
2. Autonomous life: needs, emotions, behaviour selection, procedural animation, dog sounds, long-run simulation.
3. Perception: system signals, screen understanding, camera and microphone providers.
4. Language and training in English and Persian.
5. Skills and optional Claude Code delegation, with local fallback.
6. Refinement: fur and model quality, the puppy shape, KDE.

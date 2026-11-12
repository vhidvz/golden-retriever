# Buddy

A Golden Retriever who lives on a Linux desktop. He is born as a puppy, grows
with real time, and keeps living when every AI service is unavailable.

This repository is at **Phase 2 (autonomous life)**: he has needs, moods and a
temperament of his own, chooses what to do from them, and expresses it in
movement and sound. He does not yet perceive anything, and the dog you see is a
stand-in model.

## What is here

| Part | Where | Role |
|---|---|---|
| Brain | `crates/buddy-core` | Identity, age, needs, mood, temperament, behaviour selection, memory (SQLite), and a long-run simulator. No graphics, no models. |
| Daemon and CLI | `crates/buddy` | Runs the brain, serves a Unix socket, supervises the renderer. |
| Renderer | `renderer/` | Godot 4 project: a transparent, click-through overlay window, a fur-covered placeholder dog animated procedurally, and his recorded sounds. |
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
buddy why               # what he is doing, why, how he feels, what else he considered
buddy sleep | wake
buddy feed | water | praise
buddy quiet on|off      # no sounds and no asking for attention
buddy privacy on|off    # suspends every perception source (none exist yet)
buddy set adult_days 30 # re-time growth without resetting him
buddy journal | export  # what happened; everything he knows, as JSON
buddy simulate 90 --user absent   # live 90 days offline and report how varied they were
```

On the dog: click to pet, drag to move, right-click for a menu.

## How he decides

Seven needs (fatigue, hunger, thirst, boredom, company, itch, stiffness) drift
with time and with what he does. A mood of two numbers, how pleasant and how
activated, follows from them and from what happens to him. Nine temperament
traits, set at birth and shifting slowly with how he is treated, weight
everything.

Each of his 19 activities scores itself against that state. Scores are damped
by how recently he did the same thing, and the choice is drawn in proportion to
them, so the most fitting thing usually happens without anything being certain.
Every instance gets its own variant, duration, vigour and random seed, which
the renderer turns into a pose, quick motions on top, and a mood layer (tail,
ears, mouth, eyes). Nothing is a recorded clip.

Unmet needs lower his mood only mildly, an absence leaves him rested and glad
to see you, and an unanswered bid for attention makes him wait twice as long
before the next. He is never used to pressure the user.

Now and then he looks up at whoever is in front of the screen. Without a
camera that is taken to be the middle of the monitor, a little above him.

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

## Known gaps

- The dog is a placeholder built from ellipsoids, not yet a realistic model.
- Only barks and a grunt have recordings; whines, yawns, sniffs and the rest are
  silent until freely licensed recordings are added (see `renderer/sounds/SOURCES.md`).
- No toys yet, so no fetch.
- He stays above fullscreen windows, and his behaviour over the overview and
  lock screen is unverified; hiding in those cases needs shell signals (Phase 3).
- Petting, dragging and the right-click menu are untested with a real pointer.
- Suspend is handled by time accounting only; there is no lock or sleep signal yet.
- The renderer needs Godot installed; it is not yet exported as a standalone binary.

## Roadmap

1. Foundation — done.
2. Autonomous life — this phase.
3. Perception: system signals, screen understanding, camera and microphone
   providers, and noticing recurring patterns in how the computer is used.
4. Language and training in English and Persian, with feedback he learns from
   (correction, approval, "remember this", praise and treats).
5. Skills and learned workflows: local models through Ollama and optional
   Claude Code delegation as peer providers, a playful invitation when he
   spots something he could take over, verification and undo.
6. Refinement: fur and model quality, the puppy shape, KDE.

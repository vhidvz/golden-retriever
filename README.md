# Buddy

A Golden Retriever who lives on a Linux desktop. He is born as a puppy, grows
with real time, and keeps living when every AI service is unavailable.

This repository is at **Phase 4 (language and training)**: on top of a life of
his own and senses that each have their own switch, he now understands what is
said to him in English and Persian, and learns what words mean the way a dog
does. The dog you see is still a stand-in model.

## What is here

| Part | Where | Role |
|---|---|---|
| Brain | `crates/buddy-core` | Identity, age, needs, mood, temperament, behaviour selection, memory (SQLite), and a long-run simulator. No graphics, no models. |
| Daemon and CLI | `crates/buddy` | Runs the brain, serves a Unix socket, supervises the renderer. |
| Renderer | `renderer/` | Godot 4 project: a transparent, click-through overlay window, a fur-covered placeholder dog animated procedurally, and his recorded sounds. |
| Senses | `shell-extension/`, `perception/`, `crates/buddy/src/system.rs` | What he perceives: a GNOME Shell extension for the desktop, small helpers for screen, camera and microphone, and a poller for idle time, lock and load. |
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
buddy feed | water | praise | treat
buddy say "بادی بشین"    # say something to him, in English or Persian
buddy teach "bang" roll_over   # tell him outright what a word means
buddy knows             # the words he knows, how well, and how he came by them
buddy unlearn "bang"
buddy memories          # what he was told to remember; `buddy forget N` removes one
buddy quiet on|off      # no sounds and no asking for attention
buddy privacy on|off    # suspends every sense at once
buddy senses            # what he may perceive, and what is actually reporting
buddy sense screen on   # switch one sense: system, desktop, window_titles, screen, camera, microphone
buddy look              # have him look at the screen once and say what he makes of it
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

## What he perceives

| Sense | Default | What it tells him | How |
|---|---|---|---|
| `system` | on | Seconds since you touched the computer, screen lock, system load | Session bus and `/proc`, polled every 2 s |
| `desktop` | on | Which application is in front, fullscreen, overview, pointer position, that a notification arrived | A GNOME Shell extension |
| `window_titles` | off | Window titles as well | The same extension |
| `screen` | off | A one-line description of what is on screen, when asked to look | One frame through the desktop portal, described by a local model |
| `camera` | off | Whether someone is there | Movement in a 64×48 greyscale picture; no frame is kept |
| `microphone` | off | That there was a sound, and how loud | Loudness only; nothing is recorded |

A sense that is off is not consulted at all: the extension is told to send
nothing, and the camera and microphone helpers are not running. Privacy mode
does the same for all of them at once. `buddy senses` shows what is allowed and
what has actually reported; the journal records when each sense first became
active and each look at the screen. A look keeps only its one-line summary, in
memory; the frame is deleted as soon as the model has seen it, and nothing
leaves the machine.

What he does with it: he greets you when you come back, steps aside for the
lock screen, the overview and fullscreen applications, glances up at a
notification, watches the pointer when it comes near, stays calmer and does
not ask for attention while you are busy, and takes less of the machine when
it is working hard. He also keeps count of which applications follow which,
and notes a sequence as a habit of yours once it has recurred on different
days (`buddy why` lists them). Acting on those habits is a later phase.

## Talking to him and training him

He never answers in words. What you say changes what he does.

**Understanding** happens in three steps, each optional beyond the first:

1. His own vocabulary: `crates/buddy-core/data/lexicon.json` lists every phrase
   he knows, per language, and what the user has taught him is checked before it.
   Text is normalised first, so Persian letter variants, the half-space and
   punctuation make no difference. This needs no model.
2. A local language model through Ollama (`gemma4:e4b` unless changed with
   `buddy set language_model …`), for what the vocabulary cannot place, such as
   "I really don't want you to do that anymore".
3. If neither can, he looks at you and tilts his head.

**Training.** Each word has a strength for each trick. Hearing a word, he does
the strongest trick with a probability that grows with its strength and falls
when he is tired or over-excited, so an untrained dog obeys only sometimes.
What follows within twenty seconds teaches him:

| You | Effect |
|---|---|
| "Good boy" / «آفرین», a treat, petting | Strengthens the word-to-trick pairing he just acted on |
| "That's right" / «درسته» | Strengthens it a little |
| "No, that's wrong" / «نه، اشتباهه» | Weakens that pairing, and he stops |
| "Stop that" / «بسه» | He stops and will not start that activity again for half an hour |
| "When I say X, do Y" / «وقتی می‌گم X، یعنی Y» | Sets the pairing outright |
| A new word, then praise while he happens to do something | He comes to pair the word with that |
| "Remember that …" / «یادت باشه …» | Kept word for word in his memory |

`buddy knows` shows every pairing with its strength, whether it is instinct,
told or learned, and how often it was rewarded and corrected; `buddy unlearn`
removes one. What he was told to remember is listed by `buddy memories`. The
journal records what kind of thing was said to him, never the words: talk does
not become memory unless he is told to remember it. One thing he is told
already changes how he lives: say you work late and his night starts two hours later.

**By voice.** With the microphone sense on, stretches of speech are written
down by the local model on this machine and the recording is deleted at once.
He acts on them only if his name is in them, or within twenty seconds of being
spoken to by name. This path has been tested with a recording, not with a
microphone, and not yet in Persian.

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
- The desktop sense needs GNOME; on other desktops he still has the system sense.
- Looking at the screen is on request only (`buddy look`), sees the whole monitor
  at reduced size, and has no OCR, so small text is beyond it.
- The camera and microphone senses detect presence and loudness only, and have
  been tested with synthetic input, not real devices.
- Habits are noticed at the level of application order, not of what is done in them.
- Speech has been tested with a recording only; there is no wake word, so with the
  microphone on every stretch of speech goes through the local model.
- "Fetch" is not among his tricks yet, because there is no ball.
- Suspend is handled by time accounting only; there is no lock or sleep signal yet.
- The renderer needs Godot installed; it is not yet exported as a standalone binary.

## Roadmap

1. Foundation — done.
2. Autonomous life — done.
3. Perception — done.
4. Language and training — this phase.
5. Skills and learned workflows: local models through Ollama and optional
   Claude Code delegation as peer providers, a playful invitation when he
   spots something he could take over, verification and undo.
6. Refinement: fur and model quality, the puppy shape, KDE.

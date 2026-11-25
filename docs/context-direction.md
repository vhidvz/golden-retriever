# Context and companionship direction

**Status:** implemented on the safe path 2026-11-25. The pointer/click foundation landed
2026-11-23; the semantic `context` sense, bounded `glances` sense and warmth behaviour are
now implemented and measured. The owner wants Buddy to observe more of the desktop so he
can be a better helper and feel more like a warm family dog.

## What exists today

- Focused application, fullscreen, overview and pointer position from the GNOME Shell
  extension. Pointer position is already used for his gaze.
- Window titles, only when their own sense is on.
- One-shot screen looks and one-shot camera looks through the local vision model.
- The opt-in `context` sense (semantic text changes, debounced, summarized in memory)
  and the opt-in `glances` sense (bounded local screen summaries, requires `screen`).
- Microphone speech through the local audio model, on demand.
- Accessibility: focused controls, accessible actions, text fields and workflows.
- Journal entries are summaries of events, not raw screen pixels or typed text.

## The requested direction

### Pointer and clicks

Movement already reaches him. A click event now carries the current application, the
button and a coarse screen region, not a permanent coordinate history or any content.
This is enough for reactions such as looking up, wagging or offering help without turning
Buddy into a detailed activity logger. The headless GNOME Shell harness verifies the
click path and its suppression in privacy mode.

### Screen

The screen is observed on request (`buddy look`), and by the separate, opt-in `glances`
sense: at most one portal frame every twenty minutes while the user is active, skipped
for lock/fullscreen/overview, summarized locally by the vision model and kept as one line
for five minutes. The desktop's own screen-sharing indicator is visible for the moment
of each look. `glances` requires the `screen` sense as well as its own switch. Continuous
recording and a hidden always-on screenshot loop are not implemented and are not
proposed.

### Keyboard and typed text

**Raw global keystrokes are not the path.** They would capture passwords, terminals,
consent dialogs and private messages, and a dog holding a keylogger is a poor trade for a
little context. The implemented `context` sense observes **semantic text changes in the
focused accessible control** through AT-SPI:

- off by default and separate from `accessibility` and every other sense;
- `perception/a11y_context.py` listens only to `object:text-changed`, filters on the
  FOCUSED editable text object, debounces a burst (1.2 s), and checks both application
  and frame title;
- refused for password fields, terminals, shells, password managers, login/lock/consent
  surfaces and portal surfaces, and the daemon checks the application again;
- only an application name, a fixed category (`prose`, `code`, `search`, `url`, `email`,
  `form`), word count and character count leave the helper;
- the daemon keeps one in-memory note for two minutes, never journals or exports it, and
  `buddy context clear` erases it immediately; turning the sense or privacy mode off
  erases it too;
- privacy mode kills the helper; `buddy senses`, `buddy context`, `buddy status` and the
  control panel show what is active and what summary is being kept.

The spike in `spikes/context-a11y/RESULTS.md` measured the event names, the duplicate
text-object problem, capture at 10–250 ms spacing and listener CPU (0.0–0.01 % of a core
idle) in `gnome-text-editor`. If the owner later wants raw global input, it must be a
separate high-risk decision with its own consent flow, exclusions and audit trail. It is
not implied by this phase.

### Family-dog warmth

The point of the extra context is not surveillance; it is better timing. With more
context Buddy should:

- notice routines and greet at the right moment rather than constantly;
- sit nearby, look up, rest his head or offer quiet company when the user is working;
- recognise when the user is busy, in a call or away, and be calm instead of demanding;
- offer help with a familiar next step, while keeping every action permissioned;
- remember **summaries** of what matters, not a transcript of the day.

The existing dog behaviour and sounds remain the interface. More observation must not
turn him into a chatbot or a notification stream. In the simulation, a typing report
makes him settle quietly (`Sit` gains "keeping you company while you type"), suppresses
attention-seeking and play invitations, and when the burst ends he looks up once. A
screen glance makes him look up and settles boredom mildly. A known routine earns at
most one greeting every 45 minutes when its application comes to the front; he does not
greet on every focus event, and never in quiet mode, fullscreen/overview, while away or
while the user is typing.

## Required gates

1. A new `context`/`input` permission, off by default, separate from `desktop`,
   `accessibility`, `screen`, `camera` and `microphone`.
2. Privacy mode suspends all of it, and a visible state shows when it is active.
3. Password fields, terminals, shells, login/lock/consent surfaces, clipboard and raw
   keycodes are out of scope for the first implementation.
4. Local-only processing; no cloud model, no analytics, no background upload.
5. Bounded retention with an explicit summary and a way to clear it.
6. A spike must measure false positives, capture gaps, battery/CPU cost and the exact
   semantic events available in real applications before implementation.

## Decisions taken

- Semantic text observation is useful enough without raw keys for the warmth and timing
  behaviour; the context note is counts plus a category, not a transcript.
- Scheduled screen glances are worth their cost only as an explicit opt-in with a
  five-minute first delay, twenty-minute interval, local model and visible portal
  indicator. They are off by default.
- Click granularity stays application plus coarse region; no precise history.
- Retention is a short rolling window in memory: two minutes for typing, five minutes
  for a glance, clearable at any time. Nothing is persisted.
- A separate raw-input mode is not adopted; it remains a high-risk decision of its own.

This phase is implemented in `README.md` Phase 10, `docs/decisions.md`,
`perception/a11y_context.py` and `crates/buddy/src/daemon.rs`. Phase 6 (body/fur), Phase
7 (automation), Phase 8 (photoreal body) and Phase 9 (release engineering) remain
separate tracks.

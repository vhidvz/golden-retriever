# Context and companionship direction

**Status:** requirements record, 2026-11-23. This is **not an approved decision** and
no observation beyond the existing senses has been implemented. The owner wants Buddy to
observe more of the desktop so he can be a better helper and feel more like a warm family
dog. The shape below is the safe starting point; the final choice needs a spike, a written
plan and explicit approval.

## What exists today

- Focused application, fullscreen, overview and pointer position from the GNOME Shell
  extension. Pointer position is already used for his gaze.
- Window titles, only when their own sense is on.
- One-shot screen looks and one-shot camera looks through the local vision model.
- Microphone speech through the local audio model, on demand.
- Accessibility: focused controls, accessible actions, text fields and workflows.
- Journal entries are summaries of events, not raw screen pixels or typed text.

## The requested direction

### Pointer and clicks

Movement already reaches him; clicks do not. A safe next layer is a click event carrying
the current application and a coarse screen region, not a permanent coordinate history.
This is enough for reactions such as looking up, wagging or offering help without turning
Buddy into a detailed activity logger.

### Screen

The screen is currently observed only when asked (`buddy look`). A future option is a
bounded periodic glance while the user is active, with a visible indicator, a low frequency
and a local one-line summary. Continuous recording or a hidden always-on screenshot loop
is not proposed.

### Keyboard and typed text

**Raw global keystrokes are not the proposed path.** They would capture passwords,
terminals, consent dialogs and private messages, and a dog holding a keylogger is a poor
trade for a little context. The safe alternative is opt-in observation of **semantic text
changes in the focused accessible control** through AT-SPI:

- off by default and separate from the existing `accessibility` sense;
- refused for password fields, terminals, shells, lock/login/consent dialogs and any
  application without a usable accessibility tree;
- short-lived local summaries or bounded context, never a raw keystroke log, never the
  clipboard;
- visible while active and disabled by privacy mode;
- no typed text in the journal, exports or model prompts unless the owner explicitly
  chooses that retention.

If the owner later wants raw global input, it must be a separate high-risk decision with
its own consent flow, exclusions and audit trail. It is not implied by this phase.

### Family-dog warmth

The point of the extra context is not surveillance; it is better timing. With more
context Buddy should:

- notice routines and greet at the right moment rather than constantly;
- sit nearby, look up, rest his head or offer quiet company when the user is working;
- recognise when the user is busy, in a call or away, and be calm instead of demanding;
- offer help with a familiar next step, while keeping every action permissioned;
- remember **summaries** of what matters, not a transcript of the day.

The existing dog behaviour and sounds remain the interface. More observation must not
turn him into a chatbot or a notification stream.

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

## Open decisions

- Whether semantic text observation is useful enough without raw keys.
- Whether scheduled screen glances are worth their cost and privacy risk.
- Click granularity: application only, coarse region, or precise coordinates.
- Retention: session-only, a short rolling window, or user-approved memories.
- Whether the owner ever wants a separate raw-input mode; the recommendation is no.

This phase is recorded in `README.md` Phase 10 and `docs/decisions.md`; it is not
implemented. Phase 6 (body/fur), Phase 7 (automation), Phase 8 (photoreal body) and
Phase 9 (release engineering) remain separate tracks.

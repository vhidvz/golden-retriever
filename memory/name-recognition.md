---
name: name-recognition
description: The dog's configured name is user data; he hears it in typed and spoken speech, greets the first call, and is recalled by later calls
metadata:
  type: project
---

Implemented 2026-11-23 at the owner's request. The owner wants to name the dog, for
example «پشمک», call him by that name, and have him learn to answer it over time.

- `Identity.name` is the configured name. Set it with `buddy name NAME`, the Train page's
  Rename, or Settings. The daemon validates 1–60 printable characters.
- `language::addresses` and `Heard::Called` recognize the name in English and Persian,
  including a multi-word name and address words such as "hey". The name is stripped from
  a sentence before cue matching. The English/Persian forms of "Buddy" are aliases only
  while that is the configured name; renaming to «پشمک» stops those aliases.
- A name call is a recall, not a trick. The first call produces a bright greeting; later
  calls bring him to the middle of the screen and sit. A call while he is asleep wakes him
  unless the user deliberately sent him to sleep. Quiet mode still suppresses sound.
- `Life.name_calls` persists across restarts, is shown by `buddy status` and the panel,
  and gives the behavior its sense of familiarity over time.
- The voice path uses `language::addresses` before transcribing speech for him, so a
  configured name works without hard-coding names in vocabulary data.
- Tests cover chosen names, aliases, Persian, address words, multi-word names and the
  first-call-greeting / later-recall behavior. Durable decisions are in
  `docs/decisions.md` "Phase 5 completion and names".

**Why:** a dog's name is how the owner addresses him; it must be user data, not a
hard-coded product name, and repeated calls should feel like a relationship forming.

**How to apply:** keep names out of `lexicon.json`; use the configured-name path for any
new recognition surface, and do not make name recognition depend on a cloud model.
Related: [[phase5-completion]], [[control-panel]], [[buddy-product-decisions]].

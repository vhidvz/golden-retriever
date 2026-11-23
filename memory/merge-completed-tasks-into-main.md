---
name: merge-completed-tasks-into-main
description: "After a requested task is finished and verified, commit it and fast-forward merge its branch into main; push only when asked"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 5b57146d-145d-465d-8b74-d74014873801
  modified: 2026-11-23T00:00:00.000Z
---

On 2026-11-23 the owner made this a standing rule: after a task the owner asked for is
finished and verified, commit it on its branch and merge the branch into `main` yourself
(fast-forward where possible). Do not leave finished work on a side branch. Push only
when explicitly asked.

**Why:** the owner wants `main` to reflect completed work without having to ask for a
separate merge every time, while still controlling pushes.

**How to apply:** run the full suite, commit the task, switch to `main`, merge with
`--ff-only` (or a normal merge if the branch diverged), report the commit, and leave the
working tree clean. Never push without an explicit request. New phases still need their
spike/plan/approval gate. Related: [[tracked-docs-no-untracked-refs]],
[[self-contained-ai]].

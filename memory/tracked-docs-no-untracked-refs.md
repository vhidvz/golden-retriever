---
name: tracked-docs-no-untracked-refs
description: "Tracked documents must not cite scratch, cache, session or absolute-home paths; copy durable knowledge into git"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 5b57146d-145d-465d-8b74-d74014873801
  modified: 2026-11-23T00:00:00.000Z
---

On 2026-11-23 the owner made two standing rules:

1. A git-tracked document must not point at a path that is not in git — scratch, cache,
   `.superpowers/`, a session workspace or an absolute home path — as a source or as the
   only home of durable knowledge. If a fact matters, commit a self-contained summary in
   the repository and cite that tracked file.
2. **A plan is process, not a project artifact.** Do not copy a task-by-task plan, ledger
   or plugin workspace into the repository. Extract the durable decisions, interfaces,
   constraints and evidence into the existing docs, spikes or memory, then delete the
   plan copy. Do not create a parallel `docs/plans/` or `docs/superpowers/` tree.

**Why:** references to untracked paths go stale, disappear with scratch cleanup, and
mislead a later session; copying whole plans adds process noise, not knowledge, and the
same knowledge then has two homes that drift apart.

**How to apply:** before citing a path in README, CLAUDE.md, docs or memory, check
`git ls-files`; if it is not tracked, move the knowledge into a tracked document and cite
that. When a plan produces a decision or a constraint, write that into
`docs/decisions.md`, `docs/development.md`, `spikes/*/RESULTS.md` or `memory/`; do not
commit the plan. Product/runtime paths (for example `~/.local/share/buddy`) and the
designated scratch directory as a working instruction are exempt, but never as evidence.
Related: [[work-files-location]], [[self-contained-ai]], [[workflow-recording]].

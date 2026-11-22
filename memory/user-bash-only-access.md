---
name: user-bash-only-access
description: The owner acts on the machine through bash only; give every requested step as a copy-pasteable command and flag GUI-only actions
metadata:
  node_type: memory
  type: feedback
  originSessionId: 2544f5d0-0fc6-455b-8724-10d57b4433cb
  modified: 2026-11-22T18:45:01.214Z
---

On 2026-11-12 the owner said they only have access to their system's bash and asked for
whatever they need to do to be given as a bash command. This followed a request to log out
and back in and to approve a GNOME screen-sharing dialog without providing commands.

**Why:** they work on the Buddy machine through a terminal and may not always be in front
of the graphical desktop session.

**How to apply:** any action required of the owner goes in a copy-pasteable bash command.
If something genuinely needs a click in the graphical session (a portal consent dialog, a
graphical login, an extension approval), say so explicitly rather than describing it as a
step they can just take, and offer a terminal-only alternative where one exists. Do not
design verification steps that wait on GUI interaction. They can see the screen at least
sometimes (they have confirmed visual checks), so a visual check is acceptable when there
is no terminal equivalent. Related: [[buddy-product-decisions]],
[[blender-is-installed-use-it]].

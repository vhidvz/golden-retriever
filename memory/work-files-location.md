---
name: work-files-location
description: "Session scratch under /tmp is wiped between sessions; keep renders, bakes and helper scripts in ~/.cache/buddy-spikes/"
metadata:
  node_type: memory
  type: reference
  originSessionId: 2544f5d0-0fc6-455b-8724-10d57b4433cb
  modified: 2026-11-22T18:45:01.242Z
---

On 2026-11-15 the session scratchpad under `/tmp` came back empty after a session restart,
losing helper scripts and test renders. `/tmp` is tmpfs on this Manjaro machine.

Lasting places used for Buddy:

- `~/.cache/buddy-spikes/work/` — bake outputs, test renders and helper scripts.
  `shot.sh` takes a self-test picture with retries; `sheet.py` tiles pictures on a light
  background.
- `~/.cache/buddy-spikes/realism/` — realism comparison pictures and the first hair
  proof-of-concept script.
- `~/.cache/buddy-spikes/reference/` — CC-licensed Golden Retriever reference photos
  from Wikimedia Commons.

The directory is not part of the repository; create it when needed. Related:
[[blender-is-installed-use-it]].

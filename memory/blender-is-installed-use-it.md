---
name: blender-is-installed-use-it
description: "Blender 5.2.2 is installed and the owner expects it to build Buddy's body; check installed tools before writing a substitute"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 2544f5d0-0fc6-455b-8724-10d57b4433cb
  modified: 2026-11-22T18:45:01.187Z
---

On 2026-11-14 the owner asked why Blender on their machine had not been used to build a
realistic model, after a pure-Python mesh generator produced something closer to a weasel
than a Golden Retriever. Blender had never been checked for.

**Why:** the owner wants Buddy to look like a real Golden Retriever and sees no reason to
avoid a capable free tool that is already installed. They also corrected the target
shape: the sturdy, broad-headed, long-coated Golden look, not a lean hound.

**How to apply:** Blender 5.2.2 (`/usr/bin/blender`, pacman) runs headless with
`blender -b --factory-startup --python ...`, has numpy and Cycles on CPU, and builds
Buddy's body through `tools/build_dog.py`. Before writing a from-scratch substitute for
any asset or tooling, check what is installed (`command -v`, `pacman -Q`) and use it.
Related: [[buddy-product-decisions]], [[user-bash-only-access]],
[[work-files-location]].

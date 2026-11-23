---
name: camera-look
description: One-shot camera vision is available through buddy look --camera; continuous camera sense remains presence-only
metadata:
  type: project
---

Implemented 2026-11-23 at the owner's request to let Buddy look through the camera,
not just detect presence.

- `buddy look --camera` (or **Senses → Look through the camera**) requires the camera
  sense on and privacy mode off. The daemon pauses `camera_watch.py`, runs
  `perception/camera_still.py` for one webcam frame, sends it to `gemma4:e4b` through the
  same local `ask_about_image` path as `buddy look --file`, and deletes the frame after
  reading. Only a short summary remains in memory.
- The continuous `camera` sense is unchanged: 64×48 greyscale motion, presence only, no
  frames to any model. There is no continuous camera-vision stream.
- The camera prompt avoids identifying people or inferring sensitive traits. Cloud models
  are refused by the shared vision path.
- `BUDDY_CAMERA_PIPELINE` is the test seam for a synthetic GStreamer frame; the real path
  uses `v4l2src`. Durable decisions are in `docs/decisions.md` "Camera one-shot vision";
  testing notes are in `docs/development.md`.

**Why:** the owner wanted Buddy to actually see a webcam image, while keeping the
project's privacy rule and the presence-only camera design intact.

**How to apply:** any new camera vision must remain explicit and one-shot unless the
owner changes the camera-permission decision; never leave the presence helper fighting
for the device. Related: [[phase5-completion]], [[self-contained-ai]], [[control-panel]].

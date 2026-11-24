---
name: phase6-body-refinement
description: Phase 6 started with a measured-skeleton-preserving surface pass on cheeks, lips, ears and tail; sprite bakes are invalidated; KDE focus/pointer/overview added
metadata:
  type: project
---

Started 2026-11-24 at the owner's request. The first Phase 6 increment changes only
surface craft in `tools/build_dog.py`:

- smaller, less spherical cheek and upper-lip volumes; softer brow;
- slightly wider ears without moving their roots or changing their length;
- fuller tail base/middle with unchanged length and reach.

`tools/measure_dog.py` still reports every adult and puppy reference check as `ok`; the
skeleton, head landmarks and limb proportions are untouched. The Godot self-test rendered
the adult with coat and eyes. The body change correctly invalidates the pre-drawn sprite
sets, so the baker will rebuild them while the user is away.

A KDE Plasma sense was added 2026-11-24: a KWin script reports focused application,
pointer position and overview state to a session D-Bus bridge in the daemon. The bridge
was tested over the real session bus; overview is polled with QTimer. A native C++ KWin
click effect source is in `kde/kwin-effect`; it is built only where KWin headers exist and
could not be compiled/runtime-tested on this GNOME host. KDE window titles remain open.
Still open: remaining fur/model/puppy craft. Durable decisions: `docs/decisions.md`
"Phase 6 body refinement"; measurements: `docs/proportions.md`.

**Why:** the silhouette and proportions were already measured; the visible gap is craft,
so do not disturb the skeleton while improving the surface.

**How to apply:** change body geometry only alongside a preview and `measure_dog.py`;
expect a sprite rebake. Related: [[realism-direction]], [[phase5-completion]].

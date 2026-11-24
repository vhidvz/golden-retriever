---
name: phase6-body-refinement
description: Phase 6 started with a measured-skeleton-preserving surface pass on cheeks, lips, ears and tail; sprite bakes are invalidated; KDE focus/pointer/overview/titles added
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
pointer position, window titles and overview state to a session D-Bus bridge in the
daemon. The bridge was tested over the real session bus; overview is polled with QTimer.
Titles travel with Focus and are kept only while `window_titles` is allowed; gdbus tests
covered both off and on. A native C++ KWin click effect source is in `kde/kwin-effect`;
it is built only where KWin headers exist and could not be compiled/runtime-tested on this
GNOME host. Still open: native click runtime verification and remaining fur/model/puppy
craft. Durable decisions: `docs/decisions.md` "Phase 6 body refinement"; measurements:
`docs/proportions.md`.

**Why:** the silhouette and proportions were already measured; the visible gap is craft,
so do not disturb the skeleton while improving the surface.

**How to apply:** change body geometry only alongside a preview and `measure_dog.py`;
expect a sprite rebake. Related: [[realism-direction]], [[phase5-completion]].

## Live body and motion pass (2026-11-24)

- The live mesh was refined in Blender: wider skull and muzzle, larger ears, eyes and
  nose, and longer feathering on the chest, backs of the legs and tail. `retriever.json`
  was rebuilt. Adult and puppy proportion checks still pass.
- The live renderer now uses `renderer/eye.gdshader` for a brown iris, pupil, limbal ring
  and wet highlight instead of a black sphere, and the coat shader perturbs the lighting
  normal at fur scale.
- Motion: `main.gd` caps window travel per frame (2 units/s at the current scale), so an
  activity or `x_frac` jump cannot step the leg phase by a whole stride; `sprites.gd`
  adds walk/trot/gallop hysteresis, and the 3D body is hidden once the sprite fade passes
  35%, removing the doll ghost during crossfades.
- A second small pass added thigh mass and raised the neutral tail from 35° to 18°,
  giving a more level, friendly side silhouette. Adult proportions still pass.
- This is still not photoreal; the Phase 8 800 px evidence and no-go decision stand.
- Existing pre-pass sprite sets may be used until a new set is complete; the baker
  fingerprints the body and will rebuild. A changed body invalidates old bakes by design.

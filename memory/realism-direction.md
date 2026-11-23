---
name: realism-direction
description: "Research (2026-11-23) on making Buddy pass for a real Golden Retriever in image and motion; the offline sprite bake is the recommended path, 3DGS is not viable on 2-4 GB VRAM, and the target size is undecided"
metadata:
  type: project
---

On 2026-11-23 the owner asked for the research on a fully realistic Golden Retriever —
indistinguishable from a real one in both image and behaviour — to be kept as part of
the completion roadmap for later implementation. **No option is approved.** The full
record is `docs/realism-direction.md`.

The durable conclusions:

- The current architecture is the right one: expensive pictures are baked offline in
  Cycles while the user is away, and the renderer only shows sprites. The gap is the
  body surface, head, coat, light and motion, not the runtime design. Nine growth
  stages, about 1,700 frames each, about 10 seconds per frame, roughly 40 hours total
  (`docs/development.md`, `crates/buddy/src/baker.rs`).
- Splat-based animal avatars (DogRecon, GaussianDog, GART, FurE) are research-grade:
  training needs 24–48 GB of VRAM, BITE/D-SMAL are non-commercial, and the Godot splat
  viewers need Forward+/compute, not the `gl_compatibility` renderer. Reject for now.
- No open photoreal rigged Golden Retriever asset was found. The closest is
  FigDaFloof's CC BY 4.0 model, rigged but low-detail and unanimated. The asset is the
  bottleneck.
- The only real dog motion found is AI4Animation/MANN: 51 BVH clips, CC BY-NC, research
  use only. Under decision 6 it stays an optional, separate, licence-gated download and
  may become required only for a motion feature, never for the daemon. 3DDogs and
  DigiDogs are non-commercial and non-redistributable; SuperAnimal weights are
  non-commercial.
- Recommended (proposal): upgrade the offline bake — body/head, three hair systems,
  HDRI + AgX + contact shadow, MANN-based gaits, secondary motion and more idle
  variants — and test a real-photo/video puppet (option B) as a ceiling experiment.
  Diffusion refinement is an optional experiment, not the backbone.

The first step is a spike with a blind A/B test against real Golden photographs at the
size he is actually drawn at, one MANN walk retarget with a foot-slide measurement, and
a bake cost/VRAM measurement on CPU and GPU. The goal is realistic form, anatomy, coat
and movement, not a life-size dog: he stays desktop-sized so the desktop remains
usable. A larger close-up display would raise the bar and likely need a scan or real
footage (option B).

**Why:** the owner wants the research kept for later implementation without redoing it,
and the choice changes the Phase 6 body plan.

**How to apply:** read `docs/realism-direction.md` before any Phase 6 body or motion
work. Do not add 3DGS, BITE, D-SMAL or another non-commercial dependency to the core.
Keep the runtime cheap and the bake resumable and away-only. Run the blind test and the
spike before committing to a quality level. Related: [[buddy-product-decisions]],
[[self-contained-ai]], [[tracked-docs-no-untracked-refs]],
[[merge-completed-tasks-into-main]].

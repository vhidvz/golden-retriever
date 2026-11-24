# Body realism direction: a Golden Retriever that passes for a photograph

**Status:** research record (2026-11-23) plus a measured stills baseline and a no-go
decision for this hardware (2026-11-24). The owner asked for the research to be kept as
part of Buddy's completion roadmap; the choice between the options below, and the target
on-screen size, are still open for future hardware or an optional experiment. The baseline
is in `spikes/realism/RESULTS.md`.

## The question

What is the shortest open, free, local-first path to a Buddy who cannot be
distinguished from a real Golden Retriever — in a still image and in motion — at an
acceptable computational cost on this machine?

"Acceptable" here means the runtime stays cheap: the dog is an overlay on a desktop,
not a game. The expensive work may happen offline while the user is away, as the
existing sprite bake already does.

## What exists today

- The renderer is deliberately cheap. `renderer/project.godot` uses the
  `gl_compatibility` renderer; `spikes/RESULTS.md` measures 2.7–4.5 ms of GPU per frame
  for the live 3D body at the default 420 px setting, and the sprite path costs almost
  nothing.
- The sprite bake is the expensive half and already runs offline. `tools/bake_sprites.py`
  grows individual hairs on the body, moves body and hair with the bones, and renders
  each frame in Cycles with the Principled Hair BSDF. Defaults are 24 samples and
  roughly 150,000–250,000 strands; `crates/buddy/src/baker.rs` divides his growth into
  nine stages; `docs/development.md` records about 10 seconds per frame and about
  1,700 frames per stage, or roughly 40 hours for all stages. It runs only while the
  user is away, is resumable, and draws only what changed.
- `renderer/export_poses.gd` records the full movement set: stand, sit and lie, three
  sleeps, walk/trot/gallop, eat and drink, sniff, play bow, stretch, yawn, shake, roll,
  scratch, groom, tail chase, greet, invite, seek attention, six posture transitions,
  two sleep transitions, and five drawn gaze directions for the three main postures.
  `renderer/sprites.gd` selects them by activity, posture, variant, stride, gaze and
  facing, with crossfades and mirroring.
- The body is built from numbers by `tools/build_dog.py`; its proportions were measured
  against breed standards, photographs and skeletal studies in `docs/proportions.md`.
  The shape is sound; the surface, head and coat are still simple. The baked pictures
  read as a well-made CG dog, not as a photograph.
- The project baseline is a GeForce GT 1030 with 2 GB of VRAM
  (`docs/development.md`). The machine used for this research reports a GeForce GTX
  1650 with 4 GB (measured 2026-11-23). VRAM is the scarce resource in both cases.
- The product constraints that decide this question: no paid assets or services; CC
  BY-NC material is an optional separate download, never bundled in the MIT repository
  and never a hard dependency (`docs/decisions.md`, decisions 1 and 6); a new phase
  needs spike results, a written plan and explicit approval (decision 5).

## Findings

### 1. Splat-based animal avatars are research-grade, not a Buddy runtime

The 2025–2026 state of the art for a photoreal animal from images is 3D Gaussian
Splatting anchored to an articulated template:

- **DogRecon** (IJCV 2025) turns one dog photograph into an animatable Gaussian dog and
  reports about six minutes of training on one RTX 4090. Its code was not released at
  the time of this research, and it depends on BITE and D-SMAL, whose licences are
  non-commercial scientific research only.
- **GaussianDog** is an MIT-licensed repository around the same dependency chain. Its
  own hardware table tests an RTX A6000 with 48 GB; 24 GB is described as unverified,
  and TRELLIS alone needs at least 16 GB. Its demonstration is 37,525 Gaussians, twelve
  baked actions and 60 FPS in a browser. The MIT licence covers only its own code:
  BITE, D-SMAL, TRELLIS weights and generated assets keep their own terms.
- **GART** (CVPR 2024) is the template-based method behind several of these works and
  reports 150+ FPS, but still needs the animal template and a training GPU.
- **FurE** reconstructs explicit strand-based fur from calibrated multi-view images in
  about 52 minutes and exports it to Blender or Unreal. It needs a capture rig, handles
  a single static strand layer, and has no motion.

On the engine side, the Godot splat viewers do not fit Buddy: GDGS requires Forward+
and compute shaders, `shiena/godot-gsplat` is a GDExtension for Godot 4.5+ with a
Forward+ profile, and godotGS is a fork of the engine itself with Vulkan. Buddy's
renderer is `gl_compatibility`; switching it would be a separate architectural change,
and the 2–4 GB of VRAM would still be the limit.

**Conclusion:** 3DGS can produce the best still image, but not on this hardware, not
with these licences, and not inside the current renderer. It is worth watching, not
adopting.

### 2. There is no open, photoreal, rigged Golden Retriever asset

- The closest free model found is FigDaFloof's "Simple Pre-Rigged Golden Retriever",
  published under CC BY 4.0: about 16.4k triangles, rigged, with no animation, and
  described by its creator as one of their first rigs. It is usable and modifiable,
  but it is not a photoreal scan.
- The photogrammetry scans of real Goldens on Sketchfab are not usable as open assets:
  "Maisie" is 93.1k triangles and carries a NoAI restriction with no download shown;
  the sleeping-dog scan of Gilbert is 99.9k triangles but shows no licence; another
  scan is 616.8k triangles with no licence shown. None is a confirmed CC0/CC BY
  download.
- AI-generated "CC0" meshes exist, but they are static, of variable anatomy, and not a
  dependable base for a breed-accurate groom.

**Conclusion:** the asset is the real bottleneck. The two honest routes are to build a
much better body and coat in Blender, or to take appearance from real photographs or
video (option B).

### 3. Real dog motion exists only as optional, non-commercial data

- **AI4Animation / MANN** is the only real dog motion capture found in BVH: 51 clips
  from a real dog, 21 joints, 60 FPS. The published inventory covers walk, trot, run,
  pace, turn, jump, hop, shake and sit–stand transitions. Its licence is CC BY-NC 4.0;
  the upstream README says research or education only, with no commercial use or
  redistribution. The project already verified the archive in `spikes/ai4animation/`:
  51 clips, 21 joints, 66 channels, 1,531 frames in the inspected clip. Under decision
  6 it can be an optional, separate, licence-gated download and a required part of a
  motion feature, never a hard dependency of the daemon.
- **3DDogs** (64 dogs, marker-based optical mocap) and **DigiDogs** are non-commercial
  research datasets and may not be redistributed.
- **SuperAnimal-Quadruped** weights are "modified MIT" for academic, non-commercial use
  only. The DeepLabCut code is LGPL-3.0, so a model trained from scratch on
  appropriately licensed frames would be a different case, but it needs labels and
  training time.
- **Two2Four** (Disney Research Studios and ETH Zürich) converts human motion into
  quadruped motion with a diffusion model and reports 203 ms per 32-frame window on an
  RTX 4090. No code release was found. It is a research watch item, not a component.

**Conclusion:** the walk, trot, run and posture transitions can be made real with MANN;
the rest of the repertoire is procedural and must be authored or learned separately.

### 4. The current architecture is the right one; the gap is craft

The expensive part of the current design is already offline and resumable, and the
runtime already costs almost nothing. The pictures fail because of the body surface,
the head, the eyes, the coat, the light and the motion — not because the architecture
is wrong. That matters: it means the realistic path is a quality programme on the
existing bake, not a rewrite around a neural renderer.

## Options

### A. Upgrade the offline bake (recommended)

Keep the sprite architecture and raise the quality of what is drawn:

- **Body:** rebuild the head (stop, brow, cheeks, lips, chin, eyelids with thickness,
  ears as folded shells), the paws and toes, the chest and the tail plume. Keep the
  measured proportions from `docs/proportions.md`. Use the CC BY FigDaFloof mesh, or a
  refined version of it, only if it proves better than the procedural body; its licence
  permits modification and redistribution with attribution.
- **Coat:** three hair systems — a dense short undercoat, a longer water-resistant
  topcoat, and feathering on the chest, belly, legs, tail and ears. Use guide curves
  for clumping, per-strand length and curl variation, and the Principled Hair BSDF's
  melanin, redness, random colour and roughness. The current bake already does a
  simplified version of this; the change is density, layering and grooming control.
- **Light and camera:** replace the flat world with a CC0 HDRI and a warm key light,
  use the AgX view transform, add a real contact shadow, and consider a long-lens
  perspective instead of the orthographic side view. The pictures must still composite
  cleanly on an arbitrary desktop.
- **Motion:** retarget MANN's walk, trot, run, sit–stand and turn clips onto Buddy's
  25-bone skeleton with foot-contact preservation; keep procedural clips for the rest.
  Add secondary motion (tail, ears, jowls, breathing) at bake time, and bake several
  idle variants so a loop does not repeat visibly.
- **Behaviour variety:** the sim already varies activities, but a four-second idle loop
  is a tell. Bake more variants, vary playback phase and rate, and consider separate
  tail and eyelid layers for runtime secondary motion. This is the cheapest way to
  make the pictures feel alive without a real-time renderer.

**Cost:** the bake is the cost. Doubling strands and samples is roughly 4–10x the
current time: 20–50 seconds per frame, 10–25 hours per stage, and 4–9 days for all nine
stages, depending on quality. That is acceptable only because it runs while the user is
away and is resumable. Rendering Cycles on the GTX 1650 may cut this substantially, but
4 GB of VRAM may not hold a dense groom; this must be measured. Runtime stays as it is.

**Realism:** plausible at the desktop sizes he is actually drawn at and at 420 px. At
an 800 px close-up it will probably still read as a render, because the head and coat
need more detail than an open asset provides.

### B. Real-photo or video puppet

Take a CC BY-SA photograph or a short video of a real Golden Retriever, matte the dog,
and animate it as a layered 2D puppet (warping, cut-out limbs, multiple poses). For the
views and activities it covers, the image is literally a photograph.

**Cost:** almost none; no bake, no GPU.

**Limits:** only the filmed or photographed activities and angles; fixed lighting and
background; a specific real dog's identity; CC BY-SA's share-alike terms on the
adapted frames; and 2D warping shows at the limbs when the pose changes much. It is
excellent for idle, sleep and lying, and weak for walking, turning and interaction. It
could be an optional "photo coat" pack rather than the whole dog.

### C. Neural or splat avatar

Best still-image fidelity, but it needs 24–48 GB of VRAM to train, non-commercial
template models, and a Forward+/Vulkan renderer. It is not a path on this machine.
Revisit only if the hardware and the licences change.

### D. Optional: diffusion refinement of baked frames

Use the baked frames as structure (ControlNet depth/edge) and a local image model with
a real-dog reference (IP-Adapter) to photorealise them. This can raise still quality
without a scanned asset, but it risks flicker, identity drift and licence ambiguity,
and a 4 GB GPU limits the model size. Treat it as an experiment on top of A, not as the
backbone.

## Recommendation (not yet approved)

**A as the backbone, a small B experiment to measure the ceiling, C rejected, D only if
A plateaus.**

The first step is not a build. It is a spike with a measurable acceptance test:

1. **Stills.** Render four representative frames — stand, sit, walk and a head
   close-up — at the size he is actually drawn at, and at 800 px as a stress test,
   from the upgraded bake. Put them beside CC BY/CC BY-SA photographs of real Goldens
   in a blind A/B test (each image shown for about two seconds; the viewer says "real"
   or "render"). Record the confusion rate at each size.
2. **Motion.** Retarget one MANN walk clip to Buddy's skeleton, render it beside the
   procedural walk, and measure foot sliding against ground speed. A real gait with
   sliding feet is worse than a simple gait without.
3. **Cost.** Measure seconds per frame and peak VRAM for 500k and 1M strands at 32 and
   64 samples, on CPU and on Cycles GPU. Compare with the away-time budget.
4. **Decision.** If the blind test is near chance at the chosen size and the bake fits
   the away-time budget, adopt A and write the implementation plan. If it is not, and
   the target size is large, investigate B and the scan/licence question before
   spending more on A.

The same rule as every other phase applies: spike results, a written plan, explicit
approval.

## The goal and the acceptance size

The goal is the dog himself: his form, anatomy, coat, face and movement must read as a
real Golden Retriever. Display size is not a product goal — he stays desktop-sized so
the desktop remains usable, and a life-size Golden is explicitly not wanted. Size only
sets the technical bar for the blind test: the larger the drawn dog, the more detail
the bake needs. The renderer is normally run at the default 420 px setting; an 800 px
close-up is the hardest case. Option A is realistic at the sizes he is actually drawn
at. At 800 px, only a scan or real footage (option B) is likely to be
indistinguishable. The acceptance test therefore uses the size he is actually drawn at;
a later, larger display would need a new test.

## Decision triggers

- **Choose A** if the target is the desktop size he is actually drawn at and the blind
  test is close to chance.
- **Choose B (or a hybrid A+B)** if the target is a close-up or if A's blind test fails
  at the size he is drawn at and a share-alike photo pack is acceptable.
- **Revisit C** only with a GPU of 24 GB or more and a commercial or permissive
  template licence; neither exists on this machine today.
- **Stop and reduce scope** if the upgraded bake cannot finish inside the away-time
  budget; fewer stages, fewer variants or a lighter groom are the levers.

## Sources

Web sources, checked 2026-11-23:

- DogRecon, IJCV 2025: <https://link.springer.com/article/10.1007/s11263-025-02485-5>;
  project page: <https://vision3d-lab.github.io/dogrecon/>
- GaussianDog: <https://github.com/LarryWg/GaussianDog>
- GART, CVPR 2024:
  <https://openaccess.thecvf.com/content/CVPR2024/papers/Lei_GART_Gaussian_Articulated_Template_Models_CVPR_2024_paper.pdf>
- FurE: <https://toshi2k2.github.io/fure/assets/paper/FurE.pdf>
- GDGS, Godot Asset Library: <https://godotengine.org/asset-library/asset/5076>
- shiena/godot-gsplat: <https://github.com/shiena/godot-gsplat>
- godotGS: <https://klausi3d.github.io/godotGS/latest/>
- FigDaFloof, Simple Pre-Rigged Golden Retriever (CC BY 4.0):
  <https://sketchfab.com/3d-models/simple-pre-rigged-golden-retriever-c145f814acca44c7b43636c316e7c08c>
- Maisie, photogrammetry scan (NoAI):
  <https://sketchfab.com/3d-models/maisie-d74eb23cd692466f83b20c8cc588df74>
- AI4Animation: <https://github.com/sebastianstarke/AI4Animation>; the dog capture
  archive is the one pinned in `spikes/ai4animation/README.md`.
- 3DDogs: <https://cvssp.org/data/3DDogs/>
- DigiDogs: <https://cvssp.org/data/DigiDogs/>
- SuperAnimal-Quadruped weights:
  <https://huggingface.co/mwmathis/DeepLabCutModelZoo-SuperAnimal-Quadruped>
- Two2Four, Disney Research Studios:
  <https://studios.disneyresearch.com/2026/07/30/two2four-generative-quadruped-puppeteering-from-human-motion/>
- Blender Cycles hair settings:
  <https://docs.blender.org/manual/en/latest/render/cycles/render_settings/hair.html>
- Blender Principled Hair BSDF:
  <https://docs.blender.org/manual/en/latest/render/shader_nodes/shader/hair_principled.html>

Project files this record relies on:

- `tools/bake_sprites.py` — the offline bake: Cycles, hair curves, samples, strands,
  lighting, camera and output format.
- `crates/buddy/src/baker.rs` — the nine growth stages and the away-only scheduling.
- `renderer/export_poses.gd` — the recorded movement set, including gazes and
  transitions.
- `renderer/sprites.gd` — clip selection, crossfades, mirroring and the sprite shader
  contract.
- `renderer/project.godot` — the `gl_compatibility` renderer.
- `docs/proportions.md` — the measured body, the references and the known limits.
- `spikes/RESULTS.md` — overlay and live-fur performance measurements.
- `spikes/ai4animation/README.md` and `RESULTS.md` — the MANN archive, its licence and
  the verified skeleton.
- `docs/development.md` — the hardware baseline, the 10 seconds per frame and the
  away-only bake rule.
- `docs/decisions.md` — decisions 1, 5 and 6, which bound this direction.


## Measured baseline (2026-11-24)

The first stills spike exported stand/sit/walk, trimmed one frame per clip and baked at
the exact 420 px window. It measured **28–33 s per frame** at 60k strands/12 samples and
**72.2 s** at 150k/24 on CPU. A real Cycles CUDA run of the production frame took
**25.4 s** (peak 1027 MiB). Extrapolated naively, the CPU production setting is about
34 hours per growth stage and roughly 12.8 days for all nine stages; the GPU path is
about 108 hours (4.5 days). The 150k frame is better than the 60k one but still visibly
CG. The committed evidence and the reproducible harness are under `spikes/realism/`.

This establishes the cost/quality starting point. The measured decision below closes
the option for this hardware; the list above is what a future experiment would need.


## Phase 8 decision (2026-11-24)

The 800 px close-up still reads as CG at 1M strands/64 samples; denser coat and more
samples did not fix the eye shape, muzzle or lip line. The 500k/32 run already peaked at
3725 MiB of the 4 GB card, and the GPU was slower than the CPU for these dense close-ups.
Neural/splat avatars are excluded by VRAM and non-commercial dependencies.

**Decision:** close Phase 8 as a no-go for photoreal on this hardware. Keep the current
sprite bake. Photo-puppet and diffusion refinement are optional future research, not
adopted now. Revisit only with different hardware, a permissive asset, or an explicit
owner request for an experiment.

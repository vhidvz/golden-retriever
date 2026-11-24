---
name: phase8-realism-spike
description: Phase 8 closed as a no-go for photoreal on this hardware; measured bake cost and CG head close-up retained as evidence; current sprite bake stays
metadata:
  type: project
---

Started 2026-11-24 at the owner's request. The first Phase 8 step is the measured stills
spike recommended by `docs/realism-direction.md`, not a photoreal implementation.

- Harness: `spikes/realism/run.sh`; evidence: `spikes/realism/RESULTS.md` and the tracked
  stand and close-up stills.
- Quick setting (60k strands, 12 samples, CPU Cycles): 28.5 s stand, 32.8 s sit, 32.3 s
  walk per frame. Production default (150k, 24): 72.2 s CPU, 25.4 s CUDA (2.8x faster;
  peak 1027 MiB).
- Naive extrapolation: ~34 h per growth stage, ~12.8 days for all nine stages on CPU;
  ~108 h (4.5 days) on the CUDA path. The existing away-only scheduler can absorb some
  of this; actual away time remains unmeasured.
- Quality: denser groom and more samples did not fix the CG look. At 800 px, 150k/24,
  500k/32 and 1M/64 all still read as CG (eyes, muzzle, lip line, coat edges); the 1M/64
  image is the decisive negative result. 500k/32 peaked at 3725 MiB of 4 GB, and the GPU
  was slower than CPU for the dense close-ups. No blind A/B or MANN walk test was run;
  the stills failure already closes the phase.
- **Decision:** Phase 8 is closed as a no-go for photoreal on this hardware. The current
  sprite bake stays; photo-puppet/diffusion/larger-GPU work is optional future research.
  Neural/splat avatars are rejected for this hardware/licences. The measured close-ups
  and costs are the evidence, not a photoreal success claim.

**Why:** Phase 8 must be argued from measured quality and cost, not from a renderer
rewrite or wishful settings.

**How to apply:** do not reopen photoreal implementation from this evidence alone;
revisit only with different hardware, a permissive asset or an explicit owner request.
Use the harness for any optional photo-puppet/diffusion/dense-groom experiment. Note:
setting `scene.cycles.device = "GPU"` alone does not enable CUDA; `compute_device_type`
must be set and the device enabled, or Blender silently renders on CPU. Related:
[[realism-direction]], [[phase6-body-refinement]].

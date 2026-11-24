---
name: phase8-realism-spike
description: Phase 8 started with a measured stills bake baseline; 72.2 s/frame at production defaults, still visibly CG; full photoreal acceptance untested
metadata:
  type: project
---

Started 2026-11-24 at the owner's request. The first Phase 8 step is the measured stills
spike recommended by `docs/realism-direction.md`, not a photoreal implementation.

- Harness: `spikes/realism/run.sh`; evidence: `spikes/realism/RESULTS.md` and two tracked
  stand stills.
- Quick setting (60k strands, 12 samples, CPU Cycles): 28.5 s stand, 32.8 s sit, 32.3 s
  walk per frame. Production default (150k, 24): 72.2 s stand.
- Naive extrapolation: ~34 h per growth stage, ~12.75 days for all nine stages on this
  CPU path. The existing away-only scheduler can absorb some of this; actual away time
  and GPU/VRAM performance are still unmeasured.
- Quality: the 150k frame is better than 60k but still visibly CG; head/eyes, paws and
  coat edges are the visible gaps. No blind A/B, 800 px head close-up, 500k/1M strands,
  32/64 samples, GPU test or MANN walk foot-slide test yet.
- This is a baseline, not approval of photoreal. The recommendation remains option A
  (upgrade the offline bake), with neural/splat avatars rejected for this hardware and
  licence constraints.

**Why:** Phase 8 must be argued from measured quality and cost, not from a renderer
rewrite or wishful settings.

**How to apply:** use the harness for repeatable runs; add a head close-up and dense
groom before proposing implementation. Related: [[realism-direction]],
[[phase6-body-refinement]].

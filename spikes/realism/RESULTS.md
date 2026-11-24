# Phase 8 realism spike — first stills baseline

Measured 2026-11-24 on the development machine (GeForce GTX 1650 4 GB, but Blender
Cycles is configured for CPU by `tools/bake_sprites.py`; no GPU bake was measured).
The harness is [`run.sh`](run.sh).

## What was measured

The Godot pose exporter recorded `stand`, `sit` and `walk` at adult growth, 420 px,
908×572 window. Each clip was trimmed to its first frame so the baseline could be
measured without a full movement bake. Blender then baked those frames with the current
body/coat and the exact window camera.

| Setting | Strands | Samples | Time per frame | Quality observation |
|---|---:|---:|---:|---|
| quick | 60,000 | 12 | 28.5 s (stand), 32.8 s (sit), 32.3 s (walk) | Coat visibly stringy; useful as a lower bound |
| production default | 150,000 | 24 | 72.2 s (stand) | Better, but still reads as a CG dog; head/paws/coat need work |

The two stand pictures are committed as evidence:

- [`stand-60k-12s.webp`](stand-60k-12s.webp)
- [`stand-150k-24s.webp`](stand-150k-24s.webp)

## What this means

- The current pipeline can produce usable desktop stills, but the quality is not
  photoreal and the cost is high: 72 s × roughly 1,700 frames per growth stage is about
  **34 hours per stage** on this CPU path, or roughly **306 hours (12.75 days) for all
  nine stages** before retries. The existing away-only scheduler can absorb that in
  principle, but the budget must be measured against the user's actual away time.
- This baseline does **not** satisfy the Phase 8 acceptance test. It has no real Golden
  photographs beside it, no blind A/B, no head close-up at 800 px, no 500k/1M-strand or
  32/64-sample run, and no GPU/VRAM measurement.
- The first quality gaps visible at this size are the head/eyes, the silhouette of the
  paws, and the coat's stringiness at the edges. More strands alone may not fix the
  shape; the next run must include a head crop and a denser groom.

## Next Phase 8 measurements

1. Head close-up at the actual display size and 800 px, using `FOCUS` and a real Golden
   reference.
2. 500k and 1M strands at 32 and 64 samples; CPU and Cycles GPU; peak VRAM.
3. A MANN walk retarget beside the procedural walk, with foot-slide measurement.
4. A blind A/B with freely licensed Golden photographs; record confusion rate.

The research recommendation remains option A (upgrade the offline bake), not a neural
avatar. This spike is the first measured cost baseline, not an approved photoreal
implementation.

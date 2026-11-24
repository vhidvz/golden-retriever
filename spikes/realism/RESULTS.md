# Phase 8 realism spike — measured stills and no-go

Measured 2026-11-24 on the development machine (GeForce GTX 1650 4 GB). CPU and Cycles
CUDA were both measured. The GPU is about 2.8× faster for the production 150k full-body
frame, but slower than the CPU for dense 500k/1M close-ups and leaves little VRAM
headroom. The harness is [`run.sh`](run.sh).

## What was measured

The Godot pose exporter recorded `stand`, `sit` and `walk` at adult growth, 420 px,
908×572 window. Each clip was trimmed to its first frame so the baseline could be
measured without a full movement bake. Blender then baked those frames with the current
body/coat and the exact window camera.

| Setting | Strands | Samples | Device | Time per frame | Observation |
| --- | ---: | ---: | --- | ---: | --- |
| quick | 60,000 | 12 | CPU | 28.5 s (stand), 32.8 s (sit), 32.3 s (walk) | Coat visibly stringy; lower bound |
| production default | 150,000 | 24 | CPU | 72.2 s (stand) | Better, still reads as CG |
| production default | 150,000 | 24 | CUDA | 25.4 s (stand) | 2.8× faster; peak 1027 MiB; same quality |
| 800 px head close-up | 150,000 | 24 | CPU | 34.3 s (36 s total) | Eyes/muzzle/coat still CG |
| 800 px head close-up | 500,000 | 32 | CPU | 64.9 s (69 s total) | Denser coat; still CG |
| 800 px head close-up | 500,000 | 32 | CUDA | 76.8 s (81 s total) | GPU slower here; peak 3725 MiB |
| 800 px head close-up | 1,000,000 | 64 | CPU | 143.3 s (151 s total) | Denser coat; still CG |
| 800 px head close-up | 1,000,000 | 64 | CUDA | 183.2 s (191 s total) | GPU slower here; peak 3639 MiB |

The close-ups used the head focus at `--size 2400x1600 --super 1`, then a centred
800×800 crop; `run.sh --focus 0.4 --size 2400x1600 --crop 800` reproduces them. The
production images keep the real 908×572 window and the 1.6× supersample.

The committed pictures are:

- [`stand-60k-12s.webp`](stand-60k-12s.webp)
- [`stand-150k-24s.webp`](stand-150k-24s.webp)
- [`stand-150k-24s-gpu.webp`](stand-150k-24s-gpu.webp)
- [`head-800px-150k-24s.webp`](head-800px-150k-24s.webp)
- [`head-800px-500k-32s.webp`](head-800px-500k-32s.webp)
- [`head-800px-1m-64s.webp`](head-800px-1m-64s.webp)

## What this means

- The CPU production setting needs about **34 hours per growth stage**, or roughly
  **306 hours (12.8 days) for all nine stages** before retries. A working CUDA path cuts
  the production frame to 25.4 s, about **108 hours (4.5 days)** for all nine stages.
  The existing away-only scheduler can absorb some of that, but the budget must be
  measured against the user's actual away time.
- Denser groom improves the coat, but the 1M/64 800 px close-up still fails: the eye
  shape, muzzle, lip line and coat edges read as CG. The problem is the model and
  materials, not only strand count or samples. This is the decisive negative result.
- The GPU does not rescue dense close-ups on this machine: 500k/32 is slower than CPU
  and peaks at 3725 MiB of the 4096 MiB card, leaving almost no headroom. A full-body
  1M-strand production bake is not practical at this VRAM. The 150k production setting
  remains the only viable GPU bake.
- The Phase 8 stills acceptance test is not met. A blind A/B with freely licensed Golden
  photographs was not run because the 1M/64 close-up is already clearly CG; the MANN
  walk/foot-slide comparison was not run either. Motion work cannot rescue a body that
  fails the close-up test.

## If Phase 8 is reopened

1. A blind A/B of the 800 px close-up beside at least ten freely licensed Golden
   photographs; record the confusion rate.
2. A MANN walk retarget beside the procedural walk, with foot-slide measurement.
3. A photo-puppet or diffusion experiment only with a permissive asset and a new owner
   request.

## Later live-body pass (2026-11-24)

After these measurements the live mesh was refined (wider head/muzzle, larger ears,
eyes and nose, coat feathering, procedural iris shader). The committed stills above are
from the pre-pass model and remain the measured evidence; the pass does not change the
no-go or make a photoreal claim.

## Decision

**No-go for photoreal on this hardware.** The 1M/64 800 px close-up still reads as CG,
the GPU cannot make dense production bakes faster within 4 GB, and neural/splat avatars
need 24–48 GB VRAM and non-commercial dependencies. The current sprite bake stays.
Photo-puppet and diffusion work remain optional future research, not adopted. Phase 8 is
closed as a **no-go decision**, not as a photoreal success.

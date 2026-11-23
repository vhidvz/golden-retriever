# AI4Animation dog motion — research spike

**Status:** research only. This directory is not part of Buddy's runtime and is not
installed by `scripts/install.sh`.

The Phase 4 gate in `docs/self-contained-ai-plan.md` says the data is not mandatory until
a motion feature consumes it. This spike answers the first questions without committing
Buddy to the data.

## What is used

| Item | Value |
|---|---|
| Source | MANN quadruped motion capture, SIGGRAPH 2018, `MotionCapture.zip` |
| URL | https://starke-consult.de/AI4Animation/SIGGRAPH_2018/MotionCapture.zip |
| Licence | CC BY-NC 4.0; the upstream README says research or education only, not commercial use or redistribution |
| Archive | 33,422,409 bytes (31.9 MiB) |
| Contents | 51 BVH clips; 83,240,385 bytes uncompressed; no licence file inside the archive |
| SHA-256 | `0ecd909b673cfca5e20c3f568376dcf27d7da3bd153957c2b034d95af366132a` |
| Clip inspected | `D1_001_KAN01_001.bvh` |

AI4AnimationPy (`facebookresearch/ai4animationpy`) is also CC BY-NC 4.0 and is about
1 GB; this spike does not need it. The BVH parser here uses only the Python standard
library.

## Run it

```sh
spikes/ai4animation/run.sh --accept-license
```

The flag is the licence gate. The script downloads with retry and resume, verifies the
SHA-256, extracts one clip into `~/.cache/buddy-spikes/reference/ai4animation-mann/`, and
prints the skeleton and motion summary.

```sh
python3 -I spikes/ai4animation/parse_bvh.py --selftest
python3 -I spikes/ai4animation/parse_bvh.py /path/to/clip.bvh --json
```

## What the clip says

- 21 joints, 66 channels per frame.
- 1,531 frames at 60 fps (`Frame Time: 0.0166667`).
- The skeleton is `Hips, Spine, Spine1, Neck, Head`, two foreleg chains
  (`Shoulder, Arm, ForeArm, Hand`), two hind-leg chains (`UpLeg, Leg, Foot`), and
  `Tail, Tail1`.

## Mapping to Buddy's skeleton

Buddy has 25 bones (`renderer/dog/retriever.json`). A first mapping:

| MANN | Buddy | Note |
|---|---|---|
| `Hips` | `hips` | root |
| `Spine`, `Spine1` | `chest` | two to one |
| `Neck` | `neck` | |
| `Head` | `head` | |
| `LeftShoulder`, `LeftForeArm`, `LeftHand` | `front_l_upper`, `front_l_lower`, `front_l_paw` | `LeftArm` is unused or blended |
| `RightShoulder`, `RightForeArm`, `RightHand` | `front_r_upper`, `front_r_lower`, `front_r_paw` | `RightArm` is unused or blended |
| `LeftUpLeg`, `LeftLeg`, `LeftFoot` | `hind_l_upper`, `hind_l_lower`, `hind_l_foot` | the hind paw is a separate Buddy bone |
| `RightUpLeg`, `RightLeg`, `RightFoot` | `hind_r_upper`, `hind_r_lower`, `hind_r_foot` | the hind paw is a separate Buddy bone |
| `Tail`, `Tail1` | `tail1`, `tail2` | Buddy's `tail3` has no source |
| — | `muzzle`, `jaw`, `ear_l`, `ear_r`, `hind_l_paw`, `hind_r_paw`, `tail3` | no source in the data; keep procedural |

The mapping is a design sketch, not a tested retarget. Foot contacts, root motion, scale,
coordinate systems and the two-to-one spine mapping all need a real retargeting pass.

## Decision

**Do not make the data mandatory now.** There is no loader, retargeting pipeline or
renderer integration in Buddy. The data is 31.9 MiB and the skeleton is close enough to
Buddy's to justify a future spike, but the licence is non-commercial and the motion
feature has not been approved.

If a motion feature is approved later:

- use only the MANN dog subset, never the other AI4Animation datasets;
- keep the download outside the MIT repository;
- require licence acceptance before download;
- verify a pinned SHA-256 and report readiness;
- keep Buddy alive if the data is missing;
- make the data required for the motion feature, not for the daemon.

This is the decision recorded in `docs/self-contained-ai-decision.md` D8.

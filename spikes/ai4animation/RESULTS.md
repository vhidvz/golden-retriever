# AI4Animation spike results

Measured on 2026-11-22 on the development machine (Manjaro, Ollama 0.40.16; the
hardware baseline in `docs/development.md` should be updated separately).

## What was run

- Downloaded `MotionCapture.zip` from the upstream SIGGRAPH 2018 link with `curl -C -`.
  The first attempt was interrupted by `SSL unexpected eof` after 9.7 MB; the resumed
  download completed at 31.9 MiB. This is why the installer needs retry and resume.
- Verified SHA-256:
  `0ecd909b673cfca5e20c3f568376dcf27d7da3bd153957c2b034d95af366132a`.
- Listed the archive: 51 BVH files, 83,240,385 bytes uncompressed, no licence file.
- Parsed `D1_001_KAN01_001.bvh` with `parse_bvh.py`:
  - 21 joints;
  - 66 channels per frame;
  - 1,531 frames;
  - 60 fps.
- Ran `parse_bvh.py --selftest` successfully.

## Findings

1. **The data is usable as a motion source.** BVH is simple, the parser needs no
   dependency, and the clips are small enough (hundreds of kilobytes to a few MB each).
2. **The skeleton is close, but not the same.** 19 of the 21 MANN joints have a natural
   Buddy target; `LeftArm` and `RightArm` are unused or need blending. Seven Buddy bones
   have no source: muzzle, jaw, both ears, both hind paws and `tail3`. Those must stay
   procedural or be authored.
3. **A retarget is the real work.** The data is a dog skeleton, but Buddy's bone lengths,
   rest pose, coordinate system, foot contact and root motion differ. A converter and a
   visual comparison are required before any feature claim.
4. **The licence is a product constraint.** The archive is CC BY-NC 4.0 and the upstream
   README says research or education only, not commercial use or redistribution. It
   cannot become a hard dependency of the MIT core.
5. **There is no consumer today.** No Buddy code reads BVH, NPZ or AI4Animation data.
   The model file and renderer use Buddy's own procedural poses and baked sprites.

## Go / no-go

**No-go for a mandatory installation now.** The data should stay a research spike and an
optional future download.

**Go for the next spike only if a motion feature is approved:** retarget one clip to
Buddy's 25-bone skeleton, render it beside the procedural version, and measure whether
the result justifies the licence, data and maintenance cost. Until then, the decision
record D8 keeps the data out of Buddy's installation lifecycle.

## Limits

- Only one clip was parsed; the other 50 are unexamined.
- No retargeting or rendering was done.
- No AI4AnimationPy installation was needed or attempted.
- The archive's own file dates are from 2009–2017; the download was last modified in
  2018. There is no upstream checksum, so Buddy's manifest would pin the SHA-256 above.

"""Builds Buddy's body in Blender: one skinned Golden Retriever, from numbers.

    blender -b --factory-startup --python tools/build_dog.py
    blender -b --factory-startup --python tools/build_dog.py -- --preview /some/folder

The dog is described the way a boat or an aeroplane is drawn: each part (trunk,
neck, head, a leg, the tail, an ear) is a row of cross-sections along a path.
Blender melts the parts into one surface, cuts the eye sockets, thins the mesh
where it is flat, and this script then tells every point of that surface which
bones move it, how long and what colour the coat is there, which way it lies,
and how much light reaches it. Nothing is drawn by hand or taken from anyone
else's model: change the numbers and run it again. Proportions were measured
from photographs of real dogs standing in profile.

The file holds him twice over, as an adult and as a puppy of eight weeks: the same
points and bones, carried from the one to the other by as_puppy(). The renderer
moves between the two as he grows.

It writes renderer/dog/retriever.json. Blender is only needed to rebuild that
file, never to run Buddy.

Units: the adult is 0.74 tall at the withers (about 58 cm, so one unit is about
78 cm). +X is forward, +Y up, Z across; the dog is symmetric in Z.
"""
import json, math, os, sys, time

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "renderer", "dog", "retriever.json"))
VOXEL = 0.0065          # how finely the melted surface is sampled
TRIANGLES = 38000       # how many triangles the finished body keeps
SPACING = 0.011         # distance between cross-sections along a part
RING = 36               # points around each cross-section
LAYER_GAP = 0.0044      # the coat is drawn in layers this far apart …
MAX_LAYERS = 26         # … but never more than this many


def srgb(r, g, b):
    """A colour as it looks on screen. The renderer turns these into linear light itself."""
    return (r, g, b)


def linear(colour):
    return np.where(colour > 0.04045, ((colour + 0.055) / 1.055) ** 2.4, colour / 12.92)


GOLD = srgb(0.86, 0.65, 0.37)      # the body
DEEP = srgb(0.79, 0.55, 0.28)      # along the back and on the ears
LIGHT = srgb(0.94, 0.80, 0.56)     # the longer coat underneath and behind the legs
CREAM = srgb(0.98, 0.90, 0.72)     # the tips of the feathering
FACE = srgb(0.88, 0.69, 0.42)      # the short coat of the muzzle
RIM = srgb(0.07, 0.05, 0.045)      # eyelids, lips
LEATHER = srgb(0.045, 0.04, 0.04)  # the nose
PALATE = srgb(0.30, 0.13, 0.14)    # inside the mouth
GUMS = srgb(0.45, 0.20, 0.22)
PAD = srgb(0.10, 0.08, 0.075)

HEAD_DRAWN = (0.40, 0.85)          # the head is described level, with the top of the neck here …
HEAD_AT = (0.355, 0.885)           # … and then carried here, high on an upright neck, with the nose a little down
HEAD_TILT = math.radians(-7.0)
HEAD_SIZE = 0.97                   # … at this size: occiput to nose tip about 0.40 of his height at the withers
                                   # (measured on photographs of Goldens: 0.385; see docs/proportions.md)
STOP = 0.56                        # where, on the head as described, the forehead drops to the muzzle

# The same dog at eight weeks. A puppy is not a small adult: his body is short and
# round and close to the ground, his legs short and thick, his head big for him with
# a short muzzle, his tail a stub, and his coat an even fluff with no feathering.
# Every part below is described once, for the adult; these say how the puppy differs.
PUP_SHORT = 0.63                   # how much shorter he is from front to back
PUP_LOW = 0.66                     # how much shorter his legs are
PUP_SINK = 0.60 * (1.0 - PUP_LOW)  # so his back is this much nearer the ground
PUP_HEAD_AT = (0.248, 0.605)             # on a short neck
PUP_HEAD_TILT = math.radians(-4.0)
PUP_HEAD_SIZE = 1.3                # bigger than the adult's in proportion: big and round on a small body
PUP_DOME = 0.20                    # and domed: the skull this much higher, above a steep stop
PUP_MUZZLE = 0.42                  # his muzzle is this fraction of the adult's length
PUP_TAIL = 0.55
PUP_LEG = 1.5                      # his legs are this much thicker for their length
PUP_FLUFF = (0.060, 0.064, 0.066)  # the coat on his back, flanks and belly
# His foreleg: shortening the leg below the body leaves the shoulder blade, which lies in
# the body, at the adult's length, so it came out at 35% of the foreleg where puppies of
# nine weeks have 29% (Helmsmuller et al. 2013). Lifting his shoulder joint and, less, his
# elbow restores 29 : 24 : 47 (solved with tools/measure_dog.py; docs/proportions.md).
PUP_SHOULDER_LIFT = 0.0405         # in the puppy's own units (his height at the withers is about 0.5)
PUP_ELBOW_LIFT = 0.0255
# The same for his hind leg: his shin came out a little long and his foot short (33 and 33%
# where those puppies have 31 and 35%); his hock is lifted, his stifle stays.
PUP_HOCK_LIFT = 0.0195


def tilt(p):
    """Carries a point of the head as described to where it is when he holds it naturally."""
    x, y = p[0] - HEAD_DRAWN[0], p[1] - HEAD_DRAWN[1]
    c, s = math.cos(HEAD_TILT) * HEAD_SIZE, math.sin(HEAD_TILT) * HEAD_SIZE
    return (HEAD_AT[0] + x * c - y * s, HEAD_AT[1] + x * s + y * c) + tuple(v * HEAD_SIZE for v in p[2:])


# ---- conformation: where his joints are -------------------------------------------------------
# The legs and the rump were first drawn by eye. Measured against breed standards and limb
# studies (tools/measure_dog.py, docs/proportions.md), the upper arm was too short and too
# upright, the femur too short, the shin too long, the rump overhung the hind feet and the
# body was too long. The corrected joints below follow these rules, with the height at the
# withers (0.715) unchanged:
#   - shoulder blade laid back about 30 degrees from vertical (Zink & Schlehr 2020);
#   - upper arm about 0.9 of the blade (AKC "about the same length"; Beagle adults 0.88);
#   - elbow under the withers, at half the height (AKC, UKC, GRCA judges' education);
#   - foreleg shares 28 : 25 : 47 (blade : upper arm : forearm and foot; Beagle adults);
#   - femur at about 90-100 degrees to a pelvis sloping 30 degrees (AKC);
#   - hind leg shares 36 : 34 : 30 (femur : shin : foot; Beagle adults);
#   - a plumb line from the point of buttocks reaching the ground at the front of the hind
#     toes (Zink & Schlehr 2020), and body length 12:11 of the height (AKC).
# Each part of a leg, as first described, is carried along onto the corrected skeleton by
# along(), so the shapes keep their thickness and their coat.
FRONT_CHAIN_DRAWN = [(0.20, 0.65), (0.230, 0.510), (0.172, 0.372), (0.178, 0.048)]          # above the shoulder, shoulder, elbow, paw
FRONT_CHAIN = [(0.20, 0.65), (0.257, 0.523), (0.146, 0.357), (0.152, 0.048)]
HIND_CHAIN_DRAWN = [(-0.30, 0.66), (-0.310, 0.550), (-0.238, 0.362), (-0.426, 0.186), (-0.416, 0.048)]   # above the hip, hip, stifle, hock, paw
HIND_CHAIN = [(-0.33, 0.66), (-0.332, 0.550), (-0.240, 0.296), (-0.463, 0.186), (-0.453, 0.048)]
FRONT_PAW_SHIFT = FRONT_CHAIN[-1][0] - FRONT_CHAIN_DRAWN[-1][0]
HIND_PAW_SHIFT = HIND_CHAIN[-1][0] - HIND_CHAIN_DRAWN[-1][0]
RUMP_FROM, RUMP_SCALE = -0.21, 0.70      # behind this the body, as first drawn, is shortened to this much


def rump(x):
    """Where a point of the body as first drawn lies along it now: the rump is shorter."""
    return x if x >= RUMP_FROM else RUMP_FROM + (x - RUMP_FROM) * RUMP_SCALE


TAIL_SHIFT = rump(-0.425) + 0.425        # the tail keeps its length and moves with the end of the rump
TAIL_ROOT = (-0.425 + TAIL_SHIFT, 0.632)


def along(drawn, chain, p):
    """Carries a point described against one chain of joints onto another: it keeps where it
    is along its segment and how far it stands out from it, turned with the segment."""
    p = np.asarray(p[:2], dtype=float)
    best = None
    for i in range(len(drawn) - 1):
        a, b = np.asarray(drawn[i]), np.asarray(drawn[i + 1])
        t = float(np.clip(np.dot(p - a, b - a) / np.dot(b - a, b - a), 0.0, 1.0))
        d = float(np.linalg.norm(p - (a + t * (b - a))))
        if best is None or d < best[0]:
            best = (d, i, t)
    _, i, t = best
    a, b = np.asarray(drawn[i]), np.asarray(drawn[i + 1])
    na, nb = np.asarray(chain[i]), np.asarray(chain[i + 1])
    offset = p - (a + t * (b - a))
    turn = math.atan2(*(nb - na)[::-1]) - math.atan2(*(b - a)[::-1])
    c, s_ = math.cos(turn), math.sin(turn)
    return tuple(na + t * (nb - na) + np.array([offset[0] * c - offset[1] * s_, offset[0] * s_ + offset[1] * c]))


def front(x, y):
    return along(FRONT_CHAIN_DRAWN, FRONT_CHAIN, (x, y))


def hind(x, y):
    return along(HIND_CHAIN_DRAWN, HIND_CHAIN, (x, y))


def as_puppy(points):
    """Where points of the adult's body are on the puppy. One smooth rule for the whole
    of him, so that his surface, his bones and his eyes all move together and nothing
    creases: the body and legs shorten, the tail shrinks along its length, the head
    keeps nearly its size and goes to the end of a shorter neck."""
    p = np.atleast_2d(np.asarray(points, dtype=float))
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    ease = lambda t: np.clip(t, 0.0, 1.0) ** 2 * (3.0 - 2.0 * np.clip(t, 0.0, 1.0))

    # Thick legs: below the body, everything close to the line down the middle of a leg moves out from it.
    front = x > -0.1
    line_x = np.where(front, np.interp(y, FRONT_LEG_LINE[0], FRONT_LEG_LINE[1]), np.interp(y, HIND_LEG_LINE[0], HIND_LEG_LINE[1]))
    line_z = np.where(front, 0.088, 0.092) * np.where(z >= 0.0, 1.0, -1.0)
    off = np.sqrt((x - line_x) ** 2 + (z - line_z) ** 2)
    # (fading out slowly with the distance from the leg: a quick fade would fold the surface over)
    thicker = (PUP_LEG - 1.0) * ease((0.46 - y) / 0.10) * (1.0 - ease((off - 0.035) / 0.07)) * (1.0 - np.clip((0.07 - y) / 0.05, 0.0, 1.0) * 0.6)
    # and not at all on his middle, between a pair of legs, or the two would cross; the chest
    # comes down between the forelegs to the elbow, so the fade starts well away from the middle
    thicker *= ease((np.abs(z) - 0.02) / 0.06)
    # The lift of his legs (see PUP_SHOULDER_LIFT): nothing at the ground; on a foreleg most at
    # the shoulder joint and nothing at the withers, on a hind leg most at the hock and nothing
    # at the stifle; fading out slowly away from the leg and towards his middle (a quick fade
    # folds the surface over where the elbow meets the chest).
    sy, ey, wy = FRONT_CHAIN[1][1], FRONT_CHAIN[2][1], 0.715
    fore_lift = np.where(y <= ey, PUP_ELBOW_LIFT * np.clip(y / ey, 0.0, 1.0),
                         np.where(y <= sy, PUP_ELBOW_LIFT + (PUP_SHOULDER_LIFT - PUP_ELBOW_LIFT) * (y - ey) / (sy - ey),
                                  PUP_SHOULDER_LIFT * np.clip((wy - y) / (wy - sy), 0.0, 1.0)))
    ky, hy = HIND_CHAIN[2][1], HIND_CHAIN[3][1]
    hind_lift = np.where(y <= hy, PUP_HOCK_LIFT * np.clip(y / hy, 0.0, 1.0), PUP_HOCK_LIFT * np.clip((ky - y) / (ky - hy), 0.0, 1.0))
    lift = np.where(front, fore_lift, hind_lift) * (1.0 - ease((off - 0.05) / 0.12)) * ease(np.abs(z) / 0.08)
    x = x + (x - line_x) * thicker
    z = z + (z - line_z) * thicker

    # Body and legs. Below the body a leg simply gets shorter; the body sinks by as much as
    # the legs lost; a paw keeps its size and goes where the leg above it went.
    high = ease((y - 0.30) / 0.20)
    foot = np.clip((0.09 - y) / 0.06, 0.0, 1.0)
    sole = np.where(x > -0.1, PAWS["front"][0], PAWS["hind"][0])
    bx = x * PUP_SHORT * (1.0 - foot) + (sole * PUP_SHORT + (x - sole)) * foot
    by = y * (PUP_LOW + (1.0 - PUP_LOW) * foot) * (1.0 - high) + (y - PUP_SINK) * high + lift

    # The tail: shorter along its length, as thick as it was.
    xs, ys = [c[0] for c in TAIL_LINE], [c[1] for c in TAIL_LINE]
    line = np.interp(x, xs[::-1], ys[::-1])
    behind = (x < TAIL_ROOT[0]) * ease((y - 0.40) / 0.06)
    bx = bx * (1.0 - behind) + (TAIL_ROOT[0] * PUP_SHORT + (x - TAIL_ROOT[0]) * PUP_TAIL) * behind
    by = by * (1.0 - behind) + (TAIL_ROOT[1] - PUP_SINK + (line - TAIL_ROOT[1]) * PUP_TAIL + (y - line)) * behind

    # The head: back to the head as described, then out again as the puppy carries it.
    c, s_ = math.cos(HEAD_TILT) / HEAD_SIZE, math.sin(HEAD_TILT) / HEAD_SIZE
    hx, hy = x - HEAD_AT[0], y - HEAD_AT[1]
    dx, dy = HEAD_DRAWN[0] + hx * c + hy * s_, HEAD_DRAWN[1] - hx * s_ + hy * c
    skull = np.clip((STOP + 0.02 - dx) / 0.05, 0.0, 1.0)       # 1 on the skull, 0 on the muzzle
    dx = np.where(dx > STOP, STOP + (dx - STOP) * PUP_MUZZLE, dx) - HEAD_DRAWN[0]
    dy = dy - HEAD_DRAWN[1]
    dy = np.where(dy > 0.0, dy * (1.0 + PUP_DOME * skull), dy)
    c, s_ = math.cos(PUP_HEAD_TILT) * PUP_HEAD_SIZE, math.sin(PUP_HEAD_TILT) * PUP_HEAD_SIZE
    head_x, head_y = PUP_HEAD_AT[0] + dx * c - dy * s_, PUP_HEAD_AT[1] + dx * s_ + dy * c
    head_z = z * PUP_HEAD_SIZE / HEAD_SIZE * (1.0 + 0.08 * skull)

    # Along the neck the one turns into the other.
    foot_of_neck = np.array(NECK_LINE[0])
    up_neck = np.array(NECK_LINE[1]) - foot_of_neck
    along = ((x - foot_of_neck[0]) * up_neck[0] + (y - foot_of_neck[1]) * up_neck[1]) / up_neck.dot(up_neck)
    w = ease((along - 0.15) / 0.80)
    return np.stack([bx * (1.0 - w) + head_x * w, by * (1.0 - w) + head_y * w, z * (1.0 - w) + head_z * w], axis=1)


def _line(points, carry):
    """A line down the middle of a leg, as (heights, how far along the body), carried onto the corrected leg."""
    moved = sorted(carry(x, y)[::-1] for y, x in points)
    return [p[0] for p in moved], [p[1] for p in moved]


FRONT_LEG_LINE = _line(zip([0.058, 0.125, 0.220, 0.330, 0.395, 0.470, 0.560], [0.185, 0.179, 0.175, 0.172, 0.178, 0.205, 0.216]), front)
HIND_LEG_LINE = _line(zip([0.056, 0.120, 0.186, 0.218, 0.292, 0.362, 0.420, 0.500, 0.600], [-0.412, -0.420, -0.426, -0.385, -0.300, -0.242, -0.256, -0.285, -0.300]), hind)
TAIL_LINE = [(x + TAIL_SHIFT, y) for x, y in [(-0.425, 0.632), (-0.500, 0.630), (-0.600, 0.600), (-0.665, 0.580), (-0.730, 0.555), (-0.800, 0.528), (-0.865, 0.505), (-1.2, 0.38)]]
NECK_LINE = [(0.190, 0.555), (HEAD_AT[0] + 0.018, HEAD_AT[1] - 0.008)]     # from the shoulders to the back of the skull
PAWS = {"front": (0.198 + FRONT_PAW_SHIFT, 0.0), "hind": (-0.392 + HIND_PAW_SHIFT, 0.0)}   # where each sole meets the ground (x, y)

# ---- skeleton: name, parent, where the joint is -------------------------------------------
def bones(young=False):
    def head(x, y, z=0.0):
        return tilt((x, y, z))

    found = [
        ("hips", None, (-0.30, 0.60, 0.0)),
        ("chest", "hips", (0.06, 0.60, 0.0)),
        ("neck", "chest", (0.25, 0.64, 0.0)),
        ("head", "neck", HEAD_AT + (0.0,)),
        ("muzzle", "head", head(0.545, 0.845)),
        ("jaw", "head", head(0.475, 0.800)),
        ("ear_l", "head", head(0.452, 0.920, 0.068)),
        ("ear_r", "head", head(0.452, 0.920, -0.068)),
        ("tail1", "hips", (-0.47 + TAIL_SHIFT, 0.636, 0.0)),
        ("tail2", "tail1", (-0.60 + TAIL_SHIFT, 0.600, 0.0)),
        ("tail3", "tail2", (-0.73 + TAIL_SHIFT, 0.555, 0.0)),
    ]
    for is_front, name, parent, joint, z in (
            (True, "upper", "chest", FRONT_CHAIN[1], 0.088), (True, "lower", "upper", FRONT_CHAIN[2], 0.088),
            (False, "upper", "hips", HIND_CHAIN[1], 0.092), (False, "lower", "upper", HIND_CHAIN[2], 0.092)):
        for side, sign in (("l", 1), ("r", -1)):
            leg = ("front_" if is_front else "hind_") + side
            found.append((f"{leg}_{name}", parent if name == "upper" else f"{leg}_{parent}", (joint[0], joint[1], z * sign)))
    # the toes of each paw, which stay flat on the ground whatever the leg above them does,
    # and the long foot between a hind leg's hock and its toes
    for side, sign in (("l", 1), ("r", -1)):
        found.append((f"front_{side}_paw", f"front_{side}_lower", (FRONT_CHAIN[3][0], FRONT_CHAIN[3][1], 0.088 * sign)))
    for side, sign in (("l", 1), ("r", -1)):
        found.append((f"hind_{side}_foot", f"hind_{side}_lower", (HIND_CHAIN[3][0], HIND_CHAIN[3][1], 0.092 * sign)))
    for side, sign in (("l", 1), ("r", -1)):
        found.append((f"hind_{side}_paw", f"hind_{side}_foot", (HIND_CHAIN[4][0], HIND_CHAIN[4][1], 0.092 * sign)))
    if young:
        found = [(name, parent, tuple(as_puppy(at)[0])) for name, parent, at in found]
    return found


BONES = bones()
BONE_INDEX = {name: i for i, (name, _, _) in enumerate(BONES)}


def longer(length):
    """The coat as first measured was that of a short-coated dog; this is the Golden's."""
    return length * (1.0 if length < 0.012 else 1.35 if length < 0.045 else 1.5)


def cut(x, y, up, down, wide, coat, tone, bones, z=0.0, square=2.0):
    """One cross-section. `up`, `down` and `wide` are how far the skin is from the
    path on the part's upper side, its lower side and to each side. `coat` is the
    length of the coat on the upper side, the flanks and the lower side; `tone`
    its colour in the same three places; `bones` says what moves this section."""
    return {"at": (x, y, z), "up": up, "down": down, "wide": wide, "coat": tuple(longer(c) for c in coat), "tone": tone, "bones": bones, "square": square}


def trunk():
    c, h = {"chest": 1.0}, {"hips": 1.0}
    return {"name": "trunk", "face": (0, 1, 0), "caps": (0.9, 0.8), "cuts": [
        cut(0.300, 0.560, 0.100, 0.105, 0.078, (0.045, 0.050, 0.065), (GOLD, GOLD, LIGHT), c),
        cut(0.240, 0.575, 0.125, 0.168, 0.106, (0.036, 0.040, 0.066), (GOLD, GOLD, LIGHT), c),
        cut(0.170, 0.580, 0.135, 0.214, 0.122, (0.032, 0.036, 0.058), (DEEP, GOLD, LIGHT), c, square=2.15),
        cut(0.080, 0.580, 0.128, 0.230, 0.132, (0.030, 0.036, 0.056), (DEEP, GOLD, LIGHT), {"chest": 0.6, "hips": 0.4}, square=2.2),
        cut(-0.020, 0.585, 0.117, 0.228, 0.134, (0.030, 0.036, 0.062), (DEEP, GOLD, LIGHT), {"chest": 0.25, "hips": 0.75}, square=2.2),
        cut(-0.120, 0.590, 0.108, 0.212, 0.134, (0.030, 0.036, 0.068), (DEEP, GOLD, LIGHT), h, square=2.1),
        cut(-0.210, 0.595, 0.100, 0.182, 0.117, (0.030, 0.036, 0.066), (DEEP, GOLD, LIGHT), h),
        cut(rump(-0.300), 0.595, 0.098, 0.165, 0.112, (0.030, 0.036, 0.050), (DEEP, GOLD, LIGHT), h),
        cut(rump(-0.380), 0.590, 0.098, 0.130, 0.100, (0.032, 0.040, 0.060), (DEEP, GOLD, LIGHT), h),
        cut(rump(-0.450), 0.580, 0.075, 0.098, 0.072, (0.036, 0.050, 0.075), (GOLD, LIGHT, CREAM), h),
    ]}


def neck():
    """An upright, muscular neck from the shoulders to the back of the skull."""
    bottom = np.array((0.190, 0.555))
    top = np.array((HEAD_AT[0] + 0.018, HEAD_AT[1] - 0.008))
    along = (top - bottom) / np.linalg.norm(top - bottom)
    at = lambda t: tuple(bottom + (top - bottom) * t)
    return {"name": "neck", "face": (-along[1], along[0], 0), "caps": (0.6, 0.6), "cuts": [
        cut(*at(1.0), 0.078, 0.088, 0.080, (0.026, 0.032, 0.044), (GOLD, GOLD, LIGHT), {"head": 0.75, "neck": 0.25}),
        cut(*at(0.72), 0.096, 0.110, 0.098, (0.036, 0.046, 0.074), (GOLD, GOLD, LIGHT), {"neck": 1.0}),
        cut(*at(0.45), 0.108, 0.126, 0.106, (0.040, 0.050, 0.080), (GOLD, GOLD, LIGHT), {"neck": 1.0}),
        cut(*at(0.20), 0.108, 0.124, 0.104, (0.038, 0.044, 0.070), (DEEP, GOLD, LIGHT), {"neck": 0.5, "chest": 0.5}),
        cut(*at(0.0), 0.110, 0.120, 0.105, (0.034, 0.040, 0.066), (DEEP, GOLD, LIGHT), {"chest": 1.0}),
    ]}


def skull():
    m, h = {"muzzle": 1.0}, {"head": 1.0}
    short = (0.004, 0.004, 0.005)
    cuts = [
        cut(0.662, 0.836, 0.038, 0.048, 0.043, short, (FACE, FACE, FACE), m, square=2.9),
        cut(0.630, 0.838, 0.043, 0.055, 0.049, short, (FACE, FACE, FACE), m, square=2.8),
        cut(0.596, 0.842, 0.046, 0.058, 0.053, (0.005, 0.005, 0.006), (FACE, FACE, FACE), m, square=2.6),
        cut(0.566, 0.847, 0.052, 0.062, 0.058, (0.007, 0.008, 0.010), (GOLD, FACE, FACE), {"muzzle": 0.5, "head": 0.5}, square=2.4),
        cut(0.542, 0.853, 0.072, 0.068, 0.068, (0.010, 0.011, 0.014), (GOLD, GOLD, GOLD), h),
        cut(0.515, 0.858, 0.084, 0.078, 0.082, (0.012, 0.014, 0.018), (GOLD, GOLD, GOLD), h),
        cut(0.470, 0.860, 0.090, 0.086, 0.092, (0.016, 0.019, 0.026), (GOLD, GOLD, LIGHT), h),
        cut(0.428, 0.857, 0.080, 0.080, 0.080, (0.021, 0.025, 0.032), (GOLD, GOLD, LIGHT), h),
        cut(0.398, 0.850, 0.060, 0.064, 0.062, (0.025, 0.029, 0.034), (GOLD, GOLD, LIGHT), h),
    ]
    return {"name": "skull", "face": (0, 1, 0), "caps": (0.45, 0.8), "cuts": cuts, "tilt": True}


def jaw():
    j = {"jaw": 1.0}
    short = (0.004, 0.006, 0.010)
    return {"name": "jaw", "face": (0, 1, 0), "caps": (0.7, 0.6), "apart": True, "tilt": True, "cuts": [
        cut(0.645, 0.784, 0.012, 0.018, 0.030, short, (FACE, FACE, LIGHT), j),
        cut(0.615, 0.783, 0.014, 0.023, 0.036, short, (FACE, FACE, LIGHT), j),
        cut(0.575, 0.783, 0.015, 0.027, 0.040, short, (FACE, FACE, LIGHT), j),
        cut(0.530, 0.787, 0.016, 0.030, 0.045, (0.004, 0.008, 0.014), (FACE, FACE, LIGHT), j),
        cut(0.488, 0.798, 0.016, 0.030, 0.046, (0.004, 0.010, 0.016), (FACE, GOLD, LIGHT), j),
    ]}


def ear(sign):
    s = "l" if sign > 0 else "r"
    e = {f"ear_{s}": 1.0}
    coat = (0.030, 0.026, 0.030)
    tone = (DEEP, DEEP, DEEP)
    return {"name": f"ear_{s}", "face": (-1, 0, 0), "caps": (0.5, 0.9), "own": True, "tilt": True, "cuts": [
        cut(0.450, 0.922, 0.024, 0.024, 0.016, (0.016, 0.016, 0.016), tone, {"head": 0.7, f"ear_{s}": 0.3}, z=0.070 * sign),
        cut(0.453, 0.892, 0.040, 0.038, 0.0150, coat, tone, e, z=0.090 * sign),
        cut(0.459, 0.850, 0.047, 0.042, 0.0140, coat, tone, e, z=0.098 * sign),
        cut(0.466, 0.806, 0.039, 0.035, 0.0125, coat, tone, e, z=0.099 * sign),
        cut(0.472, 0.776, 0.020, 0.020, 0.0110, coat, tone, e, z=0.096 * sign),
    ]}


def front_leg(sign):
    s = "l" if sign > 0 else "r"
    z = 0.088 * sign
    up, low = f"front_{s}_upper", f"front_{s}_lower"
    leg = {"name": f"front_{s}", "face": (-1, 0, 0), "caps": (0.6, 0.5), "cuts": [
        cut(0.216, 0.560, 0.085, 0.070, 0.046, (0.036, 0.036, 0.040), (GOLD, GOLD, GOLD), {"chest": 1.0}, z=z * 0.78),
        cut(0.205, 0.470, 0.075, 0.060, 0.046, (0.042, 0.034, 0.036), (GOLD, GOLD, LIGHT), {up: 0.7, "chest": 0.3}, z=z * 0.88),
        cut(0.178, 0.395, 0.062, 0.047, 0.045, (0.056, 0.022, 0.016), (LIGHT, GOLD, GOLD), {up: 1.0}, z=z),
        cut(0.172, 0.330, 0.055, 0.049, 0.046, (0.066, 0.026, 0.016), (LIGHT, GOLD, GOLD), {low: 1.0}, z=z),
        cut(0.175, 0.220, 0.045, 0.041, 0.040, (0.060, 0.024, 0.015), (CREAM, GOLD, GOLD), {low: 1.0}, z=z),
        cut(0.179, 0.125, 0.037, 0.037, 0.036, (0.034, 0.019, 0.013), (LIGHT, GOLD, GOLD), {low: 1.0}, z=z),
        cut(0.185, 0.058, 0.035, 0.034, 0.036, (0.014, 0.011, 0.009), (GOLD, GOLD, GOLD), {low: 1.0}, z=z),
    ]}
    paw = {"name": f"front_paw_{s}", "face": (0, 1, 0), "caps": (0.8, 0.7), "cuts": [
        cut(0.168, 0.033, 0.030, 0.030, 0.039, (0.010, 0.010, 0.004), (GOLD, GOLD, GOLD), {f"front_{s}_paw": 1.0}, z=z, square=2.3),
        cut(0.206, 0.032, 0.031, 0.029, 0.047, (0.010, 0.010, 0.004), (GOLD, GOLD, GOLD), {f"front_{s}_paw": 1.0}, z=z, square=2.4),
        cut(0.240, 0.027, 0.023, 0.024, 0.045, (0.009, 0.009, 0.004), (GOLD, GOLD, GOLD), {f"front_{s}_paw": 1.0}, z=z, square=2.5),
    ]}
    return [carried(leg, front), shifted(paw, FRONT_PAW_SHIFT)]


def hind_leg(sign):
    s = "l" if sign > 0 else "r"
    z = 0.092 * sign
    up, low, foot = f"hind_{s}_upper", f"hind_{s}_lower", f"hind_{s}_foot"
    leg = {"name": f"hind_{s}", "face": (-1, 0, 0), "caps": (0.6, 0.5), "cuts": [
        cut(-0.300, 0.600, 0.105, 0.100, 0.060, (0.040, 0.038, 0.036), (GOLD, GOLD, GOLD), {"hips": 1.0}, z=z * 0.85),
        cut(-0.285, 0.500, 0.112, 0.104, 0.062, (0.070, 0.036, 0.034), (LIGHT, GOLD, GOLD), {up: 0.65, "hips": 0.35}, z=z * 0.95),
        cut(-0.256, 0.420, 0.094, 0.084, 0.055, (0.088, 0.032, 0.030), (CREAM, GOLD, GOLD), {up: 1.0}, z=z),
        cut(-0.242, 0.362, 0.066, 0.060, 0.045, (0.080, 0.028, 0.022), (CREAM, GOLD, GOLD), {up: 0.5, low: 0.5}, z=z),
        cut(-0.300, 0.292, 0.056, 0.052, 0.045, (0.060, 0.030, 0.020), (LIGHT, GOLD, GOLD), {low: 1.0}, z=z),
        cut(-0.385, 0.218, 0.041, 0.041, 0.037, (0.040, 0.024, 0.016), (LIGHT, GOLD, GOLD), {low: 1.0}, z=z),
        cut(-0.426, 0.186, 0.037, 0.038, 0.035, (0.026, 0.018, 0.014), (GOLD, GOLD, GOLD), {low: 0.5, foot: 0.5}, z=z),
        cut(-0.420, 0.120, 0.033, 0.034, 0.033, (0.020, 0.016, 0.013), (GOLD, GOLD, GOLD), {foot: 1.0}, z=z),
        cut(-0.412, 0.056, 0.034, 0.034, 0.035, (0.016, 0.013, 0.011), (GOLD, GOLD, GOLD), {foot: 1.0}, z=z),
    ]}
    paw = {"name": f"hind_paw_{s}", "face": (0, 1, 0), "caps": (0.8, 0.7), "cuts": [
        cut(-0.432, 0.032, 0.029, 0.029, 0.037, (0.010, 0.010, 0.004), (GOLD, GOLD, GOLD), {f"hind_{s}_paw": 1.0}, z=z, square=2.3),
        cut(-0.394, 0.031, 0.030, 0.028, 0.045, (0.010, 0.010, 0.004), (GOLD, GOLD, GOLD), {f"hind_{s}_paw": 1.0}, z=z, square=2.4),
        cut(-0.360, 0.026, 0.022, 0.023, 0.042, (0.009, 0.009, 0.004), (GOLD, GOLD, GOLD), {f"hind_{s}_paw": 1.0}, z=z, square=2.5),
    ]}
    return [carried(leg, hind), shifted(paw, HIND_PAW_SHIFT)]


def carried(part, carry):
    """A part described against the legs as first drawn, carried onto the corrected skeleton."""
    for c in part["cuts"]:
        x, y = carry(c["at"][0], c["at"][1])
        c["at"] = (x, y, c["at"][2])
    return part


def shifted(part, dx):
    for c in part["cuts"]:
        c["at"] = (c["at"][0] + dx, c["at"][1], c["at"][2])
    return part


def tail():
    t1, t2, t3 = {"tail1": 1.0}, {"tail2": 1.0}, {"tail3": 1.0}
    return {"name": "tail", "face": (0, 1, 0), "caps": (0.5, 1.0), "cuts": [
        cut(-0.425 + TAIL_SHIFT, 0.632, 0.050, 0.054, 0.050, (0.036, 0.046, 0.070), (GOLD, GOLD, LIGHT), {"hips": 1.0}),
        cut(-0.500 + TAIL_SHIFT, 0.630, 0.040, 0.042, 0.040, (0.040, 0.052, 0.095), (GOLD, GOLD, LIGHT), t1),
        cut(-0.600 + TAIL_SHIFT, 0.600, 0.034, 0.036, 0.034, (0.042, 0.056, 0.125), (GOLD, LIGHT, CREAM), {"tail1": 0.5, "tail2": 0.5}),
        cut(-0.665 + TAIL_SHIFT, 0.580, 0.029, 0.031, 0.029, (0.042, 0.056, 0.130), (GOLD, LIGHT, CREAM), t2),
        cut(-0.730 + TAIL_SHIFT, 0.555, 0.024, 0.026, 0.024, (0.040, 0.054, 0.120), (GOLD, LIGHT, CREAM), {"tail2": 0.5, "tail3": 0.5}),
        cut(-0.800 + TAIL_SHIFT, 0.528, 0.019, 0.020, 0.019, (0.038, 0.050, 0.100), (GOLD, LIGHT, CREAM), t3),
        cut(-0.865 + TAIL_SHIFT, 0.505, 0.014, 0.014, 0.014, (0.034, 0.042, 0.070), (LIGHT, LIGHT, CREAM), t3),
    ]}


PARTS = [trunk(), neck(), skull(), jaw(), tail(), ear(1), ear(-1)] + front_leg(1) + front_leg(-1) + hind_leg(1) + hind_leg(-1)

# Rounded lumps added to the surface: (centre, radii, on the head?)
LUMPS = [((0.305, 0.525, 0.0), (0.045, 0.055, 0.055), False)]            # the breastbone
for _s in (1, -1):
    LUMPS += [
        (front(0.215, 0.520) + (0.078 * _s,), (0.060, 0.085, 0.030), False),      # shoulder
        ((0.505, 0.818, 0.056 * _s), (0.040, 0.030, 0.020), True),       # cheek
        ((0.545, 0.906, 0.036 * _s), (0.020, 0.010, 0.018), True),       # brow
        ((0.600, 0.792, 0.040 * _s), (0.042, 0.014, 0.009), True),       # the upper lip hanging over the jaw
    ]
    for _x, _z in ((0.248 + FRONT_PAW_SHIFT, 0.088), (-0.352 + HIND_PAW_SHIFT, 0.092)):
        for _off in (-0.035, -0.012, 0.012, 0.035):                       # four toes on every paw
            _reach = 0.0 if abs(_off) > 0.02 else 0.010
            LUMPS.append(((_x + _reach, 0.026, (_z + _off) * _s), (0.026, 0.022, 0.0125), False))

NOSE = {"at": (0.668, 0.846, 0.0), "radii": (0.016, 0.019, 0.024), "nostril": ((0.681, 0.841, 0.011), (0.0075, 0.0055, 0.006))}
EYES = {"from": (0.500, 0.872, 0.022), "towards": (0.95, 0.04, 0.30), "radius": 0.0150, "opening": (0.0145, 0.0092, 0.020)}


def head_landmarks():
    """The points of the head that anatomy is measured between, as the head is described
    (before tilt()): the occiput (the top of the back of the skull), the stop (where the
    forehead meets the muzzle, on the top line) and the tip of the nose."""
    cuts = skull()["cuts"]
    top = [(c["at"][0], c["at"][1] + c["up"]) for c in cuts]     # the top line, front to back
    occiput = top[-1]
    stop_y = float(np.interp(STOP, [t[0] for t in reversed(top)], [t[1] for t in reversed(top)]))
    nose = (NOSE["at"][0] + NOSE["radii"][0], NOSE["at"][1])
    return {"occiput": occiput + (0.0,), "stop": (STOP, stop_y, 0.0), "nose_tip": nose + (0.0,)}


def young(part):
    """The same part on the puppy. Its shape comes from as_puppy(); what is said here is his
    coat, which is an even fluff: no feathering behind the legs, no plume, no ruff."""
    name = part["name"]
    cuts = [dict(c) for c in part["cuts"]]
    for c in cuts:
        y = c["at"][1]
        if name == "trunk":
            c["coat"] = PUP_FLUFF
        elif name == "neck":
            c["coat"] = (0.046, 0.052, 0.058)         # a soft ruff
        elif name == "tail":
            c["coat"] = (0.050, 0.056, 0.062)         # a fluffy little plume
        elif name.startswith(("front_paw", "hind_paw")):
            c["coat"] = (0.028, 0.024, 0.006)         # fluffy feet, bare pads
        elif name.startswith(("front_", "hind_")):
            c["coat"] = PUP_FLUFF if y > 0.45 else (0.050, 0.046, 0.040) if y > 0.10 else (0.034, 0.030, 0.024)
        elif name.startswith("ear"):
            c["coat"] = tuple(min(v, 0.030) for v in c["coat"])
    return dict(part, cuts=cuts)


# ---- turning the cross-sections into meshes ------------------------------------------------
def catmull(p0, p1, p2, p3, t):
    return 0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t + (-p0 + 3 * p1 - 3 * p2 + p3) * t * t * t)


def loft(part, index, steps=None):
    """The mesh of one part, with what is known about each of its points. `steps` (how
    finely each stretch between two cross-sections is sampled) is given when the puppy's
    version is built, so that it has the adult's points one for one."""
    cuts = part["cuts"]
    tilted = part.get("tilt", False)
    taken = []
    rows = []
    for c in cuts:
        at = tilt(c["at"]) if tilted else c["at"]
        size = HEAD_SIZE if tilted else 1.0
        rows.append(list(at) + [c["up"] * size, c["down"] * size, c["wide"] * size, c["square"]] + list(c["coat"]) + [v for tone in c["tone"] for v in tone])
    rows = np.array(rows, dtype=float)
    face = np.array(part["face"], dtype=float)
    if tilted:
        face[:2] = (face[0] * math.cos(HEAD_TILT) - face[1] * math.sin(HEAD_TILT), face[0] * math.sin(HEAD_TILT) + face[1] * math.cos(HEAD_TILT))
    face /= np.linalg.norm(face)

    # Sample the path finely, section values following smoothly, bone weights linearly.
    samples, weights = [], []
    for i in range(len(cuts) - 1):
        a, b = rows[max(i - 1, 0)], rows[min(i + 2, len(cuts) - 1)]
        count = steps[i] if steps else max(2, int(math.ceil(np.linalg.norm(rows[i + 1, :3] - rows[i, :3]) / SPACING)))
        taken.append(count)
        for k in range(count + (1 if i == len(cuts) - 2 else 0)):
            t = k / count
            samples.append(catmull(a, rows[i], rows[i + 1], b, t))
            w = np.zeros(len(BONES))
            for name, amount in cuts[i]["bones"].items():
                w[BONE_INDEX[name]] += amount * (1 - t)
            for name, amount in cuts[i + 1]["bones"].items():
                w[BONE_INDEX[name]] += amount * t
            weights.append(w)
    samples = np.array(samples)
    centres = samples[:, :3]
    tangents = np.gradient(centres, axis=0)
    tangents /= np.linalg.norm(tangents, axis=1)[:, None]
    along = np.concatenate([[0.0], np.cumsum(np.linalg.norm(np.diff(centres, axis=0), axis=1))])

    verts, info, rings = [], [], []

    def ring(centre, tangent, row, weight, u, shrink):
        n = face - tangent * face.dot(tangent)
        n /= np.linalg.norm(n)
        b = np.cross(tangent, n)
        up, down, wide, square = row[3] * shrink, row[4] * shrink, row[5] * shrink, row[6]
        coat, tones = row[7:10], row[10:19].reshape(3, 3)
        start = len(verts)
        for k in range(RING):
            angle = 2 * math.pi * k / RING
            s, c = math.sin(angle), math.cos(angle)
            rn = (up if s >= 0 else down) * math.copysign(abs(s) ** (2 / square), s)
            rb = wide * math.copysign(abs(c) ** (2 / square), c)
            verts.append(centre + n * rn + b * rb)
            # How much this point belongs to the upper side, the flank, the lower side.
            top, under = max(s, 0.0) ** 1.5, max(-s, 0.0) ** 1.5
            flank = 1.0 - top - under
            info.append({
                "coat": top * coat[0] + flank * coat[1] + under * coat[2],
                "tone": top * tones[0] + flank * tones[1] + under * tones[2],
                "weights": weight, "flow": tangent,
                "grain": (u + index * 3.1, rn, rb),
            })
        rings.append(start)

    def cap(end, sign, roundness):
        row, centre, tangent = samples[end], centres[end], tangents[end]
        reach = min(row[3], row[4], row[5]) * roundness
        order = []
        for k in (1, 2, 3):
            angle = k / 4 * math.pi / 2
            order.append((centre + tangent * sign * reach * math.sin(angle), math.cos(angle)))
        return order, centre + tangent * sign * reach

    first, tip_first = cap(0, -1, part["caps"][0])
    for centre, shrink in reversed(first):
        ring(centre, tangents[0], samples[0], weights[0], along[0], shrink)
    for i in range(len(samples)):
        ring(centres[i], tangents[i], samples[i], weights[i], along[i], 1.0)
    last, tip_last = cap(len(samples) - 1, 1, part["caps"][1])
    for centre, shrink in last:
        ring(centre, tangents[-1], samples[-1], weights[-1], along[-1], shrink)

    faces = []
    for r in range(len(rings) - 1):
        a, b = rings[r], rings[r + 1]
        for k in range(RING):
            faces.append((a + k, a + (k + 1) % RING, b + (k + 1) % RING, b + k))
    for tip, start, flip in ((tip_first, rings[0], True), (tip_last, rings[-1], False)):
        pole = len(verts)
        verts.append(tip)
        info.append(dict(info[start]))
        for k in range(RING):
            faces.append((pole, start + (k + 1) % RING, start + k) if flip else (pole, start + k, start + (k + 1) % RING))
    return [tuple(v) for v in verts], faces, info, taken


def new_object(name, verts, faces):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(mesh)
    bm.free()
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def ellipsoid(name, centre, radii, turn=None, segments=24):
    verts, faces = [], []
    rings = segments // 2
    for i in range(1, rings):
        a = math.pi * i / rings
        for k in range(segments):
            b = 2 * math.pi * k / segments
            verts.append(Vector((math.cos(a), math.sin(a) * math.cos(b), math.sin(a) * math.sin(b))))
    for i in range(rings - 2):
        for k in range(segments):
            faces.append((i * segments + k, i * segments + (k + 1) % segments, (i + 1) * segments + (k + 1) % segments, (i + 1) * segments + k))
    verts += [Vector((1, 0, 0)), Vector((-1, 0, 0))]
    for k in range(segments):
        faces.append((len(verts) - 2, (k + 1) % segments, k))
        faces.append((len(verts) - 1, (rings - 2) * segments + k, (rings - 2) * segments + (k + 1) % segments))
    scale = Matrix.Diagonal(Vector(radii)).to_4x4()
    placed = Matrix.Translation(Vector(centre)) @ (turn.to_4x4() if turn else Matrix.Identity(4)) @ scale
    return new_object(name, [tuple(placed @ v) for v in verts], faces)


def only(obj):
    for other in bpy.context.view_layer.objects:
        other.select_set(False)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj


def modify(obj, kind, **settings):
    only(obj)
    modifier = obj.modifiers.new(kind.lower(), kind)
    for key, value in settings.items():
        setattr(modifier, key, value)
    bpy.ops.object.modifier_apply(modifier=modifier.name)


def join(objects, name):
    for other in bpy.context.view_layer.objects:
        other.select_set(False)
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.join()
    objects[0].name = name
    return objects[0]


def tree_of(obj):
    mesh = obj.data
    return BVHTree.FromPolygons([v.co for v in mesh.vertices], [p.vertices for p in mesh.polygons])


def smoothed(values, edges, degree, rounds, keep=0.5):
    for _ in range(rounds):
        total = np.zeros_like(values)
        np.add.at(total, edges[:, 0], values[edges[:, 1]])
        np.add.at(total, edges[:, 1], values[edges[:, 0]])
        values = keep * values + (1 - keep) * total / degree.reshape((-1,) + (1,) * (values.ndim - 1))
    return values


def describe(obj, lofts, body_tree=None, other_tree=None, bare=None):
    """For every point of a finished mesh: bones, coat, colour, the lie of the coat, light."""
    mesh = obj.data
    mesh.update()
    count = len(mesh.vertices)
    where = np.array([v.co[:] for v in mesh.vertices])
    normal = np.array([v.normal[:] for v in mesh.vertices])
    edges = np.array([e.vertices[:] for e in mesh.edges])
    degree = np.zeros(count)
    np.add.at(degree, edges[:, 0], 1)
    np.add.at(degree, edges[:, 1], 1)
    degree = np.maximum(degree, 1)

    trees = []
    for entry in lofts:
        verts = entry[1]
        kd = KDTree(len(verts))
        for i, v in enumerate(verts):
            kd.insert(v, i)
        kd.balance()
        trees.append(kd)

    weights = np.zeros((count, len(BONES)))
    coat, tone, flow, grain = np.zeros(count), np.zeros((count, 3)), np.zeros((count, 3)), np.zeros((count, 4))
    owners = np.zeros(count, dtype=int)
    nearest = np.zeros((count, len(lofts)), dtype=int)     # for every point, the closest point of every part
    young_coat = np.zeros(count)

    def grain_from(i, part):
        """Where point i is along and around `part`, and how squarely it faces down that part's length."""
        there = lofts[part][2][nearest[i, part]]
        return list(there["grain"]) + [abs(float(np.dot(normal[i], there["flow"])))]

    for i in range(count):
        found = [kd.find(where[i]) for kd in trees]
        nearest[i] = [f[1] for f in found]
        owner = min(range(len(found)), key=lambda p: found[p][2])
        owners[i] = owner
        if lofts[0][0]["name"] == "trunk" and found[0][2] < 0.034 and not lofts[owner][0].get("own"):
            owners[i] = 0      # close to the trunk, the pattern of the coat follows the trunk
        mine = lofts[owner][2][found[owner][1]]
        coat[i], tone[i], flow[i], grain[i] = mine["coat"], mine["tone"], lofts[owners[i]][2][found[owners[i]][1]]["flow"], grain_from(i, owners[i])
        young_coat[i] = lofts[owner][4][found[owner][1]]["coat"]
        if lofts[owner][0].get("own"):
            weights[i] = mine["weights"]
            continue
        for p, (_, at, distance) in enumerate(found):
            if not lofts[p][0].get("own"):
                weights[i] += math.exp(-(distance / 0.032) ** 2) * lofts[p][2][at]["weights"]

    weights = smoothed(weights, edges, degree, 3)
    coat = smoothed(coat, edges, degree, 4)
    tone = smoothed(tone, edges, degree, 4)
    flow = smoothed(flow, edges, degree, 2)
    skin = np.zeros(count)     # 1 where there is bare skin instead of coat
    shine = np.zeros(count)
    if bare is not None:
        for i in range(count):
            hit = bare(Vector(where[i]), Vector(normal[i]))
            if hit:
                skin[i], tone[i], shine[i] = hit[0], np.array(tone[i]) * (1 - hit[0]) + np.array(hit[1]) * hit[0], hit[2]
        skin = smoothed(skin, edges, degree, 1)
    coat *= np.clip(1.0 - skin * 1.6, 0.0, 1.0)
    young_coat = smoothed(young_coat, edges, degree, 4) * np.clip(1.0 - skin * 1.6, 0.0, 1.0)
    younger = as_puppy(where)
    younger[:, 1] = np.maximum(younger[:, 1], 0.003)
    mesh.vertices.foreach_set("co", younger.ravel())       # let Blender work out which way the puppy's surface faces
    mesh.update()
    young_normal = np.array([v.normal[:] for v in mesh.vertices])
    mesh.vertices.foreach_set("co", where.ravel())
    mesh.update()

    # The coat lies along the part, towards its far end, and hangs a little.
    flow = flow + np.array([0.0, -0.35, 0.0])
    flow -= normal * np.sum(flow * normal, axis=1)[:, None]
    length = np.linalg.norm(flow, axis=1)
    flow = np.where(length[:, None] > 1e-5, flow / np.maximum(length, 1e-5)[:, None], np.array([0.0, -1.0, 0.0]))

    # How open each point is to the sky: creases, the belly and the insides of the legs get less light.
    tree = body_tree or tree_of(obj)
    rays = []
    golden = math.pi * (3 - math.sqrt(5))
    for k in range(20):
        height = 1 - (k + 0.5) / 20 * 0.92
        radius = math.sqrt(1 - height * height)
        rays.append((radius * math.cos(golden * k), radius * math.sin(golden * k), height))
    light = np.ones(count)
    for i in range(count):
        n = Vector(normal[i])
        side = n.cross(Vector((0, 1, 0)) if abs(n.y) < 0.9 else Vector((1, 0, 0))).normalized()
        other = n.cross(side)
        start = Vector(where[i]) + n * 0.004
        hits = 0.0
        for a, b, c in rays:
            direction = side * a + other * b + n * c
            for t in (tree, other_tree):
                if t is None:
                    continue
                reached = t.ray_cast(start, direction, 0.30)
                if reached[0] is not None:
                    hits += c * (1.0 - reached[3] / 0.30) ** 0.5
                    break
            else:
                if direction.y < 0 and start.y / max(-direction.y, 1e-4) < 0.30:
                    hits += c * 0.5   # the ground
        light[i] = 1.0 - min(hits / sum(r[2] for r in rays), 1.0)
    light = smoothed(light, edges, degree, 2)

    order = np.argsort(-weights, axis=1)[:, :4]
    top = np.take_along_axis(weights, order, axis=1)
    top /= np.maximum(top.sum(axis=1), 1e-9)[:, None]

    # The pattern of the coat runs along each part. A triangle whose corners belong to
    # different parts (where a leg joins the trunk, say) would smear one pattern into
    # the other, so such a triangle takes its pattern from one part only: the corners
    # that belong elsewhere are copied, and the copies are placed on that part instead.
    triangles, copies, copied = [], {}, []
    for polygon in mesh.polygons:
        ring = polygon.vertices[:]
        parts = [owners[v] for v in ring]
        part = max(set(parts), key=parts.count)
        corners = []
        for v in ring:
            if owners[v] == part:
                corners.append(v)
                continue
            if (v, part) not in copies:
                copies[(v, part)] = count + len(copied)
                copied.append((v, grain_from(v, part)))
            corners.append(copies[(v, part)])
        for k in range(1, len(corners) - 1):
            triangles += [corners[0], corners[k + 1], corners[k]]    # the renderer's front faces wind the other way
    result = {"where": where, "normal": normal, "flow": flow, "grain": grain, "coat": coat, "tone": tone, "light": light,
              "skin": skin, "shine": shine, "bone_ids": order, "bone_weights": top,
              "young": younger, "young_normal": young_normal, "young_coat": young_coat}
    if copied:
        source = np.array([v for v, _ in copied])
        result = {key: np.concatenate([value, value[source]]) for key, value in result.items()}
        result["grain"][count:] = np.array([g for _, g in copied])
    result["triangles"] = triangles
    return result


def build(preview=None, write=True):
    """Builds the dog in the open Blender scene and, with `write`, saves the model file.
    Returns the finished pieces (body, jaw, nose), what is known about each of their
    points, and where the eyes are, for scripts that go on to render him."""
    started = time.time()
    bpy.ops.wm.read_factory_settings(use_empty=True)

    lofts, melted, apart = [], [], []
    for index, part in enumerate(PARTS):
        verts, faces, info, steps = loft(part, index)
        younger, _, young_info, _ = loft(young(part), index, steps)
        lofts.append((part, verts, info, younger, young_info, faces))
        (apart if part.get("apart") else melted).append(new_object(part["name"], verts, faces))
    for k, (centre, radii, on_head) in enumerate(LUMPS):
        melted.append(ellipsoid(f"lump{k}", tilt(centre) if on_head else centre, tuple(r * HEAD_SIZE for r in radii) if on_head else radii))
    body = join(melted, "body")
    modify(body, "REMESH", mode="VOXEL", voxel_size=VOXEL, adaptivity=0.0, use_smooth_shade=True)
    # The face, the ears and the paws keep their detail; the broad surfaces are smoothed and thinned more.
    group = body.vertex_groups.new(name="fine")
    broad = body.vertex_groups.new(name="broad")
    middle = Vector((HEAD_AT[0] + 0.13, HEAD_AT[1], 0.0))
    for v in body.data.vertices:
        near_head = max(0.0, 1.0 - (v.co - middle).length / 0.24)
        thin = 0.8 if v.co.x < -0.47 else 0.45 if v.co.y < 0.36 else 0.0     # the tail and the legs: thin parts under a long coat
        fine = min(1.0, near_head * 1.6 + (0.5 if v.co.y < 0.07 else 0.0) + thin)
        group.add([v.index], fine, "REPLACE")
        broad.add([v.index], 1.0 - min(1.0, fine * 2.0), "REPLACE")
    modify(body, "SMOOTH", factor=0.6, iterations=5)
    modify(body, "SMOOTH", factor=0.6, iterations=50, vertex_group="broad")
    print(f"melted: {len(body.data.vertices)} points, {time.time() - started:.0f} s")

    # The eyes: found by looking out from inside the skull, then set into sockets.
    tree = tree_of(body)
    eyes = []
    for sign in (1, -1):
        start = Vector(tilt((EYES["from"][0], EYES["from"][1], EYES["from"][2] * sign)))
        level = Vector((EYES["towards"][0], EYES["towards"][1], EYES["towards"][2] * sign)).normalized()
        gaze = (Vector(tilt((HEAD_DRAWN[0] + level.x, HEAD_DRAWN[1] + level.y, level.z))) - Vector(HEAD_AT + (0.0,))).normalized()
        surface = tree.ray_cast(start, gaze, 0.3)[0]
        across = gaze.cross(Vector((0, 1, 0))).normalized()
        upward = across.cross(gaze).normalized()
        frame = Matrix((gaze, upward, across)).transposed()   # columns: out of the eye, up, along the opening
        slant = Matrix.Rotation(math.radians(12.0 * sign), 3, "X")
        depth, tall, wide = (EYES["opening"][k] * HEAD_SIZE for k in (2, 1, 0))
        cutter = ellipsoid(f"socket{sign}", surface - gaze * 0.002, (depth, tall, wide), frame @ slant)
        modify(body, "BOOLEAN", operation="DIFFERENCE", object=cutter, solver="EXACT")
        bpy.data.objects.remove(cutter)
        centre = surface - gaze * (EYES["radius"] * HEAD_SIZE * 1.12)
        eyes.append({"at": centre, "surface": surface, "gaze": gaze})
    modify(body, "SMOOTH", factor=0.5, iterations=1)

    modify(body, "TRIANGULATE")
    modify(body, "DECIMATE", ratio=min(1.0, TRIANGLES / len(body.data.polygons)), vertex_group="fine", invert_vertex_group=True, vertex_group_factor=2.0)
    print(f"thinned: {len(body.data.vertices)} points, {len(body.data.polygons)} triangles, {time.time() - started:.0f} s")

    jaw_object = apart[0]
    modify(jaw_object, "TRIANGULATE")
    nose = ellipsoid("nose", tilt(NOSE["at"]), tuple(r * HEAD_SIZE for r in NOSE["radii"]), Matrix.Rotation(HEAD_TILT, 3, "Z"), segments=32)
    for sign in (1, -1):
        centre, radii = NOSE["nostril"]
        hole = ellipsoid("nostril", tilt((centre[0], centre[1], centre[2] * sign)), tuple(r * HEAD_SIZE for r in radii), Matrix.Rotation(HEAD_TILT, 3, "Z") @ Matrix.Rotation(math.radians(-25.0 * sign), 3, "Y"))
        modify(nose, "BOOLEAN", operation="DIFFERENCE", object=hole, solver="EXACT")
        bpy.data.objects.remove(hole)
    modify(nose, "REMESH", mode="VOXEL", voxel_size=0.0028, adaptivity=0.0, use_smooth_shade=True)
    modify(nose, "SMOOTH", factor=0.5, iterations=3)
    modify(nose, "TRIANGULATE")
    modify(nose, "DECIMATE", ratio=min(1.0, 900 / max(len(nose.data.polygons), 1)))

    body_tree, jaw_tree, nose_tree = tree_of(body), tree_of(jaw_object), tree_of(nose)

    def inside(tree, point, slack=0.0):
        at, normal, _, distance = tree.find_nearest(point)
        return at is not None and ((point - at).dot(normal) < 0.0 or distance < slack)

    def body_skin(point, normal):
        for eye in eyes:
            distance = (point - eye["surface"]).length
            if distance < 0.028:
                return (min(1.0, max(0.0, (0.028 - distance) / 0.010)), RIM, 0.2)
        if inside(jaw_tree, point, 0.002):
            return (1.0, PALATE if normal.y < 0.2 else RIM, 0.3)       # the roof of the mouth, hidden while it is shut
        if inside(nose_tree, point, 0.004):
            return (1.0, LEATHER, 0.6)
        if point.y < 0.012 and normal.y < -0.5:
            return (0.5, PAD, 0.0)                                      # the pads under the paws, with coat between them
        return None

    def jaw_skin(point, normal):
        if normal.y > -0.2 and inside(body_tree, point):
            return (1.0, GUMS if normal.y > 0.3 else RIM, 0.4)         # the floor of the mouth and the lower lips
        return None

    solid = [entry for entry in lofts if not entry[0].get("apart")]
    pieces = [
        describe(body, solid, body_tree, None, body_skin),
        describe(jaw_object, [entry for entry in lofts if entry[0].get("apart")], jaw_tree, body_tree, jaw_skin),
        describe(nose, solid, nose_tree, body_tree, lambda point, normal: (1.0, LEATHER, 1.0)),
    ]
    pieces[2]["bone_ids"][:] = [BONE_INDEX["muzzle"], 0, 0, 0]
    pieces[2]["bone_weights"][:] = [1.0, 0.0, 0.0, 0.0]
    pieces[2]["coat"][:] = 0.0
    pieces[2]["young_coat"][:] = 0.0
    print(f"described: {time.time() - started:.0f} s")

    out = {key: [] for key in ("vertices", "normals", "flow", "grain", "coat", "colors", "skin", "bone_ids", "bone_weights", "triangles", "young_vertices", "young_normals", "young_coat")}
    offset = 0
    for piece in pieces:
        n = len(piece["where"])
        out["vertices"] += np.round(piece["where"], 4).ravel().tolist()
        out["normals"] += np.round(piece["normal"], 3).ravel().tolist()
        out["flow"] += np.round(piece["flow"], 3).ravel().tolist()
        out["grain"] += np.round(piece["grain"], 4).ravel().tolist()
        out["coat"] += np.round(piece["coat"], 4).tolist()
        out["colors"] += np.round(np.concatenate([piece["tone"], piece["light"][:, None]], axis=1), 3).ravel().tolist()
        out["skin"] += np.round(np.stack([piece["skin"], piece["shine"]], axis=1), 2).ravel().tolist()
        out["bone_ids"] += piece["bone_ids"].ravel().tolist()
        out["bone_weights"] += np.round(piece["bone_weights"], 4).ravel().tolist()
        out["triangles"] += [t + offset for t in piece["triangles"]]
        out["young_vertices"] += np.round(piece["young"], 4).ravel().tolist()
        out["young_normals"] += np.round(piece["young_normal"], 3).ravel().tolist()
        out["young_coat"] += np.round(piece["young_coat"], 4).tolist()
        offset += n
    # The coat is drawn as layers lifted off the skin. Where it is short, few layers are
    # needed: put the points and triangles under the longest coat first, so that each
    # layer is simply the first so-many of both.
    coat = np.maximum(np.array(out["coat"]), np.array(out["young_coat"]))       # enough layers for either age
    shells = np.where(coat < 0.0008, 0, np.clip(np.ceil(coat / LAYER_GAP), 2, MAX_LAYERS)).astype(int)
    triangles = np.array(out["triangles"]).reshape(-1, 3)
    deepest = shells[triangles].max(axis=1)
    reach = np.zeros(offset, dtype=int)
    np.maximum.at(reach, triangles.ravel(), np.repeat(deepest, 3))
    points = np.argsort(-reach, kind="stable")
    moved = np.empty(offset, dtype=int)
    moved[points] = np.arange(offset)
    out["triangles"] = moved[triangles[np.argsort(-deepest, kind="stable")]].ravel().tolist()
    for key, width in (("vertices", 3), ("normals", 3), ("flow", 3), ("grain", 4), ("coat", 1), ("colors", 4), ("skin", 2), ("bone_ids", 4), ("bone_weights", 4), ("young_vertices", 3), ("young_normals", 3), ("young_coat", 1)):
        out[key] = np.array(out[key]).reshape(offset, width)[points].ravel().tolist()
    out["shells"] = shells[points].tolist()
    out["layers"] = [[int((reach >= k).sum()), int((deepest >= k).sum())] for k in range(1, MAX_LAYERS + 1)]
    print(f"coat layers: {sum(t for _, t in out['layers'])} triangles in all")

    head_joint = Vector(HEAD_AT + (0.0,))
    young_bones = bones(young=True)
    young_head, young_jaw = Vector(PUP_HEAD_AT + (0.0,)), Vector(young_bones[BONE_INDEX["jaw"]][2])
    puppy = {
        "bones": [[round(c, 4) for c in at] for _, _, at in young_bones],
        "paws": {key: [round(float(as_puppy((x, y, 0.0))[0][0]), 4), y] for key, (x, y) in PAWS.items()},
        "eyes": [{"at": [round(c, 4) for c in (Vector(as_puppy(tuple(e["at"]))[0]) - young_head)], "radius": round(EYES["radius"] * PUP_HEAD_SIZE, 4)} for e in eyes],
        "tongue": {"at": [round(c, 4) for c in (Vector(as_puppy(tilt((0.595, 0.805, 0.0)))[0]) - young_jaw)], "half": [0.046, 0.0058, 0.020]},
        "vertices": out.pop("young_vertices"), "normals": out.pop("young_normals"), "coat": out.pop("young_coat"),
    }
    dog = {
        "about": "Generated by tools/build_dog.py in Blender. Colours are as seen on screen (sRGB); the fourth colour value is how much light reaches the point. 'puppy' holds the same points, bones and coat at eight weeks.",
        "bones": [{"name": name, "parent": parent, "at": [round(c, 4) for c in at]} for name, parent, at in BONES],
        "paws": PAWS,
        "eyes": [{"at": [round(c, 4) for c in (e["at"] - head_joint)], "gaze": [round(c, 3) for c in e["gaze"]], "radius": round(EYES["radius"] * HEAD_SIZE, 4)} for e in eyes],
        "tongue": {"at": [round(c, 4) for c in (Vector(tilt((0.595, 0.805, 0.0))) - Vector(BONES[BONE_INDEX["jaw"]][2]))], "half": [0.066, 0.0065, 0.023]},
        "puppy": puppy,
        # Where tools/measure_dog.py measures the head from (the rest it finds on the surface).
        "landmarks": {
            "adult": {name: [round(c, 4) for c in tilt(at)] for name, at in head_landmarks().items()},
            "puppy": {name: [round(float(c), 4) for c in as_puppy(tilt(at))[0]] for name, at in head_landmarks().items()},
        },
        **out,
    }
    if not write:
        return [body, jaw_object, nose], pieces, eyes
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        json.dump(dog, f, separators=(",", ":"))
    print(f"wrote {OUT}: {offset} points, {len(out['triangles']) // 3} triangles, {os.path.getsize(OUT) // 1024} KB, {time.time() - started:.0f} s")

    if preview:
        pictures(preview, [body, jaw_object, nose], pieces, eyes)
    return [body, jaw_object, nose], pieces, eyes


def pictures(folder, objects, pieces, eyes):
    """Plain renders of the bare body, to judge its shape before any coat is on it."""
    os.makedirs(folder, exist_ok=True)
    scene = bpy.context.scene
    material = bpy.data.materials.new("paint")
    material.use_nodes = True
    nodes = material.node_tree.nodes
    shader = nodes["Principled BSDF"]
    paint = nodes.new("ShaderNodeAttribute")
    paint.attribute_name = "paint"
    material.node_tree.links.new(paint.outputs["Color"], shader.inputs["Base Color"])
    shader.inputs["Roughness"].default_value = 0.8
    for obj, piece in zip(objects, pieces):
        layer = obj.data.attributes.new("paint", "FLOAT_COLOR", "POINT")
        own = len(obj.data.vertices)       # a piece can hold copies of points beyond the mesh's own
        shown = np.concatenate([linear(piece["tone"][:own]) * piece["light"][:own, None], np.ones((own, 1))], axis=1)
        layer.data.foreach_set("color", shown.ravel())
        obj.data.materials.clear()
        obj.data.materials.append(material)
        for polygon in obj.data.polygons:
            polygon.use_smooth = True
    dark = bpy.data.materials.new("eye")
    dark.diffuse_color = (0.02, 0.012, 0.008, 1.0)
    for eye in eyes:
        ball = ellipsoid("eye", eye["at"], (EYES["radius"] * HEAD_SIZE,) * 3)
        ball.data.materials.append(dark)
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 24
    scene.render.film_transparent = True
    world = bpy.data.worlds.new("sky")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.55
    scene.world = world
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 3.0
    sun.rotation_euler = (math.radians(-50), math.radians(-25), 0.0)   # from above, in front and to the viewer's side
    scene.collection.objects.link(sun)
    camera = bpy.data.objects.new("camera", bpy.data.cameras.new("camera"))
    camera.data.type = "ORTHO"
    scene.collection.objects.link(camera)
    scene.camera = camera
    views = {   # what to look at, from how far round (degrees from his side) and above, how wide, picture size
        "side": ((-0.08, 0.48, 0.0), 0, 0, 1.75, (1100, 700)),
        "front": ((0.0, 0.48, 0.0), 90, 0, 1.2, (700, 700)),
        "top": ((-0.08, 0.5, 0.0), 0, 90, 1.75, (1100, 500)),
        "quarter": ((-0.05, 0.48, 0.0), 40, 8, 1.75, (1100, 700)),
        "head": ((HEAD_AT[0] + 0.13, HEAD_AT[1], 0.0), 0, 0, 0.50, (900, 700)),
        "face": ((HEAD_AT[0] + 0.13, HEAD_AT[1], 0.0), 55, 5, 0.50, (900, 700)),
    }
    for name, (target, around, above, scale, size) in views.items():
        turn = Matrix.Rotation(math.radians(around), 4, "Y") @ Matrix.Rotation(math.radians(-above), 4, "X")
        camera.matrix_world = Matrix.Translation(Vector(target) + turn.to_3x3() @ Vector((0, 0, 4.0))) @ turn
        camera.data.ortho_scale = scale
        scene.render.resolution_x, scene.render.resolution_y = size
        scene.render.filepath = os.path.join(folder, f"{name}.png")
        bpy.ops.render.render(write_still=True)
    print(f"pictures in {folder}")


if __name__ == "__main__":
    arguments = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    build(arguments[arguments.index("--preview") + 1] if "--preview" in arguments else None)

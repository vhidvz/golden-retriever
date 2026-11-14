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
HEAD_AT = (0.385, 0.825)           # … and then carried here, with the nose a little down
HEAD_TILT = math.radians(-7.0)
HEAD_SIZE = 1.18                   # … and made this much bigger: a Golden's head is broad and heavy


def tilt(p):
    """Carries a point of the head as described to where it is when he holds it naturally."""
    x, y = p[0] - HEAD_DRAWN[0], p[1] - HEAD_DRAWN[1]
    c, s = math.cos(HEAD_TILT) * HEAD_SIZE, math.sin(HEAD_TILT) * HEAD_SIZE
    return (HEAD_AT[0] + x * c - y * s, HEAD_AT[1] + x * s + y * c) + tuple(v * HEAD_SIZE for v in p[2:])


# ---- skeleton: name, parent, where the joint is -------------------------------------------
BONES = [
    ("hips", None, (-0.30, 0.60, 0.0)),
    ("chest", "hips", (0.06, 0.60, 0.0)),
    ("neck", "chest", (0.25, 0.64, 0.0)),
    ("head", "neck", HEAD_AT + (0.0,)),
    ("muzzle", "head", tilt((0.545, 0.845, 0.0))),
    ("jaw", "head", tilt((0.475, 0.800, 0.0))),
    ("ear_l", "head", tilt((0.452, 0.920, 0.068))),
    ("ear_r", "head", tilt((0.452, 0.920, -0.068))),
    ("tail1", "hips", (-0.47, 0.636, 0.0)),
    ("tail2", "tail1", (-0.60, 0.600, 0.0)),
    ("tail3", "tail2", (-0.73, 0.555, 0.0)),
    ("front_l_upper", "chest", (0.230, 0.510, 0.088)),
    ("front_l_lower", "front_l_upper", (0.172, 0.372, 0.088)),
    ("front_r_upper", "chest", (0.230, 0.510, -0.088)),
    ("front_r_lower", "front_r_upper", (0.172, 0.372, -0.088)),
    ("hind_l_upper", "hips", (-0.310, 0.550, 0.092)),
    ("hind_l_lower", "hind_l_upper", (-0.238, 0.362, 0.092)),
    ("hind_r_upper", "hips", (-0.310, 0.550, -0.092)),
    ("hind_r_lower", "hind_r_upper", (-0.238, 0.362, -0.092)),
    # the toes of each paw, which stay flat on the ground whatever the leg above them does,
    # and the long foot between a hind leg's hock and its toes
    ("front_l_paw", "front_l_lower", (0.178, 0.048, 0.088)),
    ("front_r_paw", "front_r_lower", (0.178, 0.048, -0.088)),
    ("hind_l_foot", "hind_l_lower", (-0.426, 0.186, 0.092)),
    ("hind_r_foot", "hind_r_lower", (-0.426, 0.186, -0.092)),
    ("hind_l_paw", "hind_l_foot", (-0.416, 0.048, 0.092)),
    ("hind_r_paw", "hind_r_foot", (-0.416, 0.048, -0.092)),
]
BONE_INDEX = {name: i for i, (name, _, _) in enumerate(BONES)}
PAWS = {"front": (0.198, 0.0), "hind": (-0.392, 0.0)}   # where each sole meets the ground (x, y)


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
        cut(0.240, 0.575, 0.125, 0.165, 0.106, (0.036, 0.040, 0.066), (GOLD, GOLD, LIGHT), c),
        cut(0.170, 0.580, 0.135, 0.212, 0.122, (0.032, 0.036, 0.058), (DEEP, GOLD, LIGHT), c, square=2.15),
        cut(0.080, 0.580, 0.128, 0.232, 0.132, (0.030, 0.036, 0.056), (DEEP, GOLD, LIGHT), {"chest": 0.6, "hips": 0.4}, square=2.2),
        cut(-0.020, 0.585, 0.117, 0.228, 0.134, (0.030, 0.036, 0.062), (DEEP, GOLD, LIGHT), {"chest": 0.25, "hips": 0.75}, square=2.2),
        cut(-0.120, 0.590, 0.108, 0.210, 0.134, (0.030, 0.036, 0.068), (DEEP, GOLD, LIGHT), h, square=2.1),
        cut(-0.210, 0.595, 0.100, 0.188, 0.117, (0.030, 0.036, 0.066), (DEEP, GOLD, LIGHT), h),
        cut(-0.300, 0.595, 0.098, 0.165, 0.112, (0.030, 0.036, 0.050), (DEEP, GOLD, LIGHT), h),
        cut(-0.380, 0.590, 0.098, 0.130, 0.100, (0.032, 0.040, 0.060), (DEEP, GOLD, LIGHT), h),
        cut(-0.450, 0.580, 0.075, 0.098, 0.072, (0.036, 0.050, 0.075), (GOLD, LIGHT, CREAM), h),
    ]}


def neck():
    top = (HEAD_AT[0] + 0.018, HEAD_AT[1] - 0.008)
    return {"name": "neck", "face": (-0.72, 0.69, 0), "caps": (0.6, 0.6), "cuts": [
        cut(top[0], top[1], 0.066, 0.074, 0.068, (0.026, 0.030, 0.036), (GOLD, GOLD, LIGHT), {"head": 0.75, "neck": 0.25}),
        cut(0.355, 0.760, 0.076, 0.086, 0.078, (0.036, 0.042, 0.058), (GOLD, GOLD, LIGHT), {"neck": 1.0}),
        cut(0.302, 0.690, 0.092, 0.104, 0.092, (0.040, 0.046, 0.068), (GOLD, GOLD, LIGHT), {"neck": 1.0}),
        cut(0.248, 0.618, 0.108, 0.124, 0.104, (0.038, 0.044, 0.070), (DEEP, GOLD, LIGHT), {"neck": 0.5, "chest": 0.5}),
        cut(0.190, 0.555, 0.110, 0.120, 0.105, (0.034, 0.040, 0.066), (DEEP, GOLD, LIGHT), {"chest": 1.0}),
    ]}


def skull():
    m, h = {"muzzle": 1.0}, {"head": 1.0}
    short = (0.004, 0.004, 0.005)
    cuts = [
        cut(0.678, 0.836, 0.034, 0.038, 0.040, short, (FACE, FACE, FACE), m, square=2.7),
        cut(0.640, 0.838, 0.037, 0.042, 0.044, short, (FACE, FACE, FACE), m, square=2.7),
        cut(0.600, 0.840, 0.039, 0.045, 0.047, (0.005, 0.005, 0.006), (FACE, FACE, FACE), m, square=2.6),
        cut(0.568, 0.844, 0.044, 0.050, 0.052, (0.007, 0.008, 0.010), (GOLD, FACE, FACE), {"muzzle": 0.5, "head": 0.5}, square=2.4),
        cut(0.542, 0.852, 0.066, 0.064, 0.066, (0.010, 0.011, 0.014), (GOLD, GOLD, GOLD), h),
        cut(0.515, 0.858, 0.082, 0.076, 0.078, (0.012, 0.014, 0.018), (GOLD, GOLD, GOLD), h),
        cut(0.470, 0.860, 0.088, 0.084, 0.086, (0.016, 0.019, 0.026), (GOLD, GOLD, LIGHT), h),
        cut(0.428, 0.857, 0.080, 0.080, 0.080, (0.021, 0.025, 0.032), (GOLD, GOLD, LIGHT), h),
        cut(0.398, 0.850, 0.060, 0.064, 0.062, (0.025, 0.029, 0.034), (GOLD, GOLD, LIGHT), h),
    ]
    return {"name": "skull", "face": (0, 1, 0), "caps": (0.45, 0.8), "cuts": cuts, "tilt": True}


def jaw():
    j = {"jaw": 1.0}
    short = (0.004, 0.006, 0.010)
    return {"name": "jaw", "face": (0, 1, 0), "caps": (0.7, 0.6), "apart": True, "tilt": True, "cuts": [
        cut(0.660, 0.787, 0.012, 0.018, 0.026, short, (FACE, FACE, LIGHT), j),
        cut(0.625, 0.786, 0.014, 0.022, 0.032, short, (FACE, FACE, LIGHT), j),
        cut(0.580, 0.786, 0.015, 0.026, 0.037, short, (FACE, FACE, LIGHT), j),
        cut(0.530, 0.789, 0.016, 0.029, 0.043, (0.004, 0.008, 0.014), (FACE, FACE, LIGHT), j),
        cut(0.488, 0.798, 0.016, 0.030, 0.046, (0.004, 0.010, 0.016), (FACE, GOLD, LIGHT), j),
    ]}


def ear(sign):
    s = "l" if sign > 0 else "r"
    e = {f"ear_{s}": 1.0}
    coat = (0.030, 0.026, 0.030)
    tone = (DEEP, DEEP, DEEP)
    return {"name": f"ear_{s}", "face": (-1, 0, 0), "caps": (0.5, 0.9), "own": True, "tilt": True, "cuts": [
        cut(0.452, 0.925, 0.020, 0.020, 0.014, (0.016, 0.016, 0.016), tone, {"head": 0.7, f"ear_{s}": 0.3}, z=0.066 * sign),
        cut(0.455, 0.898, 0.034, 0.032, 0.0125, coat, tone, e, z=0.084 * sign),
        cut(0.461, 0.860, 0.040, 0.036, 0.0115, coat, tone, e, z=0.092 * sign),
        cut(0.468, 0.822, 0.033, 0.030, 0.0105, coat, tone, e, z=0.093 * sign),
        cut(0.474, 0.797, 0.018, 0.018, 0.0095, coat, tone, e, z=0.090 * sign),
    ]}


def front_leg(sign):
    s = "l" if sign > 0 else "r"
    z = 0.088 * sign
    up, low = f"front_{s}_upper", f"front_{s}_lower"
    leg = {"name": f"front_{s}", "face": (-1, 0, 0), "caps": (0.6, 0.5), "cuts": [
        cut(0.216, 0.560, 0.085, 0.070, 0.046, (0.036, 0.036, 0.040), (GOLD, GOLD, GOLD), {"chest": 1.0}, z=z * 0.78),
        cut(0.205, 0.470, 0.075, 0.060, 0.046, (0.042, 0.034, 0.036), (GOLD, GOLD, LIGHT), {up: 0.7, "chest": 0.3}, z=z * 0.88),
        cut(0.178, 0.395, 0.062, 0.047, 0.045, (0.056, 0.022, 0.016), (LIGHT, GOLD, GOLD), {up: 1.0}, z=z),
        cut(0.172, 0.330, 0.048, 0.043, 0.041, (0.062, 0.022, 0.012), (LIGHT, GOLD, GOLD), {low: 1.0}, z=z),
        cut(0.175, 0.220, 0.039, 0.036, 0.035, (0.054, 0.020, 0.011), (CREAM, GOLD, GOLD), {low: 1.0}, z=z),
        cut(0.179, 0.125, 0.032, 0.033, 0.032, (0.030, 0.016, 0.010), (LIGHT, GOLD, GOLD), {low: 1.0}, z=z),
        cut(0.185, 0.058, 0.032, 0.031, 0.033, (0.014, 0.011, 0.009), (GOLD, GOLD, GOLD), {low: 1.0}, z=z),
    ]}
    paw = {"name": f"front_paw_{s}", "face": (0, 1, 0), "caps": (0.8, 0.7), "cuts": [
        cut(0.168, 0.033, 0.030, 0.030, 0.039, (0.010, 0.010, 0.004), (GOLD, GOLD, GOLD), {f"front_{s}_paw": 1.0}, z=z, square=2.3),
        cut(0.206, 0.032, 0.031, 0.029, 0.047, (0.010, 0.010, 0.004), (GOLD, GOLD, GOLD), {f"front_{s}_paw": 1.0}, z=z, square=2.4),
        cut(0.240, 0.027, 0.023, 0.024, 0.045, (0.009, 0.009, 0.004), (GOLD, GOLD, GOLD), {f"front_{s}_paw": 1.0}, z=z, square=2.5),
    ]}
    return [leg, paw]


def hind_leg(sign):
    s = "l" if sign > 0 else "r"
    z = 0.092 * sign
    up, low, foot = f"hind_{s}_upper", f"hind_{s}_lower", f"hind_{s}_foot"
    leg = {"name": f"hind_{s}", "face": (-1, 0, 0), "caps": (0.6, 0.5), "cuts": [
        cut(-0.300, 0.600, 0.105, 0.100, 0.060, (0.040, 0.038, 0.036), (GOLD, GOLD, GOLD), {"hips": 1.0}, z=z * 0.85),
        cut(-0.285, 0.500, 0.112, 0.104, 0.062, (0.070, 0.036, 0.034), (LIGHT, GOLD, GOLD), {up: 0.65, "hips": 0.35}, z=z * 0.95),
        cut(-0.256, 0.420, 0.094, 0.084, 0.055, (0.088, 0.032, 0.030), (CREAM, GOLD, GOLD), {up: 1.0}, z=z),
        cut(-0.242, 0.362, 0.066, 0.060, 0.045, (0.080, 0.028, 0.022), (CREAM, GOLD, GOLD), {up: 0.5, low: 0.5}, z=z),
        cut(-0.300, 0.292, 0.050, 0.046, 0.040, (0.060, 0.026, 0.016), (LIGHT, GOLD, GOLD), {low: 1.0}, z=z),
        cut(-0.385, 0.218, 0.036, 0.036, 0.033, (0.034, 0.020, 0.012), (LIGHT, GOLD, GOLD), {low: 1.0}, z=z),
        cut(-0.426, 0.186, 0.032, 0.033, 0.031, (0.020, 0.015, 0.011), (GOLD, GOLD, GOLD), {low: 0.5, foot: 0.5}, z=z),
        cut(-0.420, 0.120, 0.029, 0.030, 0.029, (0.016, 0.013, 0.010), (GOLD, GOLD, GOLD), {foot: 1.0}, z=z),
        cut(-0.412, 0.056, 0.030, 0.030, 0.031, (0.013, 0.011, 0.009), (GOLD, GOLD, GOLD), {foot: 1.0}, z=z),
    ]}
    paw = {"name": f"hind_paw_{s}", "face": (0, 1, 0), "caps": (0.8, 0.7), "cuts": [
        cut(-0.432, 0.032, 0.029, 0.029, 0.037, (0.010, 0.010, 0.004), (GOLD, GOLD, GOLD), {f"hind_{s}_paw": 1.0}, z=z, square=2.3),
        cut(-0.394, 0.031, 0.030, 0.028, 0.045, (0.010, 0.010, 0.004), (GOLD, GOLD, GOLD), {f"hind_{s}_paw": 1.0}, z=z, square=2.4),
        cut(-0.360, 0.026, 0.022, 0.023, 0.042, (0.009, 0.009, 0.004), (GOLD, GOLD, GOLD), {f"hind_{s}_paw": 1.0}, z=z, square=2.5),
    ]}
    return [leg, paw]


def tail():
    t1, t2, t3 = {"tail1": 1.0}, {"tail2": 1.0}, {"tail3": 1.0}
    return {"name": "tail", "face": (0, 1, 0), "caps": (0.5, 1.0), "cuts": [
        cut(-0.425, 0.632, 0.046, 0.050, 0.046, (0.036, 0.046, 0.070), (GOLD, GOLD, LIGHT), {"hips": 1.0}),
        cut(-0.500, 0.630, 0.034, 0.036, 0.034, (0.040, 0.052, 0.095), (GOLD, GOLD, LIGHT), t1),
        cut(-0.600, 0.600, 0.028, 0.030, 0.028, (0.042, 0.056, 0.125), (GOLD, LIGHT, CREAM), {"tail1": 0.5, "tail2": 0.5}),
        cut(-0.665, 0.580, 0.024, 0.026, 0.024, (0.042, 0.056, 0.130), (GOLD, LIGHT, CREAM), t2),
        cut(-0.730, 0.555, 0.020, 0.022, 0.020, (0.040, 0.054, 0.120), (GOLD, LIGHT, CREAM), {"tail2": 0.5, "tail3": 0.5}),
        cut(-0.800, 0.528, 0.016, 0.017, 0.016, (0.038, 0.050, 0.100), (GOLD, LIGHT, CREAM), t3),
        cut(-0.865, 0.505, 0.012, 0.012, 0.012, (0.034, 0.042, 0.070), (LIGHT, LIGHT, CREAM), t3),
    ]}


PARTS = [trunk(), neck(), skull(), jaw(), tail(), ear(1), ear(-1)] + front_leg(1) + front_leg(-1) + hind_leg(1) + hind_leg(-1)

# Rounded lumps added to the surface: (centre, radii, on the head?)
LUMPS = [((0.305, 0.525, 0.0), (0.045, 0.055, 0.055), False)]            # the breastbone
for _s in (1, -1):
    LUMPS += [
        ((0.215, 0.520, 0.078 * _s), (0.060, 0.085, 0.030), False),      # shoulder
        ((0.505, 0.818, 0.054 * _s), (0.050, 0.036, 0.025), True),       # cheek
        ((0.545, 0.906, 0.036 * _s), (0.022, 0.012, 0.022), True),       # brow
        ((0.605, 0.803, 0.037 * _s), (0.048, 0.018, 0.013), True),       # the upper lip hanging over the jaw
    ]
    for _x, _z in ((0.248, 0.088), (-0.352, 0.092)):
        for _off in (-0.035, -0.012, 0.012, 0.035):                       # four toes on every paw
            _reach = 0.0 if abs(_off) > 0.02 else 0.010
            LUMPS.append(((_x + _reach, 0.026, (_z + _off) * _s), (0.026, 0.022, 0.0125), False))

NOSE = {"at": (0.690, 0.846, 0.0), "radii": (0.019, 0.021, 0.026), "nostril": ((0.704, 0.842, 0.0125), (0.0085, 0.006, 0.0065))}
EYES = {"from": (0.500, 0.872, 0.022), "towards": (0.95, 0.04, 0.30), "radius": 0.0150, "opening": (0.0145, 0.0092, 0.020)}


# ---- turning the cross-sections into meshes ------------------------------------------------
def catmull(p0, p1, p2, p3, t):
    return 0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t + (-p0 + 3 * p1 - 3 * p2 + p3) * t * t * t)


def loft(part, index):
    """The mesh of one part, with what is known about each of its points."""
    cuts = part["cuts"]
    tilted = part.get("tilt", False)
    rows = []
    for c in cuts:
        at = tilt(c["at"]) if tilted else c["at"]
        size = HEAD_SIZE if tilted else 1.0
        rows.append(list(at) + [c["up"] * size, c["down"] * size, c["wide"] * size, c["square"]] + list(c["coat"]) + [v for tone in c["tone"] for v in tone])
    rows = np.array(rows, dtype=float)
    face = np.array(part["face"], dtype=float)
    if tilted:
        face[:2] = np.array(tilt((HEAD_DRAWN[0] + face[0], HEAD_DRAWN[1] + face[1]))) - np.array(HEAD_AT)
    face[2] = part["face"][2]
    face /= np.linalg.norm(face)

    # Sample the path finely, section values following smoothly, bone weights linearly.
    samples, weights = [], []
    for i in range(len(cuts) - 1):
        a, b = rows[max(i - 1, 0)], rows[min(i + 2, len(cuts) - 1)]
        steps = max(2, int(math.ceil(np.linalg.norm(rows[i + 1, :3] - rows[i, :3]) / SPACING)))
        for k in range(steps + (1 if i == len(cuts) - 2 else 0)):
            t = k / steps
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
    return [tuple(v) for v in verts], faces, info


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
    for part, verts, info in lofts:
        kd = KDTree(len(verts))
        for i, v in enumerate(verts):
            kd.insert(v, i)
        kd.balance()
        trees.append(kd)

    weights = np.zeros((count, len(BONES)))
    coat, tone, flow, grain = np.zeros(count), np.zeros((count, 3)), np.zeros((count, 3)), np.zeros((count, 4))
    owners = np.zeros(count, dtype=int)
    nearest = np.zeros((count, len(lofts)), dtype=int)     # for every point, the closest point of every part

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
              "skin": skin, "shine": shine, "bone_ids": order, "bone_weights": top}
    if copied:
        source = np.array([v for v, _ in copied])
        result = {key: np.concatenate([value, value[source]]) for key, value in result.items()}
        result["grain"][count:] = np.array([g for _, g in copied])
    result["triangles"] = triangles
    return result


def build(preview):
    started = time.time()
    bpy.ops.wm.read_factory_settings(use_empty=True)

    lofts, melted, apart = [], [], []
    for index, part in enumerate(PARTS):
        verts, faces, info = loft(part, index)
        lofts.append((part, verts, info))
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
            return (0.8, PAD, 0.0)                                      # the pads under the paws
        return None

    def jaw_skin(point, normal):
        if normal.y > -0.2 and inside(body_tree, point):
            return (1.0, GUMS if normal.y > 0.3 else RIM, 0.4)         # the floor of the mouth and the lower lips
        return None

    solid = [(p, v, i) for p, v, i in lofts if not p.get("apart")]
    pieces = [
        describe(body, solid, body_tree, None, body_skin),
        describe(jaw_object, [(p, v, i) for p, v, i in lofts if p.get("apart")], jaw_tree, body_tree, jaw_skin),
        describe(nose, solid, nose_tree, body_tree, lambda point, normal: (1.0, LEATHER, 1.0)),
    ]
    pieces[2]["bone_ids"][:] = [BONE_INDEX["muzzle"], 0, 0, 0]
    pieces[2]["bone_weights"][:] = [1.0, 0.0, 0.0, 0.0]
    pieces[2]["coat"][:] = 0.0
    print(f"described: {time.time() - started:.0f} s")

    out = {key: [] for key in ("vertices", "normals", "flow", "grain", "coat", "colors", "skin", "bone_ids", "bone_weights", "triangles")}
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
        offset += n
    # The coat is drawn as layers lifted off the skin. Where it is short, few layers are
    # needed: put the points and triangles under the longest coat first, so that each
    # layer is simply the first so-many of both.
    coat = np.array(out["coat"])
    shells = np.where(coat < 0.0008, 0, np.clip(np.ceil(coat / LAYER_GAP), 2, MAX_LAYERS)).astype(int)
    triangles = np.array(out["triangles"]).reshape(-1, 3)
    deepest = shells[triangles].max(axis=1)
    reach = np.zeros(offset, dtype=int)
    np.maximum.at(reach, triangles.ravel(), np.repeat(deepest, 3))
    points = np.argsort(-reach, kind="stable")
    moved = np.empty(offset, dtype=int)
    moved[points] = np.arange(offset)
    out["triangles"] = moved[triangles[np.argsort(-deepest, kind="stable")]].ravel().tolist()
    for key, width in (("vertices", 3), ("normals", 3), ("flow", 3), ("grain", 4), ("coat", 1), ("colors", 4), ("skin", 2), ("bone_ids", 4), ("bone_weights", 4)):
        out[key] = np.array(out[key]).reshape(offset, width)[points].ravel().tolist()
    out["shells"] = shells[points].tolist()
    out["layers"] = [[int((reach >= k).sum()), int((deepest >= k).sum())] for k in range(1, MAX_LAYERS + 1)]
    print(f"coat layers: {sum(t for _, t in out['layers'])} triangles in all")

    head_joint = Vector(HEAD_AT + (0.0,))
    dog = {
        "about": "Generated by tools/build_dog.py in Blender. Colours are as seen on screen (sRGB); the fourth colour value is how much light reaches the point.",
        "bones": [{"name": name, "parent": parent, "at": [round(c, 4) for c in at]} for name, parent, at in BONES],
        "paws": PAWS,
        "eyes": [{"at": [round(c, 4) for c in (e["at"] - head_joint)], "gaze": [round(c, 3) for c in e["gaze"]], "radius": round(EYES["radius"] * HEAD_SIZE, 4)} for e in eyes],
        "tongue": {"at": [round(c, 4) for c in (Vector(tilt((0.595, 0.805, 0.0))) - Vector(BONES[BONE_INDEX["jaw"]][2]))], "half": [0.066, 0.0065, 0.023]},
        **out,
    }
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        json.dump(dog, f, separators=(",", ":"))
    print(f"wrote {OUT}: {offset} points, {len(out['triangles']) // 3} triangles, {os.path.getsize(OUT) // 1024} KB, {time.time() - started:.0f} s")

    if preview:
        pictures(preview, [body, jaw_object, nose], pieces, eyes)


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

#!/usr/bin/env python3
"""Builds Buddy's body: one continuous, skinned mesh of a Golden Retriever.

The dog is described as a few dozen rounded shapes (a ribcage, a thigh, a
muzzle, …), each belonging to a bone. Their smooth union is turned into a
surface, and every point of that surface is told which bones move it, how long
the coat is there and what shade it is. Nothing here is drawn by hand or taken
from anyone else's model; change the numbers and run it again.

    python3 tools/make_dog.py            # writes renderer/dog/retriever.json

Units: the adult stands about 1.0 tall at the top of the head; +X is forward,
+Y up, Z across (the dog is symmetric in Z). Pure Python, no dependencies; it
takes around a minute.
"""
import json, math, os, sys, time

CELL = 0.016           # size of the sampling grid; smaller is smoother and slower
BLEND = 0.035          # how softly neighbouring shapes melt into each other

# ---- skeleton: name, parent, where the joint is -------------------------------------------
BONES = [
    ("hips", None, (-0.32, 0.58, 0.0)),
    ("chest", "hips", (0.08, 0.58, 0.0)),
    ("neck", "chest", (0.27, 0.63, 0.0)),
    ("head", "neck", (0.45, 0.82, 0.0)),
    ("muzzle", "head", (0.555, 0.83, 0.0)),
    ("jaw", "head", (0.535, 0.795, 0.0)),
    ("ear_l", "head", (0.47, 0.905, 0.08)),
    ("ear_r", "head", (0.47, 0.905, -0.08)),
    ("tail1", "hips", (-0.47, 0.60, 0.0)),
    ("tail2", "tail1", (-0.61, 0.565, 0.0)),
    ("tail3", "tail2", (-0.75, 0.515, 0.0)),
    ("front_l_upper", "chest", (0.22, 0.50, 0.105)),
    ("front_l_lower", "front_l_upper", (0.195, 0.325, 0.105)),
    ("front_r_upper", "chest", (0.22, 0.50, -0.105)),
    ("front_r_lower", "front_r_upper", (0.195, 0.325, -0.105)),
    ("hind_l_upper", "hips", (-0.32, 0.52, 0.11)),
    ("hind_l_lower", "hind_l_upper", (-0.255, 0.325, 0.11)),
    ("hind_r_upper", "hips", (-0.32, 0.52, -0.11)),
    ("hind_r_lower", "hind_r_upper", (-0.255, 0.325, -0.11)),
]
BONE_INDEX = {name: i for i, (name, _, _) in enumerate(BONES)}
PAWS = {"front": (0.225, 0.0), "hind": (-0.335, 0.0)}   # where each paw meets the ground (x, y)

GOLD, PALE, DEEP = (1.0, 1.0, 1.0), (1.18, 1.14, 1.05), (0.82, 0.78, 0.72)

# ---- shapes: (bone, start, end, radius at start, radius at end, squash, coat length, shade)
# `squash` = (y, z) factors thinning the shape; coat length is in units.
def side(sign):
    """Shapes that exist once on each side of the dog. `sign` is +1 (left) or -1 (right)."""
    s = "l" if sign > 0 else "r"
    f, h, e = 0.105 * sign, 0.11 * sign, 0.08 * sign    # how far out the front legs, hind legs and ears sit
    return [
        # front leg: upper arm, forearm, feathering, paw
        (f"front_{s}_upper", (0.225, 0.52, f * 0.95), (0.195, 0.335, f), 0.085, 0.052, (1, 0.8), 0.030, GOLD),
        (f"front_{s}_lower", (0.195, 0.335, f), (0.215, 0.085, f), 0.047, 0.033, (1, 1), 0.022, GOLD),
        (f"front_{s}_lower", (0.200, 0.30, f), (0.185, 0.12, f), 0.030, 0.020, (1, 0.6), 0.055, PALE),
        (f"front_{s}_lower", (0.215, 0.085, f), (0.262, 0.036, f), 0.036, 0.036, (0.9, 1.15), 0.012, GOLD),
        # hind leg: thigh, breeches, shank, hock to foot, paw
        (f"hind_{s}_upper", (-0.325, 0.535, h * 0.9), (-0.255, 0.335, h), 0.118, 0.066, (1, 0.72), 0.032, GOLD),
        (f"hind_{s}_upper", (-0.40, 0.50, h * 0.8), (-0.33, 0.36, h * 0.9), 0.060, 0.040, (1, 0.7), 0.060, PALE),
        (f"hind_{s}_lower", (-0.255, 0.335, h), (-0.375, 0.165, h), 0.056, 0.033, (1, 0.85), 0.024, GOLD),
        (f"hind_{s}_lower", (-0.375, 0.165, h), (-0.355, 0.070, h), 0.031, 0.029, (1, 1), 0.014, GOLD),
        (f"hind_{s}_lower", (-0.355, 0.070, h), (-0.300, 0.036, h), 0.035, 0.036, (0.9, 1.15), 0.012, GOLD),
        # ear: a soft flap hanging beside the cheek
        (f"ear_{s}", (0.475, 0.905, e * 0.8), (0.455, 0.775, e * 1.12), 0.048, 0.040, (1, 0.42), 0.030, DEEP),
    ]

SHAPES = [
    # trunk: rump, loin, ribcage, deep chest, forechest
    ("hips", (-0.375, 0.555, 0), (-0.20, 0.535, 0), 0.150, 0.150, (1, 0.90), 0.034, GOLD),
    ("hips", (-0.20, 0.535, 0), (-0.02, 0.525, 0), 0.150, 0.170, (1, 0.90), 0.034, GOLD),
    ("chest", (-0.02, 0.525, 0), (0.17, 0.515, 0), 0.170, 0.190, (1, 0.88), 0.034, GOLD),
    ("chest", (0.10, 0.44, 0), (0.24, 0.47, 0), 0.130, 0.140, (1, 0.85), 0.050, PALE),     # brisket, with longer pale coat
    ("chest", (0.24, 0.53, 0), (0.30, 0.56, 0), 0.150, 0.120, (1, 0.85), 0.045, PALE),
    ("hips", (-0.25, 0.42, 0), (0.02, 0.40, 0), 0.085, 0.105, (1, 0.9), 0.048, PALE),      # belly coat
    # neck and head
    ("neck", (0.27, 0.60, 0), (0.43, 0.775, 0), 0.128, 0.092, (1, 0.9), 0.040, GOLD),
    ("neck", (0.30, 0.55, 0), (0.42, 0.70, 0), 0.095, 0.070, (1, 0.85), 0.050, PALE),      # ruff under the throat
    ("head", (0.455, 0.845, 0), (0.525, 0.865, 0), 0.094, 0.086, (1, 0.98), 0.013, GOLD),  # skull
    ("head", (0.52, 0.845, 0), (0.565, 0.838, 0), 0.074, 0.060, (1, 1), 0.007, GOLD),      # stop and cheeks
    ("muzzle", (0.565, 0.838, 0), (0.695, 0.822, 0), 0.056, 0.043, (0.92, 1), 0.006, PALE),
    ("jaw", (0.545, 0.790, 0), (0.675, 0.782, 0), 0.036, 0.027, (0.75, 1.05), 0.005, PALE),
    # tail, carried level with a plume of long coat underneath
    ("tail1", (-0.455, 0.605, 0), (-0.61, 0.565, 0), 0.050, 0.040, (1, 0.9), 0.045, GOLD),
    ("tail2", (-0.61, 0.565, 0), (-0.75, 0.515, 0), 0.040, 0.030, (1, 0.9), 0.060, GOLD),
    ("tail3", (-0.75, 0.515, 0), (-0.87, 0.47, 0), 0.030, 0.016, (1, 0.9), 0.060, PALE),
    ("tail2", (-0.56, 0.535, 0), (-0.80, 0.45, 0), 0.022, 0.012, (1.6, 0.5), 0.085, PALE),  # the plume
] + side(1) + side(-1)


class Shape:
    __slots__ = ("bone", "a", "ab", "len2", "ra", "rb", "sy", "sz", "coat", "shade", "lo", "hi", "scale")

    def __init__(self, bone, a, b, ra, rb, squash, coat, shade):
        self.bone, self.ra, self.rb, self.coat, self.shade = BONE_INDEX[bone], ra, rb, coat, shade
        self.sy, self.sz = squash
        # Work in a space stretched so the squashed shape is round again.
        self.a = (a[0], a[1] / self.sy, a[2] / self.sz)
        b = (b[0], b[1] / self.sy, b[2] / self.sz)
        self.ab = (b[0] - self.a[0], b[1] - self.a[1], b[2] - self.a[2])
        self.len2 = max(sum(c * c for c in self.ab), 1e-9)
        self.scale = min(1.0, self.sy, self.sz)
        reach = max(ra, rb) * max(1.0, self.sy, self.sz) + BLEND * 2.5
        self.lo = tuple(min(a[i], (b[0], b[1] * self.sy, b[2] * self.sz)[i]) - reach for i in range(3))
        self.hi = tuple(max(a[i], (b[0], b[1] * self.sy, b[2] * self.sz)[i]) + reach for i in range(3))

    def near(self, x, y, z):
        lo, hi = self.lo, self.hi
        return lo[0] <= x <= hi[0] and lo[1] <= y <= hi[1] and lo[2] <= z <= hi[2]

    def distance(self, x, y, z):
        """Signed distance to the surface of this shape (negative inside), approximately."""
        px, py, pz = x - self.a[0], y / self.sy - self.a[1], z / self.sz - self.a[2]
        ab = self.ab
        t = (px * ab[0] + py * ab[1] + pz * ab[2]) / self.len2
        t = 0.0 if t < 0.0 else 1.0 if t > 1.0 else t
        dx, dy, dz = px - ab[0] * t, py - ab[1] * t, pz - ab[2] * t
        return (math.sqrt(dx * dx + dy * dy + dz * dz) - (self.ra + (self.rb - self.ra) * t)) * self.scale


SHAPE_OBJECTS = [Shape(*s) for s in SHAPES]


def field(x, y, z, far=1.0):
    """Signed distance to the whole dog: the shapes melted together."""
    d = far
    for s in SHAPE_OBJECTS:
        if s.near(x, y, z):
            di = s.distance(x, y, z)
            # polynomial smooth minimum
            h = 0.5 + 0.5 * (d - di) / BLEND
            h = 0.0 if h < 0.0 else 1.0 if h > 1.0 else h
            d = d + (di - d) * h - BLEND * h * (1.0 - h)
    return d


def build():
    started = time.time()
    lo = (-0.96, -0.02, -0.30)
    n = (int(1.78 / CELL) + 2, int(1.04 / CELL) + 2, int(0.60 / CELL) + 2)
    print(f"sampling {n[0]}x{n[1]}x{n[2]} points …", file=sys.stderr)
    values = {}
    for i in range(n[0]):
        x = lo[0] + i * CELL
        for j in range(n[1]):
            y = lo[1] + j * CELL
            for k in range(n[2]):
                z = lo[2] + k * CELL
                v = field(x, y, z)
                if v < 0.9:
                    values[(i, j, k)] = v
    get = lambda i, j, k: values.get((i, j, k), 1.0)
    print(f"  {len(values)} points near the surface, {time.time() - started:.0f} s", file=sys.stderr)

    # Surface nets: one vertex in every cell the surface passes through, placed at the
    # average of where the surface crosses that cell's edges.
    corners = [(a, b, c) for a in (0, 1) for b in (0, 1) for c in (0, 1)]
    edges = [(p, q) for p in range(8) for q in range(p + 1, 8) if sum(abs(corners[p][m] - corners[q][m]) for m in range(3)) == 1]
    vertex_of, positions = {}, []
    cells = set()
    for (i, j, k) in values:
        for a, b, c in corners:
            cells.add((i - a, j - b, k - c))
    for (i, j, k) in cells:
        if not (0 <= i < n[0] - 1 and 0 <= j < n[1] - 1 and 0 <= k < n[2] - 1):
            continue
        vals = [get(i + a, j + b, k + c) for a, b, c in corners]
        if min(vals) >= 0.0 or max(vals) < 0.0:
            continue
        sx = sy = sz = 0.0
        count = 0
        for p, q in edges:
            vp, vq = vals[p], vals[q]
            if (vp < 0.0) != (vq < 0.0):
                t = vp / (vp - vq)
                sx += corners[p][0] + (corners[q][0] - corners[p][0]) * t
                sy += corners[p][1] + (corners[q][1] - corners[p][1]) * t
                sz += corners[p][2] + (corners[q][2] - corners[p][2]) * t
                count += 1
        vertex_of[(i, j, k)] = len(positions)
        positions.append((lo[0] + (i + sx / count) * CELL, lo[1] + (j + sy / count) * CELL, lo[2] + (k + sz / count) * CELL))

    # A quad wherever a grid edge crosses the surface, joining the four cells around that edge.
    triangles = []
    for (i, j, k), v in values.items():
        inside = v < 0.0
        for axis, (di, dj, dk) in enumerate(((1, 0, 0), (0, 1, 0), (0, 0, 1))):
            if (get(i + di, j + dj, k + dk) < 0.0) == inside:
                continue
            if axis == 0:
                ring = [(i, j - 1, k - 1), (i, j, k - 1), (i, j, k), (i, j - 1, k)]
            elif axis == 1:
                ring = [(i - 1, j, k - 1), (i - 1, j, k), (i, j, k), (i, j, k - 1)]
            else:
                ring = [(i - 1, j - 1, k), (i, j - 1, k), (i, j, k), (i - 1, j, k)]
            if any(c not in vertex_of for c in ring):
                continue
            a, b, c, d = (vertex_of[c] for c in ring)
            triangles += [a, b, c, a, c, d] if inside else [a, c, b, a, d, c]   # facing is settled below
    print(f"  {len(positions)} vertices, {len(triangles) // 3} triangles, {time.time() - started:.0f} s", file=sys.stderr)

    # Per vertex: the outward direction, which bones move it, and its coat.
    normals, uvs, colors, bone_ids, bone_weights = [], [], [], [], []
    e = CELL * 0.5
    for (x, y, z) in positions:
        gx = field(x + e, y, z) - field(x - e, y, z)
        gy = field(x, y + e, z) - field(x, y - e, z)
        gz = field(x, y, z + e) - field(x, y, z - e)
        length = math.sqrt(gx * gx + gy * gy + gz * gz) or 1.0
        nx, ny, nz = gx / length, gy / length, gz / length
        normals += [nx, ny, nz]
        # Strand pattern coordinates: the two axes the surface faces least.
        ax, ay, az = abs(nx), abs(ny), abs(nz)
        uvs += [y, z] if ax >= ay and ax >= az else [x, z] if ay >= az else [x, y]
        by_bone, coat, shade, total = {}, 0.0, [0.0, 0.0, 0.0], 0.0
        for s in SHAPE_OBJECTS:
            if not s.near(x, y, z):
                continue
            d = max(s.distance(x, y, z), 0.0)
            moves = math.exp(-(d / 0.045) ** 2)       # joints bend over a few centimetres
            by_bone[s.bone] = by_bone.get(s.bone, 0.0) + moves
            covers = math.exp(-(d / 0.02) ** 2)       # the coat changes more sharply
            coat += s.coat * covers
            total += covers
            for m in range(3):
                shade[m] += s.shade[m] * covers
        top = sorted(by_bone.items(), key=lambda kv: -kv[1])[:4]
        weight_sum = sum(w for _, w in top) or 1.0
        top += [(0, 0.0)] * (4 - len(top))
        bone_ids += [b for b, _ in top]
        bone_weights += [round(w / weight_sum, 4) for _, w in top]
        total = total or 1.0
        colors += [round(shade[0] / total, 3), round(shade[1] / total, 3), round(shade[2] / total, 3), round(min(coat / total / 0.1, 1.0), 3)]

    # Make every triangle face outwards. (Godot's front faces wind clockwise, so the
    # right-hand normal of a front-facing triangle points into the body.)
    flipped = 0
    for t in range(0, len(triangles), 3):
        a, b, c = triangles[t], triangles[t + 1], triangles[t + 2]
        pa, pb, pc = positions[a], positions[b], positions[c]
        u = (pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2])
        v = (pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2])
        cross = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
        out = [normals[a * 3 + m] + normals[b * 3 + m] + normals[c * 3 + m] for m in range(3)]
        if cross[0] * out[0] + cross[1] * out[1] + cross[2] * out[2] > 0.0:
            triangles[t + 1], triangles[t + 2] = c, b
            flipped += 1
    print(f"  turned {flipped} of {len(triangles) // 3} triangles to face outwards", file=sys.stderr)

    r = lambda values, places: [round(v, places) for v in values]
    return {
        "about": "Generated by tools/make_dog.py. Coat length is colour alpha times 0.1 units.",
        "bones": [{"name": name, "parent": parent, "at": at} for name, parent, at in BONES],
        "paws": PAWS,
        "vertices": r([c for p in positions for c in p], 4),
        "normals": r(normals, 3),
        "uvs": r(uvs, 4),
        "colors": colors,
        "bone_ids": bone_ids,
        "bone_weights": bone_weights,
        "triangles": triangles,
    }


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "renderer", "dog", "retriever.json")
    dog = build()
    with open(out, "w") as f:
        json.dump(dog, f, separators=(",", ":"))
    print(f"wrote {os.path.normpath(out)} ({os.path.getsize(out) // 1024} KB)", file=sys.stderr)

#!/usr/bin/env python3
"""Measures the proportions of Buddy's body (renderer/dog/retriever.json) the way a dog
is measured, and compares them with the references in docs/proportions.md.

    python3 tools/measure_dog.py [--json] [--picture OUT.png] [--age adult|puppy]

Everything is a ratio, mostly to the height at the withers (H), so the result does not
depend on how big he is drawn. External measurements are taken on the skin surface at
the landmarks a judge or a vet would use; skeletal ones between the joints of his
skeleton. The landmarks are found as follows (y is up, x forward, z across; the ground
is y = 0; "midline" means within 3.5% of H of the middle):

  withers         where the top line of the back, followed forwards, turns up into the neck
                  (its slope first exceeds 30 degrees); the last point of the back before that
  prosternum      most forward point of the chest between the elbow and just above the point of shoulder
  point of buttocks  hindmost point of the body between 0.55 H and 0.12 H below the tail root,
                  no further back than just behind the tail root (so not the hanging tail)
  brisket         lowest midline point of the chest, from just behind the elbow forward
  loin underline  lowest midline point between 25% and 40% of the body length from the buttocks
  toe tips        most forward points of the front and hind feet at the ground
  occiput, stop, nose tip   recorded by tools/build_dog.py where it builds the head
  joints          shoulder, elbow, hip, stifle, hock: the joints of his skeleton
"""
import json, math, os, sys

import numpy as np

MODEL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "renderer", "dog", "retriever.json")

# The references: (low, high) of what fits a Golden Retriever, and where that comes from.
# Kinds: S = breed standard (appearance, live dog); J = judges' education material;
# K = skeletal / kinematic science (live dogs at bony landmarks, or X-ray);
# P = measured here on photographs (n = 2, roughly ±10%). See docs/proportions.md.
REFERENCE = {
    "length_to_height": ((1.05, 1.13), "S", "AKC 1990 / CKC / UKC: 12:11 = 1.09, 'slightly' longer than tall"),
    "elbow_height": ((0.47, 0.53), "S J", "UKC: elbow to ground about half the height; GRCA judges: 50:50"),
    "brisket_to_elbow": ((-0.03, 0.03), "S", "AKC: brisket extends down to the elbow (difference, in H)"),
    "chest_depth": ((0.47, 0.53), "S J", "follows from the two above"),
    "loin_to_brisket": ((1.00, 1.25), "S", "AKC: very little tuck-up (height of loin underline / of brisket)"),
    "head_length": ((0.35, 0.42), "P", "occiput to nose tip, two side-on photographs: 0.385 and 0.385"),
    "muzzle_to_skull": ((0.80, 1.00), "S P", "FCI: foreface about equals stop to occiput; AKC: nearly as long; photos 0.74, 0.82"),
    "neck": ((0.38, 0.46), "P", "withers to occiput, two photographs: 0.42 and 0.42"),
    "hock_height": ((0.22, 0.28), "P S", "photographs 0.27, 0.24; AKC: hocks well let down"),
    "upper_arm_to_blade": ((0.85, 1.05), "S K", "AKC: upper arm about as long as the blade; Beagle adults 0.88"),
    "scapula_share": ((0.24, 0.34), "K", "Fischer & Lilje 2011: 24-34% of the foreleg; Beagle adults 28.1%"),
    "brachium_share": ((0.23, 0.27), "K", "Beagle adults 24.8%; nearly constant across breeds (Fischer & Lilje)"),
    "distal_fore_share": ((0.45, 0.50), "K", "forearm + forefoot; Beagle adults 47.1%"),
    "scapula_layback": ((22.0, 38.0), "K", "Zink & Schlehr 2020: about 30 degrees from vertical (cineradiography)"),
    "femur_share": ((0.34, 0.38), "K", "Beagle adults 35.9% of femur + crus + pes"),
    "crus_share": ((0.32, 0.36), "K", "Beagle adults 33.7%"),
    "pes_share": ((0.28, 0.33), "K", "Beagle adults 30.4%"),
    "plumb_to_toes": ((-0.06, 0.10), "K", "Zink & Schlehr 2020: a plumb line from the point of buttocks meets the ground at the front of the hind toes, or within half a foot (in H; positive = toes ahead)"),
    "tail_reach": ((0.90, 1.10), "S", "AKC: tail bones reach the point of hock (tail length / (tail root height - hock height))"),
}
# A puppy of eight to nine weeks: the limb shares of the Beagles at nine weeks; the rest
# has no published reference this side of a breed standard, which is for adults.
PUPPY_REFERENCE = {
    "scapula_share": ((0.26, 0.32), "K", "Beagle puppies, 9 weeks: 29.0%"),
    "brachium_share": ((0.22, 0.26), "K", "Beagle puppies, 9 weeks: 24.0%"),
    "distal_fore_share": ((0.45, 0.49), "K", "Beagle puppies, 9 weeks: 47.0%"),
    "femur_share": ((0.32, 0.36), "K", "Beagle puppies, 9 weeks: 33.8%"),
    "crus_share": ((0.29, 0.33), "K", "Beagle puppies, 9 weeks: 30.9%"),
    "pes_share": ((0.33, 0.37), "K", "Beagle puppies, 9 weeks: 35.2%"),
}


def surface(corners, triangles, count=400_000, seed=1):
    """Points spread evenly over the surface (the mesh's corners alone are sparse where it is flat)."""
    rng = np.random.default_rng(seed)
    a, b, c = corners[triangles[:, 0]], corners[triangles[:, 1]], corners[triangles[:, 2]]
    area = 0.5 * np.linalg.norm(np.cross(b - a, c - a), axis=1)
    pick = rng.choice(len(triangles), size=count, p=area / area.sum())
    u, w = rng.random(count), rng.random(count)
    flip = u + w > 1
    u[flip], w[flip] = 1 - u[flip], 1 - w[flip]
    return np.concatenate([corners, a[pick] + (b[pick] - a[pick]) * u[:, None] + (c[pick] - a[pick]) * w[:, None]])


def landmarks(model, age):
    names = [b["name"] for b in model["bones"]]
    triangles = np.array(model["triangles"]).reshape(-1, 3)
    if age == "adult":
        v = np.array(model["vertices"]).reshape(-1, 3)
        joints = {n: np.array(b["at"]) for n, b in zip(names, model["bones"])}
    else:
        v = np.array(model["puppy"]["vertices"]).reshape(-1, 3)
        joints = {n: np.array(at) for n, at in zip(names, model["puppy"]["bones"])}
    v = surface(v, triangles)
    head = {k: np.array(p) for k, p in model["landmarks"][age].items()}
    x, y, z = v[:, 0], v[:, 1], v[:, 2]
    shoulder, elbow = joints["front_l_upper"], joints["front_l_lower"]
    hip, stifle, hock = joints["hind_l_upper"], joints["hind_l_lower"], joints["hind_l_foot"]
    # The top line along the middle of the back, from the hips forwards: the withers are where
    # it stops being a back and becomes a neck.
    span = shoulder[0] - hip[0]
    step = span / 40.0
    mid = np.abs(z) < 0.05 * span
    xs, tops = [], []
    for k in range(70):
        x0 = hip[0] + k * step
        sel = mid & (x >= x0) & (x < x0 + step)
        if sel.any():
            i = np.argmax(np.where(sel, y, -9))
            xs.append(i)
            tops.append(v[i])
    withers = tops[0]
    for a, b in zip(tops[len(tops) // 3:], tops[len(tops) // 3 + 1:]):
        if a[0] > shoulder[0] - 0.6 * span and (b[1] - a[1]) / max(b[0] - a[0], 1e-6) > math.tan(math.radians(30)):
            withers = a
            break
        withers = b
    H = withers[1]
    mid = np.abs(z) < 0.035 * H
    chest_band = (y > elbow[1]) & (y < shoulder[1] + 0.05 * H) & (x < head["occiput"][0])
    prosternum = v[np.argmax(np.where(chest_band, x, -9))]
    tail_root = joints["tail1"]
    rear_band = (y > 0.55 * H) & (y < tail_root[1] - 0.12 * H) & (x > tail_root[0] - 0.06 * H)
    buttocks = v[np.argmin(np.where(rear_band, x, 9))]
    L = prosternum[0] - buttocks[0]
    under = mid & (x > elbow[0] - 0.05 * H) & (x < prosternum[0] - 0.04 * H)
    brisket = v[np.argmin(np.where(under, y, 9))]
    loin_band = mid & (x > buttocks[0] + 0.25 * L) & (x < buttocks[0] + 0.40 * L)
    loin = v[np.argmin(np.where(loin_band, y, 9))]
    low = y < 0.06 * H
    fore_toe = v[np.argmax(np.where(low & (x > (elbow[0] + hock[0]) / 2) & (z > 0), x, -9))]
    hind_toe = v[np.argmax(np.where(low & (x < (elbow[0] + hock[0]) / 2) & (z > 0), x, -9))]
    tail_tip = v[np.argmin(np.where((y > hip[1] - 0.25 * H), x, 9))]
    return dict(v=v, H=H, L=L, withers=withers, prosternum=prosternum, buttocks=buttocks, brisket=brisket, loin=loin,
                fore_toe=fore_toe, hind_toe=hind_toe, shoulder=shoulder, elbow=elbow, hip=hip, stifle=stifle, hock=hock,
                tail_root=tail_root, tail_tip=tail_tip, **head)


def measure(m):
    d = lambda a, b: float(np.linalg.norm(np.asarray(a)[:2] - np.asarray(b)[:2]))
    H = m["H"]
    scapula = d(m["withers"], m["shoulder"])
    brachium = d(m["shoulder"], m["elbow"])
    distal = d(m["elbow"], (m["fore_toe"][0], 0.0))
    femur, crus, pes = d(m["hip"], m["stifle"]), d(m["stifle"], m["hock"]), d(m["hock"], (m["hind_toe"][0], 0.0))
    fore, hind = scapula + brachium + distal, femur + crus + pes
    blade = np.asarray(m["shoulder"][:2]) - np.asarray(m["withers"][:2])
    tail = d(m["tail_root"], m["tail_tip"])
    return {
        "height_at_withers (units)": H,
        "length_to_height": m["L"] / H,
        "elbow_height": m["elbow"][1] / H,
        "brisket_to_elbow": (m["brisket"][1] - m["elbow"][1]) / H,
        "chest_depth": (H - m["brisket"][1]) / H,
        "loin_to_brisket": m["loin"][1] / max(m["brisket"][1], 1e-6),
        "head_length": d(m["occiput"], m["nose_tip"]) / H,
        "muzzle_to_skull": d(m["stop"], m["nose_tip"]) / d(m["occiput"], m["stop"]),
        "neck": d(m["withers"], m["occiput"]) / H,
        "hock_height": m["hock"][1] / H,
        "upper_arm_to_blade": brachium / scapula,
        "scapula_share": scapula / fore,
        "brachium_share": brachium / fore,
        "distal_fore_share": distal / fore,
        "scapula_layback": math.degrees(math.atan2(blade[0], -blade[1])),
        "femur_share": femur / hind,
        "crus_share": crus / hind,
        "pes_share": pes / hind,
        "plumb_to_toes": (m["hind_toe"][0] - m["buttocks"][0]) / H,
        "tail_reach": tail / max(m["tail_root"][1] - m["hock"][1], 1e-6),
    }


def picture(m, path, title):
    """A side view of the body with the landmarks and the bones measured, to check them by eye."""
    from PIL import Image, ImageDraw
    v = m["v"]
    scale = 700
    pad = 40
    xmin, xmax, ymax = v[:, 0].min(), v[:, 0].max(), v[:, 1].max()
    size = (int((xmax - xmin) * scale) + 2 * pad, int(ymax * scale) + 2 * pad + 20)
    to = lambda p: (pad + (p[0] - xmin) * scale, size[1] - pad - p[1] * scale)
    im = Image.new("RGB", size, (245, 246, 248))
    dr = ImageDraw.Draw(im)
    for p in v[v[:, 2] >= 0][::8]:
        a, b = to(p)
        dr.point((a, b), fill=(205, 170, 120))
    dr.line([to((xmin, 0)), to((xmax, 0))], fill=(120, 120, 120))
    bone = lambda a, b, c: dr.line([to(a), to(b)], fill=c, width=3)
    bone(m["withers"], m["shoulder"], (40, 90, 200))
    bone(m["shoulder"], m["elbow"], (40, 150, 60))
    bone(m["elbow"], (m["fore_toe"][0], 0), (200, 60, 40))
    bone(m["hip"], m["stifle"], (40, 90, 200))
    bone(m["stifle"], m["hock"], (40, 150, 60))
    bone(m["hock"], (m["hind_toe"][0], 0), (200, 60, 40))
    bone(m["occiput"], m["stop"], (120, 60, 160))
    bone(m["stop"], m["nose_tip"], (220, 120, 0))
    dr.line([to(m["buttocks"]), to((m["buttocks"][0], 0))], fill=(150, 150, 150))
    for name in ("withers", "prosternum", "buttocks", "brisket", "loin", "fore_toe", "hind_toe", "occiput", "stop", "nose_tip", "tail_tip"):
        a, b = to(m[name])
        dr.ellipse([a - 4, b - 4, a + 4, b + 4], fill=(0, 0, 0))
        dr.text((a + 5, b - 12), name, fill=(0, 0, 0))
    dr.text((pad, 8), title, fill=(0, 0, 0))
    im.save(path)


def main():
    args = sys.argv[1:]
    age = args[args.index("--age") + 1] if "--age" in args else "adult"
    with open(MODEL) as f:
        model = json.load(f)
    m = landmarks(model, age)
    result = measure(m)
    if "--picture" in args:
        picture(m, args[args.index("--picture") + 1], f"Buddy, {age}: landmarks and measured bones")
    if "--json" in args:
        print(json.dumps(result, indent=1))
        return
    refs = dict(REFERENCE)
    if age == "puppy":
        refs = {k: v for k, v in REFERENCE.items() if k not in ("length_to_height", "elbow_height", "brisket_to_elbow", "chest_depth", "head_length", "muzzle_to_skull", "neck", "hock_height", "tail_reach", "upper_arm_to_blade", "plumb_to_toes")}
        refs.update(PUPPY_REFERENCE)
    print(f"{'measure':22} {'Buddy':>8}  {'reference':>13}  {'':4} kind  source")
    for key, value in result.items():
        if key not in refs:
            print(f"{key:22} {value:8.3f}")
            continue
        (low, high), kind, source = refs[key]
        verdict = "ok" if low <= value <= high else ("LOW" if value < low else "HIGH")
        print(f"{key:22} {value:8.3f}  {low:6.2f}–{high:<6.2f}  {verdict:4} {kind:5} {source}")


if __name__ == "__main__":
    main()

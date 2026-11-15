"""Draws Buddy's movements with a real coat of hair, as pictures for the desktop.

    blender -b --factory-startup --python tools/bake_sprites.py -- POSES.json OUT_DIR [--samples 24] [--strands 150000] [--only stand,walk]

POSES.json comes from renderer/export_poses.gd: for every frame, how each bone of
his skeleton has moved. This script builds his body at the same age from
renderer/dog/retriever.json, grows a coat of individual hairs on it (length,
direction, colour and the hang of the long coat all come from the model), moves
body and hair with the bones frame by frame, and renders each frame in Cycles
through exactly the camera his window has, so each picture can be shown in the
window as it is. Pictures that already exist are not drawn again, so a bake can be
stopped and resumed. OUT_DIR/manifest.json lists what was drawn.

It is slow (seconds per picture) and is meant to run in the background, once per
stage of his growth; showing the pictures afterwards costs almost nothing.
"""
import json, math, os, sys, time

import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.kdtree import KDTree

HERE = os.path.dirname(os.path.abspath(__file__))
MODEL = os.path.normpath(os.path.join(HERE, "..", "renderer", "dog", "retriever.json"))
POINTS = 8                    # points along each hair
PALE = np.array([0.94, 0.80, 0.56])   # the cream a puppy's coat is mixed towards (as in coat.gdshaderinc)

# The coat, as settled by comparing renders with photographs of Goldens.
LIFT, CLUMP, DRAPE, HANG = 0.08, 0.10, 0.8, 2.0
MELANIN, REDNESS, TINT = 0.74, 0.95, (1.0, 0.88, 0.65)
HAIR_PX = 0.18                # the radius of a hair, in pixels of the picture
HAIR_REAL = 0.0003            # the radius of a real hair, in units of his (adult) body
YOUNG = 0.0                   # set from his growth: a puppy's fluff stands out from the body more
# Settled by test renders (each can be overridden from the environment for further tests):
E = lambda name, default: os.environ.get(name, default)
VIEW = E("VIEW", "Standard")          # colour transform: "Standard" keeps the coat's colours as they are
LOOK = E("LOOK", "None")
PALING = float(E("PALING", "0.25"))   # how far a puppy's coat is mixed towards cream
SUPER = float(E("SUPER", "1.6"))      # drawn this much larger, then scaled down: hair stays crisp, not smeared
KEY = E("KEY", "left")                # "left": light from the upper left (mirrored when he faces left); "front"
FOCUS = float(E("FOCUS", "0"))        # tests only: render just this far (in units) around his eyes
LID_TUCK = tuple(float(v) for v in E("LID_TUCK", "12,10").split(","))   # degrees the open lids are turned back into the head


def linear(colour):
    colour = np.asarray(colour, dtype=float)
    return np.where(colour > 0.04045, ((colour + 0.055) / 1.055) ** 2.4, colour / 12.92)


def lum(c):
    return 0.2126 * c[..., 0] + 0.7152 * c[..., 1] + 0.0722 * c[..., 2]


def matrices(numbers):
    """Godot transforms (basis columns, then origin; 12 numbers each) as 3x4 arrays."""
    m = np.asarray(numbers, dtype=float).reshape(-1, 4, 3)       # rows: x column, y column, z column, origin
    return np.concatenate([m[:, :3, :].transpose(0, 2, 1), m[:, 3:, :].transpose(0, 2, 1)], axis=2)


def to_blender(m34):
    return Matrix([list(m34[0]), list(m34[1]), list(m34[2]), [0.0, 0.0, 0.0, 1.0]])


def skin(points, ids, weights, bones):
    """Linear blend skinning: each point moved by its bones' matrices, weighted."""
    blend = np.einsum("nk,nkij->nij", weights, bones[ids])
    if points.ndim == 2:
        return np.einsum("nij,nj->ni", blend[:, :, :3], points) + blend[:, :, 3]
    return np.einsum("nij,npj->npi", blend[:, :, :3], points) + blend[:, None, :, 3]


class Body:
    """His body at one age, at rest: the same points and bones the live renderer uses."""

    def __init__(self, model, growth):
        young = 1.0 - growth
        pup = model["puppy"]
        v = np.array(model["vertices"]).reshape(-1, 3)
        self.rest = v + (np.array(pup["vertices"]).reshape(-1, 3) - v) * young
        n = np.array(model["normals"]).reshape(-1, 3)
        n = n + (np.array(pup["normals"]).reshape(-1, 3) - n) * young
        self.normal = n / np.linalg.norm(n, axis=1)[:, None]
        self.flow = np.array(model["flow"]).reshape(-1, 3)
        self.coat = np.array(model["coat"]) + (np.array(pup["coat"]) - np.array(model["coat"])) * young
        colours = np.array(model["colors"]).reshape(-1, 4)
        skin_shine = np.array(model["skin"]).reshape(-1, 2)
        self.skin, self.shine = skin_shine[:, 0], skin_shine[:, 1]
        furred = (1.0 - self.skin)[:, None]
        self.tone = colours[:, :3] + (PALE - colours[:, :3]) * (PALING * young) * furred
        self.ids = np.array(model["bone_ids"]).reshape(-1, 4)
        self.weights = np.array(model["bone_weights"]).reshape(-1, 4)
        tri = np.array(model["triangles"]).reshape(-1, 3)
        self.triangles = tri[:, [0, 2, 1]]          # the renderer winds front faces the other way round
        self.young = young
        self.full = 1.9 - 0.4 * young               # a puppy's fluff is a little shorter than the adult's coat


def grow(body, count, rng):
    """Roots spread over the furred surface by area, and the hair that grows from each."""
    tri, where = body.triangles, body.rest
    a, b, c = where[tri[:, 0]], where[tri[:, 1]], where[tri[:, 2]]
    area = 0.5 * np.linalg.norm(np.cross(b - a, c - a), axis=1)
    # (no stubble where the coat runs out at the nose and the eyes: it shows as a pale ring)
    furred = (body.coat[tri].min(axis=1) > 0.004) & (body.skin[tri].max(axis=1) < 0.35)
    weight = area * furred
    pick = rng.choice(len(tri), size=count, p=weight / weight.sum())
    u, v = rng.random(count), rng.random(count)
    flip = u + v > 1.0
    u[flip], v[flip] = 1.0 - u[flip], 1.0 - v[flip]
    w = 1.0 - u - v
    t = tri[pick]
    mix = lambda arr: (arr[t[:, 0]].T * w + arr[t[:, 1]].T * u + arr[t[:, 2]].T * v).T
    root, n, f, tone = mix(where), mix(body.normal), mix(body.flow), mix(body.tone)
    n /= np.linalg.norm(n, axis=1)[:, None]
    f -= n * np.sum(f * n, axis=1)[:, None]
    f /= np.maximum(np.linalg.norm(f, axis=1), 1e-6)[:, None]
    length = mix(body.coat)
    length = length * (1.0 + (body.full - 1.0) * np.clip((length - 0.015) / 0.03, 0.0, 1.0)) * rng.uniform(0.75, 1.15, count)
    # A hair moves with the bones of the corner of its triangle it is nearest to.
    nearest = np.argmax(np.stack([w, u, v], axis=1), axis=1)
    corner = t[np.arange(count), nearest]
    return root, n, f, length, tone, body.ids[corner], body.weights[corner]


def shape(root, n, f, length, phase):
    """The points of each hair: out of the skin at a slant, lying back along the coat, hanging, waving."""
    k = np.linspace(0.0, 1.0, POINTS)[None, :, None]
    long = np.clip((length - 0.012) / 0.06, 0.0, 1.0)[:, None, None]
    behind_leg = (np.clip(-n[:, 0], 0.0, 1.0) * (root[:, 1] < 0.42) * (root[:, 1] > 0.06))[:, None, None]
    under = np.clip(-n[:, 1], 0.0, 1.0)[:, None, None]          # facing the ground
    lift = LIFT + 0.14 * long + 0.45 * behind_leg * long + 0.20 * YOUNG
    gravity = (0.10 + DRAPE * long + HANG * under * long) * k ** 1.6
    d = n[:, None, :] * lift * (1.0 - 0.7 * k) + f[:, None, :] * (1.0 - lift) + np.array([0.0, -1.0, 0.0]) * gravity
    d /= np.linalg.norm(d, axis=2)[:, :, None]
    step = (length[:, None] / (POINTS - 1))[:, :, None]
    pts = root[:, None, :] + np.concatenate([np.zeros_like(d[:, :1]), np.cumsum(d[:, 1:] * step, axis=1)], axis=1)
    # never back into the body: a little height over the skin, growing with the distance from the root
    height = np.sum((pts - root[:, None, :]) * n[:, None, :], axis=2)
    floor = (0.10 + 0.10 * (1.0 - long[:, :, 0])) * length[:, None] * np.sqrt(k[:, :, 0])
    pts += n[:, None, :] * np.maximum(floor - height, 0.0)[:, :, None]
    side = np.cross(n, f)
    wave = 0.07 * length[:, None] * long[:, :, 0] * np.sin(2.0 * math.pi * 1.6 * k[:, :, 0] + phase[:, None]) * k[:, :, 0]
    return pts + side[:, None, :] * wave[:, :, None]


def coat(body, count, rng):
    """The whole coat at rest: hairs gathered into locks, with the colour each should have."""
    root, n, f, length, tone, ids, weights = grow(body, count, rng)
    centres = rng.choice(len(root), size=max(1, len(root) // 45), replace=False)
    kd = KDTree(len(centres))
    for i, c in enumerate(centres):
        kd.insert(root[c], i)
    kd.balance()
    lock = np.array([kd.find(p)[1] for p in root])
    owner = centres[lock]
    phase = rng.uniform(0, 2 * math.pi, len(centres))[lock]
    pts = shape(root, n, f, length, phase)
    guide = shape(root[owner], n[owner], f[owner], length[owner], phase)
    k = np.linspace(0.0, 1.0, POINTS)[None, :, None]
    guide = guide - root[owner][:, None, :] + root[:, None, :] * (1 - k) + root[owner][:, None, :] * k
    long = np.clip((length - 0.01) / 0.05, 0.0, 1.0)[:, None, None]
    pts = pts + (guide - pts) * (CLUMP + 2.0 * CLUMP * long) * k ** 1.3
    short = np.clip((length - 0.008) / 0.03, 0.0, 1.0)     # short hair shows its colour undiluted: less pigment
    melanin = MELANIN * (0.70 + 0.30 * short) * np.interp(lum(tone), [0.55, 0.62, 0.68, 0.80, 0.90], [0.50, 0.42, 0.35, 0.24, 0.15])
    melanin += rng.normal(0, 0.03, len(root))
    return pts, ids, weights, np.clip(melanin, 0.03, 0.9)


def material_hair():
    m = bpy.data.materials.new("coat")
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    hair = nt.nodes.new("ShaderNodeBsdfHairPrincipled")
    hair.parametrization = "MELANIN"
    # Chiang, not Huang: measured on patches of hair under this light, Huang turns a light,
    # mostly red pigment grey-green, where Chiang gives the cream-to-gold of a Golden's coat.
    hair.model = E("MODEL", "CHIANG")
    attr = nt.nodes.new("ShaderNodeAttribute")
    attr.attribute_type = "GEOMETRY"
    attr.attribute_name = "melanin"
    info = nt.nodes.new("ShaderNodeHairInfo")
    tips = nt.nodes.new("ShaderNodeMath")
    tips.operation = "MULTIPLY_ADD"          # a little lighter towards the tips
    tips.inputs[1].default_value = -0.25
    tips.inputs[2].default_value = 1.0
    nt.links.new(info.outputs["Intercept"], tips.inputs[0])
    scale = nt.nodes.new("ShaderNodeMath")
    scale.operation = "MULTIPLY"
    nt.links.new(attr.outputs["Fac"], scale.inputs[0])
    nt.links.new(tips.outputs[0], scale.inputs[1])
    nt.links.new(scale.outputs[0], hair.inputs["Melanin"])
    hair.inputs["Melanin Redness"].default_value = float(E("REDNESS", str(REDNESS)))
    hair.inputs["Tint"].default_value = tuple(float(v) for v in E("TINT", ",".join(map(str, TINT))).split(",")) + (1.0,)
    hair.inputs["Random Color"].default_value = 0.12
    hair.inputs["Roughness"].default_value = 0.42
    hair.inputs["Random Roughness"].default_value = 0.2
    nt.links.new(hair.outputs[0], out.inputs["Surface"])
    return m


def material_painted(name, roughness_attribute=True):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    paint = nt.nodes.new("ShaderNodeAttribute")
    paint.attribute_name = "paint"
    nt.links.new(paint.outputs["Color"], bsdf.inputs["Base Color"])
    if roughness_attribute:
        rough = nt.nodes.new("ShaderNodeAttribute")
        rough.attribute_name = "rough"
        nt.links.new(rough.outputs["Fac"], bsdf.inputs["Roughness"])
    return m


def material_plain(name, colour, roughness, coat=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = tuple(colour) + (1.0,)
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Coat Weight"].default_value = coat
    return m


def material_eye():
    """A dog's eye: a large dark-brown iris with a black pupil (the lids hide almost all
    of the white), under a clear, wet cornea that catches the light. The pattern is laid
    round the ball's own +X axis, which is turned each frame to where the pupil points."""
    m = bpy.data.materials.new("eye")
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    coords = nt.nodes.new("ShaderNodeTexCoord")
    unit = nt.nodes.new("ShaderNodeVectorMath")
    unit.operation = "NORMALIZE"
    nt.links.new(coords.outputs["Object"], unit.inputs[0])
    axis = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(unit.outputs[0], axis.inputs[0])
    # How far from the middle of the pupil, as the cosine of the angle (1 = the middle).
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    nt.links.new(axis.outputs["X"], ramp.inputs["Fac"])
    stops = [(0.22, (0.50, 0.44, 0.38)), (0.30, (0.020, 0.010, 0.005)), (0.40, (0.055, 0.024, 0.009)),
             (0.88, (0.12, 0.055, 0.02)), (0.935, (0.008, 0.006, 0.005)), (1.0, (0.005, 0.004, 0.004))]
    ramp.color_ramp.elements[0].position, ramp.color_ramp.elements[0].color = stops[0][0], stops[0][1] + (1,)
    ramp.color_ramp.elements[1].position, ramp.color_ramp.elements[1].color = stops[1][0], stops[1][1] + (1,)
    for position, colour in stops[2:]:
        e = ramp.color_ramp.elements.new(position)
        e.color = colour + (1,)
    # Fine radial streaks in the iris.
    streaks = nt.nodes.new("ShaderNodeTexNoise")
    streaks.inputs["Scale"].default_value = 9.0
    nt.links.new(unit.outputs[0], streaks.inputs["Vector"])
    vary = nt.nodes.new("ShaderNodeMix")
    vary.data_type = "RGBA"
    vary.blend_type = "MULTIPLY"
    vary.inputs["Factor"].default_value = 0.5
    nt.links.new(ramp.outputs["Color"], vary.inputs[6])
    nt.links.new(streaks.outputs["Color"], vary.inputs[7])
    nt.links.new(vary.outputs[2], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.5
    bsdf.inputs["Coat Weight"].default_value = 1.0
    bsdf.inputs["Coat Roughness"].default_value = 0.02
    bsdf.inputs["Coat IOR"].default_value = 1.376
    return m


def material_nose():
    """Leather: a fine pebbled texture, dark, a little damp."""
    m = bpy.data.materials.new("nose")
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (0.022, 0.018, 0.017, 1)
    bsdf.inputs["Roughness"].default_value = 0.42
    bsdf.inputs["Coat Weight"].default_value = 0.2
    bsdf.inputs["Coat Roughness"].default_value = 0.3
    cells = nt.nodes.new("ShaderNodeTexVoronoi")
    cells.inputs["Scale"].default_value = 900.0
    cells.feature = "DISTANCE_TO_EDGE"
    coords = nt.nodes.new("ShaderNodeTexCoord")
    nt.links.new(coords.outputs["Object"], cells.inputs["Vector"])
    bump = nt.nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.6
    bump.inputs["Distance"].default_value = 0.0004
    nt.links.new(cells.outputs["Distance"], bump.inputs["Height"])
    nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return m


def mesh_object(name, verts, faces, smooth=True):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([tuple(p) for p in verts], [], [tuple(f) for f in faces])
    if smooth:
        for poly in mesh.polygons:
            poly.use_smooth = True
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def sphere(name, segments=24, rows=12, start=-90.0, end=90.0, across=180.0):
    """A unit sphere, or the band of one between two heights (degrees above the x axis, which
    is the line of sight of an eye), limited to `across` degrees to each side."""
    verts, faces = [], []
    for r in range(rows + 1):
        h = math.radians(start + (end - start) * r / rows)
        for s in range(segments + 1):
            a = math.radians(-across + 2 * across * s / segments)
            verts.append((math.cos(a) * math.cos(h), math.sin(h), math.sin(a) * math.cos(h)))
    for r in range(rows):
        for s in range(segments):
            i = r * (segments + 1) + s
            faces.append((i, i + 1, i + segments + 2, i + segments + 1))
    return verts, faces


def lid(name, upper, tone):
    """An eyelid as the live renderer has it: part of a ball a little larger than the eye,
    fur-coloured, with a dark rim along the edge that closes."""
    start, end = (40.0, 136.0) if upper else (-132.0, -38.0)
    verts, faces = sphere(name, 12, 10, start, end, 74.0)
    obj = mesh_object(name, [np.array(v) * 1.2 for v in verts], faces)
    paint = obj.data.attributes.new("paint", "FLOAT_COLOR", "POINT")
    colours = []
    rim = linear([0.09, 0.06, 0.05])
    for i in range(len(verts)):
        row = i // 13
        edge = row / 10 if upper else 1.0 - row / 10
        mixed = rim + (linear(tone) - rim) * min(1.0, max(edge - 0.25, 0.0) / 0.35)    # a wide, black lid margin
        colours += list(mixed) + [1.0]
    paint.data.foreach_set("color", colours)
    return obj


def main():
    args = sys.argv[sys.argv.index("--") + 1:]
    poses_path, out_dir = args[0], args[1]
    option = lambda flag, fallback: args[args.index(flag) + 1] if flag in args else fallback
    samples = int(option("--samples", "24"))
    only = set(filter(None, option("--only", "").split(",")))
    started = time.time()
    with open(MODEL) as f:
        model = json.load(f)
    with open(poses_path) as f:
        poses = json.load(f)
    os.makedirs(out_dir, exist_ok=True)
    rng = np.random.default_rng(7)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene

    growth = poses["growth"]
    body = Body(model, growth)
    global YOUNG
    YOUNG = body.young
    px_per_unit = poses["window"][1] / poses["camera"]["size"]
    # As many hairs as it takes to cover him at the size he is drawn (about 250,000 for the adult).
    strands = int(option("--strands", str(int(float(E("DENSITY", "1200")) * px_per_unit * poses["scale"]))))

    # The body and its paint: fur colour, darkened where hair covers it; nose, lips, eye rims, pads as they are.
    flesh = mesh_object("body", body.rest, body.triangles)
    paint = flesh.data.attributes.new("paint", "FLOAT_COLOR", "POINT")
    # Under the coat the body is painted in the coat's own colour, half as bright, so that
    # wherever the hairs leave a gap it reads as more coat rather than as dark skin.
    shade = np.where(body.skin[:, None] > 0.5, 1.0, float(E("UNDER", "0.5")))
    paint.data.foreach_set("color", np.concatenate([linear(body.tone) * shade, np.ones((len(body.rest), 1))], axis=1).ravel())
    rough = flesh.data.attributes.new("rough", "FLOAT", "POINT")
    # Matte under the coat (a shine there shows through the hair as bright patches); wet on the nose.
    rough.data.foreach_set("value", np.where(body.skin > 0.5, 0.75 - 0.6 * body.shine, 1.0).astype(np.float32))
    flesh.data.materials.append(material_painted("skin"))
    flesh.data.materials.append(material_nose())
    nose_faces = (body.skin[body.triangles].min(axis=1) > 0.5) & (body.shine[body.triangles].min(axis=1) > 0.9)
    flesh.data.polygons.foreach_set("material_index", nose_faces.astype(np.int32))

    # The coat.
    hair_rest, hair_ids, hair_weights, melanin = coat(body, strands, rng)
    curves = bpy.data.hair_curves.new("coat")
    curves.add_curves([POINTS] * len(hair_rest))
    radius = curves.attributes.new("radius", "FLOAT", "POINT")
    r = (HAIR_PX / px_per_unit) * (1.0 - 0.8 * np.linspace(0, 1, POINTS))
    radius.data.foreach_set("value", np.tile(r, len(hair_rest)).astype(np.float32))
    mel = curves.attributes.new("melanin", "FLOAT", "CURVE")
    # A hair absorbs light along its thickness. Drawn hairs are thicker than real ones (so a few
    # hundred thousand of them can cover him), so they carry proportionally less pigment, or
    # the coat would come out dark and brown instead of gold.
    # (Half the correction, in proportion: the full one leaves a young coat too pale, none too tan.)
    thicker = math.sqrt((HAIR_PX / px_per_unit) / (HAIR_REAL * poses["scale"]))
    mel.data.foreach_set("value", (melanin / thicker).astype(np.float32))
    print(f"BAKE hairs drawn {thicker:.2f} times as thick as real ones; pigment divided by as much", flush=True)
    hair = bpy.data.objects.new("coat", curves)
    scene.collection.objects.link(hair)
    curves.materials.append(material_hair())
    hair.hide_render = bool(E("NOHAIR", ""))      # tests only
    print(f"BAKE body and {len(hair_rest)} hairs ready, {time.time() - started:.0f} s", flush=True)

    # Eyes, eyelids, tongue.
    eye_tone = (PALE * (0.55 * body.young) + np.array([0.80, 0.60, 0.34]) * (1 - 0.55 * body.young)) * 0.75
    painted_lid = material_painted("lid", roughness_attribute=False)
    the_eye = material_eye()
    eyes = []
    for i in range(2):
        verts, faces = sphere(f"eye{i}", 24, 12)
        ball = mesh_object(f"eye{i}", verts, faces)
        ball.data.materials.append(the_eye)
        lids = [lid(f"upper{i}", True, eye_tone), lid(f"lower{i}", False, eye_tone)]
        for one in lids:
            one.data.materials.append(painted_lid)
        eyes.append((ball, lids))
    tongue_half = np.array(model["tongue"]["half"]) + (np.array(model["puppy"]["tongue"]["half"]) - np.array(model["tongue"]["half"])) * body.young
    verts, faces = sphere("tongue", 16, 8)
    tongue = mesh_object("tongue", [np.array(v) * tongue_half[2] for v in verts], faces)
    tongue.data.materials.append(material_plain("tongue", linear([0.80, 0.36, 0.40]), 0.35))

    # Light that looks the same mirrored (he is drawn facing right; facing left is the mirror image).
    world = bpy.data.worlds.new("sky")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.85, 0.84, 0.82, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.1
    # Seen in a reflection the sky is dimmer: an evenly bright sky mirrored in a wet eye or nose
    # greys the whole of it, where a real one shows mostly dark with a small bright catchlight.
    wt = world.node_tree
    dim = wt.nodes.new("ShaderNodeBackground")
    dim.inputs["Color"].default_value = (0.85, 0.84, 0.82, 1)
    dim.inputs["Strength"].default_value = 1.1 * float(E("GLOSSY_SKY", "0.12"))
    path = wt.nodes.new("ShaderNodeLightPath")
    choose = wt.nodes.new("ShaderNodeMixShader")
    wt.links.new(path.outputs["Is Glossy Ray"], choose.inputs["Fac"])
    wt.links.new(wt.nodes["Background"].outputs[0], choose.inputs[1])
    wt.links.new(dim.outputs[0], choose.inputs[2])
    wt.links.new(choose.outputs[0], wt.nodes["World Output"].inputs["Surface"])
    scene.world = world
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 4.5
    sun.data.angle = math.radians(10)
    if KEY == "left":
        sun.matrix_world = Matrix.Rotation(math.radians(-35), 4, "Y") @ Matrix.Rotation(math.radians(-50), 4, "X")
    else:
        sun.matrix_world = Matrix.Rotation(math.radians(-48), 4, "X")      # from above and in front, straight on
    scene.collection.objects.link(sun)
    ground = mesh_object("ground", [(-4, 0, -4), (4, 0, -4), (4, 0, 4), (-4, 0, 4)], [(3, 2, 1, 0)], smooth=False)
    ground.is_shadow_catcher = True

    # Exactly the camera of his window.
    cam = bpy.data.objects.new("camera", bpy.data.cameras.new("camera"))
    cam.data.type = "ORTHO"
    cam.data.sensor_fit = "VERTICAL"
    cam.data.ortho_scale = poses["camera"]["size"]
    cam.data.clip_start, cam.data.clip_end = poses["camera"]["near"], poses["camera"]["far"]
    cam.matrix_world = to_blender(matrices(poses["camera"]["transform"])[0])
    scene.collection.objects.link(cam)
    scene.camera = cam
    scene.render.resolution_x, scene.render.resolution_y = poses["window"]
    scene.render.resolution_percentage = int(round(100 * SUPER))
    scene.view_settings.view_transform = VIEW
    try:
        scene.view_settings.look = LOOK
    except TypeError as err:
        print("look", err)
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = samples
    scene.cycles.use_denoising = not E("NODENOISE", "")
    scene.cycles_curves.shape = "THICK"
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "WEBP"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.quality = 90

    # The manifest goes first: the renderer uses each clip as soon as all its pictures exist.
    manifest = {key: poses[key] for key in ("growth", "scale", "size_px", "window", "margin")}
    manifest["clips"] = [dict({key: clip.get(key) for key in ("name", "kind", "fps", "gaze")}, frames=len(clip["frames"])) for clip in poses["clips"]]
    with open(os.path.join(out_dir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    drawn = 0
    for clip in poses["clips"]:
        if only and clip["name"] not in only:
            continue
        os.makedirs(os.path.join(out_dir, clip["name"]), exist_ok=True)
        for index, frame in enumerate(clip["frames"]):
            path = os.path.join(out_dir, clip["name"], f"{index:03d}.webp")
            if os.path.exists(path):
                continue
            t = time.time()
            bones = matrices(frame["bones"])
            moved = skin(body.rest, body.ids, body.weights, bones)
            flesh.data.vertices.foreach_set("co", moved.ravel().astype(np.float32))
            flesh.data.update()
            strands_now = skin(hair_rest, hair_ids, hair_weights, bones)
            curves.position_data.foreach_set("vector", strands_now.ravel().astype(np.float32))
            curves.update_tag()
            for (ball, lids), eye in zip(eyes, frame["eyes"]):
                place = to_blender(matrices(eye["transform"])[0])
                size = Matrix.Scale(eye["radius"], 4)
                look = Vector(eye.get("look", (1.0, 0.0, 0.0)))
                ball.matrix_world = place @ Vector((1.0, 0.0, 0.0)).rotation_difference(look).to_matrix().to_4x4() @ size
                # Open, the lids are tucked away inside the head; closing, they come over the eye.
                shut_upper = min(max(eye["upper"] / math.radians(-70.0), 0.0), 1.0)
                shut_lower = min(max(eye["lower"] / math.radians(14.0), 0.0), 1.0)
                upper = eye["upper"] + math.radians(LID_TUCK[0]) * (1.0 - shut_upper)
                lower = eye["lower"] - math.radians(LID_TUCK[1]) * (1.0 - shut_lower)
                lids[0].matrix_world = place @ Matrix.Rotation(upper, 4, "Z") @ size
                lids[1].matrix_world = place @ Matrix.Rotation(lower, 4, "Z") @ size
            tongue.hide_render = not frame["tongue"]["shown"]
            tongue.matrix_world = to_blender(matrices(frame["tongue"]["transform"])[0])
            scene.render.filepath = path
            if FOCUS:
                from bpy_extras.object_utils import world_to_camera_view
                middle = (eyes[0][0].matrix_world.translation + eyes[1][0].matrix_world.translation) / 2
                spots = [world_to_camera_view(scene, cam, middle + Vector(d) * FOCUS * poses["scale"]) for d in ((1, 1, 0), (-1, -1, 0))]
                scene.render.use_border, scene.render.use_crop_to_border = True, True
                scene.render.border_min_x, scene.render.border_max_x = sorted((min(max(s.x, 0), 1) for s in spots))
                scene.render.border_min_y, scene.render.border_max_y = sorted((min(max(s.y, 0), 1) for s in spots))
            bpy.ops.render.render(write_still=True)
            if SUPER != 1.0 and not FOCUS:
                picture = bpy.data.images.load(path)
                picture.scale(*poses["window"])
                picture.filepath_raw = path
                picture.file_format = "WEBP"
                picture.save()
                bpy.data.images.remove(picture)
            drawn += 1
            print(f"BAKE {clip['name']} {index + 1}/{len(clip['frames'])} in {time.time() - t:.1f} s", flush=True)
    print(f"BAKE done: {drawn} pictures drawn, {time.time() - started:.0f} s", flush=True)


if __name__ == "__main__":
    main()

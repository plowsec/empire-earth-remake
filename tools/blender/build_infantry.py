"""Infantry & citizens built from a skeleton with Blender's Skin + Subdivision
modifiers (smooth organic bodies). Body parts are tagged in UV2 by nearest bone
so the Godot shader animates walk / aim / work cycles procedurally.

Arms and weapons are authored in the AIM pose and then rotated down by AIM_DROP
around the shoulder line; the shader rotates them back up when attacking."""
import math

import bmesh
import bpy
import mathutils
from mathutils import Vector

import eelib as L

HIP = 0.92
SHOULDER = 1.42
AIM_DROP = 1.05


def drop(p):
    """Rotate a point from aim pose to rest pose around the shoulder line."""
    m = mathutils.Matrix.Translation((0, 0, SHOULDER)) @ mathutils.Matrix.Rotation(AIM_DROP, 4, "X") @ mathutils.Matrix.Translation((0, 0, -SHOULDER))
    return m @ Vector(p)


def drop_obj(ob):
    L.apply_all(ob)
    m = mathutils.Matrix.Translation((0, 0, SHOULDER)) @ mathutils.Matrix.Rotation(AIM_DROP, 4, "X") @ mathutils.Matrix.Translation((0, 0, -SHOULDER))
    ob.data.transform(m)
    return ob


DETAIL = {"subdiv": 1, "segs": 12}


def skin_body(name, joints, edges, radii, subdiv=None):
    """joints: list of (x,y,z); edges: index pairs; radii: (rx, ry) per joint."""
    me = bpy.data.meshes.new(name)
    me.from_pydata([tuple(j) for j in joints], edges, [])
    ob = bpy.data.objects.new(name, me)
    L.link(ob)
    sk = ob.modifiers.new("Skin", "SKIN")
    sk.use_smooth_shade = True
    sv = me.skin_vertices[0].data
    for i, r in enumerate(radii):
        sv[i].radius = r
    sv[0].use_root = True
    sd = DETAIL["subdiv"] if subdiv is None else subdiv
    if sd > 0:
        ss = ob.modifiers.new("Subsurf", "SUBSURF")
        ss.levels = sd
        ss.render_levels = sd
    L.apply_all(ob)
    return ob


def seg_dist(p, a, b):
    ab = b - a
    t = max(0.0, min(1.0, (p - a).dot(ab) / max(ab.length_squared, 1e-9)))
    return (p - (a + ab * t)).length


def tag_parts_by_bones(ob, bones):
    """bones: list of (part_id, [points...]) polylines. Writes UV2 = part id."""
    me = ob.data
    while len(me.uv_layers) < 1:
        me.uv_layers.new(name="UVMap")
    if len(me.uv_layers) < 2:
        me.uv_layers.new(name="Part")
    vpart = []
    for v in me.vertices:
        best, bid = 1e9, 5
        for pid, pts in bones:
            for a, b in zip(pts[:-1], pts[1:]):
                d = seg_dist(v.co, Vector(a), Vector(b))
                if d < best:
                    best, bid = d, pid
        vpart.append(bid)
    uv = me.uv_layers[1]
    for poly in me.polygons:
        for li in poly.loop_indices:
            uv.data[li].uv = (float(vpart[me.loops[li].vertex_index]), 0.0)


def paint_by_region(ob, rules):
    """rules: list of (material, predicate(face_center)->bool); first match wins."""
    me = ob.data
    mats = []
    for m, _ in rules:
        if m.name not in [x.name for x in me.materials]:
            me.materials.append(m)
        mats.append([x.name for x in me.materials].index(m.name))
    for poly in me.polygons:
        c = poly.center
        for k, (_, pred) in enumerate(rules):
            if pred(c):
                poly.material_index = mats[k]
                break


def human(pal, shirt, pants, skin, glove=None, reach_l=(0.0, -0.36, -0.02), reach_r=(0.0, -0.46, 0.0), bulk=1.0):
    """Body mesh (rest pose) + bone polylines for part tagging."""
    s = bulk
    j = {
        "pelvis": (0, 0.0, HIP + 0.02),
        "spine": (0, 0.01, HIP + 0.22),
        "chest": (0, 0.0, SHOULDER - 0.12),
        "neck": (0, 0.0, SHOULDER + 0.06),
        "hipL": (0.105, 0.0, HIP - 0.02),
        "hipR": (-0.105, 0.0, HIP - 0.02),
        "kneeL": (0.115, -0.035, 0.50),
        "kneeR": (-0.115, -0.035, 0.50),
        "ankleL": (0.115, 0.01, 0.10),
        "ankleR": (-0.115, 0.01, 0.10),
        "toeL": (0.115, -0.13, 0.05),
        "toeR": (-0.115, -0.13, 0.05),
    }
    # arms in aim pose then dropped
    for side, key, reach in ((1, "L", reach_l), (-1, "R", reach_r)):
        sh = Vector((0.20 * side, 0.0, SHOULDER - 0.02))
        el = sh + Vector((-0.02 * side, -0.22, -0.13))
        ha = sh + Vector((-0.13 * side + reach[0], reach[1], reach[2] - 0.08))
        j["sh" + key] = tuple(drop(sh) if False else sh)  # shoulder stays put (on pivot line)
        j["el" + key] = tuple(drop(el))
        j["ha" + key] = tuple(drop(ha))
    names = list(j.keys())
    idx = {n: i for i, n in enumerate(names)}
    edges = [
        ("pelvis", "spine"), ("spine", "chest"), ("chest", "neck"),
        ("pelvis", "hipL"), ("hipL", "kneeL"), ("kneeL", "ankleL"), ("ankleL", "toeL"),
        ("pelvis", "hipR"), ("hipR", "kneeR"), ("kneeR", "ankleR"), ("ankleR", "toeR"),
        ("chest", "shL"), ("shL", "elL"), ("elL", "haL"),
        ("chest", "shR"), ("shR", "elR"), ("elR", "haR"),
    ]
    r = {
        "pelvis": (0.15 * s, 0.105 * s), "spine": (0.14 * s, 0.10 * s), "chest": (0.17 * s, 0.11 * s), "neck": (0.055, 0.055),
        "hipL": (0.095 * s, 0.095 * s), "hipR": (0.095 * s, 0.095 * s),
        "kneeL": (0.068 * s, 0.068 * s), "kneeR": (0.068 * s, 0.068 * s),
        "ankleL": (0.052, 0.052), "ankleR": (0.052, 0.052),
        "toeL": (0.045, 0.06), "toeR": (0.045, 0.06),
        "shL": (0.075 * s, 0.075 * s), "shR": (0.075 * s, 0.075 * s),
        "elL": (0.052 * s, 0.052 * s), "elR": (0.052 * s, 0.052 * s),
        "haL": (0.045, 0.045), "haR": (0.045, 0.045),
    }
    body = skin_body("body", [j[n] for n in names], [(idx[a], idx[b]) for a, b in edges], [r[n] for n in names])
    hands_l = Vector(j["haL"])
    hands_r = Vector(j["haR"])
    paint_by_region(body, [
        (glove or skin, lambda c: (c - hands_l).length < 0.07 or (c - hands_r).length < 0.07),
        (skin, lambda c: c.z > SHOULDER + 0.02),
        (pants, lambda c: c.z < HIP + 0.04),
        (shirt, lambda c: True),
    ])
    bones = [
        (1, [j["hipL"], j["kneeL"], j["ankleL"], j["toeL"]]),
        (2, [j["hipR"], j["kneeR"], j["ankleR"], j["toeR"]]),
        (3, [j["shL"], j["elL"], j["haL"]]),
        (4, [j["shR"], j["elR"], j["haR"]]),
        (5, [j["pelvis"], j["spine"], j["chest"], j["neck"], (0, 0, SHOULDER + 0.35)]),
    ]
    return body, bones, j


def head_gear(pal, j, hat, hat_mat, band):
    hz = SHOULDER + 0.24
    sg = DETAIL["segs"]
    parts = [L.sphere("head", 0.108, (0, -0.012, hz), pal["skin"], sg, max(6, sg * 2 // 3), (0.94, 1.02, 1.12))]
    parts.append(L.box("nose", (0.028, 0.05, 0.05), (0, -0.112, hz - 0.012), pal["skin"], 0.012))
    if hat == "helmet":
        parts.append(L.sphere("helmet", 0.138, (0, 0.006, hz + 0.04), hat_mat, sg, max(6, sg * 2 // 3), (1.0, 1.08, 0.74)))
        parts.append(L.cyl("rim", 0.146, 0.024, (0, 0.006, hz + 0.004), hat_mat, sg))
        if band:
            parts.append(L.cyl("band", 0.141, 0.04, (0, 0.006, hz + 0.035), band, sg))
        parts.append(L.box("strap", (0.2, 0.02, 0.02), (0, -0.03, hz - 0.08), pal["olive_dark"]))
    elif hat == "cap":
        parts.append(L.sphere("cap", 0.118, (0, 0.004, hz + 0.05), hat_mat, 16, 8, (1.0, 1.06, 0.66)))
        parts.append(L.box("visor", (0.16, 0.11, 0.016), (0, -0.105, hz + 0.035), hat_mat, 0.012))
    elif hat == "boonie":
        parts.append(L.sphere("crown", 0.12, (0, 0.0, hz + 0.05), hat_mat, 16, 8, (1.0, 1.05, 0.72)))
        parts.append(L.cyl("brim", 0.2, 0.016, (0, 0.0, hz + 0.025), hat_mat, 18))
    return parts


def gear(pal, j, vest, belt, backpack, cross):
    parts = []
    # vest: a slightly inflated skin shell around the torso
    vj = [(0, 0.0, HIP + 0.08), (0, 0.005, HIP + 0.26), (0, 0.0, SHOULDER - 0.06)]
    vr = [(0.165, 0.128), (0.172, 0.13), (0.205, 0.14)]
    v = skin_body("vest", vj, [(0, 1), (1, 2)], vr)
    v.data.materials.append(vest)
    parts.append(v)
    # collar
    parts.append(L.cyl("collar", 0.085, 0.05, (0, 0, SHOULDER + 0.01), vest, DETAIL["segs"]))
    if belt:
        parts.append(L.cyl("belt", 0.158, 0.06, (0, 0.0, HIP + 0.0), belt, DETAIL["segs"]))
        for sx in (0.11, -0.11, 0.0):
            parts.append(L.box("pouch", (0.075, 0.05, 0.08), (sx, -0.15, HIP + 0.01), belt, 0.012))
    if backpack:
        parts.append(L.box("pack", (0.28, 0.15, 0.34), (0, 0.17, HIP + 0.30), backpack, 0.05))
        parts.append(L.cyl("roll", 0.055, 0.30, (0, 0.17, HIP + 0.50), backpack, 12, (0, math.pi / 2, 0)))
    if cross:
        parts.append(L.box("cross_v", (0.06, 0.02, 0.17), (0, -0.152, HIP + 0.28), pal["red_cross"]))
        parts.append(L.box("cross_h", (0.17, 0.02, 0.06), (0, -0.152, HIP + 0.28), pal["red_cross"]))
    # boots
    for side in (1, -1):
        parts.append(L.box("boot", (0.115, 0.24, 0.13), (0.115 * side, -0.05, 0.065), pal["boot"], 0.03))
    return parts

# ----------------------------------------------------------------------------- weapons (aim pose, then dropped)

def _weapon(parts, name="weapon"):
    ob = L.join(parts, name)
    drop_obj(ob)
    return L.set_part(ob, 6)


def rifle(pal):
    z, x = SHOULDER - 0.13, -0.075
    return _weapon([
        L.box("stock", (0.05, 0.22, 0.09), (x, 0.02, z - 0.01), pal["gunmetal"], 0.012),
        L.box("body", (0.06, 0.34, 0.08), (x, -0.26, z + 0.01), pal["gunmetal"], 0.012),
        L.box("mag", (0.04, 0.06, 0.14), (x, -0.24, z - 0.08), pal["gunmetal"], 0.01),
        L.cyl("barrel", 0.014, 0.34, (x, -0.6, z + 0.02), pal["gunmetal"], 8, (math.pi / 2, 0, 0)),
        L.box("handguard", (0.06, 0.18, 0.065), (x, -0.49, z + 0.01), pal["olive_dark"], 0.012),
        L.box("sight", (0.022, 0.07, 0.045), (x, -0.27, z + 0.07), pal["gunmetal"], 0.005),
    ])


def mg(pal):
    z, x = SHOULDER - 0.17, -0.09
    return _weapon([
        L.box("stock", (0.06, 0.24, 0.1), (x, 0.02, z), pal["gunmetal"], 0.012),
        L.box("body", (0.09, 0.42, 0.12), (x, -0.3, z + 0.01), pal["gunmetal"], 0.014),
        L.box("ammo", (0.11, 0.15, 0.15), (x + 0.075, -0.29, z - 0.1), pal["olive_dark"], 0.018),
        L.cyl("barrel", 0.022, 0.55, (x, -0.78, z + 0.02), pal["gunmetal"], 10, (math.pi / 2, 0, 0)),
        L.cyl("shroud", 0.036, 0.25, (x, -0.6, z + 0.02), pal["gunmetal"], 12, (math.pi / 2, 0, 0)),
        L.box("handle", (0.02, 0.1, 0.06), (x, -0.36, z + 0.1), pal["gunmetal"]),
    ])


def launcher(pal, big=False):
    """Shoulder-fired tube: rides on the shoulder (torso part), not dropped."""
    z, x = SHOULDER + 0.07, 0.15
    rr = 0.068 if big else 0.058
    ln = 1.2 if big else 1.05
    parts = [
        L.cyl("tube", rr, ln, (x, -0.2, z), pal["olive"], 16, (math.pi / 2, 0, 0)),
        L.cyl("front", rr * 1.25, 0.11, (x, -0.2 - ln / 2, z), pal["olive_dark"], 16, (math.pi / 2, 0, 0)),
        L.cyl("rear", rr * 1.22, 0.09, (x, -0.2 + ln / 2, z), pal["olive_dark"], 16, (math.pi / 2, 0, 0)),
        L.box("grip", (0.04, 0.06, 0.13), (x, -0.32, z - 0.1), pal["gunmetal"], 0.01),
    ]
    if big:
        parts.append(L.box("sight", (0.11, 0.13, 0.09), (x - 0.1, -0.36, z + 0.03), pal["gunmetal"], 0.012))
        parts.append(L.box("battery", (0.07, 0.13, 0.07), (x, -0.02, z - 0.09), pal["gunmetal"], 0.012))
    ob = L.join(parts, "weapon")
    return L.set_part(ob, 5)


def sniper_rifle(pal):
    z, x = SHOULDER - 0.13, -0.075
    return _weapon([
        L.box("stock", (0.05, 0.28, 0.1), (x, 0.04, z - 0.01), pal["wood_dark"], 0.015),
        L.box("body", (0.055, 0.36, 0.07), (x, -0.28, z + 0.01), pal["olive_dark"], 0.012),
        L.cyl("barrel", 0.015, 0.62, (x, -0.74, z + 0.02), pal["gunmetal"], 8, (math.pi / 2, 0, 0)),
        L.cyl("scope", 0.028, 0.3, (x, -0.27, z + 0.085), pal["gunmetal"], 12, (math.pi / 2, 0, 0)),
        L.cyl("muzzle", 0.026, 0.08, (x, -1.05, z + 0.02), pal["gunmetal"], 8, (math.pi / 2, 0, 0)),
    ])


def mortar_tube(pal):
    """Carried on the back (torso part)."""
    parts = [
        L.cyl("tube", 0.05, 0.85, (0.0, 0.2, HIP + 0.45), pal["olive_dark"], 14, (0.35, 0, 0)),
        L.cyl("plate", 0.15, 0.03, (0.0, 0.26, HIP + 0.1), pal["gunmetal"], 16, (math.pi / 2 - 0.2, 0, 0)),
    ]
    ob = L.join(parts, "weapon")
    return L.set_part(ob, 5)


def tool(pal):
    z, x = SHOULDER - 0.15, -0.06
    return _weapon([
        L.cyl("handle", 0.022, 0.85, (x, -0.38, z), pal["wood"], 8, (math.pi / 2, 0, 0)),
        L.box("head", (0.04, 0.05, 0.42), (x, -0.78, z), pal["steel"], 0.012),
    ])


def medkit(pal):
    z = SHOULDER - 0.32
    return _weapon([
        L.box("kit", (0.22, 0.12, 0.16), (-0.02, -0.42, z), pal["medic_white"], 0.025),
        L.box("cross1", (0.04, 0.01, 0.1), (-0.02, -0.482, z), pal["red_cross"]),
        L.box("cross2", (0.1, 0.01, 0.04), (-0.02, -0.482, z), pal["red_cross"]),
    ])

# ----------------------------------------------------------------------------- figures

def soldier(kind):
    L.reset()
    L.clear_mat_cache()
    pal = L.P()
    team = pal["team"]
    shirt, pants = pal["olive"], pal["olive"]
    vest = team
    hat, hat_mat, band = "helmet", pal["olive_dark"], None
    backpack, belt, cross = pal["khaki"], pal["olive_dark"], False
    glove = pal["olive_dark"]
    reach_l, reach_r = (0.0, -0.36, -0.02), (0.0, -0.46, 0.0)
    if kind == "citizen":
        shirt, pants, vest = team, pal["khaki"], pal["khaki"]
        hat, hat_mat = "cap", team
        backpack, belt, glove = None, pal["wood_dark"], None
        reach_l = reach_r = (0.0, -0.42, 0.0)
    elif kind == "sniper":
        shirt, pants, vest = pal["olive_dark"], pal["olive_dark"], team
        hat, hat_mat = "boonie", pal["olive"]
        backpack = None
    elif kind == "medic":
        vest, cross = pal["medic_white"], True
        band = team
    elif kind in ("bazooka", "stinger"):
        reach_l = (0.08, -0.1, 0.12)
        reach_r = (0.02, -0.25, 0.1)
    elif kind == "mortar":
        backpack = None
    body, bones, j = human(pal, shirt, pants, pal["skin"], glove, reach_l, reach_r)
    tag_parts_by_bones(body, bones)
    rest = []
    for p in head_gear(pal, j, hat, hat_mat, band) + gear(pal, j, vest, belt, backpack, cross):
        L.apply_all(p)
        L.set_part(p, 5)
        rest.append(p)
    # gear on the legs must follow the legs
    for p in rest:
        if p.name.startswith("boot"):
            side = 1 if p.matrix_world.translation.x > 0 or sum(v.co.x for v in p.data.vertices) > 0 else 2
            L.set_part(p, side)
    weapon = {
        "rifleman": rifle,
        "machine_gunner": mg,
        "bazooka": lambda p: launcher(p, False),
        "stinger": lambda p: launcher(p, True),
        "sniper": sniper_rifle,
        "mortar": mortar_tube,
        "medic": medkit,
        "citizen": tool,
    }[kind](pal)
    ob = L.join([body] + rest + [weapon], "body")
    L.smooth(ob, 60)
    return ob


INFANTRY = ["citizen", "rifleman", "machine_gunner", "bazooka", "stinger", "sniper", "mortar", "medic"]


def build(key, out_dir, preview_dir=None):
    DETAIL.update(subdiv=1, segs=12)
    soldier(key)
    L.export_glb(f"{out_dir}/{key}.glb")
    if preview_dir:
        L.preview(f"{preview_dir}/{key}.png", 384, elev=14, azim=-32)
    # far LOD: unsubdivided skin, coarse spheres
    DETAIL.update(subdiv=0, segs=7)
    L.NO_BEVEL[0] = True
    soldier(key)
    L.export_glb(f"{out_dir}/{key}_lod1.glb")
    L.NO_BEVEL[0] = False
    DETAIL.update(subdiv=1, segs=12)

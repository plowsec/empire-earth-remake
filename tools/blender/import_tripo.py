"""Fit a Tripo-generated GLB into the game in place of a procedural model.

  blender -b -P tools/blender/import_tripo.py -- <key> <kind> [--turret 0.58] [--preview dir]

kind: unit (nose toward -Y, ground at z=0), ship (bow toward -Y, waterline kept),
building (footprint centered). The model is oriented, scaled to the size of the model it
replaces, optionally split into hull + "turret" (rotates to aim), textures downscaled,
and exported with a decimated LOD1. Keys listed in tools/assets/tripo_models.txt are
skipped by build_models.py so a rebuild doesn't overwrite them.
"""
import math
import os
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(__file__))
import eelib as L  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
MODELS = os.path.join(ROOT, "game", "assets", "models")
RAW = os.path.join(ROOT, "tools", "assets", "tripo_raw")


def bbox(objs):
    mn = Vector((1e9, 1e9, 1e9))
    mx = Vector((-1e9, -1e9, -1e9))
    for o in objs:
        for v in o.data.vertices:
            w = o.matrix_world @ v.co
            mn = Vector(map(min, mn, w))
            mx = Vector(map(max, mx, w))
    return mn, mx


def import_meshes(path):
    before = set(bpy.context.scene.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    return [o for o in bpy.context.scene.objects if o not in before and o.type == "MESH"]


def main():
    argv = sys.argv[sys.argv.index("--") + 1:]
    key, kind = argv[0], argv[1]
    turret = float(argv[argv.index("--turret") + 1]) if "--turret" in argv else None
    prev = argv[argv.index("--preview") + 1] if "--preview" in argv else None

    L.reset()
    # the size of the procedural model being replaced
    old = import_meshes(os.path.join(MODELS, f"{key}.glb"))
    omn, omx = bbox(old)
    for o in old:
        bpy.data.objects.remove(o, do_unlink=True)

    new = import_meshes(os.path.join(RAW, f"{key}.glb"))
    bpy.ops.object.select_all(action="DESELECT")
    for o in new:
        o.select_set(True)
    bpy.context.view_layer.objects.active = new[0]
    if len(new) > 1:
        bpy.ops.object.join()
    ob = bpy.context.view_layer.objects.active
    ob.parent = None
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    me = ob.data

    # long horizontal axis -> Y
    mn, mx = bbox([ob])
    if kind != "building" and (mx.x - mn.x) > (mx.y - mn.y):
        me.transform(Matrix.Rotation(math.radians(90), 4, "Z"))
    mn, mx = bbox([ob])
    h = mx.z - mn.z
    # which end is the front?
    if kind == "unit":
        # the gun barrel sticks out furthest at the top
        top = [v.co for v in me.vertices if v.co.z > mn.z + h * 0.55]
        front_neg = -min(c.y for c in top) > max(c.y for c in top)
    elif kind == "ship":
        # the bow is the narrower end
        L_ = mx.y - mn.y
        a = [abs(v.co.x) for v in me.vertices if v.co.y < mn.y + L_ * 0.15]
        b = [abs(v.co.x) for v in me.vertices if v.co.y > mx.y - L_ * 0.15]
        front_neg = (sum(a) / max(1, len(a))) < (sum(b) / max(1, len(b)))
    else:
        front_neg = True
    if not front_neg:
        me.transform(Matrix.Rotation(math.radians(180), 4, "Z"))

    # scale to the replaced model and sit it where that one sat
    mn, mx = bbox([ob])
    if kind == "building":
        s = max(omx.x - omn.x, omx.y - omn.y) / max(mx.x - mn.x, mx.y - mn.y)
    else:
        s = (omx.y - omn.y) / (mx.y - mn.y)
    me.transform(Matrix.Scale(s, 4))
    mn, mx = bbox([ob])
    oc = (omn + omx) / 2
    c = (mn + mx) / 2
    me.transform(Matrix.Translation(Vector((oc.x - c.x, oc.y - c.y, omn.z - mn.z))))
    ob.name = "body"

    # optional turret: everything above a height plane becomes the rotating part
    parts = [ob]
    if turret is not None:
        mn, mx = bbox([ob])
        thr = mn.z + (mx.z - mn.z) * turret
        bm = bmesh.new()
        bm.from_mesh(me)
        sel = [f for f in bm.faces if all(v.co.z > thr for v in f.verts)]
        if sel:
            bpy.ops.object.select_all(action="DESELECT")
            ob.select_set(True)
            bpy.context.view_layer.objects.active = ob
            bpy.ops.object.mode_set(mode="EDIT")
            bm2 = bmesh.from_edit_mesh(me)
            for f in bm2.faces:
                f.select = all(v.co.z > thr for v in f.verts)
            bmesh.update_edit_mesh(me)
            bpy.ops.mesh.separate(type="SELECTED")
            bpy.ops.object.mode_set(mode="OBJECT")
            tur = [o for o in bpy.context.scene.objects if o.type == "MESH" and o != ob][0]
            tur.name = "turret"
            # pivot: the middle of the turret ring (median of its vertices, barrel excluded)
            xs = sorted(v.co.x for v in tur.data.vertices)
            ys = sorted(v.co.y for v in tur.data.vertices)
            pivot = Vector((xs[len(xs) // 2], ys[len(ys) // 2], thr))
            tur.data.transform(Matrix.Translation(-pivot))
            tur.location = pivot
            parts.append(tur)
        bm.free()

    # textures: 1024 px is plenty at RTS distances
    for img in bpy.data.images:
        if img.size[0] > 1024:
            img.scale(1024, 1024 * img.size[1] // img.size[0])
            img.pack()

    L.export_glb(os.path.join(MODELS, f"{key}.glb"), parts)
    if prev:
        os.makedirs(prev, exist_ok=True)
        L.preview(os.path.join(prev, f"{key}.png"), 512)
    # LOD1
    for o in parts:
        m = o.modifiers.new("Decimate", "DECIMATE")
        m.ratio = 0.35
    L.export_glb(os.path.join(MODELS, f"{key}_lod1.glb"), parts)
    print("TRIPO_IMPORT", key, "scale", round(s, 3), "parts", [p.name for p in parts], "faces", sum(len(p.data.polygons) for p in parts))


main()

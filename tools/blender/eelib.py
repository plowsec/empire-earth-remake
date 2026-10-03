"""Procedural asset library for Blender 5: hard-surface primitives, PBR palette,
part-id tagging (UV2) for shader-driven infantry animation, GLB export, previews.

Conventions (match the Godot side):
  * meters; origin at ground center; model faces Blender -Y (glTF +Z)
  * objects named turret*, rotor*, rotor_tail*, prop* become animated parts
  * materials named team* are tinted with the player color
  * UV map #2 ("Part"): u = body part id (1 L-leg, 2 R-leg, 3 L-arm, 4 R-arm, 5 torso, 6 weapon)
"""
import math
import os

import bmesh
import bpy
from mathutils import Euler, Matrix, Vector

# ----------------------------------------------------------------------------- scene

def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for c in (bpy.data.meshes, bpy.data.materials, bpy.data.objects, bpy.data.images):
        for x in list(c):
            c.remove(x)


def active(obj):
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj


def link(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj

# ----------------------------------------------------------------------------- materials

_mats = {}


def mat(name, color, rough=0.6, metal=0.0, emit=None, emit_strength=0.0, alpha=1.0):
    """Principled material (cached by name)."""
    if name in _mats and _mats[name].name in bpy.data.materials:
        return _mats[name]
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes.get("Principled BSDF")
    c = color if len(color) == 4 else (*color, 1.0)
    bsdf.inputs["Base Color"].default_value = c
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    if emit is not None:
        bsdf.inputs["Emission Color"].default_value = (*emit, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emit_strength
    if alpha < 1.0:
        bsdf.inputs["Alpha"].default_value = alpha
    m.diffuse_color = c
    _mats[name] = m
    return m


def clear_mat_cache():
    _mats.clear()


# shared palette
def P():
    return {
        "olive": mat("olive", (0.20, 0.22, 0.13), 0.8),
        "olive_dark": mat("olive_dark", (0.12, 0.13, 0.08), 0.85),
        "tan": mat("tan", (0.50, 0.42, 0.28), 0.8),
        "khaki": mat("khaki", (0.36, 0.32, 0.22), 0.85),
        "skin": mat("skin", (0.62, 0.44, 0.33), 0.55),
        "boot": mat("boot", (0.05, 0.045, 0.04), 0.7),
        "gunmetal": mat("gunmetal", (0.05, 0.05, 0.055), 0.4, 0.7),
        "steel": mat("steel", (0.32, 0.33, 0.34), 0.35, 0.9),
        "rubber": mat("rubber", (0.03, 0.03, 0.03), 0.9),
        "track": mat("track", (0.07, 0.065, 0.06), 0.75, 0.4),
        "glass": mat("glass", (0.05, 0.09, 0.12), 0.08, 0.2),
        "team": mat("team", (0.8, 0.8, 0.8), 0.55),
        "team_metal": mat("team_metal", (0.8, 0.8, 0.8), 0.4, 0.3),
        "concrete": mat("concrete", (0.42, 0.41, 0.39), 0.92),
        "concrete_dark": mat("concrete_dark", (0.34, 0.33, 0.31), 0.95),
        "asphalt": mat("asphalt", (0.09, 0.09, 0.095), 0.9),
        "brick": mat("brick", (0.45, 0.20, 0.13), 0.9),
        "white": mat("white", (0.62, 0.61, 0.58), 0.7),
        "marble": mat("marble", (0.66, 0.64, 0.6), 0.4),
        "roof_red": mat("roof_red", (0.42, 0.12, 0.08), 0.75),
        "roof_gray": mat("roof_gray", (0.20, 0.21, 0.22), 0.6, 0.4),
        "wood": mat("wood", (0.33, 0.21, 0.12), 0.8),
        "wood_dark": mat("wood_dark", (0.18, 0.11, 0.06), 0.85),
        "metal_sheet": mat("metal_sheet", (0.42, 0.44, 0.45), 0.45, 0.8),
        "rust": mat("rust", (0.32, 0.15, 0.07), 0.85, 0.3),
        "navy_gray": mat("navy_gray", (0.36, 0.39, 0.42), 0.55, 0.3),
        "navy_dark": mat("navy_dark", (0.16, 0.18, 0.2), 0.6, 0.3),
        "hull_red": mat("hull_red", (0.32, 0.06, 0.05), 0.7),
        "deck": mat("deck", (0.27, 0.25, 0.22), 0.85),
        "air_gray": mat("air_gray", (0.42, 0.45, 0.48), 0.45, 0.25),
        "air_dark": mat("air_dark", (0.12, 0.13, 0.15), 0.5, 0.2),
        "medic_white": mat("medic_white", (0.82, 0.82, 0.8), 0.7),
        "red_cross": mat("red_cross", (0.75, 0.05, 0.04), 0.6),
        "light_warm": mat("light_warm", (1.0, 0.85, 0.55), 0.4, 0.0, (1.0, 0.8, 0.5), 3.0),
        "gold": mat("gold_ore", (0.85, 0.62, 0.15), 0.3, 1.0),
        "rock": mat("rock", (0.38, 0.36, 0.33), 0.92),
        "rock_dark": mat("rock_dark", (0.22, 0.21, 0.2), 0.95),
        "iron_ore": mat("iron_ore", (0.34, 0.18, 0.12), 0.8, 0.5),
        "stone_light": mat("stone_light", (0.48, 0.46, 0.43), 0.85),
        "leaf": mat("leaf", (0.055, 0.15, 0.03), 0.8),
        "leaf_light": mat("leaf_light", (0.10, 0.22, 0.04), 0.8),
        "pine": mat("pine", (0.03, 0.10, 0.045), 0.85),
        "palm": mat("palm", (0.11, 0.24, 0.05), 0.75),
        "bark": mat("bark", (0.22, 0.15, 0.09), 0.95),
        "bark_palm": mat("bark_palm", (0.36, 0.28, 0.18), 0.95),
        "berry": mat("berry", (0.5, 0.03, 0.1), 0.4),
        "soil": mat("soil", (0.22, 0.15, 0.09), 1.0),
        "crop": mat("crop", (0.62, 0.55, 0.18), 0.8),
        "crop_green": mat("crop_green", (0.25, 0.42, 0.10), 0.8),
        "canvas": mat("canvas", (0.40, 0.38, 0.28), 0.95),
        "black": mat("black", (0.02, 0.02, 0.02), 0.6),
        "yellow_paint": mat("yellow_paint", (0.75, 0.55, 0.05), 0.6),
        "runway_mark": mat("runway_mark", (0.85, 0.85, 0.8), 0.7),
    }

# ----------------------------------------------------------------------------- primitives

def _obj_from_bm(bm, name, material):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new(name, me)
    link(ob)
    if material is not None:
        ob.data.materials.append(material)
    return ob


def box(name, size, loc=(0, 0, 0), material=None, bevel=0.0, rot=(0, 0, 0), segs=2):
    """Axis-aligned box: size (x,y,z), loc = center."""
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((v.co.x * size[0], v.co.y * size[1], v.co.z * size[2]))
    ob = _obj_from_bm(bm, name, material)
    ob.location = loc
    ob.rotation_euler = rot
    if bevel > 0:
        add_bevel(ob, bevel, segs)
    return ob


def cyl(name, r, depth, loc=(0, 0, 0), material=None, verts=24, rot=(0, 0, 0), r2=None, bevel=0.0, cap=True):
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=cap, cap_tris=False, segments=verts, radius1=r, radius2=r if r2 is None else r2, depth=depth)
    ob = _obj_from_bm(bm, name, material)
    ob.location = loc
    ob.rotation_euler = rot
    if bevel > 0:
        add_bevel(ob, bevel, 2)
    return ob


def sphere(name, r, loc=(0, 0, 0), material=None, segs=24, rings=12, scale=(1, 1, 1)):
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=segs, v_segments=rings, radius=r)
    ob = _obj_from_bm(bm, name, material)
    ob.location = loc
    ob.scale = scale
    return ob


def ico(name, r, loc=(0, 0, 0), material=None, subdiv=2, scale=(1, 1, 1)):
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=r)
    ob = _obj_from_bm(bm, name, material)
    ob.location = loc
    ob.scale = scale
    return ob


def torus(name, r_major, r_minor, loc=(0, 0, 0), material=None, rot=(0, 0, 0), maj=24, mn=8):
    bpy.ops.mesh.primitive_torus_add(major_radius=r_major, minor_radius=r_minor, major_segments=maj, minor_segments=mn, location=loc, rotation=rot)
    ob = bpy.context.active_object
    ob.name = name
    if material:
        ob.data.materials.append(material)
    return ob


def prism(name, pts2d, depth, loc=(0, 0, 0), material=None, axis="X", bevel=0.0):
    """Extrude a 2D polygon (list of (u,v)) along an axis. axis='X': polygon in YZ plane."""
    bm = bmesh.new()
    vs0, vs1 = [], []
    for (u, v) in pts2d:
        if axis == "X":
            a, b = Vector((-depth / 2, u, v)), Vector((depth / 2, u, v))
        elif axis == "Y":
            a, b = Vector((u, -depth / 2, v)), Vector((u, depth / 2, v))
        else:
            a, b = Vector((u, v, -depth / 2)), Vector((u, v, depth / 2))
        vs0.append(bm.verts.new(a))
        vs1.append(bm.verts.new(b))
    n = len(pts2d)
    bm.faces.new(vs0[::-1])
    bm.faces.new(vs1)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((vs0[i], vs0[j], vs1[j], vs1[i]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    ob = _obj_from_bm(bm, name, material)
    ob.location = loc
    if bevel > 0:
        add_bevel(ob, bevel, 2)
    return ob


def loft(name, sections, material=None, closed_ends=True):
    """Skin a list of cross-sections (each a list of 3D points, same count) into a mesh."""
    bm = bmesh.new()
    rings = [[bm.verts.new(Vector(p)) for p in sec] for sec in sections]
    n = len(sections[0])
    for a, b in zip(rings[:-1], rings[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((a[i], a[j], b[j], b[i]))
    if closed_ends:
        bm.faces.new(rings[0][::-1])
        bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _obj_from_bm(bm, name, material)


def ellipse(cx, cz, rx, rz, y, n=16, flat_bottom=0.0):
    pts = []
    for i in range(n):
        a = 2 * math.pi * i / n
        x = cx + math.cos(a) * rx
        z = cz + math.sin(a) * rz
        if flat_bottom and z < cz - rz * flat_bottom:
            z = cz - rz * flat_bottom
        pts.append((x, y, z))
    return pts

# ----------------------------------------------------------------------------- modifiers & ops

NO_BEVEL = [False]


def add_bevel(ob, width, segs=2, angle=40):
    if NO_BEVEL[0]:
        return None
    m = ob.modifiers.new("Bevel", "BEVEL")
    m.width = width
    m.segments = segs
    m.limit_method = "ANGLE"
    m.angle_limit = math.radians(angle)
    m.harden_normals = False
    return m


def mirror_x(ob):
    m = ob.modifiers.new("Mirror", "MIRROR")
    m.use_axis[0] = True
    return m


def array(ob, count, offset):
    m = ob.modifiers.new("Array", "ARRAY")
    m.count = count
    m.use_relative_offset = False
    m.use_constant_offset = True
    m.constant_offset_displace = offset
    return m


def apply_all(ob):
    active(ob)
    for m in list(ob.modifiers):
        try:
            bpy.ops.object.modifier_apply(modifier=m.name)
        except RuntimeError:
            ob.modifiers.remove(m)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


def join(objs, name):
    objs = [o for o in objs if o is not None]
    for o in objs:
        apply_all(o)
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    if len(objs) > 1:
        bpy.ops.object.join()
    ob = bpy.context.active_object
    ob.name = name
    ob.data.name = name
    return ob


def smooth(ob, angle=35):
    active(ob)
    try:
        bpy.ops.object.shade_smooth_by_angle(angle=math.radians(angle))
    except Exception:
        bpy.ops.object.shade_smooth()
    return ob


def flat(ob):
    active(ob)
    bpy.ops.object.shade_flat()
    return ob


def set_part(ob, part_id):
    """Write part id into the second UV map (created if missing)."""
    me = ob.data
    while len(me.uv_layers) < 1:
        me.uv_layers.new(name="UVMap")
    if len(me.uv_layers) < 2:
        me.uv_layers.new(name="Part")
    uv = me.uv_layers[1]
    for loop in uv.data:
        loop.uv = (float(part_id), 0.0)
    return ob


def ensure_uv(ob):
    me = ob.data
    if len(me.uv_layers) == 0:
        me.uv_layers.new(name="UVMap")
        active(ob)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.uv.smart_project(angle_limit=math.radians(66), island_margin=0.02)
        bpy.ops.object.mode_set(mode="OBJECT")


def parent(child, par):
    child.parent = par
    child.matrix_parent_inverse = par.matrix_world.inverted()


def empty(name, loc=(0, 0, 0)):
    e = bpy.data.objects.new(name, None)
    link(e)
    e.location = loc
    return e


def jitter_verts(ob, amount, seed=1):
    import random
    rnd = random.Random(seed)
    for v in ob.data.vertices:
        v.co += Vector((rnd.uniform(-amount, amount), rnd.uniform(-amount, amount), rnd.uniform(-amount, amount)))


def decimate(ob, ratio):
    m = ob.modifiers.new("Decimate", "DECIMATE")
    m.ratio = ratio
    return m

# ----------------------------------------------------------------------------- export & preview

def export_glb(path, objects=None):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    bpy.ops.object.select_all(action="DESELECT")
    objs = objects or [o for o in bpy.context.scene.objects]
    for o in objs:
        o.select_set(True)
    # bake modifiers & make sure every mesh has a UV map (needed for TEXCOORD order)
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        use_selection=True,
        export_apply=True,
        export_texcoords=True,
        export_normals=True,
        export_materials="EXPORT",
        export_yup=True,
        export_extras=False,
        export_animations=False,
    )


def preview(path, size=512, cam_dist=None, elev=28, azim=-38, focus_z=None, ortho=False):
    """Render an Eevee beauty shot of everything in the scene (used for review & HUD portraits)."""
    scene = bpy.context.scene
    objs = [o for o in scene.objects if o.type == "MESH"]
    if not objs:
        return
    # show team-colored parts in the default player blue
    for m in bpy.data.materials:
        if m.name.startswith("team") and m.use_nodes:
            b = m.node_tree.nodes.get("Principled BSDF")
            if b:
                b.inputs["Base Color"].default_value = (0.05, 0.16, 0.75, 1.0)
    mn = Vector((1e9, 1e9, 1e9))
    mx = Vector((-1e9, -1e9, -1e9))
    for o in objs:
        for c in o.bound_box:
            w = o.matrix_world @ Vector(c)
            mn = Vector((min(mn.x, w.x), min(mn.y, w.y), min(mn.z, w.z)))
            mx = Vector((max(mx.x, w.x), max(mx.y, w.y), max(mx.z, w.z)))
    center = (mn + mx) / 2
    if focus_z is not None:
        center.z = focus_z
    radius = max((mx - mn).length / 2, 0.3)
    dist = cam_dist or radius * 2.6
    cam_data = bpy.data.cameras.new("PreviewCam")
    cam_data.lens = 50
    if ortho:
        cam_data.type = "ORTHO"
        cam_data.ortho_scale = radius * 2.3
    cam = bpy.data.objects.new("PreviewCam", cam_data)
    link(cam)
    e, a = math.radians(elev), math.radians(azim)
    cam.location = center + Vector((math.cos(e) * math.sin(a), -math.cos(e) * math.cos(a), math.sin(e))) * dist
    d = center - cam.location
    cam.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
    scene.camera = cam
    sun_d = bpy.data.lights.new("PreviewSun", "SUN")
    sun_d.energy = 4.0
    sun_d.angle = math.radians(6)
    sun = bpy.data.objects.new("PreviewSun", sun_d)
    link(sun)
    sun.rotation_euler = Euler((math.radians(50), math.radians(10), math.radians(-30)))
    fill_d = bpy.data.lights.new("PreviewFill", "SUN")
    fill_d.energy = 1.2
    fill = bpy.data.objects.new("PreviewFill", fill_d)
    link(fill)
    fill.rotation_euler = Euler((math.radians(60), 0, math.radians(150)))
    world = bpy.data.worlds.new("PreviewWorld")
    world.use_nodes = True
    bg = world.node_tree.nodes.get("Background")
    bg.inputs["Color"].default_value = (0.32, 0.36, 0.42, 1.0)
    bg.inputs["Strength"].default_value = 0.9
    scene.world = world
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = size
    scene.render.resolution_y = size
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    try:
        scene.view_settings.view_transform = "AgX"
    except Exception:
        pass
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    for o in (cam, sun, fill):
        bpy.data.objects.remove(o)

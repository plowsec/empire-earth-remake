"""Aircraft. Nose toward -Y. Origin at the aircraft center (they fly, no ground contact)."""
import math

from mathutils import Vector

import eelib as L

KEYS = ["fighter", "strike_fighter", "bomber", "helicopter"]


def fuselage(name, length, sections, mat):
    """sections: list of (t in 0..1 from nose, half-width, half-height, z-offset)."""
    secs = []
    for t, hw, hh, zo in sections:
        y = -length / 2 + t * length
        secs.append(L.ellipse(0, zo, max(hw, 0.01), max(hh, 0.01), y, 16))
    # ellipse() returns (x, y, z) with x=cx+cos*rx, z=cz+sin*rz in the XZ plane at height y -> remap
    fixed = [[(p[0], p[1], p[2]) for p in s] for s in secs]
    return L.loft(name, fixed, mat)


def wing(name, root_y, root_chord, tip_y_off, tip_chord, span, z, thickness, mat, sweep=0.0, dihedral=0.0, side=1):
    """Thin tapered wing from the fuselage (x=0) to x=span*side."""
    ry0, ry1 = root_y - root_chord / 2, root_y + root_chord / 2
    ty0 = root_y + sweep - tip_chord / 2 + tip_y_off
    ty1 = ty0 + tip_chord
    tz = z + dihedral
    top = [(0, ry0, z + thickness / 2), (0, ry1, z + thickness / 3), (span * side, ty1, tz + thickness / 4), (span * side, ty0, tz + thickness / 3)]
    bot = [(0, ry0, z - thickness / 2), (0, ry1, z - thickness / 3), (span * side, ty1, tz - thickness / 4), (span * side, ty0, tz - thickness / 3)]
    import bmesh
    bm = bmesh.new()
    vt = [bm.verts.new(Vector(p)) for p in top]
    vb = [bm.verts.new(Vector(p)) for p in bot]
    bm.faces.new(vt)
    bm.faces.new(vb[::-1])
    for i in range(4):
        j = (i + 1) % 4
        bm.faces.new((vt[i], vb[i], vb[j], vt[j]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return L._obj_from_bm(bm, name, mat)


def fighter(pal):
    P = []
    gray, dark = pal["air_gray"], pal["air_dark"]
    ln = 7.2
    P.append(fuselage("fus", ln, [
        (0.0, 0.02, 0.02, 0.0), (0.08, 0.22, 0.2, 0.0), (0.22, 0.38, 0.34, 0.05), (0.4, 0.62, 0.38, 0.0),
        (0.7, 0.78, 0.36, -0.02), (0.92, 0.66, 0.3, 0.0), (1.0, 0.6, 0.28, 0.0)], gray))
    P.append(L.sphere("canopy", 0.32, (0, -1.9, 0.36), pal["glass"], 16, 10, (0.8, 2.3, 0.75)))
    for s in (1, -1):
        P.append(wing("wing", 0.6, 3.0, 0.0, 0.9, 3.2, 0.0, 0.14, gray, sweep=1.3, side=s))
        P.append(wing("stab", 3.1, 1.2, 0.0, 0.5, 1.6, 0.0, 0.08, gray, sweep=0.5, side=s))
        fin = wing("fin", 2.7, 1.6, 0.0, 0.6, 1.5, 0.0, 0.08, dark, sweep=0.9, side=1)
        fin.rotation_euler = (0, math.radians(-90 + 10 * s), 0)
        fin.location = (0.55 * s, 0, 0.25)
        P.append(fin)
        P.append(L.cyl("nozzle", 0.32, 0.5, (0.36 * s, 3.65, 0.0), dark, 14, (math.pi / 2, 0, 0)))
        P.append(L.box("intake", (0.42, 1.4, 0.5), (0.62 * s, -0.6, -0.1), dark, 0.05))
        P.append(L.box("team_tip", (0.5, 0.6, 0.02), (2.9 * s, 1.9, 0.0), pal["team"]))
        P.append(L.cyl("missile", 0.07, 1.6, (1.9 * s, 0.6, -0.2), pal["white"], 8, (math.pi / 2, 0, 0)))
    P.append(L.cyl("pitot", 0.02, 0.6, (0, -3.85, 0), dark, 6, (math.pi / 2, 0, 0)))
    ob = L.join(P, "body")
    L.smooth(ob, 35)
    return [ob]


def strike_fighter(pal):
    P = []
    gray, dark = pal["air_gray"], pal["air_dark"]
    ln = 6.8
    P.append(fuselage("fus", ln, [
        (0.0, 0.05, 0.05, 0.0), (0.08, 0.3, 0.3, 0.0), (0.25, 0.42, 0.42, 0.08), (0.5, 0.42, 0.42, 0.05),
        (0.8, 0.3, 0.32, 0.1), (1.0, 0.12, 0.2, 0.2)], gray))
    P.append(L.sphere("canopy", 0.3, (0, -1.9, 0.45), pal["glass"], 14, 10, (0.85, 1.6, 0.8)))
    for s in (1, -1):
        P.append(wing("wing", -0.1, 1.6, 0.0, 1.0, 4.4, -0.15, 0.18, gray, sweep=0.0, side=s))
        P.append(L.cyl("engine", 0.36, 1.5, (0.62 * s, 1.6, 0.62), dark, 16, (math.pi / 2, 0, 0)))
        P.append(L.cyl("engine_in", 0.3, 0.1, (0.62 * s, 0.82, 0.62), pal["black"], 16, (math.pi / 2, 0, 0)))
        P.append(wing("stab", 3.1, 0.9, 0.0, 0.8, 1.6, 0.35, 0.08, gray, side=s))
        fin = wing("fin", 3.1, 0.9, 0.0, 0.8, 1.2, 0.0, 0.08, dark, side=1)
        fin.rotation_euler = (0, math.radians(-90), 0)
        fin.location = (1.6 * s, 0, 0.35)
        P.append(fin)
        for k in range(3):
            P.append(L.cyl("pod", 0.12, 1.0, ((1.4 + k * 0.9) * s, -0.3, -0.35), dark, 10, (math.pi / 2, 0, 0)))
        P.append(L.box("team_tip", (0.6, 0.9, 0.02), (4.0 * s, -0.1, -0.15), pal["team"]))
    P.append(L.cyl("gun", 0.08, 0.8, (0, -3.6, -0.12), pal["gunmetal"], 10, (math.pi / 2, 0, 0)))
    ob = L.join(P, "body")
    L.smooth(ob, 35)
    return [ob]


def bomber(pal):
    dark = pal["air_dark"]
    # flying wing: planform polygon (x, y) extruded thin, tapered thickness by lofting two copies
    half = [(0, -3.4), (7.0, 1.0), (7.0, 1.7), (5.2, 2.6), (3.6, 1.6), (1.8, 2.6), (0, 1.6)]
    outline = half + [(-x, y) for (x, y) in reversed(half[1:-1])]
    top = [(x, y, 0.28 * (1 - abs(x) / 7.5) + 0.04) for x, y in outline]
    bot = [(x, y, -0.18 * (1 - abs(x) / 7.5) - 0.02) for x, y in outline]
    import bmesh
    bm = bmesh.new()
    vt = [bm.verts.new(Vector(p)) for p in top]
    vb = [bm.verts.new(Vector(p)) for p in bot]
    bm.faces.new(vt)
    bm.faces.new(vb[::-1])
    n = len(vt)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((vt[i], vb[i], vb[j], vt[j]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    wingob = L._obj_from_bm(bm, "wing", dark)
    P = [wingob]
    P.append(L.sphere("hump", 1.0, (0, -0.8, 0.22), dark, 20, 10, (1.0, 2.4, 0.35)))
    P.append(L.sphere("cockpit", 0.4, (0, -2.0, 0.42), pal["glass"], 14, 8, (1.3, 0.8, 0.4)))
    for s in (1, -1):
        P.append(L.sphere("intake", 0.4, (1.4 * s, -0.4, 0.3), dark, 12, 8, (1.2, 1.4, 0.4)))
        P.append(L.box("team_band", (1.4, 0.4, 0.02), (5.8 * s, 1.3, 0.12), pal["team"]))
    ob = L.join(P, "body")
    L.smooth(ob, 40)
    return [ob]


def helicopter(pal):
    olive, dark = pal["olive"], pal["olive_dark"]
    P = []
    P.append(fuselage("fus", 5.4, [
        (0.0, 0.1, 0.18, -0.1), (0.1, 0.35, 0.5, 0.0), (0.3, 0.45, 0.65, 0.1), (0.5, 0.5, 0.6, 0.15),
        (0.62, 0.32, 0.38, 0.25), (1.0, 0.14, 0.18, 0.35)], olive))
    # tail boom
    P.append(L.cyl("boom", 0.16, 3.4, (0, 3.6, 0.42), olive, 12, (math.pi / 2, 0, 0), r2=0.09))
    tail = wing("fin", 5.1, 0.8, 0.0, 0.5, 1.2, 0.0, 0.08, dark, sweep=0.3, side=1)
    tail.rotation_euler = (0, math.radians(-90), 0)
    tail.location = (0, 0, 0.45)
    P.append(tail)
    P.append(wing("stab", 4.9, 0.5, 0, 0.4, 0.7, 0.45, 0.05, dark, side=1))
    P.append(wing("stab2", 4.9, 0.5, 0, 0.4, 0.7, 0.45, 0.05, dark, side=-1))
    # tandem canopy
    P.append(L.sphere("canopy1", 0.34, (0, -1.6, 0.42), pal["glass"], 14, 8, (0.9, 1.3, 0.8)))
    P.append(L.sphere("canopy2", 0.36, (0, -0.8, 0.62), pal["glass"], 14, 8, (0.95, 1.3, 0.8)))
    for s in (1, -1):
        P.append(L.box("stub", (1.3, 0.6, 0.1), (0.85 * s, 0.1, 0.0), dark, 0.03))
        P.append(L.cyl("rockets", 0.2, 1.0, (1.25 * s, 0.0, -0.22), dark, 12, (math.pi / 2, 0, 0)))
        P.append(L.box("hellfire", (0.3, 0.9, 0.25), (0.75 * s, 0.05, -0.22), pal["gunmetal"], 0.02))
        P.append(L.box("engine", (0.42, 1.4, 0.42), (0.55 * s, 0.4, 0.75), olive, 0.08))
        P.append(L.box("skid", (0.08, 2.2, 0.08), (0.65 * s, -0.2, -0.75), dark, 0.02))
        P.append(L.box("team_panel", (0.02, 1.2, 0.35), (0.49 * s, 0.6, 0.2), pal["team"]))
    P.append(L.cyl("gun", 0.06, 0.8, (0, -2.3, -0.45), pal["gunmetal"], 8, (math.pi / 2, 0, 0)))
    P.append(L.cyl("mast", 0.1, 0.45, (0, 0.2, 1.2), dark, 10))
    body = L.join(P, "body")
    L.smooth(body, 35)
    # main rotor (spins around its own origin) and tail rotor
    blades = [L.cyl("hub", 0.2, 0.15, (0, 0, 0), dark, 12)]
    for k in range(4):
        a = k * math.pi / 2
        b = L.box("blade", (4.2, 0.28, 0.04), (math.cos(a) * 2.1, math.sin(a) * 2.1, 0), pal["black"], 0.0, rot=(0, 0.04, a))
        blades.append(b)
    rotor = L.join(blades, "rotor")
    rotor.location = (0, 0.2, 1.45)
    tblades = [L.box("tb", (0.04, 0.2, 1.1), (0, 0, 0), pal["black"]), L.box("tb2", (0.04, 1.1, 0.2), (0, 0, 0), pal["black"])]
    trotor = L.join(tblades, "rotor_tail")
    trotor.location = (0.18, 5.15, 0.75)
    return [body, rotor, trotor]


def build(key, out_dir, preview_dir=None):
    L.reset()
    L.clear_mat_cache()
    pal = L.P()
    {"fighter": fighter, "strike_fighter": strike_fighter, "bomber": bomber, "helicopter": helicopter}[key](pal)
    L.export_glb(f"{out_dir}/{key}.glb")
    if preview_dir:
        L.preview(f"{preview_dir}/{key}.png", 384, elev=35, azim=-40)

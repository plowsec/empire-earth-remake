"""Ships. Bow toward -Y, waterline at z=0 (hull extends below)."""
import math

from mathutils import Vector

import eelib as L

KEYS = ["fishing_boat", "transport", "frigate", "battleship", "submarine"]


def hull(length, beam, depth, freeboard, pal, top_mat, side_mat, bottom_mat=None, bow_sharp=0.9, stern_square=0.5, sections=14):
    """Lofted displacement hull: tapered bow, transom stern. Returns (object, deck_z)."""
    secs = []
    for i in range(sections + 1):
        t = i / sections
        y = -length / 2 + t * length
        # half-beam profile: pointed bow (t=0), widest ~0.55, slightly narrowed transom
        if t < 0.45:
            w = math.sin((t / 0.45) * math.pi / 2) ** bow_sharp
        else:
            w = 1.0 - (t - 0.45) / 0.55 * (1 - stern_square)
        hb = max(beam / 2 * w, 0.02)
        sheer = freeboard + 0.25 * (1 - t) ** 2 * freeboard  # bow rises
        keel = -depth * (0.35 + 0.65 * min(1.0, t * 6) * (1 - max(0, t - 0.85) * 2))
        # cross-section: deck edge -> bilge -> keel -> other side
        pts = [
            (hb, y, sheer), (hb * 0.98, y, freeboard * 0.2), (hb * 0.85, y, keel * 0.55), (hb * 0.4, y, keel * 0.95), (0, y, keel),
            (-hb * 0.4, y, keel * 0.95), (-hb * 0.85, y, keel * 0.55), (-hb * 0.98, y, freeboard * 0.2), (-hb, y, sheer),
        ]
        secs.append(pts)
    ob = L.loft("hull", secs, side_mat)
    # deck plate
    deck_pts = []
    for i in range(sections + 1):
        t = i / sections
        y = -length / 2 + t * length
        w = math.sin((t / 0.45) * math.pi / 2) ** bow_sharp if t < 0.45 else 1.0 - (t - 0.45) / 0.55 * (1 - stern_square)
        deck_pts.append((y, beam / 2 * w * 0.97, freeboard + 0.25 * (1 - t) ** 2 * freeboard + 0.02))
    import bmesh
    bm = bmesh.new()
    left = [bm.verts.new(Vector((p[1], p[0], p[2]))) for p in deck_pts]
    right = [bm.verts.new(Vector((-p[1], p[0], p[2]))) for p in deck_pts]
    for i in range(len(deck_pts) - 1):
        bm.faces.new((left[i], left[i + 1], right[i + 1], right[i]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    for f in bm.faces:
        if f.normal.z < 0:
            f.normal_flip()
    deck = L._obj_from_bm(bm, "deck", top_mat)
    # boot-topping stripe at the waterline
    stripe = L.loft("boot", [[(p[0] * 1.004, p[1], z) for p in sec[1:2] + sec[7:8]] for sec, z in []], side_mat) if False else None
    return [ob, deck], freeboard


def gun_turret(name, pos, size, barrels, blen, pal, mat=None):
    x, y, z = pos
    m = mat or pal["navy_gray"]
    parts = [
        L.cyl("base", size * 0.55, size * 0.25, (0, 0, size * 0.12), m, 18),
        L.box("house", (size, size * 1.2, size * 0.5), (0, 0.05 * size, size * 0.42), m, size * 0.06),
    ]
    for k in range(barrels):
        bx = (k - (barrels - 1) / 2) * size * 0.28
        parts.append(L.cyl("barrel", size * 0.06, blen, (bx, -size * 0.6 - blen / 2, size * 0.42), pal["navy_dark"], 10, (math.pi / 2, 0, 0)))
    t = L.join(parts, name)
    t.location = (x, y, z)
    return t


def fishing_boat(pal):
    parts, fb = hull(7.0, 2.4, 0.9, 0.7, pal, pal["deck"], pal["team"], bow_sharp=0.7)
    parts.append(L.box("cabin", (1.5, 1.6, 1.3), (0, 0.6, fb + 0.65), pal["white"], 0.08))
    parts.append(L.box("roof", (1.7, 1.8, 0.1), (0, 0.6, fb + 1.35), pal["navy_dark"], 0.03))
    parts.append(L.box("windows", (1.52, 0.05, 0.35), (0, -0.21, fb + 1.0), pal["glass"]))
    parts.append(L.cyl("mast", 0.06, 3.0, (0, -0.8, fb + 1.5), pal["steel"], 8))
    parts.append(L.cyl("boom", 0.04, 2.5, (0, 0.2, fb + 2.2), pal["steel"], 6, (math.pi / 2 - 0.5, 0, 0)))
    parts.append(L.box("net", (1.4, 0.8, 0.35), (0, 2.6, fb + 0.2), pal["olive_dark"], 0.1))
    parts.append(L.box("hull_band", (2.42, 6.0, 0.15), (0, 0.4, 0.08), pal["white"]))
    ob = L.join(parts, "body")
    L.smooth(ob, 40)
    return [ob]


def transport(pal):
    parts, fb = hull(13.0, 4.2, 1.4, 1.3, pal, pal["deck"], pal["navy_gray"], bow_sharp=0.35, stern_square=0.85)
    # well deck walls + bow ramp
    for s in (1, -1):
        parts.append(L.box("wall", (0.18, 8.0, 0.8), (s * 1.95, -0.8, fb + 0.4), pal["navy_gray"], 0.03))
        parts.append(L.box("team_band", (0.02, 6.0, 0.3), (s * 2.05, -0.8, fb + 0.4), pal["team"]))
    parts.append(L.box("ramp", (3.0, 0.2, 1.4), (0, -5.6, fb + 0.6), pal["navy_dark"], 0.04, rot=(0.25, 0, 0)))
    parts.append(L.box("island", (2.8, 2.4, 2.2), (0, 4.4, fb + 1.1), pal["navy_gray"], 0.08))
    parts.append(L.box("bridge", (3.0, 1.2, 0.9), (0, 3.8, fb + 2.6), pal["navy_gray"], 0.06))
    parts.append(L.box("bridge_win", (3.02, 0.05, 0.35), (0, 3.18, fb + 2.65), pal["glass"]))
    parts.append(L.cyl("mast", 0.08, 3.0, (0, 4.6, fb + 4.2), pal["steel"], 8))
    parts.append(L.cyl("funnel", 0.45, 1.3, (0, 5.3, fb + 2.8), pal["navy_dark"], 14))
    for k in range(3):
        parts.append(L.box("cargo", (1.1, 1.3, 0.7), ((k - 1) * 1.15, -1.0, fb + 0.35), pal["olive"], 0.05))
    ob = L.join(parts, "body")
    L.smooth(ob, 40)
    return [ob]


def frigate(pal):
    parts, fb = hull(14.0, 3.6, 1.4, 1.2, pal, pal["deck"], pal["navy_gray"])
    parts.append(L.box("super1", (2.6, 4.2, 1.6), (0, 0.6, fb + 0.8), pal["navy_gray"], 0.08))
    parts.append(L.box("bridge", (2.4, 1.6, 1.1), (0, -0.9, fb + 2.0), pal["navy_gray"], 0.07))
    parts.append(L.box("bridge_win", (2.42, 0.05, 0.3), (0, -1.71, fb + 2.15), pal["glass"]))
    parts.append(L.box("hangar", (2.8, 2.6, 1.3), (0, 3.6, fb + 0.65), pal["navy_gray"], 0.07))
    parts.append(L.box("helipad", (3.0, 2.2, 0.06), (0, 5.7, fb + 0.05), pal["navy_dark"]))
    parts.append(L.cyl("mast", 0.18, 3.2, (0, 0.2, fb + 3.0), pal["navy_dark"], 8, r2=0.06))
    parts.append(L.box("radar", (1.2, 0.15, 0.4), (0, 0.2, fb + 4.0), pal["navy_dark"], 0.03))
    parts.append(L.box("funnel", (1.0, 1.4, 1.2), (0, 1.9, fb + 2.0), pal["navy_dark"], 0.1))
    parts.append(L.box("vls", (1.6, 1.4, 0.15), (0, -3.6, fb + 0.1), pal["navy_dark"], 0.02))
    for s in (1, -1):
        parts.append(L.box("team_band", (0.02, 4.5, 0.35), (s * 1.31, 0.8, fb + 0.9), pal["team"]))
    parts.append(L.box("hull_num", (3.62, 1.0, 0.3), (0, -5.5, fb * 0.7), pal["team"]))
    body = L.join(parts, "body")
    L.smooth(body, 40)
    t = gun_turret("turret_fwd", (0, -5.0, fb), 1.0, 1, 1.6, pal)
    ciws = gun_turret("turret_ciws", (0, 2.6, fb + 1.3), 0.6, 1, 0.6, pal, pal["white"])
    return [body, t, ciws]


def battleship(pal):
    parts, fb = hull(22.0, 4.8, 2.0, 1.4, pal, pal["deck"], pal["navy_gray"], bow_sharp=1.2)
    parts.append(L.box("citadel", (3.2, 7.0, 1.6), (0, 0.8, fb + 0.8), pal["navy_gray"], 0.1))
    parts.append(L.box("tower", (2.2, 2.4, 2.2), (0, -0.8, fb + 2.6), pal["navy_gray"], 0.1))
    parts.append(L.box("bridge", (2.6, 1.4, 0.9), (0, -1.2, fb + 3.9), pal["navy_gray"], 0.07))
    parts.append(L.box("bridge_win", (2.62, 0.05, 0.3), (0, -1.91, fb + 4.0), pal["glass"]))
    parts.append(L.cyl("director", 0.6, 0.6, (0, -0.8, fb + 4.7), pal["navy_dark"], 14))
    parts.append(L.cyl("mast", 0.15, 3.6, (0, 0.2, fb + 5.4), pal["navy_dark"], 8, r2=0.05))
    parts.append(L.box("yard", (2.4, 0.08, 0.08), (0, 0.2, fb + 6.2), pal["navy_dark"]))
    for k in range(2):
        parts.append(L.cyl("funnel", 0.7, 2.6, (0, 1.6 + k * 1.9, fb + 2.4), pal["navy_dark"], 16, r2=0.62))
        parts.append(L.cyl("funnel_cap", 0.72, 0.15, (0, 1.6 + k * 1.9, fb + 3.7), pal["black"], 16))
    for s in (1, -1):
        for k in range(4):
            parts.append(L.cyl("sec", 0.25, 0.25, (s * 1.9, -1.5 + k * 1.3, fb + 0.6), pal["navy_gray"], 10))
        parts.append(L.box("team_band", (0.02, 6.5, 0.4), (s * 1.61, 0.8, fb + 1.0), pal["team"]))
    body = L.join(parts, "body")
    L.smooth(body, 40)
    t1 = gun_turret("turret_a", (0, -6.2, fb), 2.0, 3, 3.6, pal)
    t2 = gun_turret("turret_b", (0, -3.9, fb + 0.9), 1.8, 3, 3.2, pal)
    t3 = gun_turret("turret_c", (0, 6.4, fb), 2.0, 3, 3.6, pal)
    t3.rotation_euler = (0, 0, math.pi)
    return [body, t1, t2, t3]


def submarine(pal):
    dark = pal["navy_dark"]
    secs = []
    n = 16
    ln = 13.0
    for i in range(n + 1):
        t = i / n
        y = -ln / 2 + t * ln
        r = 0.9 * (math.sin(min(1.0, t / 0.18) * math.pi / 2) if t < 0.18 else (1.0 if t < 0.7 else max(0.08, 1 - (t - 0.7) / 0.3)))
        secs.append(L.ellipse(0, -0.25, max(r, 0.04), max(r * 0.95, 0.04), y, 16))
    P = [L.loft("hull", secs, dark)]
    P.append(L.box("sail", (0.7, 2.2, 1.6), (0, -1.6, 1.15), dark, 0.25))
    P.append(L.box("sail_planes", (2.2, 0.5, 0.08), (0, -1.9, 1.3), dark, 0.03))
    P.append(L.cyl("periscope", 0.05, 1.0, (0, -1.4, 2.3), pal["steel"], 6))
    P.append(L.box("team_band", (0.72, 1.0, 0.25), (0, -1.6, 1.7), pal["team"]))
    for s in (1, -1):
        P.append(L.box("stern_plane", (1.6, 0.6, 0.06), (s * 0.7, 5.6, -0.25), dark, 0.02))
    P.append(L.box("rudder", (0.06, 0.7, 1.6), (0, 5.8, -0.25), dark, 0.02))
    ob = L.join(P, "body")
    L.smooth(ob, 40)
    return [ob]


def build(key, out_dir, preview_dir=None):
    L.reset()
    L.clear_mat_cache()
    pal = L.P()
    {"fishing_boat": fishing_boat, "transport": transport, "frigate": frigate, "battleship": battleship, "submarine": submarine}[key](pal)
    L.export_glb(f"{out_dir}/{key}.glb")
    if preview_dir:
        L.preview(f"{preview_dir}/{key}.png", 384, elev=30, azim=-35)

"""Armored vehicles: shared tracked hull + specific turrets; humvee; towed AT gun."""
import math

from mathutils import Vector

import eelib as L

KEYS = ["tank", "aa_vehicle", "howitzer", "recon", "at_gun"]


def track_assembly(side, length, wheel_r, n_wheels, width, pal, skirt=True):
    """One side's track: band (stadium loop), road wheels, sprocket, idler, skirt."""
    x = side
    parts = []
    # track band: stadium profile in the YZ plane, extruded along X
    r = wheel_r * 1.05
    half = length / 2 - r
    pts = []
    for i in range(10):
        a = -math.pi / 2 + math.pi * i / 9
        pts.append((half + math.cos(a) * r, r + math.sin(a) * r))
    for i in range(10):
        a = math.pi / 2 + math.pi * i / 9
        pts.append((-half + math.cos(a) * r, r + math.sin(a) * r))
    band = L.prism("track", [(-p[0], p[1]) for p in pts], width, (x, 0, 0), pal["track"], "X")
    # hollow look: inner wheels sit inside the band
    parts.append(band)
    for i in range(n_wheels):
        y = -half + (2 * half) * i / max(n_wheels - 1, 1)
        w = L.cyl("wheel", wheel_r * 0.82, width * 1.06, (x, y, wheel_r * 0.95), pal["olive_dark"], 16, (0, math.pi / 2, 0))
        parts.append(w)
        hub = L.cyl("hub", wheel_r * 0.35, width * 1.12, (x, y, wheel_r * 0.95), pal["rubber"], 10, (0, math.pi / 2, 0))
        parts.append(hub)
    # track teeth / grousers on the bottom run
    for i in range(int(length / 0.22)):
        y = -half + i * 0.22
        if y > half:
            break
        parts.append(L.box("grouser", (width * 1.02, 0.07, 0.035), (x, y, 0.02), pal["track"]))
    if skirt:
        sk = L.box("skirt", (0.08, length * 0.92, wheel_r * 1.25), (x + 0.04 * (1 if x > 0 else -1), -0.02, wheel_r * 1.55), pal["olive"], 0.02)
        parts.append(sk)
        # panel seams on the skirt
        for i in range(1, 6):
            y = -length * 0.46 + length * 0.92 * i / 6
            parts.append(L.box("seam", (0.1, 0.02, wheel_r * 1.2), (x + 0.045 * (1 if x > 0 else -1), y, wheel_r * 1.55), pal["olive_dark"]))
    return parts


def tracked_hull(length, width, height, pal, track_w=0.5, n_wheels=7, wheel_r=0.32, skirt=True, deck_mat=None):
    """Hull centered on origin, front toward -Y."""
    parts = []
    tx = width / 2 - track_w / 2
    for side in (1, -1):
        parts += track_assembly(side * tx, length, wheel_r, n_wheels, track_w, pal, skirt)
    hb = wheel_r * 0.9  # hull bottom
    ht = hb + height
    inner = width - track_w * 2 + 0.1
    # lower hull between tracks
    parts.append(L.box("lower", (inner, length * 0.94, height * 0.6), (0, 0, hb + height * 0.3), pal["olive_dark"], 0.04))
    # upper hull: overhangs the tracks, sloped glacis at the front
    prof = [
        (-length / 2 + 0.05, hb + height * 0.45),
        (-length / 2 + 0.55, ht),
        (length / 2 - 0.12, ht),
        (length / 2, ht - 0.12),
        (length / 2, hb + height * 0.45),
    ]
    upper = L.prism("upper", [(p[0], p[1]) for p in prof], width - 0.06, (0, 0, 0), pal["olive"], "X", bevel=0.03)
    parts.append(upper)
    # engine deck grilles at the rear
    deck = deck_mat or pal["olive_dark"]
    for i in range(4):
        parts.append(L.box("grille", (width * 0.55, 0.12, 0.03), (0, length / 2 - 0.35 - i * 0.2, ht + 0.01), deck))
    # headlights & tow hooks
    for sx in (1, -1):
        parts.append(L.box("light", (0.14, 0.08, 0.08), (sx * (width / 2 - 0.35), -length / 2 + 0.3, ht - 0.05), pal["steel"], 0.01))
        parts.append(L.cyl("hook", 0.05, 0.12, (sx * 0.5, -length / 2 + 0.05, hb + height * 0.4), pal["gunmetal"], 8, (math.pi / 2, 0, 0)))
    # team stripe along the upper hull sides
    for sx in (1, -1):
        parts.append(L.box("stripe", (0.02, length * 0.55, 0.1), (sx * (width / 2 - 0.03 + 0.012), 0.1, ht - 0.12), pal["team"]))
    return parts, ht


def barrel(length, r, pos, pal, muzzle=True, evac=True):
    x, y, z = pos
    parts = [L.cyl("barrel", r, length, (x, y - length / 2, z), pal["olive_dark"], 14, (math.pi / 2, 0, 0))]
    if evac:
        parts.append(L.cyl("evac", r * 1.6, length * 0.12, (x, y - length * 0.55, z), pal["olive_dark"], 14, (math.pi / 2, 0, 0)))
    if muzzle:
        parts.append(L.cyl("muzzle", r * 1.35, 0.14, (x, y - length + 0.07, z), pal["gunmetal"], 14, (math.pi / 2, 0, 0)))
    return parts


def turret_shell(bottom, top, z0, z1, mat):
    """Faceted turret: loft between a lower and upper polygon (top view, (x, y))."""
    return L.loft("shell", [[(x, y, z0) for x, y in bottom], [(x, y, z1) for x, y in top]], mat)


def finish(objs_body, objs_turret, name_turret="turret", pivot=(0, 0, 0)):
    body = L.join(objs_body, "hull")
    L.smooth(body, 30)
    out = [body]
    if objs_turret:
        t = L.join(objs_turret, name_turret)
        L.smooth(t, 30)
        # put the turret origin on its pivot so Godot rotates around it
        import bpy
        for v in t.data.vertices:
            v.co -= Vector(pivot)
        t.location = pivot
        out.append(t)
    return out


def tank(pal):
    body, ht = tracked_hull(5.6, 3.0, 0.75, pal, 0.55, 7, 0.33)
    T = []
    tz = ht
    bottom = [(-1.2, -1.25), (1.2, -1.25), (1.45, -0.3), (1.4, 1.35), (-1.4, 1.35), (-1.45, -0.3)]
    top = [(-0.95, -1.05), (0.95, -1.05), (1.25, -0.25), (1.2, 1.25), (-1.2, 1.25), (-1.25, -0.25)]
    T.append(turret_shell(bottom, top, tz, tz + 0.62, pal["olive"]))
    T.append(L.box("bustle", (2.2, 0.7, 0.4), (0, 1.55, tz + 0.3), pal["olive_dark"], 0.03))
    T.append(L.box("mantlet", (0.7, 0.35, 0.42), (0, -1.15, tz + 0.32), pal["olive_dark"], 0.04))
    T += barrel(3.6, 0.085, (0, -1.3, tz + 0.34), pal)
    T.append(L.cyl("cupola", 0.3, 0.22, (0.55, 0.25, tz + 0.72), pal["olive"], 16, bevel=0.02))
    T.append(L.cyl("hatch", 0.24, 0.05, (0.55, 0.25, tz + 0.85), pal["olive_dark"], 16))
    T.append(L.box("mg", (0.06, 0.6, 0.06), (0.55, -0.2, tz + 0.9), pal["gunmetal"]))
    T.append(L.cyl("hatch2", 0.22, 0.08, (-0.5, 0.2, tz + 0.66), pal["olive_dark"], 14))
    T.append(L.box("sight", (0.35, 0.25, 0.25), (-0.6, -0.55, tz + 0.72), pal["olive_dark"], 0.03))
    T.append(L.box("sight_glass", (0.28, 0.02, 0.14), (-0.6, -0.68, tz + 0.74), pal["glass"]))
    for sx in (1, -1):
        for k in range(3):
            T.append(L.cyl("smoke", 0.06, 0.25, (sx * (1.15 + k * 0.04), -0.75 + k * 0.12, tz + 0.5), pal["olive_dark"], 8, (0.6, 0, 0)))
        T.append(L.box("team_panel", (0.03, 1.1, 0.32), (sx * 1.33, 0.4, tz + 0.3), pal["team"]))
    T.append(L.cyl("antenna", 0.012, 1.6, (-0.95, 1.3, tz + 1.3), pal["black"], 6))
    T.append(L.box("team_roof", (1.6, 1.3, 0.04), (-0.05, 0.75, tz + 0.63), pal["team"]))
    return finish(body, T, "turret", (0, 0, tz))


def aa_vehicle(pal):
    body, ht = tracked_hull(5.0, 2.8, 0.75, pal, 0.5, 6, 0.32)
    T = []
    tz = ht
    T.append(L.box("tbody", (1.8, 2.1, 0.95), (0, 0.15, tz + 0.48), pal["olive"], 0.06))
    for sx in (1, -1):
        T.append(L.box("gunpod", (0.35, 1.2, 0.5), (sx * 1.1, -0.2, tz + 0.6), pal["olive_dark"], 0.05))
        T += barrel(2.0, 0.045, (sx * 1.1, -0.75, tz + 0.65), pal, muzzle=True, evac=False)
        T.append(L.box("team_panel", (0.03, 0.9, 0.3), (sx * 0.91, 0.3, tz + 0.55), pal["team"]))
    # search radar dish on a mast at the back, tracking radar in front
    T.append(L.cyl("mast", 0.06, 0.6, (0, 0.95, tz + 1.25), pal["gunmetal"], 8))
    T.append(L.box("radar", (1.3, 0.12, 0.5), (0, 0.95, tz + 1.6), pal["olive_dark"], 0.04, rot=(0.25, 0, 0)))
    T.append(L.sphere("track_radar", 0.32, (0, -0.85, tz + 1.05), pal["olive_dark"], 16, 10, (1.0, 0.7, 1.0)))
    T.append(L.box("team_roof", (1.6, 1.5, 0.04), (0, 0.15, tz + 0.97), pal["team"]))
    return finish(body, T, "turret", (0, 0, tz))


def howitzer(pal):
    body, ht = tracked_hull(5.8, 3.0, 0.7, pal, 0.5, 7, 0.32)
    T = []
    tz = ht
    T.append(L.box("tbody", (2.5, 3.0, 1.15), (0, 0.45, tz + 0.58), pal["olive"], 0.07))
    T.append(L.box("roof", (2.2, 2.6, 0.12), (0, 0.5, tz + 1.2), pal["olive_dark"], 0.04))
    T.append(L.box("mantlet", (0.8, 0.4, 0.6), (0, -1.15, tz + 0.55), pal["olive_dark"], 0.05))
    T += barrel(4.4, 0.1, (0, -1.3, tz + 0.6), pal)
    T.append(L.cyl("cupola", 0.28, 0.25, (0.7, 0.8, tz + 1.36), pal["olive"], 14, bevel=0.02))
    T.append(L.box("mg", (0.06, 0.55, 0.06), (0.7, 0.45, tz + 1.5), pal["gunmetal"]))
    for sx in (1, -1):
        T.append(L.box("team_panel", (0.03, 1.6, 0.4), (sx * 1.26, 0.5, tz + 0.65), pal["team"]))
        T.append(L.box("door", (0.03, 0.7, 0.8), (sx * 1.26, 1.4, tz + 0.55), pal["olive_dark"], 0.01))
    T.append(L.box("team_roof", (2.0, 2.2, 0.04), (0, 0.5, tz + 1.27), pal["team"]))
    # travel lock at the hull front (hull)
    body.append(L.box("travel_lock", (0.3, 0.15, 0.5), (0, -2.75, ht + 0.1), pal["olive_dark"], 0.02))
    return finish(body, T, "turret", (0, 0, tz))


def wheel(r, w, pos, pal):
    x, y, z = pos
    return [
        L.cyl("tire", r, w, (x, y, z), pal["rubber"], 18, (0, math.pi / 2, 0), bevel=0.03),
        L.cyl("rim", r * 0.55, w * 1.04, (x, y, z), pal["olive_dark"], 12, (0, math.pi / 2, 0)),
    ]


def recon(pal):
    B = []
    L_, W = 4.6, 2.2
    for sx in (1, -1):
        for sy in (1, -1):
            B += wheel(0.45, 0.38, (sx * 0.98, sy * 1.45, 0.45), pal)
    # chassis + body
    B.append(L.box("chassis", (W - 0.2, L_ - 0.4, 0.35), (0, 0, 0.6), pal["olive_dark"], 0.04))
    prof = [(-L_ / 2, 0.75), (-L_ / 2, 1.15), (-L_ / 2 + 0.95, 1.28), (-0.15, 1.28), (0.0, 1.95), (L_ / 2 - 0.15, 1.95), (L_ / 2, 1.85), (L_ / 2, 0.75)]
    B.append(L.prism("body", [(p[0], p[1]) for p in prof], W, (0, 0, 0), pal["tan"], "X", bevel=0.05))
    # windshield & side windows
    B.append(L.box("windshield", (W - 0.3, 0.05, 0.5), (0, -0.05, 1.65), pal["glass"], 0.0, rot=(0.45, 0, 0)))
    for sx in (1, -1):
        B.append(L.box("window", (0.03, 1.2, 0.42), (sx * (W / 2 + 0.005), 0.75, 1.62), pal["glass"]))
        B.append(L.box("team_door", (0.03, 1.5, 0.35), (sx * (W / 2 + 0.01), 0.55, 1.05), pal["team"]))
        B.append(L.box("fender", (0.45, 1.0, 0.08), (sx * 0.98, -1.45, 1.0), pal["tan"], 0.02))
        B.append(L.box("fender_r", (0.45, 1.0, 0.08), (sx * 0.98, 1.45, 1.0), pal["tan"], 0.02))
    B.append(L.box("grille", (1.4, 0.05, 0.3), (0, -L_ / 2 - 0.01, 1.0), pal["black"]))
    for sx in (1, -1):
        B.append(L.box("lamp", (0.18, 0.04, 0.12), (sx * 0.8, -L_ / 2 - 0.02, 1.08), pal["steel"]))
    B.append(L.box("spare", (0.6, 0.3, 0.6), (0, L_ / 2 + 0.12, 1.25), pal["rubber"], 0.12))
    B.append(L.box("team_roof", (1.8, 1.6, 0.04), (0, 1.2, 1.97), pal["team"]))
    B.append(L.box("team_hood", (1.4, 1.0, 0.04), (0, -1.75, 1.3), pal["team"]))
    # roof gun ring with shield + MG (turret)
    T = [
        L.cyl("ring", 0.42, 0.18, (0, 0.6, 2.04), pal["olive_dark"], 16),
        L.box("shield", (0.9, 0.08, 0.45), (0, 0.3, 2.3), pal["olive"], 0.03),
        L.box("mg_body", (0.12, 0.6, 0.14), (0, 0.15, 2.35), pal["gunmetal"], 0.01),
        L.cyl("mg_barrel", 0.025, 0.7, (0, -0.4, 2.36), pal["gunmetal"], 8, (math.pi / 2, 0, 0)),
        L.box("ammo", (0.18, 0.2, 0.16), (0.16, 0.3, 2.22), pal["olive_dark"], 0.01),
    ]
    return finish(B, T, "turret", (0, 0.6, 2.0))


def at_gun(pal):
    B = []
    # split trail legs and wheels (hull part)
    for sx in (1, -1):
        B += wheel(0.48, 0.3, (sx * 1.0, 0.1, 0.48), pal)
        B.append(L.box("trail", (0.16, 2.6, 0.16), (sx * 0.45, 1.5, 0.3), pal["olive"], 0.03, rot=(0.08, 0, sx * -0.18)))
        B.append(L.box("spade", (0.4, 0.06, 0.3), (sx * 0.72, 2.75, 0.15), pal["olive_dark"], 0.02))
    B.append(L.cyl("axle", 0.08, 2.0, (0, 0.1, 0.48), pal["gunmetal"], 10, (0, math.pi / 2, 0)))
    B.append(L.box("carriage", (0.6, 0.8, 0.35), (0, 0.15, 0.75), pal["olive_dark"], 0.04))
    # gun shield + cradle + barrel (traverses as the turret)
    T = [
        L.box("shield", (2.0, 0.08, 1.05), (0, -0.3, 1.25), pal["olive"], 0.04),
        L.box("shield_top", (2.0, 0.35, 0.06), (0, -0.15, 1.78), pal["olive"], 0.02, rot=(0.6, 0, 0)),
        L.box("cradle", (0.42, 1.4, 0.38), (0, 0.15, 1.05), pal["olive_dark"], 0.04),
        L.box("breech", (0.38, 0.5, 0.34), (0, 0.85, 1.05), pal["gunmetal"], 0.03),
        L.box("team_panel", (0.9, 0.02, 0.3), (0, -0.345, 1.35), pal["team"]),
    ]
    T += barrel(3.4, 0.07, (0, -0.25, 1.08), pal, muzzle=True, evac=False)
    T.append(L.box("brake", (0.25, 0.18, 0.18), (0, -3.6, 1.08), pal["gunmetal"], 0.02))
    return finish(B, T, "turret", (0, 0.1, 0.9))


def build(key, out_dir, preview_dir=None):
    L.reset()
    L.clear_mat_cache()
    pal = L.P()
    objs = {"tank": tank, "aa_vehicle": aa_vehicle, "howitzer": howitzer, "recon": recon, "at_gun": at_gun}[key](pal)
    L.export_glb(f"{out_dir}/{key}.glb")
    if preview_dir:
        L.preview(f"{preview_dir}/{key}.png", 384, elev=26, azim=-35)

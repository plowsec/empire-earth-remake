"""Buildings. Footprint centered on the origin, front (entrance) toward -Y.
Footprint sizes in meters = tiles * 4 (keep a small margin)."""
import math

from mathutils import Vector

import eelib as L

KEYS = ["capitol", "settlement", "house", "granary", "farm", "barracks", "tank_factory", "airport", "naval_yard", "hospital", "guard_tower", "aa_site"]


def flag(pos, pal, height=6.0, size=(1.6, 1.0)):
    x, y, z = pos
    return [
        L.cyl("pole", 0.06, height, (x, y, z + height / 2), pal["steel"], 8),
        L.sphere("finial", 0.1, (x, y, z + height + 0.05), pal["gold"], 8, 6),
        L.box("flag", (0.04, size[0], size[1]), (x, y + size[0] / 2 + 0.06, z + height - size[1] / 2 - 0.1), pal["team"]),
    ]


def windows_row(x0, x1, y, z, n, w, h, pal, axis="X", depth=0.06, frame=True):
    """Row of inset windows on a wall facing -Y (axis X) or ±X (axis Y)."""
    out = []
    for i in range(n):
        t = (i + 0.5) / n
        if axis == "X":
            x = x0 + (x1 - x0) * t
            out.append(L.box("win", (w, depth, h), (x, y, z), pal["glass"]))
            if frame:
                out.append(L.box("sill", (w + 0.12, depth + 0.06, 0.06), (x, y, z - h / 2 - 0.04), pal["white"]))
        else:
            yy = x0 + (x1 - x0) * t
            out.append(L.box("win", (depth, w, h), (y, yy, z), pal["glass"]))
            if frame:
                out.append(L.box("sill", (depth + 0.06, w + 0.12, 0.06), (y, yy, z - h / 2 - 0.04), pal["white"]))
    return out


def gable_roof(w, d, z, h, mat, overhang=0.3, axis="X"):
    """Gabled roof: ridge along X (axis X) over a w x d footprint."""
    if axis == "X":
        pts = [(-d / 2 - overhang, 0), (d / 2 + overhang, 0), (0, h)]
        r = L.prism("roof", pts, w + overhang * 2, (0, 0, z), mat, "X")
    else:
        pts = [(-w / 2 - overhang, 0), (w / 2 + overhang, 0), (0, h)]
        r = L.prism("roof", pts, d + overhang * 2, (0, 0, z), mat, "Y")
    return r


def quonset(length, radius, pos, pal, mat, axis="Y"):
    x, y, z = pos
    rot = (math.pi / 2, 0, 0) if axis == "Y" else (0, math.pi / 2, 0)
    hut = L.cyl("quonset", radius, length, (x, y, z), mat, 20, rot)
    # cut the bottom half by flattening verts below z
    L.apply_all(hut)
    for v in hut.data.vertices:
        if v.co.z < z:
            v.co.z = z
    parts = [hut]
    # ribs
    n = int(length / 1.2)
    for i in range(n + 1):
        t = -length / 2 + length * i / n
        if axis == "Y":
            rib = L.torus("rib", radius + 0.02, 0.04, (x, y + t, z), pal["metal_sheet"], (math.pi / 2, 0, 0), 20, 4)
        else:
            rib = L.torus("rib", radius + 0.02, 0.04, (x + t, y, z), pal["metal_sheet"], (0, math.pi / 2, 0), 20, 4)
        L.apply_all(rib)
        for v in rib.data.vertices:
            if v.co.z < z:
                v.co.z = z
        parts.append(rib)
    return parts


def sandbags(cx, cy, r, z, pal, n=14, arc=2 * math.pi, start=0.0, layers=2):
    out = []
    for layer in range(layers):
        for i in range(n):
            a = start + arc * (i + 0.5 * (layer % 2)) / n
            out.append(L.box("bag", (0.7, 0.38, 0.28), (cx + math.cos(a) * r, cy + math.sin(a) * r, z + 0.14 + layer * 0.26), pal["canvas"], 0.1, rot=(0, 0, a + math.pi / 2)))
    return out


def ground_pad(w, d, pal, mat=None, h=0.15):
    return L.box("pad", (w, d, h), (0, 0, h / 2 - 0.05), mat or pal["concrete_dark"], 0.05)


def capitol(pal):
    P = [ground_pad(15.6, 15.6, pal, pal["stone_light"], 0.3)]
    P.append(L.box("podium", (13.5, 11.5, 1.2), (0, 0.6, 0.75), pal["marble"], 0.08))
    for k in range(5):
        P.append(L.box("step", (6.5, 0.45, 0.24), (0, -5.25 - k * 0.42 + 0.0, 1.25 - k * 0.24), pal["marble"], 0.02))
    P.append(L.box("hall", (10.0, 7.0, 5.0), (0, 1.6, 3.85), pal["marble"], 0.06))
    P.append(L.box("cornice", (10.6, 7.6, 0.4), (0, 1.6, 6.5), pal["white"], 0.05))
    # portico with columns and pediment
    for i in range(8):
        x = -4.2 + i * 1.2
        P.append(L.cyl("column", 0.28, 4.6, (x, -2.6, 3.65), pal["marble"], 16, bevel=0.0))
        P.append(L.box("capital", (0.75, 0.75, 0.22), (x, -2.6, 6.05), pal["white"], 0.03))
        P.append(L.box("base", (0.7, 0.7, 0.2), (x, -2.6, 1.45), pal["white"], 0.03))
    P.append(L.box("entablature", (10.0, 1.4, 0.6), (0, -2.6, 6.45), pal["white"], 0.04))
    P.append(L.prism("pediment", [(-5.0, 0), (5.0, 0), (0, 1.6)], 1.4, (0, -2.6, 6.75), pal["marble"], "Y"))
    for i in range(4):
        P += windows_row(-4.4, 4.4, -1.93 + 0.0, 2.5 + i * 0 + 0.6, 6, 0.7, 1.8, pal) if i == 0 else []
    # wings
    for s in (1, -1):
        P.append(L.box("wing", (2.6, 8.0, 3.8), (s * 6.0, 1.8, 3.25), pal["marble"], 0.05))
        P.append(L.box("wing_roof", (2.9, 8.3, 0.3), (s * 6.0, 1.8, 5.25), pal["white"], 0.04))
        P += windows_row(-1.5, 5.0, s * 7.31, 3.3, 4, 0.7, 1.6, pal, axis="Y")
    # drum + dome
    P.append(L.cyl("drum", 3.0, 2.2, (0, 2.2, 7.8), pal["marble"], 32))
    for i in range(16):
        a = 2 * math.pi * i / 16
        P.append(L.cyl("drum_col", 0.16, 2.0, (math.cos(a) * 3.15, 2.2 + math.sin(a) * 3.15, 7.8), pal["white"], 10))
    dome = L.sphere("dome", 3.1, (0, 2.2, 8.9), pal["team_metal"], 32, 16)
    L.apply_all(dome)
    for v in dome.data.vertices:
        if v.co.z < 8.9:
            v.co.z = 8.9
    P.append(dome)
    P.append(L.cyl("lantern", 0.6, 1.2, (0, 2.2, 12.4), pal["marble"], 16))
    P.append(L.sphere("lantern_top", 0.62, (0, 2.2, 13.0), pal["team_metal"], 16, 8, (1, 1, 0.8)))
    P += flag((0, 2.2, 13.3), pal, 2.4, (1.4, 0.9))
    for s in (1, -1):
        P += flag((s * 5.8, -6.6, 0.25), pal, 5.0, (1.2, 0.8))
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def settlement(pal):
    P = [ground_pad(11.6, 11.6, pal, pal["stone_light"], 0.2)]
    P.append(L.box("main", (8.5, 6.0, 4.8), (0, 1.2, 2.5), pal["brick"], 0.05))
    P.append(L.box("trim", (8.8, 6.3, 0.35), (0, 1.2, 4.95), pal["white"], 0.04))
    P.append(gable_roof(8.5, 6.0, 5.1, 2.2, pal["roof_gray"]))
    P[-1].location.y = 1.2
    P += windows_row(-3.6, 3.6, -1.82, 3.4, 5, 0.75, 1.3, pal)
    P += windows_row(-3.6, 3.6, -1.82, 1.5, 4, 0.75, 1.3, pal)
    P.append(L.box("door", (1.4, 0.1, 2.2), (0, -1.85, 1.2), pal["wood_dark"]))
    P.append(L.box("canopy", (2.4, 1.2, 0.15), (0, -2.4, 2.6), pal["team"], 0.03))
    for s in (1, -1):
        P.append(L.cyl("canopy_post", 0.07, 2.4, (s * 1.1, -2.9, 1.3), pal["white"], 8))
    # clock tower
    P.append(L.box("tower", (2.4, 2.4, 9.0), (3.6, 3.5, 4.6), pal["brick"], 0.05))
    P.append(L.box("tower_trim", (2.7, 2.7, 0.3), (3.6, 3.5, 9.2), pal["white"], 0.03))
    P.append(L.cyl("clock", 0.7, 0.1, (3.6, 2.28, 7.8), pal["white"], 20, (math.pi / 2, 0, 0)))
    P.append(L.box("clock_hand", (0.06, 0.12, 0.5), (3.6, 2.22, 7.95), pal["black"]))
    P.append(L.prism("spire", [(-1.35, 0), (1.35, 0), (0, 2.6)], 2.7, (3.6, 3.5, 9.35), pal["team_metal"], "X"))
    P.append(L.prism("spire2", [(-1.35, 0), (1.35, 0), (0, 2.6)], 2.7, (3.6, 3.5, 9.35), pal["team_metal"], "Y"))
    P += flag((-4.6, -4.6, 0.2), pal, 5.0)
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def house(pal):
    P = [ground_pad(7.6, 7.6, pal, pal["stone_light"], 0.12)]
    P.append(L.box("walls", (5.6, 4.6, 4.4), (0, 0.4, 2.25), pal["white"], 0.04))
    roof = gable_roof(5.6, 4.6, 4.45, 2.0, pal["roof_red"], 0.4)
    roof.location.y = 0.4
    P.append(roof)
    P.append(L.box("chimney", (0.6, 0.6, 2.2), (1.6, 1.2, 5.6), pal["brick"], 0.03))
    P += windows_row(-2.2, 2.2, -1.92, 3.3, 3, 0.8, 0.9, pal)
    P += windows_row(-2.2, -0.6, -1.92, 1.4, 1, 0.9, 1.1, pal)
    P += windows_row(0.8, 2.2, -1.92, 1.4, 1, 0.9, 1.1, pal)
    P.append(L.box("door", (0.95, 0.1, 2.0), (0.1, -1.95, 1.1), pal["team"]))
    P.append(L.box("porch", (2.2, 1.1, 0.12), (0.1, -2.5, 2.4), pal["white"], 0.02))
    for s in (1, -1):
        P.append(L.cyl("post", 0.06, 2.3, (0.1 + s * 0.95, -2.95, 1.2), pal["white"], 8))
        P += windows_row(-1.3, 2.0, s * 2.82, 2.3, 2, 0.8, 1.0, pal, axis="Y")
    # garden fence
    for i in range(12):
        x = -3.5 + i * 0.64
        P.append(L.box("picket", (0.08, 0.06, 0.8), (x, -3.55, 0.45), pal["white"]))
    P.append(L.box("rail", (7.2, 0.05, 0.08), (0, -3.55, 0.6), pal["white"]))
    P.append(L.ico("shrub", 0.7, (-2.6, -2.6, 0.5), pal["leaf_light"], 1))
    P.append(L.ico("shrub2", 0.6, (2.7, -2.5, 0.45), pal["leaf"], 1))
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def granary(pal):
    P = [ground_pad(11.6, 11.6, pal, pal["concrete"], 0.15)]
    for k, (x, y) in enumerate([(-2.6, 1.8), (0.9, 2.6)]):
        P.append(L.cyl("silo", 1.7, 7.5, (x, y, 3.85), pal["metal_sheet"], 28))
        P.append(L.cyl("silo_cone", 1.75, 1.4, (x, y, 8.3), pal["metal_sheet"], 28, r2=0.25))
        for b in range(4):
            P.append(L.torus("band", 1.72, 0.05, (x, y, 1.2 + b * 1.8), pal["steel"], (0, 0, 0), 28, 4))
        P.append(L.box("ladder", (0.4, 0.06, 7.0), (x, y - 1.75, 3.7), pal["steel"]))
    # barn
    P.append(L.box("barn", (5.0, 4.2, 3.4), (2.4, -2.5, 1.8), pal["hull_red"], 0.04))
    gam = [(-2.25, 0), (2.25, 0), (1.6, 1.2), (0, 2.1), (-1.6, 1.2)]
    P.append(L.prism("gambrel", gam, 5.2, (2.4, -2.5, 3.5), pal["roof_gray"], "X"))
    P.append(L.box("barn_door", (2.0, 0.1, 2.6), (2.4, -4.62, 1.4), pal["white"]))
    P.append(L.box("barn_x1", (2.2, 0.12, 0.12), (2.4, -4.66, 1.4), pal["hull_red"], 0, rot=(0, 0.9, 0)))
    P.append(L.box("barn_x2", (2.2, 0.12, 0.12), (2.4, -4.66, 1.4), pal["hull_red"], 0, rot=(0, -0.9, 0)))
    P.append(L.cyl("conveyor", 0.25, 5.0, (-0.8, 0.2, 5.0), pal["steel"], 10, (0.0, 1.0, 0.3)))
    P += flag((-4.8, -4.8, 0.15), pal, 4.5)
    for k in range(4):
        P.append(L.box("sack", (0.7, 0.5, 0.35), (-3.2 + k * 0.75, -4.0, 0.3), pal["canvas"], 0.1))
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def farm(pal):
    P = [L.box("soil", (11.6, 11.6, 0.25), (0, 0, 0.06), pal["soil"], 0.1)]
    for i in range(10):
        x = -5.0 + i * 1.11
        P.append(L.box("row", (0.55, 10.6, 0.35), (x, 0, 0.28), pal["crop_green"] if i % 2 == 0 else pal["crop"], 0.15))
    # fence posts & rails
    for i in range(9):
        t = -5.6 + i * 1.4
        for (x, y) in ((t, -5.75), (t, 5.75), (-5.75, t), (5.75, t)):
            P.append(L.box("post", (0.12, 0.12, 0.9), (x, y, 0.45), pal["wood"]))
    for (x, y, sx, sy) in ((0, -5.75, 11.5, 0.06), (0, 5.75, 11.5, 0.06), (-5.75, 0, 0.06, 11.5), (5.75, 0, 0.06, 11.5)):
        P.append(L.box("rail", (sx, sy, 0.08), (x, y, 0.7), pal["wood"]))
    # scarecrow in team colors
    P.append(L.cyl("sc_pole", 0.05, 2.2, (3.8, 3.8, 1.1), pal["wood"], 6))
    P.append(L.box("sc_arms", (1.2, 0.06, 0.06), (3.8, 3.8, 1.7), pal["wood"]))
    P.append(L.box("sc_shirt", (0.6, 0.25, 0.7), (3.8, 3.8, 1.45), pal["team"], 0.05))
    P.append(L.sphere("sc_head", 0.18, (3.8, 3.8, 2.0), pal["canvas"], 10, 6))
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def barracks(pal):
    P = [ground_pad(11.6, 11.6, pal, pal["concrete_dark"], 0.15)]
    P += quonset(9.0, 1.9, (-2.6, 0.6, 0.1), pal, pal["olive"], "Y")
    P += quonset(9.0, 1.9, (2.0, 0.6, 0.1), pal, pal["olive"], "Y")
    for x in (-2.6, 2.0):
        P.append(L.box("endwall", (3.6, 0.15, 1.85), (x, -3.92, 1.0), pal["olive_dark"], 0.02))
        P.append(L.box("door", (1.1, 0.12, 1.6), (x, -4.0, 0.9), pal["team"]))
    P += sandbags(4.3, -4.3, 1.1, 0.05, pal, 10, math.pi, math.pi / 2)
    P += sandbags(-4.6, -4.6, 0.9, 0.05, pal, 8, math.pi, 0.0)
    P += flag((4.8, 4.6, 0.1), pal, 6.5)
    P.append(L.box("crate1", (0.9, 0.9, 0.9), (-4.8, 4.6, 0.5), pal["wood"], 0.04))
    P.append(L.box("crate2", (0.8, 0.8, 0.8), (-3.9, 4.8, 0.45), pal["olive_dark"], 0.04))
    P.append(L.cyl("drum", 0.32, 0.9, (-4.9, 3.6, 0.5), pal["olive_dark"], 12))
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def tank_factory(pal):
    P = [ground_pad(15.6, 15.6, pal, pal["concrete_dark"], 0.15)]
    P.append(L.box("hall", (13.0, 10.0, 5.6), (0, 1.5, 2.9), pal["concrete"], 0.06))
    # sawtooth roof
    for i in range(5):
        y = -3.0 + i * 2.0
        P.append(L.prism("tooth", [(0, 0), (2.0, 0), (2.0, 1.4)], 13.0, (0, y, 5.7), pal["metal_sheet"], "X"))
        P.append(L.box("skylight", (12.6, 0.06, 1.2), (0, y + 2.0 - 0.03, 6.35), pal["glass"]))
    P.append(L.box("door", (6.0, 0.15, 4.4), (0, -3.55, 2.3), pal["metal_sheet"], 0.02))
    for k in range(8):
        P.append(L.box("door_rib", (6.0, 0.2, 0.06), (0, -3.6, 0.4 + k * 0.55), pal["steel"]))
    P.append(L.box("door_frame", (6.6, 0.3, 0.5), (0, -3.6, 4.7), pal["yellow_paint"], 0.03))
    P.append(L.box("team_band", (13.04, 10.04, 0.6), (0, 1.5, 4.9), pal["team"]))
    for s in (1, -1):
        P.append(L.cyl("stack", 0.55, 9.0, (s * 4.6, 5.6, 4.6), pal["brick"], 16, r2=0.45))
        P.append(L.torus("stack_band", 0.5, 0.06, (s * 4.6, 5.6, 8.2), pal["black"], (0, 0, 0), 16, 4))
    P.append(L.box("annex", (3.2, 3.4, 3.0), (5.8, -4.6, 1.6), pal["concrete"], 0.05))
    P += windows_row(4.6, 7.0, -6.32, 2.0, 2, 0.8, 0.9, pal)
    P += flag((-6.8, -6.8, 0.1), pal, 6.0)
    for k in range(3):
        P.append(L.cyl("tank_drum", 0.3, 0.9, (-6.5 + k * 0.7, -4.4, 0.5), pal["olive_dark"], 12))
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def airport(pal):
    # 20 x 16 m footprint
    P = [L.box("apron", (19.6, 15.6, 0.15), (0, 0, 0.03), pal["concrete_dark"], 0.05)]
    P.append(L.box("runway", (19.6, 4.6, 0.08), (0, -4.6, 0.14), pal["asphalt"]))
    for i in range(9):
        P.append(L.box("dash", (1.2, 0.2, 0.02), (-8.4 + i * 2.1, -4.6, 0.19), pal["runway_mark"]))
    for s in (1, -1):
        P.append(L.box("edge", (19.4, 0.12, 0.02), (0, -4.6 + s * 2.1, 0.19), pal["runway_mark"]))
        for k in range(4):
            P.append(L.box("thresh", (0.8, 0.25, 0.02), (s * 9.0, -4.6 - 1.2 + k * 0.8, 0.19), pal["runway_mark"]))
    # hangar
    P += quonset(7.0, 3.0, (-4.5, 3.8, 0.1), pal, pal["metal_sheet"], "X")
    P.append(L.box("hangar_door", (0.15, 5.6, 2.8), (-1.0, 3.8, 1.5), pal["team"]))
    # control tower
    P.append(L.box("tower_base", (3.2, 3.2, 2.6), (5.6, 4.4, 1.4), pal["concrete"], 0.05))
    P.append(L.cyl("tower_shaft", 0.8, 5.0, (5.6, 4.4, 5.1), pal["white"], 16))
    P.append(L.cyl("cab_floor", 1.6, 0.25, (5.6, 4.4, 7.7), pal["concrete"], 16))
    P.append(L.cyl("cab", 1.45, 1.3, (5.6, 4.4, 8.45), pal["glass"], 16, r2=1.6))
    P.append(L.cyl("cab_roof", 1.75, 0.25, (5.6, 4.4, 9.2), pal["team"], 16))
    P.append(L.cyl("beacon", 0.15, 0.6, (5.6, 4.4, 9.6), pal["light_warm"], 8))
    P.append(L.box("radar", (1.4, 0.12, 0.45), (8.0, 1.8, 3.5), pal["white"], 0.03))
    P.append(L.cyl("radar_mast", 0.08, 3.0, (8.0, 1.8, 1.6), pal["steel"], 8))
    P.append(L.cyl("windsock_pole", 0.04, 2.5, (-8.6, 0.8, 1.3), pal["steel"], 6))
    P.append(L.cyl("windsock", 0.18, 0.9, (-8.6, 1.3, 2.4), pal["yellow_paint"], 10, (math.pi / 2, 0, 0), r2=0.08))
    for k in range(2):
        P.append(L.box("fuel", (1.6, 0.9, 0.9), (1.5 + k * 1.8, 6.9, 0.55), pal["white"], 0.15))
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def naval_yard(pal):
    P = []
    # pilings so the platform never floats over water
    for x in (-6.5, -2.5, 2.5, 6.5):
        for y in (-6.5, -2.5, 2.5, 6.5):
            P.append(L.cyl("piling", 0.3, 5.0, (x, y, -2.2), pal["concrete_dark"], 10))
    P.append(L.box("deck", (15.6, 15.6, 0.8), (0, 0, 0.1), pal["concrete"], 0.06))
    P.append(L.box("quay_edge", (15.6, 0.4, 0.3), (0, -7.6, 0.6), pal["yellow_paint"], 0.03))
    for k in range(5):
        P.append(L.cyl("bollard", 0.18, 0.4, (-6.0 + k * 3.0, -7.0, 0.7), pal["black"], 10))
    # warehouse
    P.append(L.box("shed", (8.0, 5.0, 4.0), (-2.5, 3.8, 2.5), pal["metal_sheet"], 0.05))
    roof = gable_roof(8.0, 5.0, 4.5, 1.4, pal["roof_gray"], 0.3)
    roof.location = (-2.5, 3.8, 4.5)
    P.append(roof)
    P.append(L.box("shed_door", (3.0, 0.12, 3.0), (-2.5, 1.25, 2.0), pal["team"]))
    # gantry crane (yellow)
    for s in (1, -1):
        P.append(L.box("leg", (0.35, 0.35, 8.0), (4.6 + s * 1.8, -4.0, 4.5), pal["yellow_paint"], 0.03))
        P.append(L.box("leg2", (0.35, 0.35, 8.0), (4.6 + s * 1.8, 0.0, 4.5), pal["yellow_paint"], 0.03))
    P.append(L.box("beam", (4.4, 0.5, 0.6), (4.6, -4.0, 8.5), pal["yellow_paint"], 0.03))
    P.append(L.box("beam2", (4.4, 0.5, 0.6), (4.6, 0.0, 8.5), pal["yellow_paint"], 0.03))
    P.append(L.box("boom", (0.6, 9.0, 0.6), (4.6, -5.5, 8.9), pal["yellow_paint"], 0.03))
    P.append(L.box("cab", (1.2, 1.2, 1.0), (4.6, -2.0, 8.0), pal["team"], 0.05))
    P.append(L.cyl("cable", 0.03, 3.0, (4.6, -9.0, 7.3), pal["black"], 6))
    # containers
    for k, col in enumerate(["team", "hull_red", "navy_gray", "olive"]):
        P.append(L.box("container", (2.4, 1.1, 1.1), (-5.5 + (k % 2) * 2.5, -3.2 - (k // 2) * 1.2, 1.1), pal[col], 0.03))
    P += flag((-7.0, -7.0, 0.5), pal, 6.0)
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def hospital(pal):
    P = [ground_pad(11.6, 11.6, pal, pal["stone_light"], 0.15)]
    P.append(L.box("block", (9.0, 6.5, 7.0), (0, 1.0, 3.6), pal["white"], 0.05))
    for f in range(3):
        P += windows_row(-4.0, 4.0, -2.27, 1.6 + f * 2.2, 7, 0.7, 1.1, pal, frame=False)
    P.append(L.box("entrance", (3.0, 1.6, 2.6), (0, -2.9, 1.4), pal["white"], 0.04))
    P.append(L.box("entrance_glass", (2.0, 0.06, 1.8), (0, -3.72, 1.0), pal["glass"]))
    P.append(L.box("sign", (2.2, 0.15, 2.2), (2.6, -2.35, 5.6), pal["white"], 0.03))
    P.append(L.box("cross_v", (0.5, 0.06, 1.6), (2.6, -2.45, 5.6), pal["red_cross"]))
    P.append(L.box("cross_h", (1.6, 0.06, 0.5), (2.6, -2.45, 5.6), pal["red_cross"]))
    P.append(L.cyl("helipad", 2.6, 0.12, (0, 1.0, 7.2), pal["concrete_dark"], 28))
    P.append(L.box("H1", (0.35, 1.6, 0.03), (-0.55, 1.0, 7.28), pal["white"]))
    P.append(L.box("H2", (0.35, 1.6, 0.03), (0.55, 1.0, 7.28), pal["white"]))
    P.append(L.box("H3", (1.1, 0.3, 0.03), (0, 1.0, 7.28), pal["white"]))
    P.append(L.box("team_band", (9.04, 6.54, 0.4), (0, 1.0, 7.0), pal["team"]))
    P.append(L.box("ambulance", (1.6, 3.2, 1.5), (-3.6, -4.3, 0.85), pal["white"], 0.15))
    P.append(L.box("amb_stripe", (1.62, 3.0, 0.2), (-3.6, -4.3, 0.95), pal["red_cross"]))
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def guard_tower(pal):
    P = [ground_pad(7.6, 7.6, pal, pal["concrete_dark"], 0.15)]
    P.append(L.box("bunker", (5.0, 5.0, 2.2), (0, 0, 1.15), pal["concrete"], 0.15))
    P.append(L.box("slit", (3.0, 0.08, 0.3), (0, -2.52, 1.6), pal["black"]))
    P.append(L.box("shaft", (2.4, 2.4, 4.4), (0, 0, 4.4), pal["concrete"], 0.08))
    P.append(L.box("deck", (3.6, 3.6, 0.3), (0, 0, 6.7), pal["concrete_dark"], 0.05))
    P.append(L.box("team_band", (2.44, 2.44, 0.5), (0, 0, 5.6), pal["team"]))
    P.append(L.box("ladder", (0.5, 0.06, 6.0), (1.0, 1.24, 3.6), pal["steel"]))
    P += sandbags(0, 0, 3.0, 0.05, pal, 18, 2 * math.pi, 0, 1)
    body = L.join(P, "body")
    L.smooth(body, 30)
    T = sandbags(0, 0, 1.4, 0.0, pal, 10, 2 * math.pi, 0, 2)
    T.append(L.box("mg", (0.18, 1.3, 0.22), (0, -0.5, 0.7), pal["gunmetal"], 0.02))
    T.append(L.cyl("mg_barrel", 0.05, 1.0, (0, -1.5, 0.72), pal["gunmetal"], 8, (math.pi / 2, 0, 0)))
    T.append(L.box("roof", (3.2, 3.2, 0.15), (0, 0, 2.2), pal["canvas"], 0.05))
    for sx in (1, -1):
        for sy in (1, -1):
            T.append(L.cyl("roof_post", 0.06, 2.0, (sx * 1.4, sy * 1.4, 1.1), pal["wood"], 6))
    t = L.join(T, "turret")
    t.location = (0, 0, 6.85)
    L.smooth(t, 30)
    return [body, t]


def aa_site(pal):
    P = [ground_pad(7.6, 7.6, pal, pal["concrete_dark"], 0.15)]
    P += sandbags(0, 0, 3.3, 0.05, pal, 20, 2 * math.pi, 0, 2)
    P.append(L.cyl("pad", 2.0, 0.3, (0, 0, 0.2), pal["concrete"], 24))
    P.append(L.cyl("radar_mast", 0.08, 3.4, (2.6, 2.6, 1.8), pal["steel"], 8))
    P.append(L.box("radar", (1.4, 0.14, 0.6), (2.6, 2.6, 3.6), pal["white"], 0.03, rot=(0.3, 0, 0.8)))
    P.append(L.box("console", (1.0, 0.7, 1.0), (-2.4, 2.4, 0.6), pal["olive_dark"], 0.05))
    body = L.join(P, "body")
    L.smooth(body, 30)
    T = [
        L.cyl("turntable", 1.0, 0.4, (0, 0, 0.2), pal["olive_dark"], 20),
        L.box("cradle", (1.6, 0.8, 0.6), (0, 0, 0.7), pal["olive"], 0.05),
    ]
    for sx in (1, -1):
        for sz in (0, 1):
            T.append(L.cyl("missile", 0.14, 2.6, (sx * 0.45, -0.2, 1.15 + sz * 0.42), pal["white"], 12, (math.pi / 2 - 0.55, 0, 0)))
            T.append(L.cyl("nose", 0.14, 0.4, (sx * 0.45, -1.45, 1.95 + sz * 0.42), pal["team"], 12, (math.pi / 2 - 0.55, 0, 0), r2=0.01))
    t = L.join(T, "turret")
    t.location = (0, 0, 0.35)
    L.smooth(t, 30)
    return [body, t]


def build(key, out_dir, preview_dir=None):
    L.reset()
    L.clear_mat_cache()
    pal = L.P()
    globals()[key](pal)
    L.export_glb(f"{out_dir}/{key}.glb")
    if preview_dir:
        L.preview(f"{preview_dir}/{key}.png", 384, elev=34, azim=-35)

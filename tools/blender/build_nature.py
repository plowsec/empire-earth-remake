"""Trees, bushes and resource deposits."""
import math
import random

from mathutils import Vector, noise

import eelib as L

KEYS = ["tree", "tree_pine", "tree_palm", "berries", "gold_mine", "stone_mine", "iron_mine", "deco_grass", "deco_flowers", "deco_rock", "deco_bush", "deco_reeds"]


def displace(ob, amount, scale=1.0, seed=0):
    """Organic noise displacement along vertex normals."""
    L.apply_all(ob)
    me = ob.data
    off = Vector((seed * 13.1, seed * 7.7, seed * 3.3))
    for v in me.vertices:
        n = noise.noise(v.co * scale + off)
        v.co += v.normal * n * amount


def crown_blob(r, pos, mat, seed, squash=0.85):
    b = L.ico("crown", r, pos, mat, 2, (1.0, 1.0, squash))
    displace(b, r * 0.32, 1.3 / r, seed)
    return b


def tree(pal):
    rnd = random.Random(3)
    P = [L.cyl("trunk", 0.32, 4.2, (0, 0, 2.1), pal["bark"], 10, r2=0.2)]
    # a couple of branches
    for k in range(3):
        a = k * 2.1 + 0.4
        P.append(L.cyl("branch", 0.1, 2.0, (math.cos(a) * 0.6, math.sin(a) * 0.6, 3.6), pal["bark"], 6, (0.9 * math.sin(a), -0.9 * math.cos(a), 0), r2=0.05))
    # Broken, asymmetric branch clusters leave gaps through the canopy.
    # Smaller angular lobes avoid the old six overlapping spherical crowns.
    for i in range(17):
        a = i * 2.399
        r = rnd.uniform(0.8, 2.1)
        z = rnd.uniform(4.3, 6.8)
        x, y = math.cos(a) * r, math.sin(a) * r
        end = Vector((x, y, z))
        start = Vector((0, 0, 3.0 + i % 3 * 0.45))
        branch = L.cyl("branch", 0.08, (end-start).length, (start+end)/2, pal["bark"], 6, r2=0.025)
        branch.rotation_euler = (end-start).to_track_quat("Z", "Y").to_euler()
        P.append(branch)
        for j in range(2):
            radius = rnd.uniform(0.65, 1.05)
            pos = (x + rnd.uniform(-0.45, 0.45), y + rnd.uniform(-0.45, 0.45), z + j * 0.45)
            crown = L.ico("leaves", radius, pos, pal["leaf_light"] if i % 4 == 0 else pal["leaf"], 1, (1.0, 0.8, 0.7))
            displace(crown, radius * 0.5, 2.0 / radius, i * 2 + j)
            P.append(crown)
    ob = L.join(P, "body")
    L.smooth(ob, 35)
    return [ob]


def tree_pine(pal):
    P = [L.cyl("trunk", 0.25, 3.0, (0, 0, 1.5), pal["bark"], 8, r2=0.16)]
    tiers = [(1.8, 2.4, 2.4), (3.4, 2.0, 2.2), (4.9, 1.6, 2.0), (6.3, 1.15, 1.8), (7.5, 0.7, 1.5)]
    for i, (z, r, h) in enumerate(tiers):
        c = L.cyl("tier", r, h, (0, 0, z + h / 2), pal["pine"], 12, r2=0.05)
        displace(c, 0.18, 1.4, i + 10)
        P.append(c)
    ob = L.join(P, "body")
    L.smooth(ob, 60)
    return [ob]


def tree_palm(pal):
    P = []
    # curved trunk from segments
    pts = []
    for i in range(9):
        t = i / 8
        pts.append(Vector((math.sin(t * 1.2) * 1.1, 0.0, t * 7.2)))
    for a, b in zip(pts[:-1], pts[1:]):
        d = b - a
        seg = L.cyl("seg", 0.24, d.length * 1.08, ((a + b) / 2), pal["bark_palm"], 10, r2=0.21)
        seg.rotation_euler = d.to_track_quat("Z", "Y").to_euler()
        P.append(seg)
        P.append(L.torus("ring", 0.24, 0.035, tuple(b), pal["bark_palm"], tuple(d.to_track_quat("Z", "Y").to_euler()), 10, 4))
    top = pts[-1]
    for k in range(8):
        a = k * 2 * math.pi / 8 + 0.2
        # frond: a bent, tapered strip
        secs = []
        for i in range(7):
            t = i / 6
            r = t * 3.6
            droop = -1.6 * t * t + 0.6 * t
            w = 0.55 * math.sin(math.pi * min(1.0, t * 1.2 + 0.05)) + 0.05
            cx = top.x + math.cos(a) * r
            cy = top.y + math.sin(a) * r
            cz = top.z + droop
            px, py = -math.sin(a) * w, math.cos(a) * w
            secs.append([(cx + px, cy + py, cz), (cx, cy, cz + 0.06), (cx - px, cy - py, cz), (cx, cy, cz - 0.04)])
        P.append(L.loft("frond", secs, pal["palm"]))
    for k in range(4):
        a = k * math.pi / 2
        P.append(L.sphere("coconut", 0.17, (top.x + math.cos(a) * 0.25, top.y + math.sin(a) * 0.25, top.z - 0.25), pal["wood_dark"], 8, 6))
    ob = L.join(P, "body")
    L.smooth(ob, 60)
    return [ob]


def berries(pal):
    rnd = random.Random(5)
    P = []
    for i, (x, y, r) in enumerate([(0, 0, 0.9), (0.7, 0.3, 0.65), (-0.6, 0.4, 0.6), (0.1, -0.6, 0.6)]):
        P.append(crown_blob(r, (x, y, r * 0.7), pal["leaf"], i + 20, 0.8))
    for k in range(26):
        a = rnd.uniform(0, 2 * math.pi)
        rr = rnd.uniform(0.3, 1.0)
        P.append(L.sphere("berry", 0.09, (math.cos(a) * rr, math.sin(a) * rr, rnd.uniform(0.4, 1.1)), pal["berry"], 6, 4))
    ob = L.join(P, "body")
    L.smooth(ob, 80)
    return [ob]


def rock_pile(pal, rock_mat, ore_mat, seed, n_rocks=9, n_ore=10, blocky=False, size=3.6):
    rnd = random.Random(seed)
    P = []
    for i in range(n_rocks):
        a = rnd.uniform(0, 2 * math.pi)
        rr = rnd.uniform(0.0, size * 0.75)
        r = rnd.uniform(0.9, 1.7) * (1.25 - rr / size * 0.5)
        pos = (math.cos(a) * rr, math.sin(a) * rr, r * 0.35)
        if blocky:
            b = L.box("block", (r * 1.4, r * 1.1, r * 0.9), pos, rock_mat, 0.08, rot=(rnd.uniform(-0.2, 0.2), rnd.uniform(-0.2, 0.2), rnd.uniform(0, 3)))
            L.apply_all(b)
            L.jitter_verts(b, 0.06, seed + i)
        else:
            b = L.ico("rock", r, pos, rock_mat if i % 3 else pal["rock_dark"], 1, (1.0, 0.85, 0.6))
            displace(b, r * 0.25, 1.1 / r, seed + i)
        P.append(b)
    for i in range(n_ore):
        a = rnd.uniform(0, 2 * math.pi)
        rr = rnd.uniform(0.2, size * 0.8)
        r = rnd.uniform(0.3, 0.6)
        o = L.ico("ore", r, (math.cos(a) * rr, math.sin(a) * rr, rnd.uniform(0.9, 1.9) * (1.2 - rr / size * 0.6)), ore_mat, 1)
        L.apply_all(o)
        L.jitter_verts(o, r * 0.2, seed + 100 + i)
        P.append(o)
    return P


def gold_mine(pal):
    P = rock_pile(pal, pal["rock"], pal["gold"], 7, 9, 16)
    # small timber frame mine entrance
    P.append(L.box("post_l", (0.25, 0.25, 2.0), (-0.8, -2.8, 1.0), pal["wood"]))
    P.append(L.box("post_r", (0.25, 0.25, 2.0), (0.8, -2.8, 1.0), pal["wood"]))
    P.append(L.box("lintel", (2.0, 0.3, 0.3), (0, -2.8, 2.05), pal["wood"]))
    P.append(L.box("dark", (1.4, 0.2, 1.8), (0, -2.6, 0.9), pal["black"]))
    ob = L.join(P, "body")
    flat_ob = L.smooth(ob, 30)
    return [ob]


def stone_mine(pal):
    P = rock_pile(pal, pal["stone_light"], pal["marble"], 11, 11, 4, blocky=True)
    ob = L.join(P, "body")
    L.smooth(ob, 25)
    return [ob]


def iron_mine(pal):
    P = rock_pile(pal, pal["rock_dark"], pal["iron_ore"], 13, 9, 12)
    ob = L.join(P, "body")
    L.smooth(ob, 30)
    return [ob]


def blade(pos, h, w, lean, yaw, mat):
    x, y, z = pos
    secs = []
    for i in range(4):
        t = i / 3
        bw = w * (1 - t * 0.85)
        cx = x + math.cos(yaw) * lean * t * t
        cy = y + math.sin(yaw) * lean * t * t
        cz = z + h * t
        px, py = -math.sin(yaw) * bw, math.cos(yaw) * bw
        secs.append([(cx + px, cy + py, cz), (cx, cy, cz + 0.004), (cx - px, cy - py, cz), (cx, cy, cz - 0.004)])
    return L.loft("blade", secs, mat, closed_ends=False)


def grass_tuft(pal, n=14, seed=1, h=(0.35, 0.75)):
    rnd = random.Random(seed)
    mats = [pal["leaf"], pal["leaf_light"], pal["crop_green"]]
    P = []
    for i in range(n):
        a = rnd.uniform(0, 2 * math.pi)
        r = rnd.uniform(0.0, 0.25)
        P.append(blade((math.cos(a) * r, math.sin(a) * r, 0), rnd.uniform(*h), rnd.uniform(0.025, 0.045), rnd.uniform(0.1, 0.3), a + rnd.uniform(-0.5, 0.5), mats[i % 3]))
    return P


def deco_grass(pal):
    ob = L.join(grass_tuft(pal, 16, 2), "body")
    L.smooth(ob, 80)
    return [ob]


def deco_flowers(pal):
    rnd = random.Random(9)
    P = grass_tuft(pal, 10, 4, (0.25, 0.5))
    cols = [L.mat("flower_y", (0.95, 0.78, 0.1), 0.5), L.mat("flower_w", (0.9, 0.9, 0.85), 0.5), L.mat("flower_p", (0.55, 0.2, 0.75), 0.5)]
    for i in range(6):
        a = rnd.uniform(0, 2 * math.pi)
        r = rnd.uniform(0.05, 0.3)
        hz = rnd.uniform(0.35, 0.6)
        P.append(L.cyl("stem", 0.008, hz, (math.cos(a) * r, math.sin(a) * r, hz / 2), pal["leaf"], 4))
        P.append(L.sphere("flower", 0.05, (math.cos(a) * r, math.sin(a) * r, hz), cols[i % 3], 6, 4, (1, 1, 0.5)))
    ob = L.join(P, "body")
    L.smooth(ob, 80)
    return [ob]


def deco_rock(pal):
    P = []
    for i, (x, y, r) in enumerate([(0, 0, 0.45), (0.45, 0.2, 0.25)]):
        b = L.ico("rock", r, (x, y, r * 0.3), pal["rock"], 1, (1.0, 0.85, 0.6))
        displace(b, r * 0.25, 1.2 / r, i + 50)
        P.append(b)
    ob = L.join(P, "body")
    L.smooth(ob, 40)
    return [ob]


def deco_bush(pal):
    P = []
    for i, (x, y, r) in enumerate([(0, 0, 0.6), (0.4, 0.2, 0.42), (-0.35, 0.25, 0.4)]):
        P.append(crown_blob(r, (x, y, r * 0.65), pal["leaf"] if i else pal["leaf_light"], i + 60, 0.8))
    ob = L.join(P, "body")
    L.smooth(ob, 80)
    return [ob]


def deco_reeds(pal):
    rnd = random.Random(12)
    P = []
    m = L.mat("reed", (0.32, 0.36, 0.14), 0.8)
    for i in range(18):
        a = rnd.uniform(0, 2 * math.pi)
        r = rnd.uniform(0.0, 0.35)
        P.append(blade((math.cos(a) * r, math.sin(a) * r, 0), rnd.uniform(0.8, 1.4), 0.02, rnd.uniform(0.05, 0.2), a, m))
    ob = L.join(P, "body")
    L.smooth(ob, 80)
    return [ob]


def build(key, out_dir, preview_dir=None):
    L.reset()
    L.clear_mat_cache()
    pal = L.P()
    globals()[key](pal)
    L.export_glb(f"{out_dir}/{key}.glb")
    if preview_dir:
        L.preview(f"{preview_dir}/{key}.png", 384, elev=25, azim=-35)

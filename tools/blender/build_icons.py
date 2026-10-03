"""Resource icons for the HUD: blender -b -P tools/blender/build_icons.py"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import eelib as L

OUT = os.path.join(os.path.dirname(__file__), "..", "..", "game", "assets", "icons")


def food(p):
    parts = []
    for k in range(9):
        a = k * 2 * math.pi / 9
        parts.append(L.cyl("stalk", 0.03, 1.4, (math.cos(a) * 0.08, math.sin(a) * 0.08, 0.7), p["crop"], 6, (math.sin(a) * 0.15, -math.cos(a) * 0.15, 0)))
        parts.append(L.sphere("ear", 0.07, (math.cos(a) * 0.2, math.sin(a) * 0.2, 1.45), p["crop"], 8, 6, (1, 1, 2.6)))
    parts.append(L.cyl("tie", 0.13, 0.12, (0, 0, 0.75), p["roof_red"], 12))
    parts.append(L.sphere("apple", 0.22, (0.4, -0.25, 0.22), L.mat("apple", (0.7, 0.05, 0.03), 0.4), 16, 10))
    return parts


def wood(p):
    parts = []
    for (x, z) in [(-0.25, 0.2), (0.25, 0.2), (0.0, 0.6)]:
        parts.append(L.cyl("log", 0.22, 1.4, (x, 0, z), p["bark"], 14, (math.pi / 2, 0, 0.3)))
        parts.append(L.cyl("cut", 0.2, 0.02, (x - 0.21, -0.68, z), L.mat("cutwood", (0.75, 0.55, 0.32), 0.8), 14, (math.pi / 2, 0, 0.3)))
    return parts


def stone(p):
    return [L.box("b1", (0.7, 0.5, 0.4), (-0.2, 0, 0.2), p["stone_light"], 0.05), L.box("b2", (0.6, 0.5, 0.4), (0.35, 0.1, 0.2), p["stone_light"], 0.05),
            L.box("b3", (0.6, 0.45, 0.4), (0.05, 0.0, 0.6), p["stone_light"], 0.05, rot=(0, 0, 0.3))]


def gold(p):
    m = L.mat("goldbar", (1.0, 0.72, 0.2), 0.25, 1.0)
    parts = []
    for (x, y, z) in [(-0.3, 0, 0.15), (0.3, 0, 0.15), (0, 0, 0.45)]:
        parts.append(L.prism("bar", [(-0.35, 0), (0.35, 0), (0.25, 0.28), (-0.25, 0.28)], 0.75, (x, y, z - 0.14), m, "X", 0.02))
    return parts


def iron(p):
    m = L.mat("ironbar", (0.42, 0.45, 0.5), 0.35, 0.95)
    parts = []
    for (x, z) in [(-0.3, 0.12), (0.3, 0.12), (0.0, 0.4)]:
        parts.append(L.prism("ingot", [(-0.32, 0), (0.32, 0), (0.22, 0.24), (-0.22, 0.24)], 0.75, (x, 0, z - 0.12), m, "X", 0.02))
    return parts


def pop(p):
    t = p["team"]
    return [L.sphere("head", 0.22, (0, 0, 1.3), p["skin"], 16, 10), L.cyl("body", 0.32, 0.8, (0, 0, 0.6), t, 16, r2=0.22),
            L.sphere("head2", 0.18, (0.45, 0.2, 1.05), p["skin"], 14, 8), L.cyl("body2", 0.26, 0.65, (0.45, 0.2, 0.42), t, 14, r2=0.18)]


for name, fn in [("res_food", food), ("res_wood", wood), ("res_stone", stone), ("res_gold", gold), ("res_iron", iron), ("res_pop", pop)]:
    L.reset()
    L.clear_mat_cache()
    pal = L.P()
    fn(pal)
    L.preview(os.path.join(OUT, name + ".png"), 128, elev=30, azim=-30)
    print("icon", name)

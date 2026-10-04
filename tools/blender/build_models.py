"""Build all game models: blender -b -P tools/blender/build_models.py -- [--only a,b] [--preview DIR]"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import importlib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "game", "assets", "models")

MODULES = ["build_infantry", "build_vehicles", "build_aircraft", "build_ships", "build_buildings", "build_nature"]


def make_lod(key, ratio=0.5):
    """Decimate the model just built into <key>_lod1.glb (object names/parts kept)."""
    import bpy
    import eelib as L
    for o in bpy.context.scene.objects:
        if o.type == "MESH":
            m = o.modifiers.new("Decimate", "DECIMATE")
            m.ratio = ratio
    L.export_glb(os.path.join(OUT, f"{key}_lod1.glb"))


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    only = None
    prev = None
    i = 0
    while i < len(argv):
        if argv[i] == "--only":
            only = set(argv[i + 1].split(","))
            i += 2
        elif argv[i] == "--icons":
            prev = os.path.join(ROOT, "game", "assets", "icons")
            os.makedirs(prev, exist_ok=True)
            os.environ["EE_ICON_MODE"] = "1"
            i += 1
        elif argv[i] == "--preview":
            prev = os.path.abspath(argv[i + 1])
            os.makedirs(prev, exist_ok=True)
            i += 2
        else:
            i += 1
    # models replaced by Tripo generations keep their imported version
    tripo_list = os.path.join(ROOT, "tools", "assets", "tripo_models.txt")
    tripo = set()
    if os.path.exists(tripo_list):
        tripo = {l.strip() for l in open(tripo_list) if l.strip() and not l.startswith("#")}
    for mn in MODULES:
        if not os.path.exists(os.path.join(os.path.dirname(__file__), mn + ".py")):
            continue
        mod = importlib.import_module(mn)
        keys = getattr(mod, "KEYS", None) or getattr(mod, "INFANTRY", [])
        for k in keys:
            if only and k not in only:
                continue
            if k in tripo and not os.environ.get("EE_ICON_MODE"):
                print(f"skip {k} (Tripo model)")
                continue
            t0 = time.time()
            mod.build(k, OUT, prev)
            if mn in ("build_vehicles", "build_aircraft", "build_ships"):
                make_lod(k)
            print(f"built {k} in {time.time() - t0:.1f}s", flush=True)


main()

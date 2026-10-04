"""Render HUD icons for the Tripo-imported models (same framing as build_models --icons).

  blender -b -P tools/blender/tripo_icons.py
"""
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(__file__))
os.environ["EE_ICON_MODE"] = "1"
import eelib as L  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
keys = [l.strip() for l in open(os.path.join(ROOT, "tools", "assets", "tripo_models.txt")) if l.strip() and not l.startswith("#")]
for k in keys:
    L.reset()
    bpy.ops.import_scene.gltf(filepath=os.path.join(ROOT, "game", "assets", "models", f"{k}.glb"))
    L.preview(os.path.join(ROOT, "game", "assets", "icons", f"{k}.png"), 384, elev=34, azim=-35)
    print("icon", k)

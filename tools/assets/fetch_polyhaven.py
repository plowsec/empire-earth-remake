#!/usr/bin/env python3
"""Download CC0 textures/HDRIs from Poly Haven (https://polyhaven.com, CC0 license)."""
import json, os, sys, urllib.request

OUT = os.path.join(os.path.dirname(__file__), "..", "..", "game", "assets", "textures", "src")
TEXTURES = {
    "grass": "leafy_grass",
    "meadow": "sparse_grass",
    "grassrock": "aerial_grass_rock",
    "sand": "coast_sand_01",
    "forest": "forest_leaves_02",
    "dirt": "dirt_aerial_02",
    "rock": "rock_face_03",
    "cliff": "aerial_rocks_02",
    "seabed": "coast_sand_05",
    "concrete": "dirty_concrete",
}
HDRI = "kloofendal_48d_partly_cloudy_puresky"

def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "ee-remake-asset-fetch/1.0"})
    with urllib.request.urlopen(req) as r:
        return r.read()

def main():
    os.makedirs(OUT, exist_ok=True)
    for name, pid in TEXTURES.items():
        files = json.loads(get(f"https://api.polyhaven.com/files/{pid}"))
        for kind, key in (("diff", "Diffuse"), ("nor", "nor_gl"), ("rough", "Rough"), ("disp", "Displacement")):
            entry = files.get(key, {}).get("2k", {}).get("jpg") or files.get(key, {}).get("2k", {}).get("png")
            if not entry:
                continue
            dst = os.path.join(OUT, f"{name}_{kind}.jpg")
            if not os.path.exists(dst):
                print("fetch", pid, key)
                open(dst, "wb").write(get(entry["url"]))
    files = json.loads(get(f"https://api.polyhaven.com/files/{HDRI}"))
    dst = os.path.join(OUT, "sky.hdr")
    if not os.path.exists(dst):
        print("fetch hdri")
        open(dst, "wb").write(get(files["hdri"]["2k"]["hdr"]["url"]))

if __name__ == "__main__":
    main()

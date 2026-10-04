#!/usr/bin/env python3
"""Generate textured models with the Tripo 3D API (text -> GLB).

The API key is read from $TRIPO_API_KEY or ~/.config/empire-earth/tripo_key (never
stored in the repo). Each generation costs Tripo credits: run `balance` first.

  python3 tools/assets/tripo.py balance
  python3 tools/assets/tripo.py gen tank "a modern main battle tank, olive drab, ..."
  python3 tools/assets/tripo.py gen house --faces 8000 "..."

Raw downloads land in tools/assets/tripo_raw/<key>.glb with the task metadata next to
them; tools/blender/import_tripo.py then normalizes scale/orientation, decimates, adds a
LOD and exports into game/assets/models/.
"""
import json
import os
import sys
import time
import urllib.request

API = "https://api.tripo3d.ai/v2/openapi"
OUT = os.path.join(os.path.dirname(__file__), "tripo_raw")
STYLE = ("realistic military RTS game asset, isometric-readable silhouette, clean PBR "
         "materials, slightly weathered, no base plate, no text, single object")


def key() -> str:
    k = os.environ.get("TRIPO_API_KEY")
    if not k:
        path = os.path.expanduser("~/.config/empire-earth/tripo_key")
        with open(path) as f:
            k = f.read().strip()
    if not k.startswith("tsk_"):
        sys.exit("Tripo API keys start with tsk_")
    return k


def call(method: str, path: str, body=None) -> dict:
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(API + path, data=data, method=method,
                                 headers={"Authorization": f"Bearer {key()}", "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        out = json.loads(r.read())
    if out.get("code") != 0:
        sys.exit(f"Tripo error: {out}")
    return out["data"]


def balance() -> None:
    b = call("GET", "/user/balance")
    print(f"balance {b.get('balance')} credits (frozen {b.get('frozen')})")


def gen(name: str, prompt: str, faces: int) -> str:
    os.makedirs(OUT, exist_ok=True)
    task = call("POST", "/task", {
        "type": "text_to_model",
        "prompt": f"{prompt}. {STYLE}",
        "negative_prompt": "low quality, cartoon, toy, text, watermark, ground plane, multiple objects",
        "face_limit": faces,
        "texture": True,
        "pbr": True,
        "texture_quality": "standard",
        "auto_size": True,
    })
    tid = task["task_id"]
    print(f"{name}: task {tid}")
    while True:
        t = call("GET", f"/task/{tid}")
        st = t.get("status", "")
        print(f"  {st} {t.get('progress', 0)}%", flush=True)
        if st.upper() == "SUCCESS":
            break
        if st.upper() in ("FAILED", "CANCELLED", "BANNED", "EXPIRED"):
            sys.exit(f"{name}: task ended {st}")
        time.sleep(5)
    out = t.get("output", {})
    url = out.get("pbr_model") or out.get("model") or out.get("base_model")
    if not url:
        sys.exit(f"{name}: no model in output {out}")
    path = os.path.join(OUT, f"{name}.glb")
    with urllib.request.urlopen(url, timeout=120) as r, open(path, "wb") as f:
        f.write(r.read())
    with open(os.path.join(OUT, f"{name}.json"), "w") as f:
        json.dump({"task": tid, "prompt": prompt, "faces": faces, "output": out}, f, indent=1)
    print(f"{name}: saved {path} ({os.path.getsize(path) // 1024} KiB)")
    return path


def main() -> None:
    a = sys.argv[1:]
    if not a or a[0] == "balance":
        balance()
        return
    if a[0] == "gen" and len(a) >= 3:
        faces = 6000
        if "--faces" in a:
            i = a.index("--faces")
            faces = int(a[i + 1])
            del a[i:i + 2]
        gen(a[1], " ".join(a[2:]), faces)
        return
    sys.exit(__doc__)


if __name__ == "__main__":
    main()

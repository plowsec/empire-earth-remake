#!/usr/bin/env python3
"""Pack terrain layers into vertical-strip images Godot imports as Texture2DArray.
albedo_array.png: RGB albedo + A height;  normal_array.png: RG normal(GL) + B roughness + A 255."""
import os
from PIL import Image

ROOT = os.path.join(os.path.dirname(__file__), "..", "..", "game", "assets", "textures")
SRC = os.path.join(ROOT, "src")
# order = layer index used by the terrain shader / Rust control maps
LAYERS = ["sand", "grass", "meadow", "forest", "mud", "grassrock", "rock", "seabed"]
SURFACES = ["s_concrete", "s_brick", "s_roof", "s_corrugated", "s_plaster", "s_wood", "s_paint", "s_fabric", "s_asphalt", "s_rock", "s_bark"]
SIZE = 1024

def load(name, kind, mode):
    p = os.path.join(SRC, f"{name}_{kind}.jpg")
    im = Image.open(p).convert(mode)
    return im.resize((SIZE, SIZE), Image.LANCZOS)

def pack(layers, prefix):
    alb = Image.new("RGBA", (SIZE, SIZE * len(layers)))
    nor = Image.new("RGBA", (SIZE, SIZE * len(layers)))
    for i, name in enumerate(layers):
        d = load(name, "diff", "RGB")
        try:
            h = load(name, "disp", "L")
        except FileNotFoundError:
            h = Image.new("L", (SIZE, SIZE), 128)
        r, g, b = d.split()
        alb.paste(Image.merge("RGBA", (r, g, b, h)), (0, i * SIZE))
        n = load(name, "nor", "RGB")
        nr, ng, _ = n.split()
        ro = load(name, "rough", "L")
        nor.paste(Image.merge("RGBA", (nr, ng, ro, Image.new("L", (SIZE, SIZE), 255))), (0, i * SIZE))
        print("packed", name)
    for kind, im in (("albedo", alb), ("normal", nor)):
        f = f"{prefix}_{kind}_array.png"
        im.save(os.path.join(ROOT, f))
        with open(os.path.join(ROOT, f + ".import"), "w") as fh:
            fh.write(IMPORT.format(f=f, n=len(layers)))


IMPORT = """[remap]

importer="2d_array_texture"
type="CompressedTexture2DArray"

[deps]

source_file="res://assets/textures/{f}"

[params]

compress/mode=2
compress/high_quality=true
compress/lossy_quality=0.9
compress/hdr_compression=1
compress/channel_pack=0
mipmaps/generate=true
mipmaps/limit=-1
slices/horizontal=1
slices/vertical={n}
"""


def main():
    pack(LAYERS, "terrain")
    pack(SURFACES, "surface")


def _old_main():
    alb = Image.new("RGBA", (SIZE, SIZE * len(LAYERS)))
    nor = Image.new("RGBA", (SIZE, SIZE * len(LAYERS)))
    for i, name in enumerate(LAYERS):
        d = load(name, "diff", "RGB")
        h = load(name, "disp", "L")
        r, g, b = d.split()
        alb.paste(Image.merge("RGBA", (r, g, b, h)), (0, i * SIZE))
        n = load(name, "nor", "RGB")
        nr, ng, _ = n.split()
        ro = load(name, "rough", "L")
        nor.paste(Image.merge("RGBA", (nr, ng, ro, Image.new("L", (SIZE, SIZE), 255))), (0, i * SIZE))
        print("packed", name)
    alb.save(os.path.join(ROOT, "terrain_albedo_array.png"))
    nor.save(os.path.join(ROOT, "terrain_normal_array.png"))
    for f, srgb in (("terrain_albedo_array.png", 1), ("terrain_normal_array.png", 0)):
        with open(os.path.join(ROOT, f + ".import"), "w") as fh:
            fh.write(f"""[remap]

importer="2d_array_texture"
type="CompressedTexture2DArray"

[deps]

source_file="res://assets/textures/{f}"

[params]

compress/mode=2
compress/high_quality=true
compress/lossy_quality=0.9
compress/hdr_compression=1
compress/channel_pack=0
mipmaps/generate=true
mipmaps/limit=-1
slices/horizontal=1
slices/vertical={len(LAYERS)}
""")

if __name__ == "__main__":
    main()

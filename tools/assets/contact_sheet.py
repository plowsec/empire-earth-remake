#!/usr/bin/env python3
"""Tile preview PNGs into one image: contact_sheet.py out.png in1.png in2.png ..."""
import sys
from PIL import Image, ImageDraw
out, files = sys.argv[1], sys.argv[2:]
ims = [Image.open(f).convert("RGBA") for f in files]
s = max(i.width for i in ims)
cols = min(4, len(ims))
rows = (len(ims) + cols - 1) // cols
sheet = Image.new("RGBA", (cols * s, rows * s), (60, 66, 74, 255))
d = ImageDraw.Draw(sheet)
for k, (im, f) in enumerate(zip(ims, files)):
    x, y = (k % cols) * s, (k // cols) * s
    sheet.alpha_composite(im, (x, y))
    d.text((x + 6, y + 6), f.split("/")[-1][:-4], fill=(255, 255, 255, 255))
sheet.convert("RGB").save(out)

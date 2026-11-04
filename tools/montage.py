#!/usr/bin/env python3
"""montage.py out.png cols scale img... : a grid of images downscaled by `scale` (an integer)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from compare_shots import read_png, write_png
out, cols, k = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
imgs = [read_png(p)[:3] for p in sys.argv[4:]]
cw, ch = max(i[0] for i in imgs) // k, max(i[1] for i in imgs) // k
rows_n = (len(imgs) + cols - 1) // cols
canvas = [bytearray(b"\x00\x00\x00\xff" * (cw * cols)) for _ in range(ch * rows_n)]
for n, (w, h, rows) in enumerate(imgs):
    ox, oy = (n % cols) * cw, (n // cols) * ch
    for y in range(h // k):
        src = rows[y * k]
        line = canvas[oy + y]
        for x in range(w // k):
            line[4 * (ox + x):4 * (ox + x) + 3] = src[4 * x * k:4 * x * k + 3]
write_png(out, cw * cols, ch * rows_n, canvas)

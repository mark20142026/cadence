#!/usr/bin/env python3
"""Draw a level's collision rects (from `cadence rects <level>`) as a PNG, optionally with a
trace's path on top. No dependencies (writes the PNG with zlib).

Usage: levelmap.py <rects.csv> <out.png> [scale=0.5] [x0 x1 y0 y1] [--trace t.csv ...]
Colors: solids grey, hazards red, collectibles yellow, triggers/area (45) blue outline,
other specials green; traces magenta (one per file).
"""
import csv
import struct
import sys
import zlib

COLORS = {"0": (150, 150, 150), "2": (220, 40, 40), "30": (240, 220, 0), "45": (60, 120, 255),
          "25": (120, 120, 200), "39": (0, 200, 200)}


def png(path, w, h, px):
    raw = b"".join(b"\x00" + bytes(px[y * w * 3:(y + 1) * w * 3]) for y in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


def main():
    args = [a for a in sys.argv[1:]]
    traces = []
    if "--trace" in args:
        i = args.index("--trace")
        traces = args[i + 1:]
        args = args[:i]
    rects = list(csv.DictReader(open(args[0])))
    out = args[1]
    scale = float(args[2]) if len(args) > 2 else 0.5
    if len(args) > 6:
        x0, x1, y0, y1 = map(float, args[3:7])
    else:
        x0 = min(float(r["rect_x"]) for r in rects) - 30
        x1 = max(float(r["rect_x"]) + float(r["rect_w"]) for r in rects) + 30
        y0 = min(float(r["rect_y"]) for r in rects) - 30
        y1 = max(float(r["rect_y"]) + float(r["rect_h"]) for r in rects) + 30
    w, h = int((x1 - x0) * scale) + 1, int((y1 - y0) * scale) + 1
    px = bytearray([16, 16, 24] * (w * h))

    def put(x, y, c):
        if 0 <= x < w and 0 <= y < h:
            i = (y * w + x) * 3
            px[i:i + 3] = bytes(c)

    def to_px(x, y):
        return int((x - x0) * scale), h - 1 - int((y - y0) * scale)

    # grid every 300 units
    gx = (int(x0) // 300) * 300
    while gx < x1:
        X, _ = to_px(gx, y0)
        for Y in range(h):
            if Y % 4 == 0:
                put(X, Y, (50, 50, 60))
        gx += 300
    gy = (int(y0) // 300) * 300
    while gy < y1:
        _, Y = to_px(x0, gy)
        for X in range(w):
            if X % 4 == 0:
                put(X, Y, (50, 50, 60))
        gy += 300
    for r in rects:
        c = COLORS.get(r["type"], (0, 200, 0))
        rx, ry, rw, rh = (float(r[k]) for k in ("rect_x", "rect_y", "rect_w", "rect_h"))
        if rw <= 0 or rh <= 0:
            rx, ry, rw, rh = float(r["x"]) - 5, float(r["y"]) - 5, 10, 10
        ax, ay = to_px(rx, ry + rh)
        bx, by = to_px(rx + rw, ry)
        outline = r["type"] == "45"
        for Y in range(ay, by + 1):
            for X in range(ax, bx + 1):
                if not outline or Y in (ay, by) or X in (ax, bx):
                    put(X, Y, c)
    tc = [(255, 0, 255), (0, 255, 120), (255, 160, 0), (120, 200, 255)]
    for n, t in enumerate(traces):
        for line in open(t):
            if not line[0].isdigit():
                continue
            f = line.split(",")
            X, Y = to_px(float(f[2]), float(f[3]))
            put(X, Y, tc[n % len(tc)])
    png(out, w, h, px)
    print(f"{out}: {w}x{h}, x {x0:.0f}..{x1:.0f}, y {y0:.0f}..{y1:.0f}")


main()

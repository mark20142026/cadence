#!/usr/bin/env python3
"""Compare a real-game frame with Cadence's frame of the same step.

Usage: compare_shots.py <real.png> <ours.png> <out.png> [skip_top_px]
Writes the two stacked (real on top) plus a difference image (bright = different) and prints
the mean absolute difference per channel (0..255). Pure Python, no dependencies.
`skip_top_px` ignores rows covered by the window title bar in the real capture.
"""
import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    pos, idat = 8, b""
    while pos < len(data):
        length = struct.unpack(">I", data[pos:pos + 4])[0]
        kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + length]
        if kind == b"IHDR":
            w, h, _depth, ctype = struct.unpack(">IIBB", body[:10])
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    bpp = 4 if ctype == 6 else 3
    raw = zlib.decompress(idat)
    stride = w * bpp + 1
    prev = bytearray(w * bpp)
    rows = []
    for y in range(h):
        f = raw[y * stride]
        line = bytearray(raw[y * stride + 1:(y + 1) * stride])
        for i in range(len(line)):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + ((a + b) >> 1)) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = line
        if bpp == 3:
            line = bytearray(b"".join(bytes(line[i:i + 3]) + b"\xff" for i in range(0, len(line), 3)))
        rows.append(bytes(line))
    return w, h, rows


def write_png(path, w, h, rows):
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    raw = b"".join(b"\x00" + r for r in rows)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
    open(path, "wb").write(png)


def main():
    real_path, ours_path, out_path = sys.argv[1:4]
    skip = int(sys.argv[4]) if len(sys.argv) > 4 else 0
    w, h, real = read_png(real_path)
    w2, h2, ours = read_png(ours_path)
    if (w, h) != (w2, h2):
        sys.exit(f"size mismatch: {w}x{h} vs {w2}x{h2}")
    diff_rows, total, count = [], [0, 0, 0], 0
    for y in range(h):
        a, b = real[y], ours[y]
        row = bytearray(w * 4)
        for x in range(w):
            i = x * 4
            d = [abs(a[i + k] - b[i + k]) for k in range(3)]
            m = min(255, max(d) * 3)
            row[i:i + 4] = bytes((m, m, m, 255))
            if y >= skip:
                for k in range(3):
                    total[k] += d[k]
        if y >= skip:
            count += w
        diff_rows.append(bytes(row))
    write_png(out_path, w, h * 3, real + ours + diff_rows)
    print("mean abs diff (r,g,b): " + ", ".join(f"{t / count:.2f}" for t in total))


if __name__ == "__main__":
    main()

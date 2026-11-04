#!/usr/bin/env python3
"""First visual-rotation mismatch per attempt: real trace vs `cadence replay`.

Usage: rot_diff.py <spec> [tolerance_deg] [context_steps]
Runs `target/release/cadence replay` into a temp dir and compares column `rot` (mod 360).
"""
import os
import subprocess
import sys
import tempfile


def load(path):
    rows = {}
    for line in open(path):
        if line[0] in "#s":
            continue
        f = line.strip().split(",")
        rows[int(f[0])] = f
    return rows


def main():
    spec = sys.argv[1]
    tol = float(sys.argv[2]) if len(sys.argv) > 2 else 0.001
    ctx = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    out_pattern = next(l.split()[1] for l in open(spec) if l.startswith("out "))
    attempts = sum(1 for l in open(spec) if l.strip() == "attempt")
    tmp = tempfile.mkdtemp()
    ours_pattern = os.path.join(tmp, "o%d.csv")
    subprocess.run(["target/release/cadence", "replay", spec, ours_pattern], check=True, capture_output=True)
    for i in range(attempts):
        real_path, ours_path = out_pattern.replace("%d", str(i)), ours_pattern.replace("%d", str(i))
        if not (os.path.exists(real_path) and os.path.exists(ours_path)):
            continue
        real, ours = load(real_path), load(ours_path)
        for s in sorted(real):
            if s not in ours:
                break
            d = (float(real[s][5]) - float(ours[s][5]) + 180) % 360 - 180
            if abs(d) > tol:
                print(f"attempt {i}: step {s} rot diff {d:.4f}")
                for c in range(s - ctx, s + 2):
                    if c in real and c in ours:
                        r, o = real[c], ours[c]
                        print(f"  {c}: y {r[3]} yv {r[4]} fl {r[6]} | rot real {r[5]} ours {o[5]}")
                break


if __name__ == "__main__":
    main()

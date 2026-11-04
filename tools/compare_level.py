#!/usr/bin/env python3
"""Screenshot a level in the real game and in Cadence at the same steps, side by side.

Usage: compare_level.py <level> <route.spec> <step,step,...> [outdir]

Runs the real game (harness/run-trace.sh, the isolated copy) on the first attempt of the route
with `screenshot` at each step, then renders Cadence (headless, silent) at the exact step each
real screenshot landed on (the harness can only capture on rendered frames). Writes
<outdir>/<step>.png (real on top, Cadence below, difference) plus the real `.scene` dumps, and
prints the mean difference per shot. Default outdir: shots/<level>.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    level, route, steps = sys.argv[1], os.path.abspath(sys.argv[2]), [int(s) for s in sys.argv[3].split(",")]
    out = os.path.abspath(sys.argv[4] if len(sys.argv) > 4 else os.path.join(ROOT, "shots", level))
    os.makedirs(out, exist_ok=True)
    lines, attempts = [], 0
    for line in open(route).read().splitlines():
        if line.startswith("attempt"):
            attempts += 1
            if attempts > 1:
                break
        if line.startswith("steps"):
            line = f"steps {max(steps) + 200}"
        if line.startswith("out"):
            line = f"out {out}/real%d.csv"
        lines.append(line)
    for i, step in enumerate(steps):
        lines.insert(3 + i, f"screenshot {step} {out}/real-{step}.png")
    spec = os.path.join(out, "shot.spec")
    with open(spec, "w") as f:
        f.write("\n".join(lines) + "\n")
    run = subprocess.run([os.path.join(ROOT, "harness/run-trace.sh"), spec, "400"], capture_output=True, text=True)
    sys.stderr.write(run.stderr[-500:])
    # COMPARE_REAL_ONLY=1: only take the real frames (tools/recompare.py compares them later,
    # without the real game)
    if os.environ.get("COMPARE_REAL_ONLY"):
        return
    for step in steps:
        real = f"{out}/real-{step}.png"
        if not os.path.exists(real):
            print(f"{level} {step}: no real screenshot")
            continue
        info = open(real + ".txt").read()
        actual = int(re.search(r"step (\d+)", info).group(1))
        w, h = re.search(r"size (\d+) (\d+)", info).groups()
        ours = f"{out}/ours-{actual}.png"
        env = dict(os.environ, CADENCE_SIZE=f"{w}x{h}", CADENCE_SCREENSHOT=ours, CADENCE_STEPS=str(actual), CADENCE_SPEC=route)
        subprocess.run([os.path.join(ROOT, "target/release/cadence-game"), level], env=env, capture_output=True)
        # the real frame lost its notch rows (".txt" crop N); cut ours the same
        crop = re.search(r"crop (\d+)", info)
        if crop and int(crop.group(1)):
            sys.path.insert(0, os.path.join(ROOT, "tools"))
            from compare_shots import read_png, write_png
            cw, ch, crows = read_png(ours)[:3]
            top = int(crop.group(1))
            write_png(ours, cw, ch - top, crows[top:])
        cmp = subprocess.run(
            [sys.executable, "-I", os.path.join(ROOT, "tools/compare_shots.py"), real, ours, f"{out}/{actual}.png", "32"],
            capture_output=True, text=True)
        print(f"{level} {actual}: {cmp.stdout.strip().splitlines()[-1] if cmp.stdout.strip() else cmp.stderr.strip()[-200:]}")


if __name__ == "__main__":
    main()

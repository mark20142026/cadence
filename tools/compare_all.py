#!/usr/bin/env python3
"""Real-vs-Cadence screenshots for every main level (tools/compare_level.py per level).

Usage: compare_all.py [level,level,...] [points]

For each level it uses a recorded route that completed the level (re/traces/*/route.spec whose
real trace ends "complete"), or plans one with `cadence route`, then compares `points` (default 6)
evenly spaced steps. Results: shots/<level>/<step>.png and shots/summary.csv (level, step, mean
difference per channel), worst first in the printout.
"""

import glob
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEVELS = list(range(1, 23)) + [3001]


def completed_route(level):
    best = None
    for spec in glob.glob(os.path.join(ROOT, "re/traces/*/route.spec")):
        with open(spec) as f:
            if f.readline().strip() != f"level {level}":
                continue
        for csv in glob.glob(os.path.join(os.path.dirname(spec), "r0.csv")):
            tail = open(csv).read().rstrip().splitlines()[-1]
            m = re.search(r"# end complete step=(\d+)", tail)
            if m and (best is None or int(m.group(1)) > best[1]):
                best = (spec, int(m.group(1)))
    return best


def planned_route(level):
    out = os.path.join(ROOT, "re/traces", f"plan-{level}")
    os.makedirs(out, exist_ok=True)
    spec = os.path.join(out, "route.spec")
    with open(spec, "w") as f:
        subprocess.run([os.path.join(ROOT, "target/release/cadence"), "route", str(level), "1", f"{out}/r%d.csv"],
                       stdout=f, stderr=subprocess.DEVNULL, timeout=1800)
    log = open(spec).read()
    steps = re.search(r"^steps (\d+)", log, re.M)
    # The planner's own replay tells how far the route got.
    run = subprocess.run([os.path.join(ROOT, "target/release/cadence"), "replay", spec, f"{out}/ours%d.csv"],
                         capture_output=True, text=True)
    last = 0
    for csv in glob.glob(f"{out}/ours0.csv"):
        rows = [l for l in open(csv) if l[:1].isdigit()]
        last = int(rows[-1].split(",")[0]) if rows else 0
    return (spec, last or (int(steps.group(1)) if steps else 0))


def main():
    levels = [int(v) for v in sys.argv[1].split(",")] if len(sys.argv) > 1 and sys.argv[1] else LEVELS
    points = int(sys.argv[2]) if len(sys.argv) > 2 else 6
    rows = []
    for level in levels:
        route = completed_route(level) or planned_route(level)
        spec, end = route
        if end < 400:
            print(f"{level}: no usable route")
            continue
        steps = [int(300 + i * (end - 600) / max(points - 1, 1)) for i in range(points)]
        run = subprocess.run([sys.executable, "-I", os.path.join(ROOT, "tools/compare_level.py"), str(level), spec,
                              ",".join(map(str, steps))], capture_output=True, text=True, env=dict(os.environ, CADENCE_BALL="1"))
        for line in run.stdout.splitlines():
            m = re.match(r"(\d+) (\d+): mean abs diff \(r,g,b\): ([\d.]+), ([\d.]+), ([\d.]+)", line)
            if m:
                rows.append((int(m.group(1)), int(m.group(2)), *map(float, m.group(3, 4, 5))))
        print(f"{level}: {os.path.relpath(spec, ROOT)} to step {end}: done", flush=True)
    with open(os.path.join(ROOT, "shots/summary.csv"), "w") as f:
        f.write("level,step,r,g,b\n")
        for r in rows:
            f.write(",".join(map(str, r)) + "\n")
    for r in sorted(rows, key=lambda r: -(r[2] + r[3] + r[4]))[:25]:
        print(f"level {r[0]} step {r[1]}: {r[2]:.2f} {r[3]:.2f} {r[4]:.2f}")


if __name__ == "__main__":
    main()

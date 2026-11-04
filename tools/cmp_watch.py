#!/usr/bin/env python3
"""Compare a harness `watchgroup` file with Cadence's (`cadence watch <spec>` writes <file>.ours).

Per step and object: the f64 position (`real=`), rotation, scale and opacity. Prints, per field,
the samples compared, the largest difference and the first step past the tolerance.

Usage: cmp_watch.py <real.csv> [<ours.csv>] [--tol 1e-3]
"""
import sys


def load(path):
    out = {}
    for line in open(path):
        parts = line.strip().split(",")
        if not parts[0].isdigit():
            continue
        objs = []
        for o in parts[1:]:
            f = o.split(":")
            d = {"rot": float(f[3])}
            for i, x in enumerate(f):
                if x.startswith("real="):
                    d["real"] = (float(x[5:]), float(f[i + 1]))
                elif x.startswith("op="):
                    d["op"] = (float(x[3:]),)
                elif x.startswith("sc="):
                    d["sc"] = (float(x[3:]), float(f[i + 1]))
            objs.append(d)
        out[int(parts[0])] = objs
    return out


def main():
    args = [a for a in sys.argv[1:]]
    tol = 1e-3
    if "--tol" in args:
        i = args.index("--tol")
        tol = float(args[i + 1])
        del args[i:i + 2]
    real_path = args[0]
    ours_path = args[1] if len(args) > 1 else real_path + ".ours"
    r, o = load(real_path), load(ours_path)
    name = real_path.split("/")[-1]
    for field in ("real", "rot", "sc", "op"):
        n, worst, first = 0, 0.0, None
        for step in sorted(r):
            if step not in o:
                continue
            for k, (a, b) in enumerate(zip(r[step], o[step])):
                if field not in a or field not in b:
                    continue
                va = a[field] if isinstance(a[field], tuple) else (a[field],)
                vb = b[field] if isinstance(b[field], tuple) else (b[field],)
                d = max(abs(x - y) for x, y in zip(va, vb))
                n += 1
                worst = max(worst, d)
                if d > tol and first is None:
                    first = (step, k, va, vb)
        if n:
            print(f"{name} {field}: {n} samples, max diff {worst:.6g}, first past {tol:g}: {first}")


if __name__ == "__main__":
    main()

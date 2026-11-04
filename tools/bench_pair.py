#!/usr/bin/env python3
"""Vanilla vs Cadence frame times, CPU and memory, one program at a time, level by level.

    tools/bench_pair.py [ids,...] [--modes vanilla,nocache,cache] [--out DIR]

Both run in a 640 x 360 point window kept on top in the main screen's bottom-right corner that
never takes focus (the harness mod's `corner`, Cadence's CADENCE_CORNER), with their audio
playing muted, noclip, 20 s measured. CPU and footprint come from /usr/bin/time -l for both.
A watchdog checks the app in front twice a second: if either program ever comes to the front it
gives the front back and the run is marked. Never writes the real save; checks it is unchanged.
"""
import os, re, subprocess, sys, threading, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = f"{ROOT}/re/gd-copy/Geometry Dash.app"
REAL_SAVE = os.path.expanduser("~/Library/Application Support/GeometryDash")
NAMES = {
    "69685815": "Sonic Wave Infinity", "86407629": "Tidal Wave", "52374843": "Zodiac",
    "42584142": "Bloodlust", "75206202": "Firework", "89496627": "Avernus", "58673581": "Kenos",
    "27690100": "Slaughterhouse", "49896559": "Abyss of Darkness", "59075347": "Tartarus",
    "61079355": "Acu", "35756838": "pc killer", "30267854": "Lag Machine",
    "73965779": "stress test", "10565740": "Bloodbath",
}
ORDER = list(NAMES)


def front():
    asn = subprocess.run(["lsappinfo", "front"], capture_output=True, text=True).stdout.strip()
    info = subprocess.run(["lsappinfo", "info", "-only", "name", "-only", "bundleid", asn], capture_output=True, text=True).stdout
    name = re.match(r'\s*"([^"]*)"', info)
    bundle = re.search(r'bundleID="([^"]*)"', info)
    return (name.group(1) if name else "?", bundle.group(1) if bundle else None)


class Watchdog(threading.Thread):
    """Gives the front back to the user's app if one of ours takes it."""

    def __init__(self):
        super().__init__(daemon=True)
        self.stop, self.stole, self.user = False, False, front()

    def run(self):
        while not self.stop:
            name, bundle = front()
            if name in ("Geometry Dash", "cadence-game", "Cadence"):
                self.stole = True
                if self.user[1]:
                    subprocess.run(["osascript", "-e", f'tell application id "{self.user[1]}" to activate'], capture_output=True)
            elif bundle:
                self.user = (name, bundle)
            time.sleep(0.5)


def save_state():
    return sorted((f, os.stat(f"{REAL_SAVE}/{f}").st_mtime, os.stat(f"{REAL_SAVE}/{f}").st_size) for f in os.listdir(REAL_SAVE))


def timed(cmd, env, cwd, timeout):
    """Run under /usr/bin/time -l; (output, cpu %, footprint MB)."""
    p = subprocess.Popen(["/usr/bin/time", "-l"] + cmd, env=env, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    try:
        out, _ = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        # the program is /usr/bin/time's child: stop it by its PID first
        for child in subprocess.run(["pgrep", "-P", str(p.pid)], capture_output=True, text=True).stdout.split():
            subprocess.run(["kill", "-9", child])
        p.kill()
        out, _ = p.communicate()
        out += "\n(timed out)"
    t = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", out)
    fp = re.search(r"(\d+)\s+peak memory footprint", out)
    cpu = (float(t.group(2)) + float(t.group(3))) / float(t.group(1)) * 100 if t else float("nan")
    return out, cpu, int(fp.group(1)) / 1048576 if fp else float("nan")


def intervals(path):
    try:
        return [float(l) for l in open(path) if l.strip() and not l.startswith("#")]
    except OSError:
        return []


def vanilla(lv, out):
    res = f"{out}/vanilla-{lv}.txt"
    spec = f"{out}/vanilla-{lv}.spec"
    with open(spec, "w") as f:
        f.write(f"levelstring {ROOT}/bench-levels/{lv}.txt\nbench 20 {res}\nnoclip\ncorner\nmute\nout {res}\n")
    if os.path.exists(res):
        os.remove(res)
    # Through LaunchServices with -g: a program started from this one (or a direct exec) is
    # activated by AppKit as it finishes launching, before the mod can stop it. /usr/bin/time
    # can't wrap that, so the mod writes the same counters at the end (`# rusage`).
    home = f"{ROOT}/re/home"
    env = {"HOME": home, "CFFIXED_USER_HOME": home, "CADENCE_TRACE_SPEC": spec,
           "DYLD_INSERT_LIBRARIES": f"{APP}/Contents/Frameworks/GeodeBootstrapper.dylib"}
    cmd = ["open", "-g", "-n", "-W", "-a", APP, "--stdout", f"{out}/vanilla-{lv}.log", "--stderr", f"{out}/vanilla-{lv}.log"]
    for k, v in env.items():
        cmd += ["--env", f"{k}={v}"]
    started = set(subprocess.run(["pgrep", "-x", "Geometry Dash"], capture_output=True, text=True).stdout.split())
    p = subprocess.Popen(cmd)
    try:
        p.wait(timeout=120)
    except subprocess.TimeoutExpired:
        # stop only the copy this run started, by its PID
        for pid in set(subprocess.run(["pgrep", "-x", "Geometry Dash"], capture_output=True, text=True).stdout.split()) - started:
            subprocess.run(["kill", "-9", pid])
        p.wait()
    cpu = mb = float("nan")
    try:
        ru = next(l for l in open(res) if l.startswith("# rusage"))
        v = dict(zip(ru.split()[2::2], map(float, ru.split()[3::2])))
        cpu, mb = (v["user"] + v["sys"]) / v["wall"] * 100, v["footprint"] / 1048576
    except (OSError, StopIteration):
        pass
    return intervals(res), cpu, mb


def cadence(lv, out, cache):
    res = f"{out}/{'cache' if cache else 'nocache'}-{lv}.txt"
    env = dict(os.environ, CADENCE_CORNER="1", CADENCE_BENCH_AUDIO="1", CADENCE_WINDOW="640x360", CADENCE_NOCLIP="1",
               CADENCE_BENCH="22", CADENCE_BENCH_OUT=res, CADENCE_LEVEL_FILE=f"bench-levels/{lv}.txt", CADENCE_BALL="1")
    if not cache:
        env["CADENCE_NOCACHE"] = "1"
    log, cpu, mb = timed([f"{ROOT}/target/release/cadence-game", lv], env, ROOT, 90)
    open(f"{res[:-4]}.log", "w").write(log)
    return intervals(res), cpu, mb


def row(name, mode, iv, cpu, mb, stole):
    if not iv:
        return f"{name:<22} {mode:<8} (no frames)"
    s = sorted(iv)
    q = lambda p: s[min(len(s) - 1, int(p * len(s)))]
    fps = 1000 * len(iv) / sum(iv)
    mark = "  (came to the front)" if stole else ""
    return f"{name:<22} {mode:<8} {fps:6.1f} {q(.5):6.1f} {q(.99):6.1f} {s[-1]:6.1f} {sum(1 for x in iv if x > 20):5d} {cpu:5.0f}% {mb:6.0f}{mark}"


def main():
    args = sys.argv[1:]
    ids = ORDER
    modes = ["vanilla", "nocache", "cache"]
    out = f"{ROOT}/target/bench-pair"
    while args:
        a = args.pop(0)
        if a == "--modes":
            modes = args.pop(0).split(",")
        elif a == "--out":
            out = os.path.abspath(args.pop(0))
        else:
            ids = a.split(",")
    os.makedirs(out, exist_ok=True)
    if "vanilla" in modes:
        subprocess.run(["cp", f"{ROOT}/re/build/trace-mod/cadence.trace.geode", f"{APP}/Contents/geode/mods/"], check=True)
    before = save_state()
    print(f"{'level':<22} {'mode':<8} {'FPS':>6} {'p50':>6} {'p99':>6} {'max':>6} {'>20ms':>5} {'CPU/s':>6} {'MB':>6}", flush=True)
    for lv in ids:
        for mode in modes:
            dog = Watchdog()
            dog.start()
            iv, cpu, mb = vanilla(lv, out) if mode == "vanilla" else cadence(lv, out, mode == "cache")
            dog.stop = True
            dog.join()
            print(row(NAMES.get(lv, lv), mode, iv, cpu, mb, dog.stole), flush=True)
            if save_state() != before:
                sys.exit("the real save folder changed: stopping")


if __name__ == "__main__":
    main()

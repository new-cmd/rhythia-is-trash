#!/usr/bin/env python3
"""Repeatable A/B benchmark for the release build.

  python3 tools/bench.py --label before --map "maps/x.sspm" [--repeats 3] [--only play_vsync,menu_vsync]

Each run happens in a throwaway working directory with its own settings.json, so the
player's settings, scores and maps are never touched. The game plays the map itself
(RHYTHM_BENCH, autopilot cursor) and prints one BENCH json line; nvidia-smi is sampled
alongside. Output: perf/<label>.json with the median of every number.
"""
import argparse, glob, hashlib, json, os, shutil, statistics, subprocess, sys, tempfile, threading, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target", "release", "rhythm")  # overridden by --bin; pin a copy so builds cannot swap it mid-run
QUIET = {"music_volume": 0.0, "sfx_volume": 0.0, "no_fail": True}

SCENARIOS = {
    "play_vsync": ({}, {"RHYTHM_BENCH": "play:14"}),
    "play_uncapped": ({}, {"RHYTHM_BENCH": "play:12", "RHYTHM_VSYNC": "0"}),
    "play_uncapped_noglow": ({"glow": False}, {"RHYTHM_BENCH": "play:12", "RHYTHM_VSYNC": "0"}),
    "play_uncapped_grid": ({"space": "grid"}, {"RHYTHM_BENCH": "play:12", "RHYTHM_VSYNC": "0"}),
    "play_uncapped_tunnel": ({"space": "tunnel"}, {"RHYTHM_BENCH": "play:12", "RHYTHM_VSYNC": "0"}),
    "play_uncapped_void": ({"space": "void"}, {"RHYTHM_BENCH": "play:12", "RHYTHM_VSYNC": "0"}),
    "play_uncapped_1080p": ({}, {"RHYTHM_BENCH": "play:12", "RHYTHM_VSYNC": "0"}, (1920, 1080)),
    "play_vsync_cpu_stress": ({}, {"RHYTHM_BENCH": "play:14"}, None, "cpu"),
    "play_vsync_gpu_stress": ({}, {"RHYTHM_BENCH": "play:14"}, None, "gpu"),
    # Long enough (30 s) for the pacer cap to recover more than once; needs a map longer than ~35 s.
    "play_vsync_cpu_stress_long": ({}, {"RHYTHM_BENCH": "play:30"}, None, "cpu"),
    "play_vsync_gpu_stress_long": ({}, {"RHYTHM_BENCH": "play:30"}, None, "gpu"),
    "menu_vsync": ({}, {"RHYTHM_BENCH": "menu:6"}),
    "menu_uncapped": ({}, {"RHYTHM_BENCH": "menu:6", "RHYTHM_VSYNC": "0"}),
}


def display_env():
    env = dict(os.environ)
    env.setdefault("DISPLAY", ":0")
    if "XAUTHORITY" not in env:
        found = sorted(glob.glob("/run/user/%d/xauth_*" % os.getuid()))
        if found:
            env["XAUTHORITY"] = found[-1]
    return env


_MONITOR = []


def other_monitor(env):
    """Origin of the monitor the player is not on, so the benchmark never covers their game.

    Cached: asking xrandr while the game is still connecting to X stalls the X server for
    about 450 ms and inflated window_ms / first_frame_ms (0.6 s measured, 0.15 s real)."""
    if not _MONITOR:
        _MONITOR.append(_query_monitor(env))
    return _MONITOR[0]


def _query_monitor(env):
    want = os.environ.get("BENCH_MONITOR")
    out = subprocess.run(["xrandr", "--listmonitors"], env=env, capture_output=True, text=True).stdout
    monitors = []
    for line in out.splitlines()[1:]:
        parts = line.split()
        geometry = parts[2]
        x, y = geometry.split("+")[1:3]
        monitors.append((parts[1].lstrip("+").lstrip("*"), "*" in parts[1], int(x), int(y)))
    pick = [m for m in monitors if m[0] == want] or [m for m in monitors if not m[1]] or monitors
    return pick[0][2], pick[0][3]


def place_window(env, size, title):
    x, y = other_monitor(env)
    deadline = time.time() + 10
    while time.time() < deadline:
        found = subprocess.run(["xdotool", "search", "--name", "^%s$" % title], env=env, capture_output=True, text=True).stdout.split()
        if found:
            wid = max(found, key=int)
            subprocess.run(["xdotool", "windowmove", wid, str(x), str(y)], env=env)
            if size:
                subprocess.run(["xprop", "-id", wid, "-f", "_MOTIF_WM_HINTS", "32c", "-set", "_MOTIF_WM_HINTS", "2, 0, 0, 0, 0"], env=env)
                subprocess.run(["xdotool", "windowsize", wid, str(size[0]), str(size[1])], env=env)
                subprocess.run(["xdotool", "windowmove", wid, str(x), str(y)], env=env)
            time.sleep(0.4)
            geo = subprocess.run(["xdotool", "getwindowgeometry", wid], env=env, capture_output=True, text=True).stdout
            return " ".join(geo.split())
        time.sleep(0.05)
    return None


def gpu_now():
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"], capture_output=True, text=True).stdout
        return float(out.split()[0])
    except (OSError, ValueError, IndexError):
        return None


class GpuSampler:
    """Polls nvidia-smi from a thread (the CLI buffers its own loop mode when piped)."""

    def __init__(self):
        self.rows = []
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        while not self.stop.is_set():
            try:
                out = subprocess.run(
                    ["nvidia-smi", "--query-gpu=utilization.gpu,clocks.gr,power.draw", "--format=csv,noheader,nounits"],
                    capture_output=True, text=True, timeout=5).stdout
                self.rows.append([float(x) if x.strip() not in ("[N/A]", "N/A") else 0.0 for x in out.split(",")])
            except (OSError, ValueError, subprocess.TimeoutExpired):
                pass
            self.stop.wait(0.15)

    def summary(self):
        self.stop.set()
        self.thread.join()
        rows = self.rows[6:] or self.rows
        if not rows:
            return {}
        col = lambda i: statistics.mean(r[i] for r in rows)
        return {"gpu_util_pct": col(0), "gpu_clock_mhz": col(1), "gpu_power_w": col(2), "gpu_samples": len(rows)}


def start_stress(kind, env, base):
    """Background load the player really has: busy cores, or a second game flat out on the GPU."""
    if kind == "cpu":
        return [subprocess.Popen([sys.executable, "-c", "while True: pass"]) for _ in range(os.cpu_count() or 4)]
    if kind == "gpu":
        cwd = tempfile.mkdtemp(prefix="rhythm-stress-")
        os.makedirs(os.path.join(cwd, "maps"))
        with open(os.path.join(cwd, "settings.json"), "w") as f:
            json.dump(QUIET, f)
        hog_title = "rhythm-hog-%d" % os.getpid()
        hog = subprocess.Popen([BIN], cwd=cwd, env={**env, "RHYTHM_BENCH": "menu:60", "RHYTHM_VSYNC": "0", "RHYTHM_TITLE": hog_title},
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        hog.cwd = cwd
        time.sleep(1.5)
        place_window(env, None, hog_title)
        return [hog]
    return []


def stop_stress(procs):
    for p in procs:
        p.kill()
        p.wait()
        if getattr(p, "cwd", None):
            shutil.rmtree(p.cwd, ignore_errors=True)


def one_run(settings, env_extra, map_path, size=None, stress=None):
    cwd = tempfile.mkdtemp(prefix="rhythm-bench-")
    try:
        os.makedirs(os.path.join(cwd, "maps"))
        shutil.copy(map_path, os.path.join(cwd, "maps"))
        with open(os.path.join(cwd, "settings.json"), "w") as f:
            json.dump({**QUIET, **settings}, f)
        title = "rhythm-bench-%d-%d" % (os.getpid(), int(time.time() * 1000) % 1000000)
        env = {**display_env(), **env_extra, "RHYTHM_TITLE": title}
        other_monitor(env)  # before the game starts, see other_monitor
        load = start_stress(stress, env, cwd)
        idle_gpu = gpu_now()
        load1 = os.getloadavg()[0]
        gpu = GpuSampler()
        started = time.time()
        args = [BIN] if env_extra["RHYTHM_BENCH"].startswith("menu") else [BIN, map_path]
        proc = subprocess.Popen(args, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        placed = place_window(env, size, title)
        try:
            stdout, stderr = proc.communicate(timeout=90)
        finally:
            stop_stress(load)
        out = subprocess.CompletedProcess(args, proc.returncode, stdout, stderr)
        wall = time.time() - started
        extra = gpu.summary()
        extra.update({"gpu_busy_before_pct": idle_gpu, "load1_before": load1, "window_placed": placed})
        for line in out.stdout.splitlines():
            if line.startswith("BENCH "):
                data = json.loads(line[6:])
                data.update(extra)
                data["process_wall_s"] = wall
                return data
        raise RuntimeError("no BENCH line; stderr: " + out.stderr[-400:])
    finally:
        shutil.rmtree(cwd, ignore_errors=True)


def median_tree(runs):
    first = runs[0]
    if isinstance(first, dict):
        return {k: median_tree([r[k] for r in runs if k in r]) for k in first}
    if isinstance(first, (int, float)):
        return statistics.median(runs)
    return first


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", required=True)
    ap.add_argument("--map", required=True)
    ap.add_argument("--repeats", type=int, default=3)
    ap.add_argument("--only", default="")
    ap.add_argument("--out", default=os.path.join(ROOT, "perf"))
    ap.add_argument("--bin", default=None)
    a = ap.parse_args()
    global BIN
    BIN = os.path.abspath(a.bin) if a.bin else BIN
    names = [n for n in SCENARIOS if not a.only or n in a.only.split(",")]
    path = os.path.join(a.out, a.label + ".json")
    previous = json.load(open(path)) if a.only and os.path.exists(path) else None
    result = {"label": a.label, "repeats": a.repeats, "binary_mtime": os.path.getmtime(BIN), "binary_sha": hashlib.sha256(open(BIN, "rb").read()).hexdigest()[:12], "scenarios": {}, "runs": {}}
    for name in names:
        settings, env, *rest = SCENARIOS[name]
        size = rest[0] if rest else None
        stress = rest[1] if len(rest) > 1 else None
        runs = []
        for i in range(a.repeats):
            runs.append(one_run(settings, env, os.path.abspath(a.map), size, stress))
            print(name, i + 1, "/", a.repeats, "frame avg %.2f ms p99 %.2f cpu %.2f" % (
                runs[-1]["frame_ms"]["avg"], runs[-1]["frame_ms"]["p99"], runs[-1]["cpu_ms"]["avg"]), flush=True)
        result["scenarios"][name] = median_tree(runs)
        result["runs"][name] = runs
    os.makedirs(a.out, exist_ok=True)
    if previous and previous.get("binary_sha") == result["binary_sha"]:
        previous["scenarios"].update(result["scenarios"])
        previous["runs"].update(result["runs"])
        result = previous
    with open(path, "w") as f:
        json.dump(result, f, indent=1)
    print("wrote", path)


if __name__ == "__main__":
    sys.exit(main())

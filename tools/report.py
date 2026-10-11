#!/usr/bin/env python3
"""Builds perf/report.html from two bench.py outputs.

  python3 tools/report.py --before perf/before.json --after perf/after.json [--changes perf/changes.json]

Every figure is read from the JSON; regressions are listed in red in the same table.
"""
import argparse, html, json, os, statistics

METRICS = [
    ("frame_ms.avg", "frame avg ms", "lower"),
    ("frame_ms.p99", "frame p99 ms", "lower"),
    ("frame_ms.p999", "frame p99.9 ms", "lower"),
    ("frame_ms.max", "frame max ms", "lower"),
    ("slow_frames", "slow frames", "lower"),
    ("swap_ms.avg", "swap block ms avg", "lower"),
    ("pacer_delay_ms.avg", "pacer delay ms avg (higher = later input sampling)", "higher"),
    ("pacer_zero_pct", "frames with pacer at 0 %", "lower"),
    ("cpu_ms.avg", "cpu ms / frame", "lower"),
    ("update_ms.avg", "update ms / frame", "lower"),
    ("cpu_pct", "process cpu %", "lower"),
    ("gpu_util_pct", "gpu util %", "lower"),
    ("rss_mb", "rss MB", "lower"),
    ("load_ms.parse", "map parse ms", "lower"),
    ("load_ms.decode", "audio decode ms", "lower"),
    ("load_ms.music", "audio setup ms", "lower"),
    ("window_ms", "start to window ms", "lower"),
    ("first_frame_ms", "start to first frame ms", "lower"),
]
NOISE = 0.05


def dig(tree, path):
    for key in path.split("."):
        if not isinstance(tree, dict) or key not in tree:
            return None
        tree = tree[key]
    return tree if isinstance(tree, (int, float)) else None


def spread(runs, path):
    values = [v for v in (dig(r, path) for r in runs) if v is not None]
    return (min(values), max(values)) if values else None


def fmt(v):
    return "-" if v is None else ("%.2f" % v if abs(v) < 100 else "%.0f" % v)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--before", required=True)
    ap.add_argument("--after", required=True)
    ap.add_argument("--changes", default=None)
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "perf", "report.html"))
    a = ap.parse_args()
    before, after = json.load(open(a.before)), json.load(open(a.after))
    changes = json.load(open(a.changes)) if a.changes else []
    rows, worse, better = [], [], []
    for scenario, b in before["scenarios"].items():
        if scenario not in after["scenarios"]:
            continue
        x = after["scenarios"][scenario]
        for path, label, _ in METRICS:
            vb, va = dig(b, path), dig(x, path)
            if vb is None or va is None or (vb == 0 and va == 0):
                continue
            delta = (va - vb) / vb if vb else (1.0 if va > 0 else -1.0)
            sb = spread(before["runs"][scenario], path)
            sa = spread(after["runs"][scenario], path)
            noise = max((sb[1] - sb[0]) / vb if sb and vb else 0, (sa[1] - sa[0]) / vb if sa and vb else 0)
            if not vb:
                noise = 0.0
            real = abs(delta) > max(NOISE, noise)
            good_when_lower = next(d for p, _, d in METRICS if p == path) == "lower"
            kind = "same" if not real else ("better" if (delta < 0) == good_when_lower else "worse")
            rows.append((scenario, label, vb, va, delta, kind, noise))
            if kind == "worse":
                worse.append((scenario, label, vb, va, delta))
            if kind == "better":
                better.append((scenario, label, vb, va, delta))
    def table(items, title):
        if not items:
            return "<p class=muted>%s: none.</p>" % title
        body = "".join(
            "<tr><td>%s</td><td>%s</td><td class=n>%s</td><td class=n>%s</td><td class=n>%+.0f%%</td></tr>"
            % (html.escape(s), html.escape(l), fmt(vb), fmt(va), d * 100) for s, l, vb, va, d in items)
        return "<h3>%s</h3><table><tr><th>scenario<th>metric<th>before<th>after<th>change</tr>%s</table>" % (title, body)
    full = "".join(
        "<tr class=%s><td>%s</td><td>%s</td><td class=n>%s</td><td class=n>%s</td><td class=n>%+.0f%%</td><td class=n>%.0f%%</td></tr>"
        % (k, html.escape(s), html.escape(l), fmt(vb), fmt(va), d * 100, n * 100) for s, l, vb, va, d, k, n in rows)
    changelist = "".join(
        "<li><code>%s</code> - %s <span class=muted>(%s)</span></li>" % (html.escape(c["where"]), html.escape(c["what"]), html.escape(c["evidence"]))
        for c in changes)
    page = """<!doctype html><meta charset=utf-8><title>rhythm performance</title>
<style>
:root{--bg:#0b0b10;--ink:#f1eef2;--soft:#9b95a3;--rule:#26232d;--brand:#ff0059;--bad:#ff5d6c;--good:#5ee6a0}
body{background:var(--bg);color:var(--ink);font:15px/1.5 system-ui,sans-serif;max-width:980px;margin:48px auto;padding:0 20px}
h1{font-size:28px;margin:0 0 4px}h2{margin-top:40px}h3{margin:24px 0 8px}
table{border-collapse:collapse;width:100%%;font-variant-numeric:tabular-nums}
th,td{padding:5px 10px;border-bottom:1px solid var(--rule);text-align:left}th{color:var(--soft);font-weight:500}
.n{text-align:right}.muted{color:var(--soft)}tr.better td:nth-child(5){color:var(--good)}tr.worse td:nth-child(5){color:var(--bad);font-weight:600}
.hero{display:flex;gap:32px;margin:20px 0}.hero div{border-left:3px solid var(--brand);padding-left:12px}.hero b{font-size:26px;display:block}
code{color:var(--brand)}
</style>
<h1>rhythm performance</h1>
<p class=muted>%(b)s (binary %(bs)s) vs %(a)s (binary %(as)s), %(rep)s runs per scenario, median shown. Counts are more trustworthy than milliseconds on a shared machine; a change is only called real when it exceeds both 5%% and the spread between repeated runs.</p>
<div class=hero><div><b>%(nb)d</b>metrics better</div><div><b>%(nw)d</b>metrics worse</div><div><b>%(ns)d</b>unchanged within noise</div></div>
%(worse)s
%(better)s
<h2>Changes</h2><ul>%(changes)s</ul>
<h2>Every comparison</h2>
<table><tr><th>scenario<th>metric<th>before<th>after<th>change<th>run spread</tr>%(full)s</table>
<p class=muted>Method: release build, scripted autopilot plays one real map, each run in a throwaway working directory; the window sits on the second monitor; GPU util is sampled with nvidia-smi and includes whatever else was using the GPU (see gpu_busy_before_pct in the JSON). The method cannot separate GPU time from driver time; it cannot judge how the game looks or feels.</p>
""" % {
        "b": html.escape(before["label"]), "a": html.escape(after["label"]),
        "bs": before.get("binary_sha", "?"), "as": after.get("binary_sha", "?"),
        "rep": before["repeats"], "nb": len(better), "nw": len(worse), "ns": sum(1 for r in rows if r[5] == "same"),
        "worse": table(worse, "Metrics that came out worse"), "better": table(better, "Metrics that came out better"),
        "changes": changelist or "<li class=muted>none recorded</li>", "full": full}
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    open(a.out, "w").write(page)
    print("wrote", a.out, "better", len(better), "worse", len(worse))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Compare two Afterlife performance baseline JSON reports."""

import json
import sys


def get_nested(d, path, default=0.0):
    cur = d
    for key in path:
        if isinstance(cur, dict) and key in cur and cur[key] is not None:
            cur = cur[key]
        else:
            return default
    return cur


METRICS = [
    ("TTFF (ms)", lambda s: get_nested(s, ["ttff_ms"]), "f1", True),
    ("CPU Total %", lambda s: get_nested(s, ["process", "cpu_total_pct"]), "pct", True),
    ("RSS Peak (MB)", lambda s: get_nested(s, ["process", "rss_mb_peak"]), "f1", True),
    ("Metal VRAM Peak (MB)", lambda s: get_nested(s, ["metal", "gpu_allocated_mb_peak"]), "f1", True),
    ("UI Render Hz", lambda s: get_nested(s, ["ui", "render_hz"]), "f1", True),
    ("UI Render p50 (us)", lambda s: get_nested(s, ["ui", "render_cpu_us", "p50"]), "f0", True),
    ("GPUI Prepaint p50 (us)", lambda s: get_nested(s, ["gpui_draw", "prepaint_us", "p50"]), "f0", True),
    ("GPUI Paint p50 (us)", lambda s: get_nested(s, ["gpui_draw", "paint_us", "p50"]), "f0", True),
    ("State Clone avg (us)", lambda s: get_nested(s, ["ui", "state_clone_us", "avg"]), "f1", True),
    ("Metal FPS", lambda s: get_nested(s, ["metal", "fps"]), "f1", False),
    ("Present Interval p95(ms)", lambda s: get_nested(s, ["metal", "present_interval_ms", "p95"]), "f2", True),
    ("Dropped Frames >18ms", lambda s: get_nested(s, ["metal", "dropped_frames_18ms"]), "f0", True),
    ("Metal CPU excl wait (us)", lambda s: get_nested(s, ["metal", "draw_cpu_us", "p50"]), "f0", True),
    ("next_drawable wait (us)", lambda s: get_nested(s, ["metal", "next_drawable_us", "p50"]), "f0", True),
    ("Metal Encode p50 (us)", lambda s: get_nested(s, ["metal", "encode_us", "p50"]), "f0", True),
    ("GPU Time p50 (us)", lambda s: get_nested(s, ["metal", "gpu_us", "p50"]), "f0", True),
    ("GPU Time p95 (us)", lambda s: get_nested(s, ["metal", "gpu_us", "p95"]), "f0", True),
    ("Encoder Switches/f", lambda s: get_nested(s, ["metal", "encoder_switches_avg"]), "f2", True),
    ("Blit Count/f", lambda s: get_nested(s, ["metal", "blit_count_avg"]), "f2", True),
    ("Blit Full-Frame Ratio", lambda s: get_nested(s, ["metal", "blit_full_frame_ratio_avg"]), "f3", True),
    ("Instance Bytes/f", lambda s: get_nested(s, ["metal", "primitives_avg", "instance_bytes"]), "f0", True),
]


def fmt(val, kind):
    if kind == "pct":
        return f"{val:.1f}%"
    if kind == "f0":
        return f"{val:.0f}"
    if kind == "f1":
        return f"{val:.1f}"
    if kind == "f2":
        return f"{val:.2f}"
    if kind == "f3":
        return f"{val:.3f}"
    return str(val)


def delta_str(before, after, lower_is_better):
    if abs(before) < 1e-9:
        if abs(after) < 1e-9:
            return "0.0%"
        return "n/a"
    pct = ((after - before) / before) * 100.0
    sign = "+" if pct > 0 else ""
    tag = ""
    if abs(pct) >= 3.0:
        improved = (pct < 0) if lower_is_better else (pct > 0)
        tag = " [BETTER]" if improved else " [WORSE]"
    return f"{sign}{pct:.1f}%{tag}"


def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <before.json> <after.json>", file=sys.stderr)
        sys.exit(2)

    with open(sys.argv[1]) as f:
        a = json.load(f)
    with open(sys.argv[2]) as f:
        b = json.load(f)

    print(f"BEFORE: {a['label']} ({a['git']['short_sha']})  ->  AFTER: {b['label']} ({b['git']['short_sha']})")

    for scenario in a["scenarios"]:
        if scenario not in b["scenarios"]:
            continue
        sa = a["scenarios"][scenario]
        sb = b["scenarios"][scenario]
        print(f"\n=== Scenario: {scenario} ===")
        print(f"{'Metric':<26} {'Before':>12} {'After':>12} {'Delta':>20}")
        print("-" * 74)
        for name, getter, kind, lower_is_better in METRICS:
            va = getter(sa)
            vb = getter(sb)
            print(
                f"{name:<26} {fmt(va, kind):>12} {fmt(vb, kind):>12} "
                f"{delta_str(va, vb, lower_is_better):>20}"
            )


if __name__ == "__main__":
    main()

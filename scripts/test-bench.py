#!/usr/bin/env python3
"""Reproducible benchmark harness for Afterlife first-party test suite wait times.

Measures:
  (a) Execution-only & warm no-op full suite wall-clock time (median / P95)
  (b) Single-line incremental edit compile+link & total test wall-clock time
      for afterlife-core, afterlife-cli, and afterlife-ui (median / P95)
  (c) Per-crate, per-binary, and slowest Top 10 individual test breakdowns
"""

import argparse
import json
import math
import os
import platform
import re
import statistics
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

FIRST_PARTY_CRATES = [
    "afterlife-core",
    "afterlife-protocol",
    "afterlife-storage",
    "afterlife-daemon",
    "afterlife-cli",
    "afterlife-design",
    "afterlife-ui",
]

INCREMENTAL_TARGETS = [
    ("afterlife-core", "crates/afterlife-core/src/lib.rs"),
    ("afterlife-cli", "crates/afterlife-cli/src/main.rs"),
    ("afterlife-ui", "crates/afterlife-ui/src/main.rs"),
]

PASSED_RE = re.compile(r"test result: ok\. (\d+) passed; (\d+) failed")


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def cargo_env(root: Path) -> dict:
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(root / "target"))
    return env


def stats_ms(samples_ms: list[float]) -> dict:
    ordered = sorted(samples_ms)
    n = len(ordered)
    if n == 0:
        return {"runs": 0, "median_ms": 0.0, "p95_ms": 0.0, "min_ms": 0.0, "max_ms": 0.0, "samples_ms": []}
    p95_idx = min(n - 1, max(0, math.ceil(0.95 * n) - 1))
    return {
        "runs": n,
        "median_ms": round(statistics.median(ordered), 2),
        "p95_ms": round(ordered[p95_idx], 2),
        "min_ms": round(ordered[0], 2),
        "max_ms": round(ordered[-1], 2),
        "samples_ms": [round(x, 2) for x in samples_ms],
    }


def crate_pkg_args(crates: list[str]) -> list[str]:
    args: list[str] = []
    for c in crates:
        args.extend(["-p", c])
    return args


def discover_test_binaries(root: Path, env: dict, crates: list[str]) -> list[dict]:
    cmd = ["cargo", "test", *crate_pkg_args(crates), "--no-run", "--message-format=json"]
    proc = subprocess.run(cmd, cwd=root, env=env, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        raise RuntimeError(f"Failed building test binaries: {' '.join(cmd)}")

    binaries: list[dict] = []
    seen: set[str] = set()
    for line in proc.stdout.splitlines():
        if not line.startswith("{"):
            continue
        msg = json.loads(line)
        if msg.get("reason") != "compiler-artifact":
            continue
        exe = msg.get("executable")
        if not exe or exe in seen:
            continue
        if not msg.get("profile", {}).get("test", False):
            continue
        seen.add(exe)
        pkg_id = msg.get("package_id", "")
        target = msg.get("target", {})
        pkg_name = target.get("name", "")
        for c in crates:
            if f"/{c}#" in pkg_id or pkg_id.startswith(f"{c} "):
                pkg_name = c
                break
        binaries.append(
            {
                "crate": pkg_name,
                "target": target.get("name", Path(exe).name),
                "kind": (target.get("kind") or ["unknown"])[0],
                "executable": exe,
            }
        )
    binaries.sort(key=lambda b: (b["crate"], b["kind"], b["target"]))
    return binaries


def list_tests_in_binary(exe: str, root: Path, env: dict) -> list[str]:
    proc = subprocess.run([exe, "--list", "--format", "terse"], cwd=root, env=env, capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(f"Failed listing tests in {exe}: {proc.stderr}")
    tests = []
    for line in proc.stdout.splitlines():
        if line.endswith(": test"):
            tests.append(line[: -len(": test")])
    return tests


def run_binary(b: dict, root: Path, env: dict, threads: int | None = None) -> dict:
    cmd = [b["executable"], "--color=never"]
    if threads is not None:
        cmd.append(f"--test-threads={threads}")
    t0 = time.perf_counter()
    proc = subprocess.run(cmd, cwd=root, env=env, capture_output=True, text=True)
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    if proc.returncode != 0:
        raise RuntimeError(f"Test binary failed ({b['crate']}::{b['target']}):\n{proc.stdout}\n{proc.stderr}")
    passed = 0
    failed = 0
    for m in PASSED_RE.finditer(proc.stdout):
        passed += int(m.group(1))
        failed += int(m.group(2))
    return {
        "crate": b["crate"],
        "target": b["target"],
        "kind": b["kind"],
        "passed": passed,
        "failed": failed,
        "elapsed_ms": round(elapsed_ms, 2),
    }


def measure_execution_only(root: Path, env: dict, binaries: list[dict], runs: int, parallel: bool) -> dict:
    samples_ms: list[float] = []
    per_binary_samples: dict[str, list[float]] = {}
    last_meta: dict[str, dict] = {}
    total_passed = 0

    for _ in range(runs):
        t0 = time.perf_counter()
        results: list[dict] = []
        if parallel:
            workers = min(len(binaries), os.cpu_count() or 4)
            with ThreadPoolExecutor(max_workers=max(1, workers)) as pool:
                futs = [pool.submit(run_binary, b, root, env) for b in binaries]
                for fut in as_completed(futs):
                    results.append(fut.result())
        else:
            for b in binaries:
                results.append(run_binary(b, root, env))
        samples_ms.append((time.perf_counter() - t0) * 1000.0)
        total_passed = sum(r["passed"] for r in results)
        for r in results:
            key = f"{r['crate']}::{r['target']} ({r['kind']})"
            per_binary_samples.setdefault(key, []).append(r["elapsed_ms"])
            last_meta[key] = r

    per_binary = []
    per_crate: dict[str, dict] = {}
    for key, s in per_binary_samples.items():
        meta = last_meta[key]
        st = stats_ms(s)
        entry = {
            "id": key,
            "crate": meta["crate"],
            "target": meta["target"],
            "kind": meta["kind"],
            "passed": meta["passed"],
            **st,
        }
        per_binary.append(entry)
        c = per_crate.setdefault(meta["crate"], {"crate": meta["crate"], "binaries": 0, "passed": 0, "sum_median_ms": 0.0})
        c["binaries"] += 1
        c["passed"] += meta["passed"]
        c["sum_median_ms"] = round(c["sum_median_ms"] + st["median_ms"], 2)

    per_binary.sort(key=lambda x: x["median_ms"], reverse=True)
    crate_list = sorted(per_crate.values(), key=lambda x: x["sum_median_ms"], reverse=True)
    return {
        "mode": "parallel" if parallel else "serial",
        "total_passed": total_passed,
        "summary": stats_ms(samples_ms),
        "per_crate": crate_list,
        "per_binary": per_binary,
    }


def measure_slow_tests(root: Path, env: dict, binaries: list[dict], top_n: int = 10) -> list[dict]:
    items: list[dict] = []
    for b in binaries:
        tests = list_tests_in_binary(b["executable"], root, env)
        for t in tests:
            t0 = time.perf_counter()
            proc = subprocess.run(
                [b["executable"], "--exact", t, "--test-threads=1", "--color=never"],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            elapsed_ms = (time.perf_counter() - t0) * 1000.0
            if proc.returncode != 0:
                raise RuntimeError(f"Single test failed ({b['crate']}::{b['target']}::{t}):\n{proc.stdout}\n{proc.stderr}")
            items.append(
                {
                    "crate": b["crate"],
                    "target": b["target"],
                    "test": t,
                    "elapsed_ms": round(elapsed_ms, 2),
                }
            )
    items.sort(key=lambda x: x["elapsed_ms"], reverse=True)
    return items[:top_n]


def run_timed_cmd(cmd: list[str], root: Path, env: dict) -> float:
    t0 = time.perf_counter()
    proc = subprocess.run(cmd, cwd=root, env=env, capture_output=True, text=True)
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    if proc.returncode != 0:
        raise RuntimeError(f"Command failed: {' '.join(cmd)}\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}")
    return elapsed_ms


def measure_command_runs(cmd: list[str], root: Path, env: dict, runs: int) -> dict:
    samples = [run_timed_cmd(cmd, root, env) for _ in range(runs)]
    return {"command": cmd, **stats_ms(samples)}


def measure_incremental_edits(
    root: Path,
    env: dict,
    crates: list[str],
    runs: int,
    full_test_cmd: list[str],
) -> dict:
    results = {}
    compile_cmd = ["cargo", "test", *crate_pkg_args(crates), "--no-run"]
    for crate_name, rel_path in INCREMENTAL_TARGETS:
        path = root / rel_path
        original = path.read_bytes()
        compile_samples: list[float] = []
        total_samples: list[float] = []
        try:
            for idx in range(runs):
                marker = f"\n#[allow(dead_code)]\nconst __AFTERLIFE_BENCH_NONCE: u64 = {idx + 1};\n".encode()
                path.write_bytes(original + marker)
                c_ms = run_timed_cmd(compile_cmd, root, env)
                compile_samples.append(c_ms)

                marker2 = f"\n#[allow(dead_code)]\nconst __AFTERLIFE_BENCH_NONCE: u64 = {idx + 101};\n".encode()
                path.write_bytes(original + marker2)
                t_ms = run_timed_cmd(full_test_cmd, root, env)
                total_samples.append(t_ms)
        finally:
            path.write_bytes(original)
        # Re-warm unmodified build state so subsequent targets start cleanly
        run_timed_cmd(compile_cmd, root, env)
        results[crate_name] = {
            "file": rel_path,
            "compile_link_only": stats_ms(compile_samples),
            "full_test_total": stats_ms(total_samples),
        }
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description="Benchmark Afterlife test suite compile, link, and execution times.")
    parser.add_argument("--runs", type=int, default=3, help="Number of timed repetitions per metric (default: 3)")
    parser.add_argument("--label", default="snapshot", help="Label for this measurement run")
    parser.add_argument("--out", help="Write full JSON report to this path")
    parser.add_argument(
        "--runner",
        choices=["cargo", "fast"],
        default="cargo",
        help="Full test command used for no-op and incremental total runs ('cargo' = standard cargo test, 'fast' = scripts/test-fast.sh)",
    )
    parser.add_argument("--skip-incremental", action="store_true", help="Skip 1-line edit incremental benchmarks")
    parser.add_argument("--skip-slow-tests", action="store_true", help="Skip per-test Top 10 profiling")
    args = parser.parse_args()

    root = repo_root()
    env = cargo_env(root)

    # Warm build before timing
    binaries = discover_test_binaries(root, env, FIRST_PARTY_CRATES)

    if args.runner == "fast":
        full_cmd = [str(root / "scripts" / "test-fast.sh")]
    else:
        full_cmd = ["cargo", "test", *crate_pkg_args(FIRST_PARTY_CRATES)]

    git_sha = subprocess.check_output(["git", "rev-parse", "--short", "HEAD"], cwd=root, text=True).strip()
    rustc_ver = subprocess.check_output(["rustc", "--version"], cwd=root, text=True).strip()

    report: dict = {
        "label": args.label,
        "git_sha": git_sha,
        "rustc": rustc_ver,
        "platform": platform.platform(),
        "cpu_count": os.cpu_count(),
        "runs": args.runs,
        "runner": args.runner,
        "crates": FIRST_PARTY_CRATES,
        "binary_count": len(binaries),
    }

    report["warm_full_suite"] = measure_command_runs(full_cmd, root, env, args.runs)
    report["execution_serial"] = measure_execution_only(root, env, binaries, args.runs, parallel=False)
    report["execution_parallel"] = measure_execution_only(root, env, binaries, args.runs, parallel=True)

    if not args.skip_slow_tests:
        report["slowest_tests_top10"] = measure_slow_tests(root, env, binaries, top_n=10)

    if not args.skip_incremental:
        report["incremental_single_line"] = measure_incremental_edits(root, env, FIRST_PARTY_CRATES, args.runs, full_cmd)

    out_text = json.dumps(report, indent=2, ensure_ascii=False)
    if args.out:
        out_path = Path(args.out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(out_text + "\n", encoding="utf-8")

    # Concise console summary
    print(f"=== Test Benchmark [{args.label}] ({git_sha}, runner={args.runner}, runs={args.runs}) ===")
    print(
        f"Warm full suite ({' '.join(full_cmd)}): "
        f"median={report['warm_full_suite']['median_ms']}ms  p95={report['warm_full_suite']['p95_ms']}ms"
    )
    print(
        f"Execution-only (serial, {report['execution_serial']['total_passed']} tests): "
        f"median={report['execution_serial']['summary']['median_ms']}ms  p95={report['execution_serial']['summary']['p95_ms']}ms"
    )
    print(
        f"Execution-only (parallel, {report['execution_parallel']['total_passed']} tests): "
        f"median={report['execution_parallel']['summary']['median_ms']}ms  p95={report['execution_parallel']['summary']['p95_ms']}ms"
    )
    if "incremental_single_line" in report:
        print("Incremental 1-line edit:")
        for cname, data in report["incremental_single_line"].items():
            c = data["compile_link_only"]
            t = data["full_test_total"]
            print(
                f"  {cname:16s} compile+link median={c['median_ms']}ms (p95={c['p95_ms']}ms) | "
                f"total median={t['median_ms']}ms (p95={t['p95_ms']}ms)"
            )
    if "slowest_tests_top10" in report:
        print("Slowest Top 10 tests:")
        for i, t in enumerate(report["slowest_tests_top10"], 1):
            print(f"  {i:2d}. {t['elapsed_ms']:7.2f}ms  {t['crate']}::{t['target']}::{t['test']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

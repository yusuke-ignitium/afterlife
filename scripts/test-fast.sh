#!/usr/bin/env bash
set -euo pipefail

# Fastest portable runner for all first-party Afterlife tests:
# builds once via Cargo, then executes all test binaries concurrently across CPU cores.
#
# Usage:
#   ./scripts/test-fast.sh                # Build and run all first-party tests in parallel
#   ./scripts/test-fast.sh --repeat 3     # Run 3 consecutive passes (0s rebuild between passes)

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"

REPEAT=1
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repeat)
      REPEAT="$2"
      shift 2
      ;;
    --)
      shift
      EXTRA_ARGS+=("$@")
      break
      ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

python3 - "$ROOT_DIR" "$REPEAT" ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"} <<'PY'
import json
import os
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

root = sys.argv[1]
repeat = int(sys.argv[2])
extra_args = sys.argv[3:]

crates = [
    "afterlife-core",
    "afterlife-protocol",
    "afterlife-storage",
    "afterlife-daemon",
    "afterlife-cli",
    "afterlife-design",
    "afterlife-ui",
]

pkg_args = []
for c in crates:
    pkg_args.extend(["-p", c])

t_build_start = time.perf_counter()
build_cmd = ["cargo", "test", *pkg_args, "--no-run", "--message-format=json"]
build = subprocess.run(build_cmd, cwd=root, capture_output=True, text=True)
build_ms = (time.perf_counter() - t_build_start) * 1000.0
if build.returncode != 0:
    sys.stderr.write(build.stderr)
    sys.exit(build.returncode)

binaries = []
seen = set()
for line in build.stdout.splitlines():
    if not line.startswith("{"):
        continue
    msg = json.loads(line)
    if msg.get("reason") != "compiler-artifact":
        continue
    exe = msg.get("executable")
    if not exe or exe in seen or not msg.get("profile", {}).get("test", False):
        continue
    seen.add(exe)
    target = msg.get("target", {})
    binaries.append((target.get("name", os.path.basename(exe)), exe))

passed_re = re.compile(r"test result: ok\. (\d+) passed; (\d+) failed")
workers = min(max(1, len(binaries)), os.cpu_count() or 4)

def run_one(item):
    name, exe = item
    t0 = time.perf_counter()
    proc = subprocess.run([exe, *extra_args], cwd=root, capture_output=True, text=True)
    ms = (time.perf_counter() - t0) * 1000.0
    passed = 0
    for m in passed_re.finditer(proc.stdout):
        passed += int(m.group(1))
    return name, proc.returncode, passed, ms, proc.stdout, proc.stderr

for r in range(1, repeat + 1):
    t_run_start = time.perf_counter()
    failures = []
    total_passed = 0
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futs = [pool.submit(run_one, b) for b in binaries]
        for fut in as_completed(futs):
            name, rc, passed, ms, out, err = fut.result()
            total_passed += passed
            if rc != 0:
                failures.append((name, rc, out, err))
    run_ms = (time.perf_counter() - t_run_start) * 1000.0
    if failures:
        for name, rc, out, err in failures:
            sys.stderr.write(f"\n=== FAILED: {name} (exit {rc}) ===\n{out}\n{err}\n")
        sys.exit(1)
    suffix = f" [pass {r}/{repeat}]" if repeat > 1 else ""
    print(
        f"ok{suffix}: {total_passed} passed across {len(binaries)} binaries "
        f"(build {build_ms:.0f}ms, exec {run_ms:.0f}ms)"
    )
PY

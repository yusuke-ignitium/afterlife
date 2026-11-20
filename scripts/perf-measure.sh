#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"

LABEL="${1:-run}"
WARMUP_MS="${AFTERLIFE_PERF_WARMUP_MS:-1500}"
DURATION_MS="${AFTERLIFE_PERF_DURATION_MS:-5000}"

TMP_DIR="$ROOT_DIR/perf/tmp"
OUT_DIR="$ROOT_DIR/perf/baselines"
mkdir -p "$TMP_DIR" "$OUT_DIR"

echo "==> Building release binaries (afterlife-ui, afterlife-daemon)..."
cargo build --release -p afterlife-ui --bin afterlife-desktop
cargo build --release -p afterlife-daemon --bin afterlifed

BIN="$CARGO_TARGET_DIR/release/afterlife-desktop"
DAEMON_BIN="$CARGO_TARGET_DIR/release/afterlifed"
if [[ ! -x "$BIN" ]]; then
  echo "ERROR: binary not found at $BIN" >&2
  exit 1
fi
if [[ ! -x "$DAEMON_BIN" ]]; then
  echo "ERROR: binary not found at $DAEMON_BIN" >&2
  exit 1
fi

SHORT_SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
FULL_SHA="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
DIRTY="$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
OUT_FILE="$OUT_DIR/${LABEL}_${SHORT_SHA}.json"

SCENARIOS=(
  "idle_liquid_glass"
  "idle_liquid_glass_images_on"
  "idle_appearance_drawer"
  "idle_wireframe"
  "interactive_churn"
  "inactive_power_save"
)

for SCENARIO in "${SCENARIOS[@]}"; do
  SC_OUT="$TMP_DIR/${SCENARIO}.json"
  rm -f "$SC_OUT"
  echo "==> Running UI/Metal scenario: ${SCENARIO} (warmup=${WARMUP_MS}ms, duration=${DURATION_MS}ms)..."
  AFTERLIFE_PERF_OUT="$SC_OUT" \
  AFTERLIFE_PERF_SCENARIO="$SCENARIO" \
  AFTERLIFE_PERF_WARMUP_MS="$WARMUP_MS" \
  AFTERLIFE_PERF_DURATION_MS="$DURATION_MS" \
  "$BIN"
  if [[ ! -f "$SC_OUT" ]]; then
    echo "ERROR: scenario $SCENARIO did not produce $SC_OUT" >&2
    exit 1
  fi
done

echo "==> Running Backend (afterlifed HTTP + MCP + SQLite WAL) benchmark..."
BACKEND_OUT="$TMP_DIR/backend.json"
BACKEND_TMP="$TMP_DIR/backend_data"
rm -rf "$BACKEND_TMP" "$BACKEND_OUT"
mkdir -p "$BACKEND_TMP"
PORT=14319
cat > "$BACKEND_TMP/bench.toml" <<EOF
[server]
bind_addr = "127.0.0.1:${PORT}"
public_base_url = "http://127.0.0.1:${PORT}"

[auth]
mode = "local_dev"
default_owner_id = "bench-owner"
default_email = "bench@local"
issuer = "http://127.0.0.1:${PORT}"
jwt_secret_env = "AFTERLIFE_JWT_SECRET"
access_token_ttl_secs = 900
refresh_token_ttl_secs = 2592000

[storage]
backend = "sqlite"
path = "${BACKEND_TMP}/afterlife.db"

[blobs]
backend = "local_fs"
dir = "${BACKEND_TMP}/artifacts"

[telemetry]
backend = "jsonl_file"
dir = "${BACKEND_TMP}/telemetry"

[event_bus]
backend = "in_memory"
capacity = 1024
EOF

"$DAEMON_BIN" --config "$BACKEND_TMP/bench.toml" >"$BACKEND_TMP/daemon.log" 2>&1 &
DAEMON_PID=$!
trap 'kill "$DAEMON_PID" 2>/dev/null || true' EXIT

python3 - "$PORT" "$BACKEND_OUT" <<'PY'
import http.client, json, sys, time

port = int(sys.argv[1])
out_path = sys.argv[2]

def wait_ready():
    deadline = time.time() + 10.0
    while time.time() < deadline:
        try:
            conn = http.client.HTTPConnection("127.0.0.1", port, timeout=1.0)
            conn.request("GET", "/v1/health")
            r = conn.getresponse()
            r.read()
            conn.close()
            if r.status == 200:
                return
        except Exception:
            time.sleep(0.05)
    raise RuntimeError("afterlifed failed to become ready")

def stats(samples_us):
    if not samples_us:
        return {"count": 0, "avg": 0.0, "p50": 0.0, "p95": 0.0, "p99": 0.0, "max": 0.0}
    s = sorted(samples_us)
    n = len(s)
    pick = lambda p: s[min(n - 1, round((n - 1) * p))]
    return {
        "count": n,
        "avg": round(sum(s) / n, 1),
        "p50": round(pick(0.50), 1),
        "p95": round(pick(0.95), 1),
        "p99": round(pick(0.99), 1),
        "max": round(s[-1], 1),
    }

wait_ready()
conn = http.client.HTTPConnection("127.0.0.1", port, timeout=5.0)
headers = {"Content-Type": "application/json", "Accept": "application/json"}

def mcp(req_id, method, params=None):
    payload = {"jsonrpc": "2.0", "id": req_id, "method": method}
    if params is not None:
        payload["params"] = params
    body = json.dumps(payload).encode()
    t0 = time.perf_counter()
    conn.request("POST", "/mcp", body=body, headers=headers)
    r = conn.getresponse()
    raw = r.read()
    dt_us = (time.perf_counter() - t0) * 1_000_000.0
    return r.status, json.loads(raw), dt_us

def mcp_ok(req_id, method, params=None):
    st, resp, dt = mcp(req_id, method, params)
    assert st == 200, f"HTTP {st}: {resp}"
    assert "error" not in resp, f"JSON-RPC error: {resp}"
    res = resp.get("result", {})
    assert not res.get("isError"), f"MCP tool error: {res}"
    return res, dt

mcp_ok(1, "initialize", {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "bench", "version": "1.0"}})

health_us = []
t_health = time.perf_counter()
for _ in range(200):
    t0 = time.perf_counter()
    conn.request("GET", "/v1/health")
    r = conn.getresponse()
    r.read()
    assert r.status == 200
    health_us.append((time.perf_counter() - t0) * 1_000_000.0)
health_rps = 200.0 / max(time.perf_counter() - t_health, 1e-6)

list_us = []
t_list = time.perf_counter()
for i in range(200):
    _, dt = mcp_ok(10 + i, "tools/call", {"name": "jobs_list", "arguments": {"limit": 50}})
    list_us.append(dt)
list_rps = 200.0 / max(time.perf_counter() - t_list, 1e-6)

create_us = []
created_ids = []
t_create = time.perf_counter()
for i in range(100):
    res, dt = mcp_ok(1000 + i, "tools/call", {
        "name": "jobs_create",
        "arguments": {
            "command_id": f"bench-create-{i}",
            "title": f"Bench job {i}",
            "raw_input": "Throughput benchmark payload",
            "target_district_hint": "DIST-001",
        },
    })
    create_us.append(dt)
    text = res.get("content", [{}])[0].get("text", "{}")
    parsed = json.loads(text)
    if "job_id" in parsed:
        created_ids.append(parsed["job_id"])
    else:
        for ev in parsed.get("events", []):
            if isinstance(ev, dict) and ev.get("type") == "job_queued" and "job_id" in ev:
                created_ids.append(ev["job_id"])
create_rps = 100.0 / max(time.perf_counter() - t_create, 1e-6)
assert len(created_ids) >= 50, f"expected >=50 created job ids, got {len(created_ids)}"

assign_us = []
t_assign = time.perf_counter()
for i, jid in enumerate(created_ids[:50]):
    _, dt = mcp_ok(2000 + i, "tools/call", {
        "name": "jobs_assign",
        "arguments": {"command_id": f"bench-assign-{i}", "job_id": jid, "district_id": "DIST-001"},
    })
    assign_us.append(dt)
assign_rps = 50.0 / max(time.perf_counter() - t_assign, 1e-6)

claim_us = []
t_claim = time.perf_counter()
for i, jid in enumerate(created_ids[:50]):
    _, dt = mcp_ok(3000 + i, "tools/call", {
        "name": "jobs_claim",
        "arguments": {"command_id": f"bench-claim-{i}", "job_id": jid, "runner_label": "bench-runner"},
    })
    claim_us.append(dt)
claim_rps = 50.0 / max(time.perf_counter() - t_claim, 1e-6)

conn.request("GET", "/v1/metrics")
r = conn.getresponse()
daemon_metrics = json.loads(r.read())
conn.close()

for tool_name, tool_stat in daemon_metrics.get("mcp_tools", {}).items():
    assert tool_stat.get("errors", 0) == 0, f"tool {tool_name} had errors: {tool_stat}"

result = {
    "client_benchmarks": {
        "http_health_200": {"rps": round(health_rps, 1), "latency_us": stats(health_us)},
        "mcp_jobs_list_200": {"rps": round(list_rps, 1), "latency_us": stats(list_us)},
        "mcp_jobs_create_100": {"rps": round(create_rps, 1), "latency_us": stats(create_us)},
        "mcp_jobs_assign_50": {"rps": round(assign_rps, 1), "latency_us": stats(assign_us)},
        "mcp_jobs_claim_50": {"rps": round(claim_rps, 1), "latency_us": stats(claim_us)},
    },
    "daemon_metrics": daemon_metrics,
}
with open(out_path, "w") as f:
    json.dump(result, f, indent=2)
    f.write("\n")
PY

kill "$DAEMON_PID" 2>/dev/null || true
wait "$DAEMON_PID" 2>/dev/null || true
trap - EXIT

python3 - "$OUT_FILE" "$TMP_DIR" "$LABEL" "$FULL_SHA" "$SHORT_SHA" "$DIRTY" <<'PY'
import json, os, platform, subprocess, sys, datetime

out_file, tmp_dir, label, full_sha, short_sha, dirty = sys.argv[1:7]

def sh(cmd):
    try:
        return subprocess.check_output(cmd, text=True).strip()
    except Exception:
        return "unknown"

scenario_names = [
    "idle_liquid_glass",
    "idle_liquid_glass_images_on",
    "idle_appearance_drawer",
    "idle_wireframe",
    "interactive_churn",
    "inactive_power_save",
]
scenarios = {}
for name in scenario_names:
    with open(os.path.join(tmp_dir, f"{name}.json")) as f:
        scenarios[name] = json.load(f)

with open(os.path.join(tmp_dir, "backend.json")) as f:
    backend = json.load(f)

bundle = {
    "label": label,
    "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "git": {
        "commit": full_sha,
        "short_sha": short_sha,
        "dirty_files": int(dirty),
    },
    "machine": {
        "os": platform.platform(),
        "macos_version": sh(["sw_vers", "-productVersion"]),
        "cpu": sh(["sysctl", "-n", "machdep.cpu.brand_string"]),
        "arch": platform.machine(),
    },
    "scenarios": scenarios,
    "backend": backend,
}

with open(out_file, "w") as f:
    json.dump(bundle, f, indent=2)
    f.write("\n")

print(f"\nSaved full perf report: {out_file}\n")
hdr = (
    f"{'Scenario':<28} {'TTFF':>6} {'CPU%':>6} {'RSS(MB)':>8} {'VRAM(MB)':>8} "
    f"{'FPS':>6} {'Jank>18':>7} {'DrawCPU':>8} {'WaitDrw':>8} {'Enc(us)':>8} "
    f"{'GPU p50/p95':>13} {'GPUI Draw':>10} {'BlitR':>6}"
)
print(hdr)
print("-" * len(hdr))
for name, sc in scenarios.items():
    p = sc["process"]
    m = sc["metal"]
    g = sc.get("gpui_draw", {})
    ttff = f"{sc['ttff_ms']:.0f}ms" if sc.get("ttff_ms") is not None else "n/a"
    gpu_p = f"{m['gpu_us']['p50']:.0f}/{m['gpu_us']['p95']:.0f}"
    gpui_p = f"{g.get('total_us', {}).get('p50', 0):.0f}us"
    print(
        f"{name:<28} "
        f"{ttff:>6} "
        f"{p['cpu_total_pct']:>5.1f}% "
        f"{p['rss_mb_peak']:>8.1f} "
        f"{m.get('gpu_allocated_mb_peak', 0.0):>8.1f} "
        f"{m['fps']:>6.1f} "
        f"{m.get('dropped_frames_18ms', 0):>7d} "
        f"{m.get('draw_cpu_us', {}).get('p50', 0):>7.0f}u "
        f"{m.get('next_drawable_us', {}).get('p50', 0):>7.0f}u "
        f"{m['encode_us']['p50']:>7.0f}u "
        f"{gpu_p:>13} "
        f"{gpui_p:>10} "
        f"{m['blit_full_frame_ratio_avg']:>5.2f}x"
    )

print("\nBackend (afterlifed HTTP + MCP + SQLite WAL):")
for k, v in backend["client_benchmarks"].items():
    lat = v["latency_us"]
    print(f"  {k:<22} {v['rps']:>8.1f} req/s   p50={lat['p50']:>6.0f}us  p95={lat['p95']:>6.0f}us  p99={lat['p99']:>6.0f}us  max={lat['max']:>6.0f}us")
PY

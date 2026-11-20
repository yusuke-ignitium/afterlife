#!/usr/bin/env bash
set -euo pipefail

# Ultra-fast, zero-flake black-box E2E runner with automatic parallelization & CI sharding.
#
# Usage:
#   ./scripts/e2e.sh                       # Run all E2E tests in full local CPU parallelism
#   ./scripts/e2e.sh --shard 1/4           # Run deterministic shard 1 of 4 (for O(1) CI scaling)
#   ./scripts/e2e.sh --repeat 5            # Run 5 consecutive passes to verify 0% flakiness
#   E2E_BIN=./e2e_bin E2E_SHARD=2/4 ./scripts/e2e.sh  # Run prebuilt binary directly (0s build in CI)

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

export PATH="$HOME/.cargo/bin:$PATH"

SHARD="${E2E_SHARD:-}"
E2E_BIN="${E2E_BIN:-}"
REPEAT=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --shard)
      SHARD="$2"
      shift 2
      ;;
    --bin)
      E2E_BIN="$2"
      shift 2
      ;;
    --repeat)
      REPEAT="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 2
      ;;
  esac
done

detect_cpus() {
  sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo 4
}

CPUS="$(detect_cpus)"

# Prefer cargo-nextest when installed and no prebuilt binary override is given
if [[ -z "$E2E_BIN" ]] && command -v cargo-nextest >/dev/null 2>&1; then
  NEXTEST_ARGS=(nextest run -p afterlife-daemon --test e2e_human_mcp --test-threads "$CPUS")
  if [[ -n "$SHARD" ]]; then
    NEXTEST_ARGS+=(--partition "hash:${SHARD}")
  fi
  for ((r = 1; r <= REPEAT; r++)); do
    echo "==> [nextest pass ${r}/${REPEAT}] threads=${CPUS} shard=${SHARD:-all}"
    cargo "${NEXTEST_ARGS[@]}"
  done
  exit 0
fi

# Resolve prebuilt binary once so multi-pass / sharded runs pay zero cargo lock overhead
if [[ -z "$E2E_BIN" ]]; then
  E2E_BIN="$(
    cargo test -p afterlife-daemon --test e2e_human_mcp --no-run --message-format=json \
      | awk -F'"executable":"' '/"target":\{"kind":\["test"\].*"name":"e2e_human_mcp"/ && /"executable":"/ { split($2, a, "\""); print a[1] }' \
      | tail -n 1
  )"
  if [[ -z "$E2E_BIN" || ! -x "$E2E_BIN" ]]; then
    # Fallback to standard cargo test if JSON extraction ever changes
    E2E_BIN=""
  fi
fi

list_tests() {
  if [[ -n "$E2E_BIN" ]]; then
    "$E2E_BIN" --list --format terse | awk -F': test$' '/: test$/ { print $1 }' | LC_ALL=C sort
  else
    cargo test -p afterlife-daemon --test e2e_human_mcp -- --list --format terse 2>/dev/null \
      | awk -F': test$' '/: test$/ { print $1 }' \
      | LC_ALL=C sort
  fi
}

exec_tests() {
  if [[ -n "$E2E_BIN" ]]; then
    "$E2E_BIN" "$@"
  else
    cargo test -p afterlife-daemon --test e2e_human_mcp -- "$@"
  fi
}

run_once() {
  local pass_label="$1"
  if [[ -z "$SHARD" ]]; then
    echo "==> [e2e ${pass_label}] threads=${CPUS} shard=all"
    exec_tests --test-threads="$CPUS"
    return
  fi

  local shard_idx="${SHARD%/*}"
  local shard_total="${SHARD#*/}"
  if ! [[ "$shard_idx" =~ ^[0-9]+$ && "$shard_total" =~ ^[0-9]+$ ]] || (( shard_idx < 1 || shard_idx > shard_total )); then
    echo "Invalid --shard '${SHARD}': expected 1-based INDEX/TOTAL (e.g. 1/4)" >&2
    exit 2
  fi

  local all_tests=()
  while IFS= read -r line; do
    [[ -n "$line" ]] && all_tests+=("$line")
  done < <(list_tests)

  local selected=()
  local i=0
  for t in "${all_tests[@]}"; do
    if (( (i % shard_total) == (shard_idx - 1) )); then
      selected+=("$t")
    fi
    i=$((i + 1))
  done

  echo "==> [e2e ${pass_label}] threads=${CPUS} shard=${shard_idx}/${shard_total} assigned=${#selected[@]}/${#all_tests[@]}"
  if (( ${#selected[@]} == 0 )); then
    echo "No tests assigned to shard ${shard_idx}/${shard_total}; skipping cleanly."
    return
  fi

  exec_tests --exact --test-threads="$CPUS" "${selected[@]}"
}

for ((r = 1; r <= REPEAT; r++)); do
  run_once "pass ${r}/${REPEAT}"
done

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

export PATH="$HOME/.cargo/bin:$PATH"

list_demos() {
    awk '
        /^\[\[bin\]\]/ { in_bin = 1; next }
        /^\[/ { in_bin = 0 }
        in_bin && /^name[[:space:]]*=/ {
            gsub(/^name[[:space:]]*=[[:space:]]*"/, "")
            gsub(/".*$/, "")
            print "  " $0
        }
    ' "$ROOT_DIR/tools/demos/Cargo.toml"
}

if [[ $# -lt 1 || "$1" == "-h" || "$1" == "--help" || "$1" == "--list" ]]; then
    echo "Usage: $(basename "$0") <demo-bin-name>"
    echo "Available demos:"
    list_demos
    exit 0
fi

BIN="$1"
shift

echo "==> Building demo: $BIN"
cargo build -p afterlife-demos --bin "$BIN"

BIN_PATH="$(
    cargo metadata --no-deps --format-version 1 \
        | python3 -c 'import json, os, sys; td = json.load(sys.stdin)["target_directory"]; print(os.path.join(td, "debug", sys.argv[1]))' "$BIN"
)"

if [[ ! -x "$BIN_PATH" ]]; then
    echo "error: built binary not found at $BIN_PATH" >&2
    exit 1
fi

pkill -f "/$BIN$" >/dev/null 2>&1 || true

TMP_BASE="${TMPDIR:-/tmp}"
LOG_FILE="${TMP_BASE%/}/afterlife-demo-${BIN}.log"

PID="$(
    python3 -c '
import os, subprocess, sys
log_path = sys.argv[1]
cmd = sys.argv[2:]
with open(log_path, "wb") as log_fh:
    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.DEVNULL,
        stdout=log_fh,
        stderr=subprocess.STDOUT,
        start_new_session=True,
        close_fds=True,
    )
print(proc.pid)
' "$LOG_FILE" "$BIN_PATH" "$@"
)"

sleep 0.5
if ! kill -0 "$PID" >/dev/null 2>&1; then
    echo "error: demo exited immediately (see $LOG_FILE)" >&2
    cat "$LOG_FILE" >&2 || true
    exit 1
fi

echo "==> Launched $BIN (pid $PID, log: $LOG_FILE)"

#!/usr/bin/env bash
# start.sh <openhuman-core> <work dir> <port>: start one instance from <work dir>/ws/config.toml.
# Writes <work dir>/pid and prints the time to a healthy /health.
set -euo pipefail
bin=$(realpath "$1"); w=$(realpath "$2"); p=$3
mkdir -p "$w/home" "$w/ws" "$w/action"; chmod 700 "$w/home" "$w/ws"
[ -f "$w/key" ] || (umask 077; od -An -tx1 -N32 /dev/urandom | tr -d ' \n' > "$w/key")
t0=$(date +%s%3N)
cd "$w/action"
env -i PATH=/usr/bin:/bin HOME="$w/home" LANG=C.UTF-8 OPENHUMAN_WORKSPACE="$w/ws" OPENHUMAN_ACTION_DIR="$w/action" \
  OPENHUMAN_TOOL_DISPATCHER=native OPENHUMAN_COMPOSIO_MODE=disabled OPENHUMAN_BACKEND_API_KEY=dummy \
  OPENHUMAN_KEYRING_BACKEND=encrypted_file OPENHUMAN_KEYRING_MASTER_KEY="$(cat "$w/key")" OPENHUMAN_CORE_TOKEN=core-token \
  RUST_LOG=warn,openhuman=debug,tinyagents=info \
  setsid "$bin" run --headless-api --host 127.0.0.1 --port "$p" >> "$w/core.log" 2>&1 &
pid=$!; echo $pid > "$w/pid"
for _ in $(seq 1 600); do
  curl -fsS -m 1 "http://127.0.0.1:$p/health" >/dev/null 2>&1 && break
  kill -0 $pid 2>/dev/null || { echo DIED; exit 1; }
  sleep 0.05
done
echo "pid=$pid health_ms=$(( $(date +%s%3N) - t0 ))"

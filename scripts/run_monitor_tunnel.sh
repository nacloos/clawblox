#!/usr/bin/env bash
set -euo pipefail

# One-command tunnel launcher for remote monitor + adapter.
# It avoids noisy "channel open failed" spam by checking remote ports first.

REMOTE_HOST="${REMOTE_HOST:-victoria.csail.mit.edu}"
REMOTE_USER="${REMOTE_USER:-nacloos}"
REMOTE="${REMOTE_USER}@${REMOTE_HOST}"

LOCAL_MONITOR_PORT="${LOCAL_MONITOR_PORT:-3310}"
REMOTE_MONITOR_PORT="${REMOTE_MONITOR_PORT:-4300}"

LOCAL_ADAPTER_PORT="${LOCAL_ADAPTER_PORT:-8081}"
REMOTE_ADAPTER_PORT="${REMOTE_ADAPTER_PORT:-8081}"

echo "Checking remote services on ${REMOTE}..."
echo "  monitor: 127.0.0.1:${REMOTE_MONITOR_PORT}"
echo "  adapter: 127.0.0.1:${REMOTE_ADAPTER_PORT}"

if ! ssh -o ExitOnForwardFailure=yes "$REMOTE" \
  "bash -lc '
    for p in ${REMOTE_MONITOR_PORT} ${REMOTE_ADAPTER_PORT}; do
      (echo >/dev/tcp/127.0.0.1/\$p) >/dev/null 2>&1 || { echo MISSING:\$p; exit 1; }
    done
    if command -v curl >/dev/null 2>&1; then
      curl -fsS http://127.0.0.1:${REMOTE_MONITOR_PORT}/ | grep -q \"Clawblox Monitor\" || {
        echo WRONG_MONITOR_APP:${REMOTE_MONITOR_PORT}
        exit 1
      }
    fi
    echo OK
  '" \
  >/tmp/monitor_tunnel_check.out 2>/tmp/monitor_tunnel_check.err; then
  echo "Remote port check failed."
  cat /tmp/monitor_tunnel_check.out 2>/dev/null || true
  cat /tmp/monitor_tunnel_check.err 2>/dev/null || true
  echo
  echo "Start the Clawblox monitor and adapter on remote first, then rerun."
  echo "Tip: default remote monitor port is 4300; override with REMOTE_MONITOR_PORT if needed."
  exit 1
fi

echo "Remote services are up. Starting tunnel..."
echo "  http://127.0.0.1:${LOCAL_MONITOR_PORT}  -> remote monitor ${REMOTE_MONITOR_PORT}"
echo "  http://127.0.0.1:${LOCAL_ADAPTER_PORT}  -> remote adapter ${REMOTE_ADAPTER_PORT}"
echo "Press Ctrl+C to close."

exec ssh -N \
  -o ExitOnForwardFailure=yes \
  -L "${LOCAL_MONITOR_PORT}:127.0.0.1:${REMOTE_MONITOR_PORT}" \
  -L "${LOCAL_ADAPTER_PORT}:127.0.0.1:${REMOTE_ADAPTER_PORT}" \
  "$REMOTE"

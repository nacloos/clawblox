#!/usr/bin/env bash
set -euo pipefail

# Starts (or restarts) Clawblox monitor in a remote tmux session.

REMOTE_HOST="${REMOTE_HOST:-victoria.csail.mit.edu}"
REMOTE_USER="${REMOTE_USER:-nacloos}"
REMOTE="${REMOTE_USER}@${REMOTE_HOST}"

REMOTE_ROOT="${REMOTE_ROOT:-/storage/nacloos/projects/clawblox}"
REMOTE_MONITOR_DIR="${REMOTE_MONITOR_DIR:-${REMOTE_ROOT}/worlds/minecraft/monitor}"
REMOTE_MONITOR_SESSION="${REMOTE_MONITOR_SESSION:-monitor-ui}"
REMOTE_MONITOR_PORT="${REMOTE_MONITOR_PORT:-4300}"

echo "Starting remote monitor..."
echo "  remote:  ${REMOTE}"
echo "  dir:     ${REMOTE_MONITOR_DIR}"
echo "  session: ${REMOTE_MONITOR_SESSION}"
echo "  port:    ${REMOTE_MONITOR_PORT}"

ssh "$REMOTE" "bash -lc '
  set -euo pipefail
  cd \"${REMOTE_MONITOR_DIR}\"
  tmux kill-session -t \"${REMOTE_MONITOR_SESSION}\" 2>/dev/null || true
  tmux new-session -d -s \"${REMOTE_MONITOR_SESSION}\" \"cd ${REMOTE_MONITOR_DIR} && MONITOR_PORT=${REMOTE_MONITOR_PORT} npm start\"
  sleep 1
  tmux capture-pane -pt \"${REMOTE_MONITOR_SESSION}:0.0\" -S -40 | tail -n 40
'"

echo
echo "Remote monitor started."
echo "Next:"
echo "  REMOTE_MONITOR_PORT=${REMOTE_MONITOR_PORT} ./scripts/run_monitor_tunnel.sh"

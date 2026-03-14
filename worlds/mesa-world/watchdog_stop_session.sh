#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 5 ]]; then
  echo "Usage: $(basename "$0") <duration_seconds> <tmux_session> <expected_session_created> <launch_script> <stop_grace_seconds>" >&2
  exit 1
fi

duration_seconds="$1"
tmux_session="$2"
expected_session_created="$3"
launch_script="$4"
stop_grace_seconds="$5"

sleep "$duration_seconds"

if ! tmux has-session -t "$tmux_session" 2>/dev/null; then
  exit 0
fi

current_session_created="$(tmux display-message -p -t "$tmux_session" '#{session_created}' 2>/dev/null || true)"
if [[ "$current_session_created" != "$expected_session_created" ]]; then
  exit 0
fi

bash "$launch_script" --tmux-session "$tmux_session" --stop --stop-grace-seconds "$stop_grace_seconds" >/dev/null 2>&1 || true

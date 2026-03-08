#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -f "$ROOT_DIR/.env" ]]; then
  set -a
  source "$ROOT_DIR/.env"
  set +a
fi

NUM_WORLDS="${NUM_WORLDS:-5}"
AGENTS_PER_WORLD="${AGENTS_PER_WORLD:-1}"
BASE_PORT="${BASE_PORT:-8080}"
TMUX_SESSION="${TMUX_SESSION:-clawblox}"
RECORD="${RECORD:-true}"
WORLD_DIR="${WORLD_DIR:-worlds/mesa-world}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-30}"

if [[ -f "$ROOT_DIR/target/release/clawblox" ]]; then
  CLAWBLOX_BIN="${CLAWBLOX_BIN:-$ROOT_DIR/target/release/clawblox}"
elif command -v clawblox >/dev/null 2>&1; then
  CLAWBLOX_BIN="${CLAWBLOX_BIN:-clawblox}"
else
  echo "Error: clawblox binary not found in target/release/ or PATH"
  exit 1
fi

AGENT_SCRIPT="$ROOT_DIR/agent/run_agent.sh"
if [[ ! -f "$AGENT_SCRIPT" ]]; then
  echo "Error: agent/run_agent.sh not found at $AGENT_SCRIPT"
  exit 1
fi

usage() {
  cat <<EOF
Usage: $(basename "$0") [--stop] [--force] [--status]

Launches $NUM_WORLDS mesa world servers and $AGENTS_PER_WORLD agents per world in tmux.

Options:
  --stop     Kill the tmux session and all processes
  --force    Skip confirmation when replacing an existing session
  --status   Show running session info and exit

Environment variables:
  NUM_WORLDS        Number of world servers (default: 5)
  AGENTS_PER_WORLD  Agents per world (default: 5)
  BASE_PORT         First server port (default: 8080)
  TMUX_SESSION      Tmux session name (default: clawblox)
  RECORD            Enable recording (default: true)
  WORLD_DIR         World directory relative to project root (default: worlds/mesa-world)
  HEALTH_TIMEOUT    Seconds to wait for each server to become healthy (default: 30)
  CLAWBLOX_BIN      Path to clawblox binary (auto-detected)
EOF
}

do_stop() {
  if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    echo "Killing tmux session '$TMUX_SESSION'..."
    tmux kill-session -t "$TMUX_SESSION"
    echo "Stopped."
  else
    echo "No tmux session '$TMUX_SESSION' found."
  fi
}

do_status() {
  if ! tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    echo "No tmux session '$TMUX_SESSION' running."
    exit 1
  fi
  echo "Session: $TMUX_SESSION"
  echo "Windows:"
  tmux list-windows -t "$TMUX_SESSION" -F "  #{window_index}: #{window_name} (#{window_panes} panes)"
  echo
  echo "Attach with: tmux attach -t $TMUX_SESSION"
}

FORCE=0
for arg in "$@"; do
  case "$arg" in
    --stop)   do_stop; exit 0 ;;
    --status) do_status; exit 0 ;;
    --force)  FORCE=1 ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown option: $arg"; usage; exit 1 ;;
  esac
done

if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
  if [[ "$FORCE" -eq 1 ]]; then
    echo "Killing existing session '$TMUX_SESSION'..."
    tmux kill-session -t "$TMUX_SESSION"
  else
    echo "Tmux session '$TMUX_SESSION' already exists."
    echo "Use --force to replace it, or --stop to kill it."
    exit 1
  fi
fi

wait_for_server() {
  local port="$1"
  local deadline=$((SECONDS + HEALTH_TIMEOUT))
  while ((SECONDS < deadline)); do
    if curl -sf "http://localhost:${port}/skill.md" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

echo "Starting $NUM_WORLDS world servers (ports $BASE_PORT-$((BASE_PORT + NUM_WORLDS - 1)))..."

RECORD_FLAG=""
if [[ "$RECORD" == "true" ]]; then
  RECORD_FLAG="--record"
fi

tmux new-session -d -s "$TMUX_SESSION" -n "world-0"
tmux set-option -t "$TMUX_SESSION" remain-on-exit on

tmux send-keys -t "${TMUX_SESSION}:world-0" \
  "cd '$ROOT_DIR' && exec '$CLAWBLOX_BIN' run '$WORLD_DIR' --port $BASE_PORT $RECORD_FLAG" Enter

for ((i = 1; i < NUM_WORLDS; i++)); do
  port=$((BASE_PORT + i))
  tmux new-window -t "$TMUX_SESSION" -n "world-$i"
  tmux send-keys -t "${TMUX_SESSION}:world-$i" \
    "cd '$ROOT_DIR' && exec '$CLAWBLOX_BIN' run '$WORLD_DIR' --port $port $RECORD_FLAG" Enter
done

echo "Waiting for servers to become healthy..."
ALL_HEALTHY=1
for ((i = 0; i < NUM_WORLDS; i++)); do
  port=$((BASE_PORT + i))
  printf "  world-%d (port %d)... " "$i" "$port"
  if wait_for_server "$port"; then
    echo "ready"
  else
    echo "TIMEOUT (${HEALTH_TIMEOUT}s)"
    ALL_HEALTHY=0
  fi
done

if [[ "$ALL_HEALTHY" -eq 0 ]]; then
  echo
  echo "Warning: some servers did not become healthy in time."
  echo "Agents will still be launched -- they may fail to join initially."
  echo
fi

echo "Launching $AGENTS_PER_WORLD agents per world..."

for ((i = 0; i < NUM_WORLDS; i++)); do
  port=$((BASE_PORT + i))
  url="http://localhost:${port}"

  tmux new-window -t "$TMUX_SESSION" -n "agents-$i"
  tmux send-keys -t "${TMUX_SESSION}:agents-$i" \
    "cd '$ROOT_DIR' && WORLD_BASE_URL='$url' exec bash '$AGENT_SCRIPT'" Enter

  for ((j = 1; j < AGENTS_PER_WORLD; j++)); do
    tmux split-window -t "${TMUX_SESSION}:agents-$i"
    tmux send-keys -t "${TMUX_SESSION}:agents-$i" \
      "cd '$ROOT_DIR' && WORLD_BASE_URL='$url' exec bash '$AGENT_SCRIPT'" Enter
    tmux select-layout -t "${TMUX_SESSION}:agents-$i" tiled
  done
done

TOTAL_AGENTS=$((NUM_WORLDS * AGENTS_PER_WORLD))
echo
echo "========================================"
echo "  Clawblox Multi-Instance Launcher"
echo "========================================"
echo "  Worlds:  $NUM_WORLDS (ports $BASE_PORT-$((BASE_PORT + NUM_WORLDS - 1)))"
echo "  Agents:  $TOTAL_AGENTS ($AGENTS_PER_WORLD per world)"
echo "  Record:  $RECORD"
echo "  Session: $TMUX_SESSION"
echo "========================================"
echo
echo "  tmux attach -t $TMUX_SESSION"
echo
echo "  Stop all:  $(basename "$0") --stop"
echo "  Status:    $(basename "$0") --status"
echo

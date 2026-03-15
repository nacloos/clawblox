#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ -f "$ROOT_DIR/.env" ]]; then
  set -a
  source "$ROOT_DIR/.env"
  set +a
fi

TMUX_SESSION="${TMUX_SESSION:-clawblox-replay}"
BASE_PORT="${BASE_PORT:-10081}"
WORLD_DIR="${WORLD_DIR:-worlds/mesa-world}"
REPLAY_SPEED="${REPLAY_SPEED:-1.0}"
STOP_GRACE_SECONDS="${STOP_GRACE_SECONDS:-2}"
INCLUDE_AGENT_LOGS="${INCLUDE_AGENT_LOGS:-true}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-20}"

if [[ -f "$ROOT_DIR/target/release/clawblox" ]]; then
  CLAWBLOX_BIN="${CLAWBLOX_BIN:-$ROOT_DIR/target/release/clawblox}"
elif command -v clawblox >/dev/null 2>&1; then
  CLAWBLOX_BIN="${CLAWBLOX_BIN:-clawblox}"
else
  echo "Error: clawblox binary not found in target/release/ or PATH"
  exit 1
fi

usage() {
  cat <<EOF
Usage: $(basename "$0") [RESULTS_DIR_OR_RUN_ID] [--force] [--stop] [--status]

Launches replay servers (one per world recording) in tmux for a results folder.

Examples:
  $(basename "$0") results/20260309T195534Z
  $(basename "$0") 20260309T195534Z
  $(basename "$0") --status
  $(basename "$0") --stop

Environment variables:
  TMUX_SESSION        Tmux session name (default: clawblox-replay)
  BASE_PORT           First replay port (default: 10081)
  WORLD_DIR           World directory relative to project root (default: worlds/mesa-world)
  REPLAY_SPEED        Replay speed multiplier (default: 1.0)
  INCLUDE_AGENT_LOGS  Auto-pass --agent from agents/*/session/*.jsonl (default: true)
  STOP_GRACE_SECONDS  Seconds to wait after Ctrl+C before killing tmux (default: 2)
  HEALTH_TIMEOUT      Seconds to wait per replay server (default: 20)
  CLAWBLOX_BIN        Path to clawblox binary (auto-detected)
EOF
}

resolve_world_abs_dir() {
  if [[ "$WORLD_DIR" = /* ]]; then
    printf '%s\n' "$WORLD_DIR"
  else
    printf '%s/%s\n' "$ROOT_DIR" "$WORLD_DIR"
  fi
}

resolve_results_abs_dir() {
  local world_abs_dir="$1"
  local user_target="${2:-}"
  local candidate=""

  if [[ -n "$user_target" ]]; then
    if [[ "$user_target" = /* ]]; then
      candidate="$user_target"
    elif [[ -d "$user_target" ]]; then
      candidate="$(cd "$user_target" && pwd)"
    elif [[ -d "$world_abs_dir/results/$user_target" ]]; then
      candidate="$world_abs_dir/results/$user_target"
    else
      echo "Error: results folder not found: $user_target"
      exit 1
    fi
  else
    if [[ ! -f "$world_abs_dir/results/latest_run.txt" ]]; then
      echo "Error: no results target provided and latest_run.txt is missing."
      exit 1
    fi
    candidate="$(<"$world_abs_dir/results/latest_run.txt")"
  fi

  if [[ ! -d "$candidate" ]]; then
    echo "Error: results folder does not exist: $candidate"
    exit 1
  fi

  printf '%s\n' "$(cd "$candidate" && pwd)"
}

do_stop() {
  local pane_target
  if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    echo "Sending Ctrl+C to panes in tmux session '$TMUX_SESSION'..."
    while IFS= read -r pane_target; do
      tmux send-keys -t "$pane_target" C-c
    done < <(tmux list-panes -a -t "$TMUX_SESSION" -F "#{session_name}:#{window_name}.#{pane_index}")
    echo "Waiting ${STOP_GRACE_SECONDS}s for replay shutdown..."
    sleep "$STOP_GRACE_SECONDS"
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

port_is_in_use() {
  local port="$1"
  (echo >/dev/tcp/127.0.0.1/"$port") >/dev/null 2>&1
}

wait_for_server() {
  local port="$1"
  local deadline=$((SECONDS + HEALTH_TIMEOUT))
  while ((SECONDS < deadline)); do
    if curl -sf "http://localhost:${port}/" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

finalize_recording_if_needed() {
  local recording_file="$1"
  local wal_file="${recording_file}-wal"
  local max_tick
  local max_ms
  local current_total_ticks
  local current_duration_ms

  if [[ ! -f "$wal_file" ]]; then
    :
  elif command -v sqlite3 >/dev/null 2>&1; then
    sqlite3 "$recording_file" "PRAGMA wal_checkpoint(TRUNCATE);" >/dev/null 2>&1 || true
  fi

  if ! command -v sqlite3 >/dev/null 2>&1; then
    return 0
  fi

  max_tick="$(sqlite3 "$recording_file" "SELECT COALESCE(MAX(tick),0) FROM (SELECT tick FROM checkpoints UNION ALL SELECT tick FROM deltas);")"
  max_ms="$(sqlite3 "$recording_file" "SELECT COALESCE(MAX(time_ms),0) FROM (SELECT time_ms FROM checkpoints UNION ALL SELECT time_ms FROM deltas);")"
  current_total_ticks="$(sqlite3 "$recording_file" "SELECT value FROM meta WHERE key='total_ticks' LIMIT 1;")"
  current_duration_ms="$(sqlite3 "$recording_file" "SELECT value FROM meta WHERE key='duration_ms' LIMIT 1;")"

  if [[ -z "$current_total_ticks" || "$current_total_ticks" -eq 0 ]]; then
    sqlite3 "$recording_file" "INSERT OR REPLACE INTO meta(key,value) VALUES('total_ticks','$max_tick');" >/dev/null 2>&1 || true
  fi
  if [[ -z "$current_duration_ms" || "$current_duration_ms" -eq 0 ]]; then
    sqlite3 "$recording_file" "INSERT OR REPLACE INTO meta(key,value) VALUES('duration_ms','$max_ms');" >/dev/null 2>&1 || true
  fi
}

FORCE=0
ACTION="start"
RESULTS_TARGET=""

while (($# > 0)); do
  case "$1" in
    --stop)   ACTION="stop" ;;
    --status) ACTION="status" ;;
    --force)  FORCE=1 ;;
    --help|-h) usage; exit 0 ;;
    --*)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
    *)
      if [[ -z "$RESULTS_TARGET" ]]; then
        RESULTS_TARGET="$1"
      else
        echo "Error: unexpected extra argument '$1'."
        usage
        exit 1
      fi
      ;;
  esac
  shift
done

case "$ACTION" in
  stop) do_stop; exit 0 ;;
  status) do_status; exit 0 ;;
esac

WORLD_ABS_DIR="$(resolve_world_abs_dir)"
if [[ ! -d "$WORLD_ABS_DIR" ]]; then
  echo "Error: world directory not found at $WORLD_ABS_DIR"
  exit 1
fi

RESULTS_ABS_DIR="$(resolve_results_abs_dir "$WORLD_ABS_DIR" "$RESULTS_TARGET")"
WORLDS_ROOT="$RESULTS_ABS_DIR/worlds"
if [[ ! -d "$WORLDS_ROOT" ]]; then
  echo "Error: expected worlds directory at $WORLDS_ROOT"
  exit 1
fi

if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
  if [[ "$FORCE" -eq 1 ]]; then
    echo "Replacing existing session '$TMUX_SESSION'..."
    do_stop
  else
    echo "Tmux session '$TMUX_SESSION' already exists."
    echo "Use --force to replace it, or --stop to kill it."
    exit 1
  fi
fi

mapfile -t world_dirs < <(find "$WORLDS_ROOT" -mindepth 1 -maxdepth 1 -type d -name 'world-*' | sort -V)
if ((${#world_dirs[@]} == 0)); then
  echo "Error: no world-* directories found in $WORLDS_ROOT"
  exit 1
fi

task_count=0
declare -a TASK_WORLD_DIRS=()
declare -a TASK_WORLD_NAMES=()
declare -a TASK_RECORDINGS=()
declare -a TASK_AGENTS=()
declare -a TASK_PORTS=()

for world_dir in "${world_dirs[@]}"; do
  world_name="$(basename "$world_dir")"
  recording_file="$(find "$world_dir/recordings" -maxdepth 1 -type f -name '*.clawrec' 2>/dev/null | sort | tail -n 1 || true)"
  if [[ -z "$recording_file" ]]; then
    echo "Warning: skipping $world_name (no .clawrec in $world_dir/recordings)"
    continue
  fi

  finalize_recording_if_needed "$recording_file"

  agent_file=""
  if [[ "$INCLUDE_AGENT_LOGS" == "true" ]]; then
    agent_file="$(find "$world_dir/agents" -type f -path '*/session/*.jsonl' 2>/dev/null | sort | tail -n 1 || true)"
  fi

  TASK_WORLD_DIRS+=("$world_dir")
  TASK_WORLD_NAMES+=("$world_name")
  TASK_RECORDINGS+=("$recording_file")
  TASK_AGENTS+=("$agent_file")
  TASK_PORTS+=("$((BASE_PORT + task_count))")
  task_count=$((task_count + 1))
done

if ((task_count == 0)); then
  echo "Error: found worlds but no replayable .clawrec files."
  exit 1
fi

busy=0
for port in "${TASK_PORTS[@]}"; do
  if port_is_in_use "$port"; then
    echo "Error: target port $port is already in use."
    busy=1
  fi
done
if [[ "$busy" -eq 1 ]]; then
  echo "Resolve busy ports, then rerun. If these are from a prior replay launch, run: $(basename "$0") --stop"
  exit 1
fi

echo "Starting $task_count replay server(s) from $RESULTS_ABS_DIR ..."

for ((i = 0; i < task_count; i++)); do
  world_name="${TASK_WORLD_NAMES[$i]}"
  recording_file="${TASK_RECORDINGS[$i]}"
  agent_file="${TASK_AGENTS[$i]}"
  port="${TASK_PORTS[$i]}"
  world_dir="${TASK_WORLD_DIRS[$i]}"
  replay_log="$world_dir/logs/replay.log"
  mkdir -p "$world_dir/logs"

  agent_arg=""
  if [[ -n "$agent_file" ]]; then
    agent_arg=" --agent '$agent_file'"
  fi

  if ((i == 0)); then
    tmux new-session -d -s "$TMUX_SESSION" -n "$world_name"
    tmux set-option -t "$TMUX_SESSION" remain-on-exit on
  else
    tmux new-window -t "$TMUX_SESSION" -n "$world_name"
  fi

  tmux send-keys -t "${TMUX_SESSION}:${world_name}" \
    "cd '$ROOT_DIR' && '$CLAWBLOX_BIN' replay '$recording_file' --port $port --speed $REPLAY_SPEED --world-dir '$WORLD_ABS_DIR'${agent_arg} 2>&1 | tee -a '$replay_log'" Enter
done

echo "Waiting for replay servers to become healthy..."
for ((i = 0; i < task_count; i++)); do
  world_name="${TASK_WORLD_NAMES[$i]}"
  port="${TASK_PORTS[$i]}"
  printf "  %s (port %s)... " "$world_name" "$port"
  if wait_for_server "$port"; then
    echo "ready"
  else
    echo "TIMEOUT (${HEALTH_TIMEOUT}s)"
  fi
done

echo
echo "========================================"
echo "  Clawblox Replay Launcher"
echo "========================================"
echo "  Results: $RESULTS_ABS_DIR"
echo "  Session: $TMUX_SESSION"
echo "  Speed:   $REPLAY_SPEED"
echo "========================================"
for ((i = 0; i < task_count; i++)); do
  world_name="${TASK_WORLD_NAMES[$i]}"
  port="${TASK_PORTS[$i]}"
  agent_file="${TASK_AGENTS[$i]}"
  if [[ -n "$agent_file" ]]; then
    printf "  %-12s http://localhost:%s  (agent: %s)\n" "$world_name" "$port" "$(basename "$agent_file")"
  else
    printf "  %-12s http://localhost:%s\n" "$world_name" "$port"
  fi
done
echo "========================================"
echo
echo "  tmux attach -t $TMUX_SESSION"
echo
echo "  Stop all:  $(basename "$0") --stop"
echo "  Status:    $(basename "$0") --status"
echo

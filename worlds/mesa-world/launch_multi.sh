#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

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
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
AGENT_NAME_PREFIX="${AGENT_NAME_PREFIX:-mesa}"
STOP_GRACE_SECONDS="${STOP_GRACE_SECONDS:-5}"
GOAL="${GOAL:-}"
TEMPLATE_DIR="${TEMPLATE_DIR:-}"
RESULTS_ROOT="${RESULTS_ROOT:-}"
PORT_WAIT_SECONDS="${PORT_WAIT_SECONDS:-120}"
AGENT_CHECKPOINT_AT_EPOCH="${AGENT_CHECKPOINT_AT_EPOCH:-}"
AGENT_CHECKPOINT_PROMPT="${AGENT_CHECKPOINT_PROMPT:-}"
AGENT_CHECKPOINT_TRIGGER_TURN="${AGENT_CHECKPOINT_TRIGGER_TURN:-1}"
AGENT_CHECKPOINT_DISABLE_AUTO_LOOP="${AGENT_CHECKPOINT_DISABLE_AUTO_LOOP:-0}"

AGENT_SCRIPT="$ROOT_DIR/agent/run_agent.sh"
if [[ ! -f "$AGENT_SCRIPT" ]]; then
  echo "Error: agent/run_agent.sh not found at $AGENT_SCRIPT"
  exit 1
fi

sanitize_name() {
  printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_'
}

usage() {
  cat <<EOF
Usage: $(basename "$0") [--stop|--status] [options]

Launches $NUM_WORLDS mesa world servers and $AGENTS_PER_WORLD agents per world in tmux.

Options:
  --stop                       Gracefully stop worlds/agents, then kill the tmux session
  --status                     Show running session info and exit
  --force                      Skip confirmation when replacing an existing session
  --num-worlds N               Number of world servers
  --agents-per-world N         Agents per world
  --base-port PORT             First server port
  --tmux-session NAME          Tmux session name
  --record true|false          Enable/disable recording
  --world-dir PATH             World directory (relative to project root or absolute)
  --health-timeout SECONDS     Server health-check timeout
  --run-id ID                  Run identifier under results/
  --agent-name-prefix PREFIX   Agent name prefix
  --stop-grace-seconds N       Wait after Ctrl+C before force-killing tmux
  --port-wait-seconds N        Wait up to N seconds for target ports to free before failing
  --goal TEXT                  Custom startup objective passed to agents
  --template PATH              Agent template directory (relative to project root or absolute)
  --results-root PATH          Results root directory (default: <world-dir>/results)
  --clawblox-bin PATH          Path or command name for clawblox binary

Environment variables:
  NUM_WORLDS        Number of world servers (default: 5)
  AGENTS_PER_WORLD  Agents per world (default: 1)
  BASE_PORT         First server port (default: 8080)
  TMUX_SESSION      Tmux session name (default: clawblox)
  RECORD            Enable recording (default: true)
  WORLD_DIR         World directory relative to project root (default: worlds/mesa-world)
  HEALTH_TIMEOUT    Seconds to wait for each server to become healthy (default: 30)
  RUN_ID            Run identifier under results/ (default: UTC timestamp)
  AGENT_NAME_PREFIX Agent name prefix (default: mesa)
  STOP_GRACE_SECONDS Seconds to wait after Ctrl+C before force-closing tmux (default: 5)
  PORT_WAIT_SECONDS Seconds to wait for target ports to free before failing (default: 120)
  GOAL              Custom startup objective passed to agents
  TEMPLATE_DIR      Agent template directory (run_agent.sh TEMPLATE_DIR override)
  RESULTS_ROOT      Results root directory (default: <world-dir>/results)
  AGENT_CHECKPOINT_AT_EPOCH       Unix epoch seconds when checkpoint prompt should be sent
  AGENT_CHECKPOINT_PROMPT         Checkpoint message sent from clawblox-agent extension
  AGENT_CHECKPOINT_TRIGGER_TURN   1/0 trigger a turn when checkpoint prompt is sent (default: 1)
  AGENT_CHECKPOINT_DISABLE_AUTO_LOOP 1/0 disable continue loop when checkpoint fires (default: 0)
  CLAWBLOX_BIN      Path to clawblox binary (auto-detected)
EOF
}

require_value() {
  local opt="$1"
  local maybe_value="${2:-}"
  if [[ -z "$maybe_value" ]]; then
    echo "Error: $opt requires a value."
    usage
    exit 1
  fi
}

is_nonnegative_int() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

is_positive_int() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

resolve_clawblox_bin() {
  if [[ -n "${CLAWBLOX_BIN:-}" ]]; then
    if command -v "$CLAWBLOX_BIN" >/dev/null 2>&1 || [[ -x "$CLAWBLOX_BIN" ]]; then
      return 0
    fi
    echo "Error: CLAWBLOX_BIN '$CLAWBLOX_BIN' was not found."
    exit 1
  fi

  if [[ -f "$ROOT_DIR/target/release/clawblox" ]]; then
    CLAWBLOX_BIN="$ROOT_DIR/target/release/clawblox"
  elif command -v clawblox >/dev/null 2>&1; then
    CLAWBLOX_BIN="clawblox"
  else
    echo "Error: clawblox binary not found in target/release/ or PATH"
    exit 1
  fi
}

collect_session_recordings() {
  local window_name
  tmux list-windows -t "$TMUX_SESSION" -F "#{window_name}" 2>/dev/null | while IFS= read -r window_name; do
    [[ "$window_name" == world-* ]] || continue
    tmux capture-pane -p -S -2000 -t "${TMUX_SESSION}:${window_name}" 2>/dev/null \
      | sed -n 's/^Recording to: //p' \
      | tail -n 1
  done | awk 'NF && !seen[$0]++'
}

finalize_recording_db() {
  local recording_file="$1"
  local wal_file="${recording_file}-wal"
  local shm_file="${recording_file}-shm"

  if [[ ! -f "$recording_file" ]]; then
    return 0
  fi

  if ! command -v sqlite3 >/dev/null 2>&1; then
    echo "Warning: sqlite3 not found; cannot checkpoint '$recording_file'."
    return 0
  fi

  if sqlite3 "$recording_file" "PRAGMA wal_checkpoint(TRUNCATE);" >/dev/null 2>&1; then
    if [[ -f "$wal_file" && ! -s "$wal_file" ]]; then
      rm -f "$wal_file"
    fi
    if [[ -f "$shm_file" && ! -f "$wal_file" ]]; then
      rm -f "$shm_file"
    fi
  else
    echo "Warning: failed to checkpoint recording '$recording_file'."
  fi
}

do_stop() {
  local recording_file
  local pane_target
  local recordings=()

  if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    mapfile -t recordings < <(collect_session_recordings)

    echo "Sending Ctrl+C to panes in tmux session '$TMUX_SESSION'..."
    while IFS= read -r pane_target; do
      tmux send-keys -t "$pane_target" C-c
    done < <(tmux list-panes -t "$TMUX_SESSION" -F "#{session_name}:#{window_name}.#{pane_index}")

    echo "Waiting ${STOP_GRACE_SECONDS}s for graceful shutdown..."
    sleep "$STOP_GRACE_SECONDS"

    if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
      echo "Killing tmux session '$TMUX_SESSION'..."
      tmux kill-session -t "$TMUX_SESSION" 2>/dev/null || true
    else
      echo "Tmux session '$TMUX_SESSION' already exited."
    fi

    if ((${#recordings[@]} > 0)); then
      echo "Finalizing recording databases..."
      for recording_file in "${recordings[@]}"; do
        finalize_recording_db "$recording_file"
      done
    fi

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
ACTION="run"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --stop)
      ACTION="stop"
      shift
      ;;
    --status)
      ACTION="status"
      shift
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --num-worlds)
      require_value "$1" "${2:-}"
      NUM_WORLDS="$2"
      shift 2
      ;;
    --agents-per-world)
      require_value "$1" "${2:-}"
      AGENTS_PER_WORLD="$2"
      shift 2
      ;;
    --base-port)
      require_value "$1" "${2:-}"
      BASE_PORT="$2"
      shift 2
      ;;
    --tmux-session)
      require_value "$1" "${2:-}"
      TMUX_SESSION="$2"
      shift 2
      ;;
    --record)
      require_value "$1" "${2:-}"
      RECORD="$2"
      shift 2
      ;;
    --world-dir)
      require_value "$1" "${2:-}"
      WORLD_DIR="$2"
      shift 2
      ;;
    --health-timeout)
      require_value "$1" "${2:-}"
      HEALTH_TIMEOUT="$2"
      shift 2
      ;;
    --run-id)
      require_value "$1" "${2:-}"
      RUN_ID="$2"
      shift 2
      ;;
    --agent-name-prefix)
      require_value "$1" "${2:-}"
      AGENT_NAME_PREFIX="$2"
      shift 2
      ;;
    --stop-grace-seconds)
      require_value "$1" "${2:-}"
      STOP_GRACE_SECONDS="$2"
      shift 2
      ;;
    --port-wait-seconds)
      require_value "$1" "${2:-}"
      PORT_WAIT_SECONDS="$2"
      shift 2
      ;;
    --clawblox-bin)
      require_value "$1" "${2:-}"
      CLAWBLOX_BIN="$2"
      shift 2
      ;;
    --goal)
      require_value "$1" "${2:-}"
      GOAL="$2"
      shift 2
      ;;
    --template|--template-dir)
      require_value "$1" "${2:-}"
      TEMPLATE_DIR="$2"
      shift 2
      ;;
    --results-root)
      require_value "$1" "${2:-}"
      RESULTS_ROOT="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
done

if ! is_nonnegative_int "$STOP_GRACE_SECONDS"; then
  echo "Error: STOP_GRACE_SECONDS must be a non-negative integer (got '$STOP_GRACE_SECONDS')."
  exit 1
fi
if ! is_nonnegative_int "$PORT_WAIT_SECONDS"; then
  echo "Error: PORT_WAIT_SECONDS must be a non-negative integer (got '$PORT_WAIT_SECONDS')."
  exit 1
fi

case "$ACTION" in
  stop)
    do_stop
    exit 0
    ;;
  status)
    do_status
    exit 0
    ;;
esac

if ! is_positive_int "$NUM_WORLDS"; then
  echo "Error: NUM_WORLDS must be a positive integer (got '$NUM_WORLDS')."
  exit 1
fi
if ! is_positive_int "$AGENTS_PER_WORLD"; then
  echo "Error: AGENTS_PER_WORLD must be a positive integer (got '$AGENTS_PER_WORLD')."
  exit 1
fi
if ! is_positive_int "$BASE_PORT" || ((BASE_PORT > 65535)); then
  echo "Error: BASE_PORT must be an integer in [1, 65535] (got '$BASE_PORT')."
  exit 1
fi
if ! is_positive_int "$HEALTH_TIMEOUT"; then
  echo "Error: HEALTH_TIMEOUT must be a positive integer (got '$HEALTH_TIMEOUT')."
  exit 1
fi
if [[ "$RECORD" != "true" && "$RECORD" != "false" ]]; then
  echo "Error: RECORD must be 'true' or 'false' (got '$RECORD')."
  exit 1
fi

resolve_clawblox_bin

CLAWBLOX_SUPPORTS_RECORD_DIR=0
if "$CLAWBLOX_BIN" run --help 2>&1 | grep -q -- '--record-dir'; then
  CLAWBLOX_SUPPORTS_RECORD_DIR=1
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

if [[ "$WORLD_DIR" = /* ]]; then
  WORLD_ABS_DIR="$WORLD_DIR"
else
  WORLD_ABS_DIR="$ROOT_DIR/$WORLD_DIR"
fi

if [[ ! -d "$WORLD_ABS_DIR" ]]; then
  echo "Error: world directory not found at $WORLD_ABS_DIR"
  exit 1
fi

TEMPLATE_ABS_DIR=""
if [[ -n "$TEMPLATE_DIR" ]]; then
  if [[ "$TEMPLATE_DIR" = /* ]]; then
    TEMPLATE_ABS_DIR="$TEMPLATE_DIR"
  else
    TEMPLATE_ABS_DIR="$ROOT_DIR/$TEMPLATE_DIR"
  fi

  if [[ ! -d "$TEMPLATE_ABS_DIR" ]]; then
    echo "Error: template directory not found at $TEMPLATE_ABS_DIR"
    exit 1
  fi
fi

RUN_SAFE_ID="$(sanitize_name "$RUN_ID")"
if [[ -n "$RESULTS_ROOT" ]]; then
  if [[ "$RESULTS_ROOT" = /* ]]; then
    RESULTS_ABS_ROOT="$RESULTS_ROOT"
  else
    RESULTS_ABS_ROOT="$ROOT_DIR/$RESULTS_ROOT"
  fi
else
  RESULTS_ABS_ROOT="$WORLD_ABS_DIR/results"
fi

if [[ "$RECORD" == "true" && "$CLAWBLOX_SUPPORTS_RECORD_DIR" -ne 1 ]]; then
  echo "Error: '$CLAWBLOX_BIN run' does not support --record-dir."
  echo "Rebuild/update clawblox, or set RECORD=false."
  exit 1
fi

if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
  echo "Error: ANTHROPIC_API_KEY is not set."
  echo "Add it to $ROOT_DIR/.env (or export it) before launching agents."
  exit 1
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

port_is_in_use() {
  local port="$1"
  (echo >/dev/tcp/127.0.0.1/"$port") >/dev/null 2>&1
}

ensure_ports_available() {
  local i
  local port
  local waited=0
  local deadline=0
  local busy_ports=()
  local pids=()

  while true; do
    busy_ports=()
    for ((i = 0; i < NUM_WORLDS; i++)); do
      port=$((BASE_PORT + i))
      if port_is_in_use "$port"; then
        busy_ports+=("$port")
      fi
    done

    if ((${#busy_ports[@]} == 0)); then
      return 0
    fi

    if ((PORT_WAIT_SECONDS == 0)); then
      break
    fi

    if ((waited == 0)); then
      deadline=$((SECONDS + PORT_WAIT_SECONDS))
      echo "Target ports are busy (${busy_ports[*]}). Waiting up to ${PORT_WAIT_SECONDS}s for release..."
    fi
    if ((SECONDS >= deadline)); then
      break
    fi

    waited=1
    sleep 1
  done

  for port in "${busy_ports[@]}"; do
    echo "Error: target port $port is already in use."
    if command -v lsof >/dev/null 2>&1; then
      mapfile -t pids < <(lsof -nP -t -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | sort -u)
      if ((${#pids[@]} > 0)); then
        echo "  listening pids: ${pids[*]}"
      fi
    fi
  done
  echo "Resolve busy ports, then rerun. If these are from a prior launch, run: $(basename "$0") --stop"
  exit 1
}

ensure_ports_available

RUN_DIR="$RESULTS_ABS_ROOT/$RUN_SAFE_ID"
WORLDS_ROOT="$RUN_DIR/worlds"

if [[ -e "$RUN_DIR" ]]; then
  echo "Error: run directory already exists: $RUN_DIR"
  echo "Set RUN_ID to a unique value and retry."
  exit 1
fi

mkdir -p "$WORLDS_ROOT"
printf '%s\n' "$RUN_DIR" >"$RESULTS_ABS_ROOT/latest_run.txt"

echo "Starting $NUM_WORLDS world servers (ports $BASE_PORT-$((BASE_PORT + NUM_WORLDS - 1)))..."

tmux new-session -d -s "$TMUX_SESSION" -n "world-0"
tmux set-option -t "$TMUX_SESSION" remain-on-exit on

WORLD0_ROOT="$WORLDS_ROOT/world-0"
WORLD0_LOG_DIR="$WORLD0_ROOT/logs"
WORLD0_LOG="$WORLD0_LOG_DIR/world.log"
WORLD0_RECORD_DIR="$WORLD0_ROOT/recordings"
WORLD0_AGENTS_DIR="$WORLD0_ROOT/agents"
mkdir -p "$WORLD0_LOG_DIR" "$WORLD0_RECORD_DIR" "$WORLD0_AGENTS_DIR"
if [[ "$RECORD" == "true" ]]; then
  tmux send-keys -t "${TMUX_SESSION}:world-0" \
    "cd '$ROOT_DIR' && '$CLAWBLOX_BIN' run '$WORLD_ABS_DIR' --port $BASE_PORT --record --record-dir '$WORLD0_RECORD_DIR' 2>&1 | tee -a '$WORLD0_LOG'" Enter
else
  tmux send-keys -t "${TMUX_SESSION}:world-0" \
    "cd '$ROOT_DIR' && '$CLAWBLOX_BIN' run '$WORLD_ABS_DIR' --port $BASE_PORT 2>&1 | tee -a '$WORLD0_LOG'" Enter
fi

for ((i = 1; i < NUM_WORLDS; i++)); do
  port=$((BASE_PORT + i))
  world_root="$WORLDS_ROOT/world-$i"
  world_log_dir="$world_root/logs"
  world_log="$world_log_dir/world.log"
  world_record_dir="$world_root/recordings"
  world_agents_dir="$world_root/agents"
  mkdir -p "$world_log_dir" "$world_record_dir" "$world_agents_dir"
  tmux new-window -t "$TMUX_SESSION" -n "world-$i"
  if [[ "$RECORD" == "true" ]]; then
    tmux send-keys -t "${TMUX_SESSION}:world-$i" \
      "cd '$ROOT_DIR' && '$CLAWBLOX_BIN' run '$WORLD_ABS_DIR' --port $port --record --record-dir '$world_record_dir' 2>&1 | tee -a '$world_log'" Enter
  else
    tmux send-keys -t "${TMUX_SESSION}:world-$i" \
      "cd '$ROOT_DIR' && '$CLAWBLOX_BIN' run '$WORLD_ABS_DIR' --port $port 2>&1 | tee -a '$world_log'" Enter
  fi
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

TEMPLATE_ENV_PREFIX=""
if [[ -n "$TEMPLATE_ABS_DIR" ]]; then
  TEMPLATE_ENV_PREFIX="TEMPLATE_DIR=$(printf '%q' "$TEMPLATE_ABS_DIR") "
fi

CHECKPOINT_ENV_PREFIX=""
if [[ -n "$AGENT_CHECKPOINT_AT_EPOCH" ]]; then
  CHECKPOINT_ENV_PREFIX+="AGENT_CHECKPOINT_AT_EPOCH=$(printf '%q' "$AGENT_CHECKPOINT_AT_EPOCH") "
  CHECKPOINT_ENV_PREFIX+="AGENT_CHECKPOINT_PROMPT=$(printf '%q' "$AGENT_CHECKPOINT_PROMPT") "
  CHECKPOINT_ENV_PREFIX+="AGENT_CHECKPOINT_TRIGGER_TURN=$(printf '%q' "$AGENT_CHECKPOINT_TRIGGER_TURN") "
  CHECKPOINT_ENV_PREFIX+="AGENT_CHECKPOINT_DISABLE_AUTO_LOOP=$(printf '%q' "$AGENT_CHECKPOINT_DISABLE_AUTO_LOOP") "
fi

for ((i = 0; i < NUM_WORLDS; i++)); do
  port=$((BASE_PORT + i))
  url="http://localhost:${port}"
  startup_instructions=""
  if [[ -n "$GOAL" ]]; then
    startup_instructions="$(cat <<EOF
First fetch capabilities with:
curl -sS -H "X-Session: <session-key>" ${url}/skill.md
Use the session key shown above in this prompt.

Then follow this objective:
${GOAL}
EOF
)"
  fi

  tmux new-window -t "$TMUX_SESSION" -n "agents-$i"
  for ((j = 0; j < AGENTS_PER_WORLD; j++)); do
    if ((j > 0)); then
      tmux split-window -t "${TMUX_SESSION}:agents-$i"
      tmux select-layout -t "${TMUX_SESSION}:agents-$i" tiled
    fi

    agent_name="$(sanitize_name "${AGENT_NAME_PREFIX}-r${RUN_SAFE_ID}-w${i}-a${j}")"
    world_agents_dir="$WORLDS_ROOT/world-$i/agents"
    agent_dir="$world_agents_dir/$agent_name"
    agent_log_dir="$agent_dir/logs"
    agent_workspace="$agent_dir/workspace"
    agent_session="$agent_dir/session"
    agent_log_file="$agent_log_dir/agent.log"

    mkdir -p "$agent_log_dir" "$agent_workspace" "$agent_session"

    if [[ -n "$startup_instructions" ]]; then
      startup_instructions_q="$(printf '%q' "$startup_instructions")"
      tmux send-keys -t "${TMUX_SESSION}:agents-$i" \
        "cd '$ROOT_DIR' && WORLD_BASE_URL='$url' WORLD_AGENT_NAME='$agent_name' WORKSPACE_DIR='$agent_workspace' SESSION_DIR='$agent_session' ${TEMPLATE_ENV_PREFIX}${CHECKPOINT_ENV_PREFIX}WORLD_STARTUP_INSTRUCTIONS=${startup_instructions_q} bash '$AGENT_SCRIPT' 2>&1 | tee -a '$agent_log_file'" Enter
    else
      tmux send-keys -t "${TMUX_SESSION}:agents-$i" \
        "cd '$ROOT_DIR' && WORLD_BASE_URL='$url' WORLD_AGENT_NAME='$agent_name' WORKSPACE_DIR='$agent_workspace' SESSION_DIR='$agent_session' ${TEMPLATE_ENV_PREFIX}${CHECKPOINT_ENV_PREFIX}bash '$AGENT_SCRIPT' 2>&1 | tee -a '$agent_log_file'" Enter
    fi
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
echo "  Results: $RUN_DIR"
echo "  Results Root: $RESULTS_ABS_ROOT"
echo "  World Data: $WORLDS_ROOT"
echo "========================================"
echo
echo "  tmux attach -t $TMUX_SESSION"
echo
echo "  Stop all:  $(basename "$0") --stop"
echo "  Status:    $(basename "$0") --status"
echo

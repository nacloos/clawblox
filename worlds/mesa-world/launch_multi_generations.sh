#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
LAUNCH_SCRIPT="${LAUNCH_SCRIPT:-$SCRIPT_DIR/launch_multi.sh}"

GENERATIONS="${GENERATIONS:-5}"
GENERATION_DURATION="${GENERATION_DURATION:-2h}"
EXPERIMENT_ID="${EXPERIMENT_ID:-genexp-$(date -u +%Y%m%dT%H%M%SZ)}"
RUN_PREFIX="${RUN_PREFIX:-$EXPERIMENT_ID}"
TMUX_PREFIX="${TMUX_PREFIX:-clawblox-gen}"
WORLD_DIR="${WORLD_DIR:-worlds/mesa-world}"
BASE_PORT="${BASE_PORT:-8085}"
RECORD="${RECORD:-true}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-30}"
STOP_GRACE_SECONDS="${STOP_GRACE_SECONDS:-5}"
SAVE_GRACE_SECONDS="${SAVE_GRACE_SECONDS:-300}"
SAVE_PROMPT="${SAVE_PROMPT:-}"
if [[ -z "$SAVE_PROMPT" ]]; then
  SAVE_PROMPT="5 minutes remaining before session stops."
fi
AGENT_NAME_PREFIX="${AGENT_NAME_PREFIX:-mesa}"
GOAL="${GOAL:-}"
INITIAL_TEMPLATE="${INITIAL_TEMPLATE:-}"
CLAWBLOX_BIN="${CLAWBLOX_BIN:-}"

sanitize_name() {
  printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_'
}

is_positive_int() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
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

parse_duration_seconds() {
  local value="$1"
  if [[ ! "$value" =~ ^([0-9]+)([smh]?)$ ]]; then
    echo "Error: invalid duration '$value'. Use 3600, 60m, or 1h." >&2
    exit 1
  fi

  local amount="${BASH_REMATCH[1]}"
  local unit="${BASH_REMATCH[2]}"
  case "$unit" in
    ""|"s")
      printf '%s\n' "$amount"
      ;;
    "m")
      printf '%s\n' $((amount * 60))
      ;;
    "h")
      printf '%s\n' $((amount * 3600))
      ;;
    *)
      echo "Error: unsupported duration unit in '$value'." >&2
      exit 1
      ;;
  esac
}

to_world_abs_dir() {
  local value="$1"
  if [[ "$value" = /* ]]; then
    printf '%s\n' "$value"
  else
    printf '%s\n' "$ROOT_DIR/$value"
  fi
}

to_root_abs_path() {
  local value="$1"
  if [[ "$value" = /* ]]; then
    printf '%s\n' "$value"
  else
    printf '%s\n' "$ROOT_DIR/$value"
  fi
}

csv_escape() {
  printf '%s' "$1" | sed 's/"/""/g'
}

sleep_until_deadline() {
  local deadline="$1"
  while ((SECONDS < deadline)); do
    local remaining=$((deadline - SECONDS))
    if ((remaining <= 0)); then
      break
    fi
    local sleep_for=5
    if ((remaining < sleep_for)); then
      sleep_for="$remaining"
    fi
    sleep "$sleep_for"
  done
}

schedule_stop_watchdog() {
  local tmux_session="$1"
  local expected_session_created="$2"
  local duration_seconds="$3"

  local launch_script_q
  local tmux_session_q
  local expected_created_q
  local stop_grace_q
  printf -v launch_script_q '%q' "$LAUNCH_SCRIPT"
  printf -v tmux_session_q '%q' "$tmux_session"
  printf -v expected_created_q '%q' "$expected_session_created"
  printf -v stop_grace_q '%q' "$STOP_GRACE_SECONDS"

  local watchdog_cmd
  watchdog_cmd="sleep $duration_seconds; if tmux has-session -t $tmux_session_q 2>/dev/null; then current_created=\$(tmux display-message -p -t $tmux_session_q '#{session_created}' 2>/dev/null || true); if [[ \"\$current_created\" == $expected_created_q ]]; then bash $launch_script_q --tmux-session $tmux_session_q --stop --stop-grace-seconds $stop_grace_q >/dev/null 2>&1 || true; fi; fi"
  tmux run-shell -b "$watchdog_cmd"
}

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Run a single-agent generation chain where each generation uses the previous
generation workspace as --template for launch_multi.sh.

Options:
  --generations N               Number of generations to run (default: 5)
  --generation-duration D       Per-generation runtime (e.g. 3600, 60m, 1h; default: 2h)
  --experiment-id ID            Metadata directory under results/ (default: timestamped)
  --run-prefix PREFIX           Prefix for per-generation run ids (default: experiment id)
  --tmux-prefix PREFIX          Prefix for per-generation tmux sessions (default: clawblox-gen)
  --world-dir PATH              World directory (relative to project root or absolute)
  --base-port PORT              World server port for each generation (default: 8085)
  --record true|false           Recording toggle forwarded to launch_multi.sh
  --health-timeout SECONDS      Health timeout forwarded to launch_multi.sh
  --stop-grace-seconds N        Graceful stop wait forwarded to launch_multi.sh
  --save-grace-seconds N        Final save window before stop (default: 300)
  --save-prompt TEXT            Prompt sent by clawblox-agent extension at save window start
  --agent-name-prefix PREFIX    Agent name prefix forwarded to launch_multi.sh
  --goal TEXT                   Objective forwarded to launch_multi.sh
  --template PATH               Initial template directory for generation 1
  --clawblox-bin PATH           CLAWBLOX_BIN forwarded to launch_multi.sh
  --help                        Show this message

Environment overrides:
  GENERATIONS, GENERATION_DURATION, EXPERIMENT_ID, RUN_PREFIX, TMUX_PREFIX,
  WORLD_DIR, BASE_PORT, RECORD, HEALTH_TIMEOUT, STOP_GRACE_SECONDS,
  SAVE_GRACE_SECONDS, SAVE_PROMPT, AGENT_NAME_PREFIX, GOAL, INITIAL_TEMPLATE, CLAWBLOX_BIN
EOF
}

if [[ ! -f "$LAUNCH_SCRIPT" ]]; then
  echo "Error: launch script not found at $LAUNCH_SCRIPT"
  exit 1
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --generations)
      require_value "$1" "${2:-}"
      GENERATIONS="$2"
      shift 2
      ;;
    --generation-duration)
      require_value "$1" "${2:-}"
      GENERATION_DURATION="$2"
      shift 2
      ;;
    --experiment-id)
      require_value "$1" "${2:-}"
      EXPERIMENT_ID="$2"
      shift 2
      ;;
    --run-prefix)
      require_value "$1" "${2:-}"
      RUN_PREFIX="$2"
      shift 2
      ;;
    --tmux-prefix)
      require_value "$1" "${2:-}"
      TMUX_PREFIX="$2"
      shift 2
      ;;
    --world-dir)
      require_value "$1" "${2:-}"
      WORLD_DIR="$2"
      shift 2
      ;;
    --base-port)
      require_value "$1" "${2:-}"
      BASE_PORT="$2"
      shift 2
      ;;
    --record)
      require_value "$1" "${2:-}"
      RECORD="$2"
      shift 2
      ;;
    --health-timeout)
      require_value "$1" "${2:-}"
      HEALTH_TIMEOUT="$2"
      shift 2
      ;;
    --stop-grace-seconds)
      require_value "$1" "${2:-}"
      STOP_GRACE_SECONDS="$2"
      shift 2
      ;;
    --save-grace-seconds)
      require_value "$1" "${2:-}"
      SAVE_GRACE_SECONDS="$2"
      shift 2
      ;;
    --save-prompt)
      require_value "$1" "${2:-}"
      SAVE_PROMPT="$2"
      shift 2
      ;;
    --agent-name-prefix)
      require_value "$1" "${2:-}"
      AGENT_NAME_PREFIX="$2"
      shift 2
      ;;
    --goal)
      require_value "$1" "${2:-}"
      GOAL="$2"
      shift 2
      ;;
    --template|--initial-template)
      require_value "$1" "${2:-}"
      INITIAL_TEMPLATE="$2"
      shift 2
      ;;
    --clawblox-bin)
      require_value "$1" "${2:-}"
      CLAWBLOX_BIN="$2"
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

if ! is_positive_int "$GENERATIONS"; then
  echo "Error: --generations must be a positive integer (got '$GENERATIONS')."
  exit 1
fi
if ! is_positive_int "$BASE_PORT" || ((BASE_PORT > 65535)); then
  echo "Error: --base-port must be an integer in [1, 65535] (got '$BASE_PORT')."
  exit 1
fi
if ! is_positive_int "$HEALTH_TIMEOUT"; then
  echo "Error: --health-timeout must be a positive integer (got '$HEALTH_TIMEOUT')."
  exit 1
fi
if [[ ! "$STOP_GRACE_SECONDS" =~ ^[0-9]+$ ]]; then
  echo "Error: --stop-grace-seconds must be a non-negative integer (got '$STOP_GRACE_SECONDS')."
  exit 1
fi
if [[ ! "$SAVE_GRACE_SECONDS" =~ ^[0-9]+$ ]]; then
  echo "Error: --save-grace-seconds must be a non-negative integer (got '$SAVE_GRACE_SECONDS')."
  exit 1
fi
if [[ "$RECORD" != "true" && "$RECORD" != "false" ]]; then
  echo "Error: --record must be 'true' or 'false' (got '$RECORD')."
  exit 1
fi

GENERATION_DURATION_SECONDS="$(parse_duration_seconds "$GENERATION_DURATION")"
if ((GENERATION_DURATION_SECONDS <= 0)); then
  echo "Error: --generation-duration must be greater than zero."
  exit 1
fi

WORLD_ABS_DIR="$(to_world_abs_dir "$WORLD_DIR")"
if [[ ! -d "$WORLD_ABS_DIR" ]]; then
  echo "Error: world directory not found at $WORLD_ABS_DIR"
  exit 1
fi

CURRENT_TEMPLATE=""
if [[ -n "$INITIAL_TEMPLATE" ]]; then
  CURRENT_TEMPLATE="$(to_root_abs_path "$INITIAL_TEMPLATE")"
  if [[ ! -d "$CURRENT_TEMPLATE" ]]; then
    echo "Error: initial template directory not found at $CURRENT_TEMPLATE"
    exit 1
  fi
fi

EXPERIMENT_SAFE_ID="$(sanitize_name "$EXPERIMENT_ID")"
EXPERIMENT_DIR="$WORLD_ABS_DIR/results/$EXPERIMENT_SAFE_ID"
if [[ -e "$EXPERIMENT_DIR" ]]; then
  echo "Error: experiment directory already exists: $EXPERIMENT_DIR"
  echo "Use --experiment-id with a unique value."
  exit 1
fi
mkdir -p "$EXPERIMENT_DIR"
EXPERIMENT_RUNS_DIR="$EXPERIMENT_DIR/runs"
mkdir -p "$EXPERIMENT_RUNS_DIR"

CHAIN_CSV="$EXPERIMENT_DIR/generation_chain.csv"
printf 'generation,run_id,tmux_session,template_in,workspace_out,start_utc,end_utc,duration_seconds\n' >"$CHAIN_CSV"

CURRENT_SESSION=""
cleanup_active_session() {
  if [[ -z "$CURRENT_SESSION" ]]; then
    return
  fi

  echo
  echo "Stopping active session '$CURRENT_SESSION'..."
  bash "$LAUNCH_SCRIPT" --tmux-session "$CURRENT_SESSION" --stop --stop-grace-seconds "$STOP_GRACE_SECONDS" || true
  CURRENT_SESSION=""
}

cleanup_on_signal() {
  local signal_name="$1"
  echo
  echo "Signal $signal_name received."
  cleanup_active_session
  exit 130
}

trap 'cleanup_on_signal INT' INT
trap 'cleanup_on_signal TERM' TERM
trap 'cleanup_on_signal HUP' HUP
trap cleanup_active_session EXIT

echo "Starting generation chain"
echo "  Generations: $GENERATIONS"
echo "  Duration each: ${GENERATION_DURATION_SECONDS}s"
echo "  Save grace: ${SAVE_GRACE_SECONDS}s"
echo "  Experiment metadata: $EXPERIMENT_DIR"
echo

for ((gen = 1; gen <= GENERATIONS; gen++)); do
  gen_tag="$(printf 'g%03d' "$gen")"
  run_id="$(sanitize_name "${RUN_PREFIX}-${gen_tag}")"
  tmux_session="$(sanitize_name "${TMUX_PREFIX}-${gen_tag}")"
  template_in="$CURRENT_TEMPLATE"
  start_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  echo "=== Generation $gen/$GENERATIONS ==="
  echo "  run_id: $run_id"
  echo "  tmux_session: $tmux_session"
  if [[ -n "$template_in" ]]; then
    echo "  template_in: $template_in"
  else
    echo "  template_in: (none)"
  fi

  launch_cmd=(
    bash "$LAUNCH_SCRIPT"
    --force
    --run-id "$run_id"
    --tmux-session "$tmux_session"
    --num-worlds 1
    --agents-per-world 1
    --base-port "$BASE_PORT"
    --record "$RECORD"
    --world-dir "$WORLD_DIR"
    --results-root "$EXPERIMENT_RUNS_DIR"
    --health-timeout "$HEALTH_TIMEOUT"
    --agent-name-prefix "$AGENT_NAME_PREFIX"
    --stop-grace-seconds "$STOP_GRACE_SECONDS"
  )
  if [[ -n "$GOAL" ]]; then
    launch_cmd+=(--goal "$GOAL")
  fi
  if [[ -n "$CLAWBLOX_BIN" ]]; then
    launch_cmd+=(--clawblox-bin "$CLAWBLOX_BIN")
  fi
  if [[ -n "$template_in" ]]; then
    launch_cmd+=(--template "$template_in")
  fi

  effective_save_grace="$SAVE_GRACE_SECONDS"
  if ((effective_save_grace > GENERATION_DURATION_SECONDS)); then
    effective_save_grace="$GENERATION_DURATION_SECONDS"
  fi
  active_seconds=$((GENERATION_DURATION_SECONDS - effective_save_grace))
  generation_start_epoch="$(date +%s)"
  checkpoint_at_epoch=""
  if ((effective_save_grace > 0)); then
    checkpoint_at_epoch=$((generation_start_epoch + active_seconds))
  fi

  if [[ -n "$checkpoint_at_epoch" ]]; then
    AGENT_CHECKPOINT_AT_EPOCH="$checkpoint_at_epoch" \
      AGENT_CHECKPOINT_PROMPT="$SAVE_PROMPT" \
      "${launch_cmd[@]}"
  else
    "${launch_cmd[@]}"
  fi
  CURRENT_SESSION="$tmux_session"
  session_created="$(tmux display-message -p -t "$tmux_session" '#{session_created}')"
  if [[ ! "$session_created" =~ ^[0-9]+$ ]]; then
    echo "Error: failed to resolve tmux session_created for '$tmux_session' (got '$session_created')."
    exit 1
  fi
  schedule_stop_watchdog "$tmux_session" "$session_created" "$GENERATION_DURATION_SECONDS"
  echo "  fail-safe watchdog scheduled at +${GENERATION_DURATION_SECONDS}s"

  if ((active_seconds > 0)); then
    active_deadline=$((SECONDS + active_seconds))
    sleep_until_deadline "$active_deadline"
  fi

  if ((effective_save_grace > 0)); then
    echo "  save window (${effective_save_grace}s): waiting for extension-driven checkpoint phase"
    save_deadline=$((SECONDS + effective_save_grace))
    sleep_until_deadline "$save_deadline"
  fi

  echo "  stopping session: $tmux_session"
  bash "$LAUNCH_SCRIPT" --tmux-session "$tmux_session" --stop --stop-grace-seconds "$STOP_GRACE_SECONDS"
  CURRENT_SESSION=""

  run_dir="$EXPERIMENT_RUNS_DIR/$run_id"

  agents_root="$run_dir/worlds/world-0/agents"
  if [[ ! -d "$agents_root" ]]; then
    echo "Error: expected agents directory not found: $agents_root"
    exit 1
  fi

  mapfile -t workspaces < <(find "$agents_root" -mindepth 2 -maxdepth 2 -type d -name workspace | sort)
  if ((${#workspaces[@]} == 0)); then
    echo "Error: no workspace directory found under $agents_root"
    exit 1
  fi
  if ((${#workspaces[@]} > 1)); then
    echo "Error: expected exactly 1 workspace, found ${#workspaces[@]} under $agents_root"
    exit 1
  fi
  workspace_out="${workspaces[0]}"
  CURRENT_TEMPLATE="$workspace_out"
  end_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  printf '"%s","%s","%s","%s","%s","%s","%s","%s"\n' \
    "$(csv_escape "$gen")" \
    "$(csv_escape "$run_id")" \
    "$(csv_escape "$tmux_session")" \
    "$(csv_escape "$template_in")" \
    "$(csv_escape "$workspace_out")" \
    "$(csv_escape "$start_utc")" \
    "$(csv_escape "$end_utc")" \
    "$(csv_escape "$GENERATION_DURATION_SECONDS")" >>"$CHAIN_CSV"

  echo "  workspace_out: $workspace_out"
  echo
done

echo "Generation chain complete."
echo "Metadata CSV: $CHAIN_CSV"

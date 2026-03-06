#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORLD_BASE_URL="${WORLD_BASE_URL:-http://localhost:8080}"
WORLD_JOIN_PATH="${WORLD_JOIN_PATH:-/join}"
WORLD_LEAVE_PATH="${WORLD_LEAVE_PATH:-/leave}"
WORLD_SKILL_PATH="${WORLD_SKILL_PATH:-/skill.md}"
WORLD_SESSION_FIELD="${WORLD_SESSION_FIELD:-session}"
WORLD_SESSION_HEADER="${WORLD_SESSION_HEADER:-X-Session}"
WORLD_AGENT_NAME="${WORLD_AGENT_NAME:-}"
WORKSPACE_DIR="${WORKSPACE_DIR:-}"
TEMPLATE_DIR="${TEMPLATE_DIR:-$SCRIPT_DIR/template/agent}"
PI_EXTENSION_PATH="${PI_EXTENSION_PATH:-}"
PI_NO_GLOBAL_EXTENSIONS="${PI_NO_GLOBAL_EXTENSIONS:-1}"
SESSION_DIR="${SESSION_DIR:-}"
SYSTEM_PROMPT_SNAPSHOT_FILE="${SYSTEM_PROMPT_SNAPSHOT_FILE:-}"
WORLD_STARTUP_INSTRUCTIONS="${WORLD_STARTUP_INSTRUCTIONS:-}"

generate_agent_name() {
  local -a first_parts=(Brisk Cozy Daring Fuzzy Jolly Lunar Nova Pixel Sunny Swift)
  local -a second_parts=(Fox Owl Lynx Wolf Otter Koala Panda Kiwi Moth Crab)
  local first="${first_parts[$RANDOM % ${#first_parts[@]}]}"
  local second="${second_parts[$RANDOM % ${#second_parts[@]}]}"
  local suffix
  suffix="$(printf '%02d' "$((RANDOM % 100))")"
  printf '%s' "${first}${second}${suffix}"
}

sanitize_name() {
  printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_'
}

if [[ -z "$WORLD_AGENT_NAME" ]]; then
  WORLD_AGENT_NAME="$(generate_agent_name)"
fi

SAFE_AGENT_NAME="$(sanitize_name "$WORLD_AGENT_NAME")"

if [[ -z "$WORKSPACE_DIR" ]]; then
  WORKSPACE_DIR="$SCRIPT_DIR/workspace/$SAFE_AGENT_NAME"
fi

if [[ -z "$SESSION_DIR" ]]; then
  SESSION_DIR="$SCRIPT_DIR/logs/$SAFE_AGENT_NAME"
fi

if [[ -z "$SYSTEM_PROMPT_SNAPSHOT_FILE" ]]; then
  SYSTEM_PROMPT_SNAPSHOT_FILE="$SESSION_DIR/system_prompt.txt"
fi

mkdir -p "$WORKSPACE_DIR"
mkdir -p "$SESSION_DIR"
export PI_SYSTEM_PROMPT_SNAPSHOT_FILE="$SYSTEM_PROMPT_SNAPSHOT_FILE"

if [[ -d "$TEMPLATE_DIR" ]]; then
  while IFS= read -r -d '' template_path; do
    file_name="$(basename "$template_path")"
    dest="$WORKSPACE_DIR/$file_name"
    if [[ ! -e "$dest" ]]; then
      cp "$template_path" "$dest"
    fi
  done < <(find "$TEMPLATE_DIR" -maxdepth 1 -type f -print0 | sort -z)
fi

SESSION_ID=""
if [[ -n "$WORLD_JOIN_PATH" ]]; then
  JOIN_JSON="$(curl -fsS -X POST "${WORLD_BASE_URL%/}${WORLD_JOIN_PATH}?name=${WORLD_AGENT_NAME}")"
  SESSION_ID="$(node -e 'const o=JSON.parse(process.argv[1]); const key=process.argv[2]; process.stdout.write(String(o[key]||""));' "$JOIN_JSON" "$WORLD_SESSION_FIELD")"
  if [[ -z "$SESSION_ID" ]]; then
    echo "Failed to get session field '$WORLD_SESSION_FIELD' from: $JOIN_JSON" >&2
    exit 1
  fi
fi

PROMPT_FILE="$(mktemp -t pi-agent-prompt.XXXXXX.md)"
cleanup() {
  rm -f "$PROMPT_FILE"
  if [[ -n "${SESSION_ID:-}" && -n "$WORLD_LEAVE_PATH" ]]; then
    curl -fsS -X POST -H "${WORLD_SESSION_HEADER}: ${SESSION_ID}" "${WORLD_BASE_URL%/}${WORLD_LEAVE_PATH}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

if [[ -n "$WORLD_STARTUP_INSTRUCTIONS" ]]; then
  STARTUP_INSTRUCTIONS="$WORLD_STARTUP_INSTRUCTIONS"
elif [[ -n "$WORLD_SKILL_PATH" ]]; then
  if [[ -n "$SESSION_ID" ]]; then
    STARTUP_INSTRUCTIONS="$(cat <<EOF
1. Fetch capabilities with: curl -sS -H "${WORLD_SESSION_HEADER}: ${SESSION_ID}" ${WORLD_BASE_URL%/}${WORLD_SKILL_PATH}
2. Follow the capability document exactly.
EOF
)"
  else
    STARTUP_INSTRUCTIONS="$(cat <<EOF
1. Fetch capabilities with: curl -sS ${WORLD_BASE_URL%/}${WORLD_SKILL_PATH}
2. Follow the capability document exactly.
EOF
)"
  fi
else
  STARTUP_INSTRUCTIONS="1. Follow the world's documented protocol."
fi

cat >"$PROMPT_FILE" <<EOF
You are a being living in your world.
Your workspace is ${WORKSPACE_DIR}. Only work inside this directory.

World base URL: ${WORLD_BASE_URL}
$(if [[ -n "$SESSION_ID" ]]; then printf 'Session header: %s\nSession key: %s\n' "$WORLD_SESSION_HEADER" "$SESSION_ID"; fi)
At startup:
${STARTUP_INSTRUCTIONS}
EOF

cd "$WORKSPACE_DIR"
echo "Agent name: $WORLD_AGENT_NAME"
echo "Workspace: $WORKSPACE_DIR"
echo "Pi session dir: $SESSION_DIR"
if [[ -n "$SESSION_ID" ]]; then
  echo "Session: $SESSION_ID"
fi

WATCH_FLAG=()
if pi --help 2>&1 | grep -q -- '--watch'; then
  WATCH_FLAG=(--watch)
fi

EXTENSION_FLAGS=()
if [[ "$PI_NO_GLOBAL_EXTENSIONS" == "1" ]]; then
  EXTENSION_FLAGS+=(--no-extensions)
fi
if [[ -n "$PI_EXTENSION_PATH" && ( -f "$PI_EXTENSION_PATH" || -d "$PI_EXTENSION_PATH" ) ]]; then
  EXTENSION_FLAGS+=(-e "$PI_EXTENSION_PATH")
fi

set +e
pi \
  "${WATCH_FLAG[@]}" \
  "${EXTENSION_FLAGS[@]}" \
  --session-dir "$SESSION_DIR" \
  --append-system-prompt "$PROMPT_FILE" \
  "continue"
status=$?
set -e
exit "$status"

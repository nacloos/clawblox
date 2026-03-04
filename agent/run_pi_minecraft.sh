#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ADAPTER_URL="${WORLD_BASE_URL:-http://localhost:8081}"
BOT_NAME="${WORLD_AGENT_NAME:-}"
WORKSPACE_DIR="${WORKSPACE_DIR:-$SCRIPT_DIR/workspace/agent}"
TEMPLATE_DIR="$SCRIPT_DIR/template/agent"
PI_EXTENSION_PATH="${PI_EXTENSION_PATH:-$SCRIPT_DIR/extensions/minecraft-events.ts}"
PI_NO_GLOBAL_EXTENSIONS="${PI_NO_GLOBAL_EXTENSIONS:-1}"

generate_bot_name() {
  local -a first_parts=(Brisk Cozy Daring Fuzzy Jolly Lunar Nova Pixel Sunny Swift)
  local -a second_parts=(Fox Owl Lynx Wolf Otter Koala Panda Kiwi Moth Crab)
  local first="${first_parts[$RANDOM % ${#first_parts[@]}]}"
  local second="${second_parts[$RANDOM % ${#second_parts[@]}]}"
  local suffix
  suffix="$(printf '%02d' "$((RANDOM % 100))")"
  # Minecraft usernames max out at 16 characters.
  printf '%s' "${first}${second}${suffix}" | cut -c1-16
}

if [[ -z "$BOT_NAME" ]]; then
  BOT_NAME="$(generate_bot_name)"
fi

mkdir -p "$WORKSPACE_DIR"
for file in SOUL.md IDENTITY.md SEMANTIC_MEMORY.md EPISODIC_MEMORY.md; do
  dest="$WORKSPACE_DIR/$file"
  if [[ ! -f "$dest" ]]; then
    if [[ -f "$TEMPLATE_DIR/$file" ]]; then
      cp "$TEMPLATE_DIR/$file" "$dest"
    else
      : > "$dest"
    fi
  fi
done

JOIN_JSON="$(curl -fsS -X POST "${ADAPTER_URL%/}/join?name=${BOT_NAME}")"
SESSION_ID="$(node -e 'const o=JSON.parse(process.argv[1]); process.stdout.write(String(o.session||""));' "$JOIN_JSON")"
if [[ -z "$SESSION_ID" ]]; then
  echo "Failed to get session from /join: $JOIN_JSON" >&2
  exit 1
fi

PROMPT_FILE="$(mktemp -t pi-mc-prompt.XXXXXX.md)"
cleanup() {
  rm -f "$PROMPT_FILE"
  # Best-effort: tell adapter to close session so bot disconnects cleanly.
  if [[ -n "${SESSION_ID:-}" ]]; then
    curl -fsS -X POST -H "X-Session: ${SESSION_ID}" "${ADAPTER_URL%/}/leave" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

cat >"$PROMPT_FILE" <<EOF
/no_think
You are a being living in your world.
Your workspace is ${WORKSPACE_DIR}. Only work inside this directory.
Be polite and avoid working on your memory files when interacting with someone else.

World base URL: ${ADAPTER_URL}
Session key (X-Session): ${SESSION_ID}

At startup:
1. Call tool mc_register_session with session_id "${SESSION_ID}".
2. Fetch capabilities with: curl -sS -H "X-Session: ${SESSION_ID}" ${ADAPTER_URL%/}/skill.md
3. Use /input for world actions.
EOF

cd "$WORKSPACE_DIR"
echo "Session: $SESSION_ID"
echo "Agent name: $BOT_NAME"
WATCH_FLAG=()
if pi --help 2>&1 | grep -q -- '--watch'; then
  WATCH_FLAG=(--watch)
fi

EXTENSION_FLAGS=()
if [[ "$PI_NO_GLOBAL_EXTENSIONS" == "1" ]]; then
  EXTENSION_FLAGS+=(--no-extensions)
fi
if [[ -f "$PI_EXTENSION_PATH" || -d "$PI_EXTENSION_PATH" ]]; then
  EXTENSION_FLAGS+=(-e "$PI_EXTENSION_PATH")
fi

set +e
pi \
  "${WATCH_FLAG[@]}" \
  "${EXTENSION_FLAGS[@]}" \
  --append-system-prompt "$PROMPT_FILE" \
  "continue"
status=$?
set -e
exit "$status"

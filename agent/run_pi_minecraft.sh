#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ADAPTER_URL="${WORLD_BASE_URL:-http://localhost:8081}"
BOT_NAME="${WORLD_AGENT_NAME:-clawbot1}"
WORKSPACE_DIR="${WORKSPACE_DIR:-$SCRIPT_DIR/workspace/agent}"
TEMPLATE_DIR="$SCRIPT_DIR/template/agent"

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
trap 'rm -f "$PROMPT_FILE"' EXIT

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
WATCH_FLAG=()
if pi --help 2>&1 | grep -q -- '--watch'; then
  WATCH_FLAG=(--watch)
fi

exec pi \
  "${WATCH_FLAG[@]}" \
  --append-system-prompt "$PROMPT_FILE" \
  "continue"

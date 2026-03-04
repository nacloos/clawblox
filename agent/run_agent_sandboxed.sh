#!/usr/bin/env bash
set -euo pipefail

# Safe-by-default wrapper for running the Pi Minecraft agent in bubblewrap.
# It keeps the project read-only and grants write access only to WORKSPACE_DIR.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUN_SCRIPT="${RUN_SCRIPT:-$SCRIPT_DIR/run_pi_minecraft.sh}"
WORKSPACE_DIR="${WORKSPACE_DIR:-$SCRIPT_DIR/workspace/agent}"
ADAPTER_URL="${WORLD_BASE_URL:-http://127.0.0.1:8081}"
BOT_NAME="${WORLD_AGENT_NAME:-clawbot1}"
PI_BIN="${PI_BIN:-$(command -v pi || true)}"
NODE_BIN="${NODE_BIN:-$(command -v node || true)}"
CURL_BIN="${CURL_BIN:-$(command -v curl || true)}"
BASH_BIN="${BASH_BIN:-$(command -v bash || true)}"
SANDBOX_NET="${SANDBOX_NET:-host}" # host|none

if ! command -v bwrap >/dev/null 2>&1; then
  echo "bwrap is required but not found." >&2
  echo "Install bubblewrap, or run without sandbox: $RUN_SCRIPT" >&2
  exit 1
fi

if [[ -z "$PI_BIN" || -z "$NODE_BIN" || -z "$CURL_BIN" || -z "$BASH_BIN" ]]; then
  echo "Missing required binaries on PATH (pi/node/curl/bash)." >&2
  exit 1
fi

if [[ ! -x "$RUN_SCRIPT" ]]; then
  echo "Run script not executable: $RUN_SCRIPT" >&2
  exit 1
fi

mkdir -p "$WORKSPACE_DIR"

readonly PI_DIR="$(dirname "$PI_BIN")"
readonly NODE_DIR="$(dirname "$NODE_BIN")"
readonly CURL_DIR="$(dirname "$CURL_BIN")"
readonly BASH_DIR="$(dirname "$BASH_BIN")"

readonly UID_NOW="$(id -u)"
readonly GID_NOW="$(id -g)"
readonly USER_NAME="$(id -un)"
readonly HOME_IN_SANDBOX="/home/$USER_NAME"

bwrap_args=(
  --die-with-parent
  --new-session
  --unshare-user-try
  --uid "$UID_NOW"
  --gid "$GID_NOW"
  --unshare-pid
  --proc /proc
  --dev /dev
  --tmpfs /tmp
  --dir /home
  --dir "/home/$USER_NAME"
  --dir "$(dirname "$REPO_DIR")"
  --ro-bind "$REPO_DIR" "$REPO_DIR"
  --bind "$WORKSPACE_DIR" "$WORKSPACE_DIR"
  --ro-bind /usr /usr
  --ro-bind /bin /bin
  --ro-bind /sbin /sbin
  --ro-bind /etc /etc
  --setenv HOME "$HOME_IN_SANDBOX"
  --setenv USER "$USER_NAME"
  --setenv LOGNAME "$USER_NAME"
  --setenv WORLD_BASE_URL "$ADAPTER_URL"
  --setenv WORLD_AGENT_NAME "$BOT_NAME"
  --setenv WORKSPACE_DIR "$WORKSPACE_DIR"
  --setenv PATH "$PI_DIR:$NODE_DIR:$CURL_DIR:$BASH_DIR:/usr/bin:/bin:/usr/sbin:/sbin"
)

if [[ -d /lib ]]; then
  bwrap_args+=(--ro-bind /lib /lib)
fi
if [[ -d /lib64 ]]; then
  bwrap_args+=(--ro-bind /lib64 /lib64)
fi
if [[ "$PI_DIR" != "/usr/bin" && "$PI_DIR" != "/bin" ]]; then
  bwrap_args+=(--ro-bind "$PI_DIR" "$PI_DIR")
fi
if [[ "$NODE_DIR" != "/usr/bin" && "$NODE_DIR" != "/bin" && "$NODE_DIR" != "$PI_DIR" ]]; then
  bwrap_args+=(--ro-bind "$NODE_DIR" "$NODE_DIR")
fi
if [[ "$CURL_DIR" != "/usr/bin" && "$CURL_DIR" != "/bin" && "$CURL_DIR" != "$PI_DIR" && "$CURL_DIR" != "$NODE_DIR" ]]; then
  bwrap_args+=(--ro-bind "$CURL_DIR" "$CURL_DIR")
fi
if [[ "$BASH_DIR" != "/usr/bin" && "$BASH_DIR" != "/bin" && "$BASH_DIR" != "$PI_DIR" && "$BASH_DIR" != "$NODE_DIR" && "$BASH_DIR" != "$CURL_DIR" ]]; then
  bwrap_args+=(--ro-bind "$BASH_DIR" "$BASH_DIR")
fi
if [[ "$SANDBOX_NET" == "none" ]]; then
  bwrap_args+=(--unshare-net)
fi

echo "Running sandboxed agent"
echo "  Run script:   $RUN_SCRIPT"
echo "  Workspace:    $WORKSPACE_DIR (rw)"
echo "  Adapter URL:  $ADAPTER_URL"
echo "  Agent name:   $BOT_NAME"
echo "  Network mode: $SANDBOX_NET"
echo
echo "Note: this is filesystem/process isolation. Keep adapter private (127.0.0.1)."

exec bwrap "${bwrap_args[@]}" -- "$BASH_BIN" "$RUN_SCRIPT"

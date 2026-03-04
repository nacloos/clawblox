#!/usr/bin/env bash
set -euo pipefail

GAME_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$GAME_DIR/../.." && pwd)"

MC_DIR="${MC_DIR:-$ROOT_DIR/mc-server}"
MC_JAR="${MC_JAR:-$MC_DIR/server.jar}"
MC_HOST="${MC_HOST:-127.0.0.1}"
MC_PORT="${MC_PORT:-25565}"
MC_VERSION="${MC_VERSION:-1.21.1}"
MC_USE_LAN="${MC_USE_LAN:-0}"

ADAPTER_DIR="$ROOT_DIR/minecraft-adapter"
ADAPTER_PORT="${ADAPTER_PORT:-8081}"
VIEWER_PORT="${VIEWER_PORT:-3007}"

MC_PID=""
ADAPTER_PID=""

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1"
    exit 1
  }
}

require_cmd node
require_cmd npm
require_cmd curl
if [[ "$MC_USE_LAN" != "1" ]]; then
  require_cmd java
fi

download_paper_jar() {
  local version_json build_json build jar_name download_url
  echo "Minecraft server jar not found. Downloading Paper ${MC_VERSION}..."

  version_json="$(curl -fsSL "https://api.papermc.io/v2/projects/paper/versions/${MC_VERSION}")"
  build="$(node -e 'const o=JSON.parse(process.argv[1]);const b=o.builds?.at?.(-1);if(!b)process.exit(1);process.stdout.write(String(b));' "$version_json")"
  build_json="$(curl -fsSL "https://api.papermc.io/v2/projects/paper/versions/${MC_VERSION}/builds/${build}")"
  jar_name="$(node -e 'const o=JSON.parse(process.argv[1]);const n=o.downloads?.application?.name;if(!n)process.exit(1);process.stdout.write(n);' "$build_json")"

  mkdir -p "$MC_DIR"
  download_url="https://api.papermc.io/v2/projects/paper/versions/${MC_VERSION}/builds/${build}/downloads/${jar_name}"
  curl -fL "$download_url" -o "$MC_JAR"
  echo "Downloaded: $MC_JAR"
}

jar_mc_version() {
  unzip -p "$MC_JAR" version.json 2>/dev/null \
    | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{const o=JSON.parse(d);process.stdout.write(String(o.id||""));}catch(_){}})' \
    || true
}

wait_for_port() {
  local host="$1" port="$2" name="$3"
  for _ in $(seq 1 120); do
    if (echo >/dev/tcp/"$host"/"$port") >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  echo "Timed out waiting for $name at $host:$port"
  return 1
}

port_in_use() {
  local port="$1"
  if command -v ss >/dev/null 2>&1; then
    ss -ltn "sport = :$port" 2>/dev/null | tail -n +2 | grep -q .
    return
  fi
  if command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
    return
  fi
  return 1
}

cleanup() {
  if [[ -n "$ADAPTER_PID" ]] && kill -0 "$ADAPTER_PID" 2>/dev/null; then
    kill -TERM "-$ADAPTER_PID" 2>/dev/null || kill "$ADAPTER_PID" 2>/dev/null || true
  fi
  if [[ -n "$MC_PID" ]] && kill -0 "$MC_PID" 2>/dev/null; then
    kill -TERM "-$MC_PID" 2>/dev/null || kill "$MC_PID" 2>/dev/null || true
  fi
}

trap cleanup EXIT INT TERM

if [[ ! -d "$ADAPTER_DIR/node_modules" ]]; then
  (cd "$ADAPTER_DIR" && npm install)
fi

if port_in_use "$ADAPTER_PORT"; then
  echo "Port $ADAPTER_PORT is already in use (adapter)."
  echo "Stop it first (e.g.: fuser -k ${ADAPTER_PORT}/tcp) or set ADAPTER_PORT to another port."
  exit 1
fi

if [[ "$MC_USE_LAN" != "1" ]]; then
  if [[ ! -f "$MC_JAR" ]]; then
    download_paper_jar
  else
    existing_version="$(jar_mc_version)"
    if [[ -n "$existing_version" && "$existing_version" != "$MC_VERSION" ]]; then
      echo "Existing server jar is Minecraft $existing_version, but MC_VERSION=$MC_VERSION."
      echo "Redownloading matching Paper server..."
      rm -f "$MC_JAR"
      download_paper_jar
    fi
  fi

  mkdir -p "$MC_DIR"
  if [[ ! -f "$MC_DIR/eula.txt" ]]; then
    echo "eula=true" >"$MC_DIR/eula.txt"
  fi

  echo "Starting Minecraft server..."
  if command -v setsid >/dev/null 2>&1; then
    setsid bash -lc "cd '$MC_DIR' && exec java -Xms1G -Xmx2G -jar '$MC_JAR' nogui" \
      > >(sed -u 's/^/[mc-server] /') \
      2> >(sed -u 's/^/[mc-server] /' >&2) &
  else
    (cd "$MC_DIR" && exec java -Xms1G -Xmx2G -jar "$MC_JAR" nogui) \
      > >(sed -u 's/^/[mc-server] /') \
      2> >(sed -u 's/^/[mc-server] /' >&2) &
  fi
  MC_PID="$!"

  wait_for_port "$MC_HOST" "$MC_PORT" "Minecraft server"
else
  echo "Using existing LAN/remote Minecraft at ${MC_HOST}:${MC_PORT} (MC_USE_LAN=1)"
fi

echo "Starting Minecraft adapter..."
if command -v setsid >/dev/null 2>&1; then
  setsid bash -lc "cd '$ADAPTER_DIR' && exec env PORT='$ADAPTER_PORT' MC_HOST='$MC_HOST' MC_PORT='$MC_PORT' MC_VERSION='$MC_VERSION' MC_VIEWER=1 MC_VIEWER_PORT='$VIEWER_PORT' npm run start:eval" \
    > >(sed -u 's/^/[mc-adapter] /') \
    2> >(sed -u 's/^/[mc-adapter] /' >&2) &
else
  (cd "$ADAPTER_DIR" && exec env PORT="$ADAPTER_PORT" MC_HOST="$MC_HOST" MC_PORT="$MC_PORT" MC_VERSION="$MC_VERSION" MC_VIEWER=1 MC_VIEWER_PORT="$VIEWER_PORT" npm run start:eval) \
    > >(sed -u 's/^/[mc-adapter] /') \
    2> >(sed -u 's/^/[mc-adapter] /' >&2) &
fi
ADAPTER_PID="$!"

sleep 0.4
if ! kill -0 "$ADAPTER_PID" 2>/dev/null; then
  echo "Minecraft adapter exited immediately."
  echo "Check the [mc-adapter] logs above for the cause."
  exit 1
fi

wait_for_port "127.0.0.1" "$ADAPTER_PORT" "Minecraft adapter"

echo
echo "Minecraft stack ready:"
if [[ "$MC_USE_LAN" == "1" ]]; then
  echo "  Server:   ${MC_HOST}:${MC_PORT} (LAN/remote)"
else
  echo "  Server:   ${MC_HOST}:${MC_PORT} (local Paper)"
fi
echo "  Adapter:  http://127.0.0.1:${ADAPTER_PORT}"
echo "  Viewer:   http://127.0.0.1:${VIEWER_PORT}/"
echo
echo "Join a bot to activate viewer:"
echo "  curl -X POST \"http://127.0.0.1:${ADAPTER_PORT}/join?name=pi\""
echo
echo "Press Ctrl+C to stop."

set +e
wait -n "$ADAPTER_PID" ${MC_PID:+ "$MC_PID"}
status=$?
set -e
exit "$status"

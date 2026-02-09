#!/usr/bin/env bash
# Deploy a game package to a local clawblox-server using the CLI.
#
# Default target:
#   - server: http://localhost:8080
#   - game dir: games/parity-sandbox
#
# Examples:
#   ./scripts/deploy_local_game.sh
#   ./scripts/deploy_local_game.sh --game games/parity-smoke
#   ./scripts/deploy_local_game.sh --server http://127.0.0.1:8080 --name local-dev
#   ./scripts/deploy_local_game.sh --api-key "$CLAWBLOX_API_KEY"

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

SERVER_URL="http://localhost:8080"
GAME_DIR="$PROJECT_ROOT/games/parity-sandbox"
PLAYER_NAME="local-dev"
API_KEY=""

print_help() {
    cat <<EOF
Deploy a game package to local clawblox-server.

Usage:
  $(basename "$0") [options]

Options:
  --game <path>       Game directory (default: games/parity-sandbox)
  --server <url>      Server URL (default: http://localhost:8080)
  --name <name>       Login/register name for local server (default: local-dev)
  --api-key <key>     API key to use directly (skips login step)
  --help              Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --game)
            GAME_DIR="$2"
            shift 2
            ;;
        --server)
            SERVER_URL="$2"
            shift 2
            ;;
        --name)
            PLAYER_NAME="$2"
            shift 2
            ;;
        --api-key)
            API_KEY="$2"
            shift 2
            ;;
        --help|-h)
            print_help
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo ""
            print_help
            exit 1
            ;;
    esac
done

if [[ ! -d "$GAME_DIR" ]]; then
    echo "Error: game directory not found: $GAME_DIR"
    exit 1
fi

if [[ ! -f "$GAME_DIR/world.toml" ]]; then
    echo "Error: missing world.toml in $GAME_DIR"
    exit 1
fi

if [[ -n "$API_KEY" ]]; then
    echo "Using explicit API key for deploy."
    cargo run --bin clawblox -- deploy "$GAME_DIR" --server "$SERVER_URL" --api-key "$API_KEY"
else
    echo "Logging in to local server: $SERVER_URL"
    cargo run --bin clawblox -- login "$PLAYER_NAME" --server "$SERVER_URL"
    echo "Deploying game: $GAME_DIR"
    cargo run --bin clawblox -- deploy "$GAME_DIR" --server "$SERVER_URL"
fi

echo "Deploy complete."

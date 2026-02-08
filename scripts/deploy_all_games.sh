#!/bin/bash
# Deploy all games that contain a world.toml

set -e

GAMES_DIR="$(dirname "$0")/../games"
DEPLOYED=0
FAILED=0

echo "Deploying all games in $GAMES_DIR..."
echo ""

for game_dir in "$GAMES_DIR"/*/; do
    if [ -f "$game_dir/world.toml" ]; then
        game_name=$(basename "$game_dir")
        echo "▶ Deploying $game_name..."
        if (cd "$game_dir" && clawblox deploy); then
            echo "✓ $game_name deployed"
            DEPLOYED=$((DEPLOYED + 1))
        else
            echo "✗ $game_name failed"
            FAILED=$((FAILED + 1))
        fi
        echo ""
    fi
done

echo "Done: $DEPLOYED deployed, $FAILED failed"

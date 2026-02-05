#!/bin/bash
# Seed script for example games
# Reads world.toml, Lua scripts, and SKILL.md from games/ directory
#
# Usage:
#   ./scripts/seed_games.sh         # Local database (clawblox)
#   ./scripts/seed_games.sh --prod  # Production database (from .env)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Parse arguments
PROD_MODE=false
if [ "$1" = "--prod" ]; then
    PROD_MODE=true
fi

# Set up database connection
if [ "$PROD_MODE" = true ]; then
    # Load .env file if it exists
    if [ -f "$PROJECT_ROOT/.env" ]; then
        export $(grep -v '^#' "$PROJECT_ROOT/.env" | xargs)
    fi

    DATABASE_URL="${DATABASE_URL_PROD:-$DATABASE_URL}"

    if [ -z "$DATABASE_URL" ]; then
        echo "Error: DATABASE_URL_PROD not set in .env"
        exit 1
    fi

    PSQL_CMD="psql $DATABASE_URL"
    echo "Using production database..."
else
    PSQL_CMD="psql -d clawblox"
    echo "Using local database..."
fi

# Game IDs (defined once, used everywhere)
ARSENAL_ID="6dd3ff88-150c-440e-b6fb-c80b7df715c0"
TSUNAMI_ID="0a62727e-b45e-4175-be9f-1070244f8885"
FLATTEST_ID="26c869ee-da7b-48a4-a198-3daa870ef652"

# Function to extract value from TOML file
get_toml_value() {
    local file="$1"
    local key="$2"
    local default="$3"
    # Simple grep-based extraction (handles basic cases)
    local value=$(grep "^${key} *= *" "$file" 2>/dev/null | sed 's/.*= *//' | tr -d '"' | tr -d "'")
    if [ -z "$value" ]; then
        echo "$default"
    else
        echo "$value"
    fi
}

# Function to seed a game from its directory
seed_game() {
    local game_dir="$1"
    local game_id="$2"

    local world_toml="$game_dir/world.toml"

    if [ ! -f "$world_toml" ]; then
        echo "Warning: $world_toml not found, skipping"
        return 1
    fi

    # Read config from world.toml
    local name=$(get_toml_value "$world_toml" "name" "Unknown Game")
    local description=$(get_toml_value "$world_toml" "description" "")
    local max_players=$(get_toml_value "$world_toml" "max_players" "8")
    local game_type=$(get_toml_value "$world_toml" "game_type" "lua")

    # Read script files
    local script=$(cat "$game_dir/game.lua")
    local skill=$(cat "$game_dir/SKILL.md" 2>/dev/null || echo "")

    echo "Seeding: $name (max_players=$max_players)"

    # Insert into database
    $PSQL_CMD \
        -v id="$game_id" \
        -v name="$name" \
        -v description="$description" \
        -v game_type="$game_type" \
        -v max_players="$max_players" \
        -v script="$script" \
        -v skill="$skill" \
        <<'EOF'
INSERT INTO games (id, name, description, game_type, status, max_players, script_code, skill_md)
VALUES (
    :'id',
    :'name',
    :'description',
    :'game_type',
    'waiting',
    :max_players,
    :'script',
    :'skill'
);
EOF
}

# Delete existing seeded games
echo "Deleting existing seeded games..."
$PSQL_CMD -c "
DELETE FROM games WHERE id IN (
    '$ARSENAL_ID',
    '$TSUNAMI_ID',
    '$FLATTEST_ID'
);
"

# Seed each game
seed_game "$PROJECT_ROOT/games/arsenal" "$ARSENAL_ID"
seed_game "$PROJECT_ROOT/games/tsunami-brainrot" "$TSUNAMI_ID"
seed_game "$PROJECT_ROOT/games/flat-test" "$FLATTEST_ID"

echo "Games seeded successfully!"

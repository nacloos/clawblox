#!/bin/bash
# Seed script for Railway/production database
# Usage: ./scripts/seed_games_prod.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Load .env file if it exists
if [ -f "$PROJECT_ROOT/.env" ]; then
    export $(grep -v '^#' "$PROJECT_ROOT/.env" | xargs)
fi

# Use DATABASE_URL_PROD from .env, fallback to DATABASE_URL
DATABASE_URL="${DATABASE_URL_PROD:-$DATABASE_URL}"

if [ -z "$DATABASE_URL" ]; then
    echo "Error: DATABASE_URL_PROD not set in .env"
    exit 1
fi

# Game IDs (defined once, used everywhere)
ARSENAL_ID="6dd3ff88-150c-440e-b6fb-c80b7df715c0"
TSUNAMI_ID="0a62727e-b45e-4175-be9f-1070244f8885"

# Read the game script and skill definition
ARSENAL_SCRIPT=$(cat "$PROJECT_ROOT/games/arsenal/game.lua")
ARSENAL_SKILL=$(cat "$PROJECT_ROOT/games/arsenal/SKILL.md")

TSUNAMI_SCRIPT=$(cat "$PROJECT_ROOT/games/tsunami-brainrot/game.lua")
TSUNAMI_SKILL=$(cat "$PROJECT_ROOT/games/tsunami-brainrot/SKILL.md")

echo "Connecting to production database..."

# Delete existing seeded games
psql "$DATABASE_URL" -c "
DELETE FROM games WHERE id IN (
    '$ARSENAL_ID',
    '$TSUNAMI_ID'
);
"

# Insert Block Arsenal game
psql "$DATABASE_URL" -v script="$ARSENAL_SCRIPT" -v skill="$ARSENAL_SKILL" -v id="$ARSENAL_ID" <<'EOF'
INSERT INTO games (id, name, description, game_type, status, script_code, skill_md)
VALUES (
    :'id',
    'Block Arsenal',
    'Gun Game / Arms Race - Progress through 15 weapons by getting kills. First to kill with the Golden Knife wins!',
    'lua',
    'waiting',
    :'script',
    :'skill'
);
EOF

# Insert Tsunami Brainrot game
psql "$DATABASE_URL" -v script="$TSUNAMI_SCRIPT" -v skill="$TSUNAMI_SKILL" -v id="$TSUNAMI_ID" <<'EOF'
INSERT INTO games (id, name, description, game_type, status, script_code, skill_md)
VALUES (
    :'id',
    'Escape Tsunami For Brainrots',
    'Collect brainrots, deposit for money, buy speed upgrades..',
    'lua',
    'waiting',
    :'script',
    :'skill'
);
EOF

echo "Production database seeded successfully!"

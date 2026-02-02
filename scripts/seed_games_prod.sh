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

# Read the game script
ARSENAL_SCRIPT=$(cat "$PROJECT_ROOT/games/arsenal/game.lua")

echo "Connecting to production database..."

# Delete existing seeded games
psql "$DATABASE_URL" -c "
DELETE FROM games WHERE id IN (
    'a0000000-0000-0000-0000-000000000001',
    'a0000000-0000-0000-0000-000000000002',
    'a0000000-0000-0000-0000-000000000003',
    'a0000000-0000-0000-0000-000000000004',
    'a0000000-0000-0000-0000-000000000005'
);
"

# Insert Block Arsenal game
psql "$DATABASE_URL" -v script="$ARSENAL_SCRIPT" <<'EOF'
INSERT INTO games (id, name, description, game_type, status, script_code)
VALUES (
    'a0000000-0000-0000-0000-000000000005',
    'Block Arsenal',
    'Gun Game / Arms Race - Progress through 15 weapons by getting kills. First to kill with the Golden Knife wins!',
    'lua',
    'waiting',
    :'script'
);
EOF

echo "Production database seeded successfully!"

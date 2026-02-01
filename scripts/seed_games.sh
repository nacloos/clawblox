#!/bin/bash
# Seed script for example games
# Reads Lua scripts from games/ directory instead of embedding them

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Read the game script (psql will handle escaping via the variable binding)
ARSENAL_SCRIPT=$(cat "$PROJECT_ROOT/games/arsenal/game.lua")

# Delete existing seeded games
psql -d clawblox -c "
DELETE FROM games WHERE id IN (
    'a0000000-0000-0000-0000-000000000001',
    'a0000000-0000-0000-0000-000000000002',
    'a0000000-0000-0000-0000-000000000003',
    'a0000000-0000-0000-0000-000000000004',
    'a0000000-0000-0000-0000-000000000005'
);
"

# Insert Block Arsenal game using psql variable binding (handles escaping)
psql -d clawblox -v script="$ARSENAL_SCRIPT" <<'EOF'
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

echo "Games seeded successfully!"

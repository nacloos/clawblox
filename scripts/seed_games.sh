#!/bin/bash
# Seed script for example games
# Reads Lua scripts from games/ directory instead of embedding them

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Game IDs (defined once, used everywhere)
ARSENAL_ID="6dd3ff88-150c-440e-b6fb-c80b7df715c0"
TSUNAMI_ID="0a62727e-b45e-4175-be9f-1070244f8885"
FLATTEST_ID="26c869ee-da7b-48a4-a198-3daa870ef652"

# Read the game scripts and skill definitions
ARSENAL_SCRIPT=$(cat "$PROJECT_ROOT/games/arsenal/game.lua")
ARSENAL_SKILL=$(cat "$PROJECT_ROOT/games/arsenal/SKILL.md")

TSUNAMI_SCRIPT=$(cat "$PROJECT_ROOT/games/tsunami-brainrot/game.lua")
TSUNAMI_SKILL=$(cat "$PROJECT_ROOT/games/tsunami-brainrot/SKILL.md")

FLATTEST_SCRIPT=$(cat "$PROJECT_ROOT/games/flat-test/game.lua")
FLATTEST_SKILL=$(cat "$PROJECT_ROOT/games/flat-test/SKILL.md")

# Delete existing seeded games
psql -d clawblox -c "
DELETE FROM games WHERE id IN (
    '$ARSENAL_ID',
    '$TSUNAMI_ID',
    '$FLATTEST_ID'
);
"

# Insert Block Arsenal game using psql variable binding (handles escaping)
psql -d clawblox -v script="$ARSENAL_SCRIPT" -v skill="$ARSENAL_SKILL" -v id="$ARSENAL_ID" <<'EOF'
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
psql -d clawblox -v script="$TSUNAMI_SCRIPT" -v skill="$TSUNAMI_SKILL" -v id="$TSUNAMI_ID" <<'EOF'
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

# Insert Flat Test game
psql -d clawblox -v script="$FLATTEST_SCRIPT" -v skill="$FLATTEST_SKILL" -v id="$FLATTEST_ID" <<'EOF'
INSERT INTO games (id, name, description, game_type, status, script_code, skill_md)
VALUES (
    :'id',
    'Flat Test',
    'Simple flat terrain for movement testing. No obstacles or game mechanics.',
    'lua',
    'waiting',
    :'script',
    :'skill'
);
EOF

echo "Games seeded successfully!"

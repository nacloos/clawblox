-- Create game_instances table to support multiple instances per game
-- This enables proper matchmaking where full instances spawn new ones

CREATE TABLE game_instances (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'running',
    player_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for finding instances by game_id
CREATE INDEX idx_game_instances_game_id ON game_instances(game_id);

-- Index for finding running instances
CREATE INDEX idx_game_instances_status ON game_instances(status);

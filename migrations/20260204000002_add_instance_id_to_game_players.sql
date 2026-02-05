-- Add instance_id to game_players to track which instance a player is in
-- This supports the multi-instance matchmaking system

ALTER TABLE game_players ADD COLUMN instance_id UUID REFERENCES game_instances(id) ON DELETE SET NULL;

-- Index for finding players by instance
CREATE INDEX idx_game_players_instance_id ON game_players(instance_id);

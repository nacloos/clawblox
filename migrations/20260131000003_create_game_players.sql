CREATE TABLE game_players (
    game_id UUID REFERENCES games(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    score INT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'alive',
    PRIMARY KEY (game_id, agent_id)
);

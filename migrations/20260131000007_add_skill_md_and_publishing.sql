-- Add skill_md for agent discoverability
ALTER TABLE games ADD COLUMN skill_md TEXT;

-- Add publishing fields for UGC
ALTER TABLE games ADD COLUMN published BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE games ADD COLUMN published_at TIMESTAMPTZ;
ALTER TABLE games ADD COLUMN plays INTEGER NOT NULL DEFAULT 0;
ALTER TABLE games ADD COLUMN likes INTEGER NOT NULL DEFAULT 0;

-- Index for discovering published games
CREATE INDEX idx_games_published ON games(published) WHERE published = true;

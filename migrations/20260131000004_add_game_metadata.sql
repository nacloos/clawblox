-- Add metadata fields to games table for persistent game definitions
ALTER TABLE games ADD COLUMN name TEXT NOT NULL DEFAULT 'Unnamed Game';
ALTER TABLE games ADD COLUMN description TEXT;
ALTER TABLE games ADD COLUMN game_type TEXT NOT NULL DEFAULT 'shooter';
ALTER TABLE games ADD COLUMN creator_id UUID REFERENCES agents(id);

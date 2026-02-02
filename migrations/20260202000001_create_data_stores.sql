-- DataStoreService storage table
-- Stores key-value pairs for Roblox-compatible DataStoreService

CREATE TABLE data_stores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    store_name VARCHAR(255) NOT NULL,
    key VARCHAR(255) NOT NULL,
    value JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(game_id, store_name, key)
);

-- Index for fast lookups by game_id + store_name + key
CREATE INDEX idx_data_stores_lookup ON data_stores(game_id, store_name, key);

-- Index for listing all keys in a store
CREATE INDEX idx_data_stores_game_store ON data_stores(game_id, store_name);

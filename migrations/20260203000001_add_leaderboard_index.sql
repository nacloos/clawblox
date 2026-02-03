-- Index for sorted JSONB score queries (used by OrderedDataStore leaderboards)
-- Enables efficient GetSortedAsync queries that ORDER BY score DESC/ASC

CREATE INDEX idx_data_stores_score ON data_stores (
    game_id,
    store_name,
    ((value->>'score')::numeric) DESC NULLS LAST
) WHERE value ? 'score';

CREATE TABLE agents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT UNIQUE NOT NULL,
    api_key TEXT UNIQUE NOT NULL,
    description TEXT,
    claim_token TEXT UNIQUE,
    verification_code TEXT,
    status TEXT NOT NULL DEFAULT 'pending_claim',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agents_api_key ON agents(api_key);

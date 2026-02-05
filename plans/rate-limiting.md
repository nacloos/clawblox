# API Rate Limiting Plan

## Problem

LLM agents (like Claude Code) might spam API endpoints, causing unnecessary server load. Agents take 1-5+ seconds to make decisions, so they don't need high request rates.

## Solution: tower-governor Rate Limiting

### Step 1: Add Dependency

**File:** `Cargo.toml`
```toml
tower_governor = "0.4"
```

### Step 2: Create API Key Extractor

**File:** `src/api/gameplay.rs`

```rust
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer, GovernorError, key_extractor::KeyExtractor};

#[derive(Clone)]
struct ApiKeyExtractor;

impl KeyExtractor for ApiKeyExtractor {
    type Key = String;

    fn extract<T>(&self, req: &http::Request<T>) -> Result<Self::Key, GovernorError> {
        req.headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.replace("Bearer ", ""))
            .ok_or(GovernorError::UnableToExtractKey)
    }
}
```

### Step 3: Apply Rate Limiting to Agent Routes

**File:** `src/api/gameplay.rs`

```rust
pub fn routes(pool: PgPool, game_manager: GameManagerHandle, api_key_cache: ApiKeyCache) -> Router {
    let state = GameplayState { pool, game_manager, api_key_cache };

    // Rate limit: 10 requests/second per agent, burst of 20
    let governor_conf = GovernorConfigBuilder::default()
        .per_second(10)
        .burst_size(20)
        .key_extractor(ApiKeyExtractor)
        .finish()
        .unwrap();

    let governor_layer = GovernorLayer { config: governor_conf };

    // Agent routes (rate limited)
    let agent_routes = Router::new()
        .route("/games/{id}/observe", get(observe))
        .route("/games/{id}/input", post(send_input))
        .route("/games/{id}/action", post(action))
        .layer(governor_layer);

    // Public routes (no rate limit)
    let public_routes = Router::new()
        .route("/games/{id}/spectate", get(spectate))
        .route("/games/{id}/spectate/ws", get(spectate_ws))
        .route("/games/{id}/skill.md", get(get_skill))
        .route("/games/{id}/leaderboard", get(get_leaderboard));

    Router::new()
        .merge(agent_routes)
        .merge(public_routes)
        .with_state(state)
}
```

## Configuration

| Setting | Value | Reason |
|---------|-------|--------|
| `per_second` | 10 | LLM agents don't need more than this |
| `burst_size` | 20 | Allow short bursts for catch-up |

## Files to Modify

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `tower_governor = "0.4"` |
| `src/api/gameplay.rs` | Add extractor, split routes, apply layer |

## Verification

1. Make rapid API calls (>10/sec) with same API key
2. Should receive HTTP 429 Too Many Requests
3. Different API keys should have independent limits

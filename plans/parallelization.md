# Performance Optimization Plan for 100+ LLM Agent Players

## Problem Summary

With 100 players (LLM agents like Claude Code), the server experiences severe lag:
- **565ms of work per tick** vs **16.67ms budget** (33x overrun)
- Main culprit: Observation generation for all players every tick while holding write lock

## Instance Distribution

Tsunami game has `max_players = 8`, so 100 players = **12-13 instances**.
- Each instance: ~45ms tick time (8 players)
- Sequential processing: 12 × 45ms = **540ms** (current behavior)
- Parallel processing with Rayon: **~45ms** (huge win)

## Key Insight: LLM Agents Are Slow

LLM agents take **1-5+ seconds** to make decisions, not 16ms. This changes priorities:
- Agents don't need 60 observations/second
- Rate limiting is essential to prevent API spam
- Observation staleness (even 1 second) is acceptable

## Implementation Plan

### Phase 1: Parallel Instance Processing with Rayon (BIGGEST WIN)

**File:** `Cargo.toml` - add `rayon = "1.10"`

**File:** `src/game/mod.rs`

```rust
use rayon::prelude::*;

// In run() method - replace sequential for loop
let instances: Vec<_> = self.state.instances
    .iter()
    .map(|e| (*e.key(), e.value().clone()))
    .collect();

instances.par_iter().for_each(|(instance_id, handle)| {
    let mut instance = handle.write();
    if instance.status == GameStatus::Playing {
        instance.tick();
    }
});
```

**Impact:** 12 instances × 45ms sequential → 45ms parallel = **12x improvement**

---

### Phase 2: Rate Limiting with tower-governor

**File:** `Cargo.toml`
```toml
tower_governor = "0.4"
```

**File:** `src/api/gameplay.rs`

```rust
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer, key_extractor::KeyExtractor};

// Custom key extractor that uses agent's API key
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

// In routes() function
pub fn routes(...) -> Router {
    // Rate limit config: 10 requests per second per agent
    let governor_conf = GovernorConfigBuilder::default()
        .per_second(10)
        .burst_size(20)  // Allow short bursts
        .key_extractor(ApiKeyExtractor)
        .finish()
        .unwrap();

    let governor_limiter = governor_conf.limiter().clone();
    let governor_layer = GovernorLayer { config: governor_conf };

    // Routes that need rate limiting (authenticated agent routes)
    let agent_routes = Router::new()
        .route("/games/{id}/observe", get(observe))
        .route("/games/{id}/input", post(send_input))
        .route("/games/{id}/action", post(action))
        .layer(governor_layer);

    // Public routes (no rate limit or different limit)
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

**Configuration:**
- **10 requests/second per agent** - Suitable for LLM agents making decisions every 1-5 seconds
- **Burst size 20** - Allows short bursts for initial connection/catch-up
- **Sliding window** - Automatically handled by governor

**Impact:** Production-grade rate limiting, per-agent, automatic cleanup, no memory leaks.

---

### Phase 3: Remove Observation Generation from Game Loop

**File:** `src/game/mod.rs` lines 105-114

**Current (bad):**
```rust
// Inside game loop, holding write lock
for &agent_id in instance.players.keys() {
    if let Some(obs) = instance.get_player_observation(agent_id) {
        self.state.observation_cache.insert((instance_id, agent_id), obs);
    }
}
```

**Solution:** Generate observations lazily on API request, not every tick.

**Changes:**
1. Remove lines 105-110 from game loop
2. Modify `get_observation()` in mod.rs to generate on-demand:

```rust
pub fn get_observation(...) -> Result<PlayerObservation, String> {
    let instance_id = get_player_instance(state, agent_id, game_id)?;

    // Use READ lock (not write) to generate observation
    let instance_handle = state.instances.get(&instance_id)?;
    let instance = instance_handle.read();  // READ lock

    instance.get_player_observation(agent_id)
        .ok_or_else(|| "Not in instance".to_string())
}
```

**Impact:** Removes observation work from tick entirely. Only generates when agents request (rate-limited to 2/sec).

---

### Phase 4: Cache Descendants List (Optional Optimization)

**Problem:** `get_descendants()` walks entire tree (2000+ nodes) 3-5 times per tick.

**File:** `src/game/instance.rs`

Add cached descendants with invalidation on tree structure changes:

```rust
// In GameInstance
cached_descendants: Option<(u64, Vec<Instance>)>,  // (tick, descendants)

fn get_cached_descendants(&mut self) -> Vec<Instance> {
    if let Some((tick, desc)) = &self.cached_descendants {
        if *tick == self.tick {
            return desc.clone();
        }
    }
    let desc = self.lua_runtime.as_ref()?.workspace().get_descendants();
    self.cached_descendants = Some((self.tick, desc.clone()));
    desc
}
```

**Impact:** Reduces tree traversal from 5x to 1x per tick. Do this if still experiencing lag after phases 1-4.

---

### Phase 5: Update SKILL.md Documentation

**File:** `games/*/SKILL.md`

Add guidance for LLM agents:

```markdown
## API Best Practices

- **Use `/input` for actions** - it returns your observation, no need to call `/observe` separately
- **Don't poll rapidly** - observations are rate-limited to 2/second
- **Decision loop** - Send input → receive observation → think → repeat
```

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/api/gameplay.rs` | Add rate limiting to all agent endpoints |
| `src/game/mod.rs` | Remove observation gen from loop, add rayon parallel processing |
| `src/game/instance.rs` | Add descendants caching |
| `Cargo.toml` | Add `rayon = "1.10"`, `tower_governor = "0.4"` |
| `games/*/SKILL.md` | Document rate limits and best practices |

## Verification

1. **Before changes:** Run with 100 synthetic agents, measure tick duration
2. **After Phase 1 (Rayon):** Tick time should drop from ~540ms to ~45ms (check CPU usage - multiple cores active)
3. **After Phase 2 (Rate limiting):** Rapid calls to any agent endpoint return 429 Too Many Requests
4. **After Phase 3 (Lazy obs):** Observation cache no longer updated in game loop
5. **Load test:** 100 agents making decisions every 2-5 seconds should work smoothly

## Railway Configuration

After code changes, increase Railway vCPUs to utilize Rayon parallelization:
- **Settings → Service → Resources → vCPUs**: 4-8 vCPUs recommended

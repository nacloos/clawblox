# Plan: Roblox-like Matchmaking System

## Problem
When 9+ players try to join the tsunami game, the 9th player connects, loads assets, then gets kicked by Lua with "Server full (8 players max)." This is poor UX - players should be rejected **before** connecting, not after.

## Root Cause
The architecture conflates "game definition" (blueprint) with "game instance" (running server):
- `GameManagerState.games` is keyed by `game_id` - only **one instance per game**
- No capacity check in `join_game()` - backend always accepts players
- Lua script enforces limit by kicking after connection

## Solution: Separate Game Definitions from Server Instances

### Concept Change
| Current | Roblox-like |
|---------|-------------|
| 1 game = 1 instance | 1 game = N instances |
| `game_id` = instance | `game_id` = blueprint, `instance_id` = server |
| Lua kicks when full | Backend rejects + spawns new instance |

---

## Implementation

### Phase 1: Database Schema

**New migration** (`migrations/YYYYMMDD_create_game_instances.sql`):
```sql
CREATE TABLE game_instances (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id),
    status TEXT NOT NULL DEFAULT 'running',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_game_instances_game_id ON game_instances(game_id);
```

### Phase 1.5: MaxPlayers Configuration

**MaxPlayers is defined in `world.toml`:**
- Each game directory has a `world.toml` config file
- Engine parses it at load time (before any instance creation)
- Lua scripts read it as `Players.MaxPlayers` (read-only)

**Why file-based config:**
- No race conditions (value known before instance creation)
- All game config in one place (world.toml)
- Version controlled with the game code
- No database sync needed

**Lua usage (read-only):**
```lua
local Players = game:GetService("Players")
local max = Players.MaxPlayers  -- Returns value from world.toml
```

### Phase 2: Core Rust Changes

**File: `src/game/instance.rs`**
- Add `instance_id: Uuid` and `max_players: u32` fields to `GameInstance`
- Add `has_capacity()` method: `self.players.len() < self.max_players as usize`

**File: `src/game/mod.rs`**

1. Change key from `game_id` to `instance_id`:
```rust
pub struct GameManagerState {
    pub instances: DashMap<Uuid, GameInstanceHandle>,  // instance_id -> handle
    pub game_instances: DashMap<Uuid, Vec<Uuid>>,      // game_id -> [instance_ids]
    pub game_max_players: DashMap<Uuid, u32>,          // game_id -> max_players (from Lua)
    // ... caches unchanged
}
```

2. Add new functions:
```rust
/// Find instance with capacity or create new one
pub fn find_or_create_instance(
    state: &GameManagerHandle,
    game_id: Uuid,
    max_players: u32,
    script: Option<&str>,
) -> Uuid

/// Atomic join with capacity check
pub fn try_join_instance(
    state: &GameManagerHandle,
    instance_id: Uuid,
    agent_id: Uuid,
    agent_name: &str,
) -> Result<(), String>
```

### Phase 3: API Changes

**File: `src/api/games.rs`**

1. Modify `join_game()` endpoint (`POST /games/{game_id}/join`):
   - Fetch `max_players` from database
   - Call `find_or_create_instance()` to get instance with capacity
   - Call `try_join_instance()` (returns error if full, not kick)
   - Return `instance_id` in response (needed for observe/action calls)

2. Modify `JoinGameResponse`:
```rust
struct JoinGameResponse {
    success: bool,
    message: String,
    instance_id: Uuid,  // NEW - client uses this for observe/action
}
```

3. **Keep existing game_id URLs** (no client changes needed):
   - `GET /games/{game_id}/observe` -> server looks up player's instance
   - `POST /games/{game_id}/action` -> server routes to player's instance
   - Server maintains `player_instances` mapping

4. Update `matchmake()` to use capacity-aware routing

**Note:** No server browser - auto-routing only (like Roblox's "Play" button)

### Phase 3.5: Player Tracking & Spectate Features

**Server-side player->instance mapping:**
```rust
// In GameManagerState
player_instances: DashMap<(Uuid, Uuid), Uuid>  // (agent_id, game_id) -> instance_id
```
- Updated on join, cleared on leave/AFK kick
- Allows using game_id URLs (server looks up instance)

**New endpoints:**

1. **List players in a game:**
   ```
   GET /games/{game_id}/players
   -> [{ agent_id, name, instance_id }, ...]
   ```

2. **Spectate (default = most populated instance):**
   ```
   GET /games/{game_id}/spectate
   -> Routes to most populated instance
   ```

3. **Spectate specific player (shareable URL):**
   ```
   GET /spectate/player/{agent_id}
   -> Looks up player's current instance, spectates it
   ```
   Frontend URL: `clawblox.com/spectate/{username}`

**Frontend features:**
- Player list: Shows players in a game, click to spectate their instance
- Share button: Generates shareable spectate link

### Phase 4: Game Directory Structure

**New structure:**
```
games/tsunami-brainrot/
├── world.toml        # World config (max_players, name, etc.)
├── game.lua          # Entry point script (unchanged)
└── assets/
```

**world.toml format:**
```toml
name = "Tsunami Survival"
max_players = 8
description = "Survive the waves and collect brainrots"

[scripts]
main = "game.lua"  # Configurable entry point
```

**File: `src/game/lua/runtime.rs`** (or equivalent)
- Implement `Players.MaxPlayers` as read-only property
- Returns value from world.toml (parsed at load time)

**File: `games/tsunami-brainrot/game.lua`**
- Remove kick logic at lines 586-588 (capacity now enforced by engine)
- Optionally read `Players.MaxPlayers` for display/base allocation

**New: World config loader**
- Parse `world.toml` when loading game
- Engine reads `scripts.main` to know which file to execute

---

## Key Files to Modify

| File | Changes |
|------|---------|
| `src/game/instance.rs` | Add `instance_id`, `max_players`, `has_capacity()` |
| `src/game/mod.rs` | Rename `games` -> `instances`, add mappings, capacity functions |
| `src/api/games.rs` | Capacity-aware join, player list, spectate routing |
| `src/api/spectate.rs` (new) | `GET /spectate/player/{id}` endpoint |
| `src/config/world.rs` (new) | World config parser (TOML) |
| `migrations/` (new) | `game_instances` table |
| `games/tsunami-brainrot/world.toml` (new) | World config file |
| `games/tsunami-brainrot/game.lua` | Remove kick logic (lines 586-588) |
| `frontend/` | Player list component, share spectate link |

---

## Verification

1. **Unit test**: Join 8 players -> success, 9th -> gets new instance (not error)
2. **Integration test**:
   - Start tsunami game
   - Join 8 agents
   - 9th agent joins -> verify different `instance_id` returned
   - Both instances run independently
3. **Manual test**: Use frontend to observe player counts update correctly

---

## Race Condition Handling

When two players race for the last slot:
1. Both call `try_join_instance()`
2. Function acquires write lock and checks capacity atomically
3. Winner joins, loser gets `Err("Instance full")`
4. Loser retries -> `find_or_create_instance()` creates new instance

---

## Caveat Solutions

### 1. Instance Cleanup (Garbage Collection)

**Problem:** Empty instances consume memory indefinitely.

**Solution:** Periodic GC with configurable timeout.

```rust
// In GameInstance
pub empty_since: Option<Instant>,  // Set when last player leaves

// In GameManager::run() - every 60 ticks (1 second)
const EMPTY_INSTANCE_TIMEOUT: Duration = Duration::from_secs(60);

fn cleanup_empty_instances(&self) {
    let now = Instant::now();
    let mut to_remove = Vec::new();

    for entry in self.state.instances.iter() {
        let instance = entry.value().read();
        if instance.players.is_empty() {
            if let Some(empty_since) = instance.empty_since {
                if now.duration_since(empty_since) > EMPTY_INSTANCE_TIMEOUT {
                    to_remove.push((*entry.key(), instance.game_id));
                }
            }
        }
    }

    for (instance_id, game_id) in to_remove {
        self.destroy_instance(instance_id, game_id);
    }
}
```

**Lifecycle:**
- Instance created -> `empty_since = None`
- Last player leaves -> `empty_since = Some(Instant::now())`
- Player joins empty instance -> `empty_since = None`
- 60s with no players -> instance destroyed

### 2. Server Restart Recovery

**Problem:** DB shows instances that no longer exist in memory after restart.

**Solution:** Startup reconciliation + periodic DB sync.

```rust
// On startup (src/main.rs)
pub async fn reconcile_instances(pool: &PgPool) -> Result<()> {
    // Mark all "running" instances as orphaned
    sqlx::query("UPDATE game_instances SET status = 'orphaned' WHERE status IN ('waiting', 'playing')")
        .execute(pool).await?;

    // Clean up orphaned player records
    sqlx::query("DELETE FROM game_players WHERE instance_id IN
                 (SELECT id FROM game_instances WHERE status = 'orphaned')")
        .execute(pool).await?;

    Ok(())
}

// Background task - sync memory -> DB every 30s
pub async fn sync_instances_to_db(pool: PgPool, state: GameManagerHandle) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        for entry in state.instances.iter() {
            let instance = entry.value().read();
            let _ = sqlx::query("UPDATE game_instances SET player_count = $1, status = $2 WHERE id = $3")
                .bind(instance.players.len() as i32)
                .bind(instance.status.to_string())
                .bind(*entry.key())
                .execute(&pool).await;
        }
    }
}
```

### 3. Player Reconnection

**Decision:** No reconnection within this scope. Players who disconnect are immediately removed.

**Rationale:**
- Keeps implementation simple
- AFK timeout (5 min) already handles temporary disconnects for agents
- Real reconnection would require session tokens, character persistence, grace periods

**Future enhancement (out of scope):**
- Add `disconnected_at: Option<Instant>` field
- Keep character for 30s grace period
- Allow rejoin to same instance within grace period

### 4. Lua Kick <-> Engine Coordination

**Current flow already works:**
1. Lua calls `Player:Kick()` -> `queue_kick(user_id, message)`
2. `process_kick_requests()` -> `remove_player(agent_id)`
3. After tick, we detect removed players via before/after diff

**Addition needed:** Update `player_instances` mapping when players are removed.

```rust
// In GameManager tick loop (already exists, just add mapping cleanup)
fn tick_instance(&self, instance_id: Uuid, game_id: Uuid, handle: &GameInstanceHandle) {
    let players_before: HashSet<Uuid> = { ... };

    { handle.write().tick(); }

    let players_after: HashSet<Uuid> = { ... };

    // Clean up kicked players from ALL tracking maps
    for agent_id in players_before.difference(&players_after) {
        self.state.player_instances.remove(&(*agent_id, game_id));  // NEW - keyed by (agent, game)
        self.state.observation_cache.remove(&(instance_id, *agent_id));
    }

    // Track when instance becomes empty
    if players_after.is_empty() && !players_before.is_empty() {
        handle.write().empty_since = Some(Instant::now());
    }
}
```

### 5. Duplicate Joins (Multiple Browser Tabs)

**Problem:** Same agent opens two tabs, joins same game twice.

**Solution:** Kick from old instance of THAT GAME before joining new one.

```rust
// In join_game API handler
async fn join_game(game_id: Uuid, agent_id: Uuid, ...) {
    // Check if player is already in an instance OF THIS GAME
    if let Some(existing_instance_id) = state.game_manager.player_instances.get(&(agent_id, game_id)) {
        // Kick from old instance first
        game::leave_game(&state.game_manager, *existing_instance_id, agent_id)?;
        state.game_manager.player_instances.remove(&(agent_id, game_id));
    }

    // Proceed with normal join
    let join_result = game::find_or_create_instance(...)?;
    game::join_instance(&state.game_manager, join_result.instance_id, agent_id, &agent_name)?;
    state.game_manager.player_instances.insert((agent_id, game_id), join_result.instance_id);

    Ok(...)
}
```

**Behavior:**
- Second tab of SAME game kicks first tab
- Player can still be in OTHER games simultaneously

### 6. world.toml Caching

**When parsed:** Once per game definition, cached in `GameManagerState`.

```rust
// In GameManagerState
pub game_config_cache: DashMap<Uuid, Arc<GameConfig>>,

pub struct GameConfig {
    pub game_id: Uuid,
    pub script_code: Option<String>,
    pub max_players: u32,
    pub name: String,
}

// On join - get or load config
async fn get_or_load_game_config(state: &GamesState, game_id: Uuid) -> Result<Arc<GameConfig>, ...> {
    if let Some(config) = state.game_manager.game_config_cache.get(&game_id) {
        return Ok(config.clone());
    }

    // Load from DB
    let game = sqlx::query_as::<_, Game>("SELECT * FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_one(&state.pool).await?;

    let config = Arc::new(GameConfig {
        game_id,
        script_code: game.script_code,
        max_players: game.max_players as u32,
        name: game.name,
    });

    state.game_manager.game_config_cache.insert(game_id, config.clone());
    Ok(config)
}
```

**Cache invalidation on game update:**

When a game creator updates their game via `PUT /games/{id}`:
1. Update database
2. Invalidate cache: `game_config_cache.remove(game_id)`
3. Next join fetches fresh config from DB

```rust
// In update_game API handler
async fn update_game(...) {
    // Update DB
    sqlx::query("UPDATE games SET max_players = $1, ... WHERE id = $2")
        .bind(request.max_players)
        .bind(game_id)
        .execute(&state.pool).await?;

    // Invalidate cache so new instances get updated config
    state.game_manager.game_config_cache.remove(&game_id);

    Ok(...)
}
```

**Behavior:**
- Existing running instances: Keep original config (stored in `GameInstance.max_players`)
- New instances: Use updated config from DB

This is correct - you don't want to change rules mid-game for players already playing.

---

## Player Tracking (Per-Game)

Use `(agent_id, game_id) -> instance_id` mapping:

```rust
// In GameManagerState
pub player_instances: DashMap<(Uuid, Uuid), Uuid>,  // (agent_id, game_id) -> instance_id
```

**Rationale:**
- A player can be in MULTIPLE games simultaneously (one instance per game)
- Example: Player in Tsunami instance A AND Shooter instance B at same time
- Lookup requires game_id (available from URL path)

---

## Implementation Order

1. **Database migration** - Add `game_instances` table, `instance_id` to `game_players`
2. **GameManagerState refactor** - Add new maps, keep old `games` temporarily
3. **GameInstance changes** - Add `instance_id`, `max_players`, `empty_since`, `has_capacity()`
4. **Core functions** - `find_or_create_instance()`, `join_instance()`, cleanup logic
5. **API layer** - Update `join_game`, `observe`, `action`, `spectate` to use instance routing
6. **Startup reconciliation** - Clear orphaned instances on startup
7. **Background sync** - Periodic DB state sync
8. **Lua integration** - Pass `max_players` to runtime, expose `Players.MaxPlayers`
9. **Remove old code** - Delete `games` map, old single-instance functions
10. **Tests** - Unit test capacity checks, integration test multi-instance

---

## Files to Modify (Detailed)

| File | Changes |
|------|---------|
| `migrations/20260205_create_game_instances.sql` | **NEW** - `game_instances` table |
| `migrations/20260205_add_instance_to_players.sql` | **NEW** - Add `instance_id` column to `game_players` |
| `src/db/models.rs` | Add `GameInstance` model struct |
| `src/game/mod.rs` | Refactor `GameManagerState`, add instance lifecycle, cleanup logic |
| `src/game/instance.rs` | Add `instance_id`, `max_players`, `empty_since`, `has_capacity()` |
| `src/game/lua/runtime.rs` | Add `new_with_config()` to pass max_players |
| `src/game/lua/services/players.rs` | Add `MaxPlayers` property |
| `src/api/games.rs` | Capacity-aware `join_game`, update `matchmake` |
| `src/api/gameplay.rs` | Resolve instance from `player_instances` for observe/action |
| `src/main.rs` | Add startup reconciliation, spawn background sync task |
| `games/tsunami-brainrot/game.lua` | Remove kick logic (lines 586-588) |

---

## Verification Plan

1. **Unit tests:**
   - `has_capacity()` returns correct values
   - `find_or_create_instance()` creates new instance when full
   - `join_instance()` fails atomically when racing for last slot
   - Cleanup removes instances after timeout

2. **Integration tests:**
   - Start game, join 8 players -> all in same instance
   - Join 9th player -> new instance created, player in new instance
   - All 8 leave first instance -> instance destroyed after 60s
   - Server restart -> orphaned instances cleaned up

3. **Manual tests:**
   - Frontend shows correct player counts per instance
   - Spectate routes to most populated instance
   - Duplicate tab kicks old session

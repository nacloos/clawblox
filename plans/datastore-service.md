# DataStoreService Implementation Plan

## Overview

Implement Roblox-compatible DataStoreService for persistent key-value storage, allowing game scripts to save/load data across sessions.

## Roblox API Reference

### DataStoreService (Roblox)

```lua
-- Get the service
local DataStoreService = game:GetService("DataStoreService")

-- Get a named data store
local playerStore = DataStoreService:GetDataStore("PlayerData")

-- Get a value (YIELDS until data arrives)
local success, data = pcall(function()
    return playerStore:GetAsync("player_123")
end)

-- Set a value (YIELDS until write confirms)
local success = pcall(function()
    playerStore:SetAsync("player_123", {
        coins = 500,
        level = 12,
        inventory = {"sword", "shield"}
    })
end)

-- Atomic update (read-modify-write)
playerStore:UpdateAsync("player_123", function(oldData)
    oldData = oldData or {coins = 0}
    oldData.coins = oldData.coins + 100
    return oldData
end)

-- Delete a key
playerStore:RemoveAsync("player_123")

-- Increment a number
playerStore:IncrementAsync("global_visits", 1)
```

### Key Roblox Behaviors

1. **Yielding**: All Async methods pause the current thread
2. **pcall wrapping**: Recommended because network can fail
3. **Rate limits**: 60 requests/min per key, 100K requests/min per game
4. **Data limits**: 4MB per key
5. **Scope**: Data is per-game (Universe)

## The Sync/Async Problem Explained

### Why This Is Hard

**Clawblox game loop:**
```rust
// Runs on dedicated thread, NOT async
fn tick(&mut self) {
    self.process_actions();           // 1ms
    self.sync_physics();              // 2ms
    self.run_lua_heartbeat();         // must be <10ms!
    self.physics.step();              // 3ms
}
// Total must be <16ms for 60 FPS
```

**Database call:**
```rust
// This is ASYNC - needs tokio runtime
let data = sqlx::query("SELECT * FROM data_stores WHERE key = $1")
    .fetch_one(&pool)
    .await;  // <- waits 5-50ms for network
```

**The conflict:**
- Lua runs inside `tick()` on the game thread
- Database needs `await` which requires async context
- If we `block_on()` inside tick, game freezes

### How Roblox Solves This

Roblox has a **custom Lua scheduler** that:

1. Scripts run in **coroutines** (lightweight threads)
2. When a script calls `GetAsync()`:
   - Coroutine yields (pauses)
   - Engine registers a callback for when data arrives
   - Other scripts keep running
3. When data arrives:
   - Engine resumes the coroutine
   - Script continues from where it paused

```
Script A: local data = store:GetAsync("x")  -- yields
Script B: print("still running!")           -- keeps going
Script A: print(data)                       -- resumes later
```

**Roblox engine is built for this.** They have deep integration between Lua VM and their async I/O system.

### Clawblox Options

**Option 1: Full Coroutine System (Hard)**
- Implement Roblox-style yielding in mlua
- Requires significant runtime changes
- Most compatible but complex

**Option 2: Callback/Event Pattern (Medium)**
- GetAsync doesn't return data directly
- Fires an event when data arrives
- Less Roblox-like but works

```lua
-- Not Roblox-compatible but functional
store:RequestAsync("player_123")
store.DataReceived:Connect(function(key, data)
    if key == "player_123" then
        print(data)
    end
end)
```

**Option 3: Pre-load + Cache (Recommended for MVP)**
- Load ALL game data into memory at start
- Reads are instant (from cache)
- Writes queue up, flush periodically
- Limitation: stale data if multiple servers

```lua
-- Works like Roblox API but reads from cache
local data = store:GetAsync("player_123")  -- instant, from RAM
store:SetAsync("player_123", newData)       -- queued, writes later
```

**Option 4: Sync Blocking (Simple but Risky)**
- Actually wait for DB
- Simple to implement
- Risk: lag spikes if DB is slow

## Challenge

- Lua runs **synchronously** in the game tick thread
- Database calls are **async** (sqlx + tokio)
- Need to bridge sync Lua with async DB

## Architecture

### Option A: Request Queue (Recommended)

```
Lua Script                    Game Thread                  Async Task
    │                             │                            │
    │ SetAsync("key", data)       │                            │
    ├────────────────────────────▶│                            │
    │                             │ Queue write request        │
    │                             ├───────────────────────────▶│
    │                             │                            │ Write to DB
    │ (returns immediately)       │                            │
    │                             │                            │
    │ GetAsync("key")             │                            │
    ├────────────────────────────▶│                            │
    │                             │ Check cache first          │
    │                             │ If miss, return nil        │
    │◀────────────────────────────│ (or queue fetch)           │
```

**Pros:** Non-blocking, Roblox-like behavior
**Cons:** Data might not be immediately available after set

### Option B: Sync Blocking

Use `tokio::runtime::Handle::block_on()` to run async DB calls synchronously.

**Pros:** Simple, data immediately consistent
**Cons:** Blocks game thread, could cause lag spikes

### Recommendation

**Hybrid approach:**
- In-memory cache for fast reads
- Async writes (queue + background flush)
- Sync reads from cache, async cache miss handling

## Database Schema

```sql
-- Migration: 20260201000001_create_data_stores.sql

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

CREATE INDEX idx_data_stores_lookup ON data_stores(game_id, store_name, key);
```

## Rust Implementation

### New Files

```
src/game/lua/services/data_store.rs
```

### DataStoreService

```rust
use crossbeam_channel::{Sender, Receiver};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A pending datastore operation
#[derive(Debug, Clone)]
pub enum DataStoreOp {
    Get {
        store_name: String,
        key: String,
    },
    Set {
        store_name: String,
        key: String,
        value: serde_json::Value,
    },
    Remove {
        store_name: String,
        key: String,
    },
}

/// Result of a datastore operation
#[derive(Debug, Clone)]
pub enum DataStoreResult {
    Value(Option<serde_json::Value>),
    Success,
    Error(String),
}

pub struct DataStoreServiceData {
    pub game_id: Uuid,
    /// In-memory cache: store_name -> key -> value
    pub cache: HashMap<String, HashMap<String, serde_json::Value>>,
    /// Pending write operations
    pub write_queue: Vec<DataStoreOp>,
    /// Whether cache has been loaded from DB
    pub cache_loaded: bool,
}

#[derive(Clone)]
pub struct DataStoreService {
    pub data: Arc<Mutex<DataStoreServiceData>>,
}

impl DataStoreService {
    pub fn new(game_id: Uuid) -> Self {
        Self {
            data: Arc::new(Mutex::new(DataStoreServiceData {
                game_id,
                cache: HashMap::new(),
                write_queue: Vec::new(),
                cache_loaded: false,
            })),
        }
    }

    /// Get a DataStore by name (Roblox API)
    pub fn get_data_store(&self, name: &str) -> DataStore {
        DataStore {
            service: self.clone(),
            name: name.to_string(),
        }
    }

    /// Get value from cache
    pub fn get_cached(&self, store_name: &str, key: &str) -> Option<serde_json::Value> {
        let data = self.data.lock().unwrap();
        data.cache
            .get(store_name)
            .and_then(|store| store.get(key))
            .cloned()
    }

    /// Set value in cache and queue write
    pub fn set(&self, store_name: &str, key: &str, value: serde_json::Value) {
        let mut data = self.data.lock().unwrap();
        
        // Update cache
        data.cache
            .entry(store_name.to_string())
            .or_insert_with(HashMap::new)
            .insert(key.to_string(), value.clone());
        
        // Queue write
        data.write_queue.push(DataStoreOp::Set {
            store_name: store_name.to_string(),
            key: key.to_string(),
            value,
        });
    }

    /// Remove value from cache and queue delete
    pub fn remove(&self, store_name: &str, key: &str) -> Option<serde_json::Value> {
        let mut data = self.data.lock().unwrap();
        
        // Remove from cache
        let old = data.cache
            .get_mut(store_name)
            .and_then(|store| store.remove(key));
        
        // Queue delete
        data.write_queue.push(DataStoreOp::Remove {
            store_name: store_name.to_string(),
            key: key.to_string(),
        });
        
        old
    }

    /// Take pending writes (called by async flusher)
    pub fn take_pending_writes(&self) -> Vec<DataStoreOp> {
        let mut data = self.data.lock().unwrap();
        std::mem::take(&mut data.write_queue)
    }

    /// Load cache from DB results
    pub fn load_cache(&self, entries: Vec<(String, String, serde_json::Value)>) {
        let mut data = self.data.lock().unwrap();
        for (store_name, key, value) in entries {
            data.cache
                .entry(store_name)
                .or_insert_with(HashMap::new)
                .insert(key, value);
        }
        data.cache_loaded = true;
    }
}

/// Individual DataStore (returned by GetDataStore)
#[derive(Clone)]
pub struct DataStore {
    service: DataStoreService,
    name: String,
}

impl UserData for DataStore {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetAsync", |_, this, key: String| {
            Ok(this.service.get_cached(&this.name, &key))
        });

        methods.add_method("SetAsync", |_, this, (key, value): (String, Value)| {
            let json_value = lua_to_json(&value)?;
            this.service.set(&this.name, &key, json_value);
            Ok(())
        });

        methods.add_method("RemoveAsync", |_, this, key: String| {
            Ok(this.service.remove(&this.name, &key))
        });
    }
}

impl UserData for DataStoreService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetDataStore", |_, this, name: String| {
            Ok(this.get_data_store(&name))
        });
    }

    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Name", |_, _| Ok("DataStoreService"));
        fields.add_field_method_get("ClassName", |_, _| Ok("DataStoreService"));
    }
}
```

### Async DB Flusher

```rust
// In game manager or separate task

pub async fn flush_data_stores(
    pool: &PgPool,
    game_id: Uuid,
    data_store: &DataStoreService,
) -> Result<(), sqlx::Error> {
    let ops = data_store.take_pending_writes();
    
    for op in ops {
        match op {
            DataStoreOp::Set { store_name, key, value } => {
                sqlx::query(
                    "INSERT INTO data_stores (game_id, store_name, key, value)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (game_id, store_name, key)
                     DO UPDATE SET value = $4, updated_at = NOW()"
                )
                .bind(game_id)
                .bind(&store_name)
                .bind(&key)
                .bind(&value)
                .execute(pool)
                .await?;
            }
            DataStoreOp::Remove { store_name, key } => {
                sqlx::query(
                    "DELETE FROM data_stores 
                     WHERE game_id = $1 AND store_name = $2 AND key = $3"
                )
                .bind(game_id)
                .bind(&store_name)
                .bind(&key)
                .execute(pool)
                .await?;
            }
            _ => {}
        }
    }
    
    Ok(())
}

pub async fn load_data_store_cache(
    pool: &PgPool,
    game_id: Uuid,
    data_store: &DataStoreService,
) -> Result<(), sqlx::Error> {
    let rows: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT store_name, key, value FROM data_stores WHERE game_id = $1"
    )
    .bind(game_id)
    .fetch_all(pool)
    .await?;
    
    data_store.load_cache(rows);
    Ok(())
}
```

## Integration with GameInstance

### Modify GameInstance

```rust
pub struct GameInstance {
    // ... existing fields
    pub data_store_service: DataStoreService,
}

impl GameInstance {
    pub fn new(game_id: Uuid) -> Self {
        // ...
        Self {
            // ...
            data_store_service: DataStoreService::new(game_id),
        }
    }
}
```

### Modify LuaRuntime

```rust
// In Game::new() or runtime setup
lua.globals().set("DataStoreService", data_store_service.clone())?;

// Or via GetService
methods.add_method("GetService", |lua, this, name: String| {
    match name.as_str() {
        "DataStoreService" => Ok(Value::UserData(lua.create_userdata(
            this.data_model.lock().unwrap().data_store_service.clone()
        )?)),
        // ... other services
    }
});
```

### Periodic Flush

In the game manager loop or via tokio task:

```rust
// Every 30 seconds or on game end
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        
        let state = game_manager.read().unwrap();
        for (game_id, instance) in &state.games {
            if let Err(e) = flush_data_stores(&pool, *game_id, &instance.data_store_service).await {
                eprintln!("DataStore flush error: {}", e);
            }
        }
    }
});
```

## Lua API

### Usage Example

```lua
local DataStoreService = game:GetService("DataStoreService")
local playerData = DataStoreService:GetDataStore("PlayerData")

-- On player join
Players.PlayerAdded:Connect(function(player)
    local data = playerData:GetAsync(tostring(player.UserId))
    if data then
        player.leaderstats.Coins.Value = data.coins or 0
        player.leaderstats.Level.Value = data.level or 1
    end
end)

-- On player leave (or periodic save)
Players.PlayerRemoving:Connect(function(player)
    playerData:SetAsync(tostring(player.UserId), {
        coins = player.leaderstats.Coins.Value,
        level = player.leaderstats.Level.Value,
    })
end)
```

## Implementation Steps

1. **Create migration** - `20260201000001_create_data_stores.sql`
2. **Add DataStoreService** - `src/game/lua/services/data_store.rs`
3. **Export from mod.rs** - Add to services module
4. **Add to GameDataModel** - Create service in runtime
5. **Register in Lua** - Make available via `GetService`
6. **Add flush logic** - Periodic + on game end
7. **Add load logic** - On game start
8. **Wire to GameManager** - Pass pool, handle flush

## Testing

```lua
-- Test script
local DS = game:GetService("DataStoreService")
local store = DS:GetDataStore("TestStore")

store:SetAsync("testKey", {value = 42, name = "test"})
wait(0.1)  -- Let it cache

local data = store:GetAsync("testKey")
print(data.value)  -- 42
print(data.name)   -- "test"

store:RemoveAsync("testKey")
print(store:GetAsync("testKey"))  -- nil
```

## Limitations vs Roblox

- No `UpdateAsync` (atomic read-modify-write) - could add later
- No `IncrementAsync` - could add later
- No OrderedDataStore (sorted leaderboards) - future feature
- No rate limiting - should add for production
- No data versioning - Roblox has this for rollback

## Future Enhancements

1. **OrderedDataStore** - For leaderboards
2. **UpdateAsync** - Atomic updates with retry
3. **Budgets/Rate Limits** - Prevent abuse
4. **Data versioning** - Rollback support
5. **Cross-game DataStore** - Shared universes

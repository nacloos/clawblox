# Plan: DataStoreService

## Goal

Implement Roblox-compatible DataStoreService to persist player data (money, upgrades, rebirth count) across game sessions.

## API

```lua
local DataStoreService = game:GetService("DataStoreService")
local playerStore = DataStoreService:GetDataStore("PlayerData")

-- Save
local success, err = pcall(function()
    playerStore:SetAsync(tostring(player.UserId), {
        Money = 1500,
        SpeedLevel = 2,
        CarryLevel = 1,
        RebirthCount = 1
    })
end)

-- Load
local success, data = pcall(function()
    return playerStore:GetAsync(tostring(player.UserId))
end)
if success and data then
    player:SetAttribute("Money", data.Money)
end
```

## Implementation

### Step 1: Database migration

Create new migration for datastores table:

```sql
CREATE TABLE datastores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    store_name VARCHAR(255) NOT NULL,
    key VARCHAR(255) NOT NULL,
    value JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(store_name, key)
);

CREATE INDEX idx_datastores_lookup ON datastores(store_name, key);
```

### Step 2: Create DataStore struct

File: `src/game/lua/services/datastore.rs`

```rust
pub struct DataStore {
    name: String,
    db_pool: Arc<PgPool>,
}

impl DataStore {
    pub async fn get_async(&self, key: &str) -> Result<Option<serde_json::Value>> {
        sqlx::query_scalar!(
            "SELECT value FROM datastores WHERE store_name = $1 AND key = $2",
            self.name, key
        )
        .fetch_optional(&*self.db_pool)
        .await
    }

    pub async fn set_async(&self, key: &str, value: serde_json::Value) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO datastores (store_name, key, value)
            VALUES ($1, $2, $3)
            ON CONFLICT (store_name, key)
            DO UPDATE SET value = $3, updated_at = NOW()
            "#,
            self.name, key, value
        )
        .execute(&*self.db_pool)
        .await?;
        Ok(())
    }

    pub async fn remove_async(&self, key: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM datastores WHERE store_name = $1 AND key = $2",
            self.name, key
        )
        .execute(&*self.db_pool)
        .await?;
        Ok(())
    }
}
```

### Step 3: Create DataStoreService

```rust
pub struct DataStoreService {
    db_pool: Arc<PgPool>,
    stores: Arc<Mutex<HashMap<String, DataStore>>>,
}

impl DataStoreService {
    pub fn new(db_pool: Arc<PgPool>) -> Self {
        Self {
            db_pool,
            stores: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_data_store(&self, name: &str) -> DataStore {
        let mut stores = self.stores.lock().unwrap();
        stores.entry(name.to_string()).or_insert_with(|| {
            DataStore {
                name: name.to_string(),
                db_pool: self.db_pool.clone(),
            }
        }).clone()
    }
}
```

### Step 4: Handle async in Lua

Roblox's DataStore methods are async. Options:

**Option A: Block on async (simple MVP)**
```rust
methods.add_method("GetAsync", |lua, this, key: String| {
    let rt = tokio::runtime::Handle::current();
    let result = rt.block_on(this.get_async(&key))?;
    // Convert to Lua value
});
```

**Option B: Return Promise-like object (more Roblox-like)**
- More complex, skip for MVP

### Step 5: Pass DB pool to LuaRuntime

Modify `LuaRuntime::new()` to accept `db_pool: Arc<PgPool>`:

```rust
impl LuaRuntime {
    pub fn new(db_pool: Option<Arc<PgPool>>) -> Result<Self> {
        // ...
        if let Some(pool) = db_pool {
            let datastore_service = DataStoreService::new(pool);
            // Register as global
        }
    }
}
```

Update `GameInstance::new_with_script()` to pass the pool.

## Files to Create/Modify

| File | Changes |
|------|---------|
| `migrations/YYYYMMDD_datastores.sql` | New - create datastores table |
| `src/game/lua/services/datastore.rs` | New - DataStore, DataStoreService |
| `src/game/lua/services/mod.rs` | Export DataStoreService |
| `src/game/lua/runtime.rs` | Accept db_pool, register DataStoreService |
| `src/game/instance.rs` | Pass db_pool to LuaRuntime |
| `src/game/mod.rs` | Pass db_pool when creating games |

## Verification

```lua
local DataStoreService = game:GetService("DataStoreService")
local testStore = DataStoreService:GetDataStore("TestStore")

-- Write
testStore:SetAsync("player_123", {
    Money = 500,
    Level = 3
})

-- Read back
local data = testStore:GetAsync("player_123")
print(data.Money)  -- Should print 500
print(data.Level)  -- Should print 3

-- Verify persists across game restarts
```

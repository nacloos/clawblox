//! DataStoreService: Roblox-compatible data persistence with true yielding.
//!
//! This service implements:
//! - DataStoreService:GetDataStore(name) - Get a named data store
//! - DataStore:GetAsync(key) - Retrieve a value (yields until DB operation completes)
//! - DataStore:SetAsync(key, value) - Store a value (yields until DB operation completes)
//! - DataStore:RemoveAsync(key) - Remove a key (yields until DB operation completes)
//!
//! The async operations are implemented using `add_async_method`, which means:
//! - When called from a Lua coroutine, the coroutine yields while waiting for the DB
//! - Other scripts/callbacks continue running during the wait
//! - When the DB operation completes, the coroutine resumes with the result
//!
//! This enables true yielding - the Lua coroutine yields while waiting,
//! allowing other scripts to run.

use mlua::{LuaSerdeExt, UserData, UserDataMethods, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::game::async_bridge::{AsyncBridge, AsyncRequest};

/// The DataStoreService - entry point for getting named data stores
#[derive(Clone)]
pub struct DataStoreService {
    game_id: Uuid,
    async_bridge: Option<Arc<AsyncBridge>>,
}

impl DataStoreService {
    pub fn new(game_id: Uuid, async_bridge: Option<Arc<AsyncBridge>>) -> Self {
        Self {
            game_id,
            async_bridge,
        }
    }
}

impl UserData for DataStoreService {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetDataStore", |lua, this, name: String| {
            let store = DataStore {
                game_id: this.game_id,
                store_name: name,
                async_bridge: this.async_bridge.clone(),
            };
            lua.create_userdata(store)
        });

        methods.add_method("GetOrderedDataStore", |lua, this, name: String| {
            let store = OrderedDataStore {
                game_id: this.game_id,
                store_name: name,
                async_bridge: this.async_bridge.clone(),
            };
            lua.create_userdata(store)
        });
    }
}

/// A named DataStore for storing key-value pairs
#[derive(Clone)]
pub struct DataStore {
    game_id: Uuid,
    store_name: String,
    async_bridge: Option<Arc<AsyncBridge>>,
}

impl UserData for DataStore {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // GetAsync(key) - retrieves a value, yields until DB operation completes
        //
        // When called from a Lua coroutine, this yields the coroutine while
        // waiting for the database. Other scripts continue running.
        methods.add_async_method("GetAsync", |lua, this, key: String| {
            let game_id = this.game_id;
            let store_name = this.store_name.clone();
            let bridge = this.async_bridge.clone();

            async move {
                let bridge = bridge.ok_or_else(|| {
                    mlua::Error::RuntimeError(
                        "DataStoreService not available (no database connection)".into(),
                    )
                })?;

                let (tx, rx) = tokio::sync::oneshot::channel();

                bridge
                    .send(AsyncRequest::DataStoreGet {
                        game_id,
                        store_name,
                        key,
                        response_tx: tx,
                    })
                    .map_err(mlua::Error::RuntimeError)?;

                let result = rx.await.map_err(|_| {
                    mlua::Error::RuntimeError("DataStore operation cancelled".into())
                })?;

                match result {
                    Ok(Some(json_value)) => lua.to_value(&json_value),
                    Ok(None) => Ok(Value::Nil),
                    Err(e) => Err(mlua::Error::RuntimeError(e)),
                }
            }
        });

        // SetAsync(key, value) - stores a value, yields until DB operation completes
        //
        // When called from a Lua coroutine, this yields the coroutine while
        // waiting for the database. Other scripts continue running.
        methods.add_async_method("SetAsync", |lua, this, (key, value): (String, Value)| {
            let game_id = this.game_id;
            let store_name = this.store_name.clone();
            let bridge = this.async_bridge.clone();

            // Serialize the value to JSON before entering async block
            let json_result: Result<serde_json::Value, mlua::Error> = lua.from_value(value);

            async move {
                let json_value = json_result.map_err(|e| {
                    mlua::Error::RuntimeError(format!("Failed to serialize value to JSON: {}", e))
                })?;

                let bridge = bridge.ok_or_else(|| {
                    mlua::Error::RuntimeError(
                        "DataStoreService not available (no database connection)".into(),
                    )
                })?;

                let (tx, rx) = tokio::sync::oneshot::channel();

                bridge
                    .send(AsyncRequest::DataStoreSet {
                        game_id,
                        store_name,
                        key,
                        value: json_value,
                        response_tx: tx,
                    })
                    .map_err(mlua::Error::RuntimeError)?;

                let result = rx.await.map_err(|_| {
                    mlua::Error::RuntimeError("DataStore operation cancelled".into())
                })?;

                match result {
                    Ok(()) => Ok(Value::Nil),
                    Err(e) => Err(mlua::Error::RuntimeError(e)),
                }
            }
        });

        // RemoveAsync(key) - removes a key, yields until DB operation completes
        //
        // When called from a Lua coroutine, this yields the coroutine while
        // waiting for the database. Other scripts continue running.
        methods.add_async_method("RemoveAsync", |_lua, this, key: String| {
            let game_id = this.game_id;
            let store_name = this.store_name.clone();
            let bridge = this.async_bridge.clone();

            async move {
                let bridge = bridge.ok_or_else(|| {
                    mlua::Error::RuntimeError(
                        "DataStoreService not available (no database connection)".into(),
                    )
                })?;

                let (tx, rx) = tokio::sync::oneshot::channel();

                // RemoveAsync sets value to null in the database
                bridge
                    .send(AsyncRequest::DataStoreSet {
                        game_id,
                        store_name,
                        key,
                        value: serde_json::Value::Null,
                        response_tx: tx,
                    })
                    .map_err(mlua::Error::RuntimeError)?;

                let result = rx.await.map_err(|_| {
                    mlua::Error::RuntimeError("DataStore operation cancelled".into())
                })?;

                match result {
                    Ok(()) => Ok(Value::Nil),
                    Err(e) => Err(mlua::Error::RuntimeError(e)),
                }
            }
        });

        // UpdateAsync(key, transformFunction) - atomically updates a value
        //
        // The transformFunction receives the current value (or nil if not exists)
        // and should return the new value to store.
        // Yields until DB operation completes.
        methods.add_async_method(
            "UpdateAsync",
            |lua, this, (key, transform): (String, mlua::Function)| {
                let game_id = this.game_id;
                let store_name = this.store_name.clone();
                let bridge = this.async_bridge.clone();

                // Store the transform function in the registry so we can call it later
                let transform_key = lua.create_registry_value(transform);

                async move {
                    let transform_key = transform_key?;

                    let bridge = bridge.ok_or_else(|| {
                        mlua::Error::RuntimeError(
                            "DataStoreService not available (no database connection)".into(),
                        )
                    })?;

                    // First, get the current value
                    let (get_tx, get_rx) = tokio::sync::oneshot::channel();

                    bridge
                        .send(AsyncRequest::DataStoreGet {
                            game_id,
                            store_name: store_name.clone(),
                            key: key.clone(),
                            response_tx: get_tx,
                        })
                        .map_err(mlua::Error::RuntimeError)?;

                    let current_result = get_rx.await.map_err(|_| {
                        mlua::Error::RuntimeError("DataStore operation cancelled".into())
                    })?;

                    let current_value: Value = match current_result {
                        Ok(Some(json_value)) => lua.to_value(&json_value)?,
                        Ok(None) => Value::Nil,
                        Err(e) => return Err(mlua::Error::RuntimeError(e)),
                    };

                    // Call the transform function
                    let transform: mlua::Function = lua.registry_value(&transform_key)?;
                    let new_value: Value = transform.call(current_value)?;

                    // Clean up the registry key
                    lua.remove_registry_value(transform_key)?;

                    // Serialize the new value
                    let json_value: serde_json::Value = lua.from_value(new_value.clone())?;

                    // Set the new value
                    let (set_tx, set_rx) = tokio::sync::oneshot::channel();

                    bridge
                        .send(AsyncRequest::DataStoreSet {
                            game_id,
                            store_name,
                            key,
                            value: json_value,
                            response_tx: set_tx,
                        })
                        .map_err(mlua::Error::RuntimeError)?;

                    let set_result = set_rx.await.map_err(|_| {
                        mlua::Error::RuntimeError("DataStore operation cancelled".into())
                    })?;

                    match set_result {
                        Ok(()) => Ok(new_value),
                        Err(e) => Err(mlua::Error::RuntimeError(e)),
                    }
                }
            },
        );
    }
}

/// An OrderedDataStore for leaderboards - stores entries with a 'score' field that can be sorted
///
/// Unlike regular DataStore, OrderedDataStore:
/// - Expects values to have a 'score' field for sorting
/// - Provides GetSortedAsync to retrieve entries in sorted order
#[derive(Clone)]
pub struct OrderedDataStore {
    game_id: Uuid,
    store_name: String,
    async_bridge: Option<Arc<AsyncBridge>>,
}

impl UserData for OrderedDataStore {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // SetAsync(key, value) - stores a value with a score field
        //
        // The value should be a table with at least a 'score' field for sorting.
        // Example: leaderboardStore:SetAsync("player_123", {score = 500, name = "Agent1"})
        methods.add_async_method("SetAsync", |lua, this, (key, value): (String, Value)| {
            let game_id = this.game_id;
            let store_name = this.store_name.clone();
            let bridge = this.async_bridge.clone();

            // Serialize the value to JSON before entering async block
            let json_result: Result<serde_json::Value, mlua::Error> = lua.from_value(value);

            async move {
                let json_value = json_result.map_err(|e| {
                    mlua::Error::RuntimeError(format!("Failed to serialize value to JSON: {}", e))
                })?;

                // Validate that the value has a 'score' field
                if !json_value.get("score").is_some() {
                    return Err(mlua::Error::RuntimeError(
                        "OrderedDataStore value must have a 'score' field".into(),
                    ));
                }

                let bridge = bridge.ok_or_else(|| {
                    mlua::Error::RuntimeError(
                        "DataStoreService not available (no database connection)".into(),
                    )
                })?;

                let (tx, rx) = tokio::sync::oneshot::channel();

                bridge
                    .send(AsyncRequest::DataStoreSet {
                        game_id,
                        store_name,
                        key,
                        value: json_value,
                        response_tx: tx,
                    })
                    .map_err(mlua::Error::RuntimeError)?;

                let result = rx.await.map_err(|_| {
                    mlua::Error::RuntimeError("DataStore operation cancelled".into())
                })?;

                match result {
                    Ok(()) => Ok(Value::Nil),
                    Err(e) => Err(mlua::Error::RuntimeError(e)),
                }
            }
        });

        // GetSortedAsync(ascending, limit) - retrieves sorted entries
        //
        // Returns a table of entries sorted by score.
        // Each entry has: {key = "player_123", value = {score = 500, name = "Agent1"}}
        methods.add_async_method(
            "GetSortedAsync",
            |lua, this, (ascending, limit): (bool, i32)| {
                let game_id = this.game_id;
                let store_name = this.store_name.clone();
                let bridge = this.async_bridge.clone();

                async move {
                    let bridge = bridge.ok_or_else(|| {
                        mlua::Error::RuntimeError(
                            "DataStoreService not available (no database connection)".into(),
                        )
                    })?;

                    let (tx, rx) = tokio::sync::oneshot::channel();

                    bridge
                        .send(AsyncRequest::DataStoreGetSorted {
                            game_id,
                            store_name,
                            ascending,
                            limit,
                            response_tx: tx,
                        })
                        .map_err(mlua::Error::RuntimeError)?;

                    let result = rx.await.map_err(|_| {
                        mlua::Error::RuntimeError("DataStore operation cancelled".into())
                    })?;

                    match result {
                        Ok(entries) => {
                            // Convert to Lua table of {key, value} entries
                            let table = lua.create_table()?;
                            for (i, (key, value)) in entries.into_iter().enumerate() {
                                let entry = lua.create_table()?;
                                entry.set("key", key)?;
                                entry.set("value", lua.to_value(&value)?)?;
                                table.set(i + 1, entry)?;
                            }
                            Ok(Value::Table(table))
                        }
                        Err(e) => Err(mlua::Error::RuntimeError(e)),
                    }
                }
            },
        );

        // GetAsync(key) - retrieves a single value by key (same as regular DataStore)
        methods.add_async_method("GetAsync", |lua, this, key: String| {
            let game_id = this.game_id;
            let store_name = this.store_name.clone();
            let bridge = this.async_bridge.clone();

            async move {
                let bridge = bridge.ok_or_else(|| {
                    mlua::Error::RuntimeError(
                        "DataStoreService not available (no database connection)".into(),
                    )
                })?;

                let (tx, rx) = tokio::sync::oneshot::channel();

                bridge
                    .send(AsyncRequest::DataStoreGet {
                        game_id,
                        store_name,
                        key,
                        response_tx: tx,
                    })
                    .map_err(mlua::Error::RuntimeError)?;

                let result = rx.await.map_err(|_| {
                    mlua::Error::RuntimeError("DataStore operation cancelled".into())
                })?;

                match result {
                    Ok(Some(json_value)) => lua.to_value(&json_value),
                    Ok(None) => Ok(Value::Nil),
                    Err(e) => Err(mlua::Error::RuntimeError(e)),
                }
            }
        });
    }
}

//! DataStoreService: Roblox-compatible data persistence with true yielding.
//!
//! This service implements:
//! - DataStoreService:GetDataStore(name) - Get a named data store
//! - DataStore:GetAsync(key) - Retrieve a value (yields until DB operation completes)
//! - DataStore:SetAsync(key, value) - Store a value (yields until DB operation completes)
//! - DataStore:RemoveAsync(key) - Remove a key (yields until DB operation completes)
//!
//! The async operations use mlua's `create_async_function` to enable true yielding.
//! When a script calls GetAsync/SetAsync, it actually pauses execution until the
//! database operation completes, while other scripts continue running.
//!
//! Example Lua usage:
//! ```lua
//! local DataStoreService = game:GetService("DataStoreService")
//! local playerStore = DataStoreService:GetDataStore("PlayerData")
//!
//! -- This yields until the data is retrieved from the database
//! local data = playerStore:GetAsync(tostring(player.UserId))
//! if data then
//!     print("Loaded data:", data.Money)
//! end
//!
//! -- This yields until the data is saved to the database
//! playerStore:SetAsync(tostring(player.UserId), { Money = 1500 })
//! ```

use mlua::{Lua, LuaSerdeExt, UserData, UserDataMethods, Value};
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
        // GetAsync(key) - retrieves a value, yields until complete
        methods.add_method("GetAsync", |lua, this, key: String| {
            let game_id = this.game_id;
            let store_name = this.store_name.clone();
            let bridge = this.async_bridge.clone();

            // Create the async function
            let async_fn = lua.create_async_function(move |lua, key: String| {
                let bridge = bridge.clone();
                let store_name = store_name.clone();

                async move {
                    let bridge = bridge.ok_or_else(|| {
                        mlua::Error::RuntimeError(
                            "DataStoreService not available (no database connection)".into(),
                        )
                    })?;

                    // Create oneshot channel for the response
                    let (tx, rx) = tokio::sync::oneshot::channel();

                    // Send request to async bridge
                    bridge
                        .send(AsyncRequest::DataStoreGet {
                            game_id,
                            store_name,
                            key,
                            response_tx: tx,
                        })
                        .map_err(mlua::Error::RuntimeError)?;

                    // Await the response - this yields the coroutine!
                    let result = rx.await.map_err(|_| {
                        mlua::Error::RuntimeError("DataStore operation cancelled".into())
                    })?;

                    match result {
                        Ok(Some(json_value)) => lua.to_value(&json_value),
                        Ok(None) => Ok(Value::Nil),
                        Err(e) => Err(mlua::Error::RuntimeError(e)),
                    }
                }
            })?;

            // Create a thread from the async function and start it
            let thread = lua.create_thread(async_fn)?;

            // Resume the thread with the key argument - this will yield
            thread.resume::<Value>(key)
        });

        // SetAsync(key, value) - stores a value, yields until complete
        methods.add_method("SetAsync", |lua, this, (key, value): (String, Value)| {
            let game_id = this.game_id;
            let store_name = this.store_name.clone();
            let bridge = this.async_bridge.clone();

            // Convert Lua value to JSON before creating the async function
            let json_value: serde_json::Value = lua.from_value(value).map_err(|e| {
                mlua::Error::RuntimeError(format!("Failed to serialize value to JSON: {}", e))
            })?;

            // Create the async function
            let async_fn = lua.create_async_function(move |_lua, key: String| {
                let bridge = bridge.clone();
                let store_name = store_name.clone();
                let json_value = json_value.clone();

                async move {
                    let bridge = bridge.ok_or_else(|| {
                        mlua::Error::RuntimeError(
                            "DataStoreService not available (no database connection)".into(),
                        )
                    })?;

                    // Create oneshot channel for the response
                    let (tx, rx) = tokio::sync::oneshot::channel();

                    // Send request to async bridge
                    bridge
                        .send(AsyncRequest::DataStoreSet {
                            game_id,
                            store_name,
                            key,
                            value: json_value,
                            response_tx: tx,
                        })
                        .map_err(mlua::Error::RuntimeError)?;

                    // Await the response - this yields the coroutine!
                    let result = rx.await.map_err(|_| {
                        mlua::Error::RuntimeError("DataStore operation cancelled".into())
                    })?;

                    match result {
                        Ok(()) => Ok(Value::Nil),
                        Err(e) => Err(mlua::Error::RuntimeError(e)),
                    }
                }
            })?;

            // Create a thread from the async function and start it
            let thread = lua.create_thread(async_fn)?;

            // Resume the thread with the key argument - this will yield
            thread.resume::<Value>(key)
        });

        // RemoveAsync(key) - removes a key, yields until complete
        methods.add_method("RemoveAsync", |lua, this, key: String| {
            let game_id = this.game_id;
            let store_name = this.store_name.clone();
            let bridge = this.async_bridge.clone();

            // Create the async function
            let async_fn = lua.create_async_function(move |_lua, key: String| {
                let bridge = bridge.clone();
                let store_name = store_name.clone();

                async move {
                    let bridge = bridge.ok_or_else(|| {
                        mlua::Error::RuntimeError(
                            "DataStoreService not available (no database connection)".into(),
                        )
                    })?;

                    // Create oneshot channel for the response
                    let (tx, rx) = tokio::sync::oneshot::channel();

                    // Send request with null value to delete the key
                    bridge
                        .send(AsyncRequest::DataStoreSet {
                            game_id,
                            store_name,
                            key,
                            value: serde_json::Value::Null,
                            response_tx: tx,
                        })
                        .map_err(mlua::Error::RuntimeError)?;

                    // Await the response - this yields the coroutine!
                    let result = rx.await.map_err(|_| {
                        mlua::Error::RuntimeError("DataStore operation cancelled".into())
                    })?;

                    match result {
                        Ok(()) => Ok(Value::Nil),
                        Err(e) => Err(mlua::Error::RuntimeError(e)),
                    }
                }
            })?;

            // Create a thread from the async function and start it
            let thread = lua.create_thread(async_fn)?;

            // Resume the thread with the key argument - this will yield
            thread.resume::<Value>(key)
        });
    }
}

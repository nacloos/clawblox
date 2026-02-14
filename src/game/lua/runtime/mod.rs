use mlua::{
    Function, Lua, MultiValue, ObjectLike, RegistryKey, Result, Thread, ThreadStatus, UserData,
    UserDataMethods, Value,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;

use crate::game::instance::ErrorMode;

use super::animation::AnimationScheduler;
use super::instance::{AttributeValue, Instance, InstanceData};
use super::services::{
    register_overlap_params, register_raycast_params, AgentInput, AgentInputService,
    DataStoreService, DebrisService, HttpService, PhysicsService, PlayersService,
    RemoteEventService, RunService, WorkspaceService,
};
use super::types::register_all_types;
use crate::game::async_bridge::AsyncBridge;

mod coroutines;
mod engine;
mod frame_events;

/// A request to kick a player from the game
#[derive(Debug, Clone)]
pub struct KickRequest {
    pub user_id: u64,
    pub message: Option<String>,
}

#[derive(Default)]
struct BindToCloseCallbacks {
    callbacks: Vec<RegistryKey>,
}

#[derive(Clone)]
struct RuntimeClock {
    start_time: Instant,
    now_secs: Arc<Mutex<f64>>,
}

impl RuntimeClock {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            now_secs: Arc::new(Mutex::new(0.0)),
        }
    }

    fn wall_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    fn now(&self) -> f64 {
        let sim = *self.now_secs.lock().unwrap();
        sim.max(self.wall_secs())
    }

    fn advance(&self, dt_secs: f64) -> f64 {
        let mut now = self.now_secs.lock().unwrap();
        *now += dt_secs.max(0.0);
        (*now).max(self.wall_secs())
    }
}

pub struct GameDataModel {
    pub workspace: WorkspaceService,
    pub players: PlayersService,
    pub run_service: RunService,
    pub agent_input_service: AgentInputService,
    pub data_store_service: DataStoreService,
    pub debris_service: DebrisService,
    pub physics_service: PhysicsService,
    pub remote_event_service: RemoteEventService,
    pub server_script_service: Instance,
    pub replicated_storage: Instance,
    /// Queue of pending kick requests from Lua scripts
    pub kick_requests: Vec<KickRequest>,
}

const DEFAULT_PLAYER_MODEL_URL: &str = "asset://player.glb";

impl GameDataModel {
    pub fn new(game_id: Uuid, async_bridge: Option<Arc<AsyncBridge>>) -> Self {
        Self::with_config(game_id, 100, async_bridge)
    }

    pub fn with_config(
        game_id: Uuid,
        max_players: u32,
        async_bridge: Option<Arc<AsyncBridge>>,
    ) -> Self {
        Self {
            workspace: WorkspaceService::new(),
            players: PlayersService::with_max_players(max_players),
            run_service: RunService::new(true),
            agent_input_service: AgentInputService::new(),
            data_store_service: DataStoreService::new(game_id, async_bridge),
            debris_service: DebrisService::new(),
            physics_service: PhysicsService::new(),
            remote_event_service: RemoteEventService::new(),
            server_script_service: Instance::from_data(InstanceData::new_server_script_service()),
            replicated_storage: Instance::from_data(InstanceData::new_replicated_storage()),
            kick_requests: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct Game {
    pub data_model: Arc<Mutex<GameDataModel>>,
}

impl Game {
    pub fn new(game_id: Uuid, async_bridge: Option<Arc<AsyncBridge>>) -> Self {
        Self::with_config(game_id, 100, async_bridge)
    }

    pub fn with_config(
        game_id: Uuid,
        max_players: u32,
        async_bridge: Option<Arc<AsyncBridge>>,
    ) -> Self {
        Self {
            data_model: Arc::new(Mutex::new(GameDataModel::with_config(
                game_id,
                max_players,
                async_bridge,
            ))),
        }
    }

    pub fn workspace(&self) -> WorkspaceService {
        self.data_model.lock().unwrap().workspace.clone()
    }

    pub fn players(&self) -> PlayersService {
        self.data_model.lock().unwrap().players.clone()
    }

    pub fn run_service(&self) -> RunService {
        self.data_model.lock().unwrap().run_service.clone()
    }

    pub fn agent_input_service(&self) -> AgentInputService {
        self.data_model.lock().unwrap().agent_input_service.clone()
    }

    pub fn data_store_service(&self) -> DataStoreService {
        self.data_model.lock().unwrap().data_store_service.clone()
    }

    pub fn remote_event_service(&self) -> RemoteEventService {
        self.data_model.lock().unwrap().remote_event_service.clone()
    }

    pub fn debris_service(&self) -> DebrisService {
        self.data_model.lock().unwrap().debris_service.clone()
    }

    pub fn physics_service(&self) -> PhysicsService {
        self.data_model.lock().unwrap().physics_service.clone()
    }

    pub fn server_script_service(&self) -> Instance {
        self.data_model
            .lock()
            .unwrap()
            .server_script_service
            .clone()
    }

    pub fn replicated_storage(&self) -> Instance {
        self.data_model.lock().unwrap().replicated_storage.clone()
    }

    /// Queue a kick request for a player (called from Lua Player:Kick())
    pub fn queue_kick(&self, user_id: u64, message: Option<String>) {
        self.data_model
            .lock()
            .unwrap()
            .kick_requests
            .push(KickRequest { user_id, message });
    }

    /// Drain all pending kick requests (called from GameInstance tick)
    pub fn drain_kick_requests(&self) -> Vec<KickRequest> {
        std::mem::take(&mut self.data_model.lock().unwrap().kick_requests)
    }
}

impl UserData for Game {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetService", |lua, this, name: String| {
            let dm = this.data_model.lock().unwrap();
            match name.as_str() {
                "Workspace" => Ok(Value::UserData(lua.create_userdata(dm.workspace.clone())?)),
                "Players" => Ok(Value::UserData(lua.create_userdata(dm.players.clone())?)),
                "RunService" => Ok(Value::UserData(
                    lua.create_userdata(dm.run_service.clone())?,
                )),
                "AgentInputService" => Ok(Value::UserData(
                    lua.create_userdata(dm.agent_input_service.clone())?,
                )),
                "DataStoreService" => Ok(Value::UserData(
                    lua.create_userdata(dm.data_store_service.clone())?,
                )),
                "Debris" | "DebrisService" => Ok(Value::UserData(
                    lua.create_userdata(dm.debris_service.clone())?,
                )),
                "PhysicsService" => Ok(Value::UserData(
                    lua.create_userdata(dm.physics_service.clone())?,
                )),
                "RemoteEventService" => Ok(Value::UserData(
                    lua.create_userdata(dm.remote_event_service.clone())?,
                )),
                "ServerScriptService" => Ok(Value::UserData(
                    lua.create_userdata(dm.server_script_service.clone())?,
                )),
                "ReplicatedStorage" => Ok(Value::UserData(
                    lua.create_userdata(dm.replicated_storage.clone())?,
                )),
                "HttpService" => {
                    drop(dm); // Release lock before creating userdata
                    Ok(Value::UserData(lua.create_userdata(HttpService::new())?))
                }
                _ => Ok(Value::Nil),
            }
        });

        methods.add_meta_method(mlua::MetaMethod::Index, |lua, this, key: String| {
            let dm = this.data_model.lock().unwrap();
            match key.as_str() {
                "Workspace" => Ok(Value::UserData(lua.create_userdata(dm.workspace.clone())?)),
                "Players" => Ok(Value::UserData(lua.create_userdata(dm.players.clone())?)),
                "RunService" => Ok(Value::UserData(
                    lua.create_userdata(dm.run_service.clone())?,
                )),
                "PhysicsService" => Ok(Value::UserData(
                    lua.create_userdata(dm.physics_service.clone())?,
                )),
                "RemoteEventService" => Ok(Value::UserData(
                    lua.create_userdata(dm.remote_event_service.clone())?,
                )),
                "Debris" | "DebrisService" => Ok(Value::UserData(
                    lua.create_userdata(dm.debris_service.clone())?,
                )),
                "ServerScriptService" => Ok(Value::UserData(
                    lua.create_userdata(dm.server_script_service.clone())?,
                )),
                "ReplicatedStorage" => Ok(Value::UserData(
                    lua.create_userdata(dm.replicated_storage.clone())?,
                )),
                _ => Ok(Value::Nil),
            }
        });

        methods.add_method("BindToClose", |lua, _this, callback: Function| {
            let key = lua.create_registry_value(callback)?;
            if let Some(mut existing) = lua.app_data_mut::<BindToCloseCallbacks>() {
                existing.callbacks.push(key);
            } else {
                lua.set_app_data(BindToCloseCallbacks {
                    callbacks: vec![key],
                });
            }
            Ok(())
        });
    }
}

pub struct LuaRuntime {
    lua: Lua,
    game: Game,
    script_loaded: bool,
    /// Tracks yielded coroutines that need to be resumed (e.g., callbacks waiting on DataStore)
    /// Stored as RegistryKeys to prevent garbage collection
    pending_coroutines: Arc<Mutex<Vec<RegistryKey>>>,
    /// Tracks scripts that already executed once
    executed_scripts: Arc<Mutex<HashSet<u64>>>,
    /// Simulation clock used for task scheduling and time APIs (tick/task.wait).
    runtime_clock: RuntimeClock,
}

impl LuaRuntime {
    fn json_to_lua_value(lua: &Lua, value: &serde_json::Value) -> Result<Value> {
        match value {
            serde_json::Value::Null => Ok(Value::Nil),
            serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Number(f))
                } else {
                    Ok(Value::Nil)
                }
            }
            serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
            serde_json::Value::Array(arr) => {
                let table = lua.create_table()?;
                for (i, v) in arr.iter().enumerate() {
                    table.set(i + 1, Self::json_to_lua_value(lua, v)?)?;
                }
                Ok(Value::Table(table))
            }
            serde_json::Value::Object(obj) => {
                let table = lua.create_table()?;
                for (k, v) in obj {
                    table.set(k.as_str(), Self::json_to_lua_value(lua, v)?)?;
                }
                Ok(Value::Table(table))
            }
        }
    }

    fn json_number_to_f32(v: Option<&serde_json::Value>) -> Option<f32> {
        v.and_then(|n| n.as_f64()).map(|n| n as f32)
    }

    fn json_to_remote_event_lua_value(lua: &Lua, value: &serde_json::Value) -> Result<Value> {
        match value {
            serde_json::Value::Object(obj) => {
                let ty = obj
                    .get("$type")
                    .or_else(|| obj.get("__type"))
                    .and_then(|v| v.as_str());

                if let Some(ty) = ty {
                    if ty.eq_ignore_ascii_case("Vector3") {
                        let x = Self::json_number_to_f32(obj.get("x").or_else(|| obj.get("X")));
                        let y = Self::json_number_to_f32(obj.get("y").or_else(|| obj.get("Y")));
                        let z = Self::json_number_to_f32(obj.get("z").or_else(|| obj.get("Z")));
                        if let (Some(x), Some(y), Some(z)) = (x, y, z) {
                            let v3 = super::types::Vector3::new(x, y, z);
                            return Ok(Value::UserData(lua.create_userdata(v3)?));
                        }
                    }
                }

                let table = lua.create_table()?;
                for (k, v) in obj {
                    table.set(k.as_str(), Self::json_to_remote_event_lua_value(lua, v)?)?;
                }
                Ok(Value::Table(table))
            }
            serde_json::Value::Array(arr) => {
                let table = lua.create_table()?;
                for (i, v) in arr.iter().enumerate() {
                    table.set(i + 1, Self::json_to_remote_event_lua_value(lua, v)?)?;
                }
                Ok(Value::Table(table))
            }
            _ => Self::json_to_lua_value(lua, value),
        }
    }

    fn find_remote_event_by_name(&self, name: &str) -> Option<Instance> {
        let replicated_storage = self.game.replicated_storage();
        replicated_storage
            .get_descendants()
            .into_iter()
            .find(|inst| inst.is_a("RemoteEvent") && inst.name() == name)
    }

    /// Creates a new LuaRuntime with optional async bridge for DataStore operations.
    ///
    /// # Arguments
    /// * `game_id` - The unique identifier for this game instance
    /// * `async_bridge` - Optional bridge for async database operations. If None, DataStore
    ///                    operations will return errors.
    pub fn new(game_id: Uuid, async_bridge: Option<Arc<AsyncBridge>>) -> Result<Self> {
        Self::with_config(game_id, 100, async_bridge)
    }

    /// Creates a new LuaRuntime with configuration options.
    ///
    /// # Arguments
    /// * `game_id` - The unique identifier for this game instance
    /// * `max_players` - Maximum number of players (exposed as Players.MaxPlayers in Lua)
    /// * `async_bridge` - Optional bridge for async database operations.
    pub fn with_config(
        game_id: Uuid,
        max_players: u32,
        async_bridge: Option<Arc<AsyncBridge>>,
    ) -> Result<Self> {
        let lua = Lua::new();

        register_all_types(&lua)?;

        super::instance::register_instance(&lua)?;

        register_raycast_params(&lua)?;
        register_overlap_params(&lua)?;

        let game = Game::with_config(game_id, max_players, async_bridge);
        lua.globals().set("game", game.clone())?;
        // Store game reference for internal use (e.g., Player:Kick())
        lua.globals().set("__clawblox_game", game.clone())?;

        lua.globals().set("Workspace", game.workspace())?;
        lua.globals().set("workspace", game.workspace())?;
        lua.globals().set("Players", game.players())?;
        lua.globals().set("RunService", game.run_service())?;
        lua.globals()
            .set("PhysicsService", game.physics_service())?;
        lua.globals()
            .set("ReplicatedStorage", game.replicated_storage())?;
        lua.globals()
            .set("RemoteEventService", game.remote_event_service())?;
        lua.globals()
            .set("ServerScriptService", game.server_script_service())?;

        let print_fn = lua.create_function(|_, args: MultiValue| {
            let msg: Vec<String> = args
                .iter()
                .map(|v| match v {
                    Value::Nil => "nil".to_string(),
                    Value::Boolean(b) => b.to_string(),
                    Value::Integer(n) => n.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                    Value::UserData(ud) => {
                        ud.to_string().unwrap_or_else(|_| "[userdata]".to_string())
                    }
                    _ => format!("{:?}", v),
                })
                .collect();
            println!("[Lua] {}", msg.join("\t"));
            Ok(())
        })?;
        lua.globals().set("print", print_fn)?;

        let warn_fn = lua.create_function(|_, args: MultiValue| {
            let msg: Vec<String> = args
                .iter()
                .map(|v| match v {
                    Value::Nil => "nil".to_string(),
                    Value::Boolean(b) => b.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                    _ => format!("{:?}", v),
                })
                .collect();
            eprintln!("[Lua WARN] {}", msg.join("\t"));
            Ok(())
        })?;
        lua.globals().set("warn", warn_fn)?;

        let typeof_fn = lua.create_function(|_, value: Value| {
            let type_name = match value {
                Value::Nil => "nil",
                Value::Boolean(_) => "boolean",
                Value::Integer(_) | Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Table(_) => "table",
                Value::Function(_) => "function",
                Value::Thread(_) => "thread",
                Value::UserData(ud) => {
                    if ud.borrow::<super::instance::Instance>().is_ok() {
                        "Instance"
                    } else if ud.borrow::<super::types::Vector2>().is_ok() {
                        "Vector2"
                    } else if ud.borrow::<super::types::Vector3>().is_ok() {
                        "Vector3"
                    } else if ud.borrow::<super::types::Ray>().is_ok() {
                        "Ray"
                    } else if ud.borrow::<super::types::PhysicalProperties>().is_ok() {
                        "PhysicalProperties"
                    } else if ud.borrow::<super::types::CFrame>().is_ok() {
                        "CFrame"
                    } else if ud.borrow::<super::types::Color3>().is_ok() {
                        "Color3"
                    } else if ud.borrow::<super::types::UDim>().is_ok() {
                        "UDim"
                    } else if ud.borrow::<super::types::UDim2>().is_ok() {
                        "UDim2"
                    } else {
                        "userdata"
                    }
                }
                Value::LightUserData(_) => "userdata",
                Value::Vector(_) => "vector",
                Value::Buffer(_) => "buffer",
                Value::Error(_) => "error",
                Value::Other(_) => "userdata",
            };
            Ok(type_name)
        })?;
        lua.globals().set("typeof", typeof_fn)?;

        let math_table = lua.globals().get::<mlua::Table>("math")?;
        math_table.set("huge", f64::INFINITY)?;

        let random_fn = lua.create_function(|_, (min, max): (Option<i64>, Option<i64>)| match (
            min, max,
        ) {
            (None, None) => Ok(rand::random::<f64>()),
            (Some(max), None) => Ok((rand::random::<f64>() * max as f64).floor()),
            (Some(min), Some(max)) => {
                let range = (max - min + 1) as f64;
                Ok((rand::random::<f64>() * range).floor() + min as f64)
            }
            _ => Ok(0.0),
        })?;
        math_table.set("random", random_fn)?;

        // Simulation time origin for tick() and task scheduling
        let runtime_clock = RuntimeClock::new();

        // Internal tick implementation used by legacy tick() wrapper in Lua.
        let tick_clock = runtime_clock.clone();
        let tick_fn = lua.create_function(move |_, ()| Ok(tick_clock.now()))?;
        lua.globals().set("__clawblox_tick_impl", tick_fn)?;

        // Internal tables for task scheduling (not accessible to game scripts)
        // Maps thread -> resume_at_time (seconds since start)
        let schedule_table = lua.create_table()?;
        lua.globals()
            .set("__clawblox_thread_schedule", schedule_table)?;
        // Maps thread -> {args} for deferred/delayed threads
        let args_table = lua.create_table()?;
        lua.globals().set("__clawblox_thread_args", args_table)?;
        // Maps thread -> true for cancelled threads
        let cancelled_table = lua.create_table()?;
        lua.globals()
            .set("__clawblox_cancelled_threads", cancelled_table)?;

        // --- task library ---
        // Rust helper: __clawblox_schedule_wait(seconds) — schedules current thread to resume after delay
        let schedule_wait_clock = runtime_clock.clone();
        let schedule_wait_fn = lua.create_function(move |lua, seconds: Option<f64>| {
            let seconds = seconds.unwrap_or(0.0).max(0.0);
            let now = schedule_wait_clock.now();
            let resume_at = now + seconds;
            let thread = lua.current_thread();
            let schedule: mlua::Table = lua.globals().get("__clawblox_thread_schedule")?;
            schedule.set(thread, resume_at)?;
            Ok(())
        })?;
        lua.globals()
            .set("__clawblox_schedule_wait", schedule_wait_fn)?;

        // Rust helper: __clawblox_now() — returns elapsed seconds since start
        let now_clock = runtime_clock.clone();
        let now_fn = lua.create_function(move |_, ()| Ok(now_clock.now()))?;
        lua.globals().set("__clawblox_now", now_fn)?;

        // Register the task table via Lua code that calls Rust helpers
        lua.load(
            r#"
            task = {}
            __clawblox_warned_legacy_tick = false
            __clawblox_warned_legacy_wait = false

            -- Legacy global. Prefer task.wait() or frame events.
            function tick()
                if not __clawblox_warned_legacy_tick then
                    warn("tick() is legacy; prefer task.wait() and RunService events")
                    __clawblox_warned_legacy_tick = true
                end
                return __clawblox_tick_impl()
            end

            function task.spawn(func_or_thread, ...)
                local thread
                if type(func_or_thread) == "thread" then
                    thread = func_or_thread
                else
                    thread = coroutine.create(func_or_thread)
                end
                -- Resume immediately with args
                local ok, err = coroutine.resume(thread, ...)
                if not ok then
                    warn("task.spawn error: " .. tostring(err))
                end
                -- If still yielded, track for future resumption
                if coroutine.status(thread) == "suspended" then
                    __clawblox_track_thread(thread)
                end
                return thread
            end

            function task.delay(seconds, func, ...)
                local thread = coroutine.create(func)
                local schedule = __clawblox_thread_schedule
                local args_tbl = __clawblox_thread_args
                local now = __clawblox_now()
                schedule[thread] = now + seconds
                -- Pack args into a table
                local packed = table.pack(...)
                if packed.n > 0 then
                    args_tbl[thread] = packed
                end
                __clawblox_track_thread(thread)
                return thread
            end

            function task.defer(func_or_thread, ...)
                local thread
                if type(func_or_thread) == "thread" then
                    thread = func_or_thread
                else
                    thread = coroutine.create(func_or_thread)
                end
                -- Store args if any
                local packed = table.pack(...)
                if packed.n > 0 then
                    __clawblox_thread_args[thread] = packed
                end
                __clawblox_track_thread(thread)
                return thread
            end

            function task.wait(seconds)
                seconds = seconds or 0
                local start = __clawblox_now()
                __clawblox_schedule_wait(seconds)
                coroutine.yield()
                return __clawblox_now() - start
            end

            function task.cancel(thread)
                __clawblox_cancelled_threads[thread] = true
                __clawblox_thread_schedule[thread] = nil
                __clawblox_thread_args[thread] = nil
            end

            -- Fix global wait() to use task.wait()
            function wait(seconds)
                if not __clawblox_warned_legacy_wait then
                    warn("wait() is legacy; prefer task.wait()")
                    __clawblox_warned_legacy_wait = true
                end
                return task.wait(seconds)
            end

        "#,
        )
        .exec()?;

        let table_table = lua.globals().get::<mlua::Table>("table")?;

        let insert_fn = lua.create_function(
            |_, (tbl, pos_or_val, val): (mlua::Table, Value, Option<Value>)| {
                match val {
                    Some(v) => {
                        let pos: i64 = match pos_or_val {
                            Value::Number(n) => n as i64,
                            Value::Integer(i) => i,
                            _ => return Err(mlua::Error::runtime("Invalid position")),
                        };
                        tbl.raw_insert(pos, v)?;
                    }
                    None => {
                        let len = tbl.raw_len();
                        tbl.raw_insert(len as i64 + 1, pos_or_val)?;
                    }
                }
                Ok(())
            },
        )?;
        table_table.set("insert", insert_fn)?;

        let remove_fn = lua.create_function(
            |_, (tbl, pos): (mlua::Table, Option<i64>)| -> Result<Value> {
                let pos = pos.unwrap_or(tbl.raw_len() as i64);
                let val = tbl.raw_get(pos)?;
                tbl.raw_remove(pos)?;
                Ok(val)
            },
        )?;
        table_table.set("remove", remove_fn)?;

        let pending_coroutines = Arc::new(Mutex::new(Vec::new()));
        let track_store = pending_coroutines.clone();
        let track_fn = lua.create_function(move |lua, thread: Thread| {
            if thread.status() == ThreadStatus::Resumable {
                let key = lua.create_registry_value(thread)?;
                track_store.lock().unwrap().push(key);
            }
            Ok(())
        })?;
        lua.globals().set("__clawblox_track_thread", track_fn)?;

        let executed_scripts = Arc::new(Mutex::new(HashSet::new()));
        let module_cache = Arc::new(Mutex::new(HashMap::new()));
        let loading_modules = Arc::new(Mutex::new(HashSet::new()));
        lua.set_app_data(AnimationScheduler::default());
        lua.set_app_data(BindToCloseCallbacks::default());

        Self::register_require(
            &lua,
            module_cache.clone(),
            loading_modules.clone(),
            pending_coroutines.clone(),
        )?;

        Ok(Self {
            lua,
            game,
            script_loaded: false,
            pending_coroutines,
            executed_scripts,
            runtime_clock,
        })
    }

    pub fn load_script(&mut self, source: &str) -> Result<()> {
        // Run script in its own coroutine so task.wait() works at the top level
        let func = self.lua.load(source).into_function()?;
        let thread = self.lua.create_thread(func)?;
        match thread.resume::<()>(()) {
            Ok(()) => {
                if thread.status() == ThreadStatus::Resumable {
                    let key = self.lua.create_registry_value(thread)?;
                    self.pending_coroutines.lock().unwrap().push(key);
                }
            }
            Err(e) => {
                if thread.status() == ThreadStatus::Resumable {
                    // Thread yielded through an error path (async operation)
                    let key = self.lua.create_registry_value(thread)?;
                    self.pending_coroutines.lock().unwrap().push(key);
                } else {
                    return Err(e);
                }
            }
        }
        self.script_loaded = true;
        Ok(())
    }

    /// Adds a player and returns (Player instance, HumanoidRootPart lua_id)
    pub fn add_player(&self, user_id: u64, name: &str) -> (Instance, u64) {
        let player = Instance::from_data(InstanceData::new_player(user_id, name));

        // Create character model (Roblox-compatible)
        let character = Instance::from_data(InstanceData::new_model(name));

        // Create HumanoidRootPart as a Cylinder (capsule-like shape for player)
        let mut hrp_data = InstanceData::new_part("HumanoidRootPart");
        if let Some(part) = &mut hrp_data.part_data {
            part.size = super::types::Vector3::new(2.0, 5.0, 2.0); // diameter, height, diameter (R15 scale)
            part.position = super::types::Vector3::new(0.0, 6.0, 0.0); // Spawn above floor
            part.anchored = false;
            part.shape = super::types::PartType::Cylinder;
            part.color = super::types::Color3::new(0.9, 0.45, 0.3); // Orange-reddish player color
        }
        hrp_data.attributes.insert(
            "ModelUrl".to_string(),
            AttributeValue::String(DEFAULT_PLAYER_MODEL_URL.to_string()),
        );
        let hrp = Instance::from_data(hrp_data);
        let hrp_id = hrp.data.lock().unwrap().id.0;
        hrp.set_parent(Some(&character));

        // Create Humanoid
        let humanoid = Instance::from_data(InstanceData::new_humanoid("Humanoid"));
        humanoid.set_parent(Some(&character));

        // Set HumanoidRootPart as PrimaryPart
        {
            let mut char_data = character.data.lock().unwrap();
            if let Some(model) = &mut char_data.model_data {
                model.primary_part = Some(std::sync::Arc::downgrade(&hrp.data));
            }
        }

        // Link character to player
        {
            let mut player_data = player.data.lock().unwrap();
            if let Some(pdata) = &mut player_data.player_data {
                pdata.character = Some(std::sync::Arc::downgrade(&character.data));
            }
        }

        // Parent character to workspace
        self.game.workspace().add_child(character);

        // Create PlayerGui container
        let player_gui = Instance::from_data(InstanceData::new_player_gui("PlayerGui"));
        player_gui.set_parent(Some(&player));

        // Link PlayerGui to player
        {
            let mut player_data = player.data.lock().unwrap();
            if let Some(pdata) = &mut player_data.player_data {
                pdata.player_gui = Some(std::sync::Arc::downgrade(&player_gui.data));
            }
        }

        self.game.players().add_player(player.clone());
        (player, hrp_id)
    }

    pub fn remove_player(&self, user_id: u64) {
        self.game.players().remove_player(user_id);
    }

    pub fn spawn_part(&self, name: &str) -> Instance {
        let part = Instance::from_data(InstanceData::new_part(name));
        self.game.workspace().add_child(part.clone());
        part
    }

    pub fn workspace(&self) -> WorkspaceService {
        self.game.workspace()
    }

    pub fn players(&self) -> PlayersService {
        self.game.players()
    }

    pub fn run_service(&self) -> RunService {
        self.game.run_service()
    }

    pub fn agent_input_service(&self) -> AgentInputService {
        self.game.agent_input_service()
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Queue an agent input for a player
    pub fn queue_agent_input(&self, user_id: u64, input: AgentInput) {
        self.agent_input_service().queue_input(user_id, input);
    }

    /// Process pending agent inputs by firing InputReceived events
    pub fn process_agent_inputs(&self) -> Result<()> {
        let agent_input_service = self.agent_input_service();

        // 1. Collect all (player, inputs) pairs WITHOUT holding locks during Lua calls
        let players = self.players().get_players();
        let mut to_process: Vec<(Instance, Vec<AgentInput>)> = Vec::new();

        for player in players {
            let user_id = {
                let data = player.data.lock().unwrap();
                data.player_data.as_ref().map(|pd| pd.user_id).unwrap_or(0)
            }; // Lock released here

            let inputs = agent_input_service.get_inputs(user_id);
            if !inputs.is_empty() {
                to_process.push((player, inputs));
            }
        }

        // 2. Now fire events (no locks held)
        for (player, inputs) in to_process {
            for input in inputs {
                if input.input_type == "RemoteEvent" {
                    if let Some(event_name) = input.data.get("name").and_then(|v| v.as_str()) {
                        if let Some(remote) = self.find_remote_event_by_name(event_name) {
                            let args = input
                                .data
                                .get("args")
                                .and_then(|v| v.as_array())
                                .map(|items| {
                                    items
                                        .iter()
                                        .map(|v| Self::json_to_remote_event_lua_value(&self.lua, v))
                                        .collect::<Result<Vec<Value>>>()
                                })
                                .transpose()?
                                .unwrap_or_default();
                            remote.fire_remote_on_server_event(&self.lua, &player, args)?;
                        }
                    }
                }
                agent_input_service.fire_input_received(
                    &self.lua,
                    &player,
                    &input.input_type,
                    &input.data,
                )?;
            }
        }

        Ok(())
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn fire_player_added(&self, player: &Instance) -> Result<()> {
        let signal = self
            .game
            .players()
            .data
            .lock()
            .unwrap()
            .player_added
            .clone();
        // Use fire_as_coroutines to allow callbacks to yield (e.g., for DataStore:GetAsync)
        let yielded_threads = signal.fire_as_coroutines(
            &self.lua,
            MultiValue::from_iter([Value::UserData(Instance::cached_userdata(
                &self.lua, player,
            )?)]),
        )?;
        // Track yielded threads for resumption
        self.track_yielded_threads(yielded_threads)?;
        Ok(())
    }

    pub fn fire_player_removing(&self, player: &Instance) -> Result<()> {
        let signal = self
            .game
            .players()
            .data
            .lock()
            .unwrap()
            .player_removing
            .clone();
        // Use fire_as_coroutines to allow callbacks to yield (e.g., for DataStore:SetAsync)
        let yielded_threads = signal.fire_as_coroutines(
            &self.lua,
            MultiValue::from_iter([Value::UserData(Instance::cached_userdata(
                &self.lua, player,
            )?)]),
        )?;
        self.track_yielded_threads(yielded_threads)?;
        Ok(())
    }
}

impl Drop for LuaRuntime {
    fn drop(&mut self) {
        if let Some(callbacks) = self.lua.remove_app_data::<BindToCloseCallbacks>() {
            for key in callbacks.callbacks {
                if let Ok(callback) = self.lua.registry_value::<Function>(&key) {
                    let _ = callback.call::<()>(());
                }
                let _ = self.lua.remove_registry_value(key);
            }
        }
    }
}

#[cfg(test)]
mod tests;

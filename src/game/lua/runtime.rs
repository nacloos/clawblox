use mlua::{Lua, MultiValue, ObjectLike, RegistryKey, Result, Thread, ThreadStatus, UserData, UserDataMethods, Value};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;

use crate::game::instance::ErrorMode;

use super::instance::{AttributeValue, Instance, InstanceData};
use super::services::{
    register_raycast_params, AgentInput, AgentInputService, DataStoreService, HttpService,
    PlayersService, RunService, WorkspaceService,
};
use super::types::register_all_types;
use crate::game::async_bridge::AsyncBridge;

/// A request to kick a player from the game
#[derive(Debug, Clone)]
pub struct KickRequest {
    pub user_id: u64,
    pub message: Option<String>,
}

pub struct GameDataModel {
    pub workspace: WorkspaceService,
    pub players: PlayersService,
    pub run_service: RunService,
    pub agent_input_service: AgentInputService,
    pub data_store_service: DataStoreService,
    /// Queue of pending kick requests from Lua scripts
    pub kick_requests: Vec<KickRequest>,
}

const DEFAULT_PLAYER_MODEL_URL: &str = "/static/models/player.glb";

impl GameDataModel {
    pub fn new(game_id: Uuid, async_bridge: Option<Arc<AsyncBridge>>) -> Self {
        Self::with_config(game_id, 100, async_bridge)
    }

    pub fn with_config(game_id: Uuid, max_players: u32, async_bridge: Option<Arc<AsyncBridge>>) -> Self {
        Self {
            workspace: WorkspaceService::new(),
            players: PlayersService::with_max_players(max_players),
            run_service: RunService::new(true),
            agent_input_service: AgentInputService::new(),
            data_store_service: DataStoreService::new(game_id, async_bridge),
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

    pub fn with_config(game_id: Uuid, max_players: u32, async_bridge: Option<Arc<AsyncBridge>>) -> Self {
        Self {
            data_model: Arc::new(Mutex::new(GameDataModel::with_config(game_id, max_players, async_bridge))),
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
                "RunService" => Ok(Value::UserData(lua.create_userdata(dm.run_service.clone())?)),
                "AgentInputService" => Ok(Value::UserData(
                    lua.create_userdata(dm.agent_input_service.clone())?,
                )),
                "DataStoreService" => Ok(Value::UserData(
                    lua.create_userdata(dm.data_store_service.clone())?,
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
                _ => Ok(Value::Nil),
            }
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
    /// Time when the runtime was created, used for task scheduling
    start_time: Instant,
}

impl LuaRuntime {
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
    pub fn with_config(game_id: Uuid, max_players: u32, async_bridge: Option<Arc<AsyncBridge>>) -> Result<Self> {
        let lua = Lua::new();

        register_all_types(&lua)?;

        super::instance::register_instance(&lua)?;

        register_raycast_params(&lua)?;

        let game = Game::with_config(game_id, max_players, async_bridge);
        lua.globals().set("game", game.clone())?;
        // Store game reference for internal use (e.g., Player:Kick())
        lua.globals().set("__clawblox_game", game.clone())?;

        lua.globals().set("Workspace", game.workspace())?;
        lua.globals().set("Players", game.players())?;

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

        let math_table = lua.globals().get::<mlua::Table>("math")?;
        math_table.set("huge", f64::INFINITY)?;

        let random_fn =
            lua.create_function(|_, (min, max): (Option<i64>, Option<i64>)| match (min, max) {
                (None, None) => Ok(rand::random::<f64>()),
                (Some(max), None) => Ok((rand::random::<f64>() * max as f64).floor()),
                (Some(min), Some(max)) => {
                    let range = (max - min + 1) as f64;
                    Ok((rand::random::<f64>() * range).floor() + min as f64)
                }
                _ => Ok(0.0),
            })?;
        math_table.set("random", random_fn)?;

        // Time origin for tick() and task scheduling
        let start_time = Instant::now();

        // Add tick() global - returns time since game start
        let tick_fn = lua.create_function(move |_, ()| Ok(start_time.elapsed().as_secs_f64()))?;
        lua.globals().set("tick", tick_fn)?;

        // Internal tables for task scheduling (not accessible to game scripts)
        // Maps thread -> resume_at_time (seconds since start)
        let schedule_table = lua.create_table()?;
        lua.globals().set("__clawblox_thread_schedule", schedule_table)?;
        // Maps thread -> {args} for deferred/delayed threads
        let args_table = lua.create_table()?;
        lua.globals().set("__clawblox_thread_args", args_table)?;
        // Maps thread -> true for cancelled threads
        let cancelled_table = lua.create_table()?;
        lua.globals().set("__clawblox_cancelled_threads", cancelled_table)?;

        // --- task library ---
        // Rust helper: __clawblox_schedule_wait(seconds) — schedules current thread to resume after delay
        let schedule_wait_start = start_time;
        let schedule_wait_fn = lua.create_function(move |lua, seconds: Option<f64>| {
            let seconds = seconds.unwrap_or(0.0).max(0.0);
            let now = schedule_wait_start.elapsed().as_secs_f64();
            let resume_at = now + seconds;
            let thread = lua.current_thread();
            let schedule: mlua::Table = lua.globals().get("__clawblox_thread_schedule")?;
            schedule.set(thread, resume_at)?;
            Ok(())
        })?;
        lua.globals().set("__clawblox_schedule_wait", schedule_wait_fn)?;

        // Rust helper: __clawblox_now() — returns elapsed seconds since start
        let now_start = start_time;
        let now_fn = lua.create_function(move |_, ()| {
            Ok(now_start.elapsed().as_secs_f64())
        })?;
        lua.globals().set("__clawblox_now", now_fn)?;

        // Register the task table via Lua code that calls Rust helpers
        lua.load(r#"
            task = {}

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
                return task.wait(seconds)
            end
        "#).exec()?;

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

        let remove_fn =
            lua.create_function(|_, (tbl, pos): (mlua::Table, Option<i64>)| -> Result<Value> {
                let pos = pos.unwrap_or(tbl.raw_len() as i64);
                let val = tbl.raw_get(pos)?;
                tbl.raw_remove(pos)?;
                Ok(val)
            })?;
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

        Ok(Self {
            lua,
            game,
            script_loaded: false,
            pending_coroutines,
            start_time,
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

    pub fn tick(&self, delta_time: f32) -> Result<()> {
        if !self.script_loaded {
            return Ok(());
        }

        // 1. Resume pending coroutines (callbacks that yielded on DataStore operations, etc.)
        self.resume_pending_coroutines()?;

        // 2. Fire Heartbeat as coroutines (allows callbacks to yield)
        let heartbeat = self.game.run_service().heartbeat();
        let yielded_threads = heartbeat.fire_as_coroutines(
            &self.lua,
            MultiValue::from_iter([Value::Number(delta_time as f64)]),
        )?;

        // 3. Track any newly yielded coroutines for resumption on next tick
        self.track_yielded_threads(yielded_threads)?;

        Ok(())
    }

    /// Resumes all pending coroutines and removes completed ones.
    /// Checks cancelled threads, scheduled times, and thread args before resuming.
    fn resume_pending_coroutines(&self) -> Result<()> {
        let now = self.start_time.elapsed().as_secs_f64();

        // Get internal tables
        let schedule: mlua::Table = self.lua.globals().get("__clawblox_thread_schedule")?;
        let args_table: mlua::Table = self.lua.globals().get("__clawblox_thread_args")?;
        let cancelled: mlua::Table = self.lua.globals().get("__clawblox_cancelled_threads")?;

        // Drain all keys upfront so we don't hold a mutable borrow during iteration
        let keys: Vec<RegistryKey> = {
            let mut pending = self.pending_coroutines.lock().unwrap();
            pending.drain(..).collect()
        };

        let mut still_pending = Vec::new();

        for key in keys {
            // Get the thread from the registry
            let thread: Thread = match self.lua.registry_value(&key) {
                Ok(t) => t,
                Err(_) => {
                    // Thread was garbage collected or invalid, clean up
                    let _ = self.lua.remove_registry_value(key);
                    continue;
                }
            };

            // 1. Check if cancelled
            let is_cancelled: bool = cancelled.get(thread.clone()).unwrap_or(false);
            if is_cancelled {
                cancelled.set(thread.clone(), Value::Nil)?;
                schedule.set(thread.clone(), Value::Nil)?;
                args_table.set(thread.clone(), Value::Nil)?;
                let _ = self.lua.remove_registry_value(key);
                continue;
            }

            // Check if thread is still resumable
            if thread.status() != ThreadStatus::Resumable {
                schedule.set(thread.clone(), Value::Nil)?;
                args_table.set(thread.clone(), Value::Nil)?;
                let _ = self.lua.remove_registry_value(key);
                continue;
            }

            // 2. Check schedule — if time hasn't elapsed, keep pending
            let resume_at: Option<f64> = schedule.get(thread.clone())?;
            if let Some(resume_at) = resume_at {
                if now < resume_at {
                    // Not ready yet, keep pending
                    still_pending.push(key);
                    continue;
                }
                // Time satisfied, clear schedule entry
                schedule.set(thread.clone(), Value::Nil)?;
            }

            // 3. Determine resume args
            let resume_args: MultiValue = {
                let stored_args: Value = args_table.get(thread.clone())?;
                match stored_args {
                    Value::Table(tbl) => {
                        args_table.set(thread.clone(), Value::Nil)?;
                        let n: i64 = tbl.get("n").unwrap_or(0);
                        let mut args = Vec::new();
                        for i in 1..=n {
                            let v: Value = tbl.get(i)?;
                            args.push(v);
                        }
                        MultiValue::from_iter(args)
                    }
                    _ => {
                        // For task.wait threads, pass elapsed time
                        if resume_at.is_some() {
                            MultiValue::from_iter([Value::Number(now)])
                        } else {
                            MultiValue::new()
                        }
                    }
                }
            };

            // Try to resume the thread
            match thread.resume::<()>(resume_args) {
                Ok(()) => {
                    // Check if still yielded
                    if thread.status() == ThreadStatus::Resumable {
                        still_pending.push(key);
                    } else {
                        // Thread finished, clean up
                        let _ = self.lua.remove_registry_value(key);
                    }
                }
                Err(e) => {
                    // Thread errored — in Halt mode, propagate immediately
                    let error_mode = self
                        .lua
                        .app_data_ref::<ErrorMode>()
                        .map(|m| *m)
                        .unwrap_or(ErrorMode::Continue);
                    let _ = self.lua.remove_registry_value(key);
                    if error_mode == ErrorMode::Halt {
                        // Put remaining keys back before returning
                        let mut pending = self.pending_coroutines.lock().unwrap();
                        *pending = still_pending;
                        return Err(e);
                    }
                    eprintln!("[LuaRuntime] Coroutine error: {}", e);
                }
            }
        }

        let mut pending = self.pending_coroutines.lock().unwrap();
        *pending = still_pending;
        Ok(())
    }

    /// Tracks yielded threads for resumption on the next tick.
    fn track_yielded_threads(&self, threads: Vec<Thread>) -> Result<()> {
        let mut pending = self.pending_coroutines.lock().unwrap();

        for thread in threads {
            if thread.status() == ThreadStatus::Resumable {
                // Store in registry to prevent garbage collection
                match self.lua.create_registry_value(thread) {
                    Ok(key) => pending.push(key),
                    Err(e) => {
                        eprintln!("[LuaRuntime] Failed to store yielded thread: {}", e);
                    }
                }
            }
        }

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
        let signal = self.game.players().data.lock().unwrap().player_added.clone();
        // Use fire_as_coroutines to allow callbacks to yield (e.g., for DataStore:GetAsync)
        let yielded_threads = signal.fire_as_coroutines(
            &self.lua,
            MultiValue::from_iter([Value::UserData(self.lua.create_userdata(player.clone())?)]),
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
            MultiValue::from_iter([Value::UserData(self.lua.create_userdata(player.clone())?)]),
        )?;
        self.track_yielded_threads(yielded_threads)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test runtime without async bridge
    fn test_runtime() -> LuaRuntime {
        LuaRuntime::new(Uuid::new_v4(), None).expect("Failed to create runtime")
    }

    #[test]
    fn test_runtime_creation() {
        let runtime = test_runtime();
        assert!(!runtime.script_loaded);
    }

    #[test]
    fn test_simple_script() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local part = Instance.new("Part")
            part.Name = "TestPart"
            part.Position = Vector3.new(10, 20, 30)
            part.Parent = Workspace
        "#,
            )
            .expect("Failed to load script");

        let children = runtime.workspace().get_children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name(), "TestPart");
    }

    #[test]
    fn test_heartbeat() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.tickCount = 0
            game:GetService("RunService").Heartbeat:Connect(function(dt)
                _G.tickCount = _G.tickCount + 1
            end)
        "#,
            )
            .expect("Failed to load script");

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0).expect("Failed to tick");
        }

        let tick_count: i64 = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("tickCount")
            .unwrap();
        assert_eq!(tick_count, 10);
    }

    #[test]
    fn test_tick_function() {
        let runtime = test_runtime();
        let result: f64 = runtime.lua().load("return tick()").eval().unwrap();
        assert!(result >= 0.0);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let result2: f64 = runtime.lua().load("return tick()").eval().unwrap();
        assert!(result2 > result);
    }

    #[test]
    fn test_player_has_character() {
        let runtime = test_runtime();
        let (player, hrp_id) = runtime.add_player(12345, "TestPlayer");

        // Verify HRP ID is valid
        assert!(hrp_id > 0);

        // Check player has a character
        let char = player
            .data
            .lock()
            .unwrap()
            .player_data
            .as_ref()
            .unwrap()
            .character
            .clone();
        assert!(char.is_some());

        let char_ref = char.unwrap().upgrade().unwrap();
        let char_inst = Instance::from_ref(char_ref);

        // Check character has HumanoidRootPart
        let hrp = char_inst.find_first_child("HumanoidRootPart", false);
        assert!(hrp.is_some());

        // Check character has Humanoid
        let humanoid = char_inst.find_first_child("Humanoid", false);
        assert!(humanoid.is_some());

        // Check character is in workspace
        assert!(char_inst.parent().is_some());
    }

    #[test]
    fn test_task_spawn() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.spawned = false
            _G.spawnArg = nil
            local thread = task.spawn(function(x)
                _G.spawned = true
                _G.spawnArg = x
            end, 42)
            -- task.spawn runs immediately, so these should already be set
            assert(_G.spawned == true, "task.spawn should run immediately")
            assert(_G.spawnArg == 42, "task.spawn should pass args")
            assert(type(thread) == "thread", "task.spawn should return a thread")
        "#,
            )
            .expect("task.spawn test failed");
    }

    #[test]
    fn test_task_delay() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.delayed = false
            task.delay(0.05, function()
                _G.delayed = true
            end)
        "#,
            )
            .expect("Failed to load script");

        // Should not fire immediately
        let delayed: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("delayed")
            .unwrap();
        assert!(!delayed, "task.delay should not fire immediately");

        // Wait for the delay to elapse
        std::thread::sleep(std::time::Duration::from_millis(60));

        // Tick to trigger resume
        runtime.tick(1.0 / 60.0).expect("Failed to tick");

        let delayed: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("delayed")
            .unwrap();
        assert!(delayed, "task.delay callback should have fired after delay");
    }

    #[test]
    fn test_task_delay_with_args() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.delayedVal = nil
            task.delay(0.05, function(a, b)
                _G.delayedVal = a + b
            end, 10, 20)
        "#,
            )
            .expect("Failed to load script");

        std::thread::sleep(std::time::Duration::from_millis(60));
        runtime.tick(1.0 / 60.0).expect("Failed to tick");

        let val: i64 = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("delayedVal")
            .unwrap();
        assert_eq!(val, 30, "task.delay should forward args to callback");
    }

    #[test]
    fn test_task_wait() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.waitDone = false
            task.spawn(function()
                task.wait(0.05)
                _G.waitDone = true
            end)
        "#,
            )
            .expect("Failed to load script");

        // Should not be done immediately
        let done: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("waitDone")
            .unwrap();
        assert!(!done, "task.wait should yield");

        std::thread::sleep(std::time::Duration::from_millis(60));
        runtime.tick(1.0 / 60.0).expect("Failed to tick");

        let done: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("waitDone")
            .unwrap();
        assert!(done, "task.wait should resume after delay");
    }

    #[test]
    fn test_task_defer() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.deferred = false
            task.defer(function()
                _G.deferred = true
            end)
        "#,
            )
            .expect("Failed to load script");

        // Should not run during load_script
        let deferred: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("deferred")
            .unwrap();
        assert!(!deferred, "task.defer should not run immediately");

        // Should run on next tick
        runtime.tick(1.0 / 60.0).expect("Failed to tick");

        let deferred: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("deferred")
            .unwrap();
        assert!(deferred, "task.defer should run on next tick");
    }

    #[test]
    fn test_task_cancel() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.cancelled = false
            local thread = task.delay(0.05, function()
                _G.cancelled = true
            end)
            task.cancel(thread)
        "#,
            )
            .expect("Failed to load script");

        std::thread::sleep(std::time::Duration::from_millis(60));
        runtime.tick(1.0 / 60.0).expect("Failed to tick");

        let cancelled: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("cancelled")
            .unwrap();
        assert!(
            !cancelled,
            "task.cancel should prevent callback from running"
        );
    }

    #[test]
    fn test_global_wait_delegates_to_task_wait() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.globalWaitDone = false
            task.spawn(function()
                wait(0.05)
                _G.globalWaitDone = true
            end)
        "#,
            )
            .expect("Failed to load script");

        let done: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("globalWaitDone")
            .unwrap();
        assert!(!done, "global wait() should yield");

        std::thread::sleep(std::time::Duration::from_millis(60));
        runtime.tick(1.0 / 60.0).expect("Failed to tick");

        let done: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("globalWaitDone")
            .unwrap();
        assert!(done, "global wait() should resume after delay");
    }
}

use mlua::{Lua, MultiValue, ObjectLike, RegistryKey, Result, Thread, ThreadStatus, UserData, UserDataMethods, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;

use crate::game::instance::ErrorMode;

use super::animation::AnimationScheduler;
use super::instance::{AttributeValue, Instance, InstanceData};
use super::services::{
    register_overlap_params, register_raycast_params, AgentInput, AgentInputService,
    DataStoreService, HttpService,
    PlayersService, RemoteEventService, RunService, WorkspaceService,
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

pub struct GameDataModel {
    pub workspace: WorkspaceService,
    pub players: PlayersService,
    pub run_service: RunService,
    pub agent_input_service: AgentInputService,
    pub data_store_service: DataStoreService,
    pub remote_event_service: RemoteEventService,
    pub server_script_service: Instance,
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
            remote_event_service: RemoteEventService::new(),
            server_script_service: Instance::from_data(InstanceData::new_server_script_service()),
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

    pub fn remote_event_service(&self) -> RemoteEventService {
        self.data_model.lock().unwrap().remote_event_service.clone()
    }

    pub fn server_script_service(&self) -> Instance {
        self.data_model.lock().unwrap().server_script_service.clone()
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
                "RemoteEventService" => Ok(Value::UserData(
                    lua.create_userdata(dm.remote_event_service.clone())?,
                )),
                "ServerScriptService" => {
                    Ok(Value::UserData(lua.create_userdata(dm.server_script_service.clone())?))
                }
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
                "RunService" => Ok(Value::UserData(lua.create_userdata(dm.run_service.clone())?)),
                "RemoteEventService" => Ok(Value::UserData(
                    lua.create_userdata(dm.remote_event_service.clone())?,
                )),
                "ServerScriptService" => Ok(Value::UserData(
                    lua.create_userdata(dm.server_script_service.clone())?,
                )),
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
    /// Tracks scripts that already executed once
    executed_scripts: Arc<Mutex<HashSet<u64>>>,
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
        register_overlap_params(&lua)?;

        let game = Game::with_config(game_id, max_players, async_bridge);
        lua.globals().set("game", game.clone())?;
        // Store game reference for internal use (e.g., Player:Kick())
        lua.globals().set("__clawblox_game", game.clone())?;

        lua.globals().set("Workspace", game.workspace())?;
        lua.globals().set("Players", game.players())?;
        lua.globals().set("RunService", game.run_service())?;
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

        // Internal tick implementation used by legacy tick() wrapper in Lua.
        let tick_fn = lua.create_function(move |_, ()| Ok(start_time.elapsed().as_secs_f64()))?;
        lua.globals().set("__clawblox_tick_impl", tick_fn)?;

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

        let executed_scripts = Arc::new(Mutex::new(HashSet::new()));
        let module_cache = Arc::new(Mutex::new(HashMap::new()));
        let loading_modules = Arc::new(Mutex::new(HashSet::new()));
        lua.set_app_data(AnimationScheduler::default());

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
    fn test_screen_gui_reset_on_spawn_property() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local gui = Instance.new("ScreenGui")
            _G.defaultResetOnSpawn = gui.ResetOnSpawn
            gui.ResetOnSpawn = false
            _G.updatedResetOnSpawn = gui.ResetOnSpawn
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let default_value: bool = globals.get("defaultResetOnSpawn").unwrap();
        let updated_value: bool = globals.get("updatedResetOnSpawn").unwrap();
        assert!(default_value, "ScreenGui.ResetOnSpawn should default to true");
        assert!(
            !updated_value,
            "ScreenGui.ResetOnSpawn should be writable and persist value"
        );
    }

    #[test]
    fn test_uicorner_instance_and_corner_radius_property() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local corner = Instance.new("UICorner")
            _G.defaultCornerScale = corner.CornerRadius.Scale
            _G.defaultCornerOffset = corner.CornerRadius.Offset

            corner.CornerRadius = UDim.new(0, 12)
            _G.updatedCornerScale = corner.CornerRadius.Scale
            _G.updatedCornerOffset = corner.CornerRadius.Offset
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let default_scale: f64 = globals.get("defaultCornerScale").unwrap();
        let default_offset: i64 = globals.get("defaultCornerOffset").unwrap();
        let updated_scale: f64 = globals.get("updatedCornerScale").unwrap();
        let updated_offset: i64 = globals.get("updatedCornerOffset").unwrap();

        assert_eq!(default_scale, 0.0);
        assert_eq!(default_offset, 0);
        assert_eq!(updated_scale, 0.0);
        assert_eq!(updated_offset, 12);
    }

    #[test]
    fn test_humanoid_load_animation_and_track_playback() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local humanoid = Instance.new("Humanoid")
            local animation = Instance.new("Animation")
            animation.AnimationId = "local://fire_rifle"
            local track = humanoid:LoadAnimation(animation)
            _G.track = track
            _G.length = track.Length
            _G.playingBefore = track.IsPlaying
            track:Play()
            _G.playingAfter = track.IsPlaying
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let length: f64 = globals.get("length").unwrap();
        let playing_before: bool = globals.get("playingBefore").unwrap();
        let playing_after: bool = globals.get("playingAfter").unwrap();
        assert!(length > 0.0);
        assert!(!playing_before);
        assert!(playing_after);

        for _ in 0..20 {
            runtime.tick(1.0 / 60.0).expect("Failed to tick");
        }

        let track: mlua::AnyUserData = globals.get("track").unwrap();
        let is_playing: bool = track.get("IsPlaying").unwrap();
        let time_position: f64 = track.get("TimePosition").unwrap();
        assert!(!is_playing, "Track should auto-stop after clip length");
        assert!(time_position > 0.0, "Track time should advance while playing");
    }

    #[test]
    fn test_gui_enum_and_font_properties_for_hud_compat() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local label = Instance.new("TextLabel")
            label.TextXAlignment = Enum.TextXAlignment.Left
            label.TextYAlignment = Enum.TextYAlignment.Top
            label.Font = Enum.Font.GothamBold
            label.TextStrokeTransparency = 0.35

            _G.xAlign = label.TextXAlignment
            _G.yAlign = label.TextYAlignment
            _G.font = label.Font
            _G.stroke = label.TextStrokeTransparency
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let x_align: String = globals.get("xAlign").unwrap();
        let y_align: String = globals.get("yAlign").unwrap();
        let font: String = globals.get("font").unwrap();
        let stroke: f64 = globals.get("stroke").unwrap();

        assert_eq!(x_align, "Left");
        assert_eq!(y_align, "Top");
        assert_eq!(font, "GothamBold");
        assert!((stroke - 0.35).abs() < 1e-6);
    }

    #[test]
    fn test_runservice_available_as_global_and_game_property() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.globalConnected = false
            _G.propertyConnected = false

            if RunService and RunService.Heartbeat then
                _G.globalConnected = true
            end

            if game.RunService and game.RunService.Heartbeat then
                _G.propertyConnected = true
            end
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let global_connected: bool = globals.get("globalConnected").unwrap();
        let property_connected: bool = globals.get("propertyConnected").unwrap();
        assert!(global_connected, "RunService global should be available");
        assert!(property_connected, "game.RunService should be available");
    }

    #[test]
    fn test_stepped_fires_before_heartbeat() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.order = {}
            _G.steppedDt = 0
            _G.heartbeatDt = 0
            game:GetService("RunService").Stepped:Connect(function(time, dt)
                table.insert(_G.order, "S")
                _G.steppedDt = dt
            end)
            game:GetService("RunService").Heartbeat:Connect(function(dt)
                table.insert(_G.order, "H")
                _G.heartbeatDt = dt
            end)
        "#,
            )
            .expect("Failed to load script");

        runtime.tick(1.0 / 60.0).expect("Failed to tick");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let order: mlua::Table = globals.get("order").unwrap();
        let first: String = order.get(1).unwrap();
        let second: String = order.get(2).unwrap();
        assert_eq!(first, "S");
        assert_eq!(second, "H");

        let stepped_dt: f64 = globals.get("steppedDt").unwrap();
        let heartbeat_dt: f64 = globals.get("heartbeatDt").unwrap();
        assert!((stepped_dt - (1.0 / 60.0)).abs() < 1e-6);
        assert!((heartbeat_dt - (1.0 / 60.0)).abs() < 1e-6);
    }

    #[test]
    fn test_signal_wait_yields_and_resumes() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            _G.waitDone = false
            _G.waitDt = 0

            task.spawn(function()
                local dt = game:GetService("RunService").Heartbeat:Wait()
                _G.waitDt = dt
                _G.waitDone = true
            end)
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let done: bool = globals.get("waitDone").unwrap();
        assert!(!done, "Signal:Wait should yield before first heartbeat");

        runtime.tick(1.0 / 60.0).expect("Failed to tick");
        let done: bool = globals.get("waitDone").unwrap();
        assert!(!done, "Signal:Wait should resume on the frame after signal fires");

        runtime.tick(1.0 / 60.0).expect("Failed to tick");
        let done: bool = globals.get("waitDone").unwrap();
        assert!(done, "Signal:Wait should eventually resume");
        let dt: f64 = globals.get("waitDt").unwrap();
        assert!((dt - (1.0 / 60.0)).abs() < 1e-6);
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
    fn test_workspace_raycast_hits_rotated_thin_part() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local bar = Instance.new("Part")
            bar.Name = "ThinBar"
            bar.Size = Vector3.new(12, 2, 1)
            bar.Anchored = true
            bar.CFrame = CFrame.new(0, 2, 0) * CFrame.Angles(0, math.rad(45), 0)
            bar.Parent = Workspace

            local result = Workspace:Raycast(Vector3.new(-10, 2, 4), Vector3.new(25, 0, 0))
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
            )
            .expect("Failed to load script");

        let hit_name: Option<String> = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("hitName")
            .unwrap();
        assert_eq!(hit_name.as_deref(), Some("ThinBar"));
    }

    #[test]
    fn test_workspace_raycast_hits_non_collidable_queryable_part() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local front = Instance.new("Part")
            front.Name = "FrontTrigger"
            front.Size = Vector3.new(4, 4, 4)
            front.Anchored = true
            front.CanCollide = false
            front.Position = Vector3.new(0, 2, 0)
            front.Parent = Workspace

            local back = Instance.new("Part")
            back.Name = "BackWall"
            back.Size = Vector3.new(4, 4, 4)
            back.Anchored = true
            back.Position = Vector3.new(0, 2, 8)
            back.Parent = Workspace

            local result = Workspace:Raycast(Vector3.new(0, 2, -10), Vector3.new(0, 0, 30))
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
            )
            .expect("Failed to load script");

        let hit_name: Option<String> = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("hitName")
            .unwrap();
        assert_eq!(hit_name.as_deref(), Some("FrontTrigger"));
    }

    #[test]
    fn test_workspace_raycast_respects_can_query_false() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local front = Instance.new("Part")
            front.Name = "FrontNoQuery"
            front.Size = Vector3.new(4, 4, 4)
            front.Anchored = true
            front.CanCollide = true
            front.CanQuery = false
            front.Position = Vector3.new(0, 2, 0)
            front.Parent = Workspace

            local back = Instance.new("Part")
            back.Name = "BackWall"
            back.Size = Vector3.new(4, 4, 4)
            back.Anchored = true
            back.Position = Vector3.new(0, 2, 8)
            back.Parent = Workspace

            local result = Workspace:Raycast(Vector3.new(0, 2, -10), Vector3.new(0, 0, 30))
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
            )
            .expect("Failed to load script");

        let hit_name: Option<String> = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("hitName")
            .unwrap();
        assert_eq!(hit_name.as_deref(), Some("BackWall"));
    }

    #[test]
    fn test_workspace_raycast_respect_can_collide_filters_non_collidable() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local front = Instance.new("Part")
            front.Name = "FrontTrigger"
            front.Size = Vector3.new(4, 4, 4)
            front.Anchored = true
            front.CanCollide = false
            front.Position = Vector3.new(0, 2, 0)
            front.Parent = Workspace

            local back = Instance.new("Part")
            back.Name = "BackWall"
            back.Size = Vector3.new(4, 4, 4)
            back.Anchored = true
            back.Position = Vector3.new(0, 2, 8)
            back.Parent = Workspace

            local params = RaycastParams.new()
            params.RespectCanCollide = true
            local result = Workspace:Raycast(Vector3.new(0, 2, -10), Vector3.new(0, 0, 30), params)
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
            )
            .expect("Failed to load script");

        let hit_name: Option<String> = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("hitName")
            .unwrap();
        assert_eq!(hit_name.as_deref(), Some("BackWall"));
    }

    #[test]
    fn test_workspace_raycast_collision_group_filters_parts() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local front = Instance.new("Part")
            front.Name = "FrontBlue"
            front.Size = Vector3.new(4, 4, 4)
            front.Anchored = true
            front.CollisionGroup = "Blue"
            front.Position = Vector3.new(0, 2, 0)
            front.Parent = Workspace

            local back = Instance.new("Part")
            back.Name = "BackRed"
            back.Size = Vector3.new(4, 4, 4)
            back.Anchored = true
            back.CollisionGroup = "Red"
            back.Position = Vector3.new(0, 2, 8)
            back.Parent = Workspace

            local params = RaycastParams.new()
            params.CollisionGroup = "Red"
            local result = Workspace:Raycast(Vector3.new(0, 2, -10), Vector3.new(0, 0, 30), params)
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
            )
            .expect("Failed to load script");

        let hit_name: Option<String> = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("hitName")
            .unwrap();
        assert_eq!(hit_name.as_deref(), Some("BackRed"));
    }

    #[test]
    fn test_get_part_bounds_in_box_uses_volume_overlap() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local p = Instance.new("Part")
            p.Name = "EdgeOverlapPart"
            p.Size = Vector3.new(8, 2, 2)
            p.Anchored = true
            p.Position = Vector3.new(3, 1, 0)
            p.Parent = Workspace

            local hits = Workspace:GetPartBoundsInBox(CFrame.new(0, 1, 0), Vector3.new(4, 4, 4))
            _G.boxHit = false
            for _, v in ipairs(hits) do
                if v.Name == "EdgeOverlapPart" then
                    _G.boxHit = true
                end
            end
        "#,
            )
            .expect("Failed to load script");

        let box_hit: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("boxHit")
            .unwrap();
        assert!(box_hit, "Part overlapping box by extent should be included");
    }

    #[test]
    fn test_get_part_bounds_in_radius_uses_part_extent() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local p = Instance.new("Part")
            p.Name = "RadiusExtentPart"
            p.Size = Vector3.new(8, 2, 2)
            p.Anchored = true
            p.Position = Vector3.new(3, 1, 0)
            p.Parent = Workspace

            local hits = Workspace:GetPartBoundsInRadius(Vector3.new(0, 1, 0), 1.5)
            _G.radiusHit = false
            for _, v in ipairs(hits) do
                if v.Name == "RadiusExtentPart" then
                    _G.radiusHit = true
                end
            end
        "#,
            )
            .expect("Failed to load script");

        let radius_hit: bool = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("radiusHit")
            .unwrap();
        assert!(radius_hit, "Part intersecting sphere by volume should be included");
    }

    #[test]
    fn test_get_part_bounds_in_box_respects_overlap_include_filter() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local folderA = Instance.new("Folder")
            folderA.Name = "FolderA"
            folderA.Parent = Workspace

            local folderB = Instance.new("Folder")
            folderB.Name = "FolderB"
            folderB.Parent = Workspace

            local a = Instance.new("Part")
            a.Name = "InA"
            a.Anchored = true
            a.Size = Vector3.new(2, 2, 2)
            a.Position = Vector3.new(0, 1, 0)
            a.Parent = folderA

            local b = Instance.new("Part")
            b.Name = "InB"
            b.Anchored = true
            b.Size = Vector3.new(2, 2, 2)
            b.Position = Vector3.new(0, 1, 0)
            b.Parent = folderB

            local params = OverlapParams.new()
            params.FilterType = Enum.RaycastFilterType.Include
            params.FilterDescendantsInstances = {folderA}

            local hits = Workspace:GetPartBoundsInBox(CFrame.new(0, 1, 0), Vector3.new(6, 6, 6), params)
            _G.hitA = false
            _G.hitB = false
            for _, v in ipairs(hits) do
                if v.Name == "InA" then _G.hitA = true end
                if v.Name == "InB" then _G.hitB = true end
            end
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let hit_a: bool = globals.get("hitA").unwrap();
        let hit_b: bool = globals.get("hitB").unwrap();
        assert!(hit_a, "Included folder part should be returned");
        assert!(!hit_b, "Excluded folder part should be filtered out");
    }

    #[test]
    fn test_get_part_bounds_in_radius_respects_overlap_max_parts() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local p1 = Instance.new("Part")
            p1.Name = "P1"
            p1.Anchored = true
            p1.Size = Vector3.new(2, 2, 2)
            p1.Position = Vector3.new(0, 1, 0)
            p1.Parent = Workspace

            local p2 = Instance.new("Part")
            p2.Name = "P2"
            p2.Anchored = true
            p2.Size = Vector3.new(2, 2, 2)
            p2.Position = Vector3.new(1, 1, 0)
            p2.Parent = Workspace

            local params = OverlapParams.new()
            params.MaxParts = 1

            local hits = Workspace:GetPartBoundsInRadius(Vector3.new(0, 1, 0), 4.0, params)
            _G.hitCount = #hits
        "#,
            )
            .expect("Failed to load script");

        let hit_count: i64 = runtime
            .lua()
            .globals()
            .get::<mlua::Table>("_G")
            .unwrap()
            .get("hitCount")
            .unwrap();
        assert_eq!(hit_count, 1, "MaxParts should cap overlap query result size");
    }

    #[test]
    fn test_get_part_bounds_in_radius_overlap_respects_can_query_false() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local q = Instance.new("Part")
            q.Name = "QueryablePart"
            q.Anchored = true
            q.Size = Vector3.new(2, 2, 2)
            q.Position = Vector3.new(0, 1, 0)
            q.Parent = Workspace

            local nq = Instance.new("Part")
            nq.Name = "NoQueryPart"
            nq.Anchored = true
            nq.Size = Vector3.new(2, 2, 2)
            nq.Position = Vector3.new(0, 1, 1)
            nq.CanQuery = false
            nq.Parent = Workspace

            local hits = Workspace:GetPartBoundsInRadius(Vector3.new(0, 1, 0), 4.0)
            _G.hitQueryable = false
            _G.hitNoQuery = false
            for _, v in ipairs(hits) do
                if v.Name == "QueryablePart" then _G.hitQueryable = true end
                if v.Name == "NoQueryPart" then _G.hitNoQuery = true end
            end
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let hit_queryable: bool = globals.get("hitQueryable").unwrap();
        let hit_no_query: bool = globals.get("hitNoQuery").unwrap();
        assert!(hit_queryable);
        assert!(!hit_no_query);
    }

    #[test]
    fn test_get_part_bounds_in_box_overlap_respect_can_collide() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local trigger = Instance.new("Part")
            trigger.Name = "Trigger"
            trigger.Anchored = true
            trigger.Size = Vector3.new(2, 2, 2)
            trigger.Position = Vector3.new(0, 1, 0)
            trigger.CanCollide = false
            trigger.Parent = Workspace

            local solid = Instance.new("Part")
            solid.Name = "Solid"
            solid.Anchored = true
            solid.Size = Vector3.new(2, 2, 2)
            solid.Position = Vector3.new(0, 1, 1)
            solid.Parent = Workspace

            local params = OverlapParams.new()
            params.RespectCanCollide = true
            local hits = Workspace:GetPartBoundsInBox(CFrame.new(0, 1, 0), Vector3.new(6, 6, 6), params)
            _G.hitTrigger = false
            _G.hitSolid = false
            for _, v in ipairs(hits) do
                if v.Name == "Trigger" then _G.hitTrigger = true end
                if v.Name == "Solid" then _G.hitSolid = true end
            end
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let hit_trigger: bool = globals.get("hitTrigger").unwrap();
        let hit_solid: bool = globals.get("hitSolid").unwrap();
        assert!(!hit_trigger, "RespectCanCollide should filter CanCollide=false parts");
        assert!(hit_solid, "Solid part should still be included");
    }

    #[test]
    fn test_get_part_bounds_in_box_overlap_collision_group_filters_parts() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local blue = Instance.new("Part")
            blue.Name = "BluePart"
            blue.Anchored = true
            blue.Size = Vector3.new(2, 2, 2)
            blue.CollisionGroup = "Blue"
            blue.Position = Vector3.new(0, 1, 0)
            blue.Parent = Workspace

            local red = Instance.new("Part")
            red.Name = "RedPart"
            red.Anchored = true
            red.Size = Vector3.new(2, 2, 2)
            red.CollisionGroup = "Red"
            red.Position = Vector3.new(0, 1, 1)
            red.Parent = Workspace

            local params = OverlapParams.new()
            params.CollisionGroup = "Red"
            local hits = Workspace:GetPartBoundsInBox(CFrame.new(0, 1, 0), Vector3.new(6, 6, 6), params)
            _G.hitBlue = false
            _G.hitRed = false
            for _, v in ipairs(hits) do
                if v.Name == "BluePart" then _G.hitBlue = true end
                if v.Name == "RedPart" then _G.hitRed = true end
            end
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let hit_blue: bool = globals.get("hitBlue").unwrap();
        let hit_red: bool = globals.get("hitRed").unwrap();
        assert!(!hit_blue);
        assert!(hit_red);
    }

    #[test]
    fn test_get_parts_in_part_returns_overlapping_parts() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local query = Instance.new("Part")
            query.Name = "Query"
            query.Anchored = true
            query.Size = Vector3.new(4, 4, 4)
            query.Position = Vector3.new(0, 1, 0)
            query.Parent = Workspace

            local near = Instance.new("Part")
            near.Name = "Near"
            near.Anchored = true
            near.Size = Vector3.new(2, 2, 2)
            near.Position = Vector3.new(1, 1, 0)
            near.Parent = Workspace

            local far = Instance.new("Part")
            far.Name = "Far"
            far.Anchored = true
            far.Size = Vector3.new(2, 2, 2)
            far.Position = Vector3.new(20, 1, 0)
            far.Parent = Workspace

            local hits = Workspace:GetPartsInPart(query)
            _G.hitNear = false
            _G.hitFar = false
            _G.hitQuery = false
            for _, v in ipairs(hits) do
                if v.Name == "Near" then _G.hitNear = true end
                if v.Name == "Far" then _G.hitFar = true end
                if v.Name == "Query" then _G.hitQuery = true end
            end
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let hit_near: bool = globals.get("hitNear").unwrap();
        let hit_far: bool = globals.get("hitFar").unwrap();
        let hit_query: bool = globals.get("hitQuery").unwrap();
        assert!(hit_near);
        assert!(!hit_far);
        assert!(!hit_query);
    }

    #[test]
    fn test_get_parts_in_part_respects_overlap_params() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local query = Instance.new("Part")
            query.Name = "Query"
            query.Anchored = true
            query.Size = Vector3.new(4, 4, 4)
            query.Position = Vector3.new(0, 1, 0)
            query.Parent = Workspace

            local red = Instance.new("Part")
            red.Name = "RedSolid"
            red.Anchored = true
            red.Size = Vector3.new(2, 2, 2)
            red.CollisionGroup = "Red"
            red.Position = Vector3.new(1, 1, 0)
            red.Parent = Workspace

            local blue = Instance.new("Part")
            blue.Name = "BlueTrigger"
            blue.Anchored = true
            blue.Size = Vector3.new(2, 2, 2)
            blue.CollisionGroup = "Blue"
            blue.CanCollide = false
            blue.Position = Vector3.new(-1, 1, 0)
            blue.Parent = Workspace

            local params = OverlapParams.new()
            params.CollisionGroup = "Red"
            params.RespectCanCollide = true
            local hits = Workspace:GetPartsInPart(query, params)

            _G.hitRed = false
            _G.hitBlue = false
            for _, v in ipairs(hits) do
                if v.Name == "RedSolid" then _G.hitRed = true end
                if v.Name == "BlueTrigger" then _G.hitBlue = true end
            end
        "#,
            )
            .expect("Failed to load script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let hit_red: bool = globals.get("hitRed").unwrap();
        let hit_blue: bool = globals.get("hitBlue").unwrap();
        assert!(hit_red);
        assert!(!hit_blue);
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

    #[test]
    fn test_instance_new_unknown_class_errors() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local ok, err = pcall(function()
                Instance.new("DefinitelyUnknownClass")
            end)
            _G.ok = ok
            _G.err = tostring(err)
        "#,
            )
            .expect("Failed to run script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let ok: bool = globals.get("ok").unwrap();
        let err: String = globals.get("err").unwrap();
        assert!(!ok);
        assert!(err.contains("Unknown class"));
    }

    #[test]
    fn test_attribute_and_property_changed_signals() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local part = Instance.new("Part")
            part.Parent = Workspace

            _G.attrCount = 0
            _G.propCount = 0

            part:GetAttributeChangedSignal("Coins"):Connect(function()
                _G.attrCount = _G.attrCount + 1
            end)

            part:GetPropertyChangedSignal("Name"):Connect(function()
                _G.propCount = _G.propCount + 1
            end)

            part:SetAttribute("Coins", 1)
            part.Name = "ChangedName"
        "#,
            )
            .expect("Failed to run script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let attr_count: i64 = globals.get("attrCount").unwrap();
        let prop_count: i64 = globals.get("propCount").unwrap();
        assert_eq!(attr_count, 1);
        assert_eq!(prop_count, 1);
    }

    #[test]
    fn test_wait_for_child_returns_child() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local folder = Instance.new("Folder")
            folder.Name = "Container"
            folder.Parent = Workspace

            _G.childName = nil

            task.spawn(function()
                local child = folder:WaitForChild("LatePart", 1.0)
                _G.childName = child and child.Name or "nil"
            end)

            task.delay(0.05, function()
                local part = Instance.new("Part")
                part.Name = "LatePart"
                part.Parent = folder
            end)
        "#,
            )
            .expect("Failed to run script");

        std::thread::sleep(std::time::Duration::from_millis(70));
        runtime.tick(1.0 / 60.0).unwrap();
        runtime.tick(1.0 / 60.0).unwrap();

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let child_name: String = globals.get("childName").unwrap();
        assert_eq!(child_name, "LatePart");
    }

    #[test]
    fn test_require_caches_module_result() {
        let mut runtime = test_runtime();
        runtime
            .load_script(
                r#"
            local m = Instance.new("ModuleScript")
            m.Source = "_G.runCount = (_G.runCount or 0) + 1; return { value = _G.runCount }"

            local a = require(m)
            local b = require(m)

            _G.runCount = _G.runCount
            _G.sameRef = (a == b)
            _G.value = a.value
        "#,
            )
            .expect("Failed to run script");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let run_count: i64 = globals.get("runCount").unwrap();
        let same_ref: bool = globals.get("sameRef").unwrap();
        let value: i64 = globals.get("value").unwrap();

        assert_eq!(run_count, 1);
        assert!(same_ref);
        assert_eq!(value, 1);
    }

    #[test]
    fn test_server_script_service_discovers_and_runs_scripts() {
        let runtime = test_runtime();

        let script = Instance::from_data(InstanceData::new_script("BootScript"));
        {
            let mut data = script.data.lock().unwrap();
            data.script_data.as_mut().unwrap().source = "_G.serviceBootRan = true".to_string();
        }

        let sss = runtime.game().server_script_service();
        script.set_parent(Some(&sss));

        runtime.tick(1.0 / 60.0).expect("Failed to tick");

        let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
        let ran: bool = globals.get("serviceBootRan").unwrap();
        assert!(ran);
    }
}

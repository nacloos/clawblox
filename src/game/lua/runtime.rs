use mlua::{Lua, MultiValue, ObjectLike, RegistryKey, Result, Thread, ThreadStatus, UserData, UserDataMethods, Value};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;

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
        Self {
            workspace: WorkspaceService::new(),
            players: PlayersService::new(),
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
        Self {
            data_model: Arc::new(Mutex::new(GameDataModel::new(game_id, async_bridge))),
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
}

impl LuaRuntime {
    /// Creates a new LuaRuntime with optional async bridge for DataStore operations.
    ///
    /// # Arguments
    /// * `game_id` - The unique identifier for this game instance
    /// * `async_bridge` - Optional bridge for async database operations. If None, DataStore
    ///                    operations will return errors.
    pub fn new(game_id: Uuid, async_bridge: Option<Arc<AsyncBridge>>) -> Result<Self> {
        let lua = Lua::new();

        register_all_types(&lua)?;

        super::instance::register_instance(&lua)?;

        register_raycast_params(&lua)?;

        let game = Game::new(game_id, async_bridge);
        lua.globals().set("game", game.clone())?;
        // Store game reference for internal use (e.g., Player:Kick())
        lua.globals().set("__clawblox_game", game.clone())?;

        lua.globals().set("Workspace", game.workspace())?;
        lua.globals().set("Players", game.players())?;

        let wait_fn = lua.create_function(|_, seconds: Option<f64>| {
            let _seconds = seconds.unwrap_or(0.0);
            Ok(())
        })?;
        lua.globals().set("wait", wait_fn)?;

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

        // Add tick() global - returns time since game start
        let start_time = Instant::now();
        let tick_fn = lua.create_function(move |_, ()| Ok(start_time.elapsed().as_secs_f64()))?;
        lua.globals().set("tick", tick_fn)?;

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
        })
    }

    pub fn load_script(&mut self, source: &str) -> Result<()> {
        self.lua.load(source).exec()?;
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
    fn resume_pending_coroutines(&self) -> Result<()> {
        let mut pending = self.pending_coroutines.lock().unwrap();
        let mut still_pending = Vec::new();

        for key in pending.drain(..) {
            // Get the thread from the registry
            let thread: Thread = match self.lua.registry_value(&key) {
                Ok(t) => t,
                Err(_) => {
                    // Thread was garbage collected or invalid, clean up
                    let _ = self.lua.remove_registry_value(key);
                    continue;
                }
            };

            // Check if thread is still resumable
            if thread.status() != ThreadStatus::Resumable {
                // Thread finished or errored, clean up
                let _ = self.lua.remove_registry_value(key);
                continue;
            }

            // Try to resume the thread
            match thread.resume::<()>(()) {
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
                    // Thread errored
                    eprintln!("[LuaRuntime] Coroutine error: {}", e);
                    let _ = self.lua.remove_registry_value(key);
                }
            }
        }

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
}

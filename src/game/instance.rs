use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::async_bridge::AsyncBridge;
use super::constants::physics as consts;
use super::lua::instance::{
    attributes_to_json, AttributeValue, ClassName, Instance, TextXAlignment, TextYAlignment,
};
use super::lua::services::AgentInput;
use super::lua::LuaRuntime;
use super::physics::PhysicsWorld;

/// Walk speed for player characters (studs per second)
const WALK_SPEED: f32 = consts::WALK_SPEED;

/// Default AFK timeout in seconds (5 minutes)
const DEFAULT_AFK_TIMEOUT_SECS: u64 = 300;

/// How often to check for AFK players (in ticks, 60 = 1 second)
const AFK_CHECK_INTERVAL_TICKS: u64 = 60;

/// Default max players when not specified
const DEFAULT_MAX_PLAYERS: u32 = 8;

/// Round a float to 2 decimal places (reduces JSON payload size)
#[inline]
fn round_f32(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

/// Round a position array to 2 decimal places
#[inline]
fn round_position(pos: [f32; 3]) -> [f32; 3] {
    [round_f32(pos[0]), round_f32(pos[1]), round_f32(pos[2])]
}

/// A game instance that runs Lua scripts with Rapier physics.
/// This is the Roblox-like architecture where:
/// - Lua controls game logic via Workspace, Parts, etc.
/// - Rapier handles physics simulation for non-anchored parts
pub struct GameInstance {
    /// Unique identifier for this specific instance (different from game_id)
    pub instance_id: Uuid,
    /// The game definition this instance belongs to
    pub game_id: Uuid,
    pub lua_runtime: Option<LuaRuntime>,
    pub physics: PhysicsWorld,
    pub tick: u64,
    pub players: HashMap<Uuid, u64>, // agent_id -> lua player user_id
    pub player_hrp_ids: HashMap<Uuid, u64>, // agent_id -> HumanoidRootPart lua_id
    pub player_names: HashMap<Uuid, String>, // agent_id -> player name
    observation_log_counts: Mutex<HashMap<Uuid, u8>>,
    humanoid_warn_counts: Mutex<HashMap<Uuid, u8>>,
    pub status: GameStatus,
    /// Time when the game instance was created (for server_time_ms calculation)
    start_time: Instant,
    /// Async bridge for database operations (DataStoreService)
    async_bridge: Option<Arc<AsyncBridge>>,
    /// Last activity timestamp for each player (agent_id -> Instant)
    player_last_activity: HashMap<Uuid, Instant>,
    /// AFK timeout duration (players idle longer than this are kicked)
    afk_timeout: Duration,
    /// Maximum number of players allowed in this instance
    pub max_players: u32,
    /// When the instance became empty (for garbage collection)
    /// None if instance has players, Some(Instant) when last player left
    pub empty_since: Option<Instant>,
    /// How Lua errors are handled (Continue = log and keep going, Halt = stop on first error)
    pub error_mode: ErrorMode,
    /// Set when error_mode is Halt and a Lua error occurs; prevents further ticking
    pub halted_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameStatus {
    Waiting,
    Playing,
    Finished,
}

/// Controls how Lua errors are handled by a GameInstance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ErrorMode {
    /// Log errors and continue (production server behavior)
    Continue,
    /// Stop the instance on first Lua error (CLI dev mode)
    Halt,
}


impl GameInstance {
    /// Derive a stable numeric user_id from an agent UUID (fits Lua safe integer range < 2^53).
    fn user_id_from_agent_id(agent_id: Uuid) -> u64 {
        let mask = (1u128 << 53) - 1;
        let mut id = (agent_id.as_u128() & mask) as u64;
        if id == 0 {
            id = 1;
        }
        id
    }

    /// Creates a new game instance without a script
    ///
    /// # Arguments
    /// * `game_id` - The game definition ID this instance belongs to
    /// * `async_bridge` - Optional async bridge for DataStoreService support
    pub fn new(game_id: Uuid, async_bridge: Option<Arc<AsyncBridge>>) -> Self {
        Self::new_with_config(game_id, DEFAULT_MAX_PLAYERS, async_bridge, ErrorMode::Continue)
    }

    /// Creates a new game instance with configuration
    ///
    /// # Arguments
    /// * `game_id` - The game definition ID this instance belongs to
    /// * `max_players` - Maximum number of players allowed
    /// * `async_bridge` - Optional async bridge for DataStoreService support
    /// * `error_mode` - How Lua errors are handled (Continue or Halt)
    pub fn new_with_config(
        game_id: Uuid,
        max_players: u32,
        async_bridge: Option<Arc<AsyncBridge>>,
        error_mode: ErrorMode,
    ) -> Self {
        let instance_id = Uuid::new_v4();

        Self {
            instance_id,
            game_id,
            lua_runtime: None,
            physics: PhysicsWorld::new(),
            tick: 0,
            players: HashMap::new(),
            player_hrp_ids: HashMap::new(),
            player_names: HashMap::new(),
            observation_log_counts: Mutex::new(HashMap::new()),
            humanoid_warn_counts: Mutex::new(HashMap::new()),
            status: GameStatus::Playing,
            start_time: Instant::now(),
            async_bridge,
            player_last_activity: HashMap::new(),
            afk_timeout: Duration::from_secs(DEFAULT_AFK_TIMEOUT_SECS),
            max_players,
            empty_since: Some(Instant::now()), // Starts empty
            error_mode,
            halted_error: None,
        }
    }

    /// Returns true if this instance has capacity for more players
    pub fn has_capacity(&self) -> bool {
        self.players.len() < self.max_players as usize
    }

    /// Returns the number of available slots
    pub fn available_slots(&self) -> usize {
        (self.max_players as usize).saturating_sub(self.players.len())
    }

    /// Returns milliseconds since game instance was created
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Creates a new game instance with a Lua script
    ///
    /// # Arguments
    /// * `game_id` - The game definition ID this instance belongs to
    /// * `script` - Lua script source code to execute
    /// * `async_bridge` - Optional async bridge for DataStoreService support
    pub fn new_with_script(
        game_id: Uuid,
        script: &str,
        async_bridge: Option<Arc<AsyncBridge>>,
    ) -> Self {
        Self::new_with_script_and_config(game_id, script, DEFAULT_MAX_PLAYERS, async_bridge, ErrorMode::Continue)
    }

    /// Creates a new game instance with a Lua script and configuration
    ///
    /// # Arguments
    /// * `game_id` - The game definition ID this instance belongs to
    /// * `script` - Lua script source code to execute
    /// * `max_players` - Maximum number of players allowed
    /// * `async_bridge` - Optional async bridge for DataStoreService support
    pub fn new_with_script_and_config(
        game_id: Uuid,
        script: &str,
        max_players: u32,
        async_bridge: Option<Arc<AsyncBridge>>,
        error_mode: ErrorMode,
    ) -> Self {
        let mut instance = Self::new_with_config(game_id, max_players, async_bridge, error_mode);
        instance.load_script(script);
        instance
    }

    /// Loads and executes a Lua script
    pub fn load_script(&mut self, source: &str) {
        match LuaRuntime::with_config(self.game_id, self.max_players, self.async_bridge.clone()) {
            Ok(mut runtime) => {
                // Set error mode on Lua VM so fire_as_coroutines/resume can read it
                runtime.lua().set_app_data(self.error_mode);

                if let Err(e) = runtime.load_script(source) {
                    self.handle_lua_error("Failed to load script", &e);
                } else {
                    self.lua_runtime = Some(runtime);
                }
            }
            Err(e) => {
                self.handle_lua_error("Failed to create runtime", &e);
            }
        }
    }

    /// Logs a Lua error and, in Halt mode, stores it to stop further ticking.
    fn handle_lua_error(&mut self, context: &str, err: &mlua::Error) {
        eprintln!("[Lua Error] {}: {}", context, err);
        if self.error_mode == ErrorMode::Halt {
            eprintln!("[Halted] Instance stopped due to Lua error. Fix the script and restart.");
            self.halted_error = Some(format!("{}: {}", context, err));
        }
    }

    /// Adds a player to the game
    /// Returns false if player is already in game (does NOT check capacity - use has_capacity() first)
    pub fn add_player(&mut self, agent_id: Uuid, name: &str) -> bool {
        if self.players.contains_key(&agent_id) {
            return false;
        }

        // Clear empty_since since we now have a player
        self.empty_since = None;

        // Use a stable user_id derived from agent_id so DataStore keys persist across restarts.
        let user_id = Self::user_id_from_agent_id(agent_id);
        self.players.insert(agent_id, user_id);
        self.player_names.insert(agent_id, name.to_string());
        // Initialize activity timestamp for AFK tracking
        self.player_last_activity.insert(agent_id, Instant::now());

        if let Some(runtime) = &self.lua_runtime {
            let (player, hrp_id) = runtime.add_player(user_id, name);

            // Offset spawn position based on player count to avoid overlap
            let player_index = self.players.len() as f32;
            let spawn_x = (player_index % 4.0 - 1.5) * 3.0; // -4.5, -1.5, 1.5, 4.5
            let spawn_z = (player_index / 4.0).floor() * 3.0;

            // Register character controller for player movement
            self.physics.add_character(hrp_id, [spawn_x, 6.0, spawn_z], 1.0, 5.0);
            self.player_hrp_ids.insert(agent_id, hrp_id);

            if let Err(e) = runtime.fire_player_added(&player) {
                self.handle_lua_error("Failed to fire PlayerAdded", &e);
            }
        }

        true
    }

    /// Removes a player from the game
    pub fn remove_player(&mut self, agent_id: Uuid) -> bool {
        if let Some(user_id) = self.players.remove(&agent_id) {
            // Remove character controller
            if let Some(hrp_id) = self.player_hrp_ids.remove(&agent_id) {
                self.physics.remove_character(hrp_id);
            }
            // Remove player name
            self.player_names.remove(&agent_id);
            // Remove activity timestamp
            self.player_last_activity.remove(&agent_id);
            if let Ok(mut counts) = self.observation_log_counts.lock() {
                counts.remove(&agent_id);
            }
            if let Ok(mut counts) = self.humanoid_warn_counts.lock() {
                counts.remove(&agent_id);
            }

            // Fire PlayerRemoving and remove player from Lua
            // Split borrow: capture error first, then handle it after releasing runtime ref
            let lua_err = if let Some(runtime) = &self.lua_runtime {
                let err = runtime
                    .players()
                    .get_player_by_user_id(user_id)
                    .and_then(|player| runtime.fire_player_removing(&player).err());
                runtime.remove_player(user_id);
                err
            } else {
                None
            };
            if let Some(e) = lua_err {
                self.handle_lua_error("Failed to fire PlayerRemoving", &e);
            }

            // Track when instance becomes empty for garbage collection
            if self.players.is_empty() {
                self.empty_since = Some(Instant::now());
            }

            true
        } else {
            false
        }
    }

    /// Queues an agent input for the AgentInputService in Lua
    pub fn queue_agent_input(&self, user_id: u64, input_type: String, data: serde_json::Value) {
        if let Some(runtime) = &self.lua_runtime {
            let input = AgentInput::new(input_type, data);
            runtime.queue_agent_input(user_id, input);
        }
    }

    /// Main game loop tick - called at 60 Hz
    pub fn tick(&mut self) {
        // In Halt mode, stop ticking once an error has occurred
        if self.halted_error.is_some() {
            return;
        }

        let dt = 1.0 / 60.0;

        // Process kick requests from Lua scripts (e.g., Player:Kick())
        self.process_kick_requests();

        // Check for AFK players periodically (every second)
        if self.tick % AFK_CHECK_INTERVAL_TICKS == 0 {
            self.check_afk_players();
        }

        // Sync Lua workspace gravity to physics
        self.sync_gravity();

        // Sync new/changed Lua parts to physics (skip character-controlled parts)
        self.sync_lua_to_physics();

        // Process agent inputs (fire InputReceived events)
        // Do this before syncing MoveTo targets so movement can apply in the same tick.
        if let Some(runtime) = &self.lua_runtime {
            if let Err(e) = runtime.process_agent_inputs() {
                self.handle_lua_error("Failed to process agent inputs", &e);
                if self.halted_error.is_some() {
                    return;
                }
            }
        }

        // Sync Lua humanoid MoveTo targets to physics character controllers
        self.sync_humanoid_move_targets();

        // Update query pipeline BEFORE character movement so move_shape has current collision state
        // This is necessary because the previous tick's physics.step moved the character,
        // and move_shape needs to see the updated positions.
        self.physics.query_pipeline.update(&self.physics.collider_set);

        // Update character controller movement
        self.update_character_movement(dt);

        // Step physics simulation
        self.physics.step(dt);

        // Sync physics results back to Lua (for Anchored=false parts and characters)
        self.sync_physics_to_lua();

        // Process weld constraints (update Part1 positions based on Part0)
        self.process_welds();

        // Run Lua Heartbeat
        if let Some(runtime) = &self.lua_runtime {
            if let Err(e) = runtime.tick(dt) {
                self.handle_lua_error("Tick error", &e);
                if self.halted_error.is_some() {
                    return;
                }
            }
        }

        self.tick += 1;
    }

    /// Process kick requests queued by Lua scripts (e.g., Player:Kick())
    fn process_kick_requests(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        // Drain kick requests from the game's kick queue
        let kick_requests = runtime.game().drain_kick_requests();

        for request in kick_requests {
            // Find the agent_id for this user_id
            let agent_id = self
                .players
                .iter()
                .find(|(_, &uid)| uid == request.user_id)
                .map(|(&aid, _)| aid);

            if let Some(agent_id) = agent_id {
                let name = self.player_names.get(&agent_id).cloned().unwrap_or_default();
                if let Some(msg) = &request.message {
                    eprintln!(
                        "[Kick] Removing player {} (user_id={}) - Reason: {}",
                        name, request.user_id, msg
                    );
                } else {
                    eprintln!(
                        "[Kick] Removing player {} (user_id={})",
                        name, request.user_id
                    );
                }
                self.remove_player(agent_id);
            } else {
                eprintln!(
                    "[Kick] Warning: No agent found for user_id={}",
                    request.user_id
                );
            }
        }
    }

    /// Check for AFK players and kick them if they've exceeded the timeout
    fn check_afk_players(&mut self) {
        let now = Instant::now();
        let timeout = self.afk_timeout;

        // First, check which players have pending inputs (they're active)
        let mut active_from_inputs: Vec<Uuid> = Vec::new();
        if let Some(runtime) = &self.lua_runtime {
            for (&agent_id, &user_id) in &self.players {
                if runtime.agent_input_service().has_pending_inputs(user_id) {
                    active_from_inputs.push(agent_id);
                }
            }
        }

        // Update activity for players with pending inputs
        for agent_id in active_from_inputs {
            self.player_last_activity.insert(agent_id, now);
        }

        // Collect players to kick (can't modify while iterating)
        let mut to_kick: Vec<(Uuid, String)> = Vec::new();

        for (&agent_id, last_active) in &self.player_last_activity {
            let idle_duration = now.duration_since(*last_active);
            if idle_duration > timeout {
                let name = self
                    .player_names
                    .get(&agent_id)
                    .cloned()
                    .unwrap_or_else(|| format!("{}", agent_id));
                to_kick.push((agent_id, name));
            }
        }

        // Kick AFK players
        for (agent_id, name) in to_kick {
            eprintln!(
                "[AFK] Kicking player {} (idle for {:?})",
                name, self.afk_timeout
            );
            self.remove_player(agent_id);
        }
    }

    /// Set the AFK timeout duration
    pub fn set_afk_timeout(&mut self, timeout: Duration) {
        self.afk_timeout = timeout;
    }

    /// Get the current AFK timeout duration
    pub fn afk_timeout(&self) -> Duration {
        self.afk_timeout
    }

    /// Record player activity (resets AFK timer)
    pub fn record_player_activity(&mut self, agent_id: Uuid) {
        self.player_last_activity.insert(agent_id, Instant::now());
    }

    /// Syncs Workspace.Gravity to physics world
    fn sync_gravity(&mut self) {
        if let Some(runtime) = &self.lua_runtime {
            let gravity = runtime.workspace().data.lock().unwrap().gravity;
            self.physics.set_gravity(gravity);
        }
    }

    /// Syncs Lua parts to the physics world
    /// - Creates physics bodies for new parts (skips character-controlled parts)
    /// - Updates positions for anchored parts that moved in Lua
    /// - Removes physics bodies for parts that were destroyed in Lua
    fn sync_lua_to_physics(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        let descendants = runtime.workspace().get_descendants();

        // Collect all active Lua part IDs
        let mut active_lua_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

        for part in descendants {
            let mut data = part.data.lock().unwrap();

            let lua_id = data.id.0;
            if let Some(part_data) = data.part_data.as_mut() {
                active_lua_ids.insert(lua_id);

                // Character-controlled parts: allow Lua-driven teleports (spawn) to sync into physics
                if self.physics.has_character(lua_id) {
                    if part_data.position_dirty {
                        eprintln!(
                            "[Sync] Teleport character lua_id={} -> ({:.2},{:.2},{:.2})",
                            lua_id,
                            part_data.position.x,
                            part_data.position.y,
                            part_data.position.z
                        );
                        self.physics.set_character_position(
                            lua_id,
                            [part_data.position.x, part_data.position.y, part_data.position.z],
                        );
                        part_data.position_dirty = false;
                    }
                    continue;
                }

                if !self.physics.has_part(lua_id) {
                    // Skip parts with CanCollide=false - they don't need physics
                    if !part_data.can_collide {
                        continue;
                    }
                    // New part - add to physics
                    self.physics.add_part(
                        lua_id,
                        [part_data.position.x, part_data.position.y, part_data.position.z],
                        [0.0, 0.0, 0.0, 1.0], // TODO: extract rotation from CFrame
                        [part_data.size.x, part_data.size.y, part_data.size.z],
                        part_data.anchored,
                        part_data.can_collide,
                    );

                    // Apply initial velocity for dynamic parts
                    if !part_data.anchored {
                        if let Some(handle) = self.physics.get_handle(lua_id) {
                            self.physics.set_velocity(
                                handle,
                                [part_data.velocity.x, part_data.velocity.y, part_data.velocity.z],
                            );
                        }
                    }
                } else if part_data.anchored {
                    // Anchored part - update physics position from Lua
                    if let Some(handle) = self.physics.get_handle(lua_id) {
                        self.physics.set_kinematic_position(
                            handle,
                            [part_data.position.x, part_data.position.y, part_data.position.z],
                        );
                    }
                }
            }
        }

        // Remove physics parts that no longer exist in Lua (destroyed parts)
        let orphaned_ids: Vec<u64> = self.physics.get_all_part_ids()
            .into_iter()
            .filter(|id| !active_lua_ids.contains(id))
            .collect();

        for lua_id in orphaned_ids {
            self.physics.remove_part(lua_id);
        }
    }

    /// Syncs physics positions back to Lua for non-anchored parts and character controllers
    fn sync_physics_to_lua(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        let descendants = runtime.workspace().get_descendants();

        for part in descendants {
            let mut data = part.data.lock().unwrap();
            let lua_id = data.id.0;

            if let Some(part_data) = &mut data.part_data {
                // Check if this is a character-controlled part
                if self.physics.has_character(lua_id) {
                    if part_data.position_dirty {
                        continue;
                    }
                    if let Some(pos) = self.physics.get_character_position(lua_id) {
                        part_data.position.x = pos[0];
                        part_data.position.y = pos[1];
                        part_data.position.z = pos[2];
                        part_data.cframe.position = part_data.position;
                    }
                } else if !part_data.anchored {
                    if let Some(handle) = self.physics.get_handle(lua_id) {
                        // Update position from physics
                        if let Some(pos) = self.physics.get_position(handle) {
                            part_data.position.x = pos[0];
                            part_data.position.y = pos[1];
                            part_data.position.z = pos[2];
                            part_data.cframe.position = part_data.position;
                        }

                        // Update velocity from physics
                        if let Some(vel) = self.physics.get_velocity(handle) {
                            part_data.velocity.x = vel[0];
                            part_data.velocity.y = vel[1];
                            part_data.velocity.z = vel[2];
                        }
                    }
                }
            }
        }
    }

    /// Process weld constraints - update Part1 position based on Part0's CFrame
    fn process_welds(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        let descendants = runtime.workspace().get_descendants();

        // Collect welds and their data first to avoid borrow conflicts
        let mut weld_updates: Vec<(
            crate::game::lua::instance::InstanceRef,  // Part1 ref
            crate::game::lua::types::Vector3,         // new position
        )> = Vec::new();

        for instance in &descendants {
            let data = instance.data.lock().unwrap();

            if let Some(weld_data) = &data.weld_data {
                if !weld_data.enabled {
                    continue;
                }

                // Get Part0 and Part1 references
                let part0_ref = weld_data.part0.as_ref().and_then(|w| w.upgrade());
                let part1_ref = weld_data.part1.as_ref().and_then(|w| w.upgrade());

                if let (Some(part0_ref), Some(part1_ref)) = (part0_ref, part1_ref) {
                    // Get Part0's CFrame
                    let part0_data = part0_ref.lock().unwrap();
                    if let Some(p0_part) = &part0_data.part_data {
                        let part0_cframe = p0_part.cframe;
                        let c0 = weld_data.c0;
                        let c1 = weld_data.c1;
                        drop(part0_data);

                        // Calculate Part1's new position:
                        // Part1.CFrame = Part0.CFrame * C0 * C1:Inverse()
                        // For simplicity (no rotation yet), just use position offset:
                        // Part1.Position = Part0.Position + C0.Position - C1.Position
                        let new_pos = crate::game::lua::types::Vector3::new(
                            part0_cframe.position.x + c0.position.x - c1.position.x,
                            part0_cframe.position.y + c0.position.y - c1.position.y,
                            part0_cframe.position.z + c0.position.z - c1.position.z,
                        );

                        weld_updates.push((part1_ref, new_pos));
                    }
                }
            }
        }

        // Apply updates
        for (part1_ref, new_pos) in weld_updates {
            let mut part1_data = part1_ref.lock().unwrap();
            if let Some(p1_part) = &mut part1_data.part_data {
                p1_part.position = new_pos;
                p1_part.cframe.position = new_pos;
            }
        }
    }

    /// Syncs Lua humanoid MoveTo targets to physics character controllers
    fn sync_humanoid_move_targets(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };

        // For each player, check if their humanoid has a move target
        for (&agent_id, &user_id) in &self.players {
            let Some(&hrp_id) = self.player_hrp_ids.get(&agent_id) else {
                if let Ok(mut counts) = self.humanoid_warn_counts.lock() {
                    let count = counts.entry(agent_id).or_insert(0);
                    if *count < 3 {
                        eprintln!("[MoveTo WARN] Missing HRP for agent {}", agent_id);
                        *count += 1;
                    }
                }
                continue;
            };

            // Get player's character and humanoid
            let Some(player) = runtime.players().get_player_by_user_id(user_id) else {
                if let Ok(mut counts) = self.humanoid_warn_counts.lock() {
                    let count = counts.entry(agent_id).or_insert(0);
                    if *count < 3 {
                        eprintln!("[MoveTo WARN] Missing player for agent {}", agent_id);
                        *count += 1;
                    }
                }
                continue;
            };

            let player_data = player.data.lock().unwrap();
            let Some(character_weak) = player_data.player_data.as_ref().and_then(|pd| pd.character.as_ref()) else {
                if let Ok(mut counts) = self.humanoid_warn_counts.lock() {
                    let count = counts.entry(agent_id).or_insert(0);
                    if *count < 3 {
                        eprintln!("[MoveTo WARN] Missing character for agent {}", agent_id);
                        *count += 1;
                    }
                }
                continue;
            };
            let Some(character_ref) = character_weak.upgrade() else {
                if let Ok(mut counts) = self.humanoid_warn_counts.lock() {
                    let count = counts.entry(agent_id).or_insert(0);
                    if *count < 3 {
                        eprintln!("[MoveTo WARN] Character ref expired for agent {}", agent_id);
                        *count += 1;
                    }
                }
                continue;
            };
            drop(player_data);

            // Find humanoid in character and process move target
            let character_data = character_ref.lock().unwrap();
            let mut found_humanoid = false;
            for child_ref in &character_data.children {
                let mut child_data = child_ref.lock().unwrap();
                if let Some(humanoid) = &mut child_data.humanoid_data {
                    found_humanoid = true;
                    // Check for cancel first
                    if humanoid.cancel_move_to {
                        humanoid.cancel_move_to = false;
                        self.physics.set_character_target(hrp_id, None);
                    } else if let Some(target) = humanoid.move_to_target.take() {
                        self.physics.set_character_target(hrp_id, Some([target.x, target.y, target.z]));
                    }
                }
            }
            if !found_humanoid {
                if let Ok(mut counts) = self.humanoid_warn_counts.lock() {
                    let count = counts.entry(agent_id).or_insert(0);
                    if *count < 3 {
                        eprintln!("[MoveTo WARN] No humanoid found for agent {}", agent_id);
                        *count += 1;
                    }
                }
            }
        }
    }



    /// Updates character controller movement towards targets.
    /// Uses Rapier's kinematic character controller for full 3D translation.
    fn update_character_movement(&mut self, dt: f32) {
        // Collect agent_id -> hrp_id pairs to process (avoid borrow issues)
        let agent_hrp_pairs: Vec<(Uuid, u64)> = self
            .player_hrp_ids
            .iter()
            .map(|(&agent_id, &hrp_id)| (agent_id, hrp_id))
            .collect();

        for (agent_id, hrp_id) in agent_hrp_pairs {
            // Get current position, target, and vertical velocity
            let (current_pos, target, vertical_velocity, grounded) = {
                let Some(state) = self.physics.get_character_state(hrp_id) else {
                    continue;
                };
                let Some(pos) = self.physics.get_character_position(hrp_id) else {
                    continue;
                };
                (pos, state.target_position, state.vertical_velocity, state.grounded)
            };

            // Look up the humanoid's walk_speed from Lua instance data
            let walk_speed = self.get_humanoid_walk_speed(agent_id).unwrap_or(WALK_SPEED);

            let gravity = self.physics.gravity.y;
            let mut new_vertical_velocity = vertical_velocity + gravity * dt;

            // Calculate horizontal movement towards target
            let mut dx = 0.0f32;
            let mut dz = 0.0f32;

            if let Some(target) = target {
                let tx = target[0] - current_pos[0];
                let tz = target[2] - current_pos[2];
                let dist_xz = (tx * tx + tz * tz).sqrt();

                if dist_xz > 0.5 {
                    let speed = walk_speed * dt;
                    dx = (tx / dist_xz) * speed;
                    dz = (tz / dist_xz) * speed;
                } else {
                    // Reached target, clear it
                    self.physics.set_character_target(hrp_id, None);
                }
            }

            // When grounded, use zero vertical component - snap_to_ground handles staying grounded
            // Only apply gravity when airborne
            // Note: The -0.001 epsilon was causing floor collisions that blocked horizontal movement
            let desired_y = if grounded && new_vertical_velocity <= 0.0 {
                0.0 // snap_to_ground keeps character on ground; no downward component needed
            } else {
                new_vertical_velocity * dt
            };
            let desired = [dx, desired_y, dz];
            if let Some(movement) = self.physics.move_character(hrp_id, desired, dt) {
                if movement.grounded && new_vertical_velocity < 0.0 {
                    new_vertical_velocity = 0.0;
                }
                if desired[1] > 0.0 && movement.translation.y + 1.0e-4 < desired[1] {
                    new_vertical_velocity = 0.0;
                }
            }

            if let Some(state) = self.physics.get_character_state_mut(hrp_id) {
                state.vertical_velocity = new_vertical_velocity;
            }
        }
    }

    /// Gets the observation for a specific player
    pub fn get_player_observation(&self, agent_id: Uuid) -> Option<PlayerObservation> {
        let user_id = *self.players.get(&agent_id)?;

        let runtime = self.lua_runtime.as_ref()?;
        let player = runtime.players().get_player_by_user_id(user_id)?;

        // Get position from character's HumanoidRootPart (rounded to reduce payload size)
        let position = round_position(self.get_player_position(agent_id).unwrap_or([0.0, 3.0, 0.0]));

        {
            let mut counts = self.observation_log_counts.lock().unwrap();
            let count = counts.entry(agent_id).or_insert(0);
            if *count < 5 {
                let name = self.player_names.get(&agent_id).cloned().unwrap_or_default();
                eprintln!(
                    "[Obs] tick={} agent={} name={} pos=({:.2},{:.2},{:.2})",
                    self.tick,
                    agent_id,
                    name,
                    position[0],
                    position[1],
                    position[2]
                );
                *count += 1;
            }
        }

        // Get health from humanoid
        let health = self.get_player_health(agent_id).unwrap_or(100);

        // Read all player attributes generically and convert to JSON
        let player_data = player.data.lock().unwrap();
        let attributes = attributes_to_json(&player_data.attributes);
        drop(player_data);

        // Get other players (with LOS filtering)
        let other_players = self.get_other_players(agent_id, position);

        // Get dynamic world entities only (static entities fetched via /map endpoint)
        let world = self.get_dynamic_world_info();

        Some(PlayerObservation {
            tick: self.tick,
            game_status: self.get_game_status_from_lua(),
            player: PlayerInfo {
                id: agent_id,
                position,
                health,
                attributes,
            },
            other_players,
            world,
            events: Vec::new(),
        })
    }

    /// Get the game status from Lua if available
    fn get_game_status_from_lua(&self) -> String {
        // Default to instance status
        match self.status {
            GameStatus::Waiting => "waiting".to_string(),
            GameStatus::Playing => "active".to_string(),
            GameStatus::Finished => "finished".to_string(),
        }
    }

    /// Get player position from their HumanoidRootPart
    fn get_player_position(&self, agent_id: Uuid) -> Option<[f32; 3]> {
        let hrp_id = *self.player_hrp_ids.get(&agent_id)?;
        self.physics.get_character_position(hrp_id)
    }

    /// Get player health from their Humanoid
    fn get_player_health(&self, agent_id: Uuid) -> Option<i32> {
        let user_id = *self.players.get(&agent_id)?;
        let runtime = self.lua_runtime.as_ref()?;
        let player = runtime.players().get_player_by_user_id(user_id)?;

        let player_data = player.data.lock().unwrap();
        let character = player_data
            .player_data
            .as_ref()?
            .character
            .as_ref()?
            .upgrade()?;
        drop(player_data);

        let char_data = character.lock().unwrap();
        for child in &char_data.children {
            let child_data = child.lock().unwrap();
            if child_data.name == "Humanoid" {
                if let Some(humanoid) = &child_data.humanoid_data {
                    return Some(humanoid.health as i32);
                }
            }
        }
        None
    }

    /// Get walk speed from the player's Humanoid
    fn get_humanoid_walk_speed(&self, agent_id: Uuid) -> Option<f32> {
        let user_id = *self.players.get(&agent_id)?;
        let runtime = self.lua_runtime.as_ref()?;
        let player = runtime.players().get_player_by_user_id(user_id)?;

        let player_data = player.data.lock().unwrap();
        let character = player_data
            .player_data
            .as_ref()?
            .character
            .as_ref()?
            .upgrade()?;
        drop(player_data);

        let char_data = character.lock().unwrap();
        for child in &char_data.children {
            let child_data = child.lock().unwrap();
            if let Some(humanoid) = &child_data.humanoid_data {
                return Some(humanoid.walk_speed);
            }
        }
        None
    }

    /// Get static map geometry (entities with "Static" tag)
    /// Used for one-time fetch via /map endpoint
    pub fn get_map_info(&self) -> MapInfo {
        let mut entities = Vec::new();

        if let Some(runtime) = &self.lua_runtime {
            for part in runtime.workspace().get_descendants() {
                let data = part.data.lock().unwrap();

                // Only include entities with "Static" tag
                if !data.tags.contains("Static") {
                    continue;
                }

                if let Some(part_data) = &data.part_data {
                    let attrs = attributes_to_json(&data.attributes);
                    entities.push(WorldEntity {
                        id: data.id.0,
                        name: data.name.clone(),
                        entity_type: Some("part".to_string()),
                        position: [part_data.position.x, part_data.position.y, part_data.position.z],
                        size: [part_data.size.x, part_data.size.y, part_data.size.z],
                        color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                        material: Some(part_data.material.name().to_string()),
                        anchored: part_data.anchored,
                        attributes: if attrs.is_empty() { None } else { Some(attrs) },
                    });
                }
            }
        }

        MapInfo { entities }
    }

    /// Get dynamic world info (entities WITHOUT "Static" tag + folders with attributes)
    /// Used for per-tick observations - excludes static geometry
    fn get_dynamic_world_info(&self) -> WorldInfo {
        let mut entities = Vec::new();

        if let Some(runtime) = &self.lua_runtime {
            for part in runtime.workspace().get_descendants() {
                let data = part.data.lock().unwrap();

                // Skip entities with "Static" tag - they're fetched via /map endpoint
                let is_static = data.tags.contains("Static");

                if let Some(part_data) = &data.part_data {
                    // Only include parts WITHOUT "Static" tag
                    if !is_static {
                        let attrs = attributes_to_json(&data.attributes);
                        entities.push(WorldEntity {
                            id: data.id.0,
                            name: data.name.clone(),
                            entity_type: Some("part".to_string()),
                            position: round_position([part_data.position.x, part_data.position.y, part_data.position.z]),
                            size: round_position([part_data.size.x, part_data.size.y, part_data.size.z]),
                            color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                            material: Some(part_data.material.name().to_string()),
                            anchored: part_data.anchored,
                            attributes: if attrs.is_empty() { None } else { Some(attrs) },
                        });
                    }
                } else if data.class_name == ClassName::Folder && !is_static {
                    // Include Folders with attributes (e.g., GameState) - these are dynamic
                    let attrs = attributes_to_json(&data.attributes);
                    if !attrs.is_empty() {
                        entities.push(WorldEntity {
                            id: data.id.0,
                            name: data.name.clone(),
                            entity_type: Some("folder".to_string()),
                            position: [0.0, 0.0, 0.0],
                            size: [0.0, 0.0, 0.0],
                            color: None,
                            material: None,
                            anchored: true,
                            attributes: Some(attrs),
                        });
                    }
                }
            }
        }

        WorldInfo { entities }
    }

    /// Get world info (all visible parts and folders from Workspace)
    #[allow(dead_code)]
    fn get_world_info(&self) -> WorldInfo {
        let mut entities = Vec::new();

        if let Some(runtime) = &self.lua_runtime {
            for part in runtime.workspace().get_descendants() {
                let data = part.data.lock().unwrap();

                if let Some(part_data) = &data.part_data {
                    let attrs = attributes_to_json(&data.attributes);
                    entities.push(WorldEntity {
                        id: data.id.0,
                        name: data.name.clone(),
                        entity_type: Some("part".to_string()),
                        position: [part_data.position.x, part_data.position.y, part_data.position.z],
                        size: [part_data.size.x, part_data.size.y, part_data.size.z],
                        color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                        material: Some(part_data.material.name().to_string()),
                        anchored: part_data.anchored,
                        attributes: if attrs.is_empty() { None } else { Some(attrs) },
                    });
                } else if data.class_name == ClassName::Folder {
                    // Include Folders with attributes (e.g., GameState)
                    let attrs = attributes_to_json(&data.attributes);
                    if !attrs.is_empty() {
                        entities.push(WorldEntity {
                            id: data.id.0,
                            name: data.name.clone(),
                            entity_type: Some("folder".to_string()),
                            position: [0.0, 0.0, 0.0],
                            size: [0.0, 0.0, 0.0],
                            color: None,
                            material: None,
                            anchored: true,
                            attributes: Some(attrs),
                        });
                    }
                }
            }
        }

        WorldInfo { entities }
    }

    /// Get info about other players with distance and line-of-sight filtering
    fn get_other_players(&self, exclude_agent_id: Uuid, observer_pos: [f32; 3]) -> Vec<OtherPlayerInfo> {
        const MAX_VISIBILITY_DISTANCE: f32 = 100.0;

        let mut others = Vec::new();

        // Get observer's body handle for LOS exclusion
        let observer_body = self.player_hrp_ids.get(&exclude_agent_id)
            .and_then(|&hrp_id| self.physics.get_character_state(hrp_id))
            .map(|state| state.body_handle);

        for (&agent_id, &user_id) in &self.players {
            if agent_id == exclude_agent_id {
                continue;
            }

            let position = self.get_player_position(agent_id).unwrap_or([0.0, 0.0, 0.0]);

            // Distance culling
            let dx = position[0] - observer_pos[0];
            let dy = position[1] - observer_pos[1];
            let dz = position[2] - observer_pos[2];
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();

            if distance > MAX_VISIBILITY_DISTANCE {
                continue;
            }

            // Line-of-sight check (only for nearby players)
            if !self.physics.has_line_of_sight(observer_pos, position, observer_body) {
                continue;
            }

            let health = self.get_player_health(agent_id).unwrap_or(100);

            // Get all attributes generically and convert to JSON
            let attributes = if let Some(runtime) = &self.lua_runtime {
                if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                    let data = player.data.lock().unwrap();
                    attributes_to_json(&data.attributes)
                } else {
                    std::collections::HashMap::new()
                }
            } else {
                std::collections::HashMap::new()
            };

            others.push(OtherPlayerInfo {
                id: agent_id,
                position: round_position(position),
                health,
                attributes,
            });
        }

        others
    }

    /// Serializes a GUI instance tree to GuiElement for frontend rendering
    fn serialize_gui_tree(instance: &Instance) -> Option<GuiElement> {
        let data = instance.data.lock().unwrap();

        // Only serialize GUI classes
        let element_type = match data.class_name {
            ClassName::ScreenGui => "ScreenGui",
            ClassName::Frame => "Frame",
            ClassName::TextLabel => "TextLabel",
            ClassName::TextButton => "TextButton",
            ClassName::ImageLabel => "ImageLabel",
            ClassName::ImageButton => "ImageButton",
            _ => return None,
        };

        let gui_data = data.gui_data.as_ref();

        // Build element with optional GUI properties
        let mut element = GuiElement {
            id: data.id.0,
            element_type: element_type.to_string(),
            name: data.name.clone(),
            position: gui_data.map(|g| UDim2Json {
                x_scale: g.position.x.scale,
                x_offset: g.position.x.offset,
                y_scale: g.position.y.scale,
                y_offset: g.position.y.offset,
            }),
            size: gui_data.map(|g| UDim2Json {
                x_scale: g.size.x.scale,
                x_offset: g.size.x.offset,
                y_scale: g.size.y.scale,
                y_offset: g.size.y.offset,
            }),
            anchor_point: gui_data.map(|g| [g.anchor_point.0, g.anchor_point.1]),
            rotation: gui_data.map(|g| g.rotation),
            z_index: gui_data.map(|g| g.z_index),
            visible: gui_data.map(|g| g.visible),
            background_color: gui_data.map(|g| [g.background_color.r, g.background_color.g, g.background_color.b]),
            background_transparency: gui_data.map(|g| g.background_transparency),
            border_color: gui_data.map(|g| [g.border_color.r, g.border_color.g, g.border_color.b]),
            border_size_pixel: gui_data.map(|g| g.border_size_pixel),
            text: gui_data.and_then(|g| g.text.clone()),
            text_color: gui_data.and_then(|g| g.text_color.map(|c| [c.r, c.g, c.b])),
            text_size: gui_data.and_then(|g| g.text_size),
            text_transparency: gui_data.and_then(|g| g.text_transparency),
            text_x_alignment: gui_data.map(|g| match g.text_x_alignment {
                TextXAlignment::Left => "Left".to_string(),
                TextXAlignment::Center => "Center".to_string(),
                TextXAlignment::Right => "Right".to_string(),
            }),
            text_y_alignment: gui_data.map(|g| match g.text_y_alignment {
                TextYAlignment::Top => "Top".to_string(),
                TextYAlignment::Center => "Center".to_string(),
                TextYAlignment::Bottom => "Bottom".to_string(),
            }),
            image: gui_data.and_then(|g| g.image.clone()),
            image_color: gui_data.and_then(|g| g.image_color.map(|c| [c.r, c.g, c.b])),
            image_transparency: gui_data.and_then(|g| g.image_transparency),
            display_order: gui_data.map(|g| g.display_order),
            enabled: gui_data.map(|g| g.enabled),
            children: Vec::new(),
        };

        // Recursively serialize children (release lock first to avoid deadlock)
        let children_refs: Vec<_> = data.children.iter().cloned().collect();
        drop(data);

        for child_ref in children_refs {
            let child = Instance::from_ref(child_ref);
            if let Some(child_element) = Self::serialize_gui_tree(&child) {
                element.children.push(child_element);
            }
        }

        Some(element)
    }

    /// Gets the spectator observation (full world state from Lua Workspace)
    pub fn get_spectator_observation(&self) -> SpectatorObservation {
        let mut entities = Vec::new();
        let mut players = Vec::new();

        if let Some(runtime) = &self.lua_runtime {
            // Collect all parts from Workspace
            for part in runtime.workspace().get_descendants() {
                let data = part.data.lock().unwrap();

                if let Some(part_data) = &data.part_data {
                    // Only include rotation if it's not identity
                    let rot = part_data.cframe.rotation;
                    let is_identity = (rot[0][0] - 1.0).abs() < 0.001
                        && rot[0][1].abs() < 0.001
                        && rot[0][2].abs() < 0.001
                        && rot[1][0].abs() < 0.001
                        && (rot[1][1] - 1.0).abs() < 0.001
                        && rot[1][2].abs() < 0.001
                        && rot[2][0].abs() < 0.001
                        && rot[2][1].abs() < 0.001
                        && (rot[2][2] - 1.0).abs() < 0.001;

                    // Check for BillboardGui children
                    let billboard_gui = Self::collect_billboard_gui(&data.children);

                    entities.push(SpectatorEntity {
                        id: data.id.0 as u32,
                        entity_type: "part".to_string(),
                        position: round_position([
                            part_data.position.x,
                            part_data.position.y,
                            part_data.position.z,
                        ]),
                        rotation: if is_identity { None } else { Some(rot) },
                        size: Some(round_position([part_data.size.x, part_data.size.y, part_data.size.z])),
                        color: Some([part_data.color.r, part_data.color.g, part_data.color.b]),
                        material: Some(part_data.material.name().to_string()),
                        shape: Some(part_data.shape.name().to_string()),
                        health: None,
                        pickup_type: None,
                        model_url: Self::extract_model_url(&data.attributes),
                        billboard_gui,
                    });
                }
            }

            // Collect player info
            for (&agent_id, &user_id) in &self.players {
                if let Some(player) = runtime.players().get_player_by_user_id(user_id) {
                    let player_data = player.data.lock().unwrap();

                    // Get position and health in one pass (avoid redundant locking)
                    let (position, health) = player_data.player_data.as_ref()
                        .and_then(|pd| pd.character.as_ref())
                        .and_then(|weak| weak.upgrade())
                        .map(|char_ref| {
                            let char = char_ref.lock().unwrap();

                            // Get position from HumanoidRootPart
                            let pos = char.model_data.as_ref()
                                .and_then(|m| m.primary_part.as_ref())
                                .and_then(|weak| weak.upgrade())
                                .and_then(|hrp_data| {
                                    let hrp = hrp_data.lock().unwrap();
                                    hrp.part_data.as_ref().map(|p| [p.position.x, p.position.y, p.position.z])
                                })
                                .unwrap_or([0.0, 3.0, 0.0]);

                            // Get health from Humanoid (while we have character locked)
                            let hp = char.children.iter()
                                .find_map(|child| {
                                    let child_data = child.lock().unwrap();
                                    if child_data.name == "Humanoid" {
                                        child_data.humanoid_data.as_ref().map(|h| h.health as i32)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(100);

                            (pos, hp)
                        })
                        .unwrap_or(([0.0, 3.0, 0.0], 100));

                    // Get player name from our cache, or fall back to Player_<uuid>
                    let name = self.player_names.get(&agent_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Player_{}", agent_id.as_simple()));

                    // Get all attributes and convert to JSON Value
                    let attrs = attributes_to_json(&player_data.attributes);
                    let attributes = if attrs.is_empty() {
                        None
                    } else {
                        Some(serde_json::to_value(&attrs).unwrap_or(serde_json::Value::Null))
                    };

                    // Serialize PlayerGui tree
                    let gui = player_data.player_data.as_ref()
                        .and_then(|pd| pd.player_gui.as_ref())
                        .and_then(|weak| weak.upgrade())
                        .map(|player_gui_ref| {
                            let player_gui = Instance::from_ref(player_gui_ref);
                            // Get all ScreenGui children
                            player_gui.get_children()
                                .iter()
                                .filter_map(|child| Self::serialize_gui_tree(child))
                                .collect::<Vec<_>>()
                        })
                        .filter(|v: &Vec<GuiElement>| !v.is_empty());

                    players.push(SpectatorPlayerInfo {
                        id: agent_id,
                        name,
                        position: round_position(position),
                        health,
                        attributes,
                        gui,
                    });
                    drop(player_data);
                }
            }
        }

        SpectatorObservation {
            instance_id: self.instance_id,
            tick: self.tick,
            server_time_ms: self.elapsed_ms(),
            game_status: match self.status {
                GameStatus::Waiting => "waiting".to_string(),
                GameStatus::Playing => "playing".to_string(),
                GameStatus::Finished => "finished".to_string(),
            },
            players,
            entities,
        }
    }

    /// Collects BillboardGui data from a part's children
    fn collect_billboard_gui(
        children: &[crate::game::lua::instance::InstanceRef],
    ) -> Option<BillboardGuiJson> {
        use crate::game::lua::instance::ClassName;

        for child_ref in children {
            let child_data = child_ref.lock().unwrap();

            if child_data.class_name == ClassName::BillboardGui {
                if let Some(billboard_data) = &child_data.billboard_gui_data {
                    // Collect TextLabel children
                    let mut labels = Vec::new();

                    for label_ref in &child_data.children {
                        let label_data = label_ref.lock().unwrap();

                        if label_data.class_name == ClassName::TextLabel {
                            if let Some(gui_data) = &label_data.gui_data {
                                labels.push(BillboardLabelJson {
                                    text: gui_data.text.clone().unwrap_or_default(),
                                    color: [
                                        gui_data.text_color.map(|c| c.r).unwrap_or(1.0),
                                        gui_data.text_color.map(|c| c.g).unwrap_or(1.0),
                                        gui_data.text_color.map(|c| c.b).unwrap_or(1.0),
                                    ],
                                    size: gui_data.text_size.unwrap_or(14.0) as f32,
                                });
                            }
                        }
                    }

                    return Some(BillboardGuiJson {
                        studs_offset: [
                            billboard_data.studs_offset.x,
                            billboard_data.studs_offset.y,
                            billboard_data.studs_offset.z,
                        ],
                        always_on_top: billboard_data.always_on_top,
                        labels,
                    });
                }
            }
        }

        None
    }

    /// Reads a model URL from instance attributes.
    /// Supports both `ModelUrl` and `model_url`.
    fn extract_model_url(
        attributes: &std::collections::HashMap<String, AttributeValue>,
    ) -> Option<String> {
        for key in ["ModelUrl", "model_url"] {
            if let Some(AttributeValue::String(url)) = attributes.get(key) {
                let trimmed = url.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }
}

// ============================================================================
// Observation types
// ============================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerObservation {
    pub tick: u64,
    pub game_status: String,
    pub player: PlayerInfo,
    pub other_players: Vec<OtherPlayerInfo>,
    pub world: WorldInfo,
    pub events: Vec<GameEvent>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldInfo {
    pub entities: Vec<WorldEntity>,
}

/// Static map geometry (anchored entities that never change)
#[derive(Debug, Clone, serde::Serialize)]
pub struct MapInfo {
    pub entities: Vec<WorldEntity>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorldEntity {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    pub position: [f32; 3],
    pub size: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    pub anchored: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerInfo {
    pub id: Uuid,
    pub position: [f32; 3],
    pub health: i32,
    /// Game-specific attributes set by Lua scripts
    pub attributes: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OtherPlayerInfo {
    pub id: Uuid,
    pub position: [f32; 3],
    pub health: i32,
    /// Game-specific attributes set by Lua scripts
    pub attributes: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VisibleEntity {
    pub id: u32,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub position: [f32; 3],
    pub distance: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GameEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorObservation {
    pub instance_id: Uuid,
    pub tick: u64,
    /// Milliseconds since game instance was created (for client clock synchronization)
    pub server_time_ms: u64,
    pub game_status: String,
    pub players: Vec<SpectatorPlayerInfo>,
    pub entities: Vec<SpectatorEntity>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorPlayerInfo {
    pub id: Uuid,
    pub name: String,
    pub position: [f32; 3],
    pub health: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gui: Option<Vec<GuiElement>>,
}

/// Serialized GUI element for frontend rendering
#[derive(Debug, Clone, serde::Serialize)]
pub struct GuiElement {
    pub id: u64,
    #[serde(rename = "type")]
    pub element_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<UDim2Json>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<UDim2Json>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_point: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_transparency: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_size_pixel: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_transparency: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_x_alignment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_y_alignment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_transparency: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_order: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    pub children: Vec<GuiElement>,
}

/// UDim2 serialization format
#[derive(Debug, Clone, serde::Serialize)]
pub struct UDim2Json {
    pub x_scale: f32,
    pub x_offset: i32,
    pub y_scale: f32,
    pub y_offset: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorEntity {
    pub id: u32,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub position: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<[[f32; 3]; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pickup_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billboard_gui: Option<BillboardGuiJson>,
}

/// BillboardGui serialization for 3D floating labels
#[derive(Debug, Clone, serde::Serialize)]
pub struct BillboardGuiJson {
    pub studs_offset: [f32; 3],
    pub always_on_top: bool,
    pub labels: Vec<BillboardLabelJson>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BillboardLabelJson {
    pub text: String,
    pub color: [f32; 3],
    pub size: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::lua::instance::AttributeValue;

    #[test]
    fn test_player_goto_action() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Load a simple script that creates a floor
        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace
        "#);

        // Add a player
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id, "TestPlayer"));

        // Verify player has HRP registered
        assert!(instance.player_hrp_ids.contains_key(&agent_id));
        let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();

        // Get initial position
        let initial_pos = instance.physics.get_character_position(hrp_id).unwrap();
        println!("Initial player position: {:?}", initial_pos);

        // Set movement target directly via physics
        instance.physics.set_character_target(hrp_id, Some([10.0, 5.0, 10.0]));

        // Run 120 ticks (2 seconds at 60 Hz)
        for _ in 0..120 {
            instance.tick();
        }

        // Get final position
        let final_pos = instance.physics.get_character_position(hrp_id).unwrap();
        println!("Final player position: {:?}", final_pos);

        // Player should have moved towards target (10, 5, 10)
        let moved_x = final_pos[0] - initial_pos[0];
        let moved_z = final_pos[2] - initial_pos[2];
        let horizontal_distance = (moved_x * moved_x + moved_z * moved_z).sqrt();

        println!("Horizontal distance moved: {}", horizontal_distance);

        // At 16 studs/sec for 2 seconds, should move up to 32 studs
        // Target is ~14.14 studs away horizontally, should reach it
        assert!(horizontal_distance > 5.0, "Player should have moved significantly towards target");
    }

    #[test]
    fn test_player_moveto_via_lua_input() {
        // This test mimics what the Python test does: send MoveTo via agent input
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Load a simple script that creates a floor AND handles MoveTo input
        instance.load_script(r#"
            local AgentInputService = game:GetService("AgentInputService")
            local Players = game:GetService("Players")

            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            if AgentInputService then
                AgentInputService.InputReceived:Connect(function(player, inputType, data)
                    if inputType == "MoveTo" then
                        local humanoid = player.Character and player.Character:FindFirstChild("Humanoid")
                        if humanoid and data and data.position then
                            local pos = data.position
                            print("[Test] MoveTo " .. player.Name .. " -> (" .. pos[1] .. ", " .. pos[2] .. ", " .. pos[3] .. ")")
                            humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
                        end
                    end
                end)
            end
        "#);

        // Add a player
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id, "TestPlayer"));

        let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();

        // Don't wait for landing - queue MoveTo immediately like the working test
        let initial_pos = instance.physics.get_character_position(hrp_id).unwrap();
        println!("Initial position: {:?}", initial_pos);

        // Queue a MoveTo input via agent input service (like Python test does)
        let user_id = *instance.players.get(&agent_id).unwrap();
        instance.queue_agent_input(user_id, "MoveTo".to_string(), serde_json::json!({"position": [20.0, initial_pos[1], 0.0]}));

        // Run 120 ticks (2 seconds)
        for i in 0..120 {
            instance.tick();
            if i % 30 == 0 {
                let pos = instance.physics.get_character_position(hrp_id).unwrap();
                println!("Tick {}: pos=({:.2}, {:.2}, {:.2})", i, pos[0], pos[1], pos[2]);
            }
        }

        let final_pos = instance.physics.get_character_position(hrp_id).unwrap();
        println!("Final position: {:?}", final_pos);

        let moved_x = final_pos[0] - initial_pos[0];
        let moved_z = final_pos[2] - initial_pos[2];
        let horizontal_distance = (moved_x * moved_x + moved_z * moved_z).sqrt();
        println!("Horizontal distance moved: {}", horizontal_distance);

        // Should move significantly (at 16 studs/sec, should move ~32 studs in 2 sec)
        assert!(horizontal_distance > 10.0, "Player should have moved towards target via Lua MoveTo");
    }

    #[test]
    fn test_observation_includes_world_entities() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Load a script that creates some geometry
        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            local platform = Instance.new("Part")
            platform.Name = "CenterPlatform"
            platform.Size = Vector3.new(10, 1, 10)
            platform.Position = Vector3.new(0, 5, 0)
            platform.Anchored = true
            platform.Parent = Workspace
        "#);

        let agent_id = Uuid::new_v4();
        instance.add_player(agent_id, "TestPlayer");

        // Run a tick to sync physics
        instance.tick();

        let obs = instance.get_player_observation(agent_id).unwrap();

        // Should have world entities
        assert!(!obs.world.entities.is_empty(), "World should contain entities");

        // Should have Floor and CenterPlatform
        let names: Vec<&str> = obs.world.entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Floor"), "Should contain Floor");
        assert!(names.contains(&"CenterPlatform"), "Should contain CenterPlatform");

        // Check Floor properties
        let floor = obs.world.entities.iter().find(|e| e.name == "Floor").unwrap();
        assert_eq!(floor.size, [100.0, 1.0, 100.0]);
        assert!(floor.anchored);
    }

    #[test]
    fn test_los_blocked_by_wall() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Create a wall between two player spawn points
        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            local wall = Instance.new("Part")
            wall.Name = "Wall"
            wall.Size = Vector3.new(20, 10, 2)
            wall.Position = Vector3.new(0, 5, 0)
            wall.Anchored = true
            wall.Parent = Workspace
        "#);

        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();
        instance.add_player(agent_a, "PlayerA");
        instance.add_player(agent_b, "PlayerB");

        // Run a tick to sync physics
        instance.tick();

        // Position players on opposite sides of the wall
        // Player A at z=-10, Player B at z=+10, wall at z=0
        let hrp_a = *instance.player_hrp_ids.get(&agent_a).unwrap();
        let hrp_b = *instance.player_hrp_ids.get(&agent_b).unwrap();

        // Ensure characters have physics bodies initialized
        instance.physics.set_character_position(hrp_a, [0.0, 2.0, -10.0]);
        instance.physics.set_character_position(hrp_b, [0.0, 2.0, 10.0]);

        // Manually set positions behind the wall
        // Update query pipeline
        instance.physics.query_pipeline.update(&instance.physics.collider_set);

        let obs = instance.get_player_observation(agent_a).unwrap();

        // Agent A should NOT see Agent B (blocked by wall)
        assert!(obs.other_players.is_empty(), "Should not see player through wall");
    }

    #[test]
    fn test_los_clear() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Just a floor, no obstructions
        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace
        "#);

        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();
        instance.add_player(agent_a, "PlayerA");
        instance.add_player(agent_b, "PlayerB");

        // Run several ticks to sync physics
        for _ in 0..5 {
            instance.tick();
        }

        // Position players at same height on the floor with clear LOS
        let hrp_a = *instance.player_hrp_ids.get(&agent_a).unwrap();
        let hrp_b = *instance.player_hrp_ids.get(&agent_b).unwrap();

        if let Some(state) = instance.physics.get_character_state(hrp_a) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![-10.0, 2.0, 0.0], true);
            }
        }
        if let Some(state) = instance.physics.get_character_state(hrp_b) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![10.0, 2.0, 0.0], true);
            }
        }

        // Update query pipeline
        instance.physics.query_pipeline.update(&instance.physics.collider_set);

        let obs = instance.get_player_observation(agent_a).unwrap();

        // Agent A should see Agent B (clear LOS)
        assert_eq!(obs.other_players.len(), 1, "Should see other player with clear LOS");
        assert_eq!(obs.other_players[0].id, agent_b);
    }

    #[test]
    fn test_shooting_and_killing() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Load a minimal shooting test script (no debug prints, fast startup)
        instance.load_script(r#"
            local Players = game:GetService("Players")
            local AgentInputService = game:GetService("AgentInputService")

            -- Create floor
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            -- Initialize player weapons
            Players.PlayerAdded:Connect(function(player)
                player:SetAttribute("CurrentWeapon", 1)
                player:SetAttribute("Kills", 0)
            end)

            -- Handle Fire input
            if AgentInputService then
                AgentInputService.InputReceived:Connect(function(player, inputType, data)
                    if inputType == "Fire" and data and data.direction then
                        local character = player.Character
                        if not character then return end

                        local hrp = character:FindFirstChild("HumanoidRootPart")
                        if not hrp then return end

                        local origin = hrp.Position + Vector3.new(0, 1, 0)
                        local dir = Vector3.new(data.direction[1], data.direction[2], data.direction[3])

                        -- Raycast to find target
                        local raycastParams = RaycastParams.new()
                        raycastParams.FilterType = Enum.RaycastFilterType.Exclude
                        raycastParams.FilterDescendantsInstances = {character}

                        local result = Workspace:Raycast(origin, dir * 100, raycastParams)
                        if result then
                            -- Check if we hit a player
                            local hitPart = result.Instance
                            local current = hitPart
                            while current do
                                if current:FindFirstChild("Humanoid") then
                                    -- Found a character, deal damage
                                    local humanoid = current:FindFirstChild("Humanoid")
                                    humanoid:TakeDamage(25)
                                    print("[HIT] Dealt 25 damage, health now:", humanoid.Health)

                                    if humanoid.Health <= 0 then
                                        player:SetAttribute("Kills", (player:GetAttribute("Kills") or 0) + 1)
                                        player:SetAttribute("CurrentWeapon", (player:GetAttribute("CurrentWeapon") or 1) + 1)
                                        print("[KILL] Player got kill, weapon now:", player:GetAttribute("CurrentWeapon"))
                                    end
                                    break
                                end
                                current = current.Parent
                            end
                        else
                            print("[MISS] Raycast hit nothing")
                        end
                    end
                end)
            end
            print("Shooting test script loaded")
        "#);

        // Add two players
        let attacker_id = Uuid::new_v4();
        let victim_id = Uuid::new_v4();
        instance.add_player(attacker_id, "Attacker");
        instance.add_player(victim_id, "Victim");

        let attacker_user_id = *instance.players.get(&attacker_id).unwrap();

        // Run a few ticks to initialize
        for _ in 0..5 {
            instance.tick();
        }

        // Position players facing each other
        let hrp_attacker = *instance.player_hrp_ids.get(&attacker_id).unwrap();
        let hrp_victim = *instance.player_hrp_ids.get(&victim_id).unwrap();

        if let Some(state) = instance.physics.get_character_state(hrp_attacker) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![-5.0, 2.0, 0.0], true);
            }
        }
        if let Some(state) = instance.physics.get_character_state(hrp_victim) {
            if let Some(body) = instance.physics.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(rapier3d::prelude::vector![5.0, 2.0, 0.0], true);
            }
        }

        // Sync physics to Lua
        instance.tick();

        let initial_health = instance.get_player_health(victim_id).unwrap_or(100);

        // Fire towards victim (+X direction)
        let fire_input = serde_json::json!({ "direction": [1.0, 0.0, 0.0] });
        instance.queue_agent_input(attacker_user_id, "Fire".to_string(), fire_input);

        instance.tick();

        let health_after_shot = instance.get_player_health(victim_id).unwrap_or(100);

        assert!(
            health_after_shot < initial_health,
            "Victim should have taken damage: {} -> {}",
            initial_health,
            health_after_shot
        );

        // Keep shooting until dead
        let mut shots = 1;
        while instance.get_player_health(victim_id).unwrap_or(0) > 0 && shots < 10 {
            let fire_input = serde_json::json!({ "direction": [1.0, 0.0, 0.0] });
            instance.queue_agent_input(attacker_user_id, "Fire".to_string(), fire_input);
            instance.tick();
            shots += 1;
        }

        let final_health = instance.get_player_health(victim_id).unwrap_or(0);
        assert!(final_health <= 0, "Victim should be dead after {} shots", shots);

        // Check attacker stats
        if let Some(runtime) = &instance.lua_runtime {
            if let Some(player) = runtime.players().get_player_by_user_id(attacker_user_id) {
                let data = player.data.lock().unwrap();

                // Verify kills attribute was set to 1
                if let Some(AttributeValue::Number(kills)) = data.attributes.get("Kills") {
                    assert_eq!(*kills as i32, 1, "Expected 1 kill");
                } else {
                    panic!("Kills attribute not found or not a number");
                }

                // Verify weapon was upgraded to 2
                if let Some(AttributeValue::Number(weapon)) = data.attributes.get("CurrentWeapon") {
                    assert_eq!(*weapon as i32, 2, "Expected weapon to be upgraded to 2");
                } else {
                    panic!("CurrentWeapon attribute not found or not a number");
                }
            }
        }
    }

    #[test]
    fn test_player_kick_from_lua() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Load a script that kicks a player after they join
        instance.load_script(r#"
            local Players = game:GetService("Players")

            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            _G.kickCalled = false

            Players.PlayerAdded:Connect(function(player)
                -- Schedule kick after a few ticks
                _G.playerToKick = player
            end)
        "#);

        // Add a player
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id, "TestPlayer"));
        assert_eq!(instance.players.len(), 1);

        // Run a few ticks to process PlayerAdded
        for _ in 0..5 {
            instance.tick();
        }

        // Player should still be there
        assert_eq!(instance.players.len(), 1);

        // Now trigger the kick via Lua
        if let Some(runtime) = &instance.lua_runtime {
            let lua = runtime.lua();
            lua.load(r#"
                if _G.playerToKick then
                    _G.playerToKick:Kick("Test kick")
                    _G.kickCalled = true
                end
            "#).exec().expect("Failed to run kick script");
        }

        // Run tick to process kick request
        instance.tick();

        // Player should be removed
        assert_eq!(instance.players.len(), 0, "Player should have been kicked");
    }

    #[test]
    fn test_afk_timeout() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Set a very short AFK timeout for testing (100ms)
        instance.set_afk_timeout(Duration::from_millis(100));

        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace
        "#);

        // Add a player
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id, "AFKPlayer"));
        assert_eq!(instance.players.len(), 1);

        // Run some ticks without activity - but not enough to trigger AFK check
        for _ in 0..30 {
            instance.tick();
        }

        // Player should still be there (AFK check happens every 60 ticks)
        assert_eq!(instance.players.len(), 1);

        // Sleep to exceed AFK timeout
        std::thread::sleep(Duration::from_millis(150));

        // Run 60 ticks to trigger AFK check
        for _ in 0..60 {
            instance.tick();
        }

        // Player should be kicked for being AFK
        assert_eq!(instance.players.len(), 0, "Player should have been kicked for AFK");
    }

    #[test]
    fn test_activity_resets_afk_timer() {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Set a longer AFK timeout for this test to avoid race conditions (200ms)
        instance.set_afk_timeout(Duration::from_millis(200));

        instance.load_script(r#"
            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(100, 1, 100)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace
        "#);

        // Add a player
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id, "ActivePlayer"));
        assert_eq!(instance.players.len(), 1);

        // Sleep almost to timeout
        std::thread::sleep(Duration::from_millis(150));

        // Record activity to reset AFK timer (before timeout)
        instance.record_player_activity(agent_id);

        // Process a tick
        instance.tick();

        // Sleep less than the full timeout again
        std::thread::sleep(Duration::from_millis(100));

        // Run AFK check (60 ticks)
        for _ in 0..60 {
            instance.tick();
        }

        // Player should still be there (activity reset the timer)
        // Total time since reset: ~100ms, which is less than 200ms timeout
        assert_eq!(instance.players.len(), 1, "Player should NOT be kicked - activity reset timer");
    }

    #[test]
    fn test_humanoid_walk_speed_affects_movement() {
        // Test that custom WalkSpeed on humanoid affects actual movement speed
        let mut instance = GameInstance::new(Uuid::new_v4(), None);

        // Load a script that creates a floor and handles SetSpeed input
        instance.load_script(r#"
            local AgentInputService = game:GetService("AgentInputService")

            local floor = Instance.new("Part")
            floor.Name = "Floor"
            floor.Size = Vector3.new(200, 1, 200)
            floor.Position = Vector3.new(0, 0, 0)
            floor.Anchored = true
            floor.Parent = Workspace

            -- Handle SetSpeed input to change WalkSpeed
            if AgentInputService then
                AgentInputService.InputReceived:Connect(function(player, inputType, data)
                    if inputType == "SetSpeed" and data and data.speed then
                        local humanoid = player.Character and player.Character:FindFirstChild("Humanoid")
                        if humanoid then
                            humanoid.WalkSpeed = data.speed
                            print("[Test] Set WalkSpeed to " .. data.speed)
                        end
                    end
                end)
            end
        "#);

        // Add a player
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id, "SpeedTestPlayer"));

        let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();
        let user_id = *instance.players.get(&agent_id).unwrap();

        // Verify default walk speed is returned
        let default_speed = instance.get_humanoid_walk_speed(agent_id);
        assert!(default_speed.is_some(), "Should be able to get humanoid walk speed");
        assert_eq!(default_speed.unwrap(), 16.0, "Default walk speed should be 16.0");

        // Get initial position and set movement target
        let initial_pos = instance.physics.get_character_position(hrp_id).unwrap();
        instance.physics.set_character_target(hrp_id, Some([50.0, initial_pos[1], 0.0]));

        // Run 60 ticks (1 second at default speed)
        for _ in 0..60 {
            instance.tick();
        }

        let pos_after_default = instance.physics.get_character_position(hrp_id).unwrap();
        let distance_default = (pos_after_default[0] - initial_pos[0]).abs();
        println!("Distance moved at default speed (16): {}", distance_default);

        // Now set a higher WalkSpeed via agent input (simulating speed upgrade)
        instance.queue_agent_input(user_id, "SetSpeed".to_string(), serde_json::json!({"speed": 32.0}));

        // Process the input
        instance.tick();

        // Verify the new walk speed is returned
        let new_speed = instance.get_humanoid_walk_speed(agent_id);
        assert!(new_speed.is_some(), "Should still be able to get humanoid walk speed");
        assert_eq!(new_speed.unwrap(), 32.0, "Walk speed should now be 32.0");

        // Reset position and move again
        let start_pos = instance.physics.get_character_position(hrp_id).unwrap();
        instance.physics.set_character_target(hrp_id, Some([start_pos[0] + 50.0, start_pos[1], start_pos[2]]));

        // Run another 60 ticks (1 second at double speed)
        for _ in 0..60 {
            instance.tick();
        }

        let pos_after_fast = instance.physics.get_character_position(hrp_id).unwrap();
        let distance_fast = (pos_after_fast[0] - start_pos[0]).abs();
        println!("Distance moved at fast speed (32): {}", distance_fast);

        // Player should move roughly twice as far with double speed
        // Allow some tolerance for physics/rounding
        assert!(
            distance_fast > distance_default * 1.5,
            "Player should move significantly faster with higher WalkSpeed. Default: {}, Fast: {}",
            distance_default,
            distance_fast
        );
    }
}

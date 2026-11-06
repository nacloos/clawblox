use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::async_bridge::AsyncBridge;
use super::lua::instance::{
    attributes_to_json, AttributeValue, ClassName, Instance, TextXAlignment, TextYAlignment,
};
use super::lua::LuaRuntime;
use super::physics::PhysicsWorld;
use super::profiling::ProfileFrame;
use crate::config::PhysicsConfig;

mod body_movers;
mod character_controller;
mod controller_runtime;
mod fluid_physics;
mod modern_actuators;
mod observation;
mod physics_commands;
#[cfg(test)]
mod physics_stability_tests;
mod player_lifecycle;
mod tick_pipeline;
mod workspace_physics;

pub use observation::{
    BillboardGuiJson, BillboardLabelJson, EntityAabb, EntityObb, GameEvent, GuiElement, MapInfo,
    OtherPlayerInfo, PlayerCameraPose, PlayerInfo, PlayerObservation, ReplicationObservationUpdate,
    SpectatorEntity,
    SpectatorObservation, SpectatorPlayerAnimation, SpectatorPlayerInfo, SpectatorRender,
    SpectatorReplicatedEvent, SpectatorSound, UDim2Json, VisibleEntity, WorldEntity, WorldInfo,
    PLAYER_EYE_HEIGHT,
};

/// Default AFK timeout in seconds (5 minutes)
const DEFAULT_AFK_TIMEOUT_SECS: u64 = 300;

/// How often to check for AFK players (in ticks, 60 = 1 second)
const AFK_CHECK_INTERVAL_TICKS: u64 = 60;

/// Default max players when not specified
pub(crate) const DEFAULT_MAX_PLAYERS: u32 = 8;
/// Default visibility radius for other players in observations.
pub(crate) const DEFAULT_OBSERVATION_RADIUS: f32 = 100.0;
const MAX_REPLICATED_EVENTS_BUFFER: usize = 512;
const MAX_RECENT_EVENTS_PER_OBSERVATION: usize = 128;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WorkspaceFeatureCounts {
    pub body_movers: usize,
    pub vector_forces: usize,
    pub align_constraints: usize,
    pub weld_constraints: usize,
    pub terrain_with_water: usize,
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
    /// Maximum distance for player-to-player observation visibility.
    pub observation_radius: f32,
    /// Whether player character controllers physically couple to dynamic world objects.
    player_object_physics: bool,
    /// When the instance became empty (for garbage collection)
    /// None if instance has players, Some(Instant) when last player left
    pub empty_since: Option<Instant>,
    /// How Lua errors are handled (Continue = log and keep going, Halt = stop on first error)
    pub error_mode: ErrorMode,
    /// Set when error_mode is Halt and a Lua error occurs; prevents further ticking
    pub halted_error: Option<String>,
    /// Previous frame's touch pairs for Touched/TouchEnded event detection
    prev_touches: HashSet<(u64, u64)>,
    /// Dynamic physics parts whose final sleeping state has already been
    /// reflected back into Lua part fields.
    physics_sleep_synced_parts: HashSet<u64>,
    /// Feature counts gathered during the current tick's Workspace scan.
    /// These let later physics phases skip full-tree scans when no matching
    /// class exists while preserving exact behavior when any feature is present.
    workspace_feature_counts: WorkspaceFeatureCounts,
    /// Ring buffer of recent replicated transient events (shots, impacts, etc.)
    replicated_events: VecDeque<SpectatorReplicatedEvent>,
    /// Monotonic event id assigned by server for dedupe/order.
    next_replicated_event_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GameStatus {
    Waiting,
    Playing,
    Finished,
}

/// Controls how Lua errors are handled by a GameInstance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorMode {
    /// Log errors and continue (production server behavior)
    Continue,
    /// Stop the instance on first Lua error (CLI dev mode)
    Halt,
}

impl GameInstance {
    /// Derive a stable numeric user_id from an agent UUID (fits Lua safe integer range < 2^53).
    pub(crate) fn user_id_from_agent_id(agent_id: Uuid) -> u64 {
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
        Self::new_with_config(
            game_id,
            DEFAULT_MAX_PLAYERS,
            DEFAULT_OBSERVATION_RADIUS,
            async_bridge,
            ErrorMode::Continue,
        )
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
        observation_radius: f32,
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
            observation_radius,
            player_object_physics: true,
            empty_since: Some(Instant::now()), // Starts empty
            error_mode,
            halted_error: None,
            prev_touches: HashSet::new(),
            physics_sleep_synced_parts: HashSet::new(),
            workspace_feature_counts: WorkspaceFeatureCounts::default(),
            replicated_events: VecDeque::new(),
            next_replicated_event_id: 1,
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

    pub fn set_player_object_physics(&mut self, enabled: bool) {
        self.player_object_physics = enabled;
    }

    pub fn player_object_physics(&self) -> bool {
        self.player_object_physics
    }

    pub fn set_physics_config(&mut self, config: &PhysicsConfig) {
        self.physics.set_solver_iterations(config.solver_iterations);
    }

    /// Returns milliseconds since game instance was created
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    pub(crate) fn restore_elapsed_ms(&mut self, elapsed_ms: u64) {
        let now = Instant::now();
        self.start_time = now
            .checked_sub(Duration::from_millis(elapsed_ms))
            .unwrap_or(now);
    }

    pub(crate) fn snapshot_afk_timeout_ms(&self) -> u64 {
        self.afk_timeout.as_millis() as u64
    }

    pub(crate) fn restore_afk_timeout_ms(&mut self, timeout_ms: u64) {
        self.afk_timeout = Duration::from_millis(timeout_ms);
    }

    pub(crate) fn snapshot_empty_since_elapsed_ms(&self) -> Option<u64> {
        self.empty_since
            .and_then(|empty_since| Instant::now().checked_duration_since(empty_since))
            .map(|elapsed| elapsed.as_millis() as u64)
    }

    pub(crate) fn restore_empty_since_elapsed_ms(&mut self, elapsed_ms: Option<u64>) {
        let now = Instant::now();
        self.empty_since = elapsed_ms.map(|elapsed_ms| {
            now.checked_sub(Duration::from_millis(elapsed_ms))
                .unwrap_or(now)
        });
    }

    pub(crate) fn snapshot_player_idle_ms(&self, agent_id: Uuid) -> Option<u64> {
        self.player_last_activity
            .get(&agent_id)
            .and_then(|last_active| Instant::now().checked_duration_since(*last_active))
            .map(|elapsed| elapsed.as_millis() as u64)
    }

    pub(crate) fn restore_player_idle_ms(&mut self, agent_id: Uuid, idle_ms: u64) {
        let now = Instant::now();
        let last_active = now
            .checked_sub(Duration::from_millis(idle_ms))
            .unwrap_or(now);
        self.player_last_activity.insert(agent_id, last_active);
    }

    pub(crate) fn snapshot_replicated_events(&self) -> (u64, Vec<SpectatorReplicatedEvent>) {
        (
            self.next_replicated_event_id,
            self.replicated_events.iter().cloned().collect(),
        )
    }

    pub(crate) fn restore_replicated_events(
        &mut self,
        next_id: u64,
        events: Vec<SpectatorReplicatedEvent>,
    ) {
        self.next_replicated_event_id = next_id.max(1);
        self.replicated_events = events
            .into_iter()
            .rev()
            .take(MAX_REPLICATED_EVENTS_BUFFER)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }

    pub(crate) fn snapshot_prev_touches(&self) -> Vec<(u64, u64)> {
        let mut touches = self.prev_touches.iter().copied().collect::<Vec<_>>();
        touches.sort_unstable();
        touches
    }

    pub(crate) fn restore_prev_touches(&mut self, touches: Vec<(u64, u64)>) {
        self.prev_touches = touches.into_iter().collect();
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
        Self::new_with_script_and_config(
            game_id,
            script,
            DEFAULT_MAX_PLAYERS,
            DEFAULT_OBSERVATION_RADIUS,
            async_bridge,
            ErrorMode::Continue,
        )
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
        observation_radius: f32,
        async_bridge: Option<Arc<AsyncBridge>>,
        error_mode: ErrorMode,
    ) -> Self {
        let mut instance = Self::new_with_config(
            game_id,
            max_players,
            observation_radius,
            async_bridge,
            error_mode,
        );
        instance.load_script(script);
        instance
    }

    /// Initializes the Lua runtime without loading any world script.
    pub fn init_runtime(&mut self) {
        if self.lua_runtime.is_some() {
            return;
        }

        match LuaRuntime::with_config(self.game_id, self.max_players, self.async_bridge.clone()) {
            Ok(runtime) => {
                runtime.lua().set_app_data(self.error_mode);
                self.lua_runtime = Some(runtime);
            }
            Err(e) => {
                self.handle_lua_error("Failed to create runtime", &e);
            }
        }
    }

    /// Loads and executes a Lua script
    pub fn load_script(&mut self, source: &str) {
        self.init_runtime();
        if let Some(runtime) = self.lua_runtime.as_mut() {
            if let Err(e) = runtime.load_script(source) {
                self.handle_lua_error("Failed to load script", &e);
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

    /// Main game loop tick - called at 60 Hz
    pub fn tick(&mut self) {
        let mut profile = ProfileFrame::new("instance.tick");
        // In Halt mode, stop ticking once an error has occurred
        if self.halted_error.is_some() {
            return;
        }

        let dt = 1.0 / 60.0;
        // Process kick requests from Lua scripts (e.g., Player:Kick())
        self.process_kick_requests();
        if let Some(profile) = profile.as_mut() {
            profile.mark("kick_requests");
        }

        // Check for AFK players periodically (every second)
        if self.tick % AFK_CHECK_INTERVAL_TICKS == 0 {
            self.check_afk_players();
        }
        if let Some(profile) = profile.as_mut() {
            profile.mark("afk");
        }

        self.process_pending_bot_requests();
        if let Some(profile) = profile.as_mut() {
            profile.mark("bot_requests");
        }

        // Run start-of-frame Lua work before physics (resume yields + Stepped).
        if let Some(runtime) = &self.lua_runtime {
            if let Err(e) = runtime.begin_frame(dt) {
                self.handle_lua_error("Begin frame error", &e);
                if self.halted_error.is_some() {
                    return;
                }
            }
        }
        if let Some(profile) = profile.as_mut() {
            profile.mark("begin_frame");
        }

        tick_pipeline::run_tick_phases(self, dt);
        if let Some(profile) = profile.as_mut() {
            profile.mark("tick_phases");
        }
        if self.halted_error.is_some() {
            return;
        }

        // Run end-of-frame Lua work after physics (Heartbeat).
        if let Some(runtime) = &self.lua_runtime {
            if let Err(e) = runtime.end_frame(dt) {
                self.handle_lua_error("End frame error", &e);
                if self.halted_error.is_some() {
                    return;
                }
            }
        }
        if let Some(profile) = profile.as_mut() {
            profile.mark("end_frame");
        }

        self.process_replicated_events();
        if let Some(profile) = profile.as_mut() {
            profile.mark("replicated_events");
        }

        self.tick += 1;
        if let Some(profile) = profile {
            profile.finish();
        }
    }

    fn process_replicated_events(&mut self) {
        let Some(runtime) = &self.lua_runtime else {
            return;
        };
        let queued = runtime.game().remote_event_service().drain_events();
        if queued.is_empty() {
            return;
        }

        let tick = self.tick;
        let server_time_ms = self.elapsed_ms();
        for event in queued {
            let id = self.next_replicated_event_id;
            self.next_replicated_event_id = self.next_replicated_event_id.saturating_add(1);
            self.replicated_events.push_back(SpectatorReplicatedEvent {
                id,
                tick,
                server_time_ms,
                name: event.name,
                reliable: event.reliable,
                payload: event.payload,
                target_user_id: event.target_user_id,
            });
        }

        while self.replicated_events.len() > MAX_REPLICATED_EVENTS_BUFFER {
            self.replicated_events.pop_front();
        }
    }

    fn recent_public_replicated_events(&self) -> Vec<SpectatorReplicatedEvent> {
        let count = self
            .replicated_events
            .len()
            .min(MAX_RECENT_EVENTS_PER_OBSERVATION);
        self.replicated_events
            .iter()
            .filter(|evt| evt.target_user_id.is_none())
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    fn recent_replicated_events_for_user(&self, user_id: u64) -> Vec<SpectatorReplicatedEvent> {
        self.recent_replicated_events_filtered(Some(user_id))
    }

    fn recent_replicated_events_filtered(
        &self,
        user_id: Option<u64>,
    ) -> Vec<SpectatorReplicatedEvent> {
        let count = self
            .replicated_events
            .len()
            .min(MAX_RECENT_EVENTS_PER_OBSERVATION);
        self.replicated_events
            .iter()
            .filter(|evt| match (evt.target_user_id, user_id) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(target), Some(viewer)) => target == viewer,
            })
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Sync control targets from script state into physics character controllers.
    fn sync_controller_targets(&mut self) {
        controller_runtime::sync_move_targets(self);
    }

    /// Updates character controller movement towards targets.
    /// Uses Rapier's kinematic character controller for full 3D translation.
    fn update_character_movement(&mut self, dt: f32) {
        controller_runtime::update_character_movement(self, dt);
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

    #[cfg(test)]
    /// Get walk speed from the player's controller source.
    fn get_humanoid_walk_speed(&self, agent_id: Uuid) -> Option<f32> {
        controller_runtime::get_humanoid_walk_speed(self, agent_id)
    }
}

#[cfg(test)]
mod tests;

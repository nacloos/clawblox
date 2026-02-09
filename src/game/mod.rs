pub mod async_bridge;
pub mod constants;
pub mod humanoid_movement;
pub mod instance;
pub mod lua;
pub mod physics;
pub mod script_bundle;
pub mod touch_events;
mod manager_instances;
mod manager_lifecycle;
mod manager_listing;
mod manager_observations;
mod manager_players;
mod manager_tick;

use dashmap::DashMap;
use parking_lot::RwLock;
use sqlx::PgPool;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use async_bridge::AsyncBridge;
use instance::{ErrorMode, GameInstance, MapInfo, PlayerObservation, SpectatorObservation};

/// Handle to the game manager state
pub type GameManagerHandle = Arc<GameManagerState>;

/// Per-instance lock wrapper
pub type GameInstanceHandle = Arc<RwLock<GameInstance>>;

/// Default timeout before an empty instance is destroyed (60 seconds)
const EMPTY_INSTANCE_TIMEOUT: Duration = Duration::from_secs(60);

/// Cleanup interval in ticks (60 ticks = 1 second at 60 Hz)
const CLEANUP_INTERVAL_TICKS: u64 = 60;

pub struct GameManagerState {
    /// Running instances, keyed by instance_id
    pub instances: DashMap<Uuid, GameInstanceHandle>,
    /// Maps game_id to list of active instance_ids
    pub game_instances: DashMap<Uuid, Vec<Uuid>>,
    /// Maps (agent_id, game_id) to instance_id for routing
    pub player_instances: DashMap<(Uuid, Uuid), Uuid>,
    /// Cached observations, keyed by (instance_id, agent_id)
    pub observation_cache: DashMap<(Uuid, Uuid), PlayerObservation>,
    /// Cached spectator observations, keyed by instance_id
    pub spectator_cache: DashMap<Uuid, SpectatorObservation>,
    /// Cached static map geometry, keyed by game_id (same for all instances of a game)
    pub map_cache: DashMap<Uuid, MapInfo>,
    /// Shared async bridge for database operations
    pub async_bridge: Option<Arc<AsyncBridge>>,
    /// Error mode for new instances (Halt for CLI dev, Continue for production)
    pub error_mode: ErrorMode,
    /// When true, skip garbage collection of empty instances (used by CLI)
    pub disable_gc: bool,
}

impl GameManagerState {
    pub fn new(async_bridge: Option<Arc<AsyncBridge>>, error_mode: ErrorMode, disable_gc: bool) -> Self {
        Self {
            instances: DashMap::new(),
            game_instances: DashMap::new(),
            player_instances: DashMap::new(),
            observation_cache: DashMap::new(),
            spectator_cache: DashMap::new(),
            map_cache: DashMap::new(),
            async_bridge,
            error_mode,
            disable_gc,
        }
    }
}

pub struct GameManager {
    state: GameManagerHandle,
    tick_rate: u64,
}

impl GameManager {
    pub fn new(tick_rate: u64, pool: Arc<PgPool>, error_mode: ErrorMode) -> (Self, GameManagerHandle) {
        let async_bridge = Arc::new(AsyncBridge::new(pool));
        let state = Arc::new(GameManagerState::new(Some(async_bridge), error_mode, false));
        let handle = Arc::clone(&state);
        (Self { state, tick_rate }, handle)
    }

    pub fn new_without_db(tick_rate: u64, error_mode: ErrorMode) -> (Self, GameManagerHandle) {
        let state = Arc::new(GameManagerState::new(None, error_mode, true));
        let handle = Arc::clone(&state);
        (Self { state, tick_rate }, handle)
    }

    pub fn run(self) {
        let tick_duration = Duration::from_millis(1000 / self.tick_rate);
        let mut tick_counter: u64 = 0;

        loop {
            let start = Instant::now();

            manager_tick::tick_instances(&self.state);

            // Periodic cleanup
            tick_counter += 1;
            if tick_counter % CLEANUP_INTERVAL_TICKS == 0 && !self.state.disable_gc {
                let destroyed = cleanup_empty_instances(&self.state);
                if destroyed > 0 {
                    eprintln!("[Cleanup] Destroyed {} empty instances", destroyed);
                }
            }

            let elapsed = start.elapsed();
            if elapsed < tick_duration {
                thread::sleep(tick_duration - elapsed);
            }
        }
    }
}

// =============================================================================
// Instance Management
// =============================================================================

#[derive(Debug, Clone)]
pub struct FindInstanceResult {
    pub instance_id: Uuid,
    pub created: bool,
}

/// Finds an instance with capacity or creates a new one
pub fn find_or_create_instance(
    state: &GameManagerHandle,
    game_id: Uuid,
    max_players: u32,
    script: Option<&str>,
) -> FindInstanceResult {
    manager_instances::find_or_create_instance(state, game_id, max_players, script)
}

/// Checks if any instance is running for this game
pub fn is_instance_running(state: &GameManagerHandle, game_id: Uuid) -> bool {
    manager_instances::is_instance_running(state, game_id)
}

/// Gets the instance_id a player is in for a specific game
pub fn get_player_instance(
    state: &GameManagerHandle,
    agent_id: Uuid,
    game_id: Uuid,
) -> Option<Uuid> {
    manager_instances::get_player_instance(state, agent_id, game_id)
}

// =============================================================================
// Player Management
// =============================================================================

/// Joins a player to an instance (with capacity check)
pub fn join_instance(
    state: &GameManagerHandle,
    instance_id: Uuid,
    game_id: Uuid,
    agent_id: Uuid,
    agent_name: &str,
) -> Result<(), String> {
    manager_players::join_instance(state, instance_id, game_id, agent_id, agent_name)
}

/// Leaves a player from an instance
pub fn leave_instance(
    state: &GameManagerHandle,
    instance_id: Uuid,
    agent_id: Uuid,
) -> Result<(), String> {
    manager_players::leave_instance(state, instance_id, agent_id)
}

/// Leaves a player from their instance in a game (lookup by game_id)
pub fn leave_game(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
) -> Result<(), String> {
    let instance_id = get_player_instance(state, agent_id, game_id)
        .ok_or_else(|| "Not in any instance of this game".to_string())?;
    leave_instance(state, instance_id, agent_id)
}

// =============================================================================
// Input
// =============================================================================

pub fn queue_input(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
    input_type: String,
    data: serde_json::Value,
) -> Result<(), String> {
    manager_players::queue_input(state, game_id, agent_id, input_type, data)
}

// =============================================================================
// Observations
// =============================================================================

pub fn get_observation(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
) -> Result<PlayerObservation, String> {
    manager_observations::get_observation(state, game_id, agent_id)
}

pub fn get_spectator_observation(
    state: &GameManagerHandle,
    game_id: Uuid,
) -> Result<SpectatorObservation, String> {
    manager_observations::get_spectator_observation(state, game_id)
}

pub fn get_spectator_observation_for_instance(
    state: &GameManagerHandle,
    instance_id: Uuid,
) -> Result<SpectatorObservation, String> {
    manager_observations::get_spectator_observation_for_instance(state, instance_id)
}

/// Get static map geometry for a game (cached per game_id)
pub fn get_map(
    state: &GameManagerHandle,
    game_id: Uuid,
) -> Result<MapInfo, String> {
    manager_observations::get_map(state, game_id)
}

// =============================================================================
// Info & Listing
// =============================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct GameInfo {
    pub id: Uuid,
    pub status: String,
    pub player_count: usize,
    pub tick: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstanceInfo {
    pub instance_id: Uuid,
    pub game_id: Uuid,
    pub status: String,
    pub player_count: usize,
    pub max_players: usize,
    pub tick: u64,
}

pub fn list_instances(state: &GameManagerHandle) -> Vec<InstanceInfo> {
    manager_listing::list_instances(state)
}

pub fn list_games(state: &GameManagerHandle) -> Vec<GameInfo> {
    manager_listing::list_games(state)
}

pub fn get_game_info(state: &GameManagerHandle, game_id: Uuid) -> Option<GameInfo> {
    manager_listing::get_game_info(state, game_id)
}

// =============================================================================
// Instance Lifecycle
// =============================================================================

pub fn destroy_instance(state: &GameManagerHandle, instance_id: Uuid) -> bool {
    manager_lifecycle::destroy_instance(state, instance_id)
}

pub fn cleanup_empty_instances(state: &GameManagerHandle) -> usize {
    cleanup_empty_instances_with_timeout(state, EMPTY_INSTANCE_TIMEOUT)
}

pub fn cleanup_empty_instances_with_timeout(
    state: &GameManagerHandle,
    timeout: Duration,
) -> usize {
    manager_lifecycle::cleanup_empty_instances_with_timeout(state, timeout)
}

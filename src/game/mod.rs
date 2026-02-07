pub mod async_bridge;
pub mod constants;
pub mod instance;
pub mod lua;
pub mod physics;

use dashmap::DashMap;
use parking_lot::RwLock;
use rayon::prelude::*;
use sqlx::PgPool;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use async_bridge::AsyncBridge;
use instance::{ErrorMode, GameInstance, GameStatus, MapInfo, PlayerObservation, SpectatorObservation};

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

            // Collect instances to avoid holding DashMap reference during parallel iteration
            let instances: Vec<(Uuid, GameInstanceHandle)> = self
                .state
                .instances
                .iter()
                .map(|e| (*e.key(), e.value().clone()))
                .collect();

            // Process instances in parallel using rayon
            instances
                .par_iter()
                .for_each(|(instance_id, instance_handle)| {
                    let mut instance = instance_handle.write();
                    let game_id = instance.game_id;

                    if instance.status == GameStatus::Playing {
                        let players_before: std::collections::HashSet<Uuid> =
                            instance.players.keys().copied().collect();

                        instance.tick();

                        let players_after: std::collections::HashSet<Uuid> =
                            instance.players.keys().copied().collect();

                        // Clean up kicked players
                        for agent_id in players_before.difference(&players_after) {
                            self.state
                                .observation_cache
                                .remove(&(*instance_id, *agent_id));
                            self.state.player_instances.remove(&(*agent_id, game_id));
                        }

                        // Update observation cache
                        for &agent_id in instance.players.keys() {
                            if let Some(obs) = instance.get_player_observation(agent_id) {
                                self.state
                                    .observation_cache
                                    .insert((*instance_id, agent_id), obs);
                            }
                        }

                        // Update spectator cache
                        let spectator_obs = instance.get_spectator_observation();
                        self.state.spectator_cache.insert(*instance_id, spectator_obs);
                    }
                });

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

/// Creates a new instance for a game
fn create_instance(
    state: &GameManagerHandle,
    game_id: Uuid,
    max_players: u32,
    script: Option<&str>,
) -> Uuid {
    let instance = match script {
        Some(code) => GameInstance::new_with_script_and_config(
            game_id,
            code,
            max_players,
            state.async_bridge.clone(),
            state.error_mode,
        ),
        None => GameInstance::new_with_config(game_id, max_players, state.async_bridge.clone(), state.error_mode),
    };

    let instance_id = instance.instance_id;

    // Cache initial spectator observation
    let spectator_obs = instance.get_spectator_observation();
    state.spectator_cache.insert(instance_id, spectator_obs);

    let instance_handle = Arc::new(RwLock::new(instance));
    state.instances.insert(instance_id, instance_handle);

    // Track this instance under the game
    state
        .game_instances
        .entry(game_id)
        .or_insert_with(Vec::new)
        .push(instance_id);

    eprintln!(
        "[Instance] Created {} for game {} (max_players={})",
        instance_id, game_id, max_players
    );

    instance_id
}

/// Finds an instance with capacity or creates a new one
pub fn find_or_create_instance(
    state: &GameManagerHandle,
    game_id: Uuid,
    max_players: u32,
    script: Option<&str>,
) -> FindInstanceResult {
    // Check existing instances for capacity
    if let Some(instance_ids) = state.game_instances.get(&game_id) {
        for &instance_id in instance_ids.value() {
            if let Some(handle) = state.instances.get(&instance_id) {
                let instance = handle.read();
                if instance.has_capacity() {
                    return FindInstanceResult {
                        instance_id,
                        created: false,
                    };
                }
            }
        }
    }

    // Create new instance
    let instance_id = create_instance(state, game_id, max_players, script);
    FindInstanceResult {
        instance_id,
        created: true,
    }
}

/// Checks if any instance is running for this game
pub fn is_instance_running(state: &GameManagerHandle, game_id: Uuid) -> bool {
    state
        .game_instances
        .get(&game_id)
        .map(|ids| !ids.is_empty())
        .unwrap_or(false)
}

/// Gets the instance_id a player is in for a specific game
pub fn get_player_instance(
    state: &GameManagerHandle,
    agent_id: Uuid,
    game_id: Uuid,
) -> Option<Uuid> {
    state
        .player_instances
        .get(&(agent_id, game_id))
        .map(|r| *r.value())
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
    let instance_handle = state
        .instances
        .get(&instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;

    let mut instance = instance_handle.write();

    if let Some(ref err) = instance.halted_error {
        return Err(format!("Game halted: {}", err));
    }

    if !instance.has_capacity() {
        return Err("Instance is full".to_string());
    }

    if !instance.add_player(agent_id, agent_name) {
        return Err("Already in instance".to_string());
    }

    // Track player's instance
    state.player_instances.insert((agent_id, game_id), instance_id);

    // Initialize observation cache
    if let Some(obs) = instance.get_player_observation(agent_id) {
        state.observation_cache.insert((instance_id, agent_id), obs);
    }

    Ok(())
}

/// Leaves a player from an instance
pub fn leave_instance(
    state: &GameManagerHandle,
    instance_id: Uuid,
    agent_id: Uuid,
) -> Result<(), String> {
    let instance_handle = state
        .instances
        .get(&instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;

    let game_id = {
        let mut instance = instance_handle.write();
        if !instance.remove_player(agent_id) {
            return Err("Not in instance".to_string());
        }
        instance.game_id
    };

    state.player_instances.remove(&(agent_id, game_id));
    state.observation_cache.remove(&(instance_id, agent_id));

    Ok(())
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
    let instance_id = get_player_instance(state, agent_id, game_id)
        .ok_or_else(|| "Not in any instance of this game".to_string())?;

    let instance_handle = state
        .instances
        .get(&instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;

    let mut instance = instance_handle.write();

    if let Some(ref err) = instance.halted_error {
        return Err(format!("Game halted: {}", err));
    }

    let user_id = instance
        .players
        .get(&agent_id)
        .ok_or_else(|| "Not in instance".to_string())?;

    instance.queue_agent_input(*user_id, input_type, data);
    instance.record_player_activity(agent_id);

    Ok(())
}

// =============================================================================
// Observations
// =============================================================================

pub fn get_observation(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
) -> Result<PlayerObservation, String> {
    let instance_id = get_player_instance(state, agent_id, game_id)
        .ok_or_else(|| "Not in any instance of this game".to_string())?;

    state
        .observation_cache
        .get(&(instance_id, agent_id))
        .map(|r| r.clone())
        .ok_or_else(|| "Not in instance".to_string())
}

pub fn get_spectator_observation(
    state: &GameManagerHandle,
    game_id: Uuid,
) -> Result<SpectatorObservation, String> {
    let instance_ids = state
        .game_instances
        .get(&game_id)
        .ok_or_else(|| "No instances for this game".to_string())?;

    // Find most populated instance
    let mut best_instance_id = None;
    let mut max_players = 0;

    for &instance_id in instance_ids.value() {
        if let Some(handle) = state.instances.get(&instance_id) {
            let instance = handle.read();
            let count = instance.players.len();
            if count >= max_players {
                max_players = count;
                best_instance_id = Some(instance_id);
            }
        }
    }

    let instance_id = best_instance_id.ok_or_else(|| "No valid instances found".to_string())?;

    state
        .spectator_cache
        .get(&instance_id)
        .map(|r| r.clone())
        .ok_or_else(|| "Instance not found in cache".to_string())
}

pub fn get_spectator_observation_for_instance(
    state: &GameManagerHandle,
    instance_id: Uuid,
) -> Result<SpectatorObservation, String> {
    state
        .spectator_cache
        .get(&instance_id)
        .map(|r| r.clone())
        .ok_or_else(|| "Instance not found".to_string())
}

/// Get static map geometry for a game (cached per game_id)
pub fn get_map(
    state: &GameManagerHandle,
    game_id: Uuid,
) -> Result<MapInfo, String> {
    // Check cache first
    if let Some(cached) = state.map_cache.get(&game_id) {
        return Ok(cached.clone());
    }

    // Find any instance for this game to get map info
    let instance_ids = state
        .game_instances
        .get(&game_id)
        .ok_or_else(|| "No instances for this game".to_string())?;

    let instance_id = instance_ids
        .first()
        .ok_or_else(|| "No instances available".to_string())?;

    let instance_handle = state
        .instances
        .get(instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;

    let instance = instance_handle.read();
    let map_info = instance.get_map_info();

    // Cache for future requests
    state.map_cache.insert(game_id, map_info.clone());

    Ok(map_info)
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
    state
        .instances
        .iter()
        .map(|entry| {
            let instance_id = *entry.key();
            let instance = entry.value().read();
            InstanceInfo {
                instance_id,
                game_id: instance.game_id,
                status: match instance.status {
                    GameStatus::Waiting => "waiting".to_string(),
                    GameStatus::Playing => "playing".to_string(),
                    GameStatus::Finished => "finished".to_string(),
                },
                player_count: instance.players.len(),
                max_players: instance.max_players as usize,
                tick: instance.tick,
            }
        })
        .collect()
}

pub fn list_games(state: &GameManagerHandle) -> Vec<GameInfo> {
    let mut game_infos: std::collections::HashMap<Uuid, GameInfo> = std::collections::HashMap::new();

    for entry in state.instances.iter() {
        let instance = entry.value().read();
        let game_id = instance.game_id;

        let info = game_infos.entry(game_id).or_insert_with(|| GameInfo {
            id: game_id,
            status: "waiting".to_string(),
            player_count: 0,
            tick: 0,
        });

        info.player_count += instance.players.len();
        info.tick = info.tick.max(instance.tick);
        if instance.status == GameStatus::Playing {
            info.status = "playing".to_string();
        }
    }

    game_infos.into_values().collect()
}

pub fn get_game_info(state: &GameManagerHandle, game_id: Uuid) -> Option<GameInfo> {
    let instance_ids = state.game_instances.get(&game_id)?;

    let mut total_players = 0;
    let mut max_tick = 0;
    let mut any_playing = false;

    for &instance_id in instance_ids.value() {
        if let Some(handle) = state.instances.get(&instance_id) {
            let instance = handle.read();
            total_players += instance.players.len();
            max_tick = max_tick.max(instance.tick);
            if instance.status == GameStatus::Playing {
                any_playing = true;
            }
        }
    }

    Some(GameInfo {
        id: game_id,
        status: if any_playing { "playing" } else { "waiting" }.to_string(),
        player_count: total_players,
        tick: max_tick,
    })
}

// =============================================================================
// Instance Lifecycle
// =============================================================================

pub fn destroy_instance(state: &GameManagerHandle, instance_id: Uuid) -> bool {
    let game_id = state
        .instances
        .get(&instance_id)
        .map(|h| h.read().game_id);

    if state.instances.remove(&instance_id).is_none() {
        return false;
    }

    state.spectator_cache.remove(&instance_id);

    // Clean up observation cache
    let obs_keys: Vec<_> = state
        .observation_cache
        .iter()
        .filter(|e| e.key().0 == instance_id)
        .map(|e| *e.key())
        .collect();
    for key in obs_keys {
        state.observation_cache.remove(&key);
    }

    if let Some(game_id) = game_id {
        // Remove from game_instances
        if let Some(mut ids) = state.game_instances.get_mut(&game_id) {
            ids.retain(|&id| id != instance_id);
        }

        // Clean up player_instances
        let player_keys: Vec<_> = state
            .player_instances
            .iter()
            .filter(|e| *e.value() == instance_id)
            .map(|e| *e.key())
            .collect();
        for key in player_keys {
            state.player_instances.remove(&key);
        }
    }

    eprintln!("[Instance] Destroyed {}", instance_id);
    true
}

pub fn cleanup_empty_instances(state: &GameManagerHandle) -> usize {
    cleanup_empty_instances_with_timeout(state, EMPTY_INSTANCE_TIMEOUT)
}

pub fn cleanup_empty_instances_with_timeout(
    state: &GameManagerHandle,
    timeout: Duration,
) -> usize {
    let now = Instant::now();
    let mut to_destroy = Vec::new();

    for entry in state.instances.iter() {
        let instance_id = *entry.key();
        let instance = entry.value().read();

        if instance.players.is_empty() {
            if let Some(empty_since) = instance.empty_since {
                if now.duration_since(empty_since) > timeout {
                    to_destroy.push(instance_id);
                }
            }
        }
    }

    let count = to_destroy.len();
    for instance_id in to_destroy {
        destroy_instance(state, instance_id);
    }
    count
}

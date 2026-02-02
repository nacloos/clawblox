pub mod instance;
pub mod lua;
pub mod physics;

use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use instance::{GameAction, GameInstance, GameStatus, PlayerObservation};

/// Handle to the game manager - now uses DashMap for per-game locks
pub type GameManagerHandle = Arc<GameManagerState>;

/// Per-game lock wrapper - uses parking_lot for better performance under contention
pub type GameInstanceHandle = Arc<RwLock<GameInstance>>;

pub struct GameManagerState {
    /// Each game has its own RwLock for fine-grained locking
    pub games: DashMap<Uuid, GameInstanceHandle>,
    /// Cached observations, keyed by (game_id, agent_id)
    /// Updated once per tick, read lock-free by HTTP /observe
    pub observation_cache: DashMap<(Uuid, Uuid), PlayerObservation>,
}

impl GameManagerState {
    pub fn new() -> Self {
        Self {
            games: DashMap::new(),
            observation_cache: DashMap::new(),
        }
    }
}

pub struct GameManager {
    state: GameManagerHandle,
    tick_rate: u64,
}

impl GameManager {
    pub fn new(tick_rate: u64) -> (Self, GameManagerHandle) {
        let state = Arc::new(GameManagerState::new());
        let handle = Arc::clone(&state);

        (Self { state, tick_rate }, handle)
    }

    pub fn run(self) {
        let tick_duration = Duration::from_millis(1000 / self.tick_rate);

        loop {
            let start = Instant::now();

            // Iterate over games without holding global lock
            // Each game is locked individually during its tick
            for entry in self.state.games.iter() {
                let game_id = *entry.key();
                let game_handle = entry.value().clone();
                let mut game = game_handle.write();
                if game.status == GameStatus::Playing {
                    game.tick();

                    // Cache observations for all players after tick
                    // This allows /observe HTTP requests to read without lock
                    for &agent_id in game.players.keys() {
                        if let Some(obs) = game.get_player_observation(agent_id) {
                            self.state.observation_cache.insert((game_id, agent_id), obs);
                        }
                    }
                }
            }

            let elapsed = start.elapsed();
            if elapsed < tick_duration {
                thread::sleep(tick_duration - elapsed);
            }
        }
    }
}

pub fn create_game(state: &GameManagerHandle) -> Uuid {
    let game_id = Uuid::new_v4();
    let game = GameInstance::new(game_id);
    let game_handle = Arc::new(RwLock::new(game));

    state.games.insert(game_id, game_handle);

    game_id
}

/// Gets an existing running instance or creates a new one for the given game ID.
/// Returns true if a new instance was created, false if using existing.
pub fn get_or_create_instance(state: &GameManagerHandle, game_id: Uuid) -> bool {
    get_or_create_instance_with_script(state, game_id, None)
}

/// Gets an existing running instance or creates a new one with an optional Lua script.
/// Returns true if a new instance was created, false if using existing.
pub fn get_or_create_instance_with_script(
    state: &GameManagerHandle,
    game_id: Uuid,
    script: Option<&str>,
) -> bool {
    if state.games.contains_key(&game_id) {
        return false;
    }

    let game = match script {
        Some(code) => GameInstance::new_with_script(game_id, code),
        None => GameInstance::new(game_id),
    };
    let game_handle = Arc::new(RwLock::new(game));
    state.games.insert(game_id, game_handle);
    true
}

/// Checks if a game instance is currently running in memory.
pub fn is_instance_running(state: &GameManagerHandle, game_id: Uuid) -> bool {
    state.games.contains_key(&game_id)
}

pub fn join_game(state: &GameManagerHandle, game_id: Uuid, agent_id: Uuid) -> Result<(), String> {
    let game_handle = state
        .games
        .get(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    let mut game = game_handle.write();
    if !game.add_player(agent_id) {
        return Err("Already in game".to_string());
    }

    // Initialize cached observation so player can observe before first tick
    if let Some(obs) = game.get_player_observation(agent_id) {
        state.observation_cache.insert((game_id, agent_id), obs);
    }

    Ok(())
}

pub fn leave_game(state: &GameManagerHandle, game_id: Uuid, agent_id: Uuid) -> Result<(), String> {
    let game_handle = state
        .games
        .get(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    let mut game = game_handle.write();
    if !game.remove_player(agent_id) {
        return Err("Not in game".to_string());
    }

    // Clean up cached observation
    state.observation_cache.remove(&(game_id, agent_id));

    Ok(())
}

pub fn queue_action(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
    action: GameAction,
) -> Result<(), String> {
    let game_handle = state
        .games
        .get(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    let game = game_handle.read();
    if !game.players.contains_key(&agent_id) {
        return Err("Not in game".to_string());
    }

    game.queue_action(agent_id, action);
    Ok(())
}

/// Queue an agent input for processing by the Lua AgentInputService
pub fn queue_input(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
    input_type: String,
    data: serde_json::Value,
) -> Result<(), String> {
    let game_handle = state
        .games
        .get(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    let game = game_handle.read();
    let user_id = game
        .players
        .get(&agent_id)
        .ok_or_else(|| "Not in game".to_string())?;

    game.queue_agent_input(*user_id, input_type, data);
    Ok(())
}

pub fn get_observation(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
) -> Result<PlayerObservation, String> {
    // Read from cache (lock-free) instead of acquiring game lock
    // Cache is populated by game tick loop
    state
        .observation_cache
        .get(&(game_id, agent_id))
        .map(|r| r.clone())
        .ok_or_else(|| "Not in game".to_string())
}

pub fn get_spectator_observation(
    state: &GameManagerHandle,
    game_id: Uuid,
) -> Result<instance::SpectatorObservation, String> {
    let game_handle = state
        .games
        .get(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    let game = game_handle.read();
    Ok(game.get_spectator_observation())
}

pub fn list_games(state: &GameManagerHandle) -> Vec<GameInfo> {
    state
        .games
        .iter()
        .map(|entry| {
            let id = *entry.key();
            let game = entry.value().read();
            GameInfo {
                id,
                status: match game.status {
                    GameStatus::Waiting => "waiting".to_string(),
                    GameStatus::Playing => "playing".to_string(),
                    GameStatus::Finished => "finished".to_string(),
                },
                player_count: game.players.len(),
                tick: game.tick,
            }
        })
        .collect()
}

pub fn get_game_info(state: &GameManagerHandle, game_id: Uuid) -> Option<GameInfo> {
    state.games.get(&game_id).map(|entry| {
        let game = entry.value().read();
        GameInfo {
            id: game_id,
            status: match game.status {
                GameStatus::Waiting => "waiting".to_string(),
                GameStatus::Playing => "playing".to_string(),
                GameStatus::Finished => "finished".to_string(),
            },
            player_count: game.players.len(),
            tick: game.tick,
        }
    })
}

pub fn matchmake(state: &GameManagerHandle) -> (Uuid, bool) {
    // Look for an existing waiting game
    for entry in state.games.iter() {
        let game = entry.value().read();
        if game.status == GameStatus::Waiting {
            return (*entry.key(), false);
        }
    }

    let game_id = create_game(state);
    (game_id, true)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GameInfo {
    pub id: Uuid,
    pub status: String,
    pub player_count: usize,
    pub tick: u64,
}

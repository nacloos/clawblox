pub mod actions;
pub mod instance;
pub mod lua;
pub mod shooter;
pub mod systems;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

use instance::{GameInstance, GameStatus, PlayerObservation};
use actions::GameAction;

pub type GameManagerHandle = Arc<RwLock<GameManagerState>>;

pub struct GameManagerState {
    pub games: HashMap<Uuid, GameInstance>,
}

impl GameManagerState {
    pub fn new() -> Self {
        Self {
            games: HashMap::new(),
        }
    }
}

pub struct GameManager {
    state: GameManagerHandle,
    tick_rate: u64,
}

impl GameManager {
    pub fn new(tick_rate: u64) -> (Self, GameManagerHandle) {
        let state = Arc::new(RwLock::new(GameManagerState::new()));
        let handle = Arc::clone(&state);

        (Self { state, tick_rate }, handle)
    }

    pub fn run(self) {
        let tick_duration = Duration::from_millis(1000 / self.tick_rate);

        loop {
            let start = Instant::now();

            {
                let mut state = self.state.write().unwrap();
                for (_, game) in state.games.iter_mut() {
                    if game.status == GameStatus::Playing {
                        game.tick();
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

    let mut state = state.write().unwrap();
    state.games.insert(game_id, game);

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
    let mut state = state.write().unwrap();

    if state.games.contains_key(&game_id) {
        return false;
    }

    let game = match script {
        Some(code) => GameInstance::new_with_script(game_id, code),
        None => GameInstance::new(game_id),
    };
    state.games.insert(game_id, game);
    true
}

/// Checks if a game instance is currently running in memory.
pub fn is_instance_running(state: &GameManagerHandle, game_id: Uuid) -> bool {
    let state = state.read().unwrap();
    state.games.contains_key(&game_id)
}

pub fn join_game(state: &GameManagerHandle, game_id: Uuid, agent_id: Uuid) -> Result<(), String> {
    let mut state = state.write().unwrap();
    let game = state
        .games
        .get_mut(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    if !game.add_player(agent_id) {
        return Err("Already in game".to_string());
    }

    Ok(())
}

pub fn leave_game(state: &GameManagerHandle, game_id: Uuid, agent_id: Uuid) -> Result<(), String> {
    let mut state = state.write().unwrap();
    let game = state
        .games
        .get_mut(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    if !game.remove_player(agent_id) {
        return Err("Not in game".to_string());
    }

    Ok(())
}

pub fn queue_action(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
    action: GameAction,
) -> Result<(), String> {
    let state = state.read().unwrap();
    let game = state
        .games
        .get(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    if !game.players.contains_key(&agent_id) {
        return Err("Not in game".to_string());
    }

    game.queue_action(agent_id, action);
    Ok(())
}

pub fn get_observation(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
) -> Result<PlayerObservation, String> {
    let mut state = state.write().unwrap();
    let game = state
        .games
        .get_mut(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    game.get_player_observation(agent_id)
        .ok_or_else(|| "Not in game".to_string())
}

pub fn get_spectator_observation(
    state: &GameManagerHandle,
    game_id: Uuid,
) -> Result<instance::SpectatorObservation, String> {
    let mut state = state.write().unwrap();
    let game = state
        .games
        .get_mut(&game_id)
        .ok_or_else(|| "Game not found".to_string())?;

    Ok(game.get_spectator_observation())
}

pub fn list_games(state: &GameManagerHandle) -> Vec<GameInfo> {
    let state = state.read().unwrap();
    state
        .games
        .iter()
        .map(|(id, game)| GameInfo {
            id: *id,
            status: match game.status {
                GameStatus::Waiting => "waiting".to_string(),
                GameStatus::Playing => "playing".to_string(),
                GameStatus::Finished => "finished".to_string(),
            },
            player_count: game.players.len(),
            tick: game.tick,
        })
        .collect()
}

pub fn get_game_info(state: &GameManagerHandle, game_id: Uuid) -> Option<GameInfo> {
    let state = state.read().unwrap();
    state.games.get(&game_id).map(|game| GameInfo {
        id: game_id,
        status: match game.status {
            GameStatus::Waiting => "waiting".to_string(),
            GameStatus::Playing => "playing".to_string(),
            GameStatus::Finished => "finished".to_string(),
        },
        player_count: game.players.len(),
        tick: game.tick,
    })
}

pub fn matchmake(state: &GameManagerHandle) -> (Uuid, bool) {
    {
        let state_read = state.read().unwrap();
        for (id, game) in state_read.games.iter() {
            if game.status == GameStatus::Waiting && game.players.len() < 4 {
                return (*id, false);
            }
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

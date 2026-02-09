use uuid::Uuid;

use super::{get_player_instance, GameManagerHandle};
use super::instance::{MapInfo, PlayerObservation, SpectatorObservation};

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


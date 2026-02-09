use std::sync::Arc;
use uuid::Uuid;

use parking_lot::RwLock;

use crate::game::{FindInstanceResult, GameManagerHandle};
use crate::game::instance::GameInstance;

/// Creates a new instance for a game.
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

pub fn find_or_create_instance(
    state: &GameManagerHandle,
    game_id: Uuid,
    max_players: u32,
    script: Option<&str>,
) -> FindInstanceResult {
    // Check existing instances for capacity.
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

    // Create new instance.
    let instance_id = create_instance(state, game_id, max_players, script);
    FindInstanceResult {
        instance_id,
        created: true,
    }
}

pub fn is_instance_running(state: &GameManagerHandle, game_id: Uuid) -> bool {
    state
        .game_instances
        .get(&game_id)
        .map(|ids| !ids.is_empty())
        .unwrap_or(false)
}

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

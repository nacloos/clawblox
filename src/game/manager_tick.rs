use std::collections::HashSet;
use uuid::Uuid;

use rayon::prelude::*;

use crate::game::{GameInstanceHandle, GameManagerHandle};
use crate::game::instance::GameStatus;

fn process_instance(
    state: &GameManagerHandle,
    instance_id: Uuid,
    instance_handle: &GameInstanceHandle,
) {
    let mut instance = instance_handle.write();
    let game_id = instance.game_id;

    if instance.status != GameStatus::Playing {
        return;
    }

    let players_before: HashSet<Uuid> = instance.players.keys().copied().collect();

    instance.tick();

    let players_after: HashSet<Uuid> = instance.players.keys().copied().collect();

    // Clean up kicked players.
    for agent_id in players_before.difference(&players_after) {
        state
            .observation_cache
            .remove(&(instance_id, *agent_id));
        state.player_instances.remove(&(*agent_id, game_id));
    }

    // Update observation cache for active players.
    for &agent_id in instance.players.keys() {
        if let Some(obs) = instance.get_player_observation(agent_id) {
            state
                .observation_cache
                .insert((instance_id, agent_id), obs);
        }
    }

    // Update spectator cache.
    let spectator_obs = instance.get_spectator_observation();
    state.spectator_cache.insert(instance_id, spectator_obs);
}

pub fn tick_instances(state: &GameManagerHandle) {
    // Collect instances to avoid holding DashMap references during parallel iteration.
    let instances: Vec<(Uuid, GameInstanceHandle)> = state
        .instances
        .iter()
        .map(|e| (*e.key(), e.value().clone()))
        .collect();

    instances
        .par_iter()
        .for_each(|(instance_id, instance_handle)| process_instance(state, *instance_id, instance_handle));
}

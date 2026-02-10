use std::collections::HashSet;
use uuid::Uuid;

use rayon::prelude::*;

use crate::game::{GameInstanceHandle, GameManagerHandle};
use crate::game::instance::GameStatus;
use crate::game::panic_reporting;

fn process_instance(
    state: &GameManagerHandle,
    instance_id: Uuid,
    instance_handle: &GameInstanceHandle,
) {
    let tick_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
            state.observation_cache.remove(&(instance_id, *agent_id));
            state.player_instances.remove(&(*agent_id, game_id));
        }

        // Update observation cache for active players.
        for &agent_id in instance.players.keys() {
            if let Some(obs) = instance.get_player_observation(agent_id) {
                state.observation_cache.insert((instance_id, agent_id), obs);
            }
        }

        // Update spectator cache.
        let spectator_obs = instance.get_spectator_observation();
        state.spectator_cache.insert(instance_id, spectator_obs);
    }));

    if let Err(payload) = tick_result {
        let (game_id, tick) = {
            let mut instance = instance_handle.write();
            if instance.halted_error.is_none() {
                let msg = panic_reporting::panic_payload_message(&*payload);
                instance.halted_error = Some(format!("panic in instance tick: {}", msg));
            }
            instance.status = GameStatus::Finished;
            (instance.game_id, instance.tick)
        };

        state.spectator_cache.remove(&instance_id);
        state
            .observation_cache
            .retain(|(cached_instance_id, _), _| *cached_instance_id != instance_id);

        let context = format!("game={} instance={} tick={}", game_id, instance_id, tick);
        panic_reporting::log_panic("instance_tick", &context, &*payload);
    }
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

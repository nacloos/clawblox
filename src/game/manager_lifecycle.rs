use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::game::GameManagerHandle;

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

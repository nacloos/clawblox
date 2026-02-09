use uuid::Uuid;

use crate::game::GameManagerHandle;

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

pub fn queue_input(
    state: &GameManagerHandle,
    game_id: Uuid,
    agent_id: Uuid,
    input_type: String,
    data: serde_json::Value,
) -> Result<(), String> {
    let instance_id = super::get_player_instance(state, agent_id, game_id)
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

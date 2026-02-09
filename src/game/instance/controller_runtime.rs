use uuid::Uuid;

use super::super::constants::humanoid as humanoid_consts;
use super::super::constants::physics as consts;
use super::super::humanoid_movement::{build_motion_plan, resolve_vertical_velocity_after_move};
use super::character_controller::{
    evaluate_ground_controller, sample_ground_sensor, ControllerManagerConfig,
};
use super::GameInstance;

/// Sync Lua Humanoid move targets/cancels into physics character targets.
pub(super) fn sync_move_targets(instance: &mut GameInstance) {
    if instance.lua_runtime.is_none() {
        return;
    }

    let player_pairs: Vec<(Uuid, u64)> = instance
        .players
        .iter()
        .map(|(&agent_id, &user_id)| (agent_id, user_id))
        .collect();

    for (agent_id, user_id) in player_pairs {
        let Some(&hrp_id) = instance.player_hrp_ids.get(&agent_id) else {
            warn_once_move_to(instance, agent_id, "Missing HRP");
            continue;
        };

        let runtime = instance.lua_runtime.as_ref().unwrap();
        let Some(player) = runtime.players().get_player_by_user_id(user_id) else {
            warn_once_move_to(instance, agent_id, "Missing player");
            continue;
        };

        let player_data = player.data.lock().unwrap();
        let Some(character_weak) = player_data.player_data.as_ref().and_then(|pd| pd.character.as_ref()) else {
            warn_once_move_to(instance, agent_id, "Missing character");
            continue;
        };
        let Some(character_ref) = character_weak.upgrade() else {
            warn_once_move_to(instance, agent_id, "Character ref expired");
            continue;
        };
        drop(player_data);

        let character_data = character_ref.lock().unwrap();
        let mut found_humanoid = false;
        let mut cancelled_move_to = false;
        for child_ref in &character_data.children {
            let mut child_data = child_ref.lock().unwrap();
            if let Some(humanoid) = &mut child_data.humanoid_data {
                found_humanoid = true;
                if humanoid.cancel_move_to {
                    humanoid.cancel_move_to = false;
                    instance.physics.set_character_target(hrp_id, None);
                    cancelled_move_to = true;
                } else if let Some(target) = humanoid.move_to_target.take() {
                    instance
                        .physics
                        .set_character_target(hrp_id, Some([target.x, target.y, target.z]));
                }
            }
        }
        drop(character_data);

        if cancelled_move_to {
            fire_move_to_finished(instance, agent_id, false);
        }

        if !found_humanoid {
            warn_once_move_to(instance, agent_id, "No humanoid found");
        }
    }
}

/// Updates character controller movement towards targets for one frame.
pub(super) fn update_character_movement(instance: &mut GameInstance, dt: f32) {
    let agent_hrp_pairs: Vec<(Uuid, u64)> = instance
        .player_hrp_ids
        .iter()
        .map(|(&agent_id, &hrp_id)| (agent_id, hrp_id))
        .collect();

    for (agent_id, hrp_id) in agent_hrp_pairs {
        let (current_pos, target, vertical_velocity, grounded, move_to_elapsed) = {
            let Some(state) = instance.physics.get_character_state(hrp_id) else {
                continue;
            };
            let Some(pos) = instance.physics.get_character_position(hrp_id) else {
                continue;
            };
            (
                pos,
                state.target_position,
                state.vertical_velocity,
                state.grounded,
                state.move_to_elapsed,
            )
        };

        let walk_speed = get_humanoid_walk_speed(instance, agent_id).unwrap_or(consts::WALK_SPEED);
        let jump_power = consume_humanoid_jump_request(instance, agent_id);
        let gravity = instance.physics.gravity.y;
        let controller_config = ControllerManagerConfig {
            ground_query_distance: consts::CHARACTER_GROUND_QUERY_DISTANCE,
            platform_stick_distance: consts::CHARACTER_PLATFORM_STICK_DISTANCE,
        };
        let ground_sample = sample_ground_sensor(&instance.physics, hrp_id, grounded, controller_config);
        let ground_decision = evaluate_ground_controller(ground_sample, controller_config);
        let motion_plan = build_motion_plan(
            current_pos,
            target,
            walk_speed,
            vertical_velocity,
            gravity,
            dt,
            grounded,
            ground_decision.carry_by_platform,
            jump_power,
            ground_decision.platform_velocity,
        );

        let mut new_vertical_velocity = motion_plan.new_vertical_velocity;
        let mut new_move_to_elapsed = move_to_elapsed;
        if motion_plan.reached_move_to {
            instance.physics.set_character_target(hrp_id, None);
            new_move_to_elapsed = 0.0;
        }

        let mut move_to_timed_out = false;
        if target.is_some() && !motion_plan.reached_move_to {
            new_move_to_elapsed += dt;
            if new_move_to_elapsed >= humanoid_consts::MOVE_TO_TIMEOUT_SECS {
                instance.physics.set_character_target(hrp_id, None);
                new_move_to_elapsed = 0.0;
                move_to_timed_out = true;
            }
        }

        if let Some(movement) = instance.physics.move_character(hrp_id, motion_plan.desired, dt) {
            new_vertical_velocity = resolve_vertical_velocity_after_move(
                new_vertical_velocity,
                motion_plan.desired[1],
                movement.translation.y,
                movement.grounded,
            );
        }

        if let Some(state) = instance.physics.get_character_state_mut(hrp_id) {
            state.vertical_velocity = new_vertical_velocity;
            state.move_to_elapsed = new_move_to_elapsed;
        }
        if motion_plan.reached_move_to {
            fire_move_to_finished(instance, agent_id, true);
        } else if move_to_timed_out {
            fire_move_to_finished(instance, agent_id, false);
        }
    }
}

fn warn_once_move_to(instance: &GameInstance, agent_id: Uuid, reason: &str) {
    if let Ok(mut counts) = instance.humanoid_warn_counts.lock() {
        let count = counts.entry(agent_id).or_insert(0);
        if *count < 3 {
            eprintln!("[MoveTo WARN] {} for agent {}", reason, agent_id);
            *count += 1;
        }
    }
}

pub(super) fn get_humanoid_walk_speed(instance: &GameInstance, agent_id: Uuid) -> Option<f32> {
    let user_id = *instance.players.get(&agent_id)?;
    let runtime = instance.lua_runtime.as_ref()?;
    let player = runtime.players().get_player_by_user_id(user_id)?;

    let player_data = player.data.lock().unwrap();
    let character = player_data
        .player_data
        .as_ref()?
        .character
        .as_ref()?
        .upgrade()?;
    drop(player_data);

    let char_data = character.lock().unwrap();
    for child in &char_data.children {
        let child_data = child.lock().unwrap();
        if let Some(humanoid) = &child_data.humanoid_data {
            return Some(humanoid.walk_speed);
        }
    }
    None
}

fn consume_humanoid_jump_request(instance: &mut GameInstance, agent_id: Uuid) -> Option<f32> {
    let user_id = *instance.players.get(&agent_id)?;
    let runtime = instance.lua_runtime.as_ref()?;
    let player = runtime.players().get_player_by_user_id(user_id)?;

    let player_data = player.data.lock().unwrap();
    let character = player_data
        .player_data
        .as_ref()?
        .character
        .as_ref()?
        .upgrade()?;
    drop(player_data);

    let char_data = character.lock().unwrap();
    for child in &char_data.children {
        let mut child_data = child.lock().unwrap();
        if let Some(humanoid) = &mut child_data.humanoid_data {
            if humanoid.jump_requested {
                humanoid.jump_requested = false;
                return Some(humanoid.jump_power);
            }
            return None;
        }
    }
    None
}

fn fire_move_to_finished(instance: &mut GameInstance, agent_id: Uuid, reached: bool) {
    let result: Result<(), mlua::Error> = (|| {
        let Some(user_id) = instance.players.get(&agent_id).copied() else {
            return Ok(());
        };
        let Some(runtime) = &instance.lua_runtime else {
            return Ok(());
        };
        let Some(player) = runtime.players().get_player_by_user_id(user_id) else {
            return Ok(());
        };

        let signal = {
            let player_data = player.data.lock().unwrap();
            let Some(character) = player_data
                .player_data
                .as_ref()
                .and_then(|p| p.character.as_ref())
                .and_then(|w| w.upgrade()) else {
                return Ok(());
            };
            drop(player_data);

            let char_data = character.lock().unwrap();
            let mut result = None;
            for child in &char_data.children {
                let child_data = child.lock().unwrap();
                if let Some(humanoid) = &child_data.humanoid_data {
                    result = Some(humanoid.move_to_finished.clone());
                    break;
                }
            }
            result
        };

        let Some(signal) = signal else {
            return Ok(());
        };

        let lua = runtime.lua();
        let threads = signal.fire_as_coroutines(
            lua,
            mlua::MultiValue::from_iter([mlua::Value::Boolean(reached)]),
        )?;
        super::super::lua::events::track_yielded_threads(lua, threads)?;
        Ok(())
    })();

    if let Err(e) = result {
        instance.handle_lua_error("MoveToFinished event", &e);
    }
}

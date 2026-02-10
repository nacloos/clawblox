use uuid::Uuid;

use super::super::constants::humanoid as humanoid_consts;
use super::super::constants::physics as consts;
use super::super::humanoid_movement::{build_motion_plan, resolve_vertical_velocity_after_move};
use super::super::lua::types::Vector3;
use super::super::lua::types::HumanoidStateType;
use super::character_controller::{
    evaluate_ground_controller, sample_ground_sensor, ControllerManagerConfig,
};
use super::GameInstance;

const AUTO_ROTATE_TURN_RATE_RAD_PER_SEC: f32 = 10.0;

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
                instance.physics.set_character_walk_speed(hrp_id, humanoid.walk_speed);
                if humanoid.jump_requested {
                    humanoid.jump_requested = false;
                    instance
                        .physics
                        .request_character_jump(hrp_id, humanoid.jump_power);
                }
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

        let walk_speed = instance
            .physics
            .get_character_walk_speed(hrp_id)
            .unwrap_or(consts::WALK_SPEED);
        let gravity = instance.physics.gravity.y;
        let controller_config = ControllerManagerConfig {
            ground_query_distance: consts::CHARACTER_GROUND_QUERY_DISTANCE,
            platform_stick_distance: consts::CHARACTER_PLATFORM_STICK_DISTANCE,
        };
        let ground_sample = sample_ground_sensor(&instance.physics, hrp_id, grounded, controller_config);
        let ground_decision = evaluate_ground_controller(ground_sample, controller_config);
        instance.physics.tick_character_jump_buffer(hrp_id, dt);
        let near_ground = ground_sample
            .support_distance
            .map(|d| d <= consts::SNAP_TO_GROUND + 0.05)
            .unwrap_or(false);
        let can_jump = grounded || near_ground;
        let jump_power = instance
            .physics
            .try_consume_character_jump(hrp_id, can_jump, vertical_velocity);
        let contact_velocity = instance
            .physics
            .get_character_contact_kinematic_velocity(hrp_id);
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
            contact_velocity,
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

        let mut post_move_grounded = grounded;
        if let Some(movement) = instance.physics.move_character(hrp_id, motion_plan.desired, dt) {
            new_vertical_velocity = resolve_vertical_velocity_after_move(
                new_vertical_velocity,
                motion_plan.desired[1],
                movement.translation.y,
                movement.grounded,
            );
            post_move_grounded = movement.grounded;
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

        let locomotion_vec = [motion_plan.desired[0], 0.0, motion_plan.desired[2]];
        update_humanoid_locomotion_state(
            instance,
            agent_id,
            locomotion_vec,
            post_move_grounded,
            new_vertical_velocity,
        );
        if humanoid_auto_rotate_enabled(instance, agent_id) {
            let speed =
                (locomotion_vec[0] * locomotion_vec[0] + locomotion_vec[2] * locomotion_vec[2])
                    .sqrt();
            if speed > 0.05 {
                let target_yaw = yaw_from_horizontal_velocity(locomotion_vec);
                apply_smoothed_auto_rotate(instance, hrp_id, target_yaw, dt);
            }
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

/// Converts world-space horizontal velocity to character yaw so local forward (-Z)
/// aligns with movement direction in the shared physics/render convention.
fn yaw_from_horizontal_velocity(vel: [f32; 3]) -> f32 {
    (-vel[0]).atan2(-vel[2])
}

fn wrap_angle_signed_pi(angle: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    ((angle + std::f32::consts::PI).rem_euclid(two_pi)) - std::f32::consts::PI
}

fn yaw_from_quaternion_xyzw(q: [f32; 4]) -> f32 {
    let x = q[0];
    let y = q[1];
    let z = q[2];
    let w = q[3];
    let siny_cosp = 2.0 * (w * y + x * z);
    let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
    siny_cosp.atan2(cosy_cosp)
}

fn apply_smoothed_auto_rotate(
    instance: &mut GameInstance,
    hrp_id: u64,
    target_yaw: f32,
    dt: f32,
) {
    let Some(handle) = instance.physics.get_handle(hrp_id) else {
        return;
    };
    let current_yaw = instance
        .physics
        .get_rotation(handle)
        .map(yaw_from_quaternion_xyzw)
        .unwrap_or(target_yaw);

    let delta = wrap_angle_signed_pi(target_yaw - current_yaw);
    let max_step = AUTO_ROTATE_TURN_RATE_RAD_PER_SEC * dt.max(0.0);
    let step = delta.clamp(-max_step, max_step);
    let next_yaw = current_yaw + step;
    instance.physics.set_character_yaw(hrp_id, next_yaw);
}

#[cfg(test)]
pub(super) fn get_humanoid_walk_speed(instance: &GameInstance, agent_id: Uuid) -> Option<f32> {
    let hrp_id = *instance.player_hrp_ids.get(&agent_id)?;
    instance.physics.get_character_walk_speed(hrp_id)
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

fn update_humanoid_locomotion_state(
    instance: &mut GameInstance,
    agent_id: Uuid,
    velocity: [f32; 3],
    grounded: bool,
    vertical_velocity: f32,
) {
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

        let horizontal_speed = (velocity[0] * velocity[0] + velocity[2] * velocity[2]).sqrt();
        let move_direction = if horizontal_speed > 1e-4 {
            Vector3::new(velocity[0] / horizontal_speed, 0.0, velocity[2] / horizontal_speed)
        } else {
            Vector3::zero()
        };
        let (running_signal, should_fire_running, state_changed_signal, state_transition) = {
            let player_data = player.data.lock().unwrap();
            let Some(character) = player_data
                .player_data
                .as_ref()
                .and_then(|p| p.character.as_ref())
                .and_then(|w| w.upgrade())
            else {
                return Ok(());
            };
            drop(player_data);

            let char_data = character.lock().unwrap();
            let mut signal = None;
            let mut fire_running = false;
            let mut state_signal = None;
            let mut state_transition = None;
            for child_ref in &char_data.children {
                let mut child_data = child_ref.lock().unwrap();
                if let Some(humanoid) = &mut child_data.humanoid_data {
                    let prev_speed = humanoid.running_speed;
                    humanoid.move_direction = move_direction;
                    humanoid.running_speed = horizontal_speed;

                    let crossed_motion_threshold = (prev_speed <= 0.05 && horizontal_speed > 0.05)
                        || (prev_speed > 0.05 && horizontal_speed <= 0.05);
                    let speed_changed = (prev_speed - horizontal_speed).abs() >= 0.1;
                    if crossed_motion_threshold || speed_changed {
                        signal = Some(humanoid.running.clone());
                        fire_running = true;
                    }

                    let prev_state = humanoid.state;
                    let mut next_state = if grounded {
                        HumanoidStateType::Running
                    } else if vertical_velocity > 0.05 {
                        HumanoidStateType::Jumping
                    } else {
                        HumanoidStateType::Freefall
                    };
                    if grounded
                        && matches!(
                            prev_state,
                            HumanoidStateType::Jumping | HumanoidStateType::Freefall
                        )
                    {
                        next_state = HumanoidStateType::Landed;
                    } else if grounded && prev_state == HumanoidStateType::Landed {
                        next_state = HumanoidStateType::Running;
                    }

                    if prev_state != next_state {
                        humanoid.state = next_state;
                        state_signal = Some(humanoid.state_changed.clone());
                        state_transition = Some((prev_state, next_state));
                    }
                    break;
                }
            }
            (signal, fire_running, state_signal, state_transition)
        };

        if should_fire_running {
            if let Some(signal) = running_signal {
                let lua = runtime.lua();
                let threads = signal.fire_as_coroutines(
                    lua,
                    mlua::MultiValue::from_iter([mlua::Value::Number(horizontal_speed as f64)]),
                )?;
                super::super::lua::events::track_yielded_threads(lua, threads)?;
            }
        }

        if let (Some(signal), Some((old_state, new_state))) = (state_changed_signal, state_transition)
        {
            let lua = runtime.lua();
            let threads = signal.fire_as_coroutines(
                lua,
                mlua::MultiValue::from_iter([
                    mlua::Value::UserData(lua.create_userdata(old_state)?),
                    mlua::Value::UserData(lua.create_userdata(new_state)?),
                ]),
            )?;
            super::super::lua::events::track_yielded_threads(lua, threads)?;
        }

        Ok(())
    })();

    if let Err(e) = result {
        instance.handle_lua_error("Humanoid Running event", &e);
    }
}

fn humanoid_auto_rotate_enabled(instance: &GameInstance, agent_id: Uuid) -> bool {
    let Some(user_id) = instance.players.get(&agent_id).copied() else {
        return true;
    };
    let Some(runtime) = &instance.lua_runtime else {
        return true;
    };
    let Some(player) = runtime.players().get_player_by_user_id(user_id) else {
        return true;
    };
    let player_data = player.data.lock().unwrap();
    let Some(character) = player_data
        .player_data
        .as_ref()
        .and_then(|p| p.character.as_ref())
        .and_then(|w| w.upgrade())
    else {
        return true;
    };
    drop(player_data);

    let char_data = character.lock().unwrap();
    for child_ref in &char_data.children {
        let child_data = child_ref.lock().unwrap();
        if let Some(humanoid) = &child_data.humanoid_data {
            return humanoid.auto_rotate;
        }
    }
    true
}

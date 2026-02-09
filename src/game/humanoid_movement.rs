/// Per-tick movement plan for a humanoid character.
#[derive(Debug, Clone, Copy)]
pub struct MotionPlan {
    pub desired: [f32; 3],
    pub new_vertical_velocity: f32,
    pub reached_move_to: bool,
}

/// Build desired translation and updated vertical velocity for a character this tick.
pub fn build_motion_plan(
    current_pos: [f32; 3],
    target: Option<[f32; 3]>,
    walk_speed: f32,
    vertical_velocity: f32,
    gravity: f32,
    dt: f32,
    grounded: bool,
    carry_by_platform: bool,
    jump_power: Option<f32>,
    platform_velocity: Option<[f32; 3]>,
    contact_velocity: Option<[f32; 3]>,
) -> MotionPlan {
    let mut new_vertical_velocity = vertical_velocity + gravity * dt;
    if grounded {
        if let Some(jump) = jump_power {
            let max_launch_speed = (2.0 * gravity.abs() * humanoid_consts::DEFAULT_JUMP_HEIGHT).sqrt();
            new_vertical_velocity = jump.max(0.0).min(max_launch_speed);
        }
    }

    let mut dx = 0.0f32;
    let mut dz = 0.0f32;
    let mut reached_move_to = false;

    if let Some(target) = target {
        let tx = target[0] - current_pos[0];
        let tz = target[2] - current_pos[2];
        let dist_xz = (tx * tx + tz * tz).sqrt();

        if dist_xz > humanoid_consts::MOVE_TO_REACHED_EPSILON_XZ {
            let speed = walk_speed * dt;
            dx = (tx / dist_xz) * speed;
            dz = (tz / dist_xz) * speed;
        } else {
            reached_move_to = true;
        }
    }

    let mut desired_y = if (grounded || carry_by_platform) && new_vertical_velocity <= 0.0 {
        0.0
    } else {
        new_vertical_velocity * dt
    };

    if carry_by_platform {
        if let Some(v) = platform_velocity {
            dx += v[0] * dt;
            desired_y += v[1] * dt;
            dz += v[2] * dt;
        }
    }

    if let Some(v) = contact_velocity {
        dx += v[0] * dt;
        dz += v[2] * dt;
    }

    MotionPlan {
        desired: [dx, desired_y, dz],
        new_vertical_velocity,
        reached_move_to,
    }
}

/// Resolve vertical velocity after the character controller applies movement constraints.
pub fn resolve_vertical_velocity_after_move(
    new_vertical_velocity: f32,
    desired_y: f32,
    applied_y: f32,
    grounded: bool,
) -> f32 {
    let mut v = new_vertical_velocity;
    if grounded && v < 0.0 {
        v = 0.0;
    }
    if desired_y > 0.0 && applied_y + 1.0e-4 < desired_y {
        v = 0.0;
    }
    v
}
use super::constants::humanoid as humanoid_consts;

/// Per-tick movement plan for a humanoid character.
#[derive(Debug, Clone, Copy)]
pub struct MotionPlan {
    pub desired: [f32; 3],
    pub new_vertical_velocity: f32,
    pub new_horizontal_velocity: [f32; 2],
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
    horizontal_velocity: [f32; 2],
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
    let mut new_horizontal_velocity = horizontal_velocity;
    let mut reached_move_to = false;

    if let Some(target) = target {
        let tx = target[0] - current_pos[0];
        let tz = target[2] - current_pos[2];
        let dist_xz = (tx * tx + tz * tz).sqrt();

        if dist_xz > humanoid_consts::MOVE_TO_REACHED_EPSILON_XZ {
            if grounded || carry_by_platform {
                let speed = walk_speed * dt;
                dx = (tx / dist_xz) * speed;
                dz = (tz / dist_xz) * speed;
                new_horizontal_velocity = [dx / dt, dz / dt];
            } else {
                // Preserve airborne momentum and apply limited steering toward target.
                let wish_vx = (tx / dist_xz) * walk_speed;
                let wish_vz = (tz / dist_xz) * walk_speed;
                let delta_vx = wish_vx - new_horizontal_velocity[0];
                let delta_vz = wish_vz - new_horizontal_velocity[1];
                let delta_mag = (delta_vx * delta_vx + delta_vz * delta_vz).sqrt();
                let max_delta = humanoid_consts::AIR_CONTROL_ACCEL * dt;
                if delta_mag > 1.0e-6 {
                    let scale = (max_delta / delta_mag).min(1.0);
                    new_horizontal_velocity[0] += delta_vx * scale;
                    new_horizontal_velocity[1] += delta_vz * scale;
                }
                dx = new_horizontal_velocity[0] * dt;
                dz = new_horizontal_velocity[1] * dt;
            }
        } else {
            reached_move_to = true;
            if grounded || carry_by_platform {
                new_horizontal_velocity = [0.0, 0.0];
            }
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
        new_horizontal_velocity,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_airborne_motion_plan_does_not_apply_horizontal_moveto_walk() {
        let plan = build_motion_plan(
            [0.0, 10.0, 0.0],
            Some([10.0, 10.0, 0.0]),
            16.0,
            0.0,
            -30.0,
            1.0 / 60.0,
            [8.0, 0.0],
            false,
            false,
            None,
            None,
            None,
        );

        assert!(plan.desired[0] > 0.0, "airborne should preserve forward momentum");
        assert!(plan.desired[2].abs() < 1e-6, "no Z drift for straight X target");
        assert!(plan.desired[1] < 0.0, "airborne plan should still apply gravity");
        assert!(!plan.reached_move_to, "target is still far away");
    }

    #[test]
    fn test_grounded_motion_plan_applies_horizontal_moveto_walk() {
        let plan = build_motion_plan(
            [0.0, 3.0, 0.0],
            Some([10.0, 3.0, 0.0]),
            16.0,
            0.0,
            -30.0,
            1.0 / 60.0,
            [0.0, 0.0],
            true,
            true,
            None,
            None,
            None,
        );

        assert!(plan.desired[0] > 0.0, "grounded MoveTo should steer X");
        assert!(plan.desired[2].abs() < 1e-6, "no Z offset for straight X target");
    }

    #[test]
    fn test_airborne_motion_plan_limited_steering_rate() {
        let plan = build_motion_plan(
            [0.0, 10.0, 0.0],
            Some([0.0, 10.0, 10.0]),
            16.0,
            0.0,
            -30.0,
            1.0 / 60.0,
            [16.0, 0.0],
            false,
            false,
            None,
            None,
            None,
        );

        assert!(plan.new_horizontal_velocity[0] > 0.0, "should not instantly zero X momentum");
        assert!(plan.new_horizontal_velocity[1] > 0.0, "should start steering toward +Z");
        assert!(
            plan.new_horizontal_velocity[1] < 16.0,
            "air control should not instantly reach full target speed"
        );
    }
}

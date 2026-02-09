use super::super::physics::PhysicsWorld;

/// ControllerManager-like static configuration for character locomotion.
#[derive(Debug, Clone, Copy)]
pub struct ControllerManagerConfig {
    pub ground_query_distance: f32,
    pub platform_stick_distance: f32,
}

/// GroundSensor-like sample taken each frame for a character body.
#[derive(Debug, Clone, Copy)]
pub struct GroundSensorSample {
    pub grounded: bool,
    pub support_velocity: Option<[f32; 3]>,
    pub support_distance: Option<f32>,
}

/// GroundController output used by movement planning.
#[derive(Debug, Clone, Copy)]
pub struct GroundControllerDecision {
    pub carry_by_platform: bool,
    pub platform_velocity: Option<[f32; 3]>,
}

/// Sample ground support directly below the character.
pub fn sample_ground_sensor(
    physics: &PhysicsWorld,
    character_id: u64,
    grounded: bool,
    config: ControllerManagerConfig,
) -> GroundSensorSample {
    if let Some(support) = physics.get_ground_kinematic_support(character_id, config.ground_query_distance) {
        GroundSensorSample {
            grounded,
            support_velocity: Some(support.velocity),
            support_distance: Some(support.distance),
        }
    } else {
        GroundSensorSample {
            grounded,
            support_velocity: None,
            support_distance: None,
        }
    }
}

/// Evaluate grounded/carry behavior from ground sensor data.
pub fn evaluate_ground_controller(
    sample: GroundSensorSample,
    config: ControllerManagerConfig,
) -> GroundControllerDecision {
    let near_support = sample
        .support_distance
        .map(|d| d <= config.platform_stick_distance)
        .unwrap_or(false);
    let carry_by_platform = sample.grounded || near_support;

    GroundControllerDecision {
        carry_by_platform,
        platform_velocity: sample.support_velocity,
    }
}

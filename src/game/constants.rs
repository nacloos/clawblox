//! Game physics and configuration constants.
//! Centralizing these prevents bugs from duplicated hardcoded values.

/// Physics constants
pub mod physics {
    /// Default gravity in studs/s² (matches physics-world for compatibility)
    pub const DEFAULT_GRAVITY: f32 = 30.0;

    /// Fixed timestep for physics simulation (60 Hz)
    pub const TIMESTEP: f32 = 1.0 / 60.0;

    /// Character walk speed in studs/second
    pub const WALK_SPEED: f32 = 16.0;

    /// Character capsule radius
    pub const CHARACTER_RADIUS: f32 = 0.5;

    /// Character capsule total height
    pub const CHARACTER_HEIGHT: f32 = 2.0;

    /// Character spawn height above ground
    pub const CHARACTER_SPAWN_HEIGHT: f32 = 3.0;

    /// Character controller autostep max height
    pub const AUTOSTEP_MAX_HEIGHT: f32 = 1.0;  // Generous autostep for platforms

    /// Character controller autostep min width (very small for platform edges)
    pub const AUTOSTEP_MIN_WIDTH: f32 = 0.01;

    /// Character controller snap to ground distance
    pub const SNAP_TO_GROUND: f32 = 0.2;

    /// Max downward query distance used to find supporting ground/platform body.
    pub const CHARACTER_GROUND_QUERY_DISTANCE: f32 = 4.0;

    /// Max support gap for "sticky" moving-platform carry when briefly not grounded.
    pub const CHARACTER_PLATFORM_STICK_DISTANCE: f32 = 0.6;

    /// Small epsilon for float comparisons
    pub const EPSILON: f32 = 0.001;
}

/// Humanoid default values (Roblox-compatible)
pub mod humanoid {
    /// Default health
    pub const DEFAULT_HEALTH: f32 = 100.0;

    /// Default max health
    pub const DEFAULT_MAX_HEALTH: f32 = 100.0;

    /// Default walk speed (studs/second)
    pub const DEFAULT_WALK_SPEED: f32 = 16.0;

    /// Default jump power tuned for `physics::DEFAULT_GRAVITY` (30 studs/s²).
    /// This yields roughly `DEFAULT_JUMP_HEIGHT` of vertical travel.
    pub const DEFAULT_JUMP_POWER: f32 = 20.8;

    /// Default jump height
    pub const DEFAULT_JUMP_HEIGHT: f32 = 7.2;

    /// Default hip height
    pub const DEFAULT_HIP_HEIGHT: f32 = 2.0;

    /// MoveTo timeout in seconds before MoveToFinished(false) is fired.
    pub const MOVE_TO_TIMEOUT_SECS: f32 = 8.0;

    /// Horizontal distance threshold at which a MoveTo target is considered reached.
    pub const MOVE_TO_REACHED_EPSILON_XZ: f32 = 0.5;

    /// Jump input buffer window in seconds.
    pub const JUMP_BUFFER_SECS: f32 = 0.20;

    /// Airborne horizontal steering acceleration in studs/s².
    /// Lower than ground response to avoid full walk-control in freefall.
    pub const AIR_CONTROL_ACCEL: f32 = 24.0;
}

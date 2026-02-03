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

    /// Default jump power
    pub const DEFAULT_JUMP_POWER: f32 = 50.0;

    /// Default jump height
    pub const DEFAULT_JUMP_HEIGHT: f32 = 7.2;

    /// Default hip height
    pub const DEFAULT_HIP_HEIGHT: f32 = 2.0;
}

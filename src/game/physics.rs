use nalgebra::UnitQuaternion;
use rapier3d::control::{
    CharacterAutostep, CharacterLength, EffectiveCharacterMovement, KinematicCharacterController,
};
use rapier3d::prelude::*;
use std::collections::{HashMap, HashSet};

use super::constants::humanoid as humanoid_consts;
use super::constants::physics as consts;
use super::lua::types::PartType;

/// Converts a 3x3 rotation matrix to a UnitQuaternion (Shepperd's method).
pub fn rotation_matrix_to_quaternion(m: &[[f32; 3]; 3]) -> UnitQuaternion<f32> {
    let rot = nalgebra::Matrix3::new(
        m[0][0], m[0][1], m[0][2],
        m[1][0], m[1][1], m[1][2],
        m[2][0], m[2][1], m[2][2],
    );
    let rotation = nalgebra::Rotation3::from_matrix_unchecked(rot);
    UnitQuaternion::from_rotation_matrix(&rotation)
}

/// Converts a UnitQuaternion back to a 3x3 rotation matrix.
pub fn quaternion_to_rotation_matrix(q: &UnitQuaternion<f32>) -> [[f32; 3]; 3] {
    let rot = q.to_rotation_matrix();
    let m = rot.matrix();
    [
        [m[(0, 0)], m[(0, 1)], m[(0, 2)]],
        [m[(1, 0)], m[(1, 1)], m[(1, 2)]],
        [m[(2, 0)], m[(2, 1)], m[(2, 2)]],
    ]
}

// Collision groups for Roblox-style physics behavior
// Characters don't collide with each other, only with static geometry
// Note: rapier3d uses InteractionGroups (not CollisionGroups like bevy_rapier)
const GROUP_STATIC: Group = Group::GROUP_1;    // Walls, floors, obstacles
const GROUP_CHARACTER: Group = Group::GROUP_2; // Player characters

/// State for a character controller (player or NPC)
pub struct CharacterControllerState {
    pub controller: KinematicCharacterController,
    pub collider_handle: ColliderHandle,
    pub body_handle: RigidBodyHandle,
    pub vertical_velocity: f32,
    pub target_position: Option<[f32; 3]>,
    pub walk_speed: f32,
    pub jump_power: f32,
    pub jump_requested: bool,
    pub jump_buffer_remaining: f32,
    pub jump_cooldown_remaining: f32,
    /// Seconds elapsed since current MoveTo target was set.
    pub move_to_elapsed: f32,
    pub grounded: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct GroundKinematicSupport {
    pub velocity: [f32; 3],
    pub distance: f32,
}

/// Wrapper around Rapier3D physics world for game physics simulation.
/// Syncs with Lua Workspace parts to simulate physics for non-anchored parts.
pub struct PhysicsWorld {
    pub gravity: Vector<Real>,
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    pub integration_parameters: IntegrationParameters,
    pub physics_pipeline: PhysicsPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: DefaultBroadPhase,
    pub narrow_phase: NarrowPhase,
    pub impulse_joint_set: ImpulseJointSet,
    pub multibody_joint_set: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
    pub query_pipeline: QueryPipeline,

    /// Maps Lua instance ID to Rapier rigid body handle
    pub lua_to_body: HashMap<u64, RigidBodyHandle>,
    /// Maps Rapier rigid body handle to Lua instance ID (reverse lookup)
    pub body_to_lua: HashMap<RigidBodyHandle, u64>,
    /// Maps Lua instance ID to its PartType shape (for collider rebuilds)
    pub lua_to_shape: HashMap<u64, PartType>,
    /// Character controllers for player movement
    pub character_controllers: HashMap<u64, CharacterControllerState>,
    /// Maps Rapier collider handle to Lua instance ID (for touch detection)
    pub collider_to_lua: HashMap<ColliderHandle, u64>,
    /// Script-driven kinematic linear velocity sampled from per-tick target translation.
    pub kinematic_linear_velocities: HashMap<RigidBodyHandle, [f32; 3]>,
}

/// Builds a collider with the correct shape for a given PartType and size.
fn build_collider(size: [f32; 3], shape: PartType, can_collide: bool) -> Collider {
    let [sx, sy, sz] = size;
    let shared_shape = match shape {
        PartType::Block => SharedShape::cuboid(sx / 2.0, sy / 2.0, sz / 2.0),
        PartType::Ball => SharedShape::ball(sx / 2.0),
        PartType::Cylinder => SharedShape::cylinder(sy / 2.0, sx / 2.0),
        PartType::Wedge => {
            // Triangular prism: flat bottom, slope rises from +X to -X
            // 6 vertices matching Roblox wedge convention
            let hx = sx / 2.0;
            let hy = sy / 2.0;
            let hz = sz / 2.0;
            let points = [
                point![-hx, -hy, -hz], // bottom-left-back
                point![ hx, -hy, -hz], // bottom-right-back
                point![-hx, -hy,  hz], // bottom-left-front
                point![ hx, -hy,  hz], // bottom-right-front
                point![-hx,  hy, -hz], // top-left-back
                point![-hx,  hy,  hz], // top-left-front
            ];
            SharedShape::convex_hull(&points)
                .expect("Wedge convex hull should always succeed with 6 valid vertices")
        }
    };
    ColliderBuilder::new(shared_shape)
        .sensor(!can_collide)
        .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
        .build()
}

impl PhysicsWorld {
    /// Creates a new physics world with default gravity
    pub fn new() -> Self {
        Self {
            gravity: vector![0.0, -consts::DEFAULT_GRAVITY, 0.0],
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            integration_parameters: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            lua_to_body: HashMap::new(),
            body_to_lua: HashMap::new(),
            lua_to_shape: HashMap::new(),
            character_controllers: HashMap::new(),
            collider_to_lua: HashMap::new(),
            kinematic_linear_velocities: HashMap::new(),
        }
    }

    /// Sets the gravity for the physics world
    pub fn set_gravity(&mut self, gravity_y: f32) {
        self.gravity = vector![0.0, -gravity_y, 0.0];
    }

    /// Steps the physics simulation forward by dt seconds
    pub fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt;
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline),
            &(),
            &(),
        );
    }

    /// Adds a part to the physics world
    /// - Anchored parts become kinematic (position-based, no physics simulation)
    /// - Non-anchored parts become dynamic (affected by gravity and collisions)
    pub fn add_part(
        &mut self,
        lua_id: u64,
        position: [f32; 3],
        rotation: &[[f32; 3]; 3], // 3x3 rotation matrix
        size: [f32; 3],
        anchored: bool,
        can_collide: bool,
        shape: PartType,
    ) -> RigidBodyHandle {
        let quat = rotation_matrix_to_quaternion(rotation);

        // Create rigid body
        let body = if anchored {
            RigidBodyBuilder::kinematic_position_based()
        } else {
            RigidBodyBuilder::dynamic()
        }
        .translation(vector![position[0], position[1], position[2]])
        .rotation(quat.scaled_axis())
        .build();

        let handle = self.rigid_body_set.insert(body);

        // Create collider with correct shape
        let collider = build_collider(size, shape, can_collide);

        let collider_handle = self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set);

        // Store mappings
        self.lua_to_body.insert(lua_id, handle);
        self.body_to_lua.insert(handle, lua_id);
        self.lua_to_shape.insert(lua_id, shape);
        self.collider_to_lua.insert(collider_handle, lua_id);

        handle
    }

    /// Removes a part from the physics world
    pub fn remove_part(&mut self, lua_id: u64) -> bool {
        if let Some(handle) = self.lua_to_body.remove(&lua_id) {
            self.body_to_lua.remove(&handle);
            self.lua_to_shape.remove(&lua_id);
            self.kinematic_linear_velocities.remove(&handle);
            // Remove collider->lua mappings before destroying the body
            if let Some(body) = self.rigid_body_set.get(handle) {
                for &ch in body.colliders() {
                    self.collider_to_lua.remove(&ch);
                }
            }
            self.rigid_body_set.remove(
                handle,
                &mut self.island_manager,
                &mut self.collider_set,
                &mut self.impulse_joint_set,
                &mut self.multibody_joint_set,
                true,
            );
            true
        } else {
            false
        }
    }

    /// Updates the position of an anchored (kinematic) part
    pub fn set_kinematic_position_with_dt(
        &mut self,
        handle: RigidBodyHandle,
        position: [f32; 3],
        dt: f32,
    ) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_kinematic() {
                let cur = body.translation();
                let inv_dt = if dt > consts::EPSILON { 1.0 / dt } else { 0.0 };
                self.kinematic_linear_velocities.insert(
                    handle,
                    [
                        (position[0] - cur.x) * inv_dt,
                        (position[1] - cur.y) * inv_dt,
                        (position[2] - cur.z) * inv_dt,
                    ],
                );
                body.set_next_kinematic_translation(vector![position[0], position[1], position[2]]);
            }
        }
    }

    /// Updates the position of an anchored (kinematic) part using default tick dt.
    pub fn set_kinematic_position(&mut self, handle: RigidBodyHandle, position: [f32; 3]) {
        self.set_kinematic_position_with_dt(handle, position, consts::TIMESTEP);
    }

    /// Updates the rotation of an anchored (kinematic) part from a 3x3 rotation matrix
    pub fn set_kinematic_rotation(&mut self, handle: RigidBodyHandle, rotation: &[[f32; 3]; 3]) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_kinematic() {
                let quat = rotation_matrix_to_quaternion(rotation);
                body.set_next_kinematic_rotation(quat);
            }
        }
    }

    /// Sets the velocity of a dynamic part
    pub fn set_velocity(&mut self, handle: RigidBodyHandle, velocity: [f32; 3]) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.set_linvel(vector![velocity[0], velocity[1], velocity[2]], true);
            }
        }
    }

    /// Gets the position of a rigid body
    pub fn get_position(&self, handle: RigidBodyHandle) -> Option<[f32; 3]> {
        self.rigid_body_set.get(handle).map(|body| {
            let pos = body.translation();
            [pos.x, pos.y, pos.z]
        })
    }

    /// Gets the rotation of a rigid body as quaternion [x, y, z, w]
    pub fn get_rotation(&self, handle: RigidBodyHandle) -> Option<[f32; 4]> {
        self.rigid_body_set.get(handle).map(|body| {
            let rot = body.rotation();
            [rot.i, rot.j, rot.k, rot.w]
        })
    }

    /// Gets the rotation of a rigid body as a 3x3 rotation matrix
    pub fn get_rotation_matrix(&self, handle: RigidBodyHandle) -> Option<[[f32; 3]; 3]> {
        self.rigid_body_set.get(handle).map(|body| {
            quaternion_to_rotation_matrix(body.rotation())
        })
    }

    /// Gets the velocity of a rigid body
    pub fn get_velocity(&self, handle: RigidBodyHandle) -> Option<[f32; 3]> {
        self.rigid_body_set.get(handle).map(|body| {
            let vel = body.linvel();
            [vel.x, vel.y, vel.z]
        })
    }

    /// Checks if a Lua instance has a physics body
    pub fn has_part(&self, lua_id: u64) -> bool {
        self.lua_to_body.contains_key(&lua_id)
    }

    /// Gets all Lua part IDs that have physics bodies
    pub fn get_all_part_ids(&self) -> Vec<u64> {
        self.lua_to_body.keys().copied().collect()
    }

    /// Gets the handle for a Lua instance
    pub fn get_handle(&self, lua_id: u64) -> Option<RigidBodyHandle> {
        self.lua_to_body.get(&lua_id).copied()
    }

    /// Updates whether a part is anchored or dynamic
    pub fn set_anchored(&mut self, lua_id: u64, anchored: bool) {
        if let Some(&handle) = self.lua_to_body.get(&lua_id) {
            if let Some(body) = self.rigid_body_set.get_mut(handle) {
                if anchored {
                    body.set_body_type(RigidBodyType::KinematicPositionBased, true);
                    self.kinematic_linear_velocities.insert(handle, [0.0, 0.0, 0.0]);
                } else {
                    body.set_body_type(RigidBodyType::Dynamic, true);
                    self.kinematic_linear_velocities.remove(&handle);
                }
            }
        }
    }

    /// Updates the size of a part's collider
    pub fn set_size(&mut self, lua_id: u64, size: [f32; 3]) {
        let shape = self.lua_to_shape.get(&lua_id).copied().unwrap_or(PartType::Block);
        if let Some(&handle) = self.lua_to_body.get(&lua_id) {
            if let Some(body) = self.rigid_body_set.get(handle) {
                let colliders: Vec<_> = body.colliders().iter().cloned().collect();

                // Read can_collide from existing collider before removing
                let can_collide = colliders.first()
                    .and_then(|&ch| self.collider_set.get(ch))
                    .map(|c| !c.is_sensor())
                    .unwrap_or(true);

                for collider_handle in colliders {
                    self.collider_to_lua.remove(&collider_handle);
                    self.collider_set.remove(
                        collider_handle,
                        &mut self.island_manager,
                        &mut self.rigid_body_set,
                        true,
                    );
                }

                let collider = build_collider(size, shape, can_collide);
                let new_ch = self.collider_set
                    .insert_with_parent(collider, handle, &mut self.rigid_body_set);
                self.collider_to_lua.insert(new_ch, lua_id);
            }
        }
    }

    /// Toggles collider sensor mode (can_collide=false -> sensor, can_collide=true -> solid)
    pub fn set_can_collide(&mut self, lua_id: u64, can_collide: bool) {
        if let Some(&handle) = self.lua_to_body.get(&lua_id) {
            if let Some(body) = self.rigid_body_set.get(handle) {
                let colliders: Vec<_> = body.colliders().iter().cloned().collect();
                for collider_handle in colliders {
                    if let Some(collider) = self.collider_set.get_mut(collider_handle) {
                        collider.set_sensor(!can_collide);
                    }
                }
            }
        }
    }

    /// Updates the shape of a part's collider, rebuilding it with the current size
    pub fn set_shape(&mut self, lua_id: u64, shape: PartType, size: [f32; 3]) {
        self.lua_to_shape.insert(lua_id, shape);
        if let Some(&handle) = self.lua_to_body.get(&lua_id) {
            if let Some(body) = self.rigid_body_set.get(handle) {
                let colliders: Vec<_> = body.colliders().iter().cloned().collect();

                let can_collide = colliders.first()
                    .and_then(|&ch| self.collider_set.get(ch))
                    .map(|c| !c.is_sensor())
                    .unwrap_or(true);

                for collider_handle in colliders {
                    self.collider_to_lua.remove(&collider_handle);
                    self.collider_set.remove(
                        collider_handle,
                        &mut self.island_manager,
                        &mut self.rigid_body_set,
                        true,
                    );
                }

                let collider = build_collider(size, shape, can_collide);
                let new_ch = self.collider_set
                    .insert_with_parent(collider, handle, &mut self.rigid_body_set);
                self.collider_to_lua.insert(new_ch, lua_id);
            }
        }
    }

    /// Adds a character controller for player movement
    /// Uses a capsule shape for smooth collisions with environment
    pub fn add_character(
        &mut self,
        lua_id: u64,
        position: [f32; 3],
        radius: f32,
        height: f32,
    ) -> RigidBodyHandle {
        // Create kinematic body for character
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(vector![position[0], position[1], position[2]])
            .build();
        let body_handle = self.rigid_body_set.insert(body);

        // Create capsule collider (half-height is the cylinder part, total height = 2*half_height + 2*radius)
        // Characters only collide with static geometry, not other characters (Roblox FPS style)
        let half_height = (height - 2.0 * radius).max(0.0) / 2.0;
        let collider = ColliderBuilder::capsule_y(half_height, radius)
            .collision_groups(InteractionGroups::new(GROUP_CHARACTER, GROUP_STATIC))
            .build();
        let collider_handle = self
            .collider_set
            .insert_with_parent(collider, body_handle, &mut self.rigid_body_set);

        // Create character controller with consistent settings (same as move_character)
        let mut controller = KinematicCharacterController::default();
        controller.autostep = Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(consts::AUTOSTEP_MAX_HEIGHT),
            min_width: CharacterLength::Absolute(consts::AUTOSTEP_MIN_WIDTH),
            include_dynamic_bodies: true,
        });
        controller.max_slope_climb_angle = 45.0_f32.to_radians();
        controller.min_slope_slide_angle = 30.0_f32.to_radians();
        controller.snap_to_ground = Some(CharacterLength::Absolute(consts::SNAP_TO_GROUND));

        let state = CharacterControllerState {
            controller,
            collider_handle,
            body_handle,
            vertical_velocity: 0.0,
            target_position: None,
            walk_speed: consts::WALK_SPEED,
            jump_power: humanoid_consts::DEFAULT_JUMP_POWER,
            jump_requested: false,
            jump_buffer_remaining: 0.0,
            jump_cooldown_remaining: 0.0,
            move_to_elapsed: 0.0,
            grounded: false,
        };

        self.character_controllers.insert(lua_id, state);
        self.lua_to_body.insert(lua_id, body_handle);
        self.body_to_lua.insert(body_handle, lua_id);
        self.collider_to_lua.insert(collider_handle, lua_id);

        body_handle
    }

    /// Sets the target position for a character (for Goto action)
    pub fn set_character_target(&mut self, lua_id: u64, target: Option<[f32; 3]>) {
        if let Some(state) = self.character_controllers.get_mut(&lua_id) {
            state.target_position = target;
            state.move_to_elapsed = 0.0;
        }
    }

    /// Sets locomotion walk speed for a character controller.
    pub fn set_character_walk_speed(&mut self, lua_id: u64, walk_speed: f32) {
        if let Some(state) = self.character_controllers.get_mut(&lua_id) {
            state.walk_speed = walk_speed.max(0.0);
        }
    }

    /// Requests a jump using the provided jump power.
    pub fn request_character_jump(&mut self, lua_id: u64, jump_power: f32) {
        if let Some(state) = self.character_controllers.get_mut(&lua_id) {
            state.jump_power = jump_power.max(0.0);
            state.jump_requested = true;
            state.jump_buffer_remaining = humanoid_consts::JUMP_BUFFER_SECS;
        }
    }

    /// Advance jump buffer timer and clear stale buffered jumps.
    pub fn tick_character_jump_buffer(&mut self, lua_id: u64, dt: f32) {
        if let Some(state) = self.character_controllers.get_mut(&lua_id) {
            state.jump_cooldown_remaining = (state.jump_cooldown_remaining - dt).max(0.0);
            if state.jump_requested {
                state.jump_buffer_remaining = (state.jump_buffer_remaining - dt).max(0.0);
                if state.jump_buffer_remaining <= 0.0 {
                    state.jump_requested = false;
                }
            }
        }
    }

    /// Consumes one pending jump request, returning jump power if requested.
    pub fn try_consume_character_jump(
        &mut self,
        lua_id: u64,
        can_jump: bool,
        vertical_velocity: f32,
    ) -> Option<f32> {
        let state = self.character_controllers.get_mut(&lua_id)?;
        if !state.jump_requested || !can_jump {
            return None;
        }
        if state.jump_cooldown_remaining > 0.0 {
            return None;
        }
        // Prevent repeat impulse while already traveling upward.
        if vertical_velocity > 0.0 {
            return None;
        }
        state.jump_cooldown_remaining = 0.18;
        if state.jump_requested {
            state.jump_requested = false;
            state.jump_buffer_remaining = 0.0;
            Some(state.jump_power)
        } else {
            None
        }
    }

    /// Gets current locomotion walk speed for a character controller.
    pub fn get_character_walk_speed(&self, lua_id: u64) -> Option<f32> {
        self.character_controllers.get(&lua_id).map(|s| s.walk_speed)
    }

    /// Teleports a character to a specific position (clears target + vertical velocity)
    pub fn set_character_position(&mut self, lua_id: u64, position: [f32; 3]) {
        if let Some(state) = self.character_controllers.get_mut(&lua_id) {
            if let Some(body) = self.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(vector![position[0], position[1], position[2]], true);
            }
            state.target_position = None;
            state.vertical_velocity = 0.0;
            state.jump_requested = false;
            state.jump_buffer_remaining = 0.0;
            state.jump_cooldown_remaining = 0.0;
            state.move_to_elapsed = 0.0;
        }
    }

    /// Gets the current position of a character
    pub fn get_character_position(&self, lua_id: u64) -> Option<[f32; 3]> {
        let state = self.character_controllers.get(&lua_id)?;
        let body = self.rigid_body_set.get(state.body_handle)?;
        let pos = body.translation();
        Some([pos.x, pos.y, pos.z])
    }

    /// Gets current linear velocity of a character controller body.
    pub fn get_character_velocity(&self, lua_id: u64) -> Option<[f32; 3]> {
        let state = self.character_controllers.get(&lua_id)?;
        let body = self.rigid_body_set.get(state.body_handle)?;
        let vel = body.linvel();
        Some([vel.x, vel.y, vel.z])
    }

    /// Sets the facing yaw for a character controller body.
    pub fn set_character_yaw(&mut self, lua_id: u64, yaw: f32) -> bool {
        let Some(state) = self.character_controllers.get(&lua_id) else {
            return false;
        };
        let Some(body) = self.rigid_body_set.get_mut(state.body_handle) else {
            return false;
        };
        let rot = UnitQuaternion::from_euler_angles(0.0, yaw, 0.0);
        body.set_next_kinematic_rotation(rot);
        true
    }

    /// Gets the character controller state (for checking grounded, target, etc.)
    pub fn get_character_state(&self, lua_id: u64) -> Option<&CharacterControllerState> {
        self.character_controllers.get(&lua_id)
    }

    /// Gets mutable character controller state
    pub fn get_character_state_mut(&mut self, lua_id: u64) -> Option<&mut CharacterControllerState> {
        self.character_controllers.get_mut(&lua_id)
    }

    /// Removes a character controller
    pub fn remove_character(&mut self, lua_id: u64) -> bool {
        if let Some(state) = self.character_controllers.remove(&lua_id) {
            self.lua_to_body.remove(&lua_id);
            self.body_to_lua.remove(&state.body_handle);
            self.collider_to_lua.remove(&state.collider_handle);
            self.rigid_body_set.remove(
                state.body_handle,
                &mut self.island_manager,
                &mut self.collider_set,
                &mut self.impulse_joint_set,
                &mut self.multibody_joint_set,
                true,
            );
            true
        } else {
            false
        }
    }

    /// Checks if a Lua instance has a character controller
    pub fn has_character(&self, lua_id: u64) -> bool {
        self.character_controllers.contains_key(&lua_id)
    }

    /// Checks if there is a clear line of sight between two positions
    /// Returns true if no obstacle blocks the view
    pub fn has_line_of_sight(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        exclude_body: Option<RigidBodyHandle>,
        target_body: Option<RigidBodyHandle>,
    ) -> bool {
        let direction = vector![to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let max_dist = direction.magnitude();

        if max_dist < 0.001 {
            return true; // Same position
        }

        let normalized = direction / max_dist;
        let ray = Ray::new(
            point![from[0], from[1], from[2]],
            normalized,
        );

        let filter = if let Some(body_handle) = exclude_body {
            QueryFilter::default().exclude_rigid_body(body_handle)
        } else {
            QueryFilter::default()
        };

        // Cast ray and check if we hit something before reaching the target.
        // For Roblox-style LOS we consider the target visible when the first hit
        // belongs to the target character body.
        if let Some((hit_collider, _hit_dist)) = self.query_pipeline.cast_ray(
            &self.rigid_body_set,
            &self.collider_set,
            &ray,
            max_dist,
            true, // solid
            filter,
        ) {
            if let Some(expected_body) = target_body {
                if let Some(collider) = self.collider_set.get(hit_collider) {
                    if collider.parent() == Some(expected_body) {
                        return true;
                    }
                }
            }
            false
        } else {
            true // No obstacle hit
        }
    }

    /// Detects all overlapping pairs of bodies in the physics world.
    /// Returns a set of normalized (min, max) Lua ID pairs that are currently overlapping.
    /// Uses `intersections_with_shape` which works for all body type combinations
    /// (including kinematic+kinematic which narrow_phase misses).
    pub fn detect_overlaps(&self) -> HashSet<(u64, u64)> {
        let mut overlaps = HashSet::new();

        for (&lua_id, &body_handle) in &self.lua_to_body {
            let Some(body) = self.rigid_body_set.get(body_handle) else {
                continue;
            };

            // Get the collider shape and position for this body
            let collider_handles: Vec<_> = body.colliders().iter().cloned().collect();
            for ch in collider_handles {
                let Some(collider) = self.collider_set.get(ch) else {
                    continue;
                };

                let shape = collider.shape();
                let pos = collider.position();

                // Query all intersecting colliders, excluding self
                let filter = QueryFilter::default().exclude_rigid_body(body_handle);

                self.query_pipeline.intersections_with_shape(
                    &self.rigid_body_set,
                    &self.collider_set,
                    pos,
                    shape,
                    filter,
                    |other_collider_handle| {
                        if let Some(&other_lua_id) = self.collider_to_lua.get(&other_collider_handle) {
                            if other_lua_id != lua_id {
                                let pair = if lua_id < other_lua_id {
                                    (lua_id, other_lua_id)
                                } else {
                                    (other_lua_id, lua_id)
                                };
                                overlaps.insert(pair);
                            }
                        }
                        true // continue searching
                    },
                );
            }
        }

        overlaps
    }

    /// Casts a ray downward from a position to detect ground
    /// Returns (hit_distance, hit_y) if ground is found within max_distance
    pub fn raycast_down(&self, origin: [f32; 3], max_distance: f32, exclude_body: Option<RigidBodyHandle>) -> Option<(f32, f32)> {
        let ray = Ray::new(
            point![origin[0], origin[1], origin[2]],
            vector![0.0, -1.0, 0.0],
        );

        let filter = if let Some(body_handle) = exclude_body {
            QueryFilter::default().exclude_rigid_body(body_handle)
        } else {
            QueryFilter::default()
        };

        if let Some((_, hit)) = self.query_pipeline.cast_ray(
            &self.rigid_body_set,
            &self.collider_set,
            &ray,
            max_distance,
            true, // solid
            filter,
        ) {
            let hit_point = ray.point_at(hit);
            Some((hit, hit_point.y))
        } else {
            None
        }
    }

    /// Returns the linear velocity of the kinematic body directly supporting this character.
    /// Used for Roblox-like moving platform carry behavior.
    pub fn get_ground_kinematic_velocity(&self, lua_id: u64, max_distance: f32) -> Option<[f32; 3]> {
        self.get_ground_kinematic_support(lua_id, max_distance)
            .map(|support| support.velocity)
    }

    /// Returns kinematic support data for the surface directly below this character.
    pub fn get_ground_kinematic_support(&self, lua_id: u64, max_distance: f32) -> Option<GroundKinematicSupport> {
        let state = self.character_controllers.get(&lua_id)?;
        let body = self.rigid_body_set.get(state.body_handle)?;
        let origin = body.translation();

        let ray = Ray::new(
            point![origin.x, origin.y, origin.z],
            vector![0.0, -1.0, 0.0],
        );

        let filter = QueryFilter::default()
            .exclude_rigid_body(state.body_handle)
            .exclude_sensors()
            .groups(InteractionGroups::new(GROUP_CHARACTER, GROUP_STATIC));

        let (hit_collider, toi) = self.query_pipeline.cast_ray(
            &self.rigid_body_set,
            &self.collider_set,
            &ray,
            max_distance,
            true,
            filter,
        )?;

        let collider = self.collider_set.get(hit_collider)?;
        let parent = collider.parent()?;
        let ground_body = self.rigid_body_set.get(parent)?;
        if !ground_body.is_kinematic() {
            return None;
        }

        let v = ground_body.linvel();
        let sampled_v = self
            .kinematic_linear_velocities
            .get(&parent)
            .copied()
            .unwrap_or([v.x, v.y, v.z]);
        Some(GroundKinematicSupport {
            velocity: sampled_v,
            distance: toi,
        })
    }

    /// Returns the strongest horizontal contact velocity from overlapping kinematic bodies.
    /// This captures side pushes from moving/rotating anchored obstacles (e.g. spinners).
    pub fn get_character_contact_kinematic_velocity(&self, lua_id: u64) -> Option<[f32; 3]> {
        let state = self.character_controllers.get(&lua_id)?;
        let body = self.rigid_body_set.get(state.body_handle)?;
        let collider = self.collider_set.get(state.collider_handle)?;
        let shape = collider.shape();
        let pos = body.position();
        let center = pos.translation.vector;

        let filter = QueryFilter::default()
            .exclude_rigid_body(state.body_handle)
            .exclude_sensors()
            .groups(InteractionGroups::new(GROUP_CHARACTER, GROUP_STATIC));

        let mut best: Option<[f32; 3]> = None;
        let mut best_horiz_speed_sq = 0.0_f32;

        self.query_pipeline.intersections_with_shape(
            &self.rigid_body_set,
            &self.collider_set,
            pos,
            shape,
            filter,
            |other_collider_handle| {
                let Some(other_collider) = self.collider_set.get(other_collider_handle) else {
                    return true;
                };
                let Some(parent) = other_collider.parent() else {
                    return true;
                };
                let Some(other_body) = self.rigid_body_set.get(parent) else {
                    return true;
                };
                if !other_body.is_kinematic() {
                    return true;
                }

                let point_velocity = other_body.velocity_at_point(&point![center.x, center.y, center.z]);
                let horiz_speed_sq = point_velocity.x * point_velocity.x + point_velocity.z * point_velocity.z;
                if horiz_speed_sq > best_horiz_speed_sq {
                    best_horiz_speed_sq = horiz_speed_sq;
                    best = Some([point_velocity.x, point_velocity.y, point_velocity.z]);
                }
                true
            },
        );

        best.filter(|v| (v[0] * v[0] + v[2] * v[2]) > consts::EPSILON * consts::EPSILON)
    }

    /// Moves a character using the kinematic controller for full 3D translation.
    pub fn move_character(
        &mut self,
        lua_id: u64,
        desired_translation: [f32; 3],
        dt: f32,
    ) -> Option<EffectiveCharacterMovement> {
        let state = self.character_controllers.get(&lua_id)?;
        let body_handle = state.body_handle;
        let collider_handle = state.collider_handle;

        let body = self.rigid_body_set.get(body_handle)?;
        let collider = self.collider_set.get(collider_handle)?;
        let shape = collider.shape();

        // Use full position (translation + rotation) like physics-world
        let current_pos = *body.position();

        // Create fresh controller each step (like physics-world)
        let controller = KinematicCharacterController {
            // Larger offset prevents getting stuck when sliding against surfaces
            offset: CharacterLength::Absolute(0.05),
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(consts::AUTOSTEP_MAX_HEIGHT),
                min_width: CharacterLength::Absolute(consts::AUTOSTEP_MIN_WIDTH),
                include_dynamic_bodies: true,
            }),
            max_slope_climb_angle: 45.0_f32.to_radians(),
            min_slope_slide_angle: 30.0_f32.to_radians(),
            snap_to_ground: Some(CharacterLength::Absolute(consts::SNAP_TO_GROUND)),
            ..Default::default()
        };

        // Match physics-world's collision filter: collide with all except other characters
        let desired = vector![desired_translation[0], desired_translation[1], desired_translation[2]];
        let filter = QueryFilter::default()
            .exclude_rigid_body(body_handle)
            .exclude_sensors()
            .groups(InteractionGroups::new(GROUP_STATIC, Group::ALL & !GROUP_CHARACTER));

        let movement = controller.move_shape(
            dt,
            &self.rigid_body_set,
            &self.collider_set,
            &self.query_pipeline,
            shape,
            &current_pos,
            desired,
            filter,
            |_collision| {},
        );

        // Use set_next_kinematic_translation like physics-world
        // This schedules the movement for the next physics step
        let new_pos = current_pos.translation.vector + movement.translation;

        let body = self.rigid_body_set.get_mut(body_handle)?;
        body.set_next_kinematic_translation(new_pos);

        if let Some(state) = self.character_controllers.get_mut(&lua_id) {
            state.grounded = movement.grounded;
        }
        Some(movement)
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physics_world_creation() {
        let world = PhysicsWorld::new();
        assert_eq!(world.gravity.y, -consts::DEFAULT_GRAVITY);
    }

    #[test]
    fn test_add_anchored_part() {
        let mut world = PhysicsWorld::new();

        let handle = world.add_part(
            1,
            [0.0, 10.0, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [4.0, 1.0, 2.0],
            true,  // anchored
            true,  // can_collide
            PartType::Block,
        );

        assert!(world.has_part(1));
        let pos = world.get_position(handle).unwrap();
        assert_eq!(pos, [0.0, 10.0, 0.0]);
    }

    #[test]
    fn test_dynamic_part_falls() {
        let mut world = PhysicsWorld::new();

        let handle = world.add_part(
            1,
            [0.0, 10.0, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [1.0, 1.0, 1.0],
            false, // not anchored - should fall
            true,
            PartType::Block,
        );

        let initial_pos = world.get_position(handle).unwrap();

        // Step physics a few times
        for _ in 0..10 {
            world.step(1.0 / 60.0);
        }

        let final_pos = world.get_position(handle).unwrap();

        // Y position should be lower due to gravity
        assert!(final_pos[1] < initial_pos[1]);
    }

    #[test]
    fn test_character_raycast_and_move() {
        let mut world = PhysicsWorld::new();

        // Add a floor
        world.add_part(
            1,
            [0.0, 0.0, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [100.0, 1.0, 100.0],
            true,
            true,
            PartType::Block,
        );

        // Add character above floor
        let char_id = 100;
        world.add_character(char_id, [0.0, 6.0, 0.0], 1.0, 5.0);

        world.step(1.0 / 60.0);
        world.query_pipeline.update(&world.collider_set);

        // Test raycast finds ground
        let pos = world.get_character_position(char_id).unwrap();
        let state = world.get_character_state(char_id).unwrap();
        let hit = world.raycast_down(pos, 10.0, Some(state.body_handle));
        assert!(hit.is_some(), "Should detect floor");

        let (distance, ground_y) = hit.unwrap();
        // Character at Y=6, floor top at Y=0.5, distance = 5.5
        assert!(distance > 5.0 && distance < 6.0, "Distance to floor should be ~5.5, got {}", distance);
        assert!((ground_y - 0.5).abs() < 0.1, "Ground Y should be ~0.5 (floor top), got {}", ground_y);

        // Test movement with controller translation (should not suppress horizontal)
        if let Some(state) = world.get_character_state_mut(char_id) {
            state.grounded = true;
        }
        let movement = world.move_character(char_id, [1.0, -0.05, 0.0], 1.0 / 60.0).unwrap();
        world.step(1.0 / 60.0);

        let final_pos = world.get_character_position(char_id).unwrap();
        assert!(movement.translation.x.abs() > 0.0, "Horizontal movement should be applied");
        assert!(final_pos[0] > 0.5, "Should have moved in X");
    }

    #[test]
    fn test_character_horizontal_movement_when_grounded() {
        let mut world = PhysicsWorld::new();

        world.add_part(
            1,
            [0.0, 0.0, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [100.0, 1.0, 100.0],
            true,
            true,
            PartType::Block,
        );

        let char_id = 200;
        world.add_character(char_id, [0.0, 3.0, 0.0], 1.0, 5.0);
        if let Some(state) = world.get_character_state_mut(char_id) {
            state.grounded = true;
        }

        let movement = world.move_character(char_id, [1.0, -0.05, 0.0], 1.0 / 60.0).unwrap();
        assert!(
            movement.translation.x.abs() > 0.0 || movement.translation.z.abs() > 0.0,
            "Horizontal movement should not be suppressed when grounded"
        );
    }

    #[test]
    fn test_character_ignores_sensor_colliders() {
        let mut world = PhysicsWorld::new();

        // Floor so the character can move while grounded.
        world.add_part(
            1,
            [0.0, -0.5, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [100.0, 1.0, 100.0],
            true,
            true,
            PartType::Block,
        );

        // Non-collidable "trigger" directly in the movement path.
        world.add_part(
            2,
            [0.0, 1.0, -8.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [8.0, 2.0, 8.0],
            true,
            false, // sensor
            PartType::Block,
        );

        let char_id = 300;
        world.add_character(char_id, [0.0, 2.6, 8.0], 1.0, 5.0);

        let dt = 1.0 / 60.0;
        for _ in 0..120 {
            world.query_pipeline.update(&world.collider_set);
            world.move_character(char_id, [0.0, 0.0, -0.3], dt).unwrap();
            world.step(dt);
        }

        let final_pos = world.get_character_position(char_id).unwrap();
        assert!(
            final_pos[2] < -6.0,
            "Character should pass through CanCollide=false sensor. Final z={}",
            final_pos[2]
        );
    }

    #[test]
    fn test_get_ground_kinematic_velocity() {
        let mut world = PhysicsWorld::new();

        // Moving platform under the character.
        let platform = world.add_part(
            1,
            [0.0, 0.0, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [10.0, 1.0, 10.0],
            true,
            true,
            PartType::Block,
        );

        let char_id = 301;
        world.add_character(char_id, [0.0, 2.6, 0.0], 1.0, 5.0);

        let dt = 1.0 / 60.0;
        world.step(dt);
        world.query_pipeline.update(&world.collider_set);

        // Move the kinematic platform upward one step.
        world.set_kinematic_position(platform, [0.0, 0.2, 0.0]);
        world.step(dt);
        world.query_pipeline.update(&world.collider_set);

        let v = world
            .get_ground_kinematic_velocity(char_id, 4.0)
            .expect("Expected supporting kinematic velocity");
        assert!(v[1] > 0.0, "Platform upward velocity should be positive, got {:?}", v);
    }

    #[test]
    fn test_get_character_contact_kinematic_velocity_for_rotating_body() {
        let mut world = PhysicsWorld::new();

        let spinner = world.add_part(
            1,
            [0.0, 2.0, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [16.0, 4.0, 3.0],
            true,
            true,
            PartType::Block,
        );

        let char_id = 302;
        world.add_character(char_id, [0.0, 2.0, 2.0], 1.0, 5.0);

        let dt = 1.0 / 60.0;
        world.step(dt);
        world.query_pipeline.update(&world.collider_set);

        let theta = 0.5_f32;
        let c = theta.cos();
        let s = theta.sin();
        let rot_y = [
            [c, 0.0, s],
            [0.0, 1.0, 0.0],
            [-s, 0.0, c],
        ];
        world.set_kinematic_rotation(spinner, &rot_y);
        world.step(dt);
        world.query_pipeline.update(&world.collider_set);

        let v = world
            .get_character_contact_kinematic_velocity(char_id)
            .expect("Expected contact velocity from rotating kinematic body");
        let horiz = (v[0] * v[0] + v[2] * v[2]).sqrt();
        assert!(
            horiz > 0.1,
            "Expected meaningful horizontal contact velocity, got {:?}",
            v
        );
    }

    #[test]
    fn test_character_movement_after_landing_on_thin_platform() {
        let mut world = PhysicsWorld::new();

        // Floor at Y=0 (like the game)
        world.add_part(
            1,
            [0.0, -1.0, 0.0],       // center at Y=-1
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [100.0, 2.0, 100.0],    // top at Y=0
            true,
            true,
            PartType::Block,
        );

        // Thin platform at Y=0.1 (like base platform in tsunami game)
        world.add_part(
            2,
            [0.0, 0.1, 0.0],        // center at Y=0.1
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [30.0, 0.2, 30.0],      // top at Y=0.2
            true,
            true,
            PartType::Block,
        );

        // Character spawns at Y=3 (above platform)
        let char_id = 100;
        world.add_character(char_id, [0.0, 3.0, 0.0], 1.0, 5.0);

        // Simulate falling (like the game does)
        let dt = 1.0 / 60.0;
        for _ in 0..30 {  // About 0.5 seconds of falling
            world.query_pipeline.update(&world.collider_set);
            let gravity_movement: f32 = -196.2 * dt * dt;  // Gravity acceleration
            world.move_character(char_id, [0.0, gravity_movement.max(-0.2), 0.0], dt);
            world.step(dt);
        }

        // Character should now be grounded
        let pos_after_landing = world.get_character_position(char_id).unwrap();
        println!("Position after landing: {:?}", pos_after_landing);

        // Try horizontal movement (like the game does)
        world.query_pipeline.update(&world.collider_set);
        let movement = world.move_character(char_id, [0.26, -0.02, 0.0], dt).unwrap();

        println!("Movement result: {:?}", movement.translation);

        // This should NOT be blocked
        assert!(
            movement.translation.x.abs() > 0.1,
            "Horizontal movement should work after landing. Got: {:?}",
            movement.translation
        );
    }

    #[test]
    fn test_character_direction_reversal() {
        let mut world = PhysicsWorld::new();

        // Floor at Y=0 (top surface)
        world.add_part(
            1,
            [0.0, -1.0, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [200.0, 2.0, 200.0],
            true,
            true,
            PartType::Block,
        );

        // Character with radius=1.0, height=5.0 (same as game)
        // Capsule: half_height=1.5, radius=1.0
        // Bottom of capsule is at Y = center - 1.5 - 1.0 = center - 2.5
        // For bottom to be at Y=0 (floor top), center should be at Y=2.5
        // Add small margin: Y=2.6
        let char_id = 100;
        world.add_character(char_id, [0.0, 2.6, 0.0], 1.0, 5.0);

        let dt = 1.0 / 60.0;

        // Just update the query pipeline, don't fall
        world.step(dt);
        world.query_pipeline.update(&world.collider_set);

        let pos_after_landing = world.get_character_position(char_id).unwrap();
        println!("After landing: {:?}", pos_after_landing);

        // Move +X for 20 frames
        // When grounded, desired_y should be 0 (game logic zeros it)
        println!("\n=== Moving +X ===");
        let mut grounded = false;
        for i in 0..20 {
            world.query_pipeline.update(&world.collider_set);
            // When grounded, zero vertical like the game does
            let desired_y = if grounded { 0.0 } else { -0.008 };
            let movement = world.move_character(char_id, [0.2, desired_y, 0.0], dt).unwrap();
            grounded = movement.grounded;
            world.step(dt);
            if i < 5 || movement.translation.x.abs() < 0.01 {
                let pos = world.get_character_position(char_id).unwrap();
                println!("  Frame {}: pos={:.2} applied_x={:.3} grounded={}", i, pos[0], movement.translation.x, movement.grounded);
            }
        }

        let pos_after_plus_x = world.get_character_position(char_id).unwrap();
        println!("After +X: {:?}", pos_after_plus_x);
        assert!(pos_after_plus_x[0] > 2.0, "+X movement should work");

        // Now reverse: move -X for 20 frames
        println!("\n=== Moving -X (reversal) ===");
        let mut stuck_count = 0;
        for i in 0..20 {
            world.query_pipeline.update(&world.collider_set);
            // When grounded, zero vertical like the game does
            let desired_y = if grounded { 0.0 } else { -0.008 };
            let movement = world.move_character(char_id, [-0.2, desired_y, 0.0], dt).unwrap();
            grounded = movement.grounded;
            world.step(dt);
            let pos = world.get_character_position(char_id).unwrap();
            println!("  Frame {}: pos={:.2} applied_x={:.3} grounded={}", i, pos[0], movement.translation.x, movement.grounded);
            if movement.translation.x.abs() < 0.01 {
                stuck_count += 1;
            }
        }

        let pos_after_minus_x = world.get_character_position(char_id).unwrap();
        println!("After -X: {:?}", pos_after_minus_x);

        // Should have moved back toward 0
        assert!(
            pos_after_minus_x[0] < pos_after_plus_x[0] - 1.0,
            "Direction reversal should work! pos_after_plus_x={} pos_after_minus_x={} stuck_frames={}",
            pos_after_plus_x[0], pos_after_minus_x[0], stuck_count
        );
    }

    #[test]
    fn test_character_autostep_over_small_obstacle() {
        let mut world = PhysicsWorld::new();

        // Floor
        world.add_part(
            1,
            [0.0, -0.5, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [100.0, 1.0, 100.0],  // top at Y=0
            true,
            true,
            PartType::Block,
        );

        // Small obstacle (0.3 studs tall, should be steppable with max_height=0.5)
        world.add_part(
            2,
            [5.0, 0.15, 0.0],      // center at Y=0.15
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [1.0, 0.3, 4.0],       // top at Y=0.3
            true,
            true,
            PartType::Block,
        );

        // Character starts at X=0, grounded
        let char_id = 100;
        world.add_character(char_id, [0.0, 2.6, 0.0], 1.0, 5.0);

        let dt = 1.0 / 60.0;
        world.step(dt);
        world.query_pipeline.update(&world.collider_set);

        let start_pos = world.get_character_position(char_id).unwrap();
        println!("Start: {:?}", start_pos);

        // Move toward and over the obstacle
        for i in 0..60 {
            world.query_pipeline.update(&world.collider_set);
            let movement = world.move_character(char_id, [0.2, 0.0, 0.0], dt).unwrap();
            world.step(dt);

            let pos = world.get_character_position(char_id).unwrap();
            if i % 10 == 0 || (pos[0] > 4.0 && pos[0] < 6.0) {
                println!("Frame {}: pos=({:.2}, {:.2}, {:.2}) grounded={}",
                    i, pos[0], pos[1], pos[2], movement.grounded);
            }
        }

        let final_pos = world.get_character_position(char_id).unwrap();
        println!("Final: {:?}", final_pos);

        // Character should have moved past the obstacle (X > 6)
        assert!(
            final_pos[0] > 6.0,
            "Character should autostep over 0.3 stud obstacle. Final X={:.2}",
            final_pos[0]
        );
    }

    #[test]
    fn test_character_blocked_by_tall_obstacle() {
        let mut world = PhysicsWorld::new();

        // Floor
        world.add_part(
            1,
            [0.0, -0.5, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [100.0, 1.0, 100.0],
            true,
            true,
            PartType::Block,
        );

        // Tall obstacle (1.0 stud tall, should block with max_height=0.5)
        world.add_part(
            2,
            [5.0, 0.5, 0.0],
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [1.0, 1.0, 4.0],       // top at Y=1.0
            true,
            true,
            PartType::Block,
        );

        let char_id = 100;
        world.add_character(char_id, [0.0, 2.6, 0.0], 1.0, 5.0);

        let dt = 1.0 / 60.0;
        world.step(dt);
        world.query_pipeline.update(&world.collider_set);

        // Move toward the obstacle
        for _ in 0..60 {
            world.query_pipeline.update(&world.collider_set);
            world.move_character(char_id, [0.2, 0.0, 0.0], dt);
            world.step(dt);
        }

        let final_pos = world.get_character_position(char_id).unwrap();
        println!("Final pos with tall obstacle: {:?}", final_pos);

        // Character should be blocked (X < 5, can't step over 1.0 stud obstacle)
        assert!(
            final_pos[0] < 5.0,
            "Character should be blocked by 1.0 stud obstacle. Final X={:.2}",
            final_pos[0]
        );
    }

    #[test]
    fn test_character_step_up_onto_platform() {
        // Mimics tsunami scenario: ground + raised platform
        let mut world = PhysicsWorld::new();

        // Ground (lower level) - extends UNDER the platform to avoid seam
        world.add_part(
            1,
            [0.0, -0.5, 0.0],        // center
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [200.0, 1.0, 100.0],     // top at Y=0, extends from X=-100 to X=100
            true,
            true,
            PartType::Block,
        );

        // Raised platform (like tsunami base) - sits ON TOP of ground
        world.add_part(
            2,
            [50.0, 0.1, 0.0],        // center at Y=0.1
            &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [100.0, 0.2, 100.0],     // top at Y=0.2, bottom at Y=0, X from 0 to 100
            true,
            true,
            PartType::Block,
        );

        // Character starts on ground (left side), will try to step up onto platform
        let char_id = 100;
        world.add_character(char_id, [-5.0, 2.6, 0.0], 1.0, 5.0);

        let dt = 1.0 / 60.0;
        world.step(dt);
        world.query_pipeline.update(&world.collider_set);

        let start_pos = world.get_character_position(char_id).unwrap();
        println!("Start (on ground): {:?}", start_pos);

        // Move +X toward and onto the platform
        for i in 0..40 {
            world.query_pipeline.update(&world.collider_set);
            let movement = world.move_character(char_id, [0.2, 0.0, 0.0], dt).unwrap();
            world.step(dt);

            let pos = world.get_character_position(char_id).unwrap();
            if i % 5 == 0 || (pos[0] > -2.0 && pos[0] < 5.0) {
                println!("Frame {}: pos=({:.2}, {:.2}, {:.2}) grounded={}",
                    i, pos[0], pos[1], pos[2], movement.grounded);
            }
        }

        let final_pos = world.get_character_position(char_id).unwrap();
        println!("Final: {:?}", final_pos);

        // Character should step up onto platform (X > 2, past the edge at X=0)
        assert!(
            final_pos[0] > 2.0,
            "Character should step UP onto 0.2 stud platform. Final X={:.2}",
            final_pos[0]
        );
    }
}

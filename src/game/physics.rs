use rapier3d::control::{
    CharacterAutostep, CharacterLength, EffectiveCharacterMovement, KinematicCharacterController,
};
use rapier3d::prelude::*;
use std::collections::HashMap;

use super::constants::physics as consts;

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
    pub grounded: bool,
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
    /// Character controllers for player movement
    pub character_controllers: HashMap<u64, CharacterControllerState>,
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
            character_controllers: HashMap::new(),
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
        rotation: [f32; 4], // quaternion [x, y, z, w]
        size: [f32; 3],
        anchored: bool,
        can_collide: bool,
    ) -> RigidBodyHandle {
        // Create rigid body
        let body = if anchored {
            RigidBodyBuilder::kinematic_position_based()
        } else {
            RigidBodyBuilder::dynamic()
        }
        .translation(vector![position[0], position[1], position[2]])
        .rotation(vector![rotation[0], rotation[1], rotation[2]]) // Use axis-angle
        .build();

        let handle = self.rigid_body_set.insert(body);

        // Create collider (box shape, half-extents)
        // Static geometry collides with everything (characters and other static)
        let collider = ColliderBuilder::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0)
            .sensor(!can_collide) // If can_collide is false, make it a sensor (no physical response)
            .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
            .build();

        self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set);

        // Store mappings
        self.lua_to_body.insert(lua_id, handle);
        self.body_to_lua.insert(handle, lua_id);

        handle
    }

    /// Removes a part from the physics world
    pub fn remove_part(&mut self, lua_id: u64) -> bool {
        if let Some(handle) = self.lua_to_body.remove(&lua_id) {
            self.body_to_lua.remove(&handle);
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
    pub fn set_kinematic_position(&mut self, handle: RigidBodyHandle, position: [f32; 3]) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_kinematic() {
                body.set_next_kinematic_translation(vector![position[0], position[1], position[2]]);
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
                } else {
                    body.set_body_type(RigidBodyType::Dynamic, true);
                }
            }
        }
    }

    /// Updates the size of a part's collider
    pub fn set_size(&mut self, lua_id: u64, size: [f32; 3]) {
        if let Some(&handle) = self.lua_to_body.get(&lua_id) {
            // Get colliders attached to this body
            if let Some(body) = self.rigid_body_set.get(handle) {
                let colliders: Vec<_> = body.colliders().iter().cloned().collect();

                // Remove old colliders and add new ones with updated size
                for collider_handle in colliders {
                    self.collider_set.remove(
                        collider_handle,
                        &mut self.island_manager,
                        &mut self.rigid_body_set,
                        true,
                    );
                }

                // Add new collider with updated size (maintain static collision group)
                let collider =
                    ColliderBuilder::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0)
                        .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
                        .build();

                self.collider_set
                    .insert_with_parent(collider, handle, &mut self.rigid_body_set);
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
            grounded: false,
        };

        self.character_controllers.insert(lua_id, state);
        self.lua_to_body.insert(lua_id, body_handle);
        self.body_to_lua.insert(body_handle, lua_id);

        body_handle
    }

    /// Sets the target position for a character (for Goto action)
    pub fn set_character_target(&mut self, lua_id: u64, target: Option<[f32; 3]>) {
        if let Some(state) = self.character_controllers.get_mut(&lua_id) {
            state.target_position = target;
        }
    }

    /// Teleports a character to a specific position (clears target + vertical velocity)
    pub fn set_character_position(&mut self, lua_id: u64, position: [f32; 3]) {
        if let Some(state) = self.character_controllers.get_mut(&lua_id) {
            if let Some(body) = self.rigid_body_set.get_mut(state.body_handle) {
                body.set_translation(vector![position[0], position[1], position[2]], true);
            }
            state.target_position = None;
            state.vertical_velocity = 0.0;
        }
    }

    /// Gets the current position of a character
    pub fn get_character_position(&self, lua_id: u64) -> Option<[f32; 3]> {
        let state = self.character_controllers.get(&lua_id)?;
        let body = self.rigid_body_set.get(state.body_handle)?;
        let pos = body.translation();
        Some([pos.x, pos.y, pos.z])
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

        // Cast ray and check if we hit something before reaching the target
        if let Some((_, hit_dist)) = self.query_pipeline.cast_ray(
            &self.rigid_body_set,
            &self.collider_set,
            &ray,
            max_dist,
            true, // solid
            filter,
        ) {
            // Hit something before reaching target
            hit_dist >= max_dist - 0.1 // Small tolerance
        } else {
            true // No obstacle hit
        }
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
            [0.0, 0.0, 0.0, 1.0],
            [4.0, 1.0, 2.0],
            true,  // anchored
            true,  // can_collide
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
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            false, // not anchored - should fall
            true,
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
            [0.0, 0.0, 0.0, 1.0],
            [100.0, 1.0, 100.0],
            true,
            true,
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
            [0.0, 0.0, 0.0, 1.0],
            [100.0, 1.0, 100.0],
            true,
            true,
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
    fn test_character_movement_after_landing_on_thin_platform() {
        let mut world = PhysicsWorld::new();

        // Floor at Y=0 (like the game)
        world.add_part(
            1,
            [0.0, -1.0, 0.0],       // center at Y=-1
            [0.0, 0.0, 0.0, 1.0],
            [100.0, 2.0, 100.0],    // top at Y=0
            true,
            true,
        );

        // Thin platform at Y=0.1 (like base platform in tsunami game)
        world.add_part(
            2,
            [0.0, 0.1, 0.0],        // center at Y=0.1
            [0.0, 0.0, 0.0, 1.0],
            [30.0, 0.2, 30.0],      // top at Y=0.2
            true,
            true,
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
            [0.0, 0.0, 0.0, 1.0],
            [200.0, 2.0, 200.0],
            true,
            true,
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
            [0.0, 0.0, 0.0, 1.0],
            [100.0, 1.0, 100.0],  // top at Y=0
            true,
            true,
        );

        // Small obstacle (0.3 studs tall, should be steppable with max_height=0.5)
        world.add_part(
            2,
            [5.0, 0.15, 0.0],      // center at Y=0.15
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.3, 4.0],       // top at Y=0.3
            true,
            true,
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
            [0.0, 0.0, 0.0, 1.0],
            [100.0, 1.0, 100.0],
            true,
            true,
        );

        // Tall obstacle (1.0 stud tall, should block with max_height=0.5)
        world.add_part(
            2,
            [5.0, 0.5, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 4.0],       // top at Y=1.0
            true,
            true,
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
            [0.0, 0.0, 0.0, 1.0],
            [200.0, 1.0, 100.0],     // top at Y=0, extends from X=-100 to X=100
            true,
            true,
        );

        // Raised platform (like tsunami base) - sits ON TOP of ground
        world.add_part(
            2,
            [50.0, 0.1, 0.0],        // center at Y=0.1
            [0.0, 0.0, 0.0, 1.0],
            [100.0, 0.2, 100.0],     // top at Y=0.2, bottom at Y=0, X from 0 to 100
            true,
            true,
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

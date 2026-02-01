use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;
use std::collections::HashMap;

/// State for a character controller (player or NPC)
pub struct CharacterControllerState {
    pub controller: KinematicCharacterController,
    pub collider_handle: ColliderHandle,
    pub body_handle: RigidBodyHandle,
    pub vertical_velocity: f32,
    pub target_position: Option<[f32; 3]>,
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
    /// Creates a new physics world with Roblox default gravity (196.2 studs/s^2)
    pub fn new() -> Self {
        Self {
            gravity: vector![0.0, -196.2, 0.0], // Roblox default gravity
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
        let collider = ColliderBuilder::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0)
            .sensor(!can_collide) // If can_collide is false, make it a sensor (no physical response)
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

                // Add new collider with updated size
                let collider =
                    ColliderBuilder::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0).build();

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
        let half_height = (height - 2.0 * radius).max(0.0) / 2.0;
        let collider = ColliderBuilder::capsule_y(half_height, radius).build();
        let collider_handle = self
            .collider_set
            .insert_with_parent(collider, body_handle, &mut self.rigid_body_set);

        // Create character controller with Roblox-like settings
        let mut controller = KinematicCharacterController::default();
        controller.autostep = Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(0.5),
            min_width: CharacterLength::Absolute(0.3),
            include_dynamic_bodies: false,
        });
        controller.max_slope_climb_angle = 45.0_f32.to_radians();
        controller.snap_to_ground = Some(CharacterLength::Absolute(0.1));
        controller.offset = CharacterLength::Absolute(0.01);

        let state = CharacterControllerState {
            controller,
            collider_handle,
            body_handle,
            vertical_velocity: 0.0,
            target_position: None,
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

    /// Moves a character horizontally (with wall collision) and sets Y position directly
    /// This combines horizontal movement and vertical positioning in one operation
    pub fn move_character_and_set_y(&mut self, lua_id: u64, dx: f32, dz: f32, new_y: f32, dt: f32) -> Option<[f32; 3]> {
        let state = self.character_controllers.get(&lua_id)?;
        let body_handle = state.body_handle;
        let collider_handle = state.collider_handle;

        let body = self.rigid_body_set.get(body_handle)?;
        let collider = self.collider_set.get(collider_handle)?;

        let current_pos = *body.translation();

        // Calculate horizontal movement with collision
        let desired = vector![dx, 0.0, dz];
        let filter = QueryFilter::default().exclude_rigid_body(body_handle);

        let movement = state.controller.move_shape(
            dt,
            &self.rigid_body_set,
            &self.collider_set,
            &self.query_pipeline,
            collider.shape(),
            &Isometry::translation(current_pos.x, current_pos.y, current_pos.z),
            desired,
            filter,
            |_| {},
        );

        // Combine: horizontal from controller, vertical set directly
        let new_pos = vector![
            current_pos.x + movement.translation.x,
            new_y,
            current_pos.z + movement.translation.z
        ];

        // Use set_translation directly (not set_next_kinematic_translation) because:
        // - We already computed collision-corrected movement via move_shape()
        // - set_next_kinematic_translation schedules for NEXT step, causing 1-frame delay
        // - Each frame we'd overwrite the previous scheduled position before it applied
        let body = self.rigid_body_set.get_mut(body_handle)?;
        body.set_translation(new_pos, true);

        Some([new_pos.x, new_pos.y, new_pos.z])
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
        assert_eq!(world.gravity.y, -196.2);
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
        world.add_character(char_id, [0.0, 5.0, 0.0], 0.5, 2.0);

        world.step(1.0 / 60.0);
        world.query_pipeline.update(&world.collider_set);

        // Test raycast finds ground
        let pos = world.get_character_position(char_id).unwrap();
        let state = world.get_character_state(char_id).unwrap();
        let hit = world.raycast_down(pos, 10.0, Some(state.body_handle));
        assert!(hit.is_some(), "Should detect floor");

        let (distance, ground_y) = hit.unwrap();
        // Character at Y=5, floor top at Y=0.5, distance = 4.5
        assert!(distance > 4.0 && distance < 5.0, "Distance to floor should be ~4.5, got {}", distance);
        assert!((ground_y - 0.5).abs() < 0.1, "Ground Y should be ~0.5 (floor top), got {}", ground_y);

        // Test horizontal movement with Y positioning
        let new_y = ground_y + 1.0; // half-height
        world.move_character_and_set_y(char_id, 1.0, 0.0, new_y, 1.0 / 60.0);
        world.step(1.0 / 60.0);

        let final_pos = world.get_character_position(char_id).unwrap();
        assert!(final_pos[0] > 0.5, "Should have moved in X");
        assert!((final_pos[1] - new_y).abs() < 0.1, "Y should be set directly");
    }
}

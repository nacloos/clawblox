use rapier3d::prelude::*;
use std::collections::HashMap;

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
}

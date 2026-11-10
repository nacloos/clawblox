use nalgebra::UnitQuaternion;
use rapier3d::control::{
    CharacterAutostep, CharacterLength, EffectiveCharacterMovement, KinematicCharacterController,
};
use rapier3d::prelude::*;

use super::{rotation_matrix_to_quaternion, PhysicsWorld};
use crate::game::constants::humanoid as humanoid_consts;
use crate::game::constants::physics as consts;
use crate::game::lua::services::physics_service::CollisionGroupMask;

fn interaction_groups(collision_group: CollisionGroupMask) -> InteractionGroups {
    InteractionGroups::new(
        Group::from(collision_group.membership),
        Group::from(collision_group.filter),
    )
}

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
    pub horizontal_velocity: [f32; 2],
    pub collision_group: CollisionGroupMask,
    /// Half of the capsule's total height (center to tip). The ground
    /// sensor ray starts at the body center, so a support reported closer
    /// than this is inside the capsule (penetrating), not under it.
    pub capsule_half_extent: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct GroundKinematicSupport {
    pub velocity: [f32; 3],
    pub distance: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DynamicSupportContact {
    pub lua_id: u64,
    pub point: [f32; 3],
    pub distance: f32,
}

impl PhysicsWorld {
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
        let collider = ColliderBuilder::capsule_y(half_height, radius)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .active_collision_types(ActiveCollisionTypes::all())
            .collision_groups(InteractionGroups::all())
            .solver_groups(InteractionGroups::all())
            .build();
        let collider_handle =
            self.collider_set
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
            horizontal_velocity: [0.0, 0.0],
            collision_group: CollisionGroupMask::default(),
            capsule_half_extent: height * 0.5,
        };

        self.character_controllers.insert(lua_id, state);
        self.lua_to_body.insert(lua_id, body_handle);
        self.body_to_lua.insert(body_handle, lua_id);
        self.body_to_luas.insert(body_handle, vec![lua_id]);
        self.lua_to_collider.insert(lua_id, collider_handle);
        self.lua_to_local_frame.insert(lua_id, Isometry::identity());
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
        self.character_controllers
            .get(&lua_id)
            .map(|s| s.walk_speed)
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
            state.horizontal_velocity = [0.0, 0.0];
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

    /// Sets the full orientation for a character controller body.
    pub fn set_character_rotation(&mut self, lua_id: u64, rotation: &[[f32; 3]; 3]) -> bool {
        let Some(state) = self.character_controllers.get(&lua_id) else {
            return false;
        };
        let Some(body) = self.rigid_body_set.get_mut(state.body_handle) else {
            return false;
        };
        let rot = rotation_matrix_to_quaternion(rotation);
        body.set_rotation(rot, true);
        body.set_next_kinematic_rotation(rot);
        true
    }

    /// Updates the collision group used by the character controller collider and movement queries.
    pub fn set_character_collision_group(
        &mut self,
        lua_id: u64,
        collision_group: CollisionGroupMask,
    ) {
        let Some(state) = self.character_controllers.get_mut(&lua_id) else {
            return;
        };
        if state.collision_group == collision_group {
            return;
        }
        state.collision_group = collision_group;
        if let Some(collider) = self.collider_set.get_mut(state.collider_handle) {
            let groups = interaction_groups(collision_group);
            collider.set_collision_groups(groups);
            collider.set_solver_groups(groups);
        }
    }

    /// Gets the character controller state (for checking grounded, target, etc.)
    pub fn get_character_state(&self, lua_id: u64) -> Option<&CharacterControllerState> {
        self.character_controllers.get(&lua_id)
    }

    /// Gets mutable character controller state
    pub fn get_character_state_mut(
        &mut self,
        lua_id: u64,
    ) -> Option<&mut CharacterControllerState> {
        self.character_controllers.get_mut(&lua_id)
    }

    /// Removes a character controller
    pub fn remove_character(&mut self, lua_id: u64) -> bool {
        if let Some(state) = self.character_controllers.remove(&lua_id) {
            self.lua_to_body.remove(&lua_id);
            self.body_to_lua.remove(&state.body_handle);
            self.body_to_luas.remove(&state.body_handle);
            self.lua_to_collider.remove(&lua_id);
            self.lua_to_local_frame.remove(&lua_id);
            self.collider_to_lua.remove(&state.collider_handle);
            self.kinematic_linear_velocities.remove(&state.body_handle);
            self.queue_body_removal(state.body_handle);
            true
        } else {
            false
        }
    }

    /// Checks if a Lua instance has a character controller
    pub fn has_character(&self, lua_id: u64) -> bool {
        self.character_controllers.contains_key(&lua_id)
    }

    /// Returns the linear velocity of the kinematic body directly supporting this character.
    /// Used for Roblox-like moving platform carry behavior.
    pub fn get_ground_kinematic_velocity(
        &self,
        lua_id: u64,
        max_distance: f32,
    ) -> Option<[f32; 3]> {
        self.get_ground_kinematic_support(lua_id, max_distance)
            .map(|support| support.velocity)
    }

    /// Returns kinematic support data for the surface directly below this character.
    pub fn get_ground_kinematic_support(
        &self,
        lua_id: u64,
        max_distance: f32,
    ) -> Option<GroundKinematicSupport> {
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
            .groups(interaction_groups(state.collision_group));

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

    /// Returns the dynamic rigid body directly supporting this character.
    pub fn get_ground_dynamic_support(
        &self,
        lua_id: u64,
        max_distance: f32,
    ) -> Option<DynamicSupportContact> {
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
            .groups(interaction_groups(state.collision_group));

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
        if !ground_body.is_dynamic() {
            return None;
        }
        let lua_id = self.body_to_lua.get(&parent).copied()?;
        Some(DynamicSupportContact {
            lua_id,
            point: [origin.x, origin.y - toi, origin.z],
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
            .groups(interaction_groups(state.collision_group));

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

                let point_velocity =
                    other_body.velocity_at_point(&point![center.x, center.y, center.z]);
                let horiz_speed_sq =
                    point_velocity.x * point_velocity.x + point_velocity.z * point_velocity.z;
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
        self.move_character_with_object_physics(lua_id, desired_translation, dt, true)
    }

    /// Moves a character using the kinematic controller, optionally applying
    /// newer dynamic-object coupling such as explicit push impulses.
    pub fn move_character_with_object_physics(
        &mut self,
        lua_id: u64,
        desired_translation: [f32; 3],
        dt: f32,
        player_object_physics: bool,
    ) -> Option<EffectiveCharacterMovement> {
        let state = self.character_controllers.get(&lua_id)?;
        let body_handle = state.body_handle;
        let collider_handle = state.collider_handle;
        let collision_group = state.collision_group;

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

        let desired = vector![
            desired_translation[0],
            desired_translation[1],
            desired_translation[2]
        ];
        let filter = QueryFilter::default()
            .exclude_rigid_body(body_handle)
            .exclude_sensors()
            .groups(interaction_groups(collision_group));

        let mut collisions = Vec::new();
        let mut movement = controller.move_shape(
            dt,
            &self.rigid_body_set,
            &self.collider_set,
            &self.query_pipeline,
            shape,
            &current_pos,
            desired,
            filter,
            |collision| collisions.push(collision),
        );

        // Floor-wedge recovery. When the capsule comes to rest marginally
        // below the controller's contact offset (landing/snap leaves the
        // bottom at clearance <= offset), every horizontal sweep returns an
        // immediate toi=0 hit against the floor's up-facing normal. The
        // slide projection cannot make progress against a normal
        // perpendicular to the motion, so move_shape burns its iteration
        // budget on the same contact and returns zero translation — the
        // character wedges permanently on flat walkable ground until its
        // height changes. Detect that degenerate result (horizontal motion
        // requested, none applied, every blocking contact floor-like) and
        // redo the sweep once from a slightly lifted pose; snap-to-ground
        // restores proper clearance within the same call.
        let desired_horiz = (desired.x * desired.x + desired.z * desired.z).sqrt();
        let applied_horiz = (movement.translation.x * movement.translation.x
            + movement.translation.z * movement.translation.z)
            .sqrt();
        // Not part of the mesa-island compatibility mode
        // (`player_object_physics = false`), which predates it.
        if player_object_physics
            && desired_horiz > 1.0e-5
            && applied_horiz < desired_horiz * 0.05
            && !collisions.is_empty()
            && collisions
                .iter()
                .all(|c| c.hit.normal1.y.abs().max(c.hit.normal2.y.abs()) > 0.7)
            // Kill-switch for faithful replay of recordings made before this
            // recovery existed (A/B debugging).
            && std::env::var_os("CLAWBLOX_DISABLE_WEDGE_RECOVERY").is_none()
        {
            const WEDGE_LIFT: f32 = 0.02;
            let mut lifted_pos = current_pos;
            lifted_pos.translation.y += WEDGE_LIFT;
            let mut lifted_collisions = Vec::new();
            let lifted_movement = controller.move_shape(
                dt,
                &self.rigid_body_set,
                &self.collider_set,
                &self.query_pipeline,
                shape,
                &lifted_pos,
                desired,
                filter,
                |collision| lifted_collisions.push(collision),
            );
            let lifted_horiz = (lifted_movement.translation.x * lifted_movement.translation.x
                + lifted_movement.translation.z * lifted_movement.translation.z)
                .sqrt();
            if lifted_horiz > applied_horiz + 1.0e-5 {
                movement = lifted_movement;
                movement.translation.y += WEDGE_LIFT;
                collisions = lifted_collisions;
            }
        }

        // Wall-wedge recovery. The same degenerate sweep happens against a
        // vertical face: pressed into a wall, the capsule rests at the
        // controller's contact offset, so every sweep — even one straight down
        // or up along the face — returns an immediate toi=0 hit against the
        // wall's horizontal normal and move_shape applies nothing. The
        // character hangs on the face mid-air for as long as the push lasts,
        // and a jump pressed into the face gives no rise (or wedges partway
        // through a tick's climb). A wall can't legitimately block vertical
        // motion, so detect any shortfall (vertical motion requested, less
        // applied, every blocking contact wall-like) and redo the sweep once
        // from a pose nudged off the face, without the into-wall part of the
        // motion.
        let desired_vert = desired.y.abs();
        if desired_vert > 1.0e-5
            && movement.translation.y.abs() + 1.0e-4 < desired_vert
            && !collisions.is_empty()
            && collisions
                .iter()
                .all(|c| c.hit.normal1.y.abs().max(c.hit.normal2.y.abs()) < 0.3)
            // Newer-physics only: with player_object_physics off the engine
            // reproduces the paper's, wall hang included. Kill-switch for
            // faithful replay of recordings made before this recovery existed.
            && player_object_physics
            && std::env::var_os("CLAWBLOX_DISABLE_WALL_WEDGE_RECOVERY").is_none()
        {
            // Nudge far enough to restore the contact offset plus a margin:
            // a shallow, fast descent along a face can also end slightly
            // inside it, and from there every sweep is blocked.
            const CONTACT_OFFSET: f32 = 0.05;
            const MIN_WALL_NUDGE: f32 = 0.02;
            const MAX_WALL_NUDGE: f32 = 0.5;
            let mut walls: Vec<(Vector<f32>, f32)> = Vec::new();
            for c in &collisions {
                let n = vector![c.hit.normal1.x, 0.0, c.hit.normal1.z];
                let len = n.norm();
                if len <= 1.0e-5 {
                    continue;
                }
                let n = n / len;
                let dist = self
                    .collider_set
                    .get(c.handle)
                    .and_then(|wall| {
                        rapier3d::parry::query::contact(
                            &current_pos,
                            shape,
                            wall.position(),
                            wall.shape(),
                            CONTACT_OFFSET + MAX_WALL_NUDGE,
                        )
                        .ok()
                        .flatten()
                    })
                    .map(|contact| contact.dist)
                    .unwrap_or(CONTACT_OFFSET);
                let push = (CONTACT_OFFSET + 0.01 - dist).clamp(MIN_WALL_NUDGE, MAX_WALL_NUDGE);
                match walls.iter_mut().find(|(m, _)| m.dot(&n) > 0.999) {
                    Some(wall) => wall.1 = wall.1.max(push),
                    None => walls.push((n, push)),
                }
            }
            if !walls.is_empty() {
                let nudge: Vector<f32> = walls.iter().map(|(n, push)| *n * *push).sum();
                let mut along_wall = desired;
                for (n, _) in &walls {
                    let into = along_wall.dot(n);
                    if into < 0.0 {
                        along_wall -= *n * into;
                    }
                }
                let mut nudged_pos = current_pos;
                nudged_pos.translation.vector += nudge;
                let mut nudged_collisions = Vec::new();
                let nudged_movement = controller.move_shape(
                    dt,
                    &self.rigid_body_set,
                    &self.collider_set,
                    &self.query_pipeline,
                    shape,
                    &nudged_pos,
                    along_wall,
                    filter,
                    |collision| nudged_collisions.push(collision),
                );
                if nudged_movement.translation.y.abs() > movement.translation.y.abs() + 1.0e-5 {
                    movement = nudged_movement;
                    movement.translation += nudge;
                    collisions = nudged_collisions;
                }
            }
        }

        // Diagnostic trace for wedged horizontal movement (CLAWBLOX_TRACE_MOVE=1).
        if std::env::var_os("CLAWBLOX_TRACE_MOVE").is_some() {
            let want = (desired.x * desired.x + desired.z * desired.z).sqrt();
            let got = (movement.translation.x * movement.translation.x
                + movement.translation.z * movement.translation.z)
                .sqrt();
            if want > 1.0e-4 && got < want * 0.1 {
                eprintln!(
                    "[move-blocked] pos=({:.4},{:.4},{:.4}) desired=({:.5},{:.5},{:.5}) applied=({:.6},{:.6},{:.6}) grounded={} collisions={}",
                    current_pos.translation.x,
                    current_pos.translation.y,
                    current_pos.translation.z,
                    desired.x,
                    desired.y,
                    desired.z,
                    movement.translation.x,
                    movement.translation.y,
                    movement.translation.z,
                    movement.grounded,
                    collisions.len()
                );
                for c in &collisions {
                    let lua = self.collider_to_lua.get(&c.handle);
                    eprintln!(
                        "  contact lua_id={:?} toi={:.5} normal1=({:.3},{:.3},{:.3}) witness1=({:.3},{:.3},{:.3})",
                        lua,
                        c.hit.time_of_impact,
                        c.hit.normal1.x,
                        c.hit.normal1.y,
                        c.hit.normal1.z,
                        c.hit.witness1.x,
                        c.hit.witness1.y,
                        c.hit.witness1.z
                    );
                }
            }
        }

        // Shove dynamic bodies out of the walking path. Without this, the
        // controller treats every collider as an immovable wall, so a loose
        // part that comes to rest against the capsule (and then falls
        // asleep) walls the character in permanently.
        if player_object_physics && !collisions.is_empty() {
            // Shove only: never transfer the capsule's upward motion. Rapier
            // gives the body the full remaining translation along the contact
            // normal as velocity, so a jump blocked by a part resting on the
            // head handed the part the whole launch (clamped to
            // CHARACTER_CONTACT_SPEED_GAIN_CAP) while the blocked character
            // rose nothing — a free launch for anything balanced on a head.
            let push_collisions: Vec<_> =
                if std::env::var_os("CLAWBLOX_DISABLE_UPWARD_PUSH_FILTER").is_some() {
                    collisions.clone()
                } else {
                    collisions
                        .iter()
                        .map(|c| {
                            let mut c = *c;
                            c.translation_remaining.y = c.translation_remaining.y.min(0.0);
                            c
                        })
                        .collect()
                };
            controller.solve_character_collision_impulses(
                dt,
                &mut self.rigid_body_set,
                &self.collider_set,
                &self.query_pipeline,
                shape,
                consts::CHARACTER_PUSH_MASS,
                push_collisions.iter(),
                filter,
            );
        }

        // Steep-edge support. Rapier reports the controller grounded on any
        // contact less than ~89.9° from up, so a capsule whose lower
        // hemisphere has caught a ledge's top edge from the side counts as
        // standing. Grounded characters don't accumulate gravity, so the
        // character then perches on the lip of a vertical face indefinitely.
        // Keep the grounded flag only if some support within the controller's
        // ground prediction is no steeper than its climbable slope.
        // Newer-physics only, as above: compatibility mode keeps the ledge perch.
        if movement.grounded
            && player_object_physics
            && std::env::var_os("CLAWBLOX_DISABLE_STEEP_EDGE_SUPPORT_CHECK").is_none()
        {
            let end_pos = Translation::from(movement.translation) * current_pos;
            if !self.has_walkable_support(&end_pos, shape, filter, controller.max_slope_climb_angle)
            {
                movement.grounded = false;
            }
        }

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

    /// Whether a character shape at `pos` has a support within the
    /// controller's ground prediction (offset + 0.05, as in Rapier) whose
    /// contact normal is within `max_slope` of up.
    fn has_walkable_support(
        &self,
        pos: &Isometry<f32>,
        shape: &dyn Shape,
        filter: QueryFilter,
        max_slope: f32,
    ) -> bool {
        const GROUND_PREDICTION: f32 = 0.05 + 0.05;
        let min_up = max_slope.cos();
        let aabb = shape.compute_aabb(pos).loosened(GROUND_PREDICTION);
        let mut walkable = false;
        self.query_pipeline
            .colliders_with_aabb_intersecting_aabb(&aabb, |handle| {
                let Some(collider) = self.collider_set.get(*handle) else {
                    return true;
                };
                if !filter.test(&self.rigid_body_set, *handle, collider) {
                    return true;
                }
                if let Ok(Some(contact)) = rapier3d::parry::query::contact(
                    pos,
                    shape,
                    collider.position(),
                    collider.shape(),
                    GROUND_PREDICTION,
                ) {
                    if contact.normal2.y >= min_up {
                        walkable = true;
                        return false;
                    }
                }
                true
            });
        walkable
    }

    /// Snapshot the speeds of dynamic bodies near any character capsule
    /// (within one tick of possible relative closure). Used with
    /// [`clamp_character_contact_speed_gains`] to bound how much speed such a
    /// body can gain in one step: the capsule is kinematic, so its contacts
    /// otherwise carry unlimited authority and pump trapped bodies a little
    /// faster every tick (or eject deeply penetrated ones in a single step)
    /// until they discharge as a launch.
    pub fn snapshot_character_contact_speeds(&self) -> Vec<(RigidBodyHandle, f32, f32)> {
        // Covers the capsule's own per-tick motion plus an already-capped
        // body closing from the other side.
        const CONTACT_MARGIN: f32 = 2.0;
        let mut snapshot: Vec<(RigidBodyHandle, f32, f32)> = Vec::new();
        for state in self.character_controllers.values() {
            let Some(body) = self.rigid_body_set.get(state.body_handle) else {
                continue;
            };
            let Some(collider) = self.collider_set.get(state.collider_handle) else {
                continue;
            };
            let Some(capsule) = collider.shape().as_capsule() else {
                continue;
            };
            let inflated = Capsule::new(
                capsule.segment.a,
                capsule.segment.b,
                capsule.radius + CONTACT_MARGIN,
            );
            let pos = *body.position();
            let filter = QueryFilter::default()
                .exclude_rigid_body(state.body_handle)
                .exclude_sensors()
                .groups(interaction_groups(state.collision_group));
            self.query_pipeline.intersections_with_shape(
                &self.rigid_body_set,
                &self.collider_set,
                &pos,
                &inflated,
                filter,
                |other_collider| {
                    if let Some(other) = self.collider_set.get(other_collider) {
                        if let Some(parent) = other.parent() {
                            if let Some(other_body) = self.rigid_body_set.get(parent) {
                                if other_body.is_dynamic()
                                    && !snapshot.iter().any(|(handle, ..)| *handle == parent)
                                {
                                    snapshot.push((
                                        parent,
                                        other_body.linvel().norm(),
                                        other_body.angvel().norm(),
                                    ));
                                }
                            }
                        }
                    }
                    true
                },
            );
        }
        snapshot
    }

    /// Cap the post-step speed of bodies captured by
    /// [`snapshot_character_contact_speeds`]: each may keep the speed it
    /// already had, but may not gain past `cap` from a step spent in contact
    /// with a character capsule.
    pub fn clamp_character_contact_speed_gains(
        &mut self,
        snapshot: &[(RigidBodyHandle, f32, f32)],
        cap: f32,
    ) {
        // Spin is energy too: an uncapped angular kick moves an assembly's
        // outer parts at omega*r well past any linear cap.
        const ANGULAR_CAP: f32 = 8.0;
        for &(handle, pre_speed, pre_spin) in snapshot {
            let Some(body) = self.rigid_body_set.get_mut(handle) else {
                continue;
            };
            if !body.is_dynamic() {
                continue;
            }
            let allowed = cap.max(pre_speed);
            let velocity = *body.linvel();
            let speed = velocity.norm();
            if speed > allowed {
                body.set_linvel(velocity * (allowed / speed), true);
            }
            let allowed_spin = ANGULAR_CAP.max(pre_spin);
            let spin = *body.angvel();
            let spin_speed = spin.norm();
            if spin_speed > allowed_spin {
                body.set_angvel(spin * (allowed_spin / spin_speed), true);
            }
        }
    }
}

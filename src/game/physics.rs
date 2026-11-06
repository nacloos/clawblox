use nalgebra::{Translation3, UnitQuaternion};
use rapier3d::prelude::*;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use super::constants::physics as consts;
use super::lua::services::physics_service::CollisionGroupMask;
use super::lua::types::{CFrame, PartType, PhysicalProperties, Vector3};
use super::profiling::record_values;

mod align_motors;
mod assemblies;
mod characters;
mod parts;

pub use align_motors::{AlignKind, AlignMotorSpec, AlignTarget};
pub use characters::{CharacterControllerState, GroundKinematicSupport};
use parts::build_collider_with_physical_properties;

/// Large enough for normal worlds, small enough to keep Rapier broad-phase keys
/// away from overflow and floating-point precision collapse.
pub const PHYSICS_WORLD_ENVELOPE: f32 = 50_000.0;
pub const PHYSICS_VELOCITY_ENVELOPE: f32 = 10_000.0;

fn finite_vec3(values: [f32; 3]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn within_abs_envelope(values: [f32; 3], envelope: f32) -> bool {
    values.iter().all(|v| v.abs() <= envelope)
}

pub fn transform_within_physics_envelope(position: [f32; 3]) -> bool {
    finite_vec3(position) && within_abs_envelope(position, PHYSICS_WORLD_ENVELOPE)
}

pub fn velocity_within_physics_envelope(velocity: [f32; 3]) -> bool {
    finite_vec3(velocity) && within_abs_envelope(velocity, PHYSICS_VELOCITY_ENVELOPE)
}

fn rotation_matrix_is_finite(rotation: &[[f32; 3]; 3]) -> bool {
    rotation.iter().flatten().all(|v| v.is_finite())
}

/// Converts a 3x3 rotation matrix to a UnitQuaternion (Shepperd's method).
pub fn rotation_matrix_to_quaternion(m: &[[f32; 3]; 3]) -> UnitQuaternion<f32> {
    // Roblox CFrames are rigid transforms: their 3x3 component is orientation,
    // not scale or shear. Normalize here before handing the matrix to Rapier so
    // a bad script/runtime edge cannot turn a part into a scaled physics frame.
    let basis = CFrame {
        position: Vector3::zero(),
        rotation: *m,
    }
    .orthonormalized();
    let r = basis.rotation;
    let rot = nalgebra::Matrix3::new(
        r[0][0], r[0][1], r[0][2], r[1][0], r[1][1], r[1][2], r[2][0], r[2][1], r[2][2],
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartInteractionState {
    pub can_collide: bool,
    pub can_touch: bool,
    pub can_query: bool,
    pub collision_group: CollisionGroupMask,
}

impl PartInteractionState {
    fn collider_enabled(self) -> bool {
        self.can_collide || self.can_touch || self.can_query
    }
}

#[derive(Debug, Clone)]
pub struct PhysicsAssemblyPart {
    pub lua_id: u64,
    pub position: [f32; 3],
    pub rotation: [[f32; 3]; 3],
    pub size: [f32; 3],
    pub anchored: bool,
    pub can_collide: bool,
    pub can_touch: bool,
    pub can_query: bool,
    pub collision_group: CollisionGroupMask,
    pub shape: PartType,
    pub physical_properties: Option<PhysicalProperties>,
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub position_dirty: bool,
    pub cframe_dirty: bool,
    pub size_dirty: bool,
    pub anchored_dirty: bool,
    pub interaction_dirty: bool,
    pub velocity_dirty: bool,
    pub angular_velocity_dirty: bool,
    pub shape_dirty: bool,
    pub custom_physical_properties_dirty: bool,
}

#[derive(Debug, Clone)]
pub struct PhysicsAssemblySpec {
    pub root_id: u64,
    pub parts: Vec<PhysicsAssemblyPart>,
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
    /// Maps Lua instance ID to current collider physical properties.
    pub lua_to_physical_properties: HashMap<u64, Option<PhysicalProperties>>,
    /// Maps Lua instance ID to current interaction flags used to configure colliders.
    pub lua_to_interactions: HashMap<u64, PartInteractionState>,
    /// Character controllers for player movement
    pub character_controllers: HashMap<u64, CharacterControllerState>,
    /// Maps Rapier collider handle to Lua instance ID (for touch detection)
    pub collider_to_lua: HashMap<ColliderHandle, u64>,
    /// Maps Lua part IDs to their collider handles. Multiple Lua parts can
    /// share one rigid body when rigid welds form a Roblox-style assembly.
    pub lua_to_collider: HashMap<u64, ColliderHandle>,
    /// Body-local transform for each Lua part collider within its physical assembly.
    pub lua_to_local_frame: HashMap<u64, Isometry<Real>>,
    /// Maps a Rapier body to all Lua parts represented by its colliders.
    pub body_to_luas: HashMap<RigidBodyHandle, Vec<u64>>,
    /// Stable structural signatures for physical assemblies. Matching signatures
    /// allow us to preserve Rapier bodies so sleeping and warmstarting work.
    assembly_signatures: HashMap<u64, String>,
    /// Script-driven kinematic linear velocity sampled from per-tick target translation.
    pub kinematic_linear_velocities: HashMap<RigidBodyHandle, [f32; 3]>,
    /// Bodies queued for removal after the current physics step finishes.
    pending_body_removals: Vec<RigidBodyHandle>,
    /// Fixed joints mirroring Lua weld constraints across distinct bodies.
    /// Rebuilt from Lua every tick (see `clear_weld_joints`).
    weld_joint_handles: Vec<ImpulseJointHandle>,
    /// Persistent motor joints backing AlignPosition/AlignOrientation, keyed
    /// by the constraint instance's Lua ID. Kept across ticks so the solver
    /// retains warm-start impulses; recreated only when the involved bodies
    /// change (e.g. an assembly rebuild).
    align_joints: HashMap<u64, align_motors::AlignJointEntry>,
    /// Lazily created fixed body that world-space align targets attach to.
    world_anchor_body: Option<RigidBodyHandle>,
    /// Mesa-island compatibility (`player_object_physics = false`): a part
    /// released from Anchored keeps the motion of its kinematic body, as the
    /// mesa-island engine's in-place body-type switch did.
    pub legacy_unanchor_momentum: bool,
}

fn part_isometry(position: [f32; 3], rotation: &[[f32; 3]; 3]) -> Isometry<Real> {
    Isometry::from_parts(
        Translation3::new(position[0], position[1], position[2]),
        rotation_matrix_to_quaternion(rotation),
    )
}

impl PhysicsWorld {
    fn record_profile_metrics(&self) {
        let mut dynamic_bodies = 0usize;
        let mut sleeping_dynamic_bodies = 0usize;
        let mut kinematic_bodies = 0usize;
        let mut fixed_bodies = 0usize;
        let mut body_index = HashMap::new();
        let mut dynamic_handles = Vec::new();
        let mut active_dynamic_handles = HashSet::new();

        for (handle, body) in self.rigid_body_set.iter() {
            if body.is_dynamic() {
                body_index.insert(handle, dynamic_bodies);
                dynamic_handles.push(handle);
                dynamic_bodies += 1;
                if body.is_sleeping() {
                    sleeping_dynamic_bodies += 1;
                }
            } else if body.is_kinematic() {
                kinematic_bodies += 1;
            } else {
                fixed_bodies += 1;
            }
        }
        active_dynamic_handles.extend(self.island_manager.active_dynamic_bodies().iter().copied());

        let mut parent: Vec<usize> = (0..dynamic_bodies).collect();
        let mut size = vec![1usize; dynamic_bodies];

        fn find(parent: &mut [usize], mut index: usize) -> usize {
            while parent[index] != index {
                parent[index] = parent[parent[index]];
                index = parent[index];
            }
            index
        }

        fn union(parent: &mut [usize], size: &mut [usize], a: usize, b: usize) {
            let mut root_a = find(parent, a);
            let mut root_b = find(parent, b);
            if root_a == root_b {
                return;
            }
            if size[root_a] < size[root_b] {
                std::mem::swap(&mut root_a, &mut root_b);
            }
            parent[root_b] = root_a;
            size[root_a] += size[root_b];
        }

        let mut contact_pairs = 0usize;
        let mut active_contact_pairs = 0usize;
        let mut active_contact_points = 0usize;
        let mut active_dynamic_dynamic_pairs = 0usize;
        let mut active_dynamic_static_pairs = 0usize;

        for pair in self.narrow_phase.contact_pairs() {
            contact_pairs += 1;
            if !pair.has_any_active_contact {
                continue;
            }
            active_contact_pairs += 1;
            active_contact_points += pair
                .manifolds
                .iter()
                .map(|manifold| manifold.data.num_active_contacts())
                .sum::<usize>();

            let body1 = self
                .collider_set
                .get(pair.collider1)
                .and_then(|collider| collider.parent());
            let body2 = self
                .collider_set
                .get(pair.collider2)
                .and_then(|collider| collider.parent());
            let Some(body1) = body1 else {
                continue;
            };
            let Some(body2) = body2 else {
                continue;
            };

            match (
                body_index.get(&body1).copied(),
                body_index.get(&body2).copied(),
            ) {
                (Some(a), Some(b)) => {
                    active_dynamic_dynamic_pairs += 1;
                    union(&mut parent, &mut size, a, b);
                }
                (Some(_), None) | (None, Some(_)) => {
                    active_dynamic_static_pairs += 1;
                }
                (None, None) => {}
            }
        }

        let mut component_sizes = HashMap::<usize, usize>::new();
        for index in 0..dynamic_bodies {
            let root = find(&mut parent, index);
            *component_sizes.entry(root).or_default() += 1;
        }
        let dynamic_components = component_sizes.len();
        let largest_dynamic_component = component_sizes.values().copied().max().unwrap_or(0);
        let awake_dynamic_bodies = dynamic_bodies.saturating_sub(sleeping_dynamic_bodies);
        let largest_component_root = component_sizes
            .iter()
            .max_by_key(|(_, component_size)| **component_size)
            .map(|(root, _)| *root);
        let counters = &self.physics_pipeline.counters;
        let length_unit = self.integration_parameters.length_unit;
        let mut above_linear_sleep_threshold = 0usize;
        let mut above_angular_sleep_threshold = 0usize;
        let mut above_any_sleep_threshold = 0usize;
        let mut below_sleep_threshold = 0usize;
        let mut sleep_timer_ready = 0usize;
        let mut max_linear_speed = 0.0f32;
        let mut max_angular_speed = 0.0f32;

        #[derive(Clone, Copy, Default)]
        struct ComponentProfile {
            bodies: usize,
            awake: usize,
            active: usize,
            above_any_sleep_threshold: usize,
            sleep_timer_ready: usize,
        }

        let mut component_profiles = HashMap::<usize, ComponentProfile>::new();
        for (index, handle) in dynamic_handles.iter().copied().enumerate() {
            let root = find(&mut parent, index);
            let Some(body) = self.rigid_body_set.get(handle) else {
                continue;
            };
            let lin_speed = body.linvel().norm();
            let ang_speed = body.angvel().norm();
            max_linear_speed = max_linear_speed.max(lin_speed);
            max_angular_speed = max_angular_speed.max(ang_speed);

            let activation = body.activation();
            let linear_threshold = activation.normalized_linear_threshold * length_unit;
            let above_linear = lin_speed * lin_speed >= linear_threshold * linear_threshold.abs();
            let above_angular = ang_speed * ang_speed
                >= activation.angular_threshold * activation.angular_threshold.abs();
            let above_any = above_linear || above_angular;
            if above_linear {
                above_linear_sleep_threshold += 1;
            }
            if above_angular {
                above_angular_sleep_threshold += 1;
            }
            if above_any {
                above_any_sleep_threshold += 1;
            } else {
                below_sleep_threshold += 1;
            }
            if activation.time_since_can_sleep >= activation.time_until_sleep {
                sleep_timer_ready += 1;
            }

            let profile = component_profiles.entry(root).or_default();
            profile.bodies += 1;
            if !body.is_sleeping() {
                profile.awake += 1;
            }
            if active_dynamic_handles.contains(&handle) {
                profile.active += 1;
            }
            if above_any {
                profile.above_any_sleep_threshold += 1;
            }
            if activation.time_since_can_sleep >= activation.time_until_sleep {
                profile.sleep_timer_ready += 1;
            }
        }

        let largest_component_profile = component_profiles
            .values()
            .max_by_key(|profile| profile.bodies)
            .copied()
            .unwrap_or_default();
        let mut active_kinematic_bodies = 0usize;
        let mut moving_active_kinematic_bodies = 0usize;
        let mut moving_kinematic_largest_component_contacts = 0usize;
        let mut moving_character_largest_component_contacts = 0usize;
        let mut moving_non_character_largest_component_contacts = 0usize;
        let mut max_moving_kinematic_linear_speed = 0.0f32;
        for handle in self
            .island_manager
            .active_kinematic_bodies()
            .iter()
            .copied()
        {
            let Some(body) = self.rigid_body_set.get(handle) else {
                continue;
            };
            active_kinematic_bodies += 1;
            let moving = body.linvel().norm_squared() > f32::EPSILON
                || body.angvel().norm_squared() > f32::EPSILON;
            if moving {
                moving_active_kinematic_bodies += 1;
                max_moving_kinematic_linear_speed =
                    max_moving_kinematic_linear_speed.max(body.linvel().norm());
            }
            if !moving {
                continue;
            }
            let is_character = self
                .character_controllers
                .values()
                .any(|state| state.body_handle == handle);
            for collider_handle in body.colliders() {
                for pair in self.narrow_phase.contact_pairs_with(*collider_handle) {
                    if !pair.has_any_active_contact {
                        continue;
                    }
                    let other_collider = if pair.collider1 == *collider_handle {
                        pair.collider2
                    } else {
                        pair.collider1
                    };
                    let Some(other_body) = self
                        .collider_set
                        .get(other_collider)
                        .and_then(|collider| collider.parent())
                    else {
                        continue;
                    };
                    let Some(other_index) = body_index.get(&other_body).copied() else {
                        continue;
                    };
                    let other_root = find(&mut parent, other_index);
                    if Some(other_root) == largest_component_root {
                        moving_kinematic_largest_component_contacts += 1;
                        if is_character {
                            moving_character_largest_component_contacts += 1;
                        } else {
                            moving_non_character_largest_component_contacts += 1;
                        }
                    }
                }
            }
        }

        record_values(
            "physics.world",
            &[
                ("rigid_bodies", self.rigid_body_set.len() as f64),
                ("colliders", self.collider_set.len() as f64),
                ("dynamic_bodies", dynamic_bodies as f64),
                ("awake_dynamic_bodies", awake_dynamic_bodies as f64),
                ("sleeping_dynamic_bodies", sleeping_dynamic_bodies as f64),
                (
                    "active_dynamic_bodies",
                    self.island_manager.active_dynamic_bodies().len() as f64,
                ),
                ("kinematic_bodies", kinematic_bodies as f64),
                ("fixed_bodies", fixed_bodies as f64),
                ("contact_pairs", contact_pairs as f64),
                ("active_contact_pairs", active_contact_pairs as f64),
                ("active_contact_points", active_contact_points as f64),
                (
                    "active_dynamic_dynamic_pairs",
                    active_dynamic_dynamic_pairs as f64,
                ),
                (
                    "active_dynamic_static_pairs",
                    active_dynamic_static_pairs as f64,
                ),
                ("dynamic_contact_components", dynamic_components as f64),
                (
                    "largest_dynamic_contact_component",
                    largest_dynamic_component as f64,
                ),
                (
                    "largest_component_awake_bodies",
                    largest_component_profile.awake as f64,
                ),
                (
                    "largest_component_active_bodies",
                    largest_component_profile.active as f64,
                ),
                (
                    "largest_component_above_sleep_threshold",
                    largest_component_profile.above_any_sleep_threshold as f64,
                ),
                (
                    "largest_component_sleep_timer_ready",
                    largest_component_profile.sleep_timer_ready as f64,
                ),
                ("active_kinematic_bodies", active_kinematic_bodies as f64),
                (
                    "moving_active_kinematic_bodies",
                    moving_active_kinematic_bodies as f64,
                ),
                (
                    "moving_kinematic_largest_component_contacts",
                    moving_kinematic_largest_component_contacts as f64,
                ),
                (
                    "moving_character_largest_component_contacts",
                    moving_character_largest_component_contacts as f64,
                ),
                (
                    "moving_non_character_largest_component_contacts",
                    moving_non_character_largest_component_contacts as f64,
                ),
                (
                    "max_moving_kinematic_linear_speed",
                    max_moving_kinematic_linear_speed as f64,
                ),
                (
                    "above_linear_sleep_threshold",
                    above_linear_sleep_threshold as f64,
                ),
                (
                    "above_angular_sleep_threshold",
                    above_angular_sleep_threshold as f64,
                ),
                (
                    "above_any_sleep_threshold",
                    above_any_sleep_threshold as f64,
                ),
                ("below_sleep_threshold", below_sleep_threshold as f64),
                ("sleep_timer_ready", sleep_timer_ready as f64),
                ("max_linear_speed", max_linear_speed as f64),
                ("max_angular_speed", max_angular_speed as f64),
                ("rapier_step_ms", counters.step_time_ms()),
                (
                    "rapier_collision_detection_ms",
                    counters.collision_detection_time_ms(),
                ),
                ("rapier_broad_phase_ms", counters.broad_phase_time_ms()),
                ("rapier_narrow_phase_ms", counters.narrow_phase_time_ms()),
                (
                    "rapier_island_construction_ms",
                    counters.island_construction_time_ms(),
                ),
                ("rapier_update_ms", counters.update_time_ms()),
                ("rapier_solver_ms", counters.solver_time_ms()),
                ("rapier_velocity_assembly_ms", counters.assembly_time_ms()),
                (
                    "rapier_velocity_resolution_ms",
                    counters.velocity_resolution_time_ms(),
                ),
                (
                    "rapier_velocity_update_ms",
                    counters.velocity_update_time_ms(),
                ),
            ],
        );
    }

    fn body_is_within_physics_envelope(&self, handle: RigidBodyHandle) -> bool {
        let Some(body) = self.rigid_body_set.get(handle) else {
            return true;
        };
        let pos = body.translation();
        let linvel = body.linvel();
        let angvel = body.angvel();
        let position = [pos.x, pos.y, pos.z];
        let linear_velocity = [linvel.x, linvel.y, linvel.z];
        let angular_velocity = [angvel.x, angvel.y, angvel.z];

        transform_within_physics_envelope(position)
            && velocity_within_physics_envelope(linear_velocity)
            && velocity_within_physics_envelope(angular_velocity)
    }

    fn remove_body_mappings(&mut self, body_handle: RigidBodyHandle) {
        let root_id = self.body_to_lua.remove(&body_handle);
        if let Some(root_id) = root_id {
            self.assembly_signatures.remove(&root_id);
        }

        let lua_ids = self
            .body_to_luas
            .remove(&body_handle)
            .or_else(|| root_id.map(|id| vec![id]))
            .unwrap_or_default();
        for lua_id in lua_ids {
            self.lua_to_body.remove(&lua_id);
            self.lua_to_collider.remove(&lua_id);
            self.lua_to_local_frame.remove(&lua_id);
            self.lua_to_shape.remove(&lua_id);
            self.lua_to_physical_properties.remove(&lua_id);
            self.lua_to_interactions.remove(&lua_id);
        }

        self.kinematic_linear_velocities.remove(&body_handle);
        if let Some(body) = self.rigid_body_set.get(body_handle) {
            for &collider_handle in body.colliders() {
                self.collider_to_lua.remove(&collider_handle);
            }
        }
    }

    /// Removes bodies whose transforms or velocities have left the finite range
    /// Rapier can robustly simulate. Roblox similarly prevents fallen/far-origin
    /// parts from living forever; this keeps one bad dynamic body from halting
    /// the whole server.
    pub fn purge_invalid_bodies(&mut self) -> usize {
        let handles: Vec<_> = self
            .rigid_body_set
            .iter()
            .filter_map(|(handle, _)| {
                if self.body_is_within_physics_envelope(handle) {
                    None
                } else {
                    Some(handle)
                }
            })
            .collect();

        let removed = handles.len();
        for handle in handles {
            self.remove_body_mappings(handle);
            self.queue_body_removal(handle);
        }
        removed
    }

    fn queryable_collider_predicate(&self) -> impl Fn(ColliderHandle, &Collider) -> bool + '_ {
        move |handle, _collider| {
            self.collider_to_lua
                .get(&handle)
                .and_then(|lua_id| self.lua_to_interactions.get(lua_id))
                .map(|state| state.can_query)
                .unwrap_or(true)
        }
    }

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
            lua_to_physical_properties: HashMap::new(),
            lua_to_interactions: HashMap::new(),
            character_controllers: HashMap::new(),
            collider_to_lua: HashMap::new(),
            lua_to_collider: HashMap::new(),
            lua_to_local_frame: HashMap::new(),
            body_to_luas: HashMap::new(),
            assembly_signatures: HashMap::new(),
            kinematic_linear_velocities: HashMap::new(),
            weld_joint_handles: Vec::new(),
            align_joints: HashMap::new(),
            world_anchor_body: None,
            pending_body_removals: Vec::new(),
            legacy_unanchor_momentum: false,
        }
    }

    /// Sets the gravity for the physics world
    pub fn set_gravity(&mut self, gravity_y: f32) {
        self.gravity = vector![0.0, -gravity_y, 0.0];
    }

    /// Sets the number of outer contact-solver iterations per physics step.
    ///
    /// This is a global Rapier integration parameter. Lower values reduce CPU
    /// on large awake islands; higher values improve contact convergence.
    pub fn set_solver_iterations(&mut self, iterations: usize) {
        let iterations = iterations.max(1);
        self.integration_parameters.num_solver_iterations =
            NonZeroUsize::new(iterations).expect("iterations is clamped to at least one");
    }

    /// Steps the physics simulation forward by dt seconds
    pub fn step(&mut self, dt: f32) {
        self.purge_invalid_bodies();
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
        self.purge_invalid_bodies();
        self.record_profile_metrics();
    }

    /// Removes the weld-mirror joints. Weld constraints are rebuilt from Lua
    /// every tick; align motor joints are persistent and managed separately.
    pub fn clear_weld_joints(&mut self) {
        for handle in std::mem::take(&mut self.weld_joint_handles) {
            self.impulse_joint_set.remove(handle, true);
        }
    }

    /// Inserts a fixed joint between two rigid bodies using body-local joint frames.
    pub fn insert_fixed_joint(
        &mut self,
        body1: RigidBodyHandle,
        body2: RigidBodyHandle,
        local_frame1: Isometry<Real>,
        local_frame2: Isometry<Real>,
    ) -> ImpulseJointHandle {
        let joint = FixedJointBuilder::new()
            .contacts_enabled(false)
            .local_frame1(local_frame1)
            .local_frame2(local_frame2);
        let handle = self.impulse_joint_set.insert(body1, body2, joint, true);
        self.weld_joint_handles.push(handle);
        handle
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
        can_touch: bool,
        can_query: bool,
        shape: PartType,
        physical_properties: Option<PhysicalProperties>,
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

        let interaction_state = PartInteractionState {
            can_collide,
            can_touch,
            can_query,
            collision_group: CollisionGroupMask::default(),
        };

        // Create collider with correct shape
        let collider = build_collider_with_physical_properties(
            size,
            shape,
            interaction_state,
            physical_properties,
        );

        let collider_handle =
            self.collider_set
                .insert_with_parent(collider, handle, &mut self.rigid_body_set);

        // Store mappings
        self.lua_to_body.insert(lua_id, handle);
        self.body_to_lua.insert(handle, lua_id);
        self.body_to_luas.insert(handle, vec![lua_id]);
        self.lua_to_collider.insert(lua_id, collider_handle);
        self.lua_to_local_frame.insert(lua_id, Isometry::identity());
        self.lua_to_shape.insert(lua_id, shape);
        self.lua_to_physical_properties
            .insert(lua_id, physical_properties);
        self.lua_to_interactions.insert(lua_id, interaction_state);
        self.collider_to_lua.insert(collider_handle, lua_id);

        handle
    }

    /// Removes a part from the physics world
    pub fn remove_part(&mut self, lua_id: u64) -> bool {
        // If this lua_id is a character HRP, clean up the character controller
        // to avoid stale handles that would panic in rapier during step/move.
        if let Some(state) = self.character_controllers.remove(&lua_id) {
            self.lua_to_body.remove(&lua_id);
            self.body_to_lua.remove(&state.body_handle);
            self.body_to_luas.remove(&state.body_handle);
            self.lua_to_collider.remove(&lua_id);
            self.lua_to_local_frame.remove(&lua_id);
            self.collider_to_lua.remove(&state.collider_handle);
            self.kinematic_linear_velocities.remove(&state.body_handle);
            self.queue_body_removal(state.body_handle);
            return true;
        }

        if let Some(&handle) = self.lua_to_body.get(&lua_id) {
            if let Some(root_id) = self.body_to_lua.get(&handle).copied() {
                self.remove_assembly_by_root(root_id);
                return true;
            }
            self.lua_to_body.remove(&lua_id);
            self.lua_to_shape.remove(&lua_id);
            self.lua_to_physical_properties.remove(&lua_id);
            self.lua_to_interactions.remove(&lua_id);
            self.lua_to_collider.remove(&lua_id);
            self.lua_to_local_frame.remove(&lua_id);
            self.kinematic_linear_velocities.remove(&handle);
            // Remove collider->lua mappings before disabling and queuing the body.
            if let Some(body) = self.rigid_body_set.get(handle) {
                for &ch in body.colliders() {
                    self.collider_to_lua.remove(&ch);
                }
            }
            self.queue_body_removal(handle);
            true
        } else {
            false
        }
    }

    fn queue_body_removal(&mut self, handle: RigidBodyHandle) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            body.set_enabled(false);
            let colliders: Vec<_> = body.colliders().iter().copied().collect();
            for collider_handle in colliders {
                if let Some(collider) = self.collider_set.get_mut(collider_handle) {
                    collider.set_enabled(false);
                }
            }
        }

        if !self.pending_body_removals.contains(&handle) {
            self.pending_body_removals.push(handle);
        }
    }

    /// Finalize body removals that were deferred until after the physics step.
    pub fn flush_pending_removals(&mut self) {
        let pending = std::mem::take(&mut self.pending_body_removals);
        for handle in pending {
            let _ = self.rigid_body_set.remove(
                handle,
                &mut self.island_manager,
                &mut self.collider_set,
                &mut self.impulse_joint_set,
                &mut self.multibody_joint_set,
                true,
            );
        }
        self.island_manager
            .cleanup_removed_rigid_bodies(&mut self.rigid_body_set);
    }

    /// Updates the position of an anchored (kinematic) part
    pub fn set_kinematic_position_with_dt(
        &mut self,
        handle: RigidBodyHandle,
        position: [f32; 3],
        dt: f32,
    ) {
        if !transform_within_physics_envelope(position) {
            self.remove_body_mappings(handle);
            self.queue_body_removal(handle);
            return;
        }
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
        if !rotation_matrix_is_finite(rotation) {
            self.remove_body_mappings(handle);
            self.queue_body_removal(handle);
            return;
        }
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_kinematic() {
                let quat = rotation_matrix_to_quaternion(rotation);
                body.set_next_kinematic_rotation(quat);
            }
        }
    }

    /// Teleports a dynamic body to a world position.
    pub fn set_dynamic_position(&mut self, handle: RigidBodyHandle, position: [f32; 3]) {
        if !transform_within_physics_envelope(position) {
            self.remove_body_mappings(handle);
            self.queue_body_removal(handle);
            return;
        }
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.set_translation(vector![position[0], position[1], position[2]], true);
            }
        }
    }

    /// Sets a dynamic body's world orientation from a 3x3 rotation matrix.
    pub fn set_dynamic_rotation(&mut self, handle: RigidBodyHandle, rotation: &[[f32; 3]; 3]) {
        if !rotation_matrix_is_finite(rotation) {
            self.remove_body_mappings(handle);
            self.queue_body_removal(handle);
            return;
        }
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                let quat = rotation_matrix_to_quaternion(rotation);
                body.set_rotation(quat, true);
            }
        }
    }

    /// Sets the velocity of a dynamic part
    pub fn set_velocity(&mut self, handle: RigidBodyHandle, velocity: [f32; 3]) {
        if !velocity_within_physics_envelope(velocity) {
            self.remove_body_mappings(handle);
            self.queue_body_removal(handle);
            return;
        }
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.set_linvel(vector![velocity[0], velocity[1], velocity[2]], true);
            }
        }
    }

    /// Sets the angular velocity of a dynamic part
    pub fn set_angular_velocity(&mut self, handle: RigidBodyHandle, angular_velocity: [f32; 3]) {
        if !velocity_within_physics_envelope(angular_velocity) {
            self.remove_body_mappings(handle);
            self.queue_body_removal(handle);
            return;
        }
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.set_angvel(
                    vector![
                        angular_velocity[0],
                        angular_velocity[1],
                        angular_velocity[2]
                    ],
                    true,
                );
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
        self.rigid_body_set
            .get(handle)
            .map(|body| quaternion_to_rotation_matrix(body.rotation()))
    }

    fn get_part_world_isometry(&self, lua_id: u64) -> Option<Isometry<Real>> {
        let body_handle = *self.lua_to_body.get(&lua_id)?;
        let body = self.rigid_body_set.get(body_handle)?;
        let local = self
            .lua_to_local_frame
            .get(&lua_id)
            .copied()
            .unwrap_or_else(Isometry::identity);
        Some(*body.position() * local)
    }

    /// Projects a Roblox-style CFrame write to one member of a welded assembly
    /// across every Lua part represented by the same physical body.
    ///
    /// Setting BasePart.CFrame on a welded member moves the assembly so that the
    /// written member reaches the requested frame while all weld offsets are
    /// preserved. This helper returns the resulting world frames for every Lua
    /// part in the current physical assembly.
    pub fn project_assembly_cframe_write(
        &self,
        lua_id: u64,
        position: [f32; 3],
        rotation: &[[f32; 3]; 3],
    ) -> Option<HashMap<u64, ([f32; 3], [[f32; 3]; 3])>> {
        let body_handle = *self.lua_to_body.get(&lua_id)?;
        let local = self
            .lua_to_local_frame
            .get(&lua_id)
            .copied()
            .unwrap_or_else(Isometry::identity);
        let target_member_iso = part_isometry(position, rotation);
        let target_body_iso = target_member_iso * local.inverse();
        let lua_ids = self.body_to_luas.get(&body_handle)?;

        let mut out = HashMap::with_capacity(lua_ids.len());
        for part_id in lua_ids {
            let part_local = self
                .lua_to_local_frame
                .get(part_id)
                .copied()
                .unwrap_or_else(Isometry::identity);
            let part_iso = target_body_iso * part_local;
            let pos = part_iso.translation.vector;
            out.insert(
                *part_id,
                (
                    [pos.x, pos.y, pos.z],
                    quaternion_to_rotation_matrix(&part_iso.rotation),
                ),
            );
        }
        Some(out)
    }

    pub fn get_part_position(&self, lua_id: u64) -> Option<[f32; 3]> {
        self.get_part_world_isometry(lua_id).map(|iso| {
            let pos = iso.translation.vector;
            [pos.x, pos.y, pos.z]
        })
    }

    pub fn get_part_rotation_matrix(&self, lua_id: u64) -> Option<[[f32; 3]; 3]> {
        self.get_part_world_isometry(lua_id)
            .map(|iso| quaternion_to_rotation_matrix(&iso.rotation))
    }

    pub fn get_part_velocity(&self, lua_id: u64) -> Option<[f32; 3]> {
        let body_handle = *self.lua_to_body.get(&lua_id)?;
        let body = self.rigid_body_set.get(body_handle)?;
        let part_iso = self.get_part_world_isometry(lua_id)?;
        let point = point![
            part_iso.translation.vector.x,
            part_iso.translation.vector.y,
            part_iso.translation.vector.z
        ];
        let vel = body.velocity_at_point(&point);
        Some([vel.x, vel.y, vel.z])
    }

    /// Gets the velocity of a rigid body
    pub fn get_velocity(&self, handle: RigidBodyHandle) -> Option<[f32; 3]> {
        self.rigid_body_set.get(handle).map(|body| {
            let vel = body.linvel();
            [vel.x, vel.y, vel.z]
        })
    }

    /// Gets the angular velocity of a rigid body
    pub fn get_angular_velocity(&self, handle: RigidBodyHandle) -> Option<[f32; 3]> {
        self.rigid_body_set.get(handle).map(|body| {
            let vel = body.angvel();
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

    pub fn has_exact_part_ids<I>(&self, ids: I, count: usize) -> bool
    where
        I: IntoIterator<Item = u64>,
    {
        self.lua_to_body.len() == count
            && ids.into_iter().all(|id| self.lua_to_body.contains_key(&id))
    }

    pub fn all_assemblies_are_single_part(&self) -> bool {
        self.body_to_luas.values().all(|lua_ids| lua_ids.len() == 1)
    }

    /// Gets the handle for a Lua instance
    pub fn get_handle(&self, lua_id: u64) -> Option<RigidBodyHandle> {
        self.lua_to_body.get(&lua_id).copied()
    }

    pub fn dynamic_body_count(&self) -> usize {
        self.rigid_body_set
            .iter()
            .filter(|(_, body)| body.is_dynamic())
            .count()
    }

    pub fn awake_dynamic_body_count(&self) -> usize {
        self.rigid_body_set
            .iter()
            .filter(|(_, body)| body.is_dynamic() && !body.is_sleeping())
            .count()
    }

    pub fn is_dynamic_body(&self, handle: RigidBodyHandle) -> bool {
        self.rigid_body_set
            .get(handle)
            .map(|body| body.is_dynamic())
            .unwrap_or(false)
    }

    pub fn is_part_sleeping(&self, lua_id: u64) -> bool {
        let Some(&handle) = self.lua_to_body.get(&lua_id) else {
            return false;
        };
        self.rigid_body_set
            .get(handle)
            .map(|body| body.is_dynamic() && body.is_sleeping())
            .unwrap_or(false)
    }

    /// Updates whether a part is anchored or dynamic
    pub fn set_anchored(&mut self, lua_id: u64, anchored: bool) {
        if let Some(&handle) = self.lua_to_body.get(&lua_id) {
            if let Some(body) = self.rigid_body_set.get_mut(handle) {
                if anchored {
                    body.set_body_type(RigidBodyType::KinematicPositionBased, true);
                    self.kinematic_linear_velocities
                        .insert(handle, [0.0, 0.0, 0.0]);
                } else {
                    body.set_body_type(RigidBodyType::Dynamic, true);
                    self.kinematic_linear_velocities.remove(&handle);
                }
            }
        }
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
        let ray = Ray::new(point![from[0], from[1], from[2]], normalized);

        let can_query = self.queryable_collider_predicate();
        let filter = if let Some(body_handle) = exclude_body {
            QueryFilter::default().exclude_rigid_body(body_handle)
        } else {
            QueryFilter::default()
        }
        .predicate(&can_query);

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

        for (&ch, &lua_id) in &self.collider_to_lua {
            let Some(collider) = self.collider_set.get(ch) else {
                continue;
            };
            let parent_body = collider.parent();
            let shape = collider.shape();
            let pos = collider.position();

            let filter = QueryFilter::default().exclude_collider(ch);

            self.query_pipeline.intersections_with_shape(
                &self.rigid_body_set,
                &self.collider_set,
                pos,
                shape,
                filter,
                |other_collider_handle| {
                    let Some(&other_lua_id) = self.collider_to_lua.get(&other_collider_handle)
                    else {
                        return true;
                    };
                    if other_lua_id == lua_id {
                        return true;
                    }
                    if let Some(other_collider) = self.collider_set.get(other_collider_handle) {
                        if other_collider.parent() == parent_body {
                            return true;
                        }
                    }
                    let pair = if lua_id < other_lua_id {
                        (lua_id, other_lua_id)
                    } else {
                        (other_lua_id, lua_id)
                    };
                    overlaps.insert(pair);
                    true
                },
            );
        }

        overlaps
    }

    /// Returns active contact/intersection pairs already computed by Rapier's
    /// narrow phase during the physics step. This is the Roblox-like hot path
    /// for Touched/TouchEnded: consume contact state produced by simulation
    /// instead of polling every collider against the whole world each frame.
    pub fn detect_narrow_phase_touches(&self) -> HashSet<(u64, u64)> {
        let mut touches = HashSet::new();

        let mut insert_pair = |collider_a, collider_b| {
            let Some(&lua_a) = self.collider_to_lua.get(&collider_a) else {
                return;
            };
            let Some(&lua_b) = self.collider_to_lua.get(&collider_b) else {
                return;
            };
            if lua_a == lua_b {
                return;
            }

            if let (Some(a), Some(b)) = (
                self.collider_set.get(collider_a),
                self.collider_set.get(collider_b),
            ) {
                if a.parent().is_some() && a.parent() == b.parent() {
                    return;
                }
            }

            if lua_a < lua_b {
                touches.insert((lua_a, lua_b));
            } else {
                touches.insert((lua_b, lua_a));
            }
        };

        for pair in self.narrow_phase.contact_pairs() {
            if pair.has_any_active_contact {
                insert_pair(pair.collider1, pair.collider2);
            }
        }

        for (collider_a, collider_b, intersecting) in self.narrow_phase.intersection_pairs() {
            if intersecting {
                insert_pair(collider_a, collider_b);
            }
        }

        touches
    }

    /// Casts a ray downward from a position to detect ground
    /// Returns (hit_distance, hit_y) if ground is found within max_distance
    pub fn raycast_down(
        &self,
        origin: [f32; 3],
        max_distance: f32,
        exclude_body: Option<RigidBodyHandle>,
    ) -> Option<(f32, f32)> {
        let ray = Ray::new(
            point![origin[0], origin[1], origin[2]],
            vector![0.0, -1.0, 0.0],
        );

        let can_query = self.queryable_collider_predicate();
        let filter = if let Some(body_handle) = exclude_body {
            QueryFilter::default().exclude_rigid_body(body_handle)
        } else {
            QueryFilter::default()
        }
        .predicate(&can_query);

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
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;

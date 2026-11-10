use std::collections::HashSet;
use std::time::Instant;

use rapier3d::prelude::*;

use super::{
    build_collider_with_physical_properties, finite_vec3, part_isometry, rotation_matrix_is_finite,
    transform_within_physics_envelope, velocity_within_physics_envelope, PartInteractionState,
    PhysicsAssemblyPart, PhysicsAssemblySpec, PhysicsWorld,
};
use crate::game::constants::physics as consts;
use crate::game::profiling::record_values;

fn finite_force(values: [f32; 3]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn part_within_physics_envelope(part: &PhysicsAssemblyPart) -> bool {
    transform_within_physics_envelope(part.position)
        && finite_vec3(part.size)
        && part.size.iter().all(|v| *v >= 0.0)
        && rotation_matrix_is_finite(&part.rotation)
        && velocity_within_physics_envelope(part.velocity)
        && velocity_within_physics_envelope(part.angular_velocity)
}

fn part_has_sync_dirty(part: &PhysicsAssemblyPart) -> bool {
    part.position_dirty
        || part.cframe_dirty
        || part.size_dirty
        || part.anchored_dirty
        || part.interaction_dirty
        || part.velocity_dirty
        || part.angular_velocity_dirty
        || part.shape_dirty
        || part.custom_physical_properties_dirty
}

#[derive(Default)]
struct AssemblySyncStats {
    removed_for_envelope: usize,
    rebuilt: usize,
    updated: usize,
    skipped_unchanged: usize,
    signature_ms: f64,
    rebuild_ms: f64,
    update_ms: f64,
}

fn q5(value: f32) -> i64 {
    let quantized = (value * 100_000.0).round() as i64;
    if quantized == 0 {
        0
    } else {
        quantized
    }
}

fn fmt_vec3(out: &mut String, values: [f32; 3]) {
    out.push_str(&format!(
        "{},{},{}",
        q5(values[0]),
        q5(values[1]),
        q5(values[2])
    ));
}

fn fmt_iso(out: &mut String, iso: &Isometry<Real>) {
    let t = iso.translation.vector;
    let q = iso.rotation;
    out.push_str(&format!(
        "{},{},{}|{},{},{},{}",
        q5(t.x),
        q5(t.y),
        q5(t.z),
        q5(q.i),
        q5(q.j),
        q5(q.k),
        q5(q.w)
    ));
}

fn assembly_signature(
    anchored: bool,
    parts: &[PhysicsAssemblyPart],
    local_frames: &[Isometry<Real>],
) -> String {
    let mut out = format!("anchored={};", anchored);
    for (part, local) in parts.iter().zip(local_frames) {
        out.push_str(&format!("id={};shape={:?};", part.lua_id, part.shape));
        out.push_str("size=");
        fmt_vec3(&mut out, part.size);
        out.push_str(";local=");
        fmt_iso(&mut out, local);
        out.push(';');
        if let Some(props) = part.physical_properties {
            out.push_str(&format!(
                "props={},{},{},{},{};",
                q5(props.density),
                q5(props.friction),
                q5(props.elasticity),
                q5(props.friction_weight),
                q5(props.elasticity_weight)
            ));
        }
    }
    out
}

impl PhysicsWorld {
    fn sync_assembly_spec(
        &mut self,
        spec: PhysicsAssemblySpec,
        dt: f32,
        stats: &mut AssemblySyncStats,
    ) {
        if spec.parts.is_empty() {
            return;
        }

        if !spec.parts.iter().all(part_within_physics_envelope) {
            self.remove_assembly_by_root(spec.root_id);
            stats.removed_for_envelope += 1;
            return;
        }

        let existing_signature = self.assembly_signatures.get(&spec.root_id);
        if existing_signature.is_some()
            && !spec.parts.iter().any(part_has_sync_dirty)
            && spec.parts.iter().all(|part| {
                self.lua_to_body
                    .get(&part.lua_id)
                    .and_then(|body| self.body_to_lua.get(body))
                    == Some(&spec.root_id)
            })
        {
            stats.skipped_unchanged += 1;
            return;
        }

        let root_part = spec
            .parts
            .iter()
            .find(|part| part.lua_id == spec.root_id)
            .unwrap_or(&spec.parts[0]);
        let root_iso = part_isometry(root_part.position, &root_part.rotation);
        let anchored = spec.parts.iter().any(|part| part.anchored);
        let signature_start = Instant::now();
        let local_frames = self.resolve_local_frames(&spec, &root_iso);
        let signature = assembly_signature(anchored, &spec.parts, &local_frames);
        stats.signature_ms += signature_start.elapsed().as_secs_f64() * 1000.0;

        if existing_signature != Some(&signature) {
            let rebuild_start = Instant::now();
            self.rebuild_assembly(&spec, root_iso, anchored, signature, &local_frames);
            stats.rebuild_ms += rebuild_start.elapsed().as_secs_f64() * 1000.0;
            stats.rebuilt += 1;
            return;
        }

        let update_start = Instant::now();
        self.update_existing_assembly(&spec, dt);
        stats.update_ms += update_start.elapsed().as_secs_f64() * 1000.0;
        stats.updated += 1;
    }

    /// True when `spec`'s parts are exactly the parts of one existing body.
    fn component_composition_unchanged(&self, spec: &PhysicsAssemblySpec) -> bool {
        let Some(&body) = self.lua_to_body.get(&spec.root_id) else {
            return false;
        };
        let Some(luas) = self.body_to_luas.get(&body) else {
            return false;
        };
        if luas.len() != spec.parts.len() {
            return false;
        }
        let mut current: Vec<u64> = luas.clone();
        current.sort_unstable();
        spec.parts
            .iter()
            .map(|part| part.lua_id)
            .eq(current.into_iter())
    }

    /// Resolves the weld-local frame of every part in an assembly component.
    ///
    /// Local frames define the assembly's rigid geometry, so they must stay
    /// bit-identical while the assembly merely moves. They are re-derived
    /// from world poses only when the geometry can actually have changed: on
    /// first build, when the component's composition changed (weld
    /// added/removed, assemblies merged or split), or for a part whose
    /// Position was written (Roblox parity: a Position write re-seats that
    /// part within its assembly, while a CFrame write moves the whole
    /// assembly through the existing offsets). Re-deriving them every tick
    /// would bake f32 round-trip noise into the assembly signature and
    /// rebuild the physics body — resetting sleep, contacts, and momentum —
    /// on nearly every tick the assembly is in motion.
    fn resolve_local_frames(
        &self,
        spec: &PhysicsAssemblySpec,
        root_iso: &Isometry<Real>,
    ) -> Vec<Isometry<Real>> {
        let inv_root = root_iso.inverse();
        let composition_unchanged = self.component_composition_unchanged(spec);
        let assembly_cframe_write = spec.parts.iter().any(|part| part.cframe_dirty);
        spec.parts
            .iter()
            .map(|part| {
                let position_write =
                    part.position_dirty && !part.cframe_dirty && !assembly_cframe_write;
                let stored = if composition_unchanged && !position_write {
                    self.lua_to_local_frame.get(&part.lua_id).copied()
                } else {
                    None
                };
                stored.unwrap_or_else(|| inv_root * part_isometry(part.position, &part.rotation))
            })
            .collect()
    }

    pub fn reset_continuous_forces(&mut self) {
        for body in self.rigid_body_set.iter_mut().map(|(_, body)| body) {
            if body.is_dynamic() {
                body.reset_forces(true);
                body.reset_torques(true);
            }
        }
    }

    pub fn add_force_to_part(&mut self, lua_id: u64, force: [f32; 3]) {
        if !finite_force(force) {
            return;
        }
        let Some(&handle) = self.lua_to_body.get(&lua_id) else {
            return;
        };
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.add_force(vector![force[0], force[1], force[2]], true);
            }
        }
    }

    pub fn add_force_at_position_to_part(
        &mut self,
        lua_id: u64,
        force: [f32; 3],
        position: [f32; 3],
    ) {
        if !finite_force(force) || !transform_within_physics_envelope(position) {
            return;
        }
        let Some(&handle) = self.lua_to_body.get(&lua_id) else {
            return;
        };
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.add_force_at_point(
                    vector![force[0], force[1], force[2]],
                    point![position[0], position[1], position[2]],
                    true,
                );
            }
        }
    }

    pub fn add_torque_to_part(&mut self, lua_id: u64, torque: [f32; 3]) {
        if !finite_force(torque) {
            return;
        }
        let Some(&handle) = self.lua_to_body.get(&lua_id) else {
            return;
        };
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.add_torque(vector![torque[0], torque[1], torque[2]], true);
            }
        }
    }

    pub fn set_assembly_angular_velocity_for_part(
        &mut self,
        lua_id: u64,
        angular_velocity: [f32; 3],
    ) {
        let Some(&handle) = self.lua_to_body.get(&lua_id) else {
            return;
        };
        self.set_angular_velocity(handle, angular_velocity);
    }

    pub fn get_assembly_mass_for_part(&self, lua_id: u64) -> Option<f32> {
        let handle = *self.lua_to_body.get(&lua_id)?;
        self.rigid_body_set
            .get(handle)
            .filter(|body| body.is_dynamic())
            .map(|body| body.mass())
    }

    pub fn get_assembly_linear_velocity_for_part(&self, lua_id: u64) -> Option<[f32; 3]> {
        let handle = *self.lua_to_body.get(&lua_id)?;
        self.get_velocity(handle)
    }

    pub fn get_assembly_angular_velocity_for_part(&self, lua_id: u64) -> Option<[f32; 3]> {
        let handle = *self.lua_to_body.get(&lua_id)?;
        self.get_angular_velocity(handle)
    }

    /// Torque that produces the requested world-space angular acceleration on this
    /// part's assembly: `torque = I_world * accel`, where `I_world` is the body's
    /// world-space inertia tensor. Lets an orientation controller be inertia-aware
    /// (analogous to multiplying a linear controller by mass), so heavy/long
    /// objects are actuated correctly instead of swinging.
    pub fn assembly_torque_for_angular_accel(
        &self,
        lua_id: u64,
        accel: [f32; 3],
    ) -> Option<[f32; 3]> {
        let handle = *self.lua_to_body.get(&lua_id)?;
        let body = self
            .rigid_body_set
            .get(handle)
            .filter(|body| body.is_dynamic())?;
        let inertia_local = body
            .mass_properties()
            .local_mprops
            .reconstruct_inertia_matrix();
        let rotation = body.rotation().to_rotation_matrix();
        let r = rotation.matrix();
        let inertia_world = r * inertia_local * r.transpose();
        let torque = inertia_world * vector![accel[0], accel[1], accel[2]];
        Some([torque.x, torque.y, torque.z])
    }

    pub fn get_velocity_at_position_for_part(
        &self,
        lua_id: u64,
        position: [f32; 3],
    ) -> Option<[f32; 3]> {
        if !transform_within_physics_envelope(position) {
            return None;
        }
        let handle = *self.lua_to_body.get(&lua_id)?;
        let body = self.rigid_body_set.get(handle)?;
        let vel = body.velocity_at_point(&point![position[0], position[1], position[2]]);
        Some([vel.x, vel.y, vel.z])
    }

    pub fn apply_impulse_to_part(&mut self, lua_id: u64, impulse: [f32; 3]) {
        if !velocity_within_physics_envelope(impulse) {
            return;
        }
        let Some(&handle) = self.lua_to_body.get(&lua_id) else {
            return;
        };
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.apply_impulse(vector![impulse[0], impulse[1], impulse[2]], true);
            }
        }
    }

    pub fn apply_impulse_at_position_to_part(
        &mut self,
        lua_id: u64,
        impulse: [f32; 3],
        position: [f32; 3],
    ) {
        if !velocity_within_physics_envelope(impulse)
            || !transform_within_physics_envelope(position)
        {
            return;
        }
        let Some(&handle) = self.lua_to_body.get(&lua_id) else {
            return;
        };
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.apply_impulse_at_point(
                    vector![impulse[0], impulse[1], impulse[2]],
                    point![position[0], position[1], position[2]],
                    true,
                );
            }
        }
    }

    pub fn apply_angular_impulse_to_part(&mut self, lua_id: u64, impulse: [f32; 3]) {
        if !velocity_within_physics_envelope(impulse) {
            return;
        }
        let Some(&handle) = self.lua_to_body.get(&lua_id) else {
            return;
        };
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            if body.is_dynamic() {
                body.apply_torque_impulse(vector![impulse[0], impulse[1], impulse[2]], true);
            }
        }
    }

    /// Syncs Roblox-style rigid assemblies into Rapier.
    ///
    /// Rigid weld graphs are represented as one Rapier body with one collider per
    /// Lua part. This matches Roblox's "assembly is one rigid body" physics
    /// model and avoids per-frame internal fixed-joint solves, wakeups, and sleep
    /// timer resets. The body is preserved while the assembly signature is stable.
    pub fn sync_assemblies(&mut self, specs: Vec<PhysicsAssemblySpec>, dt: f32) {
        let spec_count = specs.len();
        let spec_part_count = specs.iter().map(|spec| spec.parts.len()).sum::<usize>();
        let mut active_lua_ids = HashSet::new();
        let mut active_roots = HashSet::new();
        let mut stats = AssemblySyncStats::default();

        for spec in specs {
            if spec.parts.is_empty() {
                continue;
            }

            active_roots.insert(spec.root_id);
            for part in &spec.parts {
                active_lua_ids.insert(part.lua_id);
            }

            self.sync_assembly_spec(spec, dt, &mut stats);
        }

        let orphaned_roots: Vec<u64> = self
            .assembly_signatures
            .keys()
            .copied()
            .filter(|root_id| !active_roots.contains(root_id))
            .collect();
        let orphaned_root_count = orphaned_roots.len();
        for root_id in orphaned_roots {
            self.remove_assembly_by_root(root_id);
        }

        let orphaned_parts: Vec<u64> = self
            .lua_to_body
            .keys()
            .copied()
            .filter(|lua_id| {
                !active_lua_ids.contains(lua_id) && !self.character_controllers.contains_key(lua_id)
            })
            .collect();
        let orphaned_part_count = orphaned_parts.len();
        for lua_id in orphaned_parts {
            self.remove_part(lua_id);
        }

        record_values(
            "physics.sync_assemblies",
            &[
                ("specs", spec_count as f64),
                ("spec_parts", spec_part_count as f64),
                ("partial_sync", 0.0),
                ("removed_for_envelope", stats.removed_for_envelope as f64),
                ("rebuilt", stats.rebuilt as f64),
                ("updated", stats.updated as f64),
                ("skipped_unchanged", stats.skipped_unchanged as f64),
                ("orphaned_roots", orphaned_root_count as f64),
                ("orphaned_parts", orphaned_part_count as f64),
                ("signature_ms", stats.signature_ms),
                ("rebuild_ms", stats.rebuild_ms),
                ("update_ms", stats.update_ms),
            ],
        );
    }

    /// Updates selected single-part assemblies without treating omitted
    /// assemblies as deleted. Callers must only use this after proving the
    /// Workspace part set and the physics part set already match, there are no
    /// weld edges, and every assembly is represented by exactly one part.
    pub fn sync_single_part_assemblies_partial(
        &mut self,
        specs: Vec<PhysicsAssemblySpec>,
        dt: f32,
    ) {
        let spec_count = specs.len();
        let spec_part_count = specs.iter().map(|spec| spec.parts.len()).sum::<usize>();
        let mut stats = AssemblySyncStats::default();

        for spec in specs {
            if spec.parts.len() != 1 {
                continue;
            }
            self.sync_assembly_spec(spec, dt, &mut stats);
        }

        record_values(
            "physics.sync_assemblies",
            &[
                ("specs", spec_count as f64),
                ("spec_parts", spec_part_count as f64),
                ("partial_sync", 1.0),
                ("removed_for_envelope", stats.removed_for_envelope as f64),
                ("rebuilt", stats.rebuilt as f64),
                ("updated", stats.updated as f64),
                ("skipped_unchanged", stats.skipped_unchanged as f64),
                ("orphaned_roots", 0.0),
                ("orphaned_parts", 0.0),
                ("signature_ms", stats.signature_ms),
                ("rebuild_ms", stats.rebuild_ms),
                ("update_ms", stats.update_ms),
            ],
        );
    }

    fn rebuild_assembly(
        &mut self,
        spec: &PhysicsAssemblySpec,
        root_iso: Isometry<Real>,
        anchored: bool,
        signature: String,
        local_frames: &[Isometry<Real>],
    ) {
        // Capture the root part's old body motion before tearing the bodies
        // down, so the assembly keeps its momentum across the rebuild. A
        // kinematic (anchored) body's motion is dropped, except in mesa-island
        // compatibility mode, where a released part keeps it.
        let legacy_unanchor_momentum = self.legacy_unanchor_momentum;
        let carry_over = self
            .lua_to_body
            .get(&spec.root_id)
            .or_else(|| {
                spec.parts
                    .iter()
                    .find_map(|part| self.lua_to_body.get(&part.lua_id))
            })
            .and_then(|&handle| self.rigid_body_set.get(handle))
            .filter(|body| body.is_dynamic() || (legacy_unanchor_momentum && body.is_kinematic()))
            .map(|body| {
                // Rapier only refreshes world_com for simulated bodies; a
                // kinematic body's is stale (where it last was dynamic or
                // created), so derive it from the current pose.
                let com = if body.is_kinematic() {
                    body.position() * body.local_center_of_mass()
                } else {
                    *body.center_of_mass()
                };
                (*body.linvel(), *body.angvel(), com)
            });
        let mut old_roots: Vec<u64> = spec
            .parts
            .iter()
            .filter_map(|part| {
                let body = self.lua_to_body.get(&part.lua_id)?;
                self.body_to_lua.get(body).copied()
            })
            .collect();
        old_roots.sort_unstable();
        old_roots.dedup();

        // A weld can merge multiple previous assemblies, and the Roblox assembly
        // root can change when a lower-id part joins. Remove every old body
        // before inserting the replacement body; otherwise later orphan-root
        // cleanup can follow an old root id to the new body and delete it.
        for old_root in old_roots {
            self.remove_assembly_by_root(old_root);
        }

        let body = if anchored {
            RigidBodyBuilder::kinematic_position_based()
        } else {
            RigidBodyBuilder::dynamic()
        }
        .position(root_iso)
        .build();
        let body_handle = self.rigid_body_set.insert(body);

        let mut lua_ids = Vec::with_capacity(spec.parts.len());

        for (part, &local_frame) in spec.parts.iter().zip(local_frames) {
            let interaction_state = PartInteractionState {
                can_collide: part.can_collide,
                can_touch: part.can_touch,
                can_query: part.can_query,
                collision_group: part.collision_group,
            };
            let mut collider = build_collider_with_physical_properties(
                part.size,
                part.shape,
                interaction_state,
                part.physical_properties,
            );
            collider.set_position(local_frame);
            let collider_handle = self.collider_set.insert_with_parent(
                collider,
                body_handle,
                &mut self.rigid_body_set,
            );

            self.lua_to_body.insert(part.lua_id, body_handle);
            self.lua_to_collider.insert(part.lua_id, collider_handle);
            self.lua_to_local_frame.insert(part.lua_id, local_frame);
            self.lua_to_shape.insert(part.lua_id, part.shape);
            self.lua_to_physical_properties
                .insert(part.lua_id, part.physical_properties);
            self.lua_to_interactions
                .insert(part.lua_id, interaction_state);
            self.collider_to_lua.insert(collider_handle, part.lua_id);
            lua_ids.push(part.lua_id);
        }

        self.body_to_lua.insert(body_handle, spec.root_id);
        self.body_to_luas.insert(body_handle, lua_ids);
        self.assembly_signatures.insert(spec.root_id, signature);

        if !anchored {
            // Default: preserve the old body's momentum through the rebuild.
            // Angular velocity transfers directly; linear velocity is the old
            // body's velocity evaluated at the new body's center of mass.
            // Seeding from a part's point velocity instead would silently add
            // w x r of COM speed on every rebuild of a rotating assembly.
            let mut linvel = [0.0, 0.0, 0.0];
            let mut angvel = [0.0, 0.0, 0.0];
            if let Some((old_linvel, old_angvel, old_com)) = carry_over {
                if let Some(body) = self.rigid_body_set.get(body_handle) {
                    let v = old_linvel + old_angvel.cross(&(body.center_of_mass() - old_com));
                    linvel = [v.x, v.y, v.z];
                    angvel = [old_angvel.x, old_angvel.y, old_angvel.z];
                }
            }
            // Explicit Lua velocity writes still win.
            if let Some(part) = spec.parts.iter().find(|part| part.velocity_dirty) {
                linvel = part.velocity;
            }
            if let Some(part) = spec.parts.iter().find(|part| part.angular_velocity_dirty) {
                angvel = part.angular_velocity;
            }
            self.set_velocity(body_handle, linvel);
            self.set_angular_velocity(body_handle, angvel);
        } else {
            self.kinematic_linear_velocities
                .insert(body_handle, [0.0, 0.0, 0.0]);
        }
    }

    fn update_existing_assembly(&mut self, spec: &PhysicsAssemblySpec, dt: f32) {
        let Some(&body_handle) = self.lua_to_body.get(&spec.root_id) else {
            return;
        };

        for part in &spec.parts {
            let interaction_state = PartInteractionState {
                can_collide: part.can_collide,
                can_touch: part.can_touch,
                can_query: part.can_query,
                collision_group: part.collision_group,
            };
            if self.lua_to_interactions.get(&part.lua_id).copied() != Some(interaction_state) {
                self.set_interaction_state(part.lua_id, interaction_state);
            }

            if part.position_dirty {
                if let Some(local_frame) = self.lua_to_local_frame.get(&part.lua_id).copied() {
                    let target_body_iso =
                        part_isometry(part.position, &part.rotation) * local_frame.inverse();
                    if let Some(body) = self.rigid_body_set.get(body_handle) {
                        if body.is_kinematic() {
                            let cur = body.translation();
                            let target = target_body_iso.translation.vector;
                            let inv_dt = if dt > consts::EPSILON { 1.0 / dt } else { 0.0 };
                            self.kinematic_linear_velocities.insert(
                                body_handle,
                                [
                                    (target.x - cur.x) * inv_dt,
                                    (target.y - cur.y) * inv_dt,
                                    (target.z - cur.z) * inv_dt,
                                ],
                            );
                        }
                    }
                    if let Some(body) = self.rigid_body_set.get_mut(body_handle) {
                        if body.is_kinematic() {
                            body.set_next_kinematic_position(target_body_iso);
                        } else if body.is_dynamic() {
                            body.set_position(target_body_iso, true);
                        }
                    }
                }
            }
            if part.velocity_dirty {
                self.set_velocity(body_handle, part.velocity);
            }
            if part.angular_velocity_dirty {
                self.set_angular_velocity(body_handle, part.angular_velocity);
            }
        }
    }

    pub(super) fn remove_assembly_by_root(&mut self, root_id: u64) {
        let Some(body_handle) = self.lua_to_body.get(&root_id).copied() else {
            self.assembly_signatures.remove(&root_id);
            return;
        };

        self.remove_body_mappings(body_handle);
        self.queue_body_removal(body_handle);
    }
}

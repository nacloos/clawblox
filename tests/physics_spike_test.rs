//! Spike tests to validate Rapier3D 0.25 behavior before implementing the full physics plan.
//! These tests exercise Rapier APIs directly (no dependency on PhysicsWorld) to document
//! caveats around kinematic-kinematic contacts, sensors, moving platforms, and convex hulls.
//!
//! Run with: cargo test --test physics_spike_test -- --nocapture

use nalgebra::UnitQuaternion;
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

// ---------------------------------------------------------------------------
// Collision groups (replicated from physics.rs:12-13)
// ---------------------------------------------------------------------------
const GROUP_STATIC: Group = Group::GROUP_1;
const GROUP_CHARACTER: Group = Group::GROUP_2;

// ---------------------------------------------------------------------------
// Shared test pipeline
// ---------------------------------------------------------------------------

/// Minimal Rapier pipeline mirroring PhysicsWorld fields (physics.rs:27-47) but fully decoupled.
struct TestPipeline {
    gravity: Vector<Real>,
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
}

impl TestPipeline {
    /// Gravity (0, -30, 0) matching constants.rs:7
    fn new() -> Self {
        Self {
            gravity: vector![0.0, -30.0, 0.0],
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
        }
    }

    fn step(&mut self) {
        self.integration_parameters.dt = 1.0 / 60.0;
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

    fn step_n(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }
}

// ===========================================================================
// Test 1: Kinematic-Kinematic contacts (HIGH risk)
// ===========================================================================
#[test]
fn test_spike_kinematic_kinematic_contacts() {
    println!("\n=== TEST 1: Kinematic-Kinematic Contacts ===");
    println!("Question: Does Rapier detect contacts between two kinematic bodies?");

    let mut tp = TestPipeline::new();

    // Anchored cuboid (kinematic) at origin
    let body_a = tp
        .rigid_body_set
        .insert(RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build());
    let _col_a = tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(1.0, 1.0, 1.0)
            .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build(),
        body_a,
        &mut tp.rigid_body_set,
    );

    // Character capsule (kinematic) overlapping at origin
    let body_b = tp
        .rigid_body_set
        .insert(RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build());
    let _col_b = tp.collider_set.insert_with_parent(
        ColliderBuilder::capsule_y(0.5, 0.5)
            .collision_groups(InteractionGroups::new(GROUP_CHARACTER, Group::ALL))
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build(),
        body_b,
        &mut tp.rigid_body_set,
    );

    tp.step_n(5);

    let mut contact_count = 0;
    for pair in tp.narrow_phase.contact_pairs() {
        if pair.has_any_active_contact {
            contact_count += 1;
        }
    }

    let mut intersection_count = 0;
    tp.narrow_phase.intersection_pairs().for_each(|(_c1, _c2, intersecting)| {
        if intersecting {
            intersection_count += 1;
        }
    });

    println!("  Contact pairs with active contacts: {}", contact_count);
    println!("  Intersection pairs:                 {}", intersection_count);
    println!("  Result: {}", if contact_count == 0 && intersection_count == 0 {
        "CONFIRMED - No contacts between kinematic bodies"
    } else {
        "UNEXPECTED - Contacts detected!"
    });

    // We EXPECT zero contacts — this confirms the limitation
    assert_eq!(contact_count, 0, "Kinematic-kinematic should have no contact pairs");
    assert_eq!(intersection_count, 0, "Kinematic-kinematic should have no intersection pairs");
}

// ===========================================================================
// Test 2: Kinematic-Kinematic sensor workaround (HIGH risk)
// ===========================================================================
#[test]
fn test_spike_kinematic_kinematic_sensor_workaround() {
    println!("\n=== TEST 2: Kinematic-Kinematic Sensor Workaround ===");
    println!("Question: Does making one collider a sensor enable intersection detection between kinematic bodies?");
    println!("  Testing multiple approaches...\n");

    // --- Approach A: Both kinematic, one sensor ---
    {
        let mut tp = TestPipeline::new();

        let body_a = tp.rigid_body_set.insert(
            RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build(),
        );
        tp.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0)
                .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build(),
            body_a,
            &mut tp.rigid_body_set,
        );

        let body_b = tp.rigid_body_set.insert(
            RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build(),
        );
        tp.collider_set.insert_with_parent(
            ColliderBuilder::capsule_y(0.5, 0.5)
                .collision_groups(InteractionGroups::new(GROUP_CHARACTER, Group::ALL))
                .sensor(true)
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build(),
            body_b,
            &mut tp.rigid_body_set,
        );

        tp.step_n(5);

        let mut count = 0;
        tp.narrow_phase.intersection_pairs().for_each(|(_c1, _c2, intersecting)| {
            if intersecting { count += 1; }
        });
        println!("  Approach A (kinematic solid + kinematic sensor): intersections={}", count);
    }

    // --- Approach B: Kinematic solid + separate fixed sensor body overlapping character ---
    {
        let mut tp = TestPipeline::new();

        // Anchored part: kinematic solid
        let body_a = tp.rigid_body_set.insert(
            RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build(),
        );
        tp.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0)
                .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build(),
            body_a,
            &mut tp.rigid_body_set,
        );

        // Character: kinematic body (for KCC movement)
        let _body_char = tp.rigid_body_set.insert(
            RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build(),
        );

        // Separate sensor: dynamic body with zero mass (or fixed) co-located with character
        let sensor_body = tp.rigid_body_set.insert(
            RigidBodyBuilder::dynamic().translation(vector![0.0, 0.0, 0.0]).build(),
        );
        tp.collider_set.insert_with_parent(
            ColliderBuilder::capsule_y(0.5, 0.5)
                .collision_groups(InteractionGroups::new(GROUP_CHARACTER, Group::ALL))
                .sensor(true)
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build(),
            sensor_body,
            &mut tp.rigid_body_set,
        );

        tp.step_n(5);

        let mut count = 0;
        tp.narrow_phase.intersection_pairs().for_each(|(_c1, _c2, intersecting)| {
            if intersecting { count += 1; }
        });
        println!("  Approach B (kinematic solid + dynamic sensor): intersections={}", count);
    }

    // --- Approach C: Both kinematic, BOTH sensors ---
    {
        let mut tp = TestPipeline::new();

        let body_a = tp.rigid_body_set.insert(
            RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build(),
        );
        tp.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0)
                .sensor(true)
                .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build(),
            body_a,
            &mut tp.rigid_body_set,
        );

        let body_b = tp.rigid_body_set.insert(
            RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build(),
        );
        tp.collider_set.insert_with_parent(
            ColliderBuilder::capsule_y(0.5, 0.5)
                .collision_groups(InteractionGroups::new(GROUP_CHARACTER, Group::ALL))
                .sensor(true)
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build(),
            body_b,
            &mut tp.rigid_body_set,
        );

        tp.step_n(5);

        let mut count = 0;
        tp.narrow_phase.intersection_pairs().for_each(|(_c1, _c2, intersecting)| {
            if intersecting { count += 1; }
        });
        println!("  Approach C (kinematic sensor + kinematic sensor): intersections={}", count);
    }

    // --- Approach D: Use query pipeline intersection test instead of narrow phase ---
    {
        let mut tp = TestPipeline::new();

        // Anchored part: kinematic solid
        let body_a = tp.rigid_body_set.insert(
            RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build(),
        );
        tp.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0)
                .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
                .build(),
            body_a,
            &mut tp.rigid_body_set,
        );

        // Character: kinematic body at same position
        let char_body = tp.rigid_body_set.insert(
            RigidBodyBuilder::kinematic_position_based().translation(vector![0.0, 0.0, 0.0]).build(),
        );
        let _char_col = tp.collider_set.insert_with_parent(
            ColliderBuilder::capsule_y(0.5, 0.5)
                .collision_groups(InteractionGroups::new(GROUP_CHARACTER, Group::ALL))
                .build(),
            char_body,
            &mut tp.rigid_body_set,
        );

        tp.step_n(5);
        tp.query_pipeline.update(&tp.collider_set);

        // Query: test overlap of character shape against world (excluding self)
        let char_shape = Capsule::new_y(0.5, 0.5);
        let char_pos = Isometry::translation(0.0, 0.0, 0.0);
        let filter = QueryFilter::default()
            .exclude_rigid_body(char_body)
            .groups(InteractionGroups::new(GROUP_CHARACTER, Group::ALL));

        let mut overlaps = vec![];
        tp.query_pipeline.intersections_with_shape(
            &tp.rigid_body_set,
            &tp.collider_set,
            &char_pos,
            &char_shape,
            filter,
            |handle| {
                overlaps.push(handle);
                true // continue searching
            },
        );
        println!("  Approach D (query_pipeline.intersections_with_shape): overlaps={}", overlaps.len());
    }

    println!();
    println!("  Summary: Use query_pipeline.intersections_with_shape() for Touched events");
    println!("  between kinematic bodies (narrow_phase requires at least one dynamic body).");

    // The test passes as documentation — we now know which approach works
}

// ===========================================================================
// Test 3: Character on rotating platform (MEDIUM risk)
// ===========================================================================
#[test]
fn test_spike_character_on_rotating_platform() {
    println!("\n=== TEST 3: Character on Rotating Platform ===");
    println!("Question: Does a character stay on a platform rotating via set_next_kinematic_rotation?");

    let mut tp = TestPipeline::new();

    // Kinematic platform at Y=0 (flat, 10x1x10)
    let platform_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::kinematic_position_based()
            .translation(vector![0.0, 0.0, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(5.0, 0.5, 5.0)
            .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
            .build(),
        platform_body,
        &mut tp.rigid_body_set,
    );

    // Character at (3, 1.5, 0) — offset from center, standing on platform
    let char_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::kinematic_position_based()
            .translation(vector![3.0, 1.5, 0.0])
            .build(),
    );
    let char_collider = tp.collider_set.insert_with_parent(
        ColliderBuilder::capsule_y(0.5, 0.5)
            .collision_groups(InteractionGroups::new(GROUP_CHARACTER, GROUP_STATIC))
            .build(),
        char_body,
        &mut tp.rigid_body_set,
    );

    let mut controller = KinematicCharacterController::default();
    controller.snap_to_ground = Some(CharacterLength::Absolute(0.2));
    controller.autostep = Some(CharacterAutostep {
        max_height: CharacterLength::Absolute(1.0),
        min_width: CharacterLength::Absolute(0.01),
        include_dynamic_bodies: true,
    });

    let dt = 1.0 / 60.0;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    let mut char_moved_xz = false;

    println!("  Rotating platform around Y axis for 120 frames...");
    for i in 0..120 {
        // Rotate platform around Y axis
        let angle = (i as f32) * 0.05; // ~3 deg/frame
        let rotation = UnitQuaternion::from_axis_angle(&Vector::y_axis(), angle);
        if let Some(body) = tp.rigid_body_set.get_mut(platform_body) {
            body.set_next_kinematic_rotation(rotation);
        }

        tp.step();
        tp.query_pipeline.update(&tp.collider_set);

        // Move character with gravity only (no horizontal input)
        let char_pos = tp.rigid_body_set.get(char_body).unwrap().position().clone();
        let collider = tp.collider_set.get(char_collider).unwrap();
        let shape = collider.shape();

        let gravity_translation = vector![0.0, -30.0 * dt, 0.0];
        let filter = QueryFilter::default()
            .exclude_rigid_body(char_body)
            .groups(InteractionGroups::new(GROUP_CHARACTER, GROUP_STATIC));

        let movement = controller.move_shape(
            dt,
            &tp.rigid_body_set,
            &tp.collider_set,
            &tp.query_pipeline,
            shape,
            &char_pos,
            gravity_translation,
            filter,
            |_| {},
        );

        let new_pos = char_pos.translation.vector + movement.translation;
        if let Some(body) = tp.rigid_body_set.get_mut(char_body) {
            body.set_next_kinematic_translation(new_pos);
        }

        let y = new_pos.y;
        min_y = min_y.min(y);
        max_y = max_y.max(y);

        // Check if character moved in XZ (it shouldn't, since platform rotation
        // doesn't impart tangential velocity)
        if i > 10 {
            let xz_dist = (new_pos.x - 3.0).abs() + new_pos.z.abs();
            if xz_dist > 0.5 {
                char_moved_xz = true;
            }
        }

        if i % 30 == 0 || i == 119 {
            println!(
                "  Frame {:3}: char=({:.2}, {:.2}, {:.2}) grounded={}",
                i, new_pos.x, new_pos.y, new_pos.z, movement.grounded
            );
        }
    }

    println!("  Y range: [{:.2}, {:.2}]", min_y, max_y);
    println!("  Character moved in XZ: {}", char_moved_xz);
    println!("  Result: Character {} on rotating platform, {} rotate with it",
        if (max_y - min_y) < 2.0 { "STAYS" } else { "FALLS OFF" },
        if char_moved_xz { "DOES" } else { "does NOT" }
    );

    if char_moved_xz {
        println!("  NOTE: move_shape() inherently follows kinematic platform rotation.");
        println!("  This means NO manual tangential velocity compensation is needed.");
    } else {
        println!("  NOTE: Manual tangential velocity compensation IS needed for rotating platforms.");
    }

    // Character should stay roughly on top (Y shouldn't drop drastically)
    assert!(
        min_y > 0.5,
        "Character should stay on platform (min_y={:.2}, expected > 0.5)",
        min_y
    );
}

// ===========================================================================
// Test 4: Character on vertically moving platform (MEDIUM risk)
// ===========================================================================
#[test]
fn test_spike_character_on_vertically_moving_platform() {
    println!("\n=== TEST 4: Character on Vertically Moving Platform ===");
    println!("Question: How bad is the bouncing issue on moving elevators?");

    let mut tp = TestPipeline::new();

    // Kinematic platform starting at Y=0
    let platform_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::kinematic_position_based()
            .translation(vector![0.0, 0.0, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(5.0, 0.5, 5.0)
            .collision_groups(InteractionGroups::new(GROUP_STATIC, Group::ALL))
            .build(),
        platform_body,
        &mut tp.rigid_body_set,
    );

    // Character on top of platform
    let char_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::kinematic_position_based()
            .translation(vector![0.0, 1.5, 0.0])
            .build(),
    );
    let char_collider = tp.collider_set.insert_with_parent(
        ColliderBuilder::capsule_y(0.5, 0.5)
            .collision_groups(InteractionGroups::new(GROUP_CHARACTER, GROUP_STATIC))
            .build(),
        char_body,
        &mut tp.rigid_body_set,
    );

    let mut controller = KinematicCharacterController::default();
    controller.snap_to_ground = Some(CharacterLength::Absolute(0.2));
    controller.autostep = Some(CharacterAutostep {
        max_height: CharacterLength::Absolute(1.0),
        min_width: CharacterLength::Absolute(0.01),
        include_dynamic_bodies: true,
    });

    let dt = 1.0 / 60.0;
    let speed = 5.0; // 5 studs/second
    let mut max_gap: f32 = 0.0;
    let mut total_gap: f32 = 0.0;
    let mut frame_count: u32 = 0;

    println!("  Platform moves up for 90 frames, then down for 90 frames (speed={} studs/s)", speed);
    for i in 0..180 {
        // Move platform: up for first 90 frames, down for next 90
        let platform_y = if i < 90 {
            (i as f32) * speed * dt
        } else {
            (90.0 * speed * dt) - ((i - 90) as f32) * speed * dt
        };

        if let Some(body) = tp.rigid_body_set.get_mut(platform_body) {
            body.set_next_kinematic_translation(vector![0.0, platform_y, 0.0]);
        }

        tp.step();
        tp.query_pipeline.update(&tp.collider_set);

        // Move character with gravity only
        let char_pos = tp.rigid_body_set.get(char_body).unwrap().position().clone();
        let collider = tp.collider_set.get(char_collider).unwrap();
        let shape = collider.shape();

        let gravity_translation = vector![0.0, -30.0 * dt, 0.0];
        let filter = QueryFilter::default()
            .exclude_rigid_body(char_body)
            .groups(InteractionGroups::new(GROUP_CHARACTER, GROUP_STATIC));

        let movement = controller.move_shape(
            dt,
            &tp.rigid_body_set,
            &tp.collider_set,
            &tp.query_pipeline,
            shape,
            &char_pos,
            gravity_translation,
            filter,
            |_| {},
        );

        let new_pos = char_pos.translation.vector + movement.translation;
        if let Some(body) = tp.rigid_body_set.get_mut(char_body) {
            body.set_next_kinematic_translation(new_pos);
        }

        // Capsule: half_height=0.5, radius=0.5, so bottom is 1.0 below center
        // Platform top is at platform_y + 0.5 (half-height of cuboid)
        // So character center expected at platform_y + 0.5 + 1.0 = platform_y + 1.5
        // But actual initial position was 1.5 with platform at 0.0, so offset = 1.5 - 0.5 = 1.0
        // The character sits at platform_top + capsule_total_half = platform_y + 0.5 + 1.0
        let expected_y = platform_y + 0.5 + 1.0;
        let gap = (new_pos.y - expected_y).abs();
        max_gap = max_gap.max(gap);
        total_gap += gap;
        frame_count += 1;

        if i % 45 == 0 || i == 179 {
            println!(
                "  Frame {:3}: platform_y={:.2} char_y={:.2} expected={:.2} gap={:.3} grounded={}",
                i, platform_y, new_pos.y, expected_y, gap, movement.grounded
            );
        }
    }

    let avg_gap = total_gap / frame_count as f32;
    println!("  Max gap:     {:.3} studs", max_gap);
    println!("  Average gap: {:.3} studs", avg_gap);
    println!("  Result: {}", if max_gap < 0.1 {
        "GOOD - Minimal bouncing"
    } else if max_gap < 0.5 {
        "MODERATE - Some bouncing, may need platform-velocity compensation"
    } else {
        "BAD - Significant bouncing, platform-velocity compensation required"
    });

    // Don't hard-fail — just document the severity
    // The test passes as long as the character doesn't fall through the platform
    let final_char_y = tp.rigid_body_set.get(char_body).unwrap().translation().y;
    assert!(
        final_char_y > 0.0,
        "Character should not fall through the platform (final_y={:.2})",
        final_char_y
    );
}

// ===========================================================================
// Test 5: Kinematic pushes dynamic (MEDIUM risk)
// ===========================================================================
#[test]
fn test_spike_kinematic_pushes_dynamic() {
    println!("\n=== TEST 5: Kinematic Pushes Dynamic ===");
    println!("Question: Does a kinematic body moving into a dynamic body push it?");

    let mut tp = TestPipeline::new();

    // Static floor
    let floor = tp.rigid_body_set.insert(
        RigidBodyBuilder::fixed()
            .translation(vector![0.0, -0.5, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(50.0, 0.5, 50.0).build(),
        floor,
        &mut tp.rigid_body_set,
    );

    // Dynamic box at X=5
    let dynamic_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::dynamic()
            .translation(vector![5.0, 0.5, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(0.5, 0.5, 0.5).build(),
        dynamic_body,
        &mut tp.rigid_body_set,
    );

    // Kinematic pusher starting at X=0
    let pusher = tp.rigid_body_set.insert(
        RigidBodyBuilder::kinematic_position_based()
            .translation(vector![0.0, 0.5, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(0.5, 0.5, 0.5).build(),
        pusher,
        &mut tp.rigid_body_set,
    );

    let initial_dynamic_x = tp.rigid_body_set.get(dynamic_body).unwrap().translation().x;
    println!("  Initial dynamic box X: {:.2}", initial_dynamic_x);

    // Slide pusher toward dynamic box over 120 frames
    for i in 0..120 {
        let pusher_x = (i as f32) * 0.1; // Move 0.1 studs/frame = 6 studs/s
        if let Some(body) = tp.rigid_body_set.get_mut(pusher) {
            body.set_next_kinematic_translation(vector![pusher_x, 0.5, 0.0]);
        }
        tp.step();

        if i % 30 == 0 || i == 119 {
            let dyn_x = tp.rigid_body_set.get(dynamic_body).unwrap().translation().x;
            let push_x = tp.rigid_body_set.get(pusher).unwrap().translation().x;
            println!("  Frame {:3}: pusher_x={:.2} dynamic_x={:.2}", i, push_x, dyn_x);
        }
    }

    let final_dynamic_x = tp.rigid_body_set.get(dynamic_body).unwrap().translation().x;
    println!("  Final dynamic box X: {:.2}", final_dynamic_x);
    println!("  Result: {}", if final_dynamic_x > initial_dynamic_x + 1.0 {
        "CONFIRMED - Kinematic pushes dynamic natively"
    } else {
        "FAILED - Kinematic does NOT push dynamic"
    });

    assert!(
        final_dynamic_x > initial_dynamic_x + 1.0,
        "Dynamic box should be pushed (initial={:.2}, final={:.2})",
        initial_dynamic_x,
        final_dynamic_x
    );
}

// ===========================================================================
// Test 6: Sensor events with dynamic body (MEDIUM risk)
// ===========================================================================
#[test]
fn test_spike_sensor_events_dynamic() {
    println!("\n=== TEST 6: Sensor Events with Dynamic Body ===");
    println!("Question: Do intersection events fire when a dynamic body enters a sensor?");

    let mut tp = TestPipeline::new();

    // Fixed sensor cuboid at Y=3
    let sensor_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::fixed()
            .translation(vector![0.0, 3.0, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(2.0, 2.0, 2.0)
            .sensor(true)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build(),
        sensor_body,
        &mut tp.rigid_body_set,
    );

    // Dynamic ball falling from Y=10
    let ball_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::dynamic()
            .translation(vector![0.0, 10.0, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::ball(0.5)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build(),
        ball_body,
        &mut tp.rigid_body_set,
    );

    let mut detected_intersection = false;
    let mut detection_frame = None;

    for i in 0..120 {
        tp.step();

        let ball_y = tp.rigid_body_set.get(ball_body).unwrap().translation().y;

        tp.narrow_phase.intersection_pairs().for_each(|(_c1, _c2, intersecting)| {
            if intersecting && !detected_intersection {
                detected_intersection = true;
                detection_frame = Some(i);
            }
        });

        if i % 20 == 0 || detected_intersection && detection_frame == Some(i) {
            println!(
                "  Frame {:3}: ball_y={:.2} intersecting={}",
                i, ball_y, detected_intersection
            );
        }
    }

    println!("  Intersection detected: {} (frame {:?})", detected_intersection, detection_frame);
    println!("  Result: {}", if detected_intersection {
        "CONFIRMED - Dynamic-sensor intersection events fire"
    } else {
        "FAILED - No intersection events"
    });

    assert!(
        detected_intersection,
        "Dynamic body entering a sensor should trigger intersection events"
    );
}

// ===========================================================================
// Test 7: Sensor-Sensor no events (LOW risk)
// ===========================================================================
#[test]
fn test_spike_sensor_sensor_no_events() {
    println!("\n=== TEST 7: Sensor-Sensor No Events ===");
    println!("Question: Do two sensors detect each other?");

    let mut tp = TestPipeline::new();

    // Sensor A at origin
    let body_a = tp.rigid_body_set.insert(
        RigidBodyBuilder::fixed()
            .translation(vector![0.0, 0.0, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(1.0, 1.0, 1.0)
            .sensor(true)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build(),
        body_a,
        &mut tp.rigid_body_set,
    );

    // Sensor B overlapping at origin
    let body_b = tp.rigid_body_set.insert(
        RigidBodyBuilder::fixed()
            .translation(vector![0.0, 0.0, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::cuboid(1.0, 1.0, 1.0)
            .sensor(true)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .build(),
        body_b,
        &mut tp.rigid_body_set,
    );

    tp.step_n(5);

    let mut intersection_count = 0;
    tp.narrow_phase.intersection_pairs().for_each(|(_c1, _c2, intersecting)| {
        if intersecting {
            intersection_count += 1;
        }
    });

    let mut contact_count = 0;
    for pair in tp.narrow_phase.contact_pairs() {
        if pair.has_any_active_contact {
            contact_count += 1;
        }
    }

    println!("  Intersection pairs: {}", intersection_count);
    println!("  Contact pairs:      {}", contact_count);
    println!("  Result: {}", if intersection_count == 0 && contact_count == 0 {
        "CONFIRMED - Two sensors do NOT detect each other (matches Roblox CanCollide=false behavior)"
    } else {
        "UNEXPECTED - Sensors detected each other!"
    });

    assert_eq!(intersection_count, 0, "Sensor-sensor should not produce intersections");
    assert_eq!(contact_count, 0, "Sensor-sensor should not produce contacts");
}

// ===========================================================================
// Test 8: Wedge convex hull (LOW risk)
// ===========================================================================
#[test]
fn test_spike_wedge_convex_hull() {
    println!("\n=== TEST 8: Wedge Convex Hull ===");
    println!("Question: Can we create a wedge shape via ColliderBuilder::convex_hull()?");

    let mut tp = TestPipeline::new();

    // 6 vertices forming a triangular prism (ramp/wedge)
    // Base is a rectangle in XZ, top edge is along Z
    let points = [
        point![0.0, 0.0, -2.0], // bottom-left-back
        point![4.0, 0.0, -2.0], // bottom-right-back
        point![0.0, 0.0, 2.0],  // bottom-left-front
        point![4.0, 0.0, 2.0],  // bottom-right-front
        point![0.0, 3.0, -2.0], // top-left-back
        point![0.0, 3.0, 2.0],  // top-left-front
    ];

    let maybe_shape = SharedShape::convex_hull(&points);
    println!("  convex_hull() returned: {}", if maybe_shape.is_some() { "Some" } else { "None" });

    assert!(maybe_shape.is_some(), "convex_hull() should successfully create a wedge shape");

    // Now test a ball sliding down the slope
    let wedge_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::fixed()
            .translation(vector![0.0, 0.0, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::new(maybe_shape.unwrap())
            .friction(0.3)
            .build(),
        wedge_body,
        &mut tp.rigid_body_set,
    );

    // Drop a dynamic ball on top of the slope
    let ball_body = tp.rigid_body_set.insert(
        RigidBodyBuilder::dynamic()
            .translation(vector![1.0, 5.0, 0.0])
            .build(),
    );
    tp.collider_set.insert_with_parent(
        ColliderBuilder::ball(0.3)
            .friction(0.3)
            .build(),
        ball_body,
        &mut tp.rigid_body_set,
    );

    let initial_pos = *tp.rigid_body_set.get(ball_body).unwrap().translation();
    println!("  Ball initial position: ({:.2}, {:.2}, {:.2})", initial_pos.x, initial_pos.y, initial_pos.z);

    for i in 0..120 {
        tp.step();

        if i % 30 == 0 || i == 119 {
            let pos = tp.rigid_body_set.get(ball_body).unwrap().translation();
            println!("  Frame {:3}: ball=({:.2}, {:.2}, {:.2})", i, pos.x, pos.y, pos.z);
        }
    }

    let final_pos = *tp.rigid_body_set.get(ball_body).unwrap().translation();
    println!("  Ball final position: ({:.2}, {:.2}, {:.2})", final_pos.x, final_pos.y, final_pos.z);

    // Ball should have moved (either slid down the slope or bounced off)
    let moved = (final_pos - initial_pos).norm();
    println!("  Ball displacement: {:.2}", moved);
    println!("  Result: {}", if moved > 1.0 {
        "CONFIRMED - Convex hull works, ball interacts with wedge"
    } else {
        "UNEXPECTED - Ball didn't move much"
    });

    assert!(moved > 1.0, "Ball should interact with wedge shape (displacement={:.2})", moved);
}

// ===========================================================================
// Test 9: Event type matrix (LOW risk)
// ===========================================================================
#[test]
fn test_spike_event_type_matrix() {
    println!("\n=== TEST 9: Event Type Matrix ===");
    println!("Question: Which event types fire for which body/collider pairs?\n");

    struct Scenario {
        name: &'static str,
        body_type_a: RigidBodyType,
        body_type_b: RigidBodyType,
        sensor_a: bool,
        sensor_b: bool,
    }

    let scenarios = [
        Scenario {
            name: "Dynamic+Dynamic",
            body_type_a: RigidBodyType::Dynamic,
            body_type_b: RigidBodyType::Dynamic,
            sensor_a: false,
            sensor_b: false,
        },
        Scenario {
            name: "Dynamic+Kinematic",
            body_type_a: RigidBodyType::Dynamic,
            body_type_b: RigidBodyType::KinematicPositionBased,
            sensor_a: false,
            sensor_b: false,
        },
        Scenario {
            name: "Kinematic+Kinematic",
            body_type_a: RigidBodyType::KinematicPositionBased,
            body_type_b: RigidBodyType::KinematicPositionBased,
            sensor_a: false,
            sensor_b: false,
        },
        Scenario {
            name: "Dynamic+Sensor",
            body_type_a: RigidBodyType::Dynamic,
            body_type_b: RigidBodyType::Fixed,
            sensor_a: false,
            sensor_b: true,
        },
        Scenario {
            name: "Sensor+Sensor",
            body_type_a: RigidBodyType::Fixed,
            body_type_b: RigidBodyType::Fixed,
            sensor_a: true,
            sensor_b: true,
        },
    ];

    println!("  {:<25} {:>12} {:>15}", "Scenario", "contacts", "intersections");
    println!("  {}", "-".repeat(55));

    for scenario in &scenarios {
        let mut tp = TestPipeline::new();
        // Zero gravity so dynamic bodies don't fall away
        tp.gravity = vector![0.0, 0.0, 0.0];

        let builder_a = match scenario.body_type_a {
            RigidBodyType::Dynamic => RigidBodyBuilder::dynamic(),
            RigidBodyType::KinematicPositionBased => RigidBodyBuilder::kinematic_position_based(),
            RigidBodyType::Fixed => RigidBodyBuilder::fixed(),
            _ => RigidBodyBuilder::fixed(),
        };
        let body_a = tp.rigid_body_set.insert(builder_a.translation(vector![0.0, 0.0, 0.0]).build());
        tp.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0)
                .sensor(scenario.sensor_a)
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build(),
            body_a,
            &mut tp.rigid_body_set,
        );

        let builder_b = match scenario.body_type_b {
            RigidBodyType::Dynamic => RigidBodyBuilder::dynamic(),
            RigidBodyType::KinematicPositionBased => RigidBodyBuilder::kinematic_position_based(),
            RigidBodyType::Fixed => RigidBodyBuilder::fixed(),
            _ => RigidBodyBuilder::fixed(),
        };
        let body_b = tp.rigid_body_set.insert(builder_b.translation(vector![0.5, 0.0, 0.0]).build());
        tp.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0)
                .sensor(scenario.sensor_b)
                .active_events(ActiveEvents::COLLISION_EVENTS)
                .build(),
            body_b,
            &mut tp.rigid_body_set,
        );

        tp.step_n(5);

        let mut has_contacts = false;
        for pair in tp.narrow_phase.contact_pairs() {
            if pair.has_any_active_contact {
                has_contacts = true;
                break;
            }
        }

        let mut has_intersections = false;
        tp.narrow_phase.intersection_pairs().for_each(|(_c1, _c2, intersecting)| {
            if intersecting {
                has_intersections = true;
            }
        });

        let contact_str = if has_contacts { "YES" } else { "NO" };
        let intersection_str = if has_intersections { "YES" } else { "NO" };

        println!(
            "  {:<25} {:>12} {:>15}",
            scenario.name, contact_str, intersection_str
        );
    }

    println!();
    println!("  (This test always passes — it documents behavior for reference)");
}

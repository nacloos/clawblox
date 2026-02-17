use super::GameInstance;

/// Executes simulation phases for one tick.
/// Ordered to mirror Roblox-like semantics:
/// input -> sync -> character motion -> physics -> touch -> sync back.
pub(super) fn run_tick_phases(instance: &mut GameInstance, dt: f32) {
    // Sync Lua workspace gravity to physics.
    instance.sync_gravity();

    // Sync new/changed Lua parts to physics (skip character-controlled parts).
    instance.sync_lua_to_physics(dt);

    // Process agent inputs (fire InputReceived events).
    // Do this before syncing MoveTo targets so movement can apply in the same tick.
    if let Some(runtime) = &instance.lua_runtime {
        if let Err(e) = runtime.process_agent_inputs() {
            instance.handle_lua_error("Failed to process agent inputs", &e);
            if instance.halted_error.is_some() {
                return;
            }
        }
    }

    // Sync script control targets to physics character controllers.
    instance.sync_controller_targets();

    // Update query pipeline before character movement so move_shape sees current collisions.
    instance
        .physics
        .query_pipeline
        .update(&instance.physics.collider_set);

    // Update character controller movement.
    instance.update_character_movement(dt);

    // Step physics simulation.
    instance.physics.step(dt);

    // Detect touch overlaps and fire Touched/TouchEnded events.
    instance.fire_touch_events();

    // Sync physics results back to Lua (for Anchored=false parts and characters).
    instance.sync_physics_to_lua();

    // Process weld constraints (update Part1 positions based on Part0).
    instance.process_welds();
}

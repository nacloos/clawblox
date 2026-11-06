//! `clawblox.player_object_physics = false` is the mesa-island compatibility
//! mode: character motion must match the engine used for the paper runs
//! (branch `nacloos/mesa-island`). The expected positions below were recorded
//! by running this exact script on that engine, with this repo's Mesa World.
//! They include the paper-era wall behavior on purpose (a jump next to a face
//! can end clinging to it), since recordings and agents depend on it.
use std::path::PathBuf;

use clawblox::game::instance::GameInstance;
use clawblox::game::script_bundle::load_world_and_entry_script;
use serde_json::{json, Value};
use uuid::Uuid;

fn run(player_object_physics: bool) -> Vec<[f32; 3]> {
    let world_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("worlds/mesa-island");
    let (_, source) = load_world_and_entry_script(&world_dir).unwrap();
    let mut instance = GameInstance::new(Uuid::new_v4(), None);
    instance.load_script(&source);
    instance.set_player_object_physics(player_object_physics);
    let agent_id = Uuid::new_v4();
    assert!(instance.add_player(agent_id, "CompatProbe"));
    let user_id = instance.players[&agent_id];
    let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();
    for _ in 0..10 {
        instance.tick();
    }

    let script: Vec<(usize, &str, Value)> = vec![
        // Open ground (away from the loose block pile): walk, jump, arrive.
        (0, "Teleport", json!([-5.0, 4.0, -38.0])),
        (60, "MoveTo", json!({"position": [-5.0, 3.0, -46.0]})),
        (100, "Jump", json!({})),
        // TallMesa west face: press, jump, stop, jump.
        (300, "Teleport", json!([-36.0, 4.0, -18.0])),
        (360, "MoveTo", json!({"position": [-20.0, 3.0, -18.0]})),
        (500, "Jump", json!({})),
        (560, "Stop", json!({})),
        (570, "Jump", json!({})),
        // Spire west face: press, jump, stop, jump.
        (700, "Teleport", json!([-37.0, 4.0, -33.0])),
        (760, "MoveTo", json!({"position": [-20.0, 3.0, -33.0]})),
        (900, "Jump", json!({})),
        (960, "Stop", json!({})),
        (970, "Jump", json!({})),
    ];
    let mut positions = Vec::new();
    let mut next = 0;
    for tick in 0..1100 {
        while next < script.len() && script[next].0 == tick {
            let (_, action, data) = &script[next];
            if *action == "Teleport" {
                let p = data.as_array().unwrap();
                let p = [0, 1, 2].map(|i| p[i].as_f64().unwrap() as f32);
                instance.queue_agent_input(user_id, "Stop".to_string(), json!({}));
                instance.physics.set_character_position(hrp_id, p);
            } else {
                instance.queue_agent_input(user_id, action.to_string(), data.clone());
            }
            next += 1;
        }
        instance.tick();
        positions.push(instance.physics.get_character_position(hrp_id).unwrap());
    }
    positions
}

// (tick, position) recorded on the mesa-island engine.
const PAPER_ENGINE: &[(usize, [f32; 3])] = &[
    (200, [-4.9985, 2.55, -45.6917]),
    (299, [-4.9985, 2.55, -45.6917]),
    (520, [-32.0497, 2.5503, -17.997]),
    (580, [-32.0497, 2.5503, -17.997]),
    (599, [-32.0497, 2.5503, -17.997]),
    (920, [-33.0494, 2.5524, -33.0003]),
    (980, [-33.0494, 3.9968, -33.0003]),
    (1099, [-33.0494, 3.9968, -33.0003]),
];

#[test]
fn compat_mode_matches_mesa_island_engine() {
    let positions = run(false);
    for &(tick, expected) in PAPER_ENGINE {
        let got = positions[tick];
        let dev = (0..3)
            .map(|i| (got[i] - expected[i]).abs())
            .fold(0.0, f32::max);
        assert!(
            dev < 1.0e-3,
            "tick {tick}: expected {expected:?} (mesa-island engine), got {got:?}"
        );
    }
}

/// A block placed on its edge at exactly 45 degrees is balanced but unstable.
/// The mesa-island engine released placed parts with the motion of their
/// placement animation, so they tipped over; compat mode must too.
#[test]
fn compat_mode_block_placed_on_edge_falls_flat() {
    let world_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("worlds/mesa-island");
    let (_, source) = load_world_and_entry_script(&world_dir).unwrap();
    let mut instance = GameInstance::new(Uuid::new_v4(), None);
    instance.load_script(&source);
    instance.set_player_object_physics(false);
    let lua = |instance: &GameInstance, code: &str| {
        let _: () = instance
            .lua_runtime
            .as_ref()
            .unwrap()
            .lua()
            .load(code)
            .exec()
            .unwrap();
    };
    let tilt = |instance: &GameInstance| -> f64 {
        instance
            .lua_runtime
            .as_ref()
            .unwrap()
            .lua()
            .load(
                r#"local cf = Workspace.EdgeProbe.CFrame
                local m = math.max(math.abs(cf.UpVector.Y), math.abs(cf.RightVector.Y), math.abs(cf.LookVector.Y))
                return math.deg(math.acos(math.min(1, m)))"#,
            )
            .eval()
            .unwrap()
    };

    // A 1-stud cube on open ground; Place anchors it, eases it to the target
    // over 0.3 s (18 ticks), then unanchors it.
    lua(
        &instance,
        r#"
        local b = Instance.new("Part")
        b.Name = "EdgeProbe"
        b.Size = Vector3.new(1, 1, 1)
        b.Position = Vector3.new(-5, 3, -40)
        b.Parent = Workspace"#,
    );
    for _ in 0..180 {
        instance.tick();
    }
    lua(
        &instance,
        r#"
        local b = Workspace.EdgeProbe
        local floor = b.Position.Y - 0.5
        b.Anchored = true
        b.CanCollide = false
        _G.edgeTarget = CFrame.new(-5, floor + math.sqrt(0.5) + 0.001, -40) * CFrame.Angles(0, 0, math.rad(45))
        _G.edgeStart = CFrame.new(_G.edgeTarget.Position + Vector3.new(0, 1.5, 0))
        b.CFrame = _G.edgeStart"#,
    );
    instance.tick();
    for k in 1..=18 {
        lua(
            &instance,
            &format!(
                r#"local a = {k} / 18
                local t = 1 - (1 - a) * (1 - a)
                Workspace.EdgeProbe.CFrame = _G.edgeStart:Lerp(_G.edgeTarget, t)"#
            ),
        );
        instance.tick();
    }
    lua(
        &instance,
        "local b = Workspace.EdgeProbe; b.Anchored = false; b.CanCollide = true",
    );
    for _ in 0..300 {
        instance.tick();
    }
    let final_tilt = tilt(&instance);
    assert!(
        final_tilt < 1.0,
        "block placed on its edge should tip over and lie flat, still tilted {final_tilt:.1} deg"
    );
}

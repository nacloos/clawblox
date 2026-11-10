//! A part resting on a character's head must not be launched when the
//! character jumps. The jump is blocked by the part (like a low ceiling), and
//! the push used to hand the part the whole launch instead: in the Mesa world
//! (part gravity 10) a block on the head rose ~29 studs while the character
//! rose nothing.
use std::path::PathBuf;

use clawblox::game::instance::GameInstance;
use clawblox::game::script_bundle::load_world_and_entry_script;
use serde_json::json;
use uuid::Uuid;

fn block_y(instance: &GameInstance) -> f64 {
    instance
        .lua_runtime
        .as_ref()
        .unwrap()
        .lua()
        .load("return Workspace.Block_1.Position.Y")
        .eval()
        .unwrap()
}

#[test]
fn jump_does_not_launch_block_resting_on_head() {
    let world_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("worlds/mesa-island");
    let (_, source) = load_world_and_entry_script(&world_dir).unwrap();

    for offset in [0.0f32, 0.1, 0.2] {
        let mut instance = GameInstance::new(Uuid::new_v4(), None);
        instance.load_script(&source);
        let agent_id = Uuid::new_v4();
        assert!(instance.add_player(agent_id, "HeadBlockProbe"));
        let user_id = instance.players[&agent_id];
        let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();
        instance
            .physics
            .set_character_position(hrp_id, [0.0, 4.0, 0.0]);
        for _ in 0..60 {
            instance.tick();
        }

        // Drop a block onto the character's head and let it settle.
        let p = instance.physics.get_character_position(hrp_id).unwrap();
        let _: () = instance
            .lua_runtime
            .as_ref()
            .unwrap()
            .lua()
            .load(format!(
                r#"
                local b = Workspace.Block_1
                b.Anchored = false
                b.AssemblyLinearVelocity = Vector3.new(0, 0, 0)
                b.Position = Vector3.new({}, {}, {})
            "#,
                p[0] + offset,
                p[1] + 3.6,
                p[2]
            ))
            .exec()
            .unwrap();
        for _ in 0..60 {
            instance.tick();
        }
        let rest_y = block_y(&instance);
        let rest_above_head = rest_y - p[1] as f64;
        assert!(
            rest_above_head > 2.0,
            "block should rest on the head (offset {offset}): {rest_above_head:.3} above root"
        );

        instance.queue_agent_input(user_id, "Jump".to_string(), json!({}));
        let mut peak = rest_y;
        for _ in 0..120 {
            instance.tick();
            peak = peak.max(block_y(&instance));
        }
        assert!(
            peak - rest_y < 1.0,
            "jump launched the block on the head (offset {offset}): rose {:.2} to {:.2}",
            peak - rest_y,
            peak
        );
    }
}

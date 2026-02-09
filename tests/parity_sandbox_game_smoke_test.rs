use clawblox::game::instance::GameInstance;
use clawblox::game::lua::instance::AttributeValue;
use clawblox::game::script_bundle::load_world_and_entry_script;
use std::path::Path;
use uuid::Uuid;

fn get_attr_bool(instance: &GameInstance, name: &str, attr: &str) -> bool {
    let runtime = instance.lua_runtime.as_ref().expect("runtime missing");
    let descendants = runtime.workspace().get_descendants();
    for inst in &descendants {
        let data = inst.data.lock().expect("poisoned lock");
        if data.name == name {
            if let Some(AttributeValue::Bool(v)) = data.attributes.get(attr) {
                return *v;
            }
            return false;
        }
    }
    false
}

fn get_attr_number(instance: &GameInstance, name: &str, attr: &str) -> f64 {
    let runtime = instance.lua_runtime.as_ref().expect("runtime missing");
    let descendants = runtime.workspace().get_descendants();
    for inst in &descendants {
        let data = inst.data.lock().expect("poisoned lock");
        if data.name == name {
            if let Some(AttributeValue::Number(v)) = data.attributes.get(attr) {
                return *v;
            }
            return 0.0;
        }
    }
    0.0
}

fn get_attr_string(instance: &GameInstance, name: &str, attr: &str) -> String {
    let runtime = instance.lua_runtime.as_ref().expect("runtime missing");
    let descendants = runtime.workspace().get_descendants();
    for inst in &descendants {
        let data = inst.data.lock().expect("poisoned lock");
        if data.name == name {
            if let Some(AttributeValue::String(v)) = data.attributes.get(attr) {
                return v.clone();
            }
            return String::new();
        }
    }
    String::new()
}

#[test]
fn test_parity_sandbox_zone_control_smoke() {
    let game_dir = Path::new("games/parity-sandbox");
    assert!(game_dir.exists(), "missing game dir at {}", game_dir.display());

    let (_config, bundled_source) = load_world_and_entry_script(game_dir)
        .unwrap_or_else(|e| panic!("failed bundling game scripts from {}: {}", game_dir.display(), e));

    let mut instance = GameInstance::new_with_script(Uuid::new_v4(), &bundled_source, None);

    // Spawn a real player to exercise PlayerAdded + scoring loop.
    let added = instance.add_player(Uuid::new_v4(), "ZoneTester");
    assert!(added, "expected player to join instance");

    std::thread::sleep(std::time::Duration::from_millis(120));
    for _ in 0..140 {
        instance.tick();
    }

    // Boot and wait markers should still pass.
    assert!(get_attr_bool(&instance, "BootMarker", "Booted"));
    assert!(get_attr_bool(&instance, "BootMarker", "SameRef"));
    assert!(get_attr_bool(&instance, "WaitMarker", "Found"));

    // Gameplay markers should be populated and progressing.
    let leader_name = get_attr_string(&instance, "ScoreboardMarker", "LeaderName");
    let leader_score = get_attr_number(&instance, "ScoreboardMarker", "LeaderScore");
    let is_finished = get_attr_bool(&instance, "RoundMarker", "IsFinished");
    let winner_name = get_attr_string(&instance, "RoundMarker", "WinnerName");

    assert_eq!(leader_name, "ZoneTester", "joined player should lead");
    assert!(
        leader_score > 0.0,
        "leader score should increase in zone-control loop"
    );
    assert!(is_finished, "round should eventually finish once points threshold is reached");
    assert_eq!(winner_name, "ZoneTester", "joined player should win deterministic smoke run");
}

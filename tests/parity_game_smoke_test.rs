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

#[test]
fn test_parity_smoke_game_from_games_dir() {
    let game_dir = Path::new("games/parity-smoke");
    assert!(game_dir.exists(), "missing game dir at {}", game_dir.display());

    let (_config, bundled_source) = load_world_and_entry_script(game_dir)
        .unwrap_or_else(|e| panic!("failed bundling game scripts from {}: {}", game_dir.display(), e));

    let mut instance = GameInstance::new_with_script(Uuid::new_v4(), &bundled_source, None);

    std::thread::sleep(std::time::Duration::from_millis(80));
    for _ in 0..8 {
        instance.tick();
    }

    let module_value = get_attr_number(&instance, "ParityMarker", "ModuleValue");
    let run_count = get_attr_number(&instance, "ParityMarker", "RunCount");
    let same_ref = get_attr_bool(&instance, "ParityMarker", "SameRef");
    let found = get_attr_bool(&instance, "WaitMarker", "Found");

    assert_eq!(module_value, 321.0, "Module value should come from require() result");
    assert_eq!(run_count, 1.0, "Module should execute once due to require cache");
    assert!(same_ref, "Cached require values should be same reference");
    assert!(found, "Workspace:WaitForChild should resolve delayed child");
}

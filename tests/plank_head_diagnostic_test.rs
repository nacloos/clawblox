use clawblox::game::instance::GameInstance;
use clawblox::game::lua::instance::AttributeValue;
use uuid::Uuid;

fn read_last_action(instance: &GameInstance, user_id: u64) -> (bool, String) {
    let runtime = instance.lua_runtime.as_ref().unwrap();
    let player = runtime
        .players()
        .get_player_by_user_id(user_id)
        .expect("Player should exist");
    let data = player.data.lock().unwrap();
    let ok = match data.attributes.get("LastActionSuccess") {
        Some(AttributeValue::Bool(v)) => *v,
        _ => false,
    };
    let err = match data.attributes.get("LastActionError") {
        Some(AttributeValue::String(v)) => v.clone(),
        _ => String::new(),
    };
    (ok, err)
}

fn get_hrp_and_plank(instance: &GameInstance) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let runtime = instance.lua_runtime.as_ref().unwrap();
    let descendants = runtime.workspace().get_descendants();
    let mut hrp_pos = None;
    let mut plank_pos = None;
    let mut plank_size = None;
    for inst in descendants {
        let data = inst.data.lock().unwrap();
        if let Some(part) = &data.part_data {
            if data.name == "HumanoidRootPart" {
                hrp_pos = Some([part.position.x, part.position.y, part.position.z]);
            } else if data.name == "Plank" {
                plank_pos = Some([part.position.x, part.position.y, part.position.z]);
                plank_size = Some([part.size.x, part.size.y, part.size.z]);
            }
        }
    }
    (
        hrp_pos.expect("HRP should exist"),
        plank_pos.expect("Plank should exist"),
        plank_size.expect("Plank size should exist"),
    )
}

fn approx_on_head(hrp: [f32; 3], plank_pos: [f32; 3], plank_size: [f32; 3]) -> bool {
    // Mirror bridge_coop_carry constants in main.luau.
    let world_scale = 2.25f32;
    let head_thickness = 0.5 * world_scale;
    let head_y_offset = 2.5 + head_thickness / 2.0;
    let head_contact_threshold = 0.7 * world_scale;

    let head_center = [hrp[0], hrp[1] + head_y_offset, hrp[2]];
    let half = [plank_size[0] / 2.0, plank_size[1] / 2.0, plank_size[2] / 2.0];
    let clamped = [
        head_center[0].clamp(plank_pos[0] - half[0], plank_pos[0] + half[0]),
        head_center[1].clamp(plank_pos[1] - half[1], plank_pos[1] + half[1]),
        head_center[2].clamp(plank_pos[2] - half[2], plank_pos[2] + half[2]),
    ];
    let dx = head_center[0] - clamped[0];
    let dy = head_center[1] - clamped[1];
    let dz = head_center[2] - clamped[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let above_head = clamped[1] >= head_center[1] - 0.2 * world_scale;
    dist < head_contact_threshold && above_head
}

#[test]
fn diagnostic_welded_head_platform_lua_vs_physics() {
    let mut instance = GameInstance::new(Uuid::new_v4(), None);

    instance.load_script(
        r#"
        local Players = game:GetService("Players")
        local AgentInputService = game:GetService("AgentInputService")

        local floor = Instance.new("Part")
        floor.Name = "Floor"
        floor.Size = Vector3.new(200, 1, 200)
        floor.Position = Vector3.new(0, -1, 0)
        floor.Anchored = true
        floor.Parent = Workspace

        local function addHead(hrp)
            local head = Instance.new("Part")
            head.Name = "HeadPlatform"
            head.Size = Vector3.new(2.0, 0.5, 2.0)
            head.Anchored = false
            head.CanCollide = true
            head.Massless = true
            head.Transparency = 1
            head.Parent = hrp.Parent

            local weld = Instance.new("Weld")
            weld.Part0 = hrp
            weld.Part1 = head
            weld.C0 = CFrame.new(0, 2.75, 0)
            weld.Parent = head
        end

        local function initPlayer(player)
            local char = player.Character
            if char then
                local hrp = char:FindFirstChild("HumanoidRootPart")
                if hrp then
                    addHead(hrp)
                end
            end
        end

        Players.PlayerAdded:Connect(initPlayer)
        for _, p in ipairs(Players:GetPlayers()) do
            initPlayer(p)
        end

        if AgentInputService then
            AgentInputService.InputReceived:Connect(function(player, inputType, data)
                if inputType == "Move" and data and data.direction then
                    local humanoid = player.Character and player.Character:FindFirstChild("Humanoid")
                    if humanoid then
                        local d = data.direction
                        humanoid:Move(Vector3.new(d[1], d[2], d[3]), false)
                    end
                end
            end)
        end
    "#,
    );

    let agent_id = Uuid::new_v4();
    assert!(instance.add_player(agent_id, "DiagPlayer"));
    let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();
    let user_id = *instance.players.get(&agent_id).unwrap();

    for _ in 0..30 {
        instance.tick();
    }

    let start_pos = instance.physics.get_character_position(hrp_id).unwrap();
    for _ in 0..120 {
        instance.queue_agent_input(
            user_id,
            "Move".to_string(),
            serde_json::json!({ "direction": [1.0, 0.0, 0.0] }),
        );
        instance.tick();
    }
    let end_pos = instance.physics.get_character_position(hrp_id).unwrap();
    let moved_x = end_pos[0] - start_pos[0];
    assert!(moved_x > 2.0, "Character should move in diagnostic run");

    let runtime = instance.lua_runtime.as_ref().expect("Lua runtime should exist");
    let head = runtime
        .workspace()
        .get_descendants()
        .into_iter()
        .find(|inst| inst.name() == "HeadPlatform")
        .expect("HeadPlatform should exist");

    let (head_lua_pos, head_lua_id) = {
        let data = head.data.lock().unwrap();
        let part = data.part_data.as_ref().expect("HeadPlatform should be a part");
        (
            [part.position.x, part.position.y, part.position.z],
            data.id.0,
        )
    };

    let head_phys_pos = instance
        .physics
        .get_handle(head_lua_id)
        .and_then(|h| instance.physics.get_position(h))
        .expect("HeadPlatform should have physics body");

    let dx = head_lua_pos[0] - head_phys_pos[0];
    let dy = head_lua_pos[1] - head_phys_pos[1];
    let dz = head_lua_pos[2] - head_phys_pos[2];
    let delta_mag = (dx * dx + dy * dy + dz * dz).sqrt();

    println!(
        "[DIAG] welded head lua={:?} phys={:?} |delta|={:.3}",
        head_lua_pos, head_phys_pos, delta_mag
    );

    // This confirms the desync that likely contributes to flaky head-carry behavior.
    assert!(delta_mag > 5.0, "Expected large Lua-vs-physics desync for welded head");
}

#[test]
fn diagnostic_main_luau_find_on_head_then_measure_movement() {
    let mut instance = GameInstance::new(Uuid::new_v4(), None);
    let script_path = "/home/nacloos/Code/physics-world-clawblox/clawblox_game/main.luau";
    let script = std::fs::read_to_string(script_path).expect("Should read main.luau");
    instance.load_script(&script);

    let agent_id = Uuid::new_v4();
    assert!(instance.add_player(agent_id, "DiagPlayer"));
    let hrp_id = *instance.player_hrp_ids.get(&agent_id).unwrap();
    let user_id = *instance.players.get(&agent_id).unwrap();

    for _ in 0..40 {
        instance.tick();
    }

    instance.queue_agent_input(
        user_id,
        "MoveTo".to_string(),
        serde_json::json!({"position": [12.0, 6.0, -30.0]}),
    );
    for _ in 0..420 {
        instance.tick();
    }

    let mut found_success = false;
    let mut found_on_head = false;
    let mut chosen_y = 0.0f32;
    let mut chosen_move_x = 0.0f32;

    for y_offset in [2.8f32, 3.0, 3.2, 3.4, 3.6, 3.8, 4.0, 4.2] {
        let hrp = instance.physics.get_character_position(hrp_id).unwrap();

        instance.queue_agent_input(
            user_id,
            "PlaceObject".to_string(),
            serde_json::json!({
                "object_id": "plank_0",
                "x": hrp[0],
                "y": hrp[1] + y_offset,
                "z": hrp[2],
                "rotation_y": 0.0
            }),
        );
        for _ in 0..12 {
            instance.tick();
        }

        let (ok, err) = read_last_action(&instance, user_id);
        let (hrp_now, plank_now, plank_size) = get_hrp_and_plank(&instance);
        let on_head = approx_on_head(hrp_now, plank_now, plank_size);

        println!(
            "[DIAG] y_offset={:.2} place_ok={} err='{}' on_head={} hrp={:?} plank={:?}",
            y_offset, ok, err, on_head, hrp_now, plank_now
        );

        if ok {
            found_success = true;
        }
        if ok && on_head {
            found_on_head = true;
            chosen_y = y_offset;
            let move_start = instance.physics.get_character_position(hrp_id).unwrap();
            instance.queue_agent_input(
                user_id,
                "MoveTo".to_string(),
                serde_json::json!({
                    "position": [move_start[0] + 8.0, move_start[1], move_start[2]]
                }),
            );
            for _ in 0..180 {
                instance.tick();
            }
            let move_end = instance.physics.get_character_position(hrp_id).unwrap();
            chosen_move_x = move_end[0] - move_start[0];
            break;
        }
    }

    println!(
        "[DIAG] summary found_success={} found_on_head={} y_offset={:.2} moved_x={:.3}",
        found_success, found_on_head, chosen_y, chosen_move_x
    );

    assert!(found_success, "At least one PlaceObject attempt should succeed");
}

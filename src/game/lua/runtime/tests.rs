use super::*;

/// Helper to create a test runtime without async bridge
fn test_runtime() -> LuaRuntime {
    LuaRuntime::new(Uuid::new_v4(), None).expect("Failed to create runtime")
}

#[test]
fn test_runtime_creation() {
    let runtime = test_runtime();
    assert!(!runtime.script_loaded);
}

#[test]
fn test_simple_script() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local part = Instance.new("Part")
            part.Name = "TestPart"
            part.Position = Vector3.new(10, 20, 30)
            part.Parent = Workspace
        "#,
        )
        .expect("Failed to load script");

    let children = runtime.workspace().get_children();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name(), "TestPart");
}

#[test]
fn test_heartbeat() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.tickCount = 0
            game:GetService("RunService").Heartbeat:Connect(function(dt)
                _G.tickCount = _G.tickCount + 1
            end)
        "#,
        )
        .expect("Failed to load script");

    for _ in 0..10 {
        runtime.tick(1.0 / 60.0).expect("Failed to tick");
    }

    let tick_count: i64 = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("tickCount")
        .unwrap();
    assert_eq!(tick_count, 10);
}

#[test]
fn test_screen_gui_reset_on_spawn_property() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local gui = Instance.new("ScreenGui")
            _G.defaultResetOnSpawn = gui.ResetOnSpawn
            gui.ResetOnSpawn = false
            _G.updatedResetOnSpawn = gui.ResetOnSpawn
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let default_value: bool = globals.get("defaultResetOnSpawn").unwrap();
    let updated_value: bool = globals.get("updatedResetOnSpawn").unwrap();
    assert!(
        default_value,
        "ScreenGui.ResetOnSpawn should default to true"
    );
    assert!(
        !updated_value,
        "ScreenGui.ResetOnSpawn should be writable and persist value"
    );
}

#[test]
fn test_uicorner_instance_and_corner_radius_property() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local corner = Instance.new("UICorner")
            _G.defaultCornerScale = corner.CornerRadius.Scale
            _G.defaultCornerOffset = corner.CornerRadius.Offset

            corner.CornerRadius = UDim.new(0, 12)
            _G.updatedCornerScale = corner.CornerRadius.Scale
            _G.updatedCornerOffset = corner.CornerRadius.Offset
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let default_scale: f64 = globals.get("defaultCornerScale").unwrap();
    let default_offset: i64 = globals.get("defaultCornerOffset").unwrap();
    let updated_scale: f64 = globals.get("updatedCornerScale").unwrap();
    let updated_offset: i64 = globals.get("updatedCornerOffset").unwrap();

    assert_eq!(default_scale, 0.0);
    assert_eq!(default_offset, 0);
    assert_eq!(updated_scale, 0.0);
    assert_eq!(updated_offset, 12);
}

#[test]
fn test_humanoid_load_animation_and_track_playback() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local humanoid = Instance.new("Humanoid")
            local animation = Instance.new("Animation")
            animation.AnimationId = "local://fire_rifle"
            local track = humanoid:LoadAnimation(animation)
            _G.track = track
            _G.length = track.Length
            _G.playingBefore = track.IsPlaying
            track:Play()
            _G.playingAfter = track.IsPlaying
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let length: f64 = globals.get("length").unwrap();
    let playing_before: bool = globals.get("playingBefore").unwrap();
    let playing_after: bool = globals.get("playingAfter").unwrap();
    assert!(length > 0.0);
    assert!(!playing_before);
    assert!(playing_after);

    for _ in 0..20 {
        runtime.tick(1.0 / 60.0).expect("Failed to tick");
    }

    let track: mlua::AnyUserData = globals.get("track").unwrap();
    let is_playing: bool = track.get("IsPlaying").unwrap();
    let time_position: f64 = track.get("TimePosition").unwrap();
    assert!(!is_playing, "Track should auto-stop after clip length");
    assert!(
        time_position > 0.0,
        "Track time should advance while playing"
    );
}

#[test]
fn test_gui_enum_and_font_properties_for_hud_compat() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local label = Instance.new("TextLabel")
            label.TextXAlignment = Enum.TextXAlignment.Left
            label.TextYAlignment = Enum.TextYAlignment.Top
            label.Font = Enum.Font.GothamBold
            label.TextStrokeTransparency = 0.35

            _G.xAlign = label.TextXAlignment
            _G.yAlign = label.TextYAlignment
            _G.font = label.Font
            _G.stroke = label.TextStrokeTransparency
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let x_align: String = globals.get("xAlign").unwrap();
    let y_align: String = globals.get("yAlign").unwrap();
    let font: String = globals.get("font").unwrap();
    let stroke: f64 = globals.get("stroke").unwrap();

    assert_eq!(x_align, "Left");
    assert_eq!(y_align, "Top");
    assert_eq!(font, "GothamBold");
    assert!((stroke - 0.35).abs() < 1e-6);
}

#[test]
fn test_runservice_available_as_global_and_game_property() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.globalConnected = false
            _G.propertyConnected = false

            if RunService and RunService.Heartbeat then
                _G.globalConnected = true
            end

            if game.RunService and game.RunService.Heartbeat then
                _G.propertyConnected = true
            end
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let global_connected: bool = globals.get("globalConnected").unwrap();
    let property_connected: bool = globals.get("propertyConnected").unwrap();
    assert!(global_connected, "RunService global should be available");
    assert!(property_connected, "game.RunService should be available");
}

#[test]
fn test_stepped_fires_before_heartbeat() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.order = {}
            _G.steppedDt = 0
            _G.heartbeatDt = 0
            game:GetService("RunService").Stepped:Connect(function(time, dt)
                table.insert(_G.order, "S")
                _G.steppedDt = dt
            end)
            game:GetService("RunService").Heartbeat:Connect(function(dt)
                table.insert(_G.order, "H")
                _G.heartbeatDt = dt
            end)
        "#,
        )
        .expect("Failed to load script");

    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let order: mlua::Table = globals.get("order").unwrap();
    let first: String = order.get(1).unwrap();
    let second: String = order.get(2).unwrap();
    assert_eq!(first, "S");
    assert_eq!(second, "H");

    let stepped_dt: f64 = globals.get("steppedDt").unwrap();
    let heartbeat_dt: f64 = globals.get("heartbeatDt").unwrap();
    assert!((stepped_dt - (1.0 / 60.0)).abs() < 1e-6);
    assert!((heartbeat_dt - (1.0 / 60.0)).abs() < 1e-6);
}

#[test]
fn test_signal_wait_yields_and_resumes() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.waitDone = false
            _G.waitDt = 0

            task.spawn(function()
                local dt = game:GetService("RunService").Heartbeat:Wait()
                _G.waitDt = dt
                _G.waitDone = true
            end)
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let done: bool = globals.get("waitDone").unwrap();
    assert!(!done, "Signal:Wait should yield before first heartbeat");

    runtime.tick(1.0 / 60.0).expect("Failed to tick");
    let done: bool = globals.get("waitDone").unwrap();
    assert!(
        !done,
        "Signal:Wait should resume on the frame after signal fires"
    );

    runtime.tick(1.0 / 60.0).expect("Failed to tick");
    let done: bool = globals.get("waitDone").unwrap();
    assert!(done, "Signal:Wait should eventually resume");
    let dt: f64 = globals.get("waitDt").unwrap();
    assert!((dt - (1.0 / 60.0)).abs() < 1e-6);
}

#[test]
fn test_tick_function() {
    let runtime = test_runtime();
    let result: f64 = runtime.lua().load("return tick()").eval().unwrap();
    assert!(result >= 0.0);
    std::thread::sleep(std::time::Duration::from_millis(50));
    let result2: f64 = runtime.lua().load("return tick()").eval().unwrap();
    assert!(result2 > result);
}

#[test]
fn test_workspace_raycast_hits_rotated_thin_part() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local bar = Instance.new("Part")
            bar.Name = "ThinBar"
            bar.Size = Vector3.new(12, 2, 1)
            bar.Anchored = true
            bar.CFrame = CFrame.new(0, 2, 0) * CFrame.Angles(0, math.rad(45), 0)
            bar.Parent = Workspace

            local result = Workspace:Raycast(Vector3.new(-10, 2, 4), Vector3.new(25, 0, 0))
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
        )
        .expect("Failed to load script");

    let hit_name: Option<String> = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("hitName")
        .unwrap();
    assert_eq!(hit_name.as_deref(), Some("ThinBar"));
}

#[test]
fn test_workspace_raycast_hits_non_collidable_queryable_part() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local front = Instance.new("Part")
            front.Name = "FrontTrigger"
            front.Size = Vector3.new(4, 4, 4)
            front.Anchored = true
            front.CanCollide = false
            front.Position = Vector3.new(0, 2, 0)
            front.Parent = Workspace

            local back = Instance.new("Part")
            back.Name = "BackWall"
            back.Size = Vector3.new(4, 4, 4)
            back.Anchored = true
            back.Position = Vector3.new(0, 2, 8)
            back.Parent = Workspace

            local result = Workspace:Raycast(Vector3.new(0, 2, -10), Vector3.new(0, 0, 30))
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
        )
        .expect("Failed to load script");

    let hit_name: Option<String> = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("hitName")
        .unwrap();
    assert_eq!(hit_name.as_deref(), Some("FrontTrigger"));
}

#[test]
fn test_workspace_raycast_respects_can_query_false() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local front = Instance.new("Part")
            front.Name = "FrontNoQuery"
            front.Size = Vector3.new(4, 4, 4)
            front.Anchored = true
            front.CanCollide = true
            front.CanQuery = false
            front.Position = Vector3.new(0, 2, 0)
            front.Parent = Workspace

            local back = Instance.new("Part")
            back.Name = "BackWall"
            back.Size = Vector3.new(4, 4, 4)
            back.Anchored = true
            back.Position = Vector3.new(0, 2, 8)
            back.Parent = Workspace

            local result = Workspace:Raycast(Vector3.new(0, 2, -10), Vector3.new(0, 0, 30))
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
        )
        .expect("Failed to load script");

    let hit_name: Option<String> = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("hitName")
        .unwrap();
    assert_eq!(hit_name.as_deref(), Some("BackWall"));
}

#[test]
fn test_workspace_raycast_respect_can_collide_filters_non_collidable() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local front = Instance.new("Part")
            front.Name = "FrontTrigger"
            front.Size = Vector3.new(4, 4, 4)
            front.Anchored = true
            front.CanCollide = false
            front.Position = Vector3.new(0, 2, 0)
            front.Parent = Workspace

            local back = Instance.new("Part")
            back.Name = "BackWall"
            back.Size = Vector3.new(4, 4, 4)
            back.Anchored = true
            back.Position = Vector3.new(0, 2, 8)
            back.Parent = Workspace

            local params = RaycastParams.new()
            params.RespectCanCollide = true
            local result = Workspace:Raycast(Vector3.new(0, 2, -10), Vector3.new(0, 0, 30), params)
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
        )
        .expect("Failed to load script");

    let hit_name: Option<String> = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("hitName")
        .unwrap();
    assert_eq!(hit_name.as_deref(), Some("BackWall"));
}

#[test]
fn test_workspace_raycast_collision_group_filters_parts() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local front = Instance.new("Part")
            front.Name = "FrontBlue"
            front.Size = Vector3.new(4, 4, 4)
            front.Anchored = true
            front.CollisionGroup = "Blue"
            front.Position = Vector3.new(0, 2, 0)
            front.Parent = Workspace

            local back = Instance.new("Part")
            back.Name = "BackRed"
            back.Size = Vector3.new(4, 4, 4)
            back.Anchored = true
            back.CollisionGroup = "Red"
            back.Position = Vector3.new(0, 2, 8)
            back.Parent = Workspace

            local params = RaycastParams.new()
            params.CollisionGroup = "Red"
            local result = Workspace:Raycast(Vector3.new(0, 2, -10), Vector3.new(0, 0, 30), params)
            _G.hitName = result and result.Instance and result.Instance.Name or nil
        "#,
        )
        .expect("Failed to load script");

    let hit_name: Option<String> = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("hitName")
        .unwrap();
    assert_eq!(hit_name.as_deref(), Some("BackRed"));
}

#[test]
fn test_get_part_bounds_in_box_uses_volume_overlap() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Name = "EdgeOverlapPart"
            p.Size = Vector3.new(8, 2, 2)
            p.Anchored = true
            p.Position = Vector3.new(3, 1, 0)
            p.Parent = Workspace

            local hits = Workspace:GetPartBoundsInBox(CFrame.new(0, 1, 0), Vector3.new(4, 4, 4))
            _G.boxHit = false
            for _, v in ipairs(hits) do
                if v.Name == "EdgeOverlapPart" then
                    _G.boxHit = true
                end
            end
        "#,
        )
        .expect("Failed to load script");

    let box_hit: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("boxHit")
        .unwrap();
    assert!(box_hit, "Part overlapping box by extent should be included");
}

#[test]
fn test_get_part_bounds_in_radius_uses_part_extent() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Name = "RadiusExtentPart"
            p.Size = Vector3.new(8, 2, 2)
            p.Anchored = true
            p.Position = Vector3.new(3, 1, 0)
            p.Parent = Workspace

            local hits = Workspace:GetPartBoundsInRadius(Vector3.new(0, 1, 0), 1.5)
            _G.radiusHit = false
            for _, v in ipairs(hits) do
                if v.Name == "RadiusExtentPart" then
                    _G.radiusHit = true
                end
            end
        "#,
        )
        .expect("Failed to load script");

    let radius_hit: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("radiusHit")
        .unwrap();
    assert!(
        radius_hit,
        "Part intersecting sphere by volume should be included"
    );
}

#[test]
fn test_get_part_bounds_in_box_respects_overlap_include_filter() {
    let mut runtime = test_runtime();
    runtime
            .load_script(
                r#"
            local folderA = Instance.new("Folder")
            folderA.Name = "FolderA"
            folderA.Parent = Workspace

            local folderB = Instance.new("Folder")
            folderB.Name = "FolderB"
            folderB.Parent = Workspace

            local a = Instance.new("Part")
            a.Name = "InA"
            a.Anchored = true
            a.Size = Vector3.new(2, 2, 2)
            a.Position = Vector3.new(0, 1, 0)
            a.Parent = folderA

            local b = Instance.new("Part")
            b.Name = "InB"
            b.Anchored = true
            b.Size = Vector3.new(2, 2, 2)
            b.Position = Vector3.new(0, 1, 0)
            b.Parent = folderB

            local params = OverlapParams.new()
            params.FilterType = Enum.RaycastFilterType.Include
            params.FilterDescendantsInstances = {folderA}

            local hits = Workspace:GetPartBoundsInBox(CFrame.new(0, 1, 0), Vector3.new(6, 6, 6), params)
            _G.hitA = false
            _G.hitB = false
            for _, v in ipairs(hits) do
                if v.Name == "InA" then _G.hitA = true end
                if v.Name == "InB" then _G.hitB = true end
            end
        "#,
            )
            .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit_a: bool = globals.get("hitA").unwrap();
    let hit_b: bool = globals.get("hitB").unwrap();
    assert!(hit_a, "Included folder part should be returned");
    assert!(!hit_b, "Excluded folder part should be filtered out");
}

#[test]
fn test_get_part_bounds_in_radius_respects_overlap_max_parts() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p1 = Instance.new("Part")
            p1.Name = "P1"
            p1.Anchored = true
            p1.Size = Vector3.new(2, 2, 2)
            p1.Position = Vector3.new(0, 1, 0)
            p1.Parent = Workspace

            local p2 = Instance.new("Part")
            p2.Name = "P2"
            p2.Anchored = true
            p2.Size = Vector3.new(2, 2, 2)
            p2.Position = Vector3.new(1, 1, 0)
            p2.Parent = Workspace

            local params = OverlapParams.new()
            params.MaxParts = 1

            local hits = Workspace:GetPartBoundsInRadius(Vector3.new(0, 1, 0), 4.0, params)
            _G.hitCount = #hits
        "#,
        )
        .expect("Failed to load script");

    let hit_count: i64 = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("hitCount")
        .unwrap();
    assert_eq!(
        hit_count, 1,
        "MaxParts should cap overlap query result size"
    );
}

#[test]
fn test_get_part_bounds_in_radius_overlap_respects_can_query_false() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local q = Instance.new("Part")
            q.Name = "QueryablePart"
            q.Anchored = true
            q.Size = Vector3.new(2, 2, 2)
            q.Position = Vector3.new(0, 1, 0)
            q.Parent = Workspace

            local nq = Instance.new("Part")
            nq.Name = "NoQueryPart"
            nq.Anchored = true
            nq.Size = Vector3.new(2, 2, 2)
            nq.Position = Vector3.new(0, 1, 1)
            nq.CanQuery = false
            nq.Parent = Workspace

            local hits = Workspace:GetPartBoundsInRadius(Vector3.new(0, 1, 0), 4.0)
            _G.hitQueryable = false
            _G.hitNoQuery = false
            for _, v in ipairs(hits) do
                if v.Name == "QueryablePart" then _G.hitQueryable = true end
                if v.Name == "NoQueryPart" then _G.hitNoQuery = true end
            end
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit_queryable: bool = globals.get("hitQueryable").unwrap();
    let hit_no_query: bool = globals.get("hitNoQuery").unwrap();
    assert!(hit_queryable);
    assert!(!hit_no_query);
}

#[test]
fn test_get_part_bounds_in_box_overlap_respect_can_collide() {
    let mut runtime = test_runtime();
    runtime
            .load_script(
                r#"
            local trigger = Instance.new("Part")
            trigger.Name = "Trigger"
            trigger.Anchored = true
            trigger.Size = Vector3.new(2, 2, 2)
            trigger.Position = Vector3.new(0, 1, 0)
            trigger.CanCollide = false
            trigger.Parent = Workspace

            local solid = Instance.new("Part")
            solid.Name = "Solid"
            solid.Anchored = true
            solid.Size = Vector3.new(2, 2, 2)
            solid.Position = Vector3.new(0, 1, 1)
            solid.Parent = Workspace

            local params = OverlapParams.new()
            params.RespectCanCollide = true
            local hits = Workspace:GetPartBoundsInBox(CFrame.new(0, 1, 0), Vector3.new(6, 6, 6), params)
            _G.hitTrigger = false
            _G.hitSolid = false
            for _, v in ipairs(hits) do
                if v.Name == "Trigger" then _G.hitTrigger = true end
                if v.Name == "Solid" then _G.hitSolid = true end
            end
        "#,
            )
            .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit_trigger: bool = globals.get("hitTrigger").unwrap();
    let hit_solid: bool = globals.get("hitSolid").unwrap();
    assert!(
        !hit_trigger,
        "RespectCanCollide should filter CanCollide=false parts"
    );
    assert!(hit_solid, "Solid part should still be included");
}

#[test]
fn test_get_part_bounds_in_box_overlap_collision_group_filters_parts() {
    let mut runtime = test_runtime();
    runtime
            .load_script(
                r#"
            local blue = Instance.new("Part")
            blue.Name = "BluePart"
            blue.Anchored = true
            blue.Size = Vector3.new(2, 2, 2)
            blue.CollisionGroup = "Blue"
            blue.Position = Vector3.new(0, 1, 0)
            blue.Parent = Workspace

            local red = Instance.new("Part")
            red.Name = "RedPart"
            red.Anchored = true
            red.Size = Vector3.new(2, 2, 2)
            red.CollisionGroup = "Red"
            red.Position = Vector3.new(0, 1, 1)
            red.Parent = Workspace

            local params = OverlapParams.new()
            params.CollisionGroup = "Red"
            local hits = Workspace:GetPartBoundsInBox(CFrame.new(0, 1, 0), Vector3.new(6, 6, 6), params)
            _G.hitBlue = false
            _G.hitRed = false
            for _, v in ipairs(hits) do
                if v.Name == "BluePart" then _G.hitBlue = true end
                if v.Name == "RedPart" then _G.hitRed = true end
            end
        "#,
            )
            .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit_blue: bool = globals.get("hitBlue").unwrap();
    let hit_red: bool = globals.get("hitRed").unwrap();
    assert!(!hit_blue);
    assert!(hit_red);
}

#[test]
fn test_get_parts_in_part_returns_overlapping_parts() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local query = Instance.new("Part")
            query.Name = "Query"
            query.Anchored = true
            query.Size = Vector3.new(4, 4, 4)
            query.Position = Vector3.new(0, 1, 0)
            query.Parent = Workspace

            local near = Instance.new("Part")
            near.Name = "Near"
            near.Anchored = true
            near.Size = Vector3.new(2, 2, 2)
            near.Position = Vector3.new(1, 1, 0)
            near.Parent = Workspace

            local far = Instance.new("Part")
            far.Name = "Far"
            far.Anchored = true
            far.Size = Vector3.new(2, 2, 2)
            far.Position = Vector3.new(20, 1, 0)
            far.Parent = Workspace

            local hits = Workspace:GetPartsInPart(query)
            _G.hitNear = false
            _G.hitFar = false
            _G.hitQuery = false
            for _, v in ipairs(hits) do
                if v.Name == "Near" then _G.hitNear = true end
                if v.Name == "Far" then _G.hitFar = true end
                if v.Name == "Query" then _G.hitQuery = true end
            end
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit_near: bool = globals.get("hitNear").unwrap();
    let hit_far: bool = globals.get("hitFar").unwrap();
    let hit_query: bool = globals.get("hitQuery").unwrap();
    assert!(hit_near);
    assert!(!hit_far);
    assert!(!hit_query);
}

#[test]
fn test_get_parts_in_part_respects_overlap_params() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local query = Instance.new("Part")
            query.Name = "Query"
            query.Anchored = true
            query.Size = Vector3.new(4, 4, 4)
            query.Position = Vector3.new(0, 1, 0)
            query.Parent = Workspace

            local red = Instance.new("Part")
            red.Name = "RedSolid"
            red.Anchored = true
            red.Size = Vector3.new(2, 2, 2)
            red.CollisionGroup = "Red"
            red.Position = Vector3.new(1, 1, 0)
            red.Parent = Workspace

            local blue = Instance.new("Part")
            blue.Name = "BlueTrigger"
            blue.Anchored = true
            blue.Size = Vector3.new(2, 2, 2)
            blue.CollisionGroup = "Blue"
            blue.CanCollide = false
            blue.Position = Vector3.new(-1, 1, 0)
            blue.Parent = Workspace

            local params = OverlapParams.new()
            params.CollisionGroup = "Red"
            params.RespectCanCollide = true
            local hits = Workspace:GetPartsInPart(query, params)

            _G.hitRed = false
            _G.hitBlue = false
            for _, v in ipairs(hits) do
                if v.Name == "RedSolid" then _G.hitRed = true end
                if v.Name == "BlueTrigger" then _G.hitBlue = true end
            end
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit_red: bool = globals.get("hitRed").unwrap();
    let hit_blue: bool = globals.get("hitBlue").unwrap();
    assert!(hit_red);
    assert!(!hit_blue);
}

#[test]
fn test_player_has_character() {
    let runtime = test_runtime();
    let (player, hrp_id) = runtime.add_player(12345, "TestPlayer");

    // Verify HRP ID is valid
    assert!(hrp_id > 0);

    // Check player has a character
    let char = player
        .data
        .lock()
        .unwrap()
        .player_data
        .as_ref()
        .unwrap()
        .character
        .clone();
    assert!(char.is_some());

    let char_ref = char.unwrap().upgrade().unwrap();
    let char_inst = Instance::from_ref(char_ref);

    // Check character has HumanoidRootPart
    let hrp = char_inst.find_first_child("HumanoidRootPart", false);
    assert!(hrp.is_some());

    // Check character has Humanoid
    let humanoid = char_inst.find_first_child("Humanoid", false);
    assert!(humanoid.is_some());

    // Check character is in workspace
    assert!(char_inst.parent().is_some());
}

#[test]
fn test_task_spawn() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.spawned = false
            _G.spawnArg = nil
            local thread = task.spawn(function(x)
                _G.spawned = true
                _G.spawnArg = x
            end, 42)
            -- task.spawn runs immediately, so these should already be set
            assert(_G.spawned == true, "task.spawn should run immediately")
            assert(_G.spawnArg == 42, "task.spawn should pass args")
            assert(type(thread) == "thread", "task.spawn should return a thread")
        "#,
        )
        .expect("task.spawn test failed");
}

#[test]
fn test_task_delay() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.delayed = false
            task.delay(0.05, function()
                _G.delayed = true
            end)
        "#,
        )
        .expect("Failed to load script");

    // Should not fire immediately
    let delayed: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("delayed")
        .unwrap();
    assert!(!delayed, "task.delay should not fire immediately");

    // Wait for the delay to elapse
    std::thread::sleep(std::time::Duration::from_millis(60));

    // Tick to trigger resume
    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let delayed: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("delayed")
        .unwrap();
    assert!(delayed, "task.delay callback should have fired after delay");
}

#[test]
fn test_task_delay_with_args() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.delayedVal = nil
            task.delay(0.05, function(a, b)
                _G.delayedVal = a + b
            end, 10, 20)
        "#,
        )
        .expect("Failed to load script");

    std::thread::sleep(std::time::Duration::from_millis(60));
    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let val: i64 = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("delayedVal")
        .unwrap();
    assert_eq!(val, 30, "task.delay should forward args to callback");
}

#[test]
fn test_task_wait() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.waitDone = false
            task.spawn(function()
                task.wait(0.05)
                _G.waitDone = true
            end)
        "#,
        )
        .expect("Failed to load script");

    // Should not be done immediately
    let done: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("waitDone")
        .unwrap();
    assert!(!done, "task.wait should yield");

    std::thread::sleep(std::time::Duration::from_millis(60));
    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let done: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("waitDone")
        .unwrap();
    assert!(done, "task.wait should resume after delay");
}

#[test]
fn test_task_defer() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.deferred = false
            task.defer(function()
                _G.deferred = true
            end)
        "#,
        )
        .expect("Failed to load script");

    // Should not run during load_script
    let deferred: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("deferred")
        .unwrap();
    assert!(!deferred, "task.defer should not run immediately");

    // Should run on next tick
    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let deferred: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("deferred")
        .unwrap();
    assert!(deferred, "task.defer should run on next tick");
}

#[test]
fn test_task_cancel() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.cancelled = false
            local thread = task.delay(0.05, function()
                _G.cancelled = true
            end)
            task.cancel(thread)
        "#,
        )
        .expect("Failed to load script");

    std::thread::sleep(std::time::Duration::from_millis(60));
    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let cancelled: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("cancelled")
        .unwrap();
    assert!(
        !cancelled,
        "task.cancel should prevent callback from running"
    );
}

#[test]
fn test_global_wait_delegates_to_task_wait() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.globalWaitDone = false
            task.spawn(function()
                wait(0.05)
                _G.globalWaitDone = true
            end)
        "#,
        )
        .expect("Failed to load script");

    let done: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("globalWaitDone")
        .unwrap();
    assert!(!done, "global wait() should yield");

    std::thread::sleep(std::time::Duration::from_millis(60));
    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let done: bool = runtime
        .lua()
        .globals()
        .get::<mlua::Table>("_G")
        .unwrap()
        .get("globalWaitDone")
        .unwrap();
    assert!(done, "global wait() should resume after delay");
}

#[test]
fn test_instance_new_unknown_class_errors() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local ok, err = pcall(function()
                Instance.new("DefinitelyUnknownClass")
            end)
            _G.ok = ok
            _G.err = tostring(err)
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    let err: String = globals.get("err").unwrap();
    assert!(!ok);
    assert!(err.contains("Unknown class"));
}

#[test]
fn test_attribute_and_property_changed_signals() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local part = Instance.new("Part")
            part.Parent = Workspace

            _G.attrCount = 0
            _G.propCount = 0

            part:GetAttributeChangedSignal("Coins"):Connect(function()
                _G.attrCount = _G.attrCount + 1
            end)

            part:GetPropertyChangedSignal("Name"):Connect(function()
                _G.propCount = _G.propCount + 1
            end)

            part:SetAttribute("Coins", 1)
            part.Name = "ChangedName"
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let attr_count: i64 = globals.get("attrCount").unwrap();
    let prop_count: i64 = globals.get("propCount").unwrap();
    assert_eq!(attr_count, 1);
    assert_eq!(prop_count, 1);
}

#[test]
fn test_wait_for_child_returns_child() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local folder = Instance.new("Folder")
            folder.Name = "Container"
            folder.Parent = Workspace

            _G.childName = nil

            task.spawn(function()
                local child = folder:WaitForChild("LatePart", 1.0)
                _G.childName = child and child.Name or "nil"
            end)

            task.delay(0.05, function()
                local part = Instance.new("Part")
                part.Name = "LatePart"
                part.Parent = folder
            end)
        "#,
        )
        .expect("Failed to run script");

    std::thread::sleep(std::time::Duration::from_millis(70));
    runtime.tick(1.0 / 60.0).unwrap();
    runtime.tick(1.0 / 60.0).unwrap();

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let child_name: String = globals.get("childName").unwrap();
    assert_eq!(child_name, "LatePart");
}

#[test]
fn test_require_caches_module_result() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local m = Instance.new("ModuleScript")
            m.Source = "_G.runCount = (_G.runCount or 0) + 1; return { value = _G.runCount }"

            local a = require(m)
            local b = require(m)

            _G.runCount = _G.runCount
            _G.sameRef = (a == b)
            _G.value = a.value
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let run_count: i64 = globals.get("runCount").unwrap();
    let same_ref: bool = globals.get("sameRef").unwrap();
    let value: i64 = globals.get("value").unwrap();

    assert_eq!(run_count, 1);
    assert!(same_ref);
    assert_eq!(value, 1);
}

#[test]
fn test_server_script_service_discovers_and_runs_scripts() {
    let runtime = test_runtime();

    let script = Instance::from_data(InstanceData::new_script("BootScript"));
    {
        let mut data = script.data.lock().unwrap();
        data.script_data.as_mut().unwrap().source = "_G.serviceBootRan = true".to_string();
    }

    let sss = runtime.game().server_script_service();
    script.set_parent(Some(&sss));

    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ran: bool = globals.get("serviceBootRan").unwrap();
    assert!(ran);
}

#[test]
fn test_replicated_storage_and_remote_event_fire_all_clients() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local rs = game:GetService("ReplicatedStorage")
            assert(rs ~= nil, "ReplicatedStorage should exist")

            local remote = Instance.new("RemoteEvent")
            remote.Name = "StateUpdate"
            remote.Parent = rs

            remote:FireAllClients({ hello = "world", n = 7 })
        "#,
        )
        .expect("Failed to run script");

    let queued = runtime.game().remote_event_service().drain_events();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].name, "StateUpdate");
    assert!(queued[0].target_user_id.is_none());
    assert_eq!(
        queued[0].payload["hello"],
        serde_json::Value::String("world".to_string())
    );
    assert_eq!(queued[0].payload["n"], serde_json::Value::from(7));
}

#[test]
fn test_remote_event_on_server_event_from_agent_input() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local rs = game:GetService("ReplicatedStorage")
            local remote = Instance.new("RemoteEvent")
            remote.Name = "PlayerInput"
            remote.Parent = rs

            _G.remoteHit = false
            _G.arg1 = nil
            _G.arg2 = nil
            _G.playerName = nil

            remote.OnServerEvent:Connect(function(player, a, b)
                _G.remoteHit = true
                _G.arg1 = a
                _G.arg2 = b
                _G.playerName = player.Name
            end)
        "#,
        )
        .expect("Failed to run script");

    let (_player, _hrp_id) = runtime.add_player(123, "Tester");
    runtime.queue_agent_input(
        123,
        AgentInput::new(
            "RemoteEvent".to_string(),
            serde_json::json!({
                "name": "PlayerInput",
                "args": [42, "go"]
            }),
        ),
    );
    runtime
        .process_agent_inputs()
        .expect("Failed to process agent inputs");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit: bool = globals.get("remoteHit").unwrap();
    let arg1: i64 = globals.get("arg1").unwrap();
    let arg2: String = globals.get("arg2").unwrap();
    let player_name: String = globals.get("playerName").unwrap();
    assert!(hit, "OnServerEvent should fire");
    assert_eq!(arg1, 42);
    assert_eq!(arg2, "go");
    assert_eq!(player_name, "Tester");
}

#[test]
fn test_int_value_value_and_changed_signal() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local v = Instance.new("IntValue")
            _G.changedTo = nil
            v.Changed:Connect(function(n)
                _G.changedTo = n
            end)
            v.Value = 12
            _G.valueNow = v.Value
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let changed_to: i64 = globals.get("changedTo").unwrap();
    let value_now: i64 = globals.get("valueNow").unwrap();
    assert_eq!(changed_to, 12);
    assert_eq!(value_now, 12);
}

#[test]
fn test_other_value_base_types() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local n = Instance.new("NumberValue")
            local b = Instance.new("BoolValue")
            local s = Instance.new("StringValue")

            _G.nChanged = nil
            _G.bChanged = nil
            _G.sChanged = nil

            n.Changed:Connect(function(v) _G.nChanged = v end)
            b.Changed:Connect(function(v) _G.bChanged = v end)
            s.Changed:Connect(function(v) _G.sChanged = v end)

            n.Value = 3.5
            b.Value = true
            s.Value = "abc"

            _G.nNow = n.Value
            _G.bNow = b.Value
            _G.sNow = s.Value
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let n_changed: f64 = globals.get("nChanged").unwrap();
    let b_changed: bool = globals.get("bChanged").unwrap();
    let s_changed: String = globals.get("sChanged").unwrap();
    let n_now: f64 = globals.get("nNow").unwrap();
    let b_now: bool = globals.get("bNow").unwrap();
    let s_now: String = globals.get("sNow").unwrap();

    assert!((n_changed - 3.5).abs() < 1e-6);
    assert!(b_changed);
    assert_eq!(s_changed, "abc");
    assert!((n_now - 3.5).abs() < 1e-6);
    assert!(b_now);
    assert_eq!(s_now, "abc");
}

#[test]
fn test_body_velocity_applies_to_parent_part_velocity() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Parent = Workspace

            local bv = Instance.new("BodyVelocity")
            bv.MaxForce = Vector3.new(100000, 0, 100000)
            bv.Velocity = Vector3.new(0, 0, -70)
            bv.Parent = p

            _G.vz = p.Velocity.Z
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let vz: f32 = globals.get("vz").unwrap();
    assert!((vz + 70.0).abs() < 0.001);
}

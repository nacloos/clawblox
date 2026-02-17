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
fn test_time_function() {
    let runtime = test_runtime();
    let result: f64 = runtime.lua().load("return time()").eval().unwrap();
    assert!(result >= 0.0);
    std::thread::sleep(std::time::Duration::from_millis(50));
    let result2: f64 = runtime.lua().load("return time()").eval().unwrap();
    assert!(result2 > result);
}

#[test]
fn test_datetime_now_parity() {
    let runtime = test_runtime();
    let (type_name, unix_ms): (String, i64) = runtime
        .lua()
        .load(
            r#"
            local dt = DateTime.now()
            return typeof(dt), dt.UnixTimestampMillis
        "#,
        )
        .eval()
        .unwrap();
    assert_eq!(type_name, "DateTime");
    assert!(unix_ms > 0);
}

#[test]
fn test_datetime_from_unix_timestamp_millis_roundtrip() {
    let runtime = test_runtime();
    let (unix_ms, unix_s, iso): (i64, i64, String) = runtime
        .lua()
        .load(
            r#"
            local dt = DateTime.fromUnixTimestampMillis(1700000000123)
            return dt.UnixTimestampMillis, dt.UnixTimestamp, dt:ToIsoDate()
        "#,
        )
        .eval()
        .unwrap();
    assert_eq!(unix_ms, 1700000000123);
    assert_eq!(unix_s, 1700000000);
    assert!(!iso.is_empty());
}

#[test]
fn test_cframe_to_orientation_roundtrip_yxz() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local x, y, z = 0.2, 0.7, -0.4
            local cf =
                CFrame.Angles(0, y, 0)
                * CFrame.Angles(x, 0, 0)
                * CFrame.Angles(0, 0, z)

            local rx, ry, rz = cf:ToOrientation()
            local rebuilt =
                CFrame.Angles(0, ry, 0)
                * CFrame.Angles(rx, 0, 0)
                * CFrame.Angles(0, 0, rz)

            local function ang_diff(a, b)
                return math.atan2(math.sin(a - b), math.cos(a - b))
            end

            _G.errX = math.abs(ang_diff(rx, x))
            _G.errY = math.abs(ang_diff(ry, y))
            _G.errZ = math.abs(ang_diff(rz, z))
            _G.lookDelta = (rebuilt.LookVector - cf.LookVector).Magnitude
            _G.upDelta = (rebuilt.UpVector - cf.UpVector).Magnitude
            _G.rightDelta = (rebuilt.RightVector - cf.RightVector).Magnitude
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let err_x: f64 = globals.get("errX").unwrap();
    let err_y: f64 = globals.get("errY").unwrap();
    let err_z: f64 = globals.get("errZ").unwrap();
    let look_delta: f64 = globals.get("lookDelta").unwrap();
    let up_delta: f64 = globals.get("upDelta").unwrap();
    let right_delta: f64 = globals.get("rightDelta").unwrap();

    assert!(err_x < 1e-4);
    assert!(err_y < 1e-4);
    assert!(err_z < 1e-4);
    assert!(look_delta < 1e-4);
    assert!(up_delta < 1e-4);
    assert!(right_delta < 1e-4);
}

#[test]
fn test_cframe_to_orientation_gimbal_lock_stable() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local x, y, z = math.pi / 2, 0.6, 0.3
            local cf =
                CFrame.Angles(0, y, 0)
                * CFrame.Angles(x, 0, 0)
                * CFrame.Angles(0, 0, z)

            local rx, ry, rz = cf:ToOrientation()
            local rebuilt =
                CFrame.Angles(0, ry, 0)
                * CFrame.Angles(rx, 0, 0)
                * CFrame.Angles(0, 0, rz)

            _G.lookDelta = (rebuilt.LookVector - cf.LookVector).Magnitude
            _G.upDelta = (rebuilt.UpVector - cf.UpVector).Magnitude
            _G.rightDelta = (rebuilt.RightVector - cf.RightVector).Magnitude
            _G.rx = rx
            _G.ry = ry
            _G.rz = rz
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let look_delta: f64 = globals.get("lookDelta").unwrap();
    let up_delta: f64 = globals.get("upDelta").unwrap();
    let right_delta: f64 = globals.get("rightDelta").unwrap();
    let rx: f64 = globals.get("rx").unwrap();
    let ry: f64 = globals.get("ry").unwrap();
    let rz: f64 = globals.get("rz").unwrap();

    assert!(look_delta < 1e-4);
    assert!(up_delta < 1e-4);
    assert!(right_delta < 1e-4);
    assert!(rx.is_finite());
    assert!(ry.is_finite());
    assert!(rz.is_finite());
}

#[test]
fn test_cframe_to_euler_angles_xyz_roundtrip() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local x, y, z = 0.4, -0.3, 0.9
            local cf = CFrame.Angles(x, y, z)
            local rx, ry, rz = cf:ToEulerAnglesXYZ()
            local rebuilt = CFrame.Angles(rx, ry, rz)

            local function ang_diff(a, b)
                return math.atan2(math.sin(a - b), math.cos(a - b))
            end

            _G.errX = math.abs(ang_diff(rx, x))
            _G.errY = math.abs(ang_diff(ry, y))
            _G.errZ = math.abs(ang_diff(rz, z))
            _G.lookDelta = (rebuilt.LookVector - cf.LookVector).Magnitude
            _G.upDelta = (rebuilt.UpVector - cf.UpVector).Magnitude
            _G.rightDelta = (rebuilt.RightVector - cf.RightVector).Magnitude
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let err_x: f64 = globals.get("errX").unwrap();
    let err_y: f64 = globals.get("errY").unwrap();
    let err_z: f64 = globals.get("errZ").unwrap();
    let look_delta: f64 = globals.get("lookDelta").unwrap();
    let up_delta: f64 = globals.get("upDelta").unwrap();
    let right_delta: f64 = globals.get("rightDelta").unwrap();

    assert!(err_x < 1e-4);
    assert!(err_y < 1e-4);
    assert!(err_z < 1e-4);
    assert!(look_delta < 1e-4);
    assert!(up_delta < 1e-4);
    assert!(right_delta < 1e-4);
}

#[test]
fn test_cframe_from_orientation_matches_yxz_and_roundtrips() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local x, y, z = 0.35, -0.8, 0.5
            local cf = CFrame.fromOrientation(x, y, z)
            local manual =
                CFrame.Angles(0, y, 0)
                * CFrame.Angles(x, 0, 0)
                * CFrame.Angles(0, 0, z)

            local rx, ry, rz = cf:ToOrientation()

            local function ang_diff(a, b)
                return math.atan2(math.sin(a - b), math.cos(a - b))
            end

            _G.lookDelta = (cf.LookVector - manual.LookVector).Magnitude
            _G.upDelta = (cf.UpVector - manual.UpVector).Magnitude
            _G.rightDelta = (cf.RightVector - manual.RightVector).Magnitude
            _G.errX = math.abs(ang_diff(rx, x))
            _G.errY = math.abs(ang_diff(ry, y))
            _G.errZ = math.abs(ang_diff(rz, z))
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let look_delta: f64 = globals.get("lookDelta").unwrap();
    let up_delta: f64 = globals.get("upDelta").unwrap();
    let right_delta: f64 = globals.get("rightDelta").unwrap();
    let err_x: f64 = globals.get("errX").unwrap();
    let err_y: f64 = globals.get("errY").unwrap();
    let err_z: f64 = globals.get("errZ").unwrap();

    assert!(look_delta < 1e-4);
    assert!(up_delta < 1e-4);
    assert!(right_delta < 1e-4);
    assert!(err_x < 1e-4);
    assert!(err_y < 1e-4);
    assert!(err_z < 1e-4);
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
fn test_player_character_assignment_fires_character_signals() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local player = Instance.new("Player")
            local first = Instance.new("Model")
            first.Name = "FirstCharacter"
            local second = Instance.new("Model")
            second.Name = "SecondCharacter"

            _G.addedCount = 0
            _G.removingCount = 0
            _G.lastAddedName = ""
            _G.lastRemovingName = ""

            player.CharacterAdded:Connect(function(character)
                _G.addedCount = _G.addedCount + 1
                _G.lastAddedName = character.Name
            end)

            player.CharacterRemoving:Connect(function(character)
                _G.removingCount = _G.removingCount + 1
                _G.lastRemovingName = character.Name
            end)

            player.Character = first
            player.Character = second
            player.Character = nil
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let added_count: i64 = globals.get("addedCount").unwrap();
    let removing_count: i64 = globals.get("removingCount").unwrap();
    let last_added_name: String = globals.get("lastAddedName").unwrap();
    let last_removing_name: String = globals.get("lastRemovingName").unwrap();

    assert_eq!(added_count, 2);
    assert_eq!(removing_count, 2);
    assert_eq!(last_added_name, "SecondCharacter");
    assert_eq!(last_removing_name, "SecondCharacter");
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
fn test_instance_new_accepts_workspace_service_parent() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local ws = game:GetService("Workspace")
            local folder = Instance.new("Folder", ws)
            folder.Name = "FromInstanceNewParentArg"
            _G.parentName = folder.Parent and folder.Parent.Name or ""
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let parent_name: String = globals.get("parentName").unwrap();
    assert_eq!(parent_name, "Workspace");
}

#[test]
fn test_workspace_child_access_via_property_index() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local folder = Instance.new("Folder")
            folder.Name = "TokensFolder"
            folder.Parent = Workspace

            _G.childExists = Workspace.TokensFolder ~= nil
            _G.childCount = Workspace.TokensFolder and #Workspace.TokensFolder:GetChildren() or -1
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let child_exists: bool = globals.get("childExists").unwrap();
    let child_count: i64 = globals.get("childCount").unwrap();
    assert!(
        child_exists,
        "Workspace.TokensFolder should resolve via __index"
    );
    assert_eq!(child_count, 0);
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
fn test_remote_event_typed_vector3_parity_from_agent_input() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local rs = game:GetService("ReplicatedStorage")
            local remote = Instance.new("RemoteEvent")
            remote.Name = "PlayerInput"
            remote.Parent = rs

            _G.remoteHit = false
            _G.argType = nil
            _G.argX = nil
            _G.argY = nil
            _G.argZ = nil

            remote.OnServerEvent:Connect(function(_player, direction)
                _G.remoteHit = true
                _G.argType = typeof(direction)
                if typeof(direction) == "Vector3" then
                    _G.argX = direction.X
                    _G.argY = direction.Y
                    _G.argZ = direction.Z
                end
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
                "args": [{
                    "$type": "Vector3",
                    "x": 1.5,
                    "y": 0.0,
                    "z": -2.25
                }]
            }),
        ),
    );
    runtime
        .process_agent_inputs()
        .expect("Failed to process agent inputs");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit: bool = globals.get("remoteHit").unwrap();
    let arg_type: String = globals.get("argType").unwrap();
    let arg_x: f64 = globals.get("argX").unwrap();
    let arg_y: f64 = globals.get("argY").unwrap();
    let arg_z: f64 = globals.get("argZ").unwrap();

    assert!(hit, "OnServerEvent should fire");
    assert_eq!(arg_type, "Vector3");
    assert!((arg_x - 1.5).abs() < 1e-6);
    assert!(arg_y.abs() < 1e-6);
    assert!((arg_z + 2.25).abs() < 1e-6);
}

#[test]
fn test_instance_clear_all_children_parity() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local folder = Instance.new("Folder")
            folder.Name = "TokensFolder"
            folder.Parent = Workspace

            local a = Instance.new("Part")
            a.Name = "A"
            a.Parent = folder

            local b = Instance.new("Part")
            b.Name = "B"
            b.Parent = folder

            local c = Instance.new("Part")
            c.Name = "C"
            c.Parent = b

            _G.beforeChildren = #folder:GetChildren()
            folder:ClearAllChildren()
            _G.afterChildren = #folder:GetChildren()
            _G.aParentIsNil = (a.Parent == nil)
            _G.bParentIsNil = (b.Parent == nil)
            _G.cParentIsNil = (c.Parent == nil)
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let before_children: i64 = globals.get("beforeChildren").unwrap();
    let after_children: i64 = globals.get("afterChildren").unwrap();
    let a_parent_is_nil: bool = globals.get("aParentIsNil").unwrap();
    let b_parent_is_nil: bool = globals.get("bParentIsNil").unwrap();
    let c_parent_is_nil: bool = globals.get("cParentIsNil").unwrap();

    assert_eq!(before_children, 2);
    assert_eq!(after_children, 0);
    assert!(a_parent_is_nil);
    assert!(b_parent_is_nil);
    assert!(c_parent_is_nil);
}

#[test]
fn test_players_character_auto_loads_property_parity() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local Players = game:GetService("Players")
            _G.defaultValue = Players.CharacterAutoLoads
            Players.CharacterAutoLoads = false
            _G.afterSetFalse = Players.CharacterAutoLoads
            Players.CharacterAutoLoads = true
            _G.afterSetTrue = Players.CharacterAutoLoads
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let default_value: bool = globals.get("defaultValue").unwrap();
    let after_set_false: bool = globals.get("afterSetFalse").unwrap();
    let after_set_true: bool = globals.get("afterSetTrue").unwrap();

    assert!(default_value);
    assert!(!after_set_false);
    assert!(after_set_true);
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
fn test_object_value_value_and_changed_signal_parity() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local folder = Instance.new("Folder")
            folder.Name = "Root"
            folder.Parent = Workspace

            local ov = Instance.new("ObjectValue")
            ov.Name = "Ref"
            ov.Parent = folder

            local target = Instance.new("Part")
            target.Name = "TargetPart"
            target.Parent = folder

            _G.className = ov.ClassName
            _G.initialNil = (ov.Value == nil)

            _G.changedCount = 0
            _G.lastChangedNil = false
            ov.Changed:Connect(function(v)
                _G.changedCount = _G.changedCount + 1
                _G.lastChangedNil = (v == nil)
            end)

            ov.Value = target
            _G.valueName = ov.Value and ov.Value.Name or ""

            ov.Value = nil
            _G.afterNil = (ov.Value == nil)

            local ok, _err = pcall(function()
                ov.Value = 123
            end)
            _G.invalidOk = ok
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let class_name: String = globals.get("className").unwrap();
    let initial_nil: bool = globals.get("initialNil").unwrap();
    let value_name: String = globals.get("valueName").unwrap();
    let after_nil: bool = globals.get("afterNil").unwrap();
    let changed_count: i64 = globals.get("changedCount").unwrap();
    let last_changed_nil: bool = globals.get("lastChangedNil").unwrap();
    let invalid_ok: bool = globals.get("invalidOk").unwrap();

    assert_eq!(class_name, "ObjectValue");
    assert!(initial_nil);
    assert_eq!(value_name, "TargetPart");
    assert!(after_nil);
    assert_eq!(changed_count, 2);
    assert!(last_changed_nil);
    assert!(!invalid_ok);
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

#[test]
fn test_body_force_class_and_force_property() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local bf = Instance.new("BodyForce")
            bf.Force = Vector3.new(0, 123.5, 0)
            _G.isBodyMover = bf:IsA("BodyMover")
            _G.forceY = bf.Force.Y
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let is_body_mover: bool = globals.get("isBodyMover").unwrap();
    let force_y: f64 = globals.get("forceY").unwrap();
    assert!(is_body_mover);
    assert!((force_y - 123.5).abs() < 1e-6);
}

#[test]
fn test_set_network_owner_methods_exist_on_part() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Parent = Workspace
            p:SetNetworkOwner(nil)
            p:SetNetworkOwnershipAuto(true)
            _G.ok = true
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    assert!(ok);
}

#[test]
fn test_part_get_mass_exists_and_returns_positive_for_nonzero_part() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Size = Vector3.new(4, 2, 6)
            _G.mass = p:GetMass()
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let mass: f64 = globals.get("mass").unwrap();
    assert!(mass > 0.0);
}

#[test]
fn test_brickcolor_and_surfacetype_parity_basics() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.BrickColor = BrickColor.new("Bright yellow")
            p.TopSurface = Enum.SurfaceType.Smooth
            p.BottomSurface = Enum.SurfaceType.Smooth

            _G.top = p.TopSurface
            _G.bottom = p.BottomSurface
            _G.colorR = p.Color.R
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let top: String = globals.get("top").unwrap();
    let bottom: String = globals.get("bottom").unwrap();
    let color_r: f64 = globals.get("colorR").unwrap();
    assert_eq!(top, "Smooth");
    assert_eq!(bottom, "Smooth");
    assert!(color_r > 0.7, "BrickColor should map to a visible color");
}

#[test]
fn test_enum_partshape_alias_maps_to_part_type() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Shape = Enum.PartShape.Ball
            _G.shapeName = p.Shape.Name
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let shape_name: String = globals.get("shapeName").unwrap();
    assert_eq!(shape_name, "Ball");
}

#[test]
fn test_game_bind_to_close_api_exists() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local ok, err = pcall(function()
                game:BindToClose(function() end)
            end)
            _G.ok = ok
            _G.err = tostring(err)
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    let err: String = globals.get("err").unwrap();
    assert!(ok, "BindToClose should be callable, err={}", err);
}

#[test]
fn test_instance_get_debug_id_returns_stable_string() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            _G.id1 = p:GetDebugId()
            _G.id2 = p:GetDebugId(5)
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let id1: String = globals.get("id1").unwrap();
    let id2: String = globals.get("id2").unwrap();
    assert!(!id1.is_empty());
    assert_eq!(id1, id2);
    assert!(id1.starts_with("RBX"));
}

#[test]
fn test_constraint_shim_classes_can_be_created_and_configured() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Parent = Workspace

            local a0 = Instance.new("Attachment", p)
            local a1 = Instance.new("Attachment")
            a1.Parent = p

            local align = Instance.new("AlignOrientation")
            align.Attachment0 = a0
            align.Attachment1 = a1
            align.Responsiveness = 200
            align.RigidityEnabled = true
            align.Parent = p

            local bav = Instance.new("BodyAngularVelocity")
            bav.AngularVelocity = Vector3.new(0, 1, 0)
            bav.MaxTorque = Vector3.new(0, 999, 0)
            bav.Parent = p

            _G.responsive = align.Responsiveness
            _G.rigid = align.RigidityEnabled
            _G.maxTorqueY = bav.MaxTorque.Y
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let responsive: f64 = globals.get("responsive").unwrap();
    let rigid: bool = globals.get("rigid").unwrap();
    let max_torque_y: f64 = globals.get("maxTorqueY").unwrap();
    assert!((responsive - 200.0).abs() < 1e-6);
    assert!(rigid);
    assert!((max_torque_y - 999.0).abs() < 1e-6);
}

#[test]
fn test_find_first_child_which_is_a_finds_humanoid_child() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local model = Instance.new("Model")
            local hum = Instance.new("Humanoid")
            hum.Parent = model
            local found = model:FindFirstChildWhichIsA("Humanoid")
            _G.foundName = found and found.ClassName or ""
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let found_name: String = globals.get("foundName").unwrap();
    assert_eq!(found_name, "Humanoid");
}

#[test]
fn test_debris_service_add_item_api_exists() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local Debris = game:GetService("Debris")
            local p = Instance.new("Part")
            p.Parent = Workspace
            Debris:AddItem(p, 2)
            _G.ok = true
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    assert!(ok);
}

#[test]
fn test_debris_service_alias_resolves() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local byClass = game:GetService("Debris")
            local byAlias = game:GetService("DebrisService")
            local byIndex = game.DebrisService
            _G.ok = (byClass ~= nil) and (byAlias ~= nil) and (byIndex ~= nil)
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    assert!(ok);
}

#[test]
fn test_physics_service_api_exists() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local PhysicsService = game:GetService("PhysicsService")
            local p = Instance.new("Part")
            PhysicsService:RegisterCollisionGroup("Players")
            PhysicsService:CollisionGroupSetCollidable("Players", "Players", true)
            PhysicsService:SetPartCollisionGroup(p, "Players")
            _G.ok = PhysicsService ~= nil and p.CollisionGroup == "Players"
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    assert!(ok);
}

#[test]
fn test_vector2_global_and_operations() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local v = Vector2.new(3, 4)
            _G.mag = v.Magnitude
            _G.unitX = v.Unit.X
            _G.unitY = v.Unit.Y
            _G.t = typeof(v)
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let mag: f64 = globals.get("mag").unwrap();
    let unit_x: f64 = globals.get("unitX").unwrap();
    let unit_y: f64 = globals.get("unitY").unwrap();
    let t: String = globals.get("t").unwrap();
    assert!((mag - 5.0).abs() < 1e-6);
    assert!((unit_x - 0.6).abs() < 1e-5);
    assert!((unit_y - 0.8).abs() < 1e-5);
    assert_eq!(t, "Vector2");
}

#[test]
fn test_humanoid_display_distance_fields() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local h = Instance.new("Humanoid")
            h.HealthDisplayDistance = 0
            h.NameDisplayDistance = 12
            _G.hdd = h.HealthDisplayDistance
            _G.ndd = h.NameDisplayDistance
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hdd: f64 = globals.get("hdd").unwrap();
    let ndd: f64 = globals.get("ndd").unwrap();
    assert_eq!(hdd, 0.0);
    assert_eq!(ndd, 12.0);
}

#[test]
fn test_is_descendant_of_non_instance_returns_false() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Parent = Workspace
            _G.ok = (p:IsDescendantOf(Vector3.new(0,0,0)) == false)
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    assert!(ok);
}

#[test]
fn test_physical_properties_and_custom_physical_properties() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.CustomPhysicalProperties = PhysicalProperties.new(1, 0.3, 0.5, 1, 1)
            _G.ok = p.CustomPhysicalProperties ~= nil and typeof(p.CustomPhysicalProperties) == "PhysicalProperties"
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    assert!(ok);
}

#[test]
fn test_basepart_physical_property_aliases_update_custom_physical_properties() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Parent = Workspace

            p.Density = 1.2
            p.Friction = 0.85
            p.Elasticity = 0.1
            p.FrictionWeight = 0.6
            p.ElasticityWeight = 0.4

            local cpp = p.CustomPhysicalProperties
            local cur = p.CurrentPhysicalProperties
            _G.ok = cpp ~= nil
                and math.abs(cpp.Density - 1.2) < 1e-4
                and math.abs(cpp.Friction - 0.85) < 1e-4
                and math.abs(cpp.Elasticity - 0.1) < 1e-4
                and math.abs(cpp.FrictionWeight - 0.6) < 1e-4
                and math.abs(cpp.ElasticityWeight - 0.4) < 1e-4
                and cur ~= nil
                and math.abs(cur.Density - 1.2) < 1e-4
                and math.abs(cur.Friction - 0.85) < 1e-4
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    assert!(ok);
}

#[test]
fn test_get_mass_uses_effective_density() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Size = Vector3.new(2, 2, 2) -- volume 8 for block
            p.CustomPhysicalProperties = PhysicalProperties.new(1.25, 0.3, 0.5, 1, 1)
            _G.mass = p:GetMass()
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let mass: f32 = globals.get("mass").unwrap();
    assert!(
        (mass - 10.0).abs() < 1e-3,
        "expected mass ~= 10.0, got {}",
        mass
    );
}

#[test]
fn test_ancestry_changed_signal_exists_and_fires() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            _G.count = 0
            p.AncestryChanged:Connect(function(inst, parent)
                _G.count = _G.count + 1
            end)
            p.Parent = Workspace
            p.Parent = nil
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let count: i64 = globals.get("count").unwrap();
    assert!(count >= 2, "expected ancestry callbacks, got {}", count);
}

#[test]
fn test_legacy_ray_api_find_part_on_ray_with_ignore_list() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local p = Instance.new("Part")
            p.Name = "RayTarget"
            p.Size = Vector3.new(4, 4, 4)
            p.CFrame = CFrame.new(0, 2, 0)
            p.Anchored = true
            p.Parent = Workspace

            local ray = Ray.new(Vector3.new(0, 2, -10), Vector3.new(0, 0, 20))
            local hit1, pos1 = Workspace:FindPartOnRayWithIgnoreList(ray, {})
            local hit2, pos2 = Workspace:FindPartOnRayWithIgnoreList(ray, {p})

            _G.hitName = hit1 and hit1.Name or ""
            _G.hitIgnored = (hit2 == nil)
            _G.posOk = (pos1 ~= nil)
        "#,
        )
        .expect("Failed to run script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let hit_name: String = globals.get("hitName").unwrap();
    let hit_ignored: bool = globals.get("hitIgnored").unwrap();
    let pos_ok: bool = globals.get("posOk").unwrap();
    assert_eq!(hit_name, "RayTarget");
    assert!(hit_ignored);
    assert!(pos_ok);
}

#[test]
fn test_player_ancestry_changed_exists() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            _G.ok = false
            Players.PlayerAdded:Connect(function(player)
                player.AncestryChanged:Connect(function() end)
                _G.ok = true
            end)
        "#,
        )
        .expect("Failed to run script");

    let (player, _hrp_id) = runtime.add_player(123, "ParityPlayer");
    runtime
        .fire_player_added(&player)
        .expect("Failed to fire player added");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    assert!(ok);
}

#[test]
fn test_debris_add_item_destroys_after_lifetime() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local Debris = game:GetService("Debris")
            local p = Instance.new("Part")
            p.Name = "DebrisVictim"
            p.Parent = Workspace
            Debris:AddItem(p, 0.01)
        "#,
        )
        .expect("Failed to run script");

    let exists_before = runtime
        .workspace()
        .get_descendants()
        .into_iter()
        .any(|i| i.name() == "DebrisVictim");
    assert!(
        exists_before,
        "Debris victim should exist before frame processing"
    );

    runtime.tick(1.0 / 60.0).expect("Failed to tick");

    let exists_after = runtime
        .workspace()
        .get_descendants()
        .into_iter()
        .any(|i| i.name() == "DebrisVictim");
    assert!(
        !exists_after,
        "Debris victim should be destroyed after lifetime elapsed"
    );
}

#[test]
fn test_sound_instance_new_and_properties() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local sound = Instance.new("Sound")
            _G.streamId = sound.StreamId
            _G.volume = sound.Volume
            _G.speed = sound.PlaybackSpeed
            _G.playing = sound.IsPlaying

            sound.StreamId = "test_stream_123"
            sound.Volume = 0.8
            sound.PlaybackSpeed = 1.5

            _G.streamId2 = sound.StreamId
            _G.volume2 = sound.Volume
            _G.speed2 = sound.PlaybackSpeed

            -- Volume clamping
            sound.Volume = 2.0
            _G.volumeHigh = sound.Volume
            sound.Volume = -1.0
            _G.volumeLow = sound.Volume
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let stream_id: String = globals.get("streamId").unwrap();
    let volume: f32 = globals.get("volume").unwrap();
    let speed: f32 = globals.get("speed").unwrap();
    let playing: bool = globals.get("playing").unwrap();
    assert_eq!(stream_id, "");
    assert!((volume - 0.5).abs() < 1e-6);
    assert!((speed - 1.0).abs() < 1e-6);
    assert!(!playing);

    let stream_id2: String = globals.get("streamId2").unwrap();
    let volume2: f32 = globals.get("volume2").unwrap();
    let speed2: f32 = globals.get("speed2").unwrap();
    assert_eq!(stream_id2, "test_stream_123");
    assert!((volume2 - 0.8).abs() < 1e-6);
    assert!((speed2 - 1.5).abs() < 1e-6);

    let volume_high: f32 = globals.get("volumeHigh").unwrap();
    let volume_low: f32 = globals.get("volumeLow").unwrap();
    assert!((volume_high - 1.0).abs() < 1e-6, "Volume should clamp to 1.0");
    assert!((volume_low - 0.0).abs() < 1e-6, "Volume should clamp to 0.0");
}

#[test]
fn test_sound_play_errors_without_stream_id() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local sound = Instance.new("Sound")
            local ok, err = pcall(function()
                sound:Play()
            end)
            _G.ok = ok
            _G.err = tostring(err)
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ok: bool = globals.get("ok").unwrap();
    let err: String = globals.get("err").unwrap();
    assert!(!ok, "Play() with empty StreamId should error");
    assert!(
        err.contains("StreamId"),
        "Error should mention StreamId, got: {}",
        err
    );
}

#[test]
fn test_sound_play_and_is_playing() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local sound = Instance.new("Sound")
            sound.StreamId = "test_stream"
            _G.beforePlay = sound.IsPlaying
            sound:Play()
            _G.afterPlay = sound.IsPlaying
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let before: bool = globals.get("beforePlay").unwrap();
    let after: bool = globals.get("afterPlay").unwrap();
    assert!(!before);
    assert!(after);
}

#[test]
fn test_sound_stop_fires_stopped() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local sound = Instance.new("Sound")
            sound.StreamId = "test_stream"
            sound:Play()

            _G.stoppedFired = false
            sound.Stopped:Connect(function()
                _G.stoppedFired = true
            end)

            sound:Stop()
            _G.isPlaying = sound.IsPlaying
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let is_playing: bool = globals.get("isPlaying").unwrap();
    let stopped_fired: bool = globals.get("stoppedFired").unwrap();
    assert!(!is_playing, "IsPlaying should be false after Stop()");
    assert!(stopped_fired, "Stopped signal should fire on Stop()");
}

#[test]
fn test_sound_mark_ended_fires_ended_and_stopped() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local sound = Instance.new("Sound")
            sound.StreamId = "test_stream"
            sound:Play()

            _G.order = {}
            sound.Ended:Connect(function()
                table.insert(_G.order, "Ended")
            end)
            sound.Stopped:Connect(function()
                table.insert(_G.order, "Stopped")
            end)

            sound:MarkEnded()
            _G.isPlaying = sound.IsPlaying
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let is_playing: bool = globals.get("isPlaying").unwrap();
    assert!(!is_playing, "IsPlaying should be false after MarkEnded()");

    let order: mlua::Table = globals.get("order").unwrap();
    let first: String = order.get(1).unwrap();
    let second: String = order.get(2).unwrap();
    assert_eq!(first, "Ended", "Ended should fire first");
    assert_eq!(second, "Stopped", "Stopped should fire second");
}

#[test]
fn test_sound_stop_noop_when_not_playing() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local sound = Instance.new("Sound")
            sound.StreamId = "test_stream"

            _G.stoppedFired = false
            sound.Stopped:Connect(function()
                _G.stoppedFired = true
            end)

            sound:Stop()
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let stopped_fired: bool = globals.get("stoppedFired").unwrap();
    assert!(
        !stopped_fired,
        "Stopped signal should NOT fire when sound is not playing"
    );
}

#[test]
fn test_sound_mark_ended_noop_when_not_playing() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local sound = Instance.new("Sound")
            sound.StreamId = "test_stream"

            _G.endedFired = false
            _G.stoppedFired = false
            sound.Ended:Connect(function()
                _G.endedFired = true
            end)
            sound.Stopped:Connect(function()
                _G.stoppedFired = true
            end)

            sound:MarkEnded()
        "#,
        )
        .expect("Failed to load script");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let ended_fired: bool = globals.get("endedFired").unwrap();
    let stopped_fired: bool = globals.get("stoppedFired").unwrap();
    assert!(
        !ended_fired,
        "Ended signal should NOT fire when sound is not playing"
    );
    assert!(
        !stopped_fired,
        "Stopped signal should NOT fire when sound is not playing"
    );
}

#[test]
fn test_sound_ended_via_agent_input() {
    let mut runtime = test_runtime();
    runtime
        .load_script(
            r#"
            local sound = Instance.new("Sound")
            sound.StreamId = "stream_abc"
            sound:Play()

            _G.turnReleased = false
            sound.Ended:Connect(function()
                _G.turnReleased = true
            end)

            local AgentInputService = game:GetService("AgentInputService")
            AgentInputService.InputReceived:Connect(function(player, inputType, data)
                if inputType == "AudioEnded" and data.stream_id == "stream_abc" then
                    sound:MarkEnded()
                end
            end)
        "#,
        )
        .expect("Failed to load script");

    let (_player, _hrp_id) = runtime.add_player(123, "Tester");
    runtime.queue_agent_input(
        123,
        AgentInput::new(
            "AudioEnded".to_string(),
            serde_json::json!({ "stream_id": "stream_abc" }),
        ),
    );
    runtime
        .process_agent_inputs()
        .expect("Failed to process agent inputs");

    let globals = runtime.lua().globals().get::<mlua::Table>("_G").unwrap();
    let turn_released: bool = globals.get("turnReleased").unwrap();
    assert!(
        turn_released,
        "Ended signal should fire after AudioEnded agent input triggers MarkEnded"
    );
}

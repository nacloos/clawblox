-- Physics Test - Tests all physics plan features incrementally
-- Each section is labeled so failures point to the exact feature.
-- Sections can be commented out as features are implemented.

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")
local AgentInputService = game:GetService("AgentInputService")

--------------------------------------------------------------------------------
-- CONFIGURATION
--------------------------------------------------------------------------------

local PLATFORM_Y = 0          -- Ground level
local SPAWN_POS = Vector3.new(0, 3, 0)

--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local playerData = {}
local testParts = {}  -- Track all test parts for cleanup

local function track(part)
    table.insert(testParts, part)
    return part
end

--------------------------------------------------------------------------------
-- HELPERS
--------------------------------------------------------------------------------

local function getHumanoid(player)
    local character = player.Character
    if character then
        return character:FindFirstChild("Humanoid")
    end
    return nil
end

local function makePart(name, size, position, color, anchored)
    local part = Instance.new("Part")
    part.Name = name
    part.Size = size
    part.Position = position
    part.Anchored = anchored ~= false  -- default true
    part.Color = color or Color3.fromRGB(150, 150, 150)
    part.Parent = Workspace
    return track(part)
end

--------------------------------------------------------------------------------
-- GROUND + WALLS
--------------------------------------------------------------------------------

local function createArena()
    -- Floor
    makePart("Floor", Vector3.new(200, 2, 200), Vector3.new(0, -1, 0),
        Color3.fromRGB(100, 150, 100))

    -- Walls (invisible)
    local walls = {
        {Vector3.new(0, 25, 101), Vector3.new(200, 50, 2)},
        {Vector3.new(0, 25, -101), Vector3.new(200, 50, 2)},
        {Vector3.new(101, 25, 0), Vector3.new(2, 50, 200)},
        {Vector3.new(-101, 25, 0), Vector3.new(2, 50, 200)},
    }
    for i, w in ipairs(walls) do
        local wall = makePart("Wall_" .. i, w[2], w[1])
        wall.Transparency = 1
    end
end

--------------------------------------------------------------------------------
-- TEST 1: ROTATION SYNC (Phase 1)
-- Anchored part rotates via CFrame. Physics collider should match.
-- Player walks into the rotated wall -> should be blocked at the correct angle.
--------------------------------------------------------------------------------

local rotatingWall = nil
local rotatingWallAngle = 0

local function createRotationTest()
    -- Label
    makePart("RotationLabel", Vector3.new(0.2, 2, 6), Vector3.new(20, 2, -20),
        Color3.fromRGB(255, 255, 100))

    -- A tall wall that rotates around Y axis
    rotatingWall = makePart("RotatingWall", Vector3.new(10, 4, 1), Vector3.new(20, 2, -20),
        Color3.fromRGB(200, 50, 50))

    print("[PhysicsTest] Test 1: Rotation sync - rotating wall at (20, 2, -20)")
end

--------------------------------------------------------------------------------
-- TEST 2: RUNTIME PROPERTY CHANGES (Phase 2)
-- Size, CanCollide, Anchored, Velocity change at runtime.
--------------------------------------------------------------------------------

local growingPart = nil
local growTimer = 0
local togglePart = nil
local toggleTimer = 0

local function createPropertyTest()
    -- Part that grows over time (Size dirty flag)
    growingPart = makePart("GrowingPart", Vector3.new(2, 2, 2), Vector3.new(-20, 1, -20),
        Color3.fromRGB(50, 200, 50))

    -- Part that toggles CanCollide every 3 seconds
    togglePart = makePart("TogglePart", Vector3.new(4, 4, 4), Vector3.new(-20, 2, -10),
        Color3.fromRGB(200, 200, 50))

    -- Part that starts anchored, becomes dynamic after 5 seconds (Anchored dirty flag)
    local delayedDrop = makePart("DelayedDrop", Vector3.new(2, 2, 2), Vector3.new(-20, 10, 0),
        Color3.fromRGB(50, 50, 200))
    delayedDrop:SetAttribute("DropTime", 5)
    delayedDrop:SetAttribute("Dropped", false)

    print("[PhysicsTest] Test 2: Property changes - growing/toggle/drop parts at X=-20")
end

--------------------------------------------------------------------------------
-- TEST 3: PART SHAPES (Phase 3)
-- Ball, Cylinder, Wedge shapes in physics.
--------------------------------------------------------------------------------

local slopeBall = nil
local slopeBlock = nil
local slopeBallSpawn = Vector3.new(19, 5, 30)
local slopeBlockSpawn = Vector3.new(19, 5, 33)
local resetButtonPos = Vector3.new(20, 0.5, 24)
local resetCooldown = 0

local function spawnSlopeObjects()
    -- Remove old ones if they exist
    if slopeBall then slopeBall:Destroy() end
    if slopeBlock then slopeBlock:Destroy() end

    -- Ball dropped onto wedge slope — should roll down toward +X
    slopeBall = makePart("SlopeBall", Vector3.new(1, 1, 1), slopeBallSpawn,
        Color3.fromRGB(255, 255, 50), false)
    slopeBall.Shape = Enum.PartType.Ball

    -- Block dropped onto wedge slope for comparison
    slopeBlock = makePart("SlopeBlock", Vector3.new(1, 1, 1), slopeBlockSpawn,
        Color3.fromRGB(50, 255, 50), false)

    print("[PhysicsTest] Slope objects reset")
end

local function createShapeTest()
    -- Ball shape (should roll when unanchored)
    local ball = makePart("TestBall", Vector3.new(3, 3, 3), Vector3.new(0, 5, 30),
        Color3.fromRGB(255, 100, 100), false)
    ball.Shape = Enum.PartType.Ball

    -- Cylinder shape (anchored obstacle)
    local cylinder = makePart("TestCylinder", Vector3.new(6, 2, 6), Vector3.new(10, 1, 30),
        Color3.fromRGB(100, 100, 255))
    cylinder.Shape = Enum.PartType.Cylinder

    -- Wedge shape (ramp) - slope rises from +X to -X
    local wedge = makePart("TestWedge", Vector3.new(6, 3, 6), Vector3.new(20, 1.5, 30),
        Color3.fromRGB(200, 100, 200))
    wedge.Shape = Enum.PartType.Wedge

    -- Reset button near the wedge — walk onto it to respawn slope objects
    local resetBtn = makePart("ResetButton", Vector3.new(3, 1, 3), resetButtonPos,
        Color3.fromRGB(255, 50, 50))

    -- Spawn initial slope objects
    spawnSlopeObjects()

    print("[PhysicsTest] Test 3: Shapes at Z=30. Walk onto red button at Z=24 to reset slope demo.")
end

--------------------------------------------------------------------------------
-- TEST 4: TOUCHED EVENTS (Phase 4)
-- Parts that detect when player touches them.
--------------------------------------------------------------------------------

local touchCount = 0

local function onTouchTrigger(trigger)
    trigger.Touched:Connect(function(otherPart)
        touchCount = touchCount + 1
        trigger.Color = Color3.fromRGB(255, 255, 100)  -- Yellow flash
        print("[Touch] Trigger touched! Count: " .. touchCount)
        wait(0.5)
        trigger.Color = Color3.fromRGB(100, 255, 100)
    end)
end

local function onTouchKillZone(killZone)
    killZone.Touched:Connect(function(otherPart)
        -- Find which player's character this part belongs to
        local character = otherPart.Parent
        if character then
            local hrp = character:FindFirstChild("HumanoidRootPart")
            if hrp then
                hrp.Position = SPAWN_POS
                print("[Touch] Player respawned from kill zone")
            end
        end
    end)
end

local function createTouchTest()
    -- Trigger zone: changes color when touched
    local trigger = makePart("TouchTrigger", Vector3.new(6, 1, 6), Vector3.new(0, 0.5, -30),
        Color3.fromRGB(100, 255, 100))
    trigger.CanCollide = false
    trigger:SetAttribute("IsTrigger", true)

    -- Solid part with Touched: turns red on contact
    local touchWall = makePart("TouchWall", Vector3.new(1, 4, 6), Vector3.new(10, 2, -30),
        Color3.fromRGB(100, 100, 100))
    touchWall:SetAttribute("TouchColor", true)

    -- Kill zone: respawns player
    local killZone = makePart("KillZone", Vector3.new(6, 0.5, 6), Vector3.new(-10, 0.25, -30),
        Color3.fromRGB(255, 50, 50))
    killZone.CanCollide = false
    killZone:SetAttribute("IsKillZone", true)

    -- Connect touch handlers
    onTouchTrigger(trigger)
    onTouchKillZone(killZone)

    print("[PhysicsTest] Test 4: Touch events - trigger/wall/kill at Z=-30")
end

--------------------------------------------------------------------------------
-- TEST 5: JUMP (Phase 5)
-- Platform that requires jumping to reach.
--------------------------------------------------------------------------------

local function createJumpTest()
    -- Low platform (jumpable)
    makePart("JumpPlatform_Low", Vector3.new(6, 1, 6), Vector3.new(40, 2, 0),
        Color3.fromRGB(100, 200, 255))

    -- Medium platform
    makePart("JumpPlatform_Med", Vector3.new(6, 1, 6), Vector3.new(40, 5, 10),
        Color3.fromRGB(100, 150, 255))

    -- High platform (edge of jump height)
    makePart("JumpPlatform_High", Vector3.new(6, 1, 6), Vector3.new(40, 8, 20),
        Color3.fromRGB(100, 100, 255))

    print("[PhysicsTest] Test 5: Jump platforms at X=40, heights 2/5/8")
end

--------------------------------------------------------------------------------
-- TEST 6: KINEMATIC PUSHING (Phase 6)
-- Moving anchored parts that push the player.
--------------------------------------------------------------------------------

local pusherPart = nil
local pusherStartX = -40
local pusherEndX = -20
local pusherSpeed = 5
local pusherDir = 1

local elevatorPart = nil
local elevatorBaseY = 0.5
local elevatorTopY = 10
local elevatorSpeed = 3
local elevatorDir = 1

local spinnerPart = nil
local spinnerAngle = 0
local spinnerSpeed = 1  -- rad/s, ramps up

local function createPushTest()
    -- Horizontal pusher: slides back and forth, pushes player
    pusherPart = makePart("Pusher", Vector3.new(2, 4, 8), Vector3.new(pusherStartX, 2, 20),
        Color3.fromRGB(255, 150, 50))

    -- Vertical elevator: moves up and down, player rides on top
    elevatorPart = makePart("Elevator", Vector3.new(6, 1, 6), Vector3.new(-40, elevatorBaseY, 0),
        Color3.fromRGB(150, 255, 150))

    -- Spinning bar: rotates around Y, sweeps players off.
    -- Taller/thicker geometry improves deterministic contact in automated tests.
    spinnerPart = makePart("Spinner", Vector3.new(16, 4, 3), Vector3.new(-40, 2, -20),
        Color3.fromRGB(255, 50, 150))

    print("[PhysicsTest] Test 6: Kinematic push - pusher/elevator/spinner at X=-40")
end

--------------------------------------------------------------------------------
-- TEST 8: RAYCAST PARITY (Workspace:Raycast with rotation + thin geometry)
--------------------------------------------------------------------------------

local raycastThinBar = nil
local raycastStatus = nil
local raycastProbeOrigin = Vector3.new(50, 2, -20)
local raycastProbeDir = Vector3.new(25, 0, 0)
local raycastTimer = 0
local overlapQueryPart = nil
local overlapStatus = nil
local overlapTimer = 0

local function createRaycastParityTest()
    makePart("RaycastLabel", Vector3.new(0.2, 2, 6), Vector3.new(60, 2, -20),
        Color3.fromRGB(180, 255, 255))

    raycastThinBar = makePart("RaycastThinBar", Vector3.new(12, 2, 1), Vector3.new(60, 2, -20),
        Color3.fromRGB(50, 220, 220))
    raycastThinBar.CFrame = CFrame.new(60, 2, -20) * CFrame.Angles(0, math.rad(45), 0)

    raycastStatus = makePart("RaycastStatus", Vector3.new(2, 1, 2), Vector3.new(56, 1, -20),
        Color3.fromRGB(240, 240, 80))
    raycastStatus:SetAttribute("RaycastHitName", "none")
    raycastStatus:SetAttribute("RaycastDistance", -1)
    raycastStatus:SetAttribute("RaycastPass", false)

    print("[PhysicsTest] Test 8: Raycast parity - rotated thin bar at X=60, Z=-20")
end

--------------------------------------------------------------------------------
-- TEST 9: GETPARTSINPART + OVERLAPPARAMS PARITY
--------------------------------------------------------------------------------

local function createOverlapParityTest()
    makePart("OverlapLabel", Vector3.new(0.2, 2, 7), Vector3.new(80, 2, -20),
        Color3.fromRGB(255, 220, 180))

    overlapQueryPart = makePart("OverlapQuery", Vector3.new(8, 6, 8), Vector3.new(80, 3, -20),
        Color3.fromRGB(255, 180, 120))
    overlapQueryPart.Transparency = 0.75

    local defaultHit = makePart("OverlapHitDefault", Vector3.new(2, 2, 2), Vector3.new(81.5, 3, -20),
        Color3.fromRGB(120, 220, 120))
    defaultHit.CanCollide = false -- Should still be returned by default query behavior

    local noQuery = makePart("OverlapNoQuery", Vector3.new(2, 2, 2), Vector3.new(78.5, 3, -20),
        Color3.fromRGB(220, 120, 120))
    noQuery.CanQuery = false -- Should be excluded from overlaps

    local redSolid = makePart("OverlapRedSolid", Vector3.new(2, 2, 2), Vector3.new(80, 3, -18),
        Color3.fromRGB(220, 80, 80))
    redSolid.CollisionGroup = "Red"

    local redTrigger = makePart("OverlapRedTrigger", Vector3.new(2, 2, 2), Vector3.new(80, 3, -22),
        Color3.fromRGB(180, 60, 60))
    redTrigger.CollisionGroup = "Red"
    redTrigger.CanCollide = false -- Filtered when RespectCanCollide=true

    overlapStatus = makePart("OverlapStatus", Vector3.new(2, 1, 2), Vector3.new(84, 1, -20),
        Color3.fromRGB(240, 240, 80))
    overlapStatus:SetAttribute("OverlapHasDefault", false)
    overlapStatus:SetAttribute("OverlapHasNoQuery", false)
    overlapStatus:SetAttribute("OverlapHasRedSolid", false)
    overlapStatus:SetAttribute("OverlapHasRedTrigger", false)
    overlapStatus:SetAttribute("OverlapPass", false)

    print("[PhysicsTest] Test 9: GetPartsInPart parity - query volume at X=80, Z=-20")
end

--------------------------------------------------------------------------------
-- PLAYER MANAGEMENT
--------------------------------------------------------------------------------

local function setupPlayer(player)
    playerData[player.UserId] = { name = player.Name }

    local humanoid = getHumanoid(player)
    if humanoid then
        humanoid.WalkSpeed = 16
    end

    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            hrp.Position = SPAWN_POS
        end
    end

    print("[PhysicsTest] Player joined: " .. player.Name)
end

local function cleanupPlayer(player)
    playerData[player.UserId] = nil
end

--------------------------------------------------------------------------------
-- INPUT HANDLING
--------------------------------------------------------------------------------

if AgentInputService then
    AgentInputService.InputReceived:Connect(function(player, inputType, inputData)
        if not playerData[player.UserId] then return end

        if inputType == "MoveTo" and inputData and inputData.position then
            local humanoid = getHumanoid(player)
            if humanoid then
                local pos = inputData.position
                humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
            end
        elseif inputType == "Stop" then
            local humanoid = getHumanoid(player)
            if humanoid then
                humanoid:CancelMoveTo()
            end
        elseif inputType == "Jump" then
            local humanoid = getHumanoid(player)
            if humanoid then
                humanoid.Jump = true
            end
        end
    end)
end

--------------------------------------------------------------------------------
-- GAME LOOP
--------------------------------------------------------------------------------

local elapsed = 0

local function updateTests(dt)
    elapsed = elapsed + dt

    -- Test 1: Rotate wall
    if rotatingWall then
        rotatingWallAngle = rotatingWallAngle + dt * 0.5  -- slow rotation
        rotatingWall.CFrame = CFrame.new(20, 2, -20) * CFrame.Angles(0, rotatingWallAngle, 0)
    end

    -- Test 2: Growing part
    if growingPart then
        growTimer = growTimer + dt
        if growTimer > 2 then
            growTimer = 0
            local s = growingPart.Size
            if s.X < 8 then
                growingPart.Size = Vector3.new(s.X + 0.5, s.Y + 0.5, s.Z + 0.5)
            else
                growingPart.Size = Vector3.new(2, 2, 2)  -- Reset
            end
        end
    end

    -- Test 2: Toggle CanCollide
    if togglePart then
        toggleTimer = toggleTimer + dt
        if toggleTimer > 3 then
            toggleTimer = 0
            togglePart.CanCollide = not togglePart.CanCollide
            if togglePart.CanCollide then
                togglePart.Transparency = 0
            else
                togglePart.Transparency = 0.5
            end
        end
    end

    -- Test 2: Delayed anchored -> dynamic
    for _, part in ipairs(testParts) do
        if part:GetAttribute("DropTime") and not part:GetAttribute("Dropped") then
            if elapsed > part:GetAttribute("DropTime") then
                part.Anchored = false
                part:SetAttribute("Dropped", true)
                print("[PhysicsTest] DelayedDrop: Anchored -> false")
            end
        end
    end

    -- Test 3: Reset button proximity check
    if resetCooldown > 0 then
        resetCooldown = resetCooldown - dt
    else
        for _, player in ipairs(Players:GetPlayers()) do
            local char = player.Character
            if char then
                local hrp = char:FindFirstChild("HumanoidRootPart")
                if hrp then
                    local dx = hrp.Position.X - resetButtonPos.X
                    local dz = hrp.Position.Z - resetButtonPos.Z
                    local dist = math.sqrt(dx * dx + dz * dz)
                    if dist < 3 then
                        spawnSlopeObjects()
                        resetCooldown = 3  -- 3 second cooldown
                        break
                    end
                end
            end
        end
    end

    -- Test 6: Horizontal pusher
    if pusherPart then
        local pos = pusherPart.Position
        local newX = pos.X + pusherSpeed * pusherDir * dt
        if newX > pusherEndX then
            pusherDir = -1
        elseif newX < pusherStartX then
            pusherDir = 1
        end
        pusherPart.Position = Vector3.new(newX, pos.Y, pos.Z)
    end

    -- Test 6: Vertical elevator
    if elevatorPart then
        local pos = elevatorPart.Position
        local newY = pos.Y + elevatorSpeed * elevatorDir * dt
        if newY > elevatorTopY then
            elevatorDir = -1
        elseif newY < elevatorBaseY then
            elevatorDir = 1
        end
        elevatorPart.Position = Vector3.new(pos.X, newY, pos.Z)
    end

    -- Test 6: Spinning bar (speed ramps up over time)
    if spinnerPart then
        spinnerSpeed = 1 + elapsed * 0.05  -- Slowly ramp up
        if spinnerSpeed > 8 then spinnerSpeed = 8 end
        spinnerAngle = spinnerAngle + spinnerSpeed * dt
        spinnerPart.CFrame = CFrame.new(-40, 2, -20) * CFrame.Angles(0, spinnerAngle, 0)
    end

    -- Test 8: Probe raycast periodically and publish status via part attributes
    if raycastThinBar and raycastStatus then
        raycastTimer = raycastTimer + dt
        if raycastTimer >= 0.5 then
            raycastTimer = 0
            local result = Workspace:Raycast(raycastProbeOrigin, raycastProbeDir)
            local hitName = result and result.Instance and result.Instance.Name or "nil"
            local hitDistance = result and result.Distance or -1
            local pass = (hitName == "RaycastThinBar")

            raycastStatus:SetAttribute("RaycastHitName", hitName)
            raycastStatus:SetAttribute("RaycastDistance", hitDistance)
            raycastStatus:SetAttribute("RaycastPass", pass)
            raycastStatus.Color = pass and Color3.fromRGB(80, 240, 80) or Color3.fromRGB(240, 80, 80)
        end
    end

    -- Test 9: Probe GetPartsInPart periodically and publish status
    if overlapQueryPart and overlapStatus then
        overlapTimer = overlapTimer + dt
        if overlapTimer >= 0.5 then
            overlapTimer = 0

            local hits = Workspace:GetPartsInPart(overlapQueryPart)
            local hasDefault = false
            local hasNoQuery = false
            for _, inst in ipairs(hits) do
                if inst.Name == "OverlapHitDefault" then
                    hasDefault = true
                elseif inst.Name == "OverlapNoQuery" then
                    hasNoQuery = true
                end
            end

            local params = OverlapParams.new()
            params.CollisionGroup = "Red"
            params.RespectCanCollide = true
            local redHits = Workspace:GetPartsInPart(overlapQueryPart, params)
            local hasRedSolid = false
            local hasRedTrigger = false
            for _, inst in ipairs(redHits) do
                if inst.Name == "OverlapRedSolid" then
                    hasRedSolid = true
                elseif inst.Name == "OverlapRedTrigger" then
                    hasRedTrigger = true
                end
            end

            local pass = hasDefault and (not hasNoQuery) and hasRedSolid and (not hasRedTrigger)
            overlapStatus:SetAttribute("OverlapHasDefault", hasDefault)
            overlapStatus:SetAttribute("OverlapHasNoQuery", hasNoQuery)
            overlapStatus:SetAttribute("OverlapHasRedSolid", hasRedSolid)
            overlapStatus:SetAttribute("OverlapHasRedTrigger", hasRedTrigger)
            overlapStatus:SetAttribute("OverlapPass", pass)
            overlapStatus.Color = pass and Color3.fromRGB(80, 240, 80) or Color3.fromRGB(240, 80, 80)
        end
    end
end

--------------------------------------------------------------------------------
-- INITIALIZATION
--------------------------------------------------------------------------------

Players.PlayerAdded:Connect(setupPlayer)
Players.PlayerRemoving:Connect(cleanupPlayer)

for _, player in ipairs(Players:GetPlayers()) do
    setupPlayer(player)
end

createArena()
createRotationTest()
createPropertyTest()
createShapeTest()
createTouchTest()
createJumpTest()
createPushTest()
createRaycastParityTest()
createOverlapParityTest()

RunService.Heartbeat:Connect(function(dt)
    updateTests(dt)
end)

print("=== Physics Test Game ===")
print("Test 1 (X=20, Z=-20):  Rotation sync - wall rotates, collider should match")
print("Test 2 (X=-20):        Property changes - size/canCollide/anchored")
print("Test 3 (Z=30):         Shapes - ball/cylinder/wedge")
print("Test 4 (Z=-30):        Touched events - trigger/wall/kill zone")
print("Test 5 (X=40):         Jump - platforms at heights 2/5/8")
print("Test 6 (X=-40):        Kinematic push - pusher/elevator/spinner")
print("Test 8 (X=60, Z=-20):  Raycast parity - rotated thin bar hit should pass")
print("Test 9 (X=80, Z=-20):  GetPartsInPart parity - OverlapParams behavior")

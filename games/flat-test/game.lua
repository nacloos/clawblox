-- Flat Test - Simple flat terrain for movement testing
-- No obstacles, no game mechanics, just a flat floor

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")
local AgentInputService = game:GetService("AgentInputService")

--------------------------------------------------------------------------------
-- CONFIGURATION
--------------------------------------------------------------------------------

local MAP_SIZE = 200          -- 200x200 stud map
local PLAYER_SPEED = 20       -- Walk speed

--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local playerData = {}  -- keyed by UserId

--------------------------------------------------------------------------------
-- HELPER FUNCTIONS
--------------------------------------------------------------------------------

local function getPlayerData(player)
    return playerData[player.UserId]
end

local function getHumanoid(player)
    local character = player.Character
    if character then
        return character:FindFirstChild("Humanoid")
    end
    return nil
end

local function getCharacterPosition(player)
    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            return hrp.Position
        end
    end
    return nil
end

--------------------------------------------------------------------------------
-- MAP CREATION
--------------------------------------------------------------------------------

local function createMap()
    -- Main floor - completely flat
    local floor = Instance.new("Part")
    floor.Name = "Floor"
    floor.Size = Vector3.new(MAP_SIZE, 2, MAP_SIZE)
    floor.Position = Vector3.new(0, -1, 0)
    floor.Anchored = true
    floor.Color = Color3.fromRGB(100, 150, 100)  -- Green grass
    floor.Parent = Workspace

    -- Grid markers every 50 studs to help visualize position
    for x = -MAP_SIZE/2, MAP_SIZE/2, 50 do
        for z = -MAP_SIZE/2, MAP_SIZE/2, 50 do
            if x ~= 0 or z ~= 0 then  -- Skip center
                local marker = Instance.new("Part")
                marker.Name = "Marker_" .. x .. "_" .. z
                marker.Size = Vector3.new(2, 0.1, 2)
                marker.Position = Vector3.new(x, 0.05, z)
                marker.Anchored = true
                marker.Color = Color3.fromRGB(80, 120, 80)
                marker.CanCollide = false
                marker.Parent = Workspace
            end
        end
    end

    -- Center marker (origin)
    local center = Instance.new("Part")
    center.Name = "CenterMarker"
    center.Size = Vector3.new(4, 0.1, 4)
    center.Position = Vector3.new(0, 0.05, 0)
    center.Anchored = true
    center.Color = Color3.fromRGB(255, 100, 100)  -- Red
    center.CanCollide = false
    center.Parent = Workspace

    -- Invisible walls
    local walls = {
        {Vector3.new(0, 25, MAP_SIZE/2 + 1), Vector3.new(MAP_SIZE, 50, 2)},
        {Vector3.new(0, 25, -MAP_SIZE/2 - 1), Vector3.new(MAP_SIZE, 50, 2)},
        {Vector3.new(MAP_SIZE/2 + 1, 25, 0), Vector3.new(2, 50, MAP_SIZE)},
        {Vector3.new(-MAP_SIZE/2 - 1, 25, 0), Vector3.new(2, 50, MAP_SIZE)},
    }

    for i, data in ipairs(walls) do
        local wall = Instance.new("Part")
        wall.Name = "Wall_" .. i
        wall.Position = data[1]
        wall.Size = data[2]
        wall.Anchored = true
        wall.Transparency = 1
        wall.CanCollide = true
        wall.Parent = Workspace
    end

    print("Flat test map created: " .. MAP_SIZE .. "x" .. MAP_SIZE .. " studs")
end

--------------------------------------------------------------------------------
-- PLAYER MANAGEMENT
--------------------------------------------------------------------------------

local function setupPlayer(player)
    playerData[player.UserId] = {
        name = player.Name,
    }

    -- Configure humanoid
    local humanoid = getHumanoid(player)
    if humanoid then
        humanoid.WalkSpeed = PLAYER_SPEED
    end

    -- Move to spawn
    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            hrp.Position = Vector3.new(0, 3, 0)  -- Spawn at center
        end
    end

    print("Player joined: " .. player.Name)
end

local function cleanupPlayer(player)
    playerData[player.UserId] = nil
    print("Player left: " .. player.Name)
end

--------------------------------------------------------------------------------
-- INPUT HANDLING
--------------------------------------------------------------------------------

print("AgentInputService available: " .. tostring(AgentInputService ~= nil))
if AgentInputService then
    print("Connecting InputReceived handler...")
    AgentInputService.InputReceived:Connect(function(player, inputType, inputData)
        print("[Input] Received: " .. tostring(inputType))
        local data = getPlayerData(player)
        if not data then
            print("[Input] No player data!")
            return
        end

        print("[Input] " .. player.Name .. " -> " .. inputType)
        if inputType == "MoveTo" and inputData and inputData.position then
            local humanoid = getHumanoid(player)
            if humanoid then
                local pos = inputData.position
                humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
            end
        elseif inputType == "Stop" then
            print("[Stop] Cancelling movement for " .. player.Name)
            local humanoid = getHumanoid(player)
            if humanoid then
                humanoid:CancelMoveTo()
                print("[Stop] CancelMoveTo called")
            else
                print("[Stop] No humanoid found!")
            end
        end
    end)
end

--------------------------------------------------------------------------------
-- GAME LOOP
--------------------------------------------------------------------------------

Players.PlayerAdded:Connect(setupPlayer)
Players.PlayerRemoving:Connect(cleanupPlayer)

-- Setup existing players
for _, player in ipairs(Players:GetPlayers()) do
    setupPlayer(player)
end

-- Create map
createMap()

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    -- Nothing to do in flat test - just movement
end)

print("Flat Test game initialized")

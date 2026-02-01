-- Arena game: PvP combat with pickups
-- Players fight in an arena with health and ammo pickups

local Players = game:GetService("Players")
local RunService = game:GetService("RunService")

-- Configuration
local ARENA_SIZE = 50
local RESPAWN_TIME = 3
local PICKUP_RESPAWN = 10

-- Game state
local playerScores = {}
local pickups = {}

-- Create arena floor
local function createArena()
    local floor = Instance.new("Part")
    floor.Name = "Floor"
    floor.Size = Vector3.new(ARENA_SIZE * 2, 1, ARENA_SIZE * 2)
    floor.Position = Vector3.new(0, -0.5, 0)
    floor.Anchored = true
    floor.Color = Color3.fromRGB(100, 100, 100)
    floor.Material = Enum.Material.Concrete
    floor.Parent = Workspace

    -- Create walls
    local wallPositions = {
        { Vector3.new(ARENA_SIZE, 5, 0), Vector3.new(1, 10, ARENA_SIZE * 2) },
        { Vector3.new(-ARENA_SIZE, 5, 0), Vector3.new(1, 10, ARENA_SIZE * 2) },
        { Vector3.new(0, 5, ARENA_SIZE), Vector3.new(ARENA_SIZE * 2, 10, 1) },
        { Vector3.new(0, 5, -ARENA_SIZE), Vector3.new(ARENA_SIZE * 2, 10, 1) },
    }

    for i, wallData in ipairs(wallPositions) do
        local wall = Instance.new("Part")
        wall.Name = "Wall" .. i
        wall.Position = wallData[1]
        wall.Size = wallData[2]
        wall.Anchored = true
        wall.Color = Color3.fromRGB(80, 80, 80)
        wall.Material = Enum.Material.Brick
        wall.Parent = Workspace
    end
end

-- Create a pickup
local function createPickup(position, pickupType)
    local pickup = Instance.new("Part")
    pickup.Name = pickupType .. "Pickup"
    pickup.Position = position
    pickup.Anchored = true
    pickup:SetAttribute("Type", pickupType)
    pickup:SetAttribute("Active", true)

    if pickupType == "Health" then
        pickup.Size = Vector3.new(2, 2, 2)
        pickup.Color = Color3.fromRGB(0, 255, 0)
        pickup.Shape = Enum.PartType.Ball
    else -- Ammo
        pickup.Size = Vector3.new(1.5, 1.5, 1.5)
        pickup.Color = Color3.fromRGB(255, 200, 0)
        pickup.Shape = Enum.PartType.Block
    end

    pickup.Parent = Workspace
    table.insert(pickups, pickup)
    return pickup
end

-- Spawn pickups around the arena
local function spawnPickups()
    local pickupPositions = {
        { Vector3.new(20, 1, 20), "Health" },
        { Vector3.new(-20, 1, 20), "Ammo" },
        { Vector3.new(20, 1, -20), "Ammo" },
        { Vector3.new(-20, 1, -20), "Health" },
        { Vector3.new(0, 1, 30), "Health" },
        { Vector3.new(0, 1, -30), "Ammo" },
    }

    for _, data in ipairs(pickupPositions) do
        createPickup(data[1], data[2])
    end
end

-- Check if player touches pickup
local function checkPickups()
    for _, player in ipairs(Players:GetPlayers()) do
        local character = player.Character
        if character then
            local rootPart = character:FindFirstChild("HumanoidRootPart")
            local humanoid = character:FindFirstChild("Humanoid")

            if rootPart and humanoid and humanoid.Health > 0 then
                local playerPos = rootPart.Position

                for _, pickup in ipairs(pickups) do
                    if pickup:GetAttribute("Active") then
                        local dist = (pickup.Position - playerPos).Magnitude

                        if dist < 3 then
                            local pickupType = pickup:GetAttribute("Type")

                            if pickupType == "Health" then
                                humanoid.Health = math.min(humanoid.Health + 50, humanoid.MaxHealth)
                                print(player.Name .. " picked up health!")
                            else
                                print(player.Name .. " picked up ammo!")
                            end

                            -- Disable pickup temporarily
                            pickup:SetAttribute("Active", false)
                            pickup.Transparency = 0.8

                            -- Schedule respawn
                            pickup:SetAttribute("RespawnTime", PICKUP_RESPAWN)
                        end
                    end
                end
            end
        end
    end
end

-- Update pickup respawns
local function updatePickups(dt)
    for _, pickup in ipairs(pickups) do
        if not pickup:GetAttribute("Active") then
            local respawnTime = pickup:GetAttribute("RespawnTime") - dt
            pickup:SetAttribute("RespawnTime", respawnTime)

            if respawnTime <= 0 then
                pickup:SetAttribute("Active", true)
                pickup.Transparency = 0
            end
        end
    end
end

-- Initialize game
createArena()
spawnPickups()

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    checkPickups()
    updatePickups(dt)
end)

-- Handle player joining
Players.PlayerAdded:Connect(function(player)
    print(player.Name .. " joined the arena!")
    playerScores[player.UserId] = 0
end)

-- Handle player leaving
Players.PlayerRemoving:Connect(function(player)
    print(player.Name .. " left the arena")
    playerScores[player.UserId] = nil
end)

print("Arena game loaded!")

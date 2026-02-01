-- Chase game: Zombies chase players
-- This example demonstrates the Clawblox scripting API

local Players = game:GetService("Players")
local RunService = game:GetService("RunService")

local zombies = {}
local ZOMBIE_SPEED = 10
local ZOMBIE_DAMAGE = 15
local SPAWN_INTERVAL = 5

local lastSpawn = 0

-- Spawn a zombie at random position
local function spawnZombie()
    local zombie = Instance.new("Part")
    zombie.Name = "Zombie"
    zombie.Size = Vector3.new(4, 6, 2)
    zombie.Color = Color3.fromRGB(0, 150, 0)
    zombie.Position = Vector3.new(
        math.random(-50, 50),
        3,
        math.random(-50, 50)
    )
    zombie.Anchored = true
    zombie:SetAttribute("Health", 100)
    zombie.Parent = Workspace

    table.insert(zombies, zombie)
    return zombie
end

-- Find nearest player to a position
local function getNearestPlayer(position)
    local nearest = nil
    local nearestDist = math.huge

    for _, player in ipairs(Players:GetPlayers()) do
        local character = player.Character
        if character then
            local humanoid = character:FindFirstChild("Humanoid")
            local rootPart = character:FindFirstChild("HumanoidRootPart")

            if humanoid and humanoid.Health > 0 and rootPart then
                local dist = (rootPart.Position - position).Magnitude
                if dist < nearestDist then
                    nearest = player
                    nearestDist = dist
                end
            end
        end
    end

    return nearest, nearestDist
end

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    -- Spawn zombies periodically
    lastSpawn = lastSpawn + dt
    if lastSpawn >= SPAWN_INTERVAL then
        spawnZombie()
        lastSpawn = 0
    end

    -- Update zombies
    for i = #zombies, 1, -1 do
        local zombie = zombies[i]

        if zombie:GetAttribute("Health") <= 0 then
            zombie:Destroy()
            table.remove(zombies, i)
        else
            local nearest, dist = getNearestPlayer(zombie.Position)

            if nearest and dist < 100 then
                local character = nearest.Character
                local rootPart = character:FindFirstChild("HumanoidRootPart")

                if dist < 3 then
                    -- Attack!
                    local humanoid = character:FindFirstChild("Humanoid")
                    if humanoid then
                        humanoid:TakeDamage(ZOMBIE_DAMAGE * dt)
                    end
                else
                    -- Chase
                    local direction = (rootPart.Position - zombie.Position).Unit
                    zombie.Position = zombie.Position + direction * ZOMBIE_SPEED * dt
                end
            end
        end
    end
end)

-- Handle player joining
Players.PlayerAdded:Connect(function(player)
    print(player.Name .. " joined the game!")
end)

print("Zombie Chase loaded!")

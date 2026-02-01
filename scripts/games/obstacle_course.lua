-- Obstacle Course
-- A simple platforming course with moving platforms

local RunService = game:GetService("RunService")

-- Create ground
local ground = Instance.new("Part")
ground.Name = "Ground"
ground.Size = Vector3.new(200, 2, 30)
ground.Position = Vector3.new(0, -1, 0)
ground.Anchored = true
ground.Color = Color3.fromRGB(60, 60, 60)
ground.Parent = Workspace

-- Start platform
local startPlatform = Instance.new("Part")
startPlatform.Name = "Start"
startPlatform.Size = Vector3.new(10, 1, 10)
startPlatform.Position = Vector3.new(-80, 0.5, 0)
startPlatform.Anchored = true
startPlatform.Color = Color3.fromRGB(50, 200, 50)
startPlatform.Parent = Workspace

-- End platform
local endPlatform = Instance.new("Part")
endPlatform.Name = "End"
endPlatform.Size = Vector3.new(10, 1, 10)
endPlatform.Position = Vector3.new(80, 0.5, 0)
endPlatform.Anchored = true
endPlatform.Color = Color3.fromRGB(200, 50, 50)
endPlatform.Parent = Workspace

-- Create static platforms
local platformPositions = {
    {-60, 5, 0},
    {-40, 10, 0},
    {-20, 8, 0},
    {0, 12, 0},
    {20, 10, 0},
    {40, 15, 0},
    {60, 12, 0},
}

for i, pos in ipairs(platformPositions) do
    local platform = Instance.new("Part")
    platform.Name = "Platform_" .. i
    platform.Size = Vector3.new(8, 1, 8)
    platform.Position = Vector3.new(pos[1], pos[2], pos[3])
    platform.Anchored = true
    platform.Color = Color3.fromRGB(100, 100, 200)
    platform.Parent = Workspace
end

-- Create walls/obstacles
local wallPositions = {
    {-50, 5, 0, 2, 10, 10},
    {-30, 8, 0, 2, 16, 10},
    {10, 10, 0, 2, 20, 10},
    {50, 12, 0, 2, 24, 10},
}

for i, wall in ipairs(wallPositions) do
    local obstacle = Instance.new("Part")
    obstacle.Name = "Wall_" .. i
    obstacle.Size = Vector3.new(wall[4], wall[5], wall[6])
    obstacle.Position = Vector3.new(wall[1], wall[2], wall[3])
    obstacle.Anchored = true
    obstacle.Color = Color3.fromRGB(150, 80, 80)
    obstacle.Parent = Workspace
end

-- Moving platform (anchored, controlled by Lua)
local movingPlatform = Instance.new("Part")
movingPlatform.Name = "MovingPlatform"
movingPlatform.Size = Vector3.new(6, 1, 6)
movingPlatform.Position = Vector3.new(30, 8, 0)
movingPlatform.Anchored = true
movingPlatform.Color = Color3.fromRGB(255, 200, 50)
movingPlatform.Parent = Workspace

local time = 0

RunService.Heartbeat:Connect(function(dt)
    time = time + dt

    -- Move the platform back and forth
    local newX = 30 + math.sin(time) * 10
    movingPlatform.Position = Vector3.new(newX, 8, 0)
end)

print("Obstacle Course loaded!")
print("Navigate from green to red platform!")

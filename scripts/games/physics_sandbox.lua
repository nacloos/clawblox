-- Physics Sandbox
-- A sandbox with various physics objects to interact with

local RunService = game:GetService("RunService")

-- Create ground
local ground = Instance.new("Part")
ground.Name = "Ground"
ground.Size = Vector3.new(100, 2, 100)
ground.Position = Vector3.new(0, -1, 0)
ground.Anchored = true
ground.Color = Color3.fromRGB(80, 80, 80)
ground.Parent = Workspace

-- Create ramps
local ramp1 = Instance.new("Part")
ramp1.Name = "Ramp1"
ramp1.Size = Vector3.new(20, 1, 10)
ramp1.Position = Vector3.new(-30, 5, 0)
ramp1.Anchored = true
ramp1.Color = Color3.fromRGB(100, 150, 100)
ramp1.Parent = Workspace

local ramp2 = Instance.new("Part")
ramp2.Name = "Ramp2"
ramp2.Size = Vector3.new(20, 1, 10)
ramp2.Position = Vector3.new(30, 5, 0)
ramp2.Anchored = true
ramp2.Color = Color3.fromRGB(100, 100, 150)
ramp2.Parent = Workspace

-- Create walls to contain physics objects
local walls = {
    {50, 10, 0, 2, 20, 100},   -- Right wall
    {-50, 10, 0, 2, 20, 100},  -- Left wall
    {0, 10, 50, 100, 20, 2},   -- Back wall
    {0, 10, -50, 100, 20, 2},  -- Front wall
}

for i, w in ipairs(walls) do
    local wall = Instance.new("Part")
    wall.Name = "Wall_" .. i
    wall.Size = Vector3.new(w[4], w[5], w[6])
    wall.Position = Vector3.new(w[1], w[2], w[3])
    wall.Anchored = true
    wall.Color = Color3.fromRGB(60, 60, 60)
    wall.Transparency = 0.5
    wall.Parent = Workspace
end

-- Spawn initial physics objects
local colors = {
    Color3.fromRGB(255, 100, 100),
    Color3.fromRGB(100, 255, 100),
    Color3.fromRGB(100, 100, 255),
    Color3.fromRGB(255, 255, 100),
    Color3.fromRGB(255, 100, 255),
    Color3.fromRGB(100, 255, 255),
}

-- Spawn some initial blocks
for i = 1, 20 do
    local block = Instance.new("Part")
    block.Name = "Block_" .. i
    block.Size = Vector3.new(
        math.random(2, 4),
        math.random(2, 4),
        math.random(2, 4)
    )
    block.Position = Vector3.new(
        math.random(-40, 40),
        math.random(10, 30),
        math.random(-40, 40)
    )
    block.Anchored = false
    block.Color = colors[math.random(1, #colors)]
    block.Parent = Workspace
end

-- Dominoes setup
for i = 1, 10 do
    local domino = Instance.new("Part")
    domino.Name = "Domino_" .. i
    domino.Size = Vector3.new(0.5, 4, 2)
    domino.Position = Vector3.new(-20 + i * 3, 2, -20)
    domino.Anchored = false
    domino.Color = Color3.fromRGB(200, 200, 200)
    domino.Parent = Workspace
end

-- Trigger ball to knock over dominoes
local triggerBall = Instance.new("Part")
triggerBall.Name = "TriggerBall"
triggerBall.Size = Vector3.new(3, 3, 3)
triggerBall.Position = Vector3.new(-25, 10, -20)
triggerBall.Anchored = false
triggerBall.Color = Color3.fromRGB(255, 50, 50)
triggerBall.Parent = Workspace

print("Physics Sandbox loaded!")
print("Watch the physics simulation unfold!")

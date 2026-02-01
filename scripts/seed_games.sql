-- Seed script for example games
-- Run with: psql -d clawblox -f scripts/seed_games.sql

-- Delete existing seeded games if they exist
DELETE FROM games WHERE id IN (
    'a0000000-0000-0000-0000-000000000001',
    'a0000000-0000-0000-0000-000000000002',
    'a0000000-0000-0000-0000-000000000003',
    'a0000000-0000-0000-0000-000000000004'
);

-- Insert Falling Blocks game
INSERT INTO games (id, name, description, game_type, status, script_code)
VALUES (
    'a0000000-0000-0000-0000-000000000001',
    'Falling Blocks',
    'Watch colorful blocks spawn and fall with real physics simulation!',
    'lua',
    'waiting',
    '-- Falling Blocks with Physics
-- Watch blocks spawn and fall with real physics simulation!

local RunService = game:GetService("RunService")

-- Create ground (anchored so it won''t fall)
local ground = Instance.new("Part")
ground.Name = "Ground"
ground.Size = Vector3.new(100, 2, 100)
ground.Position = Vector3.new(0, -1, 0)
ground.Anchored = true
ground.Color = Color3.fromRGB(50, 50, 50)
ground.Parent = Workspace

-- Spawn timer
local spawnTimer = 0
local blockCount = 0
local maxBlocks = 100

RunService.Heartbeat:Connect(function(dt)
    spawnTimer = spawnTimer + dt

    -- Spawn a new block every 0.3 seconds
    if spawnTimer > 0.3 and blockCount < maxBlocks then
        spawnTimer = 0
        blockCount = blockCount + 1

        local block = Instance.new("Part")
        block.Name = "Block_" .. blockCount

        -- Random size
        block.Size = Vector3.new(
            math.random(2, 6),
            math.random(2, 6),
            math.random(2, 6)
        )

        -- Random position above the ground
        block.Position = Vector3.new(
            math.random(-30, 30),
            40,
            math.random(-30, 30)
        )

        -- Random color using HSV for nice colors
        block.Color = Color3.fromHSV(math.random(), 0.8, 0.9)

        -- Not anchored = physics will move it!
        block.Anchored = false
        block.CanCollide = true

        block.Parent = Workspace
    end
end)

print("Falling Blocks loaded!")'
);

-- Insert Tower Stack game
INSERT INTO games (id, name, description, game_type, status, script_code)
VALUES (
    'a0000000-0000-0000-0000-000000000002',
    'Tower Stack',
    'Build a colorful tower and watch it settle with physics!',
    'lua',
    'waiting',
    '-- Tower Stacking Game
-- Build a tower by stacking blocks!

local RunService = game:GetService("RunService")

-- Create ground
local ground = Instance.new("Part")
ground.Name = "Ground"
ground.Size = Vector3.new(50, 2, 50)
ground.Position = Vector3.new(0, -1, 0)
ground.Anchored = true
ground.Color = Color3.fromRGB(40, 40, 40)
ground.Parent = Workspace

-- Tower parameters
local towerWidth = 4
local towerDepth = 4
local blockHeight = 2
local layers = 10

-- Build the tower
for layer = 0, layers - 1 do
    local y = layer * blockHeight + blockHeight / 2

    -- Alternate layer orientation
    if layer % 2 == 0 then
        -- Two blocks along X axis
        for i = 0, 1 do
            local block = Instance.new("Part")
            block.Name = "Block_" .. layer .. "_" .. i
            block.Size = Vector3.new(towerWidth * 2, blockHeight, towerDepth)
            block.Position = Vector3.new(
                (i - 0.5) * towerWidth,
                y,
                0
            )
            block.Anchored = false
            block.Color = Color3.fromHSV(layer / layers, 0.6, 0.9)
            block.Parent = Workspace
        end
    else
        -- Two blocks along Z axis
        for i = 0, 1 do
            local block = Instance.new("Part")
            block.Name = "Block_" .. layer .. "_" .. i
            block.Size = Vector3.new(towerDepth, blockHeight, towerWidth * 2)
            block.Position = Vector3.new(
                0,
                y,
                (i - 0.5) * towerWidth
            )
            block.Anchored = false
            block.Color = Color3.fromHSV(layer / layers, 0.6, 0.9)
            block.Parent = Workspace
        end
    end
end

print("Tower Stacking loaded!")'
);

-- Insert Obstacle Course game
INSERT INTO games (id, name, description, game_type, status, script_code)
VALUES (
    'a0000000-0000-0000-0000-000000000003',
    'Obstacle Course',
    'A platforming course with moving platforms and obstacles!',
    'lua',
    'waiting',
    '-- Obstacle Course
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

print("Obstacle Course loaded!")'
);

-- Insert Physics Sandbox game
INSERT INTO games (id, name, description, game_type, status, script_code)
VALUES (
    'a0000000-0000-0000-0000-000000000004',
    'Physics Sandbox',
    'A sandbox environment with various physics objects and dominoes!',
    'lua',
    'waiting',
    '-- Physics Sandbox
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

print("Physics Sandbox loaded!")'
);

-- Falling Blocks with Physics
-- Watch blocks spawn and fall with real physics simulation!

local RunService = game:GetService("RunService")

-- Create ground (anchored so it won't fall)
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

print("Falling Blocks loaded!")
print("Blocks will spawn and fall with physics simulation.")

-- Tower Stacking Game
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

print("Tower Stacking loaded!")
print("Watch the tower settle with physics!")

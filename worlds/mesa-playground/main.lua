-- Mesa World - A simple mesa world with a player and ground

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")
local AgentInputService = game:GetService("AgentInputService")

-- Workspace.Gravity = 196.2
Workspace.Gravity = 10

local BLOCK_SIZE = 0.75
local PICKUP_RANGE = 6
local PLACE_RANGE = 7
local PLAYER_SPAWN_POSITION = Vector3.new(0, 5, 0)
local OUT_OF_BOUNDS_Y = -200

local heldBlocksByPlayer = {}
local placingBlocks = {} -- { block, startPos, targetPos, elapsed, duration }
local trackedBlocks = {}

local blockColors = {
    Sand = Color3.fromRGB(214, 186, 128),
    Slate = Color3.fromRGB(148, 136, 124),
    X = Color3.fromRGB(95, 85, 72),
    O = Color3.fromRGB(95, 85, 72),
}

local TIC_TAC_TOE_PLATFORM_SURFACE = Vector3.new(-30, 2, 28)
local TIC_TAC_TOE_BOARD_CENTER = TIC_TAC_TOE_PLATFORM_SURFACE + Vector3.new(0, 0.09, 0)
local TIC_TAC_TOE_CELL_SPACING = 2

local blockPhysics = PhysicalProperties.new(
    0.7, -- density
    0.9, -- friction
    0.03, -- elasticity
    1.0, -- friction weight
    1.0 -- elasticity weight
)

--------------------------------------------------------------------------------
-- GROUND
--------------------------------------------------------------------------------

local ground = Instance.new("Part")
ground.Name = "Ground"
ground.Size = Vector3.new(100, 10, 100)
ground.Position = Vector3.new(0, -5, 0)
ground.Anchored = true
ground.Color = Color3.fromRGB(85, 239, 196)
ground:SetAttribute("RenderRole", "Ground")
ground.Parent = Workspace

local mesas = {
    -- Ground-level formations
    { name = "LowStep",       size = {8, 2, 10},     pos = {8, 1, 18} },
    { name = "LargeMesa",     size = {20, 4, 16},    pos = {20, 2, 20} },
    { name = "SmallPlateau",  size = {10, 2, 8},     pos = {-30, 1, 28} },
    { name = "SteppingStone", size = {8, 2.5, 8},    pos = {-18, 1.25, -12} },
    { name = "Ridge",         size = {6, 3, 25},     pos = {35, 1.5, -10} },
    { name = "TallMesa",      size = {12, 6, 10},    pos = {-25, 3, -20} },
    -- NE climbing path: large mesa (4) -> high shelf (7) -> peak (11)
    { name = "HighShelf",     size = {10, 7, 8},     pos = {25, 3.5, 28} },
    { name = "Peak",          size = {5, 11, 5},     pos = {28, 5.5, 32} },
    -- SW climbing path: tall mesa (6) -> tower base (7) -> summit (10) -> spire (20)
    { name = "TowerBase",     size = {8, 7, 8},      pos = {-28, 3.5, -26} },
    { name = "Summit",        size = {6, 10, 6},     pos = {-28, 5, -30} },
    { name = "Spire",         size = {4, 20, 4},     pos = {-30, 10, -33} },
}

for _, m in ipairs(mesas) do
    local mesa = Instance.new("Part")
    mesa.Name = m.name
    mesa.Size = Vector3.new(m.size[1], m.size[2], m.size[3])
    mesa.Position = Vector3.new(m.pos[1], m.pos[2], m.pos[3])
    mesa.Anchored = true
    mesa.Color = Color3.fromRGB(200, 160, 100)
    mesa:SetAttribute("RenderRole", "Mesa")
    mesa.Parent = Workspace
end

--------------------------------------------------------------------------------
-- BLOCKS
--------------------------------------------------------------------------------

local function setBlockMetadata(block, colorName, primitive)
    block:SetAttribute("RenderRole", "PickupObject")
    block:SetAttribute("ItemColorName", colorName)
    block:SetAttribute("IsPickupObject", true)
    if primitive then
        block:SetAttribute("RenderPrimitive", primitive)
    end
end

local function createBlock(name, position, colorName, primitive, size)
    local block = Instance.new("Part")
    block.Name = name
    block.Size = size or Vector3.new(BLOCK_SIZE, BLOCK_SIZE, BLOCK_SIZE)
    block.Position = position
    block.Anchored = false
    block.Material = Enum.Material.SmoothPlastic
    block.Color = blockColors[colorName]
    block.CustomPhysicalProperties = blockPhysics
    setBlockMetadata(block, colorName, primitive)
    block.Parent = Workspace
    trackedBlocks[block] = true
    return block
end

local function createBoardPart(name, size, position, color)
    local part = Instance.new("Part")
    part.Name = name
    part.Size = size
    part.Position = position
    part.Anchored = true
    part.Material = Enum.Material.SmoothPlastic
    part.Color = color
    part.Parent = Workspace
    return part
end

local function buildTicTacToeBoard()
    local lineThickness = 0.18
    local lineHeight = 0.18
    local lineColor = Color3.fromRGB(108, 94, 76)

    for _, offset in ipairs({ -1, 1 }) do
        createBoardPart(
            "BoardLineX_" .. tostring(offset),
            Vector3.new(lineThickness, lineHeight, 6.4),
            TIC_TAC_TOE_BOARD_CENTER + Vector3.new(offset * TIC_TAC_TOE_CELL_SPACING / 2, 0.03, 0),
            lineColor
        )
        createBoardPart(
            "BoardLineZ_" .. tostring(offset),
            Vector3.new(6.4, lineHeight, lineThickness),
            TIC_TAC_TOE_BOARD_CENTER + Vector3.new(0, 0.03, offset * TIC_TAC_TOE_CELL_SPACING / 2),
            lineColor
        )
    end
end

local function spawnTicTacToePieces()
    local pieceThickness = 0.18
    local xSize = Vector3.new(0.85, pieceThickness, 0.85)
    local oSize = Vector3.new(0.85, pieceThickness, 0.85)
    local xGroundY = xSize.Y / 2
    local oGroundY = oSize.Y / 2
    local xPositions = {
        Vector3.new(-37.2, xGroundY, 25.7),
        Vector3.new(-36.6, xGroundY, 26.2),
        Vector3.new(-36.1, xGroundY, 25.4),
        Vector3.new(-36.8, xGroundY + 0.12, 25.9),
        Vector3.new(-36.3, xGroundY + 0.22, 26.0),
    }
    local oPositions = {
        Vector3.new(-23.9, oGroundY, 25.8),
        Vector3.new(-23.4, oGroundY, 25.4),
        Vector3.new(-22.9, oGroundY, 26.1),
        Vector3.new(-23.6, oGroundY + 0.1, 25.9),
        Vector3.new(-23.1, oGroundY + 0.18, 25.6),
    }

    for index, position in ipairs(xPositions) do
        createBlock(
            "XPiece_" .. index,
            position,
            "X",
            "box",
            xSize
        )
    end

    for index, position in ipairs(oPositions) do
        createBlock(
            "OPiece_" .. index,
            position,
            "O",
            "cylinder",
            oSize
        )
    end
end

local blockPile = {
    { pos = Vector3.new(14.5, 6.5, -1.9), color = "Sand" },
    { pos = Vector3.new(15.2, 6.9, -1.3), color = "Slate" },
    { pos = Vector3.new(15.8, 7.3, -1.8), color = "Sand" },
    { pos = Vector3.new(14.8, 7.8, -0.9), color = "Slate" },
    { pos = Vector3.new(15.5, 8.2, -1.2), color = "Slate" },
    { pos = Vector3.new(15.0, 8.7, -1.6), color = "Sand" },
    { pos = Vector3.new(14.7, 9.1, -1.1), color = "Slate" },
    { pos = Vector3.new(15.6, 9.6, -1.5), color = "Sand" },
    { pos = Vector3.new(15.1, 10.0, -0.8), color = "Slate" },
    { pos = Vector3.new(15.3, 10.5, -1.3), color = "Sand" },
    { pos = Vector3.new(14.9, 11.0, -1.7), color = "Slate" },
    { pos = Vector3.new(15.4, 11.4, -1.0), color = "Sand" },
}

for index, blockDef in ipairs(blockPile) do
    createBlock("Block_" .. index, blockDef.pos, blockDef.color)
end

buildTicTacToeBoard()
spawnTicTacToePieces()

--------------------------------------------------------------------------------
-- PLAYER
--------------------------------------------------------------------------------

local function getHumanoidRootPart(player)
    local character = player.Character
    if character then
        return character:FindFirstChild("HumanoidRootPart")
    end
    return nil
end

local function setPlayerBlockState(player, block)
    -- Intentionally keep pickup/held state implicit in observations.
    -- Agents can infer state from world changes and LastActionResult.
end

local function setupPlayer(player)
    local humanoid = nil
    local character = player.Character
    if character then
        humanoid = character:FindFirstChild("Humanoid")
    end

    if humanoid then
        humanoid.WalkSpeed = 16
        humanoid.JumpPower = 50
        -- humanoid.JumpPower = 5
    end

    if character then
        character:SetAttribute("ModelUrl", "asset://player.glb")

        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            hrp:SetAttribute("ModelYawOffsetDeg", 180)
            hrp.Position = PLAYER_SPAWN_POSITION
        end
    end

    player:SetAttribute("LastActionResult", "Ready")
end

local function cleanupPlayer(player)
    local heldBlock = heldBlocksByPlayer[player]
    if heldBlock then
        heldBlock:Destroy()
        heldBlocksByPlayer[player] = nil
    end
end

local function resetPlayerToSpawn(player)
    local character = player.Character
    if not character then
        return
    end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    local humanoid = character:FindFirstChild("Humanoid")
    if hrp then
        hrp.Position = PLAYER_SPAWN_POSITION
        hrp.AssemblyLinearVelocity = Vector3.new(0, 0, 0)
        hrp.AssemblyAngularVelocity = Vector3.new(0, 0, 0)
    end
    if humanoid then
        humanoid:CancelMoveTo()
    end
end

local function releaseBlockReferences(block)
    for player, heldBlock in pairs(heldBlocksByPlayer) do
        if heldBlock == block then
            heldBlocksByPlayer[player] = nil
            setPlayerBlockState(player, nil)
            player:SetAttribute("LastActionResult", "ResetAfterFall")
        end
    end

    local i = 1
    while i <= #placingBlocks do
        if placingBlocks[i].block == block then
            table.remove(placingBlocks, i)
        else
            i = i + 1
        end
    end
end

local function destroyBlock(block)
    if not block.Parent then
        return
    end
    releaseBlockReferences(block)
    trackedBlocks[block] = nil
    block:Destroy()
end

--------------------------------------------------------------------------------
-- INPUT HANDLING
--------------------------------------------------------------------------------

local function getHumanoid(player)
    local character = player.Character
    if character then
        return character:FindFirstChild("Humanoid")
    end
    return nil
end

local function findNearestBlock(targetPosition, maxDistance)
    local closestBlock = nil
    local closestDistance = maxDistance

    for _, inst in ipairs(Workspace:GetDescendants()) do
        if inst:IsA("Part") and inst:GetAttribute("IsPickupObject") then
            local distance = (inst.Position - targetPosition).Magnitude
            if distance <= closestDistance then
                closestBlock = inst
                closestDistance = distance
            end
        end
    end

    return closestBlock
end

local function canReach(player, worldPosition, maxDistance)
    local hrp = getHumanoidRootPart(player)
    if not hrp then
        return false
    end
    return (hrp.Position - worldPosition).Magnitude <= maxDistance
end

local function isFiniteNumber(value)
    return type(value) == "number" and value == value and value ~= math.huge and value ~= -math.huge
end

local function parseInputPosition(player, inputData, missingResult, invalidResult)
    if not inputData or type(inputData) ~= "table" or inputData.position == nil then
        player:SetAttribute("LastActionResult", missingResult)
        return nil
    end

    local pos = inputData.position
    if type(pos) ~= "table" then
        player:SetAttribute("LastActionResult", invalidResult)
        return nil
    end

    local x = pos[1]
    local y = pos[2]
    local z = pos[3]
    if not isFiniteNumber(x) or not isFiniteNumber(y) or not isFiniteNumber(z) then
        player:SetAttribute("LastActionResult", invalidResult)
        return nil
    end

    return Vector3.new(x, y, z)
end

local function pickupBlock(player, inputData)
    if heldBlocksByPlayer[player] then
        player:SetAttribute("LastActionResult", "PickupFailed:AlreadyHolding")
        return
    end

    local targetPosition = parseInputPosition(
        player,
        inputData,
        "PickupFailed:MissingPosition",
        "PickupFailed:InvalidPosition"
    )
    if not targetPosition then
        return
    end

    if not canReach(player, targetPosition, PICKUP_RANGE) then
        player:SetAttribute("LastActionResult", "PickupFailed:OutOfRange")
        return
    end

    local block = findNearestBlock(targetPosition, 1.5)
    if not block then
        player:SetAttribute("LastActionResult", "PickupFailed:NoBlock")
        return
    end

    heldBlocksByPlayer[player] = block
    block.CanCollide = false
    setPlayerBlockState(player, block)
    player:SetAttribute("LastActionResult", "PickupSuccess")
end

local function positionOccupied(player, targetPosition)
    local params = OverlapParams.new()
    params.FilterType = Enum.RaycastFilterType.Exclude
    params.FilterDescendantsInstances = { player.Character, heldBlocksByPlayer[player] }

    local hits = Workspace:GetPartBoundsInBox(
        CFrame.new(targetPosition),
        Vector3.new(BLOCK_SIZE, BLOCK_SIZE, BLOCK_SIZE),
        params
    )

    for _, hit in ipairs(hits) do
        if hit:IsA("Part") then
            return true
        end
    end

    return false
end

local function placeBlock(player, inputData)
    local block = heldBlocksByPlayer[player]
    if not block then
        player:SetAttribute("LastActionResult", "PlaceFailed:EmptyHands")
        return
    end

    local targetPosition = parseInputPosition(
        player,
        inputData,
        "PlaceFailed:MissingPosition",
        "PlaceFailed:InvalidPosition"
    )
    if not targetPosition then
        return
    end

    if not canReach(player, targetPosition, PLACE_RANGE) then
        player:SetAttribute("LastActionResult", "PlaceFailed:OutOfRange")
        return
    end

    if positionOccupied(player, targetPosition) then
        player:SetAttribute("LastActionResult", "PlaceFailed:Occupied")
        return
    end

    local startPos = block.Position
    block.Anchored = true
    block.CanCollide = false
    heldBlocksByPlayer[player] = nil
    setPlayerBlockState(player, nil)

    table.insert(placingBlocks, {
        block = block,
        startPos = startPos,
        targetPos = targetPosition,
        elapsed = 0,
        duration = 0.3,
    })

    player:SetAttribute("LastActionResult", "PlaceSuccess")
end

if AgentInputService then
    AgentInputService.InputReceived:Connect(function(player, inputType, inputData)
        local humanoid = getHumanoid(player)
        if not humanoid then return end

        local ok, err = pcall(function()
            if inputType == "MoveTo" then
                local targetPosition = parseInputPosition(
                    player,
                    inputData,
                    "MoveToFailed:MissingPosition",
                    "MoveToFailed:InvalidPosition"
                )
                if not targetPosition then
                    return
                end
                humanoid:MoveTo(targetPosition)
                player:SetAttribute("LastActionResult", "MoveToAccepted")
            elseif inputType == "Stop" then
                humanoid:CancelMoveTo()
                player:SetAttribute("LastActionResult", "StopSuccess")
            elseif inputType == "Jump" then
                humanoid.Jump = true
                player:SetAttribute("LastActionResult", "JumpSuccess")
            elseif inputType == "Pickup" then
                pickupBlock(player, inputData)
            elseif inputType == "Place" then
                placeBlock(player, inputData)
            else
                player:SetAttribute("LastActionResult", "InputIgnored:UnknownType")
            end
        end)

        if not ok then
            warn("[InputHandler] Failed to process input type '" .. tostring(inputType) .. "': " .. tostring(err))
            player:SetAttribute("LastActionResult", "InputFailed:InternalError")
        end
    end)
end

--------------------------------------------------------------------------------
-- MAIN LOOP
--------------------------------------------------------------------------------

RunService.Heartbeat:Connect(function(dt)
    for _, player in ipairs(Players:GetPlayers()) do
        local hrp = getHumanoidRootPart(player)
        if hrp and hrp.Position.Y < OUT_OF_BOUNDS_Y then
            resetPlayerToSpawn(player)
        end
    end

    -- Update held blocks
    for player, heldBlock in pairs(heldBlocksByPlayer) do
        local hrp = getHumanoidRootPart(player)
        if hrp and heldBlock then
            heldBlock.Position = hrp.Position + Vector3.new(0, 3.5, 0)
        end
    end

    -- Animate placing blocks
    local i = 1
    while i <= #placingBlocks do
        local p = placingBlocks[i]
        p.elapsed = p.elapsed + dt
        local alpha = math.min(p.elapsed / p.duration, 1)
        -- Smooth ease-out
        local t = 1 - (1 - alpha) * (1 - alpha)
        p.block.Position = p.startPos:Lerp(p.targetPos, t)

        if alpha >= 1 then
            p.block.Anchored = false
            p.block.CanCollide = true
            setBlockMetadata(p.block, p.block:GetAttribute("ItemColorName"))
            table.remove(placingBlocks, i)
        else
            i = i + 1
        end
    end

    for block, _ in pairs(trackedBlocks) do
        if not block.Parent then
            trackedBlocks[block] = nil
        elseif block.Position.Y < OUT_OF_BOUNDS_Y then
            destroyBlock(block)
        end
    end
end)

--------------------------------------------------------------------------------
-- INITIALIZATION
--------------------------------------------------------------------------------

Players.PlayerAdded:Connect(setupPlayer)
Players.PlayerRemoving:Connect(cleanupPlayer)

for _, player in ipairs(Players:GetPlayers()) do
    setupPlayer(player)
end

print("=== Mesa World ===")

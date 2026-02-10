local Players = game:GetService("Players")
local AgentInputService = game:GetService("AgentInputService")
local Workspace = game:GetService("Workspace")

local MAP_SIZE = 240
local SPAWNS = {
    Vector3.new(-40, 3, 0),
    Vector3.new(40, 3, 0),
    Vector3.new(0, 3, -40),
    Vector3.new(0, 3, 40),
}

local function makePart(name, size, position, color, transparency)
    local p = Instance.new("Part")
    p.Name = name
    p.Size = size
    p.Position = position
    p.Color = color
    p.Transparency = transparency or 0
    p.Anchored = true
    p.Parent = Workspace
    return p
end

local function buildWorld()
    makePart(
        "Floor",
        Vector3.new(MAP_SIZE, 1, MAP_SIZE),
        Vector3.new(0, 0, 0),
        Color3.fromRGB(70, 90, 70)
    )

    -- Invisible bounds to keep bots in range.
    local half = MAP_SIZE * 0.5
    makePart("BoundN", Vector3.new(MAP_SIZE, 30, 2), Vector3.new(0, 15, -half), Color3.fromRGB(0, 0, 0), 1)
    makePart("BoundS", Vector3.new(MAP_SIZE, 30, 2), Vector3.new(0, 15, half), Color3.fromRGB(0, 0, 0), 1)
    makePart("BoundW", Vector3.new(2, 30, MAP_SIZE), Vector3.new(-half, 15, 0), Color3.fromRGB(0, 0, 0), 1)
    makePart("BoundE", Vector3.new(2, 30, MAP_SIZE), Vector3.new(half, 15, 0), Color3.fromRGB(0, 0, 0), 1)
end

local spawnIndex = 0
local function spawnPlayer(player)
    local char = player.Character
    if not char then
        return
    end
    local hrp = char:FindFirstChild("HumanoidRootPart")
    local humanoid = char:FindFirstChild("Humanoid")
    if humanoid then
        humanoid.WalkSpeed = 16
    end
    if hrp then
        spawnIndex = (spawnIndex % #SPAWNS) + 1
        local base = SPAWNS[spawnIndex]
        -- Match fps-arena convention: identity orientation with -Z forward.
        hrp.CFrame = CFrame.new(base)
        hrp:SetAttribute("ModelYawOffsetDeg", 180)
    end
end

AgentInputService.InputReceived:Connect(function(player, inputType, data)
    local char = player.Character
    if not char then
        return
    end
    local humanoid = char:FindFirstChild("Humanoid")
    if not humanoid then
        return
    end

    if inputType == "MoveTo" and data and data.position then
        local pos = data.position
        humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
    elseif inputType == "Stop" then
        humanoid:CancelMoveTo()
    elseif inputType == "Fire" then
        local shots = (player:GetAttribute("ShotsFired") or 0) + 1
        player:SetAttribute("ShotsFired", shots)
    end
end)

Players.PlayerAdded:Connect(function(player)
    player:SetAttribute("DebugMode", "raycast")
    player:SetAttribute("ShotsFired", 0)
    spawnPlayer(player)
end)

for _, player in ipairs(Players:GetPlayers()) do
    spawnPlayer(player)
end

buildWorld()
print("[raycast-debug] booted")

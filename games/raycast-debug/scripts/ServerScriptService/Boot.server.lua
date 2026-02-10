local Players = game:GetService("Players")
local AgentInputService = game:GetService("AgentInputService")
local Workspace = game:GetService("Workspace")
local RunService = game:GetService("RunService")

local AnimationService = require(script.Parent.AnimationService)

local MAP_SIZE = 240
local DEFAULT_WEAPON_ID = 2
local FIRE_RATE = 0.09
local SPAWNS = {
    Vector3.new(-40, 3, 0),
    Vector3.new(40, 3, 0),
    Vector3.new(0, 3, -40),
    Vector3.new(0, 3, 40),
}
local playerCombatState = {}

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
        hrp.CFrame = CFrame.new(base)
        hrp.CanCollide = false
        hrp:SetAttribute("ModelYawOffsetDeg", 180)
    end
end

local function attachCharacter(player, character)
    spawnPlayer(player)
    AnimationService.BindCharacter(player, character)

    local humanoid = character:FindFirstChild("Humanoid")
    if humanoid then
        humanoid.Died:Connect(function()
            AnimationService.StopAll(player)
        end)
    end
end

Players.PlayerAdded:Connect(function(player)
    player:SetAttribute("DebugMode", "raycast")
    player:SetAttribute("ShotsFired", 0)
    player:SetAttribute("WeaponSlot", DEFAULT_WEAPON_ID)
    playerCombatState[player.UserId] = { lastShotAt = 0 }

    player.CharacterAdded:Connect(function(character)
        attachCharacter(player, character)
    end)
    if player.Character then
        attachCharacter(player, player.Character)
    end
end)

Players.PlayerRemoving:Connect(function(player)
    playerCombatState[player.UserId] = nil
end)

for _, player in ipairs(Players:GetPlayers()) do
    if player.Character then
        attachCharacter(player, player.Character)
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
        local moveDir = humanoid.MoveDirection
        local moveMag = math.sqrt(moveDir.X * moveDir.X + moveDir.Z * moveDir.Z)
        if moveMag <= 0.01 then
            return
        end

        local now = tick()
        local cstate = playerCombatState[player.UserId]
        if not cstate then
            cstate = { lastShotAt = 0 }
            playerCombatState[player.UserId] = cstate
        end
        if now - cstate.lastShotAt < FIRE_RATE then
            return
        end
        cstate.lastShotAt = now

        local shots = (player:GetAttribute("ShotsFired") or 0) + 1
        player:SetAttribute("ShotsFired", shots)
        local weaponId = player:GetAttribute("WeaponSlot")
        if type(weaponId) ~= "number" then
            weaponId = DEFAULT_WEAPON_ID
        end
        AnimationService.PlayFire(player, weaponId)
    end
end)

AnimationService.Init()
buildWorld()

RunService.Heartbeat:Connect(function(_dt)
    AnimationService.Tick()
end)

print("[raycast-debug] booted")

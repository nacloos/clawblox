local function makePart(name, size, position, color)
    local part = Instance.new("Part")
    part.Name = name
    part.Size = size
    part.Position = position
    part.Anchored = true
    part.Color = color
    part.Parent = Workspace
    return part
end

makePart("Ground", Vector3.new(220, 1, 220), Vector3.new(0, 0, 0), Color3.new(0.2, 0.22, 0.24))

local GameState = Instance.new("Folder")
GameState.Name = "GameState"
GameState:SetAttribute("Mode", "AnimationSmoke")
GameState:SetAttribute("Direction", "ForwardZ")
GameState:SetAttribute("Shots", 0)
GameState.Parent = Workspace

local playerTracks = {}
local AgentInputService = game:GetService("AgentInputService")

local function getFireTrack(player)
    local existing = playerTracks[player]
    if existing then
        return existing
    end

    local character = player.Character
    if not character then
        return nil
    end

    local humanoid = character:FindFirstChild("Humanoid")
    if not humanoid then
        return nil
    end

    local animation = Instance.new("Animation")
    animation.AnimationId = "local://fire_rifle"
    local ok, track = pcall(function()
        return humanoid:LoadAnimation(animation)
    end)
    if not ok or not track then
        return nil
    end

    playerTracks[player] = track
    return track
end

AgentInputService.InputReceived:Connect(function(player, inputType, _data)
    if inputType ~= "Fire" then
        return
    end

    local track = getFireTrack(player)
    if not track then
        return
    end

    pcall(function()
        track:Play()
    end)

    local shots = (GameState:GetAttribute("Shots") or 0) + 1
    GameState:SetAttribute("Shots", shots)
    player:SetAttribute("ShotsFired", shots)
end)

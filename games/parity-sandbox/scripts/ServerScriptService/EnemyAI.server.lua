local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
if not ModulesFolder then
    warn("Modules folder missing")
    return
end

local Config = require(ModulesFolder:WaitForChild("Config", 2))
local State = require(ModulesFolder:WaitForChild("State", 2))
local Enemies = require(ModulesFolder:WaitForChild("Enemies", 2))
local Markers = require(ModulesFolder:WaitForChild("Markers", 2))

local function getPrimaryPlayerHumanoidData()
    local players = Players:GetPlayers()
    if #players == 0 then
        return nil, nil, nil
    end

    local player = players[1]
    local character = player.Character
    if not character then
        return player, nil, nil
    end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    local humanoid = character:FindFirstChild("Humanoid")
    return player, hrp, humanoid
end

RunService.Heartbeat:Connect(function(dt)
    if State.phase ~= "Active" then
        return
    end

    local player, player_hrp, player_humanoid = getPrimaryPlayerHumanoidData()
    if not player or not player_hrp or not player_humanoid then
        return
    end

    local now = State.now()
    local step_dt = tonumber(dt) or 0.016

    Enemies.forEachAlive(function(enemy)
        local root = enemy.root
        if not root then
            return
        end

        local delta = player_hrp.Position - root.Position
        local dist = delta.Magnitude
        if dist > 0.001 then
            local speed = tonumber(enemy.speed) or Config.ENEMIES.SPEED
            local step = math.min(speed * step_dt, math.max(0, dist - 0.4))
            local next_pos = root.Position + delta.Unit * step
            root.Position = next_pos
        end

        if dist <= Config.ENEMIES.CONTACT_RANGE then
            if now - (enemy.last_contact_at or 0) >= Config.ENEMIES.CONTACT_COOLDOWN then
                enemy.last_contact_at = now
                player_humanoid:TakeDamage(Config.ENEMIES.CONTACT_DAMAGE)
                Markers.set(Config.MARKERS.Combat, "LastPlayerDamageTaken", Config.ENEMIES.CONTACT_DAMAGE)
            end
        end
    end)

    if player_humanoid.Health <= 0 and State.phase == "Active" then
        State.setPhase("Failed")
        Markers.set(Config.MARKERS.Round, "FailureReason", "PlayerDied")
    end
end)

local Players = game:GetService("Players")

local SpawnService = {}

local config
local map
local state

local function distanceSq(a, b)
    local dx = a.X - b.X
    local dz = a.Z - b.Z
    return dx * dx + dz * dz
end

local function getHumanoid(character)
    if not character then
        return nil
    end
    return character:FindFirstChild("Humanoid")
end

local function getRoot(character)
    if not character then
        return nil
    end
    return character:FindFirstChild("HumanoidRootPart")
end

local function chooseSpawnPosition()
    local points = map.GetSpawnPoints()
    if #points == 0 then
        return Vector3.new(0, 3, 0)
    end

    local best = points[1].Position + Vector3.new(0, 3, 0)
    local bestMinDist = -1

    for _, spawn in ipairs(points) do
        local pos = spawn.Position + Vector3.new(0, 3, 0)
        local minDist = 1e9

        for _, player in ipairs(Players:GetPlayers()) do
            local pdata = state.GetPlayer(player)
            if pdata and pdata.alive and player.Character then
                local root = getRoot(player.Character)
                if root then
                    local d = distanceSq(root.Position, pos)
                    if d < minDist then
                        minDist = d
                    end
                end
            end
        end

        if minDist > bestMinDist then
            bestMinDist = minDist
            best = pos
        end
    end

    return best
end

function SpawnService.Init(deps)
    config = deps.config
    map = deps.map
    state = deps.state
end

function SpawnService.ApplyCharacterStats(player, character)
    local pdata = state.GetPlayer(player)
    if not pdata then
        return
    end

    local humanoid = getHumanoid(character)
    if humanoid then
        humanoid.MaxHealth = pdata.maxHealth
        humanoid.Health = pdata.health
    end
end

function SpawnService.SpawnPlayer(player)
    local pdata = state.GetPlayer(player)
    if not pdata then
        return
    end

    player:LoadCharacter()
    local character = player.Character
    if not character then
        return
    end

    local root = getRoot(character)
    local humanoid = getHumanoid(character)
    if not root or not humanoid then
        return
    end

    local spawnPos = chooseSpawnPosition()
    root.Position = spawnPos

    pdata.health = pdata.maxHealth
    pdata.alive = true
    pdata.pendingRespawnAt = nil

    humanoid.MaxHealth = pdata.maxHealth
    humanoid.Health = pdata.maxHealth
end

function SpawnService.ScheduleRespawn(player, now)
    local pdata = state.GetPlayer(player)
    if not pdata then
        return
    end
    pdata.alive = false
    pdata.pendingRespawnAt = now + config.RESPAWN_DELAY
end

function SpawnService.TryProcessRespawns(now)
    for _, player in ipairs(Players:GetPlayers()) do
        local pdata = state.GetPlayer(player)
        if pdata and pdata.pendingRespawnAt and now >= pdata.pendingRespawnAt then
            SpawnService.SpawnPlayer(player)
        end
    end
end

return SpawnService

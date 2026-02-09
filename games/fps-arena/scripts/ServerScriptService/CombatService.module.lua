local Players = game:GetService("Players")

local CombatService = {}

local config
local state
local spawnService
local roundService

local function vecFromTriplet(v)
    if type(v) ~= "table" then
        return nil
    end
    if type(v[1]) ~= "number" or type(v[2]) ~= "number" or type(v[3]) ~= "number" then
        return nil
    end
    return Vector3.new(v[1], v[2], v[3])
end

local function getCharacter(player)
    return player and player.Character or nil
end

local function getRoot(character)
    if not character then
        return nil
    end
    return character:FindFirstChild("HumanoidRootPart")
end

local function getHumanoid(character)
    if not character then
        return nil
    end
    return character:FindFirstChild("Humanoid")
end

local function findCharacterFromPart(part)
    local cursor = part
    while cursor and cursor ~= Workspace do
        if cursor:IsA("Model") and cursor:FindFirstChild("Humanoid") then
            return cursor
        end
        cursor = cursor.Parent
    end
    return nil
end

local function canFire(pdata, now)
    if not pdata.alive then
        return false
    end
    if now - pdata.lastShotAt < config.FIRE_COOLDOWN then
        return false
    end
    return true
end

function CombatService.Init(deps)
    config = deps.config
    state = deps.state
    spawnService = deps.spawnService
    roundService = deps.roundService
end

function CombatService.HandleFire(player, payload)
    local round = state.GetRound()
    if round.phase ~= "active" then
        return
    end

    local pdata = state.GetPlayer(player)
    if not pdata then
        return
    end

    local now = tick()
    if not canFire(pdata, now) then
        return
    end

    local target = payload and vecFromTriplet(payload.target)
    if not target then
        return
    end

    local character = getCharacter(player)
    local root = getRoot(character)
    if not character or not root then
        return
    end

    local origin = root.Position + Vector3.new(0, 1.5, 0)
    local toTarget = target - origin
    local dist = toTarget.Magnitude
    if dist <= 0.01 then
        return
    end

    local shotDist = math.min(dist, config.FIRE_RANGE)
    local direction = toTarget.Unit * shotDist

    pdata.lastShotAt = now

    local hit = Workspace:Raycast(origin, direction)
    if not hit or not hit.Instance then
        return
    end

    local hitCharacter = findCharacterFromPart(hit.Instance)
    if not hitCharacter then
        return
    end

    if hitCharacter == character then
        return
    end

    local victim = Players:GetPlayerFromCharacter(hitCharacter)
    if not victim then
        return
    end

    local vdata = state.GetPlayer(victim)
    if not vdata or not vdata.alive then
        return
    end

    local victimHumanoid = getHumanoid(hitCharacter)
    if not victimHumanoid then
        return
    end

    local before = vdata.health
    if before <= 0 then
        return
    end

    local newHealth = math.max(0, before - config.FIRE_DAMAGE)
    vdata.health = newHealth
    victimHumanoid.Health = newHealth

    if newHealth <= 0 then
        vdata.alive = false
        vdata.deaths = vdata.deaths + 1

        pdata.kills = pdata.kills + 1
        pdata.score = pdata.score + 100

        spawnService.ScheduleRespawn(victim, now)
        roundService.OnElimination(player, victim)
    end
end

return CombatService

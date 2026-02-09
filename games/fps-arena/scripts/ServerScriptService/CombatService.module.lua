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

local function weaponDef(id)
    return config.WEAPONS[id]
end

local function weaponState(pdata)
    return pdata.weapons and pdata.weapons[pdata.weaponId] or nil
end

local function syncWeaponFields(pdata)
    state.SyncWeaponFields(pdata, config)
end

local function canFire(pdata, wdef, now)
    if not pdata.alive then
        return false
    end
    local wstate = weaponState(pdata)
    if not wstate or wstate.reloading then
        return false
    end
    if now - pdata.lastShotAt < wdef.fire_rate then
        return false
    end
    return true
end

local function startReload(pdata, now)
    local wdef = weaponDef(pdata.weaponId)
    local wstate = weaponState(pdata)
    if not wdef or not wstate then
        return
    end
    if wstate.reloading then
        return
    end
    if wstate.reserve <= 0 then
        return
    end
    if wstate.mag >= wdef.mag_size then
        return
    end
    wstate.reloading = true
    wstate.reloadEndAt = now + wdef.reload_time
    syncWeaponFields(pdata)
end

local function finishReload(pdata)
    local wdef = weaponDef(pdata.weaponId)
    local wstate = weaponState(pdata)
    if not wdef or not wstate then
        return
    end
    if not wstate.reloading then
        return
    end
    local needed = wdef.mag_size - wstate.mag
    local take = math.min(needed, wstate.reserve)
    wstate.mag = wstate.mag + take
    wstate.reserve = wstate.reserve - take
    wstate.reloading = false
    wstate.reloadEndAt = nil
    syncWeaponFields(pdata)
end

local function switchWeapon(player, pdata, slot)
    local id = math.floor(slot)
    if id < 1 or not weaponDef(id) or not pdata.weapons[id] then
        return
    end
    local prev = weaponState(pdata)
    if prev then
        prev.reloading = false
        prev.reloadEndAt = nil
    end
    pdata.weaponId = id
    syncWeaponFields(pdata)
    spawnService.SetWeaponSlot(player, pdata.weaponId)
end

local function randomSpreadDirection(baseDir, spread)
    local upAxis = Vector3.new(0, 1, 0)
    local right = baseDir:Cross(upAxis)
    if right.Magnitude < 0.001 then
        right = baseDir:Cross(Vector3.new(1, 0, 0))
    end
    right = right.Unit
    local up = right:Cross(baseDir).Unit

    local jitterX = (math.random() - 0.5) * spread
    local jitterY = (math.random() - 0.5) * spread
    return (baseDir + right * jitterX + up * jitterY).Unit
end

local function damageVictim(victim, attackerPdata, wdef)
    local vdata = state.GetPlayer(victim.player)
    if not vdata or not vdata.alive then
        return false
    end

    local before = vdata.health
    if before <= 0 then
        return false
    end

    local damage = math.floor(wdef.damage * (1 + math.random() * 0.2))
    local newHealth = math.max(0, before - damage)
    vdata.health = newHealth
    victim.humanoid.Health = newHealth

    if newHealth <= 0 then
        vdata.alive = false
        vdata.deaths = vdata.deaths + 1

        attackerPdata.kills = attackerPdata.kills + 1
        attackerPdata.score = attackerPdata.score + (wdef.kill_score or 100)

        spawnService.ScheduleRespawn(victim.player, tick())
        roundService.OnElimination(victim.attackerPlayer, victim.player)
    end
    return true
end

function CombatService.Init(deps)
    config = deps.config
    state = deps.state
    spawnService = deps.spawnService
    roundService = deps.roundService
end

function CombatService.HandleSwitchWeapon(player, payload)
    local pdata = state.GetPlayer(player)
    if not pdata then
        return
    end

    local slot = nil
    if type(payload) == "number" then
        slot = payload
    elseif type(payload) == "table" then
        slot = payload.weapon or payload.slot or payload.index
    end
    if type(slot) ~= "number" then
        return
    end
    switchWeapon(player, pdata, slot)
end

function CombatService.HandleReload(player)
    local pdata = state.GetPlayer(player)
    if not pdata then
        return
    end
    startReload(pdata, tick())
end

function CombatService.Tick(now)
    state.ForEachPlayer(function(_, pdata)
        local wstate = weaponState(pdata)
        if wstate and wstate.reloading and wstate.reloadEndAt and now >= wstate.reloadEndAt then
            finishReload(pdata)
        end
    end)
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

    local wdef = weaponDef(pdata.weaponId)
    local wstate = weaponState(pdata)
    if not wdef or not wstate then
        return
    end

    local now = tick()
    if not canFire(pdata, wdef, now) then
        return
    end

    if wstate.mag <= 0 then
        startReload(pdata, now)
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

    pdata.lastShotAt = now
    wstate.mag = wstate.mag - 1
    syncWeaponFields(pdata)

    local baseDir = toTarget.Unit
    local pellets = wdef.pellets or 1

    local rayParams = RaycastParams.new()
    rayParams.FilterType = Enum.RaycastFilterType.Blacklist
    rayParams.FilterDescendantsInstances = { character }

    for _ = 1, pellets do
        local dir = randomSpreadDirection(baseDir, wdef.spread or 0)
        local rayDir = dir * config.FIRE_RANGE
        local hit = Workspace:Raycast(origin, rayDir, rayParams)
        if hit and hit.Instance then
            local hitCharacter = findCharacterFromPart(hit.Instance)
            if hitCharacter and hitCharacter ~= character then
                local victimPlayer = Players:GetPlayerFromCharacter(hitCharacter)
                if victimPlayer then
                    local victimHumanoid = getHumanoid(hitCharacter)
                    if victimHumanoid then
                        damageVictim({
                            player = victimPlayer,
                            humanoid = victimHumanoid,
                            attackerPlayer = player,
                        }, pdata, wdef)
                    end
                end
            end
        end
    end
end

return CombatService

local State = {}

local playerState = {}
local round = {
    phase = "waiting",
    startedAt = 0,
    winnerUserId = nil,
    winnerName = nil,
}

local function buildWeaponStates(config)
    local weapons = {}
    for _, def in ipairs(config.WEAPONS) do
        weapons[def.id] = {
            mag = def.mag_size,
            reserve = def.reserve,
            reloading = false,
            reloadEndAt = nil,
        }
    end
    return weapons
end

local function syncActiveWeaponFields(pdata, config)
    local wid = pdata.weaponId
    local wdef = config.WEAPONS[wid]
    local wstate = pdata.weapons[wid]
    if not wdef or not wstate then
        return
    end
    pdata.weaponName = wdef.name
    pdata.ammo = wstate.mag
    pdata.ammoReserve = wstate.reserve
end

function State.AddPlayer(player, config)
    local weapons = buildWeaponStates(config)
    local weaponId = config.DEFAULT_WEAPON_ID
    if not weapons[weaponId] then
        weaponId = 1
    end
    playerState[player.UserId] = {
        userId = player.UserId,
        name = player.Name,
        health = config.PLAYER_MAX_HEALTH,
        maxHealth = config.PLAYER_MAX_HEALTH,
        kills = 0,
        deaths = 0,
        score = 0,
        weaponId = weaponId,
        weapons = weapons,
        ammo = 0,
        ammoReserve = 0,
        weaponName = "",
        alive = false,
        lastShotAt = -1e9,
        pendingRespawnAt = nil,
    }
    syncActiveWeaponFields(playerState[player.UserId], config)
end

function State.RemovePlayer(player)
    playerState[player.UserId] = nil
end

function State.GetPlayer(player)
    return playerState[player.UserId]
end

function State.ForEachPlayer(fn)
    for userId, data in pairs(playerState) do
        fn(userId, data)
    end
end

function State.GetRound()
    return round
end

function State.SetRoundPhase(phase)
    round.phase = phase
end

function State.StartRound(now)
    round.phase = "active"
    round.startedAt = now
    round.winnerUserId = nil
    round.winnerName = nil

    for _, data in pairs(playerState) do
        data.health = data.maxHealth
        data.alive = false
        data.lastShotAt = -1e9
        data.pendingRespawnAt = now
        data.weapons = buildWeaponStates(config)
        data.weaponId = config.DEFAULT_WEAPON_ID
        if not data.weapons[data.weaponId] then
            data.weaponId = 1
        end
        syncActiveWeaponFields(data, config)
    end
end

function State.FinishRound(winner)
    round.phase = "finished"
    round.winnerUserId = winner and winner.userId or nil
    round.winnerName = winner and winner.name or nil
end

function State.SyncWeaponFields(pdata, config)
    syncActiveWeaponFields(pdata, config)
end

return State

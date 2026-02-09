local State = {}

local playerState = {}
local round = {
    phase = "waiting",
    startedAt = 0,
    winnerUserId = nil,
    winnerName = nil,
}

function State.AddPlayer(player, config)
    playerState[player.UserId] = {
        userId = player.UserId,
        name = player.Name,
        health = config.PLAYER_MAX_HEALTH,
        maxHealth = config.PLAYER_MAX_HEALTH,
        kills = 0,
        deaths = 0,
        score = 0,
        ammo = config.MAG_SIZE,
        ammoReserve = config.RESERVE_AMMO,
        weaponName = config.WEAPON_NAME,
        alive = false,
        lastShotAt = -1e9,
        pendingRespawnAt = nil,
    }
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
    end
end

function State.FinishRound(winner)
    round.phase = "finished"
    round.winnerUserId = winner and winner.userId or nil
    round.winnerName = winner and winner.name or nil
end

return State

local Stats = {
    boot_count = 0,
    server_ticks = 0,
    joins = 0,
    leaves = 0,
    scores = {},
    names = {},
    winner_user_id = nil,
    winner_name = nil,
}

function Stats.incrementBoot()
    Stats.boot_count = Stats.boot_count + 1
    return Stats.boot_count
end

function Stats.incrementTick()
    Stats.server_ticks = Stats.server_ticks + 1
    return Stats.server_ticks
end

function Stats.playerJoined(player)
    Stats.joins = Stats.joins + 1
    local userId = player.UserId
    Stats.names[userId] = player.Name
    if Stats.scores[userId] == nil then
        Stats.scores[userId] = 0
    end
end

function Stats.playerLeft(player)
    Stats.leaves = Stats.leaves + 1
end

function Stats.activePlayers()
    return #Players:GetPlayers()
end

function Stats.getScore(userId)
    return Stats.scores[userId] or 0
end

function Stats.addScore(userId, amount)
    local nextScore = (Stats.scores[userId] or 0) + amount
    Stats.scores[userId] = nextScore
    return nextScore
end

function Stats.getLeader()
    local leaderId = nil
    local leaderName = nil
    local leaderScore = -1

    for _, player in ipairs(Players:GetPlayers()) do
        local userId = player.UserId
        local score = Stats.getScore(userId)
        if score > leaderScore then
            leaderScore = score
            leaderId = userId
            leaderName = player.Name
        end
    end

    if leaderId == nil then
        return nil, nil, 0
    end
    return leaderId, leaderName, leaderScore
end

function Stats.setWinner(userId, name)
    Stats.winner_user_id = userId
    Stats.winner_name = name
end

return Stats

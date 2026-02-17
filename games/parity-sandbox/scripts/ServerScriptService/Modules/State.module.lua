local State = {
    phase = "Waiting",
    current_wave = 0,
    wave_active = false,
    next_wave_at = 0,
    server_ticks = 0,
    winner_name = "",
    winner_user_id = 0,
    enemies = {},
    next_enemy_id = 1,
    players = {},
    joins = 0,
    leaves = 0,
}

function State.now()
    return tick()
end

function State.incrementTick()
    State.server_ticks = State.server_ticks + 1
    return State.server_ticks
end

function State.setPhase(phase)
    State.phase = phase
end

function State.addPlayer(player)
    local user_id = tonumber(player.UserId) or 0
    if State.players[user_id] == nil then
        State.players[user_id] = {
            user_id = user_id,
            name = player.Name,
            weapon = "Rifle",
            last_shot_at = 0,
            fire_timestamps = {},
            kills = 0,
            damage_dealt = 0,
            shots_fired = 0,
            animation_tracks = {},
        }
    end
    State.joins = State.joins + 1
end

function State.removePlayer(player)
    local user_id = tonumber(player.UserId) or 0
    State.players[user_id] = nil
    State.leaves = State.leaves + 1
end

function State.getPlayerState(player)
    local user_id = tonumber(player.UserId) or 0
    return State.players[user_id]
end

function State.activePlayers()
    return #Players:GetPlayers()
end

function State.setWinner(user_id, name)
    State.winner_user_id = tonumber(user_id) or 0
    State.winner_name = name or ""
end

function State.resetWinner()
    State.winner_user_id = 0
    State.winner_name = ""
end

return State

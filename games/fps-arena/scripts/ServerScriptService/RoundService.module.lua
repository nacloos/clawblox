local Players = game:GetService("Players")

local RoundService = {}

local config
local state
local spawnService

local matchEndAt = nil

local function activePlayerCount()
    local count = 0
    for _ in ipairs(Players:GetPlayers()) do
        count = count + 1
    end
    return count
end

function RoundService.Init(deps)
    config = deps.config
    state = deps.state
    spawnService = deps.spawnService
end

local function finishRoundWithWinner(winner, now)
    state.FinishRound(winner)
    matchEndAt = now + 5
end

function RoundService.StartIfReady()
    local round = state.GetRound()
    if round.phase == "active" then
        return
    end

    if activePlayerCount() == 0 then
        state.SetRoundPhase("waiting")
        return
    end

    local now = tick()
    state.StartRound(now)
    matchEndAt = now + config.MATCH_DURATION

    for _, player in ipairs(Players:GetPlayers()) do
        spawnService.SpawnPlayer(player)
    end

    print("[fps-arena] round started")
end

function RoundService.OnElimination(attacker, _victim)
    local attackerState = state.GetPlayer(attacker)
    if not attackerState then
        return
    end

    if attackerState.kills >= config.KILL_LIMIT then
        finishRoundWithWinner(attackerState, tick())
        print("[fps-arena] winner:", attackerState.name)
    end
end

function RoundService.Tick(now)
    local round = state.GetRound()

    if round.phase == "waiting" then
        RoundService.StartIfReady()
        return
    end

    if round.phase == "active" then
        spawnService.TryProcessRespawns(now)

        if matchEndAt and now >= matchEndAt then
            local best = nil
            state.ForEachPlayer(function(_, pdata)
                if not best then
                    best = pdata
                elseif pdata.kills > best.kills then
                    best = pdata
                elseif pdata.kills == best.kills and pdata.score > best.score then
                    best = pdata
                end
            end)

            finishRoundWithWinner(best, now)
            print("[fps-arena] round ended by timer")
        end

        return
    end

    if round.phase == "finished" then
        -- Brief intermission before restarting the next round.
        if matchEndAt and now >= matchEndAt then
            matchEndAt = nil
            RoundService.StartIfReady()
        end
    end
end

return RoundService

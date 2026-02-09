local Players = game:GetService("Players")

local ReplicateService = {}

local config
local state

local gameStateFolder = nil

local function setPlayerAttrs(player, pdata)
    player:SetAttribute("Health", pdata.health)
    player:SetAttribute("MaxHealth", pdata.maxHealth)
    player:SetAttribute("Kills", pdata.kills)
    player:SetAttribute("Deaths", pdata.deaths)
    player:SetAttribute("Score", pdata.score)
    player:SetAttribute("WeaponName", pdata.weaponName)
    player:SetAttribute("Ammo", pdata.ammo)
    player:SetAttribute("AmmoReserve", pdata.ammoReserve)
    player:SetAttribute("IsAlive", pdata.alive)
end

local function setGameStateAttrs()
    if not gameStateFolder then
        gameStateFolder = Workspace:FindFirstChild("GameState")
    end
    if not gameStateFolder then
        return
    end

    local round = state.GetRound()
    gameStateFolder:SetAttribute("MatchState", round.phase)
    gameStateFolder:SetAttribute("KillLimit", config.KILL_LIMIT)

    if round.winnerUserId then
        gameStateFolder:SetAttribute("LeaderUserId", tostring(round.winnerUserId))
    else
        gameStateFolder:SetAttribute("LeaderUserId", nil)
    end
    if round.winnerName then
        gameStateFolder:SetAttribute("LeaderName", round.winnerName)
    else
        gameStateFolder:SetAttribute("LeaderName", nil)
    end

    local leaderName = nil
    local leaderKills = -1
    state.ForEachPlayer(function(_, pdata)
        if pdata.kills > leaderKills then
            leaderKills = pdata.kills
            leaderName = pdata.name
        end
    end)

    if leaderName and round.phase ~= "finished" then
        gameStateFolder:SetAttribute("LeaderName", leaderName)
    end

    local timeRemaining = 0
    if round.phase == "active" then
        local elapsed = math.max(0, tick() - round.startedAt)
        timeRemaining = math.max(0, config.MATCH_DURATION - elapsed)
    end
    gameStateFolder:SetAttribute("TimeRemaining", timeRemaining)
end

function ReplicateService.Init(deps)
    config = deps.config
    state = deps.state
end

function ReplicateService.Tick()
    for _, player in ipairs(Players:GetPlayers()) do
        local pdata = state.GetPlayer(player)
        if pdata then
            setPlayerAttrs(player, pdata)
        end
    end

    setGameStateAttrs()
end

return ReplicateService

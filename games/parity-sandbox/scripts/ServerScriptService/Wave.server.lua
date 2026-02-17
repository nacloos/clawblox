local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
if not ModulesFolder then
    warn("Modules folder missing")
    return
end

local Config = require(ModulesFolder:WaitForChild("Config", 2))
local State = require(ModulesFolder:WaitForChild("State", 2))
local Markers = require(ModulesFolder:WaitForChild("Markers", 2))
local Enemies = require(ModulesFolder:WaitForChild("Enemies", 2))

local function updateMarkers()
    Markers.setMany(Config.MARKERS.Loop, {
        ServerTicks = State.server_ticks,
        HeartbeatSamples = State.server_ticks,
    })

    Markers.setMany(Config.MARKERS.Wave, {
        CurrentWave = State.current_wave,
        AliveEnemies = Enemies.aliveCount(),
        MaxWaves = Config.WAVES.MAX_WAVES,
        WaveActive = State.wave_active,
    })

    Markers.setMany(Config.MARKERS.Round, {
        Phase = State.phase,
        ActivePlayers = State.activePlayers(),
        WinnerName = State.winner_name,
        WinnerUserId = State.winner_user_id,
        IsFinished = (State.phase == "Completed" or State.phase == "Failed"),
    })

    Markers.setMany(Config.MARKERS.PlayerStats, {
        JoinCount = State.joins,
        LeaveCount = State.leaves,
        ActivePlayers = State.activePlayers(),
    })
end

local function beginRound()
    State.current_wave = 0
    State.wave_active = false
    State.next_wave_at = State.now() + Config.WAVES.PREP_TIME
    State.resetWinner()
    State.setPhase("Prep")
    Enemies.clearAll()
end

local function spawnNextWave(now)
    State.current_wave = State.current_wave + 1
    State.wave_active = true
    State.setPhase("Active")
    local spawned = Enemies.spawnWave(State.current_wave)
    Markers.set(Config.MARKERS.Wave, "SpawnedThisWave", spawned)
    Markers.set(Config.MARKERS.Wave, "WaveStartedAt", now)
end

RunService.Heartbeat:Connect(function()
    local now = State.now()
    local ticks = State.incrementTick()

    if State.phase == "Waiting" and State.activePlayers() > 0 then
        beginRound()
    end

    if State.phase == "Prep" and now >= State.next_wave_at then
        spawnNextWave(now)
    elseif State.phase == "Active" then
        local alive = Enemies.aliveCount()
        if alive == 0 and State.wave_active then
            State.wave_active = false
            if State.current_wave >= Config.WAVES.MAX_WAVES then
                local players = Players:GetPlayers()
                if #players > 0 then
                    State.setWinner(players[1].UserId, players[1].Name)
                end
                State.setPhase("Completed")
            else
                State.setPhase("Intermission")
                State.next_wave_at = now + Config.WAVES.INTERMISSION_TIME
            end
        end
    elseif State.phase == "Intermission" and now >= State.next_wave_at then
        spawnNextWave(now)
    end

    if ticks % Config.COMBAT.LOOP_MARKER_EVERY_TICKS == 0 then
        updateMarkers()
    end
end)

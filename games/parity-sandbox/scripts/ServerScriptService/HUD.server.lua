local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
if not ModulesFolder then
    warn("Modules folder missing")
    return
end

local Config = require(ModulesFolder:WaitForChild("Config", 2))
local Hud = require(ModulesFolder:WaitForChild("Hud", 2))
local State = require(ModulesFolder:WaitForChild("State", 2))
local Markers = require(ModulesFolder:WaitForChild("Markers", 2))
local Enemies = require(ModulesFolder:WaitForChild("Enemies", 2))

local function updateAllPlayersHud()
    for _, player in ipairs(Players:GetPlayers()) do
        Hud.update(player)
    end
end

Players.PlayerAdded:Connect(function(player)
    Hud.ensure(player)
    task.delay(0.1, function()
        Hud.update(player)
    end)
end)

Players.PlayerRemoving:Connect(function(player)
    Hud.clear(player)
end)

RunService.Heartbeat:Connect(function()
    if State.server_ticks % 3 ~= 0 then
        return
    end

    updateAllPlayersHud()
    Markers.setMany(Config.MARKERS.Perf, {
        HudPlayers = #Players:GetPlayers(),
        HudWave = State.current_wave,
        HudEnemies = Enemies.aliveCount(),
    })
end)

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")

local Config = require(script.Parent.Config)
local State = require(script.Parent.State)
local Map = require(script.Parent.Map)
local SpawnService = require(script.Parent.SpawnService)
local CombatService = require(script.Parent.CombatService)
local InputService = require(script.Parent.InputService)
local RoundService = require(script.Parent.RoundService)
local ReplicateService = require(script.Parent.ReplicateService)

Map.Build(Config)

SpawnService.Init({
    config = Config,
    map = Map,
    state = State,
})

RoundService.Init({
    config = Config,
    state = State,
    spawnService = SpawnService,
})

CombatService.Init({
    config = Config,
    state = State,
    spawnService = SpawnService,
    roundService = RoundService,
})

InputService.Init({
    state = State,
    combatService = CombatService,
})

ReplicateService.Init({
    config = Config,
    state = State,
})

local function onPlayerAdded(player)
    State.AddPlayer(player, Config)

    player.CharacterAdded:Connect(function(character)
        SpawnService.ApplyCharacterStats(player, character)

        local humanoid = character:FindFirstChild("Humanoid")
        if humanoid then
            humanoid.Died:Connect(function()
                local pdata = State.GetPlayer(player)
                if pdata and pdata.alive then
                    pdata.alive = false
                    pdata.health = 0
                    pdata.deaths = pdata.deaths + 1
                    SpawnService.ScheduleRespawn(player, tick())
                end
            end)
        end
    end)

    RoundService.StartIfReady()
end

local function onPlayerRemoving(player)
    State.RemovePlayer(player)
end

Players.PlayerAdded:Connect(onPlayerAdded)
Players.PlayerRemoving:Connect(onPlayerRemoving)

for _, player in ipairs(Players:GetPlayers()) do
    onPlayerAdded(player)
end

RunService.Heartbeat:Connect(function(_dt)
    local now = tick()
    RoundService.Tick(now)
    ReplicateService.Tick()
end)

print("[fps-arena] boot complete")

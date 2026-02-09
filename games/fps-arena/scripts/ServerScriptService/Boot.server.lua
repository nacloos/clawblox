local RunService = game:GetService("RunService")
local Players = game:GetService("Players")

local Config = require(script.Parent.Config)
local State = require(script.Parent.State)
local Map = require(script.Parent.Map)
local SpawnService = require(script.Parent.SpawnService)
local CombatService = require(script.Parent.CombatService)
local AnimationService = require(script.Parent.AnimationService)
local InputService = require(script.Parent.InputService)
local RoundService = require(script.Parent.RoundService)
local ReplicateService = require(script.Parent.ReplicateService)

Map.Build(Config)

AnimationService.Init()

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
    animationService = AnimationService,
})

InputService.Init({
    state = State,
    combatService = CombatService,
})

ReplicateService.Init({
    config = Config,
    state = State,
})

local function attachCharacter(player, character)
    SpawnService.ApplyCharacterStats(player, character)
    AnimationService.BindCharacter(player, character)

    local humanoid = character:FindFirstChild("Humanoid")
    if humanoid then
        humanoid.Died:Connect(function()
            local pdata = State.GetPlayer(player)
            AnimationService.StopAll(player)
            if pdata and pdata.alive then
                pdata.alive = false
                pdata.health = 0
                pdata.deaths = pdata.deaths + 1
                SpawnService.ScheduleRespawn(player, tick())
            end
        end)
    end
end

local function onPlayerAdded(player)
    State.AddPlayer(player, Config)

    player.CharacterAdded:Connect(function(character)
        attachCharacter(player, character)
    end)
    if player.Character then
        attachCharacter(player, player.Character)
    end

    local round = State.GetRound()
    if round.phase == "active" then
        -- Late join during an active match: spawn immediately so inputs are accepted.
        SpawnService.SpawnPlayer(player)
    else
        RoundService.StartIfReady()
    end
end

local function onPlayerRemoving(player)
    AnimationService.UnbindPlayer(player)
    SpawnService.RemovePlayer(player)
    State.RemovePlayer(player)
end

Players.PlayerAdded:Connect(onPlayerAdded)
Players.PlayerRemoving:Connect(onPlayerRemoving)

for _, player in ipairs(Players:GetPlayers()) do
    onPlayerAdded(player)
end

RunService.Heartbeat:Connect(function(_dt)
    local now = tick()
    SpawnService.Tick()
    CombatService.Tick(now)
    RoundService.Tick(now)
    ReplicateService.Tick()
end)

print("[fps-arena] boot complete")

local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
if not ModulesFolder then
    warn("Modules folder missing")
    return
end

local Config = require(ModulesFolder:WaitForChild("Config", 2))
local State = require(ModulesFolder:WaitForChild("State", 2))
local Markers = require(ModulesFolder:WaitForChild("Markers", 2))
local Enemies = require(ModulesFolder:WaitForChild("Enemies", 2))

local function setupPlayerAttributes(player)
    player:SetAttribute("CurrentWeapon", "Rifle")
    player:SetAttribute("Kills", 0)
    player:SetAttribute("DamageDealt", 0)
    player:SetAttribute("ShotsFired", 0)
    player:SetAttribute("LastHitEnemyId", 0)
end

local function moveCharacterToSpawn(player)
    task.spawn(function()
        local character = player.Character or player.CharacterAdded:Wait()
        if not character then
            return
        end
        local hrp = character:WaitForChild("HumanoidRootPart", 2)
        if hrp then
            hrp.Position = Config.SPAWN.Position
        end
    end)
end

Players.PlayerAdded:Connect(function(player)
    State.addPlayer(player)
    setupPlayerAttributes(player)
    moveCharacterToSpawn(player)

    local marker = Markers.getOrCreate(Config.MARKERS.PlayerStats)
    marker:SetAttribute("LastJoinedUserId", player.UserId)
    marker:SetAttribute("LastJoinedName", player.Name)

    if State.phase == "Failed" or State.phase == "Completed" then
        State.setPhase("Waiting")
        State.current_wave = 0
        State.wave_active = false
        State.next_wave_at = 0
        State.resetWinner()
        Enemies.clearAll()
    end
end)

Players.PlayerRemoving:Connect(function(player)
    State.removePlayer(player)

    local marker = Markers.getOrCreate(Config.MARKERS.PlayerStats)
    marker:SetAttribute("LastLeftUserId", player.UserId)
    marker:SetAttribute("LastLeftName", player.Name)

    if State.activePlayers() == 0 then
        State.setPhase("Waiting")
        State.current_wave = 0
        State.wave_active = false
        State.next_wave_at = 0
        State.resetWinner()
        Enemies.clearAll()
    end
end)

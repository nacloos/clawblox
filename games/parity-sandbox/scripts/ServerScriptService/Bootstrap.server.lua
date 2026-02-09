local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
if not ModulesFolder then
    warn("Modules folder missing")
    return
end

math.randomseed(1337)

local Config = require(ModulesFolder:WaitForChild("Config", 2))
local MarkersA = require(ModulesFolder:WaitForChild("Markers", 2))
local MarkersB = require(ModulesFolder:WaitForChild("Markers", 2))

MarkersA.setMany(Config.MARKERS.Boot, {
    Booted = true,
    SameRef = (MarkersA == MarkersB),
    ModuleVersion = 3,
    Mode = "PVEShooter",
})

local delayed = Workspace:WaitForChild("DelayedProbe", 1.0)
MarkersA.set(Config.MARKERS.Wait, "Found", delayed ~= nil)

MarkersA.setMany(Config.MARKERS.Round, {
    Phase = "Waiting",
    IsFinished = false,
    WinnerName = "",
    WinnerUserId = 0,
})

MarkersA.setMany(Config.MARKERS.Combat, {
    LastWeapon = "",
    LastHitEnemyId = 0,
    LastDamage = 0,
    LastKills = 0,
})

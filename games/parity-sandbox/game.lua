local function makePart(name, size, position, color, transparency, canCollide)
    local p = Instance.new("Part")
    p.Name = name
    p.Size = size
    p.Position = position
    p.Anchored = true
    p.Color = color
    p.Transparency = transparency or 0
    if canCollide ~= nil then
        p.CanCollide = canCollide
    end
    p.Parent = Workspace
    return p
end

makePart("ArenaFloor", Vector3.new(140, 1, 140), Vector3.new(0, 0, 0), Color3.new(0.2, 0.2, 0.24))
makePart("NorthWall", Vector3.new(140, 18, 2), Vector3.new(0, 9, -69), Color3.new(0.13, 0.13, 0.16))
makePart("SouthWall", Vector3.new(140, 18, 2), Vector3.new(0, 9, 69), Color3.new(0.13, 0.13, 0.16))
makePart("WestWall", Vector3.new(2, 18, 140), Vector3.new(-69, 9, 0), Color3.new(0.13, 0.13, 0.16))
makePart("EastWall", Vector3.new(2, 18, 140), Vector3.new(69, 9, 0), Color3.new(0.13, 0.13, 0.16))

makePart("SpawnPad", Vector3.new(24, 1, 24), Vector3.new(-8, 1, 0), Color3.new(0.22, 0.52, 0.22))

makePart("GateNW", Vector3.new(10, 8, 2), Vector3.new(-46, 4, -32), Color3.new(0.6, 0.18, 0.18))
makePart("GateNE", Vector3.new(10, 8, 2), Vector3.new(46, 4, -32), Color3.new(0.6, 0.18, 0.18))
makePart("GateSW", Vector3.new(10, 8, 2), Vector3.new(-46, 4, 32), Color3.new(0.6, 0.18, 0.18))
makePart("GateSE", Vector3.new(10, 8, 2), Vector3.new(46, 4, 32), Color3.new(0.6, 0.18, 0.18))

makePart("CoverA", Vector3.new(8, 5, 4), Vector3.new(-22, 2.5, -12), Color3.new(0.42, 0.31, 0.23))
makePart("CoverB", Vector3.new(8, 5, 4), Vector3.new(22, 2.5, -12), Color3.new(0.42, 0.31, 0.23))
makePart("CoverC", Vector3.new(8, 5, 4), Vector3.new(-22, 2.5, 12), Color3.new(0.42, 0.31, 0.23))
makePart("CoverD", Vector3.new(8, 5, 4), Vector3.new(22, 2.5, 12), Color3.new(0.42, 0.31, 0.23))

local worldBoot = Instance.new("Folder")
worldBoot.Name = "WorldBootMarker"
worldBoot:SetAttribute("MainLoaded", true)
worldBoot:SetAttribute("GameMode", "PVEShooter")
worldBoot:SetAttribute("Objective", "Survive 5 zombie waves and clear all enemies")
worldBoot:SetAttribute("MapName", "Containment Yard")
worldBoot.Parent = Workspace

task.delay(0.05, function()
    local delayed = Instance.new("Part")
    delayed.Name = "DelayedProbe"
    delayed.Size = Vector3.new(2, 2, 2)
    delayed.Position = Vector3.new(0, 8, 0)
    delayed.Anchored = true
    delayed.Color = Color3.new(1, 0.85, 0.2)
    delayed.Parent = Workspace
end)

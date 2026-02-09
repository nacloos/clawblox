local Map = {}

local spawnParts = {}

local function makePart(name, size, position, color, anchored, material, renderRole, renderPresetId, renderPrimitive)
    local part = Instance.new("Part")
    part.Name = name
    part.Size = size
    part.Position = position
    part.Color = color
    part.Anchored = anchored
    part.CanCollide = true
    part.Material = material or Enum.Material.Plastic
    part:SetAttribute("RenderRole", renderRole or string.lower(name))
    part:SetAttribute("RenderPresetId", renderPresetId or ("fps_arena/" .. string.lower(name)))
    part:SetAttribute("RenderStatic", anchored == true)
    part:SetAttribute("RenderPrimitive", renderPrimitive or string.lower(part.Shape.Name))
    part:SetAttribute("RenderMaterial", (material or part.Material).Name)
    part:SetAttribute("RenderColor", part.Color)

    local role = renderRole or string.lower(name)
    local castsShadow = true
    local receivesShadow = true
    if role == "floor" then
        castsShadow = false
        receivesShadow = true
    elseif role == "ceiling" then
        castsShadow = false
        receivesShadow = false
    elseif role == "trim_red" or role == "trim_blue" or role == "spawn" then
        castsShadow = false
        receivesShadow = false
    end
    part:SetAttribute("RenderCastsShadow", castsShadow)
    part:SetAttribute("RenderReceivesShadow", receivesShadow)

    part.Parent = Workspace
    return part
end

local function addSpawn(position)
    local p = Instance.new("Part")
    p.Name = "SpawnPoint"
    p.Size = Vector3.new(2.5, 0.6, 2.5)
    p.Position = position
    p.Anchored = true
    p.CanCollide = false
    p.Transparency = 0.5
    p.Color = Color3.fromRGB(255, 200, 90)
    p.Material = Enum.Material.Neon
    p:SetAttribute("RenderRole", "spawn")
    p:SetAttribute("RenderPresetId", "fps_arena/spawn")
    p:SetAttribute("RenderStatic", true)
    p:SetAttribute("RenderPrimitive", "box")
    p:SetAttribute("RenderMaterial", p.Material.Name)
    p:SetAttribute("RenderColor", p.Color)
    p:SetAttribute("RenderVisible", false)
    p.Parent = Workspace
    table.insert(spawnParts, p)
end

local function addWall(x, z, sx, sz, ht)
    local wall = makePart(
        "Wall",
        Vector3.new(sx, ht, sz),
        Vector3.new(x, ht / 2, z),
        Color3.fromRGB(58, 58, 66),
        true,
        Enum.Material.Concrete,
        "wall",
        "fps_arena/wall",
        "box"
    )
    return wall
end

local function addCrate(x, z, size, ht)
    local crate = makePart(
        "Crate",
        Vector3.new(size, ht, size),
        Vector3.new(x, ht / 2, z),
        Color3.fromRGB(107, 66, 38),
        true,
        Enum.Material.Wood,
        "crate",
        "fps_arena/crate",
        "box"
    )

    local edgeColor = Color3.fromRGB(68, 68, 68)
    local edge1 = makePart(
        "CrateEdge",
        Vector3.new(size + 0.02, 0.04, 0.04),
        Vector3.new(x, ht, z),
        edgeColor,
        true,
        Enum.Material.Metal,
        "crate_edge",
        "fps_arena/crate_edge",
        "box"
    )
    local edge2 = makePart(
        "CrateEdge",
        Vector3.new(size + 0.02, 0.04, 0.04),
        Vector3.new(x, 0, z),
        edgeColor,
        true,
        Enum.Material.Metal,
        "crate_edge",
        "fps_arena/crate_edge",
        "box"
    )

    return crate, edge1, edge2
end

local function addPillar(x, z, radius, ht)
    local p = makePart(
        "Pillar",
        Vector3.new(radius * 2, ht, radius * 2),
        Vector3.new(x, ht / 2, z),
        Color3.fromRGB(85, 85, 102),
        true,
        Enum.Material.Metal,
        "pillar",
        "fps_arena/pillar",
        "cylinder"
    )
    p.Shape = Enum.PartType.Cylinder
    p:SetAttribute("RenderPrimitive", "cylinder")
    return p
end

function Map.Build(config)
    spawnParts = {}

    local mapSize = config.MAP_SIZE
    local wallH = config.ARENA_HEIGHT
    local half = mapSize / 2
    local wt = 0.5

    local floor = makePart(
        "Floor",
        Vector3.new(mapSize, 1, mapSize),
        Vector3.new(0, -0.5, 0),
        Color3.fromRGB(58, 58, 58),
        true,
        Enum.Material.Slate,
        "floor",
        "fps_arena/floor",
        "box"
    )
    floor:AddTag("NoCollision")

    local ceiling = makePart(
        "Ceiling",
        Vector3.new(mapSize, 1, mapSize),
        Vector3.new(0, wallH, 0),
        Color3.fromRGB(42, 42, 48),
        true,
        Enum.Material.Concrete,
        "ceiling",
        "fps_arena/ceiling",
        "box"
    )

    addWall(0, -half, mapSize, wt, wallH)
    addWall(0, half, mapSize, wt, wallH)
    addWall(-half, 0, wt, mapSize, wallH)
    addWall(half, 0, wt, mapSize, wallH)

    addWall(-4, -4, 8, 0.5, wallH)
    addWall(-4, 4, 8, 0.5, wallH)
    addWall(-8, 0, 0.5, 8, wallH)
    addWall(8, 0, 0.5, 8, wallH)

    addWall(-18, -8, 0.5, 10, wallH)
    addWall(-18, 8, 0.5, 10, wallH)
    addWall(18, -8, 0.5, 10, wallH)
    addWall(18, 8, 0.5, 10, wallH)

    addWall(-12, -15, 8, 0.5, wallH)
    addWall(12, -15, 8, 0.5, wallH)
    addWall(-12, 15, 8, 0.5, wallH)
    addWall(12, 15, 8, 0.5, wallH)

    addWall(-22, -20, 6, 0.5, wallH)
    addWall(22, -20, 6, 0.5, wallH)
    addWall(-22, 20, 6, 0.5, wallH)
    addWall(22, 20, 6, 0.5, wallH)

    local shortH = 1.5
    local shortCover = {
        {-10, -10}, {10, -10}, {-10, 10}, {10, 10}, {0, -12}, {0, 12}
    }
    for _, p in ipairs(shortCover) do
        addWall(p[1], p[2], 3, 0.4, shortH)
    end

    local pillars = {
        {-6, -6}, {6, -6}, {-6, 6}, {6, 6},
        {-14, 0}, {14, 0}, {0, -20}, {0, 20},
        {-22, -10}, {22, -10}, {-22, 10}, {22, 10},
    }
    for _, p in ipairs(pillars) do
        addPillar(p[1], p[2], 0.5, wallH)
    end

    local crates = {
        {-3, -18}, {-1, -18}, {-2, -16},
        {3, 18}, {1, 18}, {2, 16},
        {-20, -2}, {-20, 2},
        {20, -2}, {20, 2},
        {-15, -22}, {15, 22},
        {-25, 0}, {25, 0},
    }
    for _, p in ipairs(crates) do
        local size = 1.2
        local height = 1.2
        addCrate(p[1], p[2], size, height)
    end

    local platforms = {
        {0, 0, 8, 0.3, 8},
        {-20, -20, 6, 0.3, 6},
        {20, 20, 6, 0.3, 6},
    }
    for _, p in ipairs(platforms) do
        makePart(
            "Platform",
            Vector3.new(p[3], p[4], p[5]),
            Vector3.new(p[1], p[4] / 2, p[2]),
            Color3.fromRGB(42, 42, 48),
            true,
            Enum.Material.Metal,
            "platform",
            "fps_arena/platform",
            "box"
        )
    end

    local stripLeft = makePart(
        "TrimRed",
        Vector3.new(0.1, 0.1, mapSize),
        Vector3.new(-half + 0.3, 0.5, 0),
        Color3.fromRGB(255, 34, 34),
        true,
        Enum.Material.Neon,
        "trim_red",
        "fps_arena/trim_red",
        "box"
    )
    local stripRight = makePart(
        "TrimBlue",
        Vector3.new(0.1, 0.1, mapSize),
        Vector3.new(half - 0.3, 0.5, 0),
        Color3.fromRGB(34, 68, 255),
        true,
        Enum.Material.Neon,
        "trim_blue",
        "fps_arena/trim_blue",
        "box"
    )
    local stripBottom = makePart(
        "TrimRed",
        Vector3.new(mapSize, 0.1, 0.1),
        Vector3.new(0, 0.5, -half + 0.3),
        Color3.fromRGB(255, 34, 34),
        true,
        Enum.Material.Neon,
        "trim_red",
        "fps_arena/trim_red",
        "box"
    )
    local stripTop = makePart(
        "TrimBlue",
        Vector3.new(mapSize, 0.1, 0.1),
        Vector3.new(0, 0.5, half - 0.3),
        Color3.fromRGB(34, 68, 255),
        true,
        Enum.Material.Neon,
        "trim_blue",
        "fps_arena/trim_blue",
        "box"
    )

    local _ = stripLeft
    local _ = stripRight
    local _ = stripBottom
    local _ = stripTop

    local spawns = {
        Vector3.new(-24, 2, -24),
        Vector3.new(24, 2, -24),
        Vector3.new(-24, 2, 24),
        Vector3.new(24, 2, 24),
        Vector3.new(0, 2, -24),
        Vector3.new(0, 2, 24),
        Vector3.new(-24, 2, 0),
        Vector3.new(24, 2, 0),
    }
    for _, pos in ipairs(spawns) do
        addSpawn(pos)
    end

    local gameState = Instance.new("Folder")
    gameState.Name = "GameState"
    gameState.Parent = Workspace

    print("[fps-arena] map built")
end

function Map.GetSpawnPoints()
    return spawnParts
end

return Map

-- Block Arsenal - Simplified Sandbox Mode
-- Continuous deathmatch with projectile-based combat

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")

--------------------------------------------------------------------------------
-- CONFIGURATION
--------------------------------------------------------------------------------

local ARENA_SIZE = 200
local RESPAWN_TIME = 3
local HEALTH_REGEN_DELAY = 5
local HEALTH_REGEN_RATE = 10

-- Single unified weapon config
local WEAPON = {
    damage = 25,
    fireRate = 0.3,       -- seconds between shots
    projectileSpeed = 100, -- studs/sec
    range = 100,          -- max travel distance
}

local SPAWN_POINTS = {
    Vector3.new(-75, 12, -75),
    Vector3.new(-75, 12, 75),
    Vector3.new(75, 12, 75),
    Vector3.new(75, 12, -75),
}

--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local gameState = "waiting" -- waiting, countdown, active
local projectiles = {}
local playerData = {} -- keyed by UserId: {lastFireTime, lastDamageTime, lastKiller, respawnTime}

-- Helper to get playerData by player object
local function getPlayerData(player)
    return playerData[player.UserId]
end

local function setPlayerData(player, data)
    playerData[player.UserId] = data
end

--------------------------------------------------------------------------------
-- ARENA CREATION
--------------------------------------------------------------------------------

local function createArena()
    -- Floor
    local floor = Instance.new("Part")
    floor.Name = "Floor"
    floor.Size = Vector3.new(ARENA_SIZE, 2, ARENA_SIZE)
    floor.Position = Vector3.new(0, -1, 0)
    floor.Anchored = true
    floor.Color = Color3.fromRGB(180, 180, 185)
    floor.Parent = Workspace

    -- Walls (invisible barriers)
    local wallPositions = {
        {Vector3.new(ARENA_SIZE/2 + 1, 25, 0), Vector3.new(2, 50, ARENA_SIZE)},
        {Vector3.new(-ARENA_SIZE/2 - 1, 25, 0), Vector3.new(2, 50, ARENA_SIZE)},
        {Vector3.new(0, 25, ARENA_SIZE/2 + 1), Vector3.new(ARENA_SIZE, 50, 2)},
        {Vector3.new(0, 25, -ARENA_SIZE/2 - 1), Vector3.new(ARENA_SIZE, 50, 2)},
    }
    for i, data in ipairs(wallPositions) do
        local wall = Instance.new("Part")
        wall.Name = "Wall_" .. i
        wall.Position = data[1]
        wall.Size = data[2]
        wall.Anchored = true
        wall.Transparency = 1
        wall.CanCollide = true
        wall.Parent = Workspace
    end

    -- Center platform (low, just above ground)
    local centerPlatform = Instance.new("Part")
    centerPlatform.Name = "CenterPlatform"
    centerPlatform.Size = Vector3.new(40, 2, 40)
    centerPlatform.Position = Vector3.new(0, 1, 0)
    centerPlatform.Anchored = true
    centerPlatform.Color = Color3.fromRGB(100, 140, 180)
    centerPlatform.Parent = Workspace

    -- Corner platforms
    local cornerPositions = {
        Vector3.new(-70, 10, -70),
        Vector3.new(-70, 10, 70),
        Vector3.new(70, 10, 70),
        Vector3.new(70, 10, -70),
    }
    for i, pos in ipairs(cornerPositions) do
        local platform = Instance.new("Part")
        platform.Name = "CornerPlatform_" .. i
        platform.Size = Vector3.new(25, 2, 25)
        platform.Position = pos
        platform.Anchored = true
        platform.Color = Color3.fromRGB(180, 120, 120)
        platform.Parent = Workspace

        -- Platform support
        local pSupport = Instance.new("Part")
        pSupport.Name = "CornerSupport_" .. i
        pSupport.Size = Vector3.new(10, 10, 10)
        pSupport.Position = pos - Vector3.new(0, 6, 0)
        pSupport.Anchored = true
        pSupport.Color = Color3.fromRGB(150, 100, 100)
        pSupport.Parent = Workspace
    end

    -- Cover blocks
    local coverPositions = {
        Vector3.new(-37.5, 7.5, -37.5),
        Vector3.new(37.5, 7.5, -37.5),
        Vector3.new(-37.5, 7.5, 37.5),
        Vector3.new(37.5, 7.5, 37.5),
        Vector3.new(-62.5, 7.5, 0),
        Vector3.new(62.5, 7.5, 0),
        Vector3.new(0, 7.5, -62.5),
        Vector3.new(0, 7.5, 62.5),
    }
    for i, pos in ipairs(coverPositions) do
        local cover = Instance.new("Part")
        cover.Name = "Cover_" .. i
        cover.Size = Vector3.new(15, 15, 15)
        cover.Position = pos
        cover.Anchored = true
        cover.Color = Color3.fromRGB(180, 180, 190)
        cover.Parent = Workspace
    end

    -- Bridges along edges (centered, touching side cover blocks)
    local bridges = {
        {Vector3.new(0, 10, -70), Vector3.new(115, 1, 8)},   -- back edge
        {Vector3.new(0, 10, 70), Vector3.new(115, 1, 8)},    -- front edge
        {Vector3.new(-70, 10, 0), Vector3.new(8, 1, 115)},   -- left edge
        {Vector3.new(70, 10, 0), Vector3.new(8, 1, 115)},    -- right edge
    }
    for i, data in ipairs(bridges) do
        local bridge = Instance.new("Part")
        bridge.Name = "Bridge_" .. i
        bridge.Position = data[1]
        bridge.Size = data[2]
        bridge.Anchored = true
        bridge.Color = Color3.fromRGB(130, 150, 180)
        bridge.Parent = Workspace
    end

    print("Arena created!")
end

--------------------------------------------------------------------------------
-- PLAYER MANAGEMENT
--------------------------------------------------------------------------------

local function initializePlayer(player)
    player:SetAttribute("Kills", 0)
    player:SetAttribute("Deaths", 0)

    setPlayerData(player, {
        lastFireTime = 0,
        lastDamageTime = 0,
        lastKiller = nil,
        respawnTime = 0,
    })
end

local function getHumanoid(player)
    local character = player.Character
    if character then
        return character:FindFirstChild("Humanoid")
    end
    return nil
end

local function getCharacterPosition(player)
    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            return hrp.Position
        end
    end
    return nil
end

local function getFurthestSpawn(fromPos)
    local furthest = SPAWN_POINTS[1]
    local maxDist = 0

    for _, spawn in ipairs(SPAWN_POINTS) do
        local dist = (spawn - fromPos).Magnitude
        if dist > maxDist then
            maxDist = dist
            furthest = spawn
        end
    end

    return furthest
end

local function respawnPlayer(player)
    local humanoid = getHumanoid(player)
    if humanoid then
        humanoid.Health = 100
    end

    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            local killer = getPlayerData(player) and getPlayerData(player).lastKiller
            local killerPos = Vector3.new(0, 0, 0)
            if killer then
                local kPos = getCharacterPosition(killer)
                if kPos then
                    killerPos = kPos
                end
            end
            local spawnPoint = getFurthestSpawn(killerPos)
            hrp.Position = spawnPoint
            hrp.Velocity = Vector3.new(0, 0, 0)
        end
    end

    if getPlayerData(player) then
        getPlayerData(player).respawnTime = 0
        getPlayerData(player).lastKiller = nil
    end
end

--------------------------------------------------------------------------------
-- DAMAGE SYSTEM
--------------------------------------------------------------------------------

local function findPlayerFromPart(part)
    -- Walk up to find the character Model (has Humanoid child)
    local current = part
    while current and current ~= Workspace do
        if current:FindFirstChild("Humanoid") then
            -- Use Roblox's built-in method
            return Players:GetPlayerFromCharacter(current)
        end
        current = current.Parent
    end
    return nil
end

local function dealDamage(attacker, victim, damage)
    local humanoid = getHumanoid(victim)
    if not humanoid or humanoid.Health <= 0 then
        return false
    end

    humanoid:TakeDamage(damage)
    getPlayerData(victim).lastDamageTime = tick()
    getPlayerData(victim).lastKiller = attacker

    -- Flash the victim red briefly
    local character = victim.Character
    if character then
        for _, part in ipairs(character:GetChildren()) do
            if part:IsA("Part") or part:IsA("MeshPart") then
                part.Color = Color3.fromRGB(255, 100, 100)
            end
        end
    end

    if humanoid.Health <= 0 then
        return true -- Kill confirmed
    end
    return false
end

local function onPlayerKilled(killer, victim)
    if gameState ~= "active" then
        return
    end

    -- Update stats
    killer:SetAttribute("Kills", killer:GetAttribute("Kills") + 1)
    victim:SetAttribute("Deaths", victim:GetAttribute("Deaths") + 1)

    print("[KILL] " .. killer.Name .. " -> " .. victim.Name)

    -- Set respawn timer
    getPlayerData(victim).respawnTime = tick() + RESPAWN_TIME
end

--------------------------------------------------------------------------------
-- PROJECTILE SYSTEM
--------------------------------------------------------------------------------

local function spawnProjectile(player, direction)
    local character = player.Character
    if not character then
        warn("spawnProjectile: player has no Character")
        return
    end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    if not hrp then
        warn("spawnProjectile: character has no HumanoidRootPart")
        return
    end

    local origin = hrp.Position + Vector3.new(0, 1, 0) + direction * 2

    local projectile = Instance.new("Part")
    projectile.Name = "Projectile"
    projectile.Size = Vector3.new(0.3, 0.3, 1.2)
    projectile.CFrame = CFrame.new(origin, origin + direction)
    projectile.Anchored = false
    projectile.CanCollide = false  -- No physics collision, we check manually
    projectile.Color = Color3.fromRGB(255, 200, 50)
    projectile.Material = Enum.Material.Neon

    projectile:SetAttribute("Damage", WEAPON.damage)
    projectile:SetAttribute("Owner", player.Name)
    projectile:SetAttribute("StartPos", origin)
    projectile:SetAttribute("MaxRange", WEAPON.range)

    -- Set velocity
    projectile.Velocity = direction * WEAPON.projectileSpeed
    projectile.Parent = Workspace

    table.insert(projectiles, projectile)
end

local function tryFire(player)
    if gameState ~= "active" then return end

    local data = getPlayerData(player)
    if not data then
        warn("tryFire: no player data for", player.Name)
        return
    end

    local now = tick()

    -- Check fire rate
    if now - data.lastFireTime < WEAPON.fireRate then
        return
    end

    -- Get aim direction from attribute (set by agent/frontend)
    local aimDir = player:GetAttribute("AimDirection")
    if not aimDir then
        local character = player.Character
        if character then
            local hrp = character:FindFirstChild("HumanoidRootPart")
            if hrp then
                aimDir = hrp.CFrame.LookVector
            end
        end
    end

    if not aimDir then
        warn("tryFire: no aim direction for", player.Name)
        return
    end
    aimDir = aimDir.Unit

    data.lastFireTime = now
    spawnProjectile(player, aimDir)
end

--------------------------------------------------------------------------------
-- GAME LOOP
--------------------------------------------------------------------------------

local function updateProjectiles(dt)
    local toRemove = {}

    for i, proj in ipairs(projectiles) do
        local shouldRemove = false

        if proj and proj.Parent then
            local damage = proj:GetAttribute("Damage")
            local ownerName = proj:GetAttribute("Owner")
            local startPos = proj:GetAttribute("StartPos")
            local maxRange = proj:GetAttribute("MaxRange")

            -- Check if projectile has traveled too far
            if startPos and maxRange then
                local traveled = (proj.Position - startPos).Magnitude
                if traveled > maxRange then
                    shouldRemove = true
                end
            end

            -- Check for collision with players
            if not shouldRemove and damage and ownerName then
                for _, player in ipairs(Players:GetPlayers()) do
                    if player.Name ~= ownerName then
                        local pos = getCharacterPosition(player)
                        if pos then
                            local dist = (pos - proj.Position).Magnitude
                            if dist < 3 then
                                -- Direct hit
                                local owner = nil
                                for _, p in ipairs(Players:GetPlayers()) do
                                    if p.Name == ownerName then
                                        owner = p
                                        break
                                    end
                                end

                                if owner then
                                    local killed = dealDamage(owner, player, damage)
                                    if killed then
                                        onPlayerKilled(owner, player)
                                    end
                                end

                                shouldRemove = true
                                break
                            end
                        end
                    end
                end
            end
        else
            shouldRemove = true
        end

        if shouldRemove then
            table.insert(toRemove, i)
        end
    end

    -- Remove expired/hit projectiles
    for i = #toRemove, 1, -1 do
        local proj = projectiles[toRemove[i]]
        if proj and proj.Parent then
            proj:Destroy()
        end
        table.remove(projectiles, toRemove[i])
    end
end

local function updateHealthRegen(dt)
    local now = tick()

    for _, player in ipairs(Players:GetPlayers()) do
        local data = getPlayerData(player)
        if data then
            local humanoid = getHumanoid(player)
            if humanoid and humanoid.Health > 0 and humanoid.Health < 100 then
                if now - data.lastDamageTime > HEALTH_REGEN_DELAY then
                    humanoid.Health = math.min(100, humanoid.Health + HEALTH_REGEN_RATE * dt)
                end
            end
        end
    end
end

local function updateRespawns()
    local now = tick()

    for _, player in ipairs(Players:GetPlayers()) do
        local data = getPlayerData(player)
        if data and data.respawnTime > 0 then
            if now >= data.respawnTime then
                respawnPlayer(player)
            end
        end
    end
end

local function updateFiring()
    for _, player in ipairs(Players:GetPlayers()) do
        local firing = player:GetAttribute("Firing")
        if firing then
            tryFire(player)
        end
    end
end

local function printLeaderboard()
    print("\n=== LEADERBOARD ===")
    local players = Players:GetPlayers()
    table.sort(players, function(a, b)
        local aKills = a:GetAttribute("Kills") or 0
        local bKills = b:GetAttribute("Kills") or 0
        return aKills > bKills
    end)

    for i, player in ipairs(players) do
        local kills = player:GetAttribute("Kills") or 0
        local deaths = player:GetAttribute("Deaths") or 0
        print(string.format("%d. %s K/D: %d/%d", i, player.Name, kills, deaths))
    end
    print("===================\n")
end

--------------------------------------------------------------------------------
-- MATCH CONTROL
--------------------------------------------------------------------------------

local countdownTime = 0
local leaderboardTimer = 0

local function startCountdown()
    gameState = "countdown"
    countdownTime = 0  -- Start immediately

    -- Reset all players
    for _, player in ipairs(Players:GetPlayers()) do
        initializePlayer(player)
        respawnPlayer(player)
    end

    print("Match starting...")
end

local function startMatch()
    gameState = "active"
    print("=== SANDBOX MODE STARTED ===")
    print("Eliminate opponents - no win condition, just frag!")
end

local function updateMatch(dt)
    if gameState == "waiting" then
        local playerCount = #Players:GetPlayers()
        if playerCount >= 1 then
            startCountdown()
        end

    elseif gameState == "countdown" then
        countdownTime = countdownTime - dt
        if countdownTime <= 0 then
            startMatch()
        elseif math.floor(countdownTime) < math.floor(countdownTime + dt) then
            print(math.floor(countdownTime + 1) .. "...")
        end

    elseif gameState == "active" then
        -- Periodic leaderboard
        leaderboardTimer = leaderboardTimer + dt
        if leaderboardTimer >= 30 then
            leaderboardTimer = 0
            printLeaderboard()
        end
    end
end

--------------------------------------------------------------------------------
-- INITIALIZATION
--------------------------------------------------------------------------------

createArena()

-- Initialize existing players
for _, player in ipairs(Players:GetPlayers()) do
    initializePlayer(player)
end

-- Handle new players
Players.PlayerAdded:Connect(function(player)
    initializePlayer(player)
    if gameState == "active" then
        respawnPlayer(player)
    end
end)

Players.PlayerRemoving:Connect(function(player)
    playerData[player.UserId] = nil
end)

-- Handle agent inputs (AI players via HTTP API)
local AgentInputService = game:GetService("AgentInputService")
if AgentInputService then
    AgentInputService.InputReceived:Connect(function(player, inputType, data)
        if gameState ~= "active" then return end
        if not getPlayerData(player) then
            warn("InputReceived: unknown player", player.Name)
            return
        end

        if inputType == "Fire" then
            -- Compute aim direction from target position
            if data and data.target then
                local target = data.target
                local targetPos = Vector3.new(target[1], target[2], target[3])
                local playerPos = getCharacterPosition(player)
                if playerPos then
                    local dir = (targetPos - playerPos).Unit
                    player:SetAttribute("AimDirection", dir)
                end
            end
            tryFire(player)

        elseif inputType == "MoveTo" then
            local humanoid = getHumanoid(player)
            if humanoid and data and data.position then
                local pos = data.position
                humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
            else
                warn("MoveTo: missing humanoid or position for", player.Name)
            end
        end
    end)
end

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    updateMatch(dt)
    updateFiring()
    updateProjectiles(dt)
    updateHealthRegen(dt)
    updateRespawns()
end)

print("Block Arsenal loaded!")
print("Sandbox mode - waiting for players...")

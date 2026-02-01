-- Seed script for example games
-- Run with: psql -d clawblox -f scripts/seed_games.sql

-- Delete existing seeded games if they exist
DELETE FROM games WHERE id IN (
    'a0000000-0000-0000-0000-000000000001',
    'a0000000-0000-0000-0000-000000000002',
    'a0000000-0000-0000-0000-000000000003',
    'a0000000-0000-0000-0000-000000000004',
    'a0000000-0000-0000-0000-000000000005'
);

-- Insert Block Arsenal game
INSERT INTO games (id, name, description, game_type, status, script_code)
VALUES (
    'a0000000-0000-0000-0000-000000000005',
    'Block Arsenal',
    'Gun Game / Arms Race - Progress through 15 weapons by getting kills. First to kill with the Golden Knife wins!',
    'lua',
    'waiting',
    '-- Block Arsenal
-- Gun Game / Arms Race - First to kill with every weapon wins

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")

--------------------------------------------------------------------------------
-- CONFIGURATION
--------------------------------------------------------------------------------

local ARENA_SIZE = 80
local RESPAWN_TIME = 3
local HEALTH_REGEN_DELAY = 5
local HEALTH_REGEN_RATE = 10
local MELEE_DAMAGE = 35
local MELEE_RANGE = 6
local MELEE_COOLDOWN = 0.8

local WEAPONS = {
    {name = "Pistol",         type = "hitscan",     damage = 25,  fireRate = 0.3,  range = 80,  pellets = 1, spread = 0},
    {name = "SMG",            type = "hitscan",     damage = 14,  fireRate = 0.08, range = 50,  pellets = 1, spread = 0.05},
    {name = "Shotgun",        type = "pellet",      damage = 12,  fireRate = 0.9,  range = 30,  pellets = 8, spread = 0.15},
    {name = "Assault Rifle",  type = "hitscan",     damage = 22,  fireRate = 0.12, range = 100, pellets = 1, spread = 0.02},
    {name = "Sniper Rifle",   type = "hitscan",     damage = 100, fireRate = 1.5,  range = 250, pellets = 1, spread = 0},
    {name = "LMG",            type = "hitscan",     damage = 18,  fireRate = 0.07, range = 80,  pellets = 1, spread = 0.03},
    {name = "Revolver",       type = "hitscan",     damage = 55,  fireRate = 0.5,  range = 90,  pellets = 1, spread = 0},
    {name = "Burst Rifle",    type = "burst",       damage = 18,  fireRate = 0.35, range = 90,  pellets = 1, spread = 0.01, burstCount = 3},
    {name = "Auto Shotgun",   type = "pellet",      damage = 9,   fireRate = 0.35, range = 25,  pellets = 6, spread = 0.12},
    {name = "DMR",            type = "hitscan",     damage = 48,  fireRate = 0.4,  range = 150, pellets = 1, spread = 0},
    {name = "Minigun",        type = "hitscan",     damage = 12,  fireRate = 0.04, range = 70,  pellets = 1, spread = 0.04, spinUp = 1.0},
    {name = "Crossbow",       type = "projectile",  damage = 85,  fireRate = 1.0,  range = 120, speed = 80},
    {name = "Dual Pistols",   type = "dual",        damage = 20,  fireRate = 0.2,  range = 70,  pellets = 1, spread = 0.02},
    {name = "Rocket Launcher",type = "projectile",  damage = 100, fireRate = 2.0,  range = 150, speed = 50, splash = 15},
    {name = "Golden Knife",   type = "melee",       damage = 999, fireRate = 0.4,  range = 6},
}

local SPAWN_POINTS = {
    Vector3.new(-30, 3, -30),
    Vector3.new(-30, 3, 30),
    Vector3.new(30, 3, 30),
    Vector3.new(30, 3, -30),
}

--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local gameState = "waiting" -- waiting, countdown, active, victory
local matchStartTime = 0
local winner = nil
local projectiles = {}
local playerData = {} -- {lastFireTime, lastMeleeTime, lastDamageTime, lastKiller, respawnTime}

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
    floor.Color = Color3.fromRGB(60, 60, 60)
    floor.Parent = Workspace

    -- Walls (invisible barriers)
    local wallPositions = {
        {Vector3.new(ARENA_SIZE/2 + 1, 10, 0), Vector3.new(2, 20, ARENA_SIZE)},
        {Vector3.new(-ARENA_SIZE/2 - 1, 10, 0), Vector3.new(2, 20, ARENA_SIZE)},
        {Vector3.new(0, 10, ARENA_SIZE/2 + 1), Vector3.new(ARENA_SIZE, 20, 2)},
        {Vector3.new(0, 10, -ARENA_SIZE/2 - 1), Vector3.new(ARENA_SIZE, 20, 2)},
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

    -- Center platform
    local centerPlatform = Instance.new("Part")
    centerPlatform.Name = "CenterPlatform"
    centerPlatform.Size = Vector3.new(16, 1, 16)
    centerPlatform.Position = Vector3.new(0, 5, 0)
    centerPlatform.Anchored = true
    centerPlatform.Color = Color3.fromRGB(80, 80, 100)
    centerPlatform.Parent = Workspace

    -- Center platform support
    local support = Instance.new("Part")
    support.Name = "CenterSupport"
    support.Size = Vector3.new(8, 5, 8)
    support.Position = Vector3.new(0, 2.5, 0)
    support.Anchored = true
    support.Color = Color3.fromRGB(70, 70, 90)
    support.Parent = Workspace

    -- Stairs to center (4 sides)
    local stairDirs = {
        {Vector3.new(12, 0, 0), Vector3.new(8, 1, 4)},
        {Vector3.new(-12, 0, 0), Vector3.new(8, 1, 4)},
        {Vector3.new(0, 0, 12), Vector3.new(4, 1, 8)},
        {Vector3.new(0, 0, -12), Vector3.new(4, 1, 8)},
    }
    for i, data in ipairs(stairDirs) do
        for step = 1, 5 do
            local stair = Instance.new("Part")
            stair.Name = "Stair_" .. i .. "_" .. step
            local offset = data[1]:Unit() * (step * 1.5)
            stair.Position = Vector3.new(offset.X, step, offset.Z)
            stair.Size = data[2]
            stair.Anchored = true
            stair.Color = Color3.fromRGB(90, 90, 90)
            stair.Parent = Workspace
        end
    end

    -- Corner platforms
    local cornerPositions = {
        Vector3.new(-28, 4, -28),
        Vector3.new(-28, 4, 28),
        Vector3.new(28, 4, 28),
        Vector3.new(28, 4, -28),
    }
    for i, pos in ipairs(cornerPositions) do
        local platform = Instance.new("Part")
        platform.Name = "CornerPlatform_" .. i
        platform.Size = Vector3.new(10, 1, 10)
        platform.Position = pos
        platform.Anchored = true
        platform.Color = Color3.fromRGB(100, 80, 80)
        platform.Parent = Workspace

        -- Platform support
        local pSupport = Instance.new("Part")
        pSupport.Name = "CornerSupport_" .. i
        pSupport.Size = Vector3.new(4, 4, 4)
        pSupport.Position = pos - Vector3.new(0, 2.5, 0)
        pSupport.Anchored = true
        pSupport.Color = Color3.fromRGB(80, 60, 60)
        pSupport.Parent = Workspace
    end

    -- Cover blocks
    local coverPositions = {
        Vector3.new(-15, 2.5, -15),
        Vector3.new(15, 2.5, -15),
        Vector3.new(-15, 2.5, 15),
        Vector3.new(15, 2.5, 15),
        Vector3.new(-25, 2.5, 0),
        Vector3.new(25, 2.5, 0),
        Vector3.new(0, 2.5, -25),
        Vector3.new(0, 2.5, 25),
    }
    for i, pos in ipairs(coverPositions) do
        local cover = Instance.new("Part")
        cover.Name = "Cover_" .. i
        cover.Size = Vector3.new(6, 5, 6)
        cover.Position = pos
        cover.Anchored = true
        cover.Color = Color3.fromRGB(120, 120, 120)
        cover.Parent = Workspace
    end

    -- Bridges connecting platforms
    local bridges = {
        {Vector3.new(-14, 4, -28), Vector3.new(18, 0.5, 3)},
        {Vector3.new(14, 4, -28), Vector3.new(18, 0.5, 3)},
        {Vector3.new(-14, 4, 28), Vector3.new(18, 0.5, 3)},
        {Vector3.new(14, 4, 28), Vector3.new(18, 0.5, 3)},
        {Vector3.new(-28, 4, -14), Vector3.new(3, 0.5, 18)},
        {Vector3.new(-28, 4, 14), Vector3.new(3, 0.5, 18)},
        {Vector3.new(28, 4, -14), Vector3.new(3, 0.5, 18)},
        {Vector3.new(28, 4, 14), Vector3.new(3, 0.5, 18)},
    }
    for i, data in ipairs(bridges) do
        local bridge = Instance.new("Part")
        bridge.Name = "Bridge_" .. i
        bridge.Position = data[1]
        bridge.Size = data[2]
        bridge.Anchored = true
        bridge.Color = Color3.fromRGB(90, 90, 110)
        bridge.Parent = Workspace
    end

    print("Arena created!")
end

--------------------------------------------------------------------------------
-- PLAYER MANAGEMENT
--------------------------------------------------------------------------------

local function initializePlayer(player)
    player:SetAttribute("CurrentWeapon", 1)
    player:SetAttribute("Kills", 0)
    player:SetAttribute("Deaths", 0)
    player:SetAttribute("MeleeKills", 0)

    playerData[player] = {
        lastFireTime = 0,
        lastMeleeTime = 0,
        lastDamageTime = 0,
        lastKiller = nil,
        respawnTime = 0,
        burstRemaining = 0,
        spinUpTime = 0,
    }
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
            local killer = playerData[player] and playerData[player].lastKiller
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

    if playerData[player] then
        playerData[player].respawnTime = 0
        playerData[player].lastKiller = nil
    end
end

--------------------------------------------------------------------------------
-- DAMAGE SYSTEM
--------------------------------------------------------------------------------

local function findPlayerFromPart(part)
    local current = part
    while current do
        if current:FindFirstChild("Humanoid") then
            for _, player in ipairs(Players:GetPlayers()) do
                if player.Character == current then
                    return player
                end
            end
        end
        current = current.Parent
    end
    return nil
end

local function dealDamage(attacker, victim, damage, isMelee)
    local humanoid = getHumanoid(victim)
    if not humanoid or humanoid.Health <= 0 then
        return false
    end

    humanoid:TakeDamage(damage)
    playerData[victim].lastDamageTime = tick()
    playerData[victim].lastKiller = attacker

    -- Flash the victim red briefly
    local character = victim.Character
    if character then
        for _, part in ipairs(character:GetChildren()) do
            if part:IsA("Part") or part:IsA("MeshPart") then
                local originalColor = part.Color
                part.Color = Color3.fromRGB(255, 100, 100)
                -- Would use TweenService here for smooth transition
            end
        end
    end

    if humanoid.Health <= 0 then
        return true -- Kill confirmed
    end
    return false
end

local function onPlayerKilled(killer, victim, wasMelee)
    if gameState ~= "active" then
        return
    end

    -- Update stats
    killer:SetAttribute("Kills", killer:GetAttribute("Kills") + 1)
    victim:SetAttribute("Deaths", victim:GetAttribute("Deaths") + 1)

    local killerWeapon = killer:GetAttribute("CurrentWeapon")
    local victimWeapon = victim:GetAttribute("CurrentWeapon")
    local weaponName = WEAPONS[killerWeapon].name

    if wasMelee and killerWeapon ~= 15 then
        -- Melee kill with non-final weapon: demote victim, no advance
        local newVictimWeapon = math.max(1, victimWeapon - 1)
        victim:SetAttribute("CurrentWeapon", newVictimWeapon)
        killer:SetAttribute("MeleeKills", killer:GetAttribute("MeleeKills") + 1)
        print("[KNIFE] " .. killer.Name .. " DEMOTED " .. victim.Name .. " to " .. WEAPONS[newVictimWeapon].name)
    else
        -- Gun kill or golden knife: advance killer
        if killerWeapon == 15 then
            -- Victory!
            winner = killer
            gameState = "victory"
            print("=== " .. killer.Name .. " WINS WITH THE GOLDEN KNIFE! ===")
        else
            local newWeapon = killerWeapon + 1
            killer:SetAttribute("CurrentWeapon", newWeapon)
            print("[" .. weaponName .. "] " .. killer.Name .. " -> " .. victim.Name .. " | Now using: " .. WEAPONS[newWeapon].name)
        end
    end

    -- Set respawn timer
    playerData[victim].respawnTime = tick() + RESPAWN_TIME
end

--------------------------------------------------------------------------------
-- WEAPON SYSTEM
--------------------------------------------------------------------------------

local function createTracer(origin, hitPos)
    local tracer = Instance.new("Part")
    tracer.Name = "Tracer"
    tracer.Anchored = true
    tracer.CanCollide = false
    tracer.Color = Color3.fromRGB(255, 255, 100)
    tracer.Material = Enum.Material.Neon

    local distance = (hitPos - origin).Magnitude
    tracer.Size = Vector3.new(0.1, 0.1, distance)
    tracer.CFrame = CFrame.new(origin, hitPos) * CFrame.new(0, 0, -distance/2)
    tracer:SetAttribute("Lifetime", 0.1)
    tracer.Parent = Workspace

    table.insert(projectiles, tracer)
end

local function fireHitscan(player, weapon, direction)
    local character = player.Character
    if not character then return end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    if not hrp then return end

    local origin = hrp.Position + Vector3.new(0, 1, 0)
    local pelletCount = weapon.pellets or 1

    for i = 1, pelletCount do
        -- Apply spread
        local spread = weapon.spread or 0
        local spreadDir = direction
        if spread > 0 then
            spreadDir = Vector3.new(
                direction.X + (math.random() - 0.5) * spread * 2,
                direction.Y + (math.random() - 0.5) * spread * 2,
                direction.Z + (math.random() - 0.5) * spread * 2
            ).Unit
        end

        local raycastParams = RaycastParams.new()
        raycastParams.FilterType = Enum.RaycastFilterType.Exclude
        raycastParams.FilterDescendantsInstances = {character}

        local result = Workspace:Raycast(origin, spreadDir * weapon.range, raycastParams)

        local hitPos = origin + spreadDir * weapon.range
        if result then
            hitPos = result.Position
            local hitPlayer = findPlayerFromPart(result.Instance)
            if hitPlayer and hitPlayer ~= player then
                local killed = dealDamage(player, hitPlayer, weapon.damage, false)
                if killed then
                    onPlayerKilled(player, hitPlayer, false)
                end
            end
        end

        createTracer(origin, hitPos)
    end
end

local function fireMelee(player, weapon)
    local character = player.Character
    if not character then return end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    if not hrp then return end

    local origin = hrp.Position
    local aimDir = player:GetAttribute("AimDirection")
    if not aimDir then
        aimDir = hrp.CFrame.LookVector
    end

    -- Check for players in melee range
    for _, other in ipairs(Players:GetPlayers()) do
        if other ~= player then
            local otherPos = getCharacterPosition(other)
            if otherPos then
                local toOther = otherPos - origin
                local distance = toOther.Magnitude

                if distance <= weapon.range then
                    -- Check if roughly facing the target
                    local dot = toOther.Unit:Dot(aimDir)
                    if dot > 0.5 then
                        local killed = dealDamage(player, other, weapon.damage, true)
                        if killed then
                            local isFinalWeapon = player:GetAttribute("CurrentWeapon") == 15
                            onPlayerKilled(player, other, not isFinalWeapon)
                        end
                        return -- Only hit one target
                    end
                end
            end
        end
    end
end

local function spawnProjectile(player, weapon, direction)
    local character = player.Character
    if not character then return end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    if not hrp then return end

    local origin = hrp.Position + Vector3.new(0, 1, 0) + direction * 2

    local projectile = Instance.new("Part")
    projectile.Name = "Projectile"
    projectile.Size = Vector3.new(0.5, 0.5, 2)
    projectile.Position = origin
    projectile.Anchored = false
    projectile.CanCollide = true
    projectile.Color = weapon.name == "Rocket Launcher" and Color3.fromRGB(255, 100, 50) or Color3.fromRGB(139, 90, 43)

    projectile:SetAttribute("Damage", weapon.damage)
    projectile:SetAttribute("Owner", player.Name)
    projectile:SetAttribute("Lifetime", 5)
    projectile:SetAttribute("Splash", weapon.splash or 0)

    -- Set velocity
    projectile.Velocity = direction * weapon.speed
    projectile.Parent = Workspace

    table.insert(projectiles, projectile)
end

local function tryFire(player)
    if gameState ~= "active" then return end

    local data = playerData[player]
    if not data then return end

    local weaponIndex = player:GetAttribute("CurrentWeapon") or 1
    local weapon = WEAPONS[weaponIndex]
    local now = tick()

    -- Check fire rate
    if now - data.lastFireTime < weapon.fireRate then
        return
    end

    -- Get aim direction from attribute (set by frontend)
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

    if not aimDir then return end
    aimDir = aimDir.Unit

    data.lastFireTime = now

    if weapon.type == "hitscan" or weapon.type == "pellet" then
        fireHitscan(player, weapon, aimDir)
    elseif weapon.type == "burst" then
        -- Fire 3 shots rapidly
        fireHitscan(player, weapon, aimDir)
        data.burstRemaining = (weapon.burstCount or 3) - 1
    elseif weapon.type == "dual" then
        fireHitscan(player, weapon, aimDir)
    elseif weapon.type == "projectile" then
        spawnProjectile(player, weapon, aimDir)
    elseif weapon.type == "melee" then
        fireMelee(player, weapon)
    end
end

local function tryMelee(player)
    if gameState ~= "active" then return end

    local data = playerData[player]
    if not data then return end

    local now = tick()

    -- Check melee cooldown
    if now - data.lastMeleeTime < MELEE_COOLDOWN then
        return
    end

    data.lastMeleeTime = now

    local character = player.Character
    if not character then return end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    if not hrp then return end

    local origin = hrp.Position
    local aimDir = player:GetAttribute("AimDirection")
    if not aimDir then
        aimDir = hrp.CFrame.LookVector
    end

    -- Check for players in melee range
    for _, other in ipairs(Players:GetPlayers()) do
        if other ~= player then
            local otherPos = getCharacterPosition(other)
            if otherPos then
                local toOther = otherPos - origin
                local distance = toOther.Magnitude

                if distance <= MELEE_RANGE then
                    local dot = toOther.Unit:Dot(aimDir)
                    if dot > 0.3 then
                        local killed = dealDamage(player, other, MELEE_DAMAGE, true)
                        if killed then
                            onPlayerKilled(player, other, true)
                        end
                        return
                    end
                end
            end
        end
    end
end

--------------------------------------------------------------------------------
-- GAME LOOP
--------------------------------------------------------------------------------

local function updateProjectiles(dt)
    local toRemove = {}

    for i, proj in ipairs(projectiles) do
        if proj and proj.Parent then
            local lifetime = proj:GetAttribute("Lifetime")
            if lifetime then
                lifetime = lifetime - dt
                proj:SetAttribute("Lifetime", lifetime)

                if lifetime <= 0 then
                    table.insert(toRemove, i)
                else
                    -- Check for collision with players (for projectiles)
                    local damage = proj:GetAttribute("Damage")
                    local ownerName = proj:GetAttribute("Owner")
                    local splash = proj:GetAttribute("Splash") or 0

                    if damage and ownerName then
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
                                            local killed = dealDamage(owner, player, damage, false)
                                            if killed then
                                                onPlayerKilled(owner, player, false)
                                            end
                                        end

                                        -- Handle splash damage
                                        if splash > 0 then
                                            for _, otherPlayer in ipairs(Players:GetPlayers()) do
                                                if otherPlayer ~= player and otherPlayer.Name ~= ownerName then
                                                    local otherPos = getCharacterPosition(otherPlayer)
                                                    if otherPos then
                                                        local splashDist = (otherPos - proj.Position).Magnitude
                                                        if splashDist < splash then
                                                            local splashDamage = damage * (1 - splashDist / splash)
                                                            if owner then
                                                                dealDamage(owner, otherPlayer, splashDamage, false)
                                                            end
                                                        end
                                                    end
                                                end
                                            end
                                        end

                                        table.insert(toRemove, i)
                                        break
                                    end
                                end
                            end
                        end
                    end
                end
            end
        else
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
        local data = playerData[player]
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
        local data = playerData[player]
        if data and data.respawnTime > 0 then
            if now >= data.respawnTime then
                respawnPlayer(player)
            end
        end
    end
end

local function updateBursts(dt)
    for _, player in ipairs(Players:GetPlayers()) do
        local data = playerData[player]
        if data and data.burstRemaining > 0 then
            local weapon = WEAPONS[player:GetAttribute("CurrentWeapon")]
            if weapon and weapon.type == "burst" then
                local aimDir = player:GetAttribute("AimDirection")
                if aimDir then
                    fireHitscan(player, weapon, aimDir)
                end
                data.burstRemaining = data.burstRemaining - 1
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

        local meleeAttack = player:GetAttribute("MeleeAttack")
        if meleeAttack then
            tryMelee(player)
            player:SetAttribute("MeleeAttack", false)
        end
    end
end

local function printLeaderboard()
    print("\n=== LEADERBOARD ===")
    local players = Players:GetPlayers()
    table.sort(players, function(a, b)
        return (a:GetAttribute("CurrentWeapon") or 1) > (b:GetAttribute("CurrentWeapon") or 1)
    end)

    for i, player in ipairs(players) do
        local weapon = player:GetAttribute("CurrentWeapon") or 1
        local kills = player:GetAttribute("Kills") or 0
        local deaths = player:GetAttribute("Deaths") or 0
        print(string.format("%d. %s [%s] K/D: %d/%d",
            i, player.Name, WEAPONS[weapon].name, kills, deaths))
    end
    print("===================\n")
end

--------------------------------------------------------------------------------
-- MATCH CONTROL
--------------------------------------------------------------------------------

local countdownTime = 0
local victoryTime = 0
local leaderboardTimer = 0

local function startCountdown()
    gameState = "countdown"
    countdownTime = 5

    -- Reset all players
    for _, player in ipairs(Players:GetPlayers()) do
        initializePlayer(player)
        respawnPlayer(player)
    end

    print("Match starting in 5 seconds...")
end

local function startMatch()
    gameState = "active"
    matchStartTime = tick()
    winner = nil
    print("=== MATCH STARTED! ===")
    print("First to kill with the Golden Knife wins!")
end

local function endMatch()
    gameState = "victory"
    victoryTime = 5

    if winner then
        print("=== " .. winner.Name .. " WINS! ===")
        -- Apply golden effect to winner
        local character = winner.Character
        if character then
            for _, part in ipairs(character:GetChildren()) do
                if part:IsA("Part") or part:IsA("MeshPart") then
                    part.Color = Color3.fromRGB(255, 215, 0)
                    part.Material = Enum.Material.Neon
                end
            end
        end
    end

    printLeaderboard()
end

local function updateMatch(dt)
    if gameState == "waiting" then
        local playerCount = #Players:GetPlayers()
        if playerCount >= 2 then
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
        -- Game is running, handled by other update functions

        -- Periodic leaderboard
        leaderboardTimer = leaderboardTimer + dt
        if leaderboardTimer >= 30 then
            leaderboardTimer = 0
            printLeaderboard()
        end

    elseif gameState == "victory" then
        victoryTime = victoryTime - dt
        if victoryTime <= 0 then
            startCountdown()
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
    playerData[player] = nil
end)

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    updateMatch(dt)
    updateFiring()
    updateBursts(dt)
    updateProjectiles(dt)
    updateHealthRegen(dt)
    updateRespawns()
end)

print("Block Arsenal loaded!")
print("Waiting for players... (minimum 2 to start)")'
);

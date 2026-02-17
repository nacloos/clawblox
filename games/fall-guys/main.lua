-- Fall Guys - Race through spinning platforms, pendulums, and hex tiles
-- Ported from index_v2.html to clawblox Lua
--
-- SCALE = 4 applied to all spatial dimensions so the course fits the
-- default 5-unit-tall player character (HumanoidRootPart 2×5×2).

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")
local AgentInputService = game:GetService("AgentInputService")

--------------------------------------------------------------------------------
-- CONFIGURATION (all lengths/velocities ×4)
--------------------------------------------------------------------------------

local S = 4 -- spatial scale factor

local CFG = {
    gravity = -196.2,
    jumpForce = 18 * S,
    moveSpeed = 8 * S,
    diveForce = 15 * S,
    friction = 0.85,
    airControl = 0.3,
    fallThreshold = -15 * S,
    playerColor = Color3.fromRGB(255, 105, 180),
    aiColors = {
        Color3.fromRGB(255, 217, 61),
        Color3.fromRGB(108, 92, 231),
        Color3.fromRGB(0, 184, 148),
        Color3.fromRGB(225, 112, 85),
        Color3.fromRGB(116, 185, 255),
        Color3.fromRGB(162, 155, 254),
    },
    hexColors = {
        Color3.fromRGB(255, 107, 107),
        Color3.fromRGB(72, 219, 251),
        Color3.fromRGB(254, 202, 87),
        Color3.fromRGB(255, 159, 243),
        Color3.fromRGB(84, 160, 255),
        Color3.fromRGB(95, 39, 205),
    },
}

-- Keep player physics gravity aligned with Fall Guys scaled world tuning.
Workspace.Gravity = math.abs(CFG.gravity)

local FINISH_Z = 250 * S
local SECTION_SPAWNS = {
    Vector3.new(0, 1 * S, 5 * S),
    Vector3.new(0, 1 * S, 5 * S),
    Vector3.new(0, 1 * S, 92 * S),
    Vector3.new(0, 1 * S, 182 * S),
}

local ACCESSORIES = {"crown", "propeller", "headband", "cone"}

--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local gameState = "waiting"
local gameTime = 0
local finishOrder = 0
local playerData = {}
local spinDiscs = {}
local pendulumList = {}
local hexTiles = {}
local aiList = {}
local elapsed = 0

--------------------------------------------------------------------------------
-- HELPERS
--------------------------------------------------------------------------------

local function getHumanoid(player)
    local character = player.Character
    if character then
        return character:FindFirstChild("Humanoid")
    end
    return nil
end

local function getSection(z)
    if z < 22 * S then return 0
    elseif z < 92 * S then return 1
    elseif z < 182 * S then return 2
    else return 3 end
end

local function respawnPosition(z)
    local s = getSection(z) + 1
    local spawn = SECTION_SPAWNS[s]
    return Vector3.new(spawn.X + (math.random() - 0.5) * 4 * S, spawn.Y, spawn.Z)
end

local function makePart(name, size, position, color)
    local part = Instance.new("Part")
    part.Name = name
    part.Size = size
    part.Position = position
    part.Anchored = true
    part.Color = color or Color3.fromRGB(150, 150, 150)
    part:SetAttribute("RenderRole", name)
    part.Parent = Workspace
    return part
end

local function makeCylinder(name, size, position, color)
    local part = Instance.new("Part")
    part.Name = name
    part.Shape = Enum.PartType.Cylinder
    part.Size = size
    part.Position = position
    part.Anchored = true
    part.Color = color or Color3.fromRGB(150, 150, 150)
    part:SetAttribute("RenderRole", name)
    part.Parent = Workspace
    return part
end

local function makeBall(name, size, position, color)
    local part = Instance.new("Part")
    part.Name = name
    part.Shape = Enum.PartType.Ball
    part.Size = size
    part.Position = position
    part.Anchored = true
    part.Color = color or Color3.fromRGB(150, 150, 150)
    part:SetAttribute("RenderRole", name)
    part.Parent = Workspace
    return part
end

--------------------------------------------------------------------------------
-- COURSE BUILDER
--------------------------------------------------------------------------------

local function buildCourse()
    -- Start platform
    makePart("StartPlatform",
        Vector3.new(14 * S, 1 * S, 20 * S),
        Vector3.new(0, -0.5 * S, 10 * S),
        Color3.fromRGB(85, 239, 196))

    -- Section 1: Spinning Discs
    local discDefs = {
        { x = 0 * S,  z = 26 * S, r = 4 * S,   spd = 0.5 },
        { x = 3 * S,  z = 38 * S, r = 3.5 * S, spd = -0.7 },
        { x = -2 * S, z = 50 * S, r = 4 * S,   spd = 0.6 },
        { x = 1 * S,  z = 62 * S, r = 3.5 * S, spd = -0.8 },
        { x = -1 * S, z = 74 * S, r = 4.5 * S, spd = 0.4 },
        { x = 2 * S,  z = 84 * S, r = 3 * S,   spd = -0.9 },
    }

    for i, d in ipairs(discDefs) do
        local diameter = d.r * 2
        local disc = makeCylinder("SpinDisc_" .. i,
            Vector3.new(diameter, 0.5 * S, diameter),
            Vector3.new(d.x, -0.25 * S, d.z),
            Color3.fromRGB(116, 185, 255))
        -- disc.CanCollide stays true: player character controller needs to stand on them
        disc:SetAttribute("DiscRadius", d.r)
        disc:SetAttribute("DiscSpeed", d.spd)
        table.insert(spinDiscs, {
            part = disc,
            speed = d.spd,
            radius = d.r,
            angle = 0,
        })
    end

    -- Transition 1->2
    makePart("Transition1",
        Vector3.new(14 * S, 1 * S, 10 * S),
        Vector3.new(0, -0.5 * S, 95 * S),
        Color3.fromRGB(85, 239, 196))

    -- Section 2: Pendulum Bridge
    makePart("Bridge",
        Vector3.new(5 * S, 0.5 * S, 80 * S),
        Vector3.new(0, -0.25 * S, 140 * S),
        Color3.fromRGB(255, 234, 167))

    -- Bridge rails
    local railL = makePart("BridgeRailL",
        Vector3.new(0.3 * S, 0.5 * S, 80 * S),
        Vector3.new(-2.65 * S, 0.25 * S, 140 * S),
        Color3.fromRGB(253, 203, 110))
    railL.CanCollide = false
    local railR = makePart("BridgeRailR",
        Vector3.new(0.3 * S, 0.5 * S, 80 * S),
        Vector3.new(2.65 * S, 0.25 * S, 140 * S),
        Color3.fromRGB(253, 203, 110))
    railR.CanCollide = false

    -- Pendulums
    local pendZs = {108 * S, 121 * S, 134 * S, 147 * S, 160 * S}
    local armLen = 12.5 * S

    for i, z in ipairs(pendZs) do
        local pillarL = makeCylinder("PendPillarL_" .. i,
            Vector3.new(1.0 * S, 14 * S, 1.0 * S),
            Vector3.new(-4 * S, 7 * S, z),
            Color3.fromRGB(99, 110, 114))
        pillarL.CanCollide = false

        local pillarR = makeCylinder("PendPillarR_" .. i,
            Vector3.new(1.0 * S, 14 * S, 1.0 * S),
            Vector3.new(4 * S, 7 * S, z),
            Color3.fromRGB(99, 110, 114))
        pillarR.CanCollide = false

        local bar = makePart("PendBar_" .. i,
            Vector3.new(9 * S, 0.6 * S, 0.6 * S),
            Vector3.new(0, 14 * S, z),
            Color3.fromRGB(99, 110, 114))
        bar.CanCollide = false

        local arm = makeCylinder("PendArm_" .. i,
            Vector3.new(0.24 * S, armLen, 0.24 * S),
            Vector3.new(0, 14 * S - armLen / 2, z),
            Color3.fromRGB(99, 110, 114))
        arm.CanCollide = false

        local ball = makeBall("PendBall_" .. i,
            Vector3.new(2.6 * S, 2.6 * S, 2.6 * S),
            Vector3.new(0, 14 * S - armLen, z),
            Color3.fromRGB(231, 76, 60))
        -- Main pendulum hazard should physically collide with players.
        -- (Arms/bars stay non-collidable to keep hitboxes fair.)
        ball.CanCollide = true

        local speed = 1.5 + (i - 1) * 0.15
        local phase = (i - 1) * 1.3

        table.insert(pendulumList, {
            arm = arm,
            ball = ball,
            z = z,
            speed = speed,
            phase = phase,
            armLen = armLen,
            pivotY = 14 * S,
        })
    end

    -- Transition 2->3
    makePart("Transition2",
        Vector3.new(14 * S, 1 * S, 10 * S),
        Vector3.new(0, -0.5 * S, 185 * S),
        Color3.fromRGB(85, 239, 196))

    -- Section 3: Hex-a-Gone
    local hexR = 1.5 * S
    local hexH = 0.4 * S
    local hexGap = 0.2 * S
    local cols = 5
    local rows = 16
    local colSp = math.sqrt(3) * hexR + hexGap
    local rowSp = 1.5 * hexR + hexGap
    local startZ = 195 * S

    for r = 0, rows - 1 do
        for c = 0, cols - 1 do
            local colorIdx = ((r + c) % #CFG.hexColors) + 1
            local color = CFG.hexColors[colorIdx]
            local x = (c - (cols - 1) / 2) * colSp + (r % 2 == 1 and colSp / 2 or 0)
            local z = startZ + r * rowSp

            local hex = makeCylinder("HexTile_" .. r .. "_" .. c,
                Vector3.new(hexR * 2, hexH, hexR * 2),
                Vector3.new(x, -hexH / 2, z),
                color)
            -- hex.CanCollide stays true: player character controller needs to stand on them
            table.insert(hexTiles, {
                part = hex,
                originalX = x,
                originalY = -hexH / 2,
                originalZ = z,
                radius = hexR,
                stepped = false,
                stepTime = 0,
                falling = false,
                gone = false,
            })
        end
    end

    -- Finish platform
    makePart("FinishPlatform",
        Vector3.new(14 * S, 1 * S, 10 * S),
        Vector3.new(0, -0.5 * S, FINISH_Z),
        Color3.fromRGB(255, 215, 0))

    -- Finish arch (decorative, no collision)
    local archL = makeCylinder("ArchPillarL",
        Vector3.new(0.6 * S, 8 * S, 0.6 * S),
        Vector3.new(-6 * S, 4 * S, FINISH_Z),
        Color3.fromRGB(255, 215, 0))
    archL.CanCollide = false
    local archR = makeCylinder("ArchPillarR",
        Vector3.new(0.6 * S, 8 * S, 0.6 * S),
        Vector3.new(6 * S, 4 * S, FINISH_Z),
        Color3.fromRGB(255, 215, 0))
    archR.CanCollide = false
    local archBar = makePart("ArchBar",
        Vector3.new(12 * S, 0.8 * S, 0.8 * S),
        Vector3.new(0, 8 * S, FINISH_Z),
        Color3.fromRGB(255, 215, 0))
    archBar.CanCollide = false

    print("[FallGuys] Course built (scale=" .. S .. ")")
end

--------------------------------------------------------------------------------
-- AI BOTS
--------------------------------------------------------------------------------

local function createBots()
    for i = 1, 6 do
        local color = CFG.aiColors[i]
        local accessory = ACCESSORIES[((i - 1) % #ACCESSORIES) + 1]

        local bot = makePart("Bot_" .. i,
            Vector3.new(0.7 * S, 1.2 * S, 0.7 * S),
            Vector3.new((math.random() - 0.5) * 6 * S, 1 * S, (3 + math.random() * 4) * S),
            color)
        bot.CanCollide = false  -- bots use Lua physics
        bot:SetAttribute("IsBot", true)
        bot:SetAttribute("BotIndex", i)
        bot:SetAttribute("Accessory", accessory)
        bot:SetAttribute("BotColor", {color.R, color.G, color.B})

        local baseSpeed = CFG.moveSpeed * (0.55 + math.random() * 0.35)

        table.insert(aiList, {
            part = bot,
            vel = {x = 0, y = 0, z = 0},
            grounded = false,
            finished = false,
            speed = baseSpeed,
            baseSpeed = baseSpeed,
            targetX = 0,
            wanderT = 0,
            jumpT = math.random(),
            stumbleT = 3 + math.random() * 5,
            idx = i,
        })
    end
    print("[FallGuys] 6 AI bots created")
end

--------------------------------------------------------------------------------
-- OBSTACLE UPDATES
--------------------------------------------------------------------------------

local function updateObstacles(dt, time)
    -- Spin discs
    for _, d in ipairs(spinDiscs) do
        d.angle = d.angle + d.speed * dt
        d.part.CFrame = CFrame.new(d.part.Position.X, d.part.Position.Y, d.part.Position.Z)
            * CFrame.Angles(0, d.angle, 0)
    end

    -- Pendulums
    for _, p in ipairs(pendulumList) do
        local angle = math.sin(time * p.speed + p.phase) * (math.pi / 3)

        local ballX = math.sin(angle) * p.armLen
        local ballY = p.pivotY - math.cos(angle) * p.armLen
        p.ball.Position = Vector3.new(ballX, ballY, p.z)

        local armCenterX = math.sin(angle) * p.armLen / 2
        local armCenterY = p.pivotY - math.cos(angle) * p.armLen / 2
        p.arm.CFrame = CFrame.new(armCenterX, armCenterY, p.z)
            * CFrame.Angles(0, 0, angle)
    end

    -- Hex tiles
    for _, h in ipairs(hexTiles) do
        if h.gone then continue end

        if h.stepped and not h.falling then
            local el = (elapsed - h.stepTime)
            if el < 1.0 then
                h.part.Position = Vector3.new(
                    h.originalX + (math.random() - 0.5) * 0.04 * S,
                    h.originalY,
                    h.originalZ + (math.random() - 0.5) * 0.04 * S
                )
                h.part.Transparency = el * 0.3
            else
                h.falling = true
                h.fallY = h.originalY
            end
        end

        if h.falling then
            h.fallY = h.fallY - 8 * S * dt
            h.fallOpacity = (h.fallOpacity or 1) - dt
            h.part.Position = Vector3.new(h.originalX, h.fallY, h.originalZ)
            h.part.Transparency = 1 - math.max(0, h.fallOpacity)
            if h.fallY < -20 * S then
                h.gone = true
                h.part.Transparency = 1
            end
        end

    end
end

--------------------------------------------------------------------------------
-- COLLISION (simple ground check for bots)
--------------------------------------------------------------------------------

local staticPlatforms = {
    {x = 0, y = -0.5 * S, z = 10 * S, w = 14 * S, h = 1 * S, d = 20 * S},
    {x = 0, y = -0.5 * S, z = 95 * S, w = 14 * S, h = 1 * S, d = 10 * S},
    {x = 0, y = -0.25 * S, z = 140 * S, w = 5 * S, h = 0.5 * S, d = 80 * S},
    {x = 0, y = -0.5 * S, z = 185 * S, w = 14 * S, h = 1 * S, d = 10 * S},
    {x = 0, y = -0.5 * S, z = FINISH_Z, w = 14 * S, h = 1 * S, d = 10 * S},
}

local function checkGroundSimple(px, py, pz)
    for _, p in ipairs(staticPlatforms) do
        local minX = p.x - p.w / 2
        local maxX = p.x + p.w / 2
        local minZ = p.z - p.d / 2
        local maxZ = p.z + p.d / 2
        local topY = p.y + p.h / 2

        if px >= minX and px <= maxX and pz >= minZ and pz <= maxZ then
            if py <= topY + 0.15 * S and py >= topY - 0.5 * S then
                return true, topY, nil
            end
        end
    end

    -- Check discs
    for _, d in ipairs(spinDiscs) do
        local dp = d.part.Position
        local dx = px - dp.X
        local dz = pz - dp.Z
        if math.sqrt(dx * dx + dz * dz) < d.radius - 0.2 * S then
            local surfY = dp.Y + 0.25 * S
            if py <= surfY + 0.15 * S and py >= surfY - 0.5 * S then
                return true, surfY, d
            end
        end
    end

    -- Check hex tiles
    for _, h in ipairs(hexTiles) do
        if h.gone then continue end
        local hp = h.part.Position
        local dx = px - hp.X
        local dz = pz - hp.Z
        if math.sqrt(dx * dx + dz * dz) < h.radius * 0.8 then
            local surfY = hp.Y + 0.2 * S
            if py <= surfY + 0.15 * S and py >= surfY - 0.5 * S then
                if not h.stepped then
                    h.stepped = true
                    h.stepTime = elapsed
                end
                return true, surfY, nil
            end
        end
    end

    return false, 0, nil
end

local function checkPendulumHit(px, py, pz, time)
    for _, p in ipairs(pendulumList) do
        local angle = math.sin(time * p.speed + p.phase) * (math.pi / 3)
        local hx = math.sin(angle) * p.armLen
        local hy = p.pivotY - math.cos(angle) * p.armLen
        local hz = p.z
        local dx = px - hx
        local dy = (py + 0.7 * S) - hy
        local dz = pz - hz
        local dist = math.sqrt(dx * dx + dy * dy + dz * dz)
        if dist < 2.3 * S then
            local nx = dx / (dist + 0.01)
            local nz = dz / (dist + 0.01)
            return true, nx * 18 * S, 5 * S, nz * 4 * S
        end
    end
    return false, 0, 0, 0
end

--------------------------------------------------------------------------------
-- AI UPDATE
--------------------------------------------------------------------------------

local function updateAI(dt, time)
    if gameState ~= "playing" then return end

    local allZ = {}
    for _, player in ipairs(Players:GetPlayers()) do
        local char = player.Character
        if char then
            local hrp = char:FindFirstChild("HumanoidRootPart")
            if hrp then table.insert(allZ, hrp.Position.Z) end
        end
    end
    for _, ai in ipairs(aiList) do
        table.insert(allZ, ai.part.Position.Z)
    end

    local maxZ, minZ = -math.huge, math.huge
    for _, z in ipairs(allZ) do
        if z > maxZ then maxZ = z end
        if z < minZ then minZ = z end
    end

    for _, ai in ipairs(aiList) do
        if ai.finished then continue end

        local pos = ai.part.Position
        local px, py, pz = pos.X, pos.Y, pos.Z
        local vx, vy, vz = ai.vel.x, ai.vel.y, ai.vel.z

        local section = getSection(pz)

        -- Rubber-banding
        local relPos = (pz - minZ) / (maxZ - minZ + 1)
        local rubberband = 1.0
        if relPos > 0.7 then rubberband = 0.85
        elseif relPos < 0.3 then rubberband = 1.2 end
        ai.speed = ai.baseSpeed * rubberband

        -- Section-aware targeting
        if section == 1 then
            local bestDisc = nil
            local bestDist = math.huge
            for _, d in ipairs(spinDiscs) do
                local dz2 = d.part.Position.Z - pz
                if dz2 > -2 * S and dz2 < 15 * S and dz2 < bestDist then
                    bestDist = dz2
                    bestDisc = d
                end
            end
            if bestDisc then
                ai.targetX = bestDisc.part.Position.X
                if ai.grounded and bestDist > 3 * S and ai.jumpT <= 0 then
                    vy = CFG.jumpForce
                    ai.grounded = false
                    ai.jumpT = 0.8
                end
            end
        elseif section == 2 then
            ai.targetX = (math.random() - 0.5) * 2 * S
        else
            ai.targetX = (math.random() - 0.5) * 5 * S
        end

        vz = vz + ai.speed * dt * 5
        ai.wanderT = ai.wanderT - dt
        if ai.wanderT <= 0 then ai.wanderT = 1 + math.random() * 2 end
        vx = vx + (ai.targetX - px) * dt * 4

        ai.jumpT = ai.jumpT - dt
        if ai.jumpT <= 0 and ai.grounded then
            vy = CFG.jumpForce * (0.8 + math.random() * 0.3)
            ai.grounded = false
            ai.jumpT = 0.5 + math.random() * 1.5
        end

        ai.stumbleT = ai.stumbleT - dt
        if ai.stumbleT <= 0 then
            vx = vx + (math.random() - 0.5) * 8 * S
            vz = vz - math.random() * 3 * S
            ai.stumbleT = 3 + math.random() * 5
        end

        vy = vy + CFG.gravity * dt
        if ai.grounded then
            vx = vx * CFG.friction
            vz = vz * 0.9
        else
            vx = vx * 0.98
            vz = vz * 0.98
        end

        local hs = math.sqrt(vx * vx + vz * vz)
        if hs > ai.speed * 1.5 then
            vx = vx * ai.speed * 1.5 / hs
            vz = vz * ai.speed * 1.5 / hs
        end

        px = px + vx * dt
        py = py + vy * dt
        pz = pz + vz * dt

        -- Disc carry
        local grounded, surfY, onDisc = checkGroundSimple(px, py, pz)
        if grounded then
            py = surfY
            vy = 0
        end
        ai.grounded = grounded

        if onDisc then
            local rd = onDisc.speed * dt
            local dx2 = px - onDisc.part.Position.X
            local dz2 = pz - onDisc.part.Position.Z
            px = onDisc.part.Position.X + dx2 * math.cos(rd) - dz2 * math.sin(rd)
            pz = onDisc.part.Position.Z + dx2 * math.sin(rd) + dz2 * math.cos(rd)
        end

        -- Pendulum hit
        local hit, hitVx, hitVy, hitVz = checkPendulumHit(px, py, pz, time)
        if hit then
            vx = vx + hitVx
            vy = vy + hitVy
            vz = vz + hitVz
        end

        -- Fall respawn
        if py < CFG.fallThreshold then
            local spawnPos = respawnPosition(pz)
            px, py, pz = spawnPos.X, spawnPos.Y, spawnPos.Z
            vx, vy, vz = 0, 0, 0
        end

        -- Update
        ai.part.Position = Vector3.new(px, py, pz)
        ai.vel.x, ai.vel.y, ai.vel.z = vx, vy, vz

        -- Rotation (face movement direction)
        if vz > 0.5 * S or math.abs(vx) > 0.5 * S then
            local rot = math.atan2(vx, vz)
            ai.part.CFrame = CFrame.new(px, py, pz) * CFrame.Angles(0, rot, 0)
        end

        -- Finish detection
        if pz >= FINISH_Z - 3 * S then
            ai.finished = true
            finishOrder = finishOrder + 1
            ai.part:SetAttribute("FinishPlace", finishOrder)
        end

    end
end

--------------------------------------------------------------------------------
-- PLAYER MANAGEMENT
--------------------------------------------------------------------------------

local function setupPlayer(player)
    playerData[player.UserId] = {
        name = player.Name,
        section = 0,
        finished = false,
        place = 0,
    }

    local humanoid = getHumanoid(player)
    if humanoid then
    humanoid.WalkSpeed = CFG.moveSpeed * 2
    humanoid.JumpPower = CFG.jumpForce
    end

    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            hrp.Position = Vector3.new(
                (math.random() - 0.5) * 4 * S, 1 * S, (3 + math.random() * 4) * S
            )
            hrp:SetAttribute("ModelYawOffsetDeg", 180)
        end
    end

    -- Set initial attributes
    player:SetAttribute("Section", 0)
    player:SetAttribute("Place", 0)
    player:SetAttribute("Progress", 0)
    player:SetAttribute("Timer", "00:00.000")
    player:SetAttribute("GameState", gameState)
    player:SetAttribute("PlayerColor", {CFG.playerColor.R, CFG.playerColor.G, CFG.playerColor.B})

    print("[FallGuys] Player joined: " .. player.Name)
end

local function cleanupPlayer(player)
    playerData[player.UserId] = nil
end

--------------------------------------------------------------------------------
-- INPUT HANDLING
--------------------------------------------------------------------------------

if AgentInputService then
    AgentInputService.InputReceived:Connect(function(player, inputType, inputData)
        if not playerData[player.UserId] then return end

        local humanoid = getHumanoid(player)
        if not humanoid then return end

        if inputType == "MoveTo" and inputData and inputData.position then
            local pos = inputData.position
            humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
        elseif inputType == "Stop" then
            humanoid:CancelMoveTo()
        elseif inputType == "Jump" then
            humanoid.Jump = true
        elseif inputType == "Dive" then
            local character = player.Character
            if character then
                local hrp = character:FindFirstChild("HumanoidRootPart")
                if hrp then
                    local fwd = hrp.CFrame.LookVector
                    hrp.Velocity = Vector3.new(
                        fwd.X * CFG.diveForce,
                        -CFG.diveForce * 0.4,
                        fwd.Z * CFG.diveForce
                    )
                end
            end
        end
    end)
end

--------------------------------------------------------------------------------
-- GAME STATE MANAGEMENT
--------------------------------------------------------------------------------

local function formatTime(ms)
    local m = math.floor(ms / 60000)
    local s = math.floor((ms % 60000) / 1000)
    local ml = math.floor(ms % 1000)
    return string.format("%02d:%02d.%03d", m, s, ml)
end

local function getPlaceSuffix(n)
    if n == 1 then return "st"
    elseif n == 2 then return "nd"
    elseif n == 3 then return "rd"
    else return "th" end
end

local function startCountdown()
    if gameState ~= "waiting" then return end
    gameState = "countdown"

    for _, player in ipairs(Players:GetPlayers()) do
        player:SetAttribute("GameState", "countdown")
    end

    wait(1)
    wait(1)
    wait(1)

    gameState = "playing"
    gameTime = 0
    finishOrder = 0

    for _, player in ipairs(Players:GetPlayers()) do
        player:SetAttribute("GameState", "playing")
    end

    print("[FallGuys] Race started!")
end

local function resetGame()
    gameState = "waiting"
    gameTime = 0
    finishOrder = 0

    for _, ai in ipairs(aiList) do
        ai.part.Position = Vector3.new(
            (math.random() - 0.5) * 6 * S, 1 * S, (3 + math.random() * 4) * S
        )
        ai.vel = {x = 0, y = 0, z = 0}
        ai.finished = false
        ai.stumbleT = math.random() * 5
        ai.part:SetAttribute("FinishPlace", 0)
    end

    for _, h in ipairs(hexTiles) do
        h.part.Position = Vector3.new(h.originalX, h.originalY, h.originalZ)
        h.stepped = false
        h.stepTime = 0
        h.falling = false
        h.gone = false
        h.fallY = nil
        h.fallOpacity = nil
        h.part.Transparency = 0
    end

    for _, player in ipairs(Players:GetPlayers()) do
        local pdata = playerData[player.UserId]
        if pdata then
            pdata.section = 0
            pdata.finished = false
            pdata.place = 0
        end
        player:SetAttribute("GameState", "waiting")
        player:SetAttribute("Section", 0)
        player:SetAttribute("Place", 0)
        player:SetAttribute("Progress", 0)

        local character = player.Character
        if character then
            local hrp = character:FindFirstChild("HumanoidRootPart")
            if hrp then
                hrp.Position = Vector3.new(
                    (math.random() - 0.5) * 4 * S, 1 * S, (3 + math.random() * 4) * S
                )
            end
        end
    end

    print("[FallGuys] Game reset")
end

--------------------------------------------------------------------------------
-- MAIN HEARTBEAT
--------------------------------------------------------------------------------

local autoStartTimer = 5
local tick = 0

RunService.Heartbeat:Connect(function(dt)
    elapsed = elapsed + dt
    tick = tick + 1
    local time = elapsed

    if gameState == "waiting" then
        local playerCount = #Players:GetPlayers()
        if playerCount > 0 then
            autoStartTimer = autoStartTimer - dt
            if autoStartTimer <= 0 then
                autoStartTimer = 999
                startCountdown()
            end
        end
    end

    if gameState == "playing" then
        gameTime = gameTime + dt * 1000
    end

    updateObstacles(dt, time)
    updateAI(dt, time)

    if gameState == "playing" then
        for _, player in ipairs(Players:GetPlayers()) do
            local pdata = playerData[player.UserId]
            if not pdata or pdata.finished then continue end

            local character = player.Character
            if not character then continue end
            local hrp = character:FindFirstChild("HumanoidRootPart")
            if not hrp then continue end

            local pz = hrp.Position.Z

            local section = getSection(pz)
            if section ~= pdata.section then
                pdata.section = section
                player:SetAttribute("Section", section)
            end

            if tick % 6 == 0 then
                local ahead = 0
                for _, ai in ipairs(aiList) do
                    if ai.part.Position.Z > pz then ahead = ahead + 1 end
                end
                local place = ahead + 1
                player:SetAttribute("Place", place)

                local progress = math.max(0, math.min(100, (pz / FINISH_Z) * 100))
                player:SetAttribute("Progress", math.floor(progress))

                player:SetAttribute("Timer", formatTime(gameTime))
            end

            if hrp.Position.Y < CFG.fallThreshold then
                local spawnPos = respawnPosition(pz)
                hrp.Position = spawnPos
            end

            for _, h in ipairs(hexTiles) do
                if h.gone or h.stepped then continue end
                local hp = h.part.Position
                local dx = hrp.Position.X - hp.X
                local dz = hrp.Position.Z - hp.Z
                local dist = math.sqrt(dx * dx + dz * dz)
                if dist < h.radius * 0.8 then
                    local dy = hrp.Position.Y - hp.Y
                    if dy >= -0.5 * S and dy <= 0.5 * S then
                        h.stepped = true
                        h.stepTime = elapsed
                    end
                end
            end

            if pz >= FINISH_Z - 3 * S and not pdata.finished then
                pdata.finished = true
                finishOrder = finishOrder + 1
                pdata.place = finishOrder
                player:SetAttribute("Place", finishOrder)
                player:SetAttribute("GameState", "finished")
                player:SetAttribute("FinishTime", formatTime(gameTime))
                print("[FallGuys] " .. player.Name .. " finished in " .. finishOrder .. getPlaceSuffix(finishOrder) .. " place!")
            end

        end
    end

end)

--------------------------------------------------------------------------------
-- INITIALIZATION
--------------------------------------------------------------------------------

Players.PlayerAdded:Connect(setupPlayer)
Players.PlayerRemoving:Connect(cleanupPlayer)

for _, player in ipairs(Players:GetPlayers()) do
    setupPlayer(player)
end

buildCourse()
createBots()

print("=== Fall Guys (scale=" .. S .. "x) ===")
print("Section 1 (Z " .. 22*S .. "-" .. 92*S .. "):  Spinning Discs")
print("Section 2 (Z " .. 92*S .. "-" .. 182*S .. "): Pendulum Bridge")
print("Section 3 (Z " .. 182*S .. "+):   Hex-a-Gone")
print("Finish at Z=" .. FINISH_Z)

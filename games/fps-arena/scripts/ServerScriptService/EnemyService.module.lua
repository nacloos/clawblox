local Players = game:GetService("Players")

local EnemyService = {}

local config
local state
local spawnService

local enemies = {}
local nextEnemyId = 1
local wave = 0

local enemyByType = {
    grunt = {
        health = 70,
        speed = 4.2,
        damage = 9,
        attackCooldown = 0.9,
        score = 20,
        size = 1.0,
        bodyColor = Color3.new(0.85, 0.22, 0.22),
        eyeColor = Color3.new(1.0, 0.2, 0.2),
    },
    brute = {
        health = 140,
        speed = 3.0,
        damage = 16,
        attackCooldown = 1.2,
        score = 50,
        size = 1.3,
        bodyColor = Color3.new(0.9, 0.45, 0.15),
        eyeColor = Color3.new(1.0, 0.45, 0.0),
    },
    elite = {
        health = 90,
        speed = 5.0,
        damage = 11,
        attackCooldown = 0.7,
        score = 45,
        size = 0.9,
        bodyColor = Color3.new(0.2, 0.45, 1.0),
        eyeColor = Color3.new(0.35, 0.65, 1.0),
    },
}

local function setRender(part, role, presetId, primitive, material, color)
    part:SetAttribute("RenderRole", role)
    part:SetAttribute("RenderPresetId", presetId)
    part:SetAttribute("RenderPrimitive", primitive)
    part:SetAttribute("RenderMaterial", material)
    part:SetAttribute("RenderColor", color)
    part:SetAttribute("RenderStatic", false)
    part:SetAttribute("RenderVisible", true)
    part:SetAttribute("RenderCastsShadow", true)
    part:SetAttribute("RenderReceivesShadow", true)
    part.CanCollide = false
    part.Anchored = true
end

local function makeEnemyPart(name, size)
    local p = Instance.new("Part")
    p.Name = name
    p.Size = size
    p.Parent = Workspace
    return p
end

local function placeEnemyParts(enemy)
    local c = enemy.pos
    local s = enemy.cfg.size
    local yaw = enemy.yaw
    local right = Vector3.new(math.cos(yaw), 0, -math.sin(yaw))

    enemy.body.Position = c + Vector3.new(0, 1.1 * s, 0)
    enemy.body.CFrame = CFrame.new(enemy.body.Position) * CFrame.Angles(0, yaw, 0)

    enemy.head.Position = c + Vector3.new(0, 1.9 * s, 0)
    enemy.eyeL.Position = enemy.head.Position + right * -0.11 * s + Vector3.new(0, 0, 0.16 * s)
    enemy.eyeR.Position = enemy.head.Position + right * 0.11 * s + Vector3.new(0, 0, 0.16 * s)

    enemy.armL.Position = c + Vector3.new(-0.34 * s, 1.1 * s, 0)
    enemy.armR.Position = c + Vector3.new(0.34 * s, 1.1 * s, 0)
    enemy.armL.CFrame = CFrame.new(enemy.armL.Position) * CFrame.Angles(0, 0, math.rad(22))
    enemy.armR.CFrame = CFrame.new(enemy.armR.Position) * CFrame.Angles(0, 0, math.rad(-22))
end

local function removeEnemy(enemy)
    for _, part in ipairs(enemy.parts) do
        if part and part.Parent then
            part:Destroy()
        end
    end
    enemies[enemy.id] = nil
end

local function alivePlayers()
    local out = {}
    for _, player in ipairs(Players:GetPlayers()) do
        local pdata = state.GetPlayer(player)
        if pdata and pdata.alive and player.Character then
            local root = player.Character:FindFirstChild("HumanoidRootPart")
            if root then
                table.insert(out, { player = player, pdata = pdata, root = root })
            end
        end
    end
    return out
end

local function chooseEnemyType()
    if wave >= 5 and math.random() < 0.20 then
        return "elite"
    end
    if wave >= 3 and math.random() < 0.30 then
        return "brute"
    end
    return "grunt"
end

local function spawnEnemyAt(enemyType, pos)
    local cfg = enemyByType[enemyType]
    if not cfg then
        return
    end

    local id = nextEnemyId
    nextEnemyId = nextEnemyId + 1

    local s = cfg.size
    local body = makeEnemyPart("EnemyBody", Vector3.new(0.72 * s, 1.45 * s, 0.72 * s))
    local head = makeEnemyPart("EnemyHead", Vector3.new(0.40 * s, 0.40 * s, 0.40 * s))
    local eyeL = makeEnemyPart("EnemyEyeL", Vector3.new(0.08 * s, 0.08 * s, 0.08 * s))
    local eyeR = makeEnemyPart("EnemyEyeR", Vector3.new(0.08 * s, 0.08 * s, 0.08 * s))
    local armL = makeEnemyPart("EnemyArmL", Vector3.new(0.16 * s, 0.56 * s, 0.16 * s))
    local armR = makeEnemyPart("EnemyArmR", Vector3.new(0.16 * s, 0.56 * s, 0.16 * s))

    body.Shape = Enum.PartType.Block
    head.Shape = Enum.PartType.Ball
    eyeL.Shape = Enum.PartType.Ball
    eyeR.Shape = Enum.PartType.Ball
    armL.Shape = Enum.PartType.Block
    armR.Shape = Enum.PartType.Block

    setRender(body, "enemy_body", "fps_arena/enemy_body_" .. enemyType, "capsule", "Metal", cfg.bodyColor)
    setRender(head, "enemy_head", "fps_arena/enemy_head", "sphere", "Concrete", Color3.new(0.87, 0.80, 0.73))
    setRender(eyeL, "enemy_eye", "fps_arena/enemy_eye", "sphere", "Neon", cfg.eyeColor)
    setRender(eyeR, "enemy_eye", "fps_arena/enemy_eye", "sphere", "Neon", cfg.eyeColor)
    setRender(armL, "enemy_arm", "fps_arena/enemy_arm_" .. enemyType, "capsule", "Metal", cfg.bodyColor)
    setRender(armR, "enemy_arm", "fps_arena/enemy_arm_" .. enemyType, "capsule", "Metal", cfg.bodyColor)

    local enemy = {
        id = id,
        type = enemyType,
        cfg = cfg,
        health = cfg.health,
        pos = pos,
        yaw = 0,
        nextAttackAt = 0,
        body = body,
        head = head,
        eyeL = eyeL,
        eyeR = eyeR,
        armL = armL,
        armR = armR,
        parts = { body, head, eyeL, eyeR, armL, armR },
    }

    for _, p in ipairs(enemy.parts) do
        p:SetAttribute("EnemyId", id)
        p:SetAttribute("EnemyType", enemyType)
    end

    placeEnemyParts(enemy)
    enemies[id] = enemy
end

local function spawnWave()
    wave = wave + 1
    local count = math.min(config.WAVE_BASE_ENEMIES + wave * config.WAVE_ENEMY_STEP, config.WAVE_MAX_ENEMIES)
    local radius = config.MAP_SIZE * 0.44
    for i = 1, count do
        local t = ((i - 1) / math.max(1, count)) * math.pi * 2 + math.random() * 0.35
        local x = math.cos(t) * radius
        local z = math.sin(t) * radius
        spawnEnemyAt(chooseEnemyType(), Vector3.new(x, 0.0, z))
    end
    print("[fps-arena] wave", wave, "spawned", count, "enemies")
end

local function applyEnemyDamageToPlayer(enemy, target, now)
    local nextHp = math.max(0, target.pdata.health - enemy.cfg.damage)
    target.pdata.health = nextHp

    local humanoid = target.root.Parent and target.root.Parent:FindFirstChild("Humanoid")
    if humanoid then
        humanoid.Health = nextHp
    end

    if nextHp <= 0 and target.pdata.alive then
        target.pdata.alive = false
        target.pdata.deaths = target.pdata.deaths + 1
        spawnService.ScheduleRespawn(target.player, now)
    end
end

function EnemyService.Init(deps)
    config = deps.config
    state = deps.state
    spawnService = deps.spawnService
end

function EnemyService.Reset()
    for _, enemy in pairs(enemies) do
        removeEnemy(enemy)
    end
    enemies = {}
    wave = 0
end

function EnemyService.GetWave()
    return wave
end

function EnemyService.GetAliveCount()
    local n = 0
    for _ in pairs(enemies) do
        n = n + 1
    end
    return n
end

function EnemyService.DamageEnemy(enemyId, damage, attacker)
    local enemy = enemies[enemyId]
    if not enemy then
        return false
    end

    enemy.health = enemy.health - damage
    if enemy.health > 0 then
        return false
    end

    removeEnemy(enemy)
    if attacker then
        local pdata = state.GetPlayer(attacker)
        if pdata then
            pdata.kills = pdata.kills + 1
            pdata.score = pdata.score + enemy.cfg.score
        end
    end
    return true
end

function EnemyService.TryDamageFromHitPart(hitPart, damage, attacker)
    if not hitPart then
        return false
    end
    local enemyId = hitPart:GetAttribute("EnemyId")
    if type(enemyId) ~= "number" then
        return false
    end
    EnemyService.DamageEnemy(enemyId, damage, attacker)
    return true
end

function EnemyService.Tick(now, dt)
    if state.GetRound().phase ~= "active" then
        return
    end

    if EnemyService.GetAliveCount() == 0 then
        spawnWave()
    end

    local targets = alivePlayers()
    if #targets == 0 then
        return
    end

    local bound = config.MAP_SIZE * 0.47

    for _, enemy in pairs(enemies) do
        local best, bestDist = nil, 1e9
        for _, t in ipairs(targets) do
            local delta = t.root.Position - enemy.pos
            local d = delta.Magnitude
            if d < bestDist then
                bestDist = d
                best = t
            end
        end

        if best then
            local toTarget = best.root.Position - enemy.pos
            local flat = Vector3.new(toTarget.X, 0, toTarget.Z)
            local dist = flat.Magnitude
            if dist > 0.001 then
                local dir = flat.Unit
                enemy.yaw = math.atan2(dir.X, dir.Z)
                local step = math.min(dist, enemy.cfg.speed * dt)
                enemy.pos = enemy.pos + dir * step
                enemy.pos = Vector3.new(
                    math.max(-bound, math.min(bound, enemy.pos.X)),
                    0.0,
                    math.max(-bound, math.min(bound, enemy.pos.Z))
                )
            end

            if bestDist <= config.ENEMY_ATTACK_RANGE and now >= enemy.nextAttackAt then
                enemy.nextAttackAt = now + enemy.cfg.attackCooldown
                applyEnemyDamageToPlayer(enemy, best, now)
            end
        end

        placeEnemyParts(enemy)
    end
end

return EnemyService

local Enemies = {}

local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
local Config = require(ModulesFolder:WaitForChild("Config", 2))
local State = require(ModulesFolder:WaitForChild("State", 2))

local function makeEnemyModel(enemy_id, wave_index, spawn_position)
    local health = Config.WAVES.BASE_HEALTH + (wave_index - 1) * Config.WAVES.HEALTH_INCREMENT

    local model = Instance.new("Model")
    model.Name = "Zombie_" .. tostring(enemy_id)

    local root = Instance.new("Part")
    root.Name = "ZombieRoot_" .. tostring(enemy_id)
    root.Size = Vector3.new(2, 5, 2)
    root.Position = spawn_position
    root.Anchored = true
    root.Color = Color3.new(0.2, 0.7, 0.2)
    root:SetAttribute("EnemyId", enemy_id)
    root:SetAttribute("EnemyType", "Zombie")
    root:SetAttribute("Health", health)
    root:SetAttribute("Alive", true)
    root.Parent = model

    local humanoid = Instance.new("Humanoid")
    humanoid.Name = "Humanoid"
    humanoid.MaxHealth = health
    humanoid.Health = health
    humanoid.WalkSpeed = Config.ENEMIES.SPEED
    humanoid.Parent = model

    model.PrimaryPart = root
    model:SetAttribute("EnemyId", enemy_id)
    model:SetAttribute("EnemyType", "Zombie")
    model:SetAttribute("Health", health)
    model:SetAttribute("MaxHealth", health)
    model:SetAttribute("Alive", true)
    model.Parent = Workspace

    return model, root, humanoid, health
end

function Enemies.spawnWave(wave_index)
    local count = Config.WAVES.BASE_ENEMIES + (wave_index - 1) * Config.WAVES.ENEMY_INCREMENT

    for i = 1, count do
        local enemy_id = State.next_enemy_id
        State.next_enemy_id = State.next_enemy_id + 1

        local spawn = Config.ENEMIES.SPAWN_POINTS[((i - 1) % #Config.ENEMIES.SPAWN_POINTS) + 1]
        local model, root, humanoid, health = makeEnemyModel(enemy_id, wave_index, spawn)

        State.enemies[enemy_id] = {
            id = enemy_id,
            model = model,
            root = root,
            humanoid = humanoid,
            health = health,
            speed = Config.ENEMIES.SPEED,
            alive = true,
            last_contact_at = 0,
        }
    end

    return count
end

function Enemies.aliveCount()
    local count = 0
    for _, enemy in pairs(State.enemies) do
        if enemy.alive then
            count = count + 1
        end
    end
    return count
end

function Enemies.forEachAlive(callback)
    for _, enemy in pairs(State.enemies) do
        if enemy.alive then
            callback(enemy)
        end
    end
end

function Enemies.findEnemyIdFromInstance(inst)
    local current = inst
    while current do
        local id = current:GetAttribute("EnemyId")
        if id ~= nil then
            return tonumber(id)
        end
        current = current.Parent
    end
    return nil
end

function Enemies.applyDamage(enemy_id, amount)
    local enemy = State.enemies[enemy_id]
    if not enemy or not enemy.alive then
        return 0, false
    end

    local applied = math.min(enemy.health, amount)
    enemy.health = enemy.health - applied

    if enemy.model then
        enemy.model:SetAttribute("Health", enemy.health)
        if enemy.health <= 0 then
            enemy.model:SetAttribute("Alive", false)
        end
    end
    if enemy.root then
        enemy.root:SetAttribute("Health", enemy.health)
        if enemy.health <= 0 then
            enemy.root:SetAttribute("Alive", false)
        end
    end

    if enemy.humanoid then
        enemy.humanoid.Health = enemy.health
    end

    local killed = false
    if enemy.health <= 0 then
        enemy.alive = false
        killed = true

        local model = enemy.model
        task.delay(Config.ENEMIES.DESTROY_DELAY, function()
            if model and model.Parent then
                model:Destroy()
            end
        end)
    end

    return applied, killed
end

function Enemies.clearAll()
    for _, enemy in pairs(State.enemies) do
        if enemy.model and enemy.model.Parent then
            enemy.model:Destroy()
        end
    end
    State.enemies = {}
end

return Enemies

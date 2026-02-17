local RaycastCombat = {}

local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
local Config = require(ModulesFolder:WaitForChild("Config", 2))
local State = require(ModulesFolder:WaitForChild("State", 2))
local Weapons = require(ModulesFolder:WaitForChild("Weapons", 2))
local Enemies = require(ModulesFolder:WaitForChild("Enemies", 2))
local Markers = require(ModulesFolder:WaitForChild("Markers", 2))

local function parseTarget(data)
    if not data or not data.target then
        return nil
    end
    local t = data.target
    local x = tonumber(t[1])
    local y = tonumber(t[2])
    local z = tonumber(t[3])
    if not x or not y or not z then
        return nil
    end
    return Vector3.new(x, y, z)
end

local function isSelfHit(player, hit_instance)
    local character = player.Character
    if not character or not hit_instance then
        return false
    end
    return hit_instance:IsDescendantOf(character)
end

local function applySpread(direction, spread_deg)
    if spread_deg <= 0 then
        return direction
    end
    local spread = math.rad(spread_deg)
    local jitter = Vector3.new(
        (math.random() * 2 - 1) * spread,
        (math.random() * 2 - 1) * spread,
        (math.random() * 2 - 1) * spread
    )
    return (direction + jitter).Unit
end

local function fireAnimationIdForWeapon(weapon_name)
    if weapon_name == "Shotgun" then
        return "local://fire_shotgun"
    end
    return "local://fire_rifle"
end

local function playFireAnimation(character, player_state, weapon_name)
    local humanoid = character:FindFirstChild("Humanoid")
    if not humanoid then
        return
    end

    local tracks = player_state.animation_tracks
    if not tracks then
        tracks = {}
        player_state.animation_tracks = tracks
    end

    local track = tracks[weapon_name]
    if not track then
        local animation = Instance.new("Animation")
        animation.AnimationId = fireAnimationIdForWeapon(weapon_name)
        local ok, loaded_track = pcall(function()
            return humanoid:LoadAnimation(animation)
        end)
        if not ok or not loaded_track then
            return
        end
        tracks[weapon_name] = loaded_track
        track = loaded_track
    end

    pcall(function()
        track:Play()
    end)
end

function RaycastCombat.fire(player, data)
    if State.phase ~= "Active" then
        return false, "RoundNotActive"
    end

    local player_state = State.getPlayerState(player)
    if not player_state then
        return false, "UnknownPlayer"
    end

    local character = player.Character
    if not character then
        return false, "NoCharacter"
    end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    if not hrp then
        return false, "NoHumanoidRootPart"
    end

    local target = parseTarget(data)
    if not target then
        return false, "InvalidTarget"
    end

    local weapon_name = player_state.weapon or "Rifle"
    if data and type(data.weapon) == "string" and Weapons.isValid(data.weapon) then
        weapon_name = data.weapon
        player_state.weapon = weapon_name
        player:SetAttribute("CurrentWeapon", weapon_name)
    end

    local now = State.now()
    local can_fire, reason = Weapons.canFire(player_state, weapon_name, now)
    if not can_fire then
        return false, reason
    end

    Weapons.recordShot(player_state, now)
    if not Weapons.withinFireCap(player_state) then
        return false, "RateLimited"
    end

    local weapon = Weapons.getConfig(weapon_name)
    if not weapon then
        return false, "UnknownWeapon"
    end

    local origin = hrp.Position + Vector3.new(0, 1.4, 0)
    local base_dir = (target - origin)
    if base_dir.Magnitude <= 0.001 then
        return false, "ZeroDirection"
    end

    local total_damage = 0
    local kills = 0
    local last_hit_enemy_id = 0

    for _ = 1, weapon.pellets do
        local dir = applySpread(base_dir.Unit, weapon.spread_deg)
        local result = Workspace:Raycast(origin, dir * weapon.range)
        if result and result.Instance and not isSelfHit(player, result.Instance) then
            local enemy_id = Enemies.findEnemyIdFromInstance(result.Instance)
            if enemy_id then
                local applied, killed = Enemies.applyDamage(enemy_id, weapon.damage)
                if applied > 0 then
                    total_damage = total_damage + applied
                    last_hit_enemy_id = enemy_id
                    if killed then
                        kills = kills + 1
                    end
                end
            end
        end
    end

    playFireAnimation(character, player_state, weapon_name)

    player_state.shots_fired = (player_state.shots_fired or 0) + 1
    player:SetAttribute("ShotsFired", player_state.shots_fired)

    if total_damage > 0 then
        player_state.damage_dealt = (player_state.damage_dealt or 0) + total_damage
        player:SetAttribute("DamageDealt", player_state.damage_dealt)
        if kills > 0 then
            player_state.kills = (player_state.kills or 0) + kills
            player:SetAttribute("Kills", player_state.kills)
        end
        player:SetAttribute("LastHitEnemyId", last_hit_enemy_id)
    end

    Markers.setMany(Config.MARKERS.Combat, {
        LastShotTick = State.server_ticks,
        LastWeapon = weapon_name,
        LastHitEnemyId = last_hit_enemy_id,
        LastDamage = total_damage,
        LastKills = kills,
    })

    return true, "OK"
end

return RaycastCombat

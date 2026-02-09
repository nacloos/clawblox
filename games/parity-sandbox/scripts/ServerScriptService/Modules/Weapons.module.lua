local Weapons = {}

local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
local Config = require(ModulesFolder:WaitForChild("Config", 2))

function Weapons.isValid(weapon_name)
    return Config.WEAPONS[weapon_name] ~= nil
end

function Weapons.getConfig(weapon_name)
    return Config.WEAPONS[weapon_name]
end

function Weapons.setWeapon(player, weapon_name)
    if not Weapons.isValid(weapon_name) then
        return false
    end
    player:SetAttribute("CurrentWeapon", weapon_name)
    return true
end

function Weapons.recordShot(player_state, now)
    player_state.last_shot_at = now
    player_state.shots_fired = (player_state.shots_fired or 0) + 1

    local list = player_state.fire_timestamps
    list[#list + 1] = now

    local i = 1
    while i <= #list do
        if now - list[i] > 1.0 then
            table.remove(list, i)
        else
            i = i + 1
        end
    end
end

function Weapons.withinFireCap(player_state)
    return #player_state.fire_timestamps <= Config.COMBAT.GLOBAL_FIRE_CAP_PER_SEC
end

function Weapons.canFire(player_state, weapon_name, now)
    local weapon = Weapons.getConfig(weapon_name)
    if not weapon then
        return false, "UnknownWeapon"
    end
    if now - (player_state.last_shot_at or 0) < weapon.cooldown then
        return false, "Cooldown"
    end
    return true, "OK"
end

return Weapons

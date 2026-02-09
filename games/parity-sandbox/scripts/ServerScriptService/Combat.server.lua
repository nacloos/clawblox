local ModulesFolder = ServerScriptService:WaitForChild("Modules", 2)
if not ModulesFolder then
    warn("Modules folder missing")
    return
end

local RaycastCombat = require(ModulesFolder:WaitForChild("RaycastCombat", 2))
local Weapons = require(ModulesFolder:WaitForChild("Weapons", 2))

local AgentInputService = game:GetService("AgentInputService")

AgentInputService.InputReceived:Connect(function(player, input_type, data)
    if input_type == "Fire" then
        local ok, reason = RaycastCombat.fire(player, data)
        if not ok then
            warn("Fire rejected: " .. tostring(reason))
        end
        return
    end

    if input_type == "SetWeapon" then
        if data and type(data.weapon) == "string" then
            if not Weapons.setWeapon(player, data.weapon) then
                warn("Invalid weapon: " .. tostring(data.weapon))
            end
        end
    end
end)

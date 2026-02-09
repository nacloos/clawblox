local AgentInputService = game:GetService("AgentInputService")

local InputService = {}

local state
local combatService

local function parseMoveTo(data)
    if type(data) ~= "table" or type(data.position) ~= "table" then
        return nil
    end
    local pos = data.position
    if type(pos[1]) ~= "number" or type(pos[2]) ~= "number" or type(pos[3]) ~= "number" then
        return nil
    end
    return Vector3.new(pos[1], pos[2], pos[3])
end

local function handleMoveTo(player, data)
    local pdata = state.GetPlayer(player)
    if not pdata or not pdata.alive then
        return
    end

    local target = parseMoveTo(data)
    if not target then
        return
    end

    local character = player.Character
    if not character then
        return
    end

    local humanoid = character:FindFirstChild("Humanoid")
    if humanoid then
        humanoid:MoveTo(target)
    end
end

function InputService.Init(deps)
    state = deps.state
    combatService = deps.combatService

    AgentInputService.InputReceived:Connect(function(player, inputType, data)
        if inputType == "MoveTo" then
            handleMoveTo(player, data)
            return
        end

        if inputType == "Fire" then
            combatService.HandleFire(player, data)
            return
        end
    end)
end

return InputService

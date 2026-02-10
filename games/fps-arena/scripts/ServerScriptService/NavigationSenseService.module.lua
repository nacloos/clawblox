local Players = game:GetService("Players")

local NavigationSenseService = {}

local state
local DEBUG_NAV = false
local lastBlockedByUserId = {}
local lastDebugTickByUserId = {}

local PROBE_DISTANCE = 10.0
local BLOCK_THRESHOLD = 2.8
local EPS = 0.01
local ANGLES = {
    0.0,
    math.rad(30),
    -math.rad(30),
    math.rad(60),
    -math.rad(60),
    math.rad(90),
    -math.rad(90),
}
local WEIGHTS = { 1.3, 1.15, 1.15, 1.0, 1.0, 0.85, 0.85 }

local function getRoot(character)
    if not character then
        return nil
    end
    return character:FindFirstChild("HumanoidRootPart")
end

local function getHumanoid(character)
    if not character then
        return nil
    end
    return character:FindFirstChild("Humanoid")
end

local function rotateXZ(dir, angle)
    local c = math.cos(angle)
    local s = math.sin(angle)
    return Vector3.new(
        dir.X * c - dir.Z * s,
        0,
        dir.X * s + dir.Z * c
    )
end

local function horizontalUnitOrNil(v)
    local h = Vector3.new(v.X, 0, v.Z)
    if h.Magnitude <= EPS then
        return nil
    end
    return h.Unit
end

local function setNoData(player)
    player:SetAttribute("NavFrontClear", false)
    player:SetAttribute("NavLeftClear", false)
    player:SetAttribute("NavRightClear", false)
    player:SetAttribute("NavClearanceFront", 0)
    player:SetAttribute("NavClearanceLeft", 0)
    player:SetAttribute("NavClearanceRight", 0)
    player:SetAttribute("NavBestDirX", 0)
    player:SetAttribute("NavBestDirZ", 0)
    player:SetAttribute("NavFrontHitName", nil)
    player:SetAttribute("NavFrontNormalX", nil)
    player:SetAttribute("NavFrontNormalZ", nil)
end

local function sensePlayer(player, pdata)
    if not player or not pdata or not pdata.alive then
        setNoData(player)
        return
    end

    local character = player.Character
    local root = getRoot(character)
    local humanoid = getHumanoid(character)
    if not root or not humanoid or humanoid.Health <= 0 then
        setNoData(player)
        return
    end

    local forward = horizontalUnitOrNil(humanoid.MoveDirection)
    if not forward then
        forward = horizontalUnitOrNil(root.CFrame.LookVector)
    end
    if not forward then
        forward = Vector3.new(0, 0, -1)
    end

    local originMid = root.Position + Vector3.new(0, 1.5, 0)
    local originLow = root.Position + Vector3.new(0, 0.6, 0)
    local rayParams = RaycastParams.new()
    rayParams.FilterType = Enum.RaycastFilterType.Blacklist
    rayParams.FilterDescendantsInstances = { character }

    local clearances = {}
    local frontHitName = nil
    local frontNormal = nil
    local bestScore = -1
    local bestDir = forward
    for i, angle in ipairs(ANGLES) do
        local dir = rotateXZ(forward, angle)
        local hitMid = Workspace:Raycast(originMid, dir * PROBE_DISTANCE, rayParams)
        local hitLow = Workspace:Raycast(originLow, dir * PROBE_DISTANCE, rayParams)
        local clearanceMid = hitMid and hitMid.Distance or PROBE_DISTANCE
        local clearanceLow = hitLow and hitLow.Distance or PROBE_DISTANCE
        local clearance = math.min(clearanceMid, clearanceLow)
        clearances[i] = clearance
        if i == 1 then
            local chosen = nil
            if clearanceMid <= clearanceLow then
                chosen = hitMid
            else
                chosen = hitLow
            end
            frontHitName = chosen and chosen.Instance and chosen.Instance.Name or nil
            frontNormal = chosen and chosen.Normal or nil
        end
        local score = clearance * (WEIGHTS[i] or 1.0)
        if score > bestScore then
            bestScore = score
            bestDir = dir
        end
    end

    local frontClearance = clearances[1] or 0
    local leftClearance = math.max(clearances[2] or 0, clearances[4] or 0, clearances[6] or 0)
    local rightClearance = math.max(clearances[3] or 0, clearances[5] or 0, clearances[7] or 0)

    player:SetAttribute("NavFrontClear", frontClearance >= BLOCK_THRESHOLD)
    player:SetAttribute("NavLeftClear", leftClearance >= BLOCK_THRESHOLD)
    player:SetAttribute("NavRightClear", rightClearance >= BLOCK_THRESHOLD)
    player:SetAttribute("NavClearanceFront", frontClearance)
    player:SetAttribute("NavClearanceLeft", leftClearance)
    player:SetAttribute("NavClearanceRight", rightClearance)
    player:SetAttribute("NavBestDirX", bestDir.X)
    player:SetAttribute("NavBestDirZ", bestDir.Z)
    player:SetAttribute("NavFrontHitName", frontHitName)
    player:SetAttribute("NavFrontNormalX", frontNormal and frontNormal.X or nil)
    player:SetAttribute("NavFrontNormalZ", frontNormal and frontNormal.Z or nil)

    if DEBUG_NAV then
        local blocked = frontClearance < BLOCK_THRESHOLD
        local userId = player.UserId
        local prev = lastBlockedByUserId[userId]
        local tickNow = tick()
        local lastTick = lastDebugTickByUserId[userId] or 0
        local periodic = (tickNow - lastTick) >= 1.0 and frontClearance < (PROBE_DISTANCE - 0.01)
        if prev == nil or prev ~= blocked or periodic then
            print(string.format(
                "[NavSense] player=%s blocked=%s front=%.2f left=%.2f right=%.2f best=(%.2f,%.2f) front_hit=%s",
                player.Name,
                tostring(blocked),
                frontClearance,
                leftClearance,
                rightClearance,
                bestDir.X,
                bestDir.Z,
                tostring(frontHitName)
            ))
            lastBlockedByUserId[userId] = blocked
            lastDebugTickByUserId[userId] = tickNow
        end
    end
end

function NavigationSenseService.Init(deps)
    state = deps.state
end

function NavigationSenseService.Tick()
    for _, player in ipairs(Players:GetPlayers()) do
        local pdata = state and state.GetPlayer(player) or nil
        sensePlayer(player, pdata)
    end
end

return NavigationSenseService

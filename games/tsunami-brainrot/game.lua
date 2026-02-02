-- Escape Tsunami For Brainrots - Phase 1 (Minimal with DataStore)
-- Collection game with DataStore persistence for money and speed upgrades

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")
local DataStoreService = game:GetService("DataStoreService")
local AgentInputService = game:GetService("AgentInputService")

--------------------------------------------------------------------------------
-- CONFIGURATION
--------------------------------------------------------------------------------

local MAP_WIDTH = 80          -- X: -40 to +40
local SAFE_ZONE_END = 50      -- Z: 0 to 50
local COLLECTION_ZONE_END = 200  -- Z: 50 to 200 (simplified for Phase 1)
local COLLECTION_RANGE = 5

local BRAINROT_VALUE = 10     -- Fixed value for Phase 1
local MAX_BRAINROTS = 20      -- Max active brainrots
local SPAWN_INTERVAL = 2      -- Seconds between spawns

-- Speed upgrades
local SPEED_UPGRADES = {
    {level = 1, cost = 0, speed = 16},
    {level = 2, cost = 100, speed = 20},
    {level = 3, cost = 300, speed = 24},
    {level = 4, cost = 700, speed = 28},
    {level = 5, cost = 1500, speed = 32},
    {level = 6, cost = 3000, speed = 36},
    {level = 7, cost = 6000, speed = 40},
    {level = 8, cost = 12000, speed = 45},
    {level = 9, cost = 25000, speed = 50},
    {level = 10, cost = 50000, speed = 60},
}

--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local gameState = "active"  -- No waiting state for Phase 1
local playerData = {}       -- keyed by UserId: {money, speedLevel, carriedBrainrots}
local brainrots = {}        -- Active brainrot parts
local lastSpawnTime = 0

-- DataStore
local playerStore = DataStoreService:GetDataStore("PlayerData")

--------------------------------------------------------------------------------
-- HELPER FUNCTIONS
--------------------------------------------------------------------------------

local function getPlayerData(player)
    return playerData[player.UserId]
end

local function setPlayerData(player, data)
    playerData[player.UserId] = data
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

local function isInSafeZone(position)
    return position.Z <= SAFE_ZONE_END
end

local function updatePlayerAttributes(player)
    local data = getPlayerData(player)
    if not data then return end

    player:SetAttribute("Money", data.money)
    player:SetAttribute("SpeedLevel", data.speedLevel)
    player:SetAttribute("CarriedCount", #data.carriedBrainrots)
    player:SetAttribute("CarriedValue", #data.carriedBrainrots * BRAINROT_VALUE)

    -- Update walk speed
    local humanoid = getHumanoid(player)
    if humanoid then
        humanoid.WalkSpeed = SPEED_UPGRADES[data.speedLevel].speed
    end
end

--------------------------------------------------------------------------------
-- DATA PERSISTENCE
--------------------------------------------------------------------------------

local function loadPlayerData(player)
    local key = "player_" .. player.UserId
    local savedData = playerStore:GetAsync(key)

    local data
    if savedData then
        print("[DataStore] Loaded data for " .. player.Name .. ": money=" .. savedData.money .. ", speedLevel=" .. savedData.speedLevel)
        data = {
            money = savedData.money or 0,
            speedLevel = savedData.speedLevel or 1,
            carriedBrainrots = {},  -- Carried brainrots are session-only
        }
    else
        print("[DataStore] No saved data for " .. player.Name .. ", using defaults")
        data = {
            money = 0,
            speedLevel = 1,
            carriedBrainrots = {},
        }
    end

    setPlayerData(player, data)
    updatePlayerAttributes(player)
end

local function savePlayerData(player)
    local data = getPlayerData(player)
    if not data then
        warn("[DataStore] Cannot save: no data for " .. player.Name)
        return
    end

    local key = "player_" .. player.UserId
    local saveData = {
        money = data.money,
        speedLevel = data.speedLevel,
    }

    playerStore:SetAsync(key, saveData)
    print("[DataStore] Saved data for " .. player.Name .. ": money=" .. data.money .. ", speedLevel=" .. data.speedLevel)
end

--------------------------------------------------------------------------------
-- MAP CREATION
--------------------------------------------------------------------------------

local function createMap()
    -- Main floor
    local floor = Instance.new("Part")
    floor.Name = "Floor"
    floor.Size = Vector3.new(MAP_WIDTH, 2, COLLECTION_ZONE_END + 50)
    floor.Position = Vector3.new(0, -1, COLLECTION_ZONE_END / 2)
    floor.Anchored = true
    floor.Color = Color3.fromRGB(100, 150, 100)  -- Green grass
    floor.Parent = Workspace

    -- Safe zone (different color)
    local safeZone = Instance.new("Part")
    safeZone.Name = "SafeZone"
    safeZone.Size = Vector3.new(MAP_WIDTH, 0.1, SAFE_ZONE_END)
    safeZone.Position = Vector3.new(0, 0.05, SAFE_ZONE_END / 2)
    safeZone.Anchored = true
    safeZone.Color = Color3.fromRGB(100, 200, 100)  -- Brighter green
    safeZone.CanCollide = false
    safeZone:SetAttribute("IsSafeZone", true)
    safeZone.Parent = Workspace

    -- Safe zone marker/sign
    local safeMarker = Instance.new("Part")
    safeMarker.Name = "SafeZoneMarker"
    safeMarker.Size = Vector3.new(MAP_WIDTH, 10, 2)
    safeMarker.Position = Vector3.new(0, 5, SAFE_ZONE_END)
    safeMarker.Anchored = true
    safeMarker.Color = Color3.fromRGB(50, 200, 50)
    safeMarker.Transparency = 0.5
    safeMarker.CanCollide = false
    safeMarker:SetAttribute("IsSafeZoneBorder", true)
    safeMarker.Parent = Workspace

    -- Collection zone marker
    local collectionMarker = Instance.new("Part")
    collectionMarker.Name = "CollectionZone"
    collectionMarker.Size = Vector3.new(MAP_WIDTH, 0.1, COLLECTION_ZONE_END - SAFE_ZONE_END)
    collectionMarker.Position = Vector3.new(0, 0.05, (SAFE_ZONE_END + COLLECTION_ZONE_END) / 2)
    collectionMarker.Anchored = true
    collectionMarker.Color = Color3.fromRGB(200, 180, 100)  -- Tan/sandy
    collectionMarker.CanCollide = false
    collectionMarker:SetAttribute("IsCollectionZone", true)
    collectionMarker.Parent = Workspace

    -- Deposit area (in safe zone)
    local depositArea = Instance.new("Part")
    depositArea.Name = "DepositArea"
    depositArea.Size = Vector3.new(20, 0.2, 20)
    depositArea.Position = Vector3.new(0, 0.1, 25)
    depositArea.Anchored = true
    depositArea.Color = Color3.fromRGB(200, 200, 50)  -- Yellow
    depositArea.CanCollide = false
    depositArea:SetAttribute("IsDepositArea", true)
    depositArea.Parent = Workspace

    -- Upgrade shop (in safe zone)
    local shop = Instance.new("Part")
    shop.Name = "SpeedShop"
    shop.Size = Vector3.new(10, 5, 10)
    shop.Position = Vector3.new(-25, 2.5, 15)
    shop.Anchored = true
    shop.Color = Color3.fromRGB(100, 100, 200)  -- Blue
    shop:SetAttribute("IsShop", true)
    shop.Parent = Workspace

    -- Walls to prevent going out of bounds
    local walls = {
        {Vector3.new(MAP_WIDTH/2 + 1, 25, COLLECTION_ZONE_END/2), Vector3.new(2, 50, COLLECTION_ZONE_END + 50)},  -- Right
        {Vector3.new(-MAP_WIDTH/2 - 1, 25, COLLECTION_ZONE_END/2), Vector3.new(2, 50, COLLECTION_ZONE_END + 50)}, -- Left
        {Vector3.new(0, 25, -1), Vector3.new(MAP_WIDTH, 50, 2)},                                                    -- Back
        {Vector3.new(0, 25, COLLECTION_ZONE_END + 26), Vector3.new(MAP_WIDTH, 50, 2)},                             -- Front
    }

    for i, data in ipairs(walls) do
        local wall = Instance.new("Part")
        wall.Name = "Wall_" .. i
        wall.Position = data[1]
        wall.Size = data[2]
        wall.Anchored = true
        wall.Transparency = 1
        wall.CanCollide = true
        wall.Parent = Workspace
    end

    print("Map created: Safe zone (Z=0-" .. SAFE_ZONE_END .. "), Collection zone (Z=" .. SAFE_ZONE_END .. "-" .. COLLECTION_ZONE_END .. ")")
end

--------------------------------------------------------------------------------
-- BRAINROT SYSTEM
--------------------------------------------------------------------------------

local function spawnBrainrot()
    if #brainrots >= MAX_BRAINROTS then
        return
    end

    -- Random position in collection zone
    local x = math.random(-35, 35)
    local z = math.random(SAFE_ZONE_END + 10, COLLECTION_ZONE_END - 10)

    local brainrot = Instance.new("Part")
    brainrot.Name = "Brainrot"
    brainrot.Size = Vector3.new(2, 2, 2)
    brainrot.Position = Vector3.new(x, 1, z)
    brainrot.Anchored = true
    brainrot.CanCollide = false
    brainrot.Shape = Enum.PartType.Ball
    brainrot.Color = Color3.fromRGB(255, 100, 255)  -- Pink/magenta
    brainrot.Material = Enum.Material.Neon
    brainrot:SetAttribute("IsBrainrot", true)
    brainrot:SetAttribute("Value", BRAINROT_VALUE)
    brainrot.Parent = Workspace

    table.insert(brainrots, brainrot)
end

local function collectBrainrot(player)
    local pos = getCharacterPosition(player)
    if not pos then
        warn("[Collect] No position for " .. player.Name)
        return false
    end

    local data = getPlayerData(player)
    if not data then
        warn("[Collect] No data for " .. player.Name)
        return false
    end

    -- Find nearest brainrot within range
    local nearestIdx = nil
    local nearestDist = COLLECTION_RANGE

    for i, brainrot in ipairs(brainrots) do
        if brainrot and brainrot.Parent then
            local dist = (brainrot.Position - pos).Magnitude
            if dist < nearestDist then
                nearestDist = dist
                nearestIdx = i
            end
        end
    end

    if not nearestIdx then
        return false
    end

    -- Collect the brainrot
    local brainrot = brainrots[nearestIdx]
    local value = brainrot:GetAttribute("Value") or BRAINROT_VALUE

    table.insert(data.carriedBrainrots, value)
    brainrot:Destroy()
    table.remove(brainrots, nearestIdx)

    updatePlayerAttributes(player)
    print("[Collect] " .. player.Name .. " collected brainrot worth " .. value .. " (carrying " .. #data.carriedBrainrots .. ")")

    return true
end

local function depositBrainrots(player)
    local pos = getCharacterPosition(player)
    if not pos then
        warn("[Deposit] No position for " .. player.Name)
        return false
    end

    if not isInSafeZone(pos) then
        print("[Deposit] " .. player.Name .. " not in safe zone (Z=" .. pos.Z .. ")")
        return false
    end

    local data = getPlayerData(player)
    if not data then
        warn("[Deposit] No data for " .. player.Name)
        return false
    end

    if #data.carriedBrainrots == 0 then
        print("[Deposit] " .. player.Name .. " has no brainrots to deposit")
        return false
    end

    -- Calculate total value
    local totalValue = 0
    for _, value in ipairs(data.carriedBrainrots) do
        totalValue = totalValue + value
    end

    -- Add to money and clear carried
    data.money = data.money + totalValue
    data.carriedBrainrots = {}

    updatePlayerAttributes(player)
    savePlayerData(player)

    print("[Deposit] " .. player.Name .. " deposited " .. totalValue .. " (total money: " .. data.money .. ")")

    return true
end

--------------------------------------------------------------------------------
-- UPGRADE SYSTEM
--------------------------------------------------------------------------------

local function buySpeedUpgrade(player)
    local data = getPlayerData(player)
    if not data then
        warn("[BuySpeed] No data for " .. player.Name)
        return false
    end

    local currentLevel = data.speedLevel
    if currentLevel >= #SPEED_UPGRADES then
        print("[BuySpeed] " .. player.Name .. " already at max speed level")
        return false
    end

    local nextUpgrade = SPEED_UPGRADES[currentLevel + 1]
    if data.money < nextUpgrade.cost then
        print("[BuySpeed] " .. player.Name .. " cannot afford upgrade (need " .. nextUpgrade.cost .. ", have " .. data.money .. ")")
        return false
    end

    -- Purchase upgrade
    data.money = data.money - nextUpgrade.cost
    data.speedLevel = currentLevel + 1

    updatePlayerAttributes(player)
    savePlayerData(player)

    print("[BuySpeed] " .. player.Name .. " upgraded to speed level " .. data.speedLevel .. " (speed: " .. nextUpgrade.speed .. ")")

    return true
end

--------------------------------------------------------------------------------
-- PLAYER MANAGEMENT
--------------------------------------------------------------------------------

local function spawnPlayer(player)
    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            -- Spawn in safe zone
            hrp.Position = Vector3.new(0, 3, 25)
            hrp.Velocity = Vector3.new(0, 0, 0)
        end
    end

    updatePlayerAttributes(player)
end

local function initializePlayer(player)
    -- Load saved data from DataStore
    loadPlayerData(player)

    -- Spawn player in safe zone
    spawnPlayer(player)

    print("[Init] " .. player.Name .. " initialized (money: " .. getPlayerData(player).money .. ", speedLevel: " .. getPlayerData(player).speedLevel .. ")")
end

--------------------------------------------------------------------------------
-- GAME LOOP
--------------------------------------------------------------------------------

local function updateBrainrotSpawning(dt)
    lastSpawnTime = lastSpawnTime + dt
    if lastSpawnTime >= SPAWN_INTERVAL then
        lastSpawnTime = 0
        spawnBrainrot()
    end
end

local function cleanupBrainrots()
    -- Remove any destroyed brainrots from the list
    for i = #brainrots, 1, -1 do
        if not brainrots[i] or not brainrots[i].Parent then
            table.remove(brainrots, i)
        end
    end
end

--------------------------------------------------------------------------------
-- AGENT INPUT HANDLING
--------------------------------------------------------------------------------

if AgentInputService then
    AgentInputService.InputReceived:Connect(function(player, inputType, data)
        local pData = getPlayerData(player)
        if not pData then
            warn("[Input] Unknown player: " .. player.Name)
            return
        end

        if inputType == "MoveTo" then
            local humanoid = getHumanoid(player)
            if humanoid and data and data.position then
                local pos = data.position
                humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
            else
                warn("[MoveTo] Missing humanoid or position for " .. player.Name)
            end

        elseif inputType == "Collect" then
            collectBrainrot(player)

        elseif inputType == "Deposit" then
            depositBrainrots(player)

        elseif inputType == "BuySpeed" then
            buySpeedUpgrade(player)
        end
    end)
end

--------------------------------------------------------------------------------
-- INITIALIZATION
--------------------------------------------------------------------------------

createMap()

-- Initialize existing players
for _, player in ipairs(Players:GetPlayers()) do
    initializePlayer(player)
end

-- Handle new players
Players.PlayerAdded:Connect(function(player)
    initializePlayer(player)
end)

-- Handle players leaving
Players.PlayerRemoving:Connect(function(player)
    -- Save data before removing
    savePlayerData(player)
    playerData[player.UserId] = nil
end)

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    updateBrainrotSpawning(dt)
    cleanupBrainrots()
end)

-- Initial brainrot spawn
for i = 1, 10 do
    spawnBrainrot()
end

print("=== Escape Tsunami For Brainrots (Phase 1) ===")
print("Collect brainrots, deposit for money, buy speed upgrades!")
print("Data is persisted via DataStore.")

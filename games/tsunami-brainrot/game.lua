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
    -- After rotation: safe zone is at high X values (X >= 50)
    return position.X >= (COLLECTION_ZONE_END/2 - SAFE_ZONE_END)
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

    -- Update GUI
    local playerGui = player.PlayerGui
    if playerGui then
        local hud = playerGui:FindFirstChild("HUD")
        if hud then
            local moneyLabel = hud:FindFirstChild("MoneyLabel")
            if moneyLabel then
                moneyLabel.Text = "$" .. data.money
            end

            local carriedLabel = hud:FindFirstChild("CarriedLabel")
            if carriedLabel then
                local value = #data.carriedBrainrots * BRAINROT_VALUE
                if #data.carriedBrainrots > 0 then
                    carriedLabel.Text = "Carrying: " .. #data.carriedBrainrots .. " ($" .. value .. ")"
                else
                    carriedLabel.Text = "Carrying: 0"
                end
            end

            local speedLabel = hud:FindFirstChild("SpeedLabel")
            if speedLabel then
                local currentSpeed = SPEED_UPGRADES[data.speedLevel].speed
                speedLabel.Text = "Speed: Lv" .. data.speedLevel .. " (" .. currentSpeed .. ")"
            end

            -- Update upgrade button visibility and text
            local upgradeBtn = hud:FindFirstChild("UpgradeButton")
            if upgradeBtn then
                if data.speedLevel >= #SPEED_UPGRADES then
                    upgradeBtn.Visible = false
                else
                    local nextUpgrade = SPEED_UPGRADES[data.speedLevel + 1]
                    upgradeBtn.Text = "Upgrade Speed ($" .. nextUpgrade.cost .. ")"
                    upgradeBtn.Visible = true
                end
            end
        end
    end
end

--------------------------------------------------------------------------------
-- GUI CREATION
--------------------------------------------------------------------------------

local function createPlayerGUI(player)
    local playerGui = player.PlayerGui
    if not playerGui then
        warn("[GUI] No PlayerGui for " .. player.Name)
        return
    end

    -- Create main ScreenGui
    local hud = Instance.new("ScreenGui")
    hud.Name = "HUD"
    hud.Parent = playerGui

    -- Money display (top left)
    local moneyLabel = Instance.new("TextLabel")
    moneyLabel.Name = "MoneyLabel"
    moneyLabel.Position = UDim2.new(0, 10, 0, 10)
    moneyLabel.Size = UDim2.new(0, 150, 0, 40)
    moneyLabel.Text = "$0"
    moneyLabel.TextSize = 28
    moneyLabel.TextColor3 = Color3.fromRGB(50, 255, 50)
    moneyLabel.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
    moneyLabel.BackgroundTransparency = 0.3
    moneyLabel.TextXAlignment = "Left"
    moneyLabel.Parent = hud

    -- Carried brainrots (below money)
    local carriedLabel = Instance.new("TextLabel")
    carriedLabel.Name = "CarriedLabel"
    carriedLabel.Position = UDim2.new(0, 10, 0, 55)
    carriedLabel.Size = UDim2.new(0, 200, 0, 30)
    carriedLabel.Text = "Carrying: 0"
    carriedLabel.TextSize = 20
    carriedLabel.TextColor3 = Color3.fromRGB(255, 100, 255)
    carriedLabel.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
    carriedLabel.BackgroundTransparency = 0.3
    carriedLabel.TextXAlignment = "Left"
    carriedLabel.Parent = hud

    -- Speed level (below carried)
    local speedLabel = Instance.new("TextLabel")
    speedLabel.Name = "SpeedLabel"
    speedLabel.Position = UDim2.new(0, 10, 0, 90)
    speedLabel.Size = UDim2.new(0, 180, 0, 25)
    speedLabel.Text = "Speed: Lv1 (16)"
    speedLabel.TextSize = 18
    speedLabel.TextColor3 = Color3.fromRGB(100, 200, 255)
    speedLabel.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
    speedLabel.BackgroundTransparency = 0.3
    speedLabel.TextXAlignment = "Left"
    speedLabel.Parent = hud

    -- Upgrade button (bottom right)
    local upgradeBtn = Instance.new("TextButton")
    upgradeBtn.Name = "UpgradeButton"
    upgradeBtn.Position = UDim2.new(1, -220, 1, -60)
    upgradeBtn.Size = UDim2.new(0, 200, 0, 50)
    upgradeBtn.Text = "Upgrade Speed ($100)"
    upgradeBtn.TextSize = 18
    upgradeBtn.TextColor3 = Color3.fromRGB(255, 255, 255)
    upgradeBtn.BackgroundColor3 = Color3.fromRGB(50, 100, 200)
    upgradeBtn.BackgroundTransparency = 0.1
    upgradeBtn.Parent = hud

    -- Connect button click
    upgradeBtn.MouseButton1Click:Connect(function()
        buySpeedUpgrade(player)
    end)

    -- Deposit button (bottom center)
    local depositBtn = Instance.new("TextButton")
    depositBtn.Name = "DepositButton"
    depositBtn.Position = UDim2.new(0.5, -100, 1, -60)
    depositBtn.Size = UDim2.new(0, 200, 0, 50)
    depositBtn.Text = "Deposit Brainrots"
    depositBtn.TextSize = 18
    depositBtn.TextColor3 = Color3.fromRGB(255, 255, 255)
    depositBtn.BackgroundColor3 = Color3.fromRGB(200, 180, 50)
    depositBtn.BackgroundTransparency = 0.1
    depositBtn.Parent = hud

    -- Connect deposit button
    depositBtn.MouseButton1Click:Connect(function()
        depositBrainrots(player)
    end)

    -- Collect button (bottom left)
    local collectBtn = Instance.new("TextButton")
    collectBtn.Name = "CollectButton"
    collectBtn.Position = UDim2.new(0, 10, 1, -60)
    collectBtn.Size = UDim2.new(0, 200, 0, 50)
    collectBtn.Text = "Collect Brainrot"
    collectBtn.TextSize = 18
    collectBtn.TextColor3 = Color3.fromRGB(255, 255, 255)
    collectBtn.BackgroundColor3 = Color3.fromRGB(255, 100, 255)
    collectBtn.BackgroundTransparency = 0.1
    collectBtn.Parent = hud

    -- Connect collect button
    collectBtn.MouseButton1Click:Connect(function()
        collectBrainrot(player)
    end)

    print("[GUI] Created HUD for " .. player.Name)
end

--------------------------------------------------------------------------------
-- DATA PERSISTENCE
--------------------------------------------------------------------------------

local function loadPlayerData(player)
    -- Set defaults FIRST so player can receive inputs while DB loads
    local data = {
        money = 0,
        speedLevel = 1,
        carriedBrainrots = {},
    }
    setPlayerData(player, data)
    updatePlayerAttributes(player)

    -- Now load from DataStore (yields)
    local key = "player_" .. player.UserId
    local savedData = playerStore:GetAsync(key)

    if savedData then
        print("[DataStore] Loaded data for " .. player.Name .. ": money=" .. savedData.money .. ", speedLevel=" .. savedData.speedLevel)
        data.money = savedData.money or 0
        data.speedLevel = savedData.speedLevel or 1
        updatePlayerAttributes(player)
    else
        print("[DataStore] No saved data for " .. player.Name .. ", using defaults")
    end
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
    -- ROTATED MAP: X is now the long axis, Z is the short axis
    -- Safe zone on right (high X), collection zone on left (low X)
    -- X: -100 to +100, Z: -40 to +40

    local MAP_CENTER_X = 0
    local SAFE_ZONE_X_START = COLLECTION_ZONE_END/2 - SAFE_ZONE_END  -- 50

    -- Main floor
    local floor = Instance.new("Part")
    floor.Name = "Floor"
    floor.Size = Vector3.new(COLLECTION_ZONE_END + 4, 2, MAP_WIDTH)
    floor.Position = Vector3.new(MAP_CENTER_X, -1, 0)
    floor.Anchored = true
    floor.Color = Color3.fromRGB(100, 150, 100)  -- Green grass
    floor.Parent = Workspace

    -- Safe zone (different color) - right side of map
    local safeZone = Instance.new("Part")
    safeZone.Name = "SafeZone"
    safeZone.Size = Vector3.new(SAFE_ZONE_END, 0.1, MAP_WIDTH)
    safeZone.Position = Vector3.new(COLLECTION_ZONE_END/2 - SAFE_ZONE_END/2, 0.05, 0)  -- X=75
    safeZone.Anchored = true
    safeZone.Color = Color3.fromRGB(100, 200, 100)  -- Brighter green
    safeZone.CanCollide = false
    safeZone:SetAttribute("IsSafeZone", true)
    safeZone.Parent = Workspace

    -- Collection zone marker - left/center of map
    local collectionMarker = Instance.new("Part")
    collectionMarker.Name = "CollectionZone"
    collectionMarker.Size = Vector3.new(COLLECTION_ZONE_END - SAFE_ZONE_END, 0.1, MAP_WIDTH)
    collectionMarker.Position = Vector3.new(-SAFE_ZONE_END/2, 0.05, 0)  -- X=-25
    collectionMarker.Anchored = true
    collectionMarker.Color = Color3.fromRGB(200, 180, 100)  -- Tan/sandy
    collectionMarker.CanCollide = false
    collectionMarker:SetAttribute("IsCollectionZone", true)
    collectionMarker.Parent = Workspace

    -- Deposit area (in safe zone, right side)
    local depositArea = Instance.new("Part")
    depositArea.Name = "DepositArea"
    depositArea.Size = Vector3.new(20, 0.2, 20)
    depositArea.Position = Vector3.new(75, 0.1, 0)
    depositArea.Anchored = true
    depositArea.Color = Color3.fromRGB(200, 200, 50)  -- Yellow
    depositArea.CanCollide = false
    depositArea:SetAttribute("IsDepositArea", true)
    depositArea.Parent = Workspace

    -- Upgrade shop (in safe zone, right side)
    local shop = Instance.new("Part")
    shop.Name = "SpeedShop"
    shop.Size = Vector3.new(10, 5, 10)
    shop.Position = Vector3.new(85, 2.5, -25)
    shop.Anchored = true
    shop.Color = Color3.fromRGB(100, 100, 200)  -- Blue
    shop:SetAttribute("IsShop", true)
    shop.Parent = Workspace

    -- Walls to prevent going out of bounds (rotated)
    local walls = {
        -- Front/back walls (along Z axis edges)
        {Vector3.new(0, 25, MAP_WIDTH/2 + 1), Vector3.new(COLLECTION_ZONE_END + 4, 50, 2)},   -- Front (Z+)
        {Vector3.new(0, 25, -MAP_WIDTH/2 - 1), Vector3.new(COLLECTION_ZONE_END + 4, 50, 2)},  -- Back (Z-)
        -- Left/right walls (along X axis edges)
        {Vector3.new(COLLECTION_ZONE_END/2 + 1, 25, 0), Vector3.new(2, 50, MAP_WIDTH)},       -- Right (X+)
        {Vector3.new(-COLLECTION_ZONE_END/2 - 1, 25, 0), Vector3.new(2, 50, MAP_WIDTH)},      -- Left (X-)
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

    print("Map created (rotated): Safe zone (X>=50), Collection zone (X<50)")
end

--------------------------------------------------------------------------------
-- BRAINROT SYSTEM
--------------------------------------------------------------------------------

local function spawnBrainrot()
    if #brainrots >= MAX_BRAINROTS then
        return
    end

    -- Random position in collection zone (rotated: X is long axis, Z is short axis)
    -- Collection zone is X from -100 to 50
    local x = math.random(-COLLECTION_ZONE_END/2 + 10, COLLECTION_ZONE_END/2 - SAFE_ZONE_END - 10)  -- -90 to 40
    local z = math.random(-35, 35)

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
            -- Spawn in safe zone (rotated: safe zone is at high X values)
            hrp.Position = Vector3.new(75, 3, 0)
            hrp.Velocity = Vector3.new(0, 0, 0)
        end
    end

    updatePlayerAttributes(player)
end

local function initializePlayer(player)
    -- Load saved data from DataStore (yields but works in coroutine)
    loadPlayerData(player)

    -- Create GUI
    createPlayerGUI(player)

    -- Spawn when character is added (or now if already exists)
    player.CharacterAdded:Connect(function(character)
        -- Wait for HumanoidRootPart
        local hrp = character:WaitForChild("HumanoidRootPart", 5)
        if hrp then
            hrp.Position = Vector3.new(75, 3, 0)
            hrp.Velocity = Vector3.new(0, 0, 0)
            print("[Spawn] " .. player.Name .. " spawned at (75, 3, 0)")
        end
        updatePlayerAttributes(player)
    end)

    -- Spawn now if character already exists
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

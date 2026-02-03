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
local CARRY_CAPACITY = 1      -- Starting carry capacity

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
local playerData = {}       -- keyed by UserId: {money, speedLevel, carryCapacity, carriedBrainrots, placedBrainrots}
local brainrots = {}        -- Active brainrot parts
local lastSpawnTime = 0
local incomeAccumulator = {} -- Per-player income accumulator for passive income

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

local function attachBrainrotToPlayer(player, brainrot)
    local character = player.Character
    if not character then return false end

    local hrp = character:FindFirstChild("HumanoidRootPart")
    if not hrp then return false end

    brainrot.CanCollide = false

    -- Weld to HumanoidRootPart, positioned on top
    local weld = Instance.new("Weld")
    weld.Name = "BrainrotWeld"
    weld.Part0 = hrp
    weld.Part1 = brainrot
    weld.C0 = CFrame.new(0, 3.5, 0)  -- On top of player (HRP is 5 studs tall)
    weld.Parent = brainrot

    brainrot.Anchored = false

    return true
end

local function placeBrainrotOnBase(brainrot, slotIndex, incomeRate)
    -- Remove weld
    local weld = brainrot:FindFirstChild("BrainrotWeld")
    if weld then weld:Destroy() end

    -- Position on base floor (deposit area at X=75)
    local spacing = 3
    local col = (slotIndex - 1) % 5
    local row = math.floor((slotIndex - 1) / 5)
    local x = 65 + col * spacing
    local z = -6 + row * spacing

    brainrot.Position = Vector3.new(x, 1, z)
    brainrot.Anchored = true
    brainrot.CanCollide = false  -- Don't block player movement
    brainrot:SetAttribute("IsPlaced", true)

    -- Update income label (green)
    local billboard = brainrot:FindFirstChild("BrainrotLabel")
    if billboard then
        local incomeLabel = billboard:FindFirstChild("IncomeLabel")
        if incomeLabel then
            incomeLabel.Text = "$" .. incomeRate .. "/s"
        end
    end
end

local function getTotalPassiveIncome(data)
    local total = 0
    for _, placed in ipairs(data.placedBrainrots) do
        total = total + placed.incomeRate
    end
    return total
end

local function updatePlayerAttributes(player)
    local data = getPlayerData(player)
    if not data then return end

    local capacity = data.carryCapacity or CARRY_CAPACITY
    local passiveIncome = getTotalPassiveIncome(data)

    -- Calculate carried value (sum of all carried brainrot values)
    local carriedValue = 0
    for _, carried in ipairs(data.carriedBrainrots) do
        carriedValue = carriedValue + (carried.value or 0)
    end

    player:SetAttribute("Money", data.money)
    player:SetAttribute("SpeedLevel", data.speedLevel)
    player:SetAttribute("CarriedCount", #data.carriedBrainrots)
    player:SetAttribute("CarriedValue", carriedValue)
    player:SetAttribute("CarryCapacity", capacity)
    player:SetAttribute("PassiveIncome", passiveIncome)

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
                -- Format money like real game (e.g., $61.97B)
                local moneyStr
                if data.money >= 1e9 then
                    moneyStr = string.format("$%.2fB", data.money / 1e9)
                elseif data.money >= 1e6 then
                    moneyStr = string.format("$%.2fM", data.money / 1e6)
                elseif data.money >= 1e3 then
                    moneyStr = string.format("$%.2fK", data.money / 1e3)
                else
                    moneyStr = string.format("$%.0f", data.money)
                end
                moneyLabel.Text = moneyStr
            end

            local speedLabel = hud:FindFirstChild("SpeedLabel")
            if speedLabel then
                local currentSpeed = SPEED_UPGRADES[data.speedLevel].speed
                speedLabel.Text = tostring(currentSpeed)
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

    -- Money display (bottom left, like real game)
    local moneyLabel = Instance.new("TextLabel")
    moneyLabel.Name = "MoneyLabel"
    moneyLabel.Position = UDim2.new(0, 10, 1, -55)
    moneyLabel.Size = UDim2.new(0, 150, 0, 40)
    moneyLabel.Text = "$0"
    moneyLabel.TextSize = 28
    moneyLabel.TextColor3 = Color3.fromRGB(50, 255, 50)
    moneyLabel.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
    moneyLabel.BackgroundTransparency = 0.3
    moneyLabel.TextXAlignment = "Left"
    moneyLabel.Parent = hud

    -- Speed display (bottom left, compact like real game)
    local speedLabel = Instance.new("TextLabel")
    speedLabel.Name = "SpeedLabel"
    speedLabel.Position = UDim2.new(0, 10, 1, -90)
    speedLabel.Size = UDim2.new(0, 80, 0, 30)
    speedLabel.Text = "16"
    speedLabel.TextSize = 24
    speedLabel.TextColor3 = Color3.fromRGB(100, 200, 255)
    speedLabel.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
    speedLabel.BackgroundTransparency = 0.3
    speedLabel.TextXAlignment = "Center"
    speedLabel.Parent = hud

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
        carryCapacity = CARRY_CAPACITY,
        carriedBrainrots = {},  -- {part, value} - attached to player
        placedBrainrots = {},   -- {part, value, incomeRate} - on base floor
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

    -- Add floating label (BillboardGui) so it is always attached
    local billboard = Instance.new("BillboardGui")
    billboard.Name = "BrainrotLabel"
    billboard.Size = UDim2.new(0, 100, 0, 50)
    billboard.StudsOffset = Vector3.new(0, 3, 0)
    billboard.AlwaysOnTop = true
    billboard.Parent = brainrot

    -- Name label
    local nameLabel = Instance.new("TextLabel")
    nameLabel.Name = "NameLabel"
    nameLabel.Size = UDim2.new(1, 0, 0.4, 0)
    nameLabel.Position = UDim2.new(0, 0, 0, 0)
    nameLabel.Text = "Brainrot"
    nameLabel.TextColor3 = Color3.fromRGB(255, 255, 255)
    nameLabel.TextScaled = true
    nameLabel.BackgroundTransparency = 1
    nameLabel.Parent = billboard

    -- Income label (green)
    local incomeLabel = Instance.new("TextLabel")
    incomeLabel.Name = "IncomeLabel"
    incomeLabel.Size = UDim2.new(1, 0, 0.4, 0)
    incomeLabel.Position = UDim2.new(0, 0, 0.5, 0)
    incomeLabel.Text = "$0/s"
    incomeLabel.TextColor3 = Color3.fromRGB(50, 255, 50)
    incomeLabel.TextScaled = true
    incomeLabel.BackgroundTransparency = 1
    incomeLabel.Parent = billboard

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

    -- Check capacity
    local capacity = data.carryCapacity or CARRY_CAPACITY
    if #data.carriedBrainrots >= capacity then
        print("[Collect] " .. player.Name .. " at full capacity (" .. #data.carriedBrainrots .. "/" .. capacity .. ")")
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

    -- Collect the brainrot - attach to player instead of destroying
    local brainrot = brainrots[nearestIdx]
    local value = brainrot:GetAttribute("Value") or BRAINROT_VALUE

    if attachBrainrotToPlayer(player, brainrot) then
        table.insert(data.carriedBrainrots, {part = brainrot, value = value})
        table.remove(brainrots, nearestIdx)
        updatePlayerAttributes(player)
        print("[Collect] " .. player.Name .. " collected brainrot worth " .. value .. " (carrying " .. #data.carriedBrainrots .. "/" .. capacity .. ")")
        return true
    else
        warn("[Collect] Failed to attach brainrot to " .. player.Name)
        return false
    end
end

local function depositBrainrots(player)
    local pos = getCharacterPosition(player)
    if not pos then
        warn("[Deposit] No position for " .. player.Name)
        return false
    end

    if not isInSafeZone(pos) then
        print("[Deposit] " .. player.Name .. " not in safe zone (X=" .. pos.X .. ")")
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

    -- Place brainrots on base floor instead of destroying
    for _, carried in ipairs(data.carriedBrainrots) do
        local incomeRate = carried.value / 10  -- e.g., value 10 = 1$/sec
        local slotIndex = #data.placedBrainrots + 1
        placeBrainrotOnBase(carried.part, slotIndex, incomeRate)

        table.insert(data.placedBrainrots, {
            part = carried.part,
            value = carried.value,
            incomeRate = incomeRate
        })
    end

    local depositedCount = #data.carriedBrainrots
    data.carriedBrainrots = {}

    updatePlayerAttributes(player)
    savePlayerData(player)

    local totalIncome = getTotalPassiveIncome(data)
    print("[Deposit] " .. player.Name .. " placed " .. depositedCount .. " brainrots (total income: $" .. totalIncome .. "/sec)")

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

        -- Clear carried brainrots on respawn (they're lost on death)
        -- Placed brainrots stay safe on base
        local data = getPlayerData(player)
        if data then
            for _, carried in ipairs(data.carriedBrainrots) do
                if carried.part and carried.part.Parent then
                    carried.part:Destroy()
                end
            end
            if #data.carriedBrainrots > 0 then
                print("[Death] " .. player.Name .. " lost " .. #data.carriedBrainrots .. " carried brainrots")
            end
            data.carriedBrainrots = {}
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

    -- Clean up placed brainrots
    local data = getPlayerData(player)
    if data then
        for _, placed in ipairs(data.placedBrainrots) do
            if placed.part and placed.part.Parent then
                placed.part:Destroy()
            end
        end
        for _, carried in ipairs(data.carriedBrainrots) do
            if carried.part and carried.part.Parent then
                carried.part:Destroy()
            end
        end
    end

    playerData[player.UserId] = nil
    incomeAccumulator[player.UserId] = nil
end)

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    updateBrainrotSpawning(dt)
    cleanupBrainrots()

    -- Passive income from placed brainrots
    for _, player in ipairs(Players:GetPlayers()) do
        local data = getPlayerData(player)
        if data and #data.placedBrainrots > 0 then
            incomeAccumulator[player.UserId] = (incomeAccumulator[player.UserId] or 0) + dt

            if incomeAccumulator[player.UserId] >= 1.0 then
                incomeAccumulator[player.UserId] = 0

                local totalIncome = getTotalPassiveIncome(data)
                data.money = data.money + totalIncome
                updatePlayerAttributes(player)
            end
        end
    end
end)

-- Initial brainrot spawn
for i = 1, 10 do
    spawnBrainrot()
end

print("=== Escape Tsunami For Brainrots (Phase 1) ===")
print("Collect brainrots, deposit for money, buy speed upgrades!")
print("Data is persisted via DataStore.")

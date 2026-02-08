-- Escape Tsunami For Brainrots - Phase 1 (Minimal with DataStore)
-- Collection game with DataStore persistence for money and speed upgrades

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")
local DataStoreService = game:GetService("DataStoreService")
local AgentInputService = game:GetService("AgentInputService")
local HttpService = game:GetService("HttpService")

--------------------------------------------------------------------------------
-- CONFIGURATION
--------------------------------------------------------------------------------

local MAP_LENGTH = 1000           -- X: -500 to +500
local BASE_ZONE_X_START = 450     -- Safe base zone starts at X=450
local BASE_COUNT = 8              -- Max 8 players/bases per place
local BASE_SIZE_X = 30            -- Base platform size (X)
local BASE_SIZE_Z = 20            -- Base platform size (Z)
local BASE_GAP = 4                -- Z gap between base platforms
local BASE_ZONE_PADDING_X = 10    -- Padding inside the safe zone along X
local BASE_ROW_MARGIN_Z = 20      -- Extra Z margin outside base row
local BASE_PLATFORM_HEIGHT = 0.2  -- Base platform thickness (keep <= autostep)
local BASE_ZONE_SIZE = BASE_SIZE_X + BASE_ZONE_PADDING_X * 2
local BASE_ROW_LENGTH_Z = BASE_COUNT * BASE_SIZE_Z + (BASE_COUNT - 1) * BASE_GAP
local MAP_WIDTH = math.max(80, BASE_ROW_LENGTH_Z + BASE_ROW_MARGIN_Z * 2)
local STORED_BRAINROT_POSITION = Vector3.new(0, -200, 0)
local COLLECTION_RANGE = 5

local BRAINROT_VALUE = 10     -- Default value for backwards compatibility
local MAX_BRAINROTS = 30      -- Max active brainrots (increased for larger map)
local SPAWN_INTERVAL = 1.5    -- Seconds between spawns (faster for larger map)
local CARRY_CAPACITY = 1      -- Starting carry capacity
local SAVE_INTERVAL = 15      -- Seconds between autosaves (if dirty)

-- Speed upgrades (15 levels, final levels very expensive)
local SPEED_UPGRADES = {
    {level = 1,  cost = 0,         speed = 16},
    {level = 2,  cost = 100,       speed = 22},
    {level = 3,  cost = 300,       speed = 30},
    {level = 4,  cost = 700,       speed = 40},
    {level = 5,  cost = 1500,      speed = 52},
    {level = 6,  cost = 3500,      speed = 66},
    {level = 7,  cost = 8000,      speed = 82},
    {level = 8,  cost = 18000,     speed = 100},
    {level = 9,  cost = 40000,     speed = 120},
    {level = 10, cost = 90000,     speed = 140},
    {level = 11, cost = 200000,    speed = 158},
    {level = 12, cost = 450000,    speed = 175},
    {level = 13, cost = 1000000,   speed = 188},
    {level = 14, cost = 2500000,   speed = 196},
    {level = 15, cost = 5000000,   speed = 200},
}

local ZONES = {
    {name = "Common",    xMin = 250, xMax = 350,  value = 10,   color = Color3.fromRGB(255, 100, 255), weight = 40},
    {name = "Uncommon",  xMin = 150, xMax = 250,  value = 30,   color = Color3.fromRGB(100, 150, 255), weight = 25},
    {name = "Rare",      xMin = 0,   xMax = 150,  value = 80,   color = Color3.fromRGB(180, 100, 255), weight = 15},
    {name = "Epic",      xMin = -150, xMax = 0,   value = 200,  color = Color3.fromRGB(255, 150, 50),  weight = 10},
    {name = "Legendary", xMin = -300, xMax = -150, value = 500, color = Color3.fromRGB(255, 255, 50),  weight = 7},
    {name = "Secret",    xMin = -500, xMax = -300, value = 1500, color = Color3.fromRGB(255, 255, 255), weight = 3},
}

-- Base capacity
local BASE_MAX_BRAINROTS = 10
local BASE_GRID_COLS = 5
local BASE_GRID_ROWS = 2

-- Wave configuration
local WAVE_SEGMENT_COUNT = 16  -- Number of wave segments
local WAVE_MODEL_SCALE = 1.12  -- Scale factor for wave model
local WAVE_MODEL_WIDTH = 12 * WAVE_MODEL_SCALE  -- 14.4 studs per segment (scaled)
local WAVE_CONFIG = {
    spawnInterval = 30,    -- seconds between waves
    speed = 35,            -- studs per second
    startX = -500,         -- spawn position (far left)
    endX = 350,            -- stop at end of Common zone (before safe area)
    modelUrl = "/static/models/wave.glb",
    modelWidth = WAVE_MODEL_WIDTH,  -- 17 × 12 = 204 studs total (fits in 228 width)
    modelHeight = 15,      -- height of wave model
    thickness = 10,        -- collision thickness in X
}

-- Characters with GLB models (from characters.json)
local CHARACTERS = {
    {name = "Turing Turing Turing Sahur", rarity = "secret", yield = 1000, model = "turing.glb"},
    {name = "Samuel de Prompto", rarity = "legendary", yield = 100, model = "altman.glb"},
    {name = "Elonio Muskarelli", rarity = "legendary", yield = 150, model = "musk.glb"},
    {name = "Zucc, Il Conte di Meta", rarity = "legendary", yield = 120, model = "zuck.glb"},
    {name = "Jensen al Silicio", rarity = "epic", yield = 85, model = "huang.glb"},
    {name = "Clawfather", rarity = "rare", yield = 60, model = "steinberger.glb"},
    {name = "Engineer", rarity = "common", yield = 5, model = "1x.glb"},
    {name = "10x Engineer", rarity = "uncommon", yield = 20, model = "10x.glb"},
}

-- Map zone names to character rarities
local ZONE_RARITY_MAP = {
    ["Secret"] = "secret",
    ["Legendary"] = "legendary",
    ["Epic"] = "epic",
    ["Rare"] = "rare",
    ["Uncommon"] = "uncommon",
    ["Common"] = "common",
}

-- Helper to look up zone config by name
local function getZoneByName(zoneName)
    for _, zone in ipairs(ZONES) do
        if zone.name == zoneName then
            return zone
        end
    end
    return nil
end

-- Helper to get random character for a zone
local function getCharacterForZone(zoneName)
    local rarity = ZONE_RARITY_MAP[zoneName]
    if not rarity then return nil end

    -- Filter characters by rarity
    local matching = {}
    for _, char in ipairs(CHARACTERS) do
        if char.rarity == rarity then
            table.insert(matching, char)
        end
    end

    if #matching == 0 then return nil end

    -- Random selection
    return matching[math.random(1, #matching)]
end

-- Helper to recreate a brainrot Part from saved data
local function createBrainrotFromData(brainrotData, zone)
    local hasModel = brainrotData.modelUrl ~= nil
    local partSize = hasModel and Vector3.new(2, 5, 2) or Vector3.new(2, 2, 2)

    local brainrot = Instance.new("Part")
    brainrot.Name = "Brainrot"
    brainrot.Size = partSize
    brainrot.Position = Vector3.new(0, -200, 0)  -- Temporary position, will be set by placeBrainrotOnBase
    brainrot.Anchored = true
    brainrot.CanCollide = false
    brainrot.Shape = Enum.PartType.Ball
    brainrot.Color = zone.color
    brainrot.Material = Enum.Material.Neon
    brainrot:SetAttribute("IsBrainrot", true)
    brainrot:SetAttribute("Value", brainrotData.value)
    brainrot:SetAttribute("Zone", brainrotData.zone)

    if brainrotData.modelUrl then
        brainrot:SetAttribute("ModelUrl", brainrotData.modelUrl)
    end

    -- Add floating label (BillboardGui)
    local billboard = Instance.new("BillboardGui")
    billboard.Name = "BrainrotLabel"
    billboard.Size = UDim2.new(0, 120, 0, 60)
    billboard.StudsOffset = Vector3.new(0, 3, 0)
    billboard.AlwaysOnTop = true
    billboard.Parent = brainrot

    -- Name label (white)
    local nameLabel = Instance.new("TextLabel")
    nameLabel.Name = "NameLabel"
    nameLabel.Size = UDim2.new(1, 0, 0.33, 0)
    nameLabel.Position = UDim2.new(0, 0, 0, 0)
    nameLabel.Text = brainrotData.displayName or zone.name
    nameLabel.TextColor3 = Color3.fromRGB(255, 255, 255)
    nameLabel.TextScaled = true
    nameLabel.BackgroundTransparency = 1
    nameLabel.Parent = billboard

    -- Rarity label (zone color)
    local rarityLabel = Instance.new("TextLabel")
    rarityLabel.Name = "RarityLabel"
    rarityLabel.Size = UDim2.new(1, 0, 0.33, 0)
    rarityLabel.Position = UDim2.new(0, 0, 0.33, 0)
    rarityLabel.Text = brainrotData.zone
    rarityLabel.TextColor3 = zone.color
    rarityLabel.TextScaled = true
    rarityLabel.BackgroundTransparency = 1
    rarityLabel.Parent = billboard

    -- Income label (green)
    local incomeLabel = Instance.new("TextLabel")
    incomeLabel.Name = "IncomeLabel"
    incomeLabel.Size = UDim2.new(1, 0, 0.33, 0)
    incomeLabel.Position = UDim2.new(0, 0, 0.66, 0)
    incomeLabel.Text = "$" .. brainrotData.incomeRate .. "/s"
    incomeLabel.TextColor3 = Color3.fromRGB(50, 255, 50)
    incomeLabel.TextScaled = true
    incomeLabel.BackgroundTransparency = 1
    incomeLabel.Parent = billboard

    brainrot.Parent = Workspace

    return brainrot
end


--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local gameState = "active"  -- No waiting state for Phase 1
local playerData = {}       -- keyed by UserId: {money, speedLevel, carryCapacity, carriedBrainrots, placedBrainrots}
local brainrots = {}        -- Active brainrot parts
local lastSpawnTime = 0
local incomeAccumulator = {} -- Per-player income accumulator for passive income
local saveAccumulator = {}   -- Per-player autosave timer
local dirtyPlayers = {}      -- Per-player dirty flag for autosave
local baseSlots = {}          -- base index -> userId
local userBaseIndex = {}      -- userId -> base index
local savedPlacedBrainrots = {} -- userId -> placedBrainrots (kept if player leaves)

-- DataStore
local playerStore = DataStoreService:GetDataStore("PlayerData")
local leaderboardStore = DataStoreService:GetOrderedDataStore("Leaderboard")

-- Leaderboard state
local leaderboardCache = {}  -- Global cache: {entries = {{key, score, name}, ...}}
local leaderboardUpdateTimer = 0
local leaderboardFetchTimer = 0
local LEADERBOARD_UPDATE_INTERVAL = 10  -- Seconds between updating player's score
local LEADERBOARD_FETCH_INTERVAL = 5    -- Seconds between fetching leaderboard

-- Wave state
local waves = {}           -- list of active wave objects {parts = {}, x = number}
local waveTimer = 0        -- time since last wave spawn

--------------------------------------------------------------------------------
-- HELPER FUNCTIONS
--------------------------------------------------------------------------------

local function getPlayerData(player)
    return playerData[player.UserId]
end

local function setPlayerData(player, data)
    playerData[player.UserId] = data
end

local function markDirty(player)
    dirtyPlayers[player.UserId] = true
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
    -- Base zone is at high X values (X >= BASE_ZONE_X_START)
    return position.X >= BASE_ZONE_X_START
end

local function getBaseCenterX()
    return BASE_ZONE_X_START + BASE_ZONE_PADDING_X + BASE_SIZE_X / 2
end

local function getBaseCenterZForIndex(index)
    local rowStartZ = -BASE_ROW_LENGTH_Z / 2 + BASE_SIZE_Z / 2
    return rowStartZ + (index - 1) * (BASE_SIZE_Z + BASE_GAP)
end

local function getPlayerBaseIndex(player)
    local data = getPlayerData(player)
    if not data then return nil end
    return data.playerIndex
end

local function getPlayerBaseCenter(player)
    local baseIndex = getPlayerBaseIndex(player)
    if not baseIndex then
        return Vector3.new(getBaseCenterX(), 0, 0)
    end
    return Vector3.new(getBaseCenterX(), 0, getBaseCenterZForIndex(baseIndex))
end

local function getPlayerSpawnPosition(player)
    local baseCenter = getPlayerBaseCenter(player)
    return Vector3.new(baseCenter.X, 3, baseCenter.Z)
end

local function isAtPlayerBase(player, position)
    local baseCenter = getPlayerBaseCenter(player)
    return position.X >= (baseCenter.X - BASE_SIZE_X / 2) and position.X <= (baseCenter.X + BASE_SIZE_X / 2) and
           position.Z >= (baseCenter.Z - BASE_SIZE_Z / 2) and position.Z <= (baseCenter.Z + BASE_SIZE_Z / 2)
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
    weld.C0 = CFrame.new(0, 5.5, 0)  -- On top of player head (raised for taller character models)
    weld.Parent = brainrot

    brainrot.Anchored = false

    return true
end

local function placeBrainrotOnBase(player, brainrot, slotIndex, incomeRate)
    -- Remove weld
    local weld = brainrot:FindFirstChild("BrainrotWeld")
    if weld then weld:Destroy() end

    -- Fixed 5×2 grid layout on front and back edges of base
    -- Slots 1-5: front edge (low Z), Slots 6-10: back edge (high Z)
    local baseCenter = getPlayerBaseCenter(player)
    local col = (slotIndex - 1) % BASE_GRID_COLS  -- 0-4
    local row = math.floor((slotIndex - 1) / BASE_GRID_COLS)  -- 0 or 1

    -- X position: 5 columns evenly distributed along X-axis
    local colSpacing = (BASE_SIZE_X - 4) / (BASE_GRID_COLS - 1)  -- spacing between columns
    local x = baseCenter.X - (BASE_SIZE_X - 4) / 2 + col * colSpacing

    -- Z position: front edge (row 0) or back edge (row 1)
    local edgeOffset = 3  -- distance from edge of base
    local z
    if row == 0 then
        z = baseCenter.Z - BASE_SIZE_Z / 2 + edgeOffset  -- front edge
    else
        z = baseCenter.Z + BASE_SIZE_Z / 2 - edgeOffset  -- back edge
    end

    -- Character models are taller (5 studs), spheres are 2 studs
    local hasModel = brainrot:GetAttribute("ModelUrl") ~= nil
    local yPos = hasModel and 2.5 or 1
    brainrot.Position = Vector3.new(x, yPos, z)
    brainrot.Anchored = true
    brainrot.CanCollide = false  -- Don't block player movement
    brainrot:SetAttribute("IsPlaced", true)
    brainrot:SetAttribute("OwnerUserId", player.UserId)

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

    -- Placed brainrots details
    local placedInfo = {}
    for _, placed in ipairs(data.placedBrainrots) do
        table.insert(placedInfo, {
            index = placed.slot,  -- Use slot number, not array index
            value = placed.value,
            incomeRate = placed.incomeRate,
            zone = placed.part:GetAttribute("Zone"),
            displayName = placed.displayName
        })
    end
    player:SetAttribute("PlacedBrainrots", HttpService:JSONEncode(placedInfo))

    -- Carried brainrots details
    local carriedInfo = {}
    for i, carried in ipairs(data.carriedBrainrots) do
        table.insert(carriedInfo, {
            index = i, value = carried.value, displayName = carried.displayName
        })
    end
    player:SetAttribute("CarriedBrainrots", HttpService:JSONEncode(carriedInfo))

    -- Base capacity
    player:SetAttribute("BaseMaxBrainrots", BASE_MAX_BRAINROTS)

    -- Note: WaveTimeRemaining is now on GameState (Folder in Workspace) for real-time updates

    -- Next speed upgrade cost
    local nextCost = 0
    if data.speedLevel < #SPEED_UPGRADES then
        nextCost = SPEED_UPGRADES[data.speedLevel + 1].cost
    end
    player:SetAttribute("NextSpeedCost", nextCost)

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

    -- Leaderboard frame (top-left)
    local leaderboardFrame = Instance.new("Frame")
    leaderboardFrame.Name = "LeaderboardFrame"
    leaderboardFrame.Position = UDim2.new(0, 10, 0, 10)
    leaderboardFrame.Size = UDim2.new(0, 220, 0, 180)
    leaderboardFrame.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
    leaderboardFrame.BackgroundTransparency = 0.3
    leaderboardFrame.Parent = hud

    -- Leaderboard title
    local leaderboardTitle = Instance.new("TextLabel")
    leaderboardTitle.Name = "Title"
    leaderboardTitle.Position = UDim2.new(0, 0, 0, 0)
    leaderboardTitle.Size = UDim2.new(1, 0, 0, 28)
    leaderboardTitle.Text = "Top Income"
    leaderboardTitle.TextSize = 18
    leaderboardTitle.TextColor3 = Color3.fromRGB(255, 215, 0)
    leaderboardTitle.BackgroundTransparency = 1
    leaderboardTitle.Parent = leaderboardFrame

    -- Create 5 entry labels
    for i = 1, 5 do
        local entryLabel = Instance.new("TextLabel")
        entryLabel.Name = "Entry" .. i
        entryLabel.Position = UDim2.new(0, 8, 0, 28 + (i - 1) * 28)
        entryLabel.Size = UDim2.new(1, -16, 0, 26)
        entryLabel.Text = ""
        entryLabel.TextSize = 16
        entryLabel.TextColor3 = Color3.fromRGB(200, 200, 200)
        entryLabel.BackgroundTransparency = 1
        entryLabel.TextXAlignment = "Left"
        entryLabel.Parent = leaderboardFrame
    end

    print("[GUI] Created HUD for " .. player.Name)
end

--------------------------------------------------------------------------------
-- LEADERBOARD FUNCTIONS
--------------------------------------------------------------------------------

local function updatePlayerLeaderboardEntry(player)
    local data = getPlayerData(player)
    if not data then return end

    local passiveIncome = getTotalPassiveIncome(data)
    if passiveIncome <= 0 then return end  -- Don't add players with 0 income

    local key = "player_" .. player.UserId
    local entry = {
        score = passiveIncome,
        name = player.Name,
    }

    local success, err = pcall(function()
        leaderboardStore:SetAsync(key, entry)
    end)

    if success then
        print("[Leaderboard] Updated " .. player.Name .. " with income $" .. passiveIncome .. "/s")
    else
        warn("[Leaderboard] Failed to update " .. player.Name .. ": " .. tostring(err))
    end
end

local function fetchLeaderboard()
    local success, result = pcall(function()
        return leaderboardStore:GetSortedAsync(false, 5)  -- false = descending, top 5
    end)

    if success and result then
        leaderboardCache = result
        print("[Leaderboard] Fetched " .. #result .. " entries")
    else
        warn("[Leaderboard] Failed to fetch: " .. tostring(result))
    end
end

local function updateLeaderboardGUI(player)
    local playerGui = player.PlayerGui
    if not playerGui then return end

    local hud = playerGui:FindFirstChild("HUD")
    if not hud then return end

    local frame = hud:FindFirstChild("LeaderboardFrame")
    if not frame then return end

    for i = 1, 5 do
        local entryLabel = frame:FindFirstChild("Entry" .. i)
        if entryLabel then
            local entry = leaderboardCache[i]
            if entry and entry.value then
                local name = entry.value.name or "Unknown"
                local score = entry.value.score or 0
                -- Truncate name if too long
                if #name > 10 then
                    name = string.sub(name, 1, 9) .. "."
                end
                -- Format score
                local scoreStr
                if score >= 1000 then
                    scoreStr = string.format("%.1fK", score / 1000)
                else
                    scoreStr = string.format("%.0f", score)
                end
                entryLabel.Text = i .. ". " .. name .. " $" .. scoreStr .. "/s"
            else
                entryLabel.Text = ""
            end
        end
    end
end

local function updateAllLeaderboardGUIs()
    for _, player in ipairs(Players:GetPlayers()) do
        updateLeaderboardGUI(player)
    end
end

--------------------------------------------------------------------------------
-- DATA PERSISTENCE
--------------------------------------------------------------------------------

local function loadPlayerData(player)
    -- Assign player index for per-player base
    if userBaseIndex[player.UserId] == nil then
        for i = 1, BASE_COUNT do
            if baseSlots[i] == nil then
                baseSlots[i] = player.UserId
                userBaseIndex[player.UserId] = i
                break
            end
        end
    end
    -- Set defaults FIRST so player can receive inputs while DB loads
    local data = {
        money = 0,
        speedLevel = 1,
        carryCapacity = CARRY_CAPACITY,
        carriedBrainrots = {},  -- {part, value} - attached to player
        placedBrainrots = savedPlacedBrainrots[player.UserId] or {},   -- {part, value, incomeRate} - on base floor
        playerIndex = userBaseIndex[player.UserId],
    }
    setPlayerData(player, data)

    local baseCenter = getPlayerBaseCenter(player)
    player:SetAttribute("BaseIndex", data.playerIndex)
    player:SetAttribute("BaseCenterX", baseCenter.X)
    player:SetAttribute("BaseCenterZ", baseCenter.Z)
    player:SetAttribute("BaseSizeX", BASE_SIZE_X)
    player:SetAttribute("BaseSizeZ", BASE_SIZE_Z)

    -- Expose zone info once at player setup
    local zoneInfo = {}
    for _, zone in ipairs(ZONES) do
        table.insert(zoneInfo, {name = zone.name, xMin = zone.xMin, xMax = zone.xMax, value = zone.value})
    end
    player:SetAttribute("ZoneInfo", HttpService:JSONEncode(zoneInfo))

    -- Restore in-memory brainrots first (for temporary disconnects within same server session)
    if #data.placedBrainrots > 0 then
        for i, placed in ipairs(data.placedBrainrots) do
            if placed.part and placed.part.Parent then
                local slotIndex = placed.slot or i  -- Use stored slot, fallback to index
                placeBrainrotOnBase(player, placed.part, slotIndex, placed.incomeRate or (placed.value or 0) / 10)
            end
        end
    end

    updatePlayerAttributes(player)

    -- Now load from DataStore (yields)
    local key = "player_" .. player.UserId
    local savedData = playerStore:GetAsync(key)

    if savedData then
        print("[DataStore] Loaded data for " .. player.Name .. ": money=" .. savedData.money .. ", speedLevel=" .. savedData.speedLevel)
        data.money = savedData.money or 0
        data.speedLevel = savedData.speedLevel or 1

        -- Restore placedBrainrots from DataStore if not already restored from memory
        if #data.placedBrainrots == 0 and savedData.placedBrainrots and #savedData.placedBrainrots > 0 then
            print("[DataStore] Restoring " .. #savedData.placedBrainrots .. " brainrots for " .. player.Name)
            for i, brainrotData in ipairs(savedData.placedBrainrots) do
                local zone = getZoneByName(brainrotData.zone)
                if zone then
                    local slotIndex = brainrotData.slot or i  -- Use saved slot, fallback to sequential
                    local part = createBrainrotFromData(brainrotData, zone)
                    placeBrainrotOnBase(player, part, slotIndex, brainrotData.incomeRate)
                    table.insert(data.placedBrainrots, {
                        part = part,
                        value = brainrotData.value,
                        incomeRate = brainrotData.incomeRate,
                        displayName = brainrotData.displayName,
                        slot = slotIndex  -- Store the slot
                    })
                else
                    warn("[DataStore] Unknown zone '" .. tostring(brainrotData.zone) .. "' for brainrot, skipping")
                end
            end
        end

        updatePlayerAttributes(player)
    else
        print("[DataStore] No saved data for " .. player.Name .. ", using defaults")
    end
    return true
end

local function savePlayerData(player)
    local data = getPlayerData(player)
    if not data then
        warn("[DataStore] Cannot save: no data for " .. player.Name)
        return
    end

    -- Serialize placedBrainrots (strip Part references, keep only data needed for recreation)
    local serializedBrainrots = {}
    for _, placed in ipairs(data.placedBrainrots) do
        table.insert(serializedBrainrots, {
            value = placed.value,
            incomeRate = placed.incomeRate,
            zone = placed.part:GetAttribute("Zone"),
            modelUrl = placed.part:GetAttribute("ModelUrl"),
            displayName = placed.displayName,
            slot = placed.slot  -- Persist slot number
        })
    end

    local key = "player_" .. player.UserId
    local saveData = {
        money = data.money,
        speedLevel = data.speedLevel,
        placedBrainrots = serializedBrainrots,
    }

    playerStore:SetAsync(key, saveData)
    print("[DataStore] Saved data for " .. player.Name .. ": money=" .. data.money .. ", speedLevel=" .. data.speedLevel .. ", brainrots=" .. #serializedBrainrots)
    dirtyPlayers[player.UserId] = false
    saveAccumulator[player.UserId] = 0
end

--------------------------------------------------------------------------------
-- MAP CREATION
--------------------------------------------------------------------------------

local function createMap()
    -- 800-STUD MAP: X is the long axis (-400 to +400), Z is short axis
    -- Base zone on right (X >= 300), collection zones spread from X=-400 to X=300

    -- Main floor (804 x 80 studs)
    local floor = Instance.new("Part")
    floor.Name = "Floor"
    floor.Size = Vector3.new(MAP_LENGTH + 4, 2, MAP_WIDTH)
    floor.Position = Vector3.new(0, -1, 0)  -- Top at Y=0
    floor.Anchored = true
    floor.Color = Color3.fromRGB(100, 150, 100)  -- Green grass
    floor:AddTag("Static")
    floor.Parent = Workspace

    -- Create zone overlays (semi-transparent colored zones)
    for i, zone in ipairs(ZONES) do
        local zoneWidth = zone.xMax - zone.xMin
        local zoneCenterX = (zone.xMin + zone.xMax) / 2

        local zoneOverlay = Instance.new("Part")
        zoneOverlay.Name = "Zone_" .. zone.name
        zoneOverlay.Size = Vector3.new(zoneWidth, 0.1, MAP_WIDTH)
        zoneOverlay.Position = Vector3.new(zoneCenterX, 0.05, 0)
        zoneOverlay.Anchored = true
        zoneOverlay.Color = zone.color
        zoneOverlay.Transparency = 0.7
        zoneOverlay.CanCollide = false
        zoneOverlay:SetAttribute("IsZone", true)
        zoneOverlay:SetAttribute("ZoneName", zone.name)
        zoneOverlay:AddTag("Static")
        zoneOverlay.Parent = Workspace
    end

    -- Safe area ground (slightly lighter green) - covers from end of zones (X=350) to right edge
    local safeAreaStartX = 350  -- Where colored zones end
    local safeAreaEndX = MAP_LENGTH / 2 + 2
    local safeAreaSizeX = safeAreaEndX - safeAreaStartX
    local safeAreaGround = Instance.new("Part")
    safeAreaGround.Name = "SafeAreaGround"
    safeAreaGround.Size = Vector3.new(safeAreaSizeX, 0.1, MAP_WIDTH)
    safeAreaGround.Position = Vector3.new(safeAreaStartX + safeAreaSizeX / 2, 0.05, 0)
    safeAreaGround.Anchored = true
    safeAreaGround.Color = Color3.fromRGB(120, 180, 120)  -- Lighter green
    safeAreaGround.CanCollide = false
    safeAreaGround:SetAttribute("IsSafeZone", true)
    safeAreaGround:AddTag("Static")
    safeAreaGround.Parent = Workspace

    -- Player base platforms + deposit areas
    local baseCenterX = getBaseCenterX()
    for i = 1, BASE_COUNT do
        local baseZ = getBaseCenterZForIndex(i)

        -- Base platform is visual only - floor provides collision
        -- This avoids autostep issues at platform edges
        local basePlatform = Instance.new("Part")
        basePlatform.Name = "BasePlatform_" .. i
        basePlatform.Size = Vector3.new(BASE_SIZE_X, BASE_PLATFORM_HEIGHT, BASE_SIZE_Z)
        basePlatform.Position = Vector3.new(baseCenterX, BASE_PLATFORM_HEIGHT / 2, baseZ)
        basePlatform.Anchored = true
        basePlatform.Color = Color3.fromRGB(120, 180, 120)  -- Slightly lighter green
        basePlatform.CanCollide = false
        basePlatform:SetAttribute("IsBase", true)
        basePlatform:SetAttribute("BaseIndex", i)
        basePlatform:AddTag("Static")
        basePlatform.Parent = Workspace

        local depositArea = Instance.new("Part")
        depositArea.Name = "DepositArea_" .. i
        depositArea.Size = Vector3.new(BASE_SIZE_X - 4, 0.1, BASE_SIZE_Z - 4)
        depositArea.Position = Vector3.new(baseCenterX, BASE_PLATFORM_HEIGHT + 0.05, baseZ)
        depositArea.Anchored = true
        depositArea.Color = Color3.fromRGB(200, 200, 50)  -- Yellow
        depositArea.CanCollide = false
        depositArea:SetAttribute("IsDepositArea", true)
        depositArea:SetAttribute("BaseIndex", i)
        depositArea:AddTag("Static")
        depositArea.Parent = Workspace
    end

    -- Speed shop (in base zone at X=390)
    local shop = Instance.new("Part")
    shop.Name = "SpeedShop"
    shop.Size = Vector3.new(10, 5, 10)
    shop.Position = Vector3.new(BASE_ZONE_X_START + BASE_ZONE_SIZE - 10, 2.5, BASE_ROW_LENGTH_Z / 2 - BASE_SIZE_Z / 2)
    shop.Anchored = true
    shop.Color = Color3.fromRGB(100, 100, 200)  -- Blue
    shop:SetAttribute("IsShop", true)
    shop:AddTag("Static")
    shop.Parent = Workspace

    -- Walls to prevent going out of bounds
    local walls = {
        -- Front/back walls (along Z axis edges)
        {Vector3.new(0, 25, MAP_WIDTH / 2 + 1), Vector3.new(MAP_LENGTH + 4, 50, 2)},   -- Front (Z+)
        {Vector3.new(0, 25, -MAP_WIDTH / 2 - 1), Vector3.new(MAP_LENGTH + 4, 50, 2)},  -- Back (Z-)
        -- Left/right walls (along X axis edges)
        {Vector3.new(MAP_LENGTH / 2 + 1, 25, 0), Vector3.new(2, 50, MAP_WIDTH)},       -- Right (X+, at X=401)
        {Vector3.new(-MAP_LENGTH / 2 - 1, 25, 0), Vector3.new(2, 50, MAP_WIDTH)},      -- Left (X-, at X=-401)
    }

    for i, data in ipairs(walls) do
        local wall = Instance.new("Part")
        wall.Name = "Wall_" .. i
        wall.Position = data[1]
        wall.Size = data[2]
        wall.Anchored = true
        wall.Transparency = 1
        wall.CanCollide = true
        wall:AddTag("Static")
        wall.Parent = Workspace
    end

    print("Map created (800-stud): Base zone (X>=350), 6 rarity zones, 8 bases")

    -- Create GameState object for shared game info (updates every tick)
    local gameStateFolder = Instance.new("Folder")
    gameStateFolder.Name = "GameState"
    gameStateFolder:SetAttribute("WaveInterval", WAVE_CONFIG.spawnInterval)
    gameStateFolder:SetAttribute("WaveTimeRemaining", WAVE_CONFIG.spawnInterval)
    gameStateFolder:SetAttribute("ActiveWaveCount", 0)
    gameStateFolder:SetAttribute("SpawnedBrainrots", 0)
    -- Static zone info (set once)
    local zoneInfoForState = {}
    for _, zone in ipairs(ZONES) do
        table.insert(zoneInfoForState, {name = zone.name, xMin = zone.xMin, xMax = zone.xMax, value = zone.value})
    end
    gameStateFolder:SetAttribute("ZoneInfo", HttpService:JSONEncode(zoneInfoForState))
    gameStateFolder.Parent = Workspace
    print("GameState folder created in Workspace")
end

--------------------------------------------------------------------------------
-- BRAINROT SYSTEM
--------------------------------------------------------------------------------

local function selectRandomZone()
    -- Calculate total weight
    local totalWeight = 0
    for _, zone in ipairs(ZONES) do
        totalWeight = totalWeight + zone.weight
    end

    -- Select zone based on weighted random
    local roll = math.random() * totalWeight
    local cumulative = 0
    for _, zone in ipairs(ZONES) do
        cumulative = cumulative + zone.weight
        if roll <= cumulative then
            return zone
        end
    end

    -- Fallback to first zone
    return ZONES[1]
end

local function spawnBrainrot()
    if #brainrots >= MAX_BRAINROTS then
        return
    end

    -- Select zone based on weights (40% Common, 25% Uncommon, etc.)
    local zone = selectRandomZone()

    -- Random position within the selected zone
    local x = math.random(zone.xMin + 5, zone.xMax - 5)
    local zMin = -math.floor(MAP_WIDTH / 2 - 5)
    local zMax = math.floor(MAP_WIDTH / 2 - 5)
    local z = math.random(zMin, zMax)

    -- Check if this zone has character models
    local character = getCharacterForZone(zone.name)
    local displayName = zone.name
    local value = zone.value
    local incomeRate = zone.value / 10  -- e.g., value 10 = 1$/sec

    -- If character model available, use character's yield for value
    if character then
        displayName = character.name
        value = character.yield * 10  -- Scale yield to match zone value scale
        incomeRate = character.yield
    end

    -- Character models are taller (player-sized), spheres are small
    local partSize = character and Vector3.new(2, 5, 2) or Vector3.new(2, 2, 2)
    local yPos = character and 2.5 or 1  -- Raise character models so feet touch ground

    local brainrot = Instance.new("Part")
    brainrot.Name = "Brainrot"
    brainrot.Size = partSize
    brainrot.Position = Vector3.new(x, yPos, z)
    brainrot.Anchored = true
    brainrot.CanCollide = false
    brainrot.Shape = Enum.PartType.Ball
    brainrot.Color = zone.color
    brainrot.Material = Enum.Material.Neon
    brainrot:SetAttribute("IsBrainrot", true)
    brainrot:SetAttribute("Value", value)
    brainrot:SetAttribute("Zone", zone.name)

    -- Set ModelUrl for character models
    if character then
        brainrot:SetAttribute("ModelUrl", "/static/models/clawrots/" .. character.model)
    end

    -- Add floating label (BillboardGui) so it is always attached
    local billboard = Instance.new("BillboardGui")
    billboard.Name = "BrainrotLabel"
    billboard.Size = UDim2.new(0, 120, 0, 60)
    billboard.StudsOffset = Vector3.new(0, 3, 0)
    billboard.AlwaysOnTop = true
    billboard.Parent = brainrot

    -- Name label (white)
    local nameLabel = Instance.new("TextLabel")
    nameLabel.Name = "NameLabel"
    nameLabel.Size = UDim2.new(1, 0, 0.33, 0)
    nameLabel.Position = UDim2.new(0, 0, 0, 0)
    nameLabel.Text = displayName
    nameLabel.TextColor3 = Color3.fromRGB(255, 255, 255)
    nameLabel.TextScaled = true
    nameLabel.BackgroundTransparency = 1
    nameLabel.Parent = billboard

    -- Rarity label (zone color)
    local rarityLabel = Instance.new("TextLabel")
    rarityLabel.Name = "RarityLabel"
    rarityLabel.Size = UDim2.new(1, 0, 0.33, 0)
    rarityLabel.Position = UDim2.new(0, 0, 0.33, 0)
    rarityLabel.Text = zone.name
    rarityLabel.TextColor3 = zone.color
    rarityLabel.TextScaled = true
    rarityLabel.BackgroundTransparency = 1
    rarityLabel.Parent = billboard

    -- Income label (green)
    local incomeLabel = Instance.new("TextLabel")
    incomeLabel.Name = "IncomeLabel"
    incomeLabel.Size = UDim2.new(1, 0, 0.33, 0)
    incomeLabel.Position = UDim2.new(0, 0, 0.66, 0)
    incomeLabel.Text = "$" .. incomeRate .. "/s"
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

    -- Get displayName from billboard for persistence
    local displayName = brainrot:GetAttribute("Zone")  -- Default to zone name
    local billboard = brainrot:FindFirstChild("BrainrotLabel")
    if billboard then
        local nameLabel = billboard:FindFirstChild("NameLabel")
        if nameLabel then
            displayName = nameLabel.Text
        end
    end

    if attachBrainrotToPlayer(player, brainrot) then
        table.insert(data.carriedBrainrots, {part = brainrot, value = value, displayName = displayName})
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

    if not isAtPlayerBase(player, pos) then
        print("[Deposit] " .. player.Name .. " not at their base (X=" .. pos.X .. ", Z=" .. pos.Z .. ")")
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

    -- Find all available slots (1 to BASE_MAX_BRAINROTS)
    local usedSlots = {}
    for _, placed in ipairs(data.placedBrainrots) do
        usedSlots[placed.slot] = true
    end

    local availableSlotList = {}
    for s = 1, BASE_MAX_BRAINROTS do
        if not usedSlots[s] then
            table.insert(availableSlotList, s)
        end
    end

    if #availableSlotList == 0 then
        print("[Deposit] " .. player.Name .. " base is full! (" .. #data.placedBrainrots .. "/" .. BASE_MAX_BRAINROTS .. ")")
        return false
    end

    -- Calculate how many we can deposit
    local depositCount = math.min(#data.carriedBrainrots, #availableSlotList)

    -- Place brainrots on base floor using available slots
    for i = 1, depositCount do
        local carried = data.carriedBrainrots[i]
        local incomeRate = carried.value / 10  -- e.g., value 10 = 1$/sec
        local slotIndex = availableSlotList[i]  -- Use first available slot
        placeBrainrotOnBase(player, carried.part, slotIndex, incomeRate)

        table.insert(data.placedBrainrots, {
            part = carried.part,
            value = carried.value,
            incomeRate = incomeRate,
            displayName = carried.displayName,
            slot = slotIndex  -- Store permanent slot number
        })
    end

    -- Keep brainrots that couldn't be deposited, remove the deposited ones
    local remaining = {}
    for i = depositCount + 1, #data.carriedBrainrots do
        table.insert(remaining, data.carriedBrainrots[i])
    end

    if #remaining > 0 then
        print("[Deposit] " .. player.Name .. " base full - kept " .. #remaining .. " brainrots")
    end

    data.carriedBrainrots = remaining

    updatePlayerAttributes(player)
    savePlayerData(player)

    local totalIncome = getTotalPassiveIncome(data)
    print("[Deposit] " .. player.Name .. " placed " .. depositCount .. " brainrots (total income: $" .. totalIncome .. "/sec, base: " .. #data.placedBrainrots .. "/" .. BASE_MAX_BRAINROTS .. ")")

    return true
end

local function destroyBrainrot(player, slotIndex)
    local data = getPlayerData(player)
    if not data then return false end

    -- Find brainrot by slot number
    local foundIdx = nil
    for i, placed in ipairs(data.placedBrainrots) do
        if placed.slot == slotIndex then
            foundIdx = i
            break
        end
    end

    if not foundIdx then
        warn("[Destroy] No brainrot at slot " .. slotIndex)
        return false
    end

    local placed = data.placedBrainrots[foundIdx]
    if placed.part then placed.part:Destroy() end
    table.remove(data.placedBrainrots, foundIdx)

    updatePlayerAttributes(player)
    markDirty(player)
    print("[Destroy] " .. player.Name .. " destroyed brainrot at slot " .. slotIndex .. " (base: " .. #data.placedBrainrots .. "/" .. BASE_MAX_BRAINROTS .. ")")
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
    markDirty(player)
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
            -- Spawn at player's base (per-player Z offset)
            local spawnPos = getPlayerSpawnPosition(player)
            hrp.CFrame = CFrame.new(spawnPos.X, spawnPos.Y, spawnPos.Z)
            hrp.Velocity = Vector3.new(0, 0, 0)
            print("[Spawn] " .. player.Name .. " spawned at (" .. spawnPos.X .. ", " .. spawnPos.Y .. ", " .. spawnPos.Z .. ")")
        end
    end

    updatePlayerAttributes(player)
end

--------------------------------------------------------------------------------
-- WAVE SYSTEM
--------------------------------------------------------------------------------

local function spawnWave()
    local waveParts = {}
    local startZ = -MAP_WIDTH / 2 + WAVE_CONFIG.modelWidth / 2

    for i = 0, WAVE_SEGMENT_COUNT - 1 do
        local wave = Instance.new("Part")
        wave.Name = "TsunamiWave_" .. i
        wave.Size = Vector3.new(WAVE_CONFIG.thickness, WAVE_CONFIG.modelHeight, WAVE_CONFIG.modelWidth)
        wave.Position = Vector3.new(WAVE_CONFIG.startX, WAVE_CONFIG.modelHeight / 2, startZ + i * WAVE_CONFIG.modelWidth)
        wave.Anchored = true
        wave.CanCollide = false
        wave.Transparency = 1  -- Hide the box, only show the model
        wave:SetAttribute("ModelUrl", WAVE_CONFIG.modelUrl)
        wave:SetAttribute("ModelScale", WAVE_MODEL_SCALE)
        wave.Parent = Workspace
        table.insert(waveParts, wave)
    end

    table.insert(waves, {parts = waveParts, x = WAVE_CONFIG.startX})
    print("Tsunami wave spawned with " .. WAVE_SEGMENT_COUNT .. " segments!")
end

local function dropBrainrot(position, brainrotData)
    -- Remove weld if attached to player
    if brainrotData.part then
        local weld = brainrotData.part:FindFirstChild("BrainrotWeld")
        if weld then weld:Destroy() end

        -- Move the carried brainrot part back to world position
        brainrotData.part.Anchored = true
        brainrotData.part.Position = Vector3.new(position.X, 1, position.Z)
        -- Re-add to global brainrots list so it can be picked up again
        table.insert(brainrots, brainrotData.part)
    end
end

local function updateWaves(dt)
    for i = #waves, 1, -1 do
        local wave = waves[i]

        -- Move wave toward base
        wave.x = wave.x + WAVE_CONFIG.speed * dt
        for _, part in ipairs(wave.parts) do
            local currentZ = part.Position.Z
            part.Position = Vector3.new(wave.x, WAVE_CONFIG.modelHeight / 2, currentZ)
        end

        -- Check collision with players
        for _, player in ipairs(Players:GetPlayers()) do
            local pos = getCharacterPosition(player)
            if pos and not isInSafeZone(pos) then
                local halfThickness = WAVE_CONFIG.thickness / 2
                if pos.X >= wave.x - halfThickness and pos.X <= wave.x + halfThickness then
                    -- Player hit by wave
                    local data = getPlayerData(player)
                    if data and #data.carriedBrainrots > 0 then
                        -- Drop carried brainrots at player position
                        for _, brainrot in ipairs(data.carriedBrainrots) do
                            dropBrainrot(pos, brainrot)
                        end
                        print(player.Name .. " dropped " .. #data.carriedBrainrots .. " brainrots!")
                        data.carriedBrainrots = {}
                    end
                    -- Respawn player in safe area
                    spawnPlayer(player)
                    print(player.Name .. " was hit by tsunami!")
                end
            end
        end

        -- Remove wave if past end point
        if wave.x >= WAVE_CONFIG.endX then
            for _, part in ipairs(wave.parts) do
                part:Destroy()
            end
            table.remove(waves, i)
        end
    end
end

local function initializePlayer(player)
    -- Load saved data from DataStore (yields but works in coroutine)
    if not loadPlayerData(player) then
        return
    end

    -- Create GUI
    createPlayerGUI(player)

    -- Spawn when character is added (or now if already exists)
    player.CharacterAdded:Connect(function(character)
        -- Wait for HumanoidRootPart
        local hrp = character:WaitForChild("HumanoidRootPart", 5)
        if hrp then
            local spawnPos = getPlayerSpawnPosition(player)
            hrp.CFrame = CFrame.new(spawnPos.X, spawnPos.Y, spawnPos.Z)
            hrp.Velocity = Vector3.new(0, 0, 0)
            print("[Spawn] " .. player.Name .. " spawned at (" .. spawnPos.X .. ", " .. spawnPos.Y .. ", " .. spawnPos.Z .. ")")
        end

        -- Disable player-to-player collisions
        for _, part in ipairs(character:GetChildren()) do
            if part:IsA("BasePart") then
                part.CanCollide = false
            end
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

    dirtyPlayers[player.UserId] = false
    saveAccumulator[player.UserId] = 0

    local data = getPlayerData(player)
    if data then
        print("[Init] " .. player.Name .. " initialized (money: " .. data.money .. ", speedLevel: " .. data.speedLevel .. ")")
    else
        warn("[Init] " .. player.Name .. " initialized but no player data found")
    end
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
                if type(pos) ~= "table" or #pos < 3 then
                    warn("[MoveTo] Invalid position payload for " .. player.Name)
                    return
                end
                if type(pos[1]) ~= "number" or type(pos[2]) ~= "number" or type(pos[3]) ~= "number" then
                    warn("[MoveTo] Non-numeric position for " .. player.Name)
                    return
                end
                print("[Input] MoveTo " .. player.Name .. " -> (" .. pos[1] .. ", " .. pos[2] .. ", " .. pos[3] .. ")")
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

        elseif inputType == "Destroy" then
            if data and data.index then
                destroyBrainrot(player, data.index)
            end
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
        savedPlacedBrainrots[player.UserId] = data.placedBrainrots
        for _, carried in ipairs(data.carriedBrainrots) do
            if carried.part and carried.part.Parent then
                carried.part:Destroy()
            end
        end
        for _, placed in ipairs(data.placedBrainrots) do
            if placed.part and placed.part.Parent then
                placed.part.Anchored = true
                placed.part.CanCollide = false
                placed.part.Position = STORED_BRAINROT_POSITION
            end
        end
    end

    local baseIndex = userBaseIndex[player.UserId]
    if baseIndex then
        baseSlots[baseIndex] = nil
    end
    userBaseIndex[player.UserId] = nil

    playerData[player.UserId] = nil
    incomeAccumulator[player.UserId] = nil
    saveAccumulator[player.UserId] = nil
    dirtyPlayers[player.UserId] = nil
end)

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    updateBrainrotSpawning(dt)
    cleanupBrainrots()

    -- Wave system update
    waveTimer = waveTimer + dt
    if waveTimer >= WAVE_CONFIG.spawnInterval then
        spawnWave()
        waveTimer = 0
    end
    updateWaves(dt)

    -- Update shared game state (every tick for real-time observation)
    local gameStateFolder = Workspace:FindFirstChild("GameState")
    if gameStateFolder then
        gameStateFolder:SetAttribute("WaveTimeRemaining", WAVE_CONFIG.spawnInterval - waveTimer)
        gameStateFolder:SetAttribute("ActiveWaveCount", #waves)
        gameStateFolder:SetAttribute("SpawnedBrainrots", #brainrots)
    end

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
                markDirty(player)
            end
        end

        saveAccumulator[player.UserId] = (saveAccumulator[player.UserId] or 0) + dt
        if saveAccumulator[player.UserId] >= SAVE_INTERVAL and dirtyPlayers[player.UserId] then
            savePlayerData(player)
        end
    end

    -- Leaderboard updates
    leaderboardUpdateTimer = leaderboardUpdateTimer + dt
    if leaderboardUpdateTimer >= LEADERBOARD_UPDATE_INTERVAL then
        leaderboardUpdateTimer = 0
        -- Update each player's leaderboard entry
        for _, player in ipairs(Players:GetPlayers()) do
            updatePlayerLeaderboardEntry(player)
        end
    end

    leaderboardFetchTimer = leaderboardFetchTimer + dt
    if leaderboardFetchTimer >= LEADERBOARD_FETCH_INTERVAL then
        leaderboardFetchTimer = 0
        -- Fetch leaderboard and update all GUIs
        fetchLeaderboard()
        updateAllLeaderboardGUIs()
    end
end)

-- Initial brainrot spawn
for i = 1, 10 do
    spawnBrainrot()
end

-- Initial leaderboard fetch
fetchLeaderboard()

print("=== Escape Tsunami For Brainrots (Phase 1) ===")
print("Collect brainrots, deposit for money, buy speed upgrades!")
print("Data is persisted via DataStore.")
print("Leaderboard tracks top passive income earners.")

-- Fall Bots - Obstacle Course Race
-- Players race through spinning bars, disappearing platforms, and dodge obstacles.
-- First to reach the crown wins!

local RunService = game:GetService("RunService")
local Players = game:GetService("Players")

--------------------------------------------------------------------------------
-- CONFIGURATION
--------------------------------------------------------------------------------

local MIN_PLAYERS = 2
local MAX_PLAYERS = 8
local COUNTDOWN_SECONDS = 3
local RACE_TIME_LIMIT = 120 -- seconds

-- Course dimensions
local COURSE_WIDTH = 30 -- X: -15 to +15
local COURSE_LENGTH = 300 -- Z: 0 to 300

-- Section boundaries (Z positions)
local SECTION_1_START = 0
local SECTION_1_END = 70
local SECTION_2_START = 70
local SECTION_2_END = 150
local SECTION_3_START = 150
local SECTION_3_END = 220
local SECTION_4_START = 220
local SECTION_4_END = 300

-- Gate Crashers config
local GATE_ROWS = 5
local DOORS_PER_ROW = 5
local BREAKABLE_PER_ROW = 3

-- Spinning Bars config
local SPIN_BAR_COUNT = 4
local SPIN_BAR_RADIUS = 12

-- Disappearing Path config
local PLATFORM_VISIBLE_TIME = 3.0
local PLATFORM_WARN_TIME = 1.0
local PLATFORM_HIDDEN_TIME = 2.0
local PLATFORM_CYCLE_TIME = PLATFORM_VISIBLE_TIME + PLATFORM_WARN_TIME + PLATFORM_HIDDEN_TIME

-- Final Dash config
local PENDULUM_COUNT = 4
local PENDULUM_RADIUS = 12

-- Finish line
local FINISH_Z = 295
local FINISH_RADIUS = 5

-- Spawn
local SPAWN_Y = 5

-- Respawn fall threshold
local FALL_Y_THRESHOLD = -10

-- Colors
local COLOR_FLOOR = Color3.fromRGB(200, 200, 210)
local COLOR_WALL = Color3.fromRGB(120, 120, 140)
local COLOR_GATE_SOLID = Color3.fromRGB(180, 60, 60)
local COLOR_GATE_BREAKABLE = Color3.fromRGB(60, 180, 60)
local COLOR_SPIN_BAR = Color3.fromRGB(255, 80, 80)
local COLOR_PLATFORM = Color3.fromRGB(80, 180, 255)
local COLOR_PLATFORM_WARN = Color3.fromRGB(255, 100, 100)
local COLOR_PENDULUM = Color3.fromRGB(200, 100, 255)
local COLOR_CROWN = Color3.fromRGB(255, 215, 0)
local COLOR_SECTION_2_FLOOR = Color3.fromRGB(180, 200, 180)
local COLOR_SECTION_4_FLOOR = Color3.fromRGB(200, 180, 200)

--------------------------------------------------------------------------------
-- GAME STATE
--------------------------------------------------------------------------------

local gameState = "waiting" -- waiting, countdown, racing, finished
local gameTime = 0
local countdownTime = COUNTDOWN_SECONDS
local raceTimer = RACE_TIME_LIMIT
local finishCount = 0
local totalPlayers = 0

local playerStates = {} -- UserId -> "racing" | "finished" | "dnf"
local finishOrder = {} -- ordered list of player UserIds who finished

-- Obstacle tracking
local spinningBars = {} -- {part, speed, radius, centerZ, height, centerX}
local disappearingSegments = {} -- {part, offset, originalColor}
local pendulumWalls = {} -- {part, speed, radius, centerX, centerZ, height}
local breakableDoors = {} -- {part}
local allObstacleParts = {} -- for cleanup

local crownPart = nil


--------------------------------------------------------------------------------
-- HELPERS
--------------------------------------------------------------------------------

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

local function teleportPlayer(player, position)
    local character = player.Character
    if character then
        local hrp = character:FindFirstChild("HumanoidRootPart")
        if hrp then
            hrp.Position = position
        end
    end
end

local function getSection(z)
    if z < SECTION_1_END then return 1
    elseif z < SECTION_2_END then return 2
    elseif z < SECTION_3_END then return 3
    else return 4 end
end

local function getCheckpointForSection(section)
    if section == 1 then return Vector3.new(0, SPAWN_Y, SECTION_1_START + 5)
    elseif section == 2 then return Vector3.new(0, SPAWN_Y, SECTION_2_START + 5)
    elseif section == 3 then return Vector3.new(0, SPAWN_Y, SECTION_3_START + 5)
    elseif section == 4 then return Vector3.new(0, SPAWN_Y, SECTION_4_START + 5)
    else return Vector3.new(0, SPAWN_Y, 5) end
end

local function createPart(name, size, position, color, anchored)
    local part = Instance.new("Part")
    part.Name = name
    part.Size = size
    part.Position = position
    part.Anchored = anchored ~= false -- default true
    part.Color = color or COLOR_FLOOR
    part.Parent = Workspace
    table.insert(allObstacleParts, part)
    return part
end

--------------------------------------------------------------------------------
-- MAP CREATION
--------------------------------------------------------------------------------

local function createFloor()
    -- Section 1 floor (Gate Crashers)
    createPart("Floor_S1",
        Vector3.new(COURSE_WIDTH, 2, SECTION_1_END - SECTION_1_START),
        Vector3.new(0, -1, (SECTION_1_START + SECTION_1_END) / 2),
        COLOR_FLOOR)

    -- Section 2 floor (Spinning Bars)
    createPart("Floor_S2",
        Vector3.new(COURSE_WIDTH, 2, SECTION_2_END - SECTION_2_START),
        Vector3.new(0, -1, (SECTION_2_START + SECTION_2_END) / 2),
        COLOR_SECTION_2_FLOOR)

    -- Section 3: no floor (disappearing platforms over void)

    -- Section 4 floor (Final Dash)
    createPart("Floor_S4",
        Vector3.new(COURSE_WIDTH, 2, SECTION_4_END - SECTION_4_START),
        Vector3.new(0, -1, (SECTION_4_START + SECTION_4_END) / 2),
        COLOR_SECTION_4_FLOOR)

    -- No side walls — players can fall off the edges

    -- Back wall (behind start)
    createPart("Wall_Back",
        Vector3.new(COURSE_WIDTH + 4, wallHeight, 2),
        Vector3.new(0, wallHeight / 2 - 1, -1),
        COLOR_WALL)
end

local function createGateCrashers()
    local doorWidth = COURSE_WIDTH / DOORS_PER_ROW
    local doorHeight = 8
    local doorDepth = 2

    for row = 1, GATE_ROWS do
        local z = SECTION_1_START + 10 + (row - 1) * 12

        -- Randomly choose which doors are breakable
        local breakable = {}
        for i = 1, DOORS_PER_ROW do
            breakable[i] = false
        end
        -- Shuffle and pick BREAKABLE_PER_ROW
        local indices = {}
        for i = 1, DOORS_PER_ROW do
            table.insert(indices, i)
        end
        for i = DOORS_PER_ROW, 2, -1 do
            local j = math.random(1, i)
            indices[i], indices[j] = indices[j], indices[i]
        end
        for i = 1, BREAKABLE_PER_ROW do
            breakable[indices[i]] = true
        end

        for col = 1, DOORS_PER_ROW do
            local x = -COURSE_WIDTH / 2 + (col - 0.5) * doorWidth
            local door = createPart(
                "Door_" .. row .. "_" .. col,
                Vector3.new(doorWidth - 0.5, doorHeight, doorDepth),
                Vector3.new(x, doorHeight / 2, z),
                breakable[col] and COLOR_GATE_BREAKABLE or COLOR_GATE_SOLID
            )
            door:SetAttribute("Breakable", breakable[col])
            door:SetAttribute("Row", row)
            table.insert(breakableDoors, door)
        end
    end
end

local function createSpinningBars()
    local speeds = {0.5, -0.67, 0.6, -0.43}
    local heights = {2, 5, 2, 4}
    local spacing = (SECTION_2_END - SECTION_2_START) / (SPIN_BAR_COUNT + 1)

    for i = 1, SPIN_BAR_COUNT do
        local centerZ = SECTION_2_START + spacing * i
        local bar = createPart(
            "SpinBar_" .. i,
            Vector3.new(COURSE_WIDTH - 4, 2, 2),
            Vector3.new(0, heights[i], centerZ),
            COLOR_SPIN_BAR
        )

        table.insert(spinningBars, {
            part = bar,
            speed = speeds[i],
            radius = SPIN_BAR_RADIUS,
            centerZ = centerZ,
            centerX = 0,
            height = heights[i],
        })
    end
end

local function createDisappearingPath()
    local segmentSize = Vector3.new(4, 1, 4)
    local pathSegments = {}

    -- Create a winding path of platforms across section 3
    -- 3 columns (left=-6, center=0, right=6), staggered rows
    local columns = {-6, 0, 6}
    local numRows = 12
    local rowSpacing = (SECTION_3_END - SECTION_3_START - 10) / numRows

    for row = 0, numRows - 1 do
        local z = SECTION_3_START + 5 + row * rowSpacing
        -- Create 2-3 platforms per row for multiple path options
        local numPlatforms = 2 + (row % 2) -- alternates 2 and 3

        local shuffled = {1, 2, 3}
        for i = 3, 2, -1 do
            local j = math.random(1, i)
            shuffled[i], shuffled[j] = shuffled[j], shuffled[i]
        end

        for p = 1, numPlatforms do
            local colIdx = shuffled[p]
            local x = columns[colIdx]
            local offset = (row * 1.3 + colIdx * 2.1) % PLATFORM_CYCLE_TIME

            local seg = createPart(
                "Platform_" .. row .. "_" .. p,
                segmentSize,
                Vector3.new(x, 0, z),
                COLOR_PLATFORM
            )

            table.insert(disappearingSegments, {
                part = seg,
                offset = offset,
                originalColor = COLOR_PLATFORM,
            })
        end
    end
end

local function createFinalDash()
    local speeds = {0.4, -0.5, 0.33, -0.6}
    local spacing = (SECTION_4_END - SECTION_4_START - 20) / (PENDULUM_COUNT + 1)

    for i = 1, PENDULUM_COUNT do
        local centerZ = SECTION_4_START + 10 + spacing * i
        local wall = createPart(
            "Pendulum_" .. i,
            Vector3.new(4, 12, 2),
            Vector3.new(0, 6, centerZ),
            COLOR_PENDULUM
        )

        table.insert(pendulumWalls, {
            part = wall,
            speed = speeds[i],
            radius = PENDULUM_RADIUS,
            centerX = 0,
            centerZ = centerZ,
            height = 6,
        })
    end

    -- Crown (finish line)
    crownPart = createPart(
        "Crown",
        Vector3.new(3, 3, 3),
        Vector3.new(0, 3, FINISH_Z),
        COLOR_CROWN
    )
    crownPart.CanCollide = false
end

--------------------------------------------------------------------------------
-- OBSTACLE UPDATES
--------------------------------------------------------------------------------

local function updateSpinningBars(dt)
    for _, bar in ipairs(spinningBars) do
        local angle = gameTime * bar.speed
        local x = math.cos(angle) * bar.radius + bar.centerX
        local z = math.sin(angle) * bar.radius + bar.centerZ
        bar.part.CFrame = CFrame.new(x, bar.height, z) * CFrame.Angles(0, angle, 0)
    end
end

local function updateDisappearingPath(dt)
    for _, seg in ipairs(disappearingSegments) do
        if seg.part and seg.part.Parent then
            local phase = (gameTime + seg.offset) % PLATFORM_CYCLE_TIME
            if phase < PLATFORM_VISIBLE_TIME then
                seg.part.Transparency = 0
                seg.part.CanCollide = true
                seg.part.Color = seg.originalColor
            elseif phase < PLATFORM_VISIBLE_TIME + PLATFORM_WARN_TIME then
                seg.part.Transparency = 0
                seg.part.CanCollide = true
                seg.part.Color = COLOR_PLATFORM_WARN
            else
                seg.part.Transparency = 1
                seg.part.CanCollide = false
            end
        end
    end
end

local function updateFinalDash(dt)
    for _, wall in ipairs(pendulumWalls) do
        local angle = gameTime * wall.speed
        local x = math.sin(angle) * wall.radius + wall.centerX
        wall.part.CFrame = CFrame.new(x, wall.height, wall.centerZ) * CFrame.Angles(0, 0, math.sin(angle) * 0.3)
    end

    -- Rotate crown for visual flair
    if crownPart and crownPart.Parent then
        crownPart.CFrame = CFrame.new(0, 3 + math.sin(gameTime * 2) * 0.5, FINISH_Z)
            * CFrame.Angles(0, gameTime * 2, 0)
    end
end

local function checkObstacleCollisions()
    for _, player in ipairs(Players:GetPlayers()) do
        local userId = player.UserId
        if playerStates[userId] == "racing" then
            local pos = getCharacterPosition(player)
            if not pos then continue end

            local section = getSection(pos.Z)

            -- Section 1: Check breakable door collisions
            if section == 1 then
                for i = #breakableDoors, 1, -1 do
                    local door = breakableDoors[i]
                    if door and door.Parent then
                        local doorPos = door.Position
                        local doorSize = door.Size
                        local dx = math.abs(pos.X - doorPos.X)
                        local dy = math.abs(pos.Y - doorPos.Y)
                        local dz = math.abs(pos.Z - doorPos.Z)

                        if dx < doorSize.X / 2 + 1.5 and dy < doorSize.Y / 2 + 1.5 and dz < doorSize.Z / 2 + 1.5 then
                            if door:GetAttribute("Breakable") then
                                door:Destroy()
                                table.remove(breakableDoors, i)
                            end
                        end
                    end
                end
            end

            -- Spinning bars and pendulums push players off the course via physics.
            -- No teleport on contact — only falling (checkPlayerFalls) respawns.
        end
    end
end

local function checkPlayerFalls()
    for _, player in ipairs(Players:GetPlayers()) do
        local userId = player.UserId
        if playerStates[userId] == "racing" then
            local pos = getCharacterPosition(player)
            if pos and pos.Y < FALL_Y_THRESHOLD then
                local section = getSection(pos.Z)
                teleportPlayer(player, getCheckpointForSection(section))
            end
        end
    end
end

local function checkFinishLine()
    for _, player in ipairs(Players:GetPlayers()) do
        local userId = player.UserId
        if playerStates[userId] == "racing" then
            local pos = getCharacterPosition(player)
            if pos and pos.Z >= FINISH_Z - FINISH_RADIUS then
                -- Player finished!
                playerStates[userId] = "finished"
                finishCount = finishCount + 1
                table.insert(finishOrder, userId)

                player:SetAttribute("Status", "finished")
                player:SetAttribute("FinishPosition", finishCount)

                print("[FINISH] " .. player.Name .. " finished in position #" .. finishCount)

                -- Update all players with finish count
                for _, p in ipairs(Players:GetPlayers()) do
                    p:SetAttribute("PlayersFinished", finishCount)
                end
            end
        end
    end
end

--------------------------------------------------------------------------------
-- PLAYER MANAGEMENT
--------------------------------------------------------------------------------

local function getSpawnPosition(playerIndex)
    -- Spread players across the start line
    local x = ((playerIndex - 1) % 4 - 1.5) * 4
    local z = math.floor((playerIndex - 1) / 4) * 3 + 3
    return Vector3.new(x, SPAWN_Y, z)
end

local function initializePlayer(player)
    local userId = player.UserId
    playerStates[userId] = "waiting"

    player:SetAttribute("Status", "waiting")
    player:SetAttribute("FinishPosition", 0)
    player:SetAttribute("PlayersFinished", finishCount)
    player:SetAttribute("TotalPlayers", totalPlayers)
    player:SetAttribute("TimeRemaining", RACE_TIME_LIMIT)
    player:SetAttribute("Section", 1)
end

local function startRaceForPlayer(player, playerIndex)
    local userId = player.UserId
    playerStates[userId] = "racing"
    player:SetAttribute("Status", "racing")

    -- Teleport to start
    teleportPlayer(player, getSpawnPosition(playerIndex))

    -- Set walk speed higher for racing
    local humanoid = getHumanoid(player)
    if humanoid then
        humanoid.WalkSpeed = 24
        humanoid.JumpPower = 50
    end
end

local function updatePlayerAttributes()
    for _, player in ipairs(Players:GetPlayers()) do
        local userId = player.UserId
        if playerStates[userId] == "racing" then
            local pos = getCharacterPosition(player)
            if pos then
                player:SetAttribute("Section", getSection(pos.Z))
            end
            player:SetAttribute("TimeRemaining", math.max(0, raceTimer))
            player:SetAttribute("PlayersFinished", finishCount)
            player:SetAttribute("TotalPlayers", totalPlayers)
        end
    end
end

--------------------------------------------------------------------------------
-- INPUT HANDLING
--------------------------------------------------------------------------------

local AgentInputService = game:GetService("AgentInputService")
if AgentInputService then
    AgentInputService.InputReceived:Connect(function(player, inputType, data)
        local userId = player.UserId

        if inputType == "MoveTo" then
            local humanoid = getHumanoid(player)
            if humanoid and data and data.position then
                local pos = data.position
                humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
            end

        elseif inputType == "Jump" then
            local humanoid = getHumanoid(player)
            if humanoid then
                humanoid:Jump()
            end
        end
    end)
end

--------------------------------------------------------------------------------
-- GAME LOOP
--------------------------------------------------------------------------------

local function updateCountdown(dt)
    countdownTime = countdownTime - dt

    local seconds = math.ceil(countdownTime)
    if seconds > 0 and math.ceil(countdownTime + dt) > seconds then
        print("[COUNTDOWN] " .. seconds .. "...")
        for _, player in ipairs(Players:GetPlayers()) do
            player:SetAttribute("Status", "countdown_" .. seconds)
        end
    end

    if countdownTime <= 0 then
        gameState = "racing"
        print("[RACE] GO!")

        -- Start all players
        local index = 1
        for _, player in ipairs(Players:GetPlayers()) do
            startRaceForPlayer(player, index)
            index = index + 1
        end
    end
end

local function updateRacing(dt)
    gameTime = gameTime + dt
    raceTimer = raceTimer - dt

    -- Update obstacles
    updateSpinningBars(dt)
    updateDisappearingPath(dt)
    updateFinalDash(dt)

    -- Check collisions
    checkObstacleCollisions()
    checkPlayerFalls()
    checkFinishLine()

    -- Update attributes
    updatePlayerAttributes()

    -- Check if race is over
    local allDone = true
    for _, player in ipairs(Players:GetPlayers()) do
        if playerStates[player.UserId] == "racing" then
            allDone = false
            break
        end
    end

    if allDone or raceTimer <= 0 then
        gameState = "finished"

        -- Mark remaining racers as DNF
        for _, player in ipairs(Players:GetPlayers()) do
            if playerStates[player.UserId] == "racing" then
                playerStates[player.UserId] = "dnf"
                player:SetAttribute("Status", "dnf")
            end
        end

        -- Print results
        print("\n=== RACE RESULTS ===")
        for i, userId in ipairs(finishOrder) do
            for _, player in ipairs(Players:GetPlayers()) do
                if player.UserId == userId then
                    print(i .. ". " .. player.Name)
                    break
                end
            end
        end
        for _, player in ipairs(Players:GetPlayers()) do
            if playerStates[player.UserId] == "dnf" then
                print("DNF: " .. player.Name)
            end
        end
        print("====================\n")
    end
end

RunService.Heartbeat:Connect(function(dt)
    if gameState == "waiting" then
        local playerCount = #Players:GetPlayers()
        if playerCount >= MIN_PLAYERS then
            gameState = "countdown"
            totalPlayers = playerCount
            countdownTime = COUNTDOWN_SECONDS
            print("[GAME] " .. totalPlayers .. " players joined. Starting countdown!")

            -- Update total for all players
            for _, player in ipairs(Players:GetPlayers()) do
                player:SetAttribute("TotalPlayers", totalPlayers)
                player:SetAttribute("Status", "countdown")
            end
        end

    elseif gameState == "countdown" then
        updateCountdown(dt)

    elseif gameState == "racing" then
        updateRacing(dt)
    end
end)

--------------------------------------------------------------------------------
-- INITIALIZATION
--------------------------------------------------------------------------------

-- Build the course
createFloor()
createGateCrashers()
createSpinningBars()
createDisappearingPath()
createFinalDash()

-- Initialize existing players
for _, player in ipairs(Players:GetPlayers()) do
    initializePlayer(player)
end

-- Handle new players joining
Players.PlayerAdded:Connect(function(player)
    initializePlayer(player)

    if gameState == "racing" then
        -- Late joiner starts racing immediately
        totalPlayers = totalPlayers + 1
        startRaceForPlayer(player, totalPlayers)
        for _, p in ipairs(Players:GetPlayers()) do
            p:SetAttribute("TotalPlayers", totalPlayers)
        end
    end
end)

Players.PlayerRemoving:Connect(function(player)
    playerStates[player.UserId] = nil
end)

print("Fall Bots obstacle course loaded!")
print("Waiting for " .. MIN_PLAYERS .. " players to start...")

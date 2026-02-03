# Tsunami Brainrot Map Expansion Plan

## Overview

Expand the map and fix multiplayer issues to match the real Roblox game.

## Current Issues

1. **Map too small**: 200 studs vs 600+ in real game
2. **Single zone**: No rarity progression
3. **Base overlap**: All players place brainrots at same location (X=75)
4. **No safe platforms**: Only one safe zone at base

---

## Implementation

### 1. Expand Map to 800 Studs

```lua
local MAP_WIDTH = 80              -- Z: -40 to +40
local MAP_LENGTH = 800            -- X: -400 to +400
local BASE_ZONE_SIZE = 100        -- Safe zone at high X
```

### 2. Add Rarity Zones

| Zone | X Range | Brainrot Value | Income/sec | Color |
|------|---------|----------------|------------|-------|
| Common | 300-400 | 10 | 1 | Pink |
| Uncommon | 200-300 | 30 | 3 | Blue |
| Rare | 50-200 | 80 | 8 | Purple |
| Epic | -100 to 50 | 200 | 20 | Orange |
| Legendary | -250 to -100 | 500 | 50 | Yellow |
| Secret | -400 to -250 | 1500 | 150 | Rainbow/White |

```lua
local ZONES = {
    {name = "Common", xMin = 300, xMax = 400, value = 10, color = Color3.fromRGB(255, 100, 255)},
    {name = "Uncommon", xMin = 200, xMax = 300, value = 30, color = Color3.fromRGB(100, 150, 255)},
    {name = "Rare", xMin = 50, xMax = 200, value = 80, color = Color3.fromRGB(180, 100, 255)},
    {name = "Epic", xMin = -100, xMax = 50, value = 200, color = Color3.fromRGB(255, 150, 50)},
    {name = "Legendary", xMin = -250, xMax = -100, value = 500, color = Color3.fromRGB(255, 255, 50)},
    {name = "Secret", xMin = -400, xMax = -250, value = 1500, color = Color3.fromRGB(255, 255, 255)},
}
```

### 3. Per-Player Base Areas

Each player gets their own base offset in Z:

```lua
local BASE_SPACING = 50  -- Studs between player bases in Z

local function getPlayerBaseOffset(player)
    local data = getPlayerData(player)
    return (data.playerIndex or 0) * BASE_SPACING
end

local function getPlayerSpawnPosition(player)
    local baseZ = getPlayerBaseOffset(player)
    return Vector3.new(350, 3, baseZ)  -- In safe zone
end
```

Update `placeBrainrotOnBase()`:
```lua
local function placeBrainrotOnBase(player, brainrot, slotIndex, incomeRate)
    local baseZ = getPlayerBaseOffset(player)
    -- ... position calculation uses baseZ
    brainrot.Position = Vector3.new(x, 1, baseZ + zOffset)
end
```

### 4. Safe Zone Platforms

Add platforms every ~100 studs along the track for tsunami hiding:

```lua
local SAFE_PLATFORMS = {
    {x = 200, size = Vector3.new(15, 0.5, 30)},
    {x = 100, size = Vector3.new(15, 0.5, 30)},
    {x = 0, size = Vector3.new(15, 0.5, 30)},
    {x = -100, size = Vector3.new(15, 0.5, 30)},
    {x = -200, size = Vector3.new(15, 0.5, 30)},
    {x = -300, size = Vector3.new(20, 0.5, 40)},  -- Larger for secret zone
}

local function createSafePlatforms()
    for i, platform in ipairs(SAFE_PLATFORMS) do
        local part = Instance.new("Part")
        part.Name = "SafePlatform_" .. i
        part.Size = platform.size
        part.Position = Vector3.new(platform.x, 0.25, 0)
        part.Anchored = true
        part.Color = Color3.fromRGB(100, 200, 100)
        part:SetAttribute("IsSafeZone", true)
        part.Parent = Workspace
    end
end
```

### 5. Zone-Based Brainrot Spawning

```lua
local function getZoneForPosition(x)
    for _, zone in ipairs(ZONES) do
        if x >= zone.xMin and x < zone.xMax then
            return zone
        end
    end
    return ZONES[1]  -- Default to Common
end

local function spawnBrainrot()
    -- Weight spawning toward closer zones (more common spawns near base)
    local zoneIndex = math.random(1, 100)
    local zone
    if zoneIndex <= 40 then zone = ZONES[1]      -- 40% Common
    elseif zoneIndex <= 65 then zone = ZONES[2]  -- 25% Uncommon
    elseif zoneIndex <= 80 then zone = ZONES[3]  -- 15% Rare
    elseif zoneIndex <= 90 then zone = ZONES[4]  -- 10% Epic
    elseif zoneIndex <= 97 then zone = ZONES[5]  -- 7% Legendary
    else zone = ZONES[6] end                      -- 3% Secret

    local x = math.random(zone.xMin, zone.xMax)
    local z = math.random(-35, 35)

    -- Create brainrot with zone-appropriate value and color
    local brainrot = Instance.new("Part")
    brainrot:SetAttribute("Value", zone.value)
    brainrot:SetAttribute("Zone", zone.name)
    brainrot.Color = zone.color
    -- ...
end
```

---

## Files to Modify

| File | Changes |
|------|---------|
| `games/tsunami-brainrot/game.lua` | All map/zone/base changes |
| `games/tsunami-brainrot/DESIGN.md` | Document new zones and mechanics |

---

## Future: Tsunami Waves

After map expansion, add tsunami wave system:

```lua
local WAVE_TYPES = {
    {name = "Slow", speed = 20, warning = 10},
    {name = "Medium", speed = 35, warning = 7},
    {name = "Fast", speed = 50, warning = 5},
    {name = "Lightning", speed = 80, warning = 3},
}

local waveTimer = 0
local WAVE_INTERVAL = 35  -- Seconds between waves

local function spawnWave()
    local waveType = WAVE_TYPES[math.random(1, #WAVE_TYPES)]
    -- Create wave part at far end of map
    -- Move toward base at waveType.speed
    -- Kill players on contact (unless in safe zone)
end
```

---

## Testing

1. Verify map renders correctly at new size
2. Test brainrot spawning in each zone
3. Test per-player base positioning with 2+ players
4. Verify deposit only works at player's own base
5. Check safe platforms are functional

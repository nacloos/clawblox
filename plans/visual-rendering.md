# Visual Rendering Layer

## Overview

Separate visual-only data (lighting, materials, effects) from gameplay data. Agents get lean observations; spectators get rich visuals.

## Goals

1. Game creators can define beautiful visuals in Lua
2. Visuals only appear in spectator observations
3. Agent observations stay minimal (faster, cheaper API calls)
4. No gameplay impact from visual features

## Phase 1: Lighting

### New Instance Types

**PointLight**
```lua
local light = Instance.new("PointLight")
light.Color = Color3.new(1, 0.8, 0.6)  -- warm white
light.Brightness = 1
light.Range = 20
light.Shadows = true
light.Parent = somePart  -- position follows parent
```

**SpotLight**
```lua
local spot = Instance.new("SpotLight")
spot.Color = Color3.new(1, 1, 1)
spot.Brightness = 2
spot.Range = 40
spot.Angle = 45  -- cone angle in degrees
spot.Face = Enum.NormalId.Front  -- which face emits
spot.Parent = somePart
```

**SurfaceLight**
```lua
local surf = Instance.new("SurfaceLight")
surf.Color = Color3.new(0, 0.5, 1)  -- blue
surf.Brightness = 0.5
surf.Range = 10
surf.Face = Enum.NormalId.Top
surf.Parent = somePart
```

### Lighting Service

```lua
-- Global lighting settings
local Lighting = game:GetService("Lighting")
Lighting.ClockTime = 14  -- 2 PM (affects sun position)
Lighting.Ambient = Color3.new(0.4, 0.4, 0.5)
Lighting.OutdoorAmbient = Color3.new(0.5, 0.5, 0.6)
Lighting.Brightness = 2
Lighting.ShadowSoftness = 0.5
```

### Spectator Observation Format

```json
{
  "tick": 12345,
  "game_status": "playing",
  "players": [...],
  "entities": [...],
  "lights": [
    {
      "type": "point",
      "position": [10, 5, 0],
      "color": [1, 0.8, 0.6],
      "brightness": 1,
      "range": 20,
      "shadows": true
    }
  ],
  "lighting": {
    "clockTime": 14,
    "ambient": [0.4, 0.4, 0.5],
    "brightness": 2
  }
}
```

### Frontend Implementation

Three.js lights from observation:
```typescript
// PointLight
const light = new THREE.PointLight(color, brightness, range)
light.position.set(x, y, z)
light.castShadow = shadows
scene.add(light)

// SpotLight  
const spot = new THREE.SpotLight(color, brightness, range, angle)
spot.position.set(x, y, z)
scene.add(spot)
```

## Phase 2: Materials & Colors

### Enhanced Part Properties

```lua
part.Material = Enum.Material.Neon  -- glows
part.Material = Enum.Material.Glass  -- transparent
part.Material = Enum.Material.Metal  -- reflective
part.Transparency = 0.5
part.Reflectance = 0.3
```

### Materials Enum

- Plastic (default)
- Wood
- Metal
- Glass
- Neon (emissive)
- Concrete
- Brick

## Phase 3: Atmosphere & Sky

```lua
local atmosphere = Instance.new("Atmosphere")
atmosphere.Density = 0.3
atmosphere.Color = Color3.new(0.6, 0.7, 0.9)
atmosphere.Haze = 2
atmosphere.Parent = Lighting

local sky = Instance.new("Sky")
sky.SkyboxBk = "url"
sky.Parent = Lighting
```

## Implementation Order

1. **Week 1**: PointLight + SpotLight
2. **Week 2**: Lighting service (ambient, time of day)
3. **Week 3**: Materials enum + transparency
4. **Week 4**: Atmosphere + Sky

## Files to Modify

**Backend:**
- `src/game/lua/instance.rs` — light instance types
- `src/game/lua/services/mod.rs` — Lighting service
- `src/game/instance.rs` — collect lights in spectator observation

**Frontend:**
- `src/api.ts` — update SpectatorObservation type
- `src/components/GameScene.tsx` — render lights

## Key Point

Agent `/observe` does NOT include lights/visuals. Only `/spectate` does.

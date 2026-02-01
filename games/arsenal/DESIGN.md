# Block Arsenal - Game Design Document

## Overview
- **Title:** Block Arsenal
- **Genre:** Gun Game / Arms Race
- **Players:** 2-8 (multiplayer FFA)
- **Objective:** Be first to get a kill with the Golden Knife (final weapon)

---

## Core Game Loop

```
Match Start → All players spawn with Weapon #1 (Pistol)
     ↓
Player kills enemy with gun → Advances to next weapon
     ↓
Player kills enemy with melee → Enemy DEMOTED 1 weapon level
     ↓
Player gets killed → Respawn (keep current weapon)
     ↓
First player to kill with Golden Knife (Weapon #15) → WINS
```

### Key Mechanics
- **Gun Kill:** Killer advances +1 weapon
- **Melee Kill:** Victim demoted -1 weapon (killer stays same)
- **Suicide/Fall:** No penalty, just respawn
- **Assist:** No weapon progress (kills only)

---

## Weapon Rotation (15 Weapons)

| # | Weapon | Type | Damage | Fire Rate | Range | Ammo | Special |
|---|--------|------|--------|-----------|-------|------|---------|
| 1 | Pistol | Hitscan | 25 | 0.3s | 80 | 12 | Starter weapon |
| 2 | SMG | Hitscan | 14 | 0.08s | 50 | 30 | High spread |
| 3 | Shotgun | Pellet×8 | 12 | 0.9s | 30 | 6 | Pump action |
| 4 | Assault Rifle | Hitscan | 22 | 0.12s | 100 | 30 | Low recoil |
| 5 | Sniper Rifle | Hitscan | 100 | 1.5s | 250 | 5 | Scope, one-shot headshot |
| 6 | LMG | Hitscan | 18 | 0.07s | 80 | 100 | -30% move speed |
| 7 | Revolver | Hitscan | 55 | 0.5s | 90 | 6 | High accuracy |
| 8 | Burst Rifle | Hitscan×3 | 18 | 0.35s | 90 | 24 | 3-round burst |
| 9 | Auto Shotgun | Pellet×6 | 9 | 0.35s | 25 | 8 | Semi-auto |
| 10 | DMR | Hitscan | 48 | 0.4s | 150 | 10 | Semi-auto, 2-shot kill |
| 11 | Minigun | Hitscan | 12 | 0.04s | 70 | 200 | 1s spin-up, -50% move |
| 12 | Crossbow | Projectile | 85 | 1.0s | 120 | 1 | Slow bolt, arc |
| 13 | Dual Pistols | Hitscan×2 | 20 | 0.2s | 70 | 24 | Alternating fire |
| 14 | Rocket Launcher | Projectile | 100 | 2.0s | 150 | 1 | 15 stud splash radius |
| 15 | Golden Knife | Melee | 999 | 0.4s | 6 | ∞ | **FINAL WEAPON** |

### Weapon Types
- **Hitscan:** Instant ray, no travel time
- **Pellet:** Multiple rays in cone pattern
- **Projectile:** Physical part with velocity, affected by gravity
- **Melee:** Short-range instant hit

### Damage Modifiers
| Body Part | Multiplier |
|-----------|------------|
| Head | 2.0x |
| Torso | 1.0x |
| Limbs | 0.75x |

---

## Melee System

Every player has access to a **melee attack** regardless of current weapon.

| Action | Key | Effect |
|--------|-----|--------|
| Melee Attack | F / Middle Click | Quick knife slash |
| Damage | 35 | (3 hits to kill) |
| Range | 6 studs | Short range |
| Cooldown | 0.8s | Between swings |

### Melee Kill Effect
When you kill an enemy with melee:
- **You:** Stay at current weapon (no advance)
- **Victim:** Demoted 1 weapon level (min: Weapon #1)
- Adds strategic depth - risk close combat to set back leaders

---

## Player Stats

| Stat | Value | Notes |
|------|-------|-------|
| Health | 100 HP | Regenerates after 5s no damage |
| Health Regen | 10 HP/s | After regen delay |
| Walk Speed | 16 studs/s | Base speed |
| Sprint Speed | 24 studs/s | Hold Shift |
| Jump Power | 50 | Standard jump |
| Respawn Time | 3 seconds | After death |

### Movement Modifiers
| Condition | Speed Modifier |
|-----------|----------------|
| LMG Equipped | -30% |
| Minigun Firing | -50% |
| Aiming Down Sights | -20% |
| Sprinting | +50% |

---

## Arena Design

### Layout (80×80 studs)
```
        SPAWN 1 (Blue)
     ┌─────────────────────────────────────┐
     │  ▓▓           ═══           ▓▓      │
     │  ▓▓    ┌───┐       ┌───┐    ▓▓      │
     │        │ C │       │ C │            │
S    │  ┌─┐   └───┘       └───┘   ┌─┐      │    S
P    │  │P│                       │P│      │    P
A    │  └─┘   ┌─────────────┐     └─┘      │    A
W    │        │   CENTER    │              │    W
N    │  ▓▓    │   PLATFORM  │     ▓▓       │    N
     │  ▓▓    │    ▲▲▲▲     │     ▓▓       │
2    │        └─────────────┘              │    4
     │  ┌─┐                       ┌─┐      │
(Grn)│  │P│   ┌───┐       ┌───┐   │P│      │(Ylw)
     │  └─┘   │ C │       │ C │   └─┘      │
     │        └───┘       └───┘            │
     │  ▓▓           ═══           ▓▓      │
     └─────────────────────────────────────┘
        SPAWN 3 (Red)

Legend:
  ▓▓  = Spawn zone (4 corners)
  C   = Cover block (8 total)
  P   = Raised platform (4 total, height: 8)
  ═══ = Bridge/walkway
  ▲▲▲ = Stairs to center
```

### Structure Details

| Element | Size | Height | Material | Notes |
|---------|------|--------|----------|-------|
| Floor | 80×80 | 0 | Concrete | Main arena |
| Walls | 80×8 | 8 | Brick | Invisible or low |
| Cover Blocks | 6×6 | 5 | Metal | 8 around arena |
| Corner Platforms | 10×10 | 8 | Concrete | Elevated positions |
| Center Platform | 16×16 | 6 | Concrete | Central high ground |
| Stairs | 4×8 | Ramp | Concrete | 4 leading to center |
| Bridges | 20×3 | 6 | Metal | Connect platforms |

### Spawn Points
| Spawn | Position | Color |
|-------|----------|-------|
| 1 | (-30, 1, -30) | Blue |
| 2 | (-30, 1, 30) | Green |
| 3 | (30, 1, 30) | Red |
| 4 | (30, 1, -30) | Yellow |

Spawns rotate to avoid spawn killing - player spawns at furthest point from killer.

---

## Match Flow

### Phase 1: Lobby (10-30 seconds)
- Players join and spawn in waiting area
- Minimum 2 players to start
- Countdown begins when minimum reached
- Late joiners can still enter

### Phase 2: Countdown (5 seconds)
- "3... 2... 1... GO!"
- Players teleport to spawn points
- All players set to Weapon #1
- Weapons locked until "GO"

### Phase 3: Active Match
- FFA combat until win condition
- Kills advance weapons
- Melee demotes victims
- Real-time leaderboard shows weapon progress

### Phase 4: Victory (5 seconds)
- Winner announced: "[Player] WINS!"
- Winner highlighted with golden effect
- Kill replay (future feature)
- All weapons disabled

### Phase 5: Reset (3 seconds)
- Stats saved (future)
- All players reset to Weapon #1
- Return to Countdown phase

### Match Settings
| Setting | Value |
|---------|-------|
| Min Players | 2 |
| Max Players | 8 |
| Time Limit | 10 minutes (optional) |
| Kill Limit | First to complete rotation |

---

## Scoring & Progression

### In-Match
| Stat | Tracking |
|------|----------|
| Current Weapon | Weapon # (1-15) |
| Kills | Total eliminations |
| Deaths | Times eliminated |
| Melee Kills | Knife kills (demotions caused) |
| Headshots | Headshot kills |

### Leaderboard Display
```
┌─────────────────────────────────┐
│  #  Player       Weapon   K/D  │
│  1  xXSlayerXx   [12]    8/2   │
│  2  NoobMaster   [10]    6/4   │
│  3  ProGamer     [9]     5/3   │
│  4  CasualFan    [7]     4/5   │
└─────────────────────────────────┘
```

---

## Visual Feedback

### Kill Feed (Top Right)
```
[SMG] PlayerA → PlayerB
[KNIFE] PlayerC → PlayerD (DEMOTED!)
[HEADSHOT] PlayerE → PlayerF
```

### Player Indicators
| Event | Visual |
|-------|--------|
| Damage Taken | Screen edge flash red |
| Kill | "+1" popup, weapon icon change |
| Demoted | "-1" popup, red flash |
| Headshot | "HEADSHOT" text, special marker |
| Low Health | Pulsing red vignette |
| Health Regen | Green pulse |

### Weapon Change
- Brief weapon model swap animation
- Weapon name popup: "ASSAULT RIFLE"
- Progress bar update

### Death
- Ragdoll physics (0.5s)
- Fade to black
- Respawn countdown overlay

### Victory
- Winner gets golden glow effect
- Confetti particles
- Slow-motion final kill (future)

---

## Audio Design (Future Implementation)

### Weapon Sounds
| Category | Sound |
|----------|-------|
| Pistol/Revolver | Sharp crack |
| SMG/Auto | Rapid pops |
| Shotgun | Deep boom |
| Rifle | Medium crack |
| Sniper | Heavy boom + echo |
| LMG/Minigun | Sustained fire |
| Rocket | Whoosh + explosion |
| Crossbow | Twang + thunk |
| Knife | Slash/stab |

### Feedback Sounds
| Event | Sound |
|-------|-------|
| Hit marker | Click/ding |
| Headshot | Louder ding |
| Kill | Kill confirmed sound |
| Death | Dramatic sting |
| Weapon Advance | Level-up jingle |
| Demoted | Sad trombone |
| Victory | Fanfare |
| Match Start | Air horn |

### Ambient
- Background music (toggleable)
- Footsteps
- Jump/land sounds
- Reload sounds

---

## Technical Implementation

### Services Used
```lua
local RunService = game:GetService("RunService")
local Players = game:GetService("Players")
local Workspace = game:GetService("Workspace")
```

### Player Attributes
| Attribute | Type | Description |
|-----------|------|-------------|
| CurrentWeapon | number | Weapon index (1-15) |
| Kills | number | Total kills this match |
| Deaths | number | Total deaths this match |
| MeleeKills | number | Knife kills |
| LastDamageTime | number | For health regen |
| AimDirection | Vector3 | Look direction (from frontend) |
| Firing | boolean | Is firing (from frontend) |
| Sprinting | boolean | Is sprinting (from frontend) |

### Core Systems

#### 1. Weapon System
```lua
Weapons = {
    {name="Pistol", type="hitscan", damage=25, fireRate=0.3, range=80, ammo=12},
    -- ... etc
}

function fireWeapon(player, weapon)
    if weapon.type == "hitscan" then
        local hit = Workspace:Raycast(origin, direction * weapon.range)
        if hit and isPlayer(hit.Instance) then
            dealDamage(hit.Instance, weapon.damage)
        end
    elseif weapon.type == "projectile" then
        spawnProjectile(origin, direction, weapon)
    end
end
```

#### 2. Kill/Advance System
```lua
function onPlayerKilled(killer, victim, wasMelee)
    if wasMelee then
        -- Demote victim
        local victimWeapon = victim:GetAttribute("CurrentWeapon")
        victim:SetAttribute("CurrentWeapon", math.max(1, victimWeapon - 1))
        print(killer.Name .. " DEMOTED " .. victim.Name)
    else
        -- Advance killer
        local killerWeapon = killer:GetAttribute("CurrentWeapon")
        if killerWeapon == 15 then
            declareWinner(killer)
        else
            killer:SetAttribute("CurrentWeapon", killerWeapon + 1)
        end
    end
end
```

#### 3. Spawn System
```lua
function getSpawnPoint(player, killer)
    -- Find furthest spawn from killer
    local spawnPoints = {spawn1, spawn2, spawn3, spawn4}
    local killerPos = killer and killer.Position or Vector3.new(0,0,0)

    local furthest = spawnPoints[1]
    local maxDist = 0
    for _, spawn in ipairs(spawnPoints) do
        local dist = (spawn.Position - killerPos).Magnitude
        if dist > maxDist then
            maxDist = dist
            furthest = spawn
        end
    end
    return furthest
end
```

#### 4. Health Regeneration
```lua
RunService.Heartbeat:Connect(function(dt)
    for _, player in ipairs(Players:GetPlayers()) do
        local humanoid = getHumanoid(player)
        local lastDamage = player:GetAttribute("LastDamageTime") or 0

        if tick() - lastDamage > 5 and humanoid.Health < 100 then
            humanoid.Health = math.min(100, humanoid.Health + 10 * dt)
        end
    end
end)
```

### Input Handling (Frontend → Attributes)
Since UserInputService is not implemented, the frontend sets attributes:

| Frontend Input | Attribute Set |
|----------------|---------------|
| Mouse position | AimDirection (Vector3) |
| Left click held | Firing (boolean) |
| Shift held | Sprinting (boolean) |
| F key pressed | MeleeAttack (boolean) |
| WASD | Direct character movement |

### Raycasting for Weapons
```lua
local raycastParams = RaycastParams.new()
raycastParams.FilterType = Enum.RaycastFilterType.Exclude
raycastParams.FilterDescendantsInstances = {player.Character}

local result = Workspace:Raycast(origin, direction * range, raycastParams)
if result then
    local hitPart = result.Instance
    local hitPosition = result.Position
    -- Process hit...
end
```

### Projectile System
```lua
function spawnProjectile(origin, direction, weapon)
    local projectile = Instance.new("Part")
    projectile.Size = Vector3.new(0.5, 0.5, 2)
    projectile.Position = origin
    projectile.Anchored = false
    projectile.CanCollide = true
    projectile:SetAttribute("Damage", weapon.damage)
    projectile:SetAttribute("Owner", player.Name)
    projectile:SetAttribute("Lifetime", 5)
    projectile.Velocity = direction * weapon.projectileSpeed
    projectile.Parent = Workspace

    -- Track for cleanup and collision
    table.insert(activeProjectiles, projectile)
end
```

---

## API Requirements

### Currently Available
| API | Status | Usage |
|-----|--------|-------|
| RunService.Heartbeat | ✓ | Game loop |
| Workspace:Raycast() | ✓ | Hitscan weapons |
| Players:GetPlayers() | ✓ | Player iteration |
| Part (all properties) | ✓ | Arena, projectiles |
| Humanoid.Health | ✓ | Damage system |
| Humanoid:TakeDamage() | ✓ | Apply damage |
| Humanoid.Died | ✓ | Death detection |
| Attributes | ✓ | Player state |
| Vector3, CFrame | ✓ | Positioning |
| Color3 | ✓ | Visual feedback |
| Events | ✓ | Connections |

### Would Enhance (Future)
| API | Priority | Enhancement |
|-----|----------|-------------|
| UserInputService | High | Native input |
| Debris:AddItem() | High | Auto-cleanup projectiles |
| TweenService | Medium | Smooth animations |
| SoundService | Medium | Audio feedback |
| GUI System | Medium | HUD, leaderboard |
| ReplicatedStorage | Low | Shared data |

---

## Balance Considerations

### Weapon Balance Philosophy
- Early weapons: Easy to use, lower skill ceiling
- Mid weapons: Balanced risk/reward
- Late weapons: High skill, high reward
- Final weapon: Intentionally difficult (melee only)

### Anti-Camping
- Health regen encourages aggression
- Small arena forces encounters
- Multiple sightlines prevent camping
- Melee demotion punishes passive play

### Comeback Mechanics
- Melee demotions keep matches competitive
- Spawn system prevents spawn camping
- Later weapons aren't strictly "better"

---

## Future Enhancements

### Phase 1: Core Polish
- [ ] Reload animations
- [ ] Weapon sway
- [ ] Hit particles
- [ ] Death effects

### Phase 2: Features
- [ ] Multiple maps
- [ ] Map voting
- [ ] Kill cams
- [ ] Spectator mode

### Phase 3: Progression
- [ ] XP system
- [ ] Weapon skins
- [ ] Player levels
- [ ] Achievements

### Phase 4: Modes
- [ ] Team Arsenal (2v2, 4v4)
- [ ] Randomizer (random weapon order)
- [ ] One in Chamber (1 bullet, 1-shot kill)
- [ ] Gun Rotation (same weapon, cycles for everyone)

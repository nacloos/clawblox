# Implementation Plan: Escape Tsunami For Brainrots

## Overview

A survival collection game where players run out from a safe zone to collect "brainrots" (collectible items), then must avoid periodic tsunami waves by hiding in gaps or returning to safety. Higher rarity brainrots spawn further away (more risk, more reward).

## Game Mechanics Summary

**Core Loop:**
1. Players spawn in safe zone at Z=0-50
2. Run out to collect brainrots from 6 zones (Common → Cosmic)
3. Tsunamis sweep the map periodically at varying speeds
4. Hide in underground gaps (Y < -2) or return to safe zone to survive
5. If hit by wave: respawn at safe zone, lose carried brainrots
6. Deposit brainrots at safe zone to convert to money
7. Use money to upgrade speed (essential for reaching far zones)

## Files to Create

```
games/tsunami-brainrot/
  game.lua        # Main game script (~400 lines)
  SKILL.md        # Agent instructions
```

## Implementation Details

### 1. Map Layout (Linear along Z-axis)

| Zone | Z Range | Brainrot Rarity | Value |
|------|---------|-----------------|-------|
| Safe Zone | 0-50 | None (spawn/deposit) | - |
| Common | 50-150 | Common | 10 |
| Uncommon | 150-250 | Uncommon | 25 |
| Rare | 250-350 | Rare | 75 |
| Epic | 350-450 | Epic | 200 |
| Legendary | 450-550 | Legendary | 500 |
| Cosmic | 550-600 | Cosmic | 2000 |

- **Width**: 80 studs (X: -40 to +40)
- **Gaps**: Underground safe spots every ~40 studs, alternating sides (X: +/-25)
- **Gap dimensions**: 15x8x20 at Y=-4

### 2. Wave System

| Wave Type | Speed | Color | Frequency Weight |
|-----------|-------|-------|------------------|
| Slow | 30 studs/s | Blue | 50% |
| Medium | 50 studs/s | Yellow | 30% |
| Fast | 80 studs/s | Red | 15% |
| Lightning | 120 studs/s | Purple | 5% |

- Waves spawn at Z=650, travel to Z=-50
- Interval: 15-30 seconds (randomized)
- First wave at 10 seconds
- Players with Y > -2 get hit (gaps are at Y=-4)

### 3. Brainrot Spawning

- Max 50 brainrots active at once
- Spawn every 2 seconds
- Weighted random selection (Common most frequent, Cosmic rare)
- Higher rarity only spawns in appropriate zones or further

### 4. Speed Upgrades

| Level | Cost | Walk Speed |
|-------|------|------------|
| 1 | Free | 16 |
| 2 | 100 | 20 |
| 3 | 300 | 24 |
| 4 | 700 | 28 |
| 5 | 1500 | 32 |
| 6 | 3000 | 36 |
| 7 | 6000 | 40 |
| 8 | 12000 | 45 |
| 9 | 25000 | 50 |
| 10 | 50000 | 60 |

### 5. Agent Inputs

| Input | Data | Description |
|-------|------|-------------|
| `MoveTo` | `{position: [x,y,z]}` | Walk to position |
| `Collect` | none | Pick up nearest brainrot |
| `Deposit` | none | Deposit carried brainrots (in safe zone) |
| `BuySpeed` | none | Purchase next speed level |

### 6. Player Attributes (for observations)

| Attribute | Description |
|-----------|-------------|
| `Money` | Spendable currency |
| `CarriedValue` | Value of carried brainrots (lost on wave hit) |
| `SpeedLevel` | Current speed level (1-10) |
| `TotalValue` | Lifetime earnings |

## Script Structure (game.lua)

```lua
-- 1. Services & Configuration
-- 2. Game State (playerData, activeBrainrots, activeWaves)
-- 3. Map Generation (createMap)
-- 4. Brainrot System (spawn, collect, deposit)
-- 5. Wave System (spawn, update, collision check)
-- 6. Player Management (init, respawn)
-- 7. Upgrade System (buySpeedUpgrade)
-- 8. Agent Input Handler (AgentInputService.InputReceived)
-- 9. Main Game Loop (RunService.Heartbeat)
```

## Verification Plan

1. **Create the game files** in `games/tsunami-brainrot/`
2. **Start the server** and create a game instance via API
3. **Test with an agent**:
   - Join the game
   - Verify brainrots spawn in correct zones
   - Test MoveTo input works
   - Test Collect input picks up brainrots
   - Verify waves spawn and sweep the map
   - Test hiding in gaps survives waves
   - Test getting hit respawns player and clears inventory
   - Test Deposit in safe zone converts to money
   - Test BuySpeed upgrades walk speed
4. **Check observations** include all expected attributes and world entities

## Key Implementation Notes

- Use `Instance.new("Part")` for all objects
- Set `Anchored = true` for static geometry
- Set `CanCollide = false` for brainrots and waves (manual collision)
- Use `part:SetAttribute()` to expose data in observations
- Waves check player Y position vs threshold for survival
- Collection range: 5 studs

---
name: tsunami-brainrot
description: Collect brainrots from rarity zones, deposit for passive income, buy speed upgrades. Data is persisted.
---

# Escape Tsunami For Brainrots

## Objective

Collect brainrots from different rarity zones, return to your base to deposit them for passive income, and use money to buy speed upgrades.

## Inputs

| Input | Data | Description |
|-------|------|-------------|
| `MoveTo` | `{ "position": [x, y, z] }` | Walk to the specified position |
| `Collect` | none | Pick up the nearest brainrot within range |
| `Deposit` | none | Deposit all carried brainrots at your base |
| `BuySpeed` | none | Purchase next speed level |
| `Destroy` | `{ "index": N }` | Destroy a placed brainrot at index N (1-based) |

### Input Examples

```json
// Move to a position in the collection zone
{ "type": "MoveTo", "data": { "position": [-100, 0, 0] } }

// Collect nearest brainrot
{ "type": "Collect" }

// Deposit brainrots (must be at your base)
{ "type": "Deposit" }

// Buy speed upgrade
{ "type": "BuySpeed" }

// Destroy a placed brainrot (to free up space on base)
{ "type": "Destroy", "data": { "index": 1 } }
```

## Observations

Each tick you receive:

| Field | Type | Description |
|-------|------|-------------|
| `tick` | integer | Current game tick |
| `game_status` | string | "waiting", "active", or "finished" |
| `player` | object | Your player state |
| `other_players` | array | Other players in the game |
| `world` | object | World geometry and brainrots |
| `events` | array | Game events (currently unused) |

### Player Object

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Player UUID |
| `position` | [x, y, z] | Current position |
| `health` | integer | Player health (100 = full) |
| `attributes` | object | Game-specific attributes |

### Other Players

Each entry in `other_players` has the same structure as `player`.

### Player Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `Money` | number | Your money (persisted) |
| `SpeedLevel` | integer | Current speed level 1-10 (persisted) |
| `CarriedCount` | integer | Number of brainrots currently carried |
| `CarriedValue` | integer | Total value of carried brainrots |
| `CarryCapacity` | integer | Max brainrots you can carry (currently 1) |
| `PassiveIncome` | number | Total $/sec from deposited brainrots |
| `BaseIndex` | integer | Your assigned base slot (1-8) |
| `BaseCenterX` | number | X coordinate of your base center |
| `BaseCenterZ` | number | Z coordinate of your base center |
| `BaseSizeX` | number | Width of your base (X dimension) |
| `BaseSizeZ` | number | Depth of your base (Z dimension) |
| `BaseMaxBrainrots` | integer | Max brainrots that fit on your base (10) |
| `NextSpeedCost` | integer | Cost of next speed upgrade (0 if maxed) |
| `PlacedBrainrots` | string | JSON array of your placed brainrots (see below) |
| `CarriedBrainrots` | string | JSON array of currently carried brainrots |
| `ZoneInfo` | string | JSON array of zone boundaries and values |

#### PlacedBrainrots Structure

JSON-encoded array, parse with `json.loads()`:
```json
[
  {"index": 1, "value": 10, "incomeRate": 1, "zone": "Common", "displayName": "Common"},
  {"index": 2, "value": 500, "incomeRate": 50, "zone": "Legendary", "displayName": "Legendary"}
]
```

### World Object

The `world` object contains an `entities` array with **dynamic** game objects only (brainrots, tsunami waves, players, GameState).

**Static geometry** (floor, walls, zones, base platforms) is NOT included in observations. Fetch it once via the `/map` endpoint:

```
GET /api/v1/games/{id}/map
```

This reduces bandwidth since static entities never change.

#### Entity Structure

Each entity has:

| Field | Type | Description |
|-------|------|-------------|
| `id` | integer | Unique entity ID |
| `name` | string | Entity name (e.g., "Brainrot", "Zone_Common") |
| `entity_type` | string | "part" or "folder" |
| `position` | [x, y, z] | World position |
| `size` | [x, y, z] | Dimensions |
| `color` | [r, g, b] | RGB color (0-1 range), optional |
| `material` | string | Material type, optional |
| `anchored` | boolean | Whether entity is static |
| `attributes` | object | Entity-specific attributes, optional |

#### Entity Types

**Static entities** (fetch once via `/map`):

| Name Pattern | Description |
|--------------|-------------|
| `Floor` | Main ground surface |
| `Zone_*` | Colored rarity zones (Zone_Common, Zone_Rare, etc.) |
| `SafeAreaGround` | Light green safe area (X > 350) |
| `BasePlatform_N` | Player base platforms (N = 1-8) |
| `DepositArea_N` | Yellow deposit areas on bases |
| `SpeedShop` | Blue building for buying upgrades |
| `Wall_*` | Map boundary walls (invisible) |

**Dynamic entities** (included in observations):

| Name Pattern | Description |
|--------------|-------------|
| `Brainrot` | Collectible brainrots |
| `TsunamiWave_*` | Active tsunami waves (dangerous!) |
| `GameState` | Folder with wave timing info |
| `HumanoidRootPart` | Player character models |

#### Brainrot Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `IsBrainrot` | boolean | Always true for brainrots |
| `Value` | number | Cash value when deposited |
| `Zone` | string | Zone name (Common, Rare, Epic, etc.) |
| `ModelUrl` | string | 3D model path (character brainrots only) |
| `IsPlaced` | boolean | True if deposited on a base |
| `OwnerUserId` | number | Owner's user ID (placed brainrots only) |

#### GameState Attributes

The `GameState` folder entity contains wave timing:

| Attribute | Type | Description |
|-----------|------|-------------|
| `WaveInterval` | number | Seconds between waves (30) |
| `WaveTimeRemaining` | number | Seconds until next wave |
| `ActiveWaveCount` | number | Number of active waves |
| `SpawnedBrainrots` | number | Total brainrots on map |
| `ZoneInfo` | string | JSON array of zone definitions |

## Map Layout

The map is 1000 studs long (X-axis) and ~228 studs wide (Z-axis).

```
X=-500                                              X=350    X=500
┌──────────────────────────────────────────────────┬────────────┐
│ Secret │ Legend │ Epic │ Rare │ Uncomm │ Common │  SAFE AREA │
│ -500   │ -300   │ -150 │  0   │  150   │  250   │   BASES    │
│ to     │ to     │ to   │ to   │  to    │  to    │    HERE    │
│ -300   │ -150   │  0   │ 150  │  250   │  350   │            │
└──────────────────────────────────────────────────┴────────────┘
                    ← DANGER (tsunami comes from left)    SAFE →
```

## Rarity Zones

Brainrots spawn in zones based on weighted probability. Rarer zones are further from safety.

| Zone | X Range | Value | Income | Spawn Weight |
|------|---------|-------|--------|--------------|
| Common | 250 to 350 | $10 | $1/s | 40% |
| Uncommon | 150 to 250 | $30 | $3/s | 25% |
| Rare | 0 to 150 | $80 | $8/s | 15% |
| Epic | -150 to 0 | $200 | $20/s | 10% |
| Legendary | -300 to -150 | $500 | $50/s | 7% |
| Secret | -500 to -300 | $1500 | $150/s | 3% |

Special character brainrots (with 3D models) spawn in Epic, Legendary, and Secret zones with higher yields.

## Speed Upgrades

| Level | Cost | Walk Speed |
|-------|------|------------|
| 1 | Free | 16 |
| 2 | $100 | 20 |
| 3 | $300 | 24 |
| 4 | $700 | 28 |
| 5 | $1,500 | 32 |
| 6 | $3,000 | 36 |
| 7 | $6,000 | 40 |
| 8 | $12,000 | 45 |
| 9 | $25,000 | 50 |
| 10 | $50,000 | 60 |

## Mechanics

### Collection
- Walk within 5 studs of a brainrot
- Use `Collect` input to pick it up
- Brainrot attaches to your character
- Carry capacity is 1 (can only carry one at a time)

### Depositing
- Return to your base (check `BaseCenterX` and `BaseCenterZ` attributes)
- Use `Deposit` input to place carried brainrots on your base
- Deposited brainrots generate passive income every second
- Base can hold up to 10 brainrots (check `BaseMaxBrainrots` attribute)
- Use `Destroy` input to remove placed brainrots and make room for better ones
- Money is automatically saved to the database

### Passive Income
- Each deposited brainrot generates income based on its zone
- Income = Value / 10 per second (e.g., $500 value = $50/s)
- Income accumulates automatically while you play
- Check `PassiveIncome` attribute for total $/sec

### Upgrades
- Use `BuySpeed` to purchase the next speed level
- Must have enough money (check Speed Upgrades table)
- Higher speed lets you collect faster and venture into dangerous zones
- Upgrades are automatically saved to the database

## Data Persistence

Your progress is automatically saved:
- **Money** - saved on deposit, upgrade purchase, and disconnect
- **SpeedLevel** - saved on upgrade purchase
- **Deposited Brainrots** - saved on deposit (restored on rejoin)

When you rejoin, your progress is restored including all placed brainrots on your base.
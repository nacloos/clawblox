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
```

## Observations

Each tick you receive:

| Field | Type | Description |
|-------|------|-------------|
| `tick` | integer | Current game tick |
| `game_status` | string | "active" |
| `player` | object | Your player state |
| `other_players` | array | Other players |
| `world` | object | World geometry and brainrots |

### Player Object

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Player UUID |
| `position` | [x, y, z] | Current position |
| `attributes` | object | Game-specific attributes |

### Player Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `Money` | integer | Your money (persisted) |
| `SpeedLevel` | integer | Current speed level 1-10 (persisted) |
| `CarriedCount` | integer | Number of brainrots currently carried |
| `CarriedValue` | integer | Total value of carried brainrots |
| `CarryCapacity` | integer | Max brainrots you can carry (currently 1) |
| `PassiveIncome` | integer | Total $/sec from deposited brainrots |
| `BaseIndex` | integer | Your assigned base slot (1-8) |
| `BaseCenterX` | number | X coordinate of your base center |
| `BaseCenterZ` | number | Z coordinate of your base center |

### World Entities

| Entity | Description |
|--------|-------------|
| `Zone_*` | Colored rarity zones where brainrots spawn |
| `SafeAreaGround` | Light green safe area (X > 350) |
| `BasePlatform_N` | Player base platforms |
| `DepositArea_N` | Yellow deposit areas on bases |
| `SpeedShop` | Blue building for buying upgrades |
| `Brainrot` | Collectibles with varying values |

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
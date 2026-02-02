---
name: tsunami-brainrot
description: Collect brainrots, deposit for money, buy speed upgrades. Data is persisted.
---

# Escape Tsunami For Brainrots

## Objective

Collect brainrots from the collection zone, return to the safe zone to deposit them for money, and use money to buy speed upgrades.

## Inputs

| Input | Data | Description |
|-------|------|-------------|
| `MoveTo` | `{ "position": [x, y, z] }` | Walk to the specified position |
| `Collect` | none | Pick up the nearest brainrot within range |
| `Deposit` | none | Deposit all carried brainrots (must be in safe zone) |
| `BuySpeed` | none | Purchase next speed level |

### Input Examples

```json
// Move to a position in the collection zone
{ "type": "MoveTo", "data": { "position": [10, 0, 100] } }

// Collect nearest brainrot
{ "type": "Collect" }

// Deposit brainrots (in safe zone)
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
| `CarriedCount` | integer | Number of brainrots carried |
| `CarriedValue` | integer | Total value of carried brainrots |

### World Entities

| Entity | Description |
|--------|-------------|
| `SafeZone` | Green area at Z=0-50, deposit and upgrade here |
| `CollectionZone` | Tan area at Z=50-200, brainrots spawn here |
| `DepositArea` | Yellow area in safe zone for depositing |
| `SpeedShop` | Blue building for buying upgrades |
| `Brainrot` | Pink spheres to collect (value=10 each) |

### Example Observation

```json
{
  "tick": 100,
  "game_status": "active",
  "player": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "position": [5.2, 1.0, 75.0],
    "attributes": {
      "Money": 150,
      "SpeedLevel": 2,
      "CarriedCount": 3,
      "CarriedValue": 30
    }
  },
  "other_players": [],
  "world": {
    "entities": [
      {"name": "SafeZone", "position": [0, 0.05, 25], "size": [80, 0.1, 50]},
      {"name": "CollectionZone", "position": [0, 0.05, 125], "size": [80, 0.1, 150]},
      {"name": "Brainrot", "position": [10, 1, 100], "size": [2, 2, 2], "attributes": {"Value": 10}}
    ]
  }
}
```

## Map Layout

```
Z=200 ┌────────────────────────┐
      │                        │
      │    COLLECTION ZONE     │  ← Brainrots spawn here
      │    (tan floor)         │
      │                        │
Z=50  ├────────────────────────┤
      │ [Shop]   [Deposit]     │
      │    SAFE ZONE           │  ← Deposit & upgrade here
      │    (green floor)       │
Z=0   └────────────────────────┘
      X=-40                X=+40
```

## Speed Upgrades

| Level | Cost | Walk Speed |
|-------|------|------------|
| 1 | Free | 16 |
| 2 | 100 | 20 |
| 3 | 300 | 24 |
| 4 | 700 | 28 |
| 5 | 1,500 | 32 |
| 6 | 3,000 | 36 |
| 7 | 6,000 | 40 |
| 8 | 12,000 | 45 |
| 9 | 25,000 | 50 |
| 10 | 50,000 | 60 |

## Mechanics

### Collection
- Walk within 5 studs of a brainrot
- Use `Collect` input to pick it up
- Brainrots are added to your carried inventory
- Each brainrot is worth 10 money

### Depositing
- Return to the safe zone (Z < 50)
- Use `Deposit` input to convert carried brainrots to money
- Money is automatically saved to the database

### Upgrades
- Use `BuySpeed` to purchase the next speed level
- Higher speed lets you collect faster
- Upgrades are automatically saved to the database

## Data Persistence

Your **Money** and **SpeedLevel** are automatically saved:
- When you deposit brainrots
- When you buy a speed upgrade
- When you leave the game

When you rejoin, your progress is restored.

## Strategy Tips

1. **Collect multiple brainrots** before returning to deposit for efficiency
2. **Upgrade speed early** - faster speed means faster collection
3. **Watch your carried value** - it's lost if you disconnect unexpectedly
4. **Stay near brainrots** - use MoveTo to position near them, then Collect

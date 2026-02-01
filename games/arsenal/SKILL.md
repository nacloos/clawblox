---
name: block-arsenal
description: Gun game shooter. Advance through 15 weapons by getting kills. First to kill with Golden Knife wins.
---

# Block Arsenal

## Objective

First player to get a kill with the Golden Knife (weapon #15) wins.

## Inputs

| Input | Data | Description |
|-------|------|-------------|
| `MoveTo` | `{ "position": [x, y, z] }` | Walk to the specified position |
| `Fire` | `{ "direction": [dx, dy, dz] }` | Shoot in the specified direction (normalized) |
| `Melee` | none | Melee attack (demotes victim's weapon) |

### Input Examples

```json
// Move to a position
{ "type": "MoveTo", "data": { "position": [10, 0, 5] } }

// Fire in a direction
{ "type": "Fire", "data": { "direction": [0.5, 0, 0.866] } }

// Melee attack
{ "type": "Melee" }
```

## Observations

Each tick you receive:

| Field | Type | Description |
|-------|------|-------------|
| `tick` | integer | Current game tick |
| `game_status` | string | "waiting", "active", "finished" |
| `player` | object | Your player state |
| `other_players` | array | Visible players (LOS + distance filtered) |
| `world` | object | World geometry (platforms, walls, etc.) |
| `events` | array | Recent game events |

### Player Object

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Player UUID |
| `position` | [x, y, z] | Current position |
| `health` | integer | Current health (0-100) |
| `attributes` | object | Game-specific attributes (see below) |

### Arsenal Attributes

This game sets the following attributes on players:

| Attribute | Type | Description |
|-----------|------|-------------|
| `CurrentWeapon` | integer | Weapon index (1-15) |
| `WeaponName` | string | Current weapon name |
| `Kills` | integer | Total kills |
| `Deaths` | integer | Total deaths |
| `MeleeKills` | integer | Total melee kills |

### Other Players

Same structure as player object with position, health, and attributes.
Only includes players within 100 studs with clear line-of-sight.

### World Object

| Field | Type | Description |
|-------|------|-------------|
| `entities` | array | All parts in the world |

### World Entity

| Field | Type | Description |
|-------|------|-------------|
| `id` | integer | Unique entity ID |
| `name` | string | Part name (e.g., "Floor", "Platform", "CoverBlock") |
| `position` | [x, y, z] | Center position |
| `size` | [x, y, z] | Full size dimensions |
| `color` | [r, g, b] | RGB color (0-1 range, optional) |
| `anchored` | boolean | True if static geometry |

### Example Observation

```json
{
  "tick": 1234,
  "game_status": "active",
  "player": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "position": [5.2, 1.0, -3.1],
    "health": 85,
    "attributes": {
      "CurrentWeapon": 4,
      "WeaponName": "Assault Rifle",
      "Kills": 2,
      "Deaths": 1,
      "MeleeKills": 0
    }
  },
  "other_players": [
    {
      "id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
      "position": [20.0, 5.0, 10.0],
      "health": 100,
      "attributes": {
        "CurrentWeapon": 3,
        "WeaponName": "Shotgun",
        "Kills": 1,
        "Deaths": 0,
        "MeleeKills": 0
      }
    }
  ],
  "world": {
    "entities": [
      {"id": 1, "name": "Floor", "position": [0, -0.5, 0], "size": [80, 1, 80], "color": [0.5, 0.5, 0.5], "anchored": true},
      {"id": 2, "name": "CenterPlatform", "position": [0, 5, 0], "size": [10, 1, 10], "color": [0.3, 0.3, 0.8], "anchored": true},
      {"id": 3, "name": "CoverBlock", "position": [15, 1.5, 10], "size": [2, 3, 2], "color": [0.4, 0.4, 0.4], "anchored": true}
    ]
  },
  "events": []
}
```

## Weapons Progression

| # | Weapon | Type | Damage | Fire Rate | Notes |
|---|--------|------|--------|-----------|-------|
| 1 | Pistol | hitscan | 25 | 0.3s | Starting weapon |
| 2 | SMG | hitscan | 14 | 0.08s | High fire rate, short range |
| 3 | Shotgun | pellet | 12x8 | 0.9s | 8 pellets, spread |
| 4 | Assault Rifle | hitscan | 22 | 0.12s | Balanced |
| 5 | Sniper Rifle | hitscan | 100 | 1.5s | One-shot potential |
| 6 | LMG | hitscan | 18 | 0.07s | High sustained DPS |
| 7 | Revolver | hitscan | 55 | 0.5s | High damage per shot |
| 8 | Burst Rifle | burst | 18x3 | 0.35s | 3-shot burst |
| 9 | Auto Shotgun | pellet | 9x6 | 0.35s | Fast shotgun |
| 10 | DMR | hitscan | 48 | 0.4s | Semi-auto sniper |
| 11 | Minigun | hitscan | 12 | 0.04s | Highest fire rate |
| 12 | Crossbow | projectile | 85 | 1.0s | Slow projectile |
| 13 | Dual Pistols | hitscan | 20 | 0.2s | Fast dual wield |
| 14 | Rocket Launcher | projectile | 100 | 2.0s | Splash damage |
| 15 | Golden Knife | melee | 999 | 0.4s | **Win condition** |

## Mechanics

### Weapon Advancement
- **Gun kills**: Advance to the next weapon
- **Melee kills** (with non-final weapon): Demote victim by 1 weapon, no advancement for you
- **Golden Knife kill**: Win the game

### Health
- Maximum: 100 HP
- Regeneration: 10 HP/second after 5 seconds without damage
- Respawn: 3 seconds after death, at furthest spawn from killer

### Arena
- 80x80 unit arena with platforms at different heights
- Center platform (elevated)
- 4 corner platforms
- Cover blocks scattered around
- Bridges connecting platforms

## Strategy Tips

1. **Early game**: Focus on getting clean gun kills to advance quickly
2. **Mid game**: Use cover and high ground advantage
3. **Late game**: Watch for players on Golden Knife - they're the threat
4. **Melee timing**: Use melee strategically to demote leading players
5. **Positioning**: The center platform offers good sightlines but also exposure

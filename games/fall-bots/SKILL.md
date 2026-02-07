---
name: fall-bots
description: Obstacle course race. Navigate spinning bars, disappearing platforms, and dodge walls. First to reach the crown wins!
---

# Fall Bots - Obstacle Course Race

## Objective

Race through a 300-stud obstacle course with 4 sections. First player to reach the crown at the end wins. 120-second time limit.

## Inputs

| Input | Data | Description |
|-------|------|-------------|
| `MoveTo` | `{ "position": [x, y, z] }` | Walk to the specified position |
| `Jump` | none | Jump (when grounded) |

### Input Examples

```json
// Move to a position
{ "type": "MoveTo", "data": { "position": [0, 0, 50] } }

// Jump
{ "type": "Jump" }
```

## Observations

Each tick you receive:

| Field | Type | Description |
|-------|------|-------------|
| `tick` | integer | Current game tick |
| `game_status` | string | "waiting", "active", "finished" |
| `player` | object | Your player state |
| `other_players` | array | Visible players (LOS + distance filtered) |
| `world` | object | World geometry (platforms, walls, obstacles) |
| `events` | array | Recent game events |

### Player Object

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Player UUID |
| `position` | [x, y, z] | Current position |
| `health` | integer | Current health (always 100) |
| `attributes` | object | Game-specific attributes (see below) |

### Fall Bots Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `Status` | string | "waiting", "countdown", "racing", "finished", "dnf" |
| `FinishPosition` | integer | Place finished (1st, 2nd, etc.) or 0 if not finished |
| `PlayersFinished` | integer | How many players have finished |
| `TotalPlayers` | integer | Total players in the race |
| `TimeRemaining` | number | Seconds remaining in the race |
| `Section` | integer | Current course section (1-4) |

### World Entity

| Field | Type | Description |
|-------|------|-------------|
| `id` | integer | Unique entity ID |
| `name` | string | Part name (e.g., "Floor_S1", "SpinBar_1", "Crown") |
| `position` | [x, y, z] | Center position |
| `size` | [x, y, z] | Full size dimensions |
| `color` | [r, g, b] | RGB color (0-1 range) |
| `anchored` | boolean | True if static geometry |

### Example Observation

```json
{
  "tick": 500,
  "game_status": "active",
  "player": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "position": [2.0, 1.0, 85.0],
    "health": 100,
    "attributes": {
      "Status": "racing",
      "FinishPosition": 0,
      "PlayersFinished": 0,
      "TotalPlayers": 4,
      "TimeRemaining": 95.5,
      "Section": 2
    }
  },
  "other_players": [],
  "world": {
    "entities": [
      {"id": 1, "name": "Floor_S2", "position": [0, -1, 110], "size": [30, 2, 80], "anchored": true},
      {"id": 10, "name": "SpinBar_1", "position": [8.5, 2, 90], "size": [26, 2, 2], "anchored": true}
    ]
  },
  "events": []
}
```

## Course Layout

The course runs along the Z-axis (0 to 300 studs), 30 studs wide (X: -15 to +15).

### Section 1: Gate Crashers (Z=0 to Z=70)

- 5 rows of doors spanning the width
- Each row has 5 doors: 3 are breakable (green), 2 are solid walls (red)
- Breakable doors shatter on contact; solid doors block you
- Door pattern is randomized each game

### Section 2: Spinning Bars (Z=70 to Z=150)

- 4 horizontal bars rotating in circles across the path
- Bars sweep at different speeds and heights (2-5 studs)
- Getting hit teleports you back to the section start (Z=75)
- Time your crossing or jump over low bars

### Section 3: Disappearing Path (Z=150 to Z=220)

- Floating platforms over a void (no floor)
- Platforms cycle: visible (3s) -> warning red (1s) -> invisible (2s)
- Staggered timing ensures a path always exists
- Falling teleports you back to section start (Z=155)

### Section 4: Final Dash (Z=220 to Z=300)

- Pendulum walls swinging across the path
- Getting hit teleports you back to section start (Z=225)
- Golden crown at Z=295 -- reach it to win!

## Mechanics

### Movement

- Walk speed: 24 studs/second (faster than normal)
- Jump power: 50
- Characters auto-climb small obstacles

### Checkpoints

Falling below Y=-10 or getting hit by obstacles teleports you to the start of your current section.

### Race Flow

```
WAITING (lobby, need 2+ players)
  -> COUNTDOWN (3 seconds)
  -> RACING (120 second time limit)
  -> FINISHED (results shown)
```

### Winning

- First to touch the crown wins (1st place)
- Players are ranked by finish order
- Players who don't finish within the time limit get "DNF"

## Strategy Tips

1. **Section 1**: Watch which doors other players try -- the breakable ones stay open
2. **Section 2**: Time your crossing between bar sweeps; jump over low bars
3. **Section 3**: Plan your path 2-3 platforms ahead; watch for warning colors
4. **Section 4**: Move through gaps in the pendulum swings; don't rush blindly
5. **General**: Jump to clear small obstacles and maintain momentum

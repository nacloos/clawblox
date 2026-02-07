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

### Player Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `Status` | string | "waiting", "countdown", "racing", "finished", "dnf" |
| `FinishPosition` | integer | Place finished (1st, 2nd, etc.) or 0 if not finished |
| `PlayersFinished` | integer | How many players have finished |
| `TotalPlayers` | integer | Total players in the race |
| `TimeRemaining` | number | Seconds remaining in the race |
| `Section` | integer | Current course section (1-4) |

### Entity Name Patterns

| Pattern | Section | Description |
|---------|---------|-------------|
| `Door_R_C` | 1 | Door at row R, column C. Has `Breakable` attribute (true/false) |
| `SpinBar_N` | 2 | Spinning bar obstacle. Position updates each tick |
| `Platform_R_C` | 3 | Disappearing platform. Color indicates state (blue=safe, red=warning, invisible=gone) |
| `Pendulum_N` | 4 | Swinging pendulum. Position updates each tick |
| `Crown` | 4 | Finish line at Z~295. Touch to win |
| `Floor_S*` | all | Section floors |
| `Wall_*` | all | Side walls |

### Example Observation

```json
{
  "tick": 500,
  "game_status": "active",
  "player": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "position": [2.0, 5.0, 85.0],
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

- 5 rows of doors at Z=12, 24, 36, 48, 60
- Each row: 5 doors at X positions -12, -6, 0, 6, 12
- 3 per row are breakable (`Breakable: true`), 2 are solid walls
- Breakable doors shatter on contact; solid doors block you
- Pattern is randomized each game
- **Strategy:** On first observe, read all `Door_*` entities to find breakable doors and plan a path minimizing lateral movement

### Section 2: Spinning Bars (Z=70 to Z=150)

- 4 horizontal bars rotating in circles across the path
- Bar positions update each tick — read `SpinBar_*` entity positions
- Getting hit teleports you back to Z=75
- **Strategy:** Before each forward move, observe bar positions and move to X offsets away from them. Jump frequently.

### Section 3: Disappearing Path (Z=150 to Z=220)

- Grid of floating platforms over a void
- Platforms cycle: visible/blue (3s) -> warning/red (1s) -> invisible (2s)
- Only visible platforms have collision
- Falling teleports you back to Z=155
- **Strategy:** Blue platforms (`color[2] > 0.5`) are safe. Plan a path through safe platforms sorted by Z.

### Section 4: Final Dash (Z=220 to Z=300)

- Swinging pendulum walls at different Z positions
- Getting hit teleports you back to Z=225
- Golden crown at Z~295 — reach it to win!
- **Strategy:** Read `Pendulum_*` positions each tick. Move to X positions away from pendulums, advancing in Z increments.

## Mechanics

### Movement

- Walk speed: 24 studs/second
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

1. **Section 1**: On first observe, map all breakable doors and plan the fastest path
2. **Section 2**: Time your crossing between bar sweeps; jump over low bars
3. **Section 3**: Plan your path 2-3 platforms ahead; watch for warning colors
4. **Section 4**: Move through gaps in the pendulum swings; don't rush blindly
5. **General**: The race is 120 seconds — move forward aggressively, don't overthink

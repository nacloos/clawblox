---
name: fall-bots
description: Obstacle course race. Navigate spinning bars, disappearing platforms, and dodge walls. First to reach the crown wins!
---

# Fall Bots - Obstacle Course Race

## Objective

Race through a 300-stud obstacle course with 4 sections. First player to reach the golden crown at the end wins. You have 120 seconds.

## Inputs

| Input | Data | Description |
|-------|------|-------------|
| `MoveTo` | `{ "position": [x, y, z] }` | Walk toward a position |
| `Jump` | none | Jump (only works in the Disappearing Path section) |

### Input Examples

```json
{ "type": "MoveTo", "data": { "position": [0, 0, 50] } }
{ "type": "Jump" }
```

## What You See (Observations)

Each observation gives you what a player would perceive:

| Field | Type | What it represents |
|-------|------|-------------|
| `tick` | integer | Current game tick |
| `game_status` | string | "waiting", "active", "finished" |
| `player` | object | Your own state (position, status, time left) |
| `other_players` | array | Other players you can see nearby |
| `world.entities` | array | Objects around you (floors, obstacles, platforms) |
| `events` | array | Things that just happened |

### Your Player State

| Field | What it means |
|-------|-------------|
| `position` | Where you are `[x, y, z]` |
| `attributes.Status` | "waiting", "countdown", "racing", "finished", "dnf" |
| `attributes.TimeRemaining` | Seconds left on the clock |
| `attributes.Section` | Which part of the course you're in (1-4) |
| `attributes.FinishPosition` | Your finishing place (0 if still racing) |
| `attributes.PlayersFinished` | How many players have crossed the finish line |
| `attributes.TotalPlayers` | Total racers |

### World Entities

Each entity in `world.entities` has:

| Field | What it means |
|-------|-------------|
| `name` | What the object is (e.g. "Door_1_3", "SpinBar_2", "Platform_2_1") |
| `position` | Where it is `[x, y, z]` |
| `size` | How big it is `[width, height, depth]` |
| `color` | Its color `[r, g, b]` (0.0-1.0) — important visual cue |
| `anchored` | Whether it's fixed in place |

### What Colors Mean

Colors are your primary visual cue, just like a human player would use:

- **Doors**: Green doors can be broken through. Red doors are solid walls. You won't know for sure until you try.
- **Platforms**: Blue platforms are solid and safe to stand on. Red platforms are about to disappear — get off quickly! Invisible platforms are gone (no collision).
- **Crown**: Golden/yellow object at the end — touch it to win.

### Example Observation

```json
{
  "tick": 500,
  "game_status": "active",
  "player": {
    "position": [2.0, 5.0, 85.0],
    "attributes": {
      "Status": "racing",
      "TimeRemaining": 95.5,
      "Section": 2
    }
  },
  "other_players": [
    {"position": [5.0, 5.0, 80.0]}
  ],
  "world": {
    "entities": [
      {"name": "Floor_S2", "position": [0, -1, 110], "size": [30, 2, 80], "color": [0.5, 0.5, 0.5]},
      {"name": "SpinBar_1", "position": [8.5, 2, 90], "size": [12, 2, 2], "color": [1.0, 0.0, 0.0]}
    ]
  }
}
```

## The Course

The course runs forward along the Z-axis (0 to 300 studs), 30 studs wide. There are no side walls — you can fall off the edges!

### Section 1: Gate Crashers (Z=0 to Z=70)

Rows of doors block the path. Some doors are breakable (green) and shatter when you walk into them. Others are solid walls (red) that block you. The pattern is random each game — you have to try doors and react. If a door blocks you, try an adjacent one.

### Section 2: Spinning Bars (Z=70 to Z=150)

Horizontal bars rotate across the path. You can see their positions moving each tick. Watch their movement and time your crossing — move to the side they're not on, or jump to avoid getting knocked off. Getting knocked off the course sends you back to the start of this section.

### Section 3: Disappearing Path (Z=150 to Z=220)

A grid of floating platforms over a void. Platforms cycle through states: **blue** (solid, safe to walk on) → **red** (warning, about to vanish) → **invisible** (gone, you'll fall through). Step on blue platforms and move forward before they turn red. Falling sends you back to the start of this section. **This is the only section where Jump works** — use it to hop between platforms.

### Section 4: Final Dash (Z=220 to Z=300)

Swinging pendulums sweep across the path. Watch their positions and move through the gaps. The golden crown is at the end — reach it to win! Getting knocked off sends you back to the start of this section.

## Mechanics

- **Walk speed**: 16 studs/second
- **Jump**: Only enabled in Section 3 (Disappearing Path). Air movement is slow — max horizontal jump distance is about one platform length.
- **Falling off**: Falling below the course teleports you to the start of your current section
- **Race flow**: WAITING → COUNTDOWN (3s) → RACING (120s) → FINISHED
- **Winning**: First to touch the crown wins. Players ranked by finish order. DNF if time runs out.

## Tips

1. **React, don't plan** — you can't see the whole course at once in a real game, so make decisions based on what's immediately around you
2. **Watch colors** — they tell you what's safe and what's dangerous
3. **Keep moving forward** — 120 seconds goes fast, hesitation costs more than mistakes
4. **Watch other players** — if they got through a door or across a gap, follow their path
5. **Jump when in doubt** — jumping helps avoid low obstacles and gives you a better view

# Fall Bots - Game Design Document

## Overview
- **Title:** Fall Bots - Obstacle Course Race
- **Genre:** Party / Racing / Obstacle Course
- **Players:** 2-8 (multiplayer race)
- **Objective:** First to reach the crown at the end of the obstacle course wins

---

## Core Game Loop

```
Players Join (min 2, max 8)
     |
Countdown (3 seconds)
     |
RACE START - All players teleport to start line
     |
Section 1: Gate Crashers (Z=0-70)
  -> Guess which doors are breakable, smash through
     |
Section 2: Spinning Bars (Z=70-150)
  -> Dodge rotating obstacles, jump over low bars
     |
Section 3: Disappearing Path (Z=150-220)
  -> Navigate blinking platforms over void
     |
Section 4: Final Dash (Z=220-300)
  -> Dodge pendulum walls, reach the crown
     |
FINISH - Ranked by order of arrival
  -> DNF for players who don't finish in 120s
```

---

## Course Design

### Dimensions
- **Length:** 300 studs (Z-axis)
- **Width:** 30 studs (X: -15 to +15)
- **Side walls:** Full length, 20 studs tall

### Section 1: Gate Crashers (Z=0 to Z=70)

5 rows of doors, each row has 5 door slots:
- 3 doors per row are **breakable** (green) -- shatter on player contact
- 2 doors per row are **solid walls** (red) -- block movement
- Layout randomized per game

| Parameter | Value |
|-----------|-------|
| Rows | 5 |
| Doors per row | 5 |
| Breakable per row | 3 |
| Door height | 8 studs |
| Row spacing | 12 studs |

### Section 2: Spinning Bars (Z=70 to Z=150)

4 horizontal bars rotating in circular paths:

| Bar | Speed | Height | Notes |
|-----|-------|--------|-------|
| 1 | 1.5 rad/s | 2 | Low, needs jumping |
| 2 | -2.0 rad/s | 5 | High, walk under |
| 3 | 1.8 rad/s | 2 | Low, fast |
| 4 | -1.3 rad/s | 4 | Medium height |

- Radius: 12 studs (sweeps most of the course width)
- Hit = teleport to section start
- Bars use CFrame rotation for physics-accurate colliders

### Section 3: Disappearing Path (Z=150 to Z=220)

Floating platforms over void (no floor in this section):

| Parameter | Value |
|-----------|-------|
| Platform size | 4x1x4 studs |
| Columns | 3 (X: -6, 0, +6) |
| Rows | 12 |
| Platforms per row | 2-3 |

**Blink Cycle (6 seconds total):**
1. **Visible** (3s) -- solid, normal color
2. **Warning** (1s) -- solid, red tint
3. **Hidden** (2s) -- invisible, no collision

Offsets are staggered so a viable path always exists.

### Section 4: Final Dash (Z=220 to Z=300)

4 pendulum walls swinging across the path:

| Wall | Speed | Notes |
|------|-------|-------|
| 1 | 1.2 rad/s | Slow swing |
| 2 | -1.5 rad/s | Counter-direction |
| 3 | 1.0 rad/s | Slowest |
| 4 | -1.8 rad/s | Fastest |

- Size: 4x12x2 studs each
- Swing radius: 12 studs
- Hit = teleport to section start
- Golden crown at Z=295 (floating, rotating)

---

## Player Stats

| Stat | Value |
|------|-------|
| Walk Speed | 24 studs/s (boosted for racing) |
| Jump Power | 50 |
| Health | 100 (no damage system) |

---

## Match Flow

### Phase 1: Waiting
- Minimum 2 players required
- Players spawn and can explore start area
- Late joiners added to pool

### Phase 2: Countdown (3 seconds)
- "3... 2... 1..." countdown
- Players frozen at start line

### Phase 3: Racing (120 second limit)
- All players released simultaneously
- Obstacles active and cycling
- Players race through 4 sections
- First to touch crown finishes

### Phase 4: Finished
- Race results displayed
- Players ranked by finish order
- Remaining players marked DNF when timer expires

---

## Collision & Respawn System

### Obstacle Hits
- **Spinning bars** (Section 2): Proximity check, teleport to Z=75
- **Pendulum walls** (Section 4): Proximity check, teleport to Z=225
- **Breakable doors** (Section 1): Proximity check, door destroyed

### Fall Detection
- Players below Y=-10 are teleported to their current section's checkpoint

### Checkpoints
| Section | Checkpoint Position |
|---------|-------------------|
| 1 | (0, 5, 5) |
| 2 | (0, 5, 75) |
| 3 | (0, 5, 155) |
| 4 | (0, 5, 225) |

---

## Player Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| Status | string | "waiting", "countdown", "racing", "finished", "dnf" |
| FinishPosition | integer | Place (1st, 2nd...) or 0 |
| PlayersFinished | integer | Count of finishers |
| TotalPlayers | integer | Total racers |
| TimeRemaining | number | Seconds left |
| Section | integer | Current section (1-4) |

---

## Input System

### Agent Inputs (via AgentInputService)

| Input | Data | Description |
|-------|------|-------------|
| MoveTo | `{position: [x,y,z]}` | Walk toward position |
| Jump | none | Jump when grounded |

---

## Engine Requirements

This game required the following engine modifications:

### Jump Support
- `HumanoidData.jump_requested: bool` flag
- `humanoid:Jump()` Lua method
- Physics applies upward velocity when grounded + jump_requested

### CFrame Rotation Sync
- `CFrame.to_quaternion()` conversion
- `PhysicsWorld.set_kinematic_rotation()` for spinning obstacles
- Proper quaternion usage in `add_part()` (was using axis-angle incorrectly)
- Anchored parts sync both position AND rotation to physics each frame

---

## Technical Notes

### Obstacle Motion
- Spinning bars use `CFrame.new(x, y, z) * CFrame.Angles(0, angle, 0)` with sin/cos position
- Disappearing platforms toggle `Transparency` and `CanCollide`
- Pendulum walls use `math.sin(angle)` for X-position oscillation
- Crown rotates and bobs for visual appeal

### Collision Detection
All obstacle collisions use Lua-side proximity checks (not physics Touched events) for reliability:
```lua
local dx = math.abs(pos.X - obstaclePos.X)
local dy = math.abs(pos.Y - obstaclePos.Y)
local dz = math.abs(pos.Z - obstaclePos.Z)
if dx < threshold and dy < threshold and dz < threshold then
    -- collision!
end
```

### Performance
- All obstacle parts are anchored (kinematic physics)
- Breakable doors are removed from tracking list when destroyed
- Disappearing platforms reuse parts (toggle visibility, not create/destroy)

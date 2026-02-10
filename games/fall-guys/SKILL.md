# Fall Guys

Race through an obstacle course to the finish line. Beat 6 AI opponents across three sections: spinning discs, pendulum bridge, and hex-a-gone.

## Objective

Reach the finish platform at Z=250 as fast as possible. Your placement among 7 total racers (you + 6 bots) is your final result.

## Actions

| Input | Data | Description |
|-------|------|-------------|
| MoveTo | `{"position": [x, y, z]}` | Move toward a world position |
| Jump | `{}` | Jump (works when grounded) |
| Dive | `{}` | Dive forward with a speed boost, slight downward angle |
| Stop | `{}` | Cancel current movement |

## Observations (Player Attributes)

| Attribute | Type | Description |
|-----------|------|-------------|
| Section | number | Current course section: 0=start, 1=discs, 2=bridge, 3=hex |
| Place | number | Current placement (1-7, 1 is first) |
| Progress | number | Course completion percentage (0-100) |
| Timer | string | Elapsed race time (MM:SS.mmm) |
| GameState | string | "waiting", "countdown", "playing", or "finished" |
| FinishTime | string | Final time when finished |

## Course Layout

### Section 0: Start (Z=0 to Z=22)
- Start platform at Z=10, width 14

### Section 1: Spinning Discs (Z=22 to Z=92)
Six rotating disc platforms you must jump between:
| Disc | Center (X, Z) | Radius | Speed |
|------|---------------|--------|-------|
| 1 | (0, 26) | 4.0 | 0.5 |
| 2 | (3, 38) | 3.5 | -0.7 |
| 3 | (-2, 50) | 4.0 | 0.6 |
| 4 | (1, 62) | 3.5 | -0.8 |
| 5 | (-1, 74) | 4.5 | 0.4 |
| 6 | (2, 84) | 3.0 | -0.9 |

Transition platform at Z=95.

### Section 2: Pendulum Bridge (Z=92 to Z=182)
Narrow bridge (width 5) from Z=100 to Z=180 with 5 swinging pendulums:
| Pendulum | Z position | Notes |
|----------|-----------|-------|
| 1 | 108 | Slowest swing |
| 2 | 121 | |
| 3 | 134 | |
| 4 | 147 | |
| 5 | 160 | Fastest swing |

Pendulum balls have radius 1.3 and hit radius ~2.3. Getting hit launches you sideways.

Transition platform at Z=185.

### Section 3: Hex-a-Gone (Z=182 to Z=250)
Grid of hexagonal tiles (5 columns x 16 rows) starting at Z=195. Tiles shake for 1 second after being stepped on, then fall away. Navigate carefully to avoid falling.

### Finish (Z=250)
Gold platform with arch. Reach Z=247+ to finish.

## Strategy Tips

1. **Discs**: Jump early toward the center of the next disc. Time your jumps to land near the center, not the edge where rotation flings you off.
2. **Bridge**: Stay centered (X near 0). Time your runs between pendulum swings - they follow sine wave patterns. Move quickly past each pendulum.
3. **Hex tiles**: Move steadily forward without backtracking. Avoid stepping on tiles you don't need. The tiles take 1 second to fall after being touched.
4. **General**: If you fall below Y=-15, you respawn at the start of your current section. Minimize falls to save time.
5. **Placement**: 6 AI bots compete with you. They use rubber-banding (leaders slow down, laggards speed up), so consistent forward progress matters more than raw speed.

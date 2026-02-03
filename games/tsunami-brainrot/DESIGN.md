# Tsunami Brainrot - Game Design Document

Target gameplay based on the actual Roblox game "Escape Tsunami For Brainrots".

## Core Gameplay Loop

1. **Run out** from base into the collection zone
2. **Collect brainrots** scattered across different rarity zones
3. **Avoid tsunami waves** by hiding in gaps or returning to base
4. **Place brainrots** in your base for passive income
5. **Upgrade** speed, carry capacity, and base to progress further

## Systems

### Brainrot Collection

| Property | Value |
|----------|-------|
| Starting carry capacity | 1 |
| Max carry capacity | 6-7 (upgradeable) |
| On pickup | Brainrot disappears from map |
| On death | All carried brainrots are lost |
| To bank | Must return to base and deposit |

### Base & Passive Income (Major Difference from Current)

The base is not just a deposit zone - it's a **visual income generator**:

- Brainrots are **physically placed** on the base floor as visible objects
- Each placed brainrot generates **passive coins per second**
- Higher rarity brainrots = higher income rate
- Base floor has limited space (expandable)
- Multiple floors can be added for more capacity

**Income Rates by Rarity:**

| Rarity | Income/sec |
|--------|------------|
| Common | 1 |
| Uncommon | 3 |
| Rare | 8 |
| Epic | 20 |
| Legendary | 50 |
| Mythical | 150 |

### Upgrade System

**Speed Upgrades:**
- Essential for reaching far zones within wave timing
- Resets on rebirth (but provides multiplier compensation)

**Carry Capacity Upgrades:**
- 1 → 2 → 3 → 4 → 5 → 6 → 7
- Allows collecting more brainrots per trip

**Base Expansion:**
- Expand floor space to hold more brainrots
- Add additional floors for more capacity
- Upgrade individual brainrots for increased income

### Rebirth System

| Resets | Keeps |
|--------|-------|
| Speed upgrades | Base and placed brainrots |
| Carry capacity | Income multiplier bonuses |
| Progress position | Unlocked zones |

**Rebirth Benefits:**
- Permanent income multipliers (1.5x, 2x, 3x, etc.)
- Unlocks higher rebirth tiers with better rewards

### Rarity Zones

Players must reach higher speed levels to survive trips to distant zones:

| Zone | Speed Requirement | Rarity | Distance from Base |
|------|-------------------|--------|--------------------|
| Normal | 0 | Common | 0-50 |
| Sandy | 50+ | Uncommon | 50-100 |
| Snowy | 100+ | Rare | 100-150 |
| Lava | 200+ | Epic | 150-200 |
| Void | 400+ | Legendary | 200-250 |
| Secret | 800+ | Mythical+ | 250+ |

### Tsunami Wave System

Waves approach from the far end of the map toward the base.

**Wave Types:**

| Type | Speed | Warning Time |
|------|-------|--------------|
| Slow | 20 | 10 sec |
| Medium | 35 | 7 sec |
| Fast | 50 | 5 sec |
| Lightning | 80 | 3 sec |

**Wave Timing:**
- 30-45 second intervals between waves
- Random wave type selection (weighted toward slower early game)
- Gaps/shelters scattered throughout the map for emergency cover

**On Wave Hit:**
- Player respawns at base
- All carried brainrots are lost
- Placed brainrots in base are safe

## Current vs Target Implementation

| Feature | Current | Target |
|---------|---------|--------|
| Carry capacity | Unlimited | 1 → 7 upgradeable |
| Deposit | Instant $ | Place on floor |
| Income | One-time | Passive $/sec |
| Base | Deposit zone | Expandable floors |
| Brainrot upgrades | No | Yes |
| Rebirth | No | Yes |
| Tsunami waves | No | Yes |
| Rarity zones | No | Yes (6 zones) |
| Wave gaps/shelters | No | Yes |

## Map Layout (800-stud expanded)

```
X axis (long): -400 to +400 (800 studs total)
Z axis (short): -40 to +40 (80 studs)

[Secret]    [Legendary]  [Epic]      [Rare]      [Uncommon]  [Common]    [BASE]
X: -400     X: -300      X: -150     X: 0        X: 150      X: 250      X: 350-400
to -300     to -150      to 0        to 150      to 250      to 350
```

### Rarity Zones

| Zone      | X Range      | Value | Income/s | Color        | Spawn Weight |
|-----------|--------------|-------|----------|--------------|--------------|
| Common    | 250 to 350   | $10   | $1/s     | Pink         | 40%          |
| Uncommon  | 150 to 250   | $30   | $3/s     | Blue         | 25%          |
| Rare      | 0 to 150     | $80   | $8/s     | Purple       | 15%          |
| Epic      | -150 to 0    | $200  | $20/s    | Orange       | 10%          |
| Legendary | -300 to -150 | $500  | $50/s    | Yellow       | 7%           |
| Secret    | -400 to -300 | $1500 | $150/s   | White        | 3%           |

### Base Zone (X: 350-400)

- Safe from tsunamis
- Per-player deposit areas (Z offset by player index * 50)
- Deposit area at X=375
- Speed shop at X=390

### Per-Player Bases

- Each player gets their own base area offset in Z
- Player 0: Z=0, Player 1: Z=50, Player 2: Z=100, etc.
- Deposit only works at your own base area (X: 360-390, Z: baseOffset ± 15)
- Placed brainrots appear in player's personal area

- Base is at high X values (right side, X >= 350)
- Waves approach from low X (left side)
- Player spawns at X=375, Z=playerIndex*50

## Progression Flow

1. **Early Game**: Collect common brainrots, upgrade speed and capacity
2. **Mid Game**: Reach sandy/snowy zones, build passive income base
3. **Late Game**: Access lava/void zones, optimize base layout
4. **End Game**: Farm secret zone, multiple rebirths, maximize multipliers

## Leaderboard System

### Tracked Metric

**PassiveIncome ($/second)** - The sum of income rates from all placed brainrots.

This metric represents true progression because:
- Higher rarity brainrots = higher income rate
- More brainrots placed = higher total income
- Encourages both collection speed and strategic zone farming

### Update Frequency

| Operation | Interval |
|-----------|----------|
| Update player's score | Every 10 seconds |
| Fetch leaderboard | Every 5 seconds |
| GUI refresh | After each fetch |

### Implementation

- Uses `OrderedDataStore` named "Leaderboard"
- Each entry: `{score: passiveIncome, name: playerName}`
- Key format: `player_{userId}`
- GUI shows top 5 players (top-right corner)

### API Endpoint

```
GET /games/{game_id}/leaderboard?store=Leaderboard&limit=10
```

Response:
```json
{
  "entries": [
    {"rank": 1, "key": "player_123", "score": 150.0, "name": "TopPlayer"},
    {"rank": 2, "key": "player_456", "score": 80.0, "name": "Runner Up"},
    ...
  ]
}
```

### Future Enhancements

- Secondary leaderboard for TotalMoney
- Weekly/monthly reset leaderboards
- Rebirth multiplier leaderboard

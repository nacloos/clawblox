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

## Map Layout (Rotated)

```
X axis (long): -100 to +100
Z axis (short): -40 to +40

[Secret Zone] [Void] [Lava] [Snowy] [Sandy] [Normal] [BASE]
   X: -100      -80    -60    -40     -20      0-50   50-100
```

- Base is at high X values (right side)
- Waves approach from low X (left side)
- Player spawns at X=75 (in base)

## Progression Flow

1. **Early Game**: Collect common brainrots, upgrade speed and capacity
2. **Mid Game**: Reach sandy/snowy zones, build passive income base
3. **Late Game**: Access lava/void zones, optimize base layout
4. **End Game**: Farm secret zone, multiple rebirths, maximize multipliers

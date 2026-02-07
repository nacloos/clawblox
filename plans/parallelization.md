# Game Loop Parallelization Plan

## Problem

With 100 players across 12-13 instances (max_players=8), the game loop takes **540ms** per tick:
- Each instance: ~45ms tick time
- Sequential processing: 12 × 45ms = **540ms** (vs 16.67ms budget)

## Solution: Rayon Parallel Processing

### Step 1: Add Rayon Dependency

**File:** `Cargo.toml`
```toml
rayon = "1.10"
```

### Step 2: Parallelize Instance Processing

**File:** `src/game/mod.rs`

Replace the sequential for loop in `run()` method (lines 84-115):

```rust
use rayon::prelude::*;

// In run() method - replace:
for entry in self.state.instances.iter() { ... }

// With:
let instances: Vec<(Uuid, GameInstanceHandle)> = self.state.instances
    .iter()
    .map(|e| (*e.key(), e.value().clone()))
    .collect();

instances.par_iter().for_each(|(instance_id, instance_handle)| {
    let mut instance = instance_handle.write();
    let game_id = instance.game_id;

    if instance.status == GameStatus::Playing {
        // ... existing tick logic
        instance.tick();
        // ... existing cleanup/cache logic
    }
});
```

## Files to Modify

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `rayon = "1.10"` |
| `src/game/mod.rs` | Add `use rayon::prelude::*`, parallelize instance loop |

## Verification

1. Add timing log: `eprintln!("[Tick] {:?}", elapsed)`
2. Before: ~540ms per tick
3. After: ~45ms per tick (12x improvement)
4. Check Railway metrics for multi-core CPU usage

## Railway Configuration

After deploying, increase vCPUs:
- **Settings → Service → Resources → vCPUs**: 4-8 recommended

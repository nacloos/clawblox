# Plan: Part.Touched Events

## Goal

Wire Rapier3D collision detection to fire `Part.Touched` signals in Lua - the standard Roblox way to detect collisions.

## Current State

- `Touched` and `TouchEnded` signals already exist on Parts (`src/game/lua/instance.rs:164-165`)
- But they are **never fired** - no code connects physics collisions to these signals
- Arsenal game uses manual distance checks as a workaround

## Implementation

### Step 1: Add collider-to-lua mapping in physics.rs

Currently we have `lua_to_body` and `body_to_lua` mappings, but we need `collider_to_lua` to look up which Lua instance a collider belongs to.

```rust
// In PhysicsWorld struct
pub collider_to_lua: HashMap<ColliderHandle, u64>,
```

Update `add_part()` to populate this mapping when creating colliders.

### Step 2: Add get_collision_pairs() in physics.rs

```rust
pub fn get_collision_pairs(&self) -> Vec<(u64, u64)> {
    let mut pairs = Vec::new();
    for pair in self.narrow_phase.contact_pairs() {
        if pair.has_any_active_contact() {
            if let (Some(&id1), Some(&id2)) = (
                self.collider_to_lua.get(&pair.collider1),
                self.collider_to_lua.get(&pair.collider2),
            ) {
                pairs.push((id1, id2));
            }
        }
    }
    pairs
}
```

### Step 3: Add fire_touched() on Instance in lua/instance.rs

```rust
impl Instance {
    pub fn fire_touched(&self, other: &Instance) {
        let data = self.data.lock().unwrap();
        if let Some(part_data) = &data.part_data {
            part_data.touched.fire(vec![other.clone()]);
        }
    }
}
```

### Step 4: Wire collision events in instance.rs tick loop

In `GameInstance::tick()`, after `physics.step()`:

```rust
// Fire Touched events for collision pairs
let collision_pairs = self.physics.get_collision_pairs();
if let Some(runtime) = &self.lua_runtime {
    runtime.fire_touched_events(&collision_pairs);
}
```

Add method to LuaRuntime:

```rust
pub fn fire_touched_events(&self, pairs: &[(u64, u64)]) {
    for (id1, id2) in pairs {
        if let (Some(inst1), Some(inst2)) = (
            self.get_instance_by_id(*id1),
            self.get_instance_by_id(*id2),
        ) {
            inst1.fire_touched(&inst2);
            inst2.fire_touched(&inst1);
        }
    }
}
```

## Files to Modify

| File | Changes |
|------|---------|
| `src/game/physics.rs` | Add `collider_to_lua` map, `get_collision_pairs()` |
| `src/game/instance.rs` | Call `fire_touched_events()` after physics step |
| `src/game/lua/instance.rs` | Add `fire_touched()` method |
| `src/game/lua/runtime.rs` | Add `fire_touched_events()`, `get_instance_by_id()` |

## Verification

```lua
-- Test script
local part1 = Instance.new("Part")
part1.Position = Vector3.new(0, 10, 0)
part1.Anchored = false
part1.Parent = Workspace

local part2 = Instance.new("Part")
part2.Position = Vector3.new(0, 0, 0)
part2.Anchored = true
part2.Parent = Workspace

part1.Touched:Connect(function(other)
    print("part1 touched", other.Name)
end)

-- part1 should fall and hit part2, firing Touched
```

# Plan: Fix Physics Engine + Rotating Bar Game

## Context
The physics engine has multiple gaps compared to Roblox: CFrame rotation isn't synced to physics colliders, Touched events never fire, runtime property changes (Size, CanCollide, Velocity) are silently ignored, only Block shape works in physics, characters can't jump, and moving anchored parts don't push characters. This plan fixes all gaps and creates a rotating bar game (Fall Guys style) to test them.

---

## Phase 1: Rotation Sync (CFrame <-> Physics)

**`src/game/physics.rs`**
- Add `rotation_matrix_to_quaternion(m: &[[f32;3];3]) -> UnitQuaternion<f32>` (Shepperd's method)
- Add `quaternion_to_rotation_matrix(q: &UnitQuaternion<f32>) -> [[f32;3];3]`
- Change `add_part()` signature: `rotation: [f32; 4]` → `rotation: [[f32;3];3]`, convert to axis-angle via quaternion
- Add `set_kinematic_rotation(&mut self, handle, rotation: &[[f32;3];3])`
- Add `get_rotation_matrix(&self, handle) -> Option<[[f32;3];3]>`
- Update all tests using `[0.0, 0.0, 0.0, 1.0]` → identity matrix

**`src/game/instance.rs`**
- `sync_lua_to_physics()`: pass `part_data.cframe.rotation` to `add_part()` (fixes line 541 TODO)
- `sync_lua_to_physics()`: call `set_kinematic_rotation()` after `set_kinematic_position()` for anchored parts
- `sync_physics_to_lua()`: call `get_rotation_matrix()` and update `part_data.cframe.rotation` for dynamic parts

## Phase 2: Dirty Flags + Property Sync

**`src/game/lua/instance.rs`**
- Add to `PartData`: `velocity_dirty`, `size_dirty`, `can_collide_dirty`, `anchored_dirty`, `shape_dirty` (all `bool`, init `false`)
- Set flags in respective property setters (Velocity:1087, Size:999, CanCollide:1027, Anchored:1015, Shape setter)

**`src/game/physics.rs`**
- Add `set_can_collide(&mut self, lua_id, can_collide: bool)` — toggles collider sensor mode

**`src/game/instance.rs`**
- In `sync_lua_to_physics()`, process dirty flags for existing parts:
  - `anchored_dirty` → `physics.set_anchored()`
  - `size_dirty` → `physics.set_size()` (already exists but never called)
  - `can_collide_dirty` → `physics.set_can_collide()` (new), also handle parts that need adding/removing from physics
  - `velocity_dirty` → `physics.set_velocity()`

## Phase 3: Part Shapes in Physics

**`src/game/physics.rs`**
- Add `shape: u8` param to `add_part()` (0=Ball, 1=Block, 2=Cylinder, 3=Wedge)
- Use `ColliderBuilder::ball()` for Ball, `ColliderBuilder::cylinder()` for Cylinder, cuboid for Block/Wedge
- Extract a `build_collider(size, shape, can_collide) -> Collider` helper for reuse in `set_size()`
- Store shape per-part: `lua_to_shape: HashMap<u64, u8>`
- Add `set_shape()` method (remove old collider, add new one)

**`src/game/instance.rs`**
- Pass `part_data.shape` to `add_part()` calls
- Handle `shape_dirty` in sync

## Phase 4: Touched/TouchEnded Events

**`src/game/physics.rs`**
- Add `collider_to_lua: HashMap<ColliderHandle, u64>`, populate in `add_part()`/`add_character()`, remove in `remove_part()`/`remove_character()`
- Add `detect_contacts(&self, prev: &HashSet<(u64,u64)>) -> (Vec<new>, Vec<ended>)` — iterates `narrow_phase.contact_pairs()` and `intersection_pairs()`

**`src/game/instance.rs`**
- Add `prev_contacts: HashSet<(u64, u64)>` field to `GameInstance`
- Add `fire_touch_events()` method: calls `physics.detect_contacts()`, builds lua_id → Instance lookup, fires `Touched`/`TouchEnded` signals respecting `CanTouch`
- Call `fire_touch_events()` in `tick()` after `physics.step()`, before `sync_physics_to_lua()`

## Phase 5: Jump Mechanics

**`src/game/lua/instance.rs`**
- Add `jump_requested: bool` to `HumanoidData` (default false)
- Add `Jump` property (getter/setter) and `Jump()` method on Humanoid

**`src/game/instance.rs`**
- In `update_character_movement()`: when `grounded && jump_requested`, set `vertical_velocity = jump_power`, clear flag
- Add helpers `get_humanoid_jump_info()` and `clear_humanoid_jump()`

## Phase 6: Kinematic Pushing Characters

**`src/game/instance.rs`**
- Add `resolve_kinematic_pushes()`: for each character, use `query_pipeline.intersections_with_shape()` to find overlapping kinematic bodies, compute penetration via `parry::query::contact()`, apply separation displacement
- Call in `tick()` after `physics.step()`, before `fire_touch_events()`

## Phase 7: Raycasting (defer or fix)

**`src/game/lua/services/workspace.rs`**
- Current: custom AABB, ignores rotation
- Fix: Replace with Rapier's `query_pipeline.cast_ray()` for rotation/shape awareness
- Requires passing physics reference to workspace — can defer if not needed for the game

## Phase 8: Rotating Bar Game

**`games/rotating-bar/game.lua`** (new file)
- Circular platform (Cylinder shape) elevated at Y=5
- Spinning bar: anchored Part, CFrame updated in Heartbeat with `CFrame.Angles(0, angle, 0)`
- Bar `Touched` event applies knockback velocity to HumanoidRootPart
- Fall detection: Y < -10 → respawn on platform
- Speed escalation: starts at 1 rad/s, ramps to 8 rad/s
- Jump input handler via AgentInputService

---

## Files Modified
| File | Changes |
|------|---------|
| `src/game/physics.rs` | Rotation conversion, shape colliders, set_kinematic_rotation, set_can_collide, detect_contacts, collider_to_lua, physics raycast |
| `src/game/instance.rs` | Rotation sync both ways, dirty flags, touch events, kinematic pushing, jump mechanics |
| `src/game/lua/instance.rs` | Dirty flags on PartData, jump_requested on HumanoidData, Jump property/method |
| `src/game/lua/services/workspace.rs` | (deferred) Replace AABB raycast with Rapier-backed |
| `games/rotating-bar/game.lua` | New game script |

## Implementation Order
```
Phase 1 (Rotation) → Phase 2 (Dirty flags) → Phase 3 (Shapes)
                                            → Phase 4 (Touched)
                                            → Phase 5 (Jump)
                                            → Phase 6 (Kinematic push)
                                            → Phase 7 (Raycast, deferred)
                                            → Phase 8 (Game script)
```

## Spike Test Findings (branch: spike/physics-caveats)

Ran 9 tests against Rapier3D 0.25 (`tests/physics_spike_test.rs`). All pass.

| # | Test | Result | Impact on Plan |
|---|------|--------|----------------|
| 1 | Kinematic+Kinematic contacts | **No contacts** | Phase 4: Cannot use `narrow_phase.contact_pairs()` for Touched between anchored parts and characters |
| 2 | Sensor workaround | **`query_pipeline.intersections_with_shape()` works** | Phase 4: Use query_pipeline, NOT narrow_phase, for Touched detection. `narrow_phase.intersection_pairs()` requires at least one dynamic body. |
| 3 | Rotating platform | **move_shape() follows rotation** | Phase 6: No manual tangential velocity needed. Simpler implementation. |
| 4 | Vertical elevator | **0.5 stud gap during upward motion** | Phase 6: Must add platform velocity to character's desired translation for elevators |
| 5 | Kinematic pushes dynamic | **Works natively** | Phase 6: No extra work for kinematic→dynamic push |
| 6 | Dynamic+Sensor events | **Intersection events fire** | Phase 4: Confirms `CanCollide=false` → sensor approach works |
| 7 | Sensor+Sensor | **No events** | Phase 4: Matches Roblox (two CanCollide=false parts don't fire Touched) |
| 8 | Wedge convex hull | **`SharedShape::convex_hull()` works** | Phase 3: Wedge shape is feasible via convex hull |
| 9 | Event type matrix | Dynamic+Dynamic=contacts, Dynamic+Kinematic=contacts, Kin+Kin=nothing, Dynamic+Sensor=intersections, Sensor+Sensor=nothing | Reference for Phase 4 implementation |

### Plan Adjustments Based on Findings
- **Phase 4 (Touched)**: `detect_contacts()` must use `query_pipeline.intersections_with_shape()` per-body, not `narrow_phase.contact_pairs()`/`intersection_pairs()`. This is more expensive (per-body query vs iterating pairs) but is the only approach that works for kinematic-kinematic.
- **Phase 6 (Kinematic push)**: Rotating platforms work for free via `move_shape()`. Vertical platforms need velocity compensation. Add platform velocity tracking to `set_kinematic_position()`.
- **Phase 3 (Shapes)**: Wedge confirmed viable via `convex_hull()`.

### API Notes
- `intersection_pairs()` returns tuples: use `|(_c1, _c2, intersecting)|` not `|_c1, _c2, intersecting|`
- `set_next_kinematic_rotation()` takes `UnitQuaternion` (from nalgebra)

## Verification
1. `cargo build` — compiles without errors
2. `cargo test` — all physics tests pass (update existing tests for new `add_part` signature)
3. Run game locally, spectate via frontend — bar visually rotates, collider matches rotation
4. Walk into bar → character blocked (physics collision)
5. Bar sweeps into standing character → character pushed off platform (kinematic push)
6. Jump input → character jumps over bar
7. Fall off platform → respawn

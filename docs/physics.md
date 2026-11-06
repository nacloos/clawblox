---
title: Physics
summary: "How the engine simulates parts, terrain, collisions, forces, and actuators."
read_when:
  - Building worlds with physical objects
  - Tuning collisions, forces, or movement
---

Clawblox has Roblox-like 3D physics backed by the engine simulation. Luau code
creates parts, terrain, welds, constraints, forces, and humanoid characters in
`Workspace`; the engine synchronizes those objects into the physics step.

## Parts

`Part` is the main physical object. Important fields include:

- `Position`
- `CFrame`
- `Size`
- `Shape`
- `Anchored`
- `CanCollide`
- `CanQuery`
- `CanTouch`
- `CollisionGroup`
- `Massless`
- `Velocity`
- `AssemblyLinearVelocity`
- `AssemblyAngularVelocity`
- `CustomPhysicalProperties`
- `CurrentPhysicalProperties`

Parts fire `Touched` and `TouchEnded`.

## Materials and shape

Parts have `Material`, `Color`, `Transparency`, `TopSurface`, and
`BottomSurface`.

Supported `Part.Shape` values are `Ball`, `Block`, `Cylinder`, `Ring`, and
`Wedge`.

## Workspace physics

`Workspace` exposes:

- `Gravity`
- `FluidForces`
- `FallenPartsDestroyHeight`
- `Raycast`
- `FindPartOnRay`
- `FindPartOnRayWithIgnoreList`
- `GetPartBoundsInBox`
- `GetPartBoundsInRadius`
- `GetPartsInPart`

`Workspace.Terrain` is created automatically and supports terrain APIs such as
`FillBlock`.

## Collision groups

`PhysicsService` supports Roblox-style collision groups with a maximum of 32
groups, registered and made collidable through methods such as
`RegisterCollisionGroup`, `CollisionGroupSetCollidable`, and
`SetPartCollisionGroup`. See the [engine API reference](/engine-api-reference)
for the full collision group method list.

Collision group settings control which parts actually collide during physics
simulation, not just what spatial queries return.

## Forces and constraints

Supported physics classes include:

- `Weld`
- `WeldConstraint`
- `BodyVelocity`
- `BodyForce`
- `BodyAngularVelocity`
- `Attachment`
- `VectorForce`
- `AlignPosition`
- `AlignOrientation`

`BasePart` also supports mass, impulse, pivot, and primary-part methods such as
`GetMass`, `GetConnectedParts`, `ApplyImpulse`, `ApplyImpulseAtPosition`,
`ApplyAngularImpulse`, `GetPivot`, and `PivotTo`. See the
[engine API reference](/engine-api-reference) for the full method list.

`SetNetworkOwner` and `SetNetworkOwnershipAuto` are accepted for Roblox
compatibility, but Clawblox currently runs authoritative server simulation, so
they are no-ops.

## Configuration

```toml
[clawblox]
player_object_physics = true

[clawblox.physics]
solver_iterations = 4
```

`solver_iterations` must be between `1` and `32`.

`player_object_physics = false` is the mesa-island compatibility mode. It
reproduces the character physics used for the paper runs (branch
`nacloos/mesa-island`): characters don't push or ride loose parts, and the
later character-motion changes (wedge recovery, terminal fall speed, the
vertical-velocity resets, support load) are off. Use it to compare against
the paper's results or replay its recordings.

## Examples

Collision groups:

```lua
local PhysicsService = game:GetService("PhysicsService")

PhysicsService:RegisterCollisionGroup("Players")
PhysicsService:RegisterCollisionGroup("Triggers")
PhysicsService:CollisionGroupSetCollidable("Players", "Triggers", false)

playerPart.CollisionGroup = "Players"
triggerPart.CollisionGroup = "Triggers"
```

Impulse:

```lua
part:ApplyImpulse(Vector3.new(0, part:GetMass() * 80, 0))
part:ApplyImpulseAtPosition(Vector3.new(0, 0, 100), part.Position + Vector3.new(0, 3, 0))
```

AlignPosition:

```lua
local attachment = Instance.new("Attachment")
attachment.Parent = part

local align = Instance.new("AlignPosition")
align.Attachment0 = attachment
align.Position = Vector3.new(20, 6, 0)
align.MaxForce = 10000
align.Responsiveness = 20
align.Parent = part
```

Terrain water:

```lua
Workspace.Terrain:FillBlock(
  CFrame.new(0, -8, 0),
  Vector3.new(64, 16, 64),
  Enum.Material.Water
)
```

`Workspace.FluidForces` controls whether terrain water applies fluid forces.
Parts can opt out with `EnableFluidForces = false`.

## Current differences from Roblox

Clawblox aims for Roblox-like semantics, but it is an engine for coding-agent
worlds rather than the Roblox client/server runtime.

- network ownership is accepted but not modeled; server simulation is
  authoritative
- terrain voxel APIs currently use resolution `4`
- `FillBlock` supports the Roblox call shape, but rotated terrain fill is an
  approximation
- the physics class/property set is partial; prefer the documented fields and
  methods for portable Clawblox worlds
- shape support is limited to `Ball`, `Block`, `Cylinder`, `Ring`, and `Wedge`

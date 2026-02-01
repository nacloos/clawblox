# Clawblox Scripting API

Clawblox implements a Roblox-compatible Luau scripting system. Scripts written for Roblox should work on Clawblox with minimal changes.

## Global Objects

### game
The root of the game hierarchy.

```lua
local Workspace = game:GetService("Workspace")
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")
```

### Workspace
Global reference to `game:GetService("Workspace")`.

### Players
Global reference to `game:GetService("Players")`.

---

## Classes

### Instance
Base class for all objects in the game hierarchy.

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `Name` | string | The name of this instance |
| `Parent` | Instance? | The parent of this instance |
| `ClassName` | string | (read-only) The class name |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `Clone()` | Instance | Creates a copy of this instance and descendants |
| `Destroy()` | void | Removes this instance and all descendants |
| `FindFirstChild(name, recursive?)` | Instance? | Finds first child with name |
| `FindFirstChildOfClass(className)` | Instance? | Finds first child of class |
| `GetChildren()` | {Instance} | Returns array of direct children |
| `GetDescendants()` | {Instance} | Returns array of all descendants |
| `IsA(className)` | bool | Checks if instance is of class |
| `IsDescendantOf(ancestor)` | bool | Checks if descendant of ancestor |
| `SetAttribute(name, value)` | void | Sets a custom attribute |
| `GetAttribute(name)` | any | Gets a custom attribute |
| `GetAttributes()` | {[string]: any} | Gets all attributes |

#### Events
| Event | Parameters | Description |
|-------|------------|-------------|
| `ChildAdded` | (child: Instance) | Fires when child is added |
| `ChildRemoved` | (child: Instance) | Fires when child is removed |
| `Destroying` | () | Fires before instance is destroyed |
| `AttributeChanged` | (name: string) | Fires when attribute changes |

#### Constructor
```lua
local part = Instance.new("Part")
local part = Instance.new("Part", parent)  -- with parent
```

---

### BasePart
Base class for all physical parts. Inherits from Instance.

#### Properties
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `Position` | Vector3 | (0,0,0) | World position |
| `CFrame` | CFrame | identity | Position and orientation |
| `Size` | Vector3 | (4,1,2) | Part dimensions |
| `Anchored` | bool | false | Immovable by physics |
| `CanCollide` | bool | true | Physical collision enabled |
| `CanTouch` | bool | true | Touched events enabled |
| `Transparency` | number | 0 | 0 = opaque, 1 = invisible |
| `Color` | Color3 | (0.6,0.6,0.6) | Part color |
| `Material` | Enum.Material | Plastic | Surface material |
| `Velocity` | Vector3 | (0,0,0) | Linear velocity |
| `AssemblyLinearVelocity` | Vector3 | (0,0,0) | Assembly velocity |

#### Events
| Event | Parameters | Description |
|-------|------------|-------------|
| `Touched` | (otherPart: BasePart) | Part touched another part |
| `TouchEnded` | (otherPart: BasePart) | Parts stopped touching |

---

### Part
A basic part. Inherits from BasePart.

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `Shape` | Enum.PartType | Ball, Block, Cylinder, Wedge |

```lua
local part = Instance.new("Part")
part.Shape = Enum.PartType.Ball
part.Size = Vector3.new(4, 4, 4)
part.Position = Vector3.new(0, 10, 0)
part.Anchored = false
part.Parent = Workspace
```

---

### Model
A container for grouping Instances. Inherits from Instance.

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `PrimaryPart` | BasePart? | The primary part for CFrame operations |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `GetPrimaryPartCFrame()` | CFrame | CFrame of primary part |
| `SetPrimaryPartCFrame(cframe)` | void | Moves model via primary part |

---

### Humanoid
Controls character behavior. Inherits from Instance.

#### Properties
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `Health` | number | 100 | Current health |
| `MaxHealth` | number | 100 | Maximum health |
| `WalkSpeed` | number | 16 | Movement speed (studs/sec) |
| `JumpPower` | number | 50 | Jump force |
| `JumpHeight` | number | 7.2 | Jump height (studs) |
| `AutoRotate` | bool | true | Rotate toward movement |
| `HipHeight` | number | 2 | Height off ground |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `TakeDamage(amount)` | void | Reduces health |
| `Move(direction, relativeToCamera?)` | void | Walk in direction |
| `MoveTo(position, part?)` | void | Walk to position |

#### Events
| Event | Parameters | Description |
|-------|------------|-------------|
| `Died` | () | Health reached 0 |
| `HealthChanged` | (health: number) | Health changed |
| `MoveToFinished` | (reached: bool) | MoveTo completed |

---

### Player
Represents a connected player. Inherits from Instance.

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `UserId` | number | Unique player ID |
| `Name` | string | Player's username |
| `DisplayName` | string | Player's display name |
| `Character` | Model? | Player's character model |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `LoadCharacter()` | void | Spawns/respawns character |
| `Kick(message?)` | void | Removes player from game |

#### Events
| Event | Parameters | Description |
|-------|------------|-------------|
| `CharacterAdded` | (character: Model) | Character spawned |
| `CharacterRemoving` | (character: Model) | Character despawning |

---

## Services

### Players
Manages connected players.

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `LocalPlayer` | Player? | (client only) The local player |
| `MaxPlayers` | number | Maximum players allowed |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `GetPlayers()` | {Player} | All connected players |
| `GetPlayerByUserId(userId)` | Player? | Find player by ID |
| `GetPlayerFromCharacter(character)` | Player? | Find player from character model |

#### Events
| Event | Parameters | Description |
|-------|------------|-------------|
| `PlayerAdded` | (player: Player) | Player joined |
| `PlayerRemoving` | (player: Player) | Player leaving |

```lua
local Players = game:GetService("Players")

Players.PlayerAdded:Connect(function(player)
    print(player.Name .. " joined!")

    player.CharacterAdded:Connect(function(character)
        local humanoid = character:FindFirstChild("Humanoid")
        humanoid.MaxHealth = 200
        humanoid.Health = 200
    end)
end)
```

---

### Workspace
The 3D world container. Inherits from Instance.

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `Gravity` | number | World gravity (default: 196.2) |
| `CurrentCamera` | Camera? | Active camera |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `Raycast(origin, direction, params?)` | RaycastResult? | Cast a ray |
| `GetPartBoundsInBox(cframe, size)` | {BasePart} | Parts in box region |
| `GetPartBoundsInRadius(position, radius)` | {BasePart} | Parts in sphere |

```lua
-- Raycast example
local result = Workspace:Raycast(origin, direction * 100)
if result then
    print("Hit:", result.Instance.Name)
    print("Position:", result.Position)
    print("Normal:", result.Normal)
    print("Distance:", result.Distance)
end
```

---

### RunService
Game loop and timing.

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `Heartbeat` | RBXScriptSignal | Fires every frame (after physics) |
| `Stepped` | RBXScriptSignal | Fires every frame (before physics) |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `IsServer()` | bool | Running on server |
| `IsClient()` | bool | Running on client |

```lua
local RunService = game:GetService("RunService")

RunService.Heartbeat:Connect(function(deltaTime)
    -- Runs every frame
end)
```

---

## Data Types

### Vector3
3D vector.

#### Constructors
```lua
Vector3.new(x, y, z)
Vector3.zero         -- (0, 0, 0)
Vector3.one          -- (1, 1, 1)
Vector3.xAxis        -- (1, 0, 0)
Vector3.yAxis        -- (0, 1, 0)
Vector3.zAxis        -- (0, 0, 1)
```

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `X` | number | X component |
| `Y` | number | Y component |
| `Z` | number | Z component |
| `Magnitude` | number | Length |
| `Unit` | Vector3 | Normalized (length 1) |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `Dot(other)` | number | Dot product |
| `Cross(other)` | Vector3 | Cross product |
| `Lerp(goal, alpha)` | Vector3 | Linear interpolation |
| `FuzzyEq(other, epsilon?)` | bool | Approximate equality |

#### Operators
```lua
v1 + v2      -- Add
v1 - v2      -- Subtract
v1 * v2      -- Component multiply
v1 * n       -- Scalar multiply
v1 / v2      -- Component divide
v1 / n       -- Scalar divide
-v           -- Negate
```

---

### CFrame
Position and orientation (Coordinate Frame).

#### Constructors
```lua
CFrame.new()                        -- Identity
CFrame.new(x, y, z)                 -- Position only
CFrame.new(pos, lookAt)             -- Look at point
CFrame.lookAt(pos, target, up?)     -- Look at with up vector
CFrame.fromEulerAnglesXYZ(rx, ry, rz)
CFrame.Angles(rx, ry, rz)           -- Alias for above
```

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `Position` | Vector3 | Position component |
| `LookVector` | Vector3 | Forward direction (-Z) |
| `RightVector` | Vector3 | Right direction (+X) |
| `UpVector` | Vector3 | Up direction (+Y) |
| `X`, `Y`, `Z` | number | Position components |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `Inverse()` | CFrame | Inverse transformation |
| `Lerp(goal, alpha)` | CFrame | Interpolate |
| `ToWorldSpace(cf)` | CFrame | Transform to world |
| `ToObjectSpace(cf)` | CFrame | Transform to local |
| `PointToWorldSpace(v3)` | Vector3 | Point to world |
| `PointToObjectSpace(v3)` | Vector3 | Point to local |
| `GetComponents()` | (12 numbers) | Matrix components |

#### Operators
```lua
cf1 * cf2    -- Combine transformations
cf * v3      -- Transform point
cf + v3      -- Translate
cf - v3      -- Translate inverse
```

---

### Color3
RGB color.

#### Constructors
```lua
Color3.new(r, g, b)           -- 0-1 range
Color3.fromRGB(r, g, b)       -- 0-255 range
Color3.fromHSV(h, s, v)       -- HSV color space
Color3.fromHex("#FF5500")     -- Hex string
```

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `R` | number | Red (0-1) |
| `G` | number | Green (0-1) |
| `B` | number | Blue (0-1) |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `Lerp(goal, alpha)` | Color3 | Interpolate colors |
| `ToHSV()` | (h, s, v) | Convert to HSV |
| `ToHex()` | string | Convert to hex |

---

### RaycastResult
Returned by Workspace:Raycast().

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `Instance` | BasePart | Part that was hit |
| `Position` | Vector3 | Hit position |
| `Normal` | Vector3 | Surface normal |
| `Distance` | number | Distance to hit |

---

### RaycastParams
Parameters for raycasting.

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `FilterType` | Enum.RaycastFilterType | Include or Exclude |
| `FilterDescendantsInstances` | {Instance} | Instances to filter |
| `IgnoreWater` | bool | Ignore water |
| `CollisionGroup` | string | Collision group |

```lua
local params = RaycastParams.new()
params.FilterType = Enum.RaycastFilterType.Exclude
params.FilterDescendantsInstances = {character}

local result = Workspace:Raycast(origin, direction, params)
```

---

## Enums

### Enum.PartType
```lua
Enum.PartType.Ball
Enum.PartType.Block
Enum.PartType.Cylinder
Enum.PartType.Wedge
```

### Enum.Material
```lua
Enum.Material.Plastic
Enum.Material.Wood
Enum.Material.Metal
Enum.Material.Glass
Enum.Material.Neon
Enum.Material.Concrete
-- ... many more
```

### Enum.HumanoidStateType
```lua
Enum.HumanoidStateType.Running
Enum.HumanoidStateType.Jumping
Enum.HumanoidStateType.Freefall
Enum.HumanoidStateType.Dead
Enum.HumanoidStateType.Physics
```

### Enum.RaycastFilterType
```lua
Enum.RaycastFilterType.Include
Enum.RaycastFilterType.Exclude
```

---

## Events Pattern

Clawblox uses the Roblox `:Connect()` pattern for events:

```lua
local connection = event:Connect(function(...)
    -- handler
end)

-- Later, to disconnect:
connection:Disconnect()

-- One-time listener:
event:Once(function(...)
    -- fires once then auto-disconnects
end)
```

---

## Example: Complete Game

```lua
-- Chase game: Zombies chase players

local Players = game:GetService("Players")
local RunService = game:GetService("RunService")

local zombies = {}
local ZOMBIE_SPEED = 10
local ZOMBIE_DAMAGE = 15
local SPAWN_INTERVAL = 5

local lastSpawn = 0

-- Spawn a zombie at random position
local function spawnZombie()
    local zombie = Instance.new("Part")
    zombie.Name = "Zombie"
    zombie.Size = Vector3.new(4, 6, 2)
    zombie.Color = Color3.fromRGB(0, 150, 0)
    zombie.Position = Vector3.new(
        math.random(-50, 50),
        3,
        math.random(-50, 50)
    )
    zombie.Anchored = true
    zombie:SetAttribute("Health", 100)
    zombie.Parent = Workspace

    table.insert(zombies, zombie)
    return zombie
end

-- Find nearest player to a position
local function getNearestPlayer(position)
    local nearest = nil
    local nearestDist = math.huge

    for _, player in ipairs(Players:GetPlayers()) do
        local character = player.Character
        if character then
            local humanoid = character:FindFirstChild("Humanoid")
            local rootPart = character:FindFirstChild("HumanoidRootPart")

            if humanoid and humanoid.Health > 0 and rootPart then
                local dist = (rootPart.Position - position).Magnitude
                if dist < nearestDist then
                    nearest = player
                    nearestDist = dist
                end
            end
        end
    end

    return nearest, nearestDist
end

-- Main game loop
RunService.Heartbeat:Connect(function(dt)
    -- Spawn zombies periodically
    lastSpawn = lastSpawn + dt
    if lastSpawn >= SPAWN_INTERVAL then
        spawnZombie()
        lastSpawn = 0
    end

    -- Update zombies
    for i = #zombies, 1, -1 do
        local zombie = zombies[i]

        if zombie:GetAttribute("Health") <= 0 then
            zombie:Destroy()
            table.remove(zombies, i)
        else
            local nearest, dist = getNearestPlayer(zombie.Position)

            if nearest and dist < 100 then
                local character = nearest.Character
                local rootPart = character:FindFirstChild("HumanoidRootPart")

                if dist < 3 then
                    -- Attack!
                    local humanoid = character:FindFirstChild("Humanoid")
                    if humanoid then
                        humanoid:TakeDamage(ZOMBIE_DAMAGE * dt)
                    end
                else
                    -- Chase
                    local direction = (rootPart.Position - zombie.Position).Unit
                    zombie.Position = zombie.Position + direction * ZOMBIE_SPEED * dt
                end
            end
        end
    end
end)

-- Handle player joining
Players.PlayerAdded:Connect(function(player)
    print(player.Name .. " joined the game!")
end)

print("Zombie Chase loaded!")
```

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

## Global Functions

### tick()
Returns the time in seconds since the game started. Used for timing and animations.

```lua
local startTime = tick()
-- ... later
local elapsed = tick() - startTime
print("Elapsed:", elapsed, "seconds")
```

**Note:** Unlike Roblox's `tick()` which returns Unix epoch time, Clawblox returns time since game start for simplicity.

### wait(seconds?)
Yields the current thread for the specified duration (default: 0 seconds / 1 frame).

```lua
wait(2)  -- Wait 2 seconds
wait()   -- Wait 1 frame
```

Delegates to `task.wait()`. Returns the actual elapsed time.

### print(...)
Outputs to the game console.

```lua
print("Hello", "World", 42)  -- Output: Hello	World	42
```

### warn(...)
Outputs a warning to the game console.

```lua
warn("Something unexpected happened")
```

---

## task Library

The `task` library provides thread scheduling functions, matching the [Roblox task library](https://create.roblox.com/docs/reference/engine/libraries/task).

### task.spawn(functionOrThread, ...args)
Creates a new coroutine and resumes it immediately with the given arguments. If passed an existing thread, resumes that thread instead.

```lua
task.spawn(function(msg)
    print(msg)  -- prints immediately
end, "Hello")
```

Returns the thread.

### task.delay(seconds, function, ...args)
Schedules a function to run after `seconds` have elapsed. The function receives the provided arguments.

```lua
task.delay(2, function()
    print("2 seconds later")
end)

task.delay(1, function(a, b)
    print(a + b)  -- prints 30 after 1 second
end, 10, 20)
```

Returns the thread (can be passed to `task.cancel`).

### task.defer(functionOrThread, ...args)
Schedules a function or thread to run on the next resumption cycle (next tick). Similar to `task.spawn` but does not resume immediately.

```lua
task.defer(function()
    print("runs next tick")
end)
```

Returns the thread.

### task.wait(seconds?)
Yields the current thread for `seconds` (default: 0, meaning resume next tick). Returns the actual elapsed time.

```lua
local elapsed = task.wait(1)
print("Waited", elapsed, "seconds")
```

### task.cancel(thread)
Cancels a scheduled thread so it never runs.

```lua
local t = task.delay(5, function()
    print("this will never print")
end)
task.cancel(t)
```

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
| `AddTag(tag)` | void | Adds a tag to this instance |
| `HasTag(tag)` | bool | Returns true if instance has the tag |
| `RemoveTag(tag)` | void | Removes a tag from this instance |
| `GetTags()` | {string} | Returns array of all tags |

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

### Weld
A constraint that welds two parts together. Inherits from Instance.

#### Properties
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `Part0` | BasePart? | nil | First part to weld |
| `Part1` | BasePart? | nil | Second part to weld |
| `C0` | CFrame | identity | Offset relative to Part0 |
| `C1` | CFrame | identity | Offset relative to Part1 |
| `Enabled` | bool | true | Whether the weld is active |

```lua
local weld = Instance.new("Weld")
weld.Part0 = partA
weld.Part1 = partB
weld.C0 = CFrame.new(0, 1, 0)
weld.Parent = partA
```

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
| `CancelMoveTo()` | void | Cancels the current MoveTo |

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
| `PlayerGui` | PlayerGui? | (read-only) Player's GUI container |

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

### Folder
A container for organizing instances. Inherits from Instance. Has no additional properties or methods.

```lua
local folder = Instance.new("Folder")
folder.Name = "Weapons"
folder.Parent = Workspace
```

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

### HttpService
Provides JSON encoding and decoding.

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `JSONEncode(value)` | string | Converts a Lua value to a JSON string |
| `JSONDecode(json)` | any | Parses a JSON string into a Lua value |

```lua
local HttpService = game:GetService("HttpService")

local data = { score = 100, name = "Player1", items = {"sword", "shield"} }
local json = HttpService:JSONEncode(data)
print(json) -- {"items":["sword","shield"],"name":"Player1","score":100}

local decoded = HttpService:JSONDecode(json)
print(decoded.score) -- 100
```

---

### DataStoreService
Persistent key-value storage backed by the database.

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `GetDataStore(name)` | DataStore | Gets a named data store |
| `GetOrderedDataStore(name)` | OrderedDataStore | Gets a named ordered data store |

#### DataStore

| Method | Returns | Description |
|--------|---------|-------------|
| `GetAsync(key)` | any | Gets the value for a key (yields) |
| `SetAsync(key, value)` | void | Sets the value for a key (yields) |
| `RemoveAsync(key)` | void | Removes a key (yields) |
| `UpdateAsync(key, transform)` | any | Atomically updates a key (yields) |

#### OrderedDataStore

| Method | Returns | Description |
|--------|---------|-------------|
| `GetAsync(key)` | any | Gets the value for a key (yields) |
| `SetAsync(key, value)` | void | Sets a value (must have `score` field) (yields) |
| `GetSortedAsync(ascending, limit)` | {{key, value}} | Returns sorted entries (yields) |

```lua
local DataStoreService = game:GetService("DataStoreService")

-- Basic key-value storage
local playerStore = DataStoreService:GetDataStore("PlayerData")
playerStore:SetAsync("player_123", { coins = 500, level = 5 })
local data = playerStore:GetAsync("player_123")
print(data.coins) -- 500

-- Atomic update
playerStore:UpdateAsync("player_123", function(old)
    old.coins = old.coins + 100
    return old
end)

-- Ordered leaderboard
local leaderboard = DataStoreService:GetOrderedDataStore("Leaderboard")
leaderboard:SetAsync("player_123", { score = 1500 })
local top10 = leaderboard:GetSortedAsync(false, 10)
for _, entry in ipairs(top10) do
    print(entry.key, entry.value.score)
end
```

---

### AgentInputService

**Clawblox extension** - Handles input from AI agents via the HTTP API.

Games should support both human players (UserInputService, future) and AI agents (AgentInputService) to work with all player types.

#### Events
| Event | Parameters | Description |
|-------|------------|-------------|
| `InputReceived` | (player, inputType, data) | Fires when an agent sends input |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `GetInputs(player)` | {Input} | Get and clear pending inputs for player |
| `HasPendingInputs(player)` | bool | Check if there are pending inputs |

#### Input Flow

```
Agent (HTTP API)  →  POST /games/{id}/input  →  AgentInputService  →  InputReceived event  →  Game Script
```

#### Usage

**Event-based** (recommended for discrete actions like Fire, Melee):
```lua
local AgentInputService = game:GetService("AgentInputService")

AgentInputService.InputReceived:Connect(function(player, inputType, data)
    if inputType == "Fire" then
        -- data.direction is {dx, dy, dz}
        local dir = data.direction
        fireWeapon(player, Vector3.new(dir[1], dir[2], dir[3]))

    elseif inputType == "MoveTo" then
        -- data.position is {x, y, z}
        local pos = data.position
        local humanoid = player.Character:FindFirstChild("Humanoid")
        if humanoid then
            humanoid:MoveTo(Vector3.new(pos[1], pos[2], pos[3]))
        end

    elseif inputType == "Melee" then
        meleeAttack(player)
    end
end)
```

**Polling** (for continuous state):
```lua
RunService.Heartbeat:Connect(function(dt)
    for _, player in ipairs(Players:GetPlayers()) do
        local inputs = AgentInputService:GetInputs(player)
        for _, input in ipairs(inputs) do
            processInput(player, input.type, input.data)
        end
    end
end)
```

---

## GUI

GUI classes for building 2D interfaces. All GUI elements inherit from GuiObject base properties.

### GuiObject (Base)
Base class for all 2D GUI elements. Not instantiated directly.

#### Properties
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `Position` | UDim2 | {0,0,0,0} | Position (scale + offset) |
| `Size` | UDim2 | {0,0,0,0} | Size (scale + offset) |
| `AnchorPoint` | {X, Y} | {0, 0} | Anchor point (0-1) |
| `Rotation` | number | 0 | Rotation in degrees |
| `BackgroundColor3` | Color3 | (1,1,1) | Background color |
| `BackgroundTransparency` | number | 0 | 0 = opaque, 1 = invisible |
| `BorderColor3` | Color3 | (0.1,0.1,0.1) | Border color |
| `BorderSizePixel` | number | 1 | Border thickness in pixels |
| `ZIndex` | number | 0 | Rendering order |
| `LayoutOrder` | number | 0 | Layout sort order |
| `Visible` | bool | true | Whether element is visible |

---

### PlayerGui
Container for a player's GUI elements. Accessed via `player.PlayerGui`. Parent ScreenGuis here.

---

### ScreenGui
Top-level GUI container. Parent to PlayerGui.

#### Properties
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `DisplayOrder` | number | 0 | Rendering order among ScreenGuis |
| `IgnoreGuiInset` | bool | false | Extend into top bar area |
| `Enabled` | bool | true | Whether this GUI is visible |

---

### BillboardGui
A GUI that appears in 3D space, attached to a part.

#### Properties
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `Size` | UDim2 | {0,100,0,50} | Size of the billboard |
| `StudsOffset` | Vector3 | (0,0,0) | Offset from adornee in studs |
| `AlwaysOnTop` | bool | false | Render on top of 3D objects |
| `Enabled` | bool | true | Whether the billboard is visible |
| `Adornee` | BasePart? | nil | Part to attach to |

```lua
local billboard = Instance.new("BillboardGui")
billboard.Size = UDim2.fromOffset(100, 30)
billboard.StudsOffset = Vector3.new(0, 3, 0)
billboard.AlwaysOnTop = true
billboard.Adornee = character:FindFirstChild("HumanoidRootPart")
billboard.Parent = character
```

---

### Frame
A container element. Inherits all GuiObject properties.

```lua
local frame = Instance.new("Frame")
frame.Size = UDim2.fromScale(0.5, 0.5)
frame.Position = UDim2.fromScale(0.25, 0.25)
frame.BackgroundColor3 = Color3.fromRGB(30, 30, 30)
frame.BackgroundTransparency = 0.2
frame.Parent = screenGui
```

---

### TextLabel
Displays text. Inherits all GuiObject properties.

#### Properties
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `Text` | string | "" | Displayed text |
| `TextColor3` | Color3 | (0,0,0) | Text color |
| `TextSize` | number | 14 | Font size (min 1) |
| `TextTransparency` | number | 0 | 0 = opaque, 1 = invisible |
| `TextScaled` | bool | false | Scale text to fit |
| `TextXAlignment` | string | "Center" | "Left", "Center", "Right" |
| `TextYAlignment` | string | "Center" | "Top", "Center", "Bottom" |

---

### TextButton
A clickable text element. Inherits all TextLabel properties.

#### Events
| Event | Parameters | Description |
|-------|------------|-------------|
| `MouseButton1Click` | () | Button clicked |
| `MouseButton1Down` | () | Mouse pressed |
| `MouseButton1Up` | () | Mouse released |
| `MouseEnter` | () | Mouse entered |
| `MouseLeave` | () | Mouse left |

---

### ImageLabel
Displays an image. Inherits all GuiObject properties.

#### Properties
| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `Image` | string | "" | Image URL or asset path |
| `ImageColor3` | Color3 | (1,1,1) | Image tint color |
| `ImageTransparency` | number | 0 | 0 = opaque, 1 = invisible |

---

### ImageButton
A clickable image element. Inherits all ImageLabel properties. Has the same events as TextButton.

---

### GUI Example

```lua
local Players = game:GetService("Players")

Players.PlayerAdded:Connect(function(player)
    player.CharacterAdded:Connect(function()
        local screenGui = Instance.new("ScreenGui")
        screenGui.Parent = player.PlayerGui

        -- Health bar background
        local bg = Instance.new("Frame")
        bg.Size = UDim2.fromOffset(200, 20)
        bg.Position = UDim2.fromOffset(10, 10)
        bg.BackgroundColor3 = Color3.fromRGB(40, 40, 40)
        bg.Parent = screenGui

        -- Health bar fill
        local fill = Instance.new("Frame")
        fill.Size = UDim2.fromScale(1, 1)
        fill.BackgroundColor3 = Color3.fromRGB(0, 200, 0)
        fill.Parent = bg

        -- Health text
        local label = Instance.new("TextLabel")
        label.Size = UDim2.fromScale(1, 1)
        label.BackgroundTransparency = 1
        label.Text = "100 / 100"
        label.TextColor3 = Color3.new(1, 1, 1)
        label.TextSize = 14
        label.Parent = bg
    end)
end)
```

---

## Game Skills

Games define their controls and rules in a `SKILL.md` file. AI agents read this file to learn how to play the game.

**Location**: `games/{game-name}/SKILL.md`

The SKILL.md file includes:
- YAML frontmatter with name and description
- Available inputs and their data format
- Observation format (what the agent sees)
- Game rules and mechanics
- Strategy tips

### Example Structure

```
games/
  arsenal/
    SKILL.md       # Agent-readable game instructions
    game.lua       # Game logic
```

### Observation Format

Observations are returned by `GET /games/{id}/observe` and include:

```json
{
  "tick": 1234,
  "game_status": "active",
  "player": {
    "id": "uuid",
    "position": [x, y, z],
    "health": 100,
    "attributes": { ... }  // Game-specific data set via SetAttribute
  },
  "other_players": [ ... ],
  "world": {
    "entities": [ ... ]  // Dynamic (non-static) workspace entities
  },
  "events": [ ... ]
}
```

The `attributes` field contains whatever the game script sets via `player:SetAttribute()`. This keeps the engine generic while allowing games to define their own data.

The `world` field contains dynamic workspace entities — parts and folders that do **not** have the `"Static"` tag. This includes projectiles, pickups, game-state folders with attributes, and other entities that change each tick. Static map geometry (tagged `"Static"`) is fetched once via `GET /games/{id}/map`.

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

### UDim
A one-dimensional value with scale (fraction) and offset (pixels).

#### Constructor
```lua
UDim.new(scale, offset)
```

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `Scale` | number | Scale component (0-1 fraction) |
| `Offset` | number | Offset component (pixels) |

#### Operators
```lua
udim1 + udim2    -- Add
udim1 - udim2    -- Subtract
```

---

### UDim2
A two-dimensional value (X and Y UDims) for GUI positioning and sizing.

#### Constructors
```lua
UDim2.new(xScale, xOffset, yScale, yOffset)
UDim2.fromScale(xScale, yScale)    -- Offset = 0
UDim2.fromOffset(xOffset, yOffset) -- Scale = 0
```

#### Properties
| Property | Type | Description |
|----------|------|-------------|
| `X` | UDim | X component |
| `Y` | UDim | Y component |
| `Width` | UDim | Alias for X |
| `Height` | UDim | Alias for Y |

#### Methods
| Method | Returns | Description |
|--------|---------|-------------|
| `Lerp(goal, alpha)` | UDim2 | Linear interpolation |

#### Operators
```lua
udim2a + udim2b    -- Add
udim2a - udim2b    -- Subtract
```

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

## 3D Models (GLB) and Assets

You can render a 3D model (`.glb` file) on a Part instead of the default primitive shape by setting the `ModelUrl` attribute.

### Asset Protocol (`asset://`)

Place game assets (models, images, audio) in the `assets/` directory and reference them using the `asset://` protocol:

```lua
part:SetAttribute("ModelUrl", "asset://models/tree.glb")
```

The engine automatically resolves `asset://` URLs:
- **Local development** (`clawblox run`): resolved to `/assets/models/tree.glb` (served from your local `assets/` directory)
- **Production** (`clawblox deploy`): resolved to the CDN URL (assets are uploaded to cloud storage on deploy)

Game developers never need to deal with server paths or CDN URLs — just use `asset://`.

**Allowed file types:** `.glb`, `.gltf`, `.png`, `.jpg`, `.jpeg`, `.wav`, `.mp3`, `.ogg`

Assets are automatically uploaded when you run `clawblox deploy` if an `assets/` directory exists.

### Legacy: Static Files

For backwards compatibility, URLs starting with `/static/` are still supported. Place files in a `static/` directory and reference them directly:

```lua
part:SetAttribute("ModelUrl", "/static/models/knight.glb")
```

Note: `/static/` files are **not** uploaded to cloud storage on deploy. Use `asset://` for new projects.

### Skeletal Animations

GLB models with skeletal animations are supported. The engine auto-detects animations named `walk`, `run`, and `idle` and plays them based on the character's movement state.

### Example

```lua
local character = Instance.new("Part")
character.Name = "Knight"
character.Size = Vector3.new(2, 4, 2)
character.Position = Vector3.new(0, 2, 0)
character.Anchored = true
character:SetAttribute("ModelUrl", "asset://models/knight.glb")
character.Parent = Workspace
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

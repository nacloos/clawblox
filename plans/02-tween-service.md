# Plan: TweenService

## Goal

Implement Roblox-compatible TweenService for smooth property animations (used for tsunami movement).

## API

```lua
local TweenService = game:GetService("TweenService")
local tweenInfo = TweenInfo.new(2, Enum.EasingStyle.Linear, Enum.EasingDirection.Out)
local tween = TweenService:Create(part, tweenInfo, {Position = Vector3.new(0, 0, 100)})
tween:Play()
tween.Completed:Connect(function()
    print("Done!")
end)
```

## Implementation

### Step 1: Create TweenInfo type

File: `src/game/lua/types/tween_info.rs`

```rust
pub struct TweenInfo {
    pub duration: f32,
    pub easing_style: EasingStyle,
    pub easing_direction: EasingDirection,
    pub repeat_count: i32,
    pub reverses: bool,
    pub delay: f32,
}

impl Default for TweenInfo {
    fn default() -> Self {
        Self {
            duration: 1.0,
            easing_style: EasingStyle::Quad,
            easing_direction: EasingDirection::Out,
            repeat_count: 0,
            reverses: false,
            delay: 0.0,
        }
    }
}
```

Register constructor: `TweenInfo.new(duration, easingStyle?, easingDirection?)`

### Step 2: Create Tween object

```rust
pub struct Tween {
    pub instance: Instance,
    pub tween_info: TweenInfo,
    pub goals: HashMap<String, PropertyValue>,  // Property name -> target value
    pub start_values: HashMap<String, PropertyValue>,
    pub elapsed: f32,
    pub playing: bool,
    pub completed: RBXScriptSignal,
}
```

Methods:
- `Play()` - start the tween
- `Pause()` - pause
- `Cancel()` - stop and reset

### Step 3: Create TweenService

File: `src/game/lua/services/tween.rs`

```rust
pub struct TweenService {
    active_tweens: Arc<Mutex<Vec<Tween>>>,
}

impl TweenService {
    pub fn create(&self, instance: Instance, info: TweenInfo, goals: Table) -> Tween {
        // Capture start values from instance
        // Create Tween object
    }

    pub fn update(&self, dt: f32) {
        // Called each tick
        // Update all active tweens
        // Interpolate properties
        // Fire Completed when done
    }
}
```

### Step 4: Wire into game loop

In `GameInstance::tick()` or `LuaRuntime::tick()`:

```rust
self.tween_service.update(dt);
```

### Step 5: Property interpolation

Support interpolating:
- `Position` (Vector3)
- `Size` (Vector3)
- `Color` (Color3)
- `Transparency` (number)
- `CFrame` (position + rotation)

Easing functions (MVP - just Linear and Quad):
```rust
fn ease(t: f32, style: EasingStyle, direction: EasingDirection) -> f32 {
    match (style, direction) {
        (EasingStyle::Linear, _) => t,
        (EasingStyle::Quad, EasingDirection::In) => t * t,
        (EasingStyle::Quad, EasingDirection::Out) => 1.0 - (1.0 - t) * (1.0 - t),
        (EasingStyle::Quad, EasingDirection::InOut) => {
            if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }
        }
        _ => t,
    }
}
```

## Files to Create/Modify

| File | Changes |
|------|---------|
| `src/game/lua/types/tween_info.rs` | New - TweenInfo struct |
| `src/game/lua/types/mod.rs` | Export TweenInfo |
| `src/game/lua/services/tween.rs` | New - TweenService, Tween |
| `src/game/lua/services/mod.rs` | Export TweenService |
| `src/game/lua/runtime.rs` | Register TweenService, call update() |

## Verification

```lua
local TweenService = game:GetService("TweenService")

local part = Instance.new("Part")
part.Position = Vector3.new(0, 5, 0)
part.Anchored = true
part.Parent = Workspace

local tween = TweenService:Create(part, TweenInfo.new(3), {
    Position = Vector3.new(100, 5, 0)
})

tween.Completed:Connect(function()
    print("Tween completed!")
end)

tween:Play()
-- Part should smoothly move from (0,5,0) to (100,5,0) over 3 seconds
```

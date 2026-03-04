# Minecraft — Agent Skill Guide

You are controlling a Minecraft player via the Clawblox HTTP API. This file describes what you can observe and what actions you can take.

---

## Observation (`GET /observe`)

```json
{
  "tick": 12345,
  "game_status": "active",
  "player": {
    "id": "<uuid>",
    "position": [x, y, z],
    "health": 20,
    "attributes": {
      "food": 20,
      "food_saturation": 5.0,
      "on_ground": true,
      "yaw": 0.0,
      "pitch": 0.0,
      "time_of_day": 6000,
      "is_day": true,
      "game_mode": "survival",
      "inventory": [{"name": "wooden_pickaxe", "count": 1}, ...],
      "nearby_blocks": [{"dx": 0, "dy": -1, "dz": 0, "name": "grass_block"}, ...]
    }
  },
  "other_players": [{"id": "<uuid>", "name": "Alice", "position": [x, y, z]}],
  "world": {
    "entities": [
      {"id": 42, "name": "zombie", "type": "mob", "position": [x, y, z], "attributes": {"health": 20}}
    ]
  },
  "events": [
    {"type": "Chat", "data": {"username": "Alice", "message": "Hello!"}},
    {"type": "Death", "data": {}},
    {"type": "BlockBroken", "data": {"position": {...}, "name": "oak_log"}},
    {"type": "HealthChanged", "data": {"health": 15, "food": 18}}
  ]
}
```

**Notes:**
- `position` is `[x, y, z]` in Minecraft world coordinates (Y is up)
- `nearby_blocks` is a 5×5×5 grid of non-air blocks around you, with offsets `dx/dy/dz`
- `time_of_day`: 0=midnight, 6000=sunrise, 12000=noon, 18000=sunset
- Events accumulate since last `/observe` call and are cleared on each read
- `other_players` visible within 64 blocks; `entities` visible within 16 blocks

---

## Actions (`POST /input`)

Send JSON body: `{"input_type": "<Type>", "data": {...}}`

### Navigation

**MoveTo** — Start moving toward a position (non-blocking, poll `/observe` for arrival)
```json
{"input_type": "MoveTo", "data": {"position": [x, y, z], "range": 1}}
```

**GoTo** — Move to a position and wait until arrival (blocking)
```json
{"input_type": "GoTo", "data": {"position": [x, y, z], "range": 1}}
```

**Stop** — Cancel movement
```json
{"input_type": "Stop", "data": {}}
```

**Jump** — Jump once
```json
{"input_type": "Jump", "data": {}}
```

**Look** — Face a direction or position
```json
{"input_type": "Look", "data": {"position": [x, y, z]}}
{"input_type": "Look", "data": {"yaw": 1.57, "pitch": 0.0}}
```

### Combat

**Attack** — Attack nearest hostile mob, or specific entity by id
```json
{"input_type": "Attack", "data": {}}
{"input_type": "Attack", "data": {"entity_id": 42}}
```

### Blocks

**DigBlock** — Break a block at position
```json
{"input_type": "DigBlock", "data": {"position": [x, y, z]}}
```

**PlaceBlock** — Place held block against a surface
```json
{"input_type": "PlaceBlock", "data": {"position": [x, y, z], "face": "top"}}
```
Face options: `top`, `bottom`, `north`, `south`, `east`, `west`

### Inventory

**EquipItem** — Equip an item from inventory
```json
{"input_type": "EquipItem", "data": {"item_name": "wooden_sword", "destination": "hand"}}
```
Destination options: `hand`, `off-hand`, `head`, `torso`, `legs`, `feet`

**UseItem** — Use/activate currently held item
```json
{"input_type": "UseItem", "data": {}}
```

**Eat** — Eat a food item from inventory
```json
{"input_type": "Eat", "data": {"item_name": "bread"}}
```

**Craft** — Craft an item if recipe materials are available
```json
{"input_type": "Craft", "data": {"item_name": "crafting_table", "count": 1}}
```

### Communication

**Chat** — Send a chat message
```json
{"input_type": "Chat", "data": {"message": "Hello, world!"}}
```

---

## Survival Tips

- **Food**: Keep food > 6 to regenerate health. Eat when food drops below 14.
- **Health**: At 0 health you die and respawn. Avoid fall damage (falling > 3 blocks).
- **Night**: At `time_of_day` > 13000 and < 23000 hostile mobs spawn outside. Find shelter or light the area.
- **Wood first**: Start by punching oak/birch trees to get logs. Craft a crafting table, then wooden tools.
- **Stone tools**: Mine stone (cobblestone) below Y=60 to upgrade from wood.
- **Coal and torches**: Find coal ore (black speckled stone) to craft torches for light.
- **Coordinates**: Y=64 is roughly sea level. Y < 16 is deep underground (diamonds, bedrock).

---

## Item Name Reference (common)

| Item | Minecraft name |
|------|---------------|
| Oak log | `oak_log` |
| Cobblestone | `cobblestone` |
| Dirt | `dirt` |
| Crafting table | `crafting_table` |
| Wooden pickaxe | `wooden_pickaxe` |
| Stone pickaxe | `stone_pickaxe` |
| Wooden sword | `wooden_sword` |
| Bread | `bread` |
| Apple | `apple` |
| Coal | `coal` |
| Iron ore | `iron_ore` |
| Diamond | `diamond` |

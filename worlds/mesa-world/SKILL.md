# Mesa World

## Actions

| Input | Data | Description |
|-------|------|-------------|
| MoveTo | `{"position": [x, y, z]}` | Move toward a world position |
| Jump | `{}` | Jump (works when grounded) |
| Stop | `{}` | Cancel current movement |
| Pickup | `{"position": [x, y, z]}` | Pick up the block nearest that world position, if it is within reach |
| Place | `{"position": [x, y, z]}` | Place the held block at that world position and let physics take over |

## Player Attributes

The player observation includes:

- `HoldingBlock`: `true` when the player is carrying a block.
- `HeldBlockColor`: The carried block color name (`Sand` or `Slate`), or `""`.
- `BlockSize`: The block edge length in studs (`0.75`).
- `PickupRange`: Max pickup reach in studs.
- `PlaceRange`: Max placement reach in studs.
- `LastBlockAction`: Last pickup/place result string such as `PickupSuccess` or `PlaceFailed:OutOfRange`.

## World Entity Attributes

Each block entity includes:

- `IsPickupBlock`: `true`
- `BlockColorName`: `Sand` or `Slate`

## API Endpoints

The game server runs on `http://localhost:8080` by default.

### Join Game

```
POST /join?name=MyAgent
```

**Response:**
```json
{
    "session": "session-token-uuid",
    "agent_id": "agent-uuid"
}
```

Save the `session` token — all other endpoints require it in the `X-Session` header.

### Send Input

```
POST /input
X-Session: <session-token>
Content-Type: application/json

{
    "type": "MoveTo",
    "data": {
        "position": [10, 0, 15]
    }
}
```

Returns the current observation after queuing the input.

### Get Observation

```
GET /observe
X-Session: <session-token>
```

**Response:**
```json
{
    "tick": 1234,
    "game_status": "active",
    "player": {
        "id": "uuid",
        "position": [5.2, 1.0, -3.1],
        "attributes": {}
    },
    "other_players": [],
    "world": {
        "entities": [
            {
                "id": 1,
                "name": "Ground",
                "position": [0, -0.5, 0],
                "size": [100, 1, 100],
                "anchored": true
            }
        ]
    },
    "events": []
}
```

### Get Game Skill

```
GET /skill.md
```

Returns this file as markdown text.

# Mesa World

## Actions

| Input | Data | Description |
|-------|------|-------------|
| MoveTo | `{"position": [x, y, z]}` | Move toward a world position |
| Jump | `{}` | Jump (works when grounded) |
| Stop | `{}` | Cancel current movement |

## World Layout

- **Ground**: A 100x100 teal platform centered at the origin (Y=0). Extends from X=-50 to X=50 and Z=-50 to Z=50.
- **Player**: Spawns at (0, 5, 0).

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

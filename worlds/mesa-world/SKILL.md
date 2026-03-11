# Mesa World

## Action Schemas

| Input | Data |
|-------|------|
| MoveTo | `{"position": [x, y, z]}` |
| Jump | `{}` |
| Stop | `{}` |
| Pickup | `{"position": [x, y, z]}` |
| Place | `{"position": [x, y, z]}` |


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

Returns an observation payload.

### Get Observation

```
GET /observe
X-Session: <session-token>
```

**Typical response shape:**
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

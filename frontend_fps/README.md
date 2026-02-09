# Clawblox FPS Spectator

WS-only Three.js spectator renderer for Clawblox games.

## Run

```bash
cd frontend_fps
npm install
npm run dev
```

Open one of:
- `http://localhost:5173/spectate/<game_id>`
- `http://localhost:5173/?game=<game_id>`

## Data Sources

- `GET /api/v1/games/{id}/spectate/ws` (primary world feed)
- `GET /api/v1/games/{id}/leaderboard` (HUD leaderboard)

This renderer does **not** use `/map`.

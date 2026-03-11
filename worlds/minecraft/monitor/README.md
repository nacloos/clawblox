# Clawblox Monitor

Read-only monitor for `pi` agents (launched via `run_pi_minecraft.sh`) with a
`shadcn/ui`-style frontend.

The backend reads session JSONL files from `~/.pi/agent/sessions` and exposes:

- `GET /api/agents` for active agent discovery
- `GET /api/events?agent=<id>` for recent events
- `WS /api/stream?agent=<id>` for live appended events
- Shared Minecraft viewer iframe (default `http://127.0.0.1:3007/`)

## Run

```bash
cd worlds/minecraft/monitor
npm install
npm start
```

`npm start` runs a frontend build automatically (`prestart`) and then starts the
Fastify server.

Open from another machine:

- `http://128.30.45.25:4310`

## Optional local UI dev

```bash
cd worlds/minecraft/monitor
npm run dev
```

## Environment

- `MONITOR_HOST` default `0.0.0.0`
- `MONITOR_PORT` default `4310`
- `PI_SESSIONS_DIR` default `~/.pi/agent/sessions`
- `VIEWER_URL` default `http://127.0.0.1:3007/`
- `POLL_MS` default `1000`

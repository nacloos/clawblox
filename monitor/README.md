# Clawblox Monitor

Live web UI for:
- Minecraft adapter events (`/ws`)
- tmux TUI streaming (pi agent, adapter logs, server console)

## What it shows

- Left panel: adapter event feed + quick agent stats
- Right panel: real tmux terminal rendering via xterm.js

## Run

```bash
cd monitor
npm install
npm start
```

Defaults:
- Monitor UI: `http://127.0.0.1:3000`
- Adapter HTTP: `http://127.0.0.1:8081`
- Adapter WS: `ws://127.0.0.1:8081/ws`

## Remote cluster usage (recommended)

Run monitor on cluster, then tunnel to your laptop.

On cluster:

```bash
cd /storage/nacloos/projects/clawblox/monitor
npm install
npm start
```

On your laptop:

```bash
ssh -N \
  -L 3000:127.0.0.1:3000 \
  -L 8081:127.0.0.1:8081 \
  -L 25565:127.0.0.1:25565 \
  nacloos@victoria.csail.mit.edu
```

Open:
- `http://localhost:3000`

## Terminal streaming details

- Endpoint: `GET /term?session=<tmux-session>&readonly=1|0`
- Uses `node-pty` to run `tmux attach -t <session>` inside a pseudo-terminal
- This preserves full-screen TUI behavior (cursor movement, alternate screen, ANSI)

## Event stream details

- Enter agent session token (`X-Session`) in UI
- UI opens adapter websocket:
  - `ws://<adapter-host>:8081/ws?session=<token>`

## Security note

By default monitor binds to `127.0.0.1`.
Keep it behind SSH tunnels for safety.

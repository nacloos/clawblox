# Minecraft Adapter

Connects an AI agent to Minecraft via HTTP, backed by [mineflayer](https://github.com/PrismarineJS/mineflayer).

This repo now has **two adapter modes**:

- `server.mjs` (legacy action-handler API)
- `server_eval.mjs` (thin eval API, recommended)

---

## Prerequisites

- Node.js 22+
- A running Minecraft Java server/world
- For local desktop play from WSL: set `MC_HOST` to your Windows host IP, not `127.0.0.1`

---

## Install

```bash
cd worlds/minecraft/minecraft-adapter
npm install
```

If you run the Pi launcher workflow, also install agent deps:

```bash
cd ../agent
npm install
```

---

## Mode A: Thin Eval Adapter (Recommended)

### Start

```bash
cd worlds/minecraft/minecraft-adapter
npm run start:eval
```

Or with explicit Minecraft target:

```bash
MC_HOST=172.30.112.1 MC_PORT=51234 MC_VERSION=1.21.1 npm run start:eval
```

### API

- `POST /join?name=<botName>` -> `{ session, agent_id }`
- `POST /input` with `X-Session` header and body:
  - `{ "code": "...javascript..." }`
- `POST /interrupt` with `X-Session` (cancel running `/input`, optional priority code)
- `GET /ws?session=<token>` (bot event stream)
- `GET /skill.md` (eval skill guide)
- `POST /leave` with `X-Session`

### `/input` example

```bash
curl -sS -X POST \
  -H "X-Session: <session>" \
  -H "Content-Type: application/json" \
  -d '{"code":"await bot.chat(\"hello\")"}' \
  http://localhost:8081/input
```

Returns:

- success: `{ "ok": true, "result": ... }`
- failure: `{ "ok": false, "error": "..." }`

---

## Mode B: Legacy Adapter

### Start

```bash
cd worlds/minecraft/minecraft-adapter
npm start
```

Legacy mode exposes fixed action handlers (`/observe`, `/input` with `input_type`).
Use this only if you specifically need handler-based compatibility.

---

## Recommended Agent Workflow (Pi + extension)

Use the launcher script from `agent/`:

```bash
cd ../agent
./run_pi_minecraft.sh
```

What it does:

- Calls `/join`
- Builds session prompt context
- Launches `pi`
- Uses autoloaded extension(s) from `~/.pi/agent/extensions`

To override workspace for a run:

```bash
WORKSPACE_DIR=/home/nacloos/Code/clawblox-minecraft/agent/workspace-agent2 ./run_pi_minecraft.sh
```

---

## Environment Variables

- `PORT` (default `8081`) adapter listen port
- `MC_HOST` (default `localhost`) Minecraft host
- `MC_PORT` (default `25565`) Minecraft port
- `MC_VERSION` (default `1.21.4`) protocol version
- `MC_AUTH` (default `offline`) mineflayer auth mode
- `EVAL_TIMEOUT_MS` (eval mode, default `30000`)
- `INTERRUPT_TIMEOUT_MS` (eval mode, default `5000`)

---

## Troubleshooting

### `Failed to decode packet 'serverbound/minecraft:hello'`

Usually caused by bot username or protocol mismatch:

- Ensure bot name is <= 16 chars
- Ensure `MC_VERSION` matches server exactly

### `ECONNREFUSED` to Minecraft host/port

- Wrong `MC_HOST` / `MC_PORT`
- For WSL + Windows Minecraft LAN, use Windows host IP and LAN port

### Adapter exits on eval errors

`server_eval.mjs` now has unhandled error guards; restart adapter if needed:

```bash
npm run start:eval
```

# Minecraft Example (One Command)

Minimal standalone world that uses Clawblox frontend runtime + prismarine viewer.
No `main.lua` is required.

## Run

```bash
clawblox run examples/minecraft --port 8080
```

This one command will:
- start Paper server (`mc-server/server.jar`, auto-download if missing)
- start minecraft adapter (`minecraft-adapter/server_eval.mjs`)
- start prismarine viewer on `http://127.0.0.1:3007/`
- start Clawblox runtime UI on `http://127.0.0.1:8080/`

Press `Ctrl+C` to stop everything.

## Config

Useful overrides:

```bash
MC_VERSION=1.20.1 MC_PORT=25565 ADAPTER_PORT=8081 VIEWER_PORT=3007 clawblox run examples/minecraft --port 8080
```

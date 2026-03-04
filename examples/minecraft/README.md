# Minecraft Example (One Command)

Minimal standalone world that uses Clawblox frontend runtime + prismarine viewer.
No `main.lua` is required.

## Run Without Clawblox (recommended for remote)

```bash
cd examples/minecraft
./run.sh
```

This does not call `clawblox` at all. It starts:
- Paper server (`mc-server/server.jar`)
- Minecraft adapter (`minecraft-adapter/server_eval.mjs`)
- Prismarine viewer (`http://127.0.0.1:3007/`)

Default version is `MC_VERSION=1.21.1`.
Default auth mode is `MC_ONLINE_MODE=false` (allows offline bot join).

Example overrides:

```bash
MC_VERSION=1.21.1 ./run.sh
MC_VERSION=1.21.1 MC_ONLINE_MODE=false ./run.sh
MC_VERSION=1.21.1 MC_ONLINE_MODE=true ./run.sh
MC_USE_LAN=1 MC_HOST=172.30.112.1 MC_PORT=51234 MC_VERSION=1.21.1 ./run.sh
```

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

# Minecraft Adapter

Connects an AI agent to a Minecraft server using the Clawblox agent API. The adapter runs a local HTTP server that translates between the Clawblox `/join`, `/observe`, and `/input` endpoints and a [mineflayer](https://github.com/PrismarineJS/mineflayer) bot.

The agent uses a locally running [Ollama](https://ollama.com) instance — no cloud API keys required.

---

## Prerequisites

- **Node.js 22+**
- **Ollama** installed and running locally
- A running **Minecraft Java Edition server** (version 1.21.4 recommended)

---

## Install

### 1. Install Ollama

Download and install from [ollama.com](https://ollama.com), then pull the model:

```bash
ollama pull lfm2.5-thinking:1.2b-bf16
```

Verify it's running:

```bash
ollama list
# should show lfm2.5-thinking:1.2b-bf16
```

### 2. Install adapter dependencies

```bash
cd minecraft-adapter
npm install
```

### 3. Install agent dependencies

```bash
cd ../agent
npm install
```

### 4. Configure the `.env` file

The `.env` file lives at the **repo root** (`clawblox/.env`). It should already exist with the right values if you followed the main setup. Verify it contains:

```env
PI_PROVIDER=ollama
PI_MODEL=lfm2.5-thinking:1.2b-bf16
# OLLAMA_BASE_URL=http://localhost:11434/v1  # default, uncomment to override
```

---

## Run

Open **three terminals**.

### Terminal 1 — Start Ollama (if not already running)

```bash
ollama serve
```

> Skip this if Ollama is already running as a background service.

### Terminal 2 — Start the Minecraft adapter

```bash
cd minecraft-adapter
npm start
```

By default the adapter connects to a Minecraft server at `localhost:25565`. Configure with environment variables:

```bash
MC_HOST=192.168.1.10 MC_PORT=25565 MC_VERSION=1.21.4 npm start
```

| Variable     | Default       | Description                          |
|--------------|---------------|--------------------------------------|
| `PORT`       | `8081`        | Adapter listen port                  |
| `MC_HOST`    | `localhost`   | Minecraft server host                |
| `MC_PORT`    | `25565`       | Minecraft server port                |
| `MC_VERSION` | `1.21.4`      | Minecraft protocol version           |
| `MC_AUTH`    | `offline`     | Auth mode: `offline` or `microsoft`  |

### Terminal 3 — Start the agent

```bash
cd agent
WORLD_BASE_URL=http://localhost:8081 npm start
```

The agent will join the Minecraft server as a bot, fetch the skill guide, and begin playing using the local Ollama model.

---

## Architecture

```
Ollama (local LLM)
      ↕
  agent/agent.mjs          ← dual-agent loop (speech + action)
      ↕  HTTP
minecraft-adapter/server.mjs  ← Clawblox API adapter
      ↕  mineflayer
Minecraft Java Server
```

- The **action agent** uses bash/curl to call `/observe` and `/input` endpoints
- The **speech agent** narrates what's happening in-world
- Both agents run on `lfm2.5-thinking:1.2b-bf16` via Ollama's OpenAI-compatible API at `http://localhost:11434/v1`

---

## Agent commands

Once the agent is running, type in the terminal:

| Command           | Description                            |
|-------------------|----------------------------------------|
| `/a <message>`    | Send a message to the action agent     |
| `/s <message>`    | Send a message to the speech agent     |
| `/reset`          | Clear conversation history             |
| `/session`        | Print the current session key          |
| `/quit`           | Exit                                   |

---

## Troubleshooting

**Bot fails to connect**
- Make sure your Minecraft server is running and the version matches `MC_VERSION`
- For offline mode servers, set `online-mode=false` in `server.properties`

**Ollama model not found**
- Run `ollama pull lfm2.5-thinking:1.2b-bf16` and wait for the download to finish
- Confirm with `ollama list`

**Agent errors with "No API key"**
- Confirm `PI_PROVIDER=ollama` is set in `clawblox/.env`

**Wrong Minecraft version**
- Set `MC_VERSION` to match the server exactly, e.g. `MC_VERSION=1.20.1 npm start`

# Minecraft Adapter

Connects an AI agent to a Minecraft server using the Clawblox agent API. The adapter runs a local HTTP server that translates between the Clawblox `/join`, `/observe`, and `/input` endpoints and a [mineflayer](https://github.com/PrismarineJS/mineflayer) bot.

The agent uses a locally running [Ollama](https://ollama.com) instance — no cloud API keys required.

---

## Prerequisites

- **Node.js 22+** (see installation below)
- **Ollama** installed and running locally
- A running **Minecraft Java Edition server** (version 1.21.4 recommended)

---

## Install

### 1. Install Node.js 22 via nvm

[nvm](https://github.com/nvm-sh/nvm) is the recommended way to manage Node.js versions.

```bash
NOTE: you might need to install npm with apt first
sudo apt install npm

# Install nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash

# Reload your shell (or open a new terminal)
source ~/.bashrc

# Install and activate Node.js 22
nvm install 22
nvm use 22

# Make v22 the default for all future shells
nvm alias default 22

# Verify
node --version   # should print v22.x.x
```

### 2. Install Ollama

Download and install from [ollama.com](https://ollama.com), then pull the model:

```bash
ollama pull lfm2.5-thinking:1.2b-bf16
```

Verify it's running:

```bash
ollama list
# should show lfm2.5-thinking:1.2b-bf16
```

### 3. Install Piper TTS (optional, for voice narration)

[Piper](https://github.com/rhasspy/piper) is a fast local text-to-speech engine. The speech agent will narrate in-game events as text regardless, but Piper enables audio output.

> **Note:** Do not use `sudo apt install piper` — that installs a mouse configuration tool, not the TTS engine.

```bash
# Download the piper binary into the agent folder
mkdir -p agent/piper
cd agent/piper
wget https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz
tar -xzf piper_linux_x86_64.tar.gz
rm piper_linux_x86_64.tar.gz
cd ../..

# Download a voice model (~63MB)
mkdir -p ~/.local/share/piper
cd ~/.local/share/piper
wget https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx
wget https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json
```

Test it works:

```bash
echo "hello world" | agent/piper/piper \
  --model ~/.local/share/piper/en_US-lessac-medium.onnx \
  --output_raw | ffplay -f s16le -ar 22050 -ac 1 -nodisp -autoexit -i pipe:0
```

### 4. Install adapter dependencies

```bash
cd minecraft-adapter
npm install
```

### 5. Install agent dependencies

```bash
cd ../agent
npm install
```

### 6. Configure the `.env` file

The `.env` file lives at the **repo root** (`clawblox/.env`). It should already exist with the right values if you followed the main setup. Verify it contains:

```env
PI_PROVIDER=ollama
PI_MODEL=lfm2.5-thinking:1.2b-bf16
# OLLAMA_BASE_URL=http://localhost:11434/v1  # default, uncomment to override

# Optional: bot name shown in-game (defaults to script name + process ID)
WORLD_AGENT_NAME=MyBot

# Optional: Piper TTS for audio narration
PIPER_BINARY=/path/to/clawblox/agent/piper/piper
PIPER_MODEL=/home/<user>/.local/share/piper/en_US-lessac-medium.onnx
# PIPER_SAMPLE_RATE=22050
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

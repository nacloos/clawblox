# Clawblox

<p align="center">
  <img src="static/logo.png" alt="Clawblox" width="120"/>
</p>

**Clawblox** is an open-source Roblox-like engine to easily create environments and games for embodied agents.

## Install

**macOS / Linux:**
```bash
curl -fsSL https://clawblox.com/install.sh | sh
```

**Windows (CMD):**
```cmd
curl -fsSL https://clawblox.com/install.cmd -o install.cmd && install.cmd && del install.cmd
```

**Windows (PowerShell):**
```powershell
irm https://clawblox.com/install.ps1 | iex
```

**From source:**
```bash
git clone https://github.com/nacloos/clawblox.git
cd clawblox
cargo install --path .
```

## Quick start

```bash
clawblox init my-game
cd my-game
clawblox run
# open http://localhost:8080
```

This creates a game with `world.toml` (config), `main.lua` (game logic), and `SKILL.md` (agent instructions).

## Features

- **Agent HTTP API**: LLM-friendly API to play games
- **Luau scripting**: Roblox-compatible API (work-in-progress)
- **3D physics**: Rapier3D engine
- **Custom renderers**: per-game Three.js renderers
- **DataStoreService**: persistent storage with local SQLite

## CLI commands

| Command | Description |
|---------|-------------|
| `clawblox init [name]` | Scaffold a new game |
| `clawblox run [path]` | Run locally (Ctrl+C to stop) |
| `clawblox start [path]` | Start in background |
| `clawblox stop` | Stop background server |
| `clawblox login` | Register/login |
| `clawblox docs` | View engine docs |

## Documentation

- [Scripting API](docs/scripting.md): Luau API reference
- [Agent API](docs/agent-api.md): HTTP API for AI agents
- [Custom renderers](docs/custom-renderers.md): Three.js renderer guide
- [Game design](docs/game-design.md): design guidelines


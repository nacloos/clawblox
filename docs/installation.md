---
title: Installation
summary: "Install the CLI and the tools needed to run agents."
read_when:
  - Setting up Clawblox for the first time
  - Checking what agent runs require
---

Install the Clawblox CLI first.

## macOS and Linux

```bash
curl -fsSL https://clawblox.com/install.sh | sh
```

Check that it works:

```bash
clawblox --version
```

## Windows

Command Prompt:

```cmd
curl -fsSL https://clawblox.com/install.cmd -o install.cmd && install.cmd && del install.cmd
```

PowerShell:

```powershell
irm https://clawblox.com/install.ps1 | iex
```

## World and agent requirements

The bundled world requires `esbuild` or Node.js with `npx` to build its renderer.
Viewing it requires a WebGL-capable browser and internet access to load browser
dependencies. The basic demo does not require a model-provider account.

To run agents, you also need:

- `tmux`
- one [coding agent](/agents#coding-agents) installed and logged in, such as Codex or Claude Code

The CLI command:

```bash
clawblox run <world> --agent codex
```

starts the world, then starts Codex inside a tmux session. The same pattern
applies to `--agent claude`.

## First check

After installing Clawblox, run a world without an agent:

```bash
clawblox run https://github.com/nacloos/clawblox/tree/main/worlds/mesa-island
```

Open:

```text
http://localhost:8080
```

Once that works, try running the same world with an agent:

```bash
clawblox run https://github.com/nacloos/clawblox/tree/main/worlds/mesa-island --agent codex
```

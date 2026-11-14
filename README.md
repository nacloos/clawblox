# Clawblox

<p align="center">
  <img src="static/logo.png" alt="Clawblox" width="120"/>
</p>

**Clawblox** is an open-source engine for building simulated worlds for embodied LLM agents. 

* engine core written in Rust with [Rapier](https://rapier.rs/) for 3D rigid-body physics
* worlds scripted in [Luau](https://luau.org/), a lightweight language developed
by Roblox, and natively support multi-agent
* simple **HTTP protocol** to embody coding agents in simulated worlds
* runs headless from the terminal for long-running experiments
* customizable [Three.js](https://threejs.org/) graphics rendering running in the browser

Paper: https://machineplaying.org

Data: [Paper supplementary data on Hugging Face](https://huggingface.co/datasets/nacloos/machine-playing-paper)

## Install

Running the bundled world requires `esbuild` or Node.js with `npx`. Viewing it
requires a WebGL-capable browser and internet access to load browser dependencies. Running
an agent additionally requires `tmux` and an installed, authenticated
[coding agent](docs/agents.md). Building from source requires Rust and Cargo;
dependency versions are recorded in `Cargo.lock`.

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
cargo install --locked --path .
```

## Quick start

### 1. Run the demo world

```bash
clawblox run https://github.com/nacloos/clawblox/tree/main/worlds/mesa-island
# open http://localhost:8080
```

From a source checkout or submission archive, use the bundled copy instead:

```bash
clawblox run worlds/mesa-island
```

Mesa Island provides the simulated demo input; no external dataset or
model-provider account is needed. The browser displays an island surrounded by
water and its world objects. The simulation runs until you press `Ctrl+C`.

To run a coding agent in the world, add `--agent codex` to either command.
See [Agents](docs/agents.md) for setup.

### 2. Create your own world

```bash
clawblox init my-game
cd my-game
clawblox run
# open http://localhost:8080
```

This creates a world with `world.toml` (config), `main.lua` (world logic), and
`API.md` (agent instructions). See [Creating worlds](docs/creating-worlds.md)
for how to customize them.

## CLI commands

| Command | Description |
|---------|-------------|
| `clawblox init [name]` | Scaffold a new world |
| `clawblox run [path]` | Run locally (Ctrl+C to stop) |
| `clawblox run [path] --resume <snapshot.json>` | Resume a world from a saved snapshot |
| `clawblox start [path]` | Start in background |
| `clawblox stop` | Stop background server |
| `clawblox login` | Register/login |
| `clawblox docs` | View engine docs |

## Documentation

- [Installation](docs/installation.md): install the CLI and prerequisites
- [Agents](docs/agents.md): run coding agents in worlds
- [Worlds](docs/worlds.md): worlds and the world protocol
- [Scripting](docs/scripting.md): Luau scripting guide
- [Custom renderers](docs/custom-renderers.md): Three.js renderer guide
- [Running worlds](docs/running.md): local runs, recording, and resume

## Run on your own world

Clawblox isn't tied to the built-in engine. Any server that implements the world
protocol (join, observe, and input over HTTP) can be a Clawblox world, and agents
connect to it the same way. `worlds/mesa-island` is a complete reference
implementation you can study or copy from.

See [Worlds](docs/worlds.md) for the protocol and
[World interface](docs/world-interface.md) for the implementer contract, including
the launch environment and snapshot/resume requirements.

---
Please cite as:

```
@misc{cloos2026machineplaying,
      title={Is this machine playing?}, 
      author={Nathan Cloos and Antonio Norelli and Daniel Durbin and Jacob Andreas and Daniela Rus and Phillip Isola},
      year={2026},
      eprint={2610.07130},
      archivePrefix={arXiv},
      primaryClass={cs.AI},
      url={https://arxiv.org/abs/2610.07130}, 
}
```

---

This research was sponsored by the Department of the Air Force Artificial Intelligence Accelerator and was
accomplished under Cooperative Agreement Number FA8750-19-2-1000. The views and conclusions
contained in this document are those of the authors and should not be interpreted as representing
the official policies, either expressed or implied, of the Department of the Air Force or the U.S.
Government. The U.S. Government is authorized to reproduce and distribute reprints for Government
purposes notwithstanding any copyright notation herein.

License: [MIT](LICENSE).

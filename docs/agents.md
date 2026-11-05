---
title: Agents
summary: "Run coding agents as embodied agents in worlds, and where their state lives."
read_when:
  - Running an agent in a world
  - Choosing a coding agent or inspecting agent state
---

Clawblox runs long-lived agents in interactive worlds. The agents are existing
coding agents such as Codex and Claude Code; Clawblox handles the world
connection, workspace, session files, logs, and resume and checkpoint plumbing
around them.

An agent has a name, a workspace, session files, transcripts, logs, and a
connection to a world. Clawblox keeps those pieces together so runs are easy to
inspect, continue, checkpoint, and compare. Agents are not tied to the Clawblox
engine: they can run in any world that implements the [world protocol](/worlds).

## Coding agents

A coding agent is a tool like Codex, Claude Code, or Kimi: a CLI that already
knows how to talk to its model provider. Clawblox starts and manages it; it does
not replace it.

Supported coding agents:

```text
codex     Codex CLI
claude    Claude Code
kimi      Kimi Code
```

Install and log in to the coding agent you want before running it. You also need
`tmux`: Clawblox starts the coding agent inside a tmux session so it keeps
running and can be inspected.

## Running an agent

The quickest way is to pass `--agent` when starting a world:

```bash
clawblox run <world> --agent codex
```

This starts the world, then starts a managed agent connected to it. Useful
options:

```bash
clawblox run <world> \
  --agent codex \
  --agent-name Rocky \
  --model gpt-5.5 \
  --agent-dir runs/rocky \
  --sandbox
```

Use Python to orchestrate worlds and agents from a script:

```python
from pathlib import Path
from clawblox import Agent, World, default_system_prompt

world = World(dir="worlds/mesa-island")
world.start(port=8080, record=True, record_dir=Path("runs/r001/recordings"))

agent = Agent(
    agent="codex",          # which coding agent
    name="Rocky",           # this agent's name
    dir=Path("runs/r001/agents/rocky"),
    model="gpt-5.5",
    sandbox=True,
)

access = world.connect(agent=agent)
agent.start(
    initial_prompt="Begin.",
    system_prompt=default_system_prompt(),
)
```

`world.connect(agent=agent)` joins the world, writes the world session id into
the agent directory, and sets the environment the agent needs. The agent then
reads the world's [protocol](/worlds#the-world-protocol) to learn how to observe
and act.

## Sandbox

Pass `--sandbox` (CLI) or `sandbox=True` (Python) to run the coding agent in a
restricted sandbox. A world can declare its sandbox policy in `world.toml`:

```toml
[sandbox]
deps_root = "/opt/conda"
enable_weaker_nested_sandbox = true
add_deps_lib_to_library_path = true
```

`deps_root` is mounted read-only at `/sandbox-deps` for sandboxed agents.
Absolute dependency prefixes are also mounted read-only at their original path,
so console scripts whose `#!` line points to an absolute interpreter path keep
working. When
`add_deps_lib_to_library_path` is true, Clawblox adds `/sandbox-deps/lib` to
`LD_LIBRARY_PATH`; disable it if the prefix has libraries that conflict with
system tools. `enable_weaker_nested_sandbox` is only for unprivileged
Docker/WSL2 containers where standard `bubblewrap` sandboxing is unavailable;
leave it unset on normal hosts.

`WORLD_OPERATOR_TOKEN` must never reach an agent. When a launcher starts the
world and the agents in the same tmux server, pass the token only to the
world's window (for example with `tmux new-window -e`), not through the tmux
client's or server's global environment. A window created later inherits the
server's global environment, so an agent window can otherwise see the token
with `env`.

## Multiple agents

Use the Python `World` and `Agent` APIs above to connect multiple agents, each
with its own name and directory, to a running world. Set `max_players` in the
world's `world.toml` to accommodate them; the bundled Mesa world defaults to
one player. See [Checkpoints](/checkpoints) for saving and resuming world and
agent state together.

## Workspace and memory

Each agent has an agent directory that keeps everything needed to inspect and
continue it:

```text
agent-dir/
  workspace/         persistent working area and memory files
  runtime/           generated system prompt and rerunnable command script
  logs/              agent and runtime logs
  session/           provider continuation state and transcripts
  home/              agent home/config directory
  events.jsonl       structured operation log
  world_session.txt  world session id
```

`workspace/` is the agent's persistent working area. Memory files live there,
along with anything the agent creates while acting in the world. The default
prompt expects files such as:

```text
SOUL.md
SEMANTIC_MEMORY.md
EPISODIC_MEMORY.md
```

World-specific templates can seed these files. `session/` stores provider
continuation state and transcripts when the coding agent supports them.

The workspace does not depend on which coding agent you used, which makes it the
safest part of an agent's state to share.

Some coding agents keep memory outside `workspace/`. Claude Code 2.1.x has an
auto-memory directory under the agent's home
(`home/.claude/projects/*/memory/`) that is loaded into every session. It is a
second memory channel next to the workspace files. For experiments that study
memory, either include it when you snapshot or compare agents, or turn it off
with `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`. See [Checkpoints](/checkpoints) to package world and agent state together,
and [Agent integration](/agent-interface) to add support for a new coding agent.

---
title: World configuration
summary: "The world.toml fields engine worlds use."
read_when:
  - Configuring a world
  - Looking up a world.toml field
---

`world.toml` tells Clawblox how to load a world. The same manifest can describe
an engine world or an external-process world, but this page focuses on the
built-in engine.

## Minimal config

```toml
name = "My World"
```

Defaults are filled in by the engine:

- `max_players = 8`
- `afk_timeout = 300`
- `runtime.mode = "clawblox"`
- `scripts.main = "main.lua"`
- `scripts.api = "API.md"`
- `renderer.name = "World Renderer"`
- `renderer.mode = "module"`
- `renderer.api_version = 1`
- `observation.radius = 100`
- `speech.mode = "global"`
- `speech.hearing_radius = 30`
- `clawblox.player_object_physics = true`
- `clawblox.physics.solver_iterations = 4`

## Basic fields

```toml
name = "Mesa World"
description = "A simple mesa world with a player and ground."
max_players = 1
afk_timeout = 0
```

`afk_timeout` is in seconds. `0` disables the idle timeout.

## Runtime

```toml
[runtime]
mode = "clawblox"
```

`clawblox` runs the built-in Luau engine. `external` is used when Clawblox is
serving the frontend/renderer around another world process.

## Scripts

```toml
[scripts]
main = "main.lua"
tree = "scripts"
api = "API.md"
```

`main` is relative to the world directory.

`tree` is optional and points to a script-tree directory.

`api` points to the agent-facing docs file, usually `API.md`.

## Renderer

```toml
[renderer]
name = "Mesa World Renderer"
mode = "module"
api_version = 1
entry = "dist/renderer.bundle.js"
source = "src/index.js"
capabilities = ["three-sdk"]
```

`entry` and `source` are relative to the `renderer/` directory.

`entry` is the manifest-level built module path. Current local engine runs use
`renderer/package.json` `clawbloxRenderer.outFile` as the build output that is
served in `/renderer/manifest`; keep the two paths aligned.

`source` is optional manifest metadata. Current local engine runs detect source
from `renderer/package.json` `clawbloxRenderer.entry` or common files such as
`renderer/src/index.js`; keep `source` aligned if you use it for tooling.

`name`, `api_version`, and `capabilities` are used as fallbacks when
`renderer/package.json` does not declare those fields.

See [Rendering and frontend](/rendering-and-frontend).

## Observation and speech

```toml
[observation]
radius = 40000

[speech]
mode = "proximity"
hearing_radius = 24
```

`observation.radius` must be a finite number greater than `0`.

`speech.mode` can be `global` or `proximity`.

`speech.hearing_radius` must be a finite number greater than `0`.

## Engine physics

```toml
[clawblox]
player_object_physics = true

[clawblox.physics]
solver_iterations = 4
```

`solver_iterations` must be between `1` and `32`.

`player_object_physics = false` is the mesa-island compatibility mode. It
reproduces the character physics used for the paper runs (branch
`nacloos/mesa-island`): characters don't push or ride loose parts, and the
later character-motion changes (wedge recovery, terminal fall speed, the
vertical-velocity resets, support load) are off. Parts released from
`Anchored` keep the motion they had, as before. Use it to compare against
the paper's results or replay its recordings.

## Agent API and sandbox

```toml
[agent_api]
allow_reset = false
vision_observation = false
state_observation = true
action_feedback = true

[sandbox]
enable_weaker_nested_sandbox = false
deps_root = "/opt/conda"
add_deps_lib_to_library_path = true
```

`agent_api.allow_reset` controls whether engine reset endpoints are available to
agent sessions.

`agent_api.vision_observation` enables `GET /observe/image`: first-person PNG
frames rendered through the world's own renderer in a headless-browser sidecar.
The legacy key `vision` is still accepted as an alias. See
[Observations and input](/observations-and-input) for details. Off by default.

`agent_api.state_observation` controls structured JSON state from `GET /observe`
and action responses. It defaults to `true`. If it is `false`,
`vision_observation` must be enabled.

`agent_api.action_feedback` controls whether `POST /input` returns a narrow
non-spatial action result when state observation is disabled. It defaults to
`true`.

Sandbox settings control dependency mounts and nested sandbox behavior.

## External command

```toml
[run]
command = ["custom-runner", "--flag"]
```

`run.command` delegates local startup to an external command. For pure engine
worlds, leave this unset.

## Validation

The loader validates `world.toml` before starting the world:

- `observation.radius` must be finite and greater than `0`
- `speech.hearing_radius` must be finite and greater than `0`
- `run.command`, when set, must contain at least one token and a non-empty
  executable name
- `sandbox.deps_root`, when set, must be a non-empty path
- `agent_api.state_observation = false` requires
  `agent_api.vision_observation = true`
- `clawblox.physics.solver_iterations` must be between `1` and `32`

For engine worlds, treat `[run]` and `runtime.mode = "external"` as the
boundary to the external-process runtime. The engine pages document the
`runtime.mode = "clawblox"` path.

## Configuration precedence

`world.toml` is read once when `clawblox run` starts. Missing fields use engine
defaults, and command-line flags can still affect local runtime behavior around
the world:

- `--port` changes the local HTTP port and the default SQLite datastore file
  name.
- `--datastore sqlite|postgres|none` selects the local `DataStoreService`
  backend.
- `--resume path/to/snapshot.json` restores engine state after the world config
  and scripts have loaded.
- `--agent`, `--agent-name`, and related agent flags start a coding agent after
  the local world runtime is available.

Keep world behavior in `world.toml` and Luau scripts. Treat CLI flags as
operator controls for the current run.

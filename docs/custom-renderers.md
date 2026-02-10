# Custom Renderers (Local CLI)

`clawblox run` now includes an embedded frontend and supports per-game custom renderers.

## File layout

```text
my-game/
  world.toml
  main.lua
  renderer/
    index.js
```

## world.toml

```toml
[renderer]
name = "Default Game Renderer"
mode = "module"
api_version = 1
entry = "index.js"
capabilities = []
```

- `entry` is relative to `renderer/`
- If missing or invalid, CLI falls back to the embedded default renderer

## Renderer contract (api_version = 1)

```js
export function createRenderer(ctx) {
  return {
    mount() {},
    unmount() {},
    onResize({ width, height }) {},
    onState(state) {},
  }
}
```

`ctx` fields:

- `apiVersion`: host renderer API version
- `canvas`: fullscreen render target
- `log(level, message, data?)`: host-integrated logger

## Runtime endpoints

- `GET /` - local frontend
- `GET /renderer/manifest` - renderer metadata
- `GET /renderer-files/*` - renderer static files from game `renderer/`
- `GET /spectate/ws` - live spectator observation stream

## Notes

- Local dev mode is intentionally permissive for iteration speed.
- Hosted platform runtime should enforce stronger UGC isolation and capability checks.

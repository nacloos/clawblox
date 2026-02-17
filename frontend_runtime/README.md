# frontend_runtime

Embedded local frontend for `clawblox run`.

- Served from the CLI binary at `/`
- Loads renderer metadata from `GET /renderer/manifest`
- Loads custom renderer modules from `renderer/` via `/renderer-files/*`

## Renderer API

```js
export function createRenderer(ctx) {
  return {
    mount() {},
    unmount() {},
    onResize({ width, height, dpr }) {},
    onState(state) {},
  }
}
```

## SDK namespaces

- `ctx.runtime.state` - snapshot interpolation + indexing
- `ctx.runtime.animation` - animation-track inspection
- `ctx.runtime.presets` - lightweight preset registry
- `ctx.runtime.three` - Three.js lifecycle/camera/material/entity helpers
- `ctx.runtime.input` - local `join/input/observe` bridge + keyboard mapping

These helpers are designed to support renderer complexity similar to FPS/FallGuys frontends.

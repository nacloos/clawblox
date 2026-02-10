# frontend_runtime

Embedded local frontend for `clawblox run`.

- Served directly from the CLI binary at `/`
- Loads renderer metadata from `GET /renderer/manifest`
- Loads custom renderer modules from `renderer/` via `/renderer-files/*`

Renderer contract (`api_version = 1`):

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

`ctx` includes:

- `apiVersion`
- `canvas`
- `log(level, message, data?)`

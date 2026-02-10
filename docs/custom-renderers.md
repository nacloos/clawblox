# Custom Renderers (Local CLI)

`clawblox run` includes an embedded frontend and supports per-game custom renderers.

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
capabilities = ["presets", "animation-tracks", "three-sdk", "input-bridge"]
```

- `entry` is relative to `renderer/`
- If missing/invalid, runtime falls back to built-in default renderer.

## Renderer contract (api_version = 1)

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

## `ctx` runtime SDK

`ctx` contains:

- `apiVersion`
- `manifest`
- `canvas`
- `log(level, message, data?)`
- `runtime`

### Core

- `runtime.state.createSnapshotBuffer({ maxSnapshots, interpolationDelayMs })`
- `runtime.state.indexById(items)`
- `runtime.animation.findTrack(player, predicate)`
- `runtime.animation.hasTrackMatching(player, /regex/)`
- `runtime.animation.mapPlayersByRootPart(players)`
- `runtime.presets.createPresetRegistry(initial)`

### Three.js (`runtime.three`)

- `createFollowCameraController(THREE, camera, options)`
- `createCameraModeController(THREE, camera, options)`
- `createPresetMaterialLibrary(initial)`
- `materialFromRender(THREE, render, presetLib?)`
- `geometryFromRender(THREE, render, size)`
- `buildEntityMesh(THREE, entity, presetLib?)`
- `applyEntityTransform(THREE, object3d, entity)`
- `createEntityStore(scene, options?)` (upsert/prune/dispose)
- `disposeObject3D(object3d)`
- `createModelTemplateCache()`
- `classifyAnimationTracks(player)`
- `applyRendererPreset(THREE, renderer, preset)`
- `applyLightingPreset(THREE, scene, preset)`

### Local input bridge (`runtime.input`)

- `createLocalInputClient({ baseUrl, playerName })`
  - Uses local `/join`, `/input`, `/observe`
- `bindKeyboardActions(inputClient, bindings, options?)`
  - Key-to-action mapping with tap/hold modes

Example:

```js
const input = ctx.runtime.input.createLocalInputClient({ playerName: 'render-bot' })
ctx.runtime.input.bindKeyboardActions(input, {
  KeyW: { mode: 'hold', type: 'MoveForward', data: {} },
  Space: { mode: 'tap', type: 'Jump', data: {} },
})
```

## Runtime endpoints

- `GET /` - local frontend host
- `GET /renderer/manifest` - renderer metadata
- `GET /renderer-files/*` - static files from game `renderer/`
- `GET /spectate/ws` - live spectator observation stream

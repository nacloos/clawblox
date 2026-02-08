# Plan: Cascaded Shadow Maps (CSM) — Roblox-style shadows

## Context

The current single shadow map approach can't cover the full visible area without sacrificing quality. Roblox, Unity, and Unreal all solve this with **Cascaded Shadow Maps**: multiple shadow maps covering near/mid/far zones at different resolutions. Three.js has a built-in CSM addon (`three/examples/jsm/csm/CSM.js`) already installed in the project (three ^0.160.0).

**Key CSM behavior** (from reading the source):
- Creates N directional lights (one per cascade), adds them to the scene
- Splits the camera frustum into cascades using practical/logarithmic/uniform modes
- Each frame, `csm.update()` repositions each cascade light to cover its frustum slice, with texel-grid snapping built in (no shadow swimming)
- Materials need `csm.setupMaterial(material)` called once — this injects `onBeforeCompile` with cascade uniforms
- `csm.dispose()` / `csm.remove()` for cleanup

## Architecture

### New file: `frontend/src/components/CSMProvider.tsx`

A React context + component that manages the CSM instance:

```tsx
// CSMContext provides the CSM instance to child components
const CSMContext = createContext<CSM | null>(null)
export const useCSM = () => useContext(CSMContext)
```

**`CSMManager` component** (inside Canvas):
- Creates CSM instance with `useThree()` camera + scene
- Calls `csm.update()` every frame via `useFrame`
- Disposes on unmount
- Provides CSM via context

**`CSMMaterial` component** — a thin wrapper:
- Takes same props as `<meshStandardMaterial>`
- Creates the material imperatively, calls `csm.setupMaterial()` once
- Returns via `<primitive object={material} attach="material" />`

This keeps Entity.tsx changes minimal — just swap `<meshStandardMaterial {...props} />` → `<CSMMaterial {...props} />`.

### File: `frontend/src/components/GameScene.tsx`

- Remove `ShadowLight` component entirely
- Remove shadow constants (CSM handles all of this)
- Keep `shadows="soft"` on Canvas
- Remove the static directional light (CSM creates its own)
- Wrap scene content in `<CSMManager>` context provider
- Keep `lookAtRef` sharing (not needed for CSM — remove it)
- Remove `lookAtRef` prop from CameraController

CSM config constants:
```ts
const CSM_CASCADES = 3
const CSM_SHADOW_MAP_SIZE = 2048
const CSM_MAX_FAR = 500
const CSM_LIGHT_DIRECTION = new THREE.Vector3(-1, -2, -1).normalize()
const CSM_SHADOW_BIAS = -0.0005
const CSM_LIGHT_INTENSITY = 1.2
const CSM_LIGHT_MARGIN = 200
```

### File: `frontend/src/components/Entity.tsx`

- Import `CSMMaterial` from CSMProvider
- In `renderPrimitiveMesh()`: replace `<meshStandardMaterial {...materialProps} />` with `<CSMMaterial {...materialProps} />`
- In `ModelMesh` GLTF traverse: call `csm.setupMaterial(obj.material)` on each mesh material (use `useCSM()` hook)
- Keep transparent material shadow logic (Glass/Ice/ForceField don't cast shadows)

### File changes summary

| File | Change |
|------|--------|
| `CSMProvider.tsx` (new) | CSM context, `CSMManager`, `CSMMaterial` component |
| `GameScene.tsx` | Remove `ShadowLight`, shadow constants, `lookAtRef`; add `<CSMManager>` wrapper |
| `Entity.tsx` | Use `<CSMMaterial>` for primitives; call `csm.setupMaterial()` in GLTF traverse |

## Verification

1. Run frontend dev server, open a running game
2. **Full coverage**: all visible objects have shadows at every zoom level
3. **Zoom in**: near shadows are crisp (small cascade, high texel density)
4. **Zoom out**: far shadows still visible (larger cascade covers distance)
5. **No swimming/jittering** when panning (CSM has built-in texel snapping)
6. **Transparent parts**: Glass/Ice/ForceField don't cast shadows
7. **No shadow acne** on flat surfaces
8. **Performance**: check FPS — 3 cascades at 2048 should be fine for web

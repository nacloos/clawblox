# Deterministic Map Rendering Contract (No Heuristics)

## Summary
Build a strict, Roblox-like replication pipeline where visuals are driven entirely by explicit server-authored metadata.
No client inference from names/shapes/material strings.
The frontend uses one shared renderer and one canonical data source:
1. unified replication stream from `/games/{id}/spectate/ws` (initial full snapshot + incremental updates)
2. visual preset registry authored in Three.js, selected by server-provided preset IDs

Policy: **Strict v2**. Missing required render metadata is treated as an error in dev/spectator views.

Roblox-aligned model: server owns the instance tree and replicated properties; client renders from replicated properties, not heuristics.

## Important API / Interface Changes

### 1) Explicit render payload (`RenderEntityV2`)
Add a required object on each world entity:
- `render.kind`: `preset | primitive | light | ui_anchor`
- `render.role`: semantic role, e.g. `floor | wall | pillar | crate | platform | trim | spawn | dynamic_prop`
- `render.preset_id`: stable visual preset ID (required when `kind=preset`)
- `render.primitive`: `box | cylinder | sphere | wedge` (required when `kind=primitive`)
- `render.material`: engine enum string (required)
- `render.color`: `[r,g,b]` (required)
- `render.static`: `true|false` (required)
- `render.casts_shadow`: bool
- `render.receives_shadow`: bool
- `render.visible`: bool
- `render.double_sided`: bool
- `render.transparency`: float

Keep current legacy top-level fields temporarily, but frontend v2 path does not consume them.

### 1.1) Preset registry contract (frontend-authored, server-selected)
- Frontend owns a versioned preset catalog (Three.js code), e.g.:
  - `enemy_grunt_v1`
  - `enemy_fast_v1`
  - `arena_wall_concrete_v1`
- Server/Lua never sends ad-hoc visual instructions for these entities; it sends `preset_id` + runtime state.
- Preset IDs are immutable once published; changes create a new ID version.

### 2) Unified replication contract
- `/spectate/ws` is the single source for static + dynamic world state and players
- first message: full snapshot (`mode=full`)
- subsequent messages: delta updates (`mode=delta`) with upserts/removals
- server defines ordering/versioning so clients apply deltas deterministically

### 3) Local dev parity
Local CLI `spectate/ws` must match production payload shape and sequencing guarantees.

### 4) Lua authoring contract
For map parts created in game scripts, set attributes (or helper constructor API) for:
- `RenderRole`
- `RenderStatic`
- `RenderPresetId` (preferred), or `RenderPrimitive` (fallback only for simple blocks)
- `RenderMaterial`
- `RenderColor`

No renderer-side guessing by `Name` or fallback from `shape`.

### 5) Frontend renderer architecture
Create one shared renderer module used by both `reference-map` and `server-map`:
- `createScenePipeline()` (lights/fog/tonemapping/textures)
- `upsertEntity(RenderEntityV2)`
- `removeEntity(id)`
- `resolvePreset(preset_id, entity_state)`

Both viewers call the same renderer; only data source differs.

## Implementation Plan

### Phase A: Contract Definition
1. Add `RenderEntityV2` Rust types and serde schema.
2. Add validation in observation build path.
   - In strict mode, reject entities missing required `render.*`.
3. Add explicit conversion from Lua instance metadata to `render.*`.
4. Add preset-id validation against a published preset manifest version.

### Phase B: Server Data Flow
1. Update map builder/scripts to attach explicit render metadata to every authored part.
2. Update spectator observation builders to emit v2 render objects for all replicated entities.
   - Enemies replicate `preset_id` + animation state + FX state (no visual inference)
3. Add unified WS envelope:
   - `mode: full | delta`
   - `tick`
   - `upserts[]`
   - `removes[]`
4. Ensure first packet for a subscriber is always `full`, then `delta` only.

### Phase C: Frontend Refactor
1. Build `src/render/core.ts` (shared scene + entity renderer) and `src/render/presets/*` (catalog).
2. Replace `reference-map.ts` and `server-map.ts` internals with shared renderer calls.
3. Remove all heuristics.
   - no `name.includes('pillar')`
   - no shape inference
   - no ceiling auto-hide heuristic
4. Keep free-camera controls as a separate reusable module.
5. Implement preset resolver and strict fallback behavior:
   - missing preset => hard error overlay in strict mode

### Phase D: Diagnostics
1. Add strict validation overlay.
   - invalid entity count
   - first N contract violations
   - fail render in strict mode if violations exist
2. Add structured map signature logs.
   - role/material/primitive counts
   - bounds
   - deterministic entity signature

## Test Cases and Scenarios

1. **Schema validation**
- entity missing `render.role` => strict mode error
- entity missing `render.material` => strict mode error

2. **Static/dynamic routing**
- full WS snapshot includes static map + dynamic entities
- delta WS updates include only changed/created/removed entities

3. **Renderer determinism**
- same input snapshot rendered by `reference-map` and `server-map` pipelines yields identical role/material/primitive counts and bounds
- same `preset_id` + state yields identical visuals across sessions

4. **No-heuristics guarantee**
- rename `Pillar` entity to `Foo123`; visuals remain correct because role/primitive drive rendering
- swap enemy display name; visuals unchanged because `preset_id` drives rendering

5. **Local/prod parity**
- `spectate/ws` behavior is consistent on local CLI and API server

## Assumptions and Defaults
- Mode: **Strict v2**.
- Data flow: **Single WS replication stream**.
- Visual authoring: **Three.js preset registry**, server-authoritative selection by preset ID.
- Legacy fields remain serialized short-term for compatibility, but are not used by v2 frontend.
- "Exactly same" means identical render pipeline + identical entity contract, not separate hand-tuned pages.

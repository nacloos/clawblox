# Minecraft Eval Adapter Skill

This adapter exposes a thin interface:

- `POST /join?name=<botName>`
- `POST /input` (requires `X-Session`)
- `POST /leave` (requires `X-Session`)

For autonomous loops, you only need:

- `POST /join` once
- then `POST /input` repeatedly

## Input Contract

Send JavaScript directly:

```json
{ "code": "await bot.chat('hello from eval adapter')" }
```

The code runs as async JS with these variables in scope:

- `bot`: mineflayer bot instance
- `session`: current session object
- `goals`: `mineflayer-pathfinder` goals
- `Vec3`: vec3 constructor from bot position
- `sleep(ms)`: async sleep helper
- `log(...args)`: server log helper
- `now()`: current timestamp in ms

Return values are serialized as JSON in:

```json
{ "ok": true, "result": ... }
```

Errors:

```json
{ "ok": false, "error": "..." }
```

## Examples

Chat:

```json
{ "code": "await bot.chat('hi')" }
```

Look at nearest entity:

```json
{
  "code": "const e = Object.values(bot.entities).find(x => x && x.position && x !== bot.entity); if (e) await bot.lookAt(e.position.offset(0, 1.5, 0)); return { lookedAt: e?.name ?? null };"
}
```

Pathfind near coordinates:

```json
{
  "code": "const { GoalNear } = goals; await bot.pathfinder.goto(new GoalNear(-82, 64, 383, 1)); return 'arrived';"
}
```

Dig targeted block:

```json
{
  "code": "const p = new Vec3(-82, 64, 383); const block = bot.blockAt(p); if (!block) throw new Error('No block'); await bot.dig(block); return { dug: block.name };"
}
```

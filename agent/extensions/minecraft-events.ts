/**
 * Minecraft Mineflayer Survival Extension
 *
 * Uses WebSocket for real-time bot events (no polling, no blocked-during-tools).
 * Uses POST /interrupt to cancel running tools and execute emergency actions.
 * Uses POST /input for normal strategic tools.
 */

import type { ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";

const BRIDGE_URL = "http://localhost:8081";

let session = "";
let ws: InstanceType<typeof WebSocket> | null = null;
let lastHealth = 20;
let lastFood = 20;
let cooldownUntil = 0;
let extensionCtx: ExtensionContext | null = null;
let piRef: ExtensionAPI | null = null;
let autoLoopEnabled = true;
const AUTO_LOOP_STEER = "continue";
let justDied = false;

// ─── Between-Action Event Log ───
// Accumulates significant events between tool calls so the agent
// gets a recap of what happened while it was "thinking".
interface EventLogEntry { time: number; msg: string; }
let eventLog: EventLogEntry[] = [];
const MAX_EVENT_LOG = 20;

function logEvent(msg: string) {
  eventLog.push({ time: Date.now(), msg });
  if (eventLog.length > MAX_EVENT_LOG) eventLog.shift();
}

function flushEventLog(): string | null {
  if (eventLog.length === 0) return null;
  const lines = eventLog.map(e => `- ${e.msg}`).join("\n");
  eventLog = [];
  return lines;
}

/** Wrap a tool result text with any accumulated events since last tool call */
function withEventRecap(text: string): string {
  const recap = flushEventLog();
  if (!recap) return text;
  return `[Events since last action]\n${recap}\n\n${text}`;
}

/** Build a standard tool result, prepending event recap */
function toolResult(text: string) {
  return { content: [{ type: "text" as const, text: withEventRecap(text) }] };
}

/** Build tool result from mc() response */
function mcResult(resp: { ok: boolean; result?: any; error?: string }, pretty = true) {
  const text = resp.ok
    ? (pretty ? JSON.stringify(resp.result, null, 2) : JSON.stringify(resp.result))
    : `Error: ${resp.error}`;
  return toolResult(text);
}

// ─── Bridge Communication ───

async function mc(code: string, timeout = 35000): Promise<{ ok: boolean; result?: any; error?: string }> {
  if (!session) return { ok: false, error: "No session" };
  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeout);
    const resp = await fetch(`${BRIDGE_URL}/input`, {
      method: "POST",
      headers: { "X-Session": session, "Content-Type": "application/json" },
      body: JSON.stringify({ code }),
      signal: controller.signal,
    });
    clearTimeout(timer);
    return (await resp.json()) as any;
  } catch (err: any) {
    const msg = err?.name === "AbortError" ? "Execution timed out" : "Bridge unreachable";
    return { ok: false, error: msg };
  }
}

/** Interrupt: cancels running /input, stops bot, runs priority code */
async function interrupt(code?: string): Promise<{ ok: boolean; result?: any; error?: string }> {
  if (!session) return { ok: false, error: "No session" };
  try {
    const body: any = {};
    if (code) body.code = code;
    const resp = await fetch(`${BRIDGE_URL}/interrupt`, {
      method: "POST",
      headers: { "X-Session": session, "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    return (await resp.json()) as any;
  } catch {
    return { ok: false, error: "Bridge unreachable" };
  }
}

// ─── Auto-Reflex Actions (via /interrupt, never blocked) ───

// Flee is SHORT — only 1.5 seconds sprint, ~15 blocks away, not 200
const FLEE_CODE = `
  const pos = bot.entity.position;
  const hostiles = Object.values(bot.entities).filter(e => e.type === "hostile" && e.position.distanceTo(pos) < 12);
  if (hostiles.length > 0) {
    let hx=0,hz=0;
    for (const h of hostiles) { hx+=h.position.x; hz+=h.position.z; }
    hx/=hostiles.length; hz/=hostiles.length;
    const away = new Vec3(pos.x-hx, 0, pos.z-hz).normalize();
    await bot.lookAt(pos.plus(away.scaled(20).offset(0, pos.y, 0)));
  }
  bot.setControlState("sprint", true);
  bot.setControlState("forward", true);
  bot.setControlState("jump", true);
  await sleep(1500);
  bot.clearControlStates();
  return { health: bot.health, pos: bot.entity.position };
`;

const ATTACK_CODE = `
  const sword = bot.inventory.items().find(i => i.name.includes("sword"));
  if (sword) await bot.equip(sword, "hand");
  const mob = Object.values(bot.entities).find(e => e.type === "hostile" && e.position.distanceTo(bot.entity.position) < 4);
  if (mob) {
    for (let i = 0; i < 5; i++) {
      const m = Object.values(bot.entities).find(e => e === mob);
      if (!m) break;
      await bot.lookAt(m.position.offset(0, 1.5, 0));
      await bot.attack(m);
      await sleep(400);
    }
  }
  return { health: bot.health, pos: bot.entity.position };
`;

// ─── WebSocket Event Handling ───

function connectWs() {
  if (ws) { try { ws.close(); } catch {} }

  ws = new WebSocket(`ws://localhost:8081/ws?session=${session}`);

  ws.onmessage = (msg: MessageEvent) => {
    let event: any;
    try { event = JSON.parse(String(msg.data)); } catch { return; }

    const now = Date.now();

    // Update status bar on any event with health
    if (event.data?.health !== undefined) {
      updateStatusBar(event.data.health, event.data.food);
    }

    switch (event.type) {
      case "connected": {
        lastHealth = event.data.health;
        lastFood = event.data.food ?? 20;
        updateStatusBar(lastHealth, lastFood);
        break;
      }

      case "health": {
        const h = event.data.health;
        const f = event.data.food;
        const dmg = lastHealth - h;

        // Log ALL damage, not just threshold-crossing damage
        if (dmg > 0) {
          logEvent(`Took ${Math.round(dmg)} damage (health: ${Math.round(h)}/20)`);
        }
        // Log food changes
        if (f < lastFood) {
          logEvent(`Food dropped to ${f}/20`);
        }

        // Alert LLM only on significant damage or getting low
        if (dmg > 0 && h <= 10 && h > 6 && now > cooldownUntil) {
          cooldownUntil = now + 5000;
          sendAlert(`⚠️ Health getting low: ${Math.round(h)}/20. Took ${Math.round(dmg)} damage.`);
        }

        // Low food warning
        if (f <= 6 && lastFood > 6 && now > cooldownUntil) {
          cooldownUntil = now + 15000;
          sendAlert(`🍖 Hungry! Food: ${f}/20. Need food soon.`);
        }

        lastHealth = h;
        lastFood = f;
        updateStatusBar(h, f);
        break;
      }

      case "hurt": {
        const h = event.data.health;
        const pos = event.data.pos;
        const posText = pos ? ` at (${pos.x}, ${pos.y}, ${pos.z})` : "";
        logEvent(`Hurt${posText}! Health now ${Math.round(h)}/20`);
        lastHealth = h;
        break;
      }

      case "death": {
        justDied = true;
        const lostItems = event.data.deathInventory;
        const lostText = Array.isArray(lostItems) && lostItems.length > 0 ? lostItems.join(", ") : "nothing";
        const deathPos = event.data.deathPos;
        const deathPosText = deathPos ? ` at (${deathPos.x}, ${deathPos.y}, ${deathPos.z})` : "";
        logEvent(`DIED${deathPosText}! Lost items: [${lostText}]`);
        if (now > cooldownUntil) {
          cooldownUntil = now + 10000;
          sendAlert(`💀 I DIED${deathPosText}! Lost items: [${lostText}]. Will respawn shortly.`);
        }
        break;
      }

      case "spawn": {
        const pos = event.data.pos;
        const posText = pos ? `(${pos.x}, ${pos.y}, ${pos.z})` : "unknown";
        const deathPos = event.data.deathPos;
        const deathPosText = deathPos ? `(${deathPos.x}, ${deathPos.y}, ${deathPos.z})` : "unknown";
        if (justDied) {
          justDied = false;
          logEvent(`Respawned at ${posText}. Items dropped at ${deathPosText}.`);
          sendAlert(`🔄 Respawned at ${posText}! Inventory is empty. Items dropped at ${deathPosText}.`);
        } else {
          logEvent(`Spawned at ${posText}`);
        }
        lastHealth = event.data.health ?? 20;
        lastFood = event.data.food ?? 20;
        updateStatusBar(lastHealth, lastFood);
        break;
      }

      case "chat": {
        logEvent(`Chat from ${event.data.username}: "${event.data.message}"`);
        if (now > cooldownUntil) {
          sendAlert(`💬 ${event.data.username}: "${event.data.message}"`);
          cooldownUntil = now + 1000;
        }
        break;
      }

      case "kicked": {
        logEvent(`KICKED from server: ${event.data.reason ?? "unknown"}`);
        sendAlert(`🚫 BOT KICKED from server! Reason: ${event.data.reason ?? "unknown"}`);
        break;
      }

      case "player_joined": {
        logEvent(`${event.data.username} joined the game`);
        sendAlert(`👋 ${event.data.username} joined the game.`);
        break;
      }

      case "player_left": {
        logEvent(`${event.data.username} left the game`);
        sendAlert(`👋 ${event.data.username} left the game.`);
        break;
      }
    }
  };

  ws.onclose = () => {
    setTimeout(() => { if (session) connectWs(); }, 2000);
  };

  ws.onerror = () => {};
}

function updateStatusBar(health: number, food?: number) {
  if (!extensionCtx?.hasUI) return;
  const f = food ?? lastFood;
  const hIcon = health <= 6 ? "❤️‍🔥" : health <= 12 ? "💛" : "❤️";
  extensionCtx.ui.setStatus("mc", `🎮 ${hIcon}${Math.round(health)} 🍖${Math.round(f)}`);
}

function notify(msg: string) {
  if (extensionCtx?.hasUI) extensionCtx.ui.notify(msg, "warning");
}

function sendAlert(msg: string) {
  if (!piRef) return;
  if (extensionCtx?.isIdle()) {
    piRef.sendUserMessage(msg);
  } else {
    piRef.sendUserMessage(msg, { deliverAs: "steer" });
  }
}

// ─── Extension Entry ───

export default function (pi: ExtensionAPI) {
  piRef = pi;

  pi.registerCommand("mc-auto", {
    description: "Control autonomous loop: /mc-auto on|off|status",
    handler: async (args, ctx) => {
      const mode = (args || "").trim().toLowerCase();
      if (mode === "off") {
        autoLoopEnabled = false;
        if (ctx.hasUI) ctx.ui.setStatus("mc-loop", "⏸ mc");
        ctx.ui.notify("Minecraft auto-loop disabled", "info");
        return;
      }
      if (mode === "on" || mode === "") {
        autoLoopEnabled = true;
        if (ctx.hasUI) ctx.ui.setStatus("mc-loop", "♾️ mc");
        ctx.ui.notify("Minecraft auto-loop enabled", "info");
        if (ctx.isIdle() && !ctx.hasPendingMessages()) {
          pi.sendUserMessage(AUTO_LOOP_STEER, { deliverAs: "steer", triggerTurn: true });
        }
        return;
      }
      if (mode === "status") {
        const status = autoLoopEnabled ? "enabled" : "disabled";
        ctx.ui.notify(`Minecraft auto-loop is ${status}`, "info");
        return;
      }
      ctx.ui.notify("Usage: /mc-auto on|off|status", "warning");
    },
  });

  pi.on("agent_end", async (_event, ctx) => {
    if (ctx.hasUI) ctx.ui.setStatus("mc-loop", autoLoopEnabled ? "♾️ mc" : "⏸ mc");
    if (!autoLoopEnabled) return;
    if (!ctx.isIdle()) return;
    if (ctx.hasPendingMessages()) return;
    pi.sendUserMessage(AUTO_LOOP_STEER, { deliverAs: "steer", triggerTurn: true });
  });

  // ── Register Session Tool ──
  pi.registerTool({
    name: "mc_register_session",
    label: "MC Register",
    description: "Register Minecraft session ID for event watching. Call after POST /join.",
    parameters: Type.Object({
      session_id: Type.String({ description: "Session UUID from /join" }),
    }),
    async execute(_id, params, _signal, _onUpdate, ctx) {
      session = params.session_id;
      lastHealth = 20;
      lastFood = 20;
      cooldownUntil = 0;
      extensionCtx = ctx;

      connectWs();

      return {
        content: [{ type: "text", text: "WebSocket connected. Alerts: damage, hunger, death, chat." }],
      };
    },
  });

  // ── Strategic Tools ──

  pi.registerTool({
    name: "mc_status",
    label: "MC Status",
    description: "Get full bot status: health, food, position, time, inventory, nearby entities and blocks.",
    parameters: Type.Object({}),
    async execute() {
      const resp = await mc(`
        const pos = bot.entity.position;
        const hostiles = Object.values(bot.entities).filter(e => e !== bot.entity && e.type === "hostile" && e.position.distanceTo(pos) < 30).map(e => ({ name: e.name, dist: Math.round(e.position.distanceTo(pos)), pos: e.position }));
        const passives = Object.values(bot.entities).filter(e => e !== bot.entity && (e.type === "animal" || e.type === "passive") && e.position.distanceTo(pos) < 30).map(e => ({ name: e.name, dist: Math.round(e.position.distanceTo(pos)) }));
        const inv = bot.inventory.items().map(i => i.name + " x" + i.count);
        const nearbyBlocks = {};
        for (let x=-4;x<=4;x++) for (let y=-2;y<=2;y++) for (let z=-4;z<=4;z++) {
          const b = bot.blockAt(pos.offset(x,y,z));
          if (b && b.name !== "air" && b.name !== "cave_air") nearbyBlocks[b.name] = (nearbyBlocks[b.name]||0)+1;
        }
        return { health: bot.health, food: bot.food, pos: { x: Math.round(pos.x), y: Math.round(pos.y), z: Math.round(pos.z) }, time: bot.time.timeOfDay, isDay: bot.time.timeOfDay < 13000, inv, hostiles, passives, nearbyBlocks };
      `);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_look",
    label: "MC Look",
    description: "Look around like a human. Returns terrain profile, nearby entities with directions, ground safety, light level, biome, and points of interest. Like opening your eyes.",
    parameters: Type.Object({
      radius: Type.Optional(Type.Number({ description: "How far to scan (default 16, max 32)" })),
    }),
    async execute(_id, params) {
      const radius = Math.min(params.radius ?? 16, 32);
      const resp = await mc(`
        const pos = bot.entity.position;
        const pf = pos.floored();
        const R = ${radius};
        const result = {};

        // === SELF ===
        result.self = {
          pos: { x: Math.round(pos.x), y: Math.round(pos.y), z: Math.round(pos.z) },
          health: bot.health, food: bot.food,
          onGround: bot.entity.onGround,
          biome: bot.blockAt(pf)?.biome?.name || "unknown",
          lightLevel: bot.blockAt(pf.offset(0,1,0))?.light || 0,
          timeOfDay: bot.time.timeOfDay,
          isDay: bot.time.timeOfDay < 13000,
          isRaining: bot.isRaining,
        };

        // === INVENTORY (compact) ===
        result.inventory = bot.inventory.items().map(i => i.name + " x" + i.count);
        result.heldItem = bot.heldItem?.name || "empty";

        // === TERRAIN PROFILE (cardinal directions) ===
        // For each direction, scan ground height for 1..R blocks out
        // This shows hills, cliffs, holes, water
        const dirs = { north: [0,-1], south: [0,1], east: [1,0], west: [-1,0] };
        result.terrain = {};
        for (const [name, [dx, dz]] of Object.entries(dirs)) {
          const profile = [];
          for (let d = 1; d <= R; d += 2) {
            const sx = pf.x + dx * d;
            const sz = pf.z + dz * d;
            // Find surface height
            let surfY = null;
            let surfBlock = "air";
            for (let y = pf.y + 5; y >= pf.y - 10; y--) {
              const b = bot.blockAt(new Vec3(sx, y, sz));
              if (b && b.name !== "air" && b.name !== "cave_air") {
                surfY = y;
                surfBlock = b.name;
                break;
              }
            }
            // Check for danger: lava, water, big drop
            let danger = null;
            if (surfBlock === "water") danger = "water";
            if (surfBlock === "lava") danger = "lava";
            if (surfY !== null && surfY < pf.y - 3) danger = "cliff";
            if (surfY === null) danger = "void";

            // Check if ground is solid (no cave below)
            let caveBelow = false;
            if (surfY !== null) {
              for (let cy = surfY - 1; cy >= surfY - 4; cy--) {
                const cb = bot.blockAt(new Vec3(sx, cy, sz));
                if (cb && (cb.name === "air" || cb.name === "cave_air")) { caveBelow = true; break; }
              }
            }

            profile.push({
              dist: d,
              y: surfY,
              block: surfBlock,
              ...(danger ? { danger } : {}),
              ...(caveBelow ? { caveBelow: true } : {}),
            });
          }
          result.terrain[name] = profile;
        }

        // === GROUND SAFETY (directly around player) ===
        const unsafeSpots = [];
        for (let dx = -2; dx <= 2; dx++) {
          for (let dz = -2; dz <= 2; dz++) {
            const gx = pf.x + dx, gz = pf.z + dz;
            const below = bot.blockAt(new Vec3(gx, pf.y - 1, gz));
            if (!below || below.name === "air" || below.name === "cave_air" || below.name === "water" || below.name === "lava") {
              unsafeSpots.push({ x: gx, z: gz, block: below?.name || "void" });
            }
          }
        }
        if (unsafeSpots.length > 0) result.unsafeGround = unsafeSpots;

        // === ENTITIES (all nearby, with direction and distance) ===
        const entities = [];
        for (const e of Object.values(bot.entities)) {
          if (e === bot.entity) continue;
          const d = e.position.distanceTo(pos);
          if (d > R * 2) continue;

          const dx = e.position.x - pos.x;
          const dz = e.position.z - pos.z;
          const angle = Math.atan2(-dx, dz) * 180 / Math.PI; // minecraft coords
          let dir;
          if (angle >= -22.5 && angle < 22.5) dir = "south";
          else if (angle >= 22.5 && angle < 67.5) dir = "southwest";
          else if (angle >= 67.5 && angle < 112.5) dir = "west";
          else if (angle >= 112.5 && angle < 157.5) dir = "northwest";
          else if (angle >= 157.5 || angle < -157.5) dir = "north";
          else if (angle >= -157.5 && angle < -112.5) dir = "northeast";
          else if (angle >= -112.5 && angle < -67.5) dir = "east";
          else dir = "southeast";

          const category = e.type === "hostile" ? "hostile"
            : (e.type === "animal" || e.type === "passive") ? "passive"
            : e.type === "player" ? "player"
            : e.name === "item" ? "item"
            : "other";

          entities.push({
            name: e.name || e.displayName,
            category,
            dist: Math.round(d),
            dir,
            pos: { x: Math.round(e.position.x), y: Math.round(e.position.y), z: Math.round(e.position.z) },
          });
        }
        // Sort by distance
        entities.sort((a, b) => a.dist - b.dist);
        result.entities = entities.slice(0, 30); // cap at 30

        // === POINTS OF INTEREST (special blocks nearby) ===
        const poi = [];
        const interestingBlocks = new Set([
          "crafting_table", "furnace", "chest", "anvil", "enchanting_table",
          "bed", "white_bed", "yellow_bed", "red_bed", "blue_bed",
          "iron_ore", "gold_ore", "diamond_ore", "coal_ore", "copper_ore",
          "deepslate_iron_ore", "deepslate_gold_ore", "deepslate_diamond_ore",
          "water", "lava", "spawner",
          "oak_door", "spruce_door", "birch_door",
          "hay_block", "barrel", "smoker", "blast_furnace",
          "sweet_berry_bush", "wheat", "carrots", "potatoes",
        ]);
        for (let x = -R; x <= R; x += 2) {
          for (let z = -R; z <= R; z += 2) {
            for (let y = -4; y <= 4; y++) {
              const b = bot.blockAt(pf.offset(x, y, z));
              if (b && interestingBlocks.has(b.name)) {
                // Deduplicate: only add if not already close to existing POI
                const exists = poi.some(p => p.name === b.name && Math.abs(p.pos.x - (pf.x+x)) <= 2 && Math.abs(p.pos.z - (pf.z+z)) <= 2);
                if (!exists) {
                  poi.push({ name: b.name, pos: { x: pf.x+x, y: pf.y+y, z: pf.z+z } });
                }
              }
            }
          }
        }
        result.pointsOfInterest = poi;

        return result;
      `);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_eval",
    label: "MC Eval",
    description: "Run arbitrary JS on the bot. Variables: bot, goals, Vec3, sleep(ms). Return a value to see it.",
    parameters: Type.Object({
      code: Type.String({ description: "JavaScript code to execute" }),
    }),
    async execute(_id, params) {
      const resp = await mc(params.code);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_flee",
    label: "MC Flee",
    description: "Sprint away from nearest hostile mobs for ~15 blocks (1.5 seconds).",
    parameters: Type.Object({}),
    async execute() {
      const resp = await interrupt(FLEE_CODE);
      return mcResult(resp, false);
    },
  });

  pi.registerTool({
    name: "mc_fight",
    label: "MC Fight",
    description: "Attack the nearest hostile mob in melee range.",
    parameters: Type.Object({}),
    async execute() {
      const resp = await interrupt(ATTACK_CODE);
      return mcResult(resp, false);
    },
  });

  pi.registerTool({
    name: "mc_goto",
    label: "MC Go To",
    description: "Pathfind to coordinates. Automatically breaks long distances into chunks.",
    parameters: Type.Object({
      x: Type.Number(),
      y: Type.Number(),
      z: Type.Number(),
      range: Type.Optional(Type.Number({ description: "How close to get (default 2)" })),
    }),
    async execute(_id, params) {
      const r = params.range ?? 2;
      // Break long distances into ~50-block chunks for reliability
      const resp = await mc(`
        const target = new Vec3(${params.x}, ${params.y}, ${params.z});
        const range = ${r};
        const MAX_CHUNK = 50;
        let pos = bot.entity.position;
        let dist = pos.distanceTo(target);

        if (dist <= MAX_CHUNK + range) {
          // Short distance — direct pathfind
          await bot.pathfinder.goto(new goals.GoalNear(target.x, target.y, target.z, range));
        } else {
          // Long distance — break into chunks
          const maxChunks = 20;
          for (let i = 0; i < maxChunks; i++) {
            pos = bot.entity.position;
            dist = pos.distanceTo(target);
            if (dist <= range) break;

            const dir = target.minus(pos).normalize();
            const chunkDist = Math.min(MAX_CHUNK, dist - range);
            const waypoint = pos.plus(dir.scaled(chunkDist));

            try {
              await bot.pathfinder.goto(new goals.GoalNear(waypoint.x, waypoint.y, waypoint.z, 3));
            } catch(e) {
              // If pathfinding fails, try walking manually for a bit
              await bot.lookAt(target);
              bot.setControlState("forward", true);
              bot.setControlState("jump", true);
              await sleep(3000);
              bot.clearControlStates();
            }
          }
        }
        return { pos: bot.entity.position };
      `, 60000); // Longer timeout for long-distance travel
      if (!resp.ok && resp.error?.includes("timed out")) {
        // On timeout, interrupt and get current state so the agent knows where it ended up
        const state = await interrupt(`
          bot.pathfinder.stop();
          bot.clearControlStates();
          const pos = bot.entity.position;
          const block = bot.blockAt(pos.floored());
          const blockBelow = bot.blockAt(pos.floored().offset(0, -1, 0));
          const inWater = block?.name === "water" || blockBelow?.name === "water";
          return {
            pos: { x: Math.round(pos.x), y: Math.round(pos.y), z: Math.round(pos.z) },
            health: bot.health, food: bot.food,
            inWater,
            block: block?.name,
            blockBelow: blockBelow?.name,
            inv: bot.inventory.items().map(i => i.name + " x" + i.count),
          };
        `);
        const stateText = state.ok ? JSON.stringify(state.result, null, 2) : "couldn't get state";
        return toolResult(`Error: Execution timed out after 30000ms\nBot state after timeout:\n${stateText}`);
      }
      return mcResult(resp, false);
    },
  });

  // mc_mine removed — mc_eval handles mining more reliably

  pi.registerTool({
    name: "mc_craft",
    label: "MC Craft",
    description: "Craft an item. Finds or places a crafting table if needed.",
    parameters: Type.Object({
      item: Type.String({ description: "Item name like 'wooden_pickaxe', 'stone_sword', 'stick'" }),
      count: Type.Optional(Type.Number({ description: "How many to craft (default 1)" })),
    }),
    async execute(_id, params) {
      const count = params.count ?? 1;
      const resp = await mc(`
        const itemName = "${params.item}";
        const itemId = bot.registry.itemsByName[itemName]?.id;
        if (itemId === undefined) return "Unknown item: " + itemName;
        let recipes = bot.recipesFor(itemId);
        if (recipes.length > 0) {
          await bot.craft(recipes[0], ${count});
          return { crafted: itemName, inv: bot.inventory.items().map(i => i.name + " x" + i.count) };
        }
        const ctId = bot.registry.blocksByName.crafting_table.id;
        let ct = bot.findBlock({ matching: ctId, maxDistance: 32 });
        if (!ct) {
          let tableItem = bot.inventory.items().find(i => i.name === "crafting_table");
          if (!tableItem) {
            const r = bot.recipesFor(bot.registry.itemsByName.crafting_table.id);
            if (r.length > 0) { await bot.craft(r[0]); tableItem = bot.inventory.items().find(i => i.name === "crafting_table"); }
          }
          if (tableItem) {
            await bot.equip(tableItem, "hand");
            const pos = bot.entity.position.floored();
            const dirs = [[1,0],[0,1],[-1,0],[0,-1]];
            for (const [dx, dz] of dirs) {
              const ground = bot.blockAt(pos.offset(dx, -1, dz));
              const target = bot.blockAt(pos.offset(dx, 0, dz));
              if (ground && ground.name !== "air" && target && target.name === "air") {
                await bot.lookAt(pos.offset(dx, 0.5, dz));
                try { await bot.placeBlock(ground, new Vec3(0, 1, 0)); await sleep(300); } catch(e) {}
                break;
              }
            }
            ct = bot.findBlock({ matching: ctId, maxDistance: 10 });
          }
        }
        if (!ct) return "No crafting table available";
        await bot.pathfinder.goto(new goals.GoalNear(ct.position.x, ct.position.y, ct.position.z, 2));
        recipes = bot.recipesFor(itemId, null, 1, ct);
        if (recipes.length === 0) return "No recipe for " + itemName + " (missing materials?)";
        await bot.craft(recipes[0], ${count}, ct);
        return { crafted: itemName, inv: bot.inventory.items().map(i => i.name + " x" + i.count) };
      `);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_chat",
    label: "MC Chat",
    description: "Send a chat message in-game.",
    parameters: Type.Object({ message: Type.String() }),
    async execute(_id, params) {
      const resp = await mc(`await bot.chat(${JSON.stringify(params.message)}); return "sent";`);
      return toolResult(resp.ok ? "Sent" : `Error: ${resp.error}`);
    },
  });

  pi.registerTool({
    name: "mc_eat",
    label: "MC Eat",
    description: "Eat a food item from inventory. Tries to eat the best food available.",
    parameters: Type.Object({
      item: Type.Optional(Type.String({ description: "Specific food item name to eat (optional — auto-picks best)" })),
    }),
    async execute(_id, params) {
      const itemFilter = params.item ? `i.name === "${params.item}"` : `["cooked_beef","cooked_porkchop","bread","golden_apple","cooked_chicken","cooked_mutton","baked_potato","cooked_rabbit","cooked_salmon","cooked_cod","porkchop","beef","chicken","mutton","rabbit","potato","carrot","apple","melon_slice","sweet_berries","dried_kelp","cookie","pumpkin_pie","mushroom_stew","beetroot_soup","suspicious_stew","rabbit_stew","golden_carrot","beetroot","rotten_flesh","spider_eye"].includes(i.name)`;
      const resp = await mc(`
        const food = bot.inventory.items().find(i => ${itemFilter});
        if (!food) return { error: "No food in inventory", inv: bot.inventory.items().map(i => i.name + " x" + i.count) };
        await bot.equip(food, "hand");
        await bot.consume();
        return { ate: food.name, health: bot.health, food: bot.food };
      `);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_sleep",
    label: "MC Sleep",
    description: "Sleep in the nearest bed. Only works at night. Sets spawn point.",
    parameters: Type.Object({}),
    async execute() {
      const resp = await mc(`
        if (bot.time.timeOfDay < 12541) return { error: "Can't sleep during daytime", time: bot.time.timeOfDay };
        const bed = bot.findBlock({ matching: b => b.name.includes("bed"), maxDistance: 20 });
        if (!bed) return { error: "No bed nearby" };
        await bot.pathfinder.goto(new goals.GoalNear(bed.position.x, bed.position.y, bed.position.z, 2));
        await bot.sleep(bed);
        return { sleeping: true, bedPos: bed.position, spawnSet: true };
      `);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_place",
    label: "MC Place",
    description: "Place a block from inventory at or near a target position.",
    parameters: Type.Object({
      block: Type.String({ description: "Block item name like 'cobblestone', 'dirt', 'crafting_table'" }),
      x: Type.Number(),
      y: Type.Number(),
      z: Type.Number(),
    }),
    async execute(_id, params) {
      const resp = await mc(`
        const item = bot.inventory.items().find(i => i.name === "${params.block}");
        if (!item) return { error: "No ${params.block} in inventory" };
        await bot.equip(item, "hand");
        const targetPos = new Vec3(${params.x}, ${params.y}, ${params.z});

        // Find an adjacent solid block to place against
        for (const [dx, dy, dz] of [[0,-1,0],[0,1,0],[1,0,0],[-1,0,0],[0,0,1],[0,0,-1]]) {
          const adj = bot.blockAt(targetPos.offset(dx, dy, dz));
          if (adj && adj.name !== "air" && adj.name !== "water" && adj.name !== "lava") {
            // Get close enough to place
            const dist = bot.entity.position.distanceTo(targetPos);
            if (dist > 4) {
              await bot.pathfinder.goto(new goals.GoalNear(targetPos.x, targetPos.y, targetPos.z, 3));
            }
            try {
              await bot.placeBlock(adj, new Vec3(-dx, -dy, -dz));
              return { placed: "${params.block}", at: targetPos };
            } catch(e) {
              return { placed: "${params.block}", at: targetPos, warning: e.message };
            }
          }
        }
        return { error: "No adjacent solid block to place against" };
      `);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_equip",
    label: "MC Equip",
    description: "Equip an item from inventory to hand or armor slot.",
    parameters: Type.Object({
      item: Type.String({ description: "Item name like 'stone_sword', 'iron_pickaxe'" }),
      slot: Type.Optional(Type.String({ description: "Slot: 'hand', 'off-hand', 'head', 'torso', 'legs', 'feet' (default: 'hand')" })),
    }),
    async execute(_id, params) {
      const slot = params.slot ?? "hand";
      const resp = await mc(`
        const item = bot.inventory.items().find(i => i.name === "${params.item}");
        if (!item) return { error: "No ${params.item} in inventory", inv: bot.inventory.items().map(i => i.name + " x" + i.count) };
        await bot.equip(item, "${slot}");
        return { equipped: "${params.item}", slot: "${slot}", inv: bot.inventory.items().map(i => i.name + " x" + i.count) };
      `);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_drop",
    label: "MC Drop",
    description: "Drop items from inventory on the ground.",
    parameters: Type.Object({
      item: Type.String({ description: "Item name to drop" }),
      count: Type.Optional(Type.Number({ description: "How many to drop (default: all)" })),
    }),
    async execute(_id, params) {
      const resp = await mc(`
        const item = bot.inventory.items().find(i => i.name === "${params.item}");
        if (!item) return { error: "No ${params.item} in inventory" };
        const count = ${params.count ?? 0} || item.count;
        await bot.tossStack(item);
        return { dropped: "${params.item}", count, inv: bot.inventory.items().map(i => i.name + " x" + i.count) };
      `);
      return mcResult(resp);
    },
  });

  pi.registerTool({
    name: "mc_dig_shelter",
    label: "MC Dig Shelter",
    description: "Emergency: dig a 3-deep hole and try to seal the top. Quick underground shelter.",
    parameters: Type.Object({}),
    async execute() {
      const resp = await mc(`
        for (let i = 0; i < 3; i++) {
          const b = bot.blockAt(bot.entity.position.floored().offset(0, -1, 0));
          if (b && b.name !== "air" && b.name !== "water" && b.name !== "lava") await bot.dig(b);
          await sleep(100);
        }
        const pos = bot.entity.position.floored();
        const sealItem = bot.inventory.items().find(i => ["gravel","dirt","cobblestone","stone","spruce_planks","oak_planks"].includes(i.name));
        if (sealItem) {
          await bot.equip(sealItem, "hand");
          // Seal above: try to close the shaft
          for (let y = 2; y <= 3; y++) {
            for (const [dx,dz] of [[1,0],[-1,0],[0,1],[0,-1]]) {
              const wall = bot.blockAt(pos.offset(dx, y, dz));
              if (wall && wall.name !== "air") {
                const target = bot.blockAt(pos.offset(0, y, 0));
                if (target && target.name === "air") {
                  try { await bot.placeBlock(wall, new Vec3(-dx, 0, -dz)); break; } catch(e) {}
                }
              }
            }
          }
        }
        return { health: bot.health, pos: bot.entity.position };
      `);
      return mcResult(resp, false);
    },
  });

  pi.registerTool({
    name: "mc_collect_drops",
    label: "MC Collect Drops",
    description: "Walk around and pick up nearby dropped items.",
    parameters: Type.Object({}),
    async execute() {
      const resp = await mc(`
        const items = Object.values(bot.entities).filter(e => e.name === "item" && e.position.distanceTo(bot.entity.position) < 15);
        let collected = 0;
        for (const it of items) {
          try { await bot.pathfinder.goto(new goals.GoalNear(it.position.x, it.position.y, it.position.z, 0)); collected++; await sleep(300); } catch(e) {}
        }
        return { collected, inv: bot.inventory.items().map(i => i.name + " x" + i.count) };
      `);
      return mcResult(resp);
    },
  });

  // ── Cleanup ──

  pi.on("session_shutdown", async () => {
    if (ws) { try { ws.close(); } catch {} ws = null; }
  });
}

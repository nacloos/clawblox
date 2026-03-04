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
let reflexCooldownUntil = 0;
let extensionCtx: ExtensionContext | null = null;
let piRef: ExtensionAPI | null = null;
let autoLoopEnabled = true;
const AUTO_LOOP_STEER = "continue";

// ─── Bridge Communication ───

async function mc(code: string, timeout = 29000): Promise<{ ok: boolean; result?: any; error?: string }> {
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
  } catch {
    return { ok: false, error: "Bridge unreachable" };
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
  await sleep(3000);
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

        // AUTO-REFLEX: only at near-death → interrupt & flee
        if (dmg > 0 && h <= 4 && now > reflexCooldownUntil) {
          reflexCooldownUntil = now + 5000;
          interrupt(FLEE_CODE);
          notify(`🏃 AUTO-FLEE: Health ${Math.round(h)}/20! Interrupting and running!`);
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
        // Immediate reaction — hurt fires even during /input execution
        const h = event.data.health;
        if (h <= 4 && now > reflexCooldownUntil) {
          reflexCooldownUntil = now + 5000;
          interrupt(FLEE_CODE);
          notify(`🏃 AUTO-FLEE: Hurt! Health ${Math.round(h)}/20!`);
        }
        lastHealth = h;
        break;
      }

      case "death": {
        if (now > cooldownUntil) {
          cooldownUntil = now + 10000;
          sendAlert(`💀 I DIED! Need to respawn and recover.`);
        }
        break;
      }

      case "chat": {
        if (now > cooldownUntil) {
          sendAlert(`💬 ${event.data.username}: "${event.data.message}"`);
          cooldownUntil = now + 1000;
        }
        break;
      }

      case "hostiles_nearby": {
        // Only if very close and we have decent health — auto-attack
        const hostiles = event.data.hostiles;
        if (hostiles.length > 0 && hostiles[0].dist < 3 && lastHealth > 10 && now > reflexCooldownUntil) {
          reflexCooldownUntil = now + 4000;
          interrupt(ATTACK_CODE);
          notify(`⚔️ AUTO-ATTACK: ${hostiles[0].name} at ${hostiles[0].dist}m`);
        }
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
      reflexCooldownUntil = 0;
      extensionCtx = ctx;

      connectWs();

      return {
        content: [{ type: "text", text: "WebSocket connected. Auto-reflexes: flee on hurt ≤8hp (via /interrupt, never blocked). Alerts: damage, hunger, death, chat." }],
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
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result, null, 2) : `Error: ${resp.error}` }] };
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
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result, null, 2) : `Error: ${resp.error}` }] };
    },
  });

  pi.registerTool({
    name: "mc_flee",
    label: "MC Flee",
    description: "Sprint away from nearest hostile mobs for a few seconds.",
    parameters: Type.Object({}),
    async execute() {
      const resp = await interrupt(FLEE_CODE);
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result) : `Error: ${resp.error}` }] };
    },
  });

  pi.registerTool({
    name: "mc_fight",
    label: "MC Fight",
    description: "Attack the nearest hostile mob in melee range.",
    parameters: Type.Object({}),
    async execute() {
      const resp = await interrupt(ATTACK_CODE);
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result) : `Error: ${resp.error}` }] };
    },
  });

  pi.registerTool({
    name: "mc_goto",
    label: "MC Go To",
    description: "Pathfind to coordinates.",
    parameters: Type.Object({
      x: Type.Number(),
      y: Type.Number(),
      z: Type.Number(),
      range: Type.Optional(Type.Number({ description: "How close to get (default 2)" })),
    }),
    async execute(_id, params) {
      const r = params.range ?? 2;
      const resp = await mc(`await bot.pathfinder.goto(new goals.GoalNear(${params.x}, ${params.y}, ${params.z}, ${r})); return { pos: bot.entity.position };`);
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result) : `Error: ${resp.error}` }] };
    },
  });

  pi.registerTool({
    name: "mc_mine",
    label: "MC Mine",
    description: "Mine a specific block type nearby. Returns inventory after.",
    parameters: Type.Object({
      block: Type.String({ description: "Block name like 'stone', 'oak_log', 'iron_ore'" }),
      count: Type.Optional(Type.Number({ description: "How many to mine (default 5)" })),
    }),
    async execute(_id, params) {
      const count = params.count ?? 5;
      const resp = await mc(`
        const pickaxe = bot.inventory.items().find(i => i.name.includes("pickaxe"));
        const axe = bot.inventory.items().find(i => i.name.includes("axe") && !i.name.includes("pickaxe"));
        const blockName = "${params.block}";
        if (blockName.includes("log") || blockName.includes("plank")) { if (axe) await bot.equip(axe, "hand"); }
        else { if (pickaxe) await bot.equip(pickaxe, "hand"); }
        const blockId = bot.registry.blocksByName[blockName]?.id;
        if (blockId === undefined) return "Unknown block: " + blockName;
        let mined = 0;
        for (let i = 0; i < ${count}; i++) {
          if (bot.health <= 8) break;
          const b = bot.findBlock({ matching: blockId, maxDistance: 32 });
          if (!b) break;
          try {
            await bot.pathfinder.goto(new goals.GoalNear(b.position.x, b.position.y, b.position.z, 1));
            await bot.dig(b);
            mined++;
            await sleep(200);
          } catch(e) { break; }
        }
        return { mined, inv: bot.inventory.items().map(i => i.name + " x" + i.count) };
      `);
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result, null, 2) : `Error: ${resp.error}` }] };
    },
  });

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
            // Place 1 block away from player to avoid hitbox overlap rejection
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
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result, null, 2) : `Error: ${resp.error}` }] };
    },
  });

  pi.registerTool({
    name: "mc_chat",
    label: "MC Chat",
    description: "Send a chat message in-game.",
    parameters: Type.Object({ message: Type.String() }),
    async execute(_id, params) {
      const resp = await mc(`await bot.chat(${JSON.stringify(params.message)}); return "sent";`);
      return { content: [{ type: "text", text: resp.ok ? "Sent" : `Error: ${resp.error}` }] };
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
          // Seal above: place on the wall 2 blocks up (not at player position)
          // First try placing block above head by referencing adjacent wall
          for (const [dx,dz] of [[1,0],[-1,0],[0,1],[0,-1]]) {
            const wall = bot.blockAt(pos.offset(dx, 2, dz));
            if (wall && wall.name !== "air") {
              try { await bot.placeBlock(wall, new Vec3(-dx, 0, -dz)); break; } catch(e) {}
            }
          }
          // Also seal the top of the shaft (1 block above head)
          for (const [dx,dz] of [[1,0],[-1,0],[0,1],[0,-1]]) {
            const wall = bot.blockAt(pos.offset(dx, 3, dz));
            if (wall && wall.name !== "air") {
              const target = bot.blockAt(pos.offset(0, 3, 0));
              if (target && target.name === "air") {
                try { await bot.placeBlock(wall, new Vec3(-dx, 0, -dz)); break; } catch(e) {}
              }
            }
          }
        }
        return { health: bot.health, pos: bot.entity.position };
      `);
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result) : `Error: ${resp.error}` }] };
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
      return { content: [{ type: "text", text: resp.ok ? JSON.stringify(resp.result, null, 2) : `Error: ${resp.error}` }] };
    },
  });

  // ── Cleanup ──

  pi.on("session_shutdown", async () => {
    if (ws) { try { ws.close(); } catch {} ws = null; }
  });
}

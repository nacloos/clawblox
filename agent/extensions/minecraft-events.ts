/**
 * Minecraft Mineflayer Extension (Minimal)
 *
 * Only 3 tools: mc_register_session, mc_look, mc_eval.
 * Everything else the agent can do via mc_eval.
 *
 * WebSocket for real-time bot events.
 * POST /interrupt to cancel running tools.
 * POST /input for normal tools.
 */

import type { ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

const BRIDGE_URL = "http://localhost:8081";

let session = "";
let ws: InstanceType<typeof WebSocket> | null = null;
let lastHealth = 20;
let lastFood = 20;
let cooldownUntil = 0;
let deathCooldownUntil = 0;
let playerJoinLeaveCooldownUntil = 0;
let extensionCtx: ExtensionContext | null = null;
let piRef: ExtensionAPI | null = null;
let autoLoopEnabled = true;
const AUTO_LOOP_STEER = "continue";
let justDied = false;

// ─── Between-Action Event Log ───
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

function withEventRecap(text: string): string {
  const recap = flushEventLog();
  if (!recap) return text;
  return `[Events since last action]\n${recap}\n\n${text}`;
}

function toolResult(text: string) {
  return { content: [{ type: "text" as const, text: withEventRecap(text) }] };
}

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

// ─── WebSocket Event Handling ───

function connectWs() {
  if (ws) { try { ws.close(); } catch {} }

  ws = new WebSocket(`ws://localhost:8081/ws?session=${session}`);

  ws.onmessage = (msg: MessageEvent) => {
    let event: any;
    try { event = JSON.parse(String(msg.data)); } catch { return; }

    const now = Date.now();

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

        if (dmg > 0) {
          logEvent(`Took ${Math.round(dmg)} damage (health: ${Math.round(h)}/20)`);
        }
        if (f < lastFood) {
          logEvent(`Food dropped to ${f}/20`);
        }

        if (dmg > 0 && h <= 10 && h > 6 && now > cooldownUntil) {
          cooldownUntil = now + 5000;
          sendAlert(`⚠️ Health getting low: ${Math.round(h)}/20. Took ${Math.round(dmg)} damage.`);
        }

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
        if (now > deathCooldownUntil) {
          deathCooldownUntil = now + 10000;
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
        if (now > playerJoinLeaveCooldownUntil) {
          playerJoinLeaveCooldownUntil = now + 3000;
          sendAlert(`👋 ${event.data.username} joined the game.`);
        }
        break;
      }

      case "player_left": {
        logEvent(`${event.data.username} left the game`);
        if (now > playerJoinLeaveCooldownUntil) {
          playerJoinLeaveCooldownUntil = now + 3000;
          sendAlert(`👋 ${event.data.username} left the game.`);
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

function sendAlert(msg: string) {
  if (!piRef) return;
  if (extensionCtx?.isIdle()) {
    piRef.sendUserMessage(msg);
  } else {
    piRef.sendUserMessage(msg, { deliverAs: "steer" });
  }
}

function stripProjectContextFromSystemPrompt(prompt: string): string {
  const marker = "\n\n# Project Context\n\n";
  const markerIndex = prompt.indexOf(marker);
  if (markerIndex === -1) return prompt;

  const suffixMarker = "\nCurrent date and time:";
  const suffixIndex = prompt.indexOf(suffixMarker, markerIndex);
  if (suffixIndex === -1) {
    return prompt.slice(0, markerIndex).trimEnd();
  }
  return `${prompt.slice(0, markerIndex)}${prompt.slice(suffixIndex)}`;
}

function writeSystemPromptSnapshot(prompt: string) {
  const outputPath = process.env.PI_SYSTEM_PROMPT_SNAPSHOT_FILE;
  if (!outputPath) return;
  if (!prompt.trim()) return;
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, prompt.endsWith("\n") ? prompt : `${prompt}\n`, "utf8");
}

// ─── Extension Entry ───

export default function (pi: ExtensionAPI) {
  piRef = pi;

  pi.on("before_agent_start", async (event) => {
    const systemPrompt = stripProjectContextFromSystemPrompt(event.systemPrompt);
    writeSystemPromptSnapshot(systemPrompt);
    return { systemPrompt };
  });

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

  // ── mc_register_session ──

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
      deathCooldownUntil = 0;
      playerJoinLeaveCooldownUntil = 0;
      extensionCtx = ctx;

      connectWs();

      return {
        content: [{ type: "text", text: "WebSocket connected. Alerts: damage, hunger, death, chat. Tools: mc_look, mc_eval." }],
      };
    },
  });

  // ── mc_look ──

  pi.registerTool({
    name: "mc_look",
    label: "MC Look",
    description: "Observe the world: self status, inventory, all nearby entities, and block counts. Raw data, no filtering.",
    parameters: Type.Object({
      radius: Type.Optional(Type.Number({ description: "Scan radius in blocks (default 16, max 64)" })),
    }),
    async execute(_id, params) {
      const radius = Math.min(params.radius ?? 16, 64);
      const resp = await mc(`
        const pos = bot.entity.position;
        const pf = pos.floored();
        const R = ${radius};
        const result = {};

        result.self = {
          pos: { x: Math.round(pos.x), y: Math.round(pos.y), z: Math.round(pos.z) },
          health: bot.health,
          food: bot.food,
          onGround: bot.entity.onGround,
          biome: bot.blockAt(pf)?.biome?.name || "unknown",
          lightLevel: bot.blockAt(pf.offset(0,1,0))?.light || 0,
          timeOfDay: bot.time.timeOfDay,
          isDay: bot.time.timeOfDay < 13000,
          isRaining: bot.isRaining,
        };

        result.inventory = bot.inventory.items().map(i => i.name + " x" + i.count);
        result.heldItem = bot.heldItem?.name || "empty";

        const entities = [];
        for (const e of Object.values(bot.entities)) {
          if (e === bot.entity) continue;
          const d = e.position.distanceTo(pos);
          if (d > R * 2) continue;
          entities.push({
            name: e.name || e.displayName,
            type: e.type,
            dist: Math.round(d),
            pos: { x: Math.round(e.position.x), y: Math.round(e.position.y), z: Math.round(e.position.z) },
          });
        }
        entities.sort((a, b) => a.dist - b.dist);
        result.entities = entities.slice(0, 40);

        const blocks = {};
        const step = R > 32 ? 2 : 1;
        for (let x = -R; x <= R; x += step) {
          for (let z = -R; z <= R; z += step) {
            for (let y = -4; y <= 4; y++) {
              const b = bot.blockAt(pf.offset(x, y, z));
              if (b && b.name !== "air" && b.name !== "cave_air") {
                blocks[b.name] = (blocks[b.name] || 0) + 1;
              }
            }
          }
        }
        result.blocks = blocks;

        return result;
      `);
      return mcResult(resp);
    },
  });

  // ── mc_eval ──

  pi.registerTool({
    name: "mc_eval",
    label: "MC Eval",
    description: "Run arbitrary JS on the bot. Available: bot, goals, Vec3, sleep(ms). Return a value to see it.",
    parameters: Type.Object({
      code: Type.String({ description: "JavaScript code to execute" }),
      timeout: Type.Optional(Type.Number({ description: "Timeout in ms (default 35000, max 120000)" })),
    }),
    async execute(_id, params) {
      const timeout = Math.min(params.timeout ?? 35000, 120000);
      const resp = await mc(params.code, timeout);
      return mcResult(resp);
    },
  });

  // ── Cleanup ──

  pi.on("session_shutdown", async () => {
    if (ws) { try { ws.close(); } catch {} ws = null; }
  });
}

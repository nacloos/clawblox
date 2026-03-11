/**
 * server_eval.mjs
 *
 * Thin eval-based Clawblox adapter for Minecraft.
 *
 * Endpoints:
 *   POST /join?name=<botName>    — create bot session, connect to Minecraft
 *   POST /input                  — execute JS code against live mineflayer bot
 *   POST /interrupt              — cancel running /input, execute priority code
 *   GET  /ws?session=<token>     — WebSocket stream of raw bot events
 *   GET  /skill.md               — serve eval-mode skill guide
 *   POST /leave                  — disconnect bot
 *
 * Auth: all endpoints except /join and /skill.md require X-Session header.
 */

import Fastify from "fastify";
import websocket from "@fastify/websocket";
import { readFile } from "fs/promises";
import { fileURLToPath } from "url";
import { dirname, join } from "path";
import pathfinderPkg from "mineflayer-pathfinder";
import { createSession, getSession, destroySession } from "./bot-manager.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILL_MD_PATH = join(__dirname, "EVAL_SKILL.md");
const PORT = parseInt(process.env.PORT || "8081", 10);
const EVAL_TIMEOUT_MS = parseInt(process.env.EVAL_TIMEOUT_MS || "30000", 10);
const INTERRUPT_TIMEOUT_MS = parseInt(process.env.INTERRUPT_TIMEOUT_MS || "5000", 10);
const { goals } = pathfinderPkg;
const VIEWER_ENABLED = (process.env.MC_VIEWER || "1") !== "0";
const VIEWER_PORT = parseInt(process.env.MC_VIEWER_PORT || "3007", 10);

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const fastify = Fastify({
  logger: true,
  // Keep useful logs but remove noisy per-request "incoming request" lines.
  disableRequestLogging: true,
});
let viewerStarted = false;

await fastify.register(websocket);

// Keep the adapter alive even if model-generated eval code spawns
// asynchronous errors outside the awaited /input call path.
process.on("unhandledRejection", (reason) => {
  fastify.log.error(
    { err: reason },
    "Unhandled promise rejection in eval adapter (kept alive)",
  );
});

process.on("uncaughtException", (err) => {
  fastify.log.error(
    { err },
    "Uncaught exception in eval adapter (kept alive)",
  );
});

/** @type {Map<string, AbortController>} session token → active /input abort controller */
const activeInputs = new Map();

/** @type {Map<string, Set<import('ws').WebSocket>>} session token → connected WS clients */
const wsClients = new Map();

// ─── Helpers ───

function resolveSession(request, reply) {
  const token = request.headers["x-session"];
  if (!token) {
    reply.code(401).send({ error: "Missing X-Session header" });
    return null;
  }
  try {
    return getSession(token);
  } catch {
    reply.code(401).send({ error: "Invalid or expired session" });
    return null;
  }
}

function serializeForJson(value) {
  try {
    return JSON.parse(
      JSON.stringify(value, (_k, v) => {
        if (typeof v === "bigint") return Number(v);
        if (v instanceof Map) return Object.fromEntries(v.entries());
        if (v instanceof Set) return [...v.values()];
        return v;
      }),
    );
  } catch {
    return String(value);
  }
}

function makeAbortableTimeout(ms, signal) {
  return new Promise((_, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`Execution timed out after ${ms}ms`)),
      ms,
    );
    if (signal) {
      signal.addEventListener("abort", () => {
        clearTimeout(timer);
        reject(new Error("Execution interrupted"));
      });
    }
  });
}

async function runCodeInSession(session, code, timeoutMs = EVAL_TIMEOUT_MS, signal = null) {
  const bot = session.bot;
  const Vec3 = bot.entity?.position?.constructor;
  const context = {
    bot,
    session,
    goals,
    Vec3,
    sleep: (ms) =>
      new Promise((resolve, reject) => {
        const timer = setTimeout(resolve, ms);
        if (signal) {
          signal.addEventListener("abort", () => {
            clearTimeout(timer);
            reject(new Error("Execution interrupted"));
          });
        }
      }),
    log: (...args) => fastify.log.info({ args }, "eval log"),
    now: () => Date.now(),
  };

  const fn = new AsyncFunction(
    ...Object.keys(context),
    `"use strict";\n${code}`,
  );

  const result = await Promise.race([
    fn(...Object.values(context)),
    makeAbortableTimeout(timeoutMs, signal),
  ]);

  return serializeForJson(result);
}

// ─── WebSocket event broadcasting ───

function broadcastEvent(sessionToken, event) {
  const clients = wsClients.get(sessionToken);
  if (!clients || clients.size === 0) return;
  const msg = JSON.stringify(event);
  for (const ws of clients) {
    if (ws.readyState === 1) ws.send(msg);
  }
}

async function maybeStartViewer(session) {
  if (!VIEWER_ENABLED || viewerStarted) return;
  try {
    const mod = await import("prismarine-viewer");
    const viewer =
      mod.mineflayer ||
      mod.default?.mineflayer ||
      (typeof mod.default === "function" ? mod.default : null);
    if (typeof viewer !== "function") {
      fastify.log.warn("prismarine-viewer loaded but mineflayer export not found");
      return;
    }
    viewer(session.bot, { port: VIEWER_PORT, firstPerson: false });
    viewerStarted = true;
    fastify.log.info(`Minecraft viewer available at http://127.0.0.1:${VIEWER_PORT}`);
  } catch (err) {
    fastify.log.warn({ err }, "Failed to start prismarine-viewer");
  }
}

/**
 * Wire mineflayer events to WebSocket broadcast for a session.
 * Forwards raw bot events — no game logic here.
 */
function wireWsBroadcast(session) {
  const { bot, sessionToken } = session;

  // Helper: get bot position as rounded coords
  function botPos() {
    const p = bot.entity?.position;
    return p ? { x: Math.round(p.x), y: Math.round(p.y), z: Math.round(p.z) } : null;
  }

  // Track last known inventory so we can report it on death
  // (by the time "death" fires, inventory may already be cleared)
  let lastKnownInventory = bot.inventory.items().map(i => i.name + " x" + i.count);
  let lastDeathPos = null;

  bot.on("health", () => {
    // Update inventory snapshot while alive
    const inv = bot.inventory.items().map(i => i.name + " x" + i.count);
    if (inv.length > 0) lastKnownInventory = inv;

    broadcastEvent(sessionToken, {
      type: "health",
      data: { health: bot.health, food: bot.food, pos: botPos() },
    });
  });

  bot.on("death", () => {
    lastDeathPos = botPos();
    broadcastEvent(sessionToken, {
      type: "death",
      data: { deathInventory: lastKnownInventory, deathPos: lastDeathPos },
    });
    lastKnownInventory = [];
  });

  bot.on("spawn", () => {
    broadcastEvent(sessionToken, {
      type: "spawn",
      data: {
        health: bot.health,
        food: bot.food,
        pos: botPos(),
        deathPos: lastDeathPos,
      },
    });
    lastDeathPos = null;
  });

  bot.on("entityHurt", (entity) => {
    if (entity === bot.entity) {
      broadcastEvent(sessionToken, {
        type: "hurt",
        data: { health: bot.health, pos: botPos() },
      });
    }
  });

  bot.on("chat", (username, message) => {
    if (username === bot.username) return;
    broadcastEvent(sessionToken, {
      type: "chat",
      data: { username, message },
    });
  });

  bot.on("playerJoined", (player) => {
    broadcastEvent(sessionToken, {
      type: "player_joined",
      data: { username: player.username },
    });
  });

  bot.on("playerLeft", (player) => {
    broadcastEvent(sessionToken, {
      type: "player_left",
      data: { username: player.username },
    });
  });

  bot.on("kicked", (reason) => {
    broadcastEvent(sessionToken, { type: "kicked", data: { reason } });
  });
}

// ─── Routes ───

fastify.post("/join", async (request, reply) => {
  const name = request.query.name || `agent_${Date.now()}`;
  try {
    const { sessionToken, agentId } = await createSession(name);
    const session = getSession(sessionToken);
    wireWsBroadcast(session);
    await maybeStartViewer(session);
    reply.code(200).send({ session: sessionToken, agent_id: agentId });
  } catch (err) {
    fastify.log.error(err, "Failed to join Minecraft server");
    reply.code(503).send({ error: `Could not connect bot: ${err.message}` });
  }
});

fastify.post("/input", async (request, reply) => {
  const session = resolveSession(request, reply);
  if (!session) return;
  if (!session.ready)
    return reply.code(503).send({ error: "Bot not yet spawned" });

  const { code } = request.body ?? {};
  if (typeof code !== "string" || code.trim().length === 0) {
    return reply.code(400).send({ ok: false, error: 'Body must contain non-empty string field "code"' });
  }

  const ac = new AbortController();
  activeInputs.set(session.sessionToken, ac);

  try {
    const result = await runCodeInSession(session, code, EVAL_TIMEOUT_MS, ac.signal);
    reply.code(200).send({ ok: true, result });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    if (msg === "Execution interrupted") {
      reply.code(200).send({ ok: false, error: "Interrupted by /interrupt" });
    } else {
      fastify.log.warn({ err }, "Eval execution failed");
      reply.code(400).send({ ok: false, error: msg });
    }
  } finally {
    activeInputs.delete(session.sessionToken);
  }
});

fastify.post("/interrupt", async (request, reply) => {
  const session = resolveSession(request, reply);
  if (!session) return;
  if (!session.ready)
    return reply.code(503).send({ error: "Bot not yet spawned" });

  // Cancel any running /input
  const ac = activeInputs.get(session.sessionToken);
  if (ac) {
    ac.abort();
    activeInputs.delete(session.sessionToken);
  }

  // Stop bot movement
  try {
    session.bot.pathfinder.stop();
    session.bot.clearControlStates();
  } catch (_) {}

  const { code } = request.body ?? {};
  if (typeof code !== "string" || code.trim().length === 0) {
    return reply.code(200).send({ ok: true, result: "interrupted" });
  }

  // Execute priority code with short timeout, NOT cancellable
  try {
    const result = await runCodeInSession(session, code, INTERRUPT_TIMEOUT_MS, null);
    reply.code(200).send({ ok: true, result });
  } catch (err) {
    fastify.log.warn({ err }, "Interrupt code failed");
    reply.code(400).send({ ok: false, error: err instanceof Error ? err.message : String(err) });
  }
});

fastify.get("/ws", { websocket: true }, (socket, request) => {
  const token = request.query.session;
  if (!token) {
    socket.send(JSON.stringify({ error: "Missing ?session= query param" }));
    socket.close();
    return;
  }

  let session;
  try {
    session = getSession(token);
  } catch {
    socket.send(JSON.stringify({ error: "Invalid session" }));
    socket.close();
    return;
  }

  if (!wsClients.has(token)) wsClients.set(token, new Set());
  wsClients.get(token).add(socket);

  // Send current state on connect
  const bot = session.bot;
  const pos = bot.entity?.position;
  socket.send(
    JSON.stringify({
      type: "connected",
      data: {
        health: bot.health,
        food: bot.food,
        pos: pos ? { x: Math.round(pos.x), y: Math.round(pos.y), z: Math.round(pos.z) } : null,
        time: bot.time?.timeOfDay,
      },
    }),
  );

  socket.on("close", () => {
    const clients = wsClients.get(token);
    if (clients) {
      clients.delete(socket);
      if (clients.size === 0) wsClients.delete(token);
    }
  });
});

fastify.get("/skill.md", async (_request, reply) => {
  try {
    const content = await readFile(SKILL_MD_PATH, "utf8");
    reply.type("text/markdown").send(content);
  } catch {
    reply.code(404).send("EVAL_SKILL.md not found");
  }
});

fastify.post("/leave", async (request, reply) => {
  const token = request.headers["x-session"];
  if (token) destroySession(token);
  reply.code(200).send({ ok: true });
});

try {
  await fastify.listen({ port: PORT, host: "0.0.0.0" });
  console.log(`\nMinecraft eval adapter listening on http://localhost:${PORT}`);
  console.log(`Minecraft target: ${process.env.MC_HOST || "localhost"}:${process.env.MC_PORT || "25565"} (v${process.env.MC_VERSION || "1.21.4"})`);
  console.log(`\nEndpoints:\n  POST /input       — execute JS\n  POST /interrupt   — cancel + priority JS\n  GET  /ws          — real-time bot events (WebSocket)\n`);
} catch (err) {
  fastify.log.error(err);
  process.exit(1);
}

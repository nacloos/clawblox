/**
 * server.mjs
 *
 * Fastify HTTP server that exposes the Clawblox agent API, backed by mineflayer.
 *
 * Endpoints:
 *   POST /join?name=<botName>    — create bot session, connect to Minecraft
 *   GET  /observe                — get current world observation
 *   POST /input                  — send an agent action
 *   GET  /skill.md               — serve SKILL.md for this world
 *   POST /leave                  — disconnect bot
 *
 * Auth: all endpoints except /join and /skill.md require X-Session header.
 *
 * Configuration via environment variables:
 *   PORT           — adapter listen port (default: 8081)
 *   MC_HOST        — Minecraft server host (default: localhost)
 *   MC_PORT        — Minecraft server port (default: 25565)
 *   MC_VERSION     — Minecraft version (default: 1.21.4)
 *   MC_AUTH        — "offline" or "microsoft" (default: offline)
 */

import Fastify from "fastify";
import { readFile } from "fs/promises";
import { fileURLToPath } from "url";
import { dirname, join } from "path";
import { createSession, getSession, destroySession } from "./bot-manager.mjs";
import { buildObservation } from "./observation-builder.mjs";
import { handleInput } from "./input-handler.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILL_MD_PATH = join(__dirname, "SKILL.md");
const PORT = parseInt(process.env.PORT || "8081", 10);

const fastify = Fastify({ logger: true });

// ---------------------------------------------------------------------------
// Middleware: resolve session from X-Session header
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

/**
 * POST /join?name=<botName>
 * Creates a mineflayer bot session and connects to the Minecraft server.
 * Returns { session, agent_id }.
 */
fastify.post("/join", async (request, reply) => {
  const name = request.query.name || `agent_${Date.now()}`;

  try {
    const { sessionToken, agentId } = await createSession(name);
    reply.code(200).send({ session: sessionToken, agent_id: agentId });
  } catch (err) {
    fastify.log.error(err, "Failed to join Minecraft server");
    reply.code(503).send({ error: `Could not connect bot: ${err.message}` });
  }
});

/**
 * GET /observe
 * Returns a PlayerObservation for the current bot state.
 */
fastify.get("/observe", async (request, reply) => {
  const session = resolveSession(request, reply);
  if (!session) return;

  if (!session.ready) {
    return reply.code(503).send({ error: "Bot not yet spawned" });
  }

  reply.code(200).send(buildObservation(session));
});

/**
 * POST /input
 * Dispatch an agent input to the bot.
 * Body: { input_type: string, data: object }
 */
fastify.post("/input", async (request, reply) => {
  const session = resolveSession(request, reply);
  if (!session) return;

  if (!session.ready) {
    return reply.code(503).send({ error: "Bot not yet spawned" });
  }

  const { input_type, data } = request.body ?? {};
  if (!input_type) {
    return reply.code(400).send({ error: 'Missing "input_type" in body' });
  }

  const result = await handleInput(session.bot, input_type, data ?? {});

  if (result.ok) {
    reply.code(200).send({ ok: true });
  } else {
    reply.code(400).send({ ok: false, error: result.error });
  }
});

/**
 * GET /skill.md
 * Returns the Minecraft-specific SKILL.md describing available actions.
 */
fastify.get("/skill.md", async (_request, reply) => {
  try {
    const content = await readFile(SKILL_MD_PATH, "utf8");
    reply.type("text/markdown").send(content);
  } catch {
    reply.code(404).send("SKILL.md not found");
  }
});

/**
 * POST /leave
 * Disconnect the bot and clean up the session.
 */
fastify.post("/leave", async (request, reply) => {
  const token = request.headers["x-session"];
  if (token) destroySession(token);
  reply.code(200).send({ ok: true });
});

// ---------------------------------------------------------------------------
// Start server
// ---------------------------------------------------------------------------

try {
  await fastify.listen({ port: PORT, host: "0.0.0.0" });
  console.log(`\nMinecraft adapter listening on http://localhost:${PORT}`);
  console.log(`Minecraft target: ${process.env.MC_HOST || "localhost"}:${process.env.MC_PORT || "25565"} (v${process.env.MC_VERSION || "1.21.4"})`);
  console.log(`\nTo use with the Clawblox agent:\n  cd ../agent && WORLD_BASE_URL=http://localhost:${PORT} npm start\n`);
} catch (err) {
  fastify.log.error(err);
  process.exit(1);
}

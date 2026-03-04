/**
 * bot-manager.mjs
 *
 * Manages the lifecycle of mineflayer bots, one per active session.
 * Each session maps to a single Minecraft bot connection.
 */

import mineflayer from "mineflayer";
import pathfinderPkg from "mineflayer-pathfinder";
const { pathfinder, Movements } = pathfinderPkg;
import { v4 as uuidv4 } from "uuid";

const MC_HOST = process.env.MC_HOST || "localhost";
const MC_PORT = parseInt(process.env.MC_PORT || "25565", 10);
const MC_VERSION = process.env.MC_VERSION || "1.21.4";
const MC_AUTH = process.env.MC_AUTH || "offline"; // "offline" for local testing

/** @type {Map<string, BotSession>} session token → session */
const sessions = new Map();

/**
 * @typedef {Object} BotSession
 * @property {string} sessionToken
 * @property {string} agentId
 * @property {string} name
 * @property {import('mineflayer').Bot} bot
 * @property {string[]} pendingEvents   - accumulated events since last /observe
 * @property {boolean} ready            - true once bot has spawned
 */

/**
 * Create a new bot session and connect it to the Minecraft server.
 *
 * @param {string} name - Bot username
 * @returns {Promise<{sessionToken: string, agentId: string}>}
 */
export async function createSession(name) {
  const sessionToken = uuidv4();
  const agentId = uuidv4();

  const bot = mineflayer.createBot({
    host: MC_HOST,
    port: MC_PORT,
    username: name,
    version: MC_VERSION,
    auth: MC_AUTH,
  });

  bot.loadPlugin(pathfinder);

  /** @type {BotSession} */
  const session = {
    sessionToken,
    agentId,
    name,
    bot,
    pendingEvents: [],
    ready: false,
  };

  sessions.set(sessionToken, session);

  // Wire up event accumulation before resolving
  wireEvents(session);

  await new Promise((resolve, reject) => {
    bot.once("spawn", () => {
      session.ready = true;

      // Configure pathfinder default movements
      const defaultMovements = new Movements(bot);
      bot.pathfinder.setMovements(defaultMovements);

      resolve();
    });

    bot.once("error", reject);

    // Reject if bot is kicked before spawning
    bot.once("kicked", (reason) => {
      reject(new Error(`Kicked before spawn: ${reason}`));
    });
  });

  return { sessionToken, agentId };
}

/**
 * Retrieve an active session by its token.
 * Throws if not found.
 *
 * @param {string} sessionToken
 * @returns {BotSession}
 */
export function getSession(sessionToken) {
  const session = sessions.get(sessionToken);
  if (!session) throw new Error(`Unknown session: ${sessionToken}`);
  return session;
}

/**
 * Disconnect and clean up a session.
 *
 * @param {string} sessionToken
 */
export function destroySession(sessionToken) {
  const session = sessions.get(sessionToken);
  if (!session) return;
  try {
    session.bot.quit("left game");
  } catch (_) {
    // ignore errors on quit
  }
  sessions.delete(sessionToken);
}

/**
 * Wire Minecraft bot events into the session's pendingEvents queue.
 *
 * @param {BotSession} session
 */
function wireEvents(session) {
  const { bot } = session;
  const MAX_EVENTS = 50;

  function push(event) {
    session.pendingEvents.push(event);
    if (session.pendingEvents.length > MAX_EVENTS) {
      session.pendingEvents.shift();
    }
  }

  bot.on("chat", (username, message) => {
    if (username === session.name) return; // ignore own messages
    push({ type: "Chat", data: { username, message } });
  });

  bot.on("health", () => {
    push({ type: "HealthChanged", data: { health: bot.health, food: bot.food } });
  });

  bot.on("death", () => {
    push({ type: "Death", data: {} });
  });

  bot.on("entityHurt", (entity) => {
    if (entity === bot.entity) {
      push({ type: "Hurt", data: { health: bot.health } });
    }
  });

  bot.on("blockBreakProgressEnd", (block) => {
    push({
      type: "BlockBroken",
      data: { position: block.position, name: block.name },
    });
  });

  bot.on("playerJoined", (player) => {
    push({ type: "PlayerJoined", data: { username: player.username } });
  });

  bot.on("playerLeft", (player) => {
    push({ type: "PlayerLeft", data: { username: player.username } });
  });

  bot.on("kicked", (reason) => {
    push({ type: "Kicked", data: { reason } });
    sessions.delete(session.sessionToken);
  });

  bot.on("end", () => {
    sessions.delete(session.sessionToken);
  });
}

/**
 * Drain and return accumulated events for a session.
 *
 * @param {BotSession} session
 * @returns {Array<{type: string, data: object}>}
 */
export function drainEvents(session) {
  const events = [...session.pendingEvents];
  session.pendingEvents.length = 0;
  return events;
}

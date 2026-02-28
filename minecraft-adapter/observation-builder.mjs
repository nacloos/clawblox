/**
 * observation-builder.mjs
 *
 * Translates mineflayer bot state into a Clawblox-compatible PlayerObservation.
 *
 * PlayerObservation schema (mirrors Clawblox):
 * {
 *   tick: number,
 *   game_status: "active" | "finished",
 *   player: { id, position, health, attributes },
 *   other_players: [{ id, name, position }],
 *   world: { entities: [{ id, name, position, attributes }] },
 *   events: [{ type, data }],
 * }
 */

import { drainEvents } from "./bot-manager.mjs";

const NEARBY_ENTITY_RADIUS = 32; // blocks
const NEARBY_PLAYER_RADIUS = 64; // blocks
const NEARBY_BLOCKS_HALF = 4;    // 9×9×9 grid (±4 around player)

/**
 * Build a PlayerObservation from the current bot state.
 *
 * @param {import('./bot-manager.mjs').BotSession} session
 * @returns {object} PlayerObservation
 */
export function buildObservation(session) {
  const { bot, agentId } = session;

  return {
    tick: bot.game?.age ?? 0,
    game_status: "active",
    player: buildPlayerInfo(bot, agentId),
    other_players: buildOtherPlayers(bot),
    world: buildWorldInfo(bot),
    events: drainEvents(session),
  };
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function round2(n) {
  return Math.round(n * 100) / 100;
}

function dist(a, b) {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  const dz = a.z - b.z;
  return Math.sqrt(dx * dx + dy * dy + dz * dz);
}

function buildPlayerInfo(bot, agentId) {
  const pos = bot.entity?.position ?? { x: 0, y: 0, z: 0 };
  return {
    id: agentId,
    position: [round2(pos.x), round2(pos.y), round2(pos.z)],
    health: Math.round(bot.health ?? 20),
    attributes: {
      food: Math.round(bot.food ?? 20),
      food_saturation: round2(bot.foodSaturation ?? 5),
      on_ground: bot.entity?.onGround ?? false,
      yaw: round2(bot.entity?.yaw ?? 0),
      pitch: round2(bot.entity?.pitch ?? 0),
      time_of_day: bot.time?.timeOfDay ?? 0,
      is_day: (bot.time?.isDay) ?? true,
      game_mode: bot.game?.gameMode ?? "survival",
      inventory: summarizeInventory(bot),
      nearby_blocks: buildNearbyBlocks(bot),
    },
  };
}

function buildOtherPlayers(bot) {
  const myPos = bot.entity?.position;
  if (!myPos) return [];

  return Object.values(bot.players)
    .filter((p) => p.entity && p.username !== bot.username)
    .filter((p) => dist(p.entity.position, myPos) <= NEARBY_PLAYER_RADIUS)
    .map((p) => ({
      id: p.uuid,
      name: p.username,
      position: [
        round2(p.entity.position.x),
        round2(p.entity.position.y),
        round2(p.entity.position.z),
      ],
    }));
}

function buildWorldInfo(bot) {
  const myPos = bot.entity?.position;
  if (!myPos) return { entities: [] };

  const entities = Object.values(bot.entities)
    .filter((e) => e !== bot.entity && e.position)
    .filter((e) => dist(e.position, myPos) <= NEARBY_ENTITY_RADIUS)
    .map((e) => ({
      id: e.id,
      name: e.name ?? e.displayName ?? "unknown",
      type: e.type,
      position: [round2(e.position.x), round2(e.position.y), round2(e.position.z)],
      attributes:
        e.type === "mob"
          ? { health: e.metadata?.[9] ?? null }
          : {},
    }));

  return { entities };
}

/**
 * Build a compact 5×5×5 grid of block names around the player.
 * Returns a flat array of { dx, dy, dz, name } objects for non-air blocks.
 */
function buildNearbyBlocks(bot) {
  const pos = bot.entity?.position;
  if (!pos || !bot.blockAt) return [];

  const blocks = [];
  const floored = pos.floored();
  const h = NEARBY_BLOCKS_HALF;

  for (let dx = -h; dx <= h; dx++) {
    for (let dy = -h; dy <= h; dy++) {
      for (let dz = -h; dz <= h; dz++) {
        const block = bot.blockAt(floored.offset(dx, dy, dz));
        if (block && block.name !== "air") {
          blocks.push({ dx, dy, dz, name: block.name });
        }
      }
    }
  }
  return blocks;
}

/**
 * Summarize bot inventory as a list of { name, count } items.
 */
function summarizeInventory(bot) {
  if (!bot.inventory) return [];

  const counts = new Map();
  for (const item of bot.inventory.items()) {
    const prev = counts.get(item.name) ?? 0;
    counts.set(item.name, prev + item.count);
  }
  return [...counts.entries()].map(([name, count]) => ({ name, count }));
}

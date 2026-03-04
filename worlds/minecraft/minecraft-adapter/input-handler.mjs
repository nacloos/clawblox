/**
 * input-handler.mjs
 *
 * Translates Clawblox-style agent inputs into mineflayer bot actions.
 *
 * Each handler receives (bot, data) and may return a Promise.
 * Handlers should NOT throw — return { ok: false, error } instead.
 */

import pathfinderPkg from "mineflayer-pathfinder";
const { goals, Movements } = pathfinderPkg;

const { GoalBlock, GoalNear, GoalXZ } = goals;

/**
 * Dispatch an agent input to the appropriate bot action.
 *
 * @param {import('mineflayer').Bot} bot
 * @param {string} inputType
 * @param {object} data
 * @returns {Promise<{ok: boolean, error?: string}>}
 */
export async function handleInput(bot, inputType, data) {
  const handler = HANDLERS[inputType];
  if (!handler) {
    return { ok: false, error: `Unknown input type: ${inputType}` };
  }
  try {
    await handler(bot, data ?? {});
    return { ok: true };
  } catch (err) {
    console.warn(`[input-handler] ${inputType} failed:`, err.message);
    return { ok: false, error: err.message };
  }
}

// ---------------------------------------------------------------------------
// Input handlers
// ---------------------------------------------------------------------------

const HANDLERS = {
  /**
   * MoveTo: navigate to a world position using pathfinder.
   * data: { position: [x, y, z], range?: number }
   */
  MoveTo: async (bot, data) => {
    const [x, y, z] = requirePosition(data);
    const range = data.range ?? 1;
    const goal = new GoalNear(x, y, z, range);
    bot.pathfinder.setGoal(goal);
    // Non-blocking: agent polls /observe to track progress
  },

  /**
   * GoTo: alias of MoveTo but waits until arrival.
   * data: { position: [x, y, z], range?: number }
   */
  GoTo: async (bot, data) => {
    const [x, y, z] = requirePosition(data);
    const range = data.range ?? 1;
    await bot.pathfinder.goto(new GoalNear(x, y, z, range));
  },

  /**
   * Stop: cancel pathfinding and release movement controls.
   * data: {}
   */
  Stop: async (bot, _data) => {
    bot.pathfinder.stop();
    for (const ctrl of ["forward", "back", "left", "right", "jump", "sprint", "sneak"]) {
      bot.setControlState(ctrl, false);
    }
  },

  /**
   * Jump: trigger a single jump.
   * data: {}
   */
  Jump: async (bot, _data) => {
    bot.setControlState("jump", true);
    await sleep(100);
    bot.setControlState("jump", false);
  },

  /**
   * Look: rotate camera to yaw/pitch or face a position.
   * data: { yaw?: number, pitch?: number } OR { position: [x, y, z] }
   */
  Look: async (bot, data) => {
    if (data.position) {
      const [x, y, z] = requirePosition(data);
      await bot.lookAt({ x, y, z });
    } else {
      const yaw = data.yaw ?? bot.entity.yaw;
      const pitch = data.pitch ?? bot.entity.pitch;
      await bot.look(yaw, pitch, false);
    }
  },

  /**
   * Attack: hit the nearest mob or a specific entity by id.
   * data: { entity_id?: number }
   */
  Attack: async (bot, data) => {
    let target;
    if (data.entity_id != null) {
      target = bot.entities[data.entity_id];
      if (!target) throw new Error(`Entity ${data.entity_id} not found`);
    } else {
      target = nearestHostile(bot);
      if (!target) throw new Error("No hostile entity nearby");
    }
    await bot.lookAt(target.position.offset(0, target.height, 0));
    bot.attack(target);
  },

  /**
   * DigBlock: break the block at a given position.
   * data: { position: [x, y, z] }
   */
  DigBlock: async (bot, data) => {
    const [x, y, z] = requirePosition(data);
    const block = bot.blockAt({ x, y, z });
    if (!block) throw new Error("No block at position");
    if (!bot.canDigBlock(block)) throw new Error(`Cannot dig ${block.name}`);
    await bot.dig(block);
  },

  /**
   * PlaceBlock: place the currently held block against a surface.
   * data: { position: [x, y, z], face?: "top"|"bottom"|"north"|"south"|"east"|"west" }
   */
  PlaceBlock: async (bot, data) => {
    const [x, y, z] = requirePosition(data);
    const refBlock = bot.blockAt({ x, y, z });
    if (!refBlock) throw new Error("No reference block at position");
    const faceVec = parseFace(data.face ?? "top");
    await bot.placeBlock(refBlock, faceVec);
  },

  /**
   * EquipItem: equip an item by name.
   * data: { item_name: string, destination?: "hand"|"off-hand"|"head"|"torso"|"legs"|"feet" }
   */
  EquipItem: async (bot, data) => {
    const name = requireString(data, "item_name");
    const dest = data.destination ?? "hand";
    const item = bot.inventory.findInventoryItem(bot.registry.itemsByName[name]?.id, null);
    if (!item) throw new Error(`Item not in inventory: ${name}`);
    await bot.equip(item, dest);
  },

  /**
   * UseItem: activate/use the currently held item.
   * data: {}
   */
  UseItem: async (bot, _data) => {
    await bot.activateItem();
  },

  /**
   * Chat: send a chat message.
   * data: { message: string }
   */
  Chat: async (bot, data) => {
    const msg = requireString(data, "message");
    bot.chat(msg);
  },

  /**
   * Eat: consume a food item from inventory by name.
   * data: { item_name: string }
   */
  Eat: async (bot, data) => {
    const name = requireString(data, "item_name");
    const food = bot.inventory.findInventoryItem(bot.registry.itemsByName[name]?.id, null);
    if (!food) throw new Error(`Food not in inventory: ${name}`);
    await bot.equip(food, "hand");
    await bot.consume();
  },

  /**
   * Craft: craft an item if recipe is available.
   * data: { item_name: string, count?: number }
   */
  Craft: async (bot, data) => {
    const name = requireString(data, "item_name");
    const count = data.count ?? 1;
    const itemId = bot.registry.itemsByName[name]?.id;
    if (itemId == null) throw new Error(`Unknown item: ${name}`);
    const recipes = bot.recipesFor(itemId, null, 1, null);
    if (!recipes.length) throw new Error(`No recipe available for: ${name}`);
    await bot.craft(recipes[0], count, null);
  },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function requirePosition(data) {
  const pos = data.position;
  if (!Array.isArray(pos) || pos.length < 3) {
    throw new Error('Missing or invalid "position" field (expected [x, y, z])');
  }
  return [pos[0], pos[1], pos[2]];
}

function requireString(data, field) {
  const val = data[field];
  if (typeof val !== "string" || !val) {
    throw new Error(`Missing or invalid "${field}" field`);
  }
  return val;
}

const FACE_VECTORS = {
  top:    { x: 0,  y: 1,  z: 0  },
  bottom: { x: 0,  y: -1, z: 0  },
  north:  { x: 0,  y: 0,  z: -1 },
  south:  { x: 0,  y: 0,  z: 1  },
  west:   { x: -1, y: 0,  z: 0  },
  east:   { x: 1,  y: 0,  z: 0  },
};

function parseFace(face) {
  const vec = FACE_VECTORS[face.toLowerCase()];
  if (!vec) throw new Error(`Unknown face: ${face}. Use top/bottom/north/south/east/west`);
  return vec;
}

const HOSTILE_TYPES = new Set([
  "zombie", "skeleton", "spider", "creeper", "witch",
  "enderman", "blaze", "ghast", "slime", "magma_cube",
]);

function nearestHostile(bot) {
  const myPos = bot.entity.position;
  let nearest = null;
  let nearestDist = Infinity;
  for (const entity of Object.values(bot.entities)) {
    if (entity === bot.entity) continue;
    if (!HOSTILE_TYPES.has(entity.name)) continue;
    const dx = entity.position.x - myPos.x;
    const dz = entity.position.z - myPos.z;
    const d = Math.sqrt(dx * dx + dz * dz);
    if (d < nearestDist) {
      nearest = entity;
      nearestDist = d;
    }
  }
  return nearest;
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

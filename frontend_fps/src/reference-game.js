import * as THREE from 'three';

// ─── Constants ───────────────────────────────────────────────────────────
const GRAVITY = 25;
const JUMP_FORCE = 9;
const PLAYER_HEIGHT = 1.7;
const PLAYER_RADIUS = 0.4;
const MOVE_SPEED = 6;
const SPRINT_MULT = 1.6;
const MOUSE_SENS = 0.002;
const MAP_SIZE = 60;
const WALL_HEIGHT = 4;

// ─── Weapon Definitions ─────────────────────────────────────────────────
const WEAPONS = [
  {
    name: 'Pistol', magSize: 12, reserve: 60, fireRate: 0.25, damage: 25,
    spread: 0.015, recoil: 0.03, reloadTime: 1.5, auto: false,
    color: 0xcccccc, barrelLen: 0.4, bodyW: 0.04, bodyH: 0.08
  },
  {
    name: 'Assault Rifle', magSize: 30, reserve: 120, fireRate: 0.09, damage: 18,
    spread: 0.025, recoil: 0.02, reloadTime: 2.0, auto: true,
    color: 0x444444, barrelLen: 0.7, bodyW: 0.04, bodyH: 0.06
  },
  {
    name: 'Shotgun', magSize: 8, reserve: 32, fireRate: 0.7, damage: 12,
    spread: 0.08, recoil: 0.06, reloadTime: 2.5, auto: false, pellets: 8,
    color: 0x8B4513, barrelLen: 0.8, bodyW: 0.04, bodyH: 0.05
  }
];

// ─── Game State ──────────────────────────────────────────────────────────
const state = {
  health: 100, score: 0, kills: 0, wave: 1,
  shotsFired: 0, shotsHit: 0,
  currentWeapon: 1,
  weapons: WEAPONS.map(w => ({ mag: w.magSize, reserve: w.reserve, reloading: false, reloadTimer: 0, fireTimer: 0 })),
  velocity: new THREE.Vector3(),
  onGround: false,
  sprinting: false,
  moveF: 0, moveR: 0,
  jumping: false,
  mouseDown: false,
  alive: true,
  started: false,
  enemies: [],
  pickups: [],
  bullets: [],
  particles: [],
  decals: [],
  spawnTimer: 0,
  waveEnemies: 0,
  waveKillTarget: 5,
  keys: {}
};

// ─── Renderer Setup ──────────────────────────────────────────────────────
const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: 'high-performance' });
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = THREE.PCFSoftShadowMap;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.8;
renderer.outputColorSpace = THREE.SRGBColorSpace;
document.body.appendChild(renderer.domElement);

const scene = new THREE.Scene();
scene.fog = new THREE.FogExp2(0x1a1a2a, 0.008);
scene.background = new THREE.Color(0x1a1a2a);

const camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 200);
camera.position.set(0, PLAYER_HEIGHT, 0);

const listener = new THREE.AudioListener();
camera.add(listener);

const pitchObject = new THREE.Object3D();
pitchObject.add(camera);
const yawObject = new THREE.Object3D();
yawObject.position.set(0, 0, 0);
yawObject.add(pitchObject);
scene.add(yawObject);

// ─── Weapon View Model ───────────────────────────────────────────────────
const weaponGroup = new THREE.Group();
camera.add(weaponGroup);
weaponGroup.position.set(0.25, -0.2, -0.4);

let currentWeaponMesh = null;
function buildWeaponModel(idx) {
  if (currentWeaponMesh) weaponGroup.remove(currentWeaponMesh);
  const w = WEAPONS[idx];
  const group = new THREE.Group();

  const bodyMat = new THREE.MeshStandardMaterial({ color: w.color, roughness: 0.3, metalness: 0.8 });
  const body = new THREE.Mesh(new THREE.BoxGeometry(w.bodyW, w.bodyH, w.barrelLen), bodyMat);
  group.add(body);

  // barrel
  const barrel = new THREE.Mesh(
    new THREE.CylinderGeometry(0.012, 0.015, w.barrelLen * 0.6, 8),
    new THREE.MeshStandardMaterial({ color: 0x222222, roughness: 0.2, metalness: 0.9 })
  );
  barrel.rotation.x = Math.PI / 2;
  barrel.position.z = -w.barrelLen * 0.5;
  group.add(barrel);

  // grip
  const grip = new THREE.Mesh(
    new THREE.BoxGeometry(0.03, 0.1, 0.04),
    bodyMat
  );
  grip.position.set(0, -0.07, 0.05);
  grip.rotation.x = 0.2;
  group.add(grip);

  if (idx === 1) {
    // magazine for rifle
    const mag = new THREE.Mesh(
      new THREE.BoxGeometry(0.025, 0.08, 0.04),
      new THREE.MeshStandardMaterial({ color: 0x333333, roughness: 0.4, metalness: 0.7 })
    );
    mag.position.set(0, -0.06, -0.05);
    group.add(mag);

    // stock
    const stock = new THREE.Mesh(
      new THREE.BoxGeometry(0.035, 0.05, 0.15),
      bodyMat
    );
    stock.position.set(0, 0.01, 0.25);
    group.add(stock);
  }

  if (idx === 2) {
    // pump for shotgun
    const pump = new THREE.Mesh(
      new THREE.BoxGeometry(0.035, 0.04, 0.12),
      new THREE.MeshStandardMaterial({ color: 0x654321, roughness: 0.5, metalness: 0.3 })
    );
    pump.position.set(0, -0.03, -0.15);
    group.add(pump);
  }

  weaponGroup.add(group);
  currentWeaponMesh = group;
}
buildWeaponModel(1);

// Muzzle flash light
const muzzleFlash = new THREE.PointLight(0xffaa44, 0, 8);
muzzleFlash.position.set(0, 0, -0.8);
weaponGroup.add(muzzleFlash);

// ─── Lighting ────────────────────────────────────────────────────────────
const ambientLight = new THREE.AmbientLight(0x8888aa, 1.2);
scene.add(ambientLight);

const dirLight = new THREE.DirectionalLight(0x8899cc, 1.5);
dirLight.position.set(20, 30, 10);
dirLight.castShadow = true;
dirLight.shadow.mapSize.set(2048, 2048);
dirLight.shadow.camera.left = -40;
dirLight.shadow.camera.right = 40;
dirLight.shadow.camera.top = 40;
dirLight.shadow.camera.bottom = -40;
dirLight.shadow.camera.near = 0.5;
dirLight.shadow.camera.far = 80;
dirLight.shadow.bias = -0.001;
scene.add(dirLight);

// Colored accent lights throughout the arena
const accentColors = [0xff4444, 0x4488ff, 0xff8800, 0x44ff88, 0xff44ff, 0xffff44];
const lightPositions = [
  [-15, 3, -15], [15, 3, -15], [-15, 3, 15], [15, 3, 15],
  [0, 3, -20], [0, 3, 20], [-20, 3, 0], [20, 3, 0],
  [-8, 3, -8], [8, 3, 8], [-8, 3, 8], [8, 3, -8]
];
const accentLights = [];
lightPositions.forEach((pos, i) => {
  const light = new THREE.PointLight(accentColors[i % accentColors.length], 4, 20);
  light.position.set(...pos);
  scene.add(light);
  accentLights.push(light);
});

// ─── Materials ───────────────────────────────────────────────────────────
function createFloorTexture() {
  const canvas = document.createElement('canvas');
  canvas.width = 512; canvas.height = 512;
  const ctx = canvas.getContext('2d');
  ctx.fillStyle = '#3a3a3a';
  ctx.fillRect(0, 0, 512, 512);
  // Grid lines
  ctx.strokeStyle = '#4a4a4a';
  ctx.lineWidth = 2;
  for (let i = 0; i <= 512; i += 64) {
    ctx.beginPath(); ctx.moveTo(i, 0); ctx.lineTo(i, 512); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(0, i); ctx.lineTo(512, i); ctx.stroke();
  }
  // Subtle noise
  for (let i = 0; i < 2000; i++) {
    const x = Math.random() * 512, y = Math.random() * 512;
    const v = 50 + Math.random() * 20;
    ctx.fillStyle = `rgb(${v},${v},${v})`;
    ctx.fillRect(x, y, 2, 2);
  }
  const tex = new THREE.CanvasTexture(canvas);
  tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
  tex.repeat.set(15, 15);
  return tex;
}

function createWallTexture() {
  const canvas = document.createElement('canvas');
  canvas.width = 256; canvas.height = 256;
  const ctx = canvas.getContext('2d');
  ctx.fillStyle = '#3a3a42';
  ctx.fillRect(0, 0, 256, 256);
  // Concrete-like texture
  for (let i = 0; i < 3000; i++) {
    const x = Math.random() * 256, y = Math.random() * 256;
    const v = 50 + Math.random() * 25;
    ctx.fillStyle = `rgb(${v},${v},${v + 5})`;
    ctx.fillRect(x, y, Math.random() * 3 + 1, Math.random() * 3 + 1);
  }
  // Panel lines
  ctx.strokeStyle = '#2e2e36';
  ctx.lineWidth = 2;
  [64, 128, 192].forEach(y => { ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(256, y); ctx.stroke(); });
  const tex = new THREE.CanvasTexture(canvas);
  tex.wrapS = tex.wrapT = THREE.RepeatWrapping;
  return tex;
}

const floorMat = new THREE.MeshStandardMaterial({ map: createFloorTexture(), roughness: 0.85, metalness: 0.1 });
const wallTex = createWallTexture();
const wallMat = new THREE.MeshStandardMaterial({ map: wallTex, roughness: 0.7, metalness: 0.2 });
const crateMat = new THREE.MeshStandardMaterial({ color: 0x6B4226, roughness: 0.9, metalness: 0.05 });
const metalMat = new THREE.MeshStandardMaterial({ color: 0x555566, roughness: 0.3, metalness: 0.8 });
const glowRedMat = new THREE.MeshStandardMaterial({ color: 0xff2222, emissive: 0xff2222, emissiveIntensity: 0.5 });
const glowBlueMat = new THREE.MeshStandardMaterial({ color: 0x2244ff, emissive: 0x2244ff, emissiveIntensity: 0.5 });

// ─── Collision System ────────────────────────────────────────────────────
const colliders = []; // Array of {min, max} AABB

function addColliderBox(x, y, z, sx, sy, sz) {
  colliders.push({
    min: new THREE.Vector3(x - sx/2, y - sy/2, z - sz/2),
    max: new THREE.Vector3(x + sx/2, y + sy/2, z + sz/2)
  });
}

function checkCollision(pos, radius) {
  for (const box of colliders) {
    const closest = new THREE.Vector3(
      Math.max(box.min.x, Math.min(pos.x, box.max.x)),
      Math.max(box.min.y, Math.min(pos.y, box.max.y)),
      Math.max(box.min.z, Math.min(pos.z, box.max.z))
    );
    const dist = pos.distanceTo(closest);
    if (dist < radius) {
      return { hit: true, box, closest, dist };
    }
  }
  return { hit: false };
}

function resolveCollision(pos, radius) {
  for (let iter = 0; iter < 4; iter++) {
    let pushed = false;
    for (const box of colliders) {
      const closest = new THREE.Vector3(
        Math.max(box.min.x, Math.min(pos.x, box.max.x)),
        Math.max(box.min.y, Math.min(pos.y, box.max.y)),
        Math.max(box.min.z, Math.min(pos.z, box.max.z))
      );
      const diff = new THREE.Vector3().subVectors(pos, closest);
      const dist = diff.length();
      if (dist < radius && dist > 0.001) {
        diff.normalize().multiplyScalar(radius - dist);
        pos.add(diff);
        pushed = true;
      }
    }
    if (!pushed) break;
  }
}

// ─── Level Building ──────────────────────────────────────────────────────
function addWall(x, z, sx, sz, ht = WALL_HEIGHT) {
  const geo = new THREE.BoxGeometry(sx, ht, sz);
  const mesh = new THREE.Mesh(geo, wallMat);
  mesh.position.set(x, ht / 2, z);
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  scene.add(mesh);
  addColliderBox(x, ht / 2, z, sx, ht, sz);
  return mesh;
}

function addCrate(x, z, size = 1.2, ht = 1.2) {
  const geo = new THREE.BoxGeometry(size, ht, size);
  const mesh = new THREE.Mesh(geo, crateMat);
  mesh.position.set(x, ht / 2, z);
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  scene.add(mesh);
  addColliderBox(x, ht / 2, z, size, ht, size);

  // Crate edge detail
  const edgeMat = new THREE.MeshStandardMaterial({ color: 0x444444, roughness: 0.4, metalness: 0.6 });
  const edgeGeo = new THREE.BoxGeometry(size + 0.02, 0.04, 0.04);
  [-1, 1].forEach(sy => {
    const edge = new THREE.Mesh(edgeGeo, edgeMat);
    edge.position.set(x, sy * ht / 2 + ht / 2, z);
    scene.add(edge);
  });
}

function addPillar(x, z, radius = 0.5, ht = WALL_HEIGHT) {
  const geo = new THREE.CylinderGeometry(radius, radius, ht, 12);
  const mesh = new THREE.Mesh(geo, metalMat);
  mesh.position.set(x, ht / 2, z);
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  scene.add(mesh);
  // Approximate cylinder with box for collider
  addColliderBox(x, ht / 2, z, radius * 1.6, ht, radius * 1.6);
}

function buildLevel() {
  // Floor
  const floor = new THREE.Mesh(new THREE.PlaneGeometry(MAP_SIZE, MAP_SIZE), floorMat);
  floor.rotation.x = -Math.PI / 2;
  floor.receiveShadow = true;
  scene.add(floor);

  // Ceiling
  const ceiling = new THREE.Mesh(
    new THREE.PlaneGeometry(MAP_SIZE, MAP_SIZE),
    new THREE.MeshStandardMaterial({ color: 0x2a2a30, roughness: 0.9 })
  );
  ceiling.rotation.x = Math.PI / 2;
  ceiling.position.y = WALL_HEIGHT;
  scene.add(ceiling);

  const hs = MAP_SIZE / 2;
  const wt = 0.5; // wall thickness

  // Outer walls
  addWall(0, -hs, MAP_SIZE, wt);
  addWall(0, hs, MAP_SIZE, wt);
  addWall(-hs, 0, wt, MAP_SIZE);
  addWall(hs, 0, wt, MAP_SIZE);

  // Interior walls - creating a complex arena layout
  // Central structure
  addWall(-4, -4, 8, 0.5);
  addWall(-4, 4, 8, 0.5);
  addWall(-8, 0, 0.5, 8);
  addWall(8, 0, 0.5, 8);

  // Side rooms
  addWall(-18, -8, 0.5, 10);
  addWall(-18, 8, 0.5, 10);
  addWall(18, -8, 0.5, 10);
  addWall(18, 8, 0.5, 10);

  // Corridors
  addWall(-12, -15, 8, 0.5);
  addWall(12, -15, 8, 0.5);
  addWall(-12, 15, 8, 0.5);
  addWall(12, 15, 8, 0.5);

  // Angled cover
  addWall(-22, -20, 6, 0.5);
  addWall(22, -20, 6, 0.5);
  addWall(-22, 20, 6, 0.5);
  addWall(22, 20, 6, 0.5);

  // Short walls for cover
  const shortH = 1.5;
  const shortGeo = new THREE.BoxGeometry(3, shortH, 0.4);
  [[-10, -10], [10, -10], [-10, 10], [10, 10], [0, -12], [0, 12]].forEach(([x, z]) => {
    const mesh = new THREE.Mesh(shortGeo, wallMat);
    mesh.position.set(x, shortH / 2, z);
    mesh.castShadow = true; mesh.receiveShadow = true;
    scene.add(mesh);
    addColliderBox(x, shortH / 2, z, 3, shortH, 0.4);
  });

  // Pillars
  [[-6, -6], [6, -6], [-6, 6], [6, 6],
   [-14, 0], [14, 0], [0, -20], [0, 20],
   [-22, -10], [22, -10], [-22, 10], [22, 10]].forEach(([x, z]) => {
    addPillar(x, z);
  });

  // Crate clusters
  [[-3, -18], [-1, -18], [-2, -16],
   [3, 18], [1, 18], [2, 16],
   [-20, -2], [-20, 2],
   [20, -2], [20, 2],
   [-15, -22], [15, 22],
   [-25, 0], [25, 0]].forEach(([x, z]) => {
    addCrate(x, z, 1.0 + Math.random() * 0.5, 0.8 + Math.random() * 0.8);
  });

  // Elevated platforms
  [[0, 0, 8, 0.3, 8], [-20, -20, 6, 0.3, 6], [20, 20, 6, 0.3, 6]].forEach(([x, y, sx, sy, sz]) => {
    const plat = new THREE.Mesh(
      new THREE.BoxGeometry(sx, sy, sz),
      new THREE.MeshStandardMaterial({ color: 0x2a2a30, roughness: 0.6, metalness: 0.3 })
    );
    plat.position.set(x, y, 0);
    plat.receiveShadow = true;
    scene.add(plat);
  });

  // Glowing trim strips along walls
  const stripGeo = new THREE.BoxGeometry(0.1, 0.1, MAP_SIZE);
  [-hs + 0.3, hs - 0.3].forEach(x => {
    const strip = new THREE.Mesh(stripGeo, x < 0 ? glowRedMat : glowBlueMat);
    strip.position.set(x, 0.5, 0);
    scene.add(strip);
  });
  const stripGeoH = new THREE.BoxGeometry(MAP_SIZE, 0.1, 0.1);
  [-hs + 0.3, hs - 0.3].forEach(z => {
    const strip = new THREE.Mesh(stripGeoH, z < 0 ? glowRedMat : glowBlueMat);
    strip.position.set(0, 0.5, z);
    scene.add(strip);
  });
}

buildLevel();

// ─── Raycaster ───────────────────────────────────────────────────────────
const raycaster = new THREE.Raycaster();
const rayTargets = [];
scene.traverse(obj => { if (obj.isMesh) rayTargets.push(obj); });

// ─── Enemy System ────────────────────────────────────────────────────────
const enemyGroup = new THREE.Group();
scene.add(enemyGroup);

function createEnemy(type = 'grunt') {
  const group = new THREE.Group();

  const configs = {
    grunt: { health: 60, speed: 3, damage: 8, color: 0xcc3333, score: 100, size: 1 },
    heavy: { health: 150, speed: 1.8, damage: 15, color: 0x8833cc, score: 250, size: 1.3 },
    fast: { health: 30, speed: 6, damage: 5, color: 0x33cc33, score: 150, size: 0.8 }
  };
  const cfg = configs[type];

  // Body
  const bodyGeo = new THREE.CapsuleGeometry(0.35 * cfg.size, 0.6 * cfg.size, 4, 8);
  const bodyMat = new THREE.MeshStandardMaterial({
    color: cfg.color, roughness: 0.5, metalness: 0.3,
    emissive: cfg.color, emissiveIntensity: 0.15
  });
  const body = new THREE.Mesh(bodyGeo, bodyMat);
  body.position.y = 0.9 * cfg.size;
  body.castShadow = true;
  group.add(body);

  // Head
  const headGeo = new THREE.SphereGeometry(0.2 * cfg.size, 8, 8);
  const headMat = new THREE.MeshStandardMaterial({ color: 0xddccbb, roughness: 0.6 });
  const head = new THREE.Mesh(headGeo, headMat);
  head.position.y = 1.5 * cfg.size;
  head.castShadow = true;
  group.add(head);

  // Eyes (glowing)
  const eyeMat = new THREE.MeshStandardMaterial({ color: cfg.color, emissive: cfg.color, emissiveIntensity: 2 });
  [-0.08, 0.08].forEach(xOff => {
    const eye = new THREE.Mesh(new THREE.SphereGeometry(0.04 * cfg.size, 6, 6), eyeMat);
    eye.position.set(xOff * cfg.size, 1.55 * cfg.size, -0.17 * cfg.size);
    group.add(eye);
  });

  // Arms
  const armGeo = new THREE.CapsuleGeometry(0.08 * cfg.size, 0.4 * cfg.size, 3, 6);
  const armMat = new THREE.MeshStandardMaterial({ color: cfg.color, roughness: 0.5 });
  [-1, 1].forEach(side => {
    const arm = new THREE.Mesh(armGeo, armMat);
    arm.position.set(side * 0.4 * cfg.size, 0.9 * cfg.size, 0);
    arm.castShadow = true;
    group.add(arm);
  });

  // Point light glow
  const glow = new THREE.PointLight(cfg.color, 0.5, 5);
  glow.position.y = 1;
  group.add(glow);

  // Spawn position
  const angle = Math.random() * Math.PI * 2;
  const dist = 15 + Math.random() * 12;
  group.position.set(Math.cos(angle) * dist, 0, Math.sin(angle) * dist);

  // Clamp to map
  const limit = MAP_SIZE / 2 - 2;
  group.position.x = Math.max(-limit, Math.min(limit, group.position.x));
  group.position.z = Math.max(-limit, Math.min(limit, group.position.z));

  enemyGroup.add(group);

  const enemy = {
    mesh: group, body, head, type,
    health: cfg.health, maxHealth: cfg.health,
    speed: cfg.speed, damage: cfg.damage,
    score: cfg.score, size: cfg.size,
    color: cfg.color,
    attackTimer: 0, attackCooldown: 1.0,
    lastSeen: 0, wanderAngle: Math.random() * Math.PI * 2,
    hitFlash: 0
  };

  state.enemies.push(enemy);
  return enemy;
}

function spawnWave() {
  const types = ['grunt'];
  if (state.wave >= 2) types.push('fast');
  if (state.wave >= 3) types.push('heavy');

  const count = 3 + state.wave * 2;
  state.waveKillTarget = state.kills + count;
  state.waveEnemies = count;

  for (let i = 0; i < count; i++) {
    const type = types[Math.floor(Math.random() * types.length)];
    createEnemy(type);
  }
}

// ─── Pickup System ───────────────────────────────────────────────────────
function spawnPickup(type, x, z) {
  const group = new THREE.Group();
  let color, emissive;

  if (type === 'health') {
    color = 0x44ff44; emissive = 0x22aa22;
    const cross1 = new THREE.Mesh(new THREE.BoxGeometry(0.4, 0.12, 0.12),
      new THREE.MeshStandardMaterial({ color, emissive, emissiveIntensity: 1 }));
    const cross2 = new THREE.Mesh(new THREE.BoxGeometry(0.12, 0.4, 0.12),
      new THREE.MeshStandardMaterial({ color, emissive, emissiveIntensity: 1 }));
    group.add(cross1, cross2);
  } else if (type === 'ammo') {
    color = 0xffaa22; emissive = 0xaa7711;
    const box = new THREE.Mesh(new THREE.BoxGeometry(0.3, 0.2, 0.15),
      new THREE.MeshStandardMaterial({ color, emissive, emissiveIntensity: 1 }));
    group.add(box);
  }

  const glow = new THREE.PointLight(color, 1, 5);
  group.add(glow);

  group.position.set(x, 0.7, z);
  scene.add(group);

  state.pickups.push({ mesh: group, type, time: 0 });
}

// Initial pickups
[[-15, -15], [15, -15], [-15, 15], [15, 15], [0, -25], [0, 25]].forEach(([x, z]) => {
  spawnPickup('health', x, z);
});
[[-25, -5], [25, 5], [-5, -25], [5, 25]].forEach(([x, z]) => {
  spawnPickup('ammo', x, z);
});

// ─── Particle System ─────────────────────────────────────────────────────
function spawnParticles(pos, color, count = 8, speed = 5) {
  for (let i = 0; i < count; i++) {
    const geo = new THREE.SphereGeometry(0.03 + Math.random() * 0.03, 4, 4);
    const mat = new THREE.MeshBasicMaterial({ color });
    const mesh = new THREE.Mesh(geo, mat);
    mesh.position.copy(pos);
    scene.add(mesh);

    const vel = new THREE.Vector3(
      (Math.random() - 0.5) * speed,
      Math.random() * speed * 0.5,
      (Math.random() - 0.5) * speed
    );
    state.particles.push({ mesh, vel, life: 0.5 + Math.random() * 0.5 });
  }
}

function spawnBulletTrail(from, to) {
  const dir = new THREE.Vector3().subVectors(to, from);
  const len = dir.length();
  const geo = new THREE.CylinderGeometry(0.005, 0.005, len, 4);
  const mat = new THREE.MeshBasicMaterial({ color: 0xffff88, transparent: true, opacity: 0.6 });
  const mesh = new THREE.Mesh(geo, mat);

  mesh.position.copy(from).add(to).multiplyScalar(0.5);
  mesh.lookAt(to);
  mesh.rotateX(Math.PI / 2);
  scene.add(mesh);

  state.particles.push({ mesh, vel: new THREE.Vector3(), life: 0.08 });
}

function spawnImpactDecal(pos, normal) {
  const geo = new THREE.CircleGeometry(0.08 + Math.random() * 0.05, 8);
  const mat = new THREE.MeshBasicMaterial({ color: 0x222222, transparent: true, opacity: 0.7, side: THREE.DoubleSide });
  const mesh = new THREE.Mesh(geo, mat);
  mesh.position.copy(pos).add(normal.clone().multiplyScalar(0.01));
  mesh.lookAt(pos.clone().add(normal));
  scene.add(mesh);
  state.decals.push({ mesh, life: 15 });
  if (state.decals.length > 100) {
    const old = state.decals.shift();
    scene.remove(old.mesh);
    old.mesh.geometry.dispose();
  }
}

// ─── Shooting ────────────────────────────────────────────────────────────
function shoot() {
  const wIdx = state.currentWeapon;
  const wDef = WEAPONS[wIdx];
  const wState = state.weapons[wIdx];

  if (wState.reloading || wState.fireTimer > 0) return;
  if (wState.mag <= 0) { reload(); return; }

  wState.mag--;
  wState.fireTimer = wDef.fireRate;
  state.shotsFired++;

  // Muzzle flash
  muzzleFlash.intensity = 3;
  setTimeout(() => { muzzleFlash.intensity = 0; }, 50);

  // Recoil
  pitchObject.rotation.x += wDef.recoil * (0.8 + Math.random() * 0.4);
  if (currentWeaponMesh) {
    currentWeaponMesh.position.z += 0.05;
    setTimeout(() => { if (currentWeaponMesh) currentWeaponMesh.position.z -= 0.05; }, 60);
  }

  const pellets = wDef.pellets || 1;
  for (let p = 0; p < pellets; p++) {
    const spread = new THREE.Vector3(
      (Math.random() - 0.5) * wDef.spread,
      (Math.random() - 0.5) * wDef.spread,
      -1
    ).normalize();

    const worldDir = spread.clone();
    camera.localToWorld(worldDir);
    worldDir.sub(camera.getWorldPosition(new THREE.Vector3()));
    worldDir.normalize();

    const origin = camera.getWorldPosition(new THREE.Vector3());
    raycaster.set(origin, worldDir);
    raycaster.far = 100;

    // Check enemy hits
    let hitEnemy = false;
    for (const enemy of state.enemies) {
      if (enemy.health <= 0) continue;
      const ePos = enemy.mesh.position.clone();
      ePos.y += 1.0 * enemy.size;
      const toEnemy = ePos.clone().sub(origin);
      const proj = toEnemy.dot(worldDir);
      if (proj < 0 || proj > 100) continue;

      const closest = origin.clone().add(worldDir.clone().multiplyScalar(proj));
      const dist = closest.distanceTo(ePos);
      const hitRadius = 0.5 * enemy.size;

      if (dist < hitRadius) {
        // Hit!
        const dmg = wDef.damage * (1 + Math.random() * 0.2);
        enemy.health -= dmg;
        enemy.hitFlash = 0.15;
        state.shotsHit++;

        spawnParticles(closest, 0xff0000, 5, 3);
        spawnBulletTrail(origin, closest);

        if (enemy.health <= 0) {
          killEnemy(enemy);
        }
        hitEnemy = true;
        break;
      }
    }

    if (!hitEnemy) {
      // Hit world
      const meshes = [];
      scene.traverse(obj => {
        if (obj.isMesh && obj !== currentWeaponMesh && !enemyGroup.getObjectById(obj.id)) {
          meshes.push(obj);
        }
      });
      const hits = raycaster.intersectObjects(meshes, false);
      if (hits.length > 0) {
        const hit = hits[0];
        spawnBulletTrail(origin, hit.point);
        spawnParticles(hit.point, 0xaaaaaa, 3, 2);
        if (hit.face) spawnImpactDecal(hit.point, hit.face.normal);
      }
    }
  }

  updateAmmoDisplay();
}

function reload() {
  const wIdx = state.currentWeapon;
  const wDef = WEAPONS[wIdx];
  const wState = state.weapons[wIdx];
  if (wState.reloading || wState.reserve <= 0 || wState.mag === wDef.magSize) return;

  wState.reloading = true;
  wState.reloadTimer = wDef.reloadTime;
}

function finishReload() {
  const wIdx = state.currentWeapon;
  const wDef = WEAPONS[wIdx];
  const wState = state.weapons[wIdx];
  const needed = wDef.magSize - wState.mag;
  const take = Math.min(needed, wState.reserve);
  wState.mag += take;
  wState.reserve -= take;
  wState.reloading = false;
  updateAmmoDisplay();
}

function switchWeapon(idx) {
  if (idx === state.currentWeapon) return;
  state.weapons[state.currentWeapon].reloading = false;
  state.currentWeapon = idx;
  buildWeaponModel(idx);
  updateAmmoDisplay();
  document.getElementById('weapon-name').textContent = WEAPONS[idx].name;
}

// ─── Enemy Death ─────────────────────────────────────────────────────────
function killEnemy(enemy) {
  state.kills++;
  state.score += enemy.score;

  // Death particles
  spawnParticles(enemy.mesh.position.clone().add(new THREE.Vector3(0, 1, 0)), enemy.color, 15, 6);

  // Kill feed
  addKillFeed(enemy.type);

  // Chance to drop pickup
  if (Math.random() < 0.4) {
    const type = Math.random() < 0.5 ? 'health' : 'ammo';
    spawnPickup(type, enemy.mesh.position.x, enemy.mesh.position.z);
  }

  // Remove enemy
  enemyGroup.remove(enemy.mesh);
  enemy.mesh.traverse(c => { if (c.geometry) c.geometry.dispose(); });

  // Check wave completion
  if (state.kills >= state.waveKillTarget) {
    state.wave++;
    document.getElementById('wave-text').textContent = `Wave ${state.wave}`;

    // Spawn next wave after delay
    setTimeout(() => {
      if (state.alive) spawnWave();
    }, 2000);
  }

  updateScoreDisplay();
}

// ─── UI Updates ──────────────────────────────────────────────────────────
function updateHealthDisplay() {
  const pct = Math.max(0, state.health);
  document.getElementById('health-text').textContent = Math.ceil(pct);
  document.getElementById('health-bar').style.width = pct + '%';

  if (pct > 60) {
    document.getElementById('health-bar').style.background = 'linear-gradient(90deg, #44ff44, #88ff44)';
  } else if (pct > 30) {
    document.getElementById('health-bar').style.background = 'linear-gradient(90deg, #ffaa00, #ffcc44)';
  } else {
    document.getElementById('health-bar').style.background = 'linear-gradient(90deg, #ff2222, #ff4444)';
  }
}

function updateAmmoDisplay() {
  const wState = state.weapons[state.currentWeapon];
  document.querySelector('#ammo-text .mag').textContent = wState.mag;
  document.querySelector('#ammo-text .reserve').textContent = wState.reserve;
}

function updateScoreDisplay() {
  document.getElementById('score-text').textContent = state.score;
}

function addKillFeed(type) {
  const feed = document.getElementById('killfeed');
  const msg = document.createElement('div');
  msg.className = 'kill-msg';
  msg.textContent = `Eliminated ${type}`;
  feed.appendChild(msg);
  setTimeout(() => { msg.remove(); }, 3500);
}

function showDamage() {
  const overlay = document.getElementById('damage-overlay');
  overlay.style.opacity = '0.6';
  setTimeout(() => { overlay.style.opacity = '0'; }, 200);
}

// ─── Minimap ─────────────────────────────────────────────────────────────
const minimapCanvas = document.getElementById('minimap-canvas');
const minimapCtx = minimapCanvas.getContext('2d');

function drawMinimap() {
  const ctx = minimapCtx;
  const w = 140, h = 140;
  ctx.clearRect(0, 0, w, h);

  ctx.fillStyle = 'rgba(0,0,0,0.8)';
  ctx.fillRect(0, 0, w, h);

  const scale = w / MAP_SIZE;
  const px = yawObject.position.x * scale + w / 2;
  const pz = yawObject.position.z * scale + h / 2;

  // Draw colliders as walls
  ctx.fillStyle = 'rgba(80,80,100,0.5)';
  for (const box of colliders) {
    const bx = box.min.x * scale + w / 2;
    const bz = box.min.z * scale + h / 2;
    const bw = (box.max.x - box.min.x) * scale;
    const bh = (box.max.z - box.min.z) * scale;
    ctx.fillRect(bx, bz, bw, bh);
  }

  // Enemies
  for (const enemy of state.enemies) {
    if (enemy.health <= 0) continue;
    const ex = enemy.mesh.position.x * scale + w / 2;
    const ez = enemy.mesh.position.z * scale + h / 2;
    ctx.fillStyle = `#${enemy.color.toString(16).padStart(6, '0')}`;
    ctx.beginPath();
    ctx.arc(ex, ez, 2.5, 0, Math.PI * 2);
    ctx.fill();
  }

  // Pickups
  for (const pickup of state.pickups) {
    const ux = pickup.mesh.position.x * scale + w / 2;
    const uz = pickup.mesh.position.z * scale + h / 2;
    ctx.fillStyle = pickup.type === 'health' ? '#44ff44' : '#ffaa22';
    ctx.fillRect(ux - 1.5, uz - 1.5, 3, 3);
  }

  // Player
  ctx.fillStyle = '#ffffff';
  ctx.beginPath();
  ctx.arc(px, pz, 3, 0, Math.PI * 2);
  ctx.fill();

  // Player direction
  const dir = new THREE.Vector3(0, 0, -1);
  dir.applyQuaternion(yawObject.quaternion);
  ctx.strokeStyle = '#ffffff';
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(px, pz);
  ctx.lineTo(px + dir.x * 10, pz + dir.z * 10);
  ctx.stroke();
}

// ─── Input ───────────────────────────────────────────────────────────────
document.addEventListener('keydown', e => {
  state.keys[e.code] = true;
  if (e.code === 'KeyR') reload();
  if (e.code === 'Digit1') switchWeapon(0);
  if (e.code === 'Digit2') switchWeapon(1);
  if (e.code === 'Digit3') switchWeapon(2);
});
document.addEventListener('keyup', e => { state.keys[e.code] = false; });
document.addEventListener('mousedown', e => {
  if (e.button === 0) state.mouseDown = true;
});
document.addEventListener('mouseup', e => {
  if (e.button === 0) state.mouseDown = false;
});

document.addEventListener('mousemove', e => {
  if (!document.pointerLockElement) return;
  yawObject.rotation.y -= e.movementX * MOUSE_SENS;
  pitchObject.rotation.x -= e.movementY * MOUSE_SENS;
  pitchObject.rotation.x = Math.max(-Math.PI / 2 + 0.01, Math.min(Math.PI / 2 - 0.01, pitchObject.rotation.x));
});

// Pointer lock
const blocker = document.getElementById('blocker');
const gameover = document.getElementById('gameover');

blocker.addEventListener('click', () => {
  renderer.domElement.requestPointerLock();
});

gameover.addEventListener('click', () => {
  restartGame();
  renderer.domElement.requestPointerLock();
});

document.addEventListener('pointerlockchange', () => {
  if (document.pointerLockElement) {
    blocker.style.display = 'none';
    document.getElementById('hud').style.display = 'block';
    if (!state.started) {
      state.started = true;
      spawnWave();
    }
  } else {
    if (state.alive) {
      blocker.style.display = 'flex';
      document.getElementById('hud').style.display = 'none';
    }
  }
});

// ─── Game Over / Restart ─────────────────────────────────────────────────
function gameOver() {
  state.alive = false;
  document.exitPointerLock();
  document.getElementById('hud').style.display = 'none';
  blocker.style.display = 'none';

  const accuracy = state.shotsFired > 0 ? Math.round(state.shotsHit / state.shotsFired * 100) : 0;
  document.getElementById('final-score').textContent = state.score;
  document.getElementById('final-kills').textContent = state.kills;
  document.getElementById('final-wave').textContent = state.wave;
  document.getElementById('final-accuracy').textContent = accuracy + '%';

  gameover.style.display = 'flex';
}

function restartGame() {
  // Clean up enemies
  state.enemies.forEach(e => {
    enemyGroup.remove(e.mesh);
    e.mesh.traverse(c => { if (c.geometry) c.geometry.dispose(); });
  });
  state.enemies = [];

  // Clean up pickups
  state.pickups.forEach(p => { scene.remove(p.mesh); });
  state.pickups = [];

  // Clean up particles
  state.particles.forEach(p => { scene.remove(p.mesh); p.mesh.geometry.dispose(); });
  state.particles = [];

  // Clean up decals
  state.decals.forEach(d => { scene.remove(d.mesh); d.mesh.geometry.dispose(); });
  state.decals = [];

  // Reset state
  state.health = 100;
  state.score = 0;
  state.kills = 0;
  state.wave = 1;
  state.shotsFired = 0;
  state.shotsHit = 0;
  state.alive = true;
  state.weapons = WEAPONS.map(w => ({ mag: w.magSize, reserve: w.reserve, reloading: false, reloadTimer: 0, fireTimer: 0 }));
  state.velocity.set(0, 0, 0);

  yawObject.position.set(0, 0, 0);
  pitchObject.rotation.x = 0;

  switchWeapon(1);
  updateHealthDisplay();
  updateScoreDisplay();
  document.getElementById('wave-text').textContent = 'Wave 1';

  gameover.style.display = 'none';

  // Re-spawn pickups
  [[-15, -15], [15, -15], [-15, 15], [15, 15], [0, -25], [0, 25]].forEach(([x, z]) => {
    spawnPickup('health', x, z);
  });
  [[-25, -5], [25, 5], [-5, -25], [5, 25]].forEach(([x, z]) => {
    spawnPickup('ammo', x, z);
  });

  spawnWave();
}

// ─── Game Loop ───────────────────────────────────────────────────────────
const clock = new THREE.Clock();
let bobTime = 0;

function update() {
  requestAnimationFrame(update);

  const dt = Math.min(clock.getDelta(), 0.05);

  if (!state.alive || !document.pointerLockElement) {
    renderer.render(scene, camera);
    return;
  }

  // ── Movement ──
  state.sprinting = state.keys['ShiftLeft'] || state.keys['ShiftRight'];
  const speed = MOVE_SPEED * (state.sprinting ? SPRINT_MULT : 1);

  const moveDir = new THREE.Vector3();
  if (state.keys['KeyW']) moveDir.z -= 1;
  if (state.keys['KeyS']) moveDir.z += 1;
  if (state.keys['KeyA']) moveDir.x -= 1;
  if (state.keys['KeyD']) moveDir.x += 1;
  moveDir.normalize();

  // Apply rotation
  const forward = new THREE.Vector3(0, 0, -1).applyQuaternion(yawObject.quaternion);
  forward.y = 0; forward.normalize();
  const right = new THREE.Vector3(1, 0, 0).applyQuaternion(yawObject.quaternion);
  right.y = 0; right.normalize();

  const wishDir = new THREE.Vector3();
  wishDir.addScaledVector(forward, -moveDir.z);
  wishDir.addScaledVector(right, moveDir.x);

  if (state.onGround) {
    state.velocity.x = wishDir.x * speed;
    state.velocity.z = wishDir.z * speed;
  } else {
    // Air control (reduced)
    state.velocity.x += wishDir.x * speed * 0.05;
    state.velocity.z += wishDir.z * speed * 0.05;
  }

  // Gravity
  state.velocity.y -= GRAVITY * dt;

  // Jump
  if (state.keys['Space'] && state.onGround) {
    state.velocity.y = JUMP_FORCE;
    state.onGround = false;
  }

  // Apply velocity
  const newPos = yawObject.position.clone();
  newPos.add(state.velocity.clone().multiplyScalar(dt));

  // Ground check
  if (newPos.y < 0) {
    newPos.y = 0;
    state.velocity.y = 0;
    state.onGround = true;
  } else {
    state.onGround = false;
  }

  // Collision - horizontal only for wall sliding
  const horizPos = new THREE.Vector3(newPos.x, yawObject.position.y + PLAYER_HEIGHT * 0.5, newPos.z);
  resolveCollision(horizPos, PLAYER_RADIUS);
  newPos.x = horizPos.x;
  newPos.z = horizPos.z;

  // Map bounds
  const bound = MAP_SIZE / 2 - 1;
  newPos.x = Math.max(-bound, Math.min(bound, newPos.x));
  newPos.z = Math.max(-bound, Math.min(bound, newPos.z));

  yawObject.position.copy(newPos);

  // View bob
  const isMoving = moveDir.length() > 0 && state.onGround;
  if (isMoving) {
    bobTime += dt * (state.sprinting ? 12 : 8);
    const bobX = Math.sin(bobTime) * 0.015;
    const bobY = Math.abs(Math.cos(bobTime)) * 0.02;
    camera.position.x = bobX;
    camera.position.y = PLAYER_HEIGHT + bobY;
  } else {
    camera.position.x = THREE.MathUtils.lerp(camera.position.x, 0, dt * 10);
    camera.position.y = THREE.MathUtils.lerp(camera.position.y, PLAYER_HEIGHT, dt * 10);
  }

  // Weapon sway
  if (currentWeaponMesh) {
    const targetX = 0.25 + (isMoving ? Math.sin(bobTime * 0.5) * 0.01 : 0);
    const targetY = -0.2 + (isMoving ? Math.abs(Math.cos(bobTime)) * 0.005 : 0);
    weaponGroup.position.x = THREE.MathUtils.lerp(weaponGroup.position.x, targetX, dt * 8);
    weaponGroup.position.y = THREE.MathUtils.lerp(weaponGroup.position.y, targetY, dt * 8);
  }

  // ── Shooting ──
  const wDef = WEAPONS[state.currentWeapon];
  const wState = state.weapons[state.currentWeapon];

  wState.fireTimer = Math.max(0, wState.fireTimer - dt);

  if (wState.reloading) {
    wState.reloadTimer -= dt;
    if (wState.reloadTimer <= 0) finishReload();
  }

  if (state.mouseDown) {
    if (wDef.auto || wState.fireTimer <= 0) {
      shoot();
    }
  }

  // ── Enemies ──
  const playerPos = yawObject.position.clone();
  playerPos.y += 1;

  for (let i = state.enemies.length - 1; i >= 0; i--) {
    const enemy = state.enemies[i];
    if (enemy.health <= 0) {
      state.enemies.splice(i, 1);
      continue;
    }

    // Hit flash fade
    if (enemy.hitFlash > 0) {
      enemy.hitFlash -= dt;
      enemy.body.material.emissiveIntensity = enemy.hitFlash > 0 ? 2 : 0.15;
    }

    // AI movement toward player
    const toPlayer = new THREE.Vector3().subVectors(playerPos, enemy.mesh.position);
    toPlayer.y = 0;
    const dist = toPlayer.length();

    if (dist > 2) {
      toPlayer.normalize();
      const moveAmt = enemy.speed * dt;

      // Check line of sight / basic pathfinding
      const testPos = enemy.mesh.position.clone();
      testPos.x += toPlayer.x * moveAmt;
      testPos.z += toPlayer.z * moveAmt;
      testPos.y = 0.5;

      const col = checkCollision(testPos, 0.4);
      if (!col.hit) {
        enemy.mesh.position.x += toPlayer.x * moveAmt;
        enemy.mesh.position.z += toPlayer.z * moveAmt;
      } else {
        // Try side-stepping
        enemy.wanderAngle += (Math.random() - 0.5) * 2;
        const sideX = Math.cos(enemy.wanderAngle) * moveAmt;
        const sideZ = Math.sin(enemy.wanderAngle) * moveAmt;
        const testPos2 = enemy.mesh.position.clone();
        testPos2.x += sideX; testPos2.z += sideZ; testPos2.y = 0.5;
        const col2 = checkCollision(testPos2, 0.4);
        if (!col2.hit) {
          enemy.mesh.position.x += sideX;
          enemy.mesh.position.z += sideZ;
        }
      }

      // Face player
      enemy.mesh.lookAt(new THREE.Vector3(playerPos.x, 0, playerPos.z));
    }

    // Attack
    if (dist < 2.5) {
      enemy.attackTimer -= dt;
      if (enemy.attackTimer <= 0) {
        state.health -= enemy.damage;
        enemy.attackTimer = enemy.attackCooldown;
        showDamage();
        updateHealthDisplay();

        if (state.health <= 0) {
          gameOver();
          return;
        }
      }
    } else {
      enemy.attackTimer = 0;
    }

    // Animate bob
    enemy.mesh.children.forEach((child, ci) => {
      if (ci === 0) { // body
        child.position.y = 0.9 * enemy.size + Math.sin(Date.now() * 0.003 + i) * 0.05;
      }
    });

    // Keep in bounds
    const eb = MAP_SIZE / 2 - 1;
    enemy.mesh.position.x = Math.max(-eb, Math.min(eb, enemy.mesh.position.x));
    enemy.mesh.position.z = Math.max(-eb, Math.min(eb, enemy.mesh.position.z));
  }

  // ── Spawn timer (continuous spawning if enemies are low) ──
  if (state.enemies.length < 3 + state.wave) {
    state.spawnTimer -= dt;
    if (state.spawnTimer <= 0) {
      const types = ['grunt'];
      if (state.wave >= 2) types.push('fast');
      if (state.wave >= 3) types.push('heavy');
      createEnemy(types[Math.floor(Math.random() * types.length)]);
      state.spawnTimer = Math.max(1, 4 - state.wave * 0.3);
    }
  }

  // ── Pickups ──
  for (let i = state.pickups.length - 1; i >= 0; i--) {
    const pickup = state.pickups[i];
    pickup.time += dt;
    pickup.mesh.rotation.y = pickup.time * 2;
    pickup.mesh.position.y = 0.7 + Math.sin(pickup.time * 3) * 0.15;

    const pDist = yawObject.position.distanceTo(pickup.mesh.position);
    if (pDist < 1.5) {
      if (pickup.type === 'health' && state.health < 100) {
        state.health = Math.min(100, state.health + 25);
        updateHealthDisplay();
        scene.remove(pickup.mesh);
        state.pickups.splice(i, 1);
      } else if (pickup.type === 'ammo') {
        const wSt = state.weapons[state.currentWeapon];
        wSt.reserve += WEAPONS[state.currentWeapon].magSize;
        updateAmmoDisplay();
        scene.remove(pickup.mesh);
        state.pickups.splice(i, 1);
      }
    }
  }

  // ── Particles ──
  for (let i = state.particles.length - 1; i >= 0; i--) {
    const p = state.particles[i];
    p.life -= dt;
    if (p.life <= 0) {
      scene.remove(p.mesh);
      p.mesh.geometry.dispose();
      state.particles.splice(i, 1);
      continue;
    }
    p.mesh.position.add(p.vel.clone().multiplyScalar(dt));
    p.vel.y -= 10 * dt;
    if (p.mesh.material.opacity !== undefined) {
      p.mesh.material.opacity = p.life;
    }
  }

  // ── Decals ──
  for (let i = state.decals.length - 1; i >= 0; i--) {
    state.decals[i].life -= dt;
    if (state.decals[i].life <= 0) {
      scene.remove(state.decals[i].mesh);
      state.decals[i].mesh.geometry.dispose();
      state.decals.splice(i, 1);
    }
  }

  // ── Accent light animation ──
  accentLights.forEach((light, i) => {
    light.intensity = 1.5 + Math.sin(Date.now() * 0.001 + i * 0.7) * 0.8;
  });

  // ── Minimap ──
  drawMinimap();

  // ── Render ──
  renderer.render(scene, camera);
}

// ─── Window Resize ───────────────────────────────────────────────────────
window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(window.innerWidth, window.innerHeight);
});

// ─── Start ───────────────────────────────────────────────────────────────
updateHealthDisplay();
updateAmmoDisplay();
updateScoreDisplay();
update();

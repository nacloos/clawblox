import * as THREE from 'three'

const MAP_SIZE = 60
const WALL_HEIGHT = 4

const canvas = document.getElementById('game') as HTMLCanvasElement
const hud = document.getElementById('hud') as HTMLDivElement
type RefEntity = {
  id: number
  name: string
  shape: 'Block' | 'Cylinder' | 'Ball'
  material: string
  position: [number, number, number]
  size: [number, number, number]
}
const refEntities: RefEntity[] = []
let refEntityId = 1
let lastDebugLogAt = 0

const scene = new THREE.Scene()
scene.fog = new THREE.FogExp2(0x1a1a2a, 0.008)
scene.background = new THREE.Color(0x1a1a2a)

const camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 500)
camera.position.set(0, 2.2, 18)

const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance' })
renderer.setSize(window.innerWidth, window.innerHeight)
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
renderer.shadowMap.enabled = true
renderer.shadowMap.type = THREE.PCFSoftShadowMap
renderer.toneMapping = THREE.ACESFilmicToneMapping
renderer.toneMappingExposure = 1.8
renderer.outputColorSpace = THREE.SRGBColorSpace

const ambientLight = new THREE.AmbientLight(0x8888aa, 1.2)
scene.add(ambientLight)

const dirLight = new THREE.DirectionalLight(0x8899cc, 1.5)
dirLight.position.set(20, 30, 10)
dirLight.castShadow = true
dirLight.shadow.mapSize.set(2048, 2048)
dirLight.shadow.camera.left = -40
dirLight.shadow.camera.right = 40
dirLight.shadow.camera.top = 40
dirLight.shadow.camera.bottom = -40
dirLight.shadow.camera.near = 0.5
dirLight.shadow.camera.far = 80
dirLight.shadow.bias = -0.001
scene.add(dirLight)

const accentColors = [0xff4444, 0x4488ff, 0xff8800, 0x44ff88, 0xff44ff, 0xffff44]
const lightPositions = [
  [-15, 3, -15], [15, 3, -15], [-15, 3, 15], [15, 3, 15],
  [0, 3, -20], [0, 3, 20], [-20, 3, 0], [20, 3, 0],
  [-8, 3, -8], [8, 3, 8], [-8, 3, 8], [8, 3, -8],
]
const accentLights: THREE.PointLight[] = []
for (let i = 0; i < lightPositions.length; i++) {
  const p = lightPositions[i]
  const light = new THREE.PointLight(accentColors[i % accentColors.length], 4, 20)
  light.position.set(p[0], p[1], p[2])
  scene.add(light)
  accentLights.push(light)
}

function createFloorTexture(): THREE.CanvasTexture {
  const c = document.createElement('canvas')
  c.width = 512
  c.height = 512
  const ctx = c.getContext('2d')
  if (!ctx) throw new Error('2d context unavailable')
  ctx.fillStyle = '#3a3a3a'
  ctx.fillRect(0, 0, 512, 512)
  ctx.strokeStyle = '#4a4a4a'
  ctx.lineWidth = 2
  for (let i = 0; i <= 512; i += 64) {
    ctx.beginPath(); ctx.moveTo(i, 0); ctx.lineTo(i, 512); ctx.stroke()
    ctx.beginPath(); ctx.moveTo(0, i); ctx.lineTo(512, i); ctx.stroke()
  }
  for (let i = 0; i < 2000; i++) {
    const x = Math.random() * 512
    const y = Math.random() * 512
    const v = 50 + Math.random() * 20
    ctx.fillStyle = `rgb(${v},${v},${v})`
    ctx.fillRect(x, y, 2, 2)
  }
  const tex = new THREE.CanvasTexture(c)
  tex.wrapS = tex.wrapT = THREE.RepeatWrapping
  tex.repeat.set(15, 15)
  return tex
}

function createWallTexture(): THREE.CanvasTexture {
  const c = document.createElement('canvas')
  c.width = 256
  c.height = 256
  const ctx = c.getContext('2d')
  if (!ctx) throw new Error('2d context unavailable')
  ctx.fillStyle = '#3a3a42'
  ctx.fillRect(0, 0, 256, 256)
  for (let i = 0; i < 3000; i++) {
    const x = Math.random() * 256
    const y = Math.random() * 256
    const v = 50 + Math.random() * 25
    ctx.fillStyle = `rgb(${v},${v},${v + 5})`
    ctx.fillRect(x, y, Math.random() * 3 + 1, Math.random() * 3 + 1)
  }
  ctx.strokeStyle = '#2e2e36'
  ctx.lineWidth = 2
  ;[64, 128, 192].forEach((y) => { ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(256, y); ctx.stroke() })
  const tex = new THREE.CanvasTexture(c)
  tex.wrapS = tex.wrapT = THREE.RepeatWrapping
  return tex
}

const floorMat = new THREE.MeshStandardMaterial({ map: createFloorTexture(), roughness: 0.85, metalness: 0.1 })
const wallTex = createWallTexture()
const wallMat = new THREE.MeshStandardMaterial({ map: wallTex, roughness: 0.7, metalness: 0.2 })
const crateMat = new THREE.MeshStandardMaterial({ color: 0x6B4226, roughness: 0.9, metalness: 0.05 })
const metalMat = new THREE.MeshStandardMaterial({ color: 0x555566, roughness: 0.3, metalness: 0.8 })
const glowRedMat = new THREE.MeshStandardMaterial({ color: 0xff2222, emissive: 0xff2222, emissiveIntensity: 0.5 })
const glowBlueMat = new THREE.MeshStandardMaterial({ color: 0x2244ff, emissive: 0x2244ff, emissiveIntensity: 0.5 })

function addWall(x: number, z: number, sx: number, sz: number, ht = WALL_HEIGHT): void {
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(sx, ht, sz), wallMat)
  mesh.position.set(x, ht / 2, z)
  mesh.castShadow = true
  mesh.receiveShadow = true
  scene.add(mesh)
  refEntities.push({
    id: refEntityId++,
    name: 'Wall',
    shape: 'Block',
    material: 'Concrete',
    position: [x, ht / 2, z],
    size: [sx, ht, sz],
  })
}

function addCrate(x: number, z: number, size = 1.2, ht = 1.2): void {
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(size, ht, size), crateMat)
  mesh.position.set(x, ht / 2, z)
  mesh.castShadow = true
  mesh.receiveShadow = true
  scene.add(mesh)
  refEntities.push({
    id: refEntityId++,
    name: 'Crate',
    shape: 'Block',
    material: 'Wood',
    position: [x, ht / 2, z],
    size: [size, ht, size],
  })

  const edgeMat = new THREE.MeshStandardMaterial({ color: 0x444444, roughness: 0.4, metalness: 0.6 })
  const edgeGeo = new THREE.BoxGeometry(size + 0.02, 0.04, 0.04)
  ;[-1, 1].forEach((sy) => {
    const edge = new THREE.Mesh(edgeGeo, edgeMat)
    edge.position.set(x, sy * ht / 2 + ht / 2, z)
    scene.add(edge)
  })
}

function addPillar(x: number, z: number, radius = 0.5, ht = WALL_HEIGHT): void {
  const mesh = new THREE.Mesh(new THREE.CylinderGeometry(radius, radius, ht, 12), metalMat)
  mesh.position.set(x, ht / 2, z)
  mesh.castShadow = true
  mesh.receiveShadow = true
  scene.add(mesh)
  refEntities.push({
    id: refEntityId++,
    name: 'Pillar',
    shape: 'Cylinder',
    material: 'Metal',
    position: [x, ht / 2, z],
    size: [radius * 2, ht, radius * 2],
  })
}

function buildReferenceMap(): void {
  const floor = new THREE.Mesh(new THREE.PlaneGeometry(MAP_SIZE, MAP_SIZE), floorMat)
  floor.rotation.x = -Math.PI / 2
  floor.receiveShadow = true
  scene.add(floor)
  refEntities.push({
    id: refEntityId++,
    name: 'Floor',
    shape: 'Block',
    material: 'Slate',
    position: [0, 0, 0],
    size: [MAP_SIZE, 0.1, MAP_SIZE],
  })

  const ceiling = new THREE.Mesh(
    new THREE.PlaneGeometry(MAP_SIZE, MAP_SIZE),
    new THREE.MeshStandardMaterial({ color: 0x2a2a30, roughness: 0.9 }),
  )
  ceiling.rotation.x = Math.PI / 2
  ceiling.position.y = WALL_HEIGHT
  scene.add(ceiling)
  refEntities.push({
    id: refEntityId++,
    name: 'Ceiling',
    shape: 'Block',
    material: 'Concrete',
    position: [0, WALL_HEIGHT, 0],
    size: [MAP_SIZE, 0.1, MAP_SIZE],
  })

  const hs = MAP_SIZE / 2
  const wt = 0.5

  addWall(0, -hs, MAP_SIZE, wt)
  addWall(0, hs, MAP_SIZE, wt)
  addWall(-hs, 0, wt, MAP_SIZE)
  addWall(hs, 0, wt, MAP_SIZE)

  addWall(-4, -4, 8, 0.5)
  addWall(-4, 4, 8, 0.5)
  addWall(-8, 0, 0.5, 8)
  addWall(8, 0, 0.5, 8)

  addWall(-18, -8, 0.5, 10)
  addWall(-18, 8, 0.5, 10)
  addWall(18, -8, 0.5, 10)
  addWall(18, 8, 0.5, 10)

  addWall(-12, -15, 8, 0.5)
  addWall(12, -15, 8, 0.5)
  addWall(-12, 15, 8, 0.5)
  addWall(12, 15, 8, 0.5)

  addWall(-22, -20, 6, 0.5)
  addWall(22, -20, 6, 0.5)
  addWall(-22, 20, 6, 0.5)
  addWall(22, 20, 6, 0.5)

  const shortH = 1.5
  const shortGeo = new THREE.BoxGeometry(3, shortH, 0.4)
  ;[[-10, -10], [10, -10], [-10, 10], [10, 10], [0, -12], [0, 12]].forEach(([x, z]) => {
    const mesh = new THREE.Mesh(shortGeo, wallMat)
    mesh.position.set(x, shortH / 2, z)
    mesh.castShadow = true
    mesh.receiveShadow = true
    scene.add(mesh)
    refEntities.push({
      id: refEntityId++,
      name: 'ShortWall',
      shape: 'Block',
      material: 'Concrete',
      position: [x, shortH / 2, z],
      size: [3, shortH, 0.4],
    })
  })

  ;[
    [-6, -6], [6, -6], [-6, 6], [6, 6],
    [-14, 0], [14, 0], [0, -20], [0, 20],
    [-22, -10], [22, -10], [-22, 10], [22, 10],
  ].forEach(([x, z]) => addPillar(x, z))

  ;[
    [-3, -18], [-1, -18], [-2, -16],
    [3, 18], [1, 18], [2, 16],
    [-20, -2], [-20, 2],
    [20, -2], [20, 2],
    [-15, -22], [15, 22],
    [-25, 0], [25, 0],
  ].forEach(([x, z]) => addCrate(x, z, 1.2, 1.2))

  ;[
    [0, 0, 8, 0.3, 8],
    [-20, -20, 6, 0.3, 6],
    [20, 20, 6, 0.3, 6],
  ].forEach(([x, z, sx, sy, sz]) => {
    const plat = new THREE.Mesh(
      new THREE.BoxGeometry(sx, sy, sz),
      new THREE.MeshStandardMaterial({ color: 0x2a2a30, roughness: 0.6, metalness: 0.3 }),
    )
    plat.position.set(x, sy * 0.5, z)
    plat.receiveShadow = true
    scene.add(plat)
    refEntities.push({
      id: refEntityId++,
      name: 'Platform',
      shape: 'Block',
      material: 'Metal',
      position: [x, sy * 0.5, z],
      size: [sx, sy, sz],
    })
  })

  const stripGeo = new THREE.BoxGeometry(0.1, 0.1, MAP_SIZE)
  ;[-hs + 0.3, hs - 0.3].forEach((x) => {
    const strip = new THREE.Mesh(stripGeo, x < 0 ? glowRedMat : glowBlueMat)
    strip.position.set(x, 0.5, 0)
    scene.add(strip)
    refEntities.push({
      id: refEntityId++,
      name: 'TrimStrip',
      shape: 'Block',
      material: 'Neon',
      position: [x, 0.5, 0],
      size: [0.1, 0.1, MAP_SIZE],
    })
  })
  const stripGeoH = new THREE.BoxGeometry(MAP_SIZE, 0.1, 0.1)
  ;[-hs + 0.3, hs - 0.3].forEach((z) => {
    const strip = new THREE.Mesh(stripGeoH, z < 0 ? glowRedMat : glowBlueMat)
    strip.position.set(0, 0.5, z)
    scene.add(strip)
    refEntities.push({
      id: refEntityId++,
      name: 'TrimStrip',
      shape: 'Block',
      material: 'Neon',
      position: [0, 0.5, z],
      size: [MAP_SIZE, 0.1, 0.1],
    })
  })
}

buildReferenceMap()

const moveKeys: Record<string, boolean> = {}
let yaw = Math.PI
let pitch = -0.12

window.addEventListener('click', () => { void canvas.requestPointerLock() })
window.addEventListener('keydown', (event) => { moveKeys[event.code] = true })
window.addEventListener('keyup', (event) => { moveKeys[event.code] = false })
window.addEventListener('mousemove', (event) => {
  if (document.pointerLockElement !== canvas) return
  yaw -= event.movementX * 0.002
  pitch -= event.movementY * 0.002
  pitch = Math.max(-1.5, Math.min(1.5, pitch))
})

function updateCamera(dt: number): void {
  camera.rotation.order = 'YXZ'
  camera.rotation.y = yaw
  camera.rotation.x = pitch
  const speedBase = moveKeys.ShiftLeft || moveKeys.ShiftRight ? 30 : 14
  const speed = speedBase * dt
  const forward = new THREE.Vector3(0, 0, -1).applyQuaternion(camera.quaternion).setY(0).normalize()
  const right = new THREE.Vector3(1, 0, 0).applyQuaternion(camera.quaternion).setY(0).normalize()
  if (moveKeys.KeyW) camera.position.addScaledVector(forward, speed)
  if (moveKeys.KeyS) camera.position.addScaledVector(forward, -speed)
  if (moveKeys.KeyA) camera.position.addScaledVector(right, -speed)
  if (moveKeys.KeyD) camera.position.addScaledVector(right, speed)
  if (moveKeys.KeyE || moveKeys.Space) camera.position.y += speed
  if (moveKeys.KeyQ || moveKeys.ControlLeft) camera.position.y -= speed
}

function updateHud(): void {
  const byShape = new Map<string, number>()
  const byMaterial = new Map<string, number>()
  const byName = new Map<string, number>()
  let minX = Number.POSITIVE_INFINITY
  let minY = Number.POSITIVE_INFINITY
  let minZ = Number.POSITIVE_INFINITY
  let maxX = Number.NEGATIVE_INFINITY
  let maxY = Number.NEGATIVE_INFINITY
  let maxZ = Number.NEGATIVE_INFINITY
  for (const e of refEntities) {
    byShape.set(e.shape, (byShape.get(e.shape) ?? 0) + 1)
    byMaterial.set(e.material, (byMaterial.get(e.material) ?? 0) + 1)
    byName.set(e.name, (byName.get(e.name) ?? 0) + 1)
    minX = Math.min(minX, e.position[0]); maxX = Math.max(maxX, e.position[0])
    minY = Math.min(minY, e.position[1]); maxY = Math.max(maxY, e.position[1])
    minZ = Math.min(minZ, e.position[2]); maxZ = Math.max(maxZ, e.position[2])
  }
  const top = (m: Map<string, number>): string =>
    [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6).map(([k, v]) => `${k}:${v}`).join(', ') || 'none'
  const samples = refEntities
    .slice(0, 8)
    .map((e) => `#${e.id} ${e.name} @ (${e.position[0].toFixed(1)},${e.position[1].toFixed(1)},${e.position[2].toFixed(1)})`)
    .join('\n')
  const summary = [
    `count=${refEntities.length} bounds=(${minX.toFixed(1)},${minY.toFixed(1)},${minZ.toFixed(1)})..(${maxX.toFixed(1)},${maxY.toFixed(1)},${maxZ.toFixed(1)})`,
    `shape: ${top(byShape)}`,
    `material: ${top(byMaterial)}`,
    `name: ${top(byName)}`,
    `sample:\n${samples || 'none'}`,
  ].join('\n')

  hud.textContent =
    'Reference map viewer\n' +
    `camera=(${camera.position.x.toFixed(1)}, ${camera.position.y.toFixed(1)}, ${camera.position.z.toFixed(1)})\n` +
    'click: lock mouse | WASD: move | E/Q: up/down | Shift: boost\n' +
    `\n${summary}`

  const now = performance.now()
  if (now - lastDebugLogAt > 2000) {
    lastDebugLogAt = now
    console.log('[reference-map] entity-summary\n' + summary)
  }
}

window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight
  camera.updateProjectionMatrix()
  renderer.setSize(window.innerWidth, window.innerHeight)
})

const clock = new THREE.Clock()
function frame(): void {
  requestAnimationFrame(frame)
  const dt = Math.min(clock.getDelta(), 0.05)
  const t = Date.now() * 0.001
  accentLights.forEach((l, i) => {
    l.intensity = 1.5 + Math.sin(t + i * 0.7) * 0.8
  })
  updateCamera(dt)
  updateHud()
  renderer.render(scene, camera)
}
frame()

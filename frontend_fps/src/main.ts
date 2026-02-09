import * as pako from 'pako'
import * as THREE from 'three'

interface SpectatorPlayerInfo {
  id: string
  name: string
  position: [number, number, number]
  root_part_id?: number
  health: number
  attributes?: Record<string, unknown>
}

interface SpectatorEntity {
  id: number
  type: string
  position: [number, number, number]
  rotation?: [[number, number, number], [number, number, number], [number, number, number]]
  size?: [number, number, number]
  color?: [number, number, number]
  material?: string
  shape?: 'Block' | 'Ball' | 'Cylinder' | 'Wedge'
  health?: number
  model_url?: string
}

interface SpectatorObservation {
  instance_id: string
  tick: number
  server_time_ms: number
  game_status: string
  players: SpectatorPlayerInfo[]
  entities: SpectatorEntity[]
}

interface LeaderboardEntry {
  rank: number
  key: string
  score: number
  name?: string
}

const canvas = document.getElementById('game') as HTMLCanvasElement
const statusEl = document.getElementById('status') as HTMLDivElement
const targetEl = document.getElementById('target') as HTMLDivElement
const killfeedEl = document.getElementById('killfeed') as HTMLDivElement
const leaderboardEl = document.getElementById('leaderboard') as HTMLDivElement
const healthTextEl = document.getElementById('health-text') as HTMLDivElement
const healthBarEl = document.getElementById('health-bar') as HTMLDivElement
const weaponEl = document.getElementById('weapon') as HTMLDivElement
const ammoEl = document.getElementById('ammo') as HTMLDivElement
const scoreEl = document.getElementById('score') as HTMLDivElement
const phaseEl = document.getElementById('phase') as HTMLDivElement
const minimapCanvas = document.getElementById('minimap') as HTMLCanvasElement
const minimapCtx = minimapCanvas.getContext('2d')

if (!minimapCtx) throw new Error('Minimap 2D context unavailable')

const renderer = new THREE.WebGLRenderer({ canvas, antialias: true })
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
renderer.setSize(window.innerWidth, window.innerHeight)
renderer.shadowMap.enabled = true
renderer.shadowMap.type = THREE.PCFSoftShadowMap
renderer.outputColorSpace = THREE.SRGBColorSpace
renderer.toneMapping = THREE.ACESFilmicToneMapping
renderer.toneMappingExposure = 1.35

const scene = new THREE.Scene()
scene.background = new THREE.Color(0x151723)
scene.fog = new THREE.FogExp2(0x141926, 0.01)

const camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 300)
camera.position.set(0, 5, 10)

const hemi = new THREE.HemisphereLight(0x7ea2ff, 0x17191f, 0.65)
scene.add(hemi)

const moon = new THREE.DirectionalLight(0xb8d1ff, 1.1)
moon.position.set(18, 24, 10)
moon.castShadow = true
moon.shadow.mapSize.set(2048, 2048)
moon.shadow.camera.left = -80
moon.shadow.camera.right = 80
moon.shadow.camera.top = 80
moon.shadow.camera.bottom = -80
scene.add(moon)

const accentLights: THREE.PointLight[] = []
const accentPositions = [
  [-26, 4, -26], [26, 4, -26], [-26, 4, 26], [26, 4, 26],
  [0, 5, -32], [0, 5, 32], [-32, 5, 0], [32, 5, 0],
]
const accentColors = [0xff4646, 0x45a9ff, 0xff8f2f, 0x4dff87]
for (let i = 0; i < accentPositions.length; i++) {
  const light = new THREE.PointLight(accentColors[i % accentColors.length], 1.7, 44, 2)
  light.position.set(accentPositions[i][0], accentPositions[i][1], accentPositions[i][2])
  scene.add(light)
  accentLights.push(light)
}

const grid = new THREE.GridHelper(240, 96, 0x26466a, 0x18273e)
const mat = grid.material as THREE.Material
mat.transparent = true
mat.opacity = 0.24
scene.add(grid)

const floor = new THREE.Mesh(
  new THREE.PlaneGeometry(260, 260),
  new THREE.MeshStandardMaterial({ color: 0x1f2531, roughness: 0.95, metalness: 0.05 }),
)
floor.rotation.x = -Math.PI / 2
floor.receiveShadow = true
scene.add(floor)

const clock = new THREE.Clock()
const entityObjects = new Map<number, THREE.Object3D>()
let latestObservation: SpectatorObservation | null = null
let selectedPlayerId: string | null = null
let leaderboardData: LeaderboardEntry[] = []

const prevPlayerHealth = new Map<string, number>()
const prevPlayers = new Set<string>()
const prevEntityIds = new Set<number>()

function clamp01(v: number): number {
  if (v < 0) return 0
  if (v > 1) return 1
  return v
}

function getGameId(): string {
  const pathMatch = window.location.pathname.match(/\/spectate\/([0-9a-fA-F-]{36})/)
  if (pathMatch?.[1]) return pathMatch[1]

  const q = new URLSearchParams(window.location.search).get('game')
  if (q) return q

  throw new Error('Missing game id. Use /spectate/<game_id> or ?game=<game_id>')
}

const gameId = getGameId()

function toThreeColor(arr?: [number, number, number]): THREE.Color {
  if (!arr) return new THREE.Color(0x8497b0)
  return new THREE.Color(arr[0], arr[1], arr[2])
}

function numberAttr(attrs: Record<string, unknown> | undefined, keys: string[]): number | null {
  if (!attrs) return null
  for (const k of keys) {
    const v = attrs[k]
    if (typeof v === 'number' && Number.isFinite(v)) return v
  }
  return null
}

function stringAttr(attrs: Record<string, unknown> | undefined, keys: string[]): string | null {
  if (!attrs) return null
  for (const k of keys) {
    const v = attrs[k]
    if (typeof v === 'string' && v.trim().length > 0) return v
  }
  return null
}

function rotationToQuaternion(rot: [[number, number, number], [number, number, number], [number, number, number]]): THREE.Quaternion {
  const m = new THREE.Matrix4()
  m.set(
    rot[0][0], rot[0][1], rot[0][2], 0,
    rot[1][0], rot[1][1], rot[1][2], 0,
    rot[2][0], rot[2][1], rot[2][2], 0,
    0, 0, 0, 1,
  )
  return new THREE.Quaternion().setFromRotationMatrix(m)
}

function materialFromEntity(entity: SpectatorEntity): THREE.Material {
  const color = toThreeColor(entity.color)

  switch (entity.material) {
    case 'Neon':
      return new THREE.MeshStandardMaterial({ color, emissive: color, emissiveIntensity: 0.45, roughness: 0.3, metalness: 0.3 })
    case 'Metal':
      return new THREE.MeshStandardMaterial({ color, roughness: 0.18, metalness: 0.94 })
    case 'Glass':
      return new THREE.MeshStandardMaterial({ color, roughness: 0.1, metalness: 0.1, transparent: true, opacity: 0.35 })
    case 'Wood':
      return new THREE.MeshStandardMaterial({ color, roughness: 0.8, metalness: 0.02 })
    default:
      return new THREE.MeshStandardMaterial({ color, roughness: 0.62, metalness: 0.16 })
  }
}

function geometryFromEntity(entity: SpectatorEntity): THREE.BufferGeometry {
  const size = entity.size ?? [1, 1, 1]
  if (entity.shape === 'Ball') return new THREE.SphereGeometry(Math.max(0.2, size[0] * 0.5), 16, 16)
  if (entity.shape === 'Cylinder') return new THREE.CylinderGeometry(Math.max(0.2, size[0] * 0.5), Math.max(0.2, size[0] * 0.5), Math.max(0.2, size[1]), 16)
  return new THREE.BoxGeometry(Math.max(0.2, size[0]), Math.max(0.2, size[1]), Math.max(0.2, size[2]))
}

function createEntityObject(entity: SpectatorEntity): THREE.Object3D {
  const geo = geometryFromEntity(entity)
  const mat = materialFromEntity(entity)
  const mesh = new THREE.Mesh(geo, mat)
  mesh.castShadow = true
  mesh.receiveShadow = true
  return mesh
}

function disposeObject(obj: THREE.Object3D): void {
  obj.traverse((node) => {
    const mesh = node as THREE.Mesh
    if (!mesh.isMesh) return

    mesh.geometry?.dispose()

    if (Array.isArray(mesh.material)) {
      for (const m of mesh.material) m.dispose()
    } else {
      mesh.material?.dispose()
    }
  })
}

function queueKillfeed(msg: string): void {
  const item = document.createElement('div')
  item.className = 'kill-msg'
  item.textContent = msg
  killfeedEl.prepend(item)
  while (killfeedEl.children.length > 6) {
    killfeedEl.lastElementChild?.remove()
  }
  window.setTimeout(() => item.remove(), 3500)
}

function chooseFollowTarget(obs: SpectatorObservation): string | null {
  if (selectedPlayerId && obs.players.some((p) => p.id === selectedPlayerId)) {
    return selectedPlayerId
  }

  let best: SpectatorPlayerInfo | null = null
  let bestScore = Number.NEGATIVE_INFINITY

  for (const p of obs.players) {
    const score = numberAttr(p.attributes, ['Score', 'Kills', 'Points']) ?? 0
    if (score > bestScore) {
      best = p
      bestScore = score
    }
  }

  return best?.id ?? obs.players[0]?.id ?? null
}

function updateScene(obs: SpectatorObservation): void {
  const activeIds = new Set<number>()

  for (const entity of obs.entities) {
    activeIds.add(entity.id)

    let obj = entityObjects.get(entity.id)
    if (!obj) {
      obj = createEntityObject(entity)
      entityObjects.set(entity.id, obj)
      scene.add(obj)
    }

    obj.position.set(entity.position[0], entity.position[1], entity.position[2])
    if (entity.rotation) obj.quaternion.copy(rotationToQuaternion(entity.rotation))
  }

  for (const [id, obj] of entityObjects) {
    if (!activeIds.has(id)) {
      scene.remove(obj)
      disposeObject(obj)
      entityObjects.delete(id)
    }
  }

  prevEntityIds.clear()
  for (const id of activeIds) prevEntityIds.add(id)
}

function updateHud(obs: SpectatorObservation): void {
  statusEl.textContent = `CONNECTED | TICK ${obs.tick}`

  selectedPlayerId = chooseFollowTarget(obs)
  const target = obs.players.find((p) => p.id === selectedPlayerId) ?? null
  targetEl.textContent = `FOLLOW: ${target ? target.name : 'NONE'}`

  if (!target) {
    healthTextEl.textContent = '-'
    healthBarEl.style.width = '0%'
    weaponEl.textContent = '-'
    ammoEl.textContent = '-'
    scoreEl.textContent = 'Score: -'
    phaseEl.textContent = `Phase: ${obs.game_status}`
    return
  }

  const hp = Math.max(0, Math.round(target.health))
  healthTextEl.textContent = String(hp)
  healthBarEl.style.width = `${clamp01(hp / 100) * 100}%`

  if (hp > 60) healthBarEl.style.background = 'linear-gradient(90deg, #3ef67d, #7dffa6)'
  else if (hp > 30) healthBarEl.style.background = 'linear-gradient(90deg, #ffad32, #ffd35f)'
  else healthBarEl.style.background = 'linear-gradient(90deg, #ff4545, #ff7777)'

  const weapon = stringAttr(target.attributes, ['WeaponName', 'CurrentWeaponName', 'Weapon'])
  const ammoMag = numberAttr(target.attributes, ['Ammo', 'AmmoMag', 'CurrentAmmo'])
  const ammoReserve = numberAttr(target.attributes, ['AmmoReserve', 'SpareAmmo', 'Reserve'])
  const score = numberAttr(target.attributes, ['Score', 'Kills', 'Points'])
  const wave = numberAttr(target.attributes, ['Wave'])
  const phase = stringAttr(target.attributes, ['Phase', 'RoundState'])

  weaponEl.textContent = weapon ?? 'Unknown Weapon'
  ammoEl.textContent = ammoMag !== null && ammoReserve !== null ? `${Math.round(ammoMag)} / ${Math.round(ammoReserve)}` : 'N/A'
  scoreEl.textContent = `Score: ${score !== null ? Math.round(score) : '-'}`

  const statusParts: string[] = []
  if (phase) statusParts.push(phase)
  if (wave !== null) statusParts.push(`Wave ${Math.round(wave)}`)
  if (!statusParts.length) statusParts.push(obs.game_status)
  phaseEl.textContent = `Phase: ${statusParts.join(' | ')}`
}

function updateKillfeed(obs: SpectatorObservation): void {
  const currentPlayerSet = new Set(obs.players.map((p) => p.id))

  for (const p of obs.players) {
    const prevHealth = prevPlayerHealth.get(p.id)
    if (prevHealth !== undefined) {
      const delta = prevHealth - p.health
      if (delta >= 20) queueKillfeed(`${p.name} took ${Math.round(delta)} damage`)
      if (prevHealth > 0 && p.health <= 0) queueKillfeed(`${p.name} eliminated`)
    }
    prevPlayerHealth.set(p.id, p.health)
  }

  for (const prevId of prevPlayers) {
    if (!currentPlayerSet.has(prevId)) {
      const gone = Array.from(obs.players).find((p) => p.id === prevId)
      queueKillfeed(`${gone?.name ?? 'Player'} left view`)
    }
  }

  prevPlayers.clear()
  for (const id of currentPlayerSet) prevPlayers.add(id)
}

function updateLeaderboard(): void {
  if (!leaderboardData.length) {
    leaderboardEl.textContent = 'No data'
    return
  }

  leaderboardEl.innerHTML = leaderboardData
    .slice(0, 8)
    .map((entry) => {
      const name = entry.name || entry.key
      return `<div class="lb-row"><span>#${entry.rank}</span><span>${name}</span><span>${Math.round(entry.score)}</span></div>`
    })
    .join('')
}

function drawMinimap(obs: SpectatorObservation): void {
  const w = minimapCanvas.width
  const h = minimapCanvas.height
  minimapCtx.clearRect(0, 0, w, h)

  minimapCtx.fillStyle = 'rgba(6, 8, 14, 0.9)'
  minimapCtx.fillRect(0, 0, w, h)

  const ents = obs.entities
  const players = obs.players

  let maxRadius = 20
  for (const e of ents) {
    const r = Math.max(Math.abs(e.position[0]), Math.abs(e.position[2]))
    if (r > maxRadius) maxRadius = r
  }
  for (const p of players) {
    const r = Math.max(Math.abs(p.position[0]), Math.abs(p.position[2]))
    if (r > maxRadius) maxRadius = r
  }

  const scale = (w * 0.45) / Math.max(20, maxRadius)

  const toScreen = (x: number, z: number): [number, number] => [x * scale + w / 2, z * scale + h / 2]

  minimapCtx.strokeStyle = 'rgba(120, 180, 255, 0.35)'
  minimapCtx.lineWidth = 1
  minimapCtx.strokeRect(4, 4, w - 8, h - 8)

  for (const e of ents) {
    const [x, z] = toScreen(e.position[0], e.position[2])
    minimapCtx.fillStyle = 'rgba(140, 180, 220, 0.55)'
    minimapCtx.fillRect(x - 1, z - 1, 2, 2)
  }

  for (const p of players) {
    const [x, z] = toScreen(p.position[0], p.position[2])
    const isTarget = p.id === selectedPlayerId
    minimapCtx.fillStyle = isTarget ? '#ffad33' : '#ffffff'
    minimapCtx.beginPath()
    minimapCtx.arc(x, z, isTarget ? 3 : 2, 0, Math.PI * 2)
    minimapCtx.fill()
  }
}

function updateCamera(obs: SpectatorObservation, dt: number): void {
  const targetPlayer = obs.players.find((p) => p.id === selectedPlayerId)
  if (!targetPlayer) return

  const rootEntity = targetPlayer.root_part_id ? obs.entities.find((e) => e.id === targetPlayer.root_part_id) : null
  const playerPos = new THREE.Vector3(targetPlayer.position[0], targetPlayer.position[1] + 2, targetPlayer.position[2])

  const forward = new THREE.Vector3(0, 0, -1)
  if (rootEntity?.rotation) {
    const q = rotationToQuaternion(rootEntity.rotation)
    forward.applyQuaternion(q)
  }

  forward.y = 0
  if (forward.lengthSq() < 0.001) forward.set(0, 0, -1)
  forward.normalize()

  const desired = playerPos.clone()
    .addScaledVector(forward, -7.5)
    .add(new THREE.Vector3(0, 3.2, 0))

  const alpha = 1 - Math.exp(-8 * dt)
  camera.position.lerp(desired, alpha)

  const lookAt = playerPos.clone().addScaledVector(forward, 10)
  camera.lookAt(lookAt)
}

function handleObservation(obs: SpectatorObservation): void {
  latestObservation = obs
  updateScene(obs)
  updateKillfeed(obs)
  updateHud(obs)
  drawMinimap(obs)
}

function setDisconnected(): void {
  statusEl.textContent = 'DISCONNECTED'
}

function connectSpectateWs(): void {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const ws = new WebSocket(`${protocol}//${window.location.host}/api/v1/games/${gameId}/spectate/ws`)
  ws.binaryType = 'arraybuffer'

  statusEl.textContent = 'CONNECTING'

  ws.onmessage = (event) => {
    try {
      let json: string
      if (event.data instanceof ArrayBuffer) {
        const decompressed = pako.ungzip(new Uint8Array(event.data))
        json = new TextDecoder().decode(decompressed)
      } else {
        json = event.data
      }

      const payload = JSON.parse(json) as SpectatorObservation | { error: string }
      if ('error' in payload) {
        statusEl.textContent = `ERROR: ${payload.error}`
        return
      }

      handleObservation(payload)
    } catch (err) {
      statusEl.textContent = `PARSE ERROR`
      console.error(err)
    }
  }

  ws.onclose = () => {
    setDisconnected()
    window.setTimeout(connectSpectateWs, 1500)
  }

  ws.onerror = () => {
    setDisconnected()
  }
}

async function refreshLeaderboard(): Promise<void> {
  try {
    const resp = await fetch(`/api/v1/games/${gameId}/leaderboard`)
    if (!resp.ok) return

    const body = await resp.json() as { entries?: LeaderboardEntry[] }
    leaderboardData = body.entries ?? []
    updateLeaderboard()
  } catch {
    // ignore
  }
}

window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight
  camera.updateProjectionMatrix()
  renderer.setSize(window.innerWidth, window.innerHeight)
})

window.addEventListener('keydown', (event) => {
  if (!latestObservation) return
  if (event.code !== 'Tab') return
  event.preventDefault()

  const players = latestObservation.players
  if (!players.length) return

  const idx = players.findIndex((p) => p.id === selectedPlayerId)
  const next = players[(idx + 1) % players.length]
  selectedPlayerId = next.id
  targetEl.textContent = `FOLLOW: ${next.name}`
})

connectSpectateWs()
void refreshLeaderboard()
window.setInterval(() => void refreshLeaderboard(), 3000)

function frame(): void {
  requestAnimationFrame(frame)

  const dt = Math.min(clock.getDelta(), 0.05)

  if (latestObservation) {
    updateCamera(latestObservation, dt)
  }

  const t = performance.now() * 0.001
  accentLights.forEach((light, i) => {
    light.intensity = 1.2 + Math.sin(t * 2 + i * 0.7) * 0.45
  })

  renderer.render(scene, camera)
}

frame()

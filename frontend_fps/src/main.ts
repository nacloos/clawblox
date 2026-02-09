import * as pako from 'pako'
import * as THREE from 'three'
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js'
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js'
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone as cloneSkeleton } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { geometryFromRender, materialFromRender, type RenderSpec } from './render/presets'

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
  size: [number, number, number]
  render: RenderSpec
  model_url?: string
}

interface SpectatorObservation {
  tick: number
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
const healthTextEl = document.getElementById('health-text') as HTMLDivElement
const healthBarEl = document.getElementById('health-bar') as HTMLDivElement
const ammoMagEl = document.querySelector('#ammo-text .mag') as HTMLSpanElement
const ammoReserveEl = document.querySelector('#ammo-text .reserve') as HTMLSpanElement
const weaponNameEl = document.getElementById('weapon-name') as HTMLDivElement
const scoreTextEl = document.getElementById('score-text') as HTMLDivElement
const waveTextEl = document.getElementById('wave-text') as HTMLDivElement
const spectateTextEl = document.getElementById('spectate-text') as HTMLDivElement
const killfeedEl = document.getElementById('killfeed') as HTMLDivElement
const leaderboardEl = document.getElementById('leaderboard') as HTMLDivElement
const damageOverlayEl = document.getElementById('damage-overlay') as HTMLDivElement
const minimapCanvas = document.getElementById('minimap-canvas') as HTMLCanvasElement
const minimapCtx = minimapCanvas.getContext('2d')
if (!minimapCtx) throw new Error('Minimap context unavailable')

const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance' })
renderer.setSize(window.innerWidth, window.innerHeight)
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
renderer.shadowMap.enabled = true
renderer.shadowMap.type = THREE.PCFSoftShadowMap
renderer.toneMapping = THREE.ACESFilmicToneMapping
renderer.toneMappingExposure = 1.8
renderer.outputColorSpace = THREE.SRGBColorSpace

const scene = new THREE.Scene()
scene.fog = new THREE.FogExp2(0x1a1a2a, 0.008)
scene.background = new THREE.Color(0x1a1a2a)

const camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 260)
camera.position.set(0, 7, 14)

const composer = new EffectComposer(renderer)
composer.addPass(new RenderPass(scene, camera))
const bloom = new UnrealBloomPass(new THREE.Vector2(window.innerWidth, window.innerHeight), 0.6, 0.8, 0.5)
composer.addPass(bloom)

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
dirLight.shadow.camera.far = 120
dirLight.shadow.bias = -0.001
scene.add(dirLight)

const accentColors = [0xff4444, 0x4488ff, 0xff8800, 0x44ff88, 0xff44ff, 0xffff44]
const lightPositions = [
  [-15, 3, -15], [15, 3, -15], [-15, 3, 15], [15, 3, 15],
  [0, 3, -20], [0, 3, 20], [-20, 3, 0], [20, 3, 0],
  [-8, 3, -8], [8, 3, 8], [-8, 3, 8], [8, 3, -8],
]
const accentLights: THREE.PointLight[] = []
lightPositions.forEach((pos, i) => {
  const light = new THREE.PointLight(accentColors[i % accentColors.length], 4, 20)
  light.position.set(pos[0], pos[1], pos[2])
  scene.add(light)
  accentLights.push(light)
})

function rotationToQuaternion(rot: [[number, number, number], [number, number, number], [number, number, number]]): THREE.Quaternion {
  const m = new THREE.Matrix4()
  m.set(rot[0][0], rot[0][1], rot[0][2], 0, rot[1][0], rot[1][1], rot[1][2], 0, rot[2][0], rot[2][1], rot[2][2], 0, 0, 0, 0, 1)
  return new THREE.Quaternion().setFromRotationMatrix(m)
}

function materialFromEntity(entity: SpectatorEntity): THREE.Material {
  return materialFromRender(entity.render)
}

function geometryFromEntity(entity: SpectatorEntity): THREE.BufferGeometry {
  return geometryFromRender(entity.render, entity.size)
}

const entityObjects = new Map<number, THREE.Object3D>()
const gltfLoader = new GLTFLoader()
const modelTemplateCache = new Map<string, Promise<THREE.Object3D>>()
const clock = new THREE.Clock()
let latestObservation: SpectatorObservation | null = null
let selectedPlayerId: string | null = null
let leaderboardData: LeaderboardEntry[] = []
let lastObservedHealth: number | null = null

const prevPlayerHealth = new Map<string, number>()

function numberAttr(attrs: Record<string, unknown> | undefined, keys: string[]): number | null {
  if (!attrs) return null
  for (const key of keys) {
    const v = attrs[key]
    if (typeof v === 'number' && Number.isFinite(v)) return v
  }
  return null
}

function stringAttr(attrs: Record<string, unknown> | undefined, keys: string[]): string | null {
  if (!attrs) return null
  for (const key of keys) {
    const v = attrs[key]
    if (typeof v === 'string' && v.length > 0) return v
  }
  return null
}

function queueKillfeed(text: string): void {
  const node = document.createElement('div')
  node.className = 'kill-msg'
  node.textContent = text
  killfeedEl.prepend(node)
  while (killfeedEl.children.length > 8) {
    killfeedEl.lastElementChild?.remove()
  }
  window.setTimeout(() => node.remove(), 3400)
}

function disposeObject(obj: THREE.Object3D): void {
  obj.traverse((n) => {
    const mesh = n as THREE.Mesh
    if (!mesh.isMesh) return
    mesh.geometry.dispose()
    const material = mesh.material
    if (Array.isArray(material)) material.forEach((m) => m.dispose())
    else material.dispose()
  })
}

function createEntityObject(entity: SpectatorEntity): THREE.Object3D {
  if (entity.model_url) {
    const root = new THREE.Group()
    root.name = `entity-${entity.id}`
    root.userData.entityId = entity.id
    const fallback = createPrimitiveEntityMesh(entity)
    root.add(fallback)
    void attachModelToEntityRoot(root, entity.model_url, entity.size, entity.render)
    return root
  }
  return createPrimitiveEntityMesh(entity)
}

function createPrimitiveEntityMesh(entity: SpectatorEntity): THREE.Mesh {
  const preset = entity.render.preset_id ?? ''
  const isFloorOrCeiling = preset === 'fps_arena/floor' || preset === 'fps_arena/ceiling'
  const geometry = isFloorOrCeiling
    ? new THREE.PlaneGeometry(entity.size[0], entity.size[2])
    : geometryFromEntity(entity)
  const mesh = new THREE.Mesh(geometry, materialFromEntity(entity))
  if (isFloorOrCeiling) {
    mesh.rotation.x = preset === 'fps_arena/floor' ? -Math.PI / 2 : Math.PI / 2
  }
  mesh.castShadow = entity.render.casts_shadow
  mesh.receiveShadow = entity.render.receives_shadow
  return mesh
}

function loadModelTemplate(url: string): Promise<THREE.Object3D> {
  let cached = modelTemplateCache.get(url)
  if (!cached) {
    cached = new Promise((resolve, reject) => {
      gltfLoader.load(
        url,
        (gltf) => resolve(gltf.scene),
        undefined,
        (error) => reject(error),
      )
    })
    modelTemplateCache.set(url, cached)
  }
  return cached
}

function fitModelToSize(model: THREE.Object3D, size: [number, number, number]): void {
  const box = new THREE.Box3().setFromObject(model)
  const source = box.getSize(new THREE.Vector3())
  if (source.y <= 0.0001) return

  const targetHeight = Math.max(size[1], 0.001)
  const scale = targetHeight / source.y
  model.scale.setScalar(scale)

  const scaledBox = new THREE.Box3().setFromObject(model)
  const center = scaledBox.getCenter(new THREE.Vector3())
  const minY = scaledBox.min.y
  const targetMinY = -targetHeight * 0.5

  model.position.set(
    model.position.x - center.x,
    model.position.y + (targetMinY - minY),
    model.position.z - center.z,
  )
}

function setShadowFlags(root: THREE.Object3D, casts: boolean, receives: boolean): void {
  root.traverse((node) => {
    const mesh = node as THREE.Mesh
    if (!mesh.isMesh) return
    mesh.castShadow = casts
    mesh.receiveShadow = receives
  })
}

async function attachModelToEntityRoot(
  root: THREE.Group,
  modelUrl: string,
  size: [number, number, number],
  render: RenderSpec,
): Promise<void> {
  try {
    const template = await loadModelTemplate(modelUrl)
    const entityId = root.userData.entityId as number | undefined
    if (typeof entityId !== 'number' || !entityObjects.has(entityId)) {
      return
    }

    for (const child of [...root.children]) {
      root.remove(child)
      disposeObject(child)
    }
    const clone = cloneSkeleton(template)
    fitModelToSize(clone, size)
    setShadowFlags(clone, render.casts_shadow, render.receives_shadow)
    root.add(clone)
  } catch (error) {
    console.warn('Failed to load model for entity', modelUrl, error)
  }
}

function chooseFollowTarget(obs: SpectatorObservation): string | null {
  if (selectedPlayerId && obs.players.some((p) => p.id === selectedPlayerId)) return selectedPlayerId

  let best: SpectatorPlayerInfo | null = null
  let bestScore = -Infinity
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

    const preset = entity.render.preset_id ?? ''
    if (preset === 'fps_arena/floor') {
      obj.position.set(entity.position[0], entity.position[1] + entity.size[1] * 0.5, entity.position[2])
    } else {
      obj.position.set(entity.position[0], entity.position[1], entity.position[2])
    }
    if (entity.rotation) obj.quaternion.copy(rotationToQuaternion(entity.rotation))
    obj.visible = entity.render.visible
  }

  for (const [id, obj] of entityObjects) {
    if (!activeIds.has(id)) {
      scene.remove(obj)
      disposeObject(obj)
      entityObjects.delete(id)
    }
  }
}

function flashDamageOverlay(): void {
  damageOverlayEl.style.opacity = '0.58'
  window.setTimeout(() => {
    damageOverlayEl.style.opacity = '0'
  }, 130)
}

function updateHud(obs: SpectatorObservation): void {
  selectedPlayerId = chooseFollowTarget(obs)
  const target = obs.players.find((p) => p.id === selectedPlayerId) ?? null

  if (!target) {
    spectateTextEl.textContent = 'No players'
    healthTextEl.textContent = '-'
    healthBarEl.style.width = '0%'
    ammoMagEl.textContent = '-'
    ammoReserveEl.textContent = '-'
    weaponNameEl.textContent = 'Unknown'
    scoreTextEl.textContent = '0'
    waveTextEl.textContent = 'Spectating'
    return
  }

  spectateTextEl.textContent = `Following ${target.name} • tick ${obs.tick}`

  const hp = Math.max(0, Math.round(target.health))
  if (lastObservedHealth !== null && hp < lastObservedHealth) flashDamageOverlay()
  lastObservedHealth = hp

  healthTextEl.textContent = String(hp)
  healthBarEl.style.width = `${Math.max(0, Math.min(100, hp))}%`

  if (hp > 60) {
    healthBarEl.style.background = 'linear-gradient(90deg, #44ff44, #88ff44)'
  } else if (hp > 30) {
    healthBarEl.style.background = 'linear-gradient(90deg, #ffaa00, #ffcc44)'
  } else {
    healthBarEl.style.background = 'linear-gradient(90deg, #ff2222, #ff4444)'
  }

  const weapon = stringAttr(target.attributes, ['WeaponName', 'CurrentWeaponName', 'Weapon'])
  const ammoMag = numberAttr(target.attributes, ['Ammo', 'AmmoMag', 'CurrentAmmo'])
  const ammoReserve = numberAttr(target.attributes, ['AmmoReserve', 'SpareAmmo', 'Reserve'])
  const score = numberAttr(target.attributes, ['Score', 'Kills', 'Points'])
  const phase = stringAttr(target.attributes, ['MatchState', 'Phase', 'RoundState'])

  weaponNameEl.textContent = weapon ?? 'Rifle'
  ammoMagEl.textContent = ammoMag === null ? '-' : `${Math.round(ammoMag)}`
  ammoReserveEl.textContent = ammoReserve === null ? '-' : `${Math.round(ammoReserve)}`
  scoreTextEl.textContent = score === null ? '0' : `${Math.round(score)}`
  waveTextEl.textContent = phase ?? obs.game_status
}

function updateKillfeed(obs: SpectatorObservation): void {
  for (const p of obs.players) {
    const prev = prevPlayerHealth.get(p.id)
    prevPlayerHealth.set(p.id, p.health)

    if (prev !== undefined) {
      const delta = prev - p.health
      if (delta >= 18) queueKillfeed(`${p.name} took ${Math.round(delta)} dmg`)
      if (prev > 0 && p.health <= 0) queueKillfeed(`${p.name} eliminated`)
    }
  }
}

function updateLeaderboard(): void {
  if (!leaderboardData.length) {
    leaderboardEl.textContent = 'No data'
    return
  }

  leaderboardEl.innerHTML = leaderboardData.slice(0, 8).map((entry) => {
    const name = entry.name || entry.key
    return `<div class="lb-row"><span>#${entry.rank}</span><span>${name}</span><span>${Math.round(entry.score)}</span></div>`
  }).join('')
}

function drawMinimap(obs: SpectatorObservation): void {
  const ctx = minimapCtx
  const w = minimapCanvas.width
  const h = minimapCanvas.height
  ctx.clearRect(0, 0, w, h)

  ctx.fillStyle = 'rgba(0,0,0,0.8)'
  ctx.fillRect(0, 0, w, h)

  const all = [...obs.entities.map((e) => e.position), ...obs.players.map((p) => p.position)]
  let maxAbs = 30
  for (const p of all) {
    maxAbs = Math.max(maxAbs, Math.abs(p[0]), Math.abs(p[2]))
  }
  const scale = (w * 0.44) / maxAbs

  const sx = (x: number) => x * scale + w / 2
  const sz = (z: number) => z * scale + h / 2

  ctx.fillStyle = 'rgba(80,80,100,0.45)'
  for (const e of obs.entities) {
    const x = sx(e.position[0])
    const z = sz(e.position[2])
    ctx.fillRect(x - 1, z - 1, 2, 2)
  }

  for (const p of obs.players) {
    const x = sx(p.position[0])
    const z = sz(p.position[2])
    const active = p.id === selectedPlayerId
    ctx.fillStyle = active ? '#ffad33' : '#ffffff'
    ctx.beginPath()
    ctx.arc(x, z, active ? 3 : 2.1, 0, Math.PI * 2)
    ctx.fill()
  }
}

function updateCamera(obs: SpectatorObservation, dt: number): void {
  const target = obs.players.find((p) => p.id === selectedPlayerId)
  if (!target) return

  const root = target.root_part_id ? obs.entities.find((e) => e.id === target.root_part_id) : null

  const pos = new THREE.Vector3(target.position[0], target.position[1] + 1.6, target.position[2])
  const forward = new THREE.Vector3(0, 0, -1)
  if (root?.rotation) forward.applyQuaternion(rotationToQuaternion(root.rotation))
  forward.y = 0
  if (forward.lengthSq() < 0.0001) forward.set(0, 0, -1)
  forward.normalize()

  const desired = pos.clone().addScaledVector(forward, -6.5).add(new THREE.Vector3(0, 2.6, 0))
  const alpha = 1 - Math.exp(-8 * dt)
  camera.position.lerp(desired, alpha)

  const lookAt = pos.clone().addScaledVector(forward, 14)
  camera.lookAt(lookAt)
}

function getGameId(): string {
  const fromPath = window.location.pathname.match(/\/spectate\/([0-9a-fA-F-]{36})/)
  if (fromPath?.[1]) return fromPath[1]
  const fromQuery = new URLSearchParams(window.location.search).get('game')
  if (fromQuery) return fromQuery
  throw new Error('Missing game id. Use /spectate/<game_id> or ?game=<game_id>')
}

const gameId = getGameId()

function handleObservation(obs: SpectatorObservation): void {
  latestObservation = obs
  updateScene(obs)
  updateHud(obs)
  updateKillfeed(obs)
  drawMinimap(obs)
}

function setConnectionState(text: string): void {
  spectateTextEl.textContent = text
}

function connectWs(): void {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const ws = new WebSocket(`${protocol}//${window.location.host}/api/v1/games/${gameId}/spectate/ws`)
  ws.binaryType = 'arraybuffer'

  setConnectionState('Connecting...')

  ws.onmessage = (event) => {
    try {
      let raw: string
      if (event.data instanceof ArrayBuffer) {
        raw = new TextDecoder().decode(pako.ungzip(new Uint8Array(event.data)))
      } else {
        raw = String(event.data)
      }

      const parsed = JSON.parse(raw) as SpectatorObservation | { error: string }
      if ('error' in parsed) {
        setConnectionState(`Error: ${parsed.error}`)
        return
      }

      handleObservation(parsed)
    } catch {
      setConnectionState('Parse error')
    }
  }

  ws.onerror = () => setConnectionState('WS error')
  ws.onclose = () => {
    setConnectionState('Disconnected - reconnecting...')
    window.setTimeout(connectWs, 1500)
  }
}

async function refreshLeaderboard(): Promise<void> {
  try {
    const r = await fetch(`/api/v1/games/${gameId}/leaderboard`)
    if (!r.ok) return
    const data = await r.json() as { entries?: LeaderboardEntry[] }
    leaderboardData = data.entries ?? []
    updateLeaderboard()
  } catch {
    // ignore
  }
}

window.addEventListener('keydown', (event) => {
  if (!latestObservation) return
  if (event.code !== 'Tab') return
  event.preventDefault()

  const players = latestObservation.players
  if (!players.length) return
  const i = players.findIndex((p) => p.id === selectedPlayerId)
  selectedPlayerId = players[(i + 1) % players.length].id
})

window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight
  camera.updateProjectionMatrix()
  renderer.setSize(window.innerWidth, window.innerHeight)
  composer.setSize(window.innerWidth, window.innerHeight)
})

connectWs()
void refreshLeaderboard()
window.setInterval(() => void refreshLeaderboard(), 3000)

function frame(): void {
  requestAnimationFrame(frame)
  const dt = Math.min(clock.getDelta(), 0.05)

  if (latestObservation) {
    updateCamera(latestObservation, dt)
  }

  const t = Date.now() * 0.001
  accentLights.forEach((l, i) => {
    l.intensity = 1.5 + Math.sin(t + i * 0.7) * 0.8
  })

  composer.render()
}

frame()

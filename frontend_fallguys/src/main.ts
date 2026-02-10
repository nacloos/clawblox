import * as pako from 'pako'
import * as THREE from 'three'
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js'
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js'
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone as cloneSkeleton } from 'three/examples/jsm/utils/SkeletonUtils.js'
import type { SpectatorObservation, SpectatorEntity } from './types'
import { createJellybean, animateJellybean } from './jellybean'
import {
  buildSky, buildWater, buildClouds, buildDecorations,
  updateDecorations, updateClouds,
} from './course'
import { spawnConfetti, updateParticles } from './particles'

// ── Config ──────────────────────────────────────────────────
const FINISH_Z = 250 * 4
const AI_COLORS = [0xffd93d, 0x6c5ce7, 0x00b894, 0xe17055, 0x74b9ff, 0xa29bfe]
const AI_ACCESSORIES: Array<'crown' | 'propeller' | 'headband' | 'cone'> = [
  'crown', 'propeller', 'headband', 'cone',
]
const PLAYER_COLOR = 0xff69b4
const HEX_COLORS = [0xff6b6b, 0x48dbfb, 0xfeca57, 0xff9ff3, 0x54a0ff, 0x5f27cd]

// ── DOM ─────────────────────────────────────────────────────
const canvas = document.getElementById('game') as HTMLCanvasElement
const timerEl = document.getElementById('timer') as HTMLDivElement
const positionEl = document.getElementById('position') as HTMLDivElement
const progressFill = document.getElementById('progress-fill') as HTMLDivElement
const spectateTextEl = document.getElementById('spectate-text') as HTMLSpanElement
const finishScreen = document.getElementById('finish-screen') as HTMLDivElement
const finishPlaceEl = document.getElementById('finish-place') as HTMLDivElement
const finishTimeEl = document.getElementById('finish-time') as HTMLDivElement

// ── Renderer ────────────────────────────────────────────────
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true })
renderer.setSize(window.innerWidth, window.innerHeight)
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
renderer.shadowMap.enabled = true
renderer.shadowMap.type = THREE.PCFSoftShadowMap
renderer.toneMapping = THREE.ACESFilmicToneMapping
renderer.toneMappingExposure = 1.2

const scene = new THREE.Scene()
const camera = new THREE.PerspectiveCamera(60, window.innerWidth / window.innerHeight, 0.1, 3000)

const composer = new EffectComposer(renderer)
composer.addPass(new RenderPass(scene, camera))
composer.addPass(new UnrealBloomPass(
  new THREE.Vector2(window.innerWidth, window.innerHeight), 0.3, 0.4, 0.85
))

window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight
  camera.updateProjectionMatrix()
  renderer.setSize(window.innerWidth, window.innerHeight)
  composer.setSize(window.innerWidth, window.innerHeight)
})

// ── Lighting ────────────────────────────────────────────────
scene.add(new THREE.AmbientLight(0xffffff, 0.6))
scene.add(new THREE.HemisphereLight(0x87ceeb, 0x98fb98, 0.4))

const dirLight = new THREE.DirectionalLight(0xffffff, 1.2)
dirLight.position.set(30, 50, 20)
dirLight.castShadow = true
dirLight.shadow.mapSize.set(2048, 2048)
dirLight.shadow.camera.left = -80
dirLight.shadow.camera.right = 80
dirLight.shadow.camera.top = 80
dirLight.shadow.camera.bottom = -80
dirLight.shadow.camera.near = 1
dirLight.shadow.camera.far = 200
dirLight.shadow.bias = -0.001
scene.add(dirLight)
scene.add(dirLight.target)

// ── Environment ─────────────────────────────────────────────
buildSky(scene)
const waterMat = buildWater(scene)
const clouds = buildClouds(scene)
const decorations = buildDecorations(scene)

// ── Entity rendering ────────────────────────────────────────
const entityObjects = new Map<number, THREE.Object3D>()
const gltfLoader = new GLTFLoader()
const modelTemplateCache = new Map<string, Promise<{ scene: THREE.Object3D; animations: THREE.AnimationClip[] }>>()

interface ModelAnimState {
  mixer: THREE.AnimationMixer
  walkAction: THREE.AnimationAction | null
  idleAction: THREE.AnimationAction | null
}

const modelAnimStates = new Map<number, ModelAnimState>()

function loadModelTemplate(url: string): Promise<{ scene: THREE.Object3D; animations: THREE.AnimationClip[] }> {
  let cached = modelTemplateCache.get(url)
  if (!cached) {
    cached = new Promise((resolve, reject) => {
      gltfLoader.load(
        url,
        (gltf) => resolve({ scene: gltf.scene, animations: gltf.animations ?? [] }),
        undefined,
        (error) => reject(error),
      )
    })
    modelTemplateCache.set(url, cached)
  }
  return cached
}

function fitModelToSize(model: THREE.Object3D, size: [number, number, number]): void {
  // Find the first skinned or regular mesh for accurate bounding box
  let mesh: THREE.Mesh | null = null
  model.traverse((obj) => {
    if (mesh) return
    const skinned = obj as THREE.SkinnedMesh
    if (skinned.isSkinnedMesh) { mesh = skinned; return }
    const regular = obj as THREE.Mesh
    if (regular.isMesh) { mesh = regular }
  })

  const sourceBox = new THREE.Box3()
  if (mesh) {
    (mesh as THREE.Mesh).geometry.computeBoundingBox()
    const bb = (mesh as THREE.Mesh).geometry.boundingBox
    if (!bb) return
    sourceBox.copy(bb)
  } else {
    sourceBox.setFromObject(model)
  }

  const src = sourceBox.getSize(new THREE.Vector3())
  if (src.y <= 0.0001) return
  const center = sourceBox.getCenter(new THREE.Vector3())
  const scale = size[1] / src.y
  model.scale.setScalar(scale)
  model.position.set(-center.x * scale, -center.y * scale, -center.z * scale)
}

async function loadAndAttachModel(
  root: THREE.Group,
  modelUrl: string,
  size: [number, number, number],
  yawOffsetDeg?: number,
): Promise<void> {
  try {
    const loaded = await loadModelTemplate(modelUrl)
    const entityId = root.userData.entityId as number | undefined
    if (typeof entityId !== 'number' || !entityObjects.has(entityId)) return

    // Remove any fallback children
    for (const child of [...root.children]) {
      root.remove(child)
    }

    const clone = cloneSkeleton(loaded.scene)
    fitModelToSize(clone, size)
    if (yawOffsetDeg) clone.rotation.y = THREE.MathUtils.degToRad(yawOffsetDeg)
    clone.traverse((node) => {
      const mesh = node as THREE.Mesh
      if (mesh.isMesh) { mesh.castShadow = true; mesh.receiveShadow = true }
    })
    root.add(clone)

    if (loaded.animations.length > 0) {
      const mixer = new THREE.AnimationMixer(clone)
      const findClip = (pattern: RegExp) => loaded.animations.find((c) => pattern.test(c.name))
      const walkClip = findClip(/(walk|run|jog|locomotion)/i) ?? loaded.animations[0]
      const idleClip = findClip(/idle/i)

      const walkAction = walkClip ? mixer.clipAction(walkClip) : null
      const idleAction = idleClip ? mixer.clipAction(idleClip) : null

      if (idleAction) { idleAction.reset(); idleAction.play() }
      if (walkAction) { walkAction.reset(); walkAction.play(); walkAction.setEffectiveWeight(0) }

      modelAnimStates.set(entityId, { mixer, walkAction, idleAction })
    }
  } catch (err) {
    console.warn('Failed to load model', modelUrl, err)
  }
}

let selectedPlayerId: string | null = null
let followTargetPos = new THREE.Vector3(0, 4, 20)
let camPos = new THREE.Vector3(0, 16, -5)
let camTarget = new THREE.Vector3(0, 6, 20)
let finishTriggered = false

// ── Interpolation buffer ────────────────────────────────────
const INTERP_DELAY = 0.10
const SNAPSHOT_BUFFER_SIZE = 8

interface TimestampedSnapshot {
  time: number
  obs: SpectatorObservation
}

const snapshotBuffer: TimestampedSnapshot[] = []

function pushSnapshot(obs: SpectatorObservation): void {
  snapshotBuffer.push({ time: performance.now() / 1000, obs })
  while (snapshotBuffer.length > SNAPSHOT_BUFFER_SIZE) {
    snapshotBuffer.shift()
  }
}

function lerpPos(
  a: [number, number, number],
  b: [number, number, number],
  t: number,
): [number, number, number] {
  return [
    a[0] + (b[0] - a[0]) * t,
    a[1] + (b[1] - a[1]) * t,
    a[2] + (b[2] - a[2]) * t,
  ]
}

function interpolatedObservation(): SpectatorObservation | null {
  if (snapshotBuffer.length === 0) return null
  if (snapshotBuffer.length === 1) return snapshotBuffer[0].obs

  const renderTime = performance.now() / 1000 - INTERP_DELAY

  if (renderTime <= snapshotBuffer[0].time) return snapshotBuffer[0].obs

  let from: TimestampedSnapshot | null = null
  let to: TimestampedSnapshot | null = null
  for (let i = 0; i < snapshotBuffer.length - 1; i++) {
    if (snapshotBuffer[i].time <= renderTime && snapshotBuffer[i + 1].time >= renderTime) {
      from = snapshotBuffer[i]
      to = snapshotBuffer[i + 1]
      break
    }
  }

  if (!from || !to) return snapshotBuffer[snapshotBuffer.length - 1].obs

  const span = to.time - from.time
  const t = span > 0.0001 ? Math.min(1, Math.max(0, (renderTime - from.time) / span)) : 1

  const obsA = from.obs
  const obsB = to.obs

  const players = obsB.players.map((pb) => {
    const pa = obsA.players.find((p) => p.id === pb.id)
    if (!pa) return pb
    return { ...pb, position: lerpPos(pa.position, pb.position, t) }
  })

  const entities = obsB.entities.map((eb) => {
    const ea = obsA.entities.find((e) => e.id === eb.id)
    if (!ea) return eb
    return { ...eb, position: lerpPos(ea.position, eb.position, t) }
  })

  return { ...obsB, players, entities }
}

// ── Helpers ──────────────────────────────────────────────────
function colorToHex(c: [number, number, number]): number {
  return (Math.round(c[0] * 255) << 16) | (Math.round(c[1] * 255) << 8) | Math.round(c[2] * 255)
}

// ── Entity classification ───────────────────────────────────
// Classify entities by name pattern to apply correct visual treatment
interface EntityClassification {
  type: 'disc' | 'hex' | 'pendulum_ball' | 'pendulum_arm' | 'pendulum_pillar' | 'pendulum_bar'
    | 'bot' | 'platform' | 'bridge' | 'rail' | 'arch' | 'generic'
  index?: number
}

function classifyEntity(entity: SpectatorEntity): EntityClassification {
  const role = entity.render?.role ?? ''

  // Bot entities
  if (role.startsWith('Bot_')) {
    const idx = parseInt(role.split('_')[1], 10) || 0
    return { type: 'bot', index: idx }
  }

  // Spinning discs
  if (role.startsWith('SpinDisc_')) {
    const idx = parseInt(role.split('_')[1], 10) || 0
    return { type: 'disc', index: idx }
  }

  // Hex tiles
  if (role.startsWith('HexTile_')) return { type: 'hex' }

  // Pendulum parts
  if (role.startsWith('PendBall_')) return { type: 'pendulum_ball' }
  if (role.startsWith('PendArm_')) return { type: 'pendulum_arm' }
  if (role.startsWith('PendPillar')) return { type: 'pendulum_pillar' }
  if (role.startsWith('PendBar_')) return { type: 'pendulum_bar' }

  // Bridge
  if (role === 'Bridge') return { type: 'bridge' }
  if (role.startsWith('BridgeRail')) return { type: 'rail' }

  // Arch
  if (role.startsWith('Arch')) return { type: 'arch' }

  // Platforms
  if (role.startsWith('Start') || role.startsWith('Transition') || role.startsWith('Finish')) {
    return { type: 'platform' }
  }

  return { type: 'generic' }
}

// ── Create visual for entity ────────────────────────────────
function rotationToQuaternion(
  rot: [[number, number, number], [number, number, number], [number, number, number]],
): THREE.Quaternion {
  const m = new THREE.Matrix4()
  m.set(
    rot[0][0], rot[0][1], rot[0][2], 0,
    rot[1][0], rot[1][1], rot[1][2], 0,
    rot[2][0], rot[2][1], rot[2][2], 0,
    0, 0, 0, 1,
  )
  return new THREE.Quaternion().setFromRotationMatrix(m)
}

function createEntityVisual(entity: SpectatorEntity): THREE.Object3D {
  const role = entity.render?.role ?? ''
  const classification = classifyEntity(entity)
  const color = colorToHex(entity.render.color)
  const size = entity.size ?? [1, 1, 1]
  const primitive = entity.render.primitive

  // Entity with model_url -> load GLB (player characters)
  if (entity.model_url) {
    const root = new THREE.Group()
    root.userData.entityId = entity.id
    root.userData.isModelEntity = true
    void loadAndAttachModel(root, entity.model_url, size, entity.model_yaw_offset_deg)
    return root
  }

  // Bot -> jellybean model (scaled to entity size)
  if (classification.type === 'bot') {
    const idx = (classification.index ?? 1) - 1
    const botColor = AI_COLORS[idx % AI_COLORS.length]
    const accessory = AI_ACCESSORIES[idx % AI_ACCESSORIES.length]
    const jelly = createJellybean(botColor, accessory)
    jelly.userData.isJellybean = true
    jelly.userData.botIndex = idx
    // Scale jellybean to match entity height (jellybean is ~1.3 units tall natively)
    const entityHeight = size[1]
    const jellyScale = entityHeight / 1.3
    jelly.scale.setScalar(jellyScale)
    return jelly
  }

  // Spinning disc -> cylinder with ring decoration
  if (classification.type === 'disc') {
    const group = new THREE.Group()
    const discMat = new THREE.MeshStandardMaterial({ color: 0x74b9ff, roughness: 0.4, metalness: 0.1 })
    const r = size[0] / 2 // diameter -> radius
    const disc = new THREE.Mesh(new THREE.CylinderGeometry(r, r, size[1], 32), discMat)
    disc.receiveShadow = true
    disc.castShadow = true
    group.add(disc)
    // Ring decoration
    const ringMat = new THREE.MeshStandardMaterial({ color: 0xdfe6e9, roughness: 0.5, side: THREE.DoubleSide })
    const ring = new THREE.Mesh(new THREE.RingGeometry(r * 0.3, r * 0.6, 32), ringMat)
    ring.rotation.x = -Math.PI / 2
    ring.position.y = size[1] / 2 + 0.01
    group.add(ring)
    return group
  }

  // Hex tile -> colored cylinder
  if (classification.type === 'hex') {
    const r = size[0] / 2
    const h = size[1]
    const hexMat = new THREE.MeshStandardMaterial({ color, roughness: 0.4, metalness: 0.1 })
    const mesh = new THREE.Mesh(new THREE.CylinderGeometry(r, r, h, 6), hexMat)
    mesh.receiveShadow = true
    mesh.castShadow = true
    mesh.userData.hexMat = hexMat
    return mesh
  }

  // Pendulum ball -> red sphere
  if (classification.type === 'pendulum_ball') {
    const r = size[0] / 2
    const ballMat = new THREE.MeshStandardMaterial({ color: 0xe74c3c, roughness: 0.3, metalness: 0.3 })
    const mesh = new THREE.Mesh(new THREE.SphereGeometry(r, 16, 16), ballMat)
    mesh.castShadow = true
    return mesh
  }

  // Pendulum arm -> gray cylinder
  if (classification.type === 'pendulum_arm') {
    const pillarMat = new THREE.MeshStandardMaterial({ color: 0x636e72, roughness: 0.6 })
    const mesh = new THREE.Mesh(
      new THREE.CylinderGeometry(size[0] / 2, size[0] / 2, size[1], 8),
      pillarMat,
    )
    mesh.castShadow = true
    return mesh
  }

  // Pendulum pillar -> gray cylinder
  if (classification.type === 'pendulum_pillar') {
    const pillarMat = new THREE.MeshStandardMaterial({ color: 0x636e72, roughness: 0.6 })
    const mesh = new THREE.Mesh(
      new THREE.CylinderGeometry(size[0] / 2, size[2] / 2, size[1], 12),
      pillarMat,
    )
    mesh.castShadow = true
    return mesh
  }

  // Pendulum bar -> gray box
  if (classification.type === 'pendulum_bar') {
    const pillarMat = new THREE.MeshStandardMaterial({ color: 0x636e72, roughness: 0.6 })
    const mesh = new THREE.Mesh(
      new THREE.BoxGeometry(size[0], size[1], size[2]),
      pillarMat,
    )
    return mesh
  }

  // Finish arch -> gold material
  if (classification.type === 'arch') {
    const archMat = new THREE.MeshStandardMaterial({ color: 0xffd700, roughness: 0.3, metalness: 0.5 })
    let geo: THREE.BufferGeometry
    if (primitive === 'cylinder') {
      geo = new THREE.CylinderGeometry(size[0] / 2, size[0] / 2, size[1], 16)
    } else {
      geo = new THREE.BoxGeometry(size[0], size[1], size[2])
    }
    const mesh = new THREE.Mesh(geo, archMat)
    mesh.castShadow = true
    return mesh
  }

  // Default: use primitive type and color from entity
  const mat = new THREE.MeshStandardMaterial({ color, roughness: 0.5 })
  let geo: THREE.BufferGeometry
  if (primitive === 'cylinder') {
    geo = new THREE.CylinderGeometry(size[0] / 2, size[0] / 2, size[1], 32)
  } else if (primitive === 'sphere') {
    geo = new THREE.SphereGeometry(size[0] / 2, 16, 16)
  } else {
    geo = new THREE.BoxGeometry(size[0], size[1], size[2])
  }
  const mesh = new THREE.Mesh(geo, mat)
  mesh.receiveShadow = true
  mesh.castShadow = true
  return mesh
}

// ── Scene update ────────────────────────────────────────────
function updateScene(obs: SpectatorObservation, dt: number): void {
  const activeIds = new Set<number>()
  const stateByRootPartId = new Map<number, string>()
  for (const player of obs.players) {
    if (typeof player.root_part_id === 'number' && typeof player.humanoid_state === 'string') {
      stateByRootPartId.set(player.root_part_id, player.humanoid_state)
    }
  }

  for (const entity of obs.entities) {
    activeIds.add(entity.id)
    let obj = entityObjects.get(entity.id)

    if (!obj) {
      obj = createEntityVisual(entity)
      entityObjects.set(entity.id, obj)
      scene.add(obj)
    }

    // Position
    obj.position.set(entity.position[0], entity.position[1], entity.position[2])

    // Rotation
    if (entity.rotation) {
      obj.quaternion.copy(rotationToQuaternion(entity.rotation))
    }

    // Visibility / transparency
    obj.visible = entity.render.visible
    if (entity.render.transparency !== undefined && entity.render.transparency > 0) {
      obj.traverse((child) => {
        const mesh = child as THREE.Mesh
        if (!mesh.isMesh) return
        const material = mesh.material as THREE.MeshStandardMaterial
        if (material.isMeshStandardMaterial) {
          material.transparent = entity.render.transparency! > 0
          material.opacity = 1 - (entity.render.transparency ?? 0)
        }
      })
    }

    // Classification for per-type logic
    const classification = classifyEntity(entity)
    const isBotOrModel = classification.type === 'bot' || !!obj.userData.isModelEntity

    // Estimate speed from position change (used for jellybeans and model animations)
    const prevPos = obj.userData.prevPosition as [number, number, number] | undefined
    let speed = 0
    if (prevPos) {
      const dx = entity.position[0] - prevPos[0]
      const dz = entity.position[2] - prevPos[2]
      speed = Math.sqrt(dx * dx + dz * dz) / Math.max(dt, 0.001)
    }
    obj.userData.prevPosition = [...entity.position]

    // Face direction of movement — only for bots (model entities use server rotation)
    if (classification.type === 'bot' && prevPos && speed > 0.5) {
      const dx = entity.position[0] - prevPos[0]
      const dz = entity.position[2] - prevPos[2]
      obj.rotation.y = Math.atan2(dx, dz)
    }

    // Animate jellybeans (bots)
    if (classification.type === 'bot' && obj.userData.isJellybean) {
      animateJellybean(obj as THREE.Group, speed, true, dt)
    }

    // Animate GLB model entities (walk/idle blend)
    const animState = modelAnimStates.get(entity.id)
    if (animState) {
      const humanoidState = stateByRootPartId.get(entity.id)
      const airborneState = humanoidState === 'Jumping' || humanoidState === 'Freefall'
      const isMoving = speed > 0.5 && !airborneState
      if (animState.walkAction) {
        const targetWeight = isMoving ? 1 : 0
        const current = animState.walkAction.getEffectiveWeight()
        animState.walkAction.setEffectiveWeight(current + (targetWeight - current) * Math.min(1, dt * 8))
        if (isMoving) {
          animState.walkAction.paused = false
          animState.walkAction.setEffectiveTimeScale(Math.min(speed / 6, 2))
        }
      }
      if (animState.idleAction) {
        const targetWeight = isMoving ? 0 : 1
        const current = animState.idleAction.getEffectiveWeight()
        animState.idleAction.setEffectiveWeight(current + (targetWeight - current) * Math.min(1, dt * 8))
      }
      animState.mixer.update(dt)
    }
  }

  // Remove old entities
  for (const [id, obj] of entityObjects) {
    if (!activeIds.has(id)) {
      scene.remove(obj)
      obj.traverse((child) => {
        const mesh = child as THREE.Mesh
        if (!mesh.isMesh) return
        mesh.geometry.dispose()
        const material = mesh.material
        if (Array.isArray(material)) material.forEach((m) => m.dispose())
        else (material as THREE.Material).dispose()
      })
      entityObjects.delete(id)
      modelAnimStates.delete(id)
    }
  }
}

// ── Player follow target tracking ────────────────────────────
// Players are rendered via the entity system (their HumanoidRootPart has model_url).
// This function only handles selecting a follow target and updating the camera position.
function updatePlayers(obs: SpectatorObservation): void {
  for (const player of obs.players) {
    if (!selectedPlayerId) {
      selectedPlayerId = player.id
    }
    if (player.id === selectedPlayerId) {
      followTargetPos.set(player.position[0], player.position[1], player.position[2])
    }
  }
}

// ── HUD ─────────────────────────────────────────────────────
function getPlaceSuffix(n: number): string {
  if (n === 1) return 'st'
  if (n === 2) return 'nd'
  if (n === 3) return 'rd'
  return 'th'
}

function numberAttr(attrs: Record<string, unknown> | undefined, key: string): number | null {
  if (!attrs) return null
  const v = attrs[key]
  if (typeof v === 'number' && Number.isFinite(v)) return v
  return null
}

function stringAttr(attrs: Record<string, unknown> | undefined, key: string): string | null {
  if (!attrs) return null
  const v = attrs[key]
  if (typeof v === 'string' && v.length > 0) return v
  return null
}

function updateHud(obs: SpectatorObservation): void {
  const target = obs.players.find((p) => p.id === selectedPlayerId) ?? obs.players[0]
  if (!target) return

  const attrs = target.attributes

  // Timer
  const timer = stringAttr(attrs, 'Timer')
  if (timer) timerEl.textContent = timer

  // Position
  const place = numberAttr(attrs, 'Place')
  if (place !== null) {
    positionEl.textContent = `${place}${getPlaceSuffix(place)} / 7`
  }

  // Progress
  const progress = numberAttr(attrs, 'Progress')
  if (progress !== null) {
    progressFill.style.width = `${progress}%`
  }

  // Game state
  const gameState = stringAttr(attrs, 'GameState')
  if (gameState === 'finished' && !finishTriggered) {
    finishTriggered = true
    const finishTime = stringAttr(attrs, 'FinishTime')
    finishPlaceEl.textContent = `${place}${getPlaceSuffix(place ?? 1)} Place`
    finishTimeEl.textContent = finishTime ?? ''
    finishScreen.style.display = 'flex'

    // Confetti
    spawnConfetti(scene, followTargetPos.clone().add(new THREE.Vector3(0, 3, 0)))
    setTimeout(() => spawnConfetti(scene, followTargetPos.clone().add(new THREE.Vector3(2, 4, 1)), 100), 300)
    setTimeout(() => spawnConfetti(scene, followTargetPos.clone().add(new THREE.Vector3(-2, 3, -1)), 100), 600)
  }

  // Status text
  if (currentMode === 'play') {
    const playingSelf = playPlayerId !== null && target.id === playPlayerId
    if (playingSelf) {
      spectateTextEl.textContent = `[${modeLabel()}] You (${target.name}) | tick ${obs.tick} | ${obs.game_status} | WASD move, Space jump, Shift dive, Esc spectate`
    } else {
      spectateTextEl.textContent = `[${modeLabel()}] Following ${target.name} | tick ${obs.tick} | ${obs.game_status} | Esc spectate`
    }
  } else {
    spectateTextEl.textContent = `[${modeLabel()}] Following ${target.name} | tick ${obs.tick} | ${obs.game_status} | Enter play, Tab cycle`
  }
}

// ── Camera ──────────────────────────────────────────────────
const CAM_DIST = 25
const CAM_HEIGHT = 12
const CAM_LAG = 0.05

function updateCamera(): void {
  const tp = followTargetPos

  let ideal: THREE.Vector3
  if (finishTriggered) {
    const t = performance.now() * 0.001
    ideal = new THREE.Vector3(
      tp.x + Math.sin(t * 0.5) * 30,
      tp.y + 15,
      tp.z + Math.cos(t * 0.5) * 30,
    )
    camPos.lerp(ideal, 0.07)
  } else {
    ideal = new THREE.Vector3(tp.x * 0.3, tp.y + CAM_HEIGHT, tp.z - CAM_DIST)
    camPos.lerp(ideal, CAM_LAG)
  }

  camera.position.copy(camPos)
  camTarget.lerp(tp.clone().add(new THREE.Vector3(0, 4, 6)), 0.08)
  camera.lookAt(camTarget)

  // Move directional light to follow player
  dirLight.position.set(tp.x + 80, 120, tp.z + 60)
  dirLight.target.position.copy(tp)
}

// ── WebSocket ───────────────────────────────────────────────
let latestObservation: SpectatorObservation | null = null
type ClientMode = 'spectator' | 'play'
let currentMode: ClientMode = 'spectator'

function getGameId(): string {
  const fromPath = window.location.pathname.match(/\/spectate\/([0-9a-fA-F-]{36})/)
  if (fromPath?.[1]) return fromPath[1]
  const fromQuery = new URLSearchParams(window.location.search).get('game')
  if (fromQuery) {
    const uuidPattern = /[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}/
    const extracted = fromQuery.match(uuidPattern)?.[0]
    if (extracted) return extracted
  }
  throw new Error('Missing game id. Use /spectate/<game_id> or ?game=<game_id>')
}

const gameId = getGameId()

let physicsTicks = 0
let physicsLast = performance.now()
let lastObsTick = -1

function handleObservation(obs: SpectatorObservation): void {
  latestObservation = obs
  pushSnapshot(obs)
  updateHud(obs)

  if (obs.tick !== lastObsTick) {
    lastObsTick = obs.tick
    physicsTicks++
  }
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
        setConnectionState(`Error: ${(parsed as { error: string }).error}`)
        return
      }

      handleObservation(parsed as SpectatorObservation)
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

function modeLabel(): string {
  return currentMode === 'play' ? 'Play' : 'Spectator'
}

function setMode(mode: ClientMode): void {
  currentMode = mode
}

// Tab to switch follow target, Enter to join as player, Esc to return to spectator mode
window.addEventListener('keydown', (event) => {
  if (event.code === 'Escape' && playJoined) {
    event.preventDefault()
    leavePlayMode()
    return
  }

  if (event.code === 'Enter' && !playJoined && !playConnecting) {
    event.preventDefault()
    void joinAsPlayer()
    return
  }

  if (!latestObservation) return
  if (event.code !== 'Tab') return
  if (currentMode === 'play') return
  event.preventDefault()
  const players = latestObservation.players
  if (!players.length) return
  const i = players.findIndex((p) => p.id === selectedPlayerId)
  selectedPlayerId = players[(i + 1) % players.length].id
  finishTriggered = false
  finishScreen.style.display = 'none'
})

connectWs()

// ── Keyboard play mode ──────────────────────────────────────
const keysDown = new Set<string>()
let playJoined = false
let playConnecting = false
let playSocket: WebSocket | null = null
let playSeq = 0
let playPlayerId: string | null = null
let jumpQueued = false
let diveQueued = false
let lastInputTime = 0
let playSocketClosingByUser = false
const INPUT_INTERVAL_MS = 50

window.addEventListener('keydown', (e) => {
  if (!e.repeat && e.code === 'Space') jumpQueued = true
  if (!e.repeat && (e.code === 'ShiftLeft' || e.code === 'ShiftRight')) diveQueued = true
  keysDown.add(e.code)
})
window.addEventListener('keyup', (e) => { keysDown.delete(e.code) })
window.addEventListener('blur', () => {
  keysDown.clear()
  jumpQueued = false
  diveQueued = false
})

interface PlayServerMessage {
  type: 'joined' | 'ack' | 'error'
  player_id?: string
  instance_id?: string
  seq?: number
  message?: string
}

function leavePlayMode(): void {
  playSocketClosingByUser = true
  playJoined = false
  playConnecting = false
  playPlayerId = null
  jumpQueued = false
  diveQueued = false
  keysDown.clear()
  setMode('spectator')
  if (playSocket && playSocket.readyState === WebSocket.OPEN) {
    playSocket.close(1000, 'switch to spectator')
  } else {
    playSocket = null
  }
  setConnectionState('Spectator mode (Enter to play)')
}

async function joinAsPlayer(): Promise<void> {
  if (playJoined || playConnecting) return
  playConnecting = true
  try {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const wsUrl = `${protocol}//${window.location.host}/api/v1/games/${gameId}/play/ws?name=keyboard-player`
    const ws = new WebSocket(wsUrl)
    playSocket = ws
    setConnectionState('Connecting play socket...')

    ws.onmessage = (event) => {
      try {
        const raw = event.data instanceof ArrayBuffer
          ? new TextDecoder().decode(new Uint8Array(event.data))
          : String(event.data)
        const parsed = JSON.parse(raw) as PlayServerMessage

        if (parsed.type === 'joined' && parsed.player_id) {
          playJoined = true
          playConnecting = false
          playPlayerId = parsed.player_id
          setMode('play')
          selectedPlayerId = parsed.player_id
          finishTriggered = false
          finishScreen.style.display = 'none'
          console.log('[play] Joined as keyboard player')
          return
        }

        if (parsed.type === 'error') {
          playJoined = false
          playConnecting = false
          playPlayerId = null
          setMode('spectator')
          setConnectionState(parsed.message ? `Play error: ${parsed.message}` : 'Play error')
        }
      } catch {
        // Keep socket alive and ignore malformed server messages.
      }
    }

    ws.onerror = () => {
      playConnecting = false
      playJoined = false
      playPlayerId = null
      setMode('spectator')
      setConnectionState('Play socket error')
    }

    ws.onclose = () => {
      const wasJoined = playJoined
      const closedByUser = playSocketClosingByUser
      playSocketClosingByUser = false
      playConnecting = false
      playJoined = false
      playPlayerId = null
      playSocket = null
      if (closedByUser) {
        setConnectionState('Spectator mode (Enter to play)')
      } else if (wasJoined) {
        setMode('spectator')
        setConnectionState('Play disconnected (Enter to rejoin)')
      }
    }
  } catch (err) {
    playConnecting = false
    playJoined = false
    playPlayerId = null
    console.error('[play] Failed to join:', err)
  }
}

function sendPlayIntent(moveX: number, moveZ: number, jump: boolean, dive: boolean): void {
  const ws = playSocket
  if (!ws || ws.readyState !== WebSocket.OPEN || !playJoined) return

  // Three.js yaw=0 faces -Z; backend intent transform expects yaw=0 to face +Z.
  const yaw = new THREE.Euler().setFromQuaternion(camera.quaternion, 'YXZ').y + Math.PI
  const payload = {
    type: 'intent',
    seq: ++playSeq,
    move: [moveX, moveZ] as [number, number],
    buttons: { jump, dive },
    camera_yaw: yaw,
  }

  try {
    ws.send(JSON.stringify(payload))
  } catch { /* ignore */ }
}

function processKeyboardInput(): void {
  if (currentMode !== 'play' || !playJoined || !playSocket) return

  const now = performance.now()
  if (now - lastInputTime < INPUT_INTERVAL_MS) return
  lastInputTime = now

  // Camera-relative movement intent (Roblox parity style).
  let moveX = 0
  let moveZ = 0
  if (keysDown.has('KeyW') || keysDown.has('ArrowUp')) moveZ += 1
  if (keysDown.has('KeyS') || keysDown.has('ArrowDown')) moveZ -= 1
  if (keysDown.has('KeyA') || keysDown.has('ArrowLeft')) moveX -= 1
  if (keysDown.has('KeyD') || keysDown.has('ArrowRight')) moveX += 1

  const len = Math.hypot(moveX, moveZ)
  if (len > 1) {
    moveX /= len
    moveZ /= len
  }

  // Backend/world transform uses opposite strafe sign from our camera-space axis.
  sendPlayIntent(-moveX, moveZ, jumpQueued, diveQueued)
  jumpQueued = false
  diveQueued = false
}

// ── Main loop ───────────────────────────────────────────────
const clock = new THREE.Clock()
const fpsEl = document.getElementById('fps-counter') as HTMLDivElement
let fpsFrames = 0
let fpsLast = performance.now()

function frame(): void {
  requestAnimationFrame(frame)
  const dt = Math.min(clock.getDelta(), 0.05)

  fpsFrames++
  const now = performance.now()
  if (now - fpsLast >= 500) {
    const elapsed = (now - fpsLast) / 1000
    const renderFps = Math.round(fpsFrames / elapsed)
    const physicsFps = Math.round(physicsTicks / elapsed)
    fpsEl.textContent = `render ${renderFps} | physics ${physicsFps}`
    fpsFrames = 0
    physicsTicks = 0
    fpsLast = now
    physicsLast = now
  }
  const time = performance.now() * 0.001

  // Update environment
  waterMat.uniforms.time.value = time
  updateClouds(clouds, dt)
  updateDecorations(decorations, time)
  updateParticles(scene, dt)

  // Update from interpolated observation
  const interpObs = interpolatedObservation()
  if (interpObs) {
    updateScene(interpObs, dt)
    updatePlayers(interpObs)
  }

  processKeyboardInput()
  updateCamera()
  composer.render()
}

frame()

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
  active_animations?: Array<{
    animation_id: string
    time_position: number
    speed: number
    looped: boolean
    is_playing: boolean
  }>
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

const CAMERA_SMOOTHING = 8
const FOLLOW_DISTANCE = 10
const FOLLOW_HEIGHT = 4.5
const MIN_CAMERA_DISTANCE = 5
const followRaycaster = new THREE.Raycaster()
const followRayDirection = new THREE.Vector3()
let lastFollowPlayerPos: THREE.Vector3 | null = null
const lastFollowForward = new THREE.Vector3(0, 0, -1)
let followWeapon: THREE.Group | null = null
let followWeaponSlot: number | null = null
let followWeaponKick = 0
let followWeaponMuzzleTime = 0
let followWeaponMuzzleLight: THREE.PointLight | null = null
const lastAmmoByPlayer = new Map<string, number>()

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
const modelTemplateCache = new Map<string, Promise<{ scene: THREE.Object3D, animations: THREE.AnimationClip[] }>>()
const modelEntityStates = new Map<number, {
  mixer: THREE.AnimationMixer
  walkAction?: THREE.AnimationAction
  idleAction?: THREE.AnimationAction
  lastPos: THREE.Vector3 | null
  moveSpeed: number
  targetYaw: number
  currentYaw: number
  modelRoot: THREE.Object3D
}>()
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

function buildWeaponModel(slot: number): THREE.Group {
  const group = new THREE.Group()
  const addBox = (
    size: [number, number, number],
    pos: [number, number, number],
    color: number,
    roughness: number,
    metalness: number,
    rot?: [number, number, number],
  ) => {
    const mesh = new THREE.Mesh(
      new THREE.BoxGeometry(size[0], size[1], size[2]),
      new THREE.MeshStandardMaterial({ color, roughness, metalness }),
    )
    mesh.position.set(pos[0], pos[1], pos[2])
    if (rot) mesh.rotation.set(rot[0], rot[1], rot[2])
    mesh.castShadow = true
    mesh.receiveShadow = true
    group.add(mesh)
  }
  const addBarrel = (len: number, pos: [number, number, number]) => {
    const barrel = new THREE.Mesh(
      new THREE.CylinderGeometry(0.045, 0.045, len, 10),
      new THREE.MeshStandardMaterial({ color: 0x222222, roughness: 0.2, metalness: 0.9 }),
    )
    barrel.rotation.x = Math.PI / 2
    barrel.position.set(pos[0], pos[1], pos[2])
    barrel.castShadow = true
    barrel.receiveShadow = true
    group.add(barrel)
  }
  const muzzleFlash = new THREE.PointLight(0xffaa44, 0, 8)
  muzzleFlash.position.set(0, 0.0, -1.05)
  group.add(muzzleFlash)
  group.userData.muzzleFlash = muzzleFlash

  if (slot === 1) {
    addBox([0.35, 0.18, 0.62], [0, -0.05, -0.42], 0xcccccc, 0.3, 0.8)
    addBarrel(0.26, [0, 0.0, -0.68])
    addBox([0.12, 0.28, 0.12], [0, -0.25, -0.20], 0xcccccc, 0.3, 0.8, [0.21, 0, 0])
    return group
  }

  if (slot === 3) {
    addBox([0.34, 0.14, 1.1], [0, -0.04, -0.50], 0x8b4513, 0.5, 0.3)
    addBarrel(0.52, [0, 0.01, -0.98])
    addBox([0.12, 0.26, 0.12], [0, -0.23, -0.26], 0x8b4513, 0.5, 0.3, [0.18, 0, 0])
    addBox([0.14, 0.12, 0.34], [0, -0.10, -0.72], 0x654321, 0.5, 0.3)
    return group
  }

  addBox([0.34, 0.16, 1.05], [0, -0.04, -0.50], 0x444444, 0.3, 0.8)
  addBarrel(0.46, [0, 0.01, -0.92])
  addBox([0.12, 0.26, 0.12], [0, -0.24, -0.28], 0x444444, 0.3, 0.8, [0.18, 0, 0])
  addBox([0.10, 0.22, 0.14], [0, -0.22, -0.50], 0x333333, 0.4, 0.7)
  addBox([0.16, 0.12, 0.30], [0, -0.02, 0.20], 0x444444, 0.3, 0.8)
  return group
}

function syncFollowWeapon(playerPos: THREE.Vector3, forward: THREE.Vector3, attrs: Record<string, unknown> | undefined): void {
  const slotValue = numberAttr(attrs, ['WeaponSlot', 'weapon_slot', 'WeaponIndex'])
  const slot = slotValue ? Math.max(1, Math.min(3, Math.round(slotValue))) : 2
  if (!followWeapon || followWeaponSlot !== slot) {
    if (followWeapon) {
      scene.remove(followWeapon)
      disposeObject(followWeapon)
    }
    followWeapon = buildWeaponModel(slot)
    followWeaponSlot = slot
    followWeaponMuzzleLight = (followWeapon.userData.muzzleFlash as THREE.PointLight | undefined) ?? null
    scene.add(followWeapon)
  }

  const right = new THREE.Vector3().crossVectors(forward, new THREE.Vector3(0, 1, 0)).normalize()
  const weaponPos = playerPos.clone()
    .addScaledVector(forward, 0.75 - 0.28 * followWeaponKick)
    .addScaledVector(right, 1.05)
    .add(new THREE.Vector3(0, -0.25, 0))
  followWeapon.position.copy(weaponPos)

  const lookTarget = weaponPos.clone().addScaledVector(forward, 5)
  followWeapon.lookAt(lookTarget)
  if (followWeaponMuzzleLight) {
    followWeaponMuzzleLight.intensity = followWeaponMuzzleTime > 0 ? 3 : 0
  }
}

function triggerWeaponFireVisual(): void {
  followWeaponKick = 1
  followWeaponMuzzleTime = 0.05
}

function tickWeaponVisual(dt: number): void {
  if (followWeaponKick > 0) {
    followWeaponKick = Math.max(0, followWeaponKick - dt * 10)
  }
  if (followWeaponMuzzleTime > 0) {
    followWeaponMuzzleTime = Math.max(0, followWeaponMuzzleTime - dt)
  }
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

function loadModelTemplate(url: string): Promise<{ scene: THREE.Object3D, animations: THREE.AnimationClip[] }> {
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

function findClip(animations: THREE.AnimationClip[], pattern: RegExp): THREE.AnimationClip | undefined {
  return animations.find((clip) => pattern.test(clip.name))
}

function fitModelToSize(model: THREE.Object3D, size: [number, number, number]): void {
  let mesh: THREE.Mesh | THREE.SkinnedMesh | null = null
  model.traverse((obj) => {
    if (mesh) return
    const skinned = obj as THREE.SkinnedMesh
    if (skinned.isSkinnedMesh) {
      mesh = skinned
      return
    }
    const regular = obj as THREE.Mesh
    if (regular.isMesh) {
      mesh = regular
    }
  })

  const sourceBox = new THREE.Box3()
  if (mesh) {
    mesh.geometry.computeBoundingBox()
    if (!mesh.geometry.boundingBox) return
    sourceBox.copy(mesh.geometry.boundingBox)
  } else {
    sourceBox.setFromObject(model)
  }

  const source = sourceBox.getSize(new THREE.Vector3())
  if (source.y <= 0.0001) return

  const center = sourceBox.getCenter(new THREE.Vector3())
  const targetHeight = Math.max(size[1], 0.001)
  const scale = targetHeight / source.y
  model.scale.setScalar(scale)

  model.position.set(
    -center.x * scale,
    -center.y * scale,
    -center.z * scale,
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
    const loaded = await loadModelTemplate(modelUrl)
    const entityId = root.userData.entityId as number | undefined
    if (typeof entityId !== 'number' || !entityObjects.has(entityId)) {
      return
    }

    for (const child of [...root.children]) {
      root.remove(child)
      disposeObject(child)
    }
    const clone = cloneSkeleton(loaded.scene)
    fitModelToSize(clone, size)
    setShadowFlags(clone, render.casts_shadow, render.receives_shadow)
    root.add(clone)

    if (loaded.animations.length > 0) {
      const mixer = new THREE.AnimationMixer(clone)
      const walkClip = findClip(loaded.animations, /(walk|run|jog|locomotion)/i) ?? loaded.animations[0]
      const idleClip = findClip(loaded.animations, /idle/i)
      const walkAction = walkClip ? mixer.clipAction(walkClip) : undefined
      const idleAction = idleClip ? mixer.clipAction(idleClip) : undefined

      if (walkAction) {
        walkAction.reset()
        walkAction.play()
        walkAction.paused = true
        walkAction.time = 0
      }
      if (idleAction && idleAction !== walkAction) {
        idleAction.reset()
        idleAction.play()
      }

      modelEntityStates.set(entityId, {
        mixer,
        walkAction,
        idleAction,
        lastPos: null,
        moveSpeed: 0,
        targetYaw: 0,
        currentYaw: 0,
        modelRoot: clone,
      })
    }
  } catch (error) {
    console.warn('Failed to load model for entity', modelUrl, error)
  }
}

function updateModelAnimations(dt: number): void {
  if (dt <= 0) return
  for (const [entityId, state] of modelEntityStates) {
    const obj = entityObjects.get(entityId)
    if (!obj) continue

    const current = obj.position.clone()
    if (state.lastPos) {
      const dx = current.x - state.lastPos.x
      const dz = current.z - state.lastPos.z
      const speed = Math.sqrt(dx * dx + dz * dz) / dt
      const lerpFactor = speed < state.moveSpeed ? 0.5 : 0.2
      state.moveSpeed = THREE.MathUtils.lerp(state.moveSpeed, speed, lerpFactor)
      if (Math.abs(dx) > 0.01 || Math.abs(dz) > 0.01) {
        state.targetYaw = Math.atan2(dx, dz)
      }
    }
    state.lastPos = current

    const rotDiff = state.targetYaw - state.currentYaw
    const shortest = Math.atan2(Math.sin(rotDiff), Math.cos(rotDiff))
    state.currentYaw += shortest * Math.min(1, dt * 10)
    state.modelRoot.rotation.y = state.currentYaw

    const isMoving = state.moveSpeed > 0.5
    if (state.walkAction) {
      if (isMoving) {
        state.walkAction.paused = false
        state.walkAction.timeScale = THREE.MathUtils.clamp(state.moveSpeed / 5, 0.7, 1.6)
      } else {
        state.walkAction.paused = true
        state.walkAction.time = 0
      }
    }

    if (state.idleAction && state.idleAction !== state.walkAction) {
      state.idleAction.paused = false
      state.idleAction.weight = isMoving ? 0 : 1
    }

    state.mixer.update(dt)
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
      const modelState = modelEntityStates.get(id)
      if (modelState) {
        modelState.walkAction?.stop()
        modelState.idleAction?.stop()
        modelState.mixer.stopAllAction()
        modelEntityStates.delete(id)
      }
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

  if (ammoMag !== null) {
    const prevAmmo = lastAmmoByPlayer.get(target.id)
    if (prevAmmo !== undefined && ammoMag < prevAmmo) {
      triggerWeaponFireVisual()
    }
    lastAmmoByPlayer.set(target.id, ammoMag)
  }
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
  if (!target) {
    if (followWeapon) followWeapon.visible = false
    return
  }

  const root = target.root_part_id ? obs.entities.find((e) => e.id === target.root_part_id) : null
  const playerPos = new THREE.Vector3(target.position[0], target.position[1] + 2, target.position[2])
  const forward = new THREE.Vector3(0, 0, -1)

  if (root?.rotation) {
    forward.applyQuaternion(rotationToQuaternion(root.rotation))
    forward.y = 0
  } else if (lastFollowPlayerPos) {
    const movement = playerPos.clone().sub(lastFollowPlayerPos)
    movement.y = 0
    if (movement.lengthSq() > 0.0004) {
      forward.copy(movement.normalize())
    } else {
      forward.copy(lastFollowForward)
    }
  } else {
    forward.copy(lastFollowForward)
  }
  if (forward.lengthSq() < 0.0001) forward.set(0, 0, -1)
  forward.normalize()
  lastFollowForward.copy(forward)
  lastFollowPlayerPos = playerPos.clone()

  syncFollowWeapon(playerPos, forward, target.attributes)
  if (followWeapon) followWeapon.visible = true

  const desiredPos = playerPos
    .clone()
    .addScaledVector(forward, -FOLLOW_DISTANCE)
    .add(new THREE.Vector3(0, FOLLOW_HEIGHT, 0))

  followRayDirection.copy(desiredPos).sub(playerPos).normalize()
  const desiredDistance = desiredPos.distanceTo(playerPos)
  followRaycaster.set(playerPos, followRayDirection)
  followRaycaster.far = desiredDistance

  const hits = followRaycaster.intersectObjects(scene.children, true)
  const validHits = hits.filter((hit) => {
    const obj = hit.object as THREE.Object3D
    return (obj as THREE.Mesh).isMesh && obj.visible
  })

  const cameraTargetPos = desiredPos.clone()
  if (validHits.length > 0 && validHits[0].distance < desiredDistance) {
    const safeDistance = Math.max(validHits[0].distance - 1, MIN_CAMERA_DISTANCE)
    cameraTargetPos.copy(playerPos).addScaledVector(followRayDirection, safeDistance)
  }

  const alpha = 1 - Math.exp(-CAMERA_SMOOTHING * dt)
  camera.position.lerp(cameraTargetPos, alpha)
  const lookAt = playerPos.clone().addScaledVector(forward, 12)
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
    updateModelAnimations(dt)
  }
  tickWeaponVisual(dt)

  const t = Date.now() * 0.001
  accentLights.forEach((l, i) => {
    l.intensity = 1.5 + Math.sin(t + i * 0.7) * 0.8
  })

  composer.render()
}

frame()

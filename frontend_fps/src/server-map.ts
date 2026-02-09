import * as pako from 'pako'
import * as THREE from 'three'
import { geometryFromRender, materialFromRender, type RenderSpec } from './render/presets'

interface SpectatorEntity {
  id: number
  type: string
  position: [number, number, number]
  rotation?: [[number, number, number], [number, number, number], [number, number, number]]
  size: [number, number, number]
  render: RenderSpec
}

interface SpectatorObservation {
  tick: number
  entities: SpectatorEntity[]
}

const canvas = document.getElementById('game') as HTMLCanvasElement
const hud = document.getElementById('hud') as HTMLDivElement

function getGameId(): string {
  const fromPath = window.location.pathname.match(/\/spectate\/([0-9a-fA-F-]{36})/)
  if (fromPath?.[1]) return fromPath[1]
  const fromQuery = new URLSearchParams(window.location.search).get('game')
  if (fromQuery) return fromQuery
  throw new Error('Missing game id. Use server-map.html?game=<uuid>')
}

const gameId = getGameId()
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

const entityObjects = new Map<number, THREE.Mesh>()
let latestEntities: SpectatorEntity[] = []
let latestTick = 0
let wsState = 'connecting'
let lastDebugLogAt = 0

function rotationToQuaternion(rot: [[number, number, number], [number, number, number], [number, number, number]]): THREE.Quaternion {
  const m = new THREE.Matrix4()
  m.set(rot[0][0], rot[0][1], rot[0][2], 0, rot[1][0], rot[1][1], rot[1][2], 0, rot[2][0], rot[2][1], rot[2][2], 0, 0, 0, 0, 1)
  return new THREE.Quaternion().setFromRotationMatrix(m)
}

function syncEntities(obs: SpectatorObservation): void {
  latestTick = obs.tick
  latestEntities = obs.entities
  const seen = new Set<number>()

  for (const e of obs.entities) {
    seen.add(e.id)
    let mesh = entityObjects.get(e.id)
    if (!mesh) {
      mesh = new THREE.Mesh(geometryFromRender(e.render, e.size), materialFromRender(e.render))
      mesh.castShadow = e.render.casts_shadow
      mesh.receiveShadow = e.render.receives_shadow
      scene.add(mesh)
      entityObjects.set(e.id, mesh)
    }

    mesh.position.set(e.position[0], e.position[1], e.position[2])
    if (e.rotation) mesh.quaternion.copy(rotationToQuaternion(e.rotation))
    mesh.visible = e.render.visible
  }

  for (const [id, mesh] of entityObjects) {
    if (seen.has(id)) continue
    scene.remove(mesh)
    mesh.geometry.dispose()
    mesh.material.dispose()
    entityObjects.delete(id)
  }
}

function summarizeEntities(entities: SpectatorEntity[]): string {
  const byPrimitive = new Map<string, number>()
  const byMaterial = new Map<string, number>()
  const byPreset = new Map<string, number>()
  let minX = Number.POSITIVE_INFINITY
  let minY = Number.POSITIVE_INFINITY
  let minZ = Number.POSITIVE_INFINITY
  let maxX = Number.NEGATIVE_INFINITY
  let maxY = Number.NEGATIVE_INFINITY
  let maxZ = Number.NEGATIVE_INFINITY

  for (const e of entities) {
    const primitive = (e.render.primitive ?? 'box').toString()
    const mat = (e.render.material ?? 'Default').toString()
    const preset = (e.render.preset_id ?? 'none').toString()
    byPrimitive.set(primitive, (byPrimitive.get(primitive) ?? 0) + 1)
    byMaterial.set(mat, (byMaterial.get(mat) ?? 0) + 1)
    byPreset.set(preset, (byPreset.get(preset) ?? 0) + 1)
    minX = Math.min(minX, e.position[0]); maxX = Math.max(maxX, e.position[0])
    minY = Math.min(minY, e.position[1]); maxY = Math.max(maxY, e.position[1])
    minZ = Math.min(minZ, e.position[2]); maxZ = Math.max(maxZ, e.position[2])
  }

  const top = (m: Map<string, number>): string =>
    [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6).map(([k, v]) => `${k}:${v}`).join(', ') || 'none'
  const samples = entities
    .slice(0, 8)
    .map((e) => `#${e.id} role=${e.render.role} preset=${e.render.preset_id ?? 'none'} @ (${e.position[0].toFixed(1)},${e.position[1].toFixed(1)},${e.position[2].toFixed(1)})`)
    .join('\n')

  const bounds = entities.length
    ? `bounds=(${minX.toFixed(1)},${minY.toFixed(1)},${minZ.toFixed(1)})..(${maxX.toFixed(1)},${maxY.toFixed(1)},${maxZ.toFixed(1)})`
    : 'bounds=n/a'

  return [
    `count=${entities.length} ${bounds}`,
    `primitive: ${top(byPrimitive)}`,
    `material: ${top(byMaterial)}`,
    `preset: ${top(byPreset)}`,
    `sample:\n${samples || 'none'}`,
  ].join('\n')
}

function connectWs(): void {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const ws = new WebSocket(`${protocol}//${window.location.host}/api/v1/games/${gameId}/spectate/ws`)
  ws.binaryType = 'arraybuffer'
  wsState = 'connecting'

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
        wsState = `error: ${parsed.error}`
        return
      }
      wsState = 'connected'
      syncEntities(parsed)
    } catch {
      wsState = 'parse error'
    }
  }

  ws.onclose = () => {
    wsState = 'disconnected; reconnecting'
    window.setTimeout(connectWs, 1200)
  }
  ws.onerror = () => { wsState = 'ws error' }
}

const moveKeys: Record<string, boolean> = {}
let yaw = Math.PI
let pitch = -0.1

window.addEventListener('click', () => {
  void canvas.requestPointerLock()
})

window.addEventListener('mousemove', (event) => {
  if (document.pointerLockElement !== canvas) return
  yaw -= event.movementX * 0.002
  pitch -= event.movementY * 0.002
  pitch = Math.max(-1.5, Math.min(1.5, pitch))
})

window.addEventListener('keydown', (event) => { moveKeys[event.code] = true })
window.addEventListener('keyup', (event) => { moveKeys[event.code] = false })

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
  const summary = summarizeEntities(latestEntities)
  hud.textContent =
    `Server map viewer\n` +
    `game=${gameId}\n` +
    `ws=${wsState} tick=${latestTick} entities=${entityObjects.size}\n` +
    `camera=(${camera.position.x.toFixed(1)}, ${camera.position.y.toFixed(1)}, ${camera.position.z.toFixed(1)})\n` +
    `click: lock mouse | WASD: move | E/Q: up/down | Shift: boost\n` +
    `\n${summary}`

  const now = performance.now()
  if (now - lastDebugLogAt > 2000) {
    lastDebugLogAt = now
    console.log('[server-map] entity-summary\n' + summary)
  }
}

window.addEventListener('resize', () => {
  camera.aspect = window.innerWidth / window.innerHeight
  camera.updateProjectionMatrix()
  renderer.setSize(window.innerWidth, window.innerHeight)
})

connectWs()

const clock = new THREE.Clock()
function frame(): void {
  requestAnimationFrame(frame)
  const dt = Math.min(clock.getDelta(), 0.05)
  updateCamera(dt)
  const t = performance.now() * 0.001
  accentLights.forEach((l, i) => {
    l.intensity = 1.5 + Math.sin(t * 2 + i * 0.7) * 0.8
  })
  updateHud()
  renderer.render(scene, camera)
}

frame()

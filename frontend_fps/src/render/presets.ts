import * as THREE from 'three'

export interface RenderSpec {
  kind: string
  role: string
  preset_id?: string
  primitive: string
  material: string
  color: [number, number, number]
  static: boolean
  casts_shadow: boolean
  receives_shadow: boolean
  visible: boolean
  double_sided: boolean
  transparency?: number
}

type PresetMaterial = {
  roughness?: number
  metalness?: number
  emissiveIntensity?: number
}

const PRESET_MATERIALS: Record<string, PresetMaterial> = {
  'fps_arena/floor': { roughness: 0.85, metalness: 0.1 },
  'fps_arena/ceiling': { roughness: 0.9, metalness: 0.0 },
  'fps_arena/wall': { roughness: 0.7, metalness: 0.2 },
  'fps_arena/shortwall': { roughness: 0.7, metalness: 0.2 },
  'fps_arena/crate': { roughness: 0.9, metalness: 0.05 },
  'fps_arena/crate_edge': { roughness: 0.4, metalness: 0.6 },
  'fps_arena/pillar': { roughness: 0.3, metalness: 0.8 },
  'fps_arena/platform': { roughness: 0.6, metalness: 0.3 },
  'fps_arena/trim_red': { roughness: 1.0, metalness: 0.0, emissiveIntensity: 0.5 },
  'fps_arena/trim_blue': { roughness: 1.0, metalness: 0.0, emissiveIntensity: 0.5 },
  'fps_arena/spawn': { roughness: 0.2, metalness: 0.25, emissiveIntensity: 0.6 },
}

function createFloorTexture(): THREE.CanvasTexture {
  const canvas = document.createElement('canvas')
  canvas.width = 512
  canvas.height = 512
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('floor texture context unavailable')
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
  const tex = new THREE.CanvasTexture(canvas)
  tex.wrapS = tex.wrapT = THREE.RepeatWrapping
  tex.repeat.set(15, 15)
  return tex
}

function createWallTexture(): THREE.CanvasTexture {
  const canvas = document.createElement('canvas')
  canvas.width = 256
  canvas.height = 256
  const ctx = canvas.getContext('2d')
  if (!ctx) throw new Error('wall texture context unavailable')
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
  const tex = new THREE.CanvasTexture(canvas)
  tex.wrapS = tex.wrapT = THREE.RepeatWrapping
  return tex
}

let floorTexture: THREE.CanvasTexture | null = null
let wallTexture: THREE.CanvasTexture | null = null

export function materialFromRender(render: RenderSpec): THREE.MeshStandardMaterial {
  const color = new THREE.Color(render.color[0], render.color[1], render.color[2])
  const preset = render.preset_id ? PRESET_MATERIALS[render.preset_id] : undefined

  const isNeonLike = render.material === 'Neon' || (preset?.emissiveIntensity ?? 0) > 0
  const mat = new THREE.MeshStandardMaterial({
    color,
    roughness: preset?.roughness ?? defaultRoughness(render.material),
    metalness: preset?.metalness ?? defaultMetalness(render.material),
    emissive: isNeonLike ? color : new THREE.Color(0x000000),
    emissiveIntensity: preset?.emissiveIntensity ?? (render.material === 'Neon' ? 0.9 : 0),
    transparent: typeof render.transparency === 'number' && render.transparency > 0,
    opacity: typeof render.transparency === 'number' ? Math.max(0, 1 - render.transparency) : 1,
    side: render.double_sided ? THREE.DoubleSide : THREE.FrontSide,
  })

  // Reference parity from fps-opus
  if (render.preset_id === 'fps_arena/floor') {
    if (!floorTexture) floorTexture = createFloorTexture()
    mat.map = floorTexture
    mat.color.setRGB(1, 1, 1)
  }
  if (render.preset_id === 'fps_arena/wall' || render.preset_id === 'fps_arena/shortwall') {
    if (!wallTexture) wallTexture = createWallTexture()
    mat.map = wallTexture
    mat.color.setRGB(1, 1, 1)
  }
  return mat
}

export function geometryFromRender(render: RenderSpec, size: [number, number, number]): THREE.BufferGeometry {
  const s = size
  const sx = Math.max(0.001, s[0])
  const sy = Math.max(0.001, s[1])
  const sz = Math.max(0.001, s[2])
  switch ((render.primitive || 'box').toLowerCase()) {
    case 'ball':
    case 'sphere':
      return new THREE.SphereGeometry(Math.max(0.001, sx * 0.5), 18, 16)
    case 'cylinder': {
      const radius = Math.max(0.001, Math.max(sx, sz) * 0.5)
      return new THREE.CylinderGeometry(radius, radius, sy, 12)
    }
    case 'wedge':
    case 'block':
    case 'box':
    default:
      return new THREE.BoxGeometry(sx, sy, sz)
  }
}

function defaultRoughness(material: string): number {
  switch (material) {
    case 'Metal': return 0.22
    case 'Concrete': return 0.72
    case 'Slate': return 0.82
    case 'Glass': return 0.1
    case 'Wood': return 0.85
    case 'Neon': return 0.2
    default: return 0.58
  }
}

function defaultMetalness(material: string): number {
  switch (material) {
    case 'Metal': return 0.92
    case 'Concrete': return 0.22
    case 'Slate': return 0.12
    case 'Glass': return 0.1
    case 'Wood': return 0.06
    case 'Neon': return 0.35
    default: return 0.25
  }
}

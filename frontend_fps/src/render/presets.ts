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
  'fps_arena/ceiling': { roughness: 0.9, metalness: 0.05 },
  'fps_arena/wall': { roughness: 0.7, metalness: 0.2 },
  'fps_arena/shortwall': { roughness: 0.7, metalness: 0.2 },
  'fps_arena/crate': { roughness: 0.9, metalness: 0.05 },
  'fps_arena/crate_edge': { roughness: 0.3, metalness: 0.8 },
  'fps_arena/pillar': { roughness: 0.25, metalness: 0.92 },
  'fps_arena/platform': { roughness: 0.6, metalness: 0.3 },
  'fps_arena/trim_red': { roughness: 0.18, metalness: 0.35, emissiveIntensity: 0.5 },
  'fps_arena/trim_blue': { roughness: 0.18, metalness: 0.35, emissiveIntensity: 0.5 },
  'fps_arena/spawn': { roughness: 0.2, metalness: 0.25, emissiveIntensity: 0.6 },
}

export function materialFromRender(render: RenderSpec): THREE.MeshStandardMaterial {
  const color = new THREE.Color(render.color[0], render.color[1], render.color[2])
  const preset = render.preset_id ? PRESET_MATERIALS[render.preset_id] : undefined

  const isNeonLike = render.material === 'Neon' || (preset?.emissiveIntensity ?? 0) > 0
  return new THREE.MeshStandardMaterial({
    color,
    roughness: preset?.roughness ?? defaultRoughness(render.material),
    metalness: preset?.metalness ?? defaultMetalness(render.material),
    emissive: isNeonLike ? color : new THREE.Color(0x000000),
    emissiveIntensity: preset?.emissiveIntensity ?? (render.material === 'Neon' ? 0.9 : 0),
    transparent: typeof render.transparency === 'number' && render.transparency > 0,
    opacity: typeof render.transparency === 'number' ? Math.max(0, 1 - render.transparency) : 1,
    side: render.double_sided ? THREE.DoubleSide : THREE.FrontSide,
  })
}

export function geometryFromRender(render: RenderSpec, size: [number, number, number]): THREE.BufferGeometry {
  const s = size
  switch ((render.primitive || 'box').toLowerCase()) {
    case 'ball':
    case 'sphere':
      return new THREE.SphereGeometry(Math.max(0.2, s[0] * 0.5), 20, 16)
    case 'cylinder': {
      const radius = Math.max(0.2, Math.max(s[0], s[2]) * 0.5)
      return new THREE.CylinderGeometry(radius, radius, Math.max(0.2, s[1]), 20)
    }
    case 'wedge':
    case 'block':
    case 'box':
    default:
      return new THREE.BoxGeometry(Math.max(0.2, s[0]), Math.max(0.2, s[1]), Math.max(0.2, s[2]))
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

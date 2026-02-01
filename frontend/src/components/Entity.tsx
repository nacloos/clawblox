import { SpectatorEntity } from '../api'

interface EntityProps {
  entity: SpectatorEntity
}

function getMaterialProps(material?: string, color?: string) {
  const base = { color: color || '#999999' }

  switch (material) {
    case 'Neon':
      return { ...base, emissive: color, emissiveIntensity: 0.8 }
    case 'Metal':
      return { ...base, metalness: 0.9, roughness: 0.2 }
    case 'Glass':
      return { ...base, transparent: true, opacity: 0.4, metalness: 0.1, roughness: 0.1 }
    case 'SmoothPlastic':
      return { ...base, metalness: 0.0, roughness: 0.3 }
    case 'Wood':
      return { ...base, metalness: 0.0, roughness: 0.8 }
    case 'Concrete':
    case 'Brick':
    case 'Slate':
    case 'Granite':
      return { ...base, metalness: 0.0, roughness: 0.95 }
    case 'Ice':
      return { ...base, metalness: 0.1, roughness: 0.1, transparent: true, opacity: 0.8 }
    case 'ForceField':
      return { ...base, transparent: true, opacity: 0.3, emissive: color, emissiveIntensity: 0.5 }
    case 'Grass':
    case 'Sand':
    case 'Fabric':
      return { ...base, metalness: 0.0, roughness: 0.9 }
    case 'Marble':
      return { ...base, metalness: 0.1, roughness: 0.4 }
    case 'Plastic':
    default:
      return { ...base, metalness: 0.0, roughness: 0.7 }
  }
}

export default function Entity({ entity }: EntityProps) {
  const [x, y, z] = entity.position

  if (entity.type === 'part') {
    const size = entity.size || [4, 1, 2]
    const color = entity.color
      ? `rgb(${Math.round(entity.color[0] * 255)}, ${Math.round(entity.color[1] * 255)}, ${Math.round(entity.color[2] * 255)})`
      : '#999999'
    const materialProps = getMaterialProps(entity.material, color)

    return (
      <mesh position={[x, y, z]} castShadow receiveShadow>
        <boxGeometry args={size as [number, number, number]} />
        <meshStandardMaterial {...materialProps} />
      </mesh>
    )
  }

  if (entity.type === 'enemy') {
    const healthRatio = (entity.health ?? 80) / 80
    return (
      <group position={[x, y, z]}>
        <mesh castShadow>
          <boxGeometry args={[1, 1.5, 1]} />
          <meshStandardMaterial color="#e74c3c" />
        </mesh>
        <mesh position={[0, 1.5, 0]}>
          <boxGeometry args={[1 * healthRatio, 0.1, 0.1]} />
          <meshBasicMaterial color="#ef4444" />
        </mesh>
      </group>
    )
  }

  if (entity.type === 'pickup') {
    const isHealth = entity.pickup_type === 'health'
    return (
      <mesh position={[x, y + 0.3 + Math.sin(Date.now() / 500) * 0.1, z]} castShadow>
        <sphereGeometry args={[0.4, 16, 16]} />
        <meshStandardMaterial
          color={isHealth ? '#22c55e' : '#3b82f6'}
          emissive={isHealth ? '#22c55e' : '#3b82f6'}
          emissiveIntensity={0.3}
        />
      </mesh>
    )
  }

  return null
}

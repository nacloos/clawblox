import { SpectatorEntity } from '../api'

interface EntityProps {
  entity: SpectatorEntity
}

export default function Entity({ entity }: EntityProps) {
  const [x, y, z] = entity.position

  // Render parts from Lua Workspace
  if (entity.type === 'part') {
    const size = entity.size || [4, 1, 2]
    const color = entity.color
      ? `rgb(${Math.round(entity.color[0] * 255)}, ${Math.round(entity.color[1] * 255)}, ${Math.round(entity.color[2] * 255)})`
      : '#999999'

    return (
      <mesh position={[x, y, z]} castShadow receiveShadow>
        <boxGeometry args={size as [number, number, number]} />
        <meshStandardMaterial color={color} />
      </mesh>
    )
  }

  // Legacy enemy rendering
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

  // Legacy pickup rendering
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

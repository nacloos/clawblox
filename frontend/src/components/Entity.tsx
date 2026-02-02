import { memo, useRef } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'
import { StateBuffer, EntitySnapshot, interpolatePosition } from '../lib/stateBuffer'

interface EntityProps {
  entityId: number
  stateBuffer: StateBuffer
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

function toColor(colorArray?: [number, number, number]): string {
  if (!colorArray) return '#999999'
  return `rgb(${Math.round(colorArray[0] * 255)}, ${Math.round(colorArray[1] * 255)}, ${Math.round(colorArray[2] * 255)})`
}

// Convert Roblox rotation matrix to Three.js Quaternion
function rotationToQuaternion(rot: [[number, number, number], [number, number, number], [number, number, number]]): THREE.Quaternion {
  const matrix = new THREE.Matrix4()
  matrix.set(
    rot[0][0], rot[0][1], rot[0][2], 0,
    rot[1][0], rot[1][1], rot[1][2], 0,
    rot[2][0], rot[2][1], rot[2][2], 0,
    0, 0, 0, 1
  )
  const quat = new THREE.Quaternion()
  quat.setFromRotationMatrix(matrix)
  return quat
}

// Get interpolated entity position and rotation from buffer
function getInterpolatedEntity(
  stateBuffer: StateBuffer,
  entityId: number
): { entity: EntitySnapshot | null; targetPos: [number, number, number] | null; targetQuat: THREE.Quaternion | null } {
  const result = stateBuffer.getInterpolatedState()

  if (!result) {
    return { entity: null, targetPos: null, targetQuat: null }
  }

  const entityBefore = result.before.entities.get(entityId)
  if (!entityBefore) {
    return { entity: null, targetPos: null, targetQuat: null }
  }

  const entityAfter = result.after?.entities.get(entityId)

  let targetPos: [number, number, number]
  let targetQuat: THREE.Quaternion | null = null

  if (entityAfter && result.alpha > 0) {
    // Interpolate position
    targetPos = interpolatePosition(entityBefore.position, entityAfter.position, result.alpha)

    // Interpolate rotation if both have rotation
    if (entityBefore.rotation && entityAfter.rotation) {
      const quatBefore = rotationToQuaternion(entityBefore.rotation)
      const quatAfter = rotationToQuaternion(entityAfter.rotation)
      targetQuat = quatBefore.clone().slerp(quatAfter, result.alpha)
    } else if (entityBefore.rotation) {
      targetQuat = rotationToQuaternion(entityBefore.rotation)
    }
  } else {
    // Use before snapshot position (hold position when no after data)
    targetPos = entityBefore.position
    if (entityBefore.rotation) {
      targetQuat = rotationToQuaternion(entityBefore.rotation)
    }
  }

  // Use the most recent entity data for static properties
  const entity = entityAfter || entityBefore

  return { entity, targetPos, targetQuat }
}

// Animated pickup component
function PickupEntity({ entityId, stateBuffer }: EntityProps) {
  const meshRef = useRef<THREE.Mesh>(null)

  // Get initial entity to determine pickup type
  const latest = stateBuffer.getLatest()
  const initialEntity = latest?.entities.get(entityId)
  const isHealth = initialEntity?.pickup_type === 'health'
  const baseColor = isHealth ? '#22c55e' : '#3b82f6'

  useFrame(({ clock }) => {
    if (!meshRef.current) return

    const { targetPos } = getInterpolatedEntity(stateBuffer, entityId)
    if (!targetPos) return

    // Render interpolated position directly
    meshRef.current.position.x = targetPos[0]
    meshRef.current.position.z = targetPos[2]
    // Bobbing animation for y
    meshRef.current.position.y = targetPos[1] + 0.3 + Math.sin(clock.getElapsedTime() * 2) * 0.1
  })

  return (
    <mesh ref={meshRef} position={[0, 0, 0]} castShadow>
      <sphereGeometry args={[0.4, 16, 16]} />
      <meshStandardMaterial
        color={baseColor}
        emissive={baseColor}
        emissiveIntensity={0.3}
      />
    </mesh>
  )
}

// Enemy entity with health bar
function EnemyEntity({ entityId, stateBuffer }: EntityProps) {
  const groupRef = useRef<THREE.Group>(null)
  const healthBarRef = useRef<THREE.Mesh>(null)

  useFrame(() => {
    if (!groupRef.current) return

    const { entity, targetPos } = getInterpolatedEntity(stateBuffer, entityId)
    if (!entity || !targetPos) return

    // Render interpolated position directly
    groupRef.current.position.set(targetPos[0], targetPos[1], targetPos[2])

    // Update health bar
    if (healthBarRef.current) {
      const healthRatio = (entity.health ?? 80) / 80
      healthBarRef.current.scale.x = healthRatio
    }
  })

  return (
    <group ref={groupRef} position={[0, 0, 0]}>
      <mesh castShadow>
        <boxGeometry args={[1, 1.5, 1]} />
        <meshStandardMaterial color="#e74c3c" />
      </mesh>
      <mesh ref={healthBarRef} position={[0, 1.5, 0]}>
        <boxGeometry args={[1, 0.1, 0.1]} />
        <meshBasicMaterial color="#ef4444" />
      </mesh>
    </group>
  )
}

// Part entity - renders based on shape property (Roblox-style)
function PartEntity({ entityId, stateBuffer }: EntityProps) {
  const meshRef = useRef<THREE.Mesh>(null)

  // Get initial entity for static properties
  const latest = stateBuffer.getLatest()
  const initialEntity = latest?.entities.get(entityId)

  const size = initialEntity?.size || [1, 1, 1]
  const color = toColor(initialEntity?.color)
  const materialProps = getMaterialProps(initialEntity?.material, color)
  const shape = initialEntity?.shape || 'Block'

  useFrame(() => {
    if (!meshRef.current) return

    const { targetPos, targetQuat } = getInterpolatedEntity(stateBuffer, entityId)
    if (!targetPos) return

    // Render interpolated position directly
    meshRef.current.position.set(targetPos[0], targetPos[1], targetPos[2])

    // Render interpolated rotation directly
    if (targetQuat) {
      meshRef.current.quaternion.copy(targetQuat)
    }
  })

  switch (shape) {
    case 'Ball': {
      const radius = Math.min(size[0], size[1], size[2]) / 2
      return (
        <mesh ref={meshRef} position={[0, 0, 0]} castShadow receiveShadow>
          <sphereGeometry args={[radius, 24, 24]} />
          <meshStandardMaterial {...materialProps} />
        </mesh>
      )
    }

    case 'Cylinder': {
      const cylRadius = size[0] / 2
      const cylHeight = size[1]
      return (
        <mesh ref={meshRef} position={[0, 0, 0]} castShadow receiveShadow>
          <capsuleGeometry args={[cylRadius, cylHeight - cylRadius * 2, 8, 16]} />
          <meshStandardMaterial {...materialProps} />
        </mesh>
      )
    }

    case 'Wedge':
      return (
        <mesh ref={meshRef} position={[0, 0, 0]} castShadow receiveShadow>
          <boxGeometry args={size as [number, number, number]} />
          <meshStandardMaterial {...materialProps} />
        </mesh>
      )

    case 'Block':
    default:
      return (
        <mesh ref={meshRef} position={[0, 0, 0]} castShadow receiveShadow>
          <boxGeometry args={size as [number, number, number]} />
          <meshStandardMaterial {...materialProps} />
        </mesh>
      )
  }
}

function Entity({ entityId, stateBuffer }: EntityProps) {
  // Get the entity type from the latest snapshot
  const latest = stateBuffer.getLatest()
  const entity = latest?.entities.get(entityId)

  if (!entity) {
    return null
  }

  // Special rendering for specific entity types (legacy support)
  if (entity.type === 'pickup') {
    return <PickupEntity entityId={entityId} stateBuffer={stateBuffer} />
  }

  if (entity.type === 'enemy') {
    return <EnemyEntity entityId={entityId} stateBuffer={stateBuffer} />
  }

  // Default: render as Part based on shape
  return <PartEntity entityId={entityId} stateBuffer={stateBuffer} />
}

// Memoize to prevent unnecessary re-renders
export default memo(Entity, (prev, next) => {
  return prev.entityId === next.entityId && prev.stateBuffer === next.stateBuffer
})

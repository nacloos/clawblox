import { memo, useRef, useEffect } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'
import { damp3, dampQ } from 'maath/easing'
import { SpectatorEntity } from '../api'

// Smoothing factor for position/rotation interpolation (higher = faster)
const SMOOTHING = 12

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

function toColor(colorArray?: [number, number, number]): string {
  if (!colorArray) return '#999999'
  return `rgb(${Math.round(colorArray[0] * 255)}, ${Math.round(colorArray[1] * 255)}, ${Math.round(colorArray[2] * 255)})`
}

// Animated pickup component - smooth position + bobbing animation
function PickupEntity({ entity }: EntityProps) {
  const meshRef = useRef<THREE.Mesh>(null)
  const targetPos = useRef(new THREE.Vector3(entity.position[0], entity.position[1], entity.position[2]))
  const isHealth = entity.pickup_type === 'health'
  const baseColor = isHealth ? '#22c55e' : '#3b82f6'

  useEffect(() => {
    targetPos.current.set(entity.position[0], entity.position[1], entity.position[2])
  }, [entity.position[0], entity.position[1], entity.position[2]])

  useFrame(({ clock }, delta) => {
    if (!meshRef.current) return
    // Smooth interpolation for x and z
    meshRef.current.position.x += (targetPos.current.x - meshRef.current.position.x) * Math.min(1, SMOOTHING * delta)
    meshRef.current.position.z += (targetPos.current.z - meshRef.current.position.z) * Math.min(1, SMOOTHING * delta)
    // Bobbing animation for y
    meshRef.current.position.y = targetPos.current.y + 0.3 + Math.sin(clock.getElapsedTime() * 2) * 0.1
  })

  const [x, y, z] = entity.position

  return (
    <mesh ref={meshRef} position={[x, y + 0.3, z]} castShadow>
      <sphereGeometry args={[0.4, 16, 16]} />
      <meshStandardMaterial
        color={baseColor}
        emissive={baseColor}
        emissiveIntensity={0.3}
      />
    </mesh>
  )
}

// Enemy entity with health bar - uses smooth interpolation
function EnemyEntity({ entity }: EntityProps) {
  const groupRef = useRef<THREE.Group>(null)
  const targetPos = useRef(new THREE.Vector3(...entity.position))
  const healthRatio = (entity.health ?? 80) / 80

  useEffect(() => {
    targetPos.current.set(...entity.position)
  }, [entity.position[0], entity.position[1], entity.position[2]])

  useFrame((_, delta) => {
    if (!groupRef.current) return
    damp3(groupRef.current.position, targetPos.current, SMOOTHING, delta)
  })

  const [x, y, z] = entity.position

  return (
    <group ref={groupRef} position={[x, y, z]}>
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

// Convert Roblox rotation matrix to Three.js Quaternion for smooth interpolation
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

// Part entity - renders based on shape property (Roblox-style)
// Uses smooth interpolation for position and rotation
function PartEntity({ entity }: EntityProps) {
  const meshRef = useRef<THREE.Mesh>(null)
  const targetPos = useRef(new THREE.Vector3(...entity.position))
  const targetQuat = useRef(entity.rotation ? rotationToQuaternion(entity.rotation) : new THREE.Quaternion())

  const size = entity.size || [1, 1, 1]
  const color = toColor(entity.color)
  const materialProps = getMaterialProps(entity.material, color)
  const shape = entity.shape || 'Block'

  // Update targets when entity data changes
  useEffect(() => {
    targetPos.current.set(...entity.position)
    if (entity.rotation) {
      targetQuat.current.copy(rotationToQuaternion(entity.rotation))
    }
  }, [entity.position[0], entity.position[1], entity.position[2], entity.rotation])

  // Smooth interpolation each frame
  useFrame((_, delta) => {
    if (!meshRef.current) return
    damp3(meshRef.current.position, targetPos.current, SMOOTHING, delta)
    dampQ(meshRef.current.quaternion, targetQuat.current, SMOOTHING, delta)
  })

  // Initial position from entity data
  const [x, y, z] = entity.position
  const initialQuat = entity.rotation ? rotationToQuaternion(entity.rotation) : new THREE.Quaternion()

  switch (shape) {
    case 'Ball':
      const radius = Math.min(size[0], size[1], size[2]) / 2
      return (
        <mesh ref={meshRef} position={[x, y, z]} quaternion={initialQuat} castShadow receiveShadow>
          <sphereGeometry args={[radius, 24, 24]} />
          <meshStandardMaterial {...materialProps} />
        </mesh>
      )

    case 'Cylinder':
      const cylRadius = size[0] / 2
      const cylHeight = size[1]
      return (
        <mesh ref={meshRef} position={[x, y, z]} quaternion={initialQuat} castShadow receiveShadow>
          <capsuleGeometry args={[cylRadius, cylHeight - cylRadius * 2, 8, 16]} />
          <meshStandardMaterial {...materialProps} />
        </mesh>
      )

    case 'Wedge':
      return (
        <mesh ref={meshRef} position={[x, y, z]} quaternion={initialQuat} castShadow receiveShadow>
          <boxGeometry args={size as [number, number, number]} />
          <meshStandardMaterial {...materialProps} />
        </mesh>
      )

    case 'Block':
    default:
      return (
        <mesh ref={meshRef} position={[x, y, z]} quaternion={initialQuat} castShadow receiveShadow>
          <boxGeometry args={size as [number, number, number]} />
          <meshStandardMaterial {...materialProps} />
        </mesh>
      )
  }
}

function Entity({ entity }: EntityProps) {
  // Special rendering for specific entity types (legacy support)
  if (entity.type === 'pickup') {
    return <PickupEntity entity={entity} />
  }

  if (entity.type === 'enemy') {
    return <EnemyEntity entity={entity} />
  }

  // Default: render as Part based on shape
  return <PartEntity entity={entity} />
}

// Memoize to prevent unnecessary re-renders
export default memo(Entity, (prev, next) => {
  const p = prev.entity
  const n = next.entity

  // Compare rotation matrices
  const rotEqual = (p.rotation === n.rotation) || (
    p.rotation && n.rotation &&
    p.rotation[0][0] === n.rotation[0][0] &&
    p.rotation[0][1] === n.rotation[0][1] &&
    p.rotation[0][2] === n.rotation[0][2] &&
    p.rotation[1][0] === n.rotation[1][0] &&
    p.rotation[1][1] === n.rotation[1][1] &&
    p.rotation[1][2] === n.rotation[1][2] &&
    p.rotation[2][0] === n.rotation[2][0] &&
    p.rotation[2][1] === n.rotation[2][1] &&
    p.rotation[2][2] === n.rotation[2][2]
  )

  // Compare key properties that would affect rendering
  return (
    p.id === n.id &&
    p.type === n.type &&
    p.position[0] === n.position[0] &&
    p.position[1] === n.position[1] &&
    p.position[2] === n.position[2] &&
    rotEqual &&
    p.size?.[0] === n.size?.[0] &&
    p.size?.[1] === n.size?.[1] &&
    p.size?.[2] === n.size?.[2] &&
    p.color?.[0] === n.color?.[0] &&
    p.color?.[1] === n.color?.[1] &&
    p.color?.[2] === n.color?.[2] &&
    p.material === n.material &&
    p.shape === n.shape &&
    p.health === n.health &&
    p.pickup_type === n.pickup_type
  )
})

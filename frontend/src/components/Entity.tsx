import { memo, Suspense, useEffect, useMemo, useRef, useState } from 'react'
import { useFrame } from '@react-three/fiber'
import { Html, useAnimations, useGLTF } from '@react-three/drei'
import * as THREE from 'three'
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js'
import { StateBuffer, EntitySnapshot, interpolatePosition, BillboardGui } from '../lib/stateBuffer'

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

function sanitizeModelUrl(url?: string): string | null {
  if (!url) return null
  const trimmed = url.trim()
  if (trimmed.startsWith('/static/') || trimmed.startsWith('/assets/') || trimmed.startsWith('https://')) return trimmed
  return null
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

// BillboardGui component - renders floating labels above parts
function BillboardGuiComponent({ billboard, offset }: { billboard: BillboardGui; offset: [number, number, number] }) {
  const toRgb = (c: [number, number, number]) =>
    `rgb(${Math.round(c[0] * 255)}, ${Math.round(c[1] * 255)}, ${Math.round(c[2] * 255)})`

  return (
    <Html
      position={[
        offset[0] + billboard.studs_offset[0],
        offset[1] + billboard.studs_offset[1],
        offset[2] + billboard.studs_offset[2],
      ]}
      center
      style={{
        pointerEvents: 'none',
        userSelect: 'none',
      }}
      zIndexRange={billboard.always_on_top ? [100, 0] : [0, 0]}
    >
      <div style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        whiteSpace: 'nowrap',
      }}>
        {billboard.labels.map((label, i) => (
          <div
            key={i}
            style={{
              color: toRgb(label.color),
              fontSize: `${Math.max(12, label.size)}px`,
              fontWeight: 'bold',
              textShadow: '1px 1px 2px black, -1px -1px 2px black',
              lineHeight: 1.2,
            }}
          >
            {label.text}
          </div>
        ))}
      </div>
    </Html>
  )
}

function billboardEqual(a: BillboardGui | null, b: BillboardGui | null): boolean {
  if (a === b) return true
  if (!a || !b) return false
  if (a.always_on_top !== b.always_on_top) return false
  if (a.studs_offset[0] !== b.studs_offset[0] || a.studs_offset[1] !== b.studs_offset[1] || a.studs_offset[2] !== b.studs_offset[2]) {
    return false
  }
  if (a.labels.length !== b.labels.length) return false
  for (let i = 0; i < a.labels.length; i++) {
    const la = a.labels[i]
    const lb = b.labels[i]
    if (la.text !== lb.text || la.size !== lb.size) return false
    if (la.color[0] !== lb.color[0] || la.color[1] !== lb.color[1] || la.color[2] !== lb.color[2]) {
      return false
    }
  }
  return true
}

function ModelMesh({
  modelUrl,
  targetSize,
}: {
  modelUrl: string
  targetSize: [number, number, number]
}) {
  const { scene, animations } = useGLTF(modelUrl)
  const rootRef = useRef<THREE.Group>(null)
  // Use SkeletonUtils.clone for proper SkinnedMesh cloning (preserves skeleton bindings)
  const clonedScene = useMemo(() => SkeletonUtils.clone(scene), [scene])
  // Create a ref-like object for useAnimations
  const sceneRef = useRef<THREE.Group>(clonedScene as THREE.Group)
  sceneRef.current = clonedScene as THREE.Group
  const { actions, names } = useAnimations(animations, sceneRef)

  useEffect(() => {
    console.log('[ModelMesh] Available animations:', names)
  }, [names])
  // Find walk/run animation or use first available animation
  const walkClipName = useMemo(
    () => names.find((name) => /(walk|run|jog|locomotion)/i.test(name)) ?? names[0] ?? null,
    [names]
  )
  const idleClipName = useMemo(
    () => names.find((name) => /idle/i.test(name)) ?? null,
    [names]
  )
  const walkAction = walkClipName ? actions[walkClipName] : undefined
  const idleAction = idleClipName ? actions[idleClipName] : undefined

  useEffect(() => {
    console.log('[ModelMesh] walkClipName:', walkClipName, 'walkAction:', !!walkAction)
  }, [walkClipName, walkAction])
  const lastWorldPos = useRef<THREE.Vector3 | null>(null)
  const moveSpeed = useRef(0)
  const fallbackPhase = useRef(0)
  const targetRotationY = useRef(0)
  const currentRotationY = useRef(0)
  const modelFit = useMemo(() => {
    // Find the actual SkinnedMesh to get accurate bounds (not bones/skeleton)
    let mesh: THREE.SkinnedMesh | THREE.Mesh | null = null
    clonedScene.traverse((obj) => {
      if (!mesh && (obj as THREE.SkinnedMesh).isSkinnedMesh) {
        mesh = obj as THREE.SkinnedMesh
      } else if (!mesh && (obj as THREE.Mesh).isMesh) {
        mesh = obj as THREE.Mesh
      }
    })

    const bounds = new THREE.Box3()
    if (mesh) {
      // Compute bounds from geometry directly
      mesh.geometry.computeBoundingBox()
      if (mesh.geometry.boundingBox) {
        bounds.copy(mesh.geometry.boundingBox)
      }
    } else {
      bounds.setFromObject(clonedScene)
    }

    const size = bounds.getSize(new THREE.Vector3())
    const center = bounds.getCenter(new THREE.Vector3())

    console.log('[ModelMesh] Mesh bounds:', { size: { x: size.x, y: size.y, z: size.z }, center: { x: center.x, y: center.y, z: center.z } })

    // Scale to fit the target bounding box exactly (no safety factor)
    // Use uniform scaling based on the height to preserve proportions for humanoid models
    const uniformScale = targetSize[1] / Math.max(size.y, 1e-3)

    // Entity position is the CENTER of the bounding box, not the bottom
    // So we need to center the model vertically, not place feet at origin
    return {
      uniformScale: Number.isFinite(uniformScale) && uniformScale > 0 ? uniformScale : 1,
      // Recenter: place model center at origin (entity position is center)
      recenter: new THREE.Vector3(-center.x, -center.y, -center.z),
    }
  }, [clonedScene, targetSize, modelUrl])

  useEffect(() => {
    clonedScene.traverse((obj) => {
      if (obj instanceof THREE.Mesh) {
        obj.castShadow = true
        obj.receiveShadow = true
        obj.frustumCulled = false
      }
    })
  }, [clonedScene])

  useEffect(() => {
    // Start walk animation but immediately pause at first frame for idle pose
    if (walkAction) {
      walkAction.reset().play()
      walkAction.paused = true
      walkAction.time = 0
    }
    if (idleAction && idleAction !== walkAction) {
      idleAction.reset().play()
    }
    return () => {
      walkAction?.stop()
      if (idleAction && idleAction !== walkAction) {
        idleAction.stop()
      }
    }
  }, [idleAction, walkAction])

  useFrame((_, delta) => {
    if (!rootRef.current) return

    // Keep model locked to the entity transform (some clips include root translation).
    rootRef.current.position.x = 0
    rootRef.current.position.z = 0

    const current = new THREE.Vector3()
    rootRef.current.getWorldPosition(current)

    if (lastWorldPos.current && delta > 0) {
      const speed = current.distanceTo(lastWorldPos.current) / delta
      // Fast decay when slowing down, slower ramp up when speeding up
      const lerpFactor = speed < moveSpeed.current ? 0.5 : 0.2
      moveSpeed.current = THREE.MathUtils.lerp(moveSpeed.current, speed, lerpFactor)

      // Calculate direction of movement and rotate to face it
      const dx = current.x - lastWorldPos.current.x
      const dz = current.z - lastWorldPos.current.z
      if (Math.abs(dx) > 0.01 || Math.abs(dz) > 0.01) {
        targetRotationY.current = Math.atan2(dx, dz)
      }
    }
    lastWorldPos.current = current.clone()

    // Smoothly interpolate rotation
    const rotDiff = targetRotationY.current - currentRotationY.current
    // Handle wrap-around for angles
    const shortestRotDiff = Math.atan2(Math.sin(rotDiff), Math.cos(rotDiff))
    currentRotationY.current += shortestRotDiff * Math.min(1, delta * 10)
    rootRef.current.rotation.y = currentRotationY.current

    const isMoving = moveSpeed.current > 0.5
    if (walkAction) {
      if (isMoving) {
        if (!walkAction.isRunning()) {
          walkAction.reset().play()
        }
        walkAction.paused = false
        walkAction.timeScale = THREE.MathUtils.clamp(moveSpeed.current / 5, 0.7, 1.6)
      } else {
        // Stop animation and reset to first frame when not moving
        walkAction.paused = true
        walkAction.time = 0
      }
    }
    if (idleAction && idleAction !== walkAction) {
      idleAction.paused = false
      idleAction.weight = isMoving ? 0 : 1
    }

    // Fallback walk-like motion for static GLBs with no animation clips.
    if (!walkAction && !idleAction) {
      if (isMoving) {
        fallbackPhase.current += delta * 10
        rootRef.current.position.y = Math.sin(fallbackPhase.current) * 0.06
      } else {
        rootRef.current.position.y = THREE.MathUtils.lerp(rootRef.current.position.y, 0, 0.2)
      }
    } else {
      rootRef.current.position.y = 0
    }
  })

  return (
    <group ref={rootRef}>
      <group scale={[modelFit.uniformScale, modelFit.uniformScale, modelFit.uniformScale]}>
        <group position={[modelFit.recenter.x, modelFit.recenter.y, modelFit.recenter.z]}>
          <primitive object={clonedScene} />
        </group>
      </group>
    </group>
  )
}

// Part entity - renders based on shape property (Roblox-style)
function PartEntity({ entityId, stateBuffer }: EntityProps) {
  const groupRef = useRef<THREE.Group>(null)
  const meshRef = useRef<THREE.Mesh>(null)
  const modelGroupRef = useRef<THREE.Group>(null)

  // Get initial entity for static properties
  const latest = stateBuffer.getLatest()
  const initialEntity = latest?.entities.get(entityId)

  const size = initialEntity?.size || [1, 1, 1]
  const color = toColor(initialEntity?.color)
  const materialProps = getMaterialProps(initialEntity?.material, color)
  const shape = initialEntity?.shape || 'Block'
  const modelUrl = sanitizeModelUrl(initialEntity?.model_url)
  const [billboardGui, setBillboardGui] = useState<BillboardGui | null>(initialEntity?.billboard_gui ?? null)
  const billboardGuiRef = useRef<BillboardGui | null>(billboardGui)

  useFrame(() => {
    if (!groupRef.current) return

    const { entity, targetPos, targetQuat } = getInterpolatedEntity(stateBuffer, entityId)
    if (!targetPos) return

    // Render interpolated position directly
    groupRef.current.position.set(targetPos[0], targetPos[1], targetPos[2])
    // Force matrix world update to propagate transforms to children (fixes model culling issues)
    groupRef.current.updateMatrixWorld(true)

    // Render interpolated rotation directly
    if (targetQuat) {
      const target = modelUrl ? modelGroupRef.current : meshRef.current
      if (target) target.quaternion.copy(targetQuat)
    }

    const nextBillboard = entity?.billboard_gui ?? null
    if (!billboardEqual(billboardGuiRef.current, nextBillboard)) {
      console.log('BillboardGui change', {
        entityId,
        prev: billboardGuiRef.current,
        next: nextBillboard,
      })
      billboardGuiRef.current = nextBillboard
      setBillboardGui(nextBillboard)
    }
  })

  const renderPrimitiveMesh = (withRef: boolean) => {
    const refProp = withRef ? { ref: meshRef } : {}

    switch (shape) {
      case 'Ball': {
        const radius = Math.min(size[0], size[1], size[2]) / 2
        return (
          <mesh {...refProp} castShadow receiveShadow>
            <sphereGeometry args={[radius, 24, 24]} />
            <meshStandardMaterial {...materialProps} />
          </mesh>
        )
      }

      case 'Cylinder': {
        const cylRadius = size[0] / 2
        const cylHeight = size[1]
        return (
          <mesh {...refProp} castShadow receiveShadow>
            <capsuleGeometry args={[cylRadius, cylHeight - cylRadius * 2, 8, 16]} />
            <meshStandardMaterial {...materialProps} />
          </mesh>
        )
      }

      case 'Wedge':
        return (
          <mesh {...refProp} castShadow receiveShadow>
            <boxGeometry args={size as [number, number, number]} />
            <meshStandardMaterial {...materialProps} />
          </mesh>
        )

      case 'Block':
      default:
        return (
          <mesh {...refProp} castShadow receiveShadow>
            <boxGeometry args={size as [number, number, number]} />
            <meshStandardMaterial {...materialProps} />
          </mesh>
        )
    }
  }

  const renderMesh = () => {
    if (!modelUrl) {
      return renderPrimitiveMesh(true)
    }

    return (
      <group ref={modelGroupRef}>
        <Suspense fallback={renderPrimitiveMesh(false)}>
          <ModelMesh modelUrl={modelUrl} targetSize={size as [number, number, number]} />
        </Suspense>
      </group>
    )
  }

  return (
    <group ref={groupRef}>
      {renderMesh()}
      {billboardGui && billboardGui.labels.length > 0 && (
        <BillboardGuiComponent billboard={billboardGui} offset={[0, size[1] / 2, 0]} />
      )}
    </group>
  )
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

useGLTF.preload('/static/models/player.glb')

// Memoize to prevent unnecessary re-renders
export default memo(Entity, (prev, next) => {
  return prev.entityId === next.entityId && prev.stateBuffer === next.stateBuffer
})

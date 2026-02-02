import { useRef, useMemo } from 'react'
import { Canvas, useFrame, useThree } from '@react-three/fiber'
import Entity from './Entity'
import * as THREE from 'three'
import { StateBuffer, interpolatePosition } from '../lib/stateBuffer'

// Camera smoothing factor (higher = faster response)
const CAMERA_SMOOTHING = 8

interface GameSceneProps {
  stateBuffer: StateBuffer
  entityIds: number[]
  followPlayerId?: string | null
}

// Arena is 200x200 units, centered at origin
// Fixed overhead isometric view that frames the arena
const OVERVIEW_POSITION = new THREE.Vector3(0, 140, 70)
const OVERVIEW_TARGET = new THREE.Vector3(0, 0, 0)

// Follow camera settings
const FOLLOW_DISTANCE = 20
const FOLLOW_HEIGHT = 15
const MIN_CAMERA_DISTANCE = 5

function CameraController({
  stateBuffer,
  followPlayerId
}: {
  stateBuffer: StateBuffer
  followPlayerId?: string | null
}) {
  const { scene } = useThree()
  const targetPosition = useRef(new THREE.Vector3())
  const targetLookAt = useRef(new THREE.Vector3())
  const raycaster = useMemo(() => new THREE.Raycaster(), [])
  const rayDirection = useRef(new THREE.Vector3())

  useFrame(({ camera }, delta) => {
    if (followPlayerId) {
      // Get interpolated player position from buffer
      const result = stateBuffer.getInterpolatedState()
      if (result) {
        const playerBefore = result.before.players.get(followPlayerId)
        const playerAfter = result.after?.players.get(followPlayerId)

        if (playerBefore) {
          let x: number, y: number, z: number

          if (playerAfter && result.alpha > 0) {
            // Interpolate player position
            const pos = interpolatePosition(playerBefore.position, playerAfter.position, result.alpha)
            ;[x, y, z] = pos
          } else {
            // Use before snapshot position (hold position)
            ;[x, y, z] = playerBefore.position
          }

          const playerPos = new THREE.Vector3(x, y + 2, z)

          // Desired camera position (behind and above player)
          const desiredPos = new THREE.Vector3(
            x + FOLLOW_DISTANCE * 0.7,
            y + FOLLOW_HEIGHT,
            z + FOLLOW_DISTANCE * 0.7
          )

          // Raycast from player to desired camera position for wall avoidance
          rayDirection.current.copy(desiredPos).sub(playerPos).normalize()
          const desiredDistance = desiredPos.distanceTo(playerPos)

          raycaster.set(playerPos, rayDirection.current)
          raycaster.far = desiredDistance

          const intersects = raycaster.intersectObjects(scene.children, true)

          // Filter out non-collidable objects (like the player itself)
          const validHits = intersects.filter(hit => {
            const obj = hit.object
            return obj.type === 'Mesh' && obj.visible
          })

          if (validHits.length > 0 && validHits[0].distance < desiredDistance) {
            // Move camera closer to avoid wall
            const safeDistance = Math.max(validHits[0].distance - 1, MIN_CAMERA_DISTANCE)
            targetPosition.current.copy(playerPos).addScaledVector(rayDirection.current, safeDistance)
          } else {
            targetPosition.current.copy(desiredPos)
          }

          targetLookAt.current.copy(playerPos)
        }
      }
    } else {
      targetPosition.current.copy(OVERVIEW_POSITION)
      targetLookAt.current.copy(OVERVIEW_TARGET)
    }

    // Frame-rate independent smooth camera movement
    const factor = 1 - Math.exp(-CAMERA_SMOOTHING * delta)
    camera.position.lerp(targetPosition.current, factor)
    camera.lookAt(targetLookAt.current)
  })

  return null
}

export default function GameScene({ stateBuffer, entityIds, followPlayerId }: GameSceneProps) {
  return (
    <Canvas
      camera={{ position: [0, 140, 70], fov: 50 }}
      shadows
      gl={{ logarithmicDepthBuffer: true }}
      style={{ background: 'linear-gradient(to bottom, #1a1a2e 0%, #0f0f1a 100%)' }}
    >
      <hemisphereLight args={['#87ceeb', '#444444', 0.6]} />
      <ambientLight intensity={0.15} />
      <directionalLight
        position={[100, 200, 100]}
        intensity={1.2}
        castShadow
        shadow-mapSize={[2048, 2048]}
        shadow-camera-near={1}
        shadow-camera-far={400}
        shadow-camera-left={-120}
        shadow-camera-right={120}
        shadow-camera-top={120}
        shadow-camera-bottom={-120}
        shadow-bias={-0.0001}
      />
      <pointLight position={[-50, 50, -50]} intensity={0.3} color="#4a9eff" />

      {entityIds.map((entityId) => (
        <Entity key={entityId} entityId={entityId} stateBuffer={stateBuffer} />
      ))}

      <CameraController stateBuffer={stateBuffer} followPlayerId={followPlayerId} />
    </Canvas>
  )
}

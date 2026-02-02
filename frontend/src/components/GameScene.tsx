import { useRef, useMemo } from 'react'
import { Canvas, useFrame, useThree } from '@react-three/fiber'
import { SpectatorObservation } from '../api'
import Entity from './Entity'
import * as THREE from 'three'

interface GameSceneProps {
  gameState: SpectatorObservation
  followPlayerId?: string | null
}

// Arena is 80x80 units, centered at origin
// Fixed overhead isometric view that frames the arena
const OVERVIEW_POSITION = new THREE.Vector3(0, 55, 28)
const OVERVIEW_TARGET = new THREE.Vector3(0, 0, 0)

// Follow camera settings
const FOLLOW_DISTANCE = 12
const FOLLOW_HEIGHT = 8
const MIN_CAMERA_DISTANCE = 3

function CameraController({
  gameState,
  followPlayerId
}: {
  gameState: SpectatorObservation
  followPlayerId?: string | null
}) {
  const { scene } = useThree()
  const targetPosition = useRef(new THREE.Vector3())
  const targetLookAt = useRef(new THREE.Vector3())
  const raycaster = useMemo(() => new THREE.Raycaster(), [])
  const rayDirection = useRef(new THREE.Vector3())

  useFrame(({ camera }) => {
    if (followPlayerId) {
      const player = gameState.players.find(p => p.id === followPlayerId)
      if (player) {
        const [x, y, z] = player.position
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
    } else {
      targetPosition.current.copy(OVERVIEW_POSITION)
      targetLookAt.current.copy(OVERVIEW_TARGET)
    }

    // Smooth camera movement
    camera.position.lerp(targetPosition.current, 0.08)
    camera.lookAt(targetLookAt.current)
  })

  return null
}

export default function GameScene({ gameState, followPlayerId }: GameSceneProps) {
  return (
    <Canvas
      camera={{ position: [0, 55, 28], fov: 50 }}
      shadows
      gl={{ logarithmicDepthBuffer: true }}
      style={{ background: 'linear-gradient(to bottom, #1a1a2e 0%, #0f0f1a 100%)' }}
    >
      <hemisphereLight args={['#87ceeb', '#444444', 0.6]} />
      <ambientLight intensity={0.15} />
      <directionalLight
        position={[50, 100, 50]}
        intensity={1.2}
        castShadow
        shadow-mapSize={[2048, 2048]}
        shadow-camera-near={1}
        shadow-camera-far={250}
        shadow-camera-left={-80}
        shadow-camera-right={80}
        shadow-camera-top={80}
        shadow-camera-bottom={-80}
        shadow-bias={-0.0001}
      />
      <pointLight position={[-50, 50, -50]} intensity={0.3} color="#4a9eff" />

      {gameState.entities.map((entity) => (
        <Entity key={entity.id} entity={entity} />
      ))}

      <CameraController gameState={gameState} followPlayerId={followPlayerId} />
    </Canvas>
  )
}

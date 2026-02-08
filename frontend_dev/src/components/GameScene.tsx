import { useRef, useMemo, useEffect } from 'react'
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

// Overview controls
const OVERVIEW_PAN_SPEED = 55
const OVERVIEW_ZOOM_SPEED = 0.001
const MIN_OVERVIEW_ZOOM = 0.2
const MAX_OVERVIEW_ZOOM = 1.8
const TOUCH_PAN_SPEED = 0.12
const CAMERA_BOUNDS_PADDING = -5 // Studs of padding around entity bounds

function CameraController({
  stateBuffer,
  followPlayerId
}: {
  stateBuffer: StateBuffer
  followPlayerId?: string | null
}) {
  const { scene, gl } = useThree()
  const targetPosition = useRef(new THREE.Vector3())
  const targetLookAt = useRef(new THREE.Vector3())
  const raycaster = useMemo(() => new THREE.Raycaster(), [])
  const rayDirection = useRef(new THREE.Vector3())
  const pressedKeys = useRef(new Set<string>())
  const overviewPan = useRef(new THREE.Vector3(0, 0, 0))
  const overviewZoom = useRef(1)
  const touchPoints = useRef(new Map<number, { x: number; y: number }>())
  const lastTouchCenter = useRef<{ x: number; y: number } | null>(null)
  const pinchStartDistance = useRef<number | null>(null)
  const pinchStartZoom = useRef<number | null>(null)

  useEffect(() => {
    const shouldIgnoreKeyboardEvent = (target: EventTarget | null): boolean => {
      if (!(target instanceof HTMLElement)) return false
      const tagName = target.tagName.toLowerCase()
      return target.isContentEditable || tagName === 'input' || tagName === 'textarea' || tagName === 'select'
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (shouldIgnoreKeyboardEvent(event.target)) return
      const key = event.key.toLowerCase()
      if (key === 'w' || key === 'a' || key === 's' || key === 'd') {
        pressedKeys.current.add(key)
      }
    }

    const onKeyUp = (event: KeyboardEvent) => {
      pressedKeys.current.delete(event.key.toLowerCase())
    }

    const onWindowBlur = () => {
      pressedKeys.current.clear()
    }

    const onWheel = (event: WheelEvent) => {
      if (followPlayerId) return

      event.preventDefault()
      // Keep camera angle fixed by zooming via distance scaling only.
      const nextZoom = overviewZoom.current * Math.exp(event.deltaY * OVERVIEW_ZOOM_SPEED)
      overviewZoom.current = THREE.MathUtils.clamp(nextZoom, MIN_OVERVIEW_ZOOM, MAX_OVERVIEW_ZOOM)
    }

    const distanceBetweenTouches = () => {
      const points = Array.from(touchPoints.current.values())
      if (points.length < 2) return null
      const dx = points[0].x - points[1].x
      const dy = points[0].y - points[1].y
      return Math.hypot(dx, dy)
    }

    const centerOfTouches = () => {
      const points = Array.from(touchPoints.current.values())
      if (points.length === 0) return null
      const sum = points.reduce((acc, p) => ({ x: acc.x + p.x, y: acc.y + p.y }), { x: 0, y: 0 })
      return { x: sum.x / points.length, y: sum.y / points.length }
    }

    const clampPanToBounds = () => {
      const bounds = stateBuffer.getAABB(CAMERA_BOUNDS_PADDING)
      if (!bounds) return
      overviewPan.current.x = THREE.MathUtils.clamp(
        overviewPan.current.x, bounds.minX, bounds.maxX
      )
      overviewPan.current.z = THREE.MathUtils.clamp(
        overviewPan.current.z, bounds.minZ, bounds.maxZ
      )
    }

    const panByScreenDelta = (dx: number, dy: number) => {
      const scale = TOUCH_PAN_SPEED * overviewZoom.current
      // Finger drag should move map with the finger.
      overviewPan.current.x -= dx * scale
      overviewPan.current.z -= dy * scale
      clampPanToBounds()
    }

    const onPointerDown = (event: PointerEvent) => {
      if (event.pointerType !== 'touch' || followPlayerId) return
      touchPoints.current.set(event.pointerId, { x: event.clientX, y: event.clientY })
      lastTouchCenter.current = centerOfTouches()

      if (touchPoints.current.size >= 2) {
        pinchStartDistance.current = distanceBetweenTouches()
        pinchStartZoom.current = overviewZoom.current
      }
    }

    const onPointerMove = (event: PointerEvent) => {
      if (event.pointerType !== 'touch' || followPlayerId) return

      const previous = touchPoints.current.get(event.pointerId)
      if (!previous) return
      event.preventDefault()

      touchPoints.current.set(event.pointerId, { x: event.clientX, y: event.clientY })

      if (touchPoints.current.size >= 2) {
        const center = centerOfTouches()
        const dist = distanceBetweenTouches()
        if (center && lastTouchCenter.current) {
          panByScreenDelta(center.x - lastTouchCenter.current.x, center.y - lastTouchCenter.current.y)
        }
        if (dist && pinchStartDistance.current && pinchStartZoom.current) {
          const ratio = dist / pinchStartDistance.current
          const nextZoom = pinchStartZoom.current / ratio
          overviewZoom.current = THREE.MathUtils.clamp(nextZoom, MIN_OVERVIEW_ZOOM, MAX_OVERVIEW_ZOOM)
        }
        lastTouchCenter.current = center
      } else {
        panByScreenDelta(event.clientX - previous.x, event.clientY - previous.y)
      }
    }

    const resetTouchState = () => {
      if (touchPoints.current.size < 2) {
        pinchStartDistance.current = null
        pinchStartZoom.current = null
      }
      lastTouchCenter.current = centerOfTouches()
    }

    const onPointerEnd = (event: PointerEvent) => {
      if (event.pointerType !== 'touch') return
      touchPoints.current.delete(event.pointerId)
      resetTouchState()
    }

    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('keyup', onKeyUp)
    window.addEventListener('blur', onWindowBlur)
    gl.domElement.addEventListener('wheel', onWheel, { passive: false })
    gl.domElement.addEventListener('pointerdown', onPointerDown)
    gl.domElement.addEventListener('pointermove', onPointerMove, { passive: false })
    gl.domElement.addEventListener('pointerup', onPointerEnd)
    gl.domElement.addEventListener('pointercancel', onPointerEnd)

    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup', onKeyUp)
      window.removeEventListener('blur', onWindowBlur)
      gl.domElement.removeEventListener('wheel', onWheel)
      gl.domElement.removeEventListener('pointerdown', onPointerDown)
      gl.domElement.removeEventListener('pointermove', onPointerMove)
      gl.domElement.removeEventListener('pointerup', onPointerEnd)
      gl.domElement.removeEventListener('pointercancel', onPointerEnd)
    }
  }, [followPlayerId, gl])

  useFrame(({ camera }, delta) => {
    camera.up.set(0, 1, 0)

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
      const direction = new THREE.Vector3()
      if (pressedKeys.current.has('w')) direction.z -= 1
      if (pressedKeys.current.has('s')) direction.z += 1
      if (pressedKeys.current.has('a')) direction.x -= 1
      if (pressedKeys.current.has('d')) direction.x += 1

      if (direction.lengthSq() > 0) {
        direction.normalize()
        const step = (OVERVIEW_PAN_SPEED * overviewZoom.current) * delta
        overviewPan.current.x += direction.x * step
        overviewPan.current.z += direction.z * step
        // Clamp pan to entity bounds
        const bounds = stateBuffer.getAABB(CAMERA_BOUNDS_PADDING)
        if (bounds) {
          overviewPan.current.x = THREE.MathUtils.clamp(overviewPan.current.x, bounds.minX, bounds.maxX)
          overviewPan.current.z = THREE.MathUtils.clamp(overviewPan.current.z, bounds.minZ, bounds.maxZ)
        }
      }

      targetLookAt.current.set(overviewPan.current.x, OVERVIEW_TARGET.y, overviewPan.current.z)
      targetPosition.current.set(
        overviewPan.current.x + OVERVIEW_POSITION.x * overviewZoom.current,
        OVERVIEW_POSITION.y * overviewZoom.current,
        overviewPan.current.z + OVERVIEW_POSITION.z * overviewZoom.current
      )
    }

    // Only smooth while following a player; keep overview movement immediate.
    const factor = followPlayerId ? (1 - Math.exp(-CAMERA_SMOOTHING * delta)) : 1
    camera.position.lerp(targetPosition.current, factor)
    if (camera.zoom !== 1) {
      camera.zoom = 1
      camera.updateProjectionMatrix()
    }
    camera.lookAt(targetLookAt.current)
  })

  return null
}

export default function GameScene({ stateBuffer, entityIds, followPlayerId }: GameSceneProps) {
  return (
    <Canvas
      camera={{ position: [0, 140, 70], fov: 50 }}
      gl={{ logarithmicDepthBuffer: true }}
      style={{ background: 'linear-gradient(to bottom, #1a1a2e 0%, #0f0f1a 100%)', touchAction: 'none' }}
    >
      <hemisphereLight args={['#87ceeb', '#444444', 0.6]} />
      <ambientLight intensity={0.15} />
      <directionalLight
        position={[100, 200, 100]}
        intensity={1.2}
      />
      <pointLight position={[-50, 50, -50]} intensity={0.3} color="#4a9eff" />

      {entityIds.map((entityId) => (
        <Entity key={entityId} entityId={entityId} stateBuffer={stateBuffer} />
      ))}

      <CameraController stateBuffer={stateBuffer} followPlayerId={followPlayerId} />
    </Canvas>
  )
}

import * as THREE from 'three'

export interface FollowCameraOptions {
  followDistance: number
  followHeight: number
  shoulderOffset: number
  minDistance: number
  obstructionPadding: number
  positionSmoothing: number
  rotationSmoothing: number
  movementDeadzone: number
  lookAheadDistance: number
}

export interface FollowCameraUpdateInput {
  playerPosition: THREE.Vector3
  rootQuaternion?: THREE.Quaternion | null
  dt: number
  collisionObjects: THREE.Object3D[]
  ignoreObjects?: Array<THREE.Object3D | null | undefined>
}

const DEFAULT_UP = new THREE.Vector3(0, 1, 0)
const DEFAULT_FORWARD = new THREE.Vector3(0, 0, -1)

export class FollowCameraController {
  private readonly camera: THREE.PerspectiveCamera
  private readonly options: FollowCameraOptions
  private readonly raycaster = new THREE.Raycaster()

  private readonly forward = new THREE.Vector3(0, 0, -1)
  private readonly tmpForward = new THREE.Vector3()
  private readonly tmpMove = new THREE.Vector3()
  private readonly desiredPos = new THREE.Vector3()
  private readonly rayDir = new THREE.Vector3()
  private readonly targetPos = new THREE.Vector3()
  private readonly lookTarget = new THREE.Vector3()
  private readonly lookMatrix = new THREE.Matrix4()
  private readonly targetQuaternion = new THREE.Quaternion()
  private readonly ignoreSet = new Set<THREE.Object3D>()

  private lastPlayerPosition: THREE.Vector3 | null = null

  constructor(camera: THREE.PerspectiveCamera, options: FollowCameraOptions) {
    this.camera = camera
    this.options = options
  }

  getForward(out?: THREE.Vector3): THREE.Vector3 {
    if (!out) return this.forward.clone()
    return out.copy(this.forward)
  }

  reset(): void {
    this.lastPlayerPosition = null
    this.forward.copy(DEFAULT_FORWARD)
  }

  update(input: FollowCameraUpdateInput): void {
    this.computeForward(input.playerPosition, input.rootQuaternion ?? null)

    this.desiredPos
      .copy(input.playerPosition)
      .addScaledVector(this.forward, -this.options.followDistance)
      .addScaledVector(DEFAULT_UP, this.options.followHeight)
      .addScaledVector(this.computeRight(), this.options.shoulderOffset)

    this.rayDir.copy(this.desiredPos).sub(input.playerPosition)
    const desiredDistance = this.rayDir.length()
    if (desiredDistance > 1e-4) this.rayDir.multiplyScalar(1 / desiredDistance)
    else this.rayDir.copy(DEFAULT_FORWARD)

    this.raycaster.set(input.playerPosition, this.rayDir)
    this.raycaster.far = desiredDistance
    this.buildIgnoreSet(input.ignoreObjects)

    const hits = this.raycaster.intersectObjects(input.collisionObjects, true)
    let nearestDistance: number | null = null
    for (const hit of hits) {
      if (!(hit.object as THREE.Mesh).isMesh || !hit.object.visible) continue
      if (this.isIgnored(hit.object)) continue
      nearestDistance = hit.distance
      break
    }

    this.targetPos.copy(this.desiredPos)
    if (nearestDistance !== null && nearestDistance < desiredDistance) {
      const safeDistance = Math.max(
        nearestDistance - this.options.obstructionPadding,
        this.options.minDistance,
      )
      this.targetPos.copy(input.playerPosition).addScaledVector(this.rayDir, safeDistance)
    }

    const posAlpha = 1 - Math.exp(-this.options.positionSmoothing * input.dt)
    this.camera.position.lerp(this.targetPos, posAlpha)

    this.lookTarget
      .copy(input.playerPosition)
      .addScaledVector(this.forward, this.options.lookAheadDistance)
    this.lookMatrix.lookAt(this.camera.position, this.lookTarget, DEFAULT_UP)
    this.targetQuaternion.setFromRotationMatrix(this.lookMatrix)

    const rotAlpha = 1 - Math.exp(-this.options.rotationSmoothing * input.dt)
    this.camera.quaternion.slerp(this.targetQuaternion, rotAlpha)
  }

  private computeRight(): THREE.Vector3 {
    return this.tmpForward.crossVectors(this.forward, DEFAULT_UP).normalize()
  }

  private computeForward(playerPosition: THREE.Vector3, rootQuaternion: THREE.Quaternion | null): void {
    if (rootQuaternion) {
      this.forward.copy(DEFAULT_FORWARD).applyQuaternion(rootQuaternion).setY(0)
    } else if (this.lastPlayerPosition) {
      this.tmpMove.copy(playerPosition).sub(this.lastPlayerPosition).setY(0)
      if (this.tmpMove.lengthSq() > this.options.movementDeadzone) {
        this.forward.copy(this.tmpMove.normalize())
      }
    }

    if (this.forward.lengthSq() < 1e-4) this.forward.copy(DEFAULT_FORWARD)
    this.forward.normalize()
    if (!this.lastPlayerPosition) this.lastPlayerPosition = new THREE.Vector3()
    this.lastPlayerPosition.copy(playerPosition)
  }

  private buildIgnoreSet(ignoreObjects: Array<THREE.Object3D | null | undefined> | undefined): void {
    this.ignoreSet.clear()
    if (!ignoreObjects) return
    for (const obj of ignoreObjects) {
      if (obj) this.ignoreSet.add(obj)
    }
  }

  private isIgnored(object: THREE.Object3D): boolean {
    let current: THREE.Object3D | null = object
    while (current) {
      if (this.ignoreSet.has(current)) return true
      current = current.parent
    }
    return false
  }
}

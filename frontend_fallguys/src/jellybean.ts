import * as THREE from 'three'

const ACCESSORIES = ['none', 'crown', 'propeller', 'headband', 'cone'] as const
type Accessory = typeof ACCESSORIES[number]

export interface JellybeanUserData {
  body: THREE.Mesh
  arms: THREE.Mesh[]
  legs: THREE.Mesh[]
  accMesh: THREE.Object3D | null
  accessory: Accessory
  squashT: number
  squashAmt: number
  prevY: number
  isAnimating: boolean
}

export function createJellybean(color: number, accessory: Accessory = 'none'): THREE.Group {
  const g = new THREE.Group()
  const mat = new THREE.MeshStandardMaterial({ color, roughness: 0.4, metalness: 0.1 })

  // Body capsule
  const body = new THREE.Mesh(new THREE.CapsuleGeometry(0.35, 0.5, 8, 16), mat)
  body.position.y = 0.7
  body.castShadow = true
  g.add(body)

  // Eyes
  const eyeMat = new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.3 })
  const pupilMat = new THREE.MeshStandardMaterial({ color: 0x111111 })
  for (const s of [-1, 1]) {
    const ew = new THREE.Mesh(new THREE.SphereGeometry(0.09, 8, 8), eyeMat)
    ew.position.set(s * 0.13, 0.88, 0.28)
    ew.scale.z = 0.6
    g.add(ew)
    const ep = new THREE.Mesh(new THREE.SphereGeometry(0.05, 6, 6), pupilMat)
    ep.position.set(s * 0.13, 0.88, 0.33)
    g.add(ep)
  }

  // Arms
  const arms: THREE.Mesh[] = []
  for (const s of [-1, 1]) {
    const a = new THREE.Mesh(new THREE.CapsuleGeometry(0.08, 0.2, 4, 8), mat)
    a.position.set(s * 0.45, 0.65, 0)
    a.rotation.z = s * 0.4
    a.castShadow = true
    g.add(a)
    arms.push(a)
  }

  // Legs
  const legs: THREE.Mesh[] = []
  for (const s of [-1, 1]) {
    const l = new THREE.Mesh(new THREE.CapsuleGeometry(0.1, 0.15, 4, 8), mat)
    l.position.set(s * 0.15, 0.18, 0)
    l.castShadow = true
    g.add(l)
    legs.push(l)
  }

  // Accessory
  let accMesh: THREE.Object3D | null = null
  if (accessory === 'crown') {
    accMesh = new THREE.Mesh(
      new THREE.CylinderGeometry(0.15, 0.25, 0.2, 5),
      new THREE.MeshStandardMaterial({ color: 0xffd700, metalness: 0.6, roughness: 0.2 })
    )
    accMesh.position.y = 1.1
    g.add(accMesh)
  } else if (accessory === 'propeller') {
    const propGroup = new THREE.Group()
    propGroup.position.y = 1.15
    const hub = new THREE.Mesh(
      new THREE.SphereGeometry(0.06, 8, 8),
      new THREE.MeshStandardMaterial({ color: 0xff0000 })
    )
    propGroup.add(hub)
    for (const s of [-1, 1]) {
      const blade = new THREE.Mesh(
        new THREE.BoxGeometry(0.4, 0.02, 0.1),
        new THREE.MeshStandardMaterial({ color: s > 0 ? 0x3498db : 0xe74c3c })
      )
      blade.position.x = s * 0.2
      propGroup.add(blade)
    }
    g.add(propGroup)
    accMesh = propGroup
  } else if (accessory === 'headband') {
    accMesh = new THREE.Mesh(
      new THREE.TorusGeometry(0.3, 0.04, 8, 16),
      new THREE.MeshStandardMaterial({ color: 0xff4757 })
    )
    accMesh.position.y = 1.0
    accMesh.rotation.x = Math.PI / 2
    g.add(accMesh)
  } else if (accessory === 'cone') {
    accMesh = new THREE.Mesh(
      new THREE.ConeGeometry(0.2, 0.35, 8),
      new THREE.MeshStandardMaterial({ color: 0x5f27cd })
    )
    accMesh.position.y = 1.15
    g.add(accMesh)
  }

  g.userData = {
    body,
    arms,
    legs,
    accMesh,
    accessory,
    squashT: 0,
    squashAmt: 0,
    prevY: 0,
    isAnimating: false,
  } as JellybeanUserData

  return g
}

export function animateJellybean(
  mesh: THREE.Group,
  speed: number,
  grounded: boolean,
  dt: number,
): void {
  const t = performance.now() * 0.005
  const moveSpeed = 8 // CFG.moveSpeed
  const w = Math.min(speed / moveSpeed, 1) * 0.1
  const ud = mesh.userData as JellybeanUserData
  const { body, arms, legs } = ud

  // Squash-stretch on landing
  if (ud.squashT > 0) {
    ud.squashT -= dt
    const p = ud.squashT / 0.3
    const squash = 1 - ud.squashAmt * 0.4 * Math.sin(p * Math.PI)
    const stretch = 1 + ud.squashAmt * 0.3 * Math.sin(p * Math.PI)
    body.scale.set(stretch, squash, stretch)
  } else {
    body.scale.set(1, 1, 1)
  }

  // Wobble and limb animation
  mesh.rotation.z = Math.sin(t * 2) * w
  mesh.rotation.x = Math.sin(t * 3) * w * 0.5
  body.position.y = 0.7 + (grounded ? Math.abs(Math.sin(t * 3)) * w * 0.3 : 0)
  arms[0].rotation.x = Math.sin(t * 3) * w * 3
  arms[1].rotation.x = -Math.sin(t * 3) * w * 3
  legs[0].rotation.x = -Math.sin(t * 3) * w * 2
  legs[1].rotation.x = Math.sin(t * 3) * w * 2

  // Spin propeller
  if (ud.accessory === 'propeller' && ud.accMesh) {
    ud.accMesh.rotation.y += 0.2
  }
}

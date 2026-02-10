import * as THREE from 'three'

interface Particle {
  mesh: THREE.Mesh
  vel: THREE.Vector3
  rot?: THREE.Vector3
  life: number
  max: number
}

const particles: Particle[] = []

export function spawnDust(scene: THREE.Scene, pos: THREE.Vector3): void {
  for (let i = 0; i < 8; i++) {
    const m = new THREE.Mesh(
      new THREE.SphereGeometry(0.05, 4, 4),
      new THREE.MeshBasicMaterial({ color: 0xdfe6e9, transparent: true, opacity: 0.8 })
    )
    m.position.copy(pos)
    m.position.y += 0.1
    scene.add(m)
    particles.push({
      mesh: m,
      vel: new THREE.Vector3(
        (Math.random() - 0.5) * 3,
        Math.random() * 2,
        (Math.random() - 0.5) * 3,
      ),
      life: 0.5 + Math.random() * 0.5,
      max: 0.8,
    })
  }
}

export function spawnTrail(scene: THREE.Scene, pos: THREE.Vector3): void {
  const m = new THREE.Mesh(
    new THREE.SphereGeometry(0.03, 4, 4),
    new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.5 })
  )
  m.position.copy(pos)
  m.position.x += (Math.random() - 0.5) * 0.3
  m.position.y += 0.05
  m.position.z += (Math.random() - 0.5) * 0.3
  scene.add(m)
  particles.push({ mesh: m, vel: new THREE.Vector3(0, 0.5, 0), life: 0.3, max: 0.3 })
}

export function spawnConfetti(scene: THREE.Scene, pos: THREE.Vector3, n = 200): void {
  const cols = [0xff6b6b, 0xfeca57, 0x48dbfb, 0xff9ff3, 0x54a0ff, 0x5f27cd, 0x00b894, 0xfd79a8]
  for (let i = 0; i < n; i++) {
    const m = new THREE.Mesh(
      new THREE.PlaneGeometry(0.15, 0.25),
      new THREE.MeshBasicMaterial({
        color: cols[i % cols.length],
        transparent: true, opacity: 1,
        side: THREE.DoubleSide,
      })
    )
    m.position.copy(pos)
    m.position.x += (Math.random() - 0.5) * 6
    m.position.z += (Math.random() - 0.5) * 6
    m.position.y += Math.random() * 2
    m.rotation.set(Math.random() * Math.PI, Math.random() * Math.PI, Math.random() * Math.PI)
    scene.add(m)
    particles.push({
      mesh: m,
      vel: new THREE.Vector3(
        (Math.random() - 0.5) * 10,
        6 + Math.random() * 12,
        (Math.random() - 0.5) * 10,
      ),
      rot: new THREE.Vector3(
        (Math.random() - 0.5) * 10,
        (Math.random() - 0.5) * 10,
        (Math.random() - 0.5) * 10,
      ),
      life: 4 + Math.random() * 2,
      max: 6,
    })
  }
}

export function updateParticles(scene: THREE.Scene, dt: number): void {
  for (let i = particles.length - 1; i >= 0; i--) {
    const p = particles[i]
    p.life -= dt
    if (p.life <= 0) {
      scene.remove(p.mesh)
      p.mesh.geometry.dispose()
      ;(p.mesh.material as THREE.Material).dispose()
      particles.splice(i, 1)
      continue
    }
    p.vel.y -= 5 * dt
    p.mesh.position.addScaledVector(p.vel, dt)
    if (p.rot) {
      p.mesh.rotation.x += p.rot.x * dt
      p.mesh.rotation.y += p.rot.y * dt
      p.mesh.rotation.z += p.rot.z * dt
    }
    ;(p.mesh.material as THREE.MeshBasicMaterial).opacity = Math.min(1, p.life / (p.max * 0.3))
  }
}

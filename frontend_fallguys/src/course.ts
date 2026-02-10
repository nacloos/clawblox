import * as THREE from 'three'

const FINISH_Z = 250 * 4
const DECO_COLORS = [0xff6b6b, 0xfeca57, 0x48dbfb, 0xff9ff3, 0x54a0ff]

export interface CourseObjects {
  clouds: Array<{ mesh: THREE.Group; speed: number }>
  decorations: THREE.Mesh[]
  waterMat: THREE.ShaderMaterial
}

export function buildSky(scene: THREE.Scene): void {
  const skyMat = new THREE.ShaderMaterial({
    uniforms: {
      topColor: { value: new THREE.Color(0x87ceeb) },
      bottomColor: { value: new THREE.Color(0xffecd2) },
    },
    vertexShader: `
      varying vec3 vWP;
      void main(){
        vWP = (modelMatrix * vec4(position, 1.0)).xyz;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: `
      uniform vec3 topColor, bottomColor;
      varying vec3 vWP;
      void main(){
        float h = normalize(vWP).y;
        gl_FragColor = vec4(mix(bottomColor, topColor, max(h, 0.0)), 1.0);
      }
    `,
    side: THREE.BackSide,
    depthWrite: false,
  })
  scene.add(new THREE.Mesh(new THREE.SphereGeometry(1500, 32, 32), skyMat))
}

export function buildWater(scene: THREE.Scene): THREE.ShaderMaterial {
  const waterMat = new THREE.ShaderMaterial({
    uniforms: {
      time: { value: 0 },
      color1: { value: new THREE.Color(0x48dbfb) },
      color2: { value: new THREE.Color(0x0abde3) },
    },
    vertexShader: `
      uniform float time;
      varying vec2 vUv;
      void main(){
        vUv = uv;
        vec3 p = position;
        p.z += sin(p.x * 0.3 + time * 2.0) * 0.5 + cos(p.y * 0.2 + time * 1.5) * 0.3;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(p, 1.0);
      }
    `,
    fragmentShader: `
      uniform vec3 color1, color2;
      uniform float time;
      varying vec2 vUv;
      void main(){
        float pattern = sin(vUv.x * 20.0 + time * 3.0) * 0.5 + 0.5;
        pattern *= sin(vUv.y * 15.0 + time * 2.0) * 0.5 + 0.5;
        vec3 col = mix(color1, color2, pattern);
        gl_FragColor = vec4(col, 0.7);
      }
    `,
    transparent: true,
    side: THREE.DoubleSide,
  })
  const waterPlane = new THREE.Mesh(new THREE.PlaneGeometry(2000, 2000), waterMat)
  waterPlane.rotation.x = -Math.PI / 2
  waterPlane.position.set(0, -80, 500)
  scene.add(waterPlane)
  return waterMat
}

export function buildClouds(scene: THREE.Scene): Array<{ mesh: THREE.Group; speed: number }> {
  const cloudList: Array<{ mesh: THREE.Group; speed: number }> = []
  const cloudMat = new THREE.MeshStandardMaterial({
    color: 0xffffff, transparent: true, opacity: 0.8, roughness: 1,
  })
  for (let i = 0; i < 20; i++) {
    const cg = new THREE.Group()
    for (let j = 0; j < 3 + Math.floor(Math.random() * 4); j++) {
      const p = new THREE.Mesh(new THREE.SphereGeometry(6 + Math.random() * 10, 8, 8), cloudMat)
      p.position.set(j * 8 - 16, Math.random() * 5, Math.random() * 6 - 3)
      p.scale.y = 0.6
      cg.add(p)
    }
    cg.position.set(Math.random() * 500 - 250, 100 + Math.random() * 80, Math.random() * 1200 - 100)
    scene.add(cg)
    cloudList.push({ mesh: cg, speed: 0.5 + Math.random() * 1.5 })
  }
  return cloudList
}

export function buildDecorations(scene: THREE.Scene): THREE.Mesh[] {
  const decorations: THREE.Mesh[] = []
  const decoGeos = [
    new THREE.IcosahedronGeometry(5),
    new THREE.OctahedronGeometry(4),
    new THREE.TorusGeometry(3.5, 1.2, 8, 16),
  ]
  for (let i = 0; i < 15; i++) {
    const dm = new THREE.MeshStandardMaterial({
      color: DECO_COLORS[i % DECO_COLORS.length],
      roughness: 0.4, metalness: 0.2,
      transparent: true, opacity: 0.7,
    })
    const d = new THREE.Mesh(decoGeos[i % decoGeos.length], dm)
    d.position.set(
      (Math.random() > 0.5 ? 1 : -1) * (50 + Math.random() * 40),
      20 + Math.random() * 40,
      Math.random() * FINISH_Z,
    )
    d.rotation.set(Math.random() * Math.PI, Math.random() * Math.PI, 0)
    d.userData.rx = 0.005 + Math.random() * 0.01
    d.userData.ry = 0.005 + Math.random() * 0.01
    d.userData.fo = Math.random() * Math.PI * 2
    d.userData.baseY = d.position.y
    scene.add(d)
    decorations.push(d)
  }
  return decorations
}

export function updateDecorations(decorations: THREE.Mesh[], time: number): void {
  for (const d of decorations) {
    d.rotation.x += d.userData.rx as number
    d.rotation.y += d.userData.ry as number
    d.position.y = (d.userData.baseY as number) + Math.sin(time * 2 + (d.userData.fo as number)) * 0.5
  }
}

export function updateClouds(
  clouds: Array<{ mesh: THREE.Group; speed: number }>,
  dt: number,
): void {
  for (const c of clouds) {
    c.mesh.position.x += c.speed * dt
    if (c.mesh.position.x > 140) c.mesh.position.x = -140
  }
}

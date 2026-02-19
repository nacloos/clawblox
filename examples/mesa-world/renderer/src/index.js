import * as THREE from 'https://esm.sh/three@0.160.0'

function rotationToQuaternion(rot) {
  const m = new THREE.Matrix4()
  m.set(
    rot[0][0], rot[0][1], rot[0][2], 0,
    rot[1][0], rot[1][1], rot[1][2], 0,
    rot[2][0], rot[2][1], rot[2][2], 0,
    0, 0, 0, 1,
  )
  return new THREE.Quaternion().setFromRotationMatrix(m)
}

function resolveEntityModelUrl(entity) {
  if (typeof entity.model_url === 'string' && entity.model_url.length > 0) {
    return entity.model_url
  }
  const attrs = entity.attributes
  if (attrs && typeof attrs.ModelUrl === 'string' && attrs.ModelUrl.length > 0) {
    return attrs.ModelUrl
  }
  return null
}

function buildSky(scene) {
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

function buildClouds(scene) {
  const cloudList = []
  const cloudMat = new THREE.MeshStandardMaterial({
    color: 0xffffff,
    transparent: true,
    opacity: 0.8,
    roughness: 1,
  })

  for (let i = 0; i < 20; i++) {
    const cg = new THREE.Group()
    for (let j = 0; j < 3 + Math.floor(Math.random() * 4); j++) {
      const p = new THREE.Mesh(new THREE.SphereGeometry(6 + Math.random() * 10, 8, 8), cloudMat)
      p.position.set(j * 8 - 16, Math.random() * 5, Math.random() * 6 - 3)
      p.scale.y = 0.6
      cg.add(p)
    }
    cg.position.set(Math.random() * 500 - 250, 100 + Math.random() * 80, Math.random() * 300 - 150)
    scene.add(cg)
    cloudList.push({ mesh: cg, speed: 0.5 + Math.random() * 1.5 })
  }

  return cloudList
}

function updateClouds(clouds, dt) {
  for (const c of clouds) {
    c.mesh.position.x += c.speed * dt
    if (c.mesh.position.x > 250) c.mesh.position.x = -250
  }
}

function createEntityVisual(entity) {
  const role = (entity.render && entity.render.role) || entity.name || ''
  const size = entity.size || [1, 1, 1]
  const modelUrl = resolveEntityModelUrl(entity)

  if (modelUrl) {
    const root = new THREE.Group()
    root.userData.isModelEntity = true
    root.userData.modelUrl = modelUrl
    const fallback = new THREE.Mesh(
      new THREE.CapsuleGeometry(0.35, 0.5, 8, 16),
      new THREE.MeshStandardMaterial({ color: 0xffa07a, roughness: 0.5, metalness: 0.05 }),
    )
    fallback.castShadow = true
    fallback.receiveShadow = true
    const scale = (size[1] || 5) / 1.3
    fallback.scale.setScalar(scale)
    root.add(fallback)
    return root
  }

  if (role === 'Ground') {
    const group = new THREE.Group()
    const mesh = new THREE.Mesh(
      new THREE.BoxGeometry(size[0], size[1], size[2]),
      new THREE.MeshStandardMaterial({ color: 0x55efC4, roughness: 0.5, metalness: 0.2 }),
    )
    mesh.receiveShadow = true
    mesh.castShadow = true
    group.add(mesh)
    return group
  }

  if (role === 'Mesa') {
    const group = new THREE.Group()
    const w = size[0], h = size[1], d = size[2]

    const layers = [
      { frac: 0.0,  hFrac: 0.40, color: 0x8b6940 },
      { frac: 0.40, hFrac: 0.40, color: 0xa57d50 },
      { frac: 0.80, hFrac: 0.20, color: 0x55cfb0 },
    ]

    for (const layer of layers) {
      const lh = h * layer.hFrac
      const ly = -h / 2 + h * layer.frac + lh / 2
      const mesh = new THREE.Mesh(
        new THREE.BoxGeometry(w, lh, d),
        new THREE.MeshStandardMaterial({ color: layer.color, roughness: 0.9, metalness: 0.0 }),
      )
      mesh.position.y = ly
      mesh.castShadow = true
      mesh.receiveShadow = true
      group.add(mesh)
    }

    return group
  }

  // Default: box with entity color
  const color = entity.render && entity.render.color
    ? (Math.round(entity.render.color[0] * 255) << 16) |
      (Math.round(entity.render.color[1] * 255) << 8) |
      Math.round(entity.render.color[2] * 255)
    : 0x999999

  let geometry
  const primitive = entity.render && entity.render.primitive
  if (primitive === 'cylinder') geometry = new THREE.CylinderGeometry(size[0] / 2, size[0] / 2, size[1], 16)
  else if (primitive === 'sphere') geometry = new THREE.SphereGeometry(size[0] / 2, 16, 16)
  else geometry = new THREE.BoxGeometry(size[0], size[1], size[2])

  const mesh = new THREE.Mesh(geometry, new THREE.MeshStandardMaterial({ color, roughness: 0.5, metalness: 0.2 }))
  mesh.castShadow = true
  mesh.receiveShadow = true
  return mesh
}

export function createRenderer(ctx) {
  const scene = new THREE.Scene()

  const camera = new THREE.PerspectiveCamera(60, 1, 0.1, 3000)

  const renderer = new THREE.WebGLRenderer({ canvas: ctx.canvas, antialias: true })
  renderer.shadowMap.enabled = true
  renderer.shadowMap.type = THREE.PCFSoftShadowMap
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  renderer.toneMappingExposure = 1.2
  renderer.outputColorSpace = THREE.SRGBColorSpace

  // Lights (matching fall-guys)
  scene.add(new THREE.AmbientLight(0xffffff, 0.6))
  scene.add(new THREE.HemisphereLight(0x87ceeb, 0x98fb98, 0.4))

  const dirLight = new THREE.DirectionalLight(0xffffff, 1.2)
  dirLight.position.set(30, 50, 20)
  dirLight.castShadow = true
  dirLight.shadow.mapSize.set(2048, 2048)
  dirLight.shadow.camera.left = -80
  dirLight.shadow.camera.right = 80
  dirLight.shadow.camera.top = 80
  dirLight.shadow.camera.bottom = -80
  dirLight.shadow.camera.near = 1
  dirLight.shadow.camera.far = 200
  dirLight.shadow.bias = -0.001
  scene.add(dirLight)
  scene.add(dirLight.target)

  // Sky and clouds
  buildSky(scene)
  const clouds = buildClouds(scene)

  const stateBuffer = ctx.runtime.state.createSnapshotBuffer({ maxSnapshots: 8, interpolationDelayMs: 100 })
  const entityObjects = new Map()
  const modelController = ctx.runtime.three.createModelEntityController(THREE, {
    onError(err, meta) {
      ctx.log('warn', 'Model entity load failed', { err: String(err), ...meta })
    },
  })

  let followTargetPos = new THREE.Vector3(0, 4, 0)
  let camPos = new THREE.Vector3(0, 15, -20)
  let camTarget = new THREE.Vector3(0, 2, 0)
  const clock = new THREE.Clock()

  function updateCamera() {
    const tp = followTargetPos
    const ideal = new THREE.Vector3(tp.x, tp.y + 12, tp.z - 20)
    camPos.lerp(ideal, 0.05)
    camera.position.copy(camPos)
    camTarget.lerp(tp.clone().add(new THREE.Vector3(0, 2, 0)), 0.08)
    camera.lookAt(camTarget)
    dirLight.position.set(tp.x + 30, 50, tp.z + 20)
    dirLight.target.position.copy(tp)
  }

  function updateScene(obs, dt) {
    const activeIds = new Set()
    const stateByRootPartId = new Map()
    for (const player of obs.players || []) {
      if (typeof player.root_part_id === 'number' && typeof player.humanoid_state === 'string') {
        stateByRootPartId.set(player.root_part_id, player.humanoid_state)
      }
    }

    for (const entity of obs.entities || []) {
      activeIds.add(entity.id)
      let obj = entityObjects.get(entity.id)
      if (!obj) {
        obj = createEntityVisual(entity)
        entityObjects.set(entity.id, obj)
        scene.add(obj)
      }

      obj.position.set(entity.position[0], entity.position[1], entity.position[2])
      if (entity.rotation) obj.quaternion.copy(rotationToQuaternion(entity.rotation))
      obj.visible = !(entity.render && entity.render.visible === false)

      const prev = obj.userData.prevPos
      let speed = 0
      if (prev) {
        const dx = entity.position[0] - prev[0]
        const dz = entity.position[2] - prev[2]
        speed = Math.sqrt(dx * dx + dz * dz) / Math.max(dt, 0.001)
      }
      obj.userData.prevPos = [entity.position[0], entity.position[1], entity.position[2]]

      if (obj.userData && obj.userData.isModelEntity) {
        const modelUrl = resolveEntityModelUrl(entity)
        if (modelUrl) {
          void modelController.upsert({
            entityId: entity.id,
            root: obj,
            modelUrl,
            size: entity.size || [2, 5, 2],
            yawOffsetDeg: entity.model_yaw_offset_deg,
          })
          modelController.update(entity.id, {
            dt,
            speed,
            humanoidState: stateByRootPartId.get(entity.id) || null,
          })
        }
      }
    }

    for (const [id, obj] of entityObjects.entries()) {
      if (!activeIds.has(id)) {
        scene.remove(obj)
        modelController.remove(id)
        entityObjects.delete(id)
      }
    }

    const players = obs.players || []
    if (players.length > 0) {
      const target = players[0]
      followTargetPos.set(target.position[0], target.position[1], target.position[2])
    }
  }

  return {
    onResize({ width, height }) {
      camera.aspect = width / Math.max(1, height)
      camera.updateProjectionMatrix()
      renderer.setSize(width, height, false)
    },

    onState(state) {
      stateBuffer.push(state)
      const obs = stateBuffer.interpolated() || state
      const dt = Math.min(clock.getDelta(), 0.05)
      updateClouds(clouds, dt)
      updateScene(obs, dt)
      updateCamera()
      renderer.render(scene, camera)
    },

    unmount() {
      for (const obj of entityObjects.values()) scene.remove(obj)
      entityObjects.clear()
      modelController.clear()
      renderer.dispose()
    },
  }
}

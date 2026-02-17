function clamp(v, min, max) {
  return Math.max(min, Math.min(max, v))
}

function toColorHex(render, fallback = '#4f46e5') {
  const c = render?.color
  if (!Array.isArray(c) || c.length !== 3) return fallback
  const r = Math.round(clamp(c[0], 0, 1) * 255)
  const g = Math.round(clamp(c[1], 0, 1) * 255)
  const b = Math.round(clamp(c[2], 0, 1) * 255)
  return `rgb(${r}, ${g}, ${b})`
}

function defaultRoughness(material) {
  switch (material) {
    case 'Metal': return 0.22
    case 'Concrete': return 0.72
    case 'Slate': return 0.82
    case 'Glass': return 0.1
    case 'Wood': return 0.85
    case 'Neon': return 0.2
    default: return 0.58
  }
}

function defaultMetalness(material) {
  switch (material) {
    case 'Metal': return 0.92
    case 'Concrete': return 0.22
    case 'Slate': return 0.12
    case 'Glass': return 0.1
    case 'Wood': return 0.06
    case 'Neon': return 0.35
    default: return 0.25
  }
}

export function createPresetMaterialLibrary(initial = {}) {
  const presets = new Map(Object.entries(initial))

  return {
    register(id, preset) {
      if (!id) return
      presets.set(String(id), preset || {})
    },
    get(id) {
      if (!id) return null
      return presets.get(String(id)) || null
    },
    resolve(render) {
      const presetId = render?.preset_id
      if (!presetId) return null
      return presets.get(String(presetId)) || null
    },
    entries() {
      return [...presets.entries()]
    },
  }
}

export function materialFromRender(THREE, render, presetLib = null) {
  const preset = presetLib?.resolve ? presetLib.resolve(render) : null
  const color = new THREE.Color(toColorHex(render, '#8b5cf6'))
  const materialName = render?.material || 'Plastic'
  const emissiveIntensity =
    preset?.emissiveIntensity ?? (materialName === 'Neon' ? 0.9 : 0)

  return new THREE.MeshStandardMaterial({
    color,
    roughness: preset?.roughness ?? defaultRoughness(materialName),
    metalness: preset?.metalness ?? defaultMetalness(materialName),
    emissive: emissiveIntensity > 0 ? color : new THREE.Color(0x000000),
    emissiveIntensity,
    transparent: typeof render?.transparency === 'number' && render.transparency > 0,
    opacity: typeof render?.transparency === 'number' ? Math.max(0, 1 - render.transparency) : 1,
    side: render?.double_sided ? THREE.DoubleSide : THREE.FrontSide,
  })
}

export function geometryFromRender(THREE, render, size = [1, 1, 1]) {
  const sx = Math.max(0.001, size?.[0] ?? 1)
  const sy = Math.max(0.001, size?.[1] ?? 1)
  const sz = Math.max(0.001, size?.[2] ?? 1)
  const primitive = String(render?.primitive || 'box').toLowerCase()

  switch (primitive) {
    case 'ball':
    case 'sphere':
      return new THREE.SphereGeometry(Math.max(0.001, sx * 0.5), 18, 16)
    case 'cylinder': {
      const radius = Math.max(0.001, Math.max(sx, sz) * 0.5)
      return new THREE.CylinderGeometry(radius, radius, sy, 16)
    }
    case 'wedge':
    case 'block':
    case 'box':
    default:
      return new THREE.BoxGeometry(sx, sy, sz)
  }
}

export function applyRendererPreset(THREE, renderer, preset = 'balanced') {
  if (!renderer) return
  renderer.shadowMap.enabled = true
  renderer.shadowMap.type = THREE.PCFSoftShadowMap
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  renderer.outputColorSpace = THREE.SRGBColorSpace

  if (preset === 'cinematic') {
    renderer.toneMappingExposure = 1.35
  } else if (preset === 'performance') {
    renderer.toneMappingExposure = 1.0
    renderer.shadowMap.type = THREE.BasicShadowMap
  } else {
    renderer.toneMappingExposure = 1.15
  }
}

export function applyLightingPreset(THREE, scene, preset = 'arena') {
  const lights = []

  const ambient = new THREE.AmbientLight(0xffffff, preset === 'cinematic' ? 0.35 : 0.6)
  lights.push(ambient)

  const hemi = new THREE.HemisphereLight(0x87ceeb, 0x1c2a3a, preset === 'cinematic' ? 0.25 : 0.4)
  lights.push(hemi)

  const dir = new THREE.DirectionalLight(0xffffff, preset === 'cinematic' ? 1.35 : 1.0)
  dir.position.set(20, 30, 12)
  dir.castShadow = true
  dir.shadow.mapSize.set(2048, 2048)
  dir.shadow.camera.left = -70
  dir.shadow.camera.right = 70
  dir.shadow.camera.top = 70
  dir.shadow.camera.bottom = -70
  dir.shadow.camera.near = 0.5
  dir.shadow.camera.far = 220
  dir.shadow.bias = -0.001
  lights.push(dir)

  for (const l of lights) scene.add(l)
  return lights
}

export function disposeObject3D(root) {
  if (!root) return
  root.traverse((node) => {
    if (node.geometry?.dispose) node.geometry.dispose()
    const mat = node.material
    if (!mat) return
    if (Array.isArray(mat)) {
      for (const m of mat) if (m?.dispose) m.dispose()
    } else if (mat.dispose) {
      mat.dispose()
    }
  })
}

export function createEntityStore(scene, options = {}) {
  const map = new Map()
  const dispose = options.dispose || disposeObject3D

  function upsert(entity, createFn, updateFn) {
    if (!entity) return null
    let object = map.get(entity.id)
    if (!object) {
      object = createFn(entity)
      if (!object) return null
      map.set(entity.id, object)
      if (scene && object.parent !== scene) scene.add(object)
    }
    if (updateFn) updateFn(object, entity)
    return object
  }

  function remove(id) {
    const object = map.get(id)
    if (!object) return
    if (object.parent) object.parent.remove(object)
    dispose(object)
    map.delete(id)
  }

  function prune(activeIds) {
    for (const id of [...map.keys()]) {
      if (!activeIds.has(id)) remove(id)
    }
  }

  function clear() {
    for (const id of [...map.keys()]) remove(id)
  }

  return {
    upsert,
    remove,
    prune,
    clear,
    get(id) {
      return map.get(id) || null
    },
    entries() {
      return [...map.entries()]
    },
    size() {
      return map.size
    },
  }
}

export function createFollowCameraController(THREE, camera, options = {}) {
  const cfg = {
    followDistance: options.followDistance ?? 7.5,
    followHeight: options.followHeight ?? 1.5,
    shoulderOffset: options.shoulderOffset ?? 0.8,
    positionSmoothing: options.positionSmoothing ?? 10,
    rotationSmoothing: options.rotationSmoothing ?? 14,
    minDistance: options.minDistance ?? 3.5,
    lookAheadDistance: options.lookAheadDistance ?? 10,
  }

  const tmpForward = new THREE.Vector3()
  const tmpRight = new THREE.Vector3()
  const desiredPos = new THREE.Vector3()
  const desiredLook = new THREE.Vector3()
  const lookMatrix = new THREE.Matrix4()
  const targetQuat = new THREE.Quaternion()

  function update(targetPosition, targetForward, dt, collisionAdjust = null) {
    if (!targetPosition) return

    const dtSafe = Math.max(1 / 240, Math.min(0.1, dt || 1 / 60))
    const posAlpha = 1 - Math.exp(-cfg.positionSmoothing * dtSafe)
    const rotAlpha = 1 - Math.exp(-cfg.rotationSmoothing * dtSafe)

    tmpForward.copy(targetForward || new THREE.Vector3(0, 0, 1)).setY(0)
    if (tmpForward.lengthSq() < 1e-5) tmpForward.set(0, 0, 1)
    tmpForward.normalize()

    tmpRight.set(-tmpForward.z, 0, tmpForward.x).normalize()

    desiredPos
      .copy(targetPosition)
      .addScaledVector(tmpForward, -Math.max(cfg.minDistance, cfg.followDistance))
      .addScaledVector(tmpRight, cfg.shoulderOffset)
    desiredPos.y += cfg.followHeight

    if (typeof collisionAdjust === 'function') {
      const adjusted = collisionAdjust(desiredPos.clone(), targetPosition.clone())
      if (adjusted) desiredPos.copy(adjusted)
    }

    desiredLook
      .copy(targetPosition)
      .addScaledVector(tmpForward, cfg.lookAheadDistance * 0.25)
    desiredLook.y += cfg.followHeight * 0.5

    camera.position.lerp(desiredPos, posAlpha)
    lookMatrix.lookAt(camera.position, desiredLook, new THREE.Vector3(0, 1, 0))
    targetQuat.setFromRotationMatrix(lookMatrix)
    camera.quaternion.slerp(targetQuat, rotAlpha)
  }

  return {
    update,
    config: cfg,
  }
}

export function createCameraModeController(THREE, camera, options = {}) {
  const followCtl = createFollowCameraController(THREE, camera, options.follow || {})
  const tmpForward = new THREE.Vector3(0, 0, 1)

  function update({ mode = 'follow', targetPosition, targetForward, dt = 1 / 60, collisionAdjust = null }) {
    if (!targetPosition) return

    if (mode === 'first_person') {
      const pos = targetPosition.clone()
      pos.y += options.firstPersonHeight ?? 1.6
      camera.position.copy(pos)

      const lookTarget = pos.clone().add(targetForward || tmpForward)
      camera.lookAt(lookTarget)
      return
    }

    if (mode === 'spectator') {
      const offset = options.spectatorOffset || new THREE.Vector3(0, 18, -18)
      const target = targetPosition.clone().add(offset)
      camera.position.lerp(target, 1 - Math.exp(-6 * dt))
      camera.lookAt(targetPosition)
      return
    }

    followCtl.update(targetPosition, targetForward, dt, collisionAdjust)
  }

  return {
    update,
    follow: followCtl,
  }
}

export function buildEntityMesh(THREE, entity, presetLib = null) {
  const render = entity?.render || {}
  const size = entity?.size || [1, 1, 1]
  const mesh = new THREE.Mesh(
    geometryFromRender(THREE, render, size),
    materialFromRender(THREE, render, presetLib),
  )

  mesh.castShadow = render?.casts_shadow !== false
  mesh.receiveShadow = render?.receives_shadow !== false
  mesh.visible = render?.visible !== false
  return mesh
}

export function applyEntityTransform(THREE, object3d, entity) {
  if (!object3d || !entity) return
  const p = entity.position || [0, 0, 0]
  object3d.position.set(p[0] || 0, p[1] || 0, p[2] || 0)

  if (entity.rotation && Array.isArray(entity.rotation) && entity.rotation.length === 3) {
    const r = entity.rotation
    const m = new THREE.Matrix4()
    m.set(
      r[0][0], r[0][1], r[0][2], 0,
      r[1][0], r[1][1], r[1][2], 0,
      r[2][0], r[2][1], r[2][2], 0,
      0, 0, 0, 1,
    )
    object3d.quaternion.setFromRotationMatrix(m)
  }
}

export function createModelTemplateCache() {
  const templates = new Map()

  return {
    async get(url, loadFn) {
      let pending = templates.get(url)
      if (!pending) {
        pending = Promise.resolve(loadFn(url))
        templates.set(url, pending)
      }
      return pending
    },
    clear() {
      templates.clear()
    },
    size() {
      return templates.size
    },
  }
}

function fitModelToSize(THREE, model, size = [1, 1, 1]) {
  let mesh = null
  model.traverse((obj) => {
    if (mesh) return
    if (obj?.isSkinnedMesh || obj?.isMesh) mesh = obj
  })

  const sourceBox = new THREE.Box3()
  if (mesh?.geometry) {
    mesh.geometry.computeBoundingBox()
    const bb = mesh.geometry.boundingBox
    if (!bb) return
    sourceBox.copy(bb)
  } else {
    sourceBox.setFromObject(model)
  }

  const src = sourceBox.getSize(new THREE.Vector3())
  if (src.y <= 0.0001) return
  const center = sourceBox.getCenter(new THREE.Vector3())
  const scale = (size?.[1] ?? 1) / src.y
  model.scale.setScalar(scale)
  model.position.set(-center.x * scale, -center.y * scale, -center.z * scale)
}

export function createModelEntityController(THREE, options = {}) {
  const templateCache = createModelTemplateCache()
  const modelStates = new Map()
  const moduleCache = { loader: null, cloneFn: null, pending: null }

  async function ensureModelModules() {
    if (moduleCache.loader && moduleCache.cloneFn) return moduleCache
    if (!moduleCache.pending) {
      moduleCache.pending = Promise.all([
        import('https://esm.sh/three@0.160.0/examples/jsm/loaders/GLTFLoader.js'),
        import('https://esm.sh/three@0.160.0/examples/jsm/utils/SkeletonUtils.js'),
      ]).then(([loaderMod, skeletonMod]) => {
        moduleCache.loader = new loaderMod.GLTFLoader()
        moduleCache.cloneFn = skeletonMod.clone
        return moduleCache
      })
    }
    return moduleCache.pending
  }

  async function loadTemplate(modelUrl) {
    const mods = await ensureModelModules()
    return templateCache.get(modelUrl, () => new Promise((resolve, reject) => {
      mods.loader.load(
        modelUrl,
        (gltf) => resolve({ scene: gltf.scene, animations: gltf.animations || [] }),
        undefined,
        reject,
      )
    }))
  }

  function chooseClip(animations, pattern, fallbackIndex = -1) {
    const found = animations.find((c) => pattern.test(c.name))
    if (found) return found
    if (fallbackIndex >= 0 && animations.length > fallbackIndex) return animations[fallbackIndex]
    return null
  }

  async function upsert({ entityId, root, modelUrl, size, yawOffsetDeg }) {
    if (!entityId || !root || !modelUrl) return

    let state = modelStates.get(entityId)
    if (state?.modelUrl === modelUrl && state.ready) return

    if (!state) {
      state = {
        modelUrl,
        ready: false,
        loading: false,
        mixer: null,
        walkAction: null,
        idleAction: null,
      }
      modelStates.set(entityId, state)
    } else {
      state.modelUrl = modelUrl
    }

    if (state.loading) return
    state.loading = true

    try {
      const template = await loadTemplate(modelUrl)
      const mods = await ensureModelModules()
      const current = modelStates.get(entityId)
      if (!current || current.modelUrl !== modelUrl) return

      for (const child of [...root.children]) root.remove(child)

      const clone = mods.cloneFn(template.scene)
      fitModelToSize(THREE, clone, size)
      if (yawOffsetDeg) clone.rotation.y = THREE.MathUtils.degToRad(yawOffsetDeg)
      clone.traverse((node) => {
        if (node?.isMesh) {
          node.castShadow = true
          node.receiveShadow = true
        }
      })
      root.add(clone)

      current.mixer = null
      current.walkAction = null
      current.idleAction = null
      if (template.animations.length > 0) {
        const mixer = new THREE.AnimationMixer(clone)
        const walkClip = chooseClip(template.animations, /(walk|run|jog|locomotion)/i, 0)
        const idleClip = chooseClip(template.animations, /idle/i, -1)
        const walkAction = walkClip ? mixer.clipAction(walkClip) : null
        const idleAction = idleClip ? mixer.clipAction(idleClip) : null

        if (idleAction) {
          idleAction.reset()
          idleAction.play()
        }
        if (walkAction) {
          walkAction.reset()
          walkAction.play()
          walkAction.setEffectiveWeight(0)
        }

        current.mixer = mixer
        current.walkAction = walkAction
        current.idleAction = idleAction
      }
      current.ready = true
    } catch (err) {
      if (options.onError) options.onError(err, { entityId, modelUrl })
    } finally {
      const cur = modelStates.get(entityId)
      if (cur) cur.loading = false
    }
  }

  function update(entityId, { dt = 1 / 60, speed = 0, humanoidState = null } = {}) {
    const state = modelStates.get(entityId)
    if (!state?.mixer) return

    const airborne = humanoidState === 'Jumping' || humanoidState === 'Freefall'
    const moving = speed > 0.5 && !airborne
    if (state.walkAction) {
      const targetWeight = moving ? 1 : 0
      const current = state.walkAction.getEffectiveWeight()
      state.walkAction.setEffectiveWeight(current + (targetWeight - current) * Math.min(1, dt * 8))
      if (moving) {
        state.walkAction.paused = false
        state.walkAction.setEffectiveTimeScale(Math.min(speed / 6, 2))
      }
    }
    if (state.idleAction) {
      const targetWeight = moving ? 0 : 1
      const current = state.idleAction.getEffectiveWeight()
      state.idleAction.setEffectiveWeight(current + (targetWeight - current) * Math.min(1, dt * 8))
    }
    state.mixer.update(dt)
  }

  function remove(entityId) {
    modelStates.delete(entityId)
  }

  function clear() {
    modelStates.clear()
    templateCache.clear()
  }

  return {
    upsert,
    update,
    remove,
    clear,
  }
}

export function classifyAnimationTracks(player) {
  const tracks = Array.isArray(player?.active_animations) ? player.active_animations : []

  function best(regex) {
    let chosen = null
    let bestWeight = -1
    for (const track of tracks) {
      if (!track || track.is_playing === false) continue
      const id = String(track.animation_id || '')
      if (!regex.test(id)) continue
      const weight = Number(track.effective_weight ?? track.weight_current ?? 1)
      if (weight > bestWeight) {
        bestWeight = weight
        chosen = track
      }
    }
    return chosen
  }

  return {
    fire: best(/fire/i),
    reload: best(/reload/i),
    walk: best(/(walk|run|jog|locomotion)/i),
    idle: best(/idle/i),
    all: tracks,
  }
}

export const defaults = {
  defaultRoughness,
  defaultMetalness,
  toColorHex,
}

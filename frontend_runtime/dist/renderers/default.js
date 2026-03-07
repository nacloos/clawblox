export function createRenderer(ctx) {
  const canvas = ctx.canvas
  const g = canvas.getContext('2d')
  const stateBuffer = ctx.runtime.state.createSnapshotBuffer({
    maxSnapshots: 10,
    interpolationDelayMs: 90,
  })

  const presets = ctx.runtime.presets.createPresetRegistry({
    'fps_arena/floor': { color: '#334155' },
    'fps_arena/wall': { color: '#475569' },
    'fps_arena/spawn': { color: '#22c55e' },
    'fall_guys/disc': { color: '#38bdf8' },
    'fall_guys/pendulum': { color: '#f97316' },
  })

  const firePattern = /(^|[:/_-])fire([:_-]|$)/i
  const FREE_ZOOM_MIN = 0.35
  const FREE_ZOOM_MAX = 4
  const FREE_WHEEL_ZOOM_SPEED = 0.0015
  const FREE_KEYBOARD_PAN_SPEED = 26
  const FREE_TOUCH_PAN_MULTIPLIER = 1
  const DEBUG_CAMERA = true
  const freeCamera = {
    centerX: 0,
    centerZ: 0,
    zoom: 1,
    seeded: false,
    mounted: false,
    keys: new Set(),
    pointers: new Map(),
    lastPointerCenter: null,
    pinchStartDistance: null,
    pinchStartZoom: null,
    lastFrameTs: (typeof performance !== 'undefined' ? performance.now() : Date.now()),
    rafId: null,
  }
  let latestState = null

  function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value))
  }

  function debugLog(...args) {
    if (!DEBUG_CAMERA) return
    console.log('[renderer-camera]', ...args)
  }

  function drawBackground() {
    g.fillStyle = '#0b1020'
    g.fillRect(0, 0, canvas.width, canvas.height)
    g.strokeStyle = 'rgba(141, 152, 198, 0.2)'
    g.lineWidth = 1
    for (let x = 0; x < canvas.width; x += 40) {
      g.beginPath()
      g.moveTo(x, 0)
      g.lineTo(x, canvas.height)
      g.stroke()
    }
    for (let y = 0; y < canvas.height; y += 40) {
      g.beginPath()
      g.moveTo(0, y)
      g.lineTo(canvas.width, y)
      g.stroke()
    }
  }

  function worldToScreen(x, z, scale, centerX, centerZ) {
    return {
      x: canvas.width * 0.5 + (x - centerX) * scale,
      y: canvas.height * 0.5 + (z - centerZ) * scale,
    }
  }

  function computeBounds(players, entities) {
    let minX = -20
    let maxX = 20
    let minZ = -20
    let maxZ = 20

    for (const e of entities) {
      if (!Array.isArray(e.position)) continue
      minX = Math.min(minX, e.position[0])
      maxX = Math.max(maxX, e.position[0])
      minZ = Math.min(minZ, e.position[2])
      maxZ = Math.max(maxZ, e.position[2])
    }
    for (const p of players) {
      if (!Array.isArray(p.position)) continue
      minX = Math.min(minX, p.position[0])
      maxX = Math.max(maxX, p.position[0])
      minZ = Math.min(minZ, p.position[2])
      maxZ = Math.max(maxZ, p.position[2])
    }

    return { minX, maxX, minZ, maxZ }
  }

  function animationBoost(player) {
    const track = ctx.runtime.animation.findTrack(player, (id) => firePattern.test(id))
    if (!track) return 0
    return 1
  }

  function computeFitScale(bounds) {
    const worldW = Math.max(1, bounds.maxX - bounds.minX)
    const worldH = Math.max(1, bounds.maxZ - bounds.minZ)
    const pad = 30
    return Math.min((canvas.width - pad * 2) / worldW, (canvas.height - pad * 2) / worldH)
  }

  function seedFreeCamera(bounds) {
    if (freeCamera.seeded) return
    freeCamera.centerX = (bounds.minX + bounds.maxX) * 0.5
    freeCamera.centerZ = (bounds.minZ + bounds.maxZ) * 0.5
    freeCamera.zoom = 1
    freeCamera.seeded = true
    debugLog('seed free camera', {
      centerX: freeCamera.centerX,
      centerZ: freeCamera.centerZ,
      bounds,
    })
  }

  function clampFreeCameraToBounds(bounds, scale) {
    if (!bounds) return
    // Match clawblox-cloud behavior more closely: clamp the pan target to world bounds,
    // not the viewport extents, so panning still works while zoomed far out.
    freeCamera.centerX = clamp(freeCamera.centerX, bounds.minX, bounds.maxX)
    freeCamera.centerZ = clamp(freeCamera.centerZ, bounds.minZ, bounds.maxZ)
  }

  function getSelectedPlayer(state, players) {
    const selectedId = state?.selected_player_id
    if (!selectedId) return null
    return players.find((p) => String(p?.id || '') === String(selectedId)) || null
  }

  function shouldIgnoreKeyboardEvent(target) {
    if (!(target instanceof HTMLElement)) return false
    const tag = String(target.tagName || '').toLowerCase()
    return target.isContentEditable || tag === 'input' || tag === 'textarea' || tag === 'select'
  }

  function pointerCenter() {
    if (!freeCamera.pointers.size) return null
    let sx = 0
    let sy = 0
    for (const p of freeCamera.pointers.values()) {
      sx += p.x
      sy += p.y
    }
    return { x: sx / freeCamera.pointers.size, y: sy / freeCamera.pointers.size }
  }

  function pointerDistance() {
    if (freeCamera.pointers.size < 2) return null
    const pts = [...freeCamera.pointers.values()]
    const dx = pts[0].x - pts[1].x
    const dy = pts[0].y - pts[1].y
    return Math.hypot(dx, dy)
  }

  function panFreeCameraByScreenDelta(dx, dy, scale, bounds) {
    if (!Number.isFinite(scale) || scale <= 0) return
    const before = { x: freeCamera.centerX, z: freeCamera.centerZ }
    freeCamera.centerX -= (dx / scale) * FREE_TOUCH_PAN_MULTIPLIER
    freeCamera.centerZ -= (dy / scale) * FREE_TOUCH_PAN_MULTIPLIER
    clampFreeCameraToBounds(bounds, scale)
    debugLog('pan drag', {
      dx,
      dy,
      scale: Number(scale.toFixed(3)),
      before,
      after: { x: freeCamera.centerX, z: freeCamera.centerZ },
    })
  }

  function mountControls() {
    if (freeCamera.mounted) return
    freeCamera.mounted = true
    if (canvas && canvas.style) canvas.style.touchAction = 'none'
    debugLog('mount controls', { touchAction: canvas?.style?.touchAction })

    const onKeyDown = (ev) => {
      if (shouldIgnoreKeyboardEvent(ev.target)) return
      const key = String(ev.key || '').toLowerCase()
      if (key === 'w' || key === 'a' || key === 's' || key === 'd') {
        freeCamera.keys.add(key)
        debugLog('keydown', key, 'held=', [...freeCamera.keys])
      }
    }

    const onKeyUp = (ev) => {
      const key = String(ev.key || '').toLowerCase()
      freeCamera.keys.delete(key)
      if (key === 'w' || key === 'a' || key === 's' || key === 'd') {
        debugLog('keyup', key, 'held=', [...freeCamera.keys])
      }
    }

    const onBlur = () => {
      freeCamera.keys.clear()
      freeCamera.pointers.clear()
      freeCamera.lastPointerCenter = null
      freeCamera.pinchStartDistance = null
      freeCamera.pinchStartZoom = null
    }

    const onWheel = (ev) => {
      if (!g) return
      ev.preventDefault()
      const before = freeCamera.zoom
      const nextZoom = freeCamera.zoom * Math.exp(ev.deltaY * FREE_WHEEL_ZOOM_SPEED)
      freeCamera.zoom = clamp(nextZoom, FREE_ZOOM_MIN, FREE_ZOOM_MAX)
      debugLog('wheel zoom', { deltaY: ev.deltaY, before, after: freeCamera.zoom })
    }

    const onPointerDown = (ev) => {
      if (canvas.setPointerCapture) {
        try { canvas.setPointerCapture(ev.pointerId) } catch (_) {}
      }
      freeCamera.pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY })
      freeCamera.lastPointerCenter = pointerCenter()
      debugLog('pointerdown', {
        id: ev.pointerId,
        type: ev.pointerType,
        x: ev.clientX,
        y: ev.clientY,
        pointers: freeCamera.pointers.size,
      })
      if (freeCamera.pointers.size >= 2) {
        freeCamera.pinchStartDistance = pointerDistance()
        freeCamera.pinchStartZoom = freeCamera.zoom
        debugLog('pinch start', {
          distance: freeCamera.pinchStartDistance,
          zoom: freeCamera.pinchStartZoom,
        })
      }
    }

    const onPointerMove = (ev) => {
      const prev = freeCamera.pointers.get(ev.pointerId)
      if (!prev) return
      freeCamera.pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY })
      ev.preventDefault()
      if (freeCamera.pointers.size === 1) {
        debugLog('pointermove', {
          id: ev.pointerId,
          dx: ev.clientX - prev.x,
          dy: ev.clientY - prev.y,
        })
      }
    }

    const onPointerEnd = (ev) => {
      freeCamera.pointers.delete(ev.pointerId)
      freeCamera.lastPointerCenter = pointerCenter()
      if (freeCamera.pointers.size < 2) {
        freeCamera.pinchStartDistance = null
        freeCamera.pinchStartZoom = null
      }
      debugLog('pointerend', { id: ev.pointerId, pointers: freeCamera.pointers.size })
    }

    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('keyup', onKeyUp)
    window.addEventListener('blur', onBlur)
    canvas.addEventListener('wheel', onWheel, { passive: false })
    canvas.addEventListener('pointerdown', onPointerDown)
    canvas.addEventListener('pointermove', onPointerMove, { passive: false })
    canvas.addEventListener('pointerup', onPointerEnd)
    canvas.addEventListener('pointercancel', onPointerEnd)

    freeCamera.dispose = () => {
      debugLog('dispose controls')
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup', onKeyUp)
      window.removeEventListener('blur', onBlur)
      canvas.removeEventListener('wheel', onWheel)
      canvas.removeEventListener('pointerdown', onPointerDown)
      canvas.removeEventListener('pointermove', onPointerMove)
      canvas.removeEventListener('pointerup', onPointerEnd)
      canvas.removeEventListener('pointercancel', onPointerEnd)
      if (freeCamera.rafId != null) {
        cancelAnimationFrame(freeCamera.rafId)
        freeCamera.rafId = null
      }
      freeCamera.mounted = false
      freeCamera.keys.clear()
      freeCamera.pointers.clear()
    }
  }

  function frameLoop() {
    if (latestState && g) {
      const smooth = stateBuffer.interpolated() || latestState
      renderState(smooth)
    }
    freeCamera.rafId = requestAnimationFrame(frameLoop)
  }

  function updateFreeCameraFromInput(bounds, fitScale) {
    const now = (typeof performance !== 'undefined' ? performance.now() : Date.now())
    const dt = Math.max(0, Math.min(0.05, (now - freeCamera.lastFrameTs) / 1000))
    freeCamera.lastFrameTs = now

    let mx = 0
    let mz = 0
    if (freeCamera.keys.has('w')) mz -= 1
    if (freeCamera.keys.has('s')) mz += 1
    if (freeCamera.keys.has('a')) mx -= 1
    if (freeCamera.keys.has('d')) mx += 1
    if (mx || mz) {
      const mag = Math.hypot(mx, mz) || 1
      const speed = FREE_KEYBOARD_PAN_SPEED * freeCamera.zoom
      const before = { x: freeCamera.centerX, z: freeCamera.centerZ }
      freeCamera.centerX += (mx / mag) * speed * dt
      freeCamera.centerZ += (mz / mag) * speed * dt
      debugLog('keyboard pan', {
        keys: [...freeCamera.keys],
        dt: Number(dt.toFixed(4)),
        speed: Number(speed.toFixed(3)),
        before,
        after: { x: freeCamera.centerX, z: freeCamera.centerZ },
      })
    }

    const scale = Math.max(0.01, fitScale * freeCamera.zoom)
    const center = pointerCenter()
    if (center && freeCamera.lastPointerCenter) {
      panFreeCameraByScreenDelta(
        center.x - freeCamera.lastPointerCenter.x,
        center.y - freeCamera.lastPointerCenter.y,
        scale,
        bounds
      )
    }
    if (freeCamera.pointers.size >= 2 && freeCamera.pinchStartDistance && freeCamera.pinchStartZoom) {
      const dist = pointerDistance()
      if (dist && dist > 0) {
        const ratio = dist / freeCamera.pinchStartDistance
        const before = freeCamera.zoom
        freeCamera.zoom = clamp(freeCamera.pinchStartZoom / ratio, FREE_ZOOM_MIN, FREE_ZOOM_MAX)
        debugLog('pinch zoom', { dist, ratio, before, after: freeCamera.zoom })
      }
    }
    freeCamera.lastPointerCenter = center
    clampFreeCameraToBounds(bounds, Math.max(0.01, fitScale * freeCamera.zoom))
  }

  function renderState(state) {
    drawBackground()

    const entities = Array.isArray(state?.entities) ? state.entities : []
    const players = Array.isArray(state?.players) ? state.players : []
    const playerByRoot = ctx.runtime.animation.mapPlayersByRootPart(players)
    const bounds = computeBounds(players, entities)
    const fitScale = computeFitScale(bounds)
    const selectedPlayer = getSelectedPlayer(state, players)
    debugLog('render state', {
      tick: state?.tick,
      players: players.length,
      entities: entities.length,
      selected_player_id: state?.selected_player_id ?? null,
      mode: selectedPlayer ? 'follow' : 'free',
    })

    let scale = fitScale
    let centerX = (bounds.minX + bounds.maxX) * 0.5
    let centerZ = (bounds.minZ + bounds.maxZ) * 0.5

    if (selectedPlayer && Array.isArray(selectedPlayer.position)) {
      centerX = selectedPlayer.position[0]
      centerZ = selectedPlayer.position[2]
      scale = Math.max(fitScale * 1.4, 8)
      debugLog('follow target', {
        player: String(selectedPlayer?.name || selectedPlayer?.id || 'player'),
        centerX,
        centerZ,
        scale: Number(scale.toFixed(3)),
      })
    } else {
      seedFreeCamera(bounds)
      updateFreeCameraFromInput(bounds, fitScale)
      centerX = freeCamera.centerX
      centerZ = freeCamera.centerZ
      scale = Math.max(0.01, fitScale * freeCamera.zoom)
      debugLog('free camera frame', {
        centerX: Number(centerX.toFixed(3)),
        centerZ: Number(centerZ.toFixed(3)),
        zoom: Number(freeCamera.zoom.toFixed(3)),
        scale: Number(scale.toFixed(3)),
      })
    }

    for (const e of entities) {
      if (!Array.isArray(e.position)) continue
      const p = worldToScreen(e.position[0], e.position[2], scale, centerX, centerZ)
      const size = Array.isArray(e.size) ? Math.max(4, (Math.max(...e.size) * scale) / 2) : 5
      const boundPlayer = playerByRoot.get(e.id)
      const boost = boundPlayer ? animationBoost(boundPlayer) : 0
      g.fillStyle = boost > 0 ? '#ef4444' : presets.color(e, '#4f46e5')
      g.fillRect(p.x - size / 2, p.y - size / 2, size, size)
    }

    for (const pinfo of players) {
      if (!Array.isArray(pinfo.position)) continue
      const p = worldToScreen(pinfo.position[0], pinfo.position[2], scale, centerX, centerZ)
      const isFiring = animationBoost(pinfo) > 0
      const isSelected = !!(state?.selected_player_id && String(pinfo?.id || '') === String(state.selected_player_id))
      const radius = isFiring ? 8 : 6

      if (isSelected) {
        g.strokeStyle = '#e8ecff'
        g.lineWidth = 2
        g.beginPath()
        g.arc(p.x, p.y, radius + 5, 0, Math.PI * 2)
        g.stroke()
      }

      g.fillStyle = isFiring ? '#ef4444' : '#f59e0b'
      g.beginPath()
      g.arc(p.x, p.y, radius, 0, Math.PI * 2)
      g.fill()

      g.fillStyle = '#e8ecff'
      g.font = '12px sans-serif'
      g.fillText(String(pinfo.name || 'Player'), p.x + 8, p.y - 8)
    }

    g.fillStyle = '#8d98c6'
    g.font = '12px sans-serif'
    const modeLabel = selectedPlayer ? `follow ${selectedPlayer?.name || 'player'}` : `free x${freeCamera.zoom.toFixed(2)}`
    g.fillText(`tick ${state?.tick ?? '-'}  status ${state?.game_status ?? 'unknown'}  camera ${modeLabel}`, 12, canvas.height - 14)
  }

  return {
    mount() {
      mountControls()
      if (freeCamera.rafId == null) {
        freeCamera.lastFrameTs = (typeof performance !== 'undefined' ? performance.now() : Date.now())
        freeCamera.rafId = requestAnimationFrame(frameLoop)
      }
    },
    unmount() {
      if (typeof freeCamera.dispose === 'function') freeCamera.dispose()
    },
    onState(state) {
      if (!g) return
      latestState = state
      stateBuffer.push(state)
      const smooth = stateBuffer.interpolated() || state
      renderState(smooth)
    },
  }
}

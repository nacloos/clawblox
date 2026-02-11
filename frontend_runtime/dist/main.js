import * as threeSdk from "/ui/sdk/three.js"
import * as inputSdk from "/ui/sdk/input.js"

const API_VERSION = 1

const statusEl = document.getElementById('status')
const metaEl = document.getElementById('meta')
const canvas = document.getElementById('stage')

function setStatus(text, level = 'muted') {
  statusEl.textContent = text
  if (level === 'ok') statusEl.style.color = 'var(--ok)'
  else if (level === 'warn') statusEl.style.color = 'var(--warn)'
  else if (level === 'error') statusEl.style.color = 'var(--error)'
  else statusEl.style.color = 'var(--muted)'
}

function resizeCanvas() {
  const dpr = Math.min(window.devicePixelRatio || 1, 2)
  const rect = canvas.getBoundingClientRect()
  canvas.width = Math.max(1, Math.floor(rect.width * dpr))
  canvas.height = Math.max(1, Math.floor(rect.height * dpr))
  return dpr
}

function isRendererInstance(value) {
  if (!value || typeof value !== 'object') return false
  return (
    value.mount === undefined || typeof value.mount === 'function'
  ) && (
    value.unmount === undefined || typeof value.unmount === 'function'
  ) && (
    value.onState === undefined || typeof value.onState === 'function'
  ) && (
    value.onResize === undefined || typeof value.onResize === 'function'
  )
}

function lerp(a, b, t) {
  return a + (b - a) * t
}

function lerpVec3(a, b, t) {
  return [
    lerp(a[0], b[0], t),
    lerp(a[1], b[1], t),
    lerp(a[2], b[2], t),
  ]
}

function indexById(items, idField = 'id') {
  const out = new Map()
  if (!Array.isArray(items)) return out
  for (const item of items) {
    if (!item || typeof item !== 'object') continue
    const id = item[idField]
    if (id === undefined || id === null) continue
    out.set(id, item)
  }
  return out
}

function createSnapshotBuffer(options = {}) {
  const maxSnapshots = Number.isFinite(options.maxSnapshots) ? Math.max(2, options.maxSnapshots) : 12
  const interpolationDelayMs = Number.isFinite(options.interpolationDelayMs)
    ? Math.max(0, options.interpolationDelayMs)
    : 100
  const snapshots = []

  function push(state, nowMs = performance.now()) {
    snapshots.push({ state, nowMs })
    while (snapshots.length > maxSnapshots) snapshots.shift()
  }

  function latest() {
    return snapshots.length ? snapshots[snapshots.length - 1].state : null
  }

  function previous() {
    return snapshots.length > 1 ? snapshots[snapshots.length - 2].state : null
  }

  function interpolated(nowMs = performance.now()) {
    if (!snapshots.length) return null
    if (snapshots.length === 1) return snapshots[0].state

    const targetTime = nowMs - interpolationDelayMs
    if (targetTime <= snapshots[0].nowMs) return snapshots[0].state

    let from = null
    let to = null
    for (let i = 0; i < snapshots.length - 1; i++) {
      const a = snapshots[i]
      const b = snapshots[i + 1]
      if (a.nowMs <= targetTime && b.nowMs >= targetTime) {
        from = a
        to = b
        break
      }
    }

    if (!from || !to) {
      return snapshots[snapshots.length - 1].state
    }

    const span = Math.max(1, to.nowMs - from.nowMs)
    const t = Math.max(0, Math.min(1, (targetTime - from.nowMs) / span))

    const fromState = from.state
    const toState = to.state
    const fromPlayers = indexById(fromState?.players)
    const fromEntities = indexById(fromState?.entities)

    const players = Array.isArray(toState?.players)
      ? toState.players.map((p) => {
        const fp = fromPlayers.get(p.id)
        if (!fp || !Array.isArray(fp.position) || !Array.isArray(p.position)) return p
        return { ...p, position: lerpVec3(fp.position, p.position, t) }
      })
      : []

    const entities = Array.isArray(toState?.entities)
      ? toState.entities.map((e) => {
        const fe = fromEntities.get(e.id)
        if (!fe || !Array.isArray(fe.position) || !Array.isArray(e.position)) return e
        return { ...e, position: lerpVec3(fe.position, e.position, t) }
      })
      : []

    return {
      ...toState,
      players,
      entities,
    }
  }

  function forEachRecent(fn) {
    for (let i = snapshots.length - 1; i >= 0; i--) fn(snapshots[i].state, i)
  }

  return {
    push,
    latest,
    previous,
    interpolated,
    forEachRecent,
    size() {
      return snapshots.length
    },
  }
}

function createAnimationInspector() {
  function tracksForPlayer(player) {
    return Array.isArray(player?.active_animations) ? player.active_animations : []
  }

  function findTrack(player, predicate) {
    const tracks = tracksForPlayer(player)
    for (const track of tracks) {
      if (!track || track.is_playing === false) continue
      const id = String(track.animation_id || '')
      if (predicate(id, track)) return track
    }
    return null
  }

  function hasTrackMatching(player, pattern) {
    const regex = pattern instanceof RegExp ? pattern : new RegExp(String(pattern), 'i')
    return !!findTrack(player, (id) => regex.test(id))
  }

  function mapPlayersByRootPart(players) {
    const out = new Map()
    if (!Array.isArray(players)) return out
    for (const player of players) {
      if (!player || typeof player.root_part_id !== 'number') continue
      out.set(player.root_part_id, player)
    }
    return out
  }

  return {
    tracksForPlayer,
    findTrack,
    hasTrackMatching,
    mapPlayersByRootPart,
  }
}

function createPresetRegistry(initialPresets = {}) {
  const presets = new Map(Object.entries(initialPresets))

  function register(id, preset) {
    if (!id) return
    presets.set(id, preset || {})
  }

  function resolve(entity) {
    const presetId = entity?.render?.preset_id || entity?.preset_id
    const base = presetId ? presets.get(String(presetId)) || null : null
    if (!base) return null
    return {
      id: String(presetId),
      ...base,
    }
  }

  function color(entity, fallback = '#4f46e5') {
    const resolved = resolve(entity)
    if (resolved?.color) return resolved.color
    const c = entity?.render?.color
    if (Array.isArray(c) && c.length === 3) {
      const r = Math.round(Math.max(0, Math.min(1, c[0])) * 255)
      const g = Math.round(Math.max(0, Math.min(1, c[1])) * 255)
      const b = Math.round(Math.max(0, Math.min(1, c[2])) * 255)
      return `rgb(${r}, ${g}, ${b})`
    }
    return fallback
  }

  return {
    register,
    resolve,
    color,
    list() {
      return [...presets.entries()]
    },
  }
}

function createRuntimeKit() {
  return {
    math: { lerp, lerpVec3 },
    state: {
      indexById,
      createSnapshotBuffer,
    },
    animation: createAnimationInspector(),
    presets: {
      createPresetRegistry,
    },
    three: threeSdk,
    input: inputSdk,
  }
}

async function loadManifest() {
  const resp = await fetch('/renderer/manifest')
  if (!resp.ok) throw new Error(`manifest HTTP ${resp.status}`)
  return resp.json()
}

async function loadRendererModule(manifest) {
  const target = manifest?.entry_url || '/ui/renderers/default.js'
  const mod = await import(/* @vite-ignore */ target)
  const createRenderer = mod?.createRenderer || mod?.default?.createRenderer
  if (typeof createRenderer !== 'function') {
    throw new Error(`renderer module missing createRenderer(): ${target}`)
  }
  return { createRenderer, target }
}

async function bootstrap() {
  let currentDpr = resizeCanvas()
  window.addEventListener('resize', () => {
    currentDpr = resizeCanvas()
    if (window.__renderer?.onResize) {
      window.__renderer.onResize({ width: canvas.width, height: canvas.height, dpr: currentDpr })
    }
  })

  setStatus('Loading renderer...', 'muted')

  let manifest
  try {
    manifest = await loadManifest()
  } catch (error) {
    console.error(error)
    setStatus('Failed to load renderer manifest', 'error')
    return
  }

  const name = manifest?.name || 'Default Renderer'
  const mode = manifest?.mode || 'default'
  const capabilities = Array.isArray(manifest?.capabilities) ? manifest.capabilities.join(', ') : ''
  metaEl.textContent = `${name} | mode: ${mode}${capabilities ? ` | ${capabilities}` : ''}`

  if (manifest?.api_version !== API_VERSION) {
    setStatus(
      `Renderer API mismatch (host=${API_VERSION}, game=${manifest?.api_version ?? 'unknown'})`,
      'error'
    )
    return
  }

  let rendererFactory
  let rendererTarget
  try {
    const loaded = await loadRendererModule(manifest)
    rendererFactory = loaded.createRenderer
    rendererTarget = loaded.target
  } catch (error) {
    console.error(error)
    const configured = manifest?.entry_url
    if (configured) {
      setStatus(`Renderer load failed: ${configured}`, 'error')
      return
    }
    setStatus('No custom renderer configured; using default', 'warn')
    const fallback = await import('/ui/renderers/default.js')
    rendererFactory = fallback.createRenderer
    rendererTarget = '/ui/renderers/default.js'
  }

  const hostContext = {
    apiVersion: API_VERSION,
    manifest,
    canvas,
    runtime: createRuntimeKit(),
    log(level, message, data) {
      const fn = level === 'error' ? console.error : level === 'warn' ? console.warn : console.log
      fn(`[renderer] ${message}`, data ?? '')
    },
  }

  const renderer = rendererFactory(hostContext)
  if (!isRendererInstance(renderer)) {
    setStatus('Invalid renderer instance contract', 'error')
    return
  }
  window.__renderer = renderer
  if (renderer.mount) renderer.mount()
  if (renderer.onResize) renderer.onResize({ width: canvas.width, height: canvas.height, dpr: currentDpr })

  const wsStats = { frames: 0, lastTick: 0 }
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const ws = new WebSocket(`${protocol}//${window.location.host}/spectate/ws`)
  ws.binaryType = 'arraybuffer'

  ws.onopen = () => setStatus(`Connected | ${rendererTarget}`, 'ok')
  ws.onerror = () => setStatus('WebSocket error', 'error')
  ws.onclose = () => setStatus('Disconnected', 'warn')

  ws.onmessage = (event) => {
    try {
      let text
      if (event.data instanceof ArrayBuffer) {
        text = new TextDecoder().decode(event.data)
      } else {
        text = String(event.data)
      }
      const state = JSON.parse(text)
      if (state && state.error) {
        setStatus(state.error, 'error')
        return
      }
      wsStats.frames += 1
      if (typeof state?.tick === 'number') wsStats.lastTick = state.tick
      if (renderer.onState) renderer.onState(state)
    } catch (error) {
      console.error(error)
      setStatus('Failed to parse spectator state', 'error')
    }
  }

  window.addEventListener('beforeunload', () => {
    if (renderer.unmount) renderer.unmount()
    ws.close()
  })
}

bootstrap()

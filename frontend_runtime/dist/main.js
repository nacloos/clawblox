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

let audioCtx = null
const activeStreams = new Map()
const pendingChunks = []
let spectateWs = null
const AUDIO_FINALIZE_GRACE_MS = 50

function ensureAudioCtx() {
  if (!audioCtx) {
    audioCtx = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 24000 })
    console.log('[audio] AudioContext created, state=' + audioCtx.state)
  }
  if (audioCtx.state === 'suspended') {
    audioCtx.resume().then(() => {
      console.log('[audio] AudioContext resumed, flushing ' + pendingChunks.length + ' buffered chunks')
      while (pendingChunks.length) playChunk(pendingChunks.shift())
    })
  } else if (audioCtx.state === 'running') {
    while (pendingChunks.length) playChunk(pendingChunks.shift())
  }
}

function onUserGesture() {
  ensureAudioCtx()
  // Only stop listening once context is actually running
  if (audioCtx && audioCtx.state === 'running') {
    document.removeEventListener('click', onUserGesture)
    document.removeEventListener('keydown', onUserGesture)
  }
}
document.addEventListener('click', onUserGesture)
document.addEventListener('keydown', onUserGesture)

function onAudioChunk(msg) {
  if (!audioCtx || audioCtx.state !== 'running') {
    // Buffer until user gesture resumes the context
    pendingChunks.push(msg)
    return
  }
  playChunk(msg)
}

let audioDebugCount = 0

function playChunk(msg) {
  let stream = activeStreams.get(msg.stream_id)
  if (!stream) {
    stream = {
      nextPlayTime: 0,
      leftover: null,
      firstChunkWall: performance.now(),
      chunkCount: 0,
      done: false,
      finalized: false,
      finalizeTimer: null,
      lastSource: null,
    }
    activeStreams.set(msg.stream_id, stream)
    console.log(`[audio] new stream ${msg.stream_id}`)
  }
  stream.chunkCount++

  if (msg.done) {
    const streamId = msg.stream_id
    stream.done = true

    const finalizeNow = (reason) => {
      if (stream.finalized) return
      stream.finalized = true
      if (stream.finalizeTimer) {
        clearTimeout(stream.finalizeTimer)
        stream.finalizeTimer = null
      }
      console.log(`[audio] finalized stream ${streamId} reason=${reason}`)
      activeStreams.delete(streamId)
    }

    const armFinalizeGuard = () => {
      const now = audioCtx.currentTime
      const buffered = stream.nextPlayTime > now ? stream.nextPlayTime - now : 0
      const delayMs = buffered * 1000 + AUDIO_FINALIZE_GRACE_MS
      const wallElapsed = ((performance.now() - stream.firstChunkWall) / 1000).toFixed(2)
      console.log(`[audio] stream ${streamId} done: ${stream.chunkCount} chunks, ` +
        `buffered=${buffered.toFixed(2)}s ahead, wall=${wallElapsed}s, guard in ${delayMs.toFixed(0)}ms`)
      stream.finalizeTimer = setTimeout(() => finalizeNow("guard_timeout"), delayMs)
    }

    if (audioCtx.state !== 'running') {
      console.log(`[audio] stream ${streamId} done but AudioContext suspended, waiting for resume`)
      const onResume = () => {
        if (audioCtx.state === 'running') {
          audioCtx.removeEventListener('statechange', onResume)
          armFinalizeGuard()
        }
      }
      audioCtx.addEventListener('statechange', onResume)
      return
    }

    if (!stream.lastSource) {
      // No scheduled source exists; finalize immediately.
      finalizeNow("done_no_source")
      return
    }

    armFinalizeGuard()
    return
  }

  // Decode base64 to bytes
  const raw = atob(msg.data)
  let bytes
  if (stream.leftover !== null) {
    bytes = new Uint8Array(1 + raw.length)
    bytes[0] = stream.leftover
    for (let i = 0; i < raw.length; i++) bytes[i + 1] = raw.charCodeAt(i)
    stream.leftover = null
  } else {
    bytes = new Uint8Array(raw.length)
    for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i)
  }

  // Carry over odd trailing byte for next chunk
  let usable = bytes.length
  if (usable % 2 !== 0) {
    stream.leftover = bytes[usable - 1]
    usable -= 1
  }
  if (usable === 0) return

  // Int16 -> Float32
  const aligned = bytes.buffer.byteLength === usable ? bytes.buffer : bytes.slice(0, usable).buffer
  const int16 = new Int16Array(aligned)
  const float32 = new Float32Array(int16.length)
  for (let i = 0; i < int16.length; i++) float32[i] = int16[i] / 32768

  // Schedule gapless playback
  const buffer = audioCtx.createBuffer(1, float32.length, 24000)
  buffer.getChannelData(0).set(float32)
  const source = audioCtx.createBufferSource()
  source.buffer = buffer
  source.connect(audioCtx.destination)
  stream.lastSource = source
  source.onended = () => {
    // Authoritative completion path: stream is done and this is the last scheduled source.
    if (!stream.done) return
    if (stream.finalized) return
    if (stream.lastSource !== source) return
    if (stream.finalizeTimer) {
      clearTimeout(stream.finalizeTimer)
      stream.finalizeTimer = null
    }
    console.log(`[audio] finalized stream ${msg.stream_id} reason=last_source_onended`)
    stream.finalized = true
    activeStreams.delete(msg.stream_id)
  }

  const now = audioCtx.currentTime
  if (stream.nextPlayTime < now) stream.nextPlayTime = now
  source.start(stream.nextPlayTime)
  stream.nextPlayTime += buffer.duration

  // Log every 10th chunk: how far ahead the buffer is scheduled
  if (stream.chunkCount % 10 === 1) {
    const ahead = (stream.nextPlayTime - now).toFixed(2)
    const wallMs = (performance.now() - stream.firstChunkWall).toFixed(0)
    console.log(`[audio] chunk #${stream.chunkCount} stream=${msg.stream_id.slice(-8)} dur=${buffer.duration.toFixed(3)}s ahead=${ahead}s wall=${wallMs}ms`)
  }
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
  spectateWs = ws

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
      const data = JSON.parse(text)
      if (data && data.type === 'audio_chunk') {
        onAudioChunk(data)
        return
      }
      const state = data
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

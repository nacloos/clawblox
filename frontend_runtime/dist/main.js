import * as threeSdk from "/ui/sdk/three.js"
import * as inputSdk from "/ui/sdk/input.js"

const API_VERSION = 1

const statusEl = document.getElementById('status')
const metaEl = document.getElementById('meta')
const canvas = document.getElementById('stage')
const chatToggleEl = document.getElementById('chat-toggle')
const chatPanelEl = document.getElementById('chat-panel')
const chatFeedEl = document.getElementById('chat-feed')
const chatFormEl = document.getElementById('chat-form')
const chatInputEl = document.getElementById('chat-input')
const chatSendEl = document.getElementById('chat-send')
const chatConnectionEl = document.getElementById('chat-connection')
const chatLocalStatusEl = document.getElementById('chat-local-status')
const playerListCountEl = document.getElementById('player-list-count')
const playerListBodyEl = document.getElementById('player-list-body')
const replayPanelEl = document.getElementById('replay-panel')
const replayPlayToggleEl = document.getElementById('replay-play-toggle')
const replayPlayIconEl = document.getElementById('replay-play-icon')
const replayPlayLabelEl = document.getElementById('replay-play-label')
const replaySliderEl = document.getElementById('replay-slider')
const replayTimeCurrentEl = document.getElementById('replay-time-current')
const replayTimeTotalEl = document.getElementById('replay-time-total')
const replaySpeedEl = document.getElementById('replay-speed')

const chatState = {
  open: false,
  wsConnected: false,
  loadedHistory: false,
  sending: false,
  messages: [],
  seenIds: new Set(),
  sessionToken: null,
  agentId: null,
  playerName: null,
}
let selectedSpectatorPlayerId = null
let latestSpectatorState = null
const replayState = {
  enabled: false,
  info: null,
  pollTimer: null,
  scrubbing: false,
}

function setStatus(text, level = 'muted') {
  statusEl.textContent = text
  if (level === 'ok') statusEl.style.color = 'var(--ok)'
  else if (level === 'warn') statusEl.style.color = 'var(--warn)'
  else if (level === 'error') statusEl.style.color = 'var(--error)'
  else statusEl.style.color = 'var(--muted)'
}

function setChatConnection(text) {
  if (chatConnectionEl) chatConnectionEl.textContent = text
}

function formatDurationMs(ms) {
  const totalSeconds = Math.max(0, Math.floor((Number(ms) || 0) / 1000))
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = totalSeconds % 60
  if (hours > 0) return `${hours}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`
  return `${minutes}:${String(seconds).padStart(2, '0')}`
}

function tickToMs(tick, info) {
  if (!info || !Number.isFinite(info.tick_rate) || info.tick_rate <= 0) return 0
  return Math.max(0, Math.round(((tick - info.first_tick) / info.tick_rate) * 1000))
}

function setReplayMode(enabled) {
  replayState.enabled = !!enabled
  document.body.dataset.mode = replayState.enabled ? 'replay' : 'live'
  if (replayPanelEl) replayPanelEl.hidden = !replayState.enabled
  if (replayState.enabled) {
    setChatPanelOpen(false)
    setChatLocalStatus('Replay mode')
    setChatConnection('Replay')
  }
}

function setReplayPlayUi(paused) {
  if (!replayPlayIconEl || !replayPlayLabelEl || !replayPlayToggleEl) return
  if (paused) {
    replayPlayIconEl.innerHTML = '<path d="M8 5v14l11-7z"/>'
    replayPlayLabelEl.textContent = 'Play'
    replayPlayToggleEl.setAttribute('aria-label', 'Play replay')
    replayPlayToggleEl.title = 'Play replay'
  } else {
    replayPlayIconEl.innerHTML = '<path d="M7 5h3v14H7zm7 0h3v14h-3z"/>'
    replayPlayLabelEl.textContent = 'Pause'
    replayPlayToggleEl.setAttribute('aria-label', 'Pause replay')
    replayPlayToggleEl.title = 'Pause replay'
  }
}

function renderReplayInfo(info) {
  replayState.info = info
  if (!replayState.enabled || !info) return
  setReplayPlayUi(!!info.paused)
  if (replaySliderEl) {
    replaySliderEl.min = String(info.first_tick || 0)
    replaySliderEl.max = String(info.total_ticks || info.first_tick || 0)
    if (!replayState.scrubbing) replaySliderEl.value = String(info.current_tick || info.first_tick || 0)
  }
  if (replayTimeCurrentEl) replayTimeCurrentEl.textContent = formatDurationMs(tickToMs(info.current_tick || 0, info))
  if (replayTimeTotalEl) replayTimeTotalEl.textContent = formatDurationMs(info.duration_ms || 0)
  if (replaySpeedEl) {
    const value = String(info.speed ?? 1)
    if ([...replaySpeedEl.options].some((opt) => opt.value === value)) {
      replaySpeedEl.value = value
    }
  }
}

async function fetchReplayInfo() {
  const resp = await fetch('/replay/info', { cache: 'no-store' })
  if (!resp.ok) throw new Error(`replay info HTTP ${resp.status}`)
  return resp.json()
}

async function probeReplayMode() {
  try {
    return await fetchReplayInfo()
  } catch (_) {
    return null
  }
}

async function postReplay(path) {
  const resp = await fetch(path, { method: 'POST' })
  if (!resp.ok) throw new Error(`${path} HTTP ${resp.status}`)
}

async function refreshReplayInfo() {
  if (!replayState.enabled) return
  try {
    const info = await fetchReplayInfo()
    renderReplayInfo(info)
  } catch (err) {
    console.error(err)
    setStatus('Replay status unavailable', 'warn')
  }
}

function startReplayPolling() {
  if (replayState.pollTimer) clearInterval(replayState.pollTimer)
  replayState.pollTimer = setInterval(() => {
    void refreshReplayInfo()
  }, 300)
}

function renderPlayerList(players) {
  if (!playerListBodyEl) return
  const list = Array.isArray(players) ? [...players] : []
  const availableIds = new Set(list.map((p) => String(p?.id || '')).filter(Boolean))
  if (selectedSpectatorPlayerId && !availableIds.has(selectedSpectatorPlayerId)) {
    selectedSpectatorPlayerId = null
  }
  list.sort((a, b) => String(a?.name || '').localeCompare(String(b?.name || '')))
  if (playerListCountEl) playerListCountEl.textContent = String(list.length)
  playerListBodyEl.textContent = ''

  if (!list.length) {
    const empty = document.createElement('div')
    empty.className = 'player-list__empty'
    empty.textContent = 'No players connected'
    playerListBodyEl.appendChild(empty)
    return
  }

  const overviewRow = document.createElement('button')
  overviewRow.className = 'player-list__row'
  overviewRow.type = 'button'
  overviewRow.title = 'Free camera overview'
  if (!selectedSpectatorPlayerId) {
    overviewRow.classList.add('is-selected')
    overviewRow.setAttribute('aria-pressed', 'true')
  } else {
    overviewRow.setAttribute('aria-pressed', 'false')
  }
  const applyOverviewSelection = () => {
    selectedSpectatorPlayerId = null
    renderPlayerList(players)
    if (latestSpectatorState && window.__renderer?.onState) {
      const nextState = prioritizeSelectedPlayer(latestSpectatorState)
      window.__renderer.onState(nextState)
    }
  }
  overviewRow.addEventListener('pointerdown', (ev) => {
    ev.preventDefault()
    applyOverviewSelection()
  })
  overviewRow.addEventListener('click', () => {
    applyOverviewSelection()
  })
  const overviewName = document.createElement('div')
  overviewName.className = 'player-list__name'
  overviewName.textContent = 'Overview / Free camera'
  const overviewHint = document.createElement('div')
  overviewHint.className = 'player-list__hp'
  overviewHint.textContent = 'WASD, drag, wheel'
  overviewRow.append(overviewName, overviewHint)
  playerListBodyEl.appendChild(overviewRow)

  for (const player of list) {
    const row = document.createElement('button')
    row.className = 'player-list__row'
    row.type = 'button'
    row.title = 'Focus player'
    const playerId = String(player?.id || '')
    if (playerId && playerId === selectedSpectatorPlayerId) {
      row.classList.add('is-selected')
      row.setAttribute('aria-pressed', 'true')
    } else {
      row.setAttribute('aria-pressed', 'false')
    }
    const applySelection = () => {
      selectedSpectatorPlayerId = playerId || null
      renderPlayerList(players)
      if (latestSpectatorState && window.__renderer?.onState) {
        const nextState = prioritizeSelectedPlayer(latestSpectatorState)
        window.__renderer.onState(nextState)
      }
    }
    row.addEventListener('pointerdown', (ev) => {
      ev.preventDefault()
      applySelection()
    })
    row.addEventListener('click', () => {
      applySelection()
    })

    const name = document.createElement('div')
    name.className = 'player-list__name'
    name.textContent = String(player?.name || 'Player')

    const hp = document.createElement('div')
    hp.className = 'player-list__hp'
    const health = Number(player?.health)
    hp.textContent = Number.isFinite(health) ? `${Math.max(0, Math.round(health))} HP` : ''

    row.append(name, hp)
    playerListBodyEl.appendChild(row)
  }
}

function prioritizeSelectedPlayer(state) {
  if (!state) return state
  if (!selectedSpectatorPlayerId || !Array.isArray(state.players)) {
    if (state.selected_player_id === undefined) return state
    return { ...state, selected_player_id: null }
  }
  const idx = state.players.findIndex((p) => String(p?.id || '') === selectedSpectatorPlayerId)
  if (idx <= 0) {
    if (state.selected_player_id === selectedSpectatorPlayerId) return state
    return { ...state, selected_player_id: selectedSpectatorPlayerId }
  }
  const players = [...state.players]
  const [selected] = players.splice(idx, 1)
  players.unshift(selected)
  return { ...state, players, selected_player_id: selectedSpectatorPlayerId }
}

function setChatLocalStatus(text) {
  if (chatLocalStatusEl) chatLocalStatusEl.textContent = text
}

function updateChatSendUi() {
  if (chatSendEl) chatSendEl.disabled = chatState.sending
  if (chatInputEl) chatInputEl.disabled = false
}

function setChatPanelOpen(open) {
  chatState.open = !!open
  if (!chatPanelEl) return
  chatPanelEl.classList.toggle('is-open', chatState.open)
  if (chatToggleEl) {
    chatToggleEl.setAttribute('aria-expanded', chatState.open ? 'true' : 'false')
  }
  if (chatState.open) renderChatFeed()
}

function makeLocalPlayerName() {
  const key = 'clawblox_local_chat_name'
  const existing = localStorage.getItem(key)
  if (existing && existing.trim()) return existing.trim()
  const generated = `Player-${Math.random().toString(36).slice(2, 6)}`
  localStorage.setItem(key, generated)
  return generated
}

async function ensureChatSession() {
  if (chatState.sessionToken) return
  const playerName = chatState.playerName || makeLocalPlayerName()
  chatState.playerName = playerName
  const resp = await fetch(`/join?name=${encodeURIComponent(playerName)}`, { method: 'POST' })
  if (!resp.ok) throw new Error(`join failed: HTTP ${resp.status}`)
  const data = await resp.json()
  chatState.sessionToken = data.session || null
  chatState.agentId = data.agent_id || null
  setChatLocalStatus(`Chatting as ${playerName}`)
}

function userAtBottom(el) {
  return el.scrollHeight - el.scrollTop - el.clientHeight < 16
}

function nameColor(name) {
  let h = 0
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0
  const hue = h % 360
  return `hsl(${hue}deg 70% 75%)`
}

function appendChatMessage(msg) {
  const id = String(msg?.id || '')
  if (!id || chatState.seenIds.has(id)) return
  chatState.seenIds.add(id)
  chatState.messages.push(msg)
  if (chatState.messages.length > 200) {
    const removed = chatState.messages.splice(0, chatState.messages.length - 200)
    for (const item of removed) chatState.seenIds.delete(String(item.id || ''))
  }
  renderChatFeed()
}

function renderChatFeed() {
  if (!chatFeedEl) return
  const shouldStick = userAtBottom(chatFeedEl)
  chatFeedEl.textContent = ''
  if (!chatState.messages.length) {
    const empty = document.createElement('div')
    empty.className = 'chat-message chat-message--system'
    empty.textContent = 'No messages yet.'
    chatFeedEl.appendChild(empty)
  } else {
    for (const msg of chatState.messages) {
      const row = document.createElement('div')
      row.className = 'chat-message'

      const meta = document.createElement('span')
      meta.className = 'chat-message__meta'

      const name = document.createElement('span')
      name.className = 'chat-message__name'
      name.textContent = String(msg.agent_name || 'Player')
      name.style.color = nameColor(name.textContent)

      const time = document.createElement('span')
      time.className = 'chat-message__time'
      const dt = new Date(String(msg.created_at || ''))
      time.textContent = Number.isFinite(dt.getTime())
        ? dt.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
        : ''

      const body = document.createElement('span')
      body.textContent = String(msg.content || '')

      meta.append(name, time)
      row.append(meta, body)
      chatFeedEl.appendChild(row)
    }
  }
  if (shouldStick) chatFeedEl.scrollTop = chatFeedEl.scrollHeight
}

async function loadChatHistory() {
  if (replayState.enabled) return
  try {
    const resp = await fetch('/chat/messages?limit=50')
    if (!resp.ok) throw new Error(`history HTTP ${resp.status}`)
    const data = await resp.json()
    const messages = Array.isArray(data?.messages) ? data.messages : []
    for (const msg of messages) appendChatMessage(msg)
    chatState.loadedHistory = true
  } catch (err) {
    console.error(err)
    setChatLocalStatus('Failed to load chat history')
  }
}

async function sendChatMessage(text) {
  if (replayState.enabled) return
  const content = String(text || '').trim()
  if (!content) return
  chatState.sending = true
  updateChatSendUi()
  try {
    await ensureChatSession()
    const resp = await fetch('/chat', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Session': chatState.sessionToken,
      },
      body: JSON.stringify({ content }),
    })
    if (!resp.ok) throw new Error(`chat send failed: HTTP ${resp.status}`)
    if (chatInputEl) chatInputEl.value = ''
    setChatLocalStatus(`Chatting as ${chatState.playerName}`)
  } catch (err) {
    console.error(err)
    setChatLocalStatus(err instanceof Error ? err.message : 'Failed to send chat')
  } finally {
    chatState.sending = false
    updateChatSendUi()
  }
}

if (chatToggleEl) {
  chatToggleEl.addEventListener('click', () => {
    setChatPanelOpen(!chatState.open)
    if (chatState.open && chatInputEl) chatInputEl.focus()
  })
}

if (chatFormEl) {
  chatFormEl.addEventListener('submit', (ev) => {
    ev.preventDefault()
    void sendChatMessage(chatInputEl?.value || '')
  })
}

window.addEventListener('keydown', (ev) => {
  const target = ev.target
  const isTextField = target instanceof HTMLElement && (
    target.tagName === 'INPUT' ||
    target.tagName === 'TEXTAREA' ||
    target.isContentEditable
  )

  if (ev.key === '/' && !isTextField) {
    ev.preventDefault()
    setChatPanelOpen(true)
    chatInputEl?.focus()
    return
  }

  if (ev.key === 'Escape' && chatState.open) {
    if (document.activeElement === chatInputEl) {
      chatInputEl.blur()
    } else {
      setChatPanelOpen(false)
    }
  }
})

updateChatSendUi()
renderChatFeed()
setChatLocalStatus('Read-only until you send a message.')
setChatConnection('Connecting...')

if (!chatState.open) {
  setChatPanelOpen(false)
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

  const replayInfo = await probeReplayMode()
  if (replayInfo) {
    setReplayMode(true)
    renderReplayInfo(replayInfo)
    startReplayPolling()
  }

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

  ws.onopen = () => {
    chatState.wsConnected = true
    setChatConnection(replayState.enabled ? 'Replay' : 'Live')
    setStatus(
      replayState.enabled ? `Connected | Replay | ${rendererTarget}` : `Connected | ${rendererTarget}`,
      'ok'
    )
    if (!replayState.enabled && !chatState.loadedHistory) void loadChatHistory()
  }
  ws.onerror = () => {
    setChatConnection('WS error')
    setStatus('WebSocket error', 'error')
  }
  ws.onclose = () => {
    chatState.wsConnected = false
    setChatConnection('Offline')
    setStatus('Disconnected', 'warn')
    renderPlayerList([])
  }

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
      if (data && data.type === 'chat_message') {
        appendChatMessage(data)
        return
      }
      if (data && typeof data.type === 'string') {
        return
      }
      const state = data
      if (state && state.error) {
        setStatus(state.error, 'error')
        return
      }
      latestSpectatorState = state
      wsStats.frames += 1
      if (typeof state?.tick === 'number') wsStats.lastTick = state.tick
      renderPlayerList(state?.players)
      const rendererState = prioritizeSelectedPlayer(state)
      if (renderer.onState) renderer.onState(rendererState)
    } catch (error) {
      console.error(error)
      setStatus('Failed to parse spectator state', 'error')
    }
  }

  window.addEventListener('beforeunload', () => {
    if (renderer.unmount) renderer.unmount()
    if (replayState.pollTimer) clearInterval(replayState.pollTimer)
    ws.close()
  })
}

if (replayPlayToggleEl) {
  replayPlayToggleEl.addEventListener('click', async () => {
    if (!replayState.enabled || !replayState.info) return
    try {
      if (replayState.info.paused) await postReplay('/replay/play')
      else await postReplay('/replay/pause')
      await refreshReplayInfo()
    } catch (err) {
      console.error(err)
      setStatus('Failed to update replay playback', 'error')
    }
  })
}

if (replaySliderEl) {
  replaySliderEl.addEventListener('pointerdown', () => {
    replayState.scrubbing = true
  })
  replaySliderEl.addEventListener('input', () => {
    if (!replayState.enabled || !replayState.info) return
    const tick = Number(replaySliderEl.value)
    if (replayTimeCurrentEl) replayTimeCurrentEl.textContent = formatDurationMs(tickToMs(tick, replayState.info))
  })
  replaySliderEl.addEventListener('change', async () => {
    if (!replayState.enabled) return
    try {
      const tick = Number(replaySliderEl.value)
      await postReplay(`/replay/seek?tick=${encodeURIComponent(String(tick))}`)
      await refreshReplayInfo()
    } catch (err) {
      console.error(err)
      setStatus('Failed to seek replay', 'error')
    } finally {
      replayState.scrubbing = false
    }
  })
  replaySliderEl.addEventListener('pointerup', () => {
    replayState.scrubbing = false
  })
}

if (replaySpeedEl) {
  replaySpeedEl.addEventListener('change', async () => {
    if (!replayState.enabled) return
    try {
      await postReplay(`/replay/speed?value=${encodeURIComponent(replaySpeedEl.value)}`)
      await refreshReplayInfo()
    } catch (err) {
      console.error(err)
      setStatus('Failed to change replay speed', 'error')
    }
  })
}

bootstrap()

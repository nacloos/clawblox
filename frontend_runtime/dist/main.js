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
  resizeCanvas()
  window.addEventListener('resize', () => {
    resizeCanvas()
    if (window.__renderer?.onResize) {
      window.__renderer.onResize({ width: canvas.width, height: canvas.height })
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
    setStatus('Renderer load failed; using default', 'warn')
    const fallback = await import('/ui/renderers/default.js')
    rendererFactory = fallback.createRenderer
    rendererTarget = '/ui/renderers/default.js'
  }

  const hostContext = {
    apiVersion: API_VERSION,
    canvas,
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
  if (renderer.onResize) renderer.onResize({ width: canvas.width, height: canvas.height })

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

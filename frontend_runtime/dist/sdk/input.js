export function createLocalInputClient(options = {}) {
  const baseUrl = options.baseUrl || ''
  const playerName = options.playerName || 'renderer-player'
  let sessionToken = null
  let agentId = null

  async function ensureSession() {
    if (sessionToken) return { sessionToken, agentId }

    const url = `${baseUrl}/join?name=${encodeURIComponent(playerName)}`
    const resp = await fetch(url, { method: 'POST' })
    if (!resp.ok) throw new Error(`join failed: HTTP ${resp.status}`)

    const data = await resp.json()
    sessionToken = data.session
    agentId = data.agent_id
    return { sessionToken, agentId }
  }

  async function sendInput(type, data = {}) {
    await ensureSession()
    const resp = await fetch(`${baseUrl}/input`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Session': sessionToken,
      },
      body: JSON.stringify({ type, data }),
    })
    if (!resp.ok) throw new Error(`input failed: HTTP ${resp.status}`)
    return resp.json()
  }

  async function observe() {
    await ensureSession()
    const resp = await fetch(`${baseUrl}/observe`, {
      headers: { 'X-Session': sessionToken },
    })
    if (!resp.ok) throw new Error(`observe failed: HTTP ${resp.status}`)
    return resp.json()
  }

  async function sendRemoteEvent(name, args = []) {
    const eventName = String(name || '').trim()
    if (!eventName) throw new Error('sendRemoteEvent requires a non-empty event name')
    const payloadArgs = Array.isArray(args) ? args : [args]
    return sendInput('RemoteEvent', { name: eventName, args: payloadArgs })
  }

  return {
    ensureSession,
    sendInput,
    sendRemoteEvent,
    observe,
    session() {
      return sessionToken
    },
    agentId() {
      return agentId
    },
  }
}

export function bindKeyboardActions(inputClient, bindings = {}, options = {}) {
  const target = options.target || window
  const repeatMs = Math.max(16, options.repeatMs ?? 100)
  const held = new Set()
  let timer = null

  async function flushHeld() {
    for (const code of held) {
      const action = bindings[code]
      if (!action) continue
      try {
        await inputClient.sendInput(action.type, action.data || {})
      } catch (_) {
        // Ignore input errors in key loop; caller can observe status via logs.
      }
    }
  }

  function onKeyDown(ev) {
    const action = bindings[ev.code]
    if (!action) return

    if (action.mode === 'hold') {
      held.add(ev.code)
      if (!timer) {
        timer = setInterval(flushHeld, repeatMs)
      }
      return
    }

    inputClient.sendInput(action.type, action.data || {}).catch(() => {})
  }

  function onKeyUp(ev) {
    held.delete(ev.code)
    if (!held.size && timer) {
      clearInterval(timer)
      timer = null
    }
  }

  target.addEventListener('keydown', onKeyDown)
  target.addEventListener('keyup', onKeyUp)

  return {
    dispose() {
      target.removeEventListener('keydown', onKeyDown)
      target.removeEventListener('keyup', onKeyUp)
      if (timer) clearInterval(timer)
    },
  }
}

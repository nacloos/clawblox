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

  function worldToScreen(x, z, scale, ox, oy) {
    return {
      x: ox + x * scale,
      y: oy + z * scale,
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

  function renderState(state) {
    drawBackground()

    const entities = Array.isArray(state?.entities) ? state.entities : []
    const players = Array.isArray(state?.players) ? state.players : []
    const playerByRoot = ctx.runtime.animation.mapPlayersByRootPart(players)
    const { minX, maxX, minZ, maxZ } = computeBounds(players, entities)

    const worldW = Math.max(1, maxX - minX)
    const worldH = Math.max(1, maxZ - minZ)
    const pad = 30
    const scale = Math.min((canvas.width - pad * 2) / worldW, (canvas.height - pad * 2) / worldH)
    const ox = pad - minX * scale
    const oy = pad - minZ * scale

    for (const e of entities) {
      if (!Array.isArray(e.position)) continue
      const p = worldToScreen(e.position[0], e.position[2], scale, ox, oy)
      const size = Array.isArray(e.size) ? Math.max(4, (Math.max(...e.size) * scale) / 2) : 5
      const boundPlayer = playerByRoot.get(e.id)
      const boost = boundPlayer ? animationBoost(boundPlayer) : 0
      g.fillStyle = boost > 0 ? '#ef4444' : presets.color(e, '#4f46e5')
      g.fillRect(p.x - size / 2, p.y - size / 2, size, size)
    }

    for (const pinfo of players) {
      if (!Array.isArray(pinfo.position)) continue
      const p = worldToScreen(pinfo.position[0], pinfo.position[2], scale, ox, oy)
      const isFiring = animationBoost(pinfo) > 0
      const radius = isFiring ? 8 : 6

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
    g.fillText(`tick ${state?.tick ?? '-'}  status ${state?.game_status ?? 'unknown'}`, 12, canvas.height - 14)
  }

  return {
    onState(state) {
      if (!g) return
      stateBuffer.push(state)
      const smooth = stateBuffer.interpolated() || state
      renderState(smooth)
    },
  }
}

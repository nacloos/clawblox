const FIRE_PATTERN = /(^|[:/_-])fire([:_-]|$)/i
const fireState = new Map()

function clamp01(value) {
  if (value < 0) return 0
  if (value > 1) return 1
  return value
}

function isPlayerFiring(player) {
  const tracks = player?.active_animations || []
  return tracks.some((track) => track && track.is_playing && FIRE_PATTERN.test(String(track.animation_id || '')))
}

export default {
  getEntityAnimationCommand({ entityId, stateBuffer, nowMs }) {
    const latest = stateBuffer.getLatest()
    if (!latest) return null

    let firing = false
    for (const player of latest.players.values()) {
      if (player.root_part_id !== entityId) continue
      firing = isPlayerFiring(player)
      break
    }

    const prev = fireState.get(entityId) || { recoil: 0, firing: false }
    const dt = 1 / 60
    const next = {
      firing,
      recoil: firing
        ? clamp01(prev.recoil + dt * 12)
        : clamp01(prev.recoil - dt * 10),
    }
    fireState.set(entityId, next)

    if (next.firing !== prev.firing) {
      console.log(
        `[shooter-flat-adapter] fire ${next.firing ? 'start' : 'end'} entity=${entityId} t=${Math.round(nowMs)}`
      )
    }

    if (next.recoil <= 0.001) return null
    return {
      additiveRotation: [-0.16 * next.recoil, 0, 0],
      additiveYOffset: 0.02 * next.recoil,
    }
  },
}

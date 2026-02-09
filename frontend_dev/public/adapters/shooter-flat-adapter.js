const FIRE_PATTERN = /(^|[:/_-])fire([:_-]|$)/i
const stateByEntity = new Map()

function clamp01(value) {
  if (value < 0) return 0
  if (value > 1) return 1
  return value
}

function isFiring(player) {
  const tracks = player?.active_animations || []
  return tracks.some((track) => track && track.is_playing && FIRE_PATTERN.test(String(track.animation_id || '')))
}

export default {
  getEntityAnimationCommand({ entityId, stateBuffer, nowMs }) {
    const latest = stateBuffer.getLatest()
    if (!latest) {
      return null
    }

    let firing = false
    let matchedPlayer = null
    let fireTrackIds = []
    for (const player of latest.players.values()) {
      if (player.root_part_id !== entityId) continue
      matchedPlayer = player
      const tracks = player?.active_animations || []
      fireTrackIds = tracks
        .filter((track) => track && track.is_playing)
        .map((track) => String(track.animation_id || ""))
      firing = isFiring(player)
      break
    }

    const prev = stateByEntity.get(entityId) || {
      recoil: 0,
      firing: false,
      lastLogMs: 0,
      lastMissingPlayerLogMs: 0,
      lastSnapshotLogMs: 0,
    }
    const dt = 1 / 60
    const recoil = firing
      ? clamp01(prev.recoil + dt * 12)
      : clamp01(prev.recoil - dt * 10)
    const next = {
      ...prev,
      firing,
      recoil,
    }
    stateByEntity.set(entityId, next)

    if (!matchedPlayer && nowMs - prev.lastMissingPlayerLogMs > 1500) {
      next.lastMissingPlayerLogMs = nowMs
      console.log(
        `[adapter] no player matched entity=${entityId}; players=${latest.players.size}`
      )
    }

    if (matchedPlayer && nowMs - prev.lastSnapshotLogMs > 1200) {
      next.lastSnapshotLogMs = nowMs
      console.log(
        `[adapter] snapshot entity=${entityId} player=${matchedPlayer.name || "?"} ` +
        `root_part_id=${matchedPlayer.root_part_id} active=${fireTrackIds.join(",") || "none"} firing=${firing}`
      )
    }

    if (next.firing !== prev.firing) {
      console.log(
        `[adapter] fire ${next.firing ? 'start' : 'end'} entity=${entityId} t=${Math.round(nowMs)}`
      )
    }

    if (Math.abs(next.recoil - prev.recoil) > 0.2 && nowMs - prev.lastLogMs > 500) {
      next.lastLogMs = nowMs
      console.log(
        `[adapter] recoil entity=${entityId} value=${next.recoil.toFixed(2)} firing=${next.firing}`
      )
    }

    if (recoil <= 0.001) return null
    return {
      additiveRotation: [-0.16 * recoil, 0, 0],
      additiveYOffset: 0.02 * recoil,
    }
  },
}

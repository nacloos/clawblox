import { SpectatorObservation } from '../api'

// Server timing
const SERVER_TICK_MS = 16.67 // 60 Hz

// Buffering - Roblox-style with larger delay for jitter absorption
const BUFFER_SIZE = 20 // More history for velocity estimation
const INTERPOLATION_DELAY_TICKS = 4 // Render 4 ticks behind (~67ms) - more buffer

// Extrapolation settings
const MAX_EXTRAPOLATION_TICKS = 6 // Max ~100ms of extrapolation
const VELOCITY_SMOOTHING = 0.3 // Blend factor for velocity updates

export interface EntitySnapshot {
  id: number
  type: string
  position: [number, number, number]
  rotation?: [[number, number, number], [number, number, number], [number, number, number]]
  size?: [number, number, number]
  color?: [number, number, number]
  material?: string
  shape?: 'Block' | 'Ball' | 'Cylinder' | 'Wedge'
  health?: number
  pickup_type?: string
}

export interface PlayerSnapshot {
  id: string
  name: string
  position: [number, number, number]
  health: number
  attributes?: Record<string, unknown>
}

export interface StateSnapshot {
  tick: number
  receiveTime: number
  entities: Map<number, EntitySnapshot>
  players: Map<string, PlayerSnapshot>
}

// Tracked velocity for extrapolation
interface EntityVelocity {
  vx: number
  vy: number
  vz: number
  lastTick: number
}

export interface InterpolatedResult {
  before: StateSnapshot | null
  after: StateSnapshot | null
  alpha: number
  isExtrapolating: boolean
}

export class StateBuffer {
  private buffer: StateSnapshot[] = []
  private readonly maxSize = BUFFER_SIZE

  // Render timing
  private lastUpdateTime: number = 0
  private renderTick: number = 0
  private lastFrameTime: number = 0

  // Velocity tracking for extrapolation (Roblox-style)
  private entityVelocities: Map<number, EntityVelocity> = new Map()
  private playerVelocities: Map<string, EntityVelocity> = new Map()

  // Cached result
  private cachedResult: InterpolatedResult | null = null
  private cachedFrameTime: number = 0

  // Jitter detection
  private lastReceiveInterval: number = 33 // Expected ~30fps from server
  private jitterBuffer: number = 0 // Additional buffer when jitter detected

  push(observation: SpectatorObservation): void {
    const receiveTime = performance.now()

    // Track receive intervals to detect jitter
    if (this.buffer.length > 0) {
      const lastReceive = this.buffer[this.buffer.length - 1].receiveTime
      const interval = receiveTime - lastReceive

      // Detect jitter: if interval varies significantly, increase buffer
      const expectedInterval = this.lastReceiveInterval
      const jitter = Math.abs(interval - expectedInterval)
      if (jitter > 20) { // More than 20ms variance
        this.jitterBuffer = Math.min(this.jitterBuffer + 0.5, 3) // Add up to 3 extra ticks
      } else {
        this.jitterBuffer = Math.max(this.jitterBuffer - 0.1, 0) // Slowly reduce
      }

      // Smooth the expected interval
      this.lastReceiveInterval = this.lastReceiveInterval * 0.9 + interval * 0.1
    }

    const entities = new Map<number, EntitySnapshot>()
    for (const entity of observation.entities) {
      entities.set(entity.id, { ...entity })
    }

    const players = new Map<string, PlayerSnapshot>()
    for (const player of observation.players) {
      players.set(player.id, { ...player })
    }

    const snapshot: StateSnapshot = {
      tick: observation.tick,
      receiveTime,
      entities,
      players,
    }

    // Update velocity estimates before adding new snapshot
    this.updateVelocities(snapshot)

    this.buffer.push(snapshot)

    while (this.buffer.length > this.maxSize) {
      this.buffer.shift()
    }

    if (this.buffer.length === 1) {
      this.renderTick = observation.tick - INTERPOLATION_DELAY_TICKS
      this.lastUpdateTime = receiveTime
    }

    this.cachedResult = null
  }

  /**
   * Update velocity estimates based on position deltas (Roblox-style)
   */
  private updateVelocities(newSnapshot: StateSnapshot): void {
    if (this.buffer.length === 0) return

    const prevSnapshot = this.buffer[this.buffer.length - 1]
    const tickDelta = newSnapshot.tick - prevSnapshot.tick
    if (tickDelta <= 0) return

    const dt = tickDelta * SERVER_TICK_MS / 1000 // Convert to seconds

    // Update entity velocities
    for (const [id, entity] of newSnapshot.entities) {
      const prevEntity = prevSnapshot.entities.get(id)
      if (prevEntity) {
        const vx = (entity.position[0] - prevEntity.position[0]) / dt
        const vy = (entity.position[1] - prevEntity.position[1]) / dt
        const vz = (entity.position[2] - prevEntity.position[2]) / dt

        const existing = this.entityVelocities.get(id)
        if (existing) {
          // Smooth velocity updates to reduce jitter
          existing.vx = existing.vx * (1 - VELOCITY_SMOOTHING) + vx * VELOCITY_SMOOTHING
          existing.vy = existing.vy * (1 - VELOCITY_SMOOTHING) + vy * VELOCITY_SMOOTHING
          existing.vz = existing.vz * (1 - VELOCITY_SMOOTHING) + vz * VELOCITY_SMOOTHING
          existing.lastTick = newSnapshot.tick
        } else {
          this.entityVelocities.set(id, { vx, vy, vz, lastTick: newSnapshot.tick })
        }
      }
    }

    // Update player velocities
    for (const [id, player] of newSnapshot.players) {
      const prevPlayer = prevSnapshot.players.get(id)
      if (prevPlayer) {
        const vx = (player.position[0] - prevPlayer.position[0]) / dt
        const vy = (player.position[1] - prevPlayer.position[1]) / dt
        const vz = (player.position[2] - prevPlayer.position[2]) / dt

        const existing = this.playerVelocities.get(id)
        if (existing) {
          existing.vx = existing.vx * (1 - VELOCITY_SMOOTHING) + vx * VELOCITY_SMOOTHING
          existing.vy = existing.vy * (1 - VELOCITY_SMOOTHING) + vy * VELOCITY_SMOOTHING
          existing.vz = existing.vz * (1 - VELOCITY_SMOOTHING) + vz * VELOCITY_SMOOTHING
          existing.lastTick = newSnapshot.tick
        } else {
          this.playerVelocities.set(id, { vx, vy, vz, lastTick: newSnapshot.tick })
        }
      }
    }
  }

  /**
   * Get velocity for an entity (for extrapolation)
   */
  getEntityVelocity(entityId: number): [number, number, number] | null {
    const vel = this.entityVelocities.get(entityId)
    if (!vel) return null
    return [vel.vx, vel.vy, vel.vz]
  }

  /**
   * Get velocity for a player (for extrapolation)
   */
  getPlayerVelocity(playerId: string): [number, number, number] | null {
    const vel = this.playerVelocities.get(playerId)
    if (!vel) return null
    return [vel.vx, vel.vy, vel.vz]
  }

  private updateRenderTick(renderTime: number): void {
    if (Math.abs(renderTime - this.lastFrameTime) < 0.1) {
      return
    }
    this.lastFrameTime = renderTime

    if (this.buffer.length === 0) {
      return
    }

    const latestTick = this.buffer[this.buffer.length - 1].tick
    const earliestTick = this.buffer[0].tick

    const elapsed = renderTime - this.lastUpdateTime
    this.lastUpdateTime = renderTime

    // Advance at server tick rate
    this.renderTick += elapsed / SERVER_TICK_MS

    // Target tick includes jitter buffer
    const effectiveDelay = INTERPOLATION_DELAY_TICKS + this.jitterBuffer
    const targetTick = latestTick - effectiveDelay

    const minTick = earliestTick
    // Allow extrapolation up to MAX_EXTRAPOLATION_TICKS beyond latest
    const maxTick = latestTick + MAX_EXTRAPOLATION_TICKS

    if (this.renderTick < minTick) {
      this.renderTick = minTick
    } else if (this.renderTick > maxTick) {
      // Hard cap at max extrapolation
      this.renderTick = maxTick
    } else if (this.renderTick > targetTick + 1) {
      // Gradually slow down if ahead of target (but don't hard clamp)
      this.renderTick = this.renderTick * 0.95 + targetTick * 0.05
    } else if (this.renderTick < targetTick - 1) {
      // Speed up slightly if falling behind
      this.renderTick = this.renderTick * 0.95 + targetTick * 0.05
    }
  }

  getInterpolatedState(renderTime: number): InterpolatedResult | null {
    if (this.buffer.length === 0) {
      return null
    }

    if (this.cachedResult && Math.abs(renderTime - this.cachedFrameTime) < 0.1) {
      return this.cachedResult
    }

    this.updateRenderTick(renderTime)

    let before: StateSnapshot | null = null
    let after: StateSnapshot | null = null

    for (let i = 0; i < this.buffer.length; i++) {
      const snapshot = this.buffer[i]
      if (snapshot.tick <= this.renderTick) {
        before = snapshot
      } else {
        after = snapshot
        break
      }
    }

    const latestTick = this.buffer[this.buffer.length - 1].tick
    const isExtrapolating = this.renderTick > latestTick

    if (!before) {
      this.cachedResult = {
        before: this.buffer[0],
        after: this.buffer.length > 1 ? this.buffer[1] : null,
        alpha: 0,
        isExtrapolating: false,
      }
      this.cachedFrameTime = renderTime
      return this.cachedResult
    }

    if (!after) {
      // Extrapolation: use last two snapshots + velocity
      if (this.buffer.length >= 2) {
        const prev = this.buffer[this.buffer.length - 2]
        const curr = this.buffer[this.buffer.length - 1]
        const tickDelta = curr.tick - prev.tick
        if (tickDelta > 0) {
          // Calculate how far to extrapolate
          const ticksPastCurr = this.renderTick - curr.tick
          const alpha = 1 + (ticksPastCurr / tickDelta)

          // Limit extrapolation
          const clampedAlpha = Math.min(alpha, 1 + MAX_EXTRAPOLATION_TICKS / tickDelta)

          this.cachedResult = {
            before: prev,
            after: curr,
            alpha: clampedAlpha,
            isExtrapolating: true,
          }
          this.cachedFrameTime = renderTime
          return this.cachedResult
        }
      }

      this.cachedResult = {
        before: this.buffer[this.buffer.length - 1],
        after: null,
        alpha: 0,
        isExtrapolating: true,
      }
      this.cachedFrameTime = renderTime
      return this.cachedResult
    }

    // Normal interpolation
    const tickDelta = after.tick - before.tick
    if (tickDelta <= 0) {
      this.cachedResult = { before, after, alpha: 0, isExtrapolating: false }
      this.cachedFrameTime = renderTime
      return this.cachedResult
    }

    const alpha = (this.renderTick - before.tick) / tickDelta

    this.cachedResult = {
      before,
      after,
      alpha: Math.max(0, Math.min(1, alpha)),
      isExtrapolating,
    }
    this.cachedFrameTime = renderTime
    return this.cachedResult
  }

  getLatest(): StateSnapshot | null {
    if (this.buffer.length === 0) return null
    return this.buffer[this.buffer.length - 1]
  }

  getEntityIds(): number[] {
    const latest = this.getLatest()
    if (!latest) return []
    return Array.from(latest.entities.keys())
  }

  getPlayerIds(): string[] {
    const latest = this.getLatest()
    if (!latest) return []
    return Array.from(latest.players.keys())
  }

  hasData(): boolean {
    return this.buffer.length > 0
  }

  clear(): void {
    this.buffer = []
    this.renderTick = 0
    this.lastUpdateTime = 0
    this.lastFrameTime = 0
    this.cachedResult = null
    this.cachedFrameTime = 0
    this.entityVelocities.clear()
    this.playerVelocities.clear()
    this.jitterBuffer = 0
  }

  /**
   * Debug: get current buffer status
   */
  getDebugInfo(): { bufferSize: number; renderTick: number; latestTick: number; jitterBuffer: number } {
    return {
      bufferSize: this.buffer.length,
      renderTick: this.renderTick,
      latestTick: this.buffer.length > 0 ? this.buffer[this.buffer.length - 1].tick : 0,
      jitterBuffer: this.jitterBuffer,
    }
  }
}

export function interpolatePosition(
  before: [number, number, number],
  after: [number, number, number],
  alpha: number
): [number, number, number] {
  return [
    before[0] + (after[0] - before[0]) * alpha,
    before[1] + (after[1] - before[1]) * alpha,
    before[2] + (after[2] - before[2]) * alpha,
  ]
}

/**
 * Extrapolate position using velocity
 */
export function extrapolatePosition(
  position: [number, number, number],
  velocity: [number, number, number],
  deltaSeconds: number
): [number, number, number] {
  return [
    position[0] + velocity[0] * deltaSeconds,
    position[1] + velocity[1] * deltaSeconds,
    position[2] + velocity[2] * deltaSeconds,
  ]
}

export function distanceSquared(
  a: [number, number, number],
  b: [number, number, number]
): number {
  const dx = b[0] - a[0]
  const dy = b[1] - a[1]
  const dz = b[2] - a[2]
  return dx * dx + dy * dy + dz * dz
}

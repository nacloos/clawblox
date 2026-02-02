import { SpectatorObservation } from '../api'

// Server timing
const SERVER_TICK_MS = 16.67 // 60 Hz

// Buffering
const BUFFER_SIZE = 10 // Snapshots to keep (~330ms of history)
const INTERPOLATION_DELAY_TICKS = 3 // Render 3 ticks behind latest (~50ms)

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
  receiveTime: number // performance.now() when received
  entities: Map<number, EntitySnapshot>
  players: Map<string, PlayerSnapshot>
}

export interface InterpolatedResult {
  before: StateSnapshot | null
  after: StateSnapshot | null
  alpha: number // 0-1 interpolation factor
}

export class StateBuffer {
  private buffer: StateSnapshot[] = []
  private readonly maxSize = BUFFER_SIZE

  // Track time progression for smooth interpolation
  private lastUpdateTime: number = 0
  private renderTick: number = 0 // Fractional tick we're currently rendering
  private lastFrameTime: number = 0 // Track which frame we're on to avoid double-updates

  // Cached result for current frame
  private cachedResult: InterpolatedResult | null = null
  private cachedFrameTime: number = 0

  /**
   * Push a new observation into the buffer
   */
  push(observation: SpectatorObservation): void {
    const receiveTime = performance.now()

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

    this.buffer.push(snapshot)

    // Keep buffer at max size
    while (this.buffer.length > this.maxSize) {
      this.buffer.shift()
    }

    // Initialize render tick if this is first snapshot
    if (this.buffer.length === 1) {
      this.renderTick = observation.tick - INTERPOLATION_DELAY_TICKS
      this.lastUpdateTime = receiveTime
    }

    // Invalidate cache when new data arrives
    this.cachedResult = null
  }

  /**
   * Update the render tick based on elapsed time.
   * Should be called once per frame before getting interpolated states.
   */
  private updateRenderTick(renderTime: number): void {
    // Only update once per frame (check if we've already updated this frame)
    // Use a small epsilon to detect same-frame calls
    if (Math.abs(renderTime - this.lastFrameTime) < 0.1) {
      return
    }
    this.lastFrameTime = renderTime

    if (this.buffer.length === 0) {
      return
    }

    const latestTick = this.buffer[this.buffer.length - 1].tick
    const earliestTick = this.buffer[0].tick

    // Advance render tick based on elapsed time
    const elapsed = renderTime - this.lastUpdateTime
    this.lastUpdateTime = renderTime

    // Advance at server tick rate (1 tick per 16.67ms)
    this.renderTick += elapsed / SERVER_TICK_MS

    // Target tick is latest minus delay
    const targetTick = latestTick - INTERPOLATION_DELAY_TICKS

    // Clamp render tick to valid range
    const minTick = earliestTick
    const maxTick = targetTick + 0.5 // Allow slight overrun

    if (this.renderTick < minTick) {
      // We're behind the buffer - jump to catch up
      this.renderTick = minTick
    } else if (this.renderTick > maxTick) {
      // We're ahead - gradually pull back
      this.renderTick = this.renderTick * 0.9 + maxTick * 0.1
    }
  }

  /**
   * Get the interpolated state for a given render time.
   * Caches result for same-frame calls (multiple entities query same frame).
   */
  getInterpolatedState(renderTime: number): InterpolatedResult | null {
    if (this.buffer.length === 0) {
      return null
    }

    // Return cached result if same frame
    if (this.cachedResult && Math.abs(renderTime - this.cachedFrameTime) < 0.1) {
      return this.cachedResult
    }

    // Update render tick for this frame
    this.updateRenderTick(renderTime)

    // Find the two snapshots to interpolate between
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

    // Handle edge cases
    if (!before) {
      this.cachedResult = {
        before: this.buffer[0],
        after: this.buffer.length > 1 ? this.buffer[1] : null,
        alpha: 0,
      }
      this.cachedFrameTime = renderTime
      return this.cachedResult
    }

    if (!after) {
      // Render tick is after all snapshots - slight extrapolation
      if (this.buffer.length >= 2) {
        const prev = this.buffer[this.buffer.length - 2]
        const curr = this.buffer[this.buffer.length - 1]
        const tickDelta = curr.tick - prev.tick
        if (tickDelta > 0) {
          const alpha = (this.renderTick - prev.tick) / tickDelta
          this.cachedResult = {
            before: prev,
            after: curr,
            alpha: Math.min(alpha, 1.5),
          }
          this.cachedFrameTime = renderTime
          return this.cachedResult
        }
      }
      this.cachedResult = {
        before: this.buffer[this.buffer.length - 1],
        after: null,
        alpha: 0,
      }
      this.cachedFrameTime = renderTime
      return this.cachedResult
    }

    // Normal interpolation between two snapshots
    const tickDelta = after.tick - before.tick
    if (tickDelta <= 0) {
      this.cachedResult = { before, after, alpha: 0 }
      this.cachedFrameTime = renderTime
      return this.cachedResult
    }

    const alpha = (this.renderTick - before.tick) / tickDelta

    this.cachedResult = {
      before,
      after,
      alpha: Math.max(0, Math.min(1, alpha)),
    }
    this.cachedFrameTime = renderTime
    return this.cachedResult
  }

  /**
   * Get the latest snapshot (for entity list, etc.)
   */
  getLatest(): StateSnapshot | null {
    if (this.buffer.length === 0) return null
    return this.buffer[this.buffer.length - 1]
  }

  /**
   * Get all entity IDs from the latest snapshot
   */
  getEntityIds(): number[] {
    const latest = this.getLatest()
    if (!latest) return []
    return Array.from(latest.entities.keys())
  }

  /**
   * Get all player IDs from the latest snapshot
   */
  getPlayerIds(): string[] {
    const latest = this.getLatest()
    if (!latest) return []
    return Array.from(latest.players.keys())
  }

  /**
   * Check if buffer has data
   */
  hasData(): boolean {
    return this.buffer.length > 0
  }

  /**
   * Clear all buffered state
   */
  clear(): void {
    this.buffer = []
    this.renderTick = 0
    this.lastUpdateTime = 0
    this.lastFrameTime = 0
    this.cachedResult = null
    this.cachedFrameTime = 0
  }
}

/**
 * Interpolate between two positions
 */
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
 * Calculate squared distance between two positions
 */
export function distanceSquared(
  a: [number, number, number],
  b: [number, number, number]
): number {
  const dx = b[0] - a[0]
  const dy = b[1] - a[1]
  const dz = b[2] - a[2]
  return dx * dx + dy * dy + dz * dz
}

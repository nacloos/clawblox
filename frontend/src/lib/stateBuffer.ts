import { SpectatorObservation, GuiElement } from '../api'

// Configuration
const INITIAL_RENDER_DELAY_MS = 200 // Increased for debugging
const MIN_RENDER_DELAY_MS = 50
const MAX_RENDER_DELAY_MS = 500
const CLOCK_SAMPLES = 10
const BUFFER_SIZE = 20

export interface BillboardLabel {
  text: string
  color: [number, number, number]
  size: number
}

export interface BillboardGui {
  studs_offset: [number, number, number]
  always_on_top: boolean
  labels: BillboardLabel[]
}

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
  model_url?: string
  billboard_gui?: BillboardGui
}

export interface PlayerSnapshot {
  id: string
  name: string
  position: [number, number, number]
  health: number
  attributes?: Record<string, unknown>
  gui?: GuiElement[]
}

export interface StateSnapshot {
  tick: number
  serverTimeMs: number // Server timestamp (ms since game start)
  localReceiveTime: number // Local time when received (for clock sync)
  entities: Map<number, EntitySnapshot>
  players: Map<string, PlayerSnapshot>
}

export interface InterpolatedResult {
  before: StateSnapshot
  after: StateSnapshot | null
  alpha: number
  isExtrapolating: boolean
}

/**
 * Compute median of an array of numbers
 */
function median(values: number[]): number {
  if (values.length === 0) return 0
  const sorted = [...values].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 !== 0
    ? sorted[mid]
    : (sorted[mid - 1] + sorted[mid]) / 2
}

/**
 * StateBuffer with server-time based interpolation.
 *
 * Key insight: Render time is derived from server time, not tracked independently.
 * This eliminates speed adjustment hacks and provides smooth interpolation.
 *
 * How it works:
 * 1. Server includes timestamp (server_time_ms) in each message
 * 2. Client calculates clock offset: offset = local_time - server_time (smoothed median)
 * 3. Fixed render delay: Always render at server_time - DELAY
 * 4. Simple interpolation: Find two snapshots bracketing target time, lerp between them
 * 5. Never extrapolate aggressively: If no future data, hold last position
 */
export class StateBuffer {
  private buffer: StateSnapshot[] = []

  // Clock synchronization: local_time ≈ server_time + offset
  private clockOffset: number = 0
  private clockOffsetSamples: number[] = []

  // Adaptive render delay
  private renderDelay: number = INITIAL_RENDER_DELAY_MS

  push(observation: SpectatorObservation): void {
    const localTime = performance.now()
    const serverTime = observation.server_time_ms

    // Update clock offset (smoothed median filters outliers from network spikes)
    this.updateClockOffset(localTime, serverTime)

    // Convert observation to snapshot
    const entities = new Map<number, EntitySnapshot>()
    for (const entity of observation.entities) {
      entities.set(entity.id, { ...entity })
    }

    const players = new Map<string, PlayerSnapshot>()
    for (const player of observation.players) {
      players.set(player.id, {
        id: player.id,
        name: player.name,
        position: player.position,
        health: player.health,
        attributes: player.attributes,
        gui: player.gui,
      })
    }

    const snapshot: StateSnapshot = {
      tick: observation.tick,
      serverTimeMs: serverTime,
      localReceiveTime: localTime,
      entities,
      players,
    }

    this.buffer.push(snapshot)

    // Trim old snapshots
    while (this.buffer.length > BUFFER_SIZE) {
      this.buffer.shift()
    }
  }

  /**
   * Update clock offset using median filtering.
   * offset = local_time - server_time
   * So: server_time_now = local_time_now - offset
   */
  private updateClockOffset(localTime: number, serverTime: number): void {
    const sample = localTime - serverTime
    this.clockOffsetSamples.push(sample)

    if (this.clockOffsetSamples.length > CLOCK_SAMPLES) {
      this.clockOffsetSamples.shift()
    }

    // Use median to filter outliers (network spikes)
    this.clockOffset = median(this.clockOffsetSamples)
  }

  /**
   * Get interpolated state for rendering.
   *
   * Returns snapshots bracketing the target render time with interpolation alpha.
   * If no future data available, returns the last snapshot with alpha=0 (hold position).
   */
  getInterpolatedState(): InterpolatedResult | null {
    if (this.buffer.length === 0) {
      return null
    }

    // Calculate target server time to render
    const localNow = performance.now()
    const serverNow = localNow - this.clockOffset
    const targetServerTime = serverNow - this.renderDelay

    // Find snapshots bracketing targetServerTime
    let before: StateSnapshot | null = null
    let after: StateSnapshot | null = null

    for (let i = 0; i < this.buffer.length; i++) {
      const snapshot = this.buffer[i]
      if (snapshot.serverTimeMs <= targetServerTime) {
        before = snapshot
      } else {
        after = snapshot
        break
      }
    }

    // No data before target - use earliest snapshot
    if (!before) {
      return {
        before: this.buffer[0],
        after: this.buffer.length > 1 ? this.buffer[1] : null,
        alpha: 0,
        isExtrapolating: false,
      }
    }

    // No data after target - HOLD last position (don't extrapolate)
    if (!after) {
      return {
        before,
        after: null,
        alpha: 0,
        isExtrapolating: true,
      }
    }

    // Normal interpolation between two snapshots
    const timeDelta = after.serverTimeMs - before.serverTimeMs
    if (timeDelta <= 0) {
      return {
        before,
        after,
        alpha: 0,
        isExtrapolating: false,
      }
    }

    const alpha = (targetServerTime - before.serverTimeMs) / timeDelta
    return {
      before,
      after,
      alpha: Math.max(0, Math.min(1, alpha)),
      isExtrapolating: false,
    }
  }

  /**
   * Get the latest snapshot (for entity list, etc.)
   */
  getLatest(): StateSnapshot | null {
    if (this.buffer.length === 0) return null
    return this.buffer[this.buffer.length - 1]
  }

  /**
   * Get all entity IDs from latest snapshot
   */
  getEntityIds(): number[] {
    const latest = this.getLatest()
    if (!latest) return []
    return Array.from(latest.entities.keys())
  }

  /**
   * Get all player IDs from latest snapshot
   */
  getPlayerIds(): string[] {
    const latest = this.getLatest()
    if (!latest) return []
    return Array.from(latest.players.keys())
  }

  /**
   * Check if buffer has any data
   */
  hasData(): boolean {
    return this.buffer.length > 0
  }

  /**
   * Clear all state
   */
  clear(): void {
    this.buffer = []
    this.clockOffset = 0
    this.clockOffsetSamples = []
    this.renderDelay = INITIAL_RENDER_DELAY_MS
  }

  /**
   * Debug info for monitoring
   */
  getDebugInfo(): {
    bufferSize: number
    clockOffset: number
    renderDelay: number
    latestServerTime: number
    targetRenderTime: number
  } {
    const localNow = performance.now()
    const serverNow = localNow - this.clockOffset
    const targetServerTime = serverNow - this.renderDelay

    return {
      bufferSize: this.buffer.length,
      clockOffset: this.clockOffset,
      renderDelay: this.renderDelay,
      latestServerTime: this.buffer.length > 0 ? this.buffer[this.buffer.length - 1].serverTimeMs : 0,
      targetRenderTime: targetServerTime,
    }
  }

  /**
   * Adjust render delay (for adaptive buffering based on jitter detection)
   */
  setRenderDelay(delayMs: number): void {
    this.renderDelay = Math.max(MIN_RENDER_DELAY_MS, Math.min(MAX_RENDER_DELAY_MS, delayMs))
  }

  /**
   * Get current render delay
   */
  getRenderDelay(): number {
    return this.renderDelay
  }

  /**
   * Compute axis-aligned bounding box from entity positions.
   * Returns bounds with optional padding, or null if no entities.
   */
  getAABB(padding: number = 50): { minX: number; maxX: number; minZ: number; maxZ: number } | null {
    const latest = this.getLatest()
    if (!latest || latest.entities.size === 0) return null

    let minX = Infinity, maxX = -Infinity
    let minZ = Infinity, maxZ = -Infinity

    for (const entity of latest.entities.values()) {
      const [x, , z] = entity.position
      minX = Math.min(minX, x)
      maxX = Math.max(maxX, x)
      minZ = Math.min(minZ, z)
      maxZ = Math.max(maxZ, z)
    }

    return {
      minX: minX - padding,
      maxX: maxX + padding,
      minZ: minZ - padding,
      maxZ: maxZ + padding,
    }
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
 * Squared distance between two points
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

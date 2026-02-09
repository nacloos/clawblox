import { StateBuffer } from './stateBuffer'

export interface EntityAnimationCommand {
  // Additive local rotation in radians applied to rendered model root.
  additiveRotation?: [number, number, number]
  // Additive local Y offset for procedural motion.
  additiveYOffset?: number
}

export interface AdapterFrameContext {
  entityId: number
  stateBuffer: StateBuffer
  nowMs: number
}

export interface GameRenderAdapter {
  getEntityAnimationCommand?: (ctx: AdapterFrameContext) => EntityAnimationCommand | null
}

export const defaultGameRenderAdapter: GameRenderAdapter = {}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function isAdapter(value: unknown): value is GameRenderAdapter {
  if (!isObject(value)) return false
  const fn = value.getEntityAnimationCommand
  return fn === undefined || typeof fn === 'function'
}

export async function loadGameRenderAdapter(adapterUrl?: string | null): Promise<GameRenderAdapter> {
  if (!adapterUrl) return defaultGameRenderAdapter

  try {
    let module: Record<string, unknown>
    if (adapterUrl.startsWith("/")) {
      const resp = await fetch(adapterUrl)
      if (!resp.ok) {
        throw new Error(`HTTP ${resp.status} while fetching adapter`)
      }
      const source = await resp.text()
      const blob = new Blob([source], { type: "text/javascript" })
      const blobUrl = URL.createObjectURL(blob)
      try {
        module = await import(/* @vite-ignore */ blobUrl)
      } finally {
        URL.revokeObjectURL(blobUrl)
      }
    } else {
      module = await import(/* @vite-ignore */ adapterUrl)
    }
    const candidate = module?.default ?? module?.adapter ?? module
    if (isAdapter(candidate)) {
      console.log(`[Adapter] loaded: ${adapterUrl}`)
      return candidate
    }
    console.warn(`[Adapter] invalid module shape: ${adapterUrl}`)
    return defaultGameRenderAdapter
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    console.warn(`[Adapter] failed to load ${adapterUrl}: ${message}`)
    return defaultGameRenderAdapter
  }
}

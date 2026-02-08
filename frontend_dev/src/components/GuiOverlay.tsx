import { useMemo } from 'react'
import { StateBuffer } from '../lib/stateBuffer'
import { GuiElement as GuiElementType } from '../api'
import GuiElement from './GuiElement'

interface GuiOverlayProps {
  stateBuffer: StateBuffer
  followPlayerId: string | null
  latestTick: number
  onGuiClick?: (elementId: number) => void
}

/**
 * GuiOverlay renders the 2D GUI for the followed player
 * It's positioned absolutely over the 3D canvas
 */
export default function GuiOverlay({ stateBuffer, followPlayerId, latestTick, onGuiClick }: GuiOverlayProps) {
  // Get GUI elements from the followed player
  const guiElements = useMemo<GuiElementType[]>(() => {
    if (!followPlayerId) return []

    const latest = stateBuffer.getLatest()
    if (!latest) return []

    const player = latest.players.get(followPlayerId)
    if (!player || !player.gui) return []

    return player.gui
  }, [stateBuffer, followPlayerId, latestTick])

  if (guiElements.length === 0) {
    return null
  }

  // Sort ScreenGuis by display_order
  const sortedGuis = [...guiElements].sort((a, b) => {
    const orderA = a.display_order ?? 0
    const orderB = b.display_order ?? 0
    return orderA - orderB
  })

  return (
    <div
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        pointerEvents: 'none',
        overflow: 'hidden',
      }}
    >
      <div
        style={{
          position: 'relative',
          width: '100%',
          height: '100%',
          pointerEvents: 'auto',
        }}
      >
        {sortedGuis.map((gui) => (
          <GuiElement key={gui.id} element={gui} onGuiClick={onGuiClick} />
        ))}
      </div>
    </div>
  )
}

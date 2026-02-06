import * as pako from 'pako'

export interface UDim2 {
  x_scale: number
  x_offset: number
  y_scale: number
  y_offset: number
}

export interface GuiElement {
  id: number
  type: 'ScreenGui' | 'Frame' | 'TextLabel' | 'TextButton' | 'ImageLabel' | 'ImageButton'
  name: string
  position?: UDim2
  size?: UDim2
  anchor_point?: [number, number]
  rotation?: number
  z_index?: number
  visible?: boolean
  background_color?: [number, number, number]
  background_transparency?: number
  border_color?: [number, number, number]
  border_size_pixel?: number
  text?: string
  text_color?: [number, number, number]
  text_size?: number
  text_transparency?: number
  text_x_alignment?: 'Left' | 'Center' | 'Right'
  text_y_alignment?: 'Top' | 'Center' | 'Bottom'
  image?: string
  image_color?: [number, number, number]
  image_transparency?: number
  display_order?: number
  enabled?: boolean
  children: GuiElement[]
}

export interface SpectatorPlayerInfo {
  id: string
  name: string
  position: [number, number, number]
  health: number
  attributes?: Record<string, unknown>
  gui?: GuiElement[]
}

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

export interface SpectatorEntity {
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

export interface SpectatorObservation {
  instance_id: string
  tick: number
  /** Milliseconds since game instance was created (for client clock synchronization) */
  server_time_ms: number
  game_status: string
  players: SpectatorPlayerInfo[]
  entities: SpectatorEntity[]
}

export async function fetchGameState(gameId: string): Promise<SpectatorObservation> {
  const response = await fetch(`/api/v1/games/${gameId}/spectate`)
  if (!response.ok) {
    throw new Error(`Failed to fetch game state: ${response.statusText}`)
  }
  return response.json()
}

export interface WebSocketMessage {
  type: 'state' | 'error'
  data?: SpectatorObservation
  error?: string
}

export function createGameWebSocket(
  gameId: string,
  onState: (state: SpectatorObservation) => void,
  onError: (error: string) => void,
  onClose: () => void
): { close: () => void } {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const ws = new WebSocket(`${protocol}//${window.location.host}/api/v1/games/${gameId}/spectate/ws`)

  ws.binaryType = 'arraybuffer'

  ws.onmessage = (event) => {
    try {
      let jsonStr: string
      if (event.data instanceof ArrayBuffer) {
        // Binary message - decompress gzip
        const decompressed = pako.ungzip(new Uint8Array(event.data))
        jsonStr = new TextDecoder().decode(decompressed)
      } else {
        // Text message - use as-is
        jsonStr = event.data
      }
      const data = JSON.parse(jsonStr)
      if (data.error) {
        onError(data.error)
      } else {
        onState(data as SpectatorObservation)
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      onError(`Failed to parse game state: ${message}`)
    }
  }

  ws.onerror = () => {
    onError('WebSocket connection error')
  }

  ws.onclose = () => {
    onClose()
  }

  return {
    close: () => {
      ws.close()
    }
  }
}

export interface GameListItem {
  id: string
  name: string
  description: string | null
  game_type: string
  status: string
  max_players: number
  player_count: number | null
  is_running: boolean
}

export async function listGames(): Promise<GameListItem[]> {
  const response = await fetch('/api/v1/games')
  if (!response.ok) {
    throw new Error(`Failed to list games: ${response.statusText}`)
  }
  const data = await response.json()
  return data.games
}

export interface ChatMessage {
  id: string
  agent_id: string
  agent_name: string
  content: string
  created_at: string
}

export async function fetchChatMessages(
  gameId: string,
  instanceId: string,
  after?: string,
  limit?: number
): Promise<ChatMessage[]> {
  const params = new URLSearchParams({ instance_id: instanceId })
  if (after) params.set('after', after)
  if (limit) params.set('limit', String(limit))
  const response = await fetch(`/api/v1/games/${gameId}/chat/messages?${params}`)
  if (!response.ok) {
    throw new Error(`Failed to fetch chat messages: ${response.statusText}`)
  }
  const data = await response.json()
  return data.messages
}

export async function sendGuiClick(gameId: string, agentId: string, elementId: number): Promise<void> {
  const response = await fetch(`/api/v1/games/${gameId}/input`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      agent_id: agentId,
      input: {
        type: 'GuiClick',
        data: { element_id: elementId },
      },
    }),
  })
  if (!response.ok) {
    console.error('Failed to send GUI click:', response.statusText)
  }
}

export interface SpectatorPlayerInfo {
  id: string
  name: string
  position: [number, number, number]
  health: number
  attributes?: Record<string, unknown>
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
}

export interface SpectatorObservation {
  tick: number
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

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data)
      if (data.error) {
        onError(data.error)
      } else {
        onState(data as SpectatorObservation)
      }
    } catch {
      onError('Failed to parse game state')
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

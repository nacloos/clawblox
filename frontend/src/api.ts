export interface SpectatorPlayerInfo {
  id: string
  position: [number, number, number]
  health: number
  ammo: number
  score: number
}

export interface SpectatorEntity {
  id: number
  type: string
  position: [number, number, number]
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

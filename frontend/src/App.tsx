import { useState, useEffect, useCallback } from 'react'
import { fetchGameState, listGames, SpectatorObservation } from './api'
import GameScene from './components/GameScene'

function App() {
  const [gameId, setGameId] = useState<string | null>(null)
  const [gameState, setGameState] = useState<SpectatorObservation | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [games, setGames] = useState<{ id: string; status: string }[]>([])

  const refreshGames = useCallback(async () => {
    try {
      const gamesList = await listGames()
      setGames(gamesList)
      if (!gameId && gamesList.length > 0) {
        setGameId(gamesList[0].id)
      }
    } catch (e) {
      console.error('Failed to list games:', e)
    }
  }, [gameId])

  useEffect(() => {
    refreshGames()
    const interval = setInterval(refreshGames, 5000)
    return () => clearInterval(interval)
  }, [refreshGames])

  useEffect(() => {
    if (!gameId) return

    const poll = async () => {
      try {
        const state = await fetchGameState(gameId)
        setGameState(state)
        setError(null)
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Unknown error')
      }
    }

    poll()
    const interval = setInterval(poll, 500)
    return () => clearInterval(interval)
  }, [gameId])

  return (
    <div style={{ width: '100%', height: '100%', position: 'relative' }}>
      <div style={{
        position: 'absolute',
        top: 10,
        left: 10,
        zIndex: 10,
        background: 'rgba(0,0,0,0.7)',
        padding: '10px',
        borderRadius: '4px',
        color: 'white',
        fontFamily: 'monospace',
        fontSize: '12px',
      }}>
        <div style={{ marginBottom: '8px' }}>
          <label>Game: </label>
          <select
            value={gameId || ''}
            onChange={(e) => setGameId(e.target.value || null)}
            style={{ marginLeft: '4px' }}
          >
            <option value="">Select a game...</option>
            {games.map((g) => (
              <option key={g.id} value={g.id}>
                {g.id.slice(0, 8)} ({g.status})
              </option>
            ))}
          </select>
          <button onClick={refreshGames} style={{ marginLeft: '8px' }}>
            Refresh
          </button>
        </div>
        {gameState && (
          <div>
            <div>Tick: {gameState.tick}</div>
            <div>Status: {gameState.game_status}</div>
            <div>Players: {gameState.players.length}</div>
            <div>Entities: {gameState.entities.length}</div>
          </div>
        )}
        {error && <div style={{ color: '#ff6b6b' }}>Error: {error}</div>}
      </div>

      {gameState ? (
        <GameScene gameState={gameState} />
      ) : (
        <div style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100%',
          color: 'white',
          fontFamily: 'monospace',
        }}>
          {error || 'Select or create a game to spectate...'}
        </div>
      )}
    </div>
  )
}

export default App

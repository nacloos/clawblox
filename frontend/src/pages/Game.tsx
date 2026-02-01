import { useState, useEffect } from 'react'
import { useParams, Link } from 'react-router-dom'
import { fetchGameState, SpectatorObservation } from '../api'
import GameScene from '../components/GameScene'

export default function Game() {
  const { id } = useParams<{ id: string }>()
  const [gameState, setGameState] = useState<SpectatorObservation | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!id) return

    const poll = async () => {
      try {
        const state = await fetchGameState(id)
        setGameState(state)
        setError(null)
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Failed to load game')
      }
    }

    poll()
    const interval = setInterval(poll, 500)
    return () => clearInterval(interval)
  }, [id])

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <div className="flex items-center gap-4 p-4 border-b border-gray-200 bg-white">
        <Link
          to="/"
          className="text-sm text-gray-500 hover:text-gray-900 transition-colors"
        >
          Back
        </Link>

        {gameState && (
          <div className="flex items-center gap-4 ml-auto text-sm">
            <span className="text-gray-500">{gameState.players.length} players</span>
            <span className="text-gray-500">Tick {gameState.tick}</span>
            <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${
              gameState.game_status === 'playing'
                ? 'bg-green-100 text-green-700'
                : gameState.game_status === 'waiting'
                ? 'bg-yellow-100 text-yellow-700'
                : 'bg-gray-100 text-gray-600'
            }`}>
              {gameState.game_status}
            </span>
          </div>
        )}
      </div>

      {/* Game view */}
      <div className="flex-1 relative bg-gray-50">
        {error ? (
          <div className="flex items-center justify-center h-full">
            <div className="text-center">
              <p className="text-red-500">{error}</p>
              <Link to="/" className="text-sm text-gray-500 hover:text-gray-900 mt-2 inline-block">
                Return to home
              </Link>
            </div>
          </div>
        ) : gameState ? (
          gameState.game_status === 'not_running' ? (
            <div className="flex items-center justify-center h-full">
              <div className="text-center">
                <p className="text-gray-500 mb-2">This game is not running yet.</p>
                <p className="text-sm text-gray-400">
                  Join with an agent to start playing.
                </p>
              </div>
            </div>
          ) : (
            <GameScene gameState={gameState} />
          )
        ) : (
          <div className="flex items-center justify-center h-full">
            <div className="text-gray-400">Loading...</div>
          </div>
        )}
      </div>
    </div>
  )
}

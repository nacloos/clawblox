import { useState, useEffect } from 'react'
import { useParams, Link } from 'react-router-dom'
import { ArrowLeft, Users } from 'lucide-react'
import { fetchGameState, SpectatorObservation } from '../api'
import GameScene from '../components/GameScene'
import { Button } from '@/components/ui/button'

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
    <div className="h-full flex flex-col bg-background">
      {/* Header */}
      <div className="flex items-center gap-4 px-4 py-3 border-b border-border bg-card">
        <Link to="/">
          <Button variant="ghost" size="icon" className="h-8 w-8">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        </Link>

        {gameState && (
          <div className="flex items-center gap-4 ml-auto text-sm">
            <span className="text-muted-foreground flex items-center gap-1.5">
              <Users className="h-4 w-4" />
              {gameState.players.length}
            </span>
            <span className="text-muted-foreground">Tick {gameState.tick}</span>
            <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${
              gameState.game_status === 'playing'
                ? 'bg-green-500/20 text-green-400'
                : gameState.game_status === 'waiting'
                ? 'bg-yellow-500/20 text-yellow-400'
                : 'bg-muted text-muted-foreground'
            }`}>
              {gameState.game_status}
            </span>
          </div>
        )}
      </div>

      {/* Game view - flex-1 with min-h-0 to allow proper sizing */}
      <div className="flex-1 min-h-0 relative">
        {error ? (
          <div className="flex items-center justify-center h-full">
            <div className="text-center">
              <p className="text-destructive">{error}</p>
              <Link to="/" className="text-sm text-muted-foreground hover:text-foreground mt-2 inline-block">
                Return to home
              </Link>
            </div>
          </div>
        ) : gameState ? (
          gameState.game_status === 'not_running' ? (
            <div className="flex items-center justify-center h-full">
              <div className="text-center">
                <p className="text-muted-foreground mb-2">This game is not running yet.</p>
                <p className="text-sm text-muted-foreground/60">
                  Join with an agent to start playing.
                </p>
              </div>
            </div>
          ) : (
            <GameScene gameState={gameState} />
          )
        ) : (
          <div className="flex items-center justify-center h-full">
            <div className="text-muted-foreground">Loading...</div>
          </div>
        )}
      </div>
    </div>
  )
}

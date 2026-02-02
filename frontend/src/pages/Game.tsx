import { useState, useEffect, useRef, useCallback } from 'react'
import { useParams, Link } from 'react-router-dom'
import { ArrowLeft, Users } from 'lucide-react'
import { createGameWebSocket, fetchGameState, SpectatorObservation } from '../api'
import GameScene from '../components/GameScene'
import PlayerList from '../components/PlayerList'
import { Button } from '@/components/ui/button'

export default function Game() {
  const { id } = useParams<{ id: string }>()
  const [gameState, setGameState] = useState<SpectatorObservation | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [connectionStatus, setConnectionStatus] = useState<'connecting' | 'connected' | 'disconnected'>('connecting')
  const [selectedPlayerId, setSelectedPlayerId] = useState<string | null>(null)
  const wsRef = useRef<{ close: () => void } | null>(null)
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const connect = useCallback(() => {
    if (!id) return

    setConnectionStatus('connecting')
    setError(null)

    wsRef.current = createGameWebSocket(
      id,
      (state) => {
        setGameState(state)
        setConnectionStatus('connected')
        setError(null)
      },
      (err) => {
        setError(err)
        setConnectionStatus('disconnected')
      },
      () => {
        setConnectionStatus('disconnected')
        // Attempt reconnect after 2 seconds
        reconnectTimeoutRef.current = setTimeout(() => {
          connect()
        }, 2000)
      }
    )
  }, [id])

  useEffect(() => {
    if (!id) return

    // Initial fetch to get state while WebSocket connects
    fetchGameState(id)
      .then((state) => {
        setGameState(state)
        setError(null)
      })
      .catch((e) => {
        setError(e instanceof Error ? e.message : 'Failed to load game')
      })

    // Connect WebSocket
    connect()

    return () => {
      wsRef.current?.close()
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current)
      }
    }
  }, [id, connect])

  // Reset selection if selected player disconnects
  useEffect(() => {
    if (!gameState || !selectedPlayerId) return

    const playerExists = gameState.players.some(p => p.id === selectedPlayerId)
    if (!playerExists) {
      setSelectedPlayerId(null)
    }
  }, [gameState, selectedPlayerId])

  return (
    <div className="h-full flex flex-col bg-background">
      {/* Header */}
      <div className="flex items-center gap-4 px-4 py-3 border-b border-border bg-card pr-16">
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
            {connectionStatus === 'connected' && (
              <span className="w-2 h-2 rounded-full bg-green-500" />
            )}
            {connectionStatus === 'connecting' && (
              <span className="w-2 h-2 rounded-full bg-yellow-500 animate-pulse" />
            )}
            {connectionStatus === 'disconnected' && (
              <span className="w-2 h-2 rounded-full bg-red-500" />
            )}
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
            <>
              <GameScene gameState={gameState} followPlayerId={selectedPlayerId} />
              <PlayerList
                players={gameState.players}
                selectedPlayerId={selectedPlayerId}
                onSelectPlayer={setSelectedPlayerId}
              />
            </>
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

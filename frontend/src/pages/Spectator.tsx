import { useState, useEffect, useRef, useCallback } from 'react'
import { useParams, useSearchParams, Link } from 'react-router-dom'
import { ArrowLeft, Users } from 'lucide-react'
import { createGameWebSocket, fetchGameState, SpectatorObservation, SpectatorPlayerInfo, sendGuiClick } from '../api'
import GameScene from '../components/GameScene'
import PlayerList from '../components/PlayerList'
import GuiOverlay from '../components/GuiOverlay'
import ChatPanel from '../components/ChatPanel'
import { Button } from '@/components/ui/button'
import { StateBuffer } from '../lib/stateBuffer'

function arraysEqual<T>(a: T[], b: T[]): boolean {
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false
  }
  return true
}

export default function Game() {
  const { id } = useParams<{ id: string }>()
  const [searchParams] = useSearchParams()
  const gameType = searchParams.get('gameType')
  const [error, setError] = useState<string | null>(null)
  const [connectionStatus, setConnectionStatus] = useState<'connecting' | 'connected' | 'disconnected'>('connecting')
  const [selectedPlayerId, setSelectedPlayerId] = useState<string | null>(null)
  const [isPlayerListOpen, setIsPlayerListOpen] = useState(false)
  const [latestTick, setLatestTick] = useState<number>(0)
  const [instanceId, setInstanceId] = useState<string | null>(null)

  // Use refs for high-frequency state to avoid React re-renders
  const stateBufferRef = useRef(new StateBuffer())

  // React state only for structural changes (entity/player list changes)
  const [entityIds, setEntityIds] = useState<number[]>([])
  const [players, setPlayers] = useState<SpectatorPlayerInfo[]>([])
  const [gameStatus, setGameStatus] = useState<string | null>(null)

  const wsRef = useRef<{ close: () => void } | null>(null)
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const handleState = useCallback((state: SpectatorObservation) => {
    // Push to buffer (no re-render)
    stateBufferRef.current.push(state)

    // Only update React state if entity list changed
    const newIds = state.entities.map(e => e.id).sort((a, b) => a - b)
    setEntityIds(prev => {
      if (!arraysEqual(prev, newIds)) {
        return newIds
      }
      return prev
    })

    // Update players for the player list (structural changes only)
    setPlayers(prev => {
      const prevIds = prev.map(p => p.id).sort()
      const newPlayerIds = state.players.map(p => p.id).sort()
      // Check if player list structure changed (ids changed)
      if (!arraysEqual(prevIds, newPlayerIds)) {
        return state.players
      }
      // Also update if player names changed
      const nameChanged = state.players.some((p, i) => {
        const prevPlayer = prev.find(pp => pp.id === p.id)
        return prevPlayer && prevPlayer.name !== p.name
      })
      if (nameChanged) {
        return state.players
      }
      return prev
    })

    // Update game status if changed
    setGameStatus(prev => {
      if (prev !== state.game_status) {
        return state.game_status
      }
      return prev
    })

    setLatestTick(state.tick)

    // Track instance_id for chat
    setInstanceId(prev => prev !== state.instance_id ? state.instance_id : prev)

    setConnectionStatus('connected')
    setError(null)
  }, [])

  const connect = useCallback(() => {
    if (!id) return

    setConnectionStatus('connecting')
    setError(null)

    wsRef.current = createGameWebSocket(
      id,
      handleState,
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
  }, [id, handleState])

  useEffect(() => {
    if (!id) return

    // Initial fetch to get state while WebSocket connects
    fetchGameState(id)
      .then((state) => {
        handleState(state)
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
  }, [id, connect, handleState])

  // Reset selection if selected player disconnects
  useEffect(() => {
    if (players.length === 0 || !selectedPlayerId) return

    const playerExists = players.some(p => p.id === selectedPlayerId)
    if (!playerExists) {
      setSelectedPlayerId(null)
    }
  }, [players, selectedPlayerId])

  // Handle GUI click events
  const handleGuiClick = useCallback((elementId: number) => {
    if (!id || !selectedPlayerId) return
    sendGuiClick(id, selectedPlayerId, elementId)
  }, [id, selectedPlayerId])

  const hasGameData = stateBufferRef.current.hasData()

  return (
    <div className="h-screen flex flex-col bg-background">
      {/* Header */}
      <div className="flex items-center gap-4 px-4 py-3 border-b border-border bg-card pr-16">
        <Link to={gameType ? `/games/${gameType}/sessions` : '/'}>
          <Button variant="ghost" size="icon" className="h-8 w-8">
            <ArrowLeft className="h-4 w-4" />
          </Button>
        </Link>

        {hasGameData && (
          <div className="flex items-center gap-4 ml-auto text-sm">
            <Button
              variant="outline"
              size="sm"
              className="h-8 gap-1.5"
              onClick={() => setIsPlayerListOpen(prev => !prev)}
            >
              <Users className="h-4 w-4" />
              {players.length}
            </Button>
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
              <Link to={gameType ? `/games/${gameType}/sessions` : '/'} className="text-sm text-muted-foreground hover:text-foreground mt-2 inline-block">
                {gameType ? 'Return to sessions' : 'Return to home'}
              </Link>
            </div>
          </div>
        ) : hasGameData ? (
          gameStatus === 'not_running' ? (
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
              <GameScene
                stateBuffer={stateBufferRef.current}
                entityIds={entityIds}
                followPlayerId={selectedPlayerId}
              />
              <GuiOverlay
                stateBuffer={stateBufferRef.current}
                followPlayerId={selectedPlayerId}
                latestTick={latestTick}
                onGuiClick={handleGuiClick}
              />
              {isPlayerListOpen && (
                <PlayerList
                  players={players}
                  selectedPlayerId={selectedPlayerId}
                  onSelectPlayer={setSelectedPlayerId}
                />
              )}
              <ChatPanel gameId={id!} instanceId={instanceId} />
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

import { useState, useEffect } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { ArrowLeft } from 'lucide-react'
import { listGames, GameListItem } from '../api'
import { getFeaturedGameById, getGameAsset } from '../config/games'
import { getGameById } from '../config/platformData'
import SessionCard from '../components/SessionCard'
import GameInstructions from '../components/GameInstructions'
import { Button } from '../components/ui/button'
import { Badge } from '../components/ui/badge'

function LoadingSkeleton() {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {[...Array(3)].map((_, i) => (
        <div key={i} className="rounded-xl bg-card animate-pulse border border-border h-32" />
      ))}
    </div>
  )
}

export default function GameSessions() {
  const { gameType } = useParams<{ gameType: string }>()
  const navigate = useNavigate()
  const [sessions, setSessions] = useState<GameListItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const featuredGame = gameType ? getFeaturedGameById(gameType) : null
  const platformGame = gameType ? getGameById(gameType) : null

  useEffect(() => {
    const loadSessions = async () => {
      try {
        setLoading(true)
        const allGames = await listGames()

        // Filter games that match the featured game name
        const filtered = allGames.filter(g =>
          featuredGame && g.name.toLowerCase().includes(featuredGame.name.toLowerCase())
        )

        setSessions(filtered)
        setError(null)
      } catch (e) {
        console.error('Failed to load sessions:', e)
        setError(e instanceof Error ? e.message : 'Failed to load sessions. Make sure the backend is running.')
      } finally {
        setLoading(false)
      }
    }

    loadSessions()
    // Refresh every 10 seconds
    const interval = setInterval(loadSessions, 10000)
    return () => clearInterval(interval)
  }, [gameType, featuredGame])

  const handleSpectate = (gameId: string) => {
    navigate(`/spectate/${gameId}?gameType=${gameType}`)
  }

  if (!featuredGame) {
    return (
      <div className="p-8">
        <p className="text-destructive">Game not found</p>
      </div>
    )
  }

  return (
    <div className="p-8 pb-20">
      {/* Header */}
      <div className="mb-8">
        <Button
          variant="ghost"
          onClick={() => navigate('/browse')}
          className="mb-4 -ml-2"
        >
          <ArrowLeft className="h-4 w-4 mr-2" />
          Back to Games
        </Button>

        <div className="flex items-start gap-6">
          {/* Thumbnail */}
          <img
            src={getGameAsset(featuredGame.assetName, 'image')}
            alt={featuredGame.name}
            className="w-48 h-64 object-cover rounded-xl border border-border"
          />

          {/* Info */}
          <div className="flex-1">
            <div className="flex items-start justify-between">
              <div>
                <h1 className="text-4xl font-bold text-foreground mb-2">{featuredGame.name}</h1>
                <p className="text-lg text-muted-foreground mb-4">{featuredGame.description}</p>
              </div>
              {platformGame?.isLive && (
                <Badge variant="default" className="bg-red-500 text-white">
                  LIVE
                </Badge>
              )}
            </div>

            <div className="mt-6 p-4 rounded-lg bg-card border border-border">
              <div className="flex items-center gap-6">
                <div>
                  <div className="text-2xl font-bold text-primary">
                    {platformGame?.agentCount.toLocaleString() || 0}
                  </div>
                  <div className="text-sm text-muted-foreground">Active Agents</div>
                </div>
                <div>
                  <div className="text-2xl font-bold text-primary">{sessions.length}</div>
                  <div className="text-sm text-muted-foreground">Live Sessions</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Instructions */}
      <div className="mb-8">
        <GameInstructions
          gameType={featuredGame.gameType}
          gameName={featuredGame.name}
          gameId={sessions.length > 0 ? sessions[0].id : undefined}
        />
      </div>

      {/* Error */}
      {error && (
        <div className="p-4 rounded-xl bg-destructive/10 border border-destructive/20 text-destructive text-sm">
          {error}
        </div>
      )}

      {/* Live Sessions */}
      <section>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-xl font-semibold text-foreground">Live Sessions</h2>
          {!loading && sessions.length > 0 && (
            <span className="text-sm text-muted-foreground">
              {sessions.length} {sessions.length === 1 ? 'session' : 'sessions'} active
            </span>
          )}
        </div>

        {loading ? (
          <LoadingSkeleton />
        ) : sessions.length === 0 ? (
          <div className="text-center py-12 rounded-xl bg-card border border-border">
            <p className="text-muted-foreground">No active sessions yet.</p>
            <p className="text-muted-foreground/60 text-sm mt-1">
              Start a new session using the API above.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {sessions.map((session) => (
              <SessionCard
                key={session.id}
                game={session}
                onSpectate={handleSpectate}
              />
            ))}
          </div>
        )}
      </section>
    </div>
  )
}

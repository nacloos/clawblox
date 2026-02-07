import { useState, useEffect } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { ArrowLeft } from 'lucide-react'
import { listGames, GameListItem } from '../api'
import { getFeaturedGameById } from '../config/games'
import SessionCard from '../components/SessionCard'
import GameInstructions from '../components/GameInstructions'
import { Button } from '../components/ui/button'

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
        setError(e instanceof Error ? e.message : 'Failed to load sessions')
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
    <div className="p-8 pb-20 space-y-8">
      {/* Header */}
      <div>
        <Button
          variant="ghost"
          onClick={() => navigate('/')}
          className="mb-4 -ml-2"
        >
          <ArrowLeft className="h-4 w-4 mr-2" />
          Back to Games
        </Button>
        <h1 className="text-3xl font-bold text-foreground">{featuredGame.name}</h1>
        <p className="text-muted-foreground mt-2">{featuredGame.description}</p>
      </div>

      {/* Instructions */}
      <GameInstructions gameType={featuredGame.gameType} />

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

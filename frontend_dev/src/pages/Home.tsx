import { useState, useEffect, useCallback } from 'react'
import { listGames, GameListItem } from '../api'
import GameCard from '../components/GameCard'

function LoadingSkeleton() {
  return (
    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4">
      {[...Array(5)].map((_, i) => (
        <div key={i} className="rounded-2xl bg-card animate-pulse">
          <div className="aspect-[4/3]" />
          <div className="p-4 space-y-2">
            <div className="h-4 bg-muted rounded w-2/3" />
            <div className="h-3 bg-muted rounded w-full" />
          </div>
        </div>
      ))}
    </div>
  )
}

export default function Home() {
  const [games, setGames] = useState<GameListItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setLoading(true)
      const gamesList = await listGames()
      setGames(gamesList)
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load games')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh()
    const interval = setInterval(refresh, 10000)
    return () => clearInterval(interval)
  }, [refresh])

  return (
    <div className="p-8 pb-20 space-y-8">
      {/* Header */}
      <h1 className="text-xl font-medium text-foreground">Games</h1>

      {/* Error */}
      {error && (
        <div className="p-4 rounded-xl bg-destructive/10 border border-destructive/20 text-destructive text-sm">
          {error}
        </div>
      )}

      {/* Games Grid */}
      {loading ? (
        <LoadingSkeleton />
      ) : games.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-muted-foreground">No games available yet.</p>
          <p className="text-muted-foreground/60 text-sm mt-1">
            Create a game using the API to get started.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4">
          {games.map((game) => (
            <GameCard key={game.id} game={game} />
          ))}
        </div>
      )}
    </div>
  )
}

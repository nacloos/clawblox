import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { FEATURED_GAMES } from '../config/games'
import { listGames, GameListItem } from '../api'
import FeaturedGameCard from '../components/FeaturedGameCard'
import ComingSoonSection from '../components/ComingSoonSection'
import Hero from '../components/Hero'
import GettingStarted from '../components/GettingStarted'

function LoadingSkeleton() {
  return (
    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
      {[...Array(4)].map((_, i) => (
        <div key={i} className="rounded-2xl bg-card animate-pulse border border-border">
          <div className="aspect-square" />
          <div className="p-4 space-y-2">
            <div className="h-4 bg-muted rounded w-2/3" />
            <div className="h-3 bg-muted rounded w-full" />
          </div>
        </div>
      ))}
    </div>
  )
}

export default function Landing() {
  const navigate = useNavigate()
  const [allGames, setAllGames] = useState<GameListItem[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const loadGames = async () => {
      try {
        const games = await listGames()
        setAllGames(games)
        setError(null)
      } catch (error) {
        console.error('Failed to load games:', error)
        setError(error instanceof Error ? error.message : 'Failed to load games')
      } finally {
        setLoading(false)
      }
    }

    loadGames()
    // Refresh every 30 seconds
    const interval = setInterval(loadGames, 30000)
    return () => clearInterval(interval)
  }, [])

  const handleGameClick = (gameId: string) => {
    navigate(`/games/${gameId}/sessions`)
  }

  return (
    <div className="pb-20">
      {/* Hero Section */}
      <Hero />

      {/* Getting Started Section */}
      <GettingStarted />

      {/* Featured Games Section */}
      <section id="featured-games" className="py-16 px-8 scroll-mt-20">
        <div className="max-w-6xl mx-auto">
          <h2 className="text-3xl font-bold mb-6 text-foreground">Featured Games</h2>
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
            {FEATURED_GAMES.map((game) => (
              <FeaturedGameCard
                key={game.id}
                game={game}
                onClick={() => handleGameClick(game.id)}
              />
            ))}
          </div>
        </div>
      </section>

      {/* Agent Creations Section - All Games */}
      <section id="agent-creations" className="py-16 px-8 scroll-mt-20">
        <div className="max-w-6xl mx-auto">
          {error ? (
            <div className="text-center py-12 rounded-xl bg-card/50 border border-destructive/20">
              <p className="text-destructive mb-2">Failed to load games</p>
              <p className="text-sm text-muted-foreground">{error}</p>
              <p className="text-xs text-muted-foreground mt-2">
                Make sure the backend is running on port 8080
              </p>
            </div>
          ) : (
            <ComingSoonSection
              title="All Games"
              games={allGames}
              loading={loading}
              onGameClick={(game) => navigate(`/spectate/${game.id}`)}
            />
          )}
        </div>
      </section>
    </div>
  )
}

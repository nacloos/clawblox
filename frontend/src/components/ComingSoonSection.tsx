import type { GameListItem } from '../api'
import { Badge } from './ui/badge'

interface ComingSoonSectionProps {
  title?: string
  games: GameListItem[]
  loading?: boolean
  onGameClick: (game: GameListItem) => void
}

export default function ComingSoonSection({
  title = 'Agent-Generated Games',
  games,
  loading = false,
  onGameClick
}: ComingSoonSectionProps) {
  if (loading) {
    return (
      <section className="relative">
        <h2 className="text-2xl font-semibold mb-6 text-foreground">{title}</h2>
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
          {[...Array(6)].map((_, i) => (
            <div key={i} className="rounded-2xl bg-card animate-pulse border border-border">
              <div className="aspect-square" />
              <div className="p-4 space-y-2">
                <div className="h-4 bg-muted rounded w-2/3" />
                <div className="h-3 bg-muted rounded w-full" />
              </div>
            </div>
          ))}
        </div>
      </section>
    )
  }

  if (games.length === 0) {
    return (
      <section className="relative">
        <h2 className="text-2xl font-semibold mb-6 text-foreground">{title}</h2>
        <div className="text-center py-12 rounded-xl bg-card/50 border border-border">
          <p className="text-muted-foreground">No agent-generated games yet.</p>
          <p className="text-sm text-muted-foreground/60 mt-1">
            Create your first game using the API!
          </p>
        </div>
      </section>
    )
  }

  return (
    <section className="relative">
      <h2 className="text-2xl font-semibold mb-6 text-foreground">{title}</h2>

      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
        {games.map((game) => (
          <button
            key={game.id}
            onClick={() => onGameClick(game)}
            className="group text-left transition-transform hover:scale-[1.02]"
          >
            <div className="aspect-square flex items-center justify-center overflow-hidden rounded-2xl bg-card border border-border transition-all group-hover:border-primary/50">
              <div className="w-full h-full relative">
                {/* Use default thumbnail */}
                <img
                  src="/games/tsunami.png"
                  alt={game.name}
                  className="w-full h-full object-cover"
                />
                {/* Live badge */}
                {game.is_running && (
                  <div className="absolute top-2 right-2">
                    <Badge variant="default" className="bg-green-500/20 text-green-400 border-green-500/30">
                      Live
                    </Badge>
                  </div>
                )}
              </div>
            </div>
            <div className="mt-3">
              <h3 className="font-semibold text-foreground truncate">{game.name}</h3>
              {game.description && (
                <p className="text-sm text-muted-foreground mt-1 line-clamp-2">
                  {game.description}
                </p>
              )}
              <div className="flex items-center gap-2 mt-2">
                <Badge variant="secondary" className="text-xs">
                  {game.game_type}
                </Badge>
                {game.player_count !== null && (
                  <span className="text-xs text-muted-foreground">
                    {game.player_count} / {game.max_players} players
                  </span>
                )}
              </div>
            </div>
          </button>
        ))}
      </div>
    </section>
  )
}

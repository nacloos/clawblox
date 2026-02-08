import { Users, Eye } from 'lucide-react'
import type { GameListItem } from '../api'
import { Badge } from './ui/badge'
import { Button } from './ui/button'

interface SessionCardProps {
  game: GameListItem
  onSpectate: (gameId: string) => void
}

export default function SessionCard({ game, onSpectate }: SessionCardProps) {
  return (
    <div className="rounded-xl bg-card border border-border overflow-hidden transition-all hover:border-primary/50 hover:shadow-lg">
      {/* Header */}
      <div className="p-4 border-b border-border">
        <div className="flex items-start justify-between">
          <div className="flex-1 min-w-0">
            <h3 className="font-semibold text-card-foreground truncate">{game.name}</h3>
            {game.description && (
              <p className="text-sm text-muted-foreground mt-1 line-clamp-1">{game.description}</p>
            )}
          </div>
          {game.is_running && (
            <Badge variant="default" className="ml-2 bg-green-500/20 text-green-400 border-green-500/30">
              Live
            </Badge>
          )}
        </div>
      </div>

      {/* Stats & Action */}
      <div className="p-4 flex items-center justify-between">
        <div className="flex items-center gap-4 text-sm text-muted-foreground">
          <div className="flex items-center gap-1.5">
            <Users className="h-4 w-4" />
            <span>{game.player_count ?? 0} / {game.max_players}</span>
          </div>
          <Badge variant="secondary" className="text-xs">
            {game.game_type}
          </Badge>
        </div>

        <Button
          size="sm"
          onClick={() => onSpectate(game.id)}
          className="gap-2"
        >
          <Eye className="h-4 w-4" />
          Watch
        </Button>
      </div>
    </div>
  )
}

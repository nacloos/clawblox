import { Link } from 'react-router-dom'
import { Users } from 'lucide-react'
import { GameListItem } from '../api'

interface GameCardProps {
  game: GameListItem
}

export default function GameCard({ game }: GameCardProps) {
  return (
    <Link to={`/spectate/${game.id}`} className="group block">
      <div className="rounded-2xl bg-card border border-border overflow-hidden transition-all group-hover:scale-[1.02] group-hover:border-primary/50">
        {/* Image / Placeholder */}
        <div className="aspect-[4/3] bg-gradient-to-br from-muted to-muted/50 flex items-center justify-center relative">
          <span className="text-5xl font-bold text-muted-foreground/30">
            {game.name.charAt(0).toUpperCase()}
          </span>
          {game.is_running && (
            <span className="absolute top-2 right-2 px-2 py-0.5 bg-green-500/20 text-green-400 text-xs rounded-full font-medium">
              Live
            </span>
          )}
        </div>

        {/* Info */}
        <div className="p-4">
          <h3 className="font-medium text-card-foreground truncate">{game.name}</h3>
          {game.description && (
            <p className="text-sm text-muted-foreground mt-1 line-clamp-2">{game.description}</p>
          )}
          <div className="flex items-center gap-1.5 mt-2 text-xs text-muted-foreground">
            <Users className="h-3 w-3" />
            <span>{game.player_count ?? 0}</span>
          </div>
        </div>
      </div>
    </Link>
  )
}

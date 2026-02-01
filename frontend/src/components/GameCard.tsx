import { Link } from 'react-router-dom'
import { GameListItem } from '../api'

interface GameCardProps {
  game: GameListItem
}

export default function GameCard({ game }: GameCardProps) {
  return (
    <Link to={`/game/${game.id}`} className="group block">
      <div className="rounded-2xl bg-gray-100 overflow-hidden transition-transform group-hover:scale-[1.02]">
        {/* Image / Placeholder */}
        <div className="aspect-[4/3] bg-gradient-to-br from-gray-200 to-gray-300 flex items-center justify-center">
          <span className="text-5xl font-bold text-gray-400">
            {game.name.charAt(0).toUpperCase()}
          </span>
        </div>

        {/* Info */}
        <div className="p-4">
          <h3 className="font-medium text-gray-900 truncate">{game.name}</h3>
          {game.description && (
            <p className="text-sm text-gray-500 mt-1 line-clamp-2">{game.description}</p>
          )}
          <div className="flex items-center gap-3 mt-2 text-xs text-gray-400">
            <span>{game.player_count ?? 0}/{game.max_players} players</span>
            {game.is_running && (
              <span className="px-2 py-0.5 bg-green-100 text-green-700 rounded-full">Live</span>
            )}
          </div>
        </div>
      </div>
    </Link>
  )
}

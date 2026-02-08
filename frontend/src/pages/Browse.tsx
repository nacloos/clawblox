import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { GAME_CATEGORIES, getGamesByCategory } from '../config/platformData'
import { Badge } from '../components/ui/badge'

export default function Browse() {
  const navigate = useNavigate()
  const [selectedCategory, setSelectedCategory] = useState('all')
  const games = getGamesByCategory(selectedCategory)

  return (
    <div className="p-8">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-3xl font-bold text-foreground mb-2">Browse Games</h1>
        <p className="text-muted-foreground">
          Discover AI agents streaming across {games.length} games
        </p>
      </div>

      {/* Category Filter */}
      <div className="mb-8 flex flex-wrap gap-2">
        {GAME_CATEGORIES.map((category) => (
          <button
            key={category.id}
            onClick={() => setSelectedCategory(category.id)}
            className={`px-4 py-2 rounded-lg font-medium transition-all ${
              selectedCategory === category.id
                ? 'bg-primary text-primary-foreground'
                : 'bg-card border border-border hover:border-primary/50 text-foreground'
            }`}
            style={
              selectedCategory === category.id
                ? { backgroundColor: category.color }
                : undefined
            }
          >
            {category.name}
          </button>
        ))}
      </div>

      {/* Games Grid */}
      {games.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-muted-foreground">No games found in this category.</p>
        </div>
      ) : (
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-6">
          {games.map((game) => (
            <button
              key={game.id}
              onClick={() => {
                if (game.gameType === 'tsunami') {
                  navigate('/games/tsunami/sessions')
                } else {
                  navigate(`/game/${game.id}`)
                }
              }}
              className="group text-left transition-all duration-200 hover:-translate-x-1 hover:-translate-y-1"
            >
              {/* Thumbnail */}
              <div className="aspect-[3/4] relative rounded-xl overflow-hidden bg-card border-2 border-border transition-all duration-200 group-hover:border-t-0 group-hover:border-l-0 group-hover:border-b-[4px] group-hover:border-b-primary group-hover:border-r-[4px] group-hover:border-r-primary group-hover:shadow-2xl mb-3">
                <img
                  src={game.thumbnail}
                  alt={game.name}
                  className="w-full h-full object-cover"
                />

                {/* Live Badge */}
                {game.isLive && (
                  <div className="absolute top-2 left-2">
                    <Badge variant="default" className="bg-red-500 text-white">
                      LIVE
                    </Badge>
                  </div>
                )}
              </div>

              {/* Game Info */}
              <div>
                <h3 className="font-semibold text-foreground mb-1 line-clamp-1">
                  {game.name}
                </h3>

                {/* Agent Count */}
                <div className="text-sm text-primary font-medium mb-1">
                  {game.agentCount.toLocaleString()} agents streaming
                </div>

                <p className="text-sm text-muted-foreground mb-2 line-clamp-2">
                  {game.description}
                </p>

                {/* Tags */}
                <div className="flex flex-wrap gap-1">
                  {game.categories.slice(0, 2).map((cat) => (
                    <Badge key={cat} variant="secondary" className="text-xs">
                      {cat}
                    </Badge>
                  ))}
                </div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

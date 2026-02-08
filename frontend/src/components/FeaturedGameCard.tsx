import { useState } from 'react'
import type { FeaturedGame } from '../config/games'
import { getGameAsset } from '../config/games'
import { Badge } from './ui/badge'

interface FeaturedGameCardProps {
  game: FeaturedGame
  onClick?: () => void
}

export default function FeaturedGameCard({ game, onClick }: FeaturedGameCardProps) {
  const [imgError, setImgError] = useState(false)
  const [videoError, setVideoError] = useState(false)
  const [isHovered, setIsHovered] = useState(false)

  const imageSrc = getGameAsset(game.assetName, 'image')
  const videoSrc = getGameAsset(game.assetName, 'video')

  const handleClick = () => {
    if (game.isActive && onClick) {
      onClick()
    }
  }

  return (
    <div
      className={`relative group ${game.isActive ? 'cursor-pointer' : 'cursor-not-allowed'}`}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onClick={handleClick}
    >
      {/* Image/Video Container */}
      <div
        className={`aspect-square flex items-center justify-center overflow-hidden rounded-2xl bg-card transition-all duration-200 ${
          game.isActive ? 'group-hover:scale-[1.02] group-hover:border-primary/50' : ''
        } border border-border ${!game.isActive ? 'coming-soon-filter' : ''}`}
      >
        <div className="w-full h-full relative video-mask">
          {imgError ? (
            <div className="w-full h-full flex items-center justify-center bg-muted">
              <span className="text-5xl font-bold text-muted-foreground/30">
                {game.name.charAt(0).toUpperCase()}
              </span>
            </div>
          ) : isHovered && !videoError && game.isActive ? (
            <video
              src={videoSrc}
              autoPlay
              loop
              muted
              playsInline
              className="w-full h-full object-cover"
              onError={() => setVideoError(true)}
            />
          ) : (
            <img
              src={imageSrc}
              alt={game.name}
              className="w-full h-full object-cover"
              onError={() => setImgError(true)}
            />
          )}
        </div>
      </div>

      {/* Coming Soon Badge */}
      {!game.isActive && (
        <div className="absolute top-3 right-3">
          <Badge variant="secondary" className="bg-muted/80 backdrop-blur-sm">
            COMING SOON
          </Badge>
        </div>
      )}

      {/* Game Info */}
      <div className="mt-3">
        <h3 className="font-semibold text-foreground truncate">
          {game.name}
          {!game.isActive && <span className="text-muted-foreground text-sm ml-2">(COMING SOON)</span>}
        </h3>
        <p className="text-sm text-muted-foreground mt-1 line-clamp-2">
          {game.description}
        </p>
      </div>
    </div>
  )
}

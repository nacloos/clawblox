import { Trophy, Crosshair } from 'lucide-react'
import { SpectatorPlayerInfo } from '../api'
import { Badge } from '@/components/ui/badge'

interface PlayerListProps {
  players: SpectatorPlayerInfo[]
  selectedPlayerId: string | null
  onSelectPlayer: (playerId: string | null) => void
}

export default function PlayerList({ players, selectedPlayerId, onSelectPlayer }: PlayerListProps) {
  return (
    <div className="absolute top-4 right-4 bg-card/90 backdrop-blur-sm border border-border rounded-lg p-3 w-[200px] max-h-[400px] overflow-y-auto z-20">
      <div className="text-sm font-medium text-foreground mb-3">Players</div>

      <div className="space-y-2">
        {players.map((player) => {
          const kills = player.attributes?.Kills as number | undefined
          const currentWeapon = player.attributes?.CurrentWeapon as number | undefined

          return (
            <button
              key={player.id}
              onClick={() => onSelectPlayer(selectedPlayerId === player.id ? null : player.id)}
              className={`w-full text-left p-2 rounded-md transition-colors ${
                selectedPlayerId === player.id
                  ? 'bg-accent border border-accent-foreground/20'
                  : 'hover:bg-accent/50'
              }`}
            >
              <div className="text-sm font-medium text-foreground truncate mb-1.5">
                {player.name}
              </div>
              <div className="flex flex-wrap gap-1">
                {kills !== undefined && (
                  <Badge variant="secondary" className="text-xs gap-1">
                    <Trophy className="h-3 w-3 text-yellow-400" />
                    {kills}
                  </Badge>
                )}
                {currentWeapon !== undefined && (
                  <Badge variant="outline" className="text-xs gap-1">
                    <Crosshair className="h-3 w-3 text-blue-400" />
                    {currentWeapon}
                  </Badge>
                )}
              </div>
            </button>
          )
        })}

        {players.length === 0 && (
          <div className="text-sm text-muted-foreground text-center py-2">
            No players connected
          </div>
        )}
      </div>
    </div>
  )
}

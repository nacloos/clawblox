import { useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { ArrowLeft, Play, Users } from 'lucide-react'
import { getGameById } from '../config/platformData'
import { Button } from '../components/ui/button'
import { Badge } from '../components/ui/badge'

// Placeholder session data
interface SessionData {
  id: string
  agentCount: number
  status: 'live' | 'waiting'
}

function generatePlaceholderSessions(gameId: string): SessionData[] {
  const count = Math.floor(Math.random() * 5) + 3
  return Array.from({ length: count }, (_, i) => ({
    id: `${gameId}-session-${i + 1}`,
    agentCount: Math.floor(Math.random() * 8) + 1,
    status: i < 2 ? 'live' : 'waiting'
  }))
}

export default function GameDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const game = id ? getGameById(id) : undefined
  const [activeTab, setActiveTab] = useState<'prompt' | 'manual'>('prompt')

  if (!game) {
    return (
      <div className="p-8">
        <p className="text-destructive">Game not found</p>
      </div>
    )
  }

  // For Tsunami, navigate to real game sessions
  if (game.gameType === 'tsunami') {
    // Redirect to the real tsunami sessions page
    navigate(`/games/tsunami/sessions`, { replace: true })
    return null
  }

  // Placeholder sessions for other games
  const sessions = generatePlaceholderSessions(game.id)

  return (
    <div className="p-8 pb-20">
      {/* Header */}
      <div className="mb-8">
        <Button
          variant="ghost"
          onClick={() => navigate('/browse')}
          className="mb-4 -ml-2"
        >
          <ArrowLeft className="h-4 w-4 mr-2" />
          Back to Browse
        </Button>

        <div className="flex items-start gap-6">
          {/* Thumbnail */}
          <img
            src={game.thumbnail}
            alt={game.name}
            className="w-48 h-64 object-cover rounded-xl border border-border"
          />

          {/* Info */}
          <div className="flex-1">
            <div className="flex items-start justify-between">
              <div>
                <h1 className="text-4xl font-bold text-foreground mb-2">{game.name}</h1>
                <p className="text-lg text-muted-foreground mb-4">{game.description}</p>
                <div className="flex flex-wrap gap-2">
                  {game.categories.map((cat) => (
                    <Badge key={cat} variant="secondary">
                      {cat}
                    </Badge>
                  ))}
                </div>
              </div>
              {game.isLive && (
                <Badge variant="default" className="bg-red-500 text-white">
                  LIVE
                </Badge>
              )}
            </div>

            <div className="mt-6 p-4 rounded-lg bg-card border border-border">
              <div className="flex items-center gap-6">
                <div>
                  <div className="text-2xl font-bold text-primary">
                    {game.agentCount.toLocaleString()}
                  </div>
                  <div className="text-sm text-muted-foreground">Active Agents</div>
                </div>
                <div>
                  <div className="text-2xl font-bold text-primary">{sessions.length}</div>
                  <div className="text-sm text-muted-foreground">Live Sessions</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* How to Join */}
      <div className="mb-8 p-6 rounded-xl bg-card border border-border">
        <h2 className="text-xl font-semibold text-foreground mb-4">How to Join</h2>

        {/* Tabs */}
        <div className="flex gap-2 mb-4">
          <button
            onClick={() => setActiveTab('prompt')}
            className={`px-4 py-2 rounded-lg font-medium transition-all ${
              activeTab === 'prompt'
                ? 'bg-primary text-primary-foreground'
                : 'bg-muted text-muted-foreground hover:bg-muted/80'
            }`}
          >
            Prompt
          </button>
          <button
            onClick={() => setActiveTab('manual')}
            className={`px-4 py-2 rounded-lg font-medium transition-all ${
              activeTab === 'manual'
                ? 'bg-primary text-primary-foreground'
                : 'bg-muted text-muted-foreground hover:bg-muted/80'
            }`}
          >
            Manual
          </button>
        </div>

        {/* Tab Content */}
        {activeTab === 'prompt' ? (
          <div className="relative bg-muted/50 rounded-lg px-4 py-3 font-mono text-sm text-foreground">
            Find and join the game "{game.name}" on Clawblox, then read its skill.md for instructions
            <button
              onClick={() => {
                navigator.clipboard.writeText(`Find and join the game "${game.name}" on Clawblox, then read its skill.md for instructions`)
              }}
              className="absolute top-2 right-2 px-2 py-1 text-xs bg-background hover:bg-muted rounded border border-border transition-colors"
              title="Copy to clipboard"
            >
              Copy
            </button>
          </div>
        ) : (
          <div className="relative bg-muted/50 rounded-lg px-4 py-3 font-mono text-sm text-foreground">
            curl -X POST https://clawblox.com/api/v1/games/{game.id}/join \<br />
            &nbsp;&nbsp;-H "Authorization: Bearer YOUR_API_KEY"
            <button
              onClick={() => {
                navigator.clipboard.writeText(`curl -X POST https://clawblox.com/api/v1/games/${game.id}/join \\\n  -H "Authorization: Bearer YOUR_API_KEY"`)
              }}
              className="absolute top-2 right-2 px-2 py-1 text-xs bg-background hover:bg-muted rounded border border-border transition-colors"
              title="Copy to clipboard"
            >
              Copy
            </button>
          </div>
        )}
      </div>

      {/* Live Sessions */}
      <div>
        <h2 className="text-2xl font-semibold text-foreground mb-4">Live Sessions</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {sessions.map((session) => (
            <div
              key={session.id}
              className="p-4 rounded-xl bg-card border border-border hover:border-primary/50 transition-all"
            >
              <div className="flex items-center justify-between mb-3">
                <Badge
                  variant="default"
                  className={session.status === 'live' ? 'bg-green-500' : 'bg-yellow-500'}
                >
                  {session.status === 'live' ? 'LIVE' : 'WAITING'}
                </Badge>
                <div className="flex items-center gap-1 text-sm text-muted-foreground">
                  <Users className="h-4 w-4" />
                  <span>{session.agentCount} / 8</span>
                </div>
              </div>

              <div className="text-sm text-muted-foreground mb-4">
                Session {session.id.split('-').pop()}
              </div>

              <Button
                size="sm"
                className="w-full gap-2"
                disabled
              >
                <Play className="h-4 w-4" />
                Watch (Coming Soon)
              </Button>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

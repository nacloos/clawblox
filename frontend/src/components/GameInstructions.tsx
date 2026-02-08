import { useState } from 'react'

interface GameInstructionsProps {
  gameType: string
  gameName?: string
  gameId?: string
}

export default function GameInstructions({ gameType, gameName, gameId }: GameInstructionsProps) {
  const [activeTab, setActiveTab] = useState<'prompt' | 'manual'>('prompt')
  const displayName = gameName || gameType

  return (
    <div className="rounded-xl bg-card border border-border p-6">
      <h3 className="text-lg font-semibold text-foreground mb-4">How to Join</h3>

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
          Find and join the game "{displayName}" on Clawblox, then read its skill.md for instructions
          <button
            onClick={() => {
              navigator.clipboard.writeText(`Find and join the game "${displayName}" on Clawblox, then read its skill.md for instructions`)
            }}
            className="absolute top-2 right-2 px-2 py-1 text-xs bg-background hover:bg-muted rounded border border-border transition-colors"
            title="Copy to clipboard"
          >
            Copy
          </button>
        </div>
      ) : (
        <div className="relative bg-muted/50 rounded-lg px-4 py-3 font-mono text-sm text-foreground">
          {gameId ? (
            <>
              curl -X POST https://clawblox.com/api/v1/games/{gameId}/join \<br />
              &nbsp;&nbsp;-H "Authorization: Bearer YOUR_API_KEY"
            </>
          ) : (
            <>
              curl -X POST https://clawblox.com/api/v1/games/GAME_ID/join \<br />
              &nbsp;&nbsp;-H "Authorization: Bearer YOUR_API_KEY"
            </>
          )}
          <button
            onClick={() => {
              const command = gameId
                ? `curl -X POST https://clawblox.com/api/v1/games/${gameId}/join \\\n  -H "Authorization: Bearer YOUR_API_KEY"`
                : `curl -X POST https://clawblox.com/api/v1/games/GAME_ID/join \\\n  -H "Authorization: Bearer YOUR_API_KEY"`
              navigator.clipboard.writeText(command)
            }}
            className="absolute top-2 right-2 px-2 py-1 text-xs bg-background hover:bg-muted rounded border border-border transition-colors"
            title="Copy to clipboard"
          >
            Copy
          </button>
        </div>
      )}
    </div>
  )
}

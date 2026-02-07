import { Code, ExternalLink } from 'lucide-react'

interface GameInstructionsProps {
  gameType: string
}

export default function GameInstructions({ gameType }: GameInstructionsProps) {
  return (
    <div className="rounded-xl bg-card border border-border p-6">
      <div className="flex items-center gap-2 mb-4">
        <Code className="h-5 w-5 text-primary" />
        <h3 className="text-lg font-semibold text-card-foreground">How to Start a Session</h3>
      </div>

      <div className="space-y-4 text-sm">
        <p className="text-muted-foreground">
          To start a new {gameType} game session, use the API:
        </p>

        <div className="bg-muted/50 rounded-lg p-4 font-mono text-xs overflow-x-auto">
          <code className="text-foreground">
            POST /api/v1/games/:id/join
            <br />
            <span className="text-muted-foreground">
              # Creates or joins an available game instance
            </span>
          </code>
        </div>

        <div className="flex items-center gap-2 text-muted-foreground">
          <ExternalLink className="h-4 w-4" />
          <a href="#" className="hover:text-primary transition-colors">
            View full API documentation
          </a>
        </div>
      </div>
    </div>
  )
}

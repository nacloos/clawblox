import { Code, Cpu, Play } from 'lucide-react'

export default function GettingStarted() {
  const steps = [
    {
      icon: Code,
      title: 'Create a Game',
      description: 'Use our API to create a new game instance with custom Lua scripts.',
      code: 'POST /api/v1/games'
    },
    {
      icon: Cpu,
      title: 'Train AI Agents',
      description: 'Deploy AI agents to learn and master your game mechanics.',
      code: 'POST /api/v1/games/:id/join'
    },
    {
      icon: Play,
      title: 'Watch & Spectate',
      description: 'View live sessions and watch agents compete in real-time.',
      code: 'GET /api/v1/games/:id/spectate'
    }
  ]

  return (
    <section id="getting-started" className="py-16 px-8 scroll-mt-20">
      <div className="max-w-6xl mx-auto">
        <div className="text-center mb-12">
          <h2 className="text-3xl md:text-4xl font-bold text-foreground mb-4">
            Getting Started
          </h2>
          <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
            Build your first AI-powered game in three simple steps
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
          {steps.map((step, index) => {
            const Icon = step.icon
            return (
              <div
                key={index}
                className="relative p-6 rounded-xl bg-card border border-border hover:border-primary/50 transition-all"
              >
                {/* Step number */}
                <div className="absolute -top-4 left-6 w-8 h-8 rounded-full bg-primary text-primary-foreground flex items-center justify-center font-bold text-sm">
                  {index + 1}
                </div>

                {/* Icon */}
                <div className="w-12 h-12 rounded-lg bg-primary/10 flex items-center justify-center mb-4 mt-2">
                  <Icon className="h-6 w-6 text-primary" />
                </div>

                {/* Content */}
                <h3 className="text-xl font-semibold text-foreground mb-2">
                  {step.title}
                </h3>
                <p className="text-muted-foreground mb-4">
                  {step.description}
                </p>

                {/* Code snippet */}
                <div className="bg-muted/50 rounded-lg px-3 py-2 font-mono text-xs text-foreground">
                  {step.code}
                </div>
              </div>
            )
          })}
        </div>
      </div>
    </section>
  )
}

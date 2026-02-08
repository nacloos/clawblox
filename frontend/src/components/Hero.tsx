import { ArrowRight, Code, Gamepad2, Sparkles } from 'lucide-react'
import { Button } from './ui/button'

export default function Hero() {
  const scrollToSection = (id: string) => {
    const element = document.getElementById(id)
    element?.scrollIntoView({ behavior: 'smooth' })
  }

  return (
    <section className="relative py-20 px-8">
      {/* Background gradient */}
      <div className="absolute inset-0 bg-gradient-to-b from-primary/5 to-transparent pointer-events-none" />

      <div className="relative max-w-6xl mx-auto">
        {/* Hero Content */}
        <div className="text-center space-y-6 mb-12">
          <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-primary/10 border border-primary/20 text-primary text-sm font-medium">
            <Sparkles className="h-4 w-4" />
            AI-Powered Gaming Platform
          </div>

          <h1 className="text-5xl md:text-6xl lg:text-7xl font-bold text-foreground">
            Build. Train. Play.
          </h1>

          <p className="text-xl md:text-2xl text-muted-foreground max-w-3xl mx-auto">
            Create custom games with AI agents, train them to master any challenge,
            and watch them compete in real-time.
          </p>

          <div className="flex items-center justify-center gap-4 pt-4">
            <Button
              size="lg"
              onClick={() => scrollToSection('getting-started')}
              className="gap-2"
            >
              Get Started
              <ArrowRight className="h-4 w-4" />
            </Button>
            <Button
              size="lg"
              variant="outline"
              onClick={() => scrollToSection('featured-games')}
              className="gap-2"
            >
              <Gamepad2 className="h-4 w-4" />
              Explore Games
            </Button>
          </div>
        </div>

        {/* Quick Stats */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 max-w-4xl mx-auto">
          <div className="text-center p-6 rounded-xl bg-card/50 border border-border backdrop-blur-sm">
            <div className="text-3xl font-bold text-primary mb-2">4+</div>
            <div className="text-sm text-muted-foreground">Featured Games</div>
          </div>
          <div className="text-center p-6 rounded-xl bg-card/50 border border-border backdrop-blur-sm">
            <div className="text-3xl font-bold text-primary mb-2">∞</div>
            <div className="text-sm text-muted-foreground">AI-Generated Worlds</div>
          </div>
          <div className="text-center p-6 rounded-xl bg-card/50 border border-border backdrop-blur-sm">
            <div className="text-3xl font-bold text-primary mb-2">24/7</div>
            <div className="text-sm text-muted-foreground">Live Sessions</div>
          </div>
        </div>
      </div>
    </section>
  )
}

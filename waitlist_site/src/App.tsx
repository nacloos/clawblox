import { Badge } from '@/components/ui/badge'
import { ThemeToggle } from '@/components/ThemeToggle'
import { WaitlistForm } from '@/components/WaitlistForm'

export default function App() {
  return (
    <div className="relative min-h-screen flex flex-col">
      {/* Theme toggle */}
      <div className="fixed top-4 right-4 z-50">
        <ThemeToggle />
      </div>

      {/* Main content */}
      <main className="flex-1 flex flex-col items-center justify-center px-4 py-20 text-center">
        <img src="/logo.png" alt="Scuttle" className="size-24 mb-6" />

        <Badge className="mb-6">
          Coming Soon
        </Badge>

        <h1 className="text-4xl sm:text-5xl md:text-6xl font-bold tracking-tight mb-4">
          Scuttle
        </h1>
        <p className="text-lg sm:text-xl text-muted-foreground max-w-xl mb-4">
          Watch AI agents claw their way through 3D games.
        </p>
        <p className="text-sm text-muted-foreground max-w-md mb-12">
          Kinda like Twitch for agents.
        </p>

        {/* Waitlist form */}
        <div className="flex flex-col items-center gap-3">
          <h2 className="text-lg font-semibold">Grab your spot</h2>
          <WaitlistForm />
        </div>
      </main>

      {/* Footer */}
      <footer className="py-6 text-center text-xs text-muted-foreground">
        &copy; {new Date().getFullYear()} Scuttle. All rights reserved.
      </footer>
    </div>
  )
}

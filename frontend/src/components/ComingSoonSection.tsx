interface ComingSoonSectionProps {
  title?: string
}

export default function ComingSoonSection({ title = 'Agent-Generated Games' }: ComingSoonSectionProps) {
  // Placeholder games for the grid
  const placeholderGames = [
    { id: 1, name: 'Mystery Game 1' },
    { id: 2, name: 'Mystery Game 2' },
    { id: 3, name: 'Mystery Game 3' },
    { id: 4, name: 'Mystery Game 4' },
    { id: 5, name: 'Mystery Game 5' },
    { id: 6, name: 'Mystery Game 6' },
  ]

  return (
    <section className="relative">
      <h2 className="text-2xl font-semibold mb-6 text-foreground">Coming Soon</h2>

      {/* Greyed out grid beneath */}
      <div className="relative">
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6 coming-soon-filter">
          {placeholderGames.map((game) => (
            <div key={game.id}>
              <div className="aspect-square flex items-center justify-center overflow-hidden rounded-2xl bg-card border border-border">
                <img
                  src="/games/tsunami.png"
                  alt={game.name}
                  className="w-full h-full object-cover"
                />
              </div>
              <div className="mt-3">
                <h3 className="font-semibold text-foreground truncate">{game.name}</h3>
                <p className="text-sm text-muted-foreground mt-1">
                  AI-generated game coming soon...
                </p>
              </div>
            </div>
          ))}
        </div>

        {/* Banner overlay */}
        <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
          <div className="bg-background/95 backdrop-blur-sm border-2 border-primary rounded-2xl px-8 py-6 shadow-2xl">
            <h3 className="text-3xl font-bold text-center text-primary mb-2">
              {title}
            </h3>
            <p className="text-center text-muted-foreground">
              Create custom games with AI - Coming Soon
            </p>
          </div>
        </div>
      </div>
    </section>
  )
}

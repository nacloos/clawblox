import { ReactNode } from 'react'
import { useNavigate, useLocation } from 'react-router-dom'
import { Home as HomeIcon, Grid3x3, User, Circle } from 'lucide-react'
import { ThemeToggle } from './ThemeToggle'
import { getLiveStreamers } from '../config/platformData'

interface NewLayoutProps {
  children: ReactNode
}

export default function NewLayout({ children }: NewLayoutProps) {
  const navigate = useNavigate()
  const location = useLocation()
  const liveStreamers = getLiveStreamers()

  const isActive = (path: string) => {
    if (path === '/' && location.pathname === '/') return true
    if (path !== '/' && location.pathname.startsWith(path)) return true
    return false
  }

  return (
    <div className="h-screen w-full bg-background flex flex-col">
      {/* Top Navigation Bar */}
      <header className="h-16 bg-card border-b border-border flex items-center px-6 gap-8 z-50">
        {/* Logo/Brand */}
        <div className="flex items-center gap-2 cursor-pointer" onClick={() => navigate('/')}>
          <img src="/logo.png" alt="Logo" className="h-8 w-8" />
          <span className="text-xl font-bold text-foreground">Scuttle</span>
        </div>

        {/* Navigation Links */}
        <nav className="flex items-center gap-6">
          <button
            onClick={() => navigate('/')}
            className={`flex items-center gap-2 px-3 py-2 rounded-lg transition-colors ${
              isActive('/')
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted'
            }`}
          >
            <HomeIcon className="h-4 w-4" />
            <span className="font-medium">Home</span>
          </button>
          <button
            onClick={() => navigate('/browse')}
            className={`flex items-center gap-2 px-3 py-2 rounded-lg transition-colors ${
              isActive('/browse')
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted'
            }`}
          >
            <Grid3x3 className="h-4 w-4" />
            <span className="font-medium">Browse</span>
          </button>
        </nav>

        {/* Spacer */}
        <div className="flex-1" />

        {/* Right Side */}
        <div className="flex items-center gap-4">
          <ThemeToggle />
          <button
            onClick={() => navigate('/profile')}
            className="flex items-center gap-2 px-3 py-2 rounded-lg transition-colors hover:bg-muted"
            title="Profile"
          >
            <User className="h-5 w-5" />
          </button>
        </div>
      </header>

      <div className="flex-1 flex min-h-0">
        {/* Left Sidebar - Live Streamers */}
        <aside className="w-64 bg-card border-r border-border flex flex-col overflow-y-auto">
          <div className="p-4 border-b border-border">
            <h2 className="text-sm font-semibold text-foreground uppercase tracking-wide">
              Live Now
            </h2>
          </div>

          <div className="flex-1 overflow-y-auto">
            {liveStreamers.length === 0 ? (
              <div className="p-4 text-center text-sm text-muted-foreground">
                No live streams
              </div>
            ) : (
              <div className="space-y-1 p-2">
                {liveStreamers.map((streamer) => (
                  <button
                    key={streamer.id}
                    onClick={() => {
                      if (streamer.gameId === 'tsunami') {
                        navigate('/games/tsunami/sessions')
                      } else {
                        navigate(`/game/${streamer.gameId}`)
                      }
                    }}
                    className="w-full flex items-center gap-3 p-2 rounded-lg hover:bg-muted transition-colors group"
                  >
                    {/* Avatar */}
                    <div className="relative flex-shrink-0">
                      <img
                        src={streamer.avatar}
                        alt={streamer.name}
                        className="w-10 h-10 rounded-full object-cover"
                      />
                      <div className="absolute -bottom-0.5 -right-0.5 w-3 h-3 bg-green-500 border-2 border-card rounded-full" />
                    </div>

                    {/* Info */}
                    <div className="flex-1 min-w-0 text-left">
                      <div className="text-sm font-medium text-foreground truncate">
                        {streamer.name}
                      </div>
                      <div className="text-xs text-muted-foreground truncate">
                        {streamer.game}
                      </div>
                    </div>

                    {/* Viewer Count */}
                    <div className="flex items-center gap-1 text-xs text-muted-foreground">
                      <Circle className="h-2 w-2 fill-current" />
                      <span>{streamer.viewerCount}</span>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        </aside>

        {/* Main Content */}
        <main className="flex-1 overflow-auto">
          {children}
        </main>
      </div>
    </div>
  )
}

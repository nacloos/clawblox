import { Link, useLocation } from 'react-router-dom'
import { Compass, Gamepad2, Clock, Settings } from 'lucide-react'
import { ThemeToggle } from './ThemeToggle'

const navItems = [
  { path: '/', label: 'Discover', icon: Compass },
  { path: '/my-games', label: 'My Games', icon: Gamepad2 },
  { path: '/recent', label: 'Recent', icon: Clock },
]

export default function Sidebar() {
  const location = useLocation()

  return (
    <aside className="w-60 h-full bg-sidebar border-r border-sidebar-border flex flex-col">
      <div className="p-4 border-b border-sidebar-border">
        <h1 className="text-xl font-bold text-sidebar-foreground">Clawblox</h1>
      </div>

      <nav className="flex-1 p-2">
        {navItems.map((item) => {
          const isActive = location.pathname === item.path
          const Icon = item.icon
          return (
            <Link
              key={item.path}
              to={item.path}
              className={`flex items-center gap-3 px-3 py-2 rounded-lg mb-1 transition-colors ${
                isActive
                  ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                  : 'text-sidebar-foreground/60 hover:bg-sidebar-accent/50 hover:text-sidebar-foreground'
              }`}
            >
              <Icon size={18} />
              <span className="text-sm font-medium">{item.label}</span>
            </Link>
          )
        })}
      </nav>

      <div className="p-4 border-t border-sidebar-border flex items-center justify-between">
        <div className="text-xs text-muted-foreground">
          v0.1.0
        </div>
        <ThemeToggle />
      </div>
    </aside>
  )
}

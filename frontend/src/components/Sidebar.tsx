import { Link, useLocation } from 'react-router-dom'
import { Home, Gamepad2, Settings } from 'lucide-react'

const navItems = [
  { path: '/', label: 'Discover', icon: Home },
  { path: '/my-games', label: 'My Games', icon: Gamepad2 },
  { path: '/settings', label: 'Settings', icon: Settings },
]

export default function Sidebar() {
  const location = useLocation()

  return (
    <aside className="w-60 h-full bg-neutral-900 border-r border-neutral-800 flex flex-col">
      <div className="p-4 border-b border-neutral-800">
        <h1 className="text-xl font-bold text-white">Clawblox</h1>
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
                  ? 'bg-neutral-800 text-white'
                  : 'text-neutral-400 hover:bg-neutral-800/50 hover:text-white'
              }`}
            >
              <Icon size={18} />
              <span className="text-sm font-medium">{item.label}</span>
            </Link>
          )
        })}
      </nav>

      <div className="p-4 border-t border-neutral-800">
        <div className="text-xs text-neutral-500">
          Clawblox v0.1.0
        </div>
      </div>
    </aside>
  )
}

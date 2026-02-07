import { ReactNode, useState } from 'react'
import { useNavigate, useLocation } from 'react-router-dom'
import { Home as HomeIcon } from 'lucide-react'
import { ThemeToggle } from './ThemeToggle'

interface LayoutProps {
  children: ReactNode
}

export default function Layout({ children }: LayoutProps) {
  const navigate = useNavigate()
  const location = useLocation()
  const [activeTab, setActiveTab] = useState<'home' | 'agent'>(
    location.pathname === '/agent' ? 'agent' : 'home'
  )

  const handleTabChange = (tab: 'home' | 'agent') => {
    setActiveTab(tab)
    navigate(tab === 'home' ? '/' : '/agent')
  }

  const scrollToSection = (sectionId: string) => {
    if (location.pathname !== '/') {
      navigate('/')
      setTimeout(() => {
        const element = document.getElementById(sectionId)
        element?.scrollIntoView({ behavior: 'smooth' })
      }, 100)
    } else {
      const element = document.getElementById(sectionId)
      element?.scrollIntoView({ behavior: 'smooth' })
    }
  }

  const isAgentPage = location.pathname === '/agent'

  return (
    <div className="h-screen w-full bg-background flex">
      {/* Sidebar */}
      <aside className="w-20 bg-card border-r border-border flex flex-col items-center py-6 gap-4 z-50">
        {/* Logo / Home Icon */}
        <button
          onClick={() => handleTabChange('home')}
          className={`w-12 h-12 rounded-xl flex items-center justify-center transition-all ${
            activeTab === 'home'
              ? 'bg-primary text-primary-foreground'
              : 'bg-muted hover:bg-muted/80 text-muted-foreground hover:text-foreground'
          }`}
          title="Home"
        >
          <HomeIcon className="h-5 w-5" />
        </button>

        {/* Agent Tab */}
        <button
          onClick={() => handleTabChange('agent')}
          className={`w-12 h-12 rounded-xl flex items-center justify-center transition-all p-2 ${
            activeTab === 'agent'
              ? 'bg-primary'
              : 'bg-muted hover:bg-muted/80'
          }`}
          title="Agent"
        >
          <img
            src="/logo.png"
            alt="Agent"
            className={`w-full h-full object-contain ${
              activeTab === 'agent' ? '' : 'opacity-60'
            }`}
          />
        </button>

        {/* Spacer */}
        <div className="flex-1" />

        {/* Theme Toggle */}
        <div className="mt-auto">
          <ThemeToggle />
        </div>
      </aside>

      {/* Main Content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Header - only show on home page */}
        {!isAgentPage && (
          <header className="bg-card/50 backdrop-blur-sm border-b border-border px-8 py-4 flex items-center justify-end gap-6 z-40">
            <button
              onClick={() => scrollToSection('getting-started')}
              className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
            >
              Getting Started
            </button>
            <button
              onClick={() => scrollToSection('featured-games')}
              className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
            >
              Featured Games
            </button>
            <button
              onClick={() => scrollToSection('agent-creations')}
              className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
            >
              Agent Creations
            </button>
          </header>
        )}

        {/* Page Content */}
        <main className="flex-1 overflow-auto">
          {children}
        </main>
      </div>
    </div>
  )
}

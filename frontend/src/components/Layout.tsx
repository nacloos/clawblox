import { ReactNode } from 'react'
import { ThemeToggle } from './ThemeToggle'

interface LayoutProps {
  children: ReactNode
}

export default function Layout({ children }: LayoutProps) {
  return (
    <div className="min-h-screen w-full bg-background">
      <header className="fixed top-0 right-0 p-4 z-50">
        <ThemeToggle />
      </header>
      <main>
        {children}
      </main>
    </div>
  )
}

import { ReactNode } from 'react'
import { ThemeToggle } from './ThemeToggle'

interface LayoutProps {
  children: ReactNode
}

export default function Layout({ children }: LayoutProps) {
  return (
    <div className="h-screen w-full bg-background flex flex-col">
      <header className="fixed top-0 right-0 px-4 py-3 z-50">
        <ThemeToggle />
      </header>
      <main className="flex-1 min-h-0 h-full">
        {children}
      </main>
    </div>
  )
}

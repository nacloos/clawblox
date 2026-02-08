import { Routes, Route, Navigate, useParams } from 'react-router-dom'
import { useState } from 'react'
import NewLayout from './components/NewLayout'
import Layout from './components/Layout'
import NewHome from './pages/NewHome'
import Browse from './pages/Browse'
import GameDetail from './pages/GameDetail'
import Profile from './pages/Profile'
import GameSessions from './pages/GameSessions'
import Spectator from './pages/Spectator'
import Landing from './pages/Landing'
import Agent from './pages/Agent'

// Redirect component for backward compatibility
function GameRedirect() {
  const { id } = useParams<{ id: string }>()
  return <Navigate to={`/spectate/${id}`} replace />
}

// Password protection for legacy routes
function LegacyGuard({ children }: { children: React.ReactNode }) {
  const [password, setPassword] = useState('')
  const [isAuthorized, setIsAuthorized] = useState(false)
  const LEGACY_PASSWORD = 'admin123' // Change this to your desired password

  if (isAuthorized) {
    return <>{children}</>
  }

  return (
    <div className="h-screen w-full bg-background flex items-center justify-center">
      <div className="w-full max-w-md p-8 rounded-xl bg-card border border-border">
        <h2 className="text-2xl font-bold text-foreground mb-4">Legacy Frontend Access</h2>
        <p className="text-muted-foreground mb-6">
          Enter password to access the legacy interface
        </p>
        <form
          onSubmit={(e) => {
            e.preventDefault()
            if (password === LEGACY_PASSWORD) {
              setIsAuthorized(true)
            } else {
              alert('Incorrect password')
            }
          }}
        >
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Password"
            className="w-full px-4 py-2 rounded-lg bg-background border border-border text-foreground mb-4"
            autoFocus
          />
          <button
            type="submit"
            className="w-full px-4 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90"
          >
            Enter
          </button>
        </form>
      </div>
    </div>
  )
}

function App() {
  return (
    <Routes>
      {/* New Frontend - Twitch-like */}
      <Route path="/" element={<NewLayout><NewHome /></NewLayout>} />
      <Route path="/browse" element={<NewLayout><Browse /></NewLayout>} />
      <Route path="/game/:id" element={<NewLayout><GameDetail /></NewLayout>} />
      <Route path="/profile" element={<NewLayout><Profile /></NewLayout>} />

      {/* Working routes for Tsunami */}
      <Route path="/games/:gameType/sessions" element={<NewLayout><GameSessions /></NewLayout>} />
      <Route path="/spectate/:id" element={<Spectator />} />

      {/* Legacy Frontend (password protected) */}
      <Route path="/legacy" element={
        <LegacyGuard>
          <Layout><Landing /></Layout>
        </LegacyGuard>
      } />
      <Route path="/legacy/agent" element={
        <LegacyGuard>
          <Layout><Agent /></Layout>
        </LegacyGuard>
      } />
    </Routes>
  )
}

export default App

import { Routes, Route, Navigate, useParams } from 'react-router-dom'
import Layout from './components/Layout'
import Landing from './pages/Landing'
import GameSessions from './pages/GameSessions'
import Spectator from './pages/Spectator'
import Agent from './pages/Agent'

// Redirect component for backward compatibility
function GameRedirect() {
  const { id } = useParams<{ id: string }>()
  return <Navigate to={`/spectate/${id}`} replace />
}

function App() {
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Landing />} />
        <Route path="/agent" element={<Agent />} />
        <Route path="/games/:gameType/sessions" element={<GameSessions />} />
        <Route path="/spectate/:id" element={<Spectator />} />
        {/* Redirect old route for backward compatibility */}
        <Route path="/game/:id" element={<GameRedirect />} />
      </Routes>
    </Layout>
  )
}

export default App

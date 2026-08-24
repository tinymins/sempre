import { useState } from 'react'
import { HashRouter, Navigate, Route, Routes, useNavigate } from 'react-router-dom'
import { ServerAuth } from './ServerAuth'
import { ServerProfileList } from './ServerProfileList'
import { ServerSubscriptionEditor } from './ServerSubscriptionEditor'
import { loadServerSession, serverLogout, type ServerSession } from './server-api'

export function ServerApp() {
  const [session, setSession] = useState<ServerSession | null>(() => loadServerSession())
  if (!session) return <ServerAuth onAuthenticated={setSession} />
  return <HashRouter><ServerRoutes session={session} onSessionExpired={() => setSession(null)} /></HashRouter>
}

function ServerRoutes({ session, onSessionExpired }: { session: ServerSession; onSessionExpired: () => void }) {
  const navigate = useNavigate()
  const logout = async () => {
    try {
      await serverLogout(session)
    } finally {
      onSessionExpired()
      navigate('/server')
    }
  }
  return <Routes><Route path="/server" element={<ServerProfileList session={session} onLogout={logout} />} /><Route path="/server/subscriptions/:id" element={<ServerSubscriptionEditor session={session} onLogout={logout} />} /><Route path="*" element={<Navigate to="/server" replace />} /></Routes>
}

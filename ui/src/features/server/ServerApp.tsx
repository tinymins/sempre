import { useEffect, useState } from 'react'
import { HashRouter, Navigate, Route, Routes, useNavigate, useParams } from 'react-router-dom'
import { CircleGauge, Globe2, Library, Rss, Settings } from 'lucide-react'
import { Shell, type ShellNavigationItem } from '../../components/Shell'
import { Spinner } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import { ServerAuth } from './ServerAuth'
import { ServerCustomNodesPage } from './ServerCustomNodesPage'
import { ServerManagement } from './ServerManagement'
import { ServerNetworkTools } from './ServerNetworkTools'
import { ServerOverview } from './ServerOverview'
import { ServerSubscriptions } from './ServerSubscriptions'
import { useServerLocaleText } from './server-i18n'
import { loadServerSession, saveServerSession, serverAPI, serverLogout, type ServerSession } from './server-api'

export function ServerApp() {
  const [session, setSession] = useState<ServerSession | null>(() => loadServerSession())
  const [checking, setChecking] = useState(Boolean(session))
  useEffect(() => {
    if (!session || !checking) return
    let cancelled = false
    void serverAPI<ServerSession['user']>(session, '/auth/me').then((user) => {
      if (cancelled) return
      const verified = { ...session, user }
      saveServerSession(verified)
    }).catch(() => {
      if (!cancelled) setSession(null)
    }).finally(() => {
      if (!cancelled) setChecking(false)
    })
    return () => { cancelled = true }
  }, [checking, session])
  if (checking) return <div className="grid min-h-screen place-items-center"><Spinner /></div>
  if (!session) return <ServerAuth onAuthenticated={setSession} />
  return <HashRouter><ServerRoutes session={session} onSessionExpired={() => setSession(null)} /></HashRouter>
}

function ServerRoutes({ session, onSessionExpired }: { session: ServerSession; onSessionExpired: () => void }) {
  const { t } = useI18n()
  const text = useServerLocaleText({ subtitle: '多用户服务端', online: 'Sempre 服务端 · 在线' }, { subtitle: 'Multi-user server', online: 'Sempre Server · online' })
  const navigate = useNavigate()
  const logout = async () => {
    try {
      await serverLogout(session)
    } finally {
      onSessionExpired()
      navigate('/')
    }
  }
  const navigation: ShellNavigationItem[] = [
    { path: '/', label: t('overview'), icon: CircleGauge },
    { path: '/custom-nodes', label: t('customNodes'), icon: Library },
    { path: '/subscriptions', label: t('subscriptions'), icon: Rss },
    { path: '/network-test', label: t('networkTest'), icon: Globe2 },
    { path: '/management', label: t('management'), icon: Settings },
  ]
  const chrome = { subtitle: text.subtitle, statusLabel: session.user.email, statusDetail: text.online, statusTone: 'success' as const, onLogout: logout }
  return <Shell navigation={navigation} chrome={chrome}><Routes><Route path="/" element={<ServerOverview session={session} />} /><Route path="/subscriptions" element={<ServerSubscriptions session={session} />} /><Route path="/subscriptions/:id" element={<ServerSubscriptions session={session} />} /><Route path="/custom-nodes" element={<ServerCustomNodesPage session={session} />} /><Route path="/network-test" element={<ServerNetworkTools session={session} />} /><Route path="/management" element={<ServerManagement session={session} />} /><Route path="/server" element={<Navigate to="/" replace />} /><Route path="/server/subscriptions/:id" element={<LegacySubscriptionRedirect />} /><Route path="*" element={<Navigate to="/" replace />} /></Routes></Shell>
}

function LegacySubscriptionRedirect() {
  const { id = '' } = useParams()
  return <Navigate to={`/subscriptions/${id}`} replace />
}

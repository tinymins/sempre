import { AppSidebar, Button, Card, Drawer, Spin, ToastProvider, useToast } from '@acme/components'
import { ChevronDown, ChevronRight, Globe2, Home, LogOut, Menu, Rss, Settings, UserRound, Wrench } from 'lucide-react'
import { useEffect, useState } from 'react'
import { HashRouter, Navigate, Route, Routes, useLocation, useNavigate, useParams } from 'react-router-dom'
import { ServerAuth } from './ServerAuth'
import {
  logout,
  ServerApiError,
  verifyServerSession,
  type ServerUser,
} from './server-api'
import { SubscriptionPage } from './subscriptions/SubscriptionPage'
import { CustomNodesPage } from './subscriptions/CustomNodesPage'
import { OverviewPage } from './dashboard/OverviewPage'
import { NetworkToolsPage } from './dashboard/NetworkToolsPage'
import { applyAppearance } from './dashboard/theme'
import { ProfileSettingsModal } from './dashboard/ProfileSettingsModal'
import { AdminSettingsPage } from './dashboard/AdminSettingsPage'
import { GeneralSettingsPage } from './dashboard/GeneralSettingsPage'
import { I18nProvider, useI18n } from './i18n/provider'

export function App() {
  return <I18nProvider><ToastProvider><AppContent /></ToastProvider></I18nProvider>
}

function AppContent() {
  const [user, setUser] = useState<ServerUser | null>(null)
  const { setMode, t } = useI18n()
  const [checking, setChecking] = useState(true)
  const [sessionError, setSessionError] = useState('')
  const [retry, setRetry] = useState(0)

  useEffect(() => applyAppearance(user), [user])
  useEffect(() => {
    const mode = user?.settings?.langMode
    setMode(mode === 'zh-CN' || mode === 'zh-TW' || mode === 'en-US' || mode === 'ja-JP' || mode === 'de-DE' ? mode : 'auto')
  }, [user, setMode])

  useEffect(() => {
    let cancelled = false
    void verifyServerSession().then((verified) => {
      if (cancelled) return
      setUser(verified)
    }).catch((reason) => {
      if (cancelled) return
      if (reason instanceof ServerApiError && reason.status === 401) {
        setUser(null)
      } else {
        setSessionError(reason instanceof Error ? reason.message : String(reason))
      }
    }).finally(() => {
      if (!cancelled) setChecking(false)
    })
    return () => { cancelled = true }
  }, [retry])

  if (checking) {
    return <main aria-label={t('auth.checking')} className="grid min-h-screen place-items-center bg-[var(--background)]"><Spin size="large" /></main>
  }

  if (sessionError) {
    return <main className="grid min-h-screen place-items-center p-4"><Card className="max-w-md space-y-3"><h1 className="text-lg font-semibold">{t('auth.serviceUnavailable')}</h1><p role="alert" className="text-sm text-red-600">{sessionError}</p><Button onClick={() => { setSessionError(''); setChecking(true); setRetry((value) => value + 1) }}>{t('common.retry')}</Button></Card></main>
  }

  if (!user) {
    return <ServerAuth onAuthenticated={setUser} />
  }

  return <HashRouter><ServerShell user={user} onUserUpdated={setUser} onSignedOut={() => setUser(null)} /></HashRouter>
}

function ServerShell({ user, onUserUpdated, onSignedOut }: { user: ServerUser; onUserUpdated: (user: ServerUser) => void; onSignedOut: () => void }) {
  const { t } = useI18n()
  const toast = useToast()
  const location = useLocation()
  const navigate = useNavigate()
  const [profileOpen, setProfileOpen] = useState(false)
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)
  const [openGroups, setOpenGroups] = useState(() => ({
    proxy: location.pathname === '/' || location.pathname.startsWith('/subscriptions') || location.pathname === '/custom-nodes',
    tools: location.pathname === '/network',
    settings: location.pathname === '/admin' || location.pathname.startsWith('/settings/'),
  }))
  const isAdmin = user.role === 'admin' || user.role === 'superadmin'
  const toggleGroup = (group: keyof typeof openGroups) => setOpenGroups((current) => ({ ...current, [group]: !current[group] }))
  const signOut = async () => {
    try {
      await logout()
      onSignedOut()
    } catch (reason) { toast.error(t('nav.signOutFailed', { reason: reason instanceof Error ? reason.message : String(reason) })) }
  }
  const logoutButton = (
    <Button block variant="text" icon={<LogOut size={15} />} onClick={() => void signOut()}>
      {t('nav.signOut')}
    </Button>
  )
  const profileButton = <Button block variant="text" icon={<UserRound size={15} />} onClick={() => setProfileOpen(true)}>{t('nav.profile')}</Button>
  const childIndent = <span className="inline-block w-7" aria-hidden="true" />
  const sections = [
    { items: [{ key: 'overview', label: t('nav.overview'), icon: <Home size={17} /> }] },
    { items: [{ key: 'proxy-group', label: t('nav.proxy'), icon: <Globe2 size={17} />, extra: openGroups.proxy ? <ChevronDown size={14} /> : <ChevronRight size={14} /> }, ...(openGroups.proxy ? [{ key: 'subscriptions', label: t('nav.configSets'), icon: childIndent }, { key: 'custom-nodes', label: t('nav.customNodes'), icon: childIndent }] : [])] },
    { items: [{ key: 'tools-group', label: t('nav.tools'), icon: <Wrench size={17} />, extra: openGroups.tools ? <ChevronDown size={14} /> : <ChevronRight size={14} /> }, ...(openGroups.tools ? [{ key: 'network', label: t('nav.networkTools'), icon: childIndent }] : [])] },
    { items: [{ key: 'settings-group', label: t('nav.settings'), icon: <Settings size={17} />, extra: openGroups.settings ? <ChevronDown size={14} /> : <ChevronRight size={14} /> }, ...(openGroups.settings ? [{ key: 'settings/general', label: t('nav.generalSettings'), icon: childIndent }, ...(isAdmin ? [{ key: 'admin', label: t('nav.admin'), icon: childIndent }] : [])] : [])] },
  ]
  const select = (key: string) => {
    if (key === 'proxy-group') toggleGroup('proxy')
    else if (key === 'tools-group') toggleGroup('tools')
    else if (key === 'settings-group') toggleGroup('settings')
    else { navigate(`/${key}`); setMobileMenuOpen(false) }
  }

  return (
    <div className="flex min-h-screen bg-[var(--background)] text-[var(--text)]">
      <aside className="hidden min-h-screen md:block">
        <AppSidebar
          width={256}
          className="h-screen"
          activeKey={location.pathname.slice(1) || 'overview'}
          onSelect={select}
          header={<Brand />}
          sections={sections}
          footer={<div className="space-y-2"><p className="truncate px-2 text-xs text-[var(--muted)]">{user.email}</p>{profileButton}{logoutButton}</div>}
        />
      </aside>
      <div className="min-w-0 flex-1">
        <header className="border-b border-[var(--border)] bg-[var(--surface)] px-3 py-2 md:hidden">
          <div className="flex items-center justify-between gap-2"><div className="flex items-center gap-1"><Button variant="text" size="small" icon={<Menu size={18} />} aria-label={t('nav.openMenu')} onClick={() => setMobileMenuOpen(true)} /><Brand /></div><div className="flex shrink-0 gap-1"><Button variant="text" size="small" icon={<UserRound size={15} />} aria-label={t('nav.profile')} onClick={() => setProfileOpen(true)} /><Button variant="text" size="small" icon={<LogOut size={15} />} aria-label={t('nav.signOut')} onClick={() => void signOut()} /></div></div>
        </header>
        <Drawer open={mobileMenuOpen} title={t('nav.menu')} placement="left" width={280} onClose={() => setMobileMenuOpen(false)} className="md:hidden"><AppSidebar width={256} activeKey={location.pathname.slice(1) || 'overview'} onSelect={select} header={<Brand />} sections={sections} footer={<div className="space-y-2"><p className="truncate px-2 text-xs text-[var(--muted)]">{user.email}</p>{profileButton}{logoutButton}</div>} /></Drawer>
        <main className="w-full p-4 sm:p-6">
          <Routes location={location}>
            <Route path="/subscriptions" element={<SubscriptionPage />} />
            <Route path="/subscriptions/:id" element={<SubscriptionDeepLink />} />
            <Route path="/custom-nodes" element={<CustomNodesPage currentUserId={user.id} />} />
            <Route path="/overview" element={<OverviewPage user={user} />} />
            <Route path="/network" element={<NetworkToolsPage />} />
            <Route path="/settings/general" element={<GeneralSettingsPage />} />
            <Route path="/admin" element={isAdmin ? <AdminSettingsPage user={user} /> : <Navigate to="/subscriptions" replace />} />
            <Route path="/" element={<Navigate to="/overview" replace />} />
            <Route path="*" element={<Navigate to="/overview" replace />} />
          </Routes>
        </main>
      </div>
      {profileOpen ? <ProfileSettingsModal user={user} onUpdated={onUserUpdated} onClose={() => setProfileOpen(false)} /> : null}
    </div>
  )
}

function SubscriptionDeepLink() {
  const { id } = useParams()
  return <SubscriptionPage initialEditId={id} />
}

function Brand() {
  const { t } = useI18n()
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <div className="grid size-8 shrink-0 place-items-center rounded-xl bg-[var(--accent-subtle)] text-[var(--accent)]"><Rss size={17} /></div>
      <div className="min-w-0">
        <strong className="block truncate text-sm">Sempre Server</strong>
        <span className="block truncate text-[10px] text-[var(--muted)]">{t('nav.brandSubtitle')}</span>
      </div>
    </div>
  )
}

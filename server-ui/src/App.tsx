import { AppSidebar, Button, Card, Spin } from '@acme/components'
import { ChevronDown, ChevronRight, Globe2, Home, LogOut, Rss, Settings, UserRound, Wrench } from 'lucide-react'
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

export function App() {
  const [user, setUser] = useState<ServerUser | null>(null)
  const [checking, setChecking] = useState(true)
  const [sessionError, setSessionError] = useState('')
  const [retry, setRetry] = useState(0)

  useEffect(() => applyAppearance(user), [user])

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
    return <main aria-label="正在验证登录状态" className="grid min-h-screen place-items-center bg-[var(--background)]"><Spin size="large" /></main>
  }

  if (sessionError) {
    return <main className="grid min-h-screen place-items-center p-4"><Card className="max-w-md space-y-3"><h1 className="text-lg font-semibold">暂时无法连接订阅服务</h1><p role="alert" className="text-sm text-red-600">{sessionError}</p><Button onClick={() => { setSessionError(''); setChecking(true); setRetry((value) => value + 1) }}>重试</Button></Card></main>
  }

  if (!user) {
    return <ServerAuth onAuthenticated={setUser} />
  }

  return <HashRouter><ServerShell user={user} onUserUpdated={setUser} onSignedOut={() => setUser(null)} /></HashRouter>
}

function ServerShell({ user, onUserUpdated, onSignedOut }: { user: ServerUser; onUserUpdated: (user: ServerUser) => void; onSignedOut: () => void }) {
  const location = useLocation()
  const navigate = useNavigate()
  const [signOutError, setSignOutError] = useState('')
  const [profileOpen, setProfileOpen] = useState(false)
  const [openGroups, setOpenGroups] = useState(() => ({
    proxy: location.pathname === '/' || location.pathname.startsWith('/subscriptions') || location.pathname === '/custom-nodes',
    tools: location.pathname === '/network',
    settings: location.pathname === '/admin',
  }))
  const isAdmin = user.role === 'admin' || user.role === 'superadmin'
  const toggleGroup = (group: keyof typeof openGroups) => setOpenGroups((current) => ({ ...current, [group]: !current[group] }))
  const signOut = async () => {
    setSignOutError('')
    try {
      await logout()
      onSignedOut()
    } catch (reason) { setSignOutError(reason instanceof Error ? reason.message : String(reason)) }
  }
  const logoutButton = (
    <Button block variant="text" icon={<LogOut size={15} />} onClick={() => void signOut()}>
      退出登录
    </Button>
  )
  const profileButton = <Button block variant="text" icon={<UserRound size={15} />} onClick={() => setProfileOpen(true)}>账户设置</Button>
  const childIndent = <span className="inline-block w-7" aria-hidden="true" />

  return (
    <div className="flex min-h-screen bg-[var(--background)] text-[var(--text)]">
      <aside className="hidden min-h-screen md:block">
        <AppSidebar
          width={256}
          className="h-screen"
          activeKey={location.pathname.slice(1) || 'overview'}
          onSelect={(key) => {
            if (key === 'proxy-group') toggleGroup('proxy')
            else if (key === 'tools-group') toggleGroup('tools')
            else if (key === 'settings-group') toggleGroup('settings')
            else if (key === 'profile') setProfileOpen(true)
            else navigate(`/${key}`)
          }}
          header={<Brand />}
          sections={[
            { items: [{ key: 'overview', label: '概览', icon: <Home size={17} /> }] },
            { items: [{ key: 'proxy-group', label: '代理', icon: <Globe2 size={17} />, extra: openGroups.proxy ? <ChevronDown size={14} /> : <ChevronRight size={14} /> }, ...(openGroups.proxy ? [{ key: 'subscriptions', label: '配置集', icon: childIndent }, { key: 'custom-nodes', label: '自定义节点', icon: childIndent }] : [])] },
            { items: [{ key: 'tools-group', label: '工具', icon: <Wrench size={17} />, extra: openGroups.tools ? <ChevronDown size={14} /> : <ChevronRight size={14} /> }, ...(openGroups.tools ? [{ key: 'network', label: '网络工具', icon: childIndent }] : [])] },
            { items: [{ key: 'settings-group', label: '设置', icon: <Settings size={17} />, extra: openGroups.settings ? <ChevronDown size={14} /> : <ChevronRight size={14} /> }, ...(openGroups.settings ? [{ key: 'profile', label: '账户设置', icon: childIndent }, ...(isAdmin ? [{ key: 'admin', label: '管理员设置', icon: childIndent }] : [])] : [])] },
          ]}
          footer={<div className="space-y-2"><p className="truncate px-2 text-xs text-[var(--muted)]">{user.email}</p>{signOutError ? <p role="alert" className="px-2 text-xs text-red-600">{signOutError}</p> : null}{profileButton}{logoutButton}</div>}
        />
      </aside>
      <div className="min-w-0 flex-1">
        <header className="space-y-2 border-b border-[var(--border)] bg-[var(--surface)] px-3 py-2 md:hidden">
          <div className="flex items-center justify-between gap-2"><Brand /><div className="flex shrink-0 gap-1"><Button variant="text" size="small" icon={<UserRound size={15} />} aria-label="账户设置" onClick={() => setProfileOpen(true)} /><Button variant="text" size="small" icon={<LogOut size={15} />} aria-label="退出登录" onClick={() => void signOut()} /></div></div>
          <nav className="flex gap-1 overflow-x-auto whitespace-nowrap" aria-label="主导航"><Button variant="text" size="small" onClick={() => navigate('/overview')}>概览</Button><span className="self-center text-xs text-[var(--muted)]">代理</span><Button variant="text" size="small" onClick={() => navigate('/subscriptions')}>配置集</Button><Button variant="text" size="small" onClick={() => navigate('/custom-nodes')}>自定义节点</Button><span className="self-center text-xs text-[var(--muted)]">工具</span><Button variant="text" size="small" onClick={() => navigate('/network')}>网络工具</Button>{isAdmin ? <><span className="self-center text-xs text-[var(--muted)]">设置</span><Button variant="text" size="small" onClick={() => navigate('/admin')}>管理员</Button></> : null}</nav>
        </header>
        {signOutError ? <p role="alert" className="px-4 pt-2 text-xs text-red-600 md:hidden">退出失败：{signOutError}</p> : null}
        <main className="w-full p-4 sm:p-6">
          <Routes location={location}>
            <Route path="/subscriptions" element={<SubscriptionPage />} />
            <Route path="/subscriptions/:id" element={<SubscriptionDeepLink />} />
            <Route path="/custom-nodes" element={<CustomNodesPage currentUserId={user.id} />} />
            <Route path="/overview" element={<OverviewPage user={user} />} />
            <Route path="/network" element={<NetworkToolsPage />} />
            <Route path="/admin" element={isAdmin ? <AdminSettingsPage user={user} /> : <Navigate to="/subscriptions" replace />} />
            <Route path="/" element={<Navigate to="/subscriptions" replace />} />
            <Route path="*" element={<Navigate to="/subscriptions" replace />} />
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
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <div className="grid size-8 shrink-0 place-items-center rounded-xl bg-[var(--accent-subtle)] text-[var(--accent)]"><Rss size={17} /></div>
      <div className="min-w-0">
        <strong className="block truncate text-sm">Sempre Server</strong>
        <span className="block truncate text-[10px] text-[var(--muted)]">订阅管理服务</span>
      </div>
    </div>
  )
}

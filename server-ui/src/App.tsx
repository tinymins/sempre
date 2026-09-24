import { AppSidebar, Button, Card, Spin } from '@acme/components'
import { LogOut, Rss } from 'lucide-react'
import { useEffect, useState } from 'react'
import { HashRouter, Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom'
import { ServerAuth } from './ServerAuth'
import {
  loadServerSession,
  logout,
  saveServerSession,
  verifyServerSession,
  type ServerSession,
} from './server-api'

export function App() {
  const [session, setSession] = useState<ServerSession | null>(() => loadServerSession())
  const [checking, setChecking] = useState(Boolean(session))

  useEffect(() => {
    if (!session || !checking) return
    let cancelled = false
    void verifyServerSession(session).then((user) => {
      if (cancelled) return
      const verified = { ...session, user }
      saveServerSession(verified)
      setSession(verified)
    }).catch(() => {
      if (cancelled) return
      saveServerSession(null)
      setSession(null)
    }).finally(() => {
      if (!cancelled) setChecking(false)
    })
    return () => { cancelled = true }
  }, [checking, session])

  if (checking) {
    return <main aria-label="正在验证登录状态" className="grid min-h-screen place-items-center bg-[var(--background)]"><Spin size="large" /></main>
  }

  if (!session) {
    return <ServerAuth onAuthenticated={(next) => { saveServerSession(next); setSession(next) }} />
  }

  return <HashRouter><ServerShell session={session} onSignedOut={() => setSession(null)} /></HashRouter>
}

function ServerShell({ session, onSignedOut }: { session: ServerSession; onSignedOut: () => void }) {
  const location = useLocation()
  const navigate = useNavigate()
  const signOut = async () => {
    try {
      await logout(session)
    } finally {
      saveServerSession(null)
      onSignedOut()
    }
  }
  const logoutButton = (
    <Button block variant="text" icon={<LogOut size={15} />} onClick={() => void signOut()}>
      退出登录
    </Button>
  )

  return (
    <div className="flex min-h-screen bg-[var(--background)] text-[var(--text)]">
      <aside className="hidden min-h-screen md:block">
        <AppSidebar
          width={220}
          className="h-screen"
          activeKey="subscriptions"
          onSelect={() => navigate('/subscriptions')}
          header={<Brand />}
          sections={[{ items: [{ key: 'subscriptions', label: '订阅', icon: <Rss size={17} /> }] }]}
          footer={<div className="space-y-2"><p className="truncate px-2 text-xs text-[var(--muted)]">{session.user.email}</p>{logoutButton}</div>}
        />
      </aside>
      <div className="min-w-0 flex-1">
        <header className="flex h-14 items-center justify-between border-b border-[var(--border)] bg-[var(--surface)] px-4 md:hidden">
          <Brand />
          <Button variant="text" size="small" icon={<LogOut size={15} />} aria-label="退出登录" onClick={() => void signOut()} />
        </header>
        <main className="mx-auto w-full max-w-6xl p-4 sm:p-6 lg:p-8">
          <Routes location={location}>
            <Route path="/subscriptions" element={<SubscriptionsShell />} />
            <Route path="/" element={<Navigate to="/subscriptions" replace />} />
            <Route path="*" element={<Navigate to="/subscriptions" replace />} />
          </Routes>
        </main>
      </div>
    </div>
  )
}

function Brand() {
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <div className="grid size-8 shrink-0 place-items-center rounded-xl bg-emerald-500/12 text-emerald-600 dark:text-emerald-400"><Rss size={17} /></div>
      <div className="min-w-0">
        <strong className="block truncate text-sm">Sempre Server</strong>
        <span className="block truncate text-[10px] text-[var(--muted)]">订阅管理服务</span>
      </div>
    </div>
  )
}

function SubscriptionsShell() {
  return (
    <section aria-labelledby="subscriptions-title" className="space-y-5">
      <div>
        <p className="text-xs font-medium uppercase tracking-[0.18em] text-emerald-600 dark:text-emerald-400">Sempre Server</p>
        <h1 id="subscriptions-title" className="mt-1 text-2xl font-semibold tracking-tight">订阅</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">管理订阅配置、输出地址和转换结果。</p>
      </div>
      <Card className="min-h-64">
        <div className="grid min-h-52 place-items-center text-center">
          <div>
            <div className="mx-auto grid size-11 place-items-center rounded-2xl bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"><Rss size={20} /></div>
            <h2 className="mt-4 font-medium">独立服务端前端已就绪</h2>
            <p className="mt-1 max-w-md text-sm leading-6 text-[var(--muted)]">下一阶段接入订阅列表；当前页面不会自动选择或跳转到任何订阅。</p>
          </div>
        </div>
      </Card>
    </section>
  )
}

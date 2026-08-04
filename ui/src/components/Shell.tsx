import { useEffect, useState, type ReactNode } from 'react'
import { NavLink } from 'react-router-dom'
import { Activity, Cable, ChartNoAxesCombined, ChevronLeft, CircleGauge, Languages, Library, ListTree, LogOut, Menu, Moon, Network, Rss, Server, Settings, Sun } from 'lucide-react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { SystemStatus } from '../lib/types'
import { cn } from '../lib/cn'
import { Badge, Button } from './ui'

type Theme = 'system' | 'light' | 'dark'

export function Shell({ children }: { children: ReactNode }) {
  const { t, locale, setLocale } = useI18n()
  const { session, setSession } = useSession()
  const [mobileOpen, setMobileOpen] = useState(false)
  const [theme, setTheme] = useState<Theme>(() => (localStorage.getItem('sempre.theme') as Theme) || 'system')
  const system = useQuery({
    queryKey: ['system'],
    queryFn: () => api<SystemStatus>(session!, '/system'),
    enabled: Boolean(session),
    refetchInterval: 5000,
  })
  useEffect(() => {
    localStorage.setItem('sempre.theme', theme)
    const dark = theme === 'dark' || (theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches)
    document.documentElement.classList.toggle('dark', dark)
  }, [theme])
  const nav = [
    { path: '/', label: t('overview'), icon: CircleGauge },
    { path: '/custom-nodes', label: t('customNodes'), icon: Library },
    { path: '/subscriptions', label: t('subscriptions'), icon: Rss },
    { path: '/proxies', label: t('proxies'), icon: Network },
    { path: '/connections', label: t('connections'), icon: Cable },
    { path: '/rules', label: t('rules'), icon: ListTree },
    { path: '/traffic', label: t('traffic'), icon: ChartNoAxesCombined },
    { path: '/logs', label: t('logs'), icon: Activity },
    { path: '/management', label: t('management'), icon: Settings },
  ]
  const runtime = system.data?.runtime.state || 'stopped'

  return (
    <div className="min-h-screen bg-[var(--background)] text-[var(--text)]">
      {mobileOpen ? <button aria-label="Close navigation" className="fixed inset-0 z-30 bg-black/35 lg:hidden" onClick={() => setMobileOpen(false)} /> : null}
      <aside className={cn('fixed inset-y-0 left-0 z-40 flex w-56 flex-col border-r border-[var(--border)] bg-[var(--sidebar)] transition-transform lg:translate-x-0', mobileOpen ? 'translate-x-0' : '-translate-x-full')}>
        <div className="flex h-16 items-center gap-3 border-b border-[var(--border)] px-4">
          <span className="grid size-9 place-items-center rounded-lg bg-emerald-600 text-white"><Server size={18} /></span>
          <div className="min-w-0"><p className="font-semibold">Sempre</p><p className="truncate text-xs text-[var(--muted)]">{system.data?.version || 'Control plane'}</p></div>
          <Button className="ml-auto lg:hidden" size="icon" variant="ghost" title="Close" onClick={() => setMobileOpen(false)}><ChevronLeft size={18} /></Button>
        </div>
        <nav className="flex-1 space-y-1 p-3">
          {nav.map(({ path, label, icon: Icon }) => (
            <NavLink key={path} to={path} end={path === '/'} onClick={() => setMobileOpen(false)} className={({ isActive }) => cn('flex h-10 items-center gap-3 rounded-md px-3 text-sm font-medium text-[var(--muted)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text)]', isActive && 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-400')}>
              <Icon size={18} /><span>{label}</span>
            </NavLink>
          ))}
        </nav>
        <div className="border-t border-[var(--border)] p-3">
          <div className="flex items-center justify-between px-2"><span className="text-xs text-[var(--muted)]">{t('core')}</span><Badge tone={runtime === 'running' ? 'success' : runtime === 'idle' ? 'warning' : 'neutral'}>{runtime}</Badge></div>
        </div>
      </aside>
      <div className="lg:pl-56">
        <header className="sticky top-0 z-20 flex h-16 items-center border-b border-[var(--border)] bg-[color:var(--background)]/95 px-4 backdrop-blur md:px-6">
          <Button className="mr-2 lg:hidden" size="icon" variant="ghost" title="Menu" onClick={() => setMobileOpen(true)}><Menu size={19} /></Button>
          <div className="flex items-center gap-2 text-sm"><span className={cn('size-2 rounded-full', runtime === 'running' ? 'bg-emerald-500' : runtime === 'idle' ? 'bg-amber-500' : 'bg-zinc-400')} /><span className="hidden text-[var(--muted)] sm:inline">{system.data?.active ? `${system.data.active.core} ${system.data.active.version}` : t('noCore')}</span></div>
          <div className="ml-auto flex items-center gap-1">
            <Button size="icon" variant="ghost" title={t('language')} onClick={() => setLocale(locale === 'zh-CN' ? 'en' : 'zh-CN')}><Languages size={18} /></Button>
            <Button size="icon" variant="ghost" title={t('theme')} onClick={() => setTheme(theme === 'system' ? 'light' : theme === 'light' ? 'dark' : 'system')}>
              {theme === 'dark' ? <Moon size={18} /> : theme === 'light' ? <Sun size={18} /> : <CircleGauge size={18} />}
            </Button>
            <Button size="icon" variant="ghost" title={t('logout')} aria-label={t('logout')} onClick={() => setSession(null)}><LogOut size={18} /></Button>
          </div>
        </header>
        {system.data?.web.password_warning || session?.warning === 'PASSWORD_EMPTY' ? <div className="border-b border-amber-400/40 bg-amber-400/12 px-4 py-2 text-center text-xs font-medium text-amber-800 dark:text-amber-300">{t('emptyPassword')}</div> : null}
        <main className="mx-auto w-full max-w-[1600px] p-4 md:p-6">{children}</main>
      </div>
    </div>
  )
}

import { useEffect, useState, type CSSProperties, type ReactNode } from 'react'
import { NavLink, useLocation } from 'react-router-dom'
import { Activity, Cable, ChartNoAxesCombined, ChevronDown, ChevronLeft, ChevronRight, CircleGauge, Cpu, DatabaseZap, Globe2, Languages, Library, ListFilter, ListTree, LogOut, Menu, Moon, Network, Router, Rss, Server, Settings, Sun, Waypoints, X, type LucideIcon } from 'lucide-react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { useTheme } from '../lib/theme'
import type { NetworkSettingsResponse, SystemStatus } from '../lib/types'
import { cn } from '../lib/cn'
import { AcmeContentBoundary } from './AcmeContentBoundary'
import { RuntimeRestartButton } from './RuntimeRestartButton'
import { Badge, Button } from './ui'

const SIDEBAR_COLLAPSED_KEY = 'sempre.sidebar.collapsed'

export interface ShellNavigationItem {
  path: string
  label: string
  icon: LucideIcon
}

interface ShellNavigationSection {
  key: string
  label?: string
  icon?: LucideIcon
  collapsible?: boolean
  items: ShellNavigationItem[]
}

export interface ShellChrome {
  subtitle: string
  statusLabel: string
  statusDetail: string
  statusTone?: 'success' | 'warning' | 'neutral'
  onLogout: () => void
}

export function Shell({ children, navigation, chrome }: { children: ReactNode; navigation?: ShellNavigationItem[]; chrome?: ShellChrome }) {
  const { t, locale, setLocale } = useI18n()
  const { session, setSession } = useSession()
  const location = useLocation()
  const [mobileOpen, setMobileOpen] = useState(false)
  const [passwordWarningDismissed, setPasswordWarningDismissed] = useState(false)
  const [desktopCollapsed, setDesktopCollapsed] = useState(() => localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === 'true')
  const analysisPaths = ['/runtime-status', '/connections', '/traffic', '/network-test', '/rules', '/logs']
  const analysisActive = analysisPaths.includes(location.pathname)
  const [analysisOpen, setAnalysisOpen] = useState(false)
  const analysisVisible = analysisActive || analysisOpen
  const { theme, setTheme } = useTheme()
  const system = useQuery({
    queryKey: ['system'],
    queryFn: () => api<SystemStatus>(session!, '/system'),
    enabled: Boolean(session) && !chrome,
    refetchInterval: 5000,
  })
  const network = useQuery({
    queryKey: ['network', 'settings'],
    queryFn: () => api<NetworkSettingsResponse>(session!, '/network/settings'),
    enabled: Boolean(session) && !chrome && !navigation,
  })
  useEffect(() => {
    localStorage.setItem(SIDEBAR_COLLAPSED_KEY, String(desktopCollapsed))
  }, [desktopCollapsed])
  const defaultNavigation: ShellNavigationSection[] = [
    { key: 'overview', items: [{ path: '/', label: t('overview'), icon: CircleGauge }] },
    { key: 'strategy', label: t('navigationStrategy'), items: [
      { path: '/proxies', label: t('proxies'), icon: Network },
      { path: '/routing-rules', label: t('routingRules'), icon: ListFilter },
    ] },
    { key: 'configuration', label: t('navigationConfiguration'), items: [
      { path: '/subscriptions', label: t('navigationSubscriptions'), icon: Rss },
      { path: '/custom-nodes', label: t('customNodes'), icon: Library },
    ] },
    { key: 'network', label: t('navigationNetwork'), items: [
      { path: '/dns', label: t('dns'), icon: DatabaseZap },
      { path: '/tunnels', label: t('tunnels'), icon: Waypoints },
      ...(network.data?.settings?.mode === 'gateway' ? [{ path: '/gateway', label: t('gateway'), icon: Router }] : []),
    ] },
    { key: 'analysis', label: t('navigationAnalysis'), icon: ChartNoAxesCombined, collapsible: true, items: [
      { path: '/runtime-status', label: t('navigationCoreStatus'), icon: Cpu },
      { path: '/connections', label: t('connections'), icon: Cable },
      { path: '/traffic', label: t('traffic'), icon: ChartNoAxesCombined },
      { path: '/network-test', label: t('networkTest'), icon: Globe2 },
      { path: '/rules', label: t('navigationEffectiveRules'), icon: ListTree },
      { path: '/logs', label: t('logs'), icon: Activity },
    ] },
    { key: 'system', label: t('navigationSystem'), items: [{ path: '/management', label: t('management'), icon: Settings }] },
  ]
  const sections: ShellNavigationSection[] = navigation ? [{ key: 'custom', items: navigation }] : defaultNavigation
  const runtime = system.data?.runtime.state || 'stopped'
  const statusLabel = chrome?.statusLabel ?? t('core')
  const statusDetail = chrome?.statusDetail ?? (system.data?.active ? `${system.data.active.core} ${system.data.active.version}` : t('noCore'))
  const statusTone = chrome?.statusTone ?? (runtime === 'running' ? 'success' : runtime === 'idle' ? 'warning' : 'neutral')
  const collapsedStatusDetail = chrome?.statusDetail ?? runtime
  const sidebarAction = desktopCollapsed ? t('expandSidebar') : t('collapseSidebar')
  const shellStyle = {
    '--shell-sidebar-width': desktopCollapsed ? '4rem' : '14rem',
    '--shell-sidebar-center-offset': desktopCollapsed ? '2rem' : '7rem',
  } as CSSProperties

  return (
    <div className="min-h-screen bg-[var(--background)] text-[var(--text)]" data-sidebar-collapsed={desktopCollapsed} style={shellStyle}>
      {mobileOpen ? <button aria-label="Close navigation" className="fixed inset-0 z-30 bg-black/35 lg:hidden" onClick={() => setMobileOpen(false)} /> : null}
      <aside id="primary-navigation" className={cn('fixed inset-y-0 left-0 z-40 flex w-56 flex-col overflow-hidden border-r border-[var(--border)] bg-[var(--sidebar)] transition-[transform,width] duration-200 ease-out lg:w-[var(--shell-sidebar-width)] lg:translate-x-0', mobileOpen ? 'translate-x-0' : '-translate-x-full')}>
        <div className={cn('flex h-16 shrink-0 items-center gap-3 border-b border-[var(--border)] px-4', desktopCollapsed && 'lg:justify-center lg:px-0')}>
          <span className="grid size-9 place-items-center rounded-lg bg-emerald-600 text-white"><Server size={18} /></span>
          <div className={cn('min-w-0', desktopCollapsed && 'lg:sr-only')}><p className="font-semibold">Sempre</p><p className="truncate text-xs text-[var(--muted)]">{chrome?.subtitle ?? system.data?.version ?? 'Control plane'}</p></div>
          <Button className="ml-auto lg:hidden" size="icon" variant="ghost" title="Close" onClick={() => setMobileOpen(false)}><ChevronLeft size={18} /></Button>
        </div>
        <nav className={cn('flex-1 space-y-3 overflow-y-auto p-3', desktopCollapsed && 'lg:p-2')}>
          {sections.map((section) => {
            const open = !section.collapsible || analysisVisible
            const SectionIcon = section.icon
            return <div key={section.key} className="space-y-1">
              {section.label ? section.collapsible ? (
                <button type="button" title={desktopCollapsed ? section.label : undefined} aria-expanded={analysisVisible} className={cn('flex h-8 w-full items-center gap-2 rounded-md px-3 text-xs font-semibold text-[var(--muted)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text)]', analysisActive && 'text-emerald-700 dark:text-emerald-400', desktopCollapsed && 'lg:h-10 lg:justify-center lg:px-0')} onClick={() => setAnalysisOpen((value) => !value)}>
                  {SectionIcon ? <SectionIcon size={16} /> : null}<span className={cn('flex-1 text-left', desktopCollapsed && 'lg:sr-only')}>{section.label}</span><ChevronDown className={cn('transition-transform', analysisVisible && 'rotate-180', desktopCollapsed && 'lg:hidden')} size={15} />
                </button>
              ) : <p className={cn('px-3 pt-2 text-[10px] font-semibold uppercase tracking-wider text-[var(--muted)]', desktopCollapsed && 'lg:hidden')}>{section.label}</p> : null}
              {open ? section.items.map(({ path, label, icon: Icon }) => (
                <NavLink key={path} to={path} end={path === '/'} aria-label={label} title={desktopCollapsed ? label : undefined} onClick={() => setMobileOpen(false)} className={({ isActive }) => cn('flex h-10 items-center gap-3 rounded-md px-3 text-sm font-medium text-[var(--muted)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text)]', section.collapsible && 'pl-7', desktopCollapsed && 'lg:justify-center lg:gap-0 lg:px-0', isActive && 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-400')}>
                  <Icon size={18} /><span className={cn(desktopCollapsed && 'lg:sr-only')}>{label}</span>
                </NavLink>
              )) : null}
            </div>
          })}
        </nav>
        <div className="border-t border-[var(--border)] p-3">
          <div className={cn('flex items-center justify-between gap-2 px-2', desktopCollapsed && 'lg:hidden')}><span className="truncate text-xs text-[var(--muted)]">{statusLabel}</span><Badge tone={statusTone}>{chrome ? statusTone : runtime}</Badge></div>
          {desktopCollapsed ? <div className="hidden place-items-center lg:grid" aria-label={`${statusLabel}: ${collapsedStatusDetail}`} title={`${statusLabel}: ${collapsedStatusDetail}`}><span className={cn('size-2.5 rounded-full', statusTone === 'success' ? 'bg-emerald-500' : statusTone === 'warning' ? 'bg-amber-500' : 'bg-zinc-400')} /></div> : null}
        </div>
      </aside>
      <div className="transition-[padding-left] duration-200 ease-out lg:pl-[var(--shell-sidebar-width)]">
        <header className="sticky top-0 z-20 flex h-16 items-center border-b border-[var(--border)] bg-[color:var(--background)]/95 px-4 backdrop-blur md:px-6">
          <Button className="mr-2 lg:hidden" size="icon" variant="ghost" title="Menu" onClick={() => setMobileOpen(true)}><Menu size={19} /></Button>
          <Button className="mr-2 hidden lg:inline-flex" size="icon" variant="ghost" title={sidebarAction} aria-label={sidebarAction} aria-controls="primary-navigation" aria-expanded={!desktopCollapsed} onClick={() => setDesktopCollapsed((collapsed) => !collapsed)}>
            {desktopCollapsed ? <ChevronRight size={18} /> : <ChevronLeft size={18} />}
          </Button>
          <div className="flex items-center gap-2 text-sm"><span className={cn('size-2 rounded-full', statusTone === 'success' ? 'bg-emerald-500' : statusTone === 'warning' ? 'bg-amber-500' : 'bg-zinc-400')} /><span className="hidden text-[var(--muted)] sm:inline">{statusDetail}</span></div>
          <div className="ml-auto flex items-center gap-1">
            {!chrome ? <RuntimeRestartButton /> : null}
            <Button size="icon" variant="ghost" title={t('language')} onClick={() => setLocale(locale === 'zh-CN' ? 'en' : 'zh-CN')}><Languages size={18} /></Button>
            <Button size="icon" variant="ghost" title={t('theme')} onClick={() => setTheme(theme === 'system' ? 'light' : theme === 'light' ? 'dark' : 'system')}>
              {theme === 'dark' ? <Moon size={18} /> : theme === 'light' ? <Sun size={18} /> : <CircleGauge size={18} />}
            </Button>
            <Button size="icon" variant="ghost" title={t('logout')} aria-label={t('logout')} onClick={chrome?.onLogout ?? (() => setSession(null))}><LogOut size={18} /></Button>
          </div>
        </header>
        {!chrome && !passwordWarningDismissed && (system.data?.web.password_warning || session?.warning === 'PASSWORD_EMPTY') ? <div className="relative border-b border-amber-400/40 bg-amber-400/12 px-10 py-2 text-center text-xs font-medium text-amber-800 dark:text-amber-300">{t('emptyPassword')}<Button className="absolute right-2 top-1/2 size-6 -translate-y-1/2 text-amber-800 hover:bg-amber-400/15 hover:text-amber-900 dark:text-amber-300 dark:hover:text-amber-200" size="icon" variant="ghost" title={t('close')} aria-label={t('close')} onClick={() => setPasswordWarningDismissed(true)}><X size={15} /></Button></div> : null}
        <main className="mx-auto w-full max-w-[1600px] p-4 md:p-6"><AcmeContentBoundary>{children}</AcmeContentBoundary></main>
      </div>
    </div>
  )
}

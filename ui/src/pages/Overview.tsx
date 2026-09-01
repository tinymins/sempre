import { useCallback, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Activity, ArrowDownToLine, ArrowRight, ArrowUpFromLine, Cable, Cpu, Gauge, Server } from 'lucide-react'
import { Link } from 'react-router-dom'
import { api } from '../lib/api'
import { formatBytes } from '../lib/format'
import { useI18n } from '../lib/i18n'
import { useRuntimeEvents } from '../lib/useRuntimeEvents'
import { useSession } from '../lib/session'
import type { Overview as OverviewData, RuntimeEvent, SystemStatus } from '../lib/types'
import { Card, EmptyState, Badge, PageTitle } from '../components/ui'
import { RuntimeChart, type ChartPoint } from '../components/RuntimeChart'
import { AutoConfigureCard } from '../features/auto-config/AutoConfigureCard'

export function Overview() {
  const { t } = useI18n()
  const { session } = useSession()
  const [points, setPoints] = useState<ChartPoint[]>([])
  const [rates, setRates] = useState({ download: 0, upload: 0, memory: 0, connections: 0 })
  const system = useQuery({ queryKey: ['system'], queryFn: () => api<SystemStatus>(session!, '/system'), refetchInterval: 5000 })
  const overview = useQuery({
    queryKey: ['runtime', 'overview'], queryFn: () => api<OverviewData>(session!, '/runtime/overview'),
    enabled: system.data?.runtime.state === 'running', refetchInterval: 5000, retry: false,
  })
  const onEvent = useCallback((event: RuntimeEvent) => {
    if (event.topic === 'traffic' && event.data) {
      const data = event.data as { down?: number; up?: number }
      const download = data.down || 0
      const upload = data.up || 0
      setRates((current) => ({ ...current, download, upload }))
      setPoints((current) => [...current, { time: Math.floor(Date.now() / 1000), download, upload }].slice(-120))
    } else if (event.topic === 'memory' && event.data) {
      const data = event.data as { inuse?: number }
      setRates((current) => ({ ...current, memory: data.inuse || 0 }))
    } else if (event.topic === 'connections' && event.data) {
      const data = event.data as { connections?: unknown[] }
      setRates((current) => ({ ...current, connections: data.connections?.length || 0 }))
    }
  }, [])
  useRuntimeEvents(['traffic', 'memory', 'connections'], onEvent, system.data?.runtime.state === 'running')

  return (
    <div className="space-y-6">
      <PageTitle title={t('overview')} />
      <SystemSummary system={system.data} />
      {system.data && (!system.data.selected || !system.data.active) ? <AutoConfigureCard /> : null}
      {system.data && system.data.runtime.state !== 'running' ? (
        <EmptyState title={system.data.active ? t('coreNotRunning') : t('noCore')} detail={system.data.active ? t('coreNotRunningDetail') : t('noCoreDetail')} />
      ) : (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-3 xl:grid-cols-6">
            <Metric icon={ArrowDownToLine} label={t('download')} value={formatBytes(rates.download, '/s')} tone="cyan" />
            <Metric icon={ArrowUpFromLine} label={t('upload')} value={formatBytes(rates.upload, '/s')} tone="green" />
            <Metric icon={Cable} label={t('activeConnections')} value={String(rates.connections || overview.data?.connections || 0)} tone="amber" />
            <Metric icon={Cpu} label={t('memory')} value={`${formatBytes(system.data?.service_memory)} + ${formatBytes(rates.memory)}`} tone="red" />
            <Metric icon={Gauge} label={t('download')} value={formatBytes(overview.data?.download || 0)} tone="cyan" />
            <Metric icon={Activity} label={t('upload')} value={formatBytes(overview.data?.upload || 0)} tone="green" />
          </div>
          <Card className="p-4 md:p-5">
            <div className="mb-4 flex items-center justify-between"><div><h2 className="text-sm font-semibold">{t('realtimeTraffic')}</h2><p className="mt-1 text-xs text-[var(--muted)]">120 seconds</p></div><Badge tone="success">{t('live')}</Badge></div>
            <RuntimeChart points={points} />
          </Card>
        </>
      )}
    </div>
  )
}

function SystemSummary({ system }: { system?: SystemStatus }) {
  const { t } = useI18n()
  const runtimeState = system?.runtime.state || ''
  const coreName = system?.active ? `${system.active.core} ${system.active.version}` : system?.selected ? `${system.selected.core}@${system.selected.ref}` : t('noCore')
  return <Card className="overflow-hidden">
    <div className="grid md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:divide-x md:divide-[var(--border)]">
      <SummaryItem icon={Server} label={t('sempreService')} value={system ? `Sempre ${system.version}` : t('loading')} detail={system?.mode || '-'} status={system?.service === 'running' ? t('online') : system?.service || t('loading')} tone={system?.service === 'running' ? 'success' : 'warning'} />
      <SummaryItem icon={Cpu} label={t('managedCore')} value={coreName} detail={system?.active?.ref || system?.selected?.ref || '-'} status={runtimeState ? runtimeLabel(runtimeState, t) : t('loading')} tone={runtimeState === 'running' ? 'success' : runtimeState === 'failed' ? 'danger' : 'warning'} />
      <Link className="flex items-center justify-center gap-2 border-t border-[var(--border)] px-5 py-4 text-sm font-medium text-emerald-700 hover:bg-[var(--surface-hover)] dark:text-emerald-400 md:border-t-0" to="/runtime-status">{t('navigationCoreStatus')}<ArrowRight size={16} /></Link>
    </div>
    {system?.pending ? <div className="border-t border-amber-500/25 bg-amber-500/8 px-4 py-2 text-sm text-amber-800 dark:text-amber-300">{t('pendingChange')}</div> : null}
    {system?.last_error ? <div className="border-t border-red-500/25 bg-red-500/8 px-4 py-2 text-sm text-red-700 dark:text-red-300">{system.last_error}</div> : null}
  </Card>
}

function SummaryItem({ icon: Icon, label, value, detail, status, tone }: { icon: typeof Server; label: string; value: string; detail: string; status: string; tone: 'success' | 'warning' | 'danger' }) {
  return <div className="flex min-w-0 items-center gap-3 border-t border-[var(--border)] p-4 first:border-t-0 md:border-t-0 md:p-5"><span className="grid size-9 shrink-0 place-items-center rounded-md bg-emerald-500/10 text-emerald-600"><Icon size={18} /></span><div className="min-w-0 flex-1"><p className="text-xs text-[var(--muted)]">{label}</p><p className="mt-1 truncate text-sm font-semibold">{value}</p><p className="mt-0.5 truncate text-xs text-[var(--muted)]">{detail}</p></div><Badge tone={tone}>{status}</Badge></div>
}

function Metric({ icon: Icon, label, value, tone }: { icon: typeof Activity; label: string; value: string; tone: 'cyan' | 'green' | 'amber' | 'red' }) {
  const colors = { cyan: 'text-cyan-600 bg-cyan-500/10', green: 'text-emerald-600 bg-emerald-500/10', amber: 'text-amber-600 bg-amber-500/10', red: 'text-red-600 bg-red-500/10' }
  return <Card className="min-w-0 p-4"><span className={`grid size-8 place-items-center rounded-md ${colors[tone]}`}><Icon size={17} /></span><p className="mt-5 truncate text-xs text-[var(--muted)]">{label}</p><p className="mt-1 truncate text-lg font-semibold tabular-nums">{value}</p></Card>
}

type Translate = ReturnType<typeof useI18n>['t']

function runtimeLabel(state: string, t: Translate) {
  return ({ running: t('running'), stopped: t('stopped'), idle: t('idle'), starting: t('starting'), stopping: t('stopping'), restarting: t('restarting'), failed: t('failed') } as Record<string, string>)[state] || state
}

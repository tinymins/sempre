import { useCallback, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Activity, ArrowDownToLine, ArrowUpFromLine, Cable, Cpu, Gauge } from 'lucide-react'
import { api } from '../lib/api'
import { formatBytes } from '../lib/format'
import { useI18n } from '../lib/i18n'
import { useRuntimeEvents } from '../lib/useRuntimeEvents'
import { useSession } from '../lib/session'
import type { Overview as OverviewData, RuntimeEvent, SystemStatus } from '../lib/types'
import { Card, EmptyState, Badge, PageTitle } from '../components/ui'
import { RuntimeChart, type ChartPoint } from '../components/RuntimeChart'
import { RuntimeControlPanel } from '../components/RuntimeControlPanel'

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
      <PageTitle title={t('overview')} detail={system.data?.active ? `${system.data.active.core} ${system.data.active.version}` : t('noCore')}>
        <Badge tone={system.data?.runtime.state === 'running' ? 'success' : 'warning'}>{system.data?.runtime.state || t('loading')}</Badge>
      </PageTitle>
      <RuntimeControlPanel />
      {system.data && system.data.runtime.state !== 'running' ? (
        <EmptyState title={system.data.active ? t('coreNotRunning') : t('noCore')} detail={system.data.active ? t('coreNotRunningDetail') : t('noCoreDetail')} />
      ) : (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-3 xl:grid-cols-6">
            <Metric icon={ArrowDownToLine} label={t('download')} value={formatBytes(rates.download, '/s')} tone="cyan" />
            <Metric icon={ArrowUpFromLine} label={t('upload')} value={formatBytes(rates.upload, '/s')} tone="green" />
            <Metric icon={Cable} label={t('activeConnections')} value={String(rates.connections || overview.data?.connections || 0)} tone="amber" />
            <Metric icon={Cpu} label={t('memory')} value={formatBytes(rates.memory)} tone="red" />
            <Metric icon={Gauge} label={t('download')} value={formatBytes(overview.data?.download || 0)} tone="cyan" />
            <Metric icon={Activity} label={t('upload')} value={formatBytes(overview.data?.upload || 0)} tone="green" />
          </div>
          <Card className="p-4 md:p-5">
            <div className="mb-4 flex items-center justify-between"><div><h2 className="text-sm font-semibold">{t('realtimeTraffic')}</h2><p className="mt-1 text-xs text-[var(--muted)]">120 seconds</p></div><Badge tone="success">{t('live')}</Badge></div>
            <RuntimeChart points={points} />
          </Card>
          <div className="grid gap-4 lg:grid-cols-2">
            <Card className="p-5"><h2 className="text-sm font-semibold">{t('core')}</h2><dl className="mt-4 grid grid-cols-2 gap-y-4 text-sm"><Info label={t('version')} value={overview.data?.version || '-'} /><Info label={t('mode')} value={overview.data?.mode || '-'} /><Info label={t('service')} value={system.data?.service || '-'} /><Info label="PID" value={String(system.data?.runtime.pid || '-')} /></dl></Card>
            <Card className="p-5"><h2 className="text-sm font-semibold">Sempre</h2><dl className="mt-4 grid grid-cols-2 gap-y-4 text-sm"><Info label={t('version')} value={system.data?.version || '-'} /><Info label="Commit" value={system.data?.commit || '-'} /><Info label={t('mode')} value={system.data?.mode || '-'} /><Info label="API" value="v1" /></dl></Card>
          </div>
        </>
      )}
    </div>
  )
}

function Metric({ icon: Icon, label, value, tone }: { icon: typeof Activity; label: string; value: string; tone: 'cyan' | 'green' | 'amber' | 'red' }) {
  const colors = { cyan: 'text-cyan-600 bg-cyan-500/10', green: 'text-emerald-600 bg-emerald-500/10', amber: 'text-amber-600 bg-amber-500/10', red: 'text-red-600 bg-red-500/10' }
  return <Card className="min-w-0 p-4"><span className={`grid size-8 place-items-center rounded-md ${colors[tone]}`}><Icon size={17} /></span><p className="mt-5 truncate text-xs text-[var(--muted)]">{label}</p><p className="mt-1 truncate text-lg font-semibold tabular-nums">{value}</p></Card>
}

function Info({ label, value }: { label: string; value: string }) {
  return <div><dt className="text-xs text-[var(--muted)]">{label}</dt><dd className="mt-1 truncate font-medium">{value}</dd></div>
}

import { useCallback, useEffect, useRef, useState } from 'react'
import { Database, Trash2 } from 'lucide-react'
import { useI18n } from '../lib/i18n'
import { useRuntimeEvents } from '../lib/useRuntimeEvents'
import type { RuntimeEvent } from '../lib/types'
import { addTraffic, aggregateTraffic, clearTraffic, type TrafficDimension } from '../lib/trafficDb'
import { formatBytes } from '../lib/format'
import { Badge, Button, Card, EmptyState, PageTitle } from '../components/ui'
import { RuntimeChart, type ChartPoint } from '../components/RuntimeChart'

interface CoreConnection {
  id: string
  upload: number
  download: number
  chains?: string[]
  metadata?: { sourceIP?: string; inboundUser?: string; host?: string; process?: string }
}

export function Traffic() {
  const { t } = useI18n()
  const [points, setPoints] = useState<ChartPoint[]>([])
  const [rate, setRate] = useState({ download: 0, upload: 0 })
  const [dimension, setDimension] = useState<TrafficDimension>('host')
  const [totals, setTotals] = useState<Array<{ label: string; download: number; upload: number }>>([])
  const previous = useRef(new Map<string, { download: number; upload: number }>())
  const onEvent = useCallback((event: RuntimeEvent) => {
    if (event.topic === 'traffic' && event.data) {
      const data = event.data as { down?: number; up?: number }
      const download = data.down || 0
      const upload = data.up || 0
      setRate({ download, upload })
      setPoints((current) => [...current, { time: Math.floor(Date.now() / 1000), download, upload }].slice(-360))
    }
    if (event.topic === 'connections' && event.data) {
      const snapshot = event.data as { connections?: CoreConnection[] }
      const records: Parameters<typeof addTraffic>[0] = []
      for (const connection of snapshot.connections || []) {
        const old = previous.current.get(connection.id) || { download: connection.download, upload: connection.upload }
        const download = Math.max(0, connection.download - old.download)
        const upload = Math.max(0, connection.upload - old.upload)
        previous.current.set(connection.id, { download: connection.download, upload: connection.upload })
        if (!download && !upload) continue
        const values: Record<TrafficDimension, string> = {
          device: connection.metadata?.sourceIP || 'unknown', user: connection.metadata?.inboundUser || 'unknown',
          host: connection.metadata?.host || 'unknown', outbound: connection.chains?.[0] || 'direct', process: connection.metadata?.process || 'unknown',
        }
        for (const [key, label] of Object.entries(values)) records.push({ time: Date.now(), dimension: key as TrafficDimension, label, download, upload })
      }
      void addTraffic(records)
    }
  }, [])
  useRuntimeEvents(['traffic', 'connections'], onEvent)
  useEffect(() => {
    let active = true
    const refresh = () => aggregateTraffic(Date.now() - 3600_000, dimension).then((value) => { if (active) setTotals(value) })
    void refresh()
    const timer = window.setInterval(refresh, 5000)
    return () => { active = false; window.clearInterval(timer) }
  }, [dimension])

  const dimensions: Array<{ value: TrafficDimension; label: string }> = [
    { value: 'device', label: t('device') }, { value: 'user', label: t('user') }, { value: 'host', label: t('host') },
    { value: 'outbound', label: t('outbound') }, { value: 'process', label: t('process') },
  ]
  return <div className="space-y-5">
    <PageTitle title={t('traffic')} detail={t('lastHour')}><Badge tone="success">{t('live')}</Badge></PageTitle>
    <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(260px,1fr)]"><Card className="p-4 md:p-5"><div className="mb-4 flex gap-6 text-sm"><div><span className="text-xs text-[var(--muted)]">↓ {t('download')}</span><p className="mt-1 font-semibold tabular-nums text-cyan-600">{formatBytes(rate.download, '/s')}</p></div><div><span className="text-xs text-[var(--muted)]">↑ {t('upload')}</span><p className="mt-1 font-semibold tabular-nums text-emerald-600">{formatBytes(rate.upload, '/s')}</p></div></div><RuntimeChart points={points} height={280} /></Card><Card className="p-5"><div className="grid size-9 place-items-center rounded-md bg-amber-500/10 text-amber-600"><Database size={18} /></div><h2 className="mt-5 text-sm font-semibold">{t('historicalTraffic')}</h2><p className="mt-2 text-sm leading-6 text-[var(--muted)]">IndexedDB · {t('lastHour')}</p><Button className="mt-5" variant="danger" size="small" onClick={() => { void clearTraffic(); setTotals([]) }}><Trash2 size={14} />{t('clear')}</Button></Card></div>
    <div className="flex gap-1 overflow-x-auto border-b border-[var(--border)] pb-2">{dimensions.map((item) => <button key={item.value} className={`h-8 shrink-0 rounded-md px-3 text-sm ${dimension === item.value ? 'bg-emerald-500/10 font-medium text-emerald-700 dark:text-emerald-400' : 'text-[var(--muted)] hover:bg-[var(--surface-hover)]'}`} onClick={() => setDimension(item.value)}>{item.label}</button>)}</div>
    {totals.length ? <div className="overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--surface)]"><table className="w-full text-left text-sm"><thead className="text-xs text-[var(--muted)]"><tr><th className="px-3 py-3 font-medium">{dimensions.find((item) => item.value === dimension)?.label}</th><th className="px-3 py-3 text-right font-medium">{t('download')}</th><th className="px-3 py-3 text-right font-medium">{t('upload')}</th><th className="px-3 py-3 text-right font-medium">{t('totalTraffic')}</th></tr></thead><tbody>{totals.slice(0, 100).map((item) => <tr key={item.label} className="border-t border-[var(--border)]"><td className="max-w-xl truncate px-3 py-3 font-medium">{item.label}</td><td className="px-3 py-3 text-right tabular-nums text-cyan-600">{formatBytes(item.download)}</td><td className="px-3 py-3 text-right tabular-nums text-emerald-600">{formatBytes(item.upload)}</td><td className="px-3 py-3 text-right tabular-nums">{formatBytes(item.download + item.upload)}</td></tr>)}</tbody></table></div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} />}
  </div>
}

import { useCallback, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Select } from '@acme/components'
import { Database, Trash2 } from 'lucide-react'
import { useI18n } from '../lib/i18n'
import { useRuntimeEvents } from '../lib/useRuntimeEvents'
import { api } from '../lib/api'
import { useSession } from '../lib/session'
import type { RuntimeEvent } from '../lib/types'
import type { TrafficDimension, TrafficHistory, TrafficSettings } from '../lib/traffic'
import { formatBytes } from '../lib/format'
import { Badge, Button, Card, EmptyState, Field, PageTitle, Spinner } from '../components/ui'
import { RuntimeChart, type ChartPoint } from '../components/RuntimeChart'

const DEFAULT_SETTINGS: TrafficSettings = { retention_hours: 24, reset_day: null, max_bytes: 32 * 1024 * 1024 }

export function Traffic() {
  const { t, locale } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [points, setPoints] = useState<ChartPoint[]>([])
  const [rate, setRate] = useState({ download: 0, upload: 0 })
  const [dimension, setDimension] = useState<TrafficDimension>('host')
  const history = useQuery({
    queryKey: ['runtime', 'traffic-history', dimension],
    queryFn: () => api<TrafficHistory>(session!, `/runtime/traffic/history?since=0&dimension=${dimension}`),
    enabled: Boolean(session),
    retry: false,
    refetchInterval: 5000,
  })
  const settings = history.data ? { ...DEFAULT_SETTINGS, ...history.data.settings } : DEFAULT_SETTINGS
  const updateSettings = useMutation({
    mutationFn: (next: TrafficSettings) => api<void>(session!, '/runtime/traffic/history', { method: 'PATCH', body: JSON.stringify(next) }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['runtime', 'traffic-history'] }),
  })
  const clear = useMutation({
    mutationFn: () => api<void>(session!, '/runtime/traffic/history', { method: 'DELETE' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['runtime', 'traffic-history'] }),
  })
  const onEvent = useCallback((event: RuntimeEvent) => {
    if (event.topic !== 'traffic' || !event.data) return
    const data = event.data as { down?: number; up?: number }
    const download = data.down || 0
    const upload = data.up || 0
    setRate({ download, upload })
    setPoints((current) => [...current, { time: Math.floor(Date.now() / 1000), download, upload }].slice(-360))
  }, [])
  useRuntimeEvents(['traffic'], onEvent)

  const dimensions: Array<{ value: TrafficDimension; label: string }> = [
    { value: 'device', label: t('device') }, { value: 'user', label: t('user') }, { value: 'host', label: t('host') },
    { value: 'outbound', label: t('outbound') }, { value: 'process', label: t('process') },
  ]
  const hour = locale === 'zh-CN' ? '小时' : 'hour'
  const day = locale === 'zh-CN' ? '天' : 'days'
  const retentionOptions = [
    { value: 1, label: `1 ${hour}` }, { value: 6, label: `6 ${hour}` }, { value: 24, label: `1 ${day}` },
    { value: 72, label: `3 ${day}` }, { value: 168, label: `7 ${day}` }, { value: 720, label: `30 ${day}` },
  ]
  const rotationOptions = [
    { value: 'rolling', label: t('rollingRetention') },
    { value: 'monthly', label: t('monthlyReset') },
  ]
  const resetDayOptions = Array.from({ length: 31 }, (_, index) => ({
    value: index + 1,
    label: locale === 'zh-CN' ? `每月 ${index + 1} 日` : `Day ${index + 1}`,
  }))
  const storageOptions = [8, 32, 64, 128, 256].map((value) => ({ value: value * 1024 * 1024, label: `${value} MiB` }))

  return <div className="space-y-5">
    <PageTitle title={t('traffic')} detail={t('lastHour')}><Badge tone="success">{t('live')}</Badge></PageTitle>
    <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(300px,1fr)]">
      <Card className="p-4 md:p-5">
        <div className="mb-4 flex gap-6 text-sm"><div><span className="text-xs text-[var(--muted)]">↓ {t('download')}</span><p className="mt-1 font-semibold tabular-nums text-cyan-600">{formatBytes(rate.download, '/s')}</p></div><div><span className="text-xs text-[var(--muted)]">↑ {t('upload')}</span><p className="mt-1 font-semibold tabular-nums text-emerald-600">{formatBytes(rate.upload, '/s')}</p></div></div>
        <RuntimeChart points={points} height={280} />
      </Card>
      <Card className="p-5">
        <div className="grid size-9 place-items-center rounded-md bg-amber-500/10 text-amber-600"><Database size={18} /></div>
        <h2 className="mt-4 text-sm font-semibold">{t('trafficStorage')}</h2>
        <p className="mt-2 text-sm leading-6 text-[var(--muted)]">{t('trafficStorageDetail')}</p>
        <p className="mt-2 text-xs tabular-nums text-[var(--muted)]">{formatBytes(history.data?.storage_bytes ?? 0)} / {formatBytes(settings.max_bytes)}</p>
        <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-1 xl:grid-cols-2">
          <Field label={t('rotationPolicy')}><Select value={settings.reset_day === null ? 'rolling' : 'monthly'} options={rotationOptions} disabled={updateSettings.isPending} onChange={(value) => updateSettings.mutate({ ...settings, reset_day: value === 'monthly' ? 1 : null })} /></Field>
          {settings.reset_day === null
            ? <Field label={t('retention')}><Select value={settings.retention_hours} options={retentionOptions} disabled={updateSettings.isPending} onChange={(value) => updateSettings.mutate({ ...settings, retention_hours: Number(value) })} /></Field>
            : <Field label={t('monthlyResetDay')}><Select value={settings.reset_day} options={resetDayOptions} disabled={updateSettings.isPending} onChange={(value) => updateSettings.mutate({ ...settings, reset_day: Number(value) })} /></Field>}
          <Field label={t('maximumStorage')}><Select value={settings.max_bytes} options={storageOptions} disabled={updateSettings.isPending} onChange={(value) => updateSettings.mutate({ ...settings, max_bytes: Number(value) })} /></Field>
        </div>
        <Button className="mt-4" variant="danger" size="small" disabled={clear.isPending} onClick={() => clear.mutate()}><Trash2 size={14} />{t('clear')}</Button>
      </Card>
    </div>
    <div className="flex gap-1 overflow-x-auto border-b border-[var(--border)] pb-2">{dimensions.map((item) => <button key={item.value} className={`h-8 shrink-0 rounded-md px-3 text-sm ${dimension === item.value ? 'bg-emerald-500/10 font-medium text-emerald-700 dark:text-emerald-400' : 'text-[var(--muted)] hover:bg-[var(--surface-hover)]'}`} onClick={() => setDimension(item.value)}>{item.label}</button>)}</div>
    {history.isLoading ? <div className="grid min-h-52 place-items-center"><Spinner /></div> : history.data?.totals.length ? <div className="overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--surface)]"><table className="w-full text-left text-sm"><thead className="text-xs text-[var(--muted)]"><tr><th className="px-3 py-3 font-medium">{dimensions.find((item) => item.value === dimension)?.label}</th><th className="px-3 py-3 text-right font-medium">{t('download')}</th><th className="px-3 py-3 text-right font-medium">{t('upload')}</th><th className="px-3 py-3 text-right font-medium">{t('totalTraffic')}</th></tr></thead><tbody>{history.data.totals.slice(0, 100).map((item) => <tr key={item.label} className="border-t border-[var(--border)]"><td className="max-w-xl truncate px-3 py-3 font-medium">{item.label}</td><td className="px-3 py-3 text-right tabular-nums text-cyan-600">{formatBytes(item.download)}</td><td className="px-3 py-3 text-right tabular-nums text-emerald-600">{formatBytes(item.upload)}</td><td className="px-3 py-3 text-right tabular-nums">{formatBytes(item.download + item.upload)}</td></tr>)}</tbody></table></div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} />}
  </div>
}

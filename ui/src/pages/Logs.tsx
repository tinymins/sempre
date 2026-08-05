import { useCallback, useMemo, useState } from 'react'
import { Select } from '@acme/components'
import { Download, Pause, Play, Search, Trash2 } from 'lucide-react'
import { useI18n } from '../lib/i18n'
import { useRuntimeEvents } from '../lib/useRuntimeEvents'
import type { RuntimeEvent } from '../lib/types'
import { Badge, Button, EmptyState, Input, PageTitle } from '../components/ui'

interface LogEntry { time: string; level: string; message: string }

export function Logs() {
  const { t } = useI18n()
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [paused, setPaused] = useState(false)
  const [search, setSearch] = useState('')
  const [level, setLevel] = useState('')
  const onEvent = useCallback((event: RuntimeEvent) => {
    if (event.topic !== 'logs' || !event.data || paused) return
    const value = event.data as { time?: string; level?: string; type?: string; payload?: string; message?: string }
    setLogs((current) => [...current, { time: value.time || event.timestamp, level: value.level || value.type || 'info', message: value.message || value.payload || JSON.stringify(value) }].slice(-2000))
  }, [paused])
  useRuntimeEvents(['logs'], onEvent)
  const levels = useMemo(() => [...new Set(logs.map((item) => item.level))].sort(), [logs])
  const filtered = useMemo(() => logs.filter((item) => (!level || item.level === level) && item.message.toLowerCase().includes(search.toLowerCase())), [logs, search, level])
  function exportLogs() {
    const blob = new Blob([filtered.map((item) => `${item.time} [${item.level}] ${item.message}`).join('\n')], { type: 'text/plain' })
    const link = document.createElement('a')
    link.href = URL.createObjectURL(blob)
    link.download = `sempre-core-${new Date().toISOString().replaceAll(':', '-')}.log`
    link.click()
    URL.revokeObjectURL(link.href)
  }

  return <div className="space-y-5">
    <PageTitle title={t('logs')} detail={`${filtered.length} / ${logs.length}`}><Badge tone={paused ? 'warning' : 'success'}>{paused ? t('paused') : t('live')}</Badge></PageTitle>
    <div className="flex flex-wrap gap-2"><div className="relative min-w-64 flex-1"><Search className="absolute left-3 top-2.5 text-[var(--muted)]" size={16} /><Input className="pl-9" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('search')} /></div><Select className="h-9 min-w-32" value={level} options={[{ value: '', label: t('all') }, ...levels.map((value) => ({ value, label: value }))]} onChange={(value) => setLevel(value as string)} /><Button title={paused ? t('resume') : t('pause')} onClick={() => setPaused(!paused)}>{paused ? <Play size={16} /> : <Pause size={16} />}{paused ? t('resume') : t('pause')}</Button><Button size="icon" title={t('export')} onClick={exportLogs}><Download size={16} /></Button><Button size="icon" variant="danger" title={t('clear')} onClick={() => setLogs([])}><Trash2 size={16} /></Button></div>
    {filtered.length ? <div className="h-[calc(100vh-205px)] min-h-96 overflow-auto rounded-lg border border-[var(--border)] bg-[#101412] p-3 font-mono text-xs leading-6 text-zinc-200">{filtered.map((item, index) => <div key={`${item.time}-${index}`} className="grid grid-cols-[9rem_4rem_minmax(0,1fr)] gap-2 border-b border-white/5 py-0.5"><span className="text-zinc-500">{new Date(item.time).toLocaleTimeString()}</span><span className={levelColor(item.level)}>{item.level}</span><span className="whitespace-pre-wrap break-all">{item.message}</span></div>)}</div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} />}
  </div>
}

function levelColor(level: string) {
  const value = level.toLowerCase()
  if (value.includes('error') || value.includes('fatal')) return 'text-red-400'
  if (value.includes('warn')) return 'text-amber-400'
  if (value.includes('debug') || value.includes('trace')) return 'text-cyan-400'
  return 'text-emerald-400'
}

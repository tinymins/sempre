import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Ban, RefreshCw, Search, X } from 'lucide-react'
import { api } from '../lib/api'
import { formatBytes, formatDate } from '../lib/format'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { Connection, ConnectionSnapshot } from '../lib/types'
import { Badge, Button, EmptyState, Input, PageTitle, Spinner } from '../components/ui'

type SortKey = 'download' | 'upload' | 'start'

export function Connections() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const [sort, setSort] = useState<SortKey>('download')
  const connections = useQuery({ queryKey: ['runtime', 'connections'], queryFn: () => api<ConnectionSnapshot>(session!, '/runtime/connections'), refetchInterval: 2000, retry: false })
  const close = useMutation({
    mutationFn: (id: string) => api(session!, '/runtime/connections/close', { method: 'POST', body: JSON.stringify({ id }) }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['runtime', 'connections'] }),
  })
  const rows = useMemo(() => {
    const query = search.toLowerCase()
    return [...(connections.data?.connections || [])].filter((item) => connectionText(item).includes(query)).sort((left, right) => {
      if (sort === 'start') return new Date(right.start || 0).valueOf() - new Date(left.start || 0).valueOf()
      return right[sort] - left[sort]
    })
  }, [connections.data, search, sort])

  return <div className="space-y-5">
    <PageTitle title={t('connections')} detail={`${connections.data?.connections.length || 0} · ↓ ${formatBytes(connections.data?.download_total)} · ↑ ${formatBytes(connections.data?.upload_total)}`}>
      <div className="flex gap-2"><Button size="icon" title={t('refresh')} onClick={() => connections.refetch()}><RefreshCw size={17} /></Button><Button variant="danger" disabled={!rows.length || close.isPending} onClick={() => close.mutate('')}><Ban size={16} />{t('closeAll')}</Button></div>
    </PageTitle>
    <div className="flex flex-wrap gap-3"><div className="relative min-w-64 flex-1"><Search className="absolute left-3 top-2.5 text-[var(--muted)]" size={16} /><Input className="pl-9" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('search')} /></div><select className="h-9 rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm" value={sort} onChange={(event) => setSort(event.target.value as SortKey)}><option value="download">{t('download')}</option><option value="upload">{t('upload')}</option><option value="start">{t('uptime')}</option></select></div>
    {connections.isLoading ? <div className="grid min-h-52 place-items-center"><Spinner /></div> : rows.length ? <div className="overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--surface)]"><div className="max-h-[calc(100vh-230px)] overflow-auto"><table className="w-full min-w-[980px] border-collapse text-left text-sm"><thead className="sticky top-0 z-10 bg-[var(--surface)] text-xs text-[var(--muted)]"><tr><th className="px-3 py-3 font-medium">{t('host')}</th><th className="px-3 py-3 font-medium">{t('source')}</th><th className="px-3 py-3 font-medium">{t('process')}</th><th className="px-3 py-3 font-medium">{t('chain')}</th><th className="px-3 py-3 text-right font-medium">{t('download')}</th><th className="px-3 py-3 text-right font-medium">{t('upload')}</th><th className="w-14" /></tr></thead><tbody>{rows.map((item) => <ConnectionRow key={item.id} item={item} close={() => close.mutate(item.id)} busy={close.isPending} />)}</tbody></table></div></div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} />}
  </div>
}

function ConnectionRow({ item, close, busy }: { item: Connection; close: () => void; busy: boolean }) {
  const { t } = useI18n()
  const target = item.metadata.host || item.metadata.destination_ip || '-'
  return <tr className="border-t border-[var(--border)] hover:bg-[var(--surface-hover)]"><td className="max-w-72 px-3 py-3"><p className="truncate font-medium" title={target}>{target}</p><p className="mt-1 text-xs text-[var(--muted)]">{item.metadata.destination_port} · {item.metadata.network} · {formatDate(item.start)}</p></td><td className="px-3 py-3"><p>{item.metadata.source_ip || '-'}</p><p className="mt-1 text-xs text-[var(--muted)]">{item.metadata.inbound_user || item.metadata.source_port || '-'}</p></td><td className="max-w-48 px-3 py-3"><p className="truncate" title={item.metadata.process_path}>{item.metadata.process || '-'}</p><p className="mt-1 truncate text-xs text-[var(--muted)]">{item.rule || '-'}</p></td><td className="max-w-64 px-3 py-3"><div className="flex flex-wrap gap-1">{item.chains?.map((chain) => <Badge key={chain} tone="info">{chain}</Badge>)}</div></td><td className="px-3 py-3 text-right tabular-nums">{formatBytes(item.download)}</td><td className="px-3 py-3 text-right tabular-nums">{formatBytes(item.upload)}</td><td className="px-2 py-2"><Button size="icon" variant="ghost" title={t('close')} disabled={busy} onClick={close}>{busy ? <Spinner /> : <X size={16} />}</Button></td></tr>
}

function connectionText(item: Connection) {
  return [item.metadata.host, item.metadata.source_ip, item.metadata.destination_ip, item.metadata.process, item.metadata.process_path, item.rule, ...(item.chains || [])].join(' ').toLowerCase()
}

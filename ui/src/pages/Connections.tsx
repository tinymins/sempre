import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button as AcmeButton } from '@acme/components'
import { ArrowDown, ArrowUp, ArrowUpDown, Ban, RefreshCw, Search, X } from 'lucide-react'
import { api } from '../lib/api'
import { formatBytes, formatDate } from '../lib/format'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { compareText } from '../lib/sort'
import type { Connection, ConnectionSnapshot } from '../lib/types'
import { Badge, Button, EmptyState, Input, PageTitle, Spinner } from '../components/ui'

type SortKey = 'host' | 'source' | 'process' | 'chain' | 'download' | 'upload' | 'start'
type SortDirection = 'asc' | 'desc'

export function Connections() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const [sort, setSort] = useState<{ key: SortKey; direction: SortDirection }>({ key: 'download', direction: 'desc' })
  const connections = useQuery({ queryKey: ['runtime', 'connections'], queryFn: () => api<ConnectionSnapshot>(session!, '/runtime/connections'), refetchInterval: 2000, retry: false })
  const close = useMutation({
    mutationFn: (id: string) => api(session!, '/runtime/connections/close', { method: 'POST', body: JSON.stringify({ id }) }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['runtime', 'connections'] }),
  })
  const connectionItems = useMemo(() => Array.isArray(connections.data?.connections) ? connections.data.connections : [], [connections.data])
  const rows = useMemo(() => {
    const query = search.toLowerCase()
    return [...connectionItems].filter((item) => connectionText(item).includes(query)).sort((left, right) => {
      const leftValue = connectionSortValue(left, sort.key)
      const rightValue = connectionSortValue(right, sort.key)
      const difference = typeof leftValue === 'number' && typeof rightValue === 'number'
        ? leftValue - rightValue
        : compareText(leftValue, rightValue)
      return sort.direction === 'asc' ? difference : -difference
    })
  }, [connectionItems, search, sort])
  const toggleSort = (key: SortKey) => setSort((current) => ({
    key,
    direction: current.key === key && current.direction === 'desc' ? 'asc' : 'desc',
  }))

  return <div className="space-y-5">
    <PageTitle title={t('connections')} detail={`${connectionItems.length} · ↓ ${formatBytes(connections.data?.download_total)} · ↑ ${formatBytes(connections.data?.upload_total)}`}>
      <div className="flex gap-2"><Button size="icon" title={t('refresh')} onClick={() => connections.refetch()}><RefreshCw size={17} /></Button><Button variant="danger" disabled={!rows.length || close.isPending} onClick={() => close.mutate('')}><Ban size={16} />{t('closeAll')}</Button></div>
    </PageTitle>
    <div className="relative min-w-64"><Search className="absolute left-3 top-2.5 text-[var(--muted)]" size={16} /><Input className="pl-9" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('search')} /></div>
    {connections.isLoading ? <div className="grid min-h-52 place-items-center"><Spinner /></div> : rows.length ? <div className="overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--surface)]"><div className="max-h-[calc(100vh-230px)] overflow-auto"><table className="w-full min-w-[1100px] border-collapse text-left text-sm"><thead className="sticky top-0 z-10 bg-[var(--surface)] text-xs text-[var(--muted)]"><tr><SortableHeader label={t('host')} sortKey="host" sort={sort} onSort={toggleSort} /><SortableHeader label={t('source')} sortKey="source" sort={sort} onSort={toggleSort} /><SortableHeader label={t('process')} sortKey="process" sort={sort} onSort={toggleSort} /><SortableHeader label={t('chain')} sortKey="chain" sort={sort} onSort={toggleSort} /><SortableHeader label={t('download')} sortKey="download" sort={sort} onSort={toggleSort} align="right" /><SortableHeader label={t('upload')} sortKey="upload" sort={sort} onSort={toggleSort} align="right" /><SortableHeader label={t('uptime')} sortKey="start" sort={sort} onSort={toggleSort} /><th className="w-14" /></tr></thead><tbody>{rows.map((item) => <ConnectionRow key={item.id} item={item} close={() => close.mutate(item.id)} busy={close.isPending} />)}</tbody></table></div></div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} />}
  </div>
}

function ConnectionRow({ item, close, busy }: { item: Connection; close: () => void; busy: boolean }) {
  const { t } = useI18n()
  const target = item.metadata.host || item.metadata.destination_ip || '-'
  return <tr className="border-t border-[var(--border)] hover:bg-[var(--surface-hover)]"><td className="max-w-72 px-3 py-3"><p className="truncate font-medium" title={target}>{target}</p><p className="mt-1 text-xs text-[var(--muted)]">{item.metadata.destination_port} · {item.metadata.network}</p></td><td className="px-3 py-3"><p>{item.metadata.source_ip || '-'}</p><p className="mt-1 text-xs text-[var(--muted)]">{item.metadata.inbound_user || item.metadata.source_port || '-'}</p></td><td className="max-w-48 px-3 py-3"><p className="truncate" title={item.metadata.process_path}>{item.metadata.process || '-'}</p><p className="mt-1 truncate text-xs text-[var(--muted)]">{item.rule || '-'}</p></td><td className="max-w-64 px-3 py-3"><div className="flex flex-wrap gap-1">{item.chains?.map((chain) => <Badge key={chain} tone="info">{chain}</Badge>)}</div></td><td className="px-3 py-3 text-right tabular-nums">{formatBytes(item.download)}</td><td className="px-3 py-3 text-right tabular-nums">{formatBytes(item.upload)}</td><td className="whitespace-nowrap px-3 py-3 tabular-nums">{formatDate(item.start)}</td><td className="px-2 py-2"><Button size="icon" variant="ghost" title={t('close')} disabled={busy} onClick={close}>{busy ? <Spinner /> : <X size={16} />}</Button></td></tr>
}

function SortableHeader({ label, sortKey, sort, onSort, align = 'left' }: { label: string; sortKey: SortKey; sort: { key: SortKey; direction: SortDirection }; onSort: (key: SortKey) => void; align?: 'left' | 'right' }) {
  const active = sort.key === sortKey
  const Icon = active ? (sort.direction === 'asc' ? ArrowUp : ArrowDown) : ArrowUpDown
  return <th className="px-3 py-3 font-medium" aria-sort={active ? (sort.direction === 'asc' ? 'ascending' : 'descending') : 'none'}><AcmeButton variant="unstyled" className={`!h-auto w-full !border-0 !p-0 hover:text-[var(--text-primary)] ${align === 'right' ? 'justify-end' : 'justify-start'}`} onClick={() => onSort(sortKey)}>{label}<Icon aria-hidden size={13} /></AcmeButton></th>
}

function connectionStart(item: Connection) {
  const value = new Date(item.start || 0).valueOf()
  return Number.isNaN(value) ? 0 : value
}

function connectionSortValue(item: Connection, key: SortKey) {
  if (key === 'download' || key === 'upload') return item[key]
  if (key === 'start') return connectionStart(item)
  if (key === 'host') return item.metadata.host || item.metadata.destination_ip || ''
  if (key === 'source') return item.metadata.source_ip || ''
  if (key === 'process') return item.metadata.process || ''
  return (item.chains || []).join(' ')
}

function connectionText(item: Connection) {
  return [item.metadata.host, item.metadata.source_ip, item.metadata.destination_ip, item.metadata.process, item.metadata.process_path, item.rule, ...(item.chains || [])].join(' ').toLowerCase()
}

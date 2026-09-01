import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Activity, ChevronDown, Gauge, RefreshCw, Search } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { ProxyNode, ProxyProvider } from '../lib/types'
import { Badge, Button, Card, EmptyState, Input, PageTitle, Spinner } from '../components/ui'

export function Proxies() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [tab, setTab] = useState<'groups' | 'providers'>('groups')
  const [search, setSearch] = useState('')
  const [notice, setNotice] = useState('')
  const proxies = useQuery({ queryKey: ['runtime', 'proxies'], queryFn: () => api<ProxyNode[]>(session!, '/runtime/proxies'), refetchInterval: 5000, retry: false })
  const providers = useQuery({ queryKey: ['runtime', 'providers'], queryFn: () => api<ProxyProvider[]>(session!, '/runtime/providers'), retry: false })
  const select = useMutation({
    mutationFn: ({ group, proxy }: { group: string; proxy: string }) => api(session!, '/runtime/proxies/select', { method: 'POST', body: JSON.stringify({ group, proxy }) }),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['runtime', 'proxies'] }); setNotice(t('operationDone')) },
    onError: (error) => setNotice(error.message),
  })
  const delay = useMutation({
    mutationFn: (name: string) => api<{ delay: number }>(session!, '/runtime/proxies/delay', { method: 'POST', body: JSON.stringify({ name }) }),
    onSuccess: (result) => setNotice(`${result.delay} ms`), onError: (error) => setNotice(error.message),
  })
  const providerAction = useMutation({
    mutationFn: ({ name, action }: { name: string; action: 'update' | 'healthcheck' }) => api(session!, `/runtime/providers/${action}`, { method: 'POST', body: JSON.stringify({ name }) }),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['runtime', 'providers'] }); setNotice(t('operationDone')) }, onError: (error) => setNotice(error.message),
  })
  const groups = useMemo(() => (proxies.data || []).filter((item) => item.all?.length && item.name.toLowerCase().includes(search.toLowerCase())), [proxies.data, search])
  const filteredProviders = useMemo(() => (providers.data || []).filter((item) => item.name.toLowerCase().includes(search.toLowerCase())), [providers.data, search])
  const hasProviders = Boolean(providers.data?.length)
  const activeTab = hasProviders ? tab : 'groups'

  return <div className="space-y-5">
    <PageTitle title={t('proxies')} detail={`${t('proxyGroups')} ${groups.length}${hasProviders ? ` · ${t('proxyProviders')} ${providers.data?.length}` : ''}`}>
      <Button size="icon" title={t('refresh')} onClick={() => { proxies.refetch(); providers.refetch() }}><RefreshCw size={17} /></Button>
    </PageTitle>
    <div className="flex flex-wrap items-center gap-3 border-b border-[var(--border)] pb-3">
      {hasProviders ? <div className="flex h-9 rounded-md bg-[var(--surface-hover)] p-1">
        <button className={`rounded px-3 text-sm ${activeTab === 'groups' ? 'bg-[var(--surface)] font-medium shadow-sm' : 'text-[var(--muted)]'}`} onClick={() => setTab('groups')}>{t('proxies')}</button>
        <button className={`rounded px-3 text-sm ${activeTab === 'providers' ? 'bg-[var(--surface)] font-medium shadow-sm' : 'text-[var(--muted)]'}`} onClick={() => setTab('providers')}>{t('proxyProviders')}</button>
      </div> : null}
      <div className="relative ml-auto w-full sm:w-72"><Search className="absolute left-3 top-2.5 text-[var(--muted)]" size={16} /><Input className="pl-9" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('search')} /></div>
    </div>
    {notice ? <div className="border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm">{notice}</div> : null}
    {proxies.isLoading ? <Loading /> : activeTab === 'groups' ? (
      groups.length ? <div className="grid gap-4">{groups.map((group) => <ProxyGroup key={group.name} group={group} onSelect={(proxy) => select.mutate({ group: group.name, proxy })} onDelay={(proxy) => delay.mutate(proxy)} busy={select.isPending || delay.isPending} />)}</div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} />
    ) : filteredProviders.length ? (
      <div className="grid gap-4 xl:grid-cols-2">{filteredProviders.map((provider) => <Card key={provider.name} className="p-4"><div className="flex items-start gap-3"><div className="grid size-9 place-items-center rounded-md bg-cyan-500/10 text-cyan-600"><Activity size={18} /></div><div className="min-w-0 flex-1"><h2 className="truncate text-sm font-semibold">{provider.name}</h2><p className="mt-1 text-xs text-[var(--muted)]">{provider.vehicle_type || provider.type} · {provider.proxies.length} {t('nodes')}</p></div><Button size="small" onClick={() => providerAction.mutate({ name: provider.name, action: 'update' })}><RefreshCw size={14} />{t('update')}</Button></div><div className="mt-4 flex flex-wrap gap-2">{provider.proxies.slice(0, 20).map((proxy) => <Badge key={proxy.name}>{proxy.name}</Badge>)}</div><Button className="mt-4" size="small" variant="ghost" onClick={() => providerAction.mutate({ name: provider.name, action: 'healthcheck' })}><Gauge size={14} />{t('healthcheck')}</Button></Card>)}</div>
    ) : <EmptyState title={t('noData')} detail={t('noDataDetail')} />}
  </div>
}

function ProxyGroup({ group, onSelect, onDelay, busy }: { group: ProxyNode; onSelect: (name: string) => void; onDelay: (name: string) => void; busy: boolean }) {
  const { t } = useI18n()
  const [open, setOpen] = useState(false)
  return <Card className="overflow-hidden"><button className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-[var(--surface-hover)]" onClick={() => setOpen(!open)}><div className="min-w-0 flex-1"><h2 className="truncate text-sm font-semibold">{group.name}</h2><p className="mt-1 truncate text-xs text-[var(--muted)]">{group.type} · {group.all?.length || 0} nodes</p></div><Badge tone="success">{group.now || '-'}</Badge><ChevronDown className={`transition-transform ${open ? 'rotate-180' : ''}`} size={17} /></button>{open ? <div className="grid border-t border-[var(--border)]" role="radiogroup">{group.all?.map((name) => { const active = name === group.now; const selectNode = () => { if (!busy && !active) onSelect(name) }; return <div key={name} role="radio" aria-checked={active} aria-disabled={busy} tabIndex={busy ? -1 : 0} className={`flex min-w-0 cursor-pointer items-center gap-2 border-b border-[var(--border)] px-3 py-2.5 transition-colors last:border-b-0 ${active ? 'bg-emerald-500/12' : 'hover:bg-[var(--surface-hover)]'} ${busy ? 'pointer-events-none opacity-50' : ''}`} onClick={selectNode} onKeyDown={(event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); selectNode() } }}><span className="min-w-0 flex-1 truncate text-sm">{name}</span><Button size="icon" variant="ghost" title={t('testLatency')} disabled={busy} onClick={(event) => { event.stopPropagation(); onDelay(name) }} onKeyDown={(event) => event.stopPropagation()}>{busy ? <Spinner /> : <Gauge size={15} />}</Button></div> })}</div> : null}</Card>
}

function Loading() { return <div className="grid min-h-52 place-items-center"><Spinner /></div> }

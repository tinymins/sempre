import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { RefreshCw, Search } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { Rule } from '../lib/types'
import { Badge, Button, EmptyState, Input, PageTitle, Spinner } from '../components/ui'

export function Rules() {
  const { t } = useI18n()
  const { session } = useSession()
  const [search, setSearch] = useState('')
  const [type, setType] = useState('')
  const rules = useQuery({ queryKey: ['runtime', 'rules'], queryFn: () => api<Rule[]>(session!, '/runtime/rules'), retry: false })
  const types = useMemo(() => [...new Set((rules.data || []).map((rule) => rule.type))].sort(), [rules.data])
  const filtered = useMemo(() => {
    const query = search.toLowerCase()
    return (rules.data || []).filter((rule) => (!type || rule.type === type) && `${rule.type} ${rule.payload} ${rule.proxy}`.toLowerCase().includes(query))
  }, [rules.data, search, type])

  return <div className="space-y-5">
    <PageTitle title={t('rules')} detail={`${filtered.length} / ${rules.data?.length || 0}`}><Button size="icon" title={t('refresh')} onClick={() => rules.refetch()}><RefreshCw size={17} /></Button></PageTitle>
    <div className="flex flex-wrap gap-3"><div className="relative min-w-64 flex-1"><Search className="absolute left-3 top-2.5 text-[var(--muted)]" size={16} /><Input className="pl-9" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('search')} /></div><select className="h-9 min-w-40 rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm" value={type} onChange={(event) => setType(event.target.value)}><option value="">{t('all')}</option>{types.map((value) => <option key={value}>{value}</option>)}</select></div>
    {rules.isLoading ? <div className="grid min-h-52 place-items-center"><Spinner /></div> : filtered.length ? <div className="overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--surface)]"><div className="max-h-[calc(100vh-220px)] overflow-auto"><table className="w-full min-w-[720px] text-left text-sm"><thead className="sticky top-0 bg-[var(--surface)] text-xs text-[var(--muted)]"><tr><th className="w-16 px-3 py-3 font-medium">#</th><th className="w-44 px-3 py-3 font-medium">{t('type')}</th><th className="px-3 py-3 font-medium">{t('payload')}</th><th className="w-52 px-3 py-3 font-medium">{t('outbound')}</th></tr></thead><tbody>{filtered.map((rule, index) => <tr key={`${rule.type}-${index}-${rule.payload}`} className="border-t border-[var(--border)] hover:bg-[var(--surface-hover)]"><td className="px-3 py-2.5 text-xs tabular-nums text-[var(--muted)]">{index + 1}</td><td className="px-3 py-2.5"><Badge tone="neutral">{rule.type}</Badge></td><td className="max-w-xl break-all px-3 py-2.5 font-mono text-xs">{rule.payload || '-'}</td><td className="px-3 py-2.5"><Badge tone="info">{rule.proxy}</Badge></td></tr>)}</tbody></table></div></div> : <EmptyState title={t('noData')} detail={t('noDataDetail')} />}
  </div>
}

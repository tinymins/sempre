import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Select, Table, type TableColumn } from '@acme/components'
import { RefreshCw, Search } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { compareText } from '../lib/sort'
import type { Rule } from '../lib/types'
import { Badge, Button, EmptyState, Input, PageTitle } from '../components/ui'

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
  const columns: Array<TableColumn<Rule>> = [
    { title: '#', key: 'index', width: 64, render: (_value, _rule, index) => <span className="text-xs tabular-nums text-[var(--muted)]">{index + 1}</span> },
    { title: t('type'), dataIndex: 'type', width: 176, sorter: (left, right) => compareText(left.type, right.type), render: (value) => <Badge tone="neutral">{value}</Badge> },
    { title: t('payload'), dataIndex: 'payload', sorter: (left, right) => compareText(left.payload, right.payload), render: (value) => <span className="break-all font-mono text-xs">{value || '-'}</span> },
    { title: t('outbound'), dataIndex: 'proxy', width: 208, sorter: (left, right) => compareText(left.proxy, right.proxy), render: (value) => <Badge tone="info">{value}</Badge> },
  ]

  return <div className="space-y-5">
    <PageTitle title={t('runtimeRules')} detail={`${filtered.length} / ${rules.data?.length || 0}`}><Button size="icon" title={t('refresh')} onClick={() => rules.refetch()}><RefreshCw size={17} /></Button></PageTitle>
    <div className="flex flex-wrap gap-3"><div className="relative min-w-64 flex-1"><Search className="absolute left-3 top-2.5 text-[var(--muted)]" size={16} /><Input className="pl-9" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('search')} /></div><Select className="h-9 min-w-40" value={type} options={[{ value: '', label: t('all') }, ...types.map((value) => ({ value, label: value }))]} onChange={(value) => setType(value as string)} /></div>
    <Table<Rule> rowKey={(rule, index) => `${rule.type}-${index}-${rule.payload}`} loading={rules.isLoading} pagination={false} columns={columns} dataSource={filtered} scroll={{ x: 720, y: 'calc(100vh - 220px)' }} locale={{ emptyText: <EmptyState title={t('noData')} detail={t('noDataDetail')} /> }} />
  </div>
}

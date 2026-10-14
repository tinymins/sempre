import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Pagination, Select, Table, type TableColumn } from '@acme/components'
import { RefreshCw, Search } from 'lucide-react'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { Button, EmptyState, Input, PageTitle, Spinner } from '../components/ui'

interface SystemRoute {
  family: 'ipv4' | 'ipv6'
  destination: string
  gateway: string | null
  interface: string | null
  metric: number | null
  kind: string | null
  table: string | null
}

const pageSize = 50

export function RouteTable() {
  const { t, locale } = useI18n()
  const { session } = useSession()
  const zh = locale === 'zh-CN'
  const [search, setSearch] = useState('')
  const [family, setFamily] = useState('all')
  const [page, setPage] = useState(1)
  const routes = useQuery({ queryKey: ['network', 'routes'], queryFn: () => api<{ routes: SystemRoute[] }>(session!, '/network/routes'), retry: false })
  const rows = useMemo(() => {
    const query = search.trim().toLowerCase()
    return (routes.data?.routes ?? []).filter((route) => (family === 'all' || route.family === family)
      && [route.destination, route.gateway, route.interface, route.metric, route.table, route.kind].join(' ').toLowerCase().includes(query))
  }, [routes.data, search, family])
  const pages = Math.max(1, Math.ceil(rows.length / pageSize))
  const currentPage = Math.min(page, pages)
  const visible = rows.slice((currentPage - 1) * pageSize, currentPage * pageSize)
  const columns: TableColumn<SystemRoute>[] = [
    { title: zh ? '协议' : 'Protocol', dataIndex: 'family', render: (value) => value === 'ipv4' ? 'IPv4' : 'IPv6' },
    { title: zh ? '目标网段' : 'Destination', dataIndex: 'destination', className: 'font-mono' },
    { title: zh ? '网关' : 'Gateway', dataIndex: 'gateway', className: 'font-mono', render: (value) => value ?? '—' },
    { title: zh ? '接口' : 'Interface', dataIndex: 'interface', render: (value) => value ?? '—' },
    { title: 'Metric', dataIndex: 'metric', render: (value) => value ?? '—' },
    { title: zh ? '类型 / 标记' : 'Type / Flags', dataIndex: 'kind', render: (value) => value ?? '—' },
    { title: zh ? '路由表' : 'Table', dataIndex: 'table', render: (value) => value ?? '—' },
  ]

  return <div className="space-y-5">
    <PageTitle title={t('routeTable')} detail={zh ? 'Sempre 主机的系统路由表 · 只读' : 'System routing table on the Sempre host · Read only'}>
      <Button disabled={routes.isFetching} onClick={() => { void routes.refetch() }}><RefreshCw size={16} className={routes.isFetching ? 'animate-spin' : ''} />{zh ? '刷新' : 'Refresh'}</Button>
    </PageTitle>
    <div className="flex flex-wrap gap-2">
      <div className="relative min-w-48 flex-1"><Search className="absolute left-3 top-2.5 text-[var(--muted)]" size={16} /><Input aria-label={t('search')} className="pl-9" value={search} onChange={(event) => { setSearch(event.target.value); setPage(1) }} placeholder={zh ? '搜索网段、网关、接口…' : 'Search destination, gateway, interface…'} /></div>
      <Select aria-label={zh ? '协议' : 'Protocol'} className="w-36" value={family} onChange={(value) => { setFamily(value); setPage(1) }} options={[{ value: 'all', label: zh ? '全部协议' : 'All protocols' }, { value: 'ipv4', label: 'IPv4' }, { value: 'ipv6', label: 'IPv6' }]} />
    </div>
    {routes.isError ? <div role="alert" className="rounded-lg border border-[var(--border)] p-4 text-[var(--danger)]"><p>{zh ? '无法读取系统路由表' : 'Unable to read the system routing table'}</p><p className="mt-1 text-sm">{routes.error.message}</p></div> : null}
    {routes.isLoading ? <div className="grid min-h-52 place-items-center"><Spinner /></div> : rows.length ? <>
      <Table rowKey={(_route, index) => `${currentPage}-${index}`} columns={columns} dataSource={visible} pagination={false} scroll={{ x: 860 }} />
      <Pagination current={currentPage} total={rows.length} pageSize={pageSize} onChange={setPage} showTotal={(total) => zh ? `共 ${total} 条` : `${total} routes`} />
    </> : !routes.isError ? <EmptyState title={t('noData')} detail={search || family !== 'all' ? (zh ? '没有符合筛选条件的路由' : 'No routes match the filters') : (zh ? '系统未返回路由记录' : 'The system returned no routes')} /> : null}
    <p className="text-xs text-[var(--muted)]">{zh ? '— 表示系统未提供该字段。Metric 为系统返回的路由指标；网关可显示链路或接口作用域标识。' : '— means the system did not provide this field. Metric is the route metric reported by the system; gateways may contain link or interface scope identifiers.'}</p>
  </div>
}

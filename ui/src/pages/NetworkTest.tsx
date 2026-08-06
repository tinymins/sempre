import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Activity, CheckCircle2, Clock3, Globe2, RefreshCw, XCircle } from 'lucide-react'
import { Button, Card, Table, Tag, type TableColumn } from '@acme/components'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { NetworkTestReport, NetworkTestResult } from '../lib/types'

const defaultResults: NetworkTestResult[] = [
  { id: 'baidu', name: 'Baidu', region: 'domestic', category: 'reachability', url: 'https://www.baidu.com/', ok: false, latency_ms: 0 },
  { id: 'google', name: 'Google', region: 'foreign', category: 'reachability', url: 'https://www.google.com/generate_204', ok: false, latency_ms: 0 },
  { id: 'domestic-ip', name: 'Domestic IP', region: 'domestic', category: 'ip', url: 'https://ip.3322.net', ok: false, latency_ms: 0 },
  { id: 'foreign-ip', name: 'Foreign IP', region: 'foreign', category: 'ip', url: 'https://api64.ipify.org?format=json', ok: false, latency_ms: 0 },
]

export function NetworkTest() {
  const { t } = useI18n()
  const { session } = useSession()
  const report = useQuery({
    queryKey: ['network', 'test'],
    queryFn: () => api<NetworkTestReport>(session!, '/network/test', { method: 'POST' }),
    retry: false,
  })
  const results = report.data?.results.length ? report.data.results : defaultResults
  const okResults = results.filter((item) => item.ok)
  const averageLatency = okResults.length ? Math.round(okResults.reduce((sum, item) => sum + item.latency_ms, 0) / okResults.length) : 0
  const domesticIP = results.find((item) => item.id === 'domestic-ip')?.ip || '-'
  const foreignIP = results.find((item) => item.id === 'foreign-ip')?.ip || '-'
  const columns = useMemo<Array<TableColumn<NetworkTestResult>>>(() => [
    {
      title: t('networkTarget'),
      key: 'target',
      minWidth: 190,
      render: (_value, record) => <div className="min-w-0"><div className="flex items-center gap-2"><span className="font-medium">{record.name}</span><Tag color={record.region === 'domestic' ? 'green' : 'blue'} bordered={false}>{record.region === 'domestic' ? t('domestic') : t('foreign')}</Tag></div><p className="mt-1 truncate text-xs text-[var(--muted)]" title={record.url}>{record.url}</p></div>,
    },
    {
      title: t('status'),
      key: 'status',
      width: 120,
      render: (_value, record) => report.isFetching ? <Tag color="processing">{t('loading')}...</Tag> : <Tag color={record.ok ? 'success' : 'error'} icon={record.ok ? <CheckCircle2 /> : <XCircle />}>{record.ok ? t('reachable') : t('unreachable')}</Tag>,
    },
    {
      title: t('latency'),
      dataIndex: 'latency_ms',
      width: 120,
      align: 'right',
      sorter: (left, right) => left.latency_ms - right.latency_ms,
      render: (value) => report.isFetching && !report.data ? '-' : value ? `${value} ms` : '-',
    },
    {
      title: 'HTTP',
      dataIndex: 'http_status',
      width: 90,
      align: 'right',
      render: (value) => report.isFetching && !report.data ? '-' : value || '-',
    },
    {
      title: t('ipAddress'),
      dataIndex: 'ip',
      width: 180,
      render: (value) => report.isFetching && !report.data ? '-' : value || '-',
    },
    {
      title: t('details'),
      dataIndex: 'detail',
      minWidth: 220,
      render: (value) => report.isError ? <span className="text-red-600 dark:text-red-400">{report.error instanceof Error ? report.error.message : t('operationFailed')}</span> : value ? <span className="text-red-600 dark:text-red-400">{value}</span> : <span className="text-[var(--muted)]">-</span>,
    },
  ], [report.data, report.error, report.isError, report.isFetching, t])

  return <div className="space-y-5">
    <div className="flex min-h-10 items-start justify-between gap-4">
      <div><h1 className="text-xl font-semibold">{t('networkTest')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('networkTestDetail')}</p></div>
      <Button variant="primary" icon={<RefreshCw size={16} />} disabled={report.isFetching} onClick={() => report.refetch()}>{t('refresh')}</Button>
    </div>
    <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
      <Metric icon={CheckCircle2} label={t('reachable')} value={`${okResults.length}/${results.length}`} tone="green" />
      <Metric icon={Clock3} label={t('averageLatency')} value={averageLatency ? `${averageLatency} ms` : '-'} tone="amber" />
      <Metric icon={Activity} label={t('domesticIP')} value={domesticIP} tone="cyan" />
      <Metric icon={Globe2} label={t('foreignIP')} value={foreignIP} tone="blue" />
    </div>
    <Card className="!rounded-lg" bodyStyle={{ padding: 0 }}>
      <Table<NetworkTestResult>
        rowKey="id"
        size="middle"
        pagination={false}
        columns={columns}
        dataSource={results}
        scroll={{ x: 980 }}
        locale={{ emptyText: report.isError ? (report.error instanceof Error ? report.error.message : t('operationFailed')) : t('noData') }}
      />
    </Card>
  </div>
}

function Metric({ icon: Icon, label, value, tone }: { icon: typeof Activity; label: string; value: string; tone: 'green' | 'amber' | 'cyan' | 'blue' }) {
  const colors = {
    green: 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400',
    amber: 'bg-amber-500/12 text-amber-700 dark:text-amber-400',
    cyan: 'bg-cyan-500/10 text-cyan-700 dark:text-cyan-400',
    blue: 'bg-blue-500/10 text-blue-700 dark:text-blue-400',
  }
  return <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}><span className={`grid size-8 place-items-center rounded-md ${colors[tone]}`}><Icon size={17} /></span><p className="mt-5 truncate text-xs text-[var(--muted)]">{label}</p><p className="mt-1 truncate text-lg font-semibold tabular-nums" title={value}>{value}</p></Card>
}

import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Bug, Gauge, RefreshCw, XCircle } from 'lucide-react'
import { Button, Card, Empty, Table, Tag, type TableColumn } from '@acme/components'
import { api } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'
import { compareText } from '../../lib/sort'
import { NodeDebugModal } from './NodeDebugModal'

interface NodeSummary {
  name: string
  type: string
}

interface LatencyState {
  loading?: boolean
  value?: number
  error?: string
}

export function NodeTestPanel() {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const [latencies, setLatencies] = useState<Record<string, LatencyState>>({})
  const [debugNode, setDebugNode] = useState<string>()
  const nodes = useQuery({
    queryKey: ['runtime', 'nodes'],
    queryFn: () => api<NodeSummary[]>(session!, '/runtime/nodes'),
    retry: false,
  })

  const testLatency = async (name: string) => {
    setLatencies((current) => ({ ...current, [name]: { loading: true } }))
    try {
      const result = await api<{ delay: number }>(session!, '/runtime/nodes/delay', {
        method: 'POST',
        body: JSON.stringify({ name }),
      })
      setLatencies((current) => ({ ...current, [name]: { value: result.delay } }))
    } catch (error) {
      setLatencies((current) => ({
        ...current,
        [name]: { error: error instanceof Error ? error.message : t('operationFailed') },
      }))
    }
  }

  const columns: Array<TableColumn<NodeSummary>> = [
    {
      title: locale === 'zh-CN' ? '节点' : 'Node',
      dataIndex: 'name',
      minWidth: 260,
      sorter: (left, right) => compareText(left.name, right.name),
      render: (value) => <span className="font-medium">{value}</span>,
    },
    {
      title: t('type'),
      dataIndex: 'type',
      width: 160,
      sorter: (left, right) => compareText(left.type, right.type),
      render: (value) => <Tag bordered={false}>{value || '-'}</Tag>,
    },
    {
      title: locale === 'zh-CN' ? '操作' : 'Actions',
      key: 'actions',
      width: 190,
      align: 'right',
      render: (_value, record) => {
        const latency = latencies[record.name] || {}
        return <div className="flex items-center justify-end gap-2">
          <LatencyButton state={latency} onClick={() => testLatency(record.name)} locale={locale} privateAccess={record.type.toLowerCase() === 'wireguard'} />
          <Button
            size="small"
            shape="circle"
            icon={<Bug />}
            aria-label={`${locale === 'zh-CN' ? '调试' : 'Debug'} ${record.name}`}
            title={locale === 'zh-CN' ? '完整流量调试' : 'Full traffic diagnostics'}
            onClick={() => setDebugNode(record.name)}
          />
        </div>
      },
    },
  ]

  return <div className="mt-5 space-y-4">
    <div className="flex items-start justify-between gap-4">
      <p className="text-sm text-[var(--muted)]">
        {locale === 'zh-CN' ? '延迟测试用于快速探活；调试会通过所选节点执行独立的 DNS 与 HTTP 流量测试。' : 'Latency is a quick liveness check. Debug runs isolated DNS and HTTP traffic through the selected node.'}
      </p>
      <Button size="small" icon={<RefreshCw />} loading={nodes.isFetching} onClick={() => nodes.refetch()}>{t('refresh')}</Button>
    </div>
    <Card className="!rounded-lg" bodyStyle={{ padding: 0 }}>
      <Table<NodeSummary>
        rowKey="name"
        size="middle"
        pagination={false}
        columns={columns}
        dataSource={nodes.data || []}
        locale={{
          emptyText: nodes.isError
            ? <span className="text-red-600 dark:text-red-400">{nodes.error instanceof Error ? nodes.error.message : t('operationFailed')}</span>
            : <Empty description={nodes.isFetching ? `${t('loading')}...` : t('noData')} />,
        }}
      />
    </Card>
    <NodeDebugModal key={debugNode} node={debugNode} open={Boolean(debugNode)} onClose={() => setDebugNode(undefined)} />
  </div>
}

function LatencyButton({ state, locale, privateAccess, onClick }: { state: LatencyState; locale: 'zh-CN' | 'en'; privateAccess: boolean; onClick: () => void }) {
  if (state.error) {
    return <Button
      size="small"
      danger
      icon={<XCircle />}
      aria-label={`${locale === 'zh-CN' ? '延迟测试失败' : 'Latency failed'}: ${state.error}`}
      title={state.error}
      onClick={onClick}
    >{locale === 'zh-CN' ? '失败' : 'Failed'}</Button>
  }
  const tone = state.value === undefined
    ? ''
    : state.value < 200
      ? '!border-emerald-500/40 !text-emerald-600 dark:!text-emerald-400'
      : state.value < 500
        ? '!border-amber-500/50 !text-amber-700 dark:!text-amber-400'
        : '!border-red-500/40 !text-red-600 dark:!text-red-400'
  const title = privateAccess
    ? (locale === 'zh-CN' ? '私网连通延迟' : 'Private reachability latency')
    : (locale === 'zh-CN' ? '延迟测试' : 'Test latency')
  return <Button
    size="small"
    shape={state.value === undefined ? 'circle' : 'default'}
    className={tone}
    loading={state.loading}
    icon={<Gauge />}
    aria-label={title}
    title={title}
    onClick={onClick}
  >{state.value === undefined ? null : `${state.value} ms`}</Button>
}

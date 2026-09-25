import { Button, Input, Modal, Select, Spin, Table } from '@acme/components'
import { useState } from 'react'
import { subscriptionApi } from './api'
import type { NodeTraceResult, PreviewNode, Target } from './diagnostic-types'
import type { Subscription } from './types'

interface Props {
  subscription: Subscription
  targets: Target[]
  onClose: () => void
}

export function SubscriptionPreview({ subscription, targets, onClose }: Props) {
  const [format, setFormat] = useState(targets[0]?.format ?? '')
  const [nodes, setNodes] = useState<PreviewNode[] | null>(null)
  const [search, setSearch] = useState('')
  const [detail, setDetail] = useState<PreviewNode | null>(null)
  const [trace, setTrace] = useState<NodeTraceResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [traceLoading, setTraceLoading] = useState(false)
  const [error, setError] = useState('')
  const target = targets.find((item) => item.format === format)

  const preview = async () => {
    if (!target) return
    setLoading(true)
    setError('')
    setNodes(null)
    setDetail(null)
    setTrace(null)
    try {
      const result = await subscriptionApi.preview(subscription.id, target)
      setNodes(result.nodes)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }
  const traceNode = async (node: PreviewNode) => {
    if (!target) return
    setTraceLoading(true)
    setError('')
    try {
      setTrace(await subscriptionApi.trace(subscription.id, target, node.name))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setTraceLoading(false)
    }
  }
  const filtered = nodes?.filter((node) => `${node.name} ${node.type} ${node.server} ${node.sourceUrl}`.toLowerCase().includes(search.toLowerCase())) ?? []

  return <Modal open title={`节点预览 · ${subscription.remark || '未命名配置集'}`} footer={null} onCancel={onClose} size="almost-full">
    <div className="space-y-4">
      <div className="flex flex-wrap items-end gap-2">
        <label className="min-w-44 flex-1 space-y-1 text-sm">输出格式
          <Select value={format} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(next) => { setFormat(String(next)); setNodes(null); setDetail(null); setTrace(null) }} className="w-full" />
        </label>
        <Button variant="primary" onClick={() => void preview()} loading={loading} disabled={!target}>生成预览</Button>
      </div>
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin /> : null}
      {nodes ? <>
        <div className="flex items-center justify-between gap-3"><p className="text-sm">{nodes.length} 个节点</p><Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索名称、协议、地址或来源" className="max-w-sm" /></div>
        <Table<PreviewNode> rowKey={(_, index) => String(index)} dataSource={filtered} pagination={false} scroll={{ x: 750 }} columns={[
          { title: '名称', dataIndex: 'name' }, { title: '协议', dataIndex: 'type', width: 110 },
          { title: '服务器', render: (_, node) => `${node.server}:${node.port}` },
          { title: '来源', render: (_, node) => node.sourceUrl || `来源 ${node.sourceIndex}` },
          { title: '状态', render: (_, node) => node.filtered ? `已过滤${node.filteredBy ? `: ${node.filteredBy}` : ''}` : '保留' },
          { title: '操作', render: (_, node) => <Button size="small" onClick={() => { setDetail(node); setTrace(null) }}>详情</Button> },
        ]} />
      </> : null}
      {detail ? <section className="rounded-lg border border-[var(--border)] p-3 space-y-3">
        <div className="flex items-center justify-between gap-2"><h3 className="font-medium">{detail.name}</h3><Button size="small" loading={traceLoading} onClick={() => void traceNode(detail)}>追踪节点</Button></div>
        <p className="text-xs text-[var(--muted)]">{detail.type} · {detail.server}:{detail.port} · {detail.sourceUrl || `来源 ${detail.sourceIndex}`}</p>
        <pre className="max-h-60 overflow-auto rounded bg-[var(--surface)] p-3 text-xs">{JSON.stringify(detail.raw, null, 2)}</pre>
        {trace ? <div className="space-y-2"><h4 className="text-sm font-medium">{trace.nodeName} 的处理步骤</h4>{trace.steps.map((step, index) => <div key={index} className="rounded border border-[var(--border)] p-2"><strong className="text-xs">{step.type}</strong><pre className="mt-1 max-h-40 overflow-auto text-xs">{JSON.stringify(step.data, null, 2)}</pre></div>)}</div> : null}
      </section> : null}
    </div>
  </Modal>
}

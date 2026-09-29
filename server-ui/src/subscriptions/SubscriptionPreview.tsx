import { Button, Collapse, Input, Modal, Select, Spin, Table } from '@acme/components'
import { useCallback, useEffect, useRef, useState } from 'react'
import { subscriptionApi } from './api'
import type { NodeTraceResult, PreviewNode, Target } from './diagnostic-types'
import type { Subscription } from './types'
import { useI18n } from '../i18n/provider'
import { DiagnosticValue } from './DiagnosticValue'
import { TraceSteps } from './DiagnosticTrace'

interface Props {
  subscription: Subscription
  targets: Target[]
  onClose: () => void
}

export function SubscriptionPreview({ subscription, targets, onClose }: Props) {
  const { t, number } = useI18n()
  const [format, setFormat] = useState(targets.find((item) => item.format === 'clash-meta')?.format ?? targets[0]?.format ?? '')
  const [nodes, setNodes] = useState<PreviewNode[] | null>(null)
  const [search, setSearch] = useState('')
  const [detail, setDetail] = useState<PreviewNode | null>(null)
  const [trace, setTrace] = useState<NodeTraceResult | null>(null)
  const [loading, setLoading] = useState(targets.length > 0)
  const [traceLoading, setTraceLoading] = useState(false)
  const [error, setError] = useState('')
  const requestId = useRef(0)
  const traceRequestId = useRef(0)
  const target = targets.find((item) => item.format === format)

  const preview = useCallback(async (selected: Target) => {
    const currentRequest = ++requestId.current
    try {
      const result = await subscriptionApi.preview(subscription.id, selected)
      if (requestId.current === currentRequest) setNodes(result.nodes)
    } catch (reason) {
      if (requestId.current === currentRequest) setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      if (requestId.current === currentRequest) setLoading(false)
    }
  }, [subscription.id])
  const preparePreview = () => {
    setLoading(true)
    setError('')
    setNodes(null)
    setDetail(null)
    setTrace(null)
    traceRequestId.current += 1
    setTraceLoading(false)
  }
  useEffect(() => {
    if (!target) return
    const currentRequest = ++requestId.current
    void subscriptionApi.preview(subscription.id, target).then((result) => {
      if (requestId.current === currentRequest) setNodes(result.nodes)
    }).catch((reason) => {
      if (requestId.current === currentRequest) setError(reason instanceof Error ? reason.message : String(reason))
    }).finally(() => {
      if (requestId.current === currentRequest) setLoading(false)
    })
    return () => { requestId.current += 1 }
  }, [subscription.id, target])
  const traceNode = async (node: PreviewNode) => {
    if (!target) return
    const currentTrace = ++traceRequestId.current
    const currentPreview = requestId.current
    setTraceLoading(true)
    setError('')
    try {
      const result = await subscriptionApi.trace(subscription.id, target, node.name)
      if (traceRequestId.current === currentTrace && requestId.current === currentPreview) setTrace(result)
    } catch (reason) {
      if (traceRequestId.current === currentTrace && requestId.current === currentPreview) setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      if (traceRequestId.current === currentTrace && requestId.current === currentPreview) setTraceLoading(false)
    }
  }
  const filtered = nodes?.filter((node) => `${node.name} ${node.type} ${node.server} ${node.sourceUrl}`.toLowerCase().includes(search.toLowerCase())) ?? []
  const activeCount = nodes?.filter((node) => !node.filtered).length ?? 0
  const filteredCount = (nodes?.length ?? 0) - activeCount
  const typeCounts = Object.entries((nodes ?? []).filter((node) => !node.filtered).reduce<Record<string, number>>((counts, node) => ({ ...counts, [node.type]: (counts[node.type] ?? 0) + 1 }), {}))

  return <Modal open title={t('preview.title', { name: subscription.remark || t('configs.unnamed') })} footer={null} onCancel={onClose} size="almost-full">
    <div className="space-y-4">
      <div className="flex min-w-0 flex-wrap items-center gap-3">
        {nodes ? <p className="min-w-0 flex-1 basis-64 break-words text-sm">{t('preview.summary', { total: number(nodes.length), active: number(activeCount), filtered: number(filteredCount) })}{typeCounts.length ? ` · ${typeCounts.map(([type, count]) => `${type} ${number(count)}`).join(' / ')}` : ''}</p> : <div className="min-w-0 flex-1 basis-64" />}
        <div className="flex w-full min-w-0 flex-wrap items-center justify-end gap-2 sm:w-auto">
          {nodes ? <Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('preview.search')} className="w-full min-w-0 sm:w-60" /> : null}
          <label className="inline-flex min-w-0 items-center gap-2 text-sm"><span className="shrink-0 whitespace-nowrap">{t('preview.target')}</span>
            <Select value={format} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(next) => { requestId.current += 1; preparePreview(); setFormat(String(next)) }} className="w-36 shrink-0" />
          </label>
          <Button variant="primary" onClick={() => { if (target) { preparePreview(); void preview(target) } }} loading={loading} disabled={!target}>{t('preview.generate')}</Button>
        </div>
      </div>
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin /> : null}
      {nodes ? <>
        <Table<PreviewNode> rowKey={(_, index) => String(index)} dataSource={filtered} pagination={false} scroll={{ x: 750 }} columns={[
          { title: t('common.name'), dataIndex: 'name' }, { title: t('common.protocol'), dataIndex: 'type', width: 110 },
          { title: t('common.server'), render: (_, node) => `${node.server}:${node.port}` },
          { title: t('preview.transportTls'), render: (_, node) => `${typeof node.raw.network === 'string' ? node.raw.network.toUpperCase() : '—'}${node.raw.tls === true ? ' · TLS' : ''}` },
          { title: t('preview.credential'), render: (_, node) => { const credential = [node.raw.uuid, node.raw.password, node.raw['auth-str']].find((value): value is string => typeof value === 'string'); return credential ? credential.length > 16 ? `${credential.slice(0, 8)}…${credential.slice(-4)}` : '••••' : '—' } },
          { title: t('common.source'), render: (_, node) => node.sourceUrl || `${t('common.source')} ${number(node.sourceIndex)}` },
          { title: t('common.status'), render: (_, node) => node.filtered ? t('preview.filtered', { rule: node.filteredBy ?? '' }) : t('preview.kept') },
          { title: t('common.actions'), render: (_, node) => <Button size="small" onClick={() => { traceRequestId.current += 1; setTraceLoading(false); setDetail(node); setTrace(null) }}>{t('preview.detail')}</Button> },
        ]} locale={{ emptyText: t('common.noData') }} />
      </> : null}
      {detail ? <section className="rounded-lg border border-[var(--border)] p-3 space-y-3">
        <div className="flex items-center justify-between gap-2"><h3 className="font-medium">{detail.name}</h3><Button size="small" loading={traceLoading} onClick={() => void traceNode(detail)}>{t('preview.trace')}</Button></div>
        <p className="text-xs text-[var(--muted)]">{detail.type} · {detail.server}:{detail.port} · {detail.sourceUrl || `${t('common.source')} ${number(detail.sourceIndex)}`}</p>
        <Collapse size="small" items={[{ key: 'raw', label: t('preview.raw'), children: <DiagnosticValue value={detail.raw} /> }]} />
        {trace ? <div className="space-y-2"><h4 className="text-sm font-medium">{t('preview.traceSteps', { name: trace.nodeName })}</h4><TraceSteps trace={trace} /></div> : null}
      </section> : null}
    </div>
  </Modal>
}

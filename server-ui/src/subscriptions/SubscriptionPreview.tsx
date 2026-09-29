import { Button, Input, Modal, Select, Spin, Table, Tag, Tooltip } from '@acme/components'
import { useCallback, useEffect, useRef, useState } from 'react'
import { subscriptionApi } from './api'
import type { NodeTraceResult, PreviewNode, Target } from './diagnostic-types'
import type { Subscription } from './types'
import { useI18n } from '../i18n/provider'
import { PreviewNodeDetails } from './PreviewNodeDetails'

interface Props {
  subscription: Subscription
  targets: Target[]
  onClose: () => void
}

type IndexedPreviewNode = PreviewNode & { previewIndex: number }
const typeColors: Record<string, string> = { vmess: 'blue', vless: 'purple', ss: 'green', trojan: 'orange', hysteria2: 'magenta', hysteria: 'red', tuic: 'cyan', socks5: 'default', http: 'default' }

export function SubscriptionPreview({ subscription, targets, onClose }: Props) {
  const { t, number } = useI18n()
  const [format, setFormat] = useState(targets.find((item) => item.format === 'clash-meta')?.format ?? targets[0]?.format ?? '')
  const [nodes, setNodes] = useState<PreviewNode[] | null>(null)
  const [search, setSearch] = useState('')
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null)
  const [trace, setTrace] = useState<{ index: number; result: NodeTraceResult } | null>(null)
  const [loading, setLoading] = useState(targets.length > 0)
  const [traceLoading, setTraceLoading] = useState(false)
  const [error, setError] = useState('')
  const requestId = useRef(0)
  const traceRequestId = useRef(0)
  const target = targets.find((item) => item.format === format)

  const expandNode = (index: number | null) => {
    traceRequestId.current += 1
    setTraceLoading(false)
    setTrace(null)
    setExpandedIndex(index)
  }

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
    expandNode(null)
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
  const traceNode = async (node: IndexedPreviewNode) => {
    if (!target) return
    const currentTrace = ++traceRequestId.current
    const currentPreview = requestId.current
    setTraceLoading(true)
    setError('')
    try {
      const result = await subscriptionApi.trace(subscription.id, target, node.name)
      if (traceRequestId.current === currentTrace && requestId.current === currentPreview) setTrace({ index: node.previewIndex, result })
    } catch (reason) {
      if (traceRequestId.current === currentTrace && requestId.current === currentPreview) setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      if (traceRequestId.current === currentTrace && requestId.current === currentPreview) setTraceLoading(false)
    }
  }
  const filtered = nodes?.map((node, previewIndex) => ({ ...node, previewIndex })).filter((node) => `${node.name} ${node.type} ${node.server} ${node.sourceUrl}`.toLowerCase().includes(search.toLowerCase())) ?? []
  const activeCount = nodes?.filter((node) => !node.filtered).length ?? 0
  const filteredCount = (nodes?.length ?? 0) - activeCount
  const typeCounts = Object.entries((nodes ?? []).filter((node) => !node.filtered).reduce<Record<string, number>>((counts, node) => ({ ...counts, [node.type]: (counts[node.type] ?? 0) + 1 }), {}))

  return <Modal open title={t('preview.title', { name: subscription.remark || t('configs.unnamed') })} footer={null} onCancel={onClose} size="almost-full">
    <div className="space-y-4">
      <div className="flex min-w-0 flex-wrap items-center gap-3">
        {nodes ? <p className="min-w-0 flex-1 basis-64 break-words text-sm">{t('preview.summary', { total: number(nodes.length), active: number(activeCount), filtered: number(filteredCount) })}{typeCounts.length ? ` · ${typeCounts.map(([type, count]) => `${type} ${number(count)}`).join(' / ')}` : ''}</p> : <div className="min-w-0 flex-1 basis-64" />}
        <div className="flex w-full min-w-0 flex-wrap items-center justify-end gap-2 sm:w-auto">
          {nodes ? <Input value={search} onChange={(event) => { setSearch(event.target.value); expandNode(null) }} placeholder={t('preview.search')} className="w-full min-w-0 sm:w-60" /> : null}
          <Select value={format} addonBefore={t('preview.target')} aria-label={t('preview.target')} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(next) => { requestId.current += 1; preparePreview(); setFormat(String(next)) }} className="w-56 shrink-0 sm:w-60" />
          <Button variant="primary" onClick={() => { if (target) { preparePreview(); void preview(target) } }} loading={loading} disabled={!target}>{t('preview.generate')}</Button>
        </div>
      </div>
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin /> : null}
      {nodes ? <>
        <div className="hidden md:block"><Table<IndexedPreviewNode> rowKey="previewIndex" dataSource={filtered} pagination={false} scroll={{ x: 1250 }} onRow={(node) => ({
          className: 'cursor-pointer',
          tabIndex: 0,
          'aria-expanded': expandedIndex === node.previewIndex,
          onClick: (event) => { if (!(event.target as Element).closest('button')) expandNode(expandedIndex === node.previewIndex ? null : node.previewIndex) },
          onKeyDown: (event) => { if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) { event.preventDefault(); expandNode(expandedIndex === node.previewIndex ? null : node.previewIndex) } },
        })} expandable={{
          expandedRowKeys: expandedIndex === null ? [] : [String(expandedIndex)],
          onExpand: (expanded, node) => expandNode(expanded ? node.previewIndex : null),
          expandedRowRender: (node) => <PreviewNodeDetails node={node} trace={trace?.index === node.previewIndex ? trace.result : null} traceLoading={traceLoading && expandedIndex === node.previewIndex} onTrace={() => void traceNode(node)} />,
        }} columns={[
          { title: t('common.name'), width: 250, minWidth: 220, ellipsis: true, render: (_, node) => <Tooltip title={node.name}><span className="block truncate">{node.name}</span></Tooltip> },
          { title: t('common.protocol'), width: 110, minWidth: 110, render: (_, node) => <Tag color={node.filtered ? 'default' : typeColors[node.type] ?? 'default'}>{node.type.toUpperCase()}</Tag> },
          { title: t('common.server'), width: 200, minWidth: 200, ellipsis: true, render: (_, node) => <Tooltip title={`${node.server}:${node.port}`}><span className="block truncate font-mono text-xs">{node.server}:{node.port}</span></Tooltip> },
          { title: t('preview.transportTls'), width: 130, minWidth: 130, render: (_, node) => <span className="whitespace-nowrap">{typeof node.raw.network === 'string' ? node.raw.network.toUpperCase() : '—'}{node.raw.tls === true ? ' · TLS' : ''}</span> },
          { title: t('preview.credential'), width: 175, minWidth: 175, render: (_, node) => { const credential = [node.raw.uuid, node.raw.password, node.raw['auth-str']].find((value): value is string => typeof value === 'string'); return <span className="block truncate whitespace-nowrap font-mono text-xs">{credential ? credential.length > 16 ? `${credential.slice(0, 8)}…${credential.slice(-4)}` : '••••' : '—'}</span> } },
          { title: t('common.source'), width: 90, minWidth: 90, render: (_, node) => <Tooltip title={node.sourceUrl || `${t('common.source')} #${number(node.sourceIndex)}`}><span className="whitespace-nowrap">#{number(node.sourceIndex)}</span></Tooltip> },
          { title: t('common.status'), width: 165, minWidth: 165, ellipsis: true, render: (_, node) => { const status = node.filtered ? t('preview.filtered', { rule: node.filteredBy ?? '' }) : t('preview.kept'); return <Tooltip title={status}><span className="block truncate">{status}</span></Tooltip> } },
          { title: t('common.actions'), width: 78, minWidth: 78, render: (_, node) => <Button size="small" className="whitespace-nowrap" aria-expanded={expandedIndex === node.previewIndex} onClick={() => expandNode(expandedIndex === node.previewIndex ? null : node.previewIndex)}>{t('preview.detail')}</Button> },
        ]} locale={{ emptyText: t('common.noData') }} /></div>
        <div className="space-y-2 md:hidden">{filtered.map((node) => <article key={node.previewIndex} className="min-w-0 rounded-lg border border-[var(--border)]">
          <div className="flex min-w-0 items-start justify-between gap-2 p-3"><div className="min-w-0"><strong className="block break-words text-sm">{node.name}</strong><p className="mt-1 break-words text-xs text-[var(--muted)]">{node.server}:{node.port}</p></div><Tag color={node.filtered ? 'default' : typeColors[node.type] ?? 'default'}>{node.type.toUpperCase()}</Tag></div>
          <div className="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--border)] px-3 py-2 text-xs"><div className="min-w-0"><p className="break-words text-[var(--muted)]">{node.sourceUrl || `${t('common.source')} ${number(node.sourceIndex)}`}</p><p className={node.filtered ? 'mt-1 text-orange-600' : 'mt-1 text-green-600'}>{node.filtered ? t('preview.filtered', { rule: node.filteredBy ?? '' }) : t('preview.kept')}</p></div><Button size="small" variant="text" className="whitespace-nowrap" aria-expanded={expandedIndex === node.previewIndex} onClick={() => expandNode(expandedIndex === node.previewIndex ? null : node.previewIndex)}>{t('preview.detail')}</Button></div>
          {expandedIndex === node.previewIndex ? <div className="border-t border-[var(--border)] p-3"><PreviewNodeDetails node={node} trace={trace?.index === node.previewIndex ? trace.result : null} traceLoading={traceLoading} onTrace={() => void traceNode(node)} mobile /></div> : null}
        </article>)}</div>
      </> : null}
    </div>
  </Modal>
}

import { Button, Input, Modal, Select, Spin } from '@acme/components'
import { useState } from 'react'
import { subscriptionApi } from './api'
import type { DraftDebugResult, Target } from './diagnostic-types'
import type { SubscriptionDraft } from './types'

export function SubscriptionDebug({ draft, targets, subscriptionId, onClose }: { draft: SubscriptionDraft; targets: Target[]; subscriptionId?: string; onClose: () => void }) {
  const [format, setFormat] = useState(targets[0]?.format ?? '')
  const [result, setResult] = useState<DraftDebugResult | null>(null)
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const target = targets.find((item) => item.format === format)
  const run = async () => {
    if (!target) return
    setLoading(true)
    setError('')
    setResult(null)
    try {
      setResult(await subscriptionApi.debug(draft, target, subscriptionId))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }
  const query = search.trim().toLowerCase()
  const lines = result?.content?.split('\n') ?? []
  const visibleLines = query ? lines.filter((line) => line.toLowerCase().includes(query)) : lines
  const diagnostics = result?.diagnostics.filter((item) => !query || `${item.level} ${item.sourceId ?? ''} ${item.message}`.toLowerCase().includes(query)) ?? []
  const stages = result?.stages.filter((item) => !query || `${item.type} ${item.status} ${item.message ?? ''}`.toLowerCase().includes(query)) ?? []
  return <Modal open title="调试当前草稿" size="almost-full" footer={null} onCancel={onClose}>
    <div className="space-y-4">
      <p className="text-sm text-[var(--muted)]">调试使用当前未保存的表单内容，不会保存配置或更改公开链接。</p>
      <div className="flex flex-wrap items-end gap-2">
        <label className="min-w-48 flex-1 space-y-1 text-sm">输出格式
          <Select value={format} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(next) => setFormat(String(next))} className="w-full" />
        </label>
        <Button variant="primary" loading={loading} disabled={!target} onClick={() => void run()}>运行调试</Button>
      </div>
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin /> : null}
      {result ? <>
        <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-sm">{result.ok ? '调试完成' : '调试失败'}{result.format ? ` · ${result.format}` : ''}{result.nodeCount !== undefined ? ` · ${result.nodeCount} 个节点` : ''}</p><Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索步骤、诊断或输出" className="max-w-xs" /></div>
        <section className="space-y-2"><h3 className="text-sm font-semibold">处理阶段</h3>
          {stages.map((stage, index) => <div key={index} className="rounded border border-[var(--border)] p-2 text-sm"><strong>{stage.type}</strong> · {stage.status}{stage.sourceId ? ` · ${stage.sourceId}` : ''}{stage.cacheState ? ` · ${stage.cacheState}` : stage.cached !== undefined ? ` · ${stage.cached ? '缓存' : '实时'}` : ''}{stage.message ? <p className="text-xs text-[var(--muted)]">{stage.message}</p> : null}</div>)}
        </section>
        <section className="space-y-2"><h3 className="text-sm font-semibold">诊断</h3>
          {diagnostics.length ? diagnostics.map((item, index) => <p key={index} className="rounded border border-[var(--border)] p-2 text-xs">{item.level}{item.sourceId ? ` · ${item.sourceId}` : ''}: {item.message}</p>) : <p className="text-xs text-[var(--muted)]">无匹配诊断</p>}
        </section>
        {result.content !== undefined ? <section className="space-y-2"><h3 className="text-sm font-semibold">配置输出{query ? ` · ${visibleLines.length} 行匹配` : ''}</h3>
          <pre className="max-h-96 overflow-auto rounded bg-[var(--surface)] p-3 text-xs">{visibleLines.join('\n')}</pre>
        </section> : null}
        {result.fieldDiffs != null ? <details><summary className="cursor-pointer text-sm">字段差异</summary><pre className="max-h-60 overflow-auto text-xs">{JSON.stringify(result.fieldDiffs, null, 2)}</pre></details> : null}
      </> : null}
    </div>
  </Modal>
}

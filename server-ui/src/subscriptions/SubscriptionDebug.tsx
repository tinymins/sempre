import { Button, Collapse, Input, Modal, Select, Spin } from '@acme/components'
import { useEffect, useRef, useState } from 'react'
import { subscriptionApi } from './api'
import type { DraftDebugResult, Target } from './diagnostic-types'
import type { Subscription, SubscriptionDraft } from './types'
import { useI18n } from '../i18n/provider'

export function SubscriptionDebug({ draft, savedSubscription, targets, subscriptionId, initialTarget, onClose }: { draft?: SubscriptionDraft; savedSubscription?: Subscription; targets: Target[]; subscriptionId?: string; initialTarget?: Target; onClose: () => void }) {
  const { t, number } = useI18n()
  const [format, setFormat] = useState(initialTarget?.format ?? targets[0]?.format ?? '')
  const [result, setResult] = useState<DraftDebugResult | null>(null)
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(Boolean(savedSubscription && initialTarget))
  const [error, setError] = useState('')
  const requestId = useRef(0)
  const target = targets.find((item) => item.format === format)
  useEffect(() => {
    if (!savedSubscription || !initialTarget) return
    const currentRequest = ++requestId.current
    void subscriptionApi.debugSaved(savedSubscription.id, initialTarget).then((next) => {
      if (requestId.current === currentRequest) setResult(next)
    }).catch((reason) => {
      if (requestId.current === currentRequest) setError(reason instanceof Error ? reason.message : String(reason))
    }).finally(() => { if (requestId.current === currentRequest) setLoading(false) })
    return () => { requestId.current += 1 }
  }, [savedSubscription, initialTarget])
  const run = async () => {
    if (!target) return
    const currentRequest = ++requestId.current
    setLoading(true)
    setError('')
    setResult(null)
    try {
      const next = savedSubscription
        ? await subscriptionApi.debugSaved(savedSubscription.id, target)
        : draft ? await subscriptionApi.debug(draft, target, subscriptionId) : null
      if (requestId.current === currentRequest) setResult(next)
    } catch (reason) {
      if (requestId.current === currentRequest) setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      if (requestId.current === currentRequest) setLoading(false)
    }
  }
  const query = search.trim().toLowerCase()
  const lines = result?.content?.split('\n') ?? []
  const visibleLines = query ? lines.filter((line) => line.toLowerCase().includes(query)) : lines
  const diagnostics = result?.diagnostics.filter((item) => !query || `${item.level} ${item.sourceId ?? ''} ${item.message}`.toLowerCase().includes(query)) ?? []
  const stages = result?.stages.filter((item) => !query || `${item.type} ${item.status} ${item.message ?? ''}`.toLowerCase().includes(query)) ?? []
  const errors = result?.diagnostics.filter((item) => item.level === 'error') ?? []
  return <Modal open title={savedSubscription ? t('debug.savedTitle', { name: savedSubscription.remark || t('configs.unnamed') }) : t('debug.draftTitle')} size="almost-full" footer={null} onCancel={onClose}>
    <div className="space-y-4">
      <p className="text-sm text-[var(--muted)]">{savedSubscription ? t('debug.savedHint') : t('debug.draftHint')}</p>
      <div className="flex flex-wrap items-end gap-2">
        <label className="min-w-48 flex-1 space-y-1 text-sm">{t('preview.target')}
          <Select value={format} disabled={loading} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(next) => { requestId.current += 1; setFormat(String(next)); setResult(null); setError(''); setLoading(false) }} className="w-full" />
        </label>
        <Button variant="primary" loading={loading} disabled={!target} onClick={() => void run()}>{t('debug.run')}</Button>
      </div>
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin /> : null}
      {result ? <>
        <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-sm">{t('debug.summary', { status: result.ok ? t('common.completed') : t('common.failed'), format: result.format ?? '', count: number(result.nodeCount ?? 0) })}</p><Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('debug.search')} className="max-w-xs" /></div>
        {errors.length ? <p role="alert" className="rounded border border-red-500 p-2 text-sm text-red-700">{errors.map((item) => item.message).join(' · ')}</p> : null}
        <section className="space-y-2"><h3 className="text-sm font-semibold">{t('debug.stages')}</h3>
          {stages.map((stage, index) => <div key={index} className="rounded border border-[var(--border)] p-2 text-sm"><strong>{stage.type}</strong> · {stage.status}{stage.sourceId ? ` · ${stage.sourceId}` : ''}{stage.cacheState ? ` · ${stage.cacheState}` : stage.cached !== undefined ? ` · ${stage.cached ? t('debug.cached') : t('debug.live')}` : ''}{stage.message ? <p className="text-xs text-[var(--muted)]">{stage.message}</p> : null}</div>)}
        </section>
        <section className="space-y-2"><h3 className="text-sm font-semibold">{t('debug.diagnostics')}</h3>
          {diagnostics.length ? diagnostics.map((item, index) => <p key={index} className="rounded border border-[var(--border)] p-2 text-xs">{item.level}{item.sourceId ? ` · ${item.sourceId}` : ''}: {item.message}</p>) : <p className="text-xs text-[var(--muted)]">{t('debug.noDiagnostics')}</p>}
        </section>
        {result.content !== undefined ? <section className="space-y-2"><h3 className="text-sm font-semibold">{query ? t('debug.outputMatches', { count: number(visibleLines.length) }) : t('debug.output')}</h3>
          <pre className="max-h-96 overflow-auto rounded bg-[var(--surface)] p-3 text-xs">{visibleLines.join('\n')}</pre>
        </section> : null}
        <Collapse size="small" items={[
          ...(result.decoded != null ? [{ key: 'decoded', label: t('debug.decoded'), children: <pre className="max-h-60 overflow-auto text-xs">{JSON.stringify(result.decoded, null, 2)}</pre> }] : []),
          ...(result.nodeOrigins != null ? [{ key: 'origins', label: t('debug.origins'), children: <pre className="max-h-60 overflow-auto text-xs">{JSON.stringify(result.nodeOrigins, null, 2)}</pre> }] : []),
          ...(result.fieldDiffs != null ? [{ key: 'diffs', label: t('debug.fieldDiffs'), children: <pre className="max-h-60 overflow-auto text-xs">{JSON.stringify(result.fieldDiffs, null, 2)}</pre> }] : []),
        ]} />
      </> : null}
    </div>
  </Modal>
}

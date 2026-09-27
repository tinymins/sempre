import { Button, Collapse, Input, Modal, Select, Spin } from '@acme/components'
import { useEffect, useRef, useState } from 'react'
import { subscriptionApi } from './api'
import type { DebugStage, DraftDebugResult, Target } from './diagnostic-types'
import type { Subscription, SubscriptionDraft } from './types'
import { useI18n } from '../i18n/provider'

export function SubscriptionDebug({ draft, savedSubscription, targets, subscriptionId, initialTarget, onClose }: { draft?: SubscriptionDraft; savedSubscription?: Subscription; targets: Target[]; subscriptionId?: string; initialTarget?: Target; onClose: () => void }) {
  const { t, number } = useI18n()
  const [format, setFormat] = useState(initialTarget?.format ?? targets[0]?.format ?? '')
  const [result, setResult] = useState<DraftDebugResult | null>(null)
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(Boolean(savedSubscription && initialTarget))
  const [error, setError] = useState('')
  const [progress, setProgress] = useState<DebugStage[]>([])
  const requestId = useRef(0)
  const controller = useRef<AbortController | null>(null)
  const target = targets.find((item) => item.format === format)
  const savedId = savedSubscription?.id
  useEffect(() => () => { requestId.current += 1; controller.current?.abort() }, [])
  const stop = () => {
    requestId.current += 1
    controller.current?.abort()
    controller.current = null
    setLoading(false)
  }
  const execute = async (selected: Target, currentRequest: number, signal: AbortSignal) => {
    try {
      const onStage = (stage: DebugStage) => {
        if (requestId.current === currentRequest && !signal.aborted) setProgress((items) => [...items, stage])
      }
      const next = savedSubscription
        ? await subscriptionApi.debugSaved(savedSubscription.id, selected, signal, onStage)
        : draft ? await subscriptionApi.debug(draft, selected, signal, onStage, subscriptionId) : null
      if (requestId.current === currentRequest && !signal.aborted) {
        setResult(next)
        setProgress(next?.stages ?? [])
      }
    } catch (reason) {
      if (requestId.current === currentRequest && !signal.aborted) setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      if (requestId.current === currentRequest && !signal.aborted) {
        controller.current = null
        setLoading(false)
      }
    }
  }
  useEffect(() => {
    if (!savedId || !initialTarget) return
    const currentRequest = ++requestId.current
    const nextController = new AbortController()
    controller.current = nextController
    void subscriptionApi.debugSaved(savedId, initialTarget, nextController.signal, (stage) => {
      if (requestId.current === currentRequest && !nextController.signal.aborted) setProgress((items) => [...items, stage])
    }).then((next) => {
      if (requestId.current === currentRequest && !nextController.signal.aborted) {
        setResult(next)
        setProgress(next.stages)
      }
    }).catch((reason) => {
      if (requestId.current === currentRequest && !nextController.signal.aborted) setError(reason instanceof Error ? reason.message : String(reason))
    }).finally(() => {
      if (requestId.current === currentRequest && !nextController.signal.aborted) {
        controller.current = null
        setLoading(false)
      }
    })
    return () => {
      if (controller.current === nextController) {
        requestId.current += 1
        nextController.abort()
        controller.current = null
      }
    }
  }, [savedId, initialTarget])
  const run = () => {
    if (!target) return
    controller.current?.abort()
    const currentRequest = ++requestId.current
    const nextController = new AbortController()
    controller.current = nextController
    setLoading(true)
    setError('')
    setResult(null)
    setProgress([])
    void execute(target, currentRequest, nextController.signal)
  }
  const query = search.trim().toLowerCase()
  const lines = result?.content?.split('\n') ?? []
  const visibleLines = query ? lines.filter((line) => line.toLowerCase().includes(query)) : lines
  const diagnostics = result?.diagnostics.filter((item) => !query || `${item.level} ${item.sourceId ?? ''} ${item.message}`.toLowerCase().includes(query)) ?? []
  const stages = progress.filter((item) => !query || `${item.type} ${item.status} ${item.message ?? ''}`.toLowerCase().includes(query))
  const errors = result?.diagnostics.filter((item) => item.level === 'error') ?? []
  return <Modal open title={savedSubscription ? t('debug.savedTitle', { name: savedSubscription.remark || t('configs.unnamed') }) : t('debug.draftTitle')} size="almost-full" footer={null} onCancel={() => { stop(); onClose() }}>
    <div className="space-y-4">
      <p className="text-sm text-[var(--muted)]">{savedSubscription ? t('debug.savedHint') : t('debug.draftHint')}</p>
      <div className="flex flex-wrap items-end gap-2">
        <label className="min-w-48 flex-1 space-y-1 text-sm">{t('preview.target')}
          <Select value={format} disabled={loading} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(next) => { stop(); setFormat(String(next)); setResult(null); setProgress([]); setError('') }} className="w-full" />
        </label>
        <Button variant="primary" loading={loading} disabled={!target} onClick={run}>{t('debug.run')}</Button>
        {loading ? <Button onClick={stop}>{t('debug.cancel')}</Button> : null}
      </div>
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin /> : null}
      {stages.length ? <section className="space-y-2"><h3 className="text-sm font-semibold">{t('debug.stages')}</h3>
        {stages.map((stage, index) => <div key={index} className="rounded border border-[var(--border)] p-2 text-sm"><strong>{stage.type}</strong> · {stage.status}{stage.sourceId ? ` · ${stage.sourceId}` : ''}{stage.cacheState ? ` · ${stage.cacheState}` : stage.cached !== undefined ? ` · ${stage.cached ? t('debug.cached') : t('debug.live')}` : ''}{stage.message ? <p className="text-xs text-[var(--muted)]">{stage.message}</p> : null}</div>)}
      </section> : null}
      {result ? <>
        <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-sm">{t('debug.summary', { status: result.ok ? t('common.completed') : t('common.failed'), format: result.format ?? '', count: number(result.nodeCount ?? 0) })}</p><Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('debug.search')} className="max-w-xs" /></div>
        {errors.length ? <p role="alert" className="rounded border border-red-500 p-2 text-sm text-red-700">{errors.map((item) => item.message).join(' · ')}</p> : null}
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

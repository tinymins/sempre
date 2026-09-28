import { Button, Collapse, Input, Modal, Select, Spin, Tag } from '@acme/components'
import { useEffect, useRef, useState } from 'react'
import { subscriptionApi } from './api'
import type { DebugStage, DraftDebugResult, Target } from './diagnostic-types'
import type { Subscription, SubscriptionDraft } from './types'
import { useI18n } from '../i18n/provider'
import { DiagnosticTimeline } from './DiagnosticTimeline'
import { DiagnosticSearch } from './DiagnosticSearch'
import { DiagnosticTrace } from './DiagnosticTrace'
import { DiagnosticValue } from './DiagnosticValue'
import { diagnosticText } from './diagnostic-locale'
import { ServerCodeBlock } from '../ServerCodeBlock'

export function SubscriptionDebug({ draft, savedSubscription, targets, subscriptionId, initialTarget, onClose }: { draft?: SubscriptionDraft; savedSubscription?: Subscription; targets: Target[]; subscriptionId?: string; initialTarget?: Target; onClose: () => void }) {
  const { t, number, locale } = useI18n()
  const [format, setFormat] = useState(initialTarget?.format ?? targets[0]?.format ?? '')
  const [result, setResult] = useState<DraftDebugResult | null>(null)
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(Boolean(savedSubscription && initialTarget))
  const [error, setError] = useState('')
  const [progress, setProgress] = useState<DebugStage[]>([])
  const [searchOpen, setSearchOpen] = useState(false)
  const [traceOpen, setTraceOpen] = useState(false)
  const [traceId, setTraceId] = useState<string | undefined>()
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
    setSearchOpen(false)
    setTraceOpen(false)
    setTraceId(undefined)
    void execute(target, currentRequest, nextController.signal)
  }
  const query = search.trim().toLowerCase()
  const diagnostics = result?.diagnostics.filter((item) => !query || `${item.level} ${item.sourceId ?? ''} ${item.message}`.toLowerCase().includes(query)) ?? []
  const errors = result?.diagnostics.filter((item) => item.level === 'error') ?? []
  const openTrace = (id?: string) => { setTraceId(id); setTraceOpen(true) }
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
      {result ? <>
        <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-sm">{t('debug.summary', { status: result.ok ? t('common.completed') : t('common.failed'), format: result.format ?? '', count: number(result.nodeCount ?? 0) })}</p><Input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('debug.search')} className="max-w-xs" /></div>
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <Tag color={result.runtimeValidated ? 'green' : 'blue'}>{diagnosticText(locale, result.runtimeValidated ? 'runtime' : 'structural')}</Tag>
          <Tag>{diagnosticText(locale, 'elapsedMs')}: {number(result.elapsedMs)} ms</Tag>
          <Button size="small" onClick={() => setSearchOpen(true)}>{diagnosticText(locale, 'searchTitle')}</Button>
          <Button size="small" disabled={!result.nodeTraces?.length} onClick={() => openTrace()}>{diagnosticText(locale, 'trace')} · {number(result.nodeTraces?.length ?? 0)}</Button>
          {!result.nodeTraces?.length ? <span className="text-xs text-[var(--muted)]">{diagnosticText(locale, 'traceUnavailable')}</span> : null}
        </div>
      </> : null}
      {progress.length ? <section className="space-y-2"><h3 className="text-sm font-semibold">{t('debug.stages')}</h3>
        <DiagnosticTimeline stages={progress} search={search} traces={result?.nodeTraces} onTrace={result?.nodeTraces?.length ? (id) => openTrace(id) : undefined} />
      </section> : null}
      {result ? <>
        {errors.length ? <p role="alert" className="rounded border border-red-500 p-2 text-sm text-red-700">{errors.map((item) => item.message).join(' · ')}</p> : null}
        <section className="space-y-2"><h3 className="text-sm font-semibold">{t('debug.diagnostics')}</h3>
          {diagnostics.length ? diagnostics.map((item, index) => <p key={index} className="rounded border border-[var(--border)] p-2 text-xs">{item.level}{item.sourceId ? ` · ${item.sourceId}` : ''}: {item.message}</p>) : <p className="text-xs text-[var(--muted)]">{t('debug.noDiagnostics')}</p>}
        </section>
        {result.content !== undefined ? <section className="space-y-2"><h3 className="text-sm font-semibold">{t('debug.output')}</h3>
          <ServerCodeBlock value={result.content} language={result.format?.startsWith('sing-box') ? 'JSON' : 'YAML'} maxHeight={420} />
        </section> : null}
        <Collapse size="small" items={[
          ...(result.decoded != null ? [{ key: 'decoded', label: t('debug.decoded'), children: <DiagnosticValue value={result.decoded} /> }] : []),
          ...(result.nodeOrigins != null ? [{ key: 'origins', label: t('debug.origins'), children: <DiagnosticValue value={result.nodeOrigins} /> }] : []),
          ...(result.fieldDiffs != null ? [{ key: 'diffs', label: t('debug.fieldDiffs'), children: <DiagnosticValue value={result.fieldDiffs} /> }] : []),
        ]} />
        {searchOpen ? <DiagnosticSearch result={result} open onClose={() => setSearchOpen(false)} /> : null}
        {traceOpen ? <DiagnosticTrace traces={result.nodeTraces ?? []} selectedId={traceId} open onClose={() => setTraceOpen(false)} /> : null}
      </> : null}
    </div>
  </Modal>
}

import { Button, Collapse, Modal, Select, Spin, Table, Tag } from '@acme/components'
import { useEffect, useRef, useState } from 'react'
import { subscriptionApi } from './api'
import type { DebugStage, SourceDebugResult } from './diagnostic-types'
import type { SubscriptionSource } from './types'
import { useI18n } from '../i18n/provider'
import { DiagnosticTimeline } from './DiagnosticTimeline'
import { DiagnosticValue } from './DiagnosticValue'
import { diagnosticText } from './diagnostic-locale'

export function SourceDebug({ source, saved, onClose }: { source: SubscriptionSource; saved?: { id: string; index: number; source: SubscriptionSource }; onClose: () => void }) {
  const { t, number, locale } = useI18n()
  const [mode, setMode] = useState<'bypass-cache' | 'production'>(saved ? 'production' : 'bypass-cache')
  const [result, setResult] = useState<SourceDebugResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [progress, setProgress] = useState<DebugStage[]>([])
  const requestId = useRef(0)
  const controller = useRef<AbortController | null>(null)
  useEffect(() => () => { requestId.current += 1; controller.current?.abort() }, [])
  const stop = () => {
    requestId.current += 1
    controller.current?.abort()
    controller.current = null
    setLoading(false)
  }
  const run = async () => {
    controller.current?.abort()
    const currentRequest = ++requestId.current
    const nextController = new AbortController()
    controller.current = nextController
    setLoading(true)
    setError('')
    setResult(null)
    setProgress([])
    try {
      const next = await subscriptionApi.debugSource(source, mode, nextController.signal, (stage) => {
        if (requestId.current === currentRequest && !nextController.signal.aborted) setProgress((items) => [...items, stage])
      }, saved)
      if (requestId.current === currentRequest && !nextController.signal.aborted) {
        setResult(next)
        setProgress(next.stages)
      }
    } catch (reason) {
      if (requestId.current === currentRequest && !nextController.signal.aborted) setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      if (requestId.current === currentRequest && !nextController.signal.aborted) {
        controller.current = null
        setLoading(false)
      }
    }
  }
  const displayedSource = mode === 'production' && saved ? saved.source : source
  return <Modal open title={t('sourceDebug.title', { name: displayedSource.remark || displayedSource.url })} size="large" footer={null} onCancel={() => { stop(); onClose() }}>
    <div className="space-y-4">
      <p className="break-all text-xs text-[var(--muted)]">{displayedSource.url}</p>
      <p className="text-xs text-[var(--muted)]">{saved ? t('sourceDebug.savedHint') : t('sourceDebug.draftHint')}</p>
      <dl className="grid grid-cols-2 gap-2 rounded border border-[var(--border)] p-3 text-xs md:grid-cols-4">
        <div><dt className="text-[var(--muted)]">User-Agent</dt><dd className="break-all">{displayedSource.fetchUa || 'clash.meta'}</dd></div>
        <div><dt className="text-[var(--muted)]">{diagnosticText(locale, 'fetchMode')}</dt><dd>{displayedSource.fetchMode || 'auto'}</dd></div>
        <div><dt className="text-[var(--muted)]">{diagnosticText(locale, 'cacheTtlMinutes')}</dt><dd>{number(displayedSource.cacheTtlMinutes ?? 60)}</dd></div>
        <div><dt className="text-[var(--muted)]">{diagnosticText(locale, 'prefix')}</dt><dd>{displayedSource.prefix || '—'}</dd></div>
      </dl>
      <div className="flex items-end gap-2"><label className="min-w-48 flex-1 space-y-1 text-sm">{t('sourceDebug.mode')}
        <Select value={mode} disabled={loading} options={[{ value: 'bypass-cache', label: t('sourceDebug.bypass') }, ...(saved ? [{ value: 'production', label: t('sourceDebug.production') }] : [])]} onChange={(next) => { stop(); setMode(next as typeof mode); setResult(null); setProgress([]); setError('') }} className="w-full" />
      </label><Button variant="primary" loading={loading} onClick={() => void run()}>{t('sourceDebug.run')}</Button>{loading ? <Button onClick={stop}>{t('debug.cancel')}</Button> : null}</div>
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin /> : null}
      {progress.length ? <section className="space-y-2"><h3 className="text-sm font-semibold">{t('debug.stages')}</h3>
        <DiagnosticTimeline stages={progress} />
      </section> : null}
      {result ? <>
        <div className="flex flex-wrap gap-2 text-sm">
          <Tag color={result.status && result.status >= 400 ? 'red' : 'blue'}>{result.status === null ? result.cacheState === 'fresh' ? t('sourceDebug.cacheHit') : t('sourceDebug.noHttp') : `HTTP ${result.status}`}</Tag>
          <Tag color={result.cached ? 'green' : 'default'}>{t('sourceDebug.cache', { state: result.cacheState })}</Tag>
          <Tag>{diagnosticText(locale, 'elapsedMs')}: {number(result.elapsedMs)} ms</Tag><Tag>{number(result.bodyBytes)} B</Tag>
        </div>
        <p className="text-xs text-[var(--muted)]">{t('sourceDebug.summary', { status: result.ok ? t('common.completed') : t('common.failed'), ua: result.ua, count: number(result.nodeCount) })}</p>
        {result.warning ? <p className="text-xs text-amber-700">{result.warning}</p> : null}
        <Collapse size="small" items={[
          { key: 'diagnostics', label: t('sourceDebug.diagnostics'), children: <DiagnosticValue value={result.diagnostics} /> },
          ...(Object.keys(result.responseHeaders).length ? [{ key: 'headers', label: t('sourceDebug.headers'), children: <DiagnosticValue value={result.responseHeaders} /> }] : []),
          ...(result.raw ? [{ key: 'raw', label: `${t('sourceDebug.raw')}${result.rawTruncated ? t('sourceDebug.truncated') : ''}`, children: <pre className="max-h-60 overflow-auto whitespace-pre-wrap break-all text-xs">{result.raw}</pre> }] : []),
          ...(result.decodedText ? [{ key: 'decodedText', label: `${t('sourceDebug.decodedText')}${result.decodedTextTruncated ? t('sourceDebug.truncated') : ''}`, children: <pre className="max-h-60 overflow-auto whitespace-pre-wrap break-all text-xs">{result.decodedText}</pre> }] : []),
          ...(result.decoded.length ? [{ key: 'decoded', label: t('sourceDebug.decoded', { count: number(result.decoded.length) }), children: <DiagnosticValue value={result.decoded} /> }] : []),
        ]} />
        <Table rowKey={(_, index) => String(index)} dataSource={result.nodes} pagination={false} size="small" scroll={{ x: 500 }} columns={[
          { title: t('common.name'), dataIndex: 'name' }, { title: t('common.protocol'), dataIndex: 'type' },
          { title: t('common.server'), render: (_, node) => `${node.server}:${node.port}` },
        ]} locale={{ emptyText: t('common.noData') }} />
      </> : null}
    </div>
  </Modal>
}

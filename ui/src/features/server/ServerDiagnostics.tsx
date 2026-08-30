import { useState } from 'react'
import { Select } from '@acme/components'
import { Button, Card, Field, Spinner } from '../../components/ui'
import type { SubscriptionSource, SubscriptionTarget } from '../../lib/types'
import { serverAPI, type ServerPreviewNode, type ServerSession, type ServerSourceTestResult } from './server-api'
import { useServerT } from './server-i18n'

interface Props {
  session: ServerSession
  profileId: string
  sources: SubscriptionSource[]
  target: string
  targets: SubscriptionTarget[]
}

export function ServerDiagnostics({ session, profileId, sources, target, targets }: Props) {
  const t = useServerT()
  const [sourceId, setSourceId] = useState(sources[0]?.id ?? '')
  const [nodes, setNodes] = useState<ServerPreviewNode[]>([])
  const [nodeName, setNodeName] = useState('')
  const [sourceResult, setSourceResult] = useState<ServerSourceTestResult | null>(null)
  const [trace, setTrace] = useState<unknown>(null)
  const [pending, setPending] = useState('')
  const [notice, setNotice] = useState('')

  const effectiveSourceId = sources.some((source) => source.id === sourceId) ? sourceId : sources[0]?.id ?? ''
  const targetValue = targets.find((item) => item.format === target) ?? { format: target }

  const run = async (name: string, operation: () => Promise<void>) => {
    setPending(name)
    setNotice('')
    try {
      await operation()
    } catch (reason) {
      setNotice(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending('')
    }
  }

  const preview = () => run('preview', async () => {
    const result = await serverAPI<{ nodes: ServerPreviewNode[] }>(session, `/profiles/${profileId}/preview-nodes`, {
      method: 'POST', body: JSON.stringify({ target: targetValue }),
    })
    setNodes(result.nodes)
    setNodeName(result.nodes[0]?.name ?? '')
    setTrace(null)
  })

  const traceNode = () => run('trace', async () => {
    setTrace(await serverAPI<unknown>(session, `/profiles/${profileId}/trace-node`, {
      method: 'POST', body: JSON.stringify({ target: targetValue, name: nodeName }),
    }))
  })

  const testSource = () => run('source', async () => {
    setSourceResult(await serverAPI<ServerSourceTestResult>(session, `/profiles/${profileId}/sources/${encodeURIComponent(effectiveSourceId)}/test`, { method: 'POST' }))
  })

  const clearCache = () => run('cache', async () => {
    await serverAPI<void>(session, `/profiles/${profileId}/sources/${encodeURIComponent(effectiveSourceId)}/cache`, { method: 'DELETE' })
    setNotice(t('cacheCleared'))
  })

  return <Card className="space-y-4 p-4">
    <div><h3 className="font-medium">{t('diagnosticsTitle')}</h3><p className="text-xs text-[var(--muted)]">{t('diagnosticsDetail')}</p></div>
    {notice ? <p role={notice === t('cacheCleared') ? 'status' : 'alert'} className="text-sm">{notice}</p> : null}
    <div className="flex flex-wrap items-end gap-2">
      <Field label={t('source')}><Select className="min-w-56" value={effectiveSourceId} options={sources.map((source) => ({ value: source.id, label: source.remark || source.url || source.id }))} onChange={(value) => setSourceId(String(value))} /></Field>
      <Button disabled={!effectiveSourceId || Boolean(pending)} onClick={testSource}>{pending === 'source' ? <Spinner /> : null}{t('testSource')}</Button>
      <Button disabled={!effectiveSourceId || Boolean(pending)} onClick={clearCache}>{pending === 'cache' ? <Spinner /> : null}{t('clearCache')}</Button>
    </div>
    {sourceResult ? <div className="rounded border border-[var(--border)] p-3 text-xs"><p>{sourceResult.node_count} node(s) · {sourceResult.format} · {sourceResult.byte_count} bytes · SHA-256 {sourceResult.content_hash}</p>{sourceResult.diagnostics.map((message) => <p key={message} className="mt-1 text-amber-700 dark:text-amber-300">{message}</p>)}</div> : null}
    <div className="flex flex-wrap items-end gap-2">
      <Button disabled={Boolean(pending)} onClick={preview}>{pending === 'preview' ? <Spinner /> : null}{t('previewNodes')}</Button>
      {nodes.length ? <Field label={t('node')}><Select className="min-w-56" value={nodeName} options={nodes.map((node) => ({ value: node.name, label: `${node.filtered ? 'filtered · ' : ''}${node.name}` }))} onChange={(value) => setNodeName(String(value))} /></Field> : null}
      {nodes.length ? <Button disabled={!nodeName || Boolean(pending)} onClick={traceNode}>{pending === 'trace' ? <Spinner /> : null}{t('traceNode')}</Button> : null}
    </div>
    {nodes.length ? <div className="max-h-56 space-y-1 overflow-auto rounded border border-[var(--border)] p-2">{nodes.map((node) => <div key={`${node.sourceIndex}-${node.name}`} className={`grid gap-1 rounded px-2 py-1 text-xs sm:grid-cols-[minmax(10rem,1fr)_7rem_minmax(10rem,1fr)_4rem] ${node.filtered ? 'text-[var(--muted)] line-through' : ''}`}><strong>{node.name}</strong><span>{node.type}</span><span>{node.server}:{node.port}</span><span>#{node.sourceIndex}</span></div>)}</div> : null}
    {trace ? <pre aria-label="Node trace" className="max-h-72 overflow-auto whitespace-pre-wrap rounded border border-[var(--border)] p-3 text-xs">{JSON.stringify(trace, null, 2)}</pre> : null}
  </Card>
}

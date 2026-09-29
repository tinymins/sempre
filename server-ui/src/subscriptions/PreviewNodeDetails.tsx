import { Button, Collapse, Descriptions } from '@acme/components'
import type { NodeTraceResult, PreviewNode } from './diagnostic-types'
import { DiagnosticValue } from './DiagnosticValue'
import { TraceSteps } from './DiagnosticTrace'
import { useI18n } from '../i18n/provider'

const protocolFields: Record<string, readonly string[]> = {
  vmess: ['uuid', 'alterId', 'cipher', 'network', 'tls', 'servername', 'ws-opts', 'grpc-opts'],
  vless: ['uuid', 'flow', 'network', 'tls', 'sni', 'client-fingerprint', 'reality-opts', 'ws-opts', 'grpc-opts'],
  ss: ['cipher', 'password', 'plugin', 'plugin-opts', 'udp'],
  trojan: ['password', 'sni', 'alpn', 'skip-cert-verify', 'client-fingerprint', 'network', 'ws-opts', 'grpc-opts'],
  hysteria2: ['password', 'sni', 'obfs', 'obfs-password', 'alpn', 'skip-cert-verify'],
  hysteria: ['auth-str', 'obfs', 'protocol', 'up', 'down', 'sni', 'alpn'],
  tuic: ['uuid', 'password', 'congestion-controller', 'udp-relay-mode', 'sni', 'alpn', 'reduce-rtt'],
}
const basicFields = new Set(['name', 'type', 'server', 'port'])
const credentialFields = new Set(['uuid', 'password', 'auth-str', 'obfs-password'])

function isCredential(key: string) {
  return credentialFields.has(key) || /password|secret|token|private.?key/i.test(key)
}

function maskNested(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(maskNested)
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, isCredential(key) ? '••••' : maskNested(child)]))
  return value
}

function previewValue(key: string, value: unknown) {
  if (value === undefined || value === null) return '—'
  if (isCredential(key)) {
    if (typeof value !== 'string' || value.length <= 16) return '••••'
    return `${value.slice(0, 8)}…${value.slice(-4)}`
  }
  return String(value)
}

export function PreviewNodeDetails({ node, trace, traceLoading, onTrace, mobile = false }: {
  node: PreviewNode
  trace: NodeTraceResult | null
  traceLoading: boolean
  onTrace: () => void
  mobile?: boolean
}) {
  const { t, number } = useI18n()
  const preferred = protocolFields[node.type] ?? []
  const fields = [
    ...preferred.filter((key) => node.raw[key] !== undefined),
    ...Object.keys(node.raw).filter((key) => !basicFields.has(key) && !preferred.includes(key)),
  ]
  const labels: Record<string, string> = {
    uuid: 'UUID', password: t('common.password'), 'auth-str': t('preview.credential'), 'obfs-password': t('common.password'),
    network: t('dns.transport'), tls: 'TLS', sni: 'SNI', servername: 'SNI', 'client-fingerprint': 'Fingerprint',
  }
  const items = fields.map((key) => {
    const value = node.raw[key]
    const structured = value !== null && typeof value === 'object' && !isCredential(key)
    return {
      key,
      label: labels[key] ?? key,
      children: structured ? <DiagnosticValue value={maskNested(value)} /> : <span className="break-words font-mono">{previewValue(key, value)}</span>,
      span: structured && !mobile ? 2 : undefined,
    }
  })

  return <div className="min-w-0 space-y-3">
    <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
      <div className="min-w-0"><strong className="break-words text-sm">{node.name}</strong><p className="min-w-0 break-words text-xs text-[var(--muted)]">{node.type} · {node.server}:{node.port}</p><p className="truncate text-xs text-[var(--muted)]" title={node.sourceUrl}>{node.sourceUrl || `${t('common.source')} ${number(node.sourceIndex)}`}</p></div>
      <Button size="small" loading={traceLoading} onClick={onTrace}>{t('preview.trace')}</Button>
    </div>
    {items.length ? <div className="min-w-0 overflow-x-auto"><Descriptions bordered size="small" column={mobile ? 1 : 2} items={items} /></div> : null}
    <Collapse size="small" items={[{ key: 'raw', label: t('preview.raw'), children: <DiagnosticValue value={node.raw} /> }]} />
    {trace ? <div className="space-y-2"><h4 className="text-sm font-medium">{t('preview.traceSteps', { name: trace.nodeName })}</h4><TraceSteps trace={trace} /></div> : null}
  </div>
}

import { Card, Tag } from '@acme/components'
import { ConnectionCard } from './ApplicationEvidence'
import type { LeakReport } from './types'

export function ActiveProbes({ report, zh }: { report: LeakReport | null; zh: boolean }) {
  const probes = report?.probes
  const connections = report?.connections.filter((connection) => connection.app === 'probe') ?? []
  const labels = { pending: zh ? '待执行' : 'Pending', running: zh ? '执行中' : 'Running', passed: zh ? '已返回' : 'Responded', failed: zh ? '失败' : 'Failed', timeout: zh ? '超时' : 'Timed out', stopped: zh ? '已中止' : 'Stopped' }
  return <section className="min-w-0 space-y-3" aria-label={zh ? '本机探针证据' : 'Local probe evidence'}>
    <div className="grid min-w-0 grid-cols-1 gap-3 lg:grid-cols-3">
      {probes?.results.map((result) => <Card key={result.id} className="min-w-0" bodyStyle={{ padding: '1rem' }}>
        <div className="flex min-w-0 flex-wrap items-center justify-between gap-2"><h3 className="text-sm font-semibold">{result.id === 'dns' ? 'DNS' : `HTTPS / ${result.id === 'ipv4' ? 'IPv4' : 'IPv6'}`}</h3><Tag color={result.status === 'running' ? 'processing' : result.status === 'failed' || result.status === 'timeout' ? 'warning' : 'default'}>{labels[result.status]}</Tag></div>
        <p className="mt-3 break-words text-xs text-[var(--muted)]">{result.detail}</p>
        {result.publicIp && <p className="mt-3 break-all font-mono text-sm">{result.publicIp}</p>}
        {result.addresses.map((address) => <p key={address} className="mt-2 break-all font-mono text-xs">{address}</p>)}
        <p className="mt-3 text-xs text-[var(--muted)]">{result.pid != null ? `PID ${result.pid} · ` : ''}{result.elapsedMs} ms</p>
      </Card>)}
    </div>
    {connections.length > 0 && <div className="min-w-0 space-y-3"><h3 className="text-sm font-medium">{zh ? '探针连接证据' : 'Probe connection evidence'}</h3>{connections.map((connection) => <ConnectionCard key={connection.id} connection={connection} zh={zh} />)}</div>}
  </section>
}

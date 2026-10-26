import { Alert, Card, Tag } from '@acme/components'
import { normalizeIp } from './browserChecks'
import { ConnectionCard } from './ApplicationEvidence'
import type { LeakReport } from './types'

export function ActiveProbes({ report, submitting, expectedIpv4, expectedIpv6, zh }: { report: LeakReport | null; submitting: boolean; expectedIpv4: string; expectedIpv6: string; zh: boolean }) {
  const probes = report?.probes
  const connections = report?.connections.filter((connection) => connection.app === 'probe') ?? []
  const labels = { pending: zh ? '待执行' : 'Pending', running: zh ? '执行中' : 'Running', passed: zh ? '已返回' : 'Responded', failed: zh ? '失败' : 'Failed', timeout: zh ? '超时' : 'Timed out', stopped: zh ? '已中止' : 'Stopped' }
  return <section className="min-w-0 space-y-4" aria-label={zh ? '本机主动探针结果' : 'Local active probe results'}>
    <div className="min-w-0"><h2 className="text-lg font-semibold">{zh ? '本机主动探针' : 'Local active probes'}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? 'Sempre 启动独立临时进程检查 DNS 与 HTTPS IPv4 / IPv6。其出口、套接字和物理接口证据只代表探针，不代表 Codex / Claude Code。' : 'Sempre starts independent temporary processes for DNS and HTTPS IPv4 / IPv6. Egress, socket and interface evidence represents these probes, not Codex / Claude Code.'}</p></div>
    {probes?.reason && <Alert type={probes.available ? 'info' : 'warning'} message={probes.reason} />}
    <div className="grid min-w-0 grid-cols-1 gap-3 xl:grid-cols-3">
      {(['dns', 'ipv4', 'ipv6'] as const).map((id) => {
        const result = probes?.results.find((item) => item.id === id)
        const status = result?.status
        const expected = id === 'ipv4' ? expectedIpv4.trim() : id === 'ipv6' ? expectedIpv6.trim() : ''
        const mismatch = Boolean(result?.publicIp && expected && normalizeIp(result.publicIp) !== normalizeIp(expected))
        return <Card key={id} className="min-w-0" bodyStyle={{ padding: '1rem' }}>
          <div className="flex min-w-0 flex-wrap items-center justify-between gap-2"><h3 className="text-sm font-semibold">{id === 'dns' ? 'DNS' : `HTTPS / ${id === 'ipv4' ? 'IPv4' : 'IPv6'}`}</h3><Tag color={status === 'running' || submitting ? 'processing' : status === 'failed' || status === 'timeout' ? 'warning' : 'default'}>{submitting && !status ? (zh ? '正在发起' : 'Starting') : status ? labels[status] : (zh ? '待发起' : 'Not started')}</Tag></div>
          <p className="mt-3 break-words text-xs text-[var(--muted)]">{result?.detail || (zh ? '先开始观测，采集器就绪后点击“发起探针”。' : 'Start observation, then run probes after the collector is ready.')}</p>
          {result?.publicIp && <p className="mt-3 break-all font-mono text-sm">{result.publicIp}</p>}
          {expected && <p className="mt-2 break-all text-xs text-[var(--muted)]">{zh ? '本次填写的预期出口' : 'Expected egress for this run'}: {expected}</p>}
          {mismatch && <p className="mt-2 break-words text-xs text-amber-600 dark:text-amber-400">{zh ? '与预期不同，请核实。代理出口轮换也可能导致差异，不能仅凭此确认泄漏。' : 'Differs from expected egress; review required. Proxy egress rotation can also cause this difference, which alone does not confirm a leak.'}</p>}
          {result?.addresses.map((address) => <p key={address} className="mt-2 break-all font-mono text-xs">{address}</p>)}
          {result && <p className="mt-3 text-xs text-[var(--muted)]">{result.pid != null ? `PID ${result.pid} · ` : ''}{result.elapsedMs} ms</p>}
        </Card>
      })}
    </div>
    {connections.length > 0 && <div className="min-w-0 space-y-3"><h3 className="text-sm font-medium">{zh ? '探针连接证据' : 'Probe connection evidence'}</h3>{connections.map((connection) => <ConnectionCard key={connection.id} connection={connection} zh={zh} />)}</div>}
  </section>
}

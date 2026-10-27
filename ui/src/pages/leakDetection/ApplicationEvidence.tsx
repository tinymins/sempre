import { Card, Collapse, Tag } from '@acme/components'
import type { LeakConnection, LeakReport } from './types'

export function ApplicationEvidence({ report, zh }: { report: LeakReport | null; zh: boolean }) {
  if (!report) return <p className="py-5 text-sm text-[var(--muted)]">{zh ? '开始观测后，这里显示目标应用的真实连接。' : 'Real application connections appear here after observation starts.'}</p>
  const connections = report.connections.filter((connection) => connection.app !== 'probe')
  const processes = report.processes.filter((process) => process.app !== 'probe')
  return <section className="min-w-0 space-y-3">
    {!report.completed && <Tag color="warning">{zh ? '阶段记录 · 非完整结果' : 'Partial observation · incomplete result'}</Tag>}
    {processes.length > 0 && <div className="flex flex-wrap gap-2">{processes.map((process) => <Tag key={`${process.pid}-${process.startedAt}`}>{process.name} · PID {process.pid} · {process.app}</Tag>)}</div>}
    {connections.length ? connections.map((connection) => <ConnectionCard key={connection.id} connection={connection} zh={zh} />) : <p className="py-5 text-sm text-[var(--muted)]">{zh ? '没有观测到目标连接，无法判断是否泄漏。' : 'No target connections observed; leak status cannot be determined.'}</p>}
  </section>
}

export function ConnectionCard({ connection, zh }: { connection: LeakConnection; zh: boolean }) {
  const label = { 'proxy-route': zh ? '核心代理路由' : 'Core proxy route', 'direct-route': zh ? '核心直连路由' : 'Core direct route', unattributed: zh ? '归属未确认' : 'Unattributed' }[connection.verdict]
  return <Card className="min-w-0" bodyStyle={{ padding: '0.75rem 1rem' }}>
    <div className="flex flex-wrap items-start justify-between gap-2">
      <div className="min-w-0"><p className="break-all text-sm font-medium">{connection.host || connection.destination}</p><p className="mt-1 break-all text-xs text-[var(--muted)]">{connection.app} {connection.pid != null ? `· PID ${connection.pid}` : ''} · {connection.protocol} · <span className="font-mono">{connection.source} → {connection.destination}</span></p></div>
      <div className="flex flex-wrap gap-1">{connection.physicalEgress && <Tag color="warning">{zh ? '物理接口' : 'Physical interface'}</Tag>}<Tag color={connection.verdict === 'direct-route' ? 'warning' : 'default'}>{label}</Tag></div>
    </div>
    <Collapse className="mt-1" size="small" ghost items={[{ key: 'detail', label: zh ? '详细证据' : 'Details', children: <div className="min-w-0 space-y-2 text-xs">
      {(connection.rule || connection.chains.length > 0) && <p className="break-all">{connection.rule} {connection.chains.length > 0 ? `· ${connection.chains.join(' → ')}` : ''}</p>}
      <ul className="space-y-1 text-[var(--muted)]">{connection.evidence.map((item, index) => <li key={index} className="break-all">{item}</li>)}</ul>
    </div> }]} />
  </Card>
}

export function DnsEvidence({ report, zh }: { report: LeakReport | null; zh: boolean }) {
  return <section className="space-y-3">
    {report?.dns.length ? report.dns.map((record, index) => <Card key={index} bodyStyle={{ padding: '0.75rem 1rem' }}><div className="flex flex-wrap gap-2 text-sm"><Tag>{record.scope}</Tag><span>{record.protocol}</span><code className="break-all">{record.server}</code></div><p className="mt-2 break-words text-xs text-[var(--muted)]">{record.evidence}</p></Card>) : <p className="py-5 text-sm text-[var(--muted)]">{zh ? '尚无 DNS 路径证据。' : 'No DNS path evidence yet.'}</p>}
  </section>
}

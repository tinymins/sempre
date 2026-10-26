import { Alert, Card, Tag } from '@acme/components'
import type { LeakConnection, LeakReport } from './types'

export function ApplicationEvidence({ report, zh }: { report: LeakReport | null; zh: boolean }) {
  if (!report) return <p className="py-5 text-sm text-[var(--muted)]">{zh ? '这是可选的真实应用被动观测。主动探针无需启动 Codex / Claude Code，其结果单独显示在上方。' : 'Optional passive observation of real applications. Active probes above do not require Codex / Claude Code to be running.'}</p>
  const connections = report.connections.filter((connection) => connection.app !== 'probe')
  const processes = report.processes.filter((process) => process.app !== 'probe')
  return <section className="min-w-0 space-y-3">
    {!report.completed && <Tag color="warning">{zh ? '阶段记录 · 非完整结果' : 'Partial observation · incomplete result'}</Tag>}
    <Alert type="info" message={zh ? '连接证据的含义' : 'What connection evidence means'} description={zh ? '核心代理路由表示连接命中代理选路，不等于已证明物理出口。DIRECT 表示核心选择直连，需按目标与策略核实；未归属连接不能直接判定绕过。未捕获的短连接、独立 MCP 和系统共享 DNS 仍可能未覆盖。' : 'A proxy route proves a core routing decision, not the physical egress. DIRECT means the core selected direct routing and needs policy review. Unattributed connections do not prove bypass. Short-lived connections, independent MCP processes and shared system DNS may remain uncovered.'} />
    {processes.length > 0 && <div className="flex flex-wrap gap-2">{processes.map((process) => <Tag key={`${process.pid}-${process.startedAt}`}>{process.name} · PID {process.pid} · {process.app}</Tag>)}</div>}
    {connections.length ? connections.map((connection) => <ConnectionCard key={connection.id} connection={connection} zh={zh} />) : <p className="py-5 text-sm text-[var(--muted)]">{zh ? '没有观测到目标连接，无法判断是否泄漏。' : 'No target connections observed; leak status cannot be determined.'}</p>}
  </section>
}

export function ConnectionCard({ connection, zh }: { connection: LeakConnection; zh: boolean }) {
  const label = { 'proxy-route': zh ? '核心代理路由' : 'Core proxy route', 'direct-route': zh ? '核心直连路由' : 'Core direct route', unattributed: zh ? '归属未确认' : 'Unattributed' }[connection.verdict]
  return <Card className="min-w-0" bodyStyle={{ padding: '1rem' }}>
    <div className="flex flex-wrap items-start justify-between gap-2"><div className="min-w-0"><p className="break-all text-sm font-medium">{connection.host || connection.destination}</p><p className="mt-1 text-xs text-[var(--muted)]">{connection.app} {connection.pid != null ? `· PID ${connection.pid}` : ''} · {connection.protocol} · {connection.ipVersion}</p></div><Tag color={connection.verdict === 'direct-route' ? 'warning' : 'default'}>{label}</Tag></div>
    {connection.physicalEgress && <p className="mt-3 text-xs text-amber-600 dark:text-amber-400">{zh ? '观测到物理连接，需核对是否为显式代理端点；不能据此断言服务商收到真实 IP。' : 'Physical connection observed; check whether this is an explicit proxy endpoint. This does not prove the provider received a real IP.'}</p>}
    <p className="mt-3 break-all font-mono text-xs">{connection.source} → {connection.destination}</p>
    {(connection.rule || connection.chains.length > 0) && <p className="mt-2 break-all text-xs">{connection.rule} {connection.chains.length > 0 ? `· ${connection.chains.join(' → ')}` : ''}</p>}
    <ul className="mt-3 space-y-1 text-xs text-[var(--muted)]">{connection.evidence.map((item, index) => <li key={index} className="break-all">{item}</li>)}</ul>
  </Card>
}

export function DnsEvidence({ report, zh }: { report: LeakReport | null; zh: boolean }) {
  return <section className="space-y-3">
    <Alert type="info" message={zh ? 'DNS 路径需要独立证据' : 'DNS paths need independent evidence'} description={zh ? '公共 DNS 地址、解析器所在国家或 FakeIP 地址不能单独证明泄漏。系统解析器可能由多个应用共享；加密 DNS 的内容通常不可见，443 端口也不能单独证明 DoH。这里保留采集来源和归属边界。' : 'A public resolver address, resolver country or FakeIP address does not alone prove a leak. System resolvers may be shared. Encrypted DNS contents are usually opaque, and port 443 alone does not prove DoH. Evidence sources and attribution boundaries are retained below.'} />
    {report?.dns.length ? report.dns.map((record, index) => <Card key={index} bodyStyle={{ padding: '1rem' }}><div className="flex flex-wrap gap-2 text-sm"><Tag>{record.scope}</Tag><span>{record.protocol}</span><code className="break-all">{record.server}</code></div><p className="mt-2 break-words text-xs text-[var(--muted)]">{record.evidence}</p></Card>) : <p className="py-5 text-sm text-[var(--muted)]">{zh ? '尚无 DNS 路径证据。未观测不代表没有 DNS 泄漏。' : 'No DNS path evidence. Absence of observations does not prove absence of DNS leaks.'}</p>}
  </section>
}

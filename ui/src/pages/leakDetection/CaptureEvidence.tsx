import { Alert, Table, Tag } from '@acme/components'
import type { LeakReport } from './types'

export function CaptureEvidence({ report, zh }: { report: LeakReport | null; zh: boolean }) {
  if (!report) return <p className="py-5 text-sm text-[var(--muted)]">{zh ? '尚无接口采集结果。' : 'No interface evidence collected yet.'}</p>
  const capture = report.capture
  return <section className="space-y-3">
    <div className="flex flex-wrap items-center gap-2"><h2 className="text-sm font-medium">{zh ? '被动接口观测' : 'Passive interface observation'}</h2><Tag>{capture.status}</Tag></div>
    <p className="text-sm text-[var(--muted)]">{capture.detail}</p>
    <Alert type="info" message={zh ? '接口位置决定含义' : 'Interface position matters'} description={zh ? 'TUN 或网关 LAN 中出现目标 IP 本身不代表泄漏。需结合接口方向、进程套接字和核心选路关联；物理接口上的代理服务器连接是正常隧道流量。缺少权限、方向或关联证据时保持未知。' : 'A destination on TUN or gateway LAN does not itself indicate a leak. Interface direction, process sockets and core routing must be correlated. Proxy-server connections on physical interfaces are normal tunnel traffic. Missing permissions, direction or attribution leave the result unknown.'} />
    <Table size="small" dataSource={capture.packets} rowKey={(_packet, index) => String(index)} pagination={false} scroll={{ x: 640, y: 384 }} locale={{ emptyText: zh ? '未采集到接口记录；这不能证明没有绕过流量。' : 'No interface records collected; this does not prove absence of bypass traffic.' }} columns={[
      { key: 'interface', title: zh ? '接口 / 方向' : 'Interface / direction', render: (_, packet) => <span>{packet.interface}<span className="block text-[var(--muted)]">{packet.interfaceKind} · {packet.direction}</span></span> },
      { dataIndex: 'protocol', title: zh ? '协议' : 'Protocol' },
      { key: 'source', title: zh ? '源地址' : 'Source', render: (_, packet) => <code className="break-all">{packet.sourceIp.includes(':') ? `[${packet.sourceIp}]` : packet.sourceIp}:{packet.sourcePort}</code> },
      { key: 'destination', title: zh ? '目标地址' : 'Destination', render: (_, packet) => <code className="break-all">{packet.destinationIp.includes(':') ? `[${packet.destinationIp}]` : packet.destinationIp}:{packet.destinationPort}</code> },
    ]} />
  </section>
}

import { Table, Tag } from '@acme/components'
import type { LeakReport } from './types'

export function CaptureEvidence({ report, zh }: { report: LeakReport | null; zh: boolean }) {
  if (!report) return <p className="py-5 text-sm text-[var(--muted)]">{zh ? '尚无接口采集结果。' : 'No interface evidence collected yet.'}</p>
  const capture = report.capture
  return <section className="space-y-3">
    <div className="flex min-w-0 flex-wrap items-center gap-2"><Tag>{capture.status}</Tag><p className="min-w-0 break-words text-xs text-[var(--muted)]">{capture.detail}</p></div>
    <Table size="small" dataSource={capture.packets} rowKey={(_packet, index) => String(index)} pagination={false} scroll={{ x: 640, y: 384 }} locale={{ emptyText: zh ? '未采集到接口记录；这不能证明没有绕过流量。' : 'No interface records collected; this does not prove absence of bypass traffic.' }} columns={[
      { key: 'interface', title: zh ? '接口 / 方向' : 'Interface / direction', render: (_, packet) => <span>{packet.interface}<span className="block text-[var(--muted)]">{packet.interfaceKind} · {packet.direction}</span></span> },
      { dataIndex: 'protocol', title: zh ? '协议' : 'Protocol' },
      { key: 'source', title: zh ? '源地址' : 'Source', render: (_, packet) => <code className="break-all">{packet.sourceIp.includes(':') ? `[${packet.sourceIp}]` : packet.sourceIp}:{packet.sourcePort}</code> },
      { key: 'destination', title: zh ? '目标地址' : 'Destination', render: (_, packet) => <code className="break-all">{packet.destinationIp.includes(':') ? `[${packet.destinationIp}]` : packet.destinationIp}:{packet.destinationPort}</code> },
    ]} />
  </section>
}

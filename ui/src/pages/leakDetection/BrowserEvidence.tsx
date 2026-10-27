import { Card, Tag } from '@acme/components'
import type { AddressProbe, BrowserReport } from './types'

export function BrowserEvidence({ report, zh }: { report: BrowserReport | null; zh: boolean }) {
  if (!report) return <p className="py-3 text-sm text-[var(--muted)]">{zh ? '尚未检测浏览器。' : 'Browser not checked yet.'}</p>
  const scope = { public: zh ? '公网候选' : 'Public candidate', nonpublic: zh ? '非公网' : 'Non-public', mdns: 'mDNS', unknown: zh ? '未知' : 'Unknown' }
  return <section className="min-w-0 space-y-3">
    <p className="break-all text-xs text-[var(--muted)]">{zh ? '采集位置' : 'Collected at'}: {report.origin} · {new Date(report.startedAt).toLocaleString()}</p>
    <div className="grid min-w-0 grid-cols-1 gap-3 lg:grid-cols-2">
      <AddressCard label="HTTPS / IPv4" result={report.ipv4} zh={zh} />
      <AddressCard label="HTTPS / IPv6" result={report.ipv6} zh={zh} />
    </div>
    <Card className="min-w-0" title="WebRTC / STUN">
      <p className="text-sm text-[var(--muted)]">{report.webrtc.detail}</p>
      <div className="mt-3 space-y-2">{report.webrtc.candidates.map((candidate) => <div key={`${candidate.address}-${candidate.type}-${candidate.protocol}`} className="flex flex-wrap items-center gap-2 rounded-lg border border-[var(--border)] p-3 text-xs">
        <code className="break-all">{candidate.address}</code><Tag>{scope[candidate.scope]}</Tag><span>{candidate.type} · {candidate.protocol}</span>
      </div>)}</div>
    </Card>
  </section>
}

function AddressCard({ label, result, zh }: { label: string; result: AddressProbe; zh: boolean }) {
  return <Card className="min-w-0" title={label}>
    <code className="break-all text-base">{result.ip ?? (result.status === 'running' ? (zh ? '检测中…' : 'Checking…') : (zh ? '未知' : 'Unknown'))}</code>
    <p className="mt-2 break-words text-xs text-[var(--muted)]">{result.detail}</p>
    <p className="mt-2 break-all text-[11px] text-[var(--muted)]">{result.endpoint}</p>
  </Card>
}

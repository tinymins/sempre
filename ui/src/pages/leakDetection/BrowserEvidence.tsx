import { Alert, Card, Tag } from '@acme/components'
import { normalizeIp } from './browserChecks'
import type { AddressProbe, BrowserReport } from './types'

export function BrowserEvidence({ report, zh }: { report: BrowserReport | null; zh: boolean }) {
  if (!report) return <p className="py-5 text-sm text-[var(--muted)]">{zh ? '尚未检测浏览器。浏览器与 Sempre 服务可能位于不同设备。' : 'Browser not checked. The browser and Sempre service may be on different devices.'}</p>
  return <section className="space-y-4">
    <p className="break-all text-xs text-[var(--muted)]">{zh ? '采集位置' : 'Collected at'}: {report.origin} · {new Date(report.startedAt).toLocaleString()}</p>
    {!report.completed && <Alert type="warning" message={zh ? '浏览器检测已中止，结果不完整' : 'Browser checks stopped; results are incomplete'} />}
    <div className="grid gap-3 md:grid-cols-2">
      <AddressCard label="HTTPS / IPv4" result={report.ipv4} expected={report.expectedIpv4} zh={zh} />
      <AddressCard label="HTTPS / IPv6" result={report.ipv6} expected={report.expectedIpv6} zh={zh} />
    </div>
    <Card title="WebRTC / STUN">
      <p className="text-sm text-[var(--muted)]">{report.webrtc.detail}</p>
      <p className="mt-2 text-xs text-[var(--muted)]">{zh ? '局域网、保留地址和 mDNS 不作为公网 IP 泄漏。公网候选地址与预期出口不同仅提示路径差异，仍需结合实际路由核实。' : 'LAN, reserved and mDNS addresses are not public IP leaks. A public candidate differing from expected egress flags a path difference that still needs route evidence.'}</p>
      <div className="mt-3 space-y-2">{report.webrtc.candidates.length ? report.webrtc.candidates.map((candidate) => {
        const family = candidate.address.includes(':') ? 'ipv6' : 'ipv4'
        const expected = family === 'ipv6' ? report.expectedIpv6 : report.expectedIpv4
        const observed = report[family].ip
        const baseline = expected || observed
        const difference = candidate.scope === 'public' && baseline && normalizeIp(candidate.address) !== normalizeIp(baseline)
        const scope = { public: zh ? '公网候选' : 'Public candidate', nonpublic: zh ? '非公网' : 'Non-public', mdns: 'mDNS', unknown: zh ? '未知' : 'Unknown' }[candidate.scope]
        return <div key={`${candidate.address}-${candidate.type}-${candidate.protocol}`} className="rounded-lg border border-[var(--border)] p-3 text-xs">
          <div className="flex flex-wrap items-center gap-2"><code className="break-all">{candidate.address}</code><Tag color={difference ? 'warning' : 'default'}>{scope}</Tag><span>{candidate.type} · {candidate.protocol}</span></div>
          {difference && <p className="mt-2 text-amber-600 dark:text-amber-400">{expected ? (zh ? '与填写的预期出口不同，请核实。' : 'Differs from the supplied expected egress; review required.') : (zh ? '与同地址族的 HTTPS 出口不同，请核实。' : 'Differs from HTTPS egress in the same address family; review required.')}</p>}
        </div>
      }) : <p className="text-sm text-[var(--muted)]">{zh ? '未取得候选地址，结果未知。' : 'No candidate addresses; result unknown.'}</p>}</div>
    </Card>
    <Alert type="info" message={zh ? '浏览器覆盖边界' : 'Browser coverage boundaries'} description={zh ? '本页检测 HTTPS 出口和 WebRTC 候选地址。浏览器 DNS / DoH 实际解析路径、HTTP/3、其他扩展或其他进程连接未由这些探针证明；WebSocket 未检测。浏览器结果不能替代 Codex / Claude Code 的连接证据。' : 'These probes observe HTTPS egress and WebRTC candidates. They do not prove the browser DNS / DoH path, HTTP/3, extension or other process traffic. WebSocket is not tested. Browser results cannot replace Codex / Claude Code connection evidence.'} />
  </section>
}

function AddressCard({ label, result, expected, zh }: { label: string; result: AddressProbe; expected: string; zh: boolean }) {
  const mismatch = result.ip && expected && normalizeIp(result.ip) !== normalizeIp(expected)
  return <Card title={label}>
    <div className="flex flex-wrap items-center gap-2"><code className="break-all text-base">{result.ip ?? (zh ? '未知' : 'Unknown')}</code><Tag color={mismatch ? 'warning' : 'default'}>{mismatch ? (zh ? '出口不同' : 'Different egress') : result.status === 'observed' ? (zh ? '已观测' : 'Observed') : (zh ? '未覆盖' : 'Uncovered')}</Tag></div>
    {expected && <p className="mt-2 break-all text-xs">{zh ? '预期出口' : 'Expected egress'}: {expected}</p>}
    <p className="mt-2 break-words text-xs text-[var(--muted)]">{result.detail}</p>
    <p className="mt-2 break-all text-[11px] text-[var(--muted)]">{result.endpoint}</p>
    {mismatch && <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">{zh ? '可能存在路径差异；代理出口轮换也会导致不一致，不能仅凭此确认泄漏。' : 'Possible path difference; proxy egress rotation can also cause this mismatch. This alone does not confirm a leak.'}</p>}
  </Card>
}

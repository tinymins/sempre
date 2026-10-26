import { Card, Checkbox, Input, Select } from '@acme/components'
import type { LeakRunOptions } from './useLeakDetection'

export function LeakControls({ options, disabled, update, zh }: {
  options: LeakRunOptions
  disabled: boolean
  update: <K extends keyof LeakRunOptions>(key: K, value: LeakRunOptions[K]) => void
  zh: boolean
}) {
  return <Card className="min-w-0" bodyStyle={{ padding: '1rem' }}>
    <div className="grid min-w-0 grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3">
      <div className="min-w-0"><p className="mb-2 text-xs font-medium">{zh ? '部署方式' : 'Topology'}</p><Select aria-label={zh ? '部署方式' : 'Topology'} className="w-full min-w-0" value={options.topology} disabled={disabled} onChange={(value) => update('topology', value as LeakRunOptions['topology'])} options={[{ value: 'local', label: zh ? '本机 TUN' : 'Local TUN' }, { value: 'gateway', label: zh ? '网关 TUN' : 'Gateway TUN' }]} /></div>
      <div className="min-w-0"><p className="mb-2 text-xs font-medium">{zh ? '观测时长' : 'Duration'}</p><Select aria-label={zh ? '观测时长' : 'Duration'} className="w-full min-w-0" value={options.durationSeconds} disabled={disabled} onChange={(value) => update('durationSeconds', Number(value))} options={[30, 60, 120].map((value) => ({ value, label: `${value} ${zh ? '秒' : 'seconds'}` }))} /></div>
      <div className="min-w-0"><p className="mb-2 text-xs font-medium">{zh ? '附加被动观测' : 'Additional passive observation'}</p><Select aria-label={zh ? '附加被动观测' : 'Additional passive observation'} className="w-full min-w-0" value={options.target} disabled={disabled} onChange={(value) => update('target', value as LeakRunOptions['target'])} options={[{ value: 'all', label: 'Codex + Claude Code' }, { value: 'codex', label: 'Codex' }, { value: 'claude', label: 'Claude Code' }]} /></div>
    </div>
    {options.topology === 'gateway' && <div className="mt-4 min-w-0 space-y-2"><label className="block text-xs font-medium" htmlFor="leak-client-ip">{zh ? '受测客户端 IP' : 'Client IP to observe'}</label><Input id="leak-client-ip" className="w-full min-w-0" value={options.clientIp} placeholder="192.168.1.100" disabled={disabled} onChange={(event) => update('clientIp', event.target.value)} /><p className="text-xs text-[var(--muted)]">{zh ? '网关模式禁用本机主动探针：服务进程的出口不能代表客户端。仍可独立检测当前浏览器，并观测此 IP 经过网关的流量。' : 'Local active probes are disabled in gateway mode: service egress cannot represent the client. This browser can still be tested, alongside traffic from the selected client IP.'}</p></div>}
    <div className="mt-5 min-w-0 space-y-3 border-t border-[var(--border)] pt-4">
      <Checkbox className="items-start text-sm" checked={options.includeBrowser} disabled={disabled} onChange={(event) => update('includeBrowser', event.target.checked)}>{zh ? '点击“发起探针”时，同时检测浏览器 IPv4 / IPv6 / WebRTC' : 'Also check browser IPv4 / IPv6 / WebRTC when running probes'}</Checkbox>
      <p className="break-words text-xs text-[var(--muted)]">{zh ? '探针会访问 ipify 和 Google STUN，服务方可看到探针来源地址。开始观测本身不会发起这些测试；不发送 Sempre 登录凭据。' : 'Probes contact ipify and Google STUN, which can see the probe source address. Starting observation alone does not run these tests. Sempre credentials are not sent.'}</p>
      <div className="grid min-w-0 grid-cols-1 gap-3 xl:grid-cols-2">
        <div className="min-w-0"><label className="mb-2 block text-xs" htmlFor="leak-expected-v4">{zh ? '预期代理 IPv4（可选）' : 'Expected proxy IPv4 (optional)'}</label><Input id="leak-expected-v4" className="w-full min-w-0" value={options.expectedIpv4} disabled={disabled} onChange={(event) => update('expectedIpv4', event.target.value)} placeholder={zh ? '已知的代理出口' : 'Known proxy egress'} /></div>
        <div className="min-w-0"><label className="mb-2 block text-xs" htmlFor="leak-expected-v6">{zh ? '预期代理 IPv6（可选）' : 'Expected proxy IPv6 (optional)'}</label><Input id="leak-expected-v6" className="w-full min-w-0" value={options.expectedIpv6} disabled={disabled} onChange={(event) => update('expectedIpv6', event.target.value)} placeholder={zh ? '已知的代理出口' : 'Known proxy egress'} /></div>
      </div>
      <p className="text-xs text-[var(--muted)]">{zh ? '预期出口用于同地址族的本机探针与浏览器结果比较；若浏览器在另一台设备，请分别核对其网络路径。' : 'Expected egress is compared with local and browser probes of the same address family. If the browser is on another device, review its network path separately.'}</p>
      <p className="break-all text-[11px] text-[var(--muted)]">api.ipify.org · api6.ipify.org · stun.l.google.com:19302</p>
    </div>
  </Card>
}

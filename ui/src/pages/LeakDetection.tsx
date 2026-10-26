import { useState } from 'react'
import { Download, Play, ShieldCheck, Square } from 'lucide-react'
import { Alert, Button, Card, Checkbox, Input, Select, Tabs, Tag } from '@acme/components'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { ApplicationEvidence, DnsEvidence } from './leakDetection/ApplicationEvidence'
import { CaptureEvidence } from './leakDetection/CaptureEvidence'
import { BrowserEvidence } from './leakDetection/BrowserEvidence'
import { normalizeIp } from './leakDetection/browserChecks'
import { useLeakDetection, type LeakRunOptions } from './leakDetection/useLeakDetection'

export function LeakDetection() {
  const { locale } = useI18n()
  const { session } = useSession()
  const zh = locale === 'zh-CN'
  const [options, setOptions] = useState<LeakRunOptions>({ target: 'all', topology: 'local', clientIp: '', durationSeconds: 30, includeBrowser: true, expectedIpv4: '', expectedIpv6: '' })
  const check = useLeakDetection(session, zh)
  const [tab, setTab] = useState('application')
  const update = <K extends keyof LeakRunOptions>(key: K, value: LeakRunOptions[K]) => setOptions((previous) => ({ ...previous, [key]: value }))
  const expectedValid = (!options.expectedIpv4.trim() || Boolean(normalizeIp(options.expectedIpv4) && !options.expectedIpv4.includes(':')))
    && (!options.expectedIpv6.trim() || Boolean(normalizeIp(options.expectedIpv6) && options.expectedIpv6.includes(':')))
  const clientValid = options.topology === 'local' || Boolean(normalizeIp(options.clientIp))
  const gaps = [...new Set([...(check.preflight?.gaps ?? []), ...(check.report?.gaps ?? [])])]
  const summary = check.report?.summary
  const count = (value: number | undefined) => value ? `${value} ${zh ? '条观测' : 'observations'}` : (zh ? '未覆盖' : 'Uncovered')

  return <div className="space-y-5">
    <header className="flex flex-wrap items-start justify-between gap-3">
      <div><div className="flex items-center gap-2"><ShieldCheck size={23} className="text-[var(--muted)]" /><h1 className="text-2xl font-semibold">{zh ? '泄漏检测' : 'Leak detection'}</h1></div><p className="mt-2 text-sm text-[var(--muted)]">{zh ? '检查 Codex / Claude Code 的网络出口与 DNS 泄漏' : 'Inspect Codex / Claude Code egress and DNS leakage'}</p></div>
      <Tag color={check.running ? 'processing' : 'default'}>{check.running ? (zh ? '正在观测' : 'Observing') : (zh ? '按证据判断' : 'Evidence based')}</Tag>
    </header>
    <Card bodyStyle={{ padding: '1.25rem' }}>
      <div className="grid gap-4 sm:grid-cols-3">
        <div><p className="mb-2 text-xs font-medium">{zh ? '检测对象' : 'Application'}</p><Select aria-label={zh ? '检测对象' : 'Application'} className="w-full" value={options.target} disabled={check.running} onChange={(value) => update('target', value as LeakRunOptions['target'])} options={[{ value: 'all', label: 'Codex + Claude Code' }, { value: 'codex', label: 'Codex' }, { value: 'claude', label: 'Claude Code' }]} /></div>
        <div><p className="mb-2 text-xs font-medium">{zh ? '部署方式' : 'Topology'}</p><Select aria-label={zh ? '部署方式' : 'Topology'} className="w-full" value={options.topology} disabled={check.running} onChange={(value) => update('topology', value as LeakRunOptions['topology'])} options={[{ value: 'local', label: zh ? '本机 TUN' : 'Local TUN' }, { value: 'gateway', label: zh ? '网关 TUN' : 'Gateway TUN' }]} /></div>
        <div><p className="mb-2 text-xs font-medium">{zh ? '观测时长' : 'Duration'}</p><Select aria-label={zh ? '观测时长' : 'Duration'} className="w-full" value={options.durationSeconds} disabled={check.running} onChange={(value) => update('durationSeconds', Number(value))} options={[30, 60, 120].map((value) => ({ value, label: `${value} ${zh ? '秒' : 'seconds'}` }))} /></div>
      </div>
      {options.topology === 'gateway' && <div className="mt-4 space-y-2"><label className="block text-xs font-medium" htmlFor="leak-client-ip">{zh ? '受测客户端 IP' : 'Client IP to observe'}</label><Input id="leak-client-ip" value={options.clientIp} placeholder="192.168.1.100" disabled={check.running} onChange={(event) => update('clientIp', event.target.value)} /><p className="text-xs text-[var(--muted)]">{zh ? '仅观察此客户端经过网关的连接。网关无法直接确认客户端进程身份；LAN 流量不等于 WAN 泄漏，其他 IPv4 / IPv6 地址未自动包含。' : 'Observe this client address through the gateway. The gateway cannot directly verify client process identity. LAN traffic is not WAN leakage; other IPv4 / IPv6 addresses are not automatically included.'}</p></div>}
      <div className="mt-4 border-t border-[var(--border)] pt-4">
        <Checkbox checked={options.includeBrowser} disabled={check.running} onChange={(event) => update('includeBrowser', event.target.checked)}>{zh ? '同时检测此浏览器（IPv4 / IPv6 / WebRTC）' : 'Also check this browser (IPv4 / IPv6 / WebRTC)'}</Checkbox>
        <p className="mt-2 break-words text-xs text-[var(--muted)]">{zh ? '浏览器检测将访问 api.ipify.org、api6.ipify.org 和 Google STUN（stun.l.google.com:19302），这些服务会看到探针来源地址；不发送 Sempre 登录凭据。' : 'Browser checks contact api.ipify.org, api6.ipify.org and Google STUN (stun.l.google.com:19302). These services see the probe source address; Sempre login credentials are not sent.'}</p>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <div><label className="mb-2 block text-xs" htmlFor="leak-expected-v4">{zh ? '预期代理 IPv4（可选）' : 'Expected proxy IPv4 (optional)'}</label><Input id="leak-expected-v4" value={options.expectedIpv4} disabled={check.running} onChange={(event) => update('expectedIpv4', event.target.value)} placeholder={zh ? '已知的代理出口' : 'Known proxy egress'} /></div>
          <div><label className="mb-2 block text-xs" htmlFor="leak-expected-v6">{zh ? '预期代理 IPv6（可选）' : 'Expected proxy IPv6 (optional)'}</label><Input id="leak-expected-v6" value={options.expectedIpv6} disabled={check.running} onChange={(event) => update('expectedIpv6', event.target.value)} placeholder={zh ? '已知的代理出口' : 'Known proxy egress'} /></div>
        </div>
      </div>
      {(!expectedValid || !clientValid) && <p className="mt-3 text-xs text-amber-600">{zh ? '请填写有效的 IP 地址，并匹配 IPv4 / IPv6 类型。' : 'Enter valid IP addresses in the correct IPv4 / IPv6 fields.'}</p>}
      <div className="mt-5 flex flex-wrap gap-2">
        {check.running ? <Button icon={<Square size={14} />} onClick={check.stop}>{zh ? '停止' : 'Stop'}</Button> : <Button variant="primary" icon={<Play size={14} />} disabled={!clientValid || (options.includeBrowser && !expectedValid)} onClick={() => { setTab('application'); check.start(options) }}>{zh ? '开始检测' : 'Start observation'}</Button>}
        <Button disabled={check.running || !expectedValid} onClick={() => { setTab('browser'); check.start(options, true) }}>{zh ? '仅检测浏览器' : 'Browser only'}</Button>
        <Button icon={<Download size={14} />} disabled={check.running || (!check.report && !check.browser)} onClick={check.exportReport}>{zh ? '导出报告' : 'Export report'}</Button>
      </div>
      <p className="mt-3 text-xs text-[var(--muted)]">{zh ? '检测期间请在应用中发送实际请求。记录仅保留在当前页面内存；导出的 JSON 包含 IP、进程和连接信息。检测不会调整 TUN、DNS 或代理配置。' : 'Make real requests in the application during observation. Records stay in page memory; exported JSON includes IP, process and connection information. Checks do not change TUN, DNS or proxy settings.'}</p>
    </Card>
    <p className="break-all text-xs text-[var(--muted)]">{zh ? '服务采集位置' : 'Service collector'}: {session?.baseURL} · {check.preflight?.platform ?? (zh ? '平台未知' : 'Platform unknown')}<br />{zh ? '浏览器位置' : 'Browser location'}: {window.location.origin} · {zh ? '本机模式指 Sempre 服务所在设备。' : 'Local mode means the device running the Sempre service.'}</p>
    {check.preflightError && <Alert type="warning" message={zh ? '预检查不可用' : 'Preflight unavailable'} description={check.preflightError} />}
    {check.error && <Alert type="error" message={zh ? '采集未完整完成' : 'Observation did not complete'} description={check.error} />}
    {check.message && <p className="text-sm" role="status">{check.message}{check.report ? ` · ${check.report.elapsedSeconds}s` : ''}</p>}
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
      {[{ label: zh ? '物理连接' : 'Physical connections', value: count(summary?.physicalEgress) }, { label: zh ? '代理路由' : 'Proxy route', value: count(summary?.proxyRoute) }, { label: zh ? '直连路由' : 'Direct route', value: count(summary?.directRoute) }, { label: 'IPv4', value: count(summary?.ipv4) }, { label: 'IPv6', value: count(summary?.ipv6) }, { label: zh ? '归属未确认' : 'Unattributed', value: count(summary?.unattributed) }].map((item) => <Card key={item.label} bodyStyle={{ padding: '1rem' }}><p className="text-xs text-[var(--muted)]">{item.label}</p><p className="mt-2 text-sm font-semibold">{item.value}</p></Card>)}
    </div>
    <Tabs activeKey={tab} onChange={setTab} items={[
      { key: 'application', label: zh ? '应用连接' : 'Application connections', children: <ApplicationEvidence report={check.report} zh={zh} /> },
      { key: 'capture', label: zh ? '接口证据' : 'Interface evidence', children: <CaptureEvidence report={check.report} zh={zh} /> },
      { key: 'dns', label: 'DNS', children: <DnsEvidence report={check.report} zh={zh} /> },
      { key: 'browser', label: zh ? '浏览器' : 'Browser', children: <BrowserEvidence report={check.browser} zh={zh} /> },
      { key: 'coverage', label: zh ? '采集能力与缺口' : 'Capabilities & gaps', children: <div className="space-y-3">
        <Alert type="warning" message={zh ? '未观测不等于没有泄漏' : 'Unobserved does not mean leak-free'} description={zh ? '结论仅限本次观测时段、目标进程和已覆盖协议。核心快照、系统套接字和物理接口证据的覆盖范围不同；缺少任一层时会保留未知。' : 'Results apply only to this observation window, selected processes and covered protocols. Core snapshots, system sockets and physical interface evidence have different coverage; missing layers remain unknown.'} />
        {check.preflight?.capabilities.map((capability) => <Card key={capability.id} bodyStyle={{ padding: '1rem' }}><div className="flex flex-wrap items-center gap-2 text-sm"><span>{capability.id}</span><Tag>{capability.status}</Tag></div><p className="mt-2 text-xs text-[var(--muted)]">{capability.detail}</p></Card>)}
        {gaps.length > 0 && <ul className="list-disc space-y-2 pl-5 text-sm text-[var(--muted)]">{gaps.map((gap) => <li key={gap}>{gap}</li>)}</ul>}
      </div> },
    ]} />
  </div>
}

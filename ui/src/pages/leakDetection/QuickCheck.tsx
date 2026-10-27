import { useState } from 'react'
import { Play, Square } from 'lucide-react'
import { Alert, Button, Checkbox, Collapse, Input, Select } from '@acme/components'
import { ActiveProbes } from './ActiveProbes'
import { BrowserEvidence } from './BrowserEvidence'
import { LeakNotes } from './LeakNotes'
import { LeakProgress } from './LeakProgress'
import { LeakVerdict } from './LeakVerdict'
import { normalizeIp } from './browserChecks'
import type { LeakRunOptions, useLeakDetection } from './useLeakDetection'
import { quickVerdict } from './verdict'

const validFamily = (value: string, ipv6: boolean) => !value.trim() || Boolean(normalizeIp(value) && value.includes(':') === ipv6)

export function QuickCheck({ check, zh }: { check: ReturnType<typeof useLeakDetection>; zh: boolean }) {
  const [topology, setTopology] = useState<LeakRunOptions['topology']>('local')
  const [includeBrowser, setIncludeBrowser] = useState(true)
  const [expectedIpv4, setExpectedIpv4] = useState('')
  const [expectedIpv6, setExpectedIpv6] = useState('')
  const run = check.run?.mode === 'quick' ? check.run : null
  const localUnavailable = topology === 'local' && Boolean(check.preflightError)
  const expectedValid = validFamily(expectedIpv4, false) && validFamily(expectedIpv6, true)
  const nothingToRun = (topology === 'gateway' || localUnavailable) && !includeBrowser
  const start = () => check.start({ mode: 'quick', topology, clientIp: '', target: 'all', durationSeconds: 0, includeBrowser, expectedIpv4, expectedIpv6 })

  return <div className="min-w-0 space-y-5">
    <div className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-end">
      <div className="min-w-0 sm:w-64"><p className="mb-2 text-xs font-medium">{zh ? '检测对象' : 'Check'}</p><Select aria-label={zh ? '检测对象' : 'Check'} className="w-full min-w-0" value={topology} disabled={check.running} onChange={(value) => setTopology(value as LeakRunOptions['topology'])} options={[{ value: 'local', label: zh ? 'Sempre 所在设备 + 当前浏览器' : 'Sempre device + this browser' }, { value: 'gateway', label: zh ? '仅当前浏览器（网关客户端）' : 'This browser only (gateway client)' }]} /></div>
      {check.running
        ? <Button className="w-full sm:w-auto" icon={<Square size={14} />} onClick={check.stop}>{zh ? '停止' : 'Stop'}</Button>
        : <Button className="w-full sm:w-auto" variant="primary" icon={<Play size={14} />} disabled={!expectedValid || nothingToRun} onClick={start}>{zh ? '开始检测' : 'Start check'}</Button>}
    </div>
    {localUnavailable && <Alert type="warning" message={zh ? '无法连接 Sempre 采集器，本次只检测浏览器' : 'Sempre collector unavailable; only this browser will be checked'} description={check.preflightError} />}
    {topology === 'gateway' && <p className="text-xs text-[var(--muted)]">{zh ? '请在受测客户端设备上打开本页。' : 'Open this page on the client device.'}</p>}
    <Collapse size="small" items={[{ key: 'advanced', label: zh ? '高级选项' : 'Advanced options', children: <div className="min-w-0 space-y-3">
      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
        <div className="min-w-0"><label className="mb-2 block text-xs" htmlFor="leak-expected-v4">{zh ? '预期代理出口 IPv4' : 'Expected proxy IPv4'}</label><Input id="leak-expected-v4" className="w-full min-w-0" value={expectedIpv4} disabled={check.running} onChange={(event) => setExpectedIpv4(event.target.value)} placeholder={zh ? '可选' : 'Optional'} /></div>
        <div className="min-w-0"><label className="mb-2 block text-xs" htmlFor="leak-expected-v6">{zh ? '预期代理出口 IPv6' : 'Expected proxy IPv6'}</label><Input id="leak-expected-v6" className="w-full min-w-0" value={expectedIpv6} disabled={check.running} onChange={(event) => setExpectedIpv6(event.target.value)} placeholder={zh ? '可选' : 'Optional'} /></div>
      </div>
      {!expectedValid && <p className="text-xs text-amber-600">{zh ? '预期出口必须为对应类型的有效 IP 地址。' : 'Expected egress must be a valid IP address of the indicated family.'}</p>}
      <p className="text-xs text-[var(--muted)]">{zh ? '不填写时，浏览器出口与本机探针出口互相比对。' : 'When empty, browser egress is compared with local probe egress.'}</p>
      <Checkbox className="items-start text-sm" checked={includeBrowser} disabled={check.running} onChange={(event) => setIncludeBrowser(event.target.checked)}>{zh ? '同时检测当前浏览器 IPv4 / IPv6 / WebRTC' : 'Also check this browser’s IPv4 / IPv6 / WebRTC'}</Checkbox>
    </div> }]} />
    <LeakProgress run={run} phase={run ? check.phase : 'idle'} remainingSeconds={check.remainingSeconds} error={run ? check.error : ''} zh={zh} />
    {run && <LeakVerdict verdict={quickVerdict({ ...run, report: check.report, browser: check.browser }, zh)} running={check.running} zh={zh} />}
    {run && <Collapse size="small" items={[
      ...(run.localProbes ? [{ key: 'local', label: zh ? '本机探针证据' : 'Local probe evidence', children: <ActiveProbes report={check.report} zh={zh} /> }] : []),
      ...(run.includeBrowser ? [{ key: 'browser', label: zh ? '浏览器探针证据' : 'Browser probe evidence', children: <BrowserEvidence report={check.browser} zh={zh} /> }] : []),
    ]} />}
    <LeakNotes mode="quick" zh={zh} />
  </div>
}

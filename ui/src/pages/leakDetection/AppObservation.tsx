import { type ReactNode, useState } from 'react'
import { Play, Square } from 'lucide-react'
import { Alert, Button, Card, Collapse, Input, Select, Tabs, Tag } from '@acme/components'
import { ApplicationEvidence, DnsEvidence } from './ApplicationEvidence'
import { CaptureEvidence } from './CaptureEvidence'
import { LeakNotes } from './LeakNotes'
import { LeakProgress } from './LeakProgress'
import { LeakVerdict } from './LeakVerdict'
import { normalizeIp } from './browserChecks'
import type { LeakRunOptions, useLeakDetection } from './useLeakDetection'
import { observationVerdict } from './verdict'

export function AppObservation({ check, zh }: { check: ReturnType<typeof useLeakDetection>; zh: boolean }) {
  const [topology, setTopology] = useState<LeakRunOptions['topology']>('local')
  const [clientIp, setClientIp] = useState('')
  const [target, setTarget] = useState<LeakRunOptions['target']>('all')
  const [durationSeconds, setDurationSeconds] = useState(60)
  const run = check.run?.mode === 'observe' ? check.run : null
  const report = run ? check.report : null
  const clientValid = topology === 'local' || Boolean(normalizeIp(clientIp))
  const gaps = [...new Set([...(check.preflight?.gaps ?? []), ...(report?.gaps ?? [])])]
  const start = () => check.start({ mode: 'observe', topology, clientIp, target, durationSeconds, includeBrowser: false, expectedIpv4: '', expectedIpv6: '' })
  const field = (label: string, control: ReactNode) => <div className="min-w-0"><p className="mb-2 text-xs font-medium">{label}</p>{control}</div>

  return <div className="min-w-0 space-y-5">
    <p className="text-sm text-[var(--muted)]">{zh ? '开始后正常使用 Codex / Claude Code，检查其真实连接是否经过代理。' : 'Start, then use Codex / Claude Code normally to check whether their connections use the proxy.'}</p>
    <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-[repeat(3,minmax(0,12rem))_auto] lg:items-end">
      {field(zh ? '观测位置' : 'Observe on', <Select aria-label={zh ? '观测位置' : 'Observe on'} className="w-full min-w-0" value={topology} disabled={check.running} onChange={(value) => setTopology(value as LeakRunOptions['topology'])} options={[{ value: 'local', label: zh ? '本机 TUN' : 'Local TUN' }, { value: 'gateway', label: zh ? '网关 TUN' : 'Gateway TUN' }]} />)}
      {field(zh ? '目标应用' : 'Applications', <Select aria-label={zh ? '目标应用' : 'Applications'} className="w-full min-w-0" value={target} disabled={check.running} onChange={(value) => setTarget(value as LeakRunOptions['target'])} options={[{ value: 'all', label: 'Codex + Claude Code' }, { value: 'codex', label: 'Codex' }, { value: 'claude', label: 'Claude Code' }]} />)}
      {field(zh ? '观测时长' : 'Duration', <Select aria-label={zh ? '观测时长' : 'Duration'} className="w-full min-w-0" value={durationSeconds} disabled={check.running} onChange={(value) => setDurationSeconds(Number(value))} options={[30, 60, 120].map((value) => ({ value, label: `${value} ${zh ? '秒' : 'seconds'}` }))} />)}
      {check.running
        ? <Button className="w-full sm:w-auto" icon={<Square size={14} />} onClick={check.stop}>{zh ? '停止' : 'Stop'}</Button>
        : <Button className="w-full sm:w-auto" variant="primary" icon={<Play size={14} />} disabled={!clientValid} onClick={start}>{zh ? '开始观测' : 'Start observation'}</Button>}
    </div>
    {topology === 'gateway' && <div className="min-w-0 space-y-2 sm:max-w-sm"><label className="block text-xs font-medium" htmlFor="leak-client-ip">{zh ? '受测客户端 IP' : 'Client IP to observe'}</label><Input id="leak-client-ip" className="w-full min-w-0" value={clientIp} placeholder="192.168.1.100" disabled={check.running} onChange={(event) => setClientIp(event.target.value)} />{clientIp.trim() && !clientValid && <p className="text-xs text-amber-600">{zh ? '请输入有效的 IP 地址。' : 'Enter a valid IP address.'}</p>}</div>}
    {check.preflightError && <Alert type="warning" message={zh ? '预检查不可用' : 'Preflight unavailable'} description={check.preflightError} />}
    <LeakProgress run={run} phase={run ? check.phase : 'idle'} remainingSeconds={check.remainingSeconds} error={run ? check.error : ''} zh={zh} />
    {run && <LeakVerdict verdict={observationVerdict(report, zh)} running={check.running} zh={zh} />}
    {run && <Collapse size="small" items={[{ key: 'evidence', label: zh ? '证据详情' : 'Evidence details', children: <Tabs className="min-w-0" items={[
      { key: 'application', label: zh ? '真实应用' : 'Applications', children: <div className="min-w-0 pt-4"><ApplicationEvidence report={report} zh={zh} /></div> },
      { key: 'capture', label: zh ? '设备接口' : 'Device interfaces', children: <div className="min-w-0 pt-4"><CaptureEvidence report={report} zh={zh} /></div> },
      { key: 'dns', label: 'DNS', children: <div className="min-w-0 pt-4"><DnsEvidence report={report} zh={zh} /></div> },
      { key: 'coverage', label: zh ? '采集能力与缺口' : 'Capabilities & gaps', children: <div className="min-w-0 space-y-3 pt-4">
        {check.preflight?.capabilities.map((capability) => <Card key={capability.id} className="min-w-0" bodyStyle={{ padding: '1rem' }}><div className="flex min-w-0 flex-wrap items-center gap-2 text-sm"><span className="break-all">{capability.id}</span><Tag>{capability.status}</Tag></div><p className="mt-2 break-words text-xs text-[var(--muted)]">{capability.detail}</p></Card>)}
        {gaps.length > 0 && <ul className="list-disc space-y-2 pl-5 text-sm text-[var(--muted)]">{gaps.map((gap) => <li key={gap} className="break-words">{gap}</li>)}</ul>}
      </div> },
    ]} /> }]} />}
    <LeakNotes mode="observe" zh={zh} />
  </div>
}

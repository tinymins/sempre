import { useState } from 'react'
import { Download, Play, Radar, ShieldCheck, Square } from 'lucide-react'
import { Alert, Button, Card, Tabs, Tag } from '@acme/components'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { ActiveProbes } from './leakDetection/ActiveProbes'
import { ApplicationEvidence, DnsEvidence } from './leakDetection/ApplicationEvidence'
import { CaptureEvidence } from './leakDetection/CaptureEvidence'
import { BrowserEvidence } from './leakDetection/BrowserEvidence'
import { LeakControls } from './leakDetection/LeakControls'
import { normalizeIp } from './leakDetection/browserChecks'
import { useLeakDetection, type LeakRunOptions } from './leakDetection/useLeakDetection'

export function LeakDetection() {
  const { locale } = useI18n()
  const { session } = useSession()
  const zh = locale === 'zh-CN'
  const [options, setOptions] = useState<LeakRunOptions>({ target: 'all', topology: 'local', clientIp: '', durationSeconds: 60, includeBrowser: true, expectedIpv4: '', expectedIpv6: '' })
  const check = useLeakDetection(session, zh)
  const update = <K extends keyof LeakRunOptions>(key: K, value: LeakRunOptions[K]) => setOptions((previous) => ({ ...previous, [key]: value }))
  const browserRunning = Boolean(check.browser?.running)
  const busy = check.observing || browserRunning
  const expectedValid = (!options.expectedIpv4.trim() || Boolean(normalizeIp(options.expectedIpv4) && !options.expectedIpv4.includes(':')))
    && (!options.expectedIpv6.trim() || Boolean(normalizeIp(options.expectedIpv6) && options.expectedIpv6.includes(':')))
  const clientValid = options.topology === 'local' || Boolean(normalizeIp(options.clientIp))
  const gaps = [...new Set([...(check.preflight?.gaps ?? []), ...(check.report?.gaps ?? [])])]
  const enoughTime = check.remainingSeconds >= 20
  const canProbe = check.observing && check.report?.ready && check.report.probes.available && check.report.probes.status === 'idle' && !check.probeRequested && enoughTime && expectedValid
  const phases = { idle: zh ? '待开始' : 'Not started', connecting: zh ? '准备采集器' : 'Preparing collector', ready: zh ? '采集器就绪' : 'Collector ready', completed: zh ? '观测结束' : 'Completed', stopped: zh ? '已停止' : 'Stopped', timeout: zh ? '超时' : 'Timed out', failed: zh ? '失败' : 'Failed' }
  const probeStatus = check.report?.probes.status
  const nativeLabel = check.probeSubmitting ? (zh ? '正在发起' : 'Starting') : probeStatus === 'running' ? (zh ? '执行中' : 'Running') : probeStatus === 'completed' ? (zh ? '已返回结果' : 'Results received') : probeStatus === 'stopped' ? (zh ? '已中止' : 'Stopped') : options.topology === 'gateway' || probeStatus === 'unavailable' ? (zh ? '不可用于此环境' : 'Unavailable here') : (zh ? '等待发起' : 'Waiting to run')
  const summary = check.report?.summary
  const count = (value: number | undefined) => value == null ? (zh ? '尚无记录' : 'No records yet') : `${value} ${zh ? '条观测' : 'observations'}`

  return <div className="min-w-0 space-y-6">
    <header className="min-w-0 space-y-2">
      <div className="flex min-w-0 items-center gap-2"><ShieldCheck size={23} className="shrink-0 text-[var(--muted)]" /><h1 className="text-2xl font-semibold">{zh ? '泄漏检测' : 'Leak detection'}</h1></div>
      <p className="text-sm text-[var(--muted)]">{zh ? '先开始观测，再发起主动探针。无需启动 Codex / Claude Code；真实应用连接作为独立的附加证据。' : 'Start observation, then run active probes. Codex / Claude Code need not be running; real application connections provide separate additional evidence.'}</p>
    </header>
    <LeakControls options={options} disabled={busy} update={update} zh={zh} />
    <section className="min-w-0 space-y-3" aria-label={zh ? '检测操作' : 'Detection controls'}>
      <div className="grid grid-cols-1 gap-2 sm:flex sm:flex-wrap">
        {check.observing ? <Button className="w-full shrink-0 sm:w-auto" icon={<Square size={14} />} onClick={check.stop}>{zh ? '停止观测与探针' : 'Stop observation & probes'}</Button> : <Button className="w-full shrink-0 sm:w-auto" variant="primary" icon={<Play size={14} />} disabled={browserRunning || !clientValid} onClick={() => check.start(options)}>{zh ? '1. 开始观测' : '1. Start observation'}</Button>}
        <Button className="w-full shrink-0 sm:w-auto" variant="primary" icon={<Radar size={14} />} disabled={!canProbe} onClick={() => { void check.triggerProbes() }}>{zh ? '2. 发起探针' : '2. Run probes'}</Button>
        <Button className="w-full shrink-0 sm:w-auto" disabled={browserRunning || !expectedValid} onClick={() => check.startBrowser(options)}>{zh ? '仅检测浏览器' : 'Browser only'}</Button>
        {browserRunning && !check.observing && <Button className="w-full shrink-0 sm:w-auto" icon={<Square size={14} />} onClick={check.stop}>{zh ? '停止浏览器检测' : 'Stop browser checks'}</Button>}
        <Button className="w-full shrink-0 sm:w-auto" icon={<Download size={14} />} disabled={busy || (!check.report && !check.browser)} onClick={check.exportReport}>{zh ? '导出报告' : 'Export report'}</Button>
      </div>
      {!expectedValid && <p className="text-xs text-amber-600">{zh ? '预期出口必须为对应类型的有效 IP 地址。' : 'Expected egress must be a valid IP address of the indicated family.'}</p>}
      {!clientValid && <p className="text-xs text-amber-600">{zh ? '网关观测需要有效的客户端 IP。' : 'Gateway observation requires a valid client IP.'}</p>}
      {check.observing && !enoughTime && !check.probeRequested && options.topology === 'local' && <Alert type="warning" message={zh ? '剩余时间不足，请重新开始观测后发起探针。' : 'Not enough time remains. Restart observation before running probes.'} />}
      <p className="text-xs text-[var(--muted)]">{zh ? '每组本机探针最多执行 18 秒，发起时需至少剩余 20 秒。停止会中止本次探针并保留已收到的结果；检测不会调整 TUN、DNS 或代理配置。' : 'Local probes run for up to 18 seconds and need at least 20 seconds remaining. Stop aborts this run’s probes while retaining received results. Checks do not change TUN, DNS or proxy settings.'}</p>
    </section>
    <div className="grid min-w-0 grid-cols-1 gap-3 xl:grid-cols-3" aria-live="polite">
      <TaskCard title={zh ? '观测会话' : 'Observation'} status={phases[check.phase]} detail={check.observing ? `${zh ? '剩余' : 'Remaining'} ${check.remainingSeconds}s${check.remainingSeconds === 0 ? (zh ? ' · 等待收尾，最多 20 秒' : ' · finalizing, up to 20 seconds') : ''}` : (zh ? '采集进程、核心与接口证据' : 'Process, core and interface evidence')} active={check.observing} />
      <TaskCard title={zh ? '本机主动探针' : 'Local active probes'} status={nativeLabel} detail="DNS · HTTPS IPv4 · HTTPS IPv6" active={check.probeSubmitting || probeStatus === 'running'} />
      <TaskCard title={zh ? '浏览器探针' : 'Browser probes'} status={browserRunning ? (zh ? '逐项执行中' : 'Running individually') : check.browser ? (check.browser.completed ? (zh ? '已返回结果' : 'Results received') : (zh ? '已中止' : 'Stopped')) : (zh ? '等待发起' : 'Waiting to run')} detail="HTTPS IPv4 · HTTPS IPv6 · WebRTC" active={browserRunning} />
    </div>
    {check.message && <p className="break-words text-sm" role="status">{check.message}</p>}
    {check.preflightError && <Alert type="warning" message={zh ? '预检查不可用' : 'Preflight unavailable'} description={check.preflightError} />}
    {check.error && <Alert type="error" message={zh ? '观测未完整完成' : 'Observation did not complete'} description={check.error} />}
    {check.probeError && <Alert type="warning" message={zh ? '探针请求未完成' : 'Probe request did not complete'} description={check.probeError} />}
    <div className="min-w-0 space-y-1 text-xs text-[var(--muted)]"><p className="break-all">{zh ? '服务采集位置' : 'Service collector'}: {session?.baseURL} · {check.preflight?.platform ?? (zh ? '平台未知' : 'Platform unknown')}</p><p className="break-all">{zh ? '浏览器位置' : 'Browser location'}: {window.location.origin}</p><p>{zh ? '“本机”指 Sempre 服务所在设备。导出 JSON 包含 IP、进程和连接信息；页面不会自动持久保存这些记录。' : '“Local” means the device running Sempre. Exported JSON contains IP, process and connection information; the page does not persist these records automatically.'}</p></div>
    <ActiveProbes report={check.report} submitting={check.probeSubmitting} expectedIpv4={check.runOptions?.expectedIpv4 ?? ''} expectedIpv6={check.runOptions?.expectedIpv6 ?? ''} zh={zh} />
    <section className="min-w-0 space-y-3" aria-label={zh ? '浏览器探针结果' : 'Browser probe results'}>
      <h2 className="text-lg font-semibold">{zh ? '浏览器探针' : 'Browser probes'}</h2>
      <BrowserEvidence report={check.browser} zh={zh} />
    </section>
    <section className="min-w-0 space-y-4 border-t border-[var(--border)] pt-5">
      <div><h2 className="text-lg font-semibold">{zh ? '附加被动观测' : 'Additional passive observation'}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '这里展示真实应用及设备流量。没有启动应用或未触发连接时，可以没有应用记录；不会影响上方主动探针。' : 'Real application and device traffic appears here. Applications that are not running or have no traffic may produce no records; this does not affect active probes above.'}</p></div>
      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">{[{ label: zh ? '物理连接记录' : 'Physical connection records', value: count(summary?.physicalEgress) }, { label: zh ? '代理路由记录' : 'Proxy route records', value: count(summary?.proxyRoute) }, { label: zh ? '直连路由记录' : 'Direct route records', value: count(summary?.directRoute) }, { label: 'IPv4', value: count(summary?.ipv4) }, { label: 'IPv6', value: count(summary?.ipv6) }, { label: zh ? '归属未确认' : 'Unattributed', value: count(summary?.unattributed) }].map((item) => <Card key={item.label} className="min-w-0" bodyStyle={{ padding: '1rem' }}><p className="break-words text-xs text-[var(--muted)]">{item.label}</p><p className="mt-2 text-sm font-semibold">{item.value}</p></Card>)}</div>
      <Tabs className="min-w-0" items={[
        { key: 'application', label: zh ? '真实应用' : 'Real applications', children: <div className="min-w-0 pt-4"><ApplicationEvidence report={check.report} zh={zh} /></div> },
        { key: 'capture', label: zh ? '设备接口' : 'Device interfaces', children: <div className="min-w-0 pt-4"><CaptureEvidence report={check.report} zh={zh} /></div> },
        { key: 'dns', label: 'DNS', children: <div className="min-w-0 pt-4"><DnsEvidence report={check.report} zh={zh} /></div> },
        { key: 'coverage', label: zh ? '采集能力与缺口' : 'Capabilities & gaps', children: <div className="min-w-0 space-y-3 pt-4">
          <Alert type="warning" message={zh ? '未观测不等于没有泄漏' : 'Unobserved does not mean leak-free'} description={zh ? '结论仅限本次观测时段、目标进程和已覆盖协议。核心快照、套接字和物理接口证据的范围不同；缺少证据时保留未知。' : 'Results apply only to this observation window, selected processes and covered protocols. Core snapshots, sockets and physical interface evidence cover different scopes; missing evidence remains unknown.'} />
          {check.preflight?.capabilities.map((capability) => <Card key={capability.id} className="min-w-0" bodyStyle={{ padding: '1rem' }}><div className="flex min-w-0 flex-wrap items-center gap-2 text-sm"><span className="break-all">{capability.id}</span><Tag>{capability.status}</Tag></div><p className="mt-2 break-words text-xs text-[var(--muted)]">{capability.detail}</p></Card>)}
          {gaps.length > 0 && <ul className="list-disc space-y-2 pl-5 text-sm text-[var(--muted)]">{gaps.map((gap) => <li key={gap} className="break-words">{gap}</li>)}</ul>}
        </div> },
      ]} />
    </section>
  </div>
}

function TaskCard({ title, status, detail, active }: { title: string; status: string; detail: string; active: boolean }) {
  return <Card className="min-w-0" bodyStyle={{ padding: '1rem' }}><h2 className="text-sm font-medium">{title}</h2><div className="mt-3"><Tag color={active ? 'processing' : 'default'}>{status}</Tag></div><p className="mt-3 break-words text-xs text-[var(--muted)]">{detail}</p></Card>
}

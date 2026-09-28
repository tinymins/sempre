import { useEffect, useRef, useState, type FormEvent } from 'react'
import { AlertTriangle, CheckCircle2, MinusCircle, RefreshCw, Search, ShieldAlert, XCircle } from 'lucide-react'
import { Alert, Button, Card, Input, Tag } from '@acme/components'
import { streamRequest } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { NetworkDiagnosticFinding, NetworkDiagnosticLayer, NetworkDiagnosticLayerID, NetworkDiagnosticReport, NetworkDiagnosticStatus } from '../lib/types'

const DEFAULT_TARGET = 'https://www.google.com/generate_204'
const LAYER_IDS: NetworkDiagnosticLayerID[] = ['runtime', 'dns', 'route', 'tcp', 'tls', 'http']
const STEP_DELAY_MS = 180

type StepPhase = 'pending' | 'running' | 'completed'
type DiagnosticStep = { id: NetworkDiagnosticLayerID; phase: StepPhase; result?: NetworkDiagnosticLayer }

const initialSteps = (): DiagnosticStep[] => LAYER_IDS.map((id) => ({ id, phase: 'pending' }))

export function NetworkDiagnostics() {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const [draft, setDraft] = useState(DEFAULT_TARGET)
  const [steps, setSteps] = useState(initialSteps)
  const [report, setReport] = useState<NetworkDiagnosticReport | null>(null)
  const [running, setRunning] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const controllerRef = useRef<AbortController | null>(null)
  const zh = locale === 'zh-CN'

  useEffect(() => () => controllerRef.current?.abort(), [])

  const startDiagnostics = (target: string) => {
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    setSteps(initialSteps())
    setReport(null)
    setError(null)
    setRunning(true)

    let terminalReceived = false
    let presentation = Promise.resolve()
    const enqueue = (update: () => void, delay = STEP_DELAY_MS) => {
      presentation = presentation.then(async () => {
        if (controller.signal.aborted) return
        update()
        if (delay > 0) await new Promise((resolve) => window.setTimeout(resolve, delay))
      })
    }

    void (async () => {
      try {
        await streamRequest(session!, '/network/diagnostics', { target }, (event, data) => {
          if (event === 'layer-started') {
            const { layer } = data as { layer: NetworkDiagnosticLayerID }
            enqueue(() => setSteps((current) => current.map((step) => step.id === layer ? { ...step, phase: 'running' } : step)))
          } else if (event === 'layer-completed') {
            const { layer } = data as { layer: NetworkDiagnosticLayer }
            enqueue(() => setSteps((current) => current.map((step) => step.id === layer.id ? { id: step.id, phase: 'completed', result: layer } : step)))
          } else if (event === 'result') {
            terminalReceived = true
            enqueue(() => {
              setReport(data as NetworkDiagnosticReport)
              setRunning(false)
            }, 0)
          } else if (event === 'error') {
            terminalReceived = true
            enqueue(() => {
              setError((data as { message?: string }).message ?? (zh ? '诊断流返回错误' : 'The diagnostic stream returned an error'))
              setRunning(false)
            }, 0)
          }
        }, controller.signal)
        await presentation
        if (!terminalReceived && !controller.signal.aborted) {
          setError(zh ? '诊断流在返回最终结果前结束' : 'The diagnostic stream ended before returning a result')
          setRunning(false)
        }
      } catch (streamError) {
        await presentation
        if (!controller.signal.aborted) {
          setError(streamError instanceof Error ? streamError.message : String(streamError))
          setRunning(false)
        }
      }
    })()
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const target = draft.trim()
    if (target && !running) startDiagnostics(target)
  }

  return <div className="space-y-5">
    <div><h1 className="text-xl font-semibold">{t('networkDiagnostics')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('networkDiagnosticsDetail')}</p></div>
    <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
      <form className="flex flex-col gap-3 sm:flex-row" onSubmit={submit}>
        <label className="min-w-0 flex-1"><span className="mb-2 block text-xs font-medium text-[var(--muted)]">{zh ? '诊断目标' : 'Diagnostic target'}</span><Input value={draft} inputMode="url" placeholder={DEFAULT_TARGET} disabled={running} onChange={(event) => setDraft(event.target.value)} /></label>
        <Button className="self-end" htmlType="submit" variant="primary" disabled={!draft.trim() || running} loading={running} icon={<Search size={16} />}>{zh ? (report ? '重新诊断' : '开始诊断') : (report ? 'Run again' : 'Run diagnostics')}</Button>
      </form>
    </Card>
    {error ? <Alert type="error" showIcon message={zh ? '诊断运行失败' : 'Diagnostics failed'} description={error} /> : null}
    <DiagnosticTimeline steps={steps} zh={zh} />
    {report ? <>
      <DiagnosticSummary report={report} zh={zh} />
      <Findings findings={report.findings} zh={zh} />
    </> : null}
  </div>
}

function DiagnosticTimeline({ steps, zh }: { steps: DiagnosticStep[]; zh: boolean }) {
  return <Card className="!rounded-lg" bodyStyle={{ padding: '1rem 1rem 0' }}><section aria-label={zh ? '分层诊断进度' : 'Layered diagnostic progress'}>{steps.map((step, index) => <div key={step.id} className="flex gap-3"><div className="flex flex-col items-center"><StepIcon step={step} index={index} />{index < steps.length - 1 ? <div className={`my-1 w-0.5 flex-1 ${step.phase === 'completed' ? 'bg-emerald-500/40' : 'bg-[var(--border)]'}`} /> : null}</div><div className="min-w-0 flex-1 pb-5"><div className="flex flex-wrap items-center gap-2"><h2 className="font-semibold">{index + 1}. {layerName(step.id, zh)}</h2><StepStatus step={step} zh={zh} /></div><p className="mt-1 text-sm text-[var(--muted)]">{step.result ? layerSummary(step.result, zh) : step.phase === 'running' ? (zh ? '正在检查这一层…' : 'Checking this layer…') : (zh ? '等待前一层完成' : 'Waiting for the previous layer')}</p>{step.result?.evidence?.length ? <ul className="mt-3 space-y-1 rounded-md bg-[var(--surface-hover)] px-3 py-2 font-mono text-xs"><li className="font-sans font-medium text-[var(--muted)]">{zh ? '证据' : 'Evidence'}</li>{step.result.evidence.map((item) => <li key={item} className="break-all">{item}</li>)}</ul> : null}</div></div>)}</section></Card>
}

function StepIcon({ step, index }: { step: DiagnosticStep; index: number }) {
  if (step.phase === 'pending') return <span className="grid size-8 shrink-0 place-items-center rounded-full bg-zinc-500/10 text-sm font-semibold text-[var(--muted)]">{index + 1}</span>
  if (step.phase === 'running') return <span className="grid size-8 shrink-0 place-items-center rounded-full bg-blue-500/10 text-blue-600"><RefreshCw className="animate-spin" size={16} /></span>
  if (step.result?.status === 'failed') return <span className="grid size-8 shrink-0 place-items-center rounded-full bg-red-500/10 text-red-600"><XCircle size={18} /></span>
  if (step.result?.status === 'warning') return <span className="grid size-8 shrink-0 place-items-center rounded-full bg-amber-500/10 text-amber-600"><AlertTriangle size={17} /></span>
  if (step.result?.status === 'skipped') return <span className="grid size-8 shrink-0 place-items-center rounded-full bg-zinc-500/10 text-[var(--muted)]"><MinusCircle size={18} /></span>
  return <span className="grid size-8 shrink-0 place-items-center rounded-full bg-emerald-500/10 text-emerald-600"><CheckCircle2 size={18} /></span>
}

function StepStatus({ step, zh }: { step: DiagnosticStep; zh: boolean }) {
  if (step.phase === 'pending') return <Tag color="default">{zh ? '等待' : 'Pending'}</Tag>
  if (step.phase === 'running') return <Tag color="processing">{zh ? '检测中' : 'Running'}</Tag>
  return <StatusTag status={step.result?.status ?? 'skipped'} zh={zh} />
}

function DiagnosticSummary({ report, zh }: { report: NetworkDiagnosticReport; zh: boolean }) {
  const failed = report.layers.find((layer) => layer.status === 'failed')
  const healthy = report.status === 'passed'
  return <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}><div className="flex flex-wrap items-start gap-3"><span className={`grid size-10 shrink-0 place-items-center rounded-full ${healthy ? 'bg-emerald-500/10 text-emerald-600' : 'bg-red-500/10 text-red-600'}`}>{healthy ? <CheckCircle2 size={21} /> : <ShieldAlert size={21} />}</span><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><h2 className="font-semibold">{healthy ? (zh ? '网络路径正常' : 'Network path is healthy') : (zh ? `问题定位在${layerName(failed?.id, zh)}层` : `Issue located at the ${layerName(failed?.id, zh)} layer`)}</h2><StatusTag status={report.status} zh={zh} /></div><p className="mt-1 break-all text-xs text-[var(--muted)]">{report.target} · {report.host}:{report.port}</p></div></div></Card>
}

function Findings({ findings, zh }: { findings: NetworkDiagnosticFinding[]; zh: boolean }) {
  if (!findings.length) return <Alert type="success" showIcon message={zh ? '未发现可定位的网络故障' : 'No actionable network issue was found'} />
  return <section aria-label={zh ? '问题与解决方法' : 'Problems and solutions'} className="space-y-3"><div><h2 className="font-semibold">{zh ? '具体问题与解决方法' : 'Problems and solutions'}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '先处理最上面的根因，再重新运行诊断。' : 'Fix the first root cause, then run diagnostics again.'}</p></div>{findings.map((finding) => { const copy = findingCopy(finding, zh); return <Card key={finding.code} className="!rounded-lg !border-amber-500/40" bodyStyle={{ padding: '1rem' }}><div className="flex gap-3"><AlertTriangle className="mt-0.5 shrink-0 text-amber-600" size={19} /><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><h3 className="font-semibold">{copy.title}</h3><code className="rounded bg-[var(--surface-hover)] px-1.5 py-0.5 text-[11px]">{finding.code}</code></div><p className="mt-2 break-words text-sm">{copy.detail}</p><h4 className="mt-4 text-xs font-semibold uppercase tracking-wide text-[var(--muted)]">{zh ? '建议' : 'Recommended actions'}</h4><ol className="mt-2 list-decimal space-y-1 pl-5 text-sm">{copy.solutions.map((solution) => <li key={solution}>{solution}</li>)}</ol></div></div></Card> })}</section>
}

function StatusTag({ status, zh }: { status: NetworkDiagnosticStatus; zh: boolean }) {
  const labels = zh ? { passed: '通过', failed: '失败', warning: '警告', skipped: '跳过' } : { passed: 'Passed', failed: 'Failed', warning: 'Warning', skipped: 'Skipped' }
  const colors = { passed: 'success', failed: 'error', warning: 'warning', skipped: 'default' } as const
  return <Tag color={colors[status]}>{labels[status]}</Tag>
}

function layerName(id: string | undefined, zh: boolean) {
  const names: Record<string, [string, string]> = { runtime: ['运行时', 'runtime'], dns: ['DNS', 'DNS'], route: ['路由 / TUN', 'route / TUN'], tcp: ['TCP', 'TCP'], tls: ['TLS', 'TLS'], http: ['HTTP', 'HTTP'] }
  return names[id ?? '']?.[zh ? 0 : 1] ?? (zh ? '未知' : 'unknown')
}

function layerSummary(layer: NetworkDiagnosticLayer, zh: boolean) {
  if (!zh) return layer.summary
  const text: Record<string, Partial<Record<NetworkDiagnosticStatus, string>>> = {
    runtime: { passed: 'Sempre 受管核心正在运行。', failed: 'Sempre 受管核心没有运行。' }, dns: { passed: '域名已解析，下面继续检查地址类型和选路。', failed: '域名没有解析出可用地址。' }, route: { passed: '目标地址存在明确路由。', failed: 'Sempre 管理的 FakeIP 地址空间被多个 TUN 分割接管。', warning: '当前平台无法取得目标路由。' }, tcp: { passed: '目标端口已建立 TCP 连接。', failed: '所有目标地址的 TCP 连接均失败。' }, tls: { passed: 'TLS 握手和证书校验成功。', failed: 'TCP 已连接，但 TLS 握手失败。', skipped: '前置层失败，未执行 TLS。' }, http: { passed: `已收到 ${layer.summary} 响应。`, failed: '没有收到 HTTP 响应。', skipped: '前置层失败，未执行 HTTP。' },
  }
  return text[layer.id]?.[layer.status] ?? layer.summary
}

function findingCopy(finding: NetworkDiagnosticFinding, zh: boolean) {
  if (!zh) return finding
  const copies: Record<string, { title: string; detail: string; solutions: string[] }> = {
    runtime_not_running: { title: 'Sempre 核心没有运行', detail: '透明代理与 FakeIP 流量没有运行中的核心可以接管。', solutions: ['打开“核心状态”查看最近一次失败', '先修复核心启动错误，再重新运行网络诊断'] }, dns_failure: { title: 'DNS 解析失败', detail: finding.detail, solutions: ['检查系统当前 DNS 所有者和 Sempre DNS 状态', '确认配置的上游 DNS 可以从当前网络访问'] }, fake_ip_route_conflict: { title: '多个 TUN 同时接管 FakeIP 网段', detail: finding.detail, solutions: ['不要同时运行 FakeIP 网段重叠的透明代理或 SASE 客户端', '必须共存时，为 Sempre 设置不重叠的 FakeIP 网段并同步路由', '不要手工删除受管路由，所属客户端通常会重新写回'] }, tcp_failure: { title: 'TCP 连接失败', detail: 'DNS 已完成，但目标端口无法建立连接。', solutions: ['检查证据中的目标路由、防火墙和出口限制', '在“节点测试”中检查当前代理节点及其上游端点'] }, tls_failure: { title: 'TLS 握手失败', detail: finding.detail, solutions: ['优先解决前面报告的 TUN/FakeIP 路由冲突', '检查代理节点健康、TLS 中间人证书和路径 MTU'] }, http_failure: { title: 'HTTP 请求失败', detail: finding.detail, solutions: ['检查目标返回和实际命中的分流规则', '对比目标在直连与代理路径下的结果'] },
  }
  return copies[finding.code] ?? finding
}

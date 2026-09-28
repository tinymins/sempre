import { useState, type FormEvent } from 'react'
import { useQuery } from '@tanstack/react-query'
import { AlertTriangle, CheckCircle2, RefreshCw, Search, ShieldAlert } from 'lucide-react'
import { Alert, Button, Card, Input, Tag } from '@acme/components'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { NetworkDiagnosticFinding, NetworkDiagnosticLayer, NetworkDiagnosticReport, NetworkDiagnosticStatus } from '../lib/types'

const DEFAULT_TARGET = 'https://www.google.com/generate_204'

export function NetworkDiagnostics() {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const [draft, setDraft] = useState(DEFAULT_TARGET)
  const [target, setTarget] = useState(DEFAULT_TARGET)
  const report = useQuery({
    queryKey: ['network', 'diagnostics', target],
    queryFn: () => api<NetworkDiagnosticReport>(session!, '/network/diagnostics', {
      method: 'POST',
      body: JSON.stringify({ target }),
    }),
    retry: false,
  })
  const zh = locale === 'zh-CN'
  const submit = (event: FormEvent) => {
    event.preventDefault()
    const value = draft.trim()
    if (value === target) void report.refetch()
    else setTarget(value)
  }

  return <div className="space-y-5">
    <div><h1 className="text-xl font-semibold">{t('networkDiagnostics')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('networkDiagnosticsDetail')}</p></div>
    <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
      <form className="flex flex-col gap-3 sm:flex-row" onSubmit={submit}>
        <label className="min-w-0 flex-1"><span className="mb-2 block text-xs font-medium text-[var(--muted)]">{zh ? '诊断目标' : 'Diagnostic target'}</span><Input value={draft} inputMode="url" placeholder={DEFAULT_TARGET} onChange={(event) => setDraft(event.target.value)} /></label>
        <Button className="self-end" htmlType="submit" variant="primary" disabled={!draft.trim()} loading={report.isFetching} icon={<Search size={16} />}>{zh ? '开始诊断' : 'Run diagnostics'}</Button>
      </form>
    </Card>
    {report.isError ? <Alert type="error" showIcon message={zh ? '诊断运行失败' : 'Diagnostics failed'} description={report.error instanceof Error ? report.error.message : String(report.error)} /> : null}
    {report.data ? <>
      <DiagnosticSummary report={report.data} zh={zh} refreshing={report.isFetching} onRefresh={() => report.refetch()} />
      <section aria-label={zh ? '分层诊断结果' : 'Layered diagnostic results'} className="space-y-3">
        {report.data.layers.map((layer, index) => <LayerResult key={layer.id} layer={layer} index={index} zh={zh} />)}
      </section>
      <Findings findings={report.data.findings} zh={zh} />
    </> : report.isFetching ? <Card className="!rounded-lg" bodyStyle={{ padding: '2rem' }}><div className="flex items-center justify-center gap-2 text-sm text-[var(--muted)]"><RefreshCw className="animate-spin" size={17} />{zh ? '正在逐层检查网络路径…' : 'Checking the network path layer by layer…'}</div></Card> : null}
  </div>
}

function DiagnosticSummary({ report, zh, refreshing, onRefresh }: { report: NetworkDiagnosticReport; zh: boolean; refreshing: boolean; onRefresh: () => unknown }) {
  const failed = report.layers.find((layer) => layer.status === 'failed')
  const healthy = report.status === 'passed'
  return <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}><div className="flex flex-wrap items-start gap-3"><span className={`grid size-10 shrink-0 place-items-center rounded-full ${healthy ? 'bg-emerald-500/10 text-emerald-600' : 'bg-red-500/10 text-red-600'}`}>{healthy ? <CheckCircle2 size={21} /> : <ShieldAlert size={21} />}</span><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><h2 className="font-semibold">{healthy ? (zh ? '网络路径正常' : 'Network path is healthy') : (zh ? `问题定位在${layerName(failed?.id, zh)}层` : `Issue located at the ${layerName(failed?.id, zh)} layer`)}</h2><StatusTag status={report.status} zh={zh} /></div><p className="mt-1 break-all text-xs text-[var(--muted)]">{report.target} · {report.host}:{report.port}</p></div><Button size="small" disabled={refreshing} icon={<RefreshCw className={refreshing ? 'animate-spin' : ''} size={15} />} onClick={onRefresh}>{zh ? '重新检查' : 'Run again'}</Button></div></Card>
}

function LayerResult({ layer, index, zh }: { layer: NetworkDiagnosticLayer; index: number; zh: boolean }) {
  const failed = layer.status === 'failed'
  return <Card className={`!rounded-lg ${failed ? '!border-red-500/40' : ''}`} bodyStyle={{ padding: '1rem' }}><div className="flex items-start gap-3"><span className={`grid size-8 shrink-0 place-items-center rounded-full text-sm font-semibold ${statusTone(layer.status)}`}>{index + 1}</span><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><h3 className="font-semibold">{layerName(layer.id, zh)}</h3><StatusTag status={layer.status} zh={zh} /></div><p className="mt-1 text-sm text-[var(--muted)]">{layerSummary(layer, zh)}</p>{layer.evidence?.length ? <ul className="mt-3 space-y-1 rounded-md bg-[var(--surface-hover)] px-3 py-2 font-mono text-xs"><li className="font-sans font-medium text-[var(--muted)]">{zh ? '证据' : 'Evidence'}</li>{layer.evidence.map((item) => <li key={item} className="break-all">{item}</li>)}</ul> : null}</div></div></Card>
}

function Findings({ findings, zh }: { findings: NetworkDiagnosticFinding[]; zh: boolean }) {
  if (!findings.length) return <Alert type="success" showIcon message={zh ? '未发现可定位的网络故障' : 'No actionable network issue was found'} />
  return <section aria-label={zh ? '问题与解决方法' : 'Problems and solutions'} className="space-y-3"><div><h2 className="font-semibold">{zh ? '具体问题与解决方法' : 'Problems and solutions'}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '先处理最上面的根因，再重新运行诊断。' : 'Fix the first root cause, then run diagnostics again.'}</p></div>{findings.map((finding) => {
    const copy = findingCopy(finding, zh)
    return <Card key={finding.code} className="!rounded-lg !border-amber-500/40" bodyStyle={{ padding: '1rem' }}><div className="flex gap-3"><AlertTriangle className="mt-0.5 shrink-0 text-amber-600" size={19} /><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><h3 className="font-semibold">{copy.title}</h3><code className="rounded bg-[var(--surface-hover)] px-1.5 py-0.5 text-[11px]">{finding.code}</code></div><p className="mt-2 break-words text-sm">{copy.detail}</p><h4 className="mt-4 text-xs font-semibold uppercase tracking-wide text-[var(--muted)]">{zh ? '建议' : 'Recommended actions'}</h4><ol className="mt-2 list-decimal space-y-1 pl-5 text-sm">{copy.solutions.map((solution) => <li key={solution}>{solution}</li>)}</ol></div></div></Card>
  })}</section>
}

function StatusTag({ status, zh }: { status: NetworkDiagnosticStatus; zh: boolean }) {
  const labels = zh ? { passed: '通过', failed: '失败', warning: '警告', skipped: '跳过' } : { passed: 'Passed', failed: 'Failed', warning: 'Warning', skipped: 'Skipped' }
  const colors = { passed: 'success', failed: 'error', warning: 'warning', skipped: 'default' } as const
  return <Tag color={colors[status]}>{labels[status]}</Tag>
}

function statusTone(status: NetworkDiagnosticStatus) {
  if (status === 'passed') return 'bg-emerald-500/10 text-emerald-600'
  if (status === 'failed') return 'bg-red-500/10 text-red-600'
  if (status === 'warning') return 'bg-amber-500/10 text-amber-600'
  return 'bg-zinc-500/10 text-[var(--muted)]'
}

function layerName(id: string | undefined, zh: boolean) {
  const names: Record<string, [string, string]> = { runtime: ['运行时', 'runtime'], dns: ['DNS', 'DNS'], route: ['路由 / TUN', 'route / TUN'], tcp: ['TCP', 'TCP'], tls: ['TLS', 'TLS'], http: ['HTTP', 'HTTP'] }
  return names[id ?? '']?.[zh ? 0 : 1] ?? (zh ? '未知' : 'unknown')
}

function layerSummary(layer: NetworkDiagnosticLayer, zh: boolean) {
  if (!zh) return layer.summary
  const text: Record<string, Partial<Record<NetworkDiagnosticStatus, string>>> = {
    runtime: { passed: 'Sempre 受管核心正在运行。', failed: 'Sempre 受管核心没有运行。' },
    dns: { passed: '域名已解析，下面继续检查地址类型和选路。', failed: '域名没有解析出可用地址。' },
    route: { passed: '目标地址存在明确路由。', failed: 'Sempre 管理的 FakeIP 地址空间被多个 TUN 分割接管。', warning: '当前平台无法取得目标路由。' },
    tcp: { passed: '目标端口已建立 TCP 连接。', failed: '所有目标地址的 TCP 连接均失败。' },
    tls: { passed: 'TLS 握手和证书校验成功。', failed: 'TCP 已连接，但 TLS 握手失败。', skipped: '前置层失败，未执行 TLS。' },
    http: { passed: `已收到 ${layer.summary} 响应。`, failed: '没有收到 HTTP 响应。', skipped: '前置层失败，未执行 HTTP。' },
  }
  return text[layer.id]?.[layer.status] ?? layer.summary
}

function findingCopy(finding: NetworkDiagnosticFinding, zh: boolean) {
  if (!zh) return finding
  const copies: Record<string, { title: string; detail: string; solutions: string[] }> = {
    runtime_not_running: { title: 'Sempre 核心没有运行', detail: '透明代理与 FakeIP 流量没有运行中的核心可以接管。', solutions: ['打开“核心状态”查看最近一次失败', '先修复核心启动错误，再重新运行网络诊断'] },
    dns_failure: { title: 'DNS 解析失败', detail: finding.detail, solutions: ['检查系统当前 DNS 所有者和 Sempre DNS 状态', '确认配置的上游 DNS 可以从当前网络访问'] },
    fake_ip_route_conflict: { title: '多个 TUN 同时接管 FakeIP 网段', detail: finding.detail, solutions: ['不要同时运行 FakeIP 网段重叠的透明代理或 SASE 客户端', '必须共存时，为 Sempre 设置不重叠的 FakeIP 网段并同步路由', '不要手工删除受管路由，所属客户端通常会重新写回'] },
    tcp_failure: { title: 'TCP 连接失败', detail: 'DNS 已完成，但目标端口无法建立连接。', solutions: ['检查证据中的目标路由、防火墙和出口限制', '在“节点测试”中检查当前代理节点及其上游端点'] },
    tls_failure: { title: 'TLS 握手失败', detail: finding.detail, solutions: ['优先解决前面报告的 TUN/FakeIP 路由冲突', '检查代理节点健康、TLS 中间人证书和路径 MTU'] },
    http_failure: { title: 'HTTP 请求失败', detail: finding.detail, solutions: ['检查目标返回和实际命中的分流规则', '对比目标在直连与代理路径下的结果'] },
  }
  return copies[finding.code] ?? finding
}

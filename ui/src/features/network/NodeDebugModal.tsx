import { useEffect, useState, type ReactNode } from 'react'
import { CheckCircle2, Circle, CircleSlash2, Globe2, LoaderCircle, MapPin, Network, RotateCw, XCircle } from 'lucide-react'
import { Button, Modal, Tag } from '@acme/components'
import { streamRequest } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'

type StepState = 'waiting' | 'running' | 'succeeded' | 'failed' | 'skipped'

interface DebugStep {
  id: string
  label: string
  state: StepState
  duration_ms?: number
  data?: {
    url?: string
    status?: number
    bytes?: number
    domain?: string
    resolver?: string
    answers?: string[]
    ip?: string
    metadata?: IpMetadata
    metadata_error?: string
    delay?: number
    method?: string
    route_scope?: 'default' | 'private'
    allowed_ips?: string[]
  }
  error?: string
  message?: string
}

interface IpMetadata {
  country_code?: string
  country?: string
  region?: string
  city?: string
  asn?: number
  asn_organization?: string
  isp?: string
  organization?: string
}

export function NodeDebugModal({ node, nodeType, open, onClose }: { node?: string; nodeType?: string; open: boolean; onClose: () => void }) {
  const { locale, t } = useI18n()
  const { session } = useSession()
  const [steps, setSteps] = useState<DebugStep[]>([])
  const [finished, setFinished] = useState(false)
  const [fatalError, setFatalError] = useState<string>()
  const [run, setRun] = useState(0)

  useEffect(() => {
    if (!open || !node || !session) return
    const controller = new AbortController()
    void streamRequest(session, '/runtime/nodes/debug', { name: node }, (event, payload) => {
      if (event === 'step') {
        const step = payload as DebugStep
        setSteps((current) => {
          const index = current.findIndex((item) => item.id === step.id)
          if (index < 0) return [...current, step]
          return current.map((item) => item.id === step.id ? step : item)
        })
      } else if (event === 'done') {
        setFinished(true)
      } else if (event === 'error') {
        setFatalError((payload as { message?: string }).message || t('operationFailed'))
      }
    }, controller.signal).catch((error) => {
      if (!controller.signal.aborted) setFatalError(error instanceof Error ? error.message : t('operationFailed'))
    })
    return () => controller.abort()
  }, [node, open, run, session, t])

  const restart = () => {
    setSteps([])
    setFinished(false)
    setFatalError(undefined)
    setRun((value) => value + 1)
  }

  const wireguard = nodeType?.toLowerCase() === 'wireguard'
  const labels = stepLabels(locale, wireguard)
  const visibleSteps = steps.length ? steps : [{
    id: 'prepare',
    label: labels.prepare,
    state: 'waiting' as const,
  }]

  return <Modal
    open={open}
    width={720}
    title={`${locale === 'zh-CN' ? '节点调试' : 'Node diagnostics'} · ${node || ''}`}
    onCancel={onClose}
    footer={<div className="flex justify-end gap-2 pt-4">
      <Button icon={<RotateCw />} disabled={!finished && !fatalError} onClick={restart}>{locale === 'zh-CN' ? '重新测试' : 'Run again'}</Button>
      <Button variant="primary" onClick={onClose}>{t('close')}</Button>
    </div>}
  >
    <div className="space-y-3">
      <div className="rounded-lg border border-black/[0.06] bg-black/[0.02] px-4 py-3 dark:border-white/[0.08] dark:bg-white/[0.03]">
        <div className="flex items-center justify-between gap-3">
          <span className="min-w-0 truncate font-medium" title={node}>{node}</span>
          <Tag color={fatalError ? 'error' : finished ? 'success' : 'processing'}>
            {fatalError ? t('failed') : finished ? (locale === 'zh-CN' ? '测试完成' : 'Completed') : (locale === 'zh-CN' ? '测试中' : 'Testing')}
          </Tag>
        </div>
        <p className="mt-1 text-xs text-[var(--muted)]">
          {wireguard
            ? (locale === 'zh-CN' ? '仅复用当前 Core 已加载的 WireGuard endpoint；不会创建临时接口或修改机器路由。' : 'Reuses only the WireGuard endpoint loaded by the current Core. No temporary interface or host route is created.')
            : (locale === 'zh-CN' ? '所有请求都经过该节点的隔离 Core 实例，不会切换当前使用中的代理节点。' : 'Every request uses an isolated Core instance pinned to this node. The active proxy selection is unchanged.')}
        </p>
      </div>
      <div className="overflow-hidden rounded-lg border border-black/[0.06] dark:border-white/[0.08]">
        {visibleSteps.map((step, index) => <StepRow key={step.id} step={{ ...step, label: labels[step.id] || step.label }} last={index === visibleSteps.length - 1} locale={locale} />)}
      </div>
      {fatalError ? <div className="rounded-lg border border-red-500/30 bg-red-500/8 px-4 py-3 text-sm text-red-600 dark:text-red-400">{fatalError}</div> : null}
    </div>
  </Modal>
}

function StepRow({ step, last, locale }: { step: DebugStep; last: boolean; locale: 'zh-CN' | 'en' }) {
  const Icon = step.state === 'running' ? LoaderCircle : step.state === 'succeeded' ? CheckCircle2 : step.state === 'failed' ? XCircle : step.state === 'skipped' ? CircleSlash2 : Circle
  const iconColor = step.state === 'running'
    ? 'text-sky-500'
    : step.state === 'succeeded'
      ? 'text-emerald-500'
      : step.state === 'failed'
        ? 'text-red-500'
        : step.state === 'skipped'
          ? 'text-amber-500'
        : 'text-[var(--muted)] opacity-40'
  return <div className={`flex gap-3 px-4 py-3 ${last ? '' : 'border-b border-black/[0.06] dark:border-white/[0.08]'}`}>
    <Icon className={`mt-0.5 size-4 shrink-0 ${iconColor} ${step.state === 'running' ? 'animate-spin' : ''}`} />
    <div className="min-w-0 flex-1">
      <div className="flex items-center justify-between gap-3">
        <span className="text-sm font-medium">{step.label}</span>
        {step.duration_ms !== undefined ? <span className="shrink-0 text-xs tabular-nums text-[var(--muted)]">{step.duration_ms} ms</span> : null}
      </div>
      <StepResult step={step} locale={locale} />
    </div>
  </div>
}

function StepResult({ step, locale }: { step: DebugStep; locale: 'zh-CN' | 'en' }) {
  if (step.state === 'waiting') return <p className="mt-1 text-xs text-[var(--muted)]">{locale === 'zh-CN' ? '等待执行' : 'Waiting'}</p>
  if (step.state === 'running') return <p className="mt-1 text-xs text-sky-600 dark:text-sky-400">{locale === 'zh-CN' ? '正在执行…' : 'Running…'}</p>
  if (step.state === 'skipped') return <div className="mt-1 text-xs text-amber-700 dark:text-amber-400">
    <p>{localizedReason(step.message, locale)}</p>
    {step.data?.allowed_ips?.length ? <p className="mt-1 break-all font-mono text-[11px] text-[var(--muted)]">AllowedIPs · {step.data.allowed_ips.join(' · ')}</p> : null}
  </div>
  if (step.error) return <p className="mt-1 break-words text-xs text-red-600 dark:text-red-400">{step.error}</p>
  const data = step.data
  if (!data) return null
  if (data.allowed_ips?.length) return <div className="mt-1 text-xs text-[var(--muted)]">
    <p className="font-medium text-[var(--text)]">{data.route_scope === 'default' ? (locale === 'zh-CN' ? '默认路由型 WireGuard' : 'Default-route WireGuard') : (locale === 'zh-CN' ? '仅私网 WireGuard' : 'Private-only WireGuard')}</p>
    <p className="mt-1 break-all font-mono text-[11px]">AllowedIPs · {data.allowed_ips.join(' · ')}</p>
  </div>
  if (data.ip) return <IpResultCard step={step} locale={locale} />
  if (data.domain) {
    return <div className="mt-1 text-xs text-[var(--muted)]">
      <span>DoH · {data.resolver} · RCODE {data.status}</span>
      <p className="mt-0.5 break-all text-[var(--text)]">{data.answers?.length ? data.answers.join(' · ') : (locale === 'zh-CN' ? '无应答记录' : 'No answer records')}</p>
    </div>
  }
  if (data.delay !== undefined) return <p className="mt-1 break-all text-xs text-[var(--muted)]">URL-test · {data.delay} ms · {data.method || 'HEAD'} · {data.url}</p>
  return <p className="mt-1 break-all text-xs text-[var(--muted)]">HTTP {data.status} · {formatBytes(data.bytes || 0)} · {data.url}</p>
}

function IpResultCard({ step, locale }: { step: DebugStep; locale: 'zh-CN' | 'en' }) {
  const data = step.data!
  const metadata = data.metadata
  const domestic = step.id === 'domestic-ip'
  const location = [metadata?.country, metadata?.region, metadata?.city].filter(Boolean).join(' · ')
  const network = metadata?.isp || metadata?.organization || metadata?.asn_organization
  const asn = metadata?.asn ? `AS${metadata.asn}` : undefined
  return <div className="mt-2 overflow-hidden rounded-lg border border-black/[0.06] bg-black/[0.02] dark:border-white/[0.08] dark:bg-white/[0.03]">
    <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2.5">
      <div className="flex min-w-0 items-center gap-2">
        <Globe2 className={`size-4 shrink-0 ${domestic ? 'text-cyan-500' : 'text-blue-500'}`} />
        <span className="break-all font-mono text-sm font-semibold tracking-tight text-[var(--text)]">{data.ip}</span>
      </div>
      <div className="flex items-center gap-1.5">
        {asn ? <Tag color="purple" size="small">{asn}</Tag> : null}
        <Tag color={domestic ? 'cyan' : 'blue'} size="small">
          {domestic ? (locale === 'zh-CN' ? '国内探测' : 'Domestic probe') : (locale === 'zh-CN' ? '国外探测' : 'Foreign probe')}
        </Tag>
      </div>
    </div>
    {location || network ? <div className="grid gap-2 border-t border-black/[0.05] px-3 py-2.5 text-xs dark:border-white/[0.06] sm:grid-cols-2">
      <IpDetail icon={<MapPin />} label={locale === 'zh-CN' ? '出口位置' : 'Location'} value={`${countryFlag(metadata?.country_code)}${location || '—'}`} />
      <IpDetail icon={<Network />} label={locale === 'zh-CN' ? '网络归属' : 'Network'} value={network || metadata?.asn_organization || '—'} />
    </div> : null}
    <div className="border-t border-black/[0.05] px-3 py-2 text-[11px] text-[var(--muted)] dark:border-white/[0.06]">
      <span>{locale === 'zh-CN' ? '探测来源' : 'Source'} · {sourceHost(data.url)}</span>
      {metadata ? <span> · ASN via api.ip.sb</span> : null}
      {data.metadata_error ? <span className="block break-words text-amber-600 dark:text-amber-400">{locale === 'zh-CN' ? 'ASN 信息暂不可用' : 'ASN metadata unavailable'} · {data.metadata_error}</span> : null}
    </div>
  </div>
}

function IpDetail({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return <div className="flex min-w-0 items-start gap-2">
    <span className="mt-0.5 shrink-0 text-[var(--muted)] [&>svg]:size-3.5">{icon}</span>
    <div className="min-w-0">
      <p className="text-[10px] uppercase tracking-wide text-[var(--muted)]">{label}</p>
      <p className="mt-0.5 break-words font-medium text-[var(--text)]">{value}</p>
    </div>
  </div>
}

function countryFlag(code?: string) {
  if (!code || !/^[a-z]{2}$/i.test(code)) return ''
  return `${String.fromCodePoint(...code.toUpperCase().split('').map((letter) => 127397 + letter.charCodeAt(0)))} `
}

function sourceHost(url?: string) {
  if (!url) return '—'
  try {
    return new URL(url).host
  } catch {
    return url
  }
}

function stepLabels(locale: 'zh-CN' | 'en', wireguard: boolean): Record<string, string> {
  if (wireguard) return locale === 'zh-CN'
    ? { prepare: '复用当前 WireGuard', probe: '节点探活', 'private-probe': '私网连通', 'public-ip': '公网出口 IP', 'domestic-ip': '国内出口 IP', 'foreign-ip': '国外出口 IP', 'http-baidu': 'URL-test · www.baidu.com', 'http-google': 'URL-test · www.google.com' }
    : { prepare: 'Reuse current WireGuard', probe: 'Node liveness', 'private-probe': 'Private reachability', 'public-ip': 'Public exit IP', 'domestic-ip': 'Domestic exit IP', 'foreign-ip': 'Foreign exit IP', 'http-baidu': 'URL-test · www.baidu.com', 'http-google': 'URL-test · www.google.com' }
  return locale === 'zh-CN'
    ? { prepare: '启动隔离 Core', probe: '节点探活', 'private-probe': '私网连通', 'domestic-ip': '国内出口 IP', 'foreign-ip': '国外出口 IP', 'dns-baidu': 'DNS · www.baidu.com', 'dns-google': 'DNS · www.google.com', 'http-baidu': 'HTTP · www.baidu.com', 'http-google': 'HTTP · www.google.com' }
    : { prepare: 'Start isolated Core', probe: 'Node liveness', 'private-probe': 'Private reachability', 'domestic-ip': 'Domestic exit IP', 'foreign-ip': 'Foreign exit IP', 'dns-baidu': 'DNS · www.baidu.com', 'dns-google': 'DNS · www.google.com', 'http-baidu': 'HTTP · www.baidu.com', 'http-google': 'HTTP · www.google.com' }
}

function localizedReason(message: string | undefined, locale: 'zh-CN' | 'en') {
  if (!message) return locale === 'zh-CN' ? '该步骤不适用，已跳过。' : 'This step does not apply and was skipped.'
  if (locale === 'en') return message
  if (message.startsWith('This WireGuard endpoint has private-only')) return '该 WireGuard 仅包含私网 AllowedIPs，且未配置 HTTPS 私网健康地址；未发送任何测试流量。'
  if (message.startsWith('Private WireGuard routing')) return '私网 WireGuard 不是公网出口，公网 IP 与 ASN 探测不适用。'
  if (message.startsWith('sing-box URL-test only returns')) return 'sing-box URL-test 仅返回延迟，无法读取 IP 与 ASN 探测所需的响应正文。'
  return message
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  return `${(bytes / 1024).toFixed(1)} KiB`
}

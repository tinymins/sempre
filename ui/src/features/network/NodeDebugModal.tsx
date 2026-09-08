import { useEffect, useState } from 'react'
import { CheckCircle2, Circle, LoaderCircle, RotateCw, XCircle } from 'lucide-react'
import { Button, Modal, Tag } from '@acme/components'
import { streamRequest } from '../../lib/api'
import { useI18n } from '../../lib/i18n'
import { useSession } from '../../lib/session'

type StepState = 'waiting' | 'running' | 'succeeded' | 'failed'

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
  }
  error?: string
}

const stepIds = ['prepare', 'probe', 'dns-baidu', 'dns-google', 'http-baidu', 'http-google']

export function NodeDebugModal({ node, open, onClose }: { node?: string; open: boolean; onClose: () => void }) {
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

  const labels = stepLabels(locale)
  const visibleSteps = stepIds.map((id) => steps.find((step) => step.id === id) || {
    id,
    label: labels[id],
    state: 'waiting' as const,
  })

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
          {locale === 'zh-CN' ? '所有请求都经过该节点的隔离 Core 实例，不会切换当前使用中的代理节点。' : 'Every request uses an isolated Core instance pinned to this node. The active proxy selection is unchanged.'}
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
  const Icon = step.state === 'running' ? LoaderCircle : step.state === 'succeeded' ? CheckCircle2 : step.state === 'failed' ? XCircle : Circle
  const iconColor = step.state === 'running'
    ? 'text-sky-500'
    : step.state === 'succeeded'
      ? 'text-emerald-500'
      : step.state === 'failed'
        ? 'text-red-500'
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
  if (step.error) return <p className="mt-1 break-words text-xs text-red-600 dark:text-red-400">{step.error}</p>
  const data = step.data
  if (!data) return null
  if (data.domain) {
    return <div className="mt-1 text-xs text-[var(--muted)]">
      <span>DoH · {data.resolver} · RCODE {data.status}</span>
      <p className="mt-0.5 break-all text-[var(--text)]">{data.answers?.length ? data.answers.join(' · ') : (locale === 'zh-CN' ? '无应答记录' : 'No answer records')}</p>
    </div>
  }
  return <p className="mt-1 break-all text-xs text-[var(--muted)]">HTTP {data.status} · {formatBytes(data.bytes || 0)} · {data.url}</p>
}

function stepLabels(locale: 'zh-CN' | 'en'): Record<string, string> {
  return locale === 'zh-CN'
    ? { prepare: '启动隔离 Core', probe: '节点探活', 'dns-baidu': 'DNS · www.baidu.com', 'dns-google': 'DNS · www.google.com', 'http-baidu': 'HTTP · www.baidu.com', 'http-google': 'HTTP · www.google.com' }
    : { prepare: 'Start isolated Core', probe: 'Node liveness', 'dns-baidu': 'DNS · www.baidu.com', 'dns-google': 'DNS · www.google.com', 'http-baidu': 'HTTP · www.baidu.com', 'http-google': 'HTTP · www.google.com' }
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  return `${(bytes / 1024).toFixed(1)} KiB`
}

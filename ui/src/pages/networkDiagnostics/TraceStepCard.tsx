import { useId, useState } from 'react'
import { AlertTriangle, Check, ChevronDown, Circle, X } from 'lucide-react'
import { Button, Tag } from '@acme/components'
import type { NetworkTraceStep } from '../../lib/networkDiagnosticsTypes'

export function TraceStepCard({ step, zh, preview = false, active = false }: {
  step: NetworkTraceStep
  zh: boolean
  preview?: boolean
  active?: boolean
}) {
  const [expanded, setExpanded] = useState(false)
  const detailId = useId()
  const unfinished = step.status === 'running' && !active
  const color = preview || step.status === 'running' ? 'text-[var(--muted)] bg-zinc-500/10' : step.status === 'failed' ? 'text-red-600 bg-red-500/10'
    : step.status === 'unknown' ? 'text-amber-600 bg-amber-500/10' : 'text-emerald-600 bg-emerald-500/10'
  const sources = zh
    ? { session: '诊断会话', frontend: 'Sempre 前置', core: '内核 API', socket: 'Socket / TLS', http: 'HTTP 响应', config: '配置推演' }
    : { session: 'Diagnostic session', frontend: 'Sempre frontend', core: 'Core API', socket: 'Socket / TLS', http: 'HTTP response', config: 'Configuration prediction' }
  const states = zh ? { running: '进行中', passed: '已记录', failed: '失败', unknown: '证据缺失' }
    : { running: 'Running', passed: 'Observed', failed: 'Failed', unknown: 'Evidence unavailable' }
  return <li className="relative pl-8 pb-4 last:pb-0 motion-safe:animate-[trace-arrive_200ms_ease-out_both] before:absolute before:top-7 before:bottom-0 before:left-2.5 before:w-px before:bg-[var(--border)] last:before:hidden">
    <span className={`absolute left-0 top-3 grid size-5 place-items-center rounded-full ${color}`} aria-hidden="true">
      {preview || unfinished ? <Circle size={11} /> : step.status === 'running' ? <span className="size-2 rounded-full bg-current motion-safe:animate-pulse" /> : step.status === 'passed' ? <Check size={13} /> : step.status === 'failed' ? <X size={13} /> : <AlertTriangle size={12} />}
    </span>
    <div className="overflow-hidden rounded-lg border border-[var(--border)]">
      <Button variant="unstyled" className="w-full !justify-start gap-3 px-3 py-3 text-left hover:bg-[var(--surface-hover)]" aria-expanded={expanded} aria-controls={detailId} onClick={() => setExpanded(!expanded)}>
        <span className="min-w-0 flex-1">
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1"><span className="text-sm font-medium">{step.title}</span><span className="text-[11px] text-[var(--muted)]">{sources[step.source]}</span></span>
          <span className="mt-1 block break-words text-xs text-[var(--muted)]">{step.detail}</span>
        </span>
        <span className="flex shrink-0 flex-col items-end gap-1">
          <span className={`rounded px-1.5 py-0.5 text-[10px] ${color}`}>{preview ? (zh ? '预测' : 'Predicted') : unfinished ? (zh ? '未完成' : 'Unfinished') : states[step.status]}</span>
          {!preview && Number.isFinite(step.elapsed_ms) ? <span className="text-[10px] tabular-nums text-[var(--muted)]">+{step.elapsed_ms} ms</span> : null}
        </span>
        <ChevronDown size={15} className={`shrink-0 text-[var(--muted)] motion-safe:transition-transform motion-safe:duration-200 ${expanded ? 'rotate-180' : ''}`} />
      </Button>
      {!preview && step.source === 'core' && (step.rule || step.chains?.length) ? <div className="space-y-2 px-3 pb-3 text-xs">
        {step.rule ? <div className="flex flex-wrap items-baseline gap-2"><span className="text-[var(--muted)]">{zh ? '实际命中规则' : 'Matched rule'}</span><code className="break-all rounded bg-[var(--surface-hover)] px-2 py-1">{step.rule.type}{step.rule.payload ? ` · ${step.rule.payload}` : ''}</code></div> : null}
        {step.chains?.length ? <div><p className="mb-1.5 text-[11px] text-[var(--muted)]">{zh ? '内核返回链（API 顺序）· 策略组与节点' : 'Core chain (API order) · policy groups and nodes'}</p><div className="flex flex-wrap gap-1.5">{step.chains.map((name, index) => <span key={`${index}-${name}`} className="max-w-full break-words rounded-md bg-emerald-500/10 px-2 py-1 text-emerald-700 dark:text-emerald-400">{name}</span>)}</div></div> : null}
      </div> : null}
      {expanded ? <div id={detailId} className="border-t border-[var(--border)] bg-[var(--surface-hover)] p-3 text-xs">
        <div className="mb-3 flex flex-wrap items-center gap-2"><Tag color={preview ? 'default' : 'processing'}>{preview ? (zh ? '规则预览 · 不是运行证据' : 'Rule preview · not runtime evidence') : sources[step.source]}</Tag>{step.attempt_id ? <span className="break-all font-mono text-[var(--muted)]">{step.attempt_id}</span> : null}</div>
        {step.evidence.length ? <ul className="space-y-2">{step.evidence.map((item, index) => <li key={index} className="break-words border-b border-[var(--border)] pb-2 font-mono last:border-0 last:pb-0">{item}</li>)}</ul> : <p className="text-[var(--muted)]">{zh ? '没有额外记录；不补充假定的中间步骤。' : 'No additional record; intermediate steps are not assumed.'}</p>}
        {!preview && step.phase === 'core' ? <p className="mt-3 text-[var(--muted)]">{zh ? '内核返回的策略组链不代表逐条规则匹配过程；同一快照作为一组证据展示。' : 'A policy chain is not a rule-by-rule evaluation. One snapshot is shown as one evidence group.'}</p> : null}
      </div> : null}
    </div>
  </li>
}

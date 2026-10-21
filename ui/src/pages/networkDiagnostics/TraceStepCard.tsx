import { useId, useState } from 'react'
import { AlertTriangle, Check, ChevronDown, Circle, X } from 'lucide-react'
import { Button } from '@acme/components'
import type { NetworkTraceStep } from '../../lib/networkDiagnosticsTypes'
import { TraceElapsed } from './TraceElapsed'
import { decisionRows, traceTitle } from './tracePresentation'

export function TraceStepCard({ step, zh, preview = false, active = false, onExpand }: {
  step: NetworkTraceStep
  zh: boolean
  preview?: boolean
  active?: boolean
  onExpand?: () => void
}) {
  const [expanded, setExpanded] = useState(false)
  const [rawExpanded, setRawExpanded] = useState(false)
  const detailId = useId()
  const unfinished = step.status === 'running' && !active
  const color = preview || step.status === 'running' ? 'text-zinc-500 dark:text-zinc-400 bg-zinc-500/10' : step.status === 'failed' ? 'text-red-600 bg-red-500/10'
    : step.status === 'unknown' ? 'text-amber-600 bg-amber-500/10' : 'text-emerald-700 dark:text-emerald-300 bg-emerald-500/10'
  const sources = zh
    ? { session: '诊断请求', frontend: 'Sempre · DNS', core: '内核 · 同一快照', socket: step.phase === 'dns' ? '系统解析' : '连接', http: '请求结果', config: '配置推演' }
    : { session: 'Diagnostic request', frontend: 'Sempre · DNS', core: 'Core · same snapshot', socket: 'Transport', http: 'Request result', config: 'Configuration prediction' }
  const rows = decisionRows(step.evidence, zh)
  const title = preview ? (zh ? '前置规则判断' : 'Frontend rule evaluation') : traceTitle(step, zh)
  return <li className="relative pb-[15px] pl-7 sm:pl-[34px] motion-safe:animate-[trace-arrive_220ms_ease-out_both] before:absolute before:top-[21px] before:-bottom-1 before:left-2.5 before:w-px before:bg-[var(--border)] last:before:hidden">
    <span className={`absolute left-0 top-2 z-1 grid size-[21px] place-items-center rounded-full ${color}`} aria-hidden="true">
      {preview || unfinished ? <Circle size={11} /> : step.status === 'running' ? <span className="size-2 rounded-full bg-current motion-safe:animate-[trace-breathe_1.4s_ease-in-out_infinite]" /> : step.status === 'passed' ? <Check size={13} /> : step.status === 'failed' ? <X size={13} /> : <AlertTriangle size={12} />}
    </span>
    <div className="overflow-hidden rounded-[9px] border border-[var(--border)]">
      <Button variant="unstyled" className="w-full !justify-start gap-2 px-3 py-[11px] text-left hover:bg-[var(--surface-hover)] sm:gap-2.5" aria-expanded={expanded} aria-controls={detailId} onClick={() => { setExpanded(!expanded); if (!expanded) onExpand?.() }}>
        <span className="min-w-0 flex-1">
          <span className="flex flex-wrap items-baseline gap-x-2 gap-y-1"><span className="text-sm font-medium">{title}</span><span className="text-[11px] text-[var(--muted)]">{sources[step.source]}{unfinished ? (zh ? ' · 未完成' : ' · unfinished') : ''}</span></span>
          <span className="mt-1 block break-words text-xs text-[var(--muted)]">{step.detail}</span>
          {!preview ? <span className="mt-1 block sm:hidden"><TraceElapsed step={step} active={active} zh={zh} /></span> : null}
        </span>
        {!preview ? <span className="hidden sm:block"><TraceElapsed step={step} active={active} zh={zh} /></span> : null}
        <ChevronDown size={15} className={`shrink-0 text-[var(--muted)] motion-safe:transition-transform motion-safe:duration-200 ${expanded ? 'rotate-180' : ''}`} />
      </Button>
      {expanded ? <div id={detailId} className="border-t border-[var(--border)] bg-[var(--surface-hover)] p-3 text-xs">
        <div className={`mb-2 font-medium ${!preview && step.status === 'passed' ? 'text-emerald-700 dark:text-emerald-300' : 'text-zinc-500 dark:text-zinc-400'}`}>{preview ? (zh ? '配置判断 · 不是运行证据' : 'Configuration evaluation · not runtime evidence') : sources[step.source]}</div>
        {step.attempt_id ? <div className="flex justify-between gap-3 border-b border-[var(--border)] py-1.5"><span>{zh ? '关联连接' : 'Connection attempt'}</span><span className="break-all text-right text-[var(--muted)]">{step.attempt_id}</span></div> : null}
        {rows.map(([label, value], index) => <div key={index} className="flex items-start justify-between gap-3 border-b border-[var(--border)] py-1.5 last:border-0"><span className="min-w-0 break-words">{label}</span><span className="max-w-[65%] break-words text-right text-[var(--muted)]">{value}</span></div>)}
        {step.rule ? <div className="flex items-start justify-between gap-3 py-1.5"><span>{zh ? '最终命中规则' : 'Matched rule'}</span><span className="break-all text-right text-[var(--muted)]">{step.rule.type} · {step.rule.payload}</span></div> : null}
        {step.chains?.length ? <div className="my-2"><p className="mb-2 text-[var(--muted)]">{zh ? '策略组与节点 · 内核返回顺序' : 'Policy groups and nodes · API order'}</p><div className="flex flex-wrap items-center gap-1.5">{step.chains.map((name, index) => <span key={`${index}-${name}`} className="max-w-full break-words rounded bg-[var(--surface)] px-2 py-1">{name}</span>)}</div></div> : null}
        {step.phase === 'core' ? <p className="mt-2 text-[var(--muted)]">{zh ? '内核未提供逐条规则匹配过程；这里只展示最终命中规则和同一快照中的策略组链。' : 'The core does not expose every evaluated rule; this shows the final match and policy chain from one snapshot.'}</p> : step.phase === 'frontend' ? <p className="mt-2 text-[var(--muted)]">{zh ? 'DNS 判断只说明解析路径，不能证明业务连接直连或经过内核。' : 'DNS decisions describe resolution only; they do not prove a direct or core-routed connection.'}</p> : null}
        {!rows.length && !step.rule ? <p className="break-words text-[var(--muted)]">{step.detail}</p> : null}
        {step.evidence.length ? <div className="mt-3"><Button variant="unstyled" className="gap-1 text-[11px] text-[var(--muted)]" aria-expanded={rawExpanded} onClick={() => setRawExpanded(!rawExpanded)}><ChevronDown size={12} className={rawExpanded ? 'rotate-180' : ''} />{zh ? '查看原始证据' : 'View raw evidence'}</Button>{rawExpanded ? <ul className="mt-2 space-y-1 break-all font-mono text-[11px] text-[var(--muted)]">{step.evidence.map((item, index) => <li key={index}>{item}</li>)}</ul> : null}</div> : null}
      </div> : null}
    </div>
  </li>
}

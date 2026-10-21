import { Fragment, useEffect, useId, useState } from 'react'
import { ChevronDown } from 'lucide-react'
import { Button, Tag } from '@acme/components'
import { api } from '../../lib/api'
import type { Session } from '../../lib/types'
import type { NetworkTracePreview } from '../../lib/networkDiagnosticsTypes'
import { TraceStepCard } from './TraceStepCard'

export function RulePreview({ target, session, zh, expanded, onToggle }: {
  target: string
  session: Session | null
  zh: boolean
  expanded: boolean
  onToggle: () => void
}) {
  const [result, setResult] = useState<{ target: string; data?: NetworkTracePreview; error?: string } | null>(null)
  const detailId = useId()
  const trimmed = target.trim()
  useEffect(() => {
    if (!session || !trimmed) return
    const controller = new AbortController()
    const timer = window.setTimeout(() => {
      void api<NetworkTracePreview>(session, '/network/diagnostics/preview', { method: 'POST', body: JSON.stringify({ target: trimmed }), signal: controller.signal })
        .then((data) => {
          if (data.mode !== 'preview' || data.steps.some((step) => step.source !== 'config')) throw new Error(zh ? '规则预览返回了不正确的数据来源' : 'Invalid source in the rule preview')
          if (!controller.signal.aborted) setResult({ target: trimmed, data })
        })
        .catch((error) => {
          if (!controller.signal.aborted) setResult({ target: trimmed, error: error instanceof Error ? error.message : String(error) })
        })
    }, 300)
    return () => { window.clearTimeout(timer); controller.abort() }
  }, [trimmed, session, zh])
  const current = result?.target === trimmed ? result : null
  return <section className="mt-4 rounded-lg border border-dashed border-[var(--border)]" aria-label={zh ? '规则模拟预览' : 'Simulated rule preview'}>
    <Button variant="unstyled" className="w-full !justify-start gap-2 px-3 py-3 text-left" aria-expanded={expanded} aria-controls={detailId} onClick={onToggle}>
      <span className="text-sm font-medium">{zh ? '规则预览' : 'Rule preview'}</span><Tag color="default">{zh ? '模拟 · 未发起目标请求' : 'Simulation · no target request'}</Tag><ChevronDown size={15} className={`ml-auto shrink-0 text-[var(--muted)] motion-safe:transition-transform motion-safe:duration-200 ${expanded ? 'rotate-180' : ''}`} />
    </Button>
    {expanded ? <div id={detailId} className="border-t border-dashed border-[var(--border)] p-3">
      {current?.data ? <div className="mb-3 flex flex-wrap items-center gap-[7px] text-xs" aria-label={zh ? '模拟路径' : 'Simulated path'}>{current.data.path.map((name, index) => <Fragment key={`${index}-${name}`}>{index ? <span aria-hidden="true" className="text-[var(--muted)]">→</span> : null}<span className="rounded bg-[var(--surface-hover)] px-2 py-1.5">{name}</span></Fragment>)}</div> : null}
      <p className="mb-3 text-xs text-[var(--muted)]">{zh ? '仅根据当前生效规则推演前置决策；DNS 结果、内核命中规则和实际出口等待实测。' : 'Predicts frontend decisions using active rules only. DNS results, core rules and the actual exit require a measured test.'}</p>
      {current?.error ? <p role="status" className="break-words text-xs text-amber-600">{current.error}</p> : current?.data ? <ol className="list-none">{current.data.steps.map((step) => <TraceStepCard key={step.id} step={step} zh={zh} preview />)}</ol> : <p role="status" className="text-xs text-[var(--muted)]">{trimmed ? (zh ? '正在更新规则预览…' : 'Updating rule preview…') : (zh ? '输入目标地址查看规则预览。' : 'Enter a target to preview its rules.')}</p>}
    </div> : null}
  </section>
}

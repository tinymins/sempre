import { Alert, Progress, Tag } from '@acme/components'
import type { LeakPhase, LeakRun } from './useLeakDetection'

export function LeakProgress({ run, phase, remainingSeconds, error, zh }: { run: LeakRun | null; phase: LeakPhase; remainingSeconds: number; error: string; zh: boolean }) {
  if (!run || phase === 'idle') return null
  const quick = run.mode === 'quick'
  const steps = quick
    ? [...(run.localProbes ? [{ key: 'preparing', label: zh ? '准备采集器' : 'Prepare collector' }] : []), { key: 'probing', label: zh ? '执行探针' : 'Run probes' }]
    : [{ key: 'preparing', label: zh ? '准备采集器' : 'Prepare collector' }, { key: 'observing', label: zh ? '被动观测' : 'Observe traffic' }]
  const ended = phase === 'done' || phase === 'stopped' || phase === 'failed'
  const index = ended ? steps.length : steps.findIndex((step) => step.key === phase)
  const outcome = { done: { color: 'success', label: zh ? '已完成' : 'Completed' }, stopped: { color: 'default', label: zh ? '已停止，结果不完整' : 'Stopped; results incomplete' }, failed: { color: 'error', label: zh ? '未完整完成' : 'Did not complete' } } as const
  return <section className="min-w-0 space-y-3" aria-live="polite" aria-label={zh ? '检测进度' : 'Check progress'}>
    <ol className="flex min-w-0 flex-wrap items-center gap-2 text-sm">
      {steps.map((step, position) => <li key={step.key} className="flex items-center gap-2">
        <Tag color={position < index ? 'success' : position === index ? 'processing' : 'default'}>{position + 1}. {step.label}</Tag>
        <span className="text-[var(--muted)]" aria-hidden>›</span>
      </li>)}
      <li>{ended ? <Tag color={outcome[phase].color}>{outcome[phase].label}</Tag> : <Tag>{zh ? '完成' : 'Done'}</Tag>}</li>
    </ol>
    {phase === 'observing' && <div className="max-w-md"><Progress percent={Math.round((1 - remainingSeconds / run.durationSeconds) * 100)} format={() => `${zh ? '剩余' : 'Remaining'} ${remainingSeconds}s`} /></div>}
    {error && <Alert type="error" message={error} />}
  </section>
}

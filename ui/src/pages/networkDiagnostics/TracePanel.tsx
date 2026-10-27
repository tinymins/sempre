import { AlertTriangle, ArrowDown, CheckCircle2, GitBranch, XCircle } from 'lucide-react'
import { Alert, Button } from '@acme/components'
import type { NetworkTraceRun } from '../../lib/networkDiagnosticsTypes'
import { TraceStepCard } from './TraceStepCard'
import { traceTitle } from './tracePresentation'
import { useTraceFollow } from './useTraceFollow'
import './networkTrace.css'

export function TracePanel({ run, zh }: { run: NetworkTraceRun | null; zh: boolean }) {
  const { latestRef, following, pause, resume } = useTraceFollow(run, !!run?.running)
  const pending = run?.steps.slice().reverse().find((step) => step.status === 'running')
  const last = run?.steps.at(-1)
  const progress = pending ? traceTitle(pending, zh) : (zh ? '等待本次请求证据' : 'Waiting for request evidence')
  return <section aria-label={zh ? '本次实测链路' : 'Measured request trace'}>
    <div className="flex items-start justify-between gap-2.5">
      <div><h2 className="text-base font-medium">{zh ? '实测链路' : 'Measured path'} <span className="text-xs font-normal text-[var(--muted)]">· {zh ? '本次运行' : 'current run'}</span></h2><p className="mt-1 text-xs text-[var(--muted)]" role="status" aria-live="polite">{run ? (zh ? `已收到 ${run.steps.length} 组证据 · ${run.running ? progress : '点击节点查看依据'}` : `${run.steps.length} evidence groups · ${run.running ? progress : 'Expand nodes to inspect evidence'}`) : (zh ? '等待开始 · 点击节点可展开判断依据' : 'Ready · expand nodes to inspect decisions')}</p></div>
      <span className={`shrink-0 rounded px-2 py-1 text-[11px] ${run?.running ? 'bg-zinc-500/10 text-zinc-500 dark:text-zinc-400' : 'bg-[var(--surface-hover)] text-[var(--muted)]'}`}>{run?.running ? (zh ? '测试进行中' : 'Testing') : run ? (zh ? '测试已结束' : 'Finished') : (zh ? '未开始' : 'Not started')}</span>
    </div>
    {run ? <p className="mt-2 break-all text-xs text-[var(--muted)]">{zh ? '实测目标：' : 'Measured target: '}{run.target}</p> : null}
    {!run ? <div className="flex min-h-[148px] flex-col items-center justify-center gap-2 text-center text-sm text-[var(--muted)]"><GitBranch size={25} className="mb-1" /><span>{zh ? '链路会随着证据逐步展开' : 'The path unfolds as evidence arrives'}</span><small className="text-xs">{zh ? '前置处理记录 + 内核连接记录' : 'Frontend decisions + core connection records'}</small></div> : null}
    <ol className="mt-[22px] list-none empty:mt-0" aria-label={zh ? '按证据到达顺序显示的链路' : 'Path in order of received evidence'}>{run?.steps.map((step) => <TraceStepCard key={step.id} step={step} zh={zh} target={run.target} active={run.running} onExpand={pause} />)}</ol>
    <div ref={latestRef} className="scroll-mt-20 scroll-mb-5">
      {run?.running ? <div role="status" className="mb-2 ml-1 mt-0.5 flex items-center gap-2.5 text-xs text-[var(--muted)]"><span className="size-3 shrink-0 rounded-full bg-zinc-500 dark:bg-zinc-400 motion-safe:animate-[trace-breathe_1.4s_ease-in-out_infinite]" /><span>{progress}</span></div> : null}
      {run?.error ? <Alert className="mt-3" type="warning" showIcon message={run.error} /> : null}
      {run?.status ? <div role="status" className={`flex items-start gap-2 rounded-[9px] px-3.5 py-3 text-xs ${run.status === 'passed' ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300' : run.status === 'failed' ? 'bg-red-500/10 text-red-700 dark:text-red-300' : 'bg-amber-500/10 text-amber-700 dark:text-amber-300'}`}>{run.status === 'passed' ? <CheckCircle2 size={16} className="shrink-0" /> : run.status === 'failed' ? <XCircle size={16} className="shrink-0" /> : <AlertTriangle size={16} className="shrink-0" />}<p>{run.status === 'passed' ? (zh ? '本次请求已完成 · 已观察的记录保留，连接关闭后仍可展开。' : 'Request completed · observed records remain available after closure.') : run.status === 'failed' ? (zh ? '本次请求失败 · 已收到的证据保留，请查看失败步骤。' : 'Request failed · received evidence is retained; inspect the failed step.') : (zh ? '测试结束 · 部分链路证据未能确认，不推断未观测的出口。' : 'Test finished · parts of the path remain unconfirmed; no exit is inferred.')}</p></div> : null}
    </div>
    {run && !following ? <div className="sticky bottom-4 z-10 mt-3 flex justify-end"><Button icon={<ArrowDown size={14} />} onClick={resume}>{zh ? '回到最新' : 'Back to latest'}</Button></div> : null}
    {run && !run.running && last ? <p className="mt-3 break-all text-[10px] text-[var(--muted)]">{run.target}{run.traceId ? ` · ${run.traceId}` : ''}</p> : null}
  </section>
}

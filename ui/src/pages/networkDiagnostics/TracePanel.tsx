import { AlertTriangle, CheckCircle2, Route, Square, XCircle } from 'lucide-react'
import { Alert, Button, Card, Tag } from '@acme/components'
import type { NetworkTraceRun } from '../../lib/networkDiagnosticsTypes'
import { TraceStepCard } from './TraceStepCard'
import './networkTrace.css'

export function TracePanel({ run, zh, onStop }: { run: NetworkTraceRun | null; zh: boolean; onStop: () => void }) {
  return <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
    <section aria-label={zh ? '本次实测链路' : 'Measured request trace'}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div><h2 className="font-semibold">{zh ? '本次实测链路' : 'Measured request trace'}</h2><p className="mt-1 text-xs text-[var(--muted)]">{zh ? '只展示本次运行记录 · 点击节点展开证据' : 'Runtime records only · expand nodes to inspect evidence'}</p></div>
        <div className="flex items-center gap-2"><Tag color={run?.running ? 'processing' : 'default'}>{run?.running ? (zh ? '测试中' : 'Running') : run ? (zh ? '已结束' : 'Finished') : (zh ? '未开始' : 'Not started')}</Tag>{run?.running ? <Button size="small" icon={<Square size={12} />} onClick={onStop}>{zh ? '停止' : 'Stop'}</Button> : null}</div>
      </div>
      {run ? <div className="mt-3 space-y-1 break-all text-xs text-[var(--muted)]"><p>{run.target}</p>{run.traceId ? <p className="font-mono">{run.traceId}</p> : null}<p>{zh ? '解析入口以本次证据为准；调用前置解析器不验证系统 DNS 接管。' : 'The resolver entry is shown by this run’s evidence. Calling the frontend resolver does not verify system DNS interception.'}</p></div> : null}
      <div className="mt-4">
        {!run ? <div className="flex min-h-36 flex-col items-center justify-center gap-2 text-center text-sm text-[var(--muted)]"><Route size={24} /><p>{zh ? '点击“开始测试”，真实链路将随证据逐步出现。' : 'Run a test to reveal the measured path as evidence arrives.'}</p><p className="text-xs">{zh ? '规则预览不会转换成实测节点。' : 'Preview nodes never become measured evidence.'}</p></div> : null}
        <ol className="list-none">{run?.steps.map((step) => <TraceStepCard key={step.id} step={step} zh={zh} active={run.running} />)}</ol>
        {run?.running ? <div role="status" className="mt-4 flex items-center gap-2 text-xs text-[var(--muted)]"><span className="size-2.5 rounded-full bg-[var(--accent)] motion-safe:animate-pulse" />{zh ? `已收到 ${run.steps.length} 组记录，等待后续证据…` : `${run.steps.length} record groups received; awaiting more evidence…`}</div> : null}
        {run?.error ? <Alert className="mt-4" type="warning" showIcon message={run.error} /> : null}
        {run?.status ? <div role="status" className="mt-4 flex items-start gap-2 rounded-lg bg-[var(--surface-hover)] p-3 text-xs">{run.status === 'passed' ? <CheckCircle2 size={16} className="shrink-0 text-emerald-600" /> : run.status === 'failed' ? <XCircle size={16} className="shrink-0 text-red-600" /> : <AlertTriangle size={16} className="shrink-0 text-amber-600" />}<p>{run.status === 'passed' ? (zh ? '本次请求已完成。已捕获的前置和内核记录保留，可展开查看。' : 'Request completed. Captured frontend and core records are retained.') : run.status === 'failed' ? (zh ? '本次请求失败，已收到的证据保留，请查看失败步骤。' : 'The request failed. Received evidence is retained; inspect the failed step.') : (zh ? '本次测试结束，链路证据不完整；未观测部分不作推断。' : 'Test finished with incomplete evidence. Unobserved steps are not inferred.')}</p></div> : null}
      </div>
    </section>
  </Card>
}

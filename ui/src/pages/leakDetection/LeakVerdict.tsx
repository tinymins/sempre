import { Card, Tag } from '@acme/components'
import type { Verdict, VerdictState } from './verdict'

export function LeakVerdict({ verdict, running, zh }: { verdict: Verdict; running: boolean; zh: boolean }) {
  const states: Record<VerdictState, { color: string; label: string }> = {
    ok: { color: 'success', label: zh ? '正常' : 'OK' },
    review: { color: 'warning', label: zh ? '需核实' : 'Review' },
    unknown: { color: 'default', label: zh ? '未知' : 'Unknown' },
    skipped: { color: 'default', label: zh ? '未运行' : 'Not run' },
  }
  const overall = running ? { color: 'processing', label: zh ? '检测中' : 'Checking' } : {
    pending: { color: 'default', label: zh ? '尚未检测' : 'Not checked' },
    ok: { color: 'success', label: zh ? '未发现异常' : 'No issues found' },
    review: { color: 'warning', label: zh ? `发现 ${verdict.reviewCount} 项需核实` : `${verdict.reviewCount} item(s) need review` },
    incomplete: { color: 'default', label: zh ? '覆盖不完整' : 'Coverage incomplete' },
  }[verdict.overall]
  return <Card className="min-w-0" bodyStyle={{ padding: '1rem' }}>
    <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
      <h2 className="text-base font-semibold">{zh ? '检测结论' : 'Verdict'}</h2>
      <Tag color={overall.color}>{overall.label}</Tag>
    </div>
    <ul className="mt-3 divide-y divide-[var(--border)]">
      {verdict.rows.map((row) => <li key={row.key} className="grid min-w-0 grid-cols-[1fr_auto] gap-x-3 gap-y-1 py-3 sm:grid-cols-[11rem_1fr_auto]">
        <span className="text-sm font-medium">{row.label}</span>
        <span className="col-span-2 row-start-2 min-w-0 sm:col-span-1 sm:row-start-auto">
          {row.values.length > 0 && <span className="block break-all font-mono text-sm">{row.values.join(' · ')}</span>}
          <span className="block break-words text-xs text-[var(--muted)]">{row.note}</span>
        </span>
        <span className="col-start-2 row-start-1 sm:col-start-auto sm:row-start-auto"><Tag color={states[row.state].color}>{states[row.state].label}</Tag></span>
      </li>)}
    </ul>
    <p className="mt-2 text-xs text-[var(--muted)]">{zh ? '结论仅覆盖本次检测时段与已执行的探针；“未知”不代表没有泄漏。' : 'Results cover only this run and executed probes; “Unknown” does not mean leak-free.'}</p>
  </Card>
}

import { useEffect, useState } from 'react'
import type { NetworkTraceStep } from '../../lib/networkDiagnosticsTypes'

export function TraceElapsed({ step, active, zh }: { step: NetworkTraceStep; active: boolean; zh: boolean }) {
  const ticking = active && step.status === 'running'
  const [now, setNow] = useState(() => performance.now())
  useEffect(() => {
    if (!ticking) return
    const timer = window.setInterval(() => setNow(performance.now()), 100)
    return () => window.clearInterval(timer)
  }, [ticking])
  const unfinished = step.status === 'running'
  const waiting = unfinished && step.phase === 'http'
  const sinceEvent = step.receivedAt === undefined ? 0 : Math.max(0, now - step.receivedAt)
  const elapsed = waiting ? sinceEvent : step.elapsed_ms + (unfinished ? sinceEvent : 0)
  return <span className="whitespace-nowrap text-[11px] tabular-nums text-[var(--muted)]">
    {waiting ? (zh ? '已等待 ' : 'Waiting ') : '+'}{elapsed >= 1000 ? `${(elapsed / 1000).toFixed(1)} s` : `${Math.round(elapsed)} ms`}
  </span>
}

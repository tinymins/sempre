import { useEffect, useRef, useState } from 'react'
import { streamRequest } from '../../lib/api'
import type { Session } from '../../lib/types'
import type { NetworkTraceRun, NetworkTraceStep } from '../../lib/networkDiagnosticsTypes'

export function useNetworkTrace(session: Session | null, zh: boolean) {
  const [run, setRun] = useState<NetworkTraceRun | null>(null)
  const controllerRef = useRef<AbortController | null>(null)
  const generationRef = useRef(0)
  useEffect(() => () => controllerRef.current?.abort(), [])

  const start = (target: string) => {
    if (!session) return
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    const generation = ++generationRef.current
    setRun({ generation, target, steps: [], running: true })
    let traceId: string | undefined
    let terminal = false
    const update = (change: (current: NetworkTraceRun) => NetworkTraceRun) => {
      if (!controller.signal.aborted) setRun((current) => current?.generation === generation ? change(current) : current)
    }
    void (async () => {
      try {
        await streamRequest(session, '/network/diagnostics/trace', { target }, (event, data) => {
          if (controller.signal.aborted || terminal) return
          if (event === 'trace-started') {
            const started = data as { trace_id: string; target: string }
            traceId = started.trace_id
            update((current) => ({ ...current, traceId, target: started.target }))
          } else if (event === 'trace-step') {
            const payload = data as { trace_id: string; step: NetworkTraceStep }
            if (!traceId || payload.trace_id !== traceId) throw new Error(zh ? '链路证据的会话关联不一致' : 'Trace evidence does not match this session')
            if (!['session', 'frontend', 'core', 'socket', 'http'].includes(payload.step.source)) {
              throw new Error(zh ? '实测链路拒绝接收非运行时证据' : 'Measured traces require runtime evidence')
            }
            const observed = { ...payload.step, receivedAt: performance.now() }
            update((current) => ({ ...current, steps: current.steps.some((step) => step.id === payload.step.id)
              ? current.steps.map((step) => step.id === payload.step.id ? observed : step)
              : [...current.steps, observed] }))
          } else if (event === 'trace-completed') {
            const completed = data as { trace_id: string; status: NetworkTraceRun['status'] }
            if (!traceId || completed.trace_id !== traceId) throw new Error(zh ? '链路结果的会话关联不一致' : 'Trace result does not match this session')
            terminal = true
            update((current) => ({ ...current, running: false, status: completed.status }))
          } else if (event === 'error') {
            terminal = true
            update((current) => ({ ...current, running: false, error: (data as { message?: string }).message ?? (zh ? '链路测试失败' : 'Trace failed') }))
          }
        }, controller.signal)
        if (!terminal && !controller.signal.aborted) throw new Error(zh ? '证据流提前结束，已有记录保留' : 'The evidence stream ended early; observed records are retained')
      } catch (error) {
        update((current) => ({ ...current, running: false, error: error instanceof Error ? error.message : String(error) }))
        controller.abort()
      }
    })()
  }

  const stop = () => {
    controllerRef.current?.abort()
    setRun((current) => current ? { ...current, running: false, error: zh ? '测试已停止，已收到的证据保留；未完成步骤不作结论。' : 'Stopped. Received evidence is retained; unfinished steps have no conclusion.' } : current)
  }
  return { run, start, stop }
}

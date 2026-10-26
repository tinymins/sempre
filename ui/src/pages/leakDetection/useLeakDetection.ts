import { useEffect, useRef, useState } from 'react'
import { api, streamRequest } from '../../lib/api'
import type { Session } from '../../lib/types'
import { runBrowserChecks } from './browserChecks'
import type { BrowserReport, LeakPreflight, LeakReport } from './types'

export interface LeakRunOptions {
  target: 'all' | 'codex' | 'claude'
  topology: 'local' | 'gateway'
  clientIp: string
  durationSeconds: number
  includeBrowser: boolean
  expectedIpv4: string
  expectedIpv6: string
}

export function useLeakDetection(session: Session | null, zh: boolean) {
  const [preflight, setPreflight] = useState<LeakPreflight | null>(null)
  const [preflightError, setPreflightError] = useState('')
  const [report, setReport] = useState<LeakReport | null>(null)
  const [browser, setBrowser] = useState<BrowserReport | null>(null)
  const [running, setRunning] = useState(false)
  const [message, setMessage] = useState('')
  const [error, setError] = useState('')
  const [context, setContext] = useState<Record<string, unknown> | null>(null)
  const controller = useRef<AbortController | null>(null)

  useEffect(() => {
    if (!session) return
    const request = new AbortController()
    void api<LeakPreflight>(session, '/network/leak-detection/preflight', { signal: request.signal }).then((value) => {
      if (!request.signal.aborted) { setPreflight(value); setPreflightError('') }
    }).catch((cause: unknown) => {
      if (!request.signal.aborted) setPreflightError(cause instanceof Error ? cause.message : String(cause))
    })
    return () => request.abort()
  }, [session])

  useEffect(() => () => controller.current?.abort(), [session])

  const start = (options: LeakRunOptions, browserOnly = false) => {
    if (!session || running) return
    controller.current?.abort()
    const active = new AbortController()
    controller.current = active
    setRunning(true)
    setError('')
    setReport(null)
    setBrowser(null)
    setMessage(zh ? '正在采集；请在目标应用中发起实际请求。' : 'Observing; make a real request in the target application.')
    setContext({ startedAt: new Date().toISOString(), daemon: session.baseURL, browserOrigin: window.location.origin, browserOnly, ...options })
    const current = () => controller.current === active
    const observe = async () => {
      let terminal = false
      try {
        await streamRequest(session, '/network/leak-detection/observe', {
          target: options.target, topology: options.topology, durationSeconds: options.durationSeconds,
          ...(options.topology === 'gateway' ? { clientIp: options.clientIp.trim() } : {}),
        }, (event, data) => {
          if (!current() || active.signal.aborted) return
          if (event === 'status') setMessage((data as { message: string }).message)
          if (event === 'snapshot' || event === 'result') {
            setReport(data as LeakReport)
            if (event === 'result') terminal = true
          }
          if (event === 'error') {
            terminal = true
            setError((data as { message?: string }).message ?? (zh ? '采集失败' : 'Observation failed'))
          }
        }, active.signal)
        if (!terminal && !active.signal.aborted && current()) setError(zh ? '采集连接提前结束；已有结果覆盖不完整。' : 'Observation stream ended early; existing results are incomplete.')
      } catch (cause) {
        if (!active.signal.aborted && current()) setError(cause instanceof Error ? cause.message : String(cause))
      }
    }
    const browserTask = async () => {
      if (!browserOnly && !options.includeBrowser) return
      const result = await runBrowserChecks(active.signal, zh, options.expectedIpv4.trim(), options.expectedIpv6.trim())
      if (current()) setBrowser(result)
    }
    void Promise.all([browserOnly ? Promise.resolve() : observe(), browserTask()]).catch((cause: unknown) => {
      if (!active.signal.aborted && current()) setError(cause instanceof Error ? cause.message : String(cause))
    }).finally(() => {
      if (current()) {
        setRunning(false)
        setMessage(active.signal.aborted ? (zh ? '已停止；结果只包含停止前的观测。' : 'Stopped; results contain observations made before stopping.') : (zh ? '本次采集结束；请结合覆盖缺口判断。' : 'Observation ended; review coverage gaps alongside the evidence.'))
      }
    })
  }

  const stop = () => {
    controller.current?.abort()
    setMessage(zh ? '正在停止采集…' : 'Stopping observation…')
  }

  const exportReport = () => {
    const url = URL.createObjectURL(new Blob([JSON.stringify({ schemaVersion: 1, exportedAt: new Date().toISOString(), context, preflight, report, browser, error, running }, null, 2)], { type: 'application/json' }))
    const link = document.createElement('a')
    link.href = url
    link.download = `sempre-leak-detection-${new Date().toISOString().replaceAll(':', '-')}.json`
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }

  return { preflight, preflightError, report, browser, running, message, error, start, stop, exportReport }
}

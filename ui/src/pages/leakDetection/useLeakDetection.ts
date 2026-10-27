import { useEffect, useRef, useState } from 'react'
import { api, streamRequest } from '../../lib/api'
import type { Session } from '../../lib/types'
import { runBrowserChecks } from './browserChecks'
import type { BrowserReport, LeakPreflight, LeakProbes, LeakReport } from './types'

export type LeakMode = 'quick' | 'observe'
export interface LeakRunOptions {
  mode: LeakMode
  topology: 'local' | 'gateway'
  clientIp: string
  target: 'all' | 'codex' | 'claude'
  durationSeconds: number
  includeBrowser: boolean
  expectedIpv4: string
  expectedIpv6: string
}
export type LeakPhase = 'idle' | 'preparing' | 'probing' | 'observing' | 'done' | 'stopped' | 'failed'
export interface LeakRun extends LeakRunOptions { startedAt: string; daemon: string; browserOrigin: string; localProbes: boolean }

// Quick checks only need the collector long enough for one probe round: the
// backend requires 20 seconds of remaining observation when probes start.
export const QUICK_DURATION_SECONDS = 40

interface Active {
  run: LeakRun
  controller: AbortController
  timer: ReturnType<typeof setTimeout>
  ticker: ReturnType<typeof setInterval>
  probesRequested: boolean
  probesDone: boolean
  browserDone: boolean
}

const terminal = (probes: LeakProbes) => probes.status === 'completed' || probes.status === 'stopped' || probes.status === 'unavailable'

function stopPendingProbes(probes: LeakProbes, detail: string): LeakProbes {
  if (probes.status !== 'running') return probes
  return { ...probes, status: 'stopped', results: probes.results.map((result) => result.status === 'pending' || result.status === 'running' ? { ...result, status: 'stopped', detail } : result) }
}

export function useLeakDetection(session: Session | null, zh: boolean) {
  const [preflight, setPreflight] = useState<LeakPreflight | null>(null)
  const [preflightError, setPreflightError] = useState('')
  const [report, setReport] = useState<LeakReport | null>(null)
  const [browser, setBrowser] = useState<BrowserReport | null>(null)
  const [phase, setPhase] = useState<LeakPhase>('idle')
  const [remainingSeconds, setRemainingSeconds] = useState(0)
  const [error, setError] = useState('')
  const [run, setRun] = useState<LeakRun | null>(null)
  const active = useRef<Active | null>(null)
  const browserController = useRef<AbortController | null>(null)
  const running = phase === 'preparing' || phase === 'probing' || phase === 'observing'

  useEffect(() => {
    if (!session) return
    const request = new AbortController()
    let disposed = false
    let timedOut = false
    const timer = setTimeout(() => { timedOut = true; request.abort() }, 10000)
    void api<LeakPreflight>(session, '/network/leak-detection/preflight', { signal: request.signal }).then((value) => {
      if (!disposed) { setPreflight(value); setPreflightError('') }
    }).catch((cause: unknown) => {
      if (!disposed) setPreflightError(timedOut ? (zh ? '预检查超时' : 'Preflight timed out') : cause instanceof Error ? cause.message : String(cause))
    }).finally(() => clearTimeout(timer))
    return () => { disposed = true; clearTimeout(timer); request.abort() }
  }, [session, zh])

  useEffect(() => () => {
    const current = active.current
    active.current = null
    if (current) { clearTimeout(current.timer); clearInterval(current.ticker); current.controller.abort() }
    browserController.current?.abort()
    browserController.current = null
  }, [session])

  const finish = (current: Active, next: LeakPhase, detail = '') => {
    if (active.current !== current) return
    active.current = null
    clearTimeout(current.timer)
    clearInterval(current.ticker)
    current.controller.abort()
    browserController.current?.abort()
    browserController.current = null
    setPhase(next)
    if (detail) setError(detail)
    setReport((value) => value ? { ...value, ready: false, probes: stopPendingProbes(value.probes, zh ? '检测已结束，结果不完整' : 'Check ended; result is incomplete') } : value)
  }

  const settleQuick = (current: Active) => {
    if (current.run.mode === 'quick' && current.probesDone && current.browserDone) finish(current, 'done')
  }

  const startBrowser = (current: Active) => {
    const controller = new AbortController()
    browserController.current = controller
    void runBrowserChecks(controller.signal, zh, current.run.expectedIpv4.trim(), current.run.expectedIpv6.trim(), (value) => {
      if (browserController.current === controller) setBrowser(value)
    }).catch((cause: unknown) => {
      if (active.current === current) setError(cause instanceof Error ? cause.message : String(cause))
    }).finally(() => {
      if (browserController.current === controller) browserController.current = null
      current.browserDone = true
      settleQuick(current)
    })
  }

  const triggerProbes = async (current: Active, id: string) => {
    if (!session) return
    const request = new AbortController()
    const abort = () => request.abort()
    current.controller.signal.addEventListener('abort', abort, { once: true })
    const timer = setTimeout(abort, 10000)
    try {
      await api(session, `/network/leak-detection/observe/${encodeURIComponent(id)}/probes`, { method: 'POST', body: '{}', signal: request.signal })
    } catch (cause) {
      if (active.current === current) finish(current, 'failed', request.signal.aborted ? (zh ? '发起本机探针超时' : 'Starting local probes timed out') : cause instanceof Error ? cause.message : String(cause))
    } finally {
      clearTimeout(timer)
      current.controller.signal.removeEventListener('abort', abort)
    }
  }

  const onSnapshot = (current: Active, value: LeakReport) => {
    setReport(value)
    setRemainingSeconds(Math.max(0, current.run.durationSeconds - value.elapsedSeconds))
    if (!value.ready) return
    if (current.run.mode === 'observe') { setPhase('observing'); return }
    if (!current.probesRequested) {
      current.probesRequested = true
      setPhase('probing')
      if (current.run.includeBrowser) startBrowser(current)
      void triggerProbes(current, value.id)
      return
    }
    // Finish on a snapshot, not a probe event, so the probes' connection evidence is included.
    if (terminal(value.probes)) { current.probesDone = true; settleQuick(current) }
  }

  const start = (options: LeakRunOptions) => {
    if (!session || active.current) return
    const localProbes = options.mode === 'quick' && options.topology === 'local' && !preflightError
    const observe = options.mode === 'observe' || localProbes
    const durationSeconds = options.mode === 'quick' ? QUICK_DURATION_SECONDS : options.durationSeconds
    const started = Date.now()
    const current: Active = {
      run: { ...options, durationSeconds, localProbes, startedAt: new Date(started).toISOString(), daemon: session.baseURL, browserOrigin: window.location.origin },
      controller: new AbortController(), probesRequested: false, probesDone: !localProbes, browserDone: !(options.mode === 'quick' && options.includeBrowser),
      ticker: setInterval(() => setRemainingSeconds(Math.max(0, durationSeconds - Math.floor((Date.now() - started) / 1000))), 1000),
      timer: setTimeout(() => finish(current, 'failed', zh ? '等待结果超时，已停止；仅保留已收到的证据' : 'Timed out and stopped; only received evidence is retained'), (durationSeconds + 20) * 1000),
    }
    active.current = current
    setRun(current.run); setReport(null); setBrowser(null); setError(''); setRemainingSeconds(durationSeconds)
    if (!observe) {
      setPhase('probing')
      startBrowser(current)
      return
    }
    setPhase('preparing')
    void (async () => {
      try {
        await streamRequest(session, '/network/leak-detection/observe', {
          target: options.target, topology: localProbes ? 'local' : options.topology, durationSeconds,
          ...(options.mode === 'observe' && options.topology === 'gateway' ? { clientIp: options.clientIp.trim() } : {}),
        }, (event, data) => {
          if (active.current !== current) return false
          if (event === 'probe') setReport((value) => value ? { ...value, probes: data as LeakProbes } : value)
          if (event === 'snapshot') onSnapshot(current, data as LeakReport)
          if (event === 'result') { setReport(data as LeakReport); finish(current, 'done'); return false }
          if (event === 'error') { finish(current, 'failed', (data as { message?: string }).message ?? (zh ? '采集失败' : 'Observation failed')); return false }
        }, current.controller.signal)
        finish(current, 'failed', zh ? '采集流提前结束，结果不完整' : 'Observation stream ended early; results are incomplete')
      } catch (cause) {
        finish(current, 'failed', cause instanceof Error ? cause.message : String(cause))
      }
    })()
  }

  const stop = () => {
    const current = active.current
    if (current) finish(current, 'stopped')
  }

  const exportReport = () => {
    const payload = { schemaVersion: 3, exportedAt: new Date().toISOString(), run, preflight, report, browser, phase, error }
    const url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' }))
    const link = document.createElement('a')
    link.href = url
    link.download = `sempre-leak-detection-${new Date().toISOString().replaceAll(':', '-')}.json`
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }

  return { preflight, preflightError, report, browser, run, phase, running, remainingSeconds, error, start, stop, exportReport }
}

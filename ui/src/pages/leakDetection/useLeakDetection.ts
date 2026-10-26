import { useEffect, useRef, useState } from 'react'
import { api, streamRequest } from '../../lib/api'
import type { Session } from '../../lib/types'
import { runBrowserChecks } from './browserChecks'
import type { BrowserReport, LeakPreflight, LeakProbes, LeakReport } from './types'

export interface LeakRunOptions {
  target: 'all' | 'codex' | 'claude'
  topology: 'local' | 'gateway'
  clientIp: string
  durationSeconds: number
  includeBrowser: boolean
  expectedIpv4: string
  expectedIpv6: string
}
export type ObservationPhase = 'idle' | 'connecting' | 'ready' | 'completed' | 'stopped' | 'timeout' | 'failed'
interface Observation {
  controller: AbortController
  timer: ReturnType<typeof setTimeout>
  ticker: ReturnType<typeof setInterval>
  options: LeakRunOptions
  elapsedAtSample: number
  sampledAt: number
  probesRequested: boolean
}

function stopPendingProbes(probes: LeakProbes, detail: string, requested: boolean): LeakProbes {
  if (probes.status !== 'running' && !(requested && probes.status === 'idle')) return probes
  return { ...probes, status: 'stopped', results: probes.results.map((result) => result.status === 'pending' || result.status === 'running' ? { ...result, status: 'stopped', detail } : result) }
}

export function useLeakDetection(session: Session | null, zh: boolean) {
  const [preflight, setPreflight] = useState<LeakPreflight | null>(null)
  const [preflightError, setPreflightError] = useState('')
  const [report, setReport] = useState<LeakReport | null>(null)
  const [browser, setBrowser] = useState<BrowserReport | null>(null)
  const [phase, setPhase] = useState<ObservationPhase>('idle')
  const [remainingSeconds, setRemainingSeconds] = useState(0)
  const [message, setMessage] = useState('')
  const [error, setError] = useState('')
  const [probeError, setProbeError] = useState('')
  const [probeRequested, setProbeRequested] = useState(false)
  const [probeSubmitting, setProbeSubmitting] = useState(false)
  const [context, setContext] = useState<(LeakRunOptions & { startedAt: string; daemon: string; browserOrigin: string }) | null>(null)
  const observation = useRef<Observation | null>(null)
  const browserController = useRef<AbortController | null>(null)
  const observing = phase === 'connecting' || phase === 'ready'

  useEffect(() => {
    if (!session) return
    const request = new AbortController()
    let disposed = false
    let timedOut = false
    const timer = setTimeout(() => { timedOut = true; request.abort() }, 10000)
    void api<LeakPreflight>(session, '/network/leak-detection/preflight', { signal: request.signal }).then((value) => {
      if (!disposed) { setPreflight(value); setPreflightError('') }
    }).catch((cause: unknown) => {
      if (!disposed) setPreflightError(timedOut ? (zh ? '预检查超时；仍可尝试开始观测。' : 'Preflight timed out; observation can still be started.') : cause instanceof Error ? cause.message : String(cause))
    }).finally(() => clearTimeout(timer))
    return () => { disposed = true; clearTimeout(timer); request.abort() }
  }, [session, zh])

  useEffect(() => () => {
    const active = observation.current
    observation.current = null
    if (active) { clearTimeout(active.timer); clearInterval(active.ticker); active.controller.abort() }
    browserController.current?.abort()
    browserController.current = null
  }, [session])

  const stopBrowser = () => {
    browserController.current?.abort()
    browserController.current = null
    const detail = zh ? '已中止；此项结果不完整。' : 'Stopped; this result is incomplete.'
    setBrowser((value) => value?.running ? {
      ...value, running: false, completed: false,
      ipv4: value.ipv4.status === 'running' ? { ...value.ipv4, status: 'unknown', detail } : value.ipv4,
      ipv6: value.ipv6.status === 'running' ? { ...value.ipv6, status: 'unknown', detail } : value.ipv6,
      webrtc: value.webrtc.status === 'running' ? { ...value.webrtc, status: 'unknown', detail } : value.webrtc,
    } : value)
  }

  const finish = (active: Observation, next: ObservationPhase, detail: string) => {
    if (observation.current !== active) return
    observation.current = null
    clearTimeout(active.timer)
    clearInterval(active.ticker)
    active.controller.abort()
    stopBrowser()
    setPhase(next)
    setMessage(detail)
    setProbeSubmitting(false)
    setReport((value) => value ? { ...value, ready: false, probes: stopPendingProbes(value.probes, zh ? '观测已结束，探针结果不完整。' : 'Observation ended; probe result is incomplete.', active.probesRequested) } : value)
  }

  const start = (options: LeakRunOptions) => {
    if (!session || observation.current || browserController.current) return
    const controller = new AbortController()
    const started = Date.now()
    setPhase('connecting'); setRemainingSeconds(options.durationSeconds)
    setError(''); setProbeError(''); setReport(null); setBrowser(null); setProbeRequested(false)
    setMessage(zh ? '正在连接采集器；就绪后可发起探针，无需启动 Codex / Claude Code。' : 'Connecting to the collector. Run probes when ready; Codex / Claude Code need not be running.')
    setContext({ startedAt: new Date(started).toISOString(), daemon: session.baseURL, browserOrigin: window.location.origin, ...options })
    const active: Observation = {
      controller, options, elapsedAtSample: 0, sampledAt: started, probesRequested: false,
      ticker: setInterval(() => {
        const elapsed = Math.max((Date.now() - started) / 1000, active.elapsedAtSample + (Date.now() - active.sampledAt) / 1000)
        setRemainingSeconds(Math.max(0, options.durationSeconds - Math.floor(elapsed)))
      }, 1000),
      timer: setTimeout(() => {
        const detail = zh ? '等待采集结果超时，已停止。本次仅保留已收到的证据。' : 'Observation timed out and was stopped. Only received evidence is retained.'
        setError(detail); finish(active, 'timeout', detail)
      }, (options.durationSeconds + 20) * 1000),
    }
    observation.current = active
    void (async () => {
      try {
        await streamRequest(session, '/network/leak-detection/observe', {
          target: options.target, topology: options.topology, durationSeconds: options.durationSeconds,
          ...(options.topology === 'gateway' ? { clientIp: options.clientIp.trim() } : {}),
        }, (event, data) => {
          if (observation.current !== active) return false
          if (event === 'status') setMessage((data as { message: string }).message)
          if (event === 'probe') setReport((value) => value ? { ...value, probes: data as LeakProbes } : value)
          if (event === 'snapshot' || event === 'result') {
            const value = data as LeakReport
            setReport(value)
            active.elapsedAtSample = value.elapsedSeconds
            active.sampledAt = Date.now()
            setRemainingSeconds(Math.max(0, options.durationSeconds - value.elapsedSeconds))
            if (event === 'result') {
              finish(active, 'completed', zh ? '观测已结束。已收到的结果保留在下方。' : 'Observation completed. Received results are retained below.')
              return false
            }
            setPhase(value.ready ? 'ready' : 'connecting')
            if (value.ready) setMessage(value.probes.status === 'running' ? (zh ? '探针执行中，结果将逐项显示。' : 'Probes are running; results appear individually.') : (zh ? '采集器已就绪，结果会持续更新。' : 'Collector ready; evidence will continue to update.'))
          }
          if (event === 'error') {
            const detail = (data as { message?: string }).message ?? (zh ? '采集失败' : 'Observation failed')
            setError(detail); finish(active, 'failed', detail)
            return false
          }
        }, controller.signal)
        if (observation.current === active) {
          const detail = zh ? '采集流提前结束，结果不完整。' : 'Observation stream ended early; results are incomplete.'
          setError(detail); finish(active, 'failed', detail)
        }
      } catch (cause) {
        if (observation.current === active) {
          const detail = cause instanceof Error ? cause.message : String(cause)
          setError(detail); finish(active, 'failed', detail)
        }
      }
    })()
  }

  const startBrowser = (options: LeakRunOptions) => {
    if (browserController.current) return
    const controller = new AbortController()
    browserController.current = controller
    void runBrowserChecks(controller.signal, zh, options.expectedIpv4.trim(), options.expectedIpv6.trim(), (value) => {
      if (browserController.current === controller) setBrowser(value)
    }).catch((cause: unknown) => {
      if (browserController.current === controller) {
        setProbeError(cause instanceof Error ? cause.message : String(cause)); stopBrowser()
      }
    }).finally(() => {
      if (browserController.current === controller) browserController.current = null
    })
  }

  const triggerProbes = async () => {
    const active = observation.current
    if (!session || !active || !report?.ready || !report.probes.available || report.probes.status !== 'idle' || probeRequested || remainingSeconds < 20) return
    active.probesRequested = true
    setProbeRequested(true); setProbeSubmitting(true); setProbeError('')
    if (active.options.includeBrowser) startBrowser(active.options)
    const request = new AbortController()
    const abort = () => request.abort()
    active.controller.signal.addEventListener('abort', abort, { once: true })
    const timer = setTimeout(abort, 10000)
    try {
      await api(session, `/network/leak-detection/observe/${encodeURIComponent(report.id)}/probes`, { method: 'POST', body: '{}', signal: request.signal })
    } catch (cause) {
      if (observation.current === active) {
        setProbeRequested(false)
        setProbeError(request.signal.aborted ? (zh ? '探针请求超时；请核对下方实时结果。' : 'Probe request timed out; review the live results below.') : cause instanceof Error ? cause.message : String(cause))
      }
    } finally {
      clearTimeout(timer)
      active.controller.signal.removeEventListener('abort', abort)
      if (observation.current === active) setProbeSubmitting(false)
    }
  }

  const stop = () => {
    const active = observation.current
    if (active) finish(active, 'stopped', zh ? '已停止观测；结果只包含停止前的证据。' : 'Observation stopped; only evidence received before stopping is retained.')
    else stopBrowser()
  }

  const exportReport = () => {
    const payload = { schemaVersion: 2, exportedAt: new Date().toISOString(), context, preflight, report, browser, phase, error, probeError }
    const url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' }))
    const link = document.createElement('a')
    link.href = url
    link.download = `sempre-leak-detection-${new Date().toISOString().replaceAll(':', '-')}.json`
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }

  return { preflight, preflightError, report, browser, runOptions: context, phase, observing, remainingSeconds, message, error, probeError, probeRequested, probeSubmitting, start, triggerProbes, startBrowser, stop, exportReport }
}

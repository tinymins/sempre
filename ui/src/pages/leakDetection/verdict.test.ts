import { describe, expect, it } from 'vitest'
import type { BrowserReport, LeakConnection, LeakProbeResult, LeakReport } from './types'
import { observationVerdict, quickVerdict } from './verdict'

const summary = { physicalEgress: 0, proxyRoute: 0, directRoute: 0, unattributed: 0, ipv4: 0, ipv6: 0 }
const probe = (id: LeakProbeResult['id'], pid: number, publicIp: string | null, addresses: string[] = []): LeakProbeResult => ({ id, status: 'passed', detail: '', pid, addresses, publicIp, elapsedMs: 10 })
const connection = (pid: number, verdict: LeakConnection['verdict'], app = 'probe'): LeakConnection => ({ id: `${pid}-${verdict}`, pid, app, protocol: 'tcp', source: '', destination: '', host: '', ipVersion: 'ipv4', physicalEgress: false, verdict, rule: '', chains: [], evidence: [] })
const report = (results: LeakProbeResult[], connections: LeakConnection[] = [], extra: Partial<LeakReport> = {}): LeakReport => ({
  id: 'run', startedAt: '', elapsedSeconds: 10, target: 'all', topology: 'local', platform: 'macos', processes: [], connections, dns: [], gaps: [],
  capture: { status: 'ok', detail: '', packets: [] }, ready: true, probes: { available: false, reason: '', status: 'completed', results }, probeSummary: summary, summary, completed: false, ...extra,
})
const browser = (ipv4: string | undefined, candidates: BrowserReport['webrtc']['candidates'] = []): BrowserReport => ({
  running: false, startedAt: '', origin: '', expectedIpv4: '', expectedIpv6: '', completed: true,
  ipv4: { status: ipv4 ? 'observed' : 'unknown', endpoint: '', ip: ipv4, detail: 'no ipv4' },
  ipv6: { status: 'unknown', endpoint: '', detail: 'no ipv6' },
  webrtc: { status: 'observed', candidates, detail: '' },
})
const state = (verdict: ReturnType<typeof quickVerdict>, key: string) => verdict.rows.find((row) => row.key === key)?.state
const input = { localProbes: true, includeBrowser: true, expectedIpv4: '', expectedIpv6: '' }

describe('quickVerdict', () => {
  it('uses probe routing evidence and expected egress', () => {
    const value = report([probe('dns', 1, null, ['1.1.1.1']), probe('ipv4', 2, '203.0.113.9'), probe('ipv6', 3, null)], [connection(1, 'proxy-route'), connection(2, 'proxy-route')])
    const verdict = quickVerdict({ ...input, report: value, browser: null, includeBrowser: false, expectedIpv4: '198.51.100.1' }, false)
    expect(state(verdict, 'local-dns')).toBe('ok')
    expect(state(verdict, 'local-ipv4')).toBe('review')
    expect(state(verdict, 'local-ipv6')).toBe('unknown')
    expect(state(verdict, 'browser')).toBe('skipped')
    expect(verdict.overall).toBe('review')
  })

  it('flags direct routes even when egress matches', () => {
    const value = report([probe('ipv4', 2, '203.0.113.9')], [connection(2, 'direct-route')])
    expect(state(quickVerdict({ ...input, report: value, browser: null, expectedIpv4: '203.0.113.9' }, false), 'local-ipv4')).toBe('review')
  })

  it('compares the browser with local probe egress when no expectation is given', () => {
    const value = report([probe('ipv4', 2, '203.0.113.9')], [connection(2, 'proxy-route')])
    expect(state(quickVerdict({ ...input, report: value, browser: browser('203.0.113.9') }, false), 'browser-ipv4')).toBe('ok')
    expect(state(quickVerdict({ ...input, report: value, browser: browser('192.0.2.50') }, false), 'browser-ipv4')).toBe('review')
  })

  it('reviews public WebRTC candidates that differ from egress', () => {
    const candidates = [{ address: '192.0.2.77', type: 'srflx', protocol: 'udp', scope: 'public' as const }]
    const verdict = quickVerdict({ ...input, localProbes: false, report: null, browser: browser('203.0.113.9', candidates) }, false)
    expect(state(verdict, 'local')).toBe('skipped')
    expect(state(verdict, 'webrtc')).toBe('review')
    expect(state(quickVerdict({ ...input, localProbes: false, report: null, browser: browser('203.0.113.9') }, false), 'webrtc')).toBe('ok')
  })

  it('never reports unknown results as clean', () => {
    const verdict = quickVerdict({ ...input, localProbes: false, report: null, browser: browser(undefined) }, false)
    expect(verdict.overall).toBe('incomplete')
    expect(quickVerdict({ ...input, report: null, browser: null }, false).overall).toBe('pending')
  })
})

describe('observationVerdict', () => {
  it('requires observed application traffic', () => {
    expect(observationVerdict(report([]), false).overall).toBe('incomplete')
    const observed = report([], [connection(9, 'proxy-route', 'codex')], { summary: { ...summary, proxyRoute: 1 } })
    expect(observationVerdict(observed, false).overall).toBe('ok')
    expect(observationVerdict({ ...observed, summary: { ...summary, directRoute: 1 } }, false).overall).toBe('review')
  })
})

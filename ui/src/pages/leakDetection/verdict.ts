import { normalizeIp } from './browserChecks'
import type { AddressProbe, BrowserReport, LeakConnection, LeakProbeResult, LeakReport } from './types'

export type VerdictState = 'ok' | 'review' | 'unknown' | 'skipped'
export interface VerdictRow { key: string; label: string; values: string[]; state: VerdictState; note: string }
export interface Verdict { rows: VerdictRow[]; overall: 'pending' | 'ok' | 'review' | 'incomplete'; reviewCount: number }

export interface QuickVerdictInput {
  report: LeakReport | null
  browser: BrowserReport | null
  localProbes: boolean
  includeBrowser: boolean
  expectedIpv4: string
  expectedIpv6: string
}

const same = (left: string, right: string) => normalizeIp(left) === normalizeIp(right)

const order: Record<VerdictState, number> = { review: 0, unknown: 1, ok: 2, skipped: 3 }

function summarize(unsorted: VerdictRow[], started: boolean): Verdict {
  const rows = [...unsorted].sort((left, right) => order[left.state] - order[right.state])
  const active = rows.filter((row) => row.state !== 'skipped')
  const reviewCount = active.filter((row) => row.state === 'review').length
  const overall = !started ? 'pending' : reviewCount > 0 ? 'review' : active.some((row) => row.state === 'unknown') ? 'incomplete' : 'ok'
  return { rows, overall, reviewCount }
}

function routeEvidence(result: LeakProbeResult, connections: LeakConnection[], zh: boolean): { state: VerdictState; note: string } {
  const owned = result.pid == null ? [] : connections.filter((connection) => connection.app === 'probe' && connection.pid === result.pid)
  if (owned.some((connection) => connection.verdict === 'direct-route')) return { state: 'review', note: zh ? '核心选择了直连路由' : 'The core selected a direct route' }
  if (owned.some((connection) => connection.verdict === 'proxy-route')) return { state: 'ok', note: zh ? '经核心代理路由' : 'Routed through a core proxy' }
  return { state: 'unknown', note: zh ? '未关联到核心路由证据' : 'No core routing evidence was correlated' }
}

function localRow(id: LeakProbeResult['id'], report: LeakReport | null, expected: string, zh: boolean): VerdictRow {
  const label = id === 'dns' ? (zh ? '本机 DNS' : 'Local DNS') : `${zh ? '本机' : 'Local'} ${id === 'ipv4' ? 'IPv4' : 'IPv6'} ${zh ? '出口' : 'egress'}`
  const result = report?.probes.results.find((item) => item.id === id)
  if (!result || result.status === 'pending' || result.status === 'running') return { key: `local-${id}`, label, values: [], state: 'unknown', note: zh ? '等待结果' : 'Waiting for result' }
  if (result.status !== 'passed') return { key: `local-${id}`, label, values: [], state: 'unknown', note: result.detail }
  const values = id === 'dns' ? result.addresses : result.publicIp ? [result.publicIp] : []
  const route = routeEvidence(result, report?.connections ?? [], zh)
  if (expected && result.publicIp && !same(result.publicIp, expected)) return { key: `local-${id}`, label, values, state: 'review', note: zh ? `与预期出口 ${expected} 不同；代理出口轮换也可能导致差异` : `Differs from expected egress ${expected}; proxy egress rotation can also cause this` }
  const matched = Boolean(expected && result.publicIp)
  return { key: `local-${id}`, label, values, state: route.state, note: matched ? `${route.note}${zh ? '，且与预期出口一致' : '; matches expected egress'}` : route.note }
}

function browserRow(family: 'ipv4' | 'ipv6', probe: AddressProbe | undefined, expected: string, local: string | null, zh: boolean): VerdictRow {
  const label = `${zh ? '浏览器' : 'Browser'} ${family === 'ipv4' ? 'IPv4' : 'IPv6'} ${zh ? '出口' : 'egress'}`
  const key = `browser-${family}`
  if (!probe || probe.status === 'running') return { key, label, values: [], state: 'unknown', note: zh ? '等待结果' : 'Waiting for result' }
  if (!probe.ip) return { key, label, values: [], state: 'unknown', note: probe.detail }
  const baseline = expected || local
  if (!baseline) return { key, label, values: [probe.ip], state: 'unknown', note: zh ? '没有预期出口或本机探针出口可比对' : 'No expected or local probe egress to compare' }
  const source = expected ? (zh ? '预期出口' : 'expected egress') : (zh ? '本机探针出口' : 'local probe egress')
  return same(probe.ip, baseline)
    ? { key, label, values: [probe.ip], state: 'ok', note: zh ? `与${source}一致` : `Matches ${source}` }
    : { key, label, values: [probe.ip], state: 'review', note: zh ? `与${source} ${baseline} 不同` : `Differs from ${source} ${baseline}` }
}

function webRtcRow(browser: BrowserReport | null, baselines: Record<'ipv4' | 'ipv6', string>, zh: boolean): VerdictRow {
  const label = zh ? 'WebRTC 公网候选' : 'WebRTC public candidates'
  const webrtc = browser?.webrtc
  if (!webrtc || webrtc.status === 'running') return { key: 'webrtc', label, values: [], state: 'unknown', note: zh ? '等待结果' : 'Waiting for result' }
  const publics = webrtc.candidates.filter((candidate) => candidate.scope === 'public').map((candidate) => candidate.address)
  if (!publics.length) return { key: 'webrtc', label, values: [], state: webrtc.status === 'observed' ? 'ok' : 'unknown', note: webrtc.status === 'observed' ? (zh ? '未暴露公网候选地址' : 'No public candidate exposed') : webrtc.detail }
  const differing = publics.filter((address) => {
    const baseline = baselines[address.includes(':') ? 'ipv6' : 'ipv4']
    return !baseline || !same(address, baseline)
  })
  return differing.length
    ? { key: 'webrtc', label, values: publics, state: 'review', note: zh ? `${differing.join('、')} 与出口不同或无法比对` : `${differing.join(', ')} differs from egress or cannot be compared` }
    : { key: 'webrtc', label, values: publics, state: 'ok', note: zh ? '公网候选与出口一致' : 'Public candidates match egress' }
}

export function quickVerdict(input: QuickVerdictInput, zh: boolean): Verdict {
  const { report, browser, localProbes, includeBrowser } = input
  const expected = { ipv4: input.expectedIpv4.trim(), ipv6: input.expectedIpv6.trim() }
  const localIp = (id: 'ipv4' | 'ipv6') => localProbes ? report?.probes.results.find((item) => item.id === id)?.publicIp ?? null : null
  const skipped = (key: string, label: string, note: string): VerdictRow => ({ key, label, values: [], state: 'skipped', note })
  const rows: VerdictRow[] = localProbes
    ? [localRow('dns', report, '', zh), localRow('ipv4', report, expected.ipv4, zh), localRow('ipv6', report, expected.ipv6, zh)]
    : [skipped('local', zh ? '本机探针' : 'Local probes', zh ? '网关客户端模式不运行本机探针' : 'Local probes do not run for gateway clients')]
  if (includeBrowser) {
    rows.push(browserRow('ipv4', browser?.ipv4, expected.ipv4, localIp('ipv4'), zh), browserRow('ipv6', browser?.ipv6, expected.ipv6, localIp('ipv6'), zh))
    rows.push(webRtcRow(browser, { ipv4: expected.ipv4 || browser?.ipv4.ip || '', ipv6: expected.ipv6 || browser?.ipv6.ip || '' }, zh))
  } else {
    rows.push(skipped('browser', zh ? '浏览器探针' : 'Browser probes', zh ? '已在高级选项中关闭' : 'Disabled in advanced options'))
  }
  return summarize(rows, Boolean(report || browser))
}

export function observationVerdict(report: LeakReport | null, zh: boolean): Verdict {
  const summary = report?.summary
  const count = (value: number | undefined) => value == null ? [] : [String(value)]
  const row = (key: string, label: string, value: number | undefined, state: (value: number) => VerdictState, note: string): VerdictRow =>
    ({ key, label, values: count(value), state: value == null ? 'unknown' : state(value), note })
  const observed = report?.connections.some((connection) => connection.app !== 'probe') ?? false
  const rows = [
    row('proxy', zh ? '代理路由连接' : 'Proxy-routed connections', summary?.proxyRoute, () => 'ok', zh ? '命中核心代理选路，不等于已证明物理出口' : 'Matched core proxy routing; does not prove physical egress'),
    row('direct', zh ? '直连路由连接' : 'Direct-routed connections', summary?.directRoute, (value) => value ? 'review' : 'ok', zh ? '核心选择直连，需按目标与策略核实' : 'The core selected direct routing; review target and policy'),
    row('physical', zh ? '物理接口连接' : 'Physical interface connections', summary?.physicalEgress, (value) => value ? 'review' : 'ok', zh ? '需核对是否为显式代理端点' : 'Check whether these are explicit proxy endpoints'),
    row('unattributed', zh ? '归属未确认' : 'Unattributed', summary?.unattributed, (value) => value ? 'unknown' : 'ok', zh ? '无法关联路由，不能直接判定绕过' : 'Routing could not be correlated; does not prove bypass'),
  ]
  if (report && !observed) rows.push({ key: 'none', label: zh ? '目标应用连接' : 'Target application traffic', values: [], state: 'unknown', note: zh ? '未观测到目标应用连接；请在观测期间使用 Codex / Claude Code' : 'No target traffic observed; use Codex / Claude Code during observation' })
  return summarize(rows, Boolean(report))
}

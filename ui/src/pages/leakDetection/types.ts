export interface LeakProcess {
  pid: number
  parentPid: number | null
  startedAt: number
  name: string
  app: string
}

export interface LeakPreflight {
  platform: string
  processes: LeakProcess[]
  capabilities: { id: string; status: string; detail: string }[]
  gaps: string[]
}

export interface LeakConnection {
  id: string
  pid: number | null
  app: string
  protocol: string
  source: string
  destination: string
  host: string
  ipVersion: 'ipv4' | 'ipv6' | 'unknown'
  physicalEgress: boolean
  verdict: 'proxy-route' | 'direct-route' | 'unattributed'
  rule: string
  chains: string[]
  evidence: string[]
}

export interface LeakProbeResult {
  id: 'dns' | 'ipv4' | 'ipv6'
  status: 'pending' | 'running' | 'passed' | 'failed' | 'timeout' | 'stopped'
  detail: string
  pid: number | null
  addresses: string[]
  publicIp: string | null
  elapsedMs: number
}

export interface LeakProbes {
  available: boolean
  reason: string
  status: 'idle' | 'running' | 'completed' | 'stopped' | 'unavailable'
  results: LeakProbeResult[]
}

export interface LeakSummary {
  physicalEgress: number
  proxyRoute: number
  directRoute: number
  unattributed: number
  ipv4: number
  ipv6: number
}

export interface LeakReport {
  id: string
  startedAt: string
  elapsedSeconds: number
  target: string
  topology: string
  platform: string
  processes: LeakProcess[]
  connections: LeakConnection[]
  dns: { scope: string; protocol: string; server: string; evidence: string }[]
  gaps: string[]
  capture: { status: string; detail: string; packets: { interface: string; interfaceKind: string; direction: string; protocol: string; sourceIp: string; sourcePort: number; destinationIp: string; destinationPort: number }[] }
  ready: boolean
  probes: LeakProbes
  probeSummary: LeakSummary
  summary: LeakSummary
  completed: boolean
}

export interface AddressProbe {
  status: 'running' | 'observed' | 'unknown'
  endpoint: string
  ip?: string
  detail: string
}

export interface IceAddress {
  address: string
  type: string
  protocol: string
  scope: 'public' | 'nonpublic' | 'mdns' | 'unknown'
}

export interface BrowserReport {
  running: boolean
  startedAt: string
  origin: string
  expectedIpv4: string
  expectedIpv6: string
  ipv4: AddressProbe
  ipv6: AddressProbe
  webrtc: { status: 'running' | 'observed' | 'unknown'; candidates: IceAddress[]; detail: string }
  completed: boolean
}

export type NetworkDiagnosticStatus = 'passed' | 'failed' | 'warning' | 'skipped'
export type NetworkDiagnosticLayerID = 'runtime' | 'dns' | 'route' | 'tcp' | 'tls' | 'http'

export interface NetworkDiagnosticLayer {
  id: NetworkDiagnosticLayerID
  status: NetworkDiagnosticStatus
  summary: string
  evidence?: string[]
}

export interface NetworkDiagnosticFinding {
  code: 'runtime_not_running' | 'dns_failure' | 'fake_ip_route_conflict' | 'tcp_failure' | 'tls_failure' | 'http_failure' | string
  severity: 'error' | 'warning' | 'info'
  title: string
  detail: string
  solutions: string[]
}

export interface NetworkDiagnosticReport {
  checked_at: string
  target: string
  host: string
  port: number
  status: NetworkDiagnosticStatus
  layers: NetworkDiagnosticLayer[]
  findings: NetworkDiagnosticFinding[]
}

export interface NetworkTraceStep {
  id: string
  phase: 'frontend' | 'dns' | 'connection' | 'core' | 'tls' | 'request' | 'http'
  title: string
  status: 'running' | 'passed' | 'failed' | 'unknown'
  source: 'session' | 'frontend' | 'core' | 'socket' | 'http' | 'config'
  detail: string
  evidence: string[]
  attempt_id?: string
  elapsed_ms: number
  receivedAt?: number
  rule?: { type: string; payload: string }
  chains?: string[]
  chain_order?: 'core_api'
}

export interface NetworkTracePreview {
  mode: 'preview'
  path: string[]
  target: string
  steps: NetworkTraceStep[]
}

export interface NetworkTraceRun {
  generation: number
  target: string
  traceId?: string
  steps: NetworkTraceStep[]
  running: boolean
  status?: 'passed' | 'failed' | 'warning'
  error?: string
}

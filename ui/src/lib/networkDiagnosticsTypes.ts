export type NetworkDiagnosticStatus = 'passed' | 'failed' | 'warning' | 'skipped'

export interface NetworkDiagnosticLayer {
  id: 'runtime' | 'dns' | 'route' | 'tcp' | 'tls' | 'http'
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

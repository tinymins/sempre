export interface NetworkTestResult {
  id: string
  name: string
  region: 'domestic' | 'foreign'
  category: 'reachability' | 'ip'
  url: string
  ok: boolean
  latency_ms: number
  response_latency_ms?: number
  http_status?: number
  ip?: string
  ip_metadata?: IpMetadata
  detail?: string
}

export interface IpMetadata {
  country_code?: string
  country?: string
  region?: string
  city?: string
  asn?: number
  asn_organization?: string
  isp?: string
  organization?: string
}

export interface NetworkTestReport {
  checked_at: string
  results: NetworkTestResult[]
}

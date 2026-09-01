export interface DnsRewrite {
  id: string
  enabled: boolean
  domain: string
  type: string
  answer: string
  ttl: number
  comment: string
}

export interface DnsRoutingDomain {
  id: string
  domain: string
  include_subdomains: boolean
}

export interface DnsRoutingRuleSet {
  id: string
  name: string
  mode: 'direct' | 'proxy'
  domains: DnsRoutingDomain[]
}

export interface DnsSettings {
  schema: number
  revision: number
  enabled: boolean
  direct_upstreams: string[]
  rule_sets: DnsRoutingRuleSet[]
  reject_https: boolean
  rewrites: DnsRewrite[]
  query_log_enabled: boolean
  query_log_max_entries: number
}

export interface DnsFrontendStatus {
  enabled: boolean
  running: boolean
  core_dns_healthy: boolean
  mode: string
  core_upstream: string
  original_upstreams: string[]
  direct_upstreams: string[]
  domestic_domain_source: string
  domestic_domain_count: number
  last_error?: string
}

export interface DnsSettingsResponse {
  settings: DnsSettings
  status: DnsFrontendStatus
}

import { useQuery } from '@tanstack/react-query'
import { api } from '../../lib/api'
import { useSession } from '../../lib/session'

export interface FetchProxySummary {
  id: string
  name: string
  custom_node_id: string | null
  enabled: boolean
  protocol: string | null
  reference_count: number
  error: string | null
}

export interface FetchProxyInput { name: string; content: string; custom_node_id: string | null; enabled: boolean }

export type ResourceOutbound = { type: 'system' } | { type: 'direct' } | { type: 'proxy'; proxy_id: string }
export interface ResourceHostRule { host: string; outbound: ResourceOutbound }
export interface ResourceRules { revision: number; default_outbound: ResourceOutbound; host_rules: ResourceHostRule[] }
export interface ResourceRulesTest {
  ok: boolean
  host: string
  matched_host: string | null
  outbound: ResourceOutbound
  outbound_name: string
  http_status: number | null
  elapsed_ms: number
  body_bytes: number
  error?: string
}

export const fetchProxiesKey = ['fetch-proxies']

export function useFetchProxies() {
  const { session } = useSession()
  return useQuery({
    queryKey: fetchProxiesKey,
    queryFn: () => api<{ items: FetchProxySummary[] }>(session!, '/fetch-proxies'),
    enabled: Boolean(session),
  })
}

/** Without any proxy, sources keep the plain fetch mode choice. Proxies only run during a download. */
export function useSourceProxyOptions() {
  const items = useFetchProxies().data?.items
  if (!items?.length) return undefined
  return items.map(({ id, name, enabled }) => ({ id, name, enabled, status: enabled ? 'running' : 'stopped' }))
}

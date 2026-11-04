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

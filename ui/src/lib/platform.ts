import { useQuery } from '@tanstack/react-query'
import { api } from './api'
import { useSession } from './session'
import type { SystemStatus } from './types'

/** Pages for features that OpenWrt does not run, because Sempre only maintains sing-box and its rules there. */
export const OPENWRT_HIDDEN_PATHS: ReadonlySet<string> = new Set([
  '/custom-nodes',
  '/network-automation',
  '/routing-rules',
  '/dns',
  '/tunnels',
  '/gateway',
  '/leak-detection',
])

/** `undefined` while the server has not answered yet. */
export function useOpenwrt(): boolean | undefined {
  const { session } = useSession()
  const system = useQuery({
    queryKey: ['system'],
    queryFn: () => api<SystemStatus>(session!, '/system'),
    enabled: Boolean(session),
  })
  return system.isPending ? undefined : system.data?.capabilities?.openwrt === true
}

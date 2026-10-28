import { useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from './api'
import { useSession } from './session'
import { serverIdentityKey } from './serverIdentity'

export type UIMode = 'simple' | 'advanced'
type UISettings = { ui_mode: UIMode; friendly_name: string }

export function useUIMode() {
  const { session } = useSession()
  const queryClient = useQueryClient()
  const queryKey = ['ui-settings', session?.baseURL]
  const query = useQuery({
    queryKey,
    queryFn: () => api<UISettings>(session!, '/ui/settings'),
    enabled: Boolean(session),
    staleTime: 5000,
    refetchInterval: 5000,
  })

  return {
    ...query,
    mode: query.data?.ui_mode,
    friendlyName: query.data?.friendly_name ?? '',
    async setFriendlyName(name: string) {
      const settings = await api<UISettings>(session!, '/ui/settings', { method: 'PUT', body: JSON.stringify({ friendly_name: name }) })
      queryClient.setQueryData(queryKey, settings)
      queryClient.setQueryData(serverIdentityKey(session!.baseURL), settings.friendly_name)
    },
    async setMode(mode: UIMode) {
      const settings = await api<UISettings>(session!, '/ui/settings', { method: 'PUT', body: JSON.stringify({ ui_mode: mode }) })
      queryClient.setQueryData(queryKey, settings)
    },
  }
}

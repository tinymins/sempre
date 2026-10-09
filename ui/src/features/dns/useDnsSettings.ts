import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useToast } from '@acme/components'
import { api } from '../../lib/api'
import { useSession } from '../../lib/session'
import type { DnsSettings, DnsSettingsResponse } from './types'

const queryKey = ['dns', 'settings']

export function useDnsSettings() {
  const { session } = useSession()
  const queryClient = useQueryClient()
  const message = useToast()
  const settings = useQuery({
    queryKey,
    queryFn: ({ signal }) => api<DnsSettingsResponse>(session!, '/dns/settings', { signal }),
    enabled: Boolean(session),
    refetchInterval: 5000,
  })
  const save = useMutation({
    scope: { id: 'dns-settings' },
    mutationFn: async (update: (current: DnsSettings) => DnsSettings) => {
      await queryClient.cancelQueries({ queryKey })
      const current = queryClient.getQueryData<DnsSettingsResponse>(queryKey)!
      return api<DnsSettingsResponse>(session!, '/dns/settings', { method: 'PUT', body: JSON.stringify(update(current.settings)) })
    },
    onSuccess: async (response) => {
      await queryClient.cancelQueries({ queryKey })
      queryClient.setQueryData(queryKey, response)
      void queryClient.invalidateQueries({ queryKey: ['system'] })
    },
    onError: (error) => message.error(error.message),
  })
  return { settings, save }
}

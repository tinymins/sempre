import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useToast } from '@acme/components'
import { api } from '../../lib/api'
import { useSession } from '../../lib/session'
import { useI18n } from '../../lib/i18n'
import type { DnsSettings, DnsSettingsResponse } from './types'

const queryKey = ['dns', 'settings']

export function useDnsSettings() {
  const { session } = useSession()
  const queryClient = useQueryClient()
  const message = useToast()
  const { locale } = useI18n()
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
      void queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] })
      const change = response.change
      if (!change || (!change.Changed && !change.NeedsRestart)) return
      const zh = locale === 'zh-CN'
      if (change.Message === 'dns_rule_sets_published') {
        message.success(zh ? '规则已发布，核心将自动重载。新连接使用新规则。' : 'Rules published; the core reloads automatically. New connections use the updated rules.')
      } else if (change.Message === 'dns_rule_sets_legacy_restart_required') {
        message.info(zh ? '规则已保存。请重启核心一次以启用规则集自动重载。' : 'Rules saved. Restart the core once to enable automatic rule-set reloads.')
      } else if (change.NeedsRestart) {
        message.info(zh ? '配置已保存，需要重启核心后生效。' : 'Configuration saved. Restart the core to apply it.')
      }
    },
    onError: (error) => message.error(error.message),
  })
  return { settings, save }
}

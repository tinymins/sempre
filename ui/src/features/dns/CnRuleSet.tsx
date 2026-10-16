import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Select, Switch, useToast } from '@acme/components'
import { api } from '../../lib/api'
import { useSession } from '../../lib/session'
import type { NetworkSettings, NetworkSettingsResponse, ProxyNode } from '../../lib/types'
import { RuleSetRuntime } from './RuleSetRuntime'

const NAME = 'CN pre-routing'

export function CnRuleSet({ proxyGroups, selecting, zh, onSelectProxy }: { proxyGroups: ProxyNode[]; selecting: boolean; zh: boolean; onSelectProxy: (group: string, proxy: string) => void }) {
  const { session } = useSession()
  const client = useQueryClient()
  const toast = useToast()
  const network = useQuery({ queryKey: ['network', 'settings'], queryFn: () => api<NetworkSettingsResponse>(session!, '/network/settings') })
  const update = useMutation({
    mutationFn: async (patch: Partial<NetworkSettings>) => {
      const latest = await api<NetworkSettingsResponse>(session!, '/network/settings')
      return api<NetworkSettingsResponse>(session!, '/network/settings', { method: 'PUT', body: JSON.stringify({ ...latest.settings, ...patch }) })
    },
    onSuccess: (result) => {
      client.setQueryData(['network', 'settings'], result)
      client.invalidateQueries({ queryKey: ['system'] })
      toast.success(zh ? '已保存，重启核心后生效。' : 'Saved. Restart the core to apply.')
    },
    onError: (error) => toast.error(error.message),
  })
  const settings = network.data?.settings
  const mode = settings?.cn_routing_mode ?? 'direct-route'
  const enabled = settings?.cn_direct_routes ?? false
  const disabled = !settings || settings.mode !== 'local' || update.isPending
  const group = proxyGroups.find((item) => item.name === `DNS · ${NAME}` && item.all?.length)
  return <div className="space-y-4">
    <div className="flex items-start justify-between gap-3">
      <div><h2 className="font-semibold">{zh ? 'CN 前置分流' : NAME}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '内置 geoip-cn，按真实目标 IP 在域名及核心分流规则之前生效。' : 'Built-in geoip-cn matches real destination IPs before domain and core routing rules.'}</p></div>
      <Switch aria-label={zh ? '启用 CN 前置分流' : 'Enable CN pre-routing'} checked={enabled} disabled={disabled} onChange={(cn_direct_routes) => update.mutate({ cn_direct_routes })} />
    </div>
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm"><span>{zh ? '分流方式' : 'Routing mode'}</span><Select className="w-48" aria-label={zh ? 'CN 前置分流方式' : 'CN pre-routing mode'} disabled={disabled} value={mode} options={[{ value: 'direct-route', label: zh ? '直连路由' : 'Direct system route' }, { value: 'proxy', label: zh ? '代理' : 'Proxy' }]} onChange={(cn_routing_mode) => update.mutate({ cn_routing_mode })} /></label>
    {!enabled ? <Alert type="info" showIcon message={zh ? '默认关闭；启用后使用真实 IP DNS。' : 'Disabled by default. Uses real-IP DNS when enabled.'} /> : mode === 'direct-route' ? <Alert type="info" showIcon message={zh ? 'CN IP 经物理出口直连，不进入 TUN；其他公网流量进入 TUN 后继续分流。' : 'CN IPs use the physical uplink without entering TUN. Other public traffic enters TUN for routing.'} /> : <RuleSetRuntime ruleSet={{ name: NAME, mode: 'proxy' }} proxyGroup={group} selecting={selecting} zh={zh} onSelect={onSelectProxy} />}
    {enabled && mode === 'proxy' ? <p className="text-sm text-[var(--muted)]">{zh ? 'CN IP 进入 TUN 后优先使用此代理组，覆盖大陆域名及自定义域名规则的直连选择。' : 'CN IPs enter TUN and use this proxy group before Mainland China and custom domain routing choices.'}</p> : null}
    <p className="text-xs text-[var(--muted)]">{zh ? '仅支持本机 sing-box TUN 模式。修改直接保存，重启核心后生效。' : 'Requires local sing-box TUN mode. Changes are saved directly and take effect after restarting the core.'}</p>
  </div>
}

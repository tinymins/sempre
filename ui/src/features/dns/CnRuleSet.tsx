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
      <div><h2 className="font-semibold">{zh ? 'CN 路由分流' : 'CN routing'}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '网络包层规则：按原始真实目标 IP 匹配 geoip-cn，与 DNS 域名分流分属不同层。' : 'Packet routing matches geoip-cn against original real destination IPs, separately from DNS domain routing.'}</p></div>
      <Switch aria-label={zh ? '启用 CN 路由分流' : 'Enable CN routing'} checked={enabled} disabled={disabled} onChange={(cn_direct_routes) => update.mutate({ cn_direct_routes })} />
    </div>
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm"><span>{zh ? '分流方式' : 'Routing mode'}</span><Select className="w-48" aria-label={zh ? 'CN 路由分流方式' : 'CN routing mode'} disabled={disabled} value={mode} options={[{ value: 'direct-route', label: zh ? '直连路由' : 'Direct system route' }, { value: 'proxy', label: zh ? '代理' : 'Proxy' }]} onChange={(cn_routing_mode) => update.mutate({ cn_routing_mode })} /></label>
    {!enabled ? <Alert type="info" showIcon message={zh ? '默认关闭；此项按目标 IP 分流，与下方域名规则分属不同层。' : 'Disabled by default. Destination IP routing is separate from the domain rules below.'} /> : mode === 'direct-route' ? <Alert type="info" showIcon message={zh ? '真实 CN IP 经物理出口直连；真实非 CN IP 和全部 FakeIP 流量进入核心。' : 'Real CN IPs use the physical uplink. Real non-CN IPs and all FakeIP traffic enter the core.'} /> : <RuleSetRuntime ruleSet={{ name: NAME, mode: 'proxy' }} proxyGroup={group} selecting={selecting} zh={zh} onSelect={onSelectProxy} />}
    {enabled && mode === 'proxy' ? <p className="text-sm text-[var(--muted)]">{zh ? '原始真实 CN 目标 IP 进入核心后优先使用此出口；FakeIP 流量仍按核心中的域名策略分流。' : 'Original real CN destinations use this outbound first inside the core. FakeIP traffic follows the core domain policy.'}</p> : null}
    <p className="text-xs text-[var(--muted)]">{zh ? '仅支持本机 sing-box TUN 模式。修改直接保存，重启核心后生效。' : 'Requires local sing-box TUN mode. Changes are saved directly and take effect after restarting the core.'}</p>
  </div>
}

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Alert, Select, Switch, useToast } from '@acme/components'
import { api } from '../../lib/api'
import { useSession } from '../../lib/session'
import type { NetworkSettings, NetworkSettingsResponse, ProxyNode, SystemStatus } from '../../lib/types'
import { RuleSetRuntime } from './RuleSetRuntime'

export function IntranetRuleSet({ proxyGroups, selecting, zh, onSelectProxy }: { proxyGroups: ProxyNode[]; selecting: boolean; zh: boolean; onSelectProxy: (group: string, proxy: string) => void }) {
  const { session } = useSession()
  const client = useQueryClient()
  const toast = useToast()
  const network = useQuery({ queryKey: ['network', 'settings'], queryFn: () => api<NetworkSettingsResponse>(session!, '/network/settings') })
  const system = useQuery({ queryKey: ['system'], queryFn: () => api<SystemStatus>(session!, '/system') })
  const core = system.data?.selected?.core ?? system.data?.active?.core
  const supported = !core || ['sing-box', 'mihomo', 'clash-rs'].includes(core)
  const proxySupported = !core || core === 'sing-box' || (core === 'mihomo' && network.data?.platform === 'linux')
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
  const enabled = settings?.intranet_routing_enabled ?? true
  const mode = settings?.intranet_routing_mode ?? 'direct'
  const disabled = !settings || !supported || update.isPending
  const group = proxyGroups.find((item) => item.name === '内网地址' && item.all?.length)
  return <div className="space-y-4">
    <div className="flex items-start justify-between gap-3">
      <div><h2 className="font-semibold">{zh ? '内网分流' : 'Intranet routing'}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '按内网目标地址分流，显式配置的私网隧道规则优先。' : 'Route private destinations while preserving explicit private tunnel rules.'}</p></div>
      <Switch aria-label={zh ? '启用内网分流' : 'Enable intranet routing'} checked={enabled} disabled={!settings || update.isPending || (!supported && !enabled)} onChange={(intranet_routing_enabled) => update.mutate({ intranet_routing_enabled })} />
    </div>
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm"><span>{zh ? '分流方式' : 'Routing mode'}</span><Select className="w-48" aria-label={zh ? '内网分流方式' : 'Intranet routing mode'} disabled={disabled} value={mode} options={[{ value: 'direct', label: zh ? '直连' : 'Direct' }, { value: 'proxy', label: zh ? '代理' : 'Proxy', disabled: !proxySupported }]} onChange={(intranet_routing_mode) => update.mutate({ intranet_routing_mode })} /></label>
    {supported && !proxySupported ? <Alert type="info" showIcon message={zh ? '当前核心仅支持内网直连；内网代理接管需要 sing-box，或 Linux 上的 mihomo。' : 'This core supports direct intranet routing. Intranet proxy capture requires sing-box, or mihomo on Linux.'} /> : null}
    {!supported ? <Alert type="warning" showIcon message={zh ? '当前核心不支持内网分流，请使用 sing-box、mihomo 或 clash-rs。' : 'Intranet routing requires sing-box, mihomo or clash-rs.'} /> : null}
    {!enabled ? <Alert type="info" showIcon message={zh ? '已关闭通用内网分流，流量继续匹配后续规则；显式私网隧道规则仍然生效。' : 'The general intranet policy is disabled. Later rules apply; explicit private tunnel rules remain active.'} /> : mode === 'direct' ? <Alert type="info" showIcon message={zh ? '默认直连；匹配内网地址的流量通过核心直连出口。' : 'Direct by default. Private destinations use the core direct outbound.'} /> : <RuleSetRuntime ruleSet={{ name: '内网地址', mode: 'proxy' }} proxyGroup={group} selecting={selecting} zh={zh} onSelect={onSelectProxy} />}
    {enabled && mode === 'proxy' ? <p className="text-sm text-[var(--muted)]">{zh ? '内网流量进入核心后使用所选出口；保留本机、网关、DNS 上游和隧道连接所需的直连例外。' : 'Private traffic enters the core and uses the selected outbound. Host, gateway, DNS upstream and tunnel transport exceptions remain direct.'}</p> : null}
    <p className="text-xs text-[var(--muted)]">{zh ? '修改直接保存，重启核心后生效。' : 'Changes are saved directly and take effect after restarting the core.'}</p>
  </div>
}

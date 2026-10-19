import { Alert, Select } from '@acme/components'
import { RuntimeRestartButton } from '../../components/RuntimeRestartButton'
import { directLabel } from '../../lib/directLabel'
import type { ProxyNode } from '../../lib/types'
import type { DnsRoutingRuleSet } from './types'

export function RuleSetRuntime({ ruleSet, proxyGroup, selecting, zh, onSelect }: { ruleSet: Pick<DnsRoutingRuleSet, 'mode' | 'name'>; proxyGroup?: ProxyNode; selecting: boolean; zh: boolean; onSelect: (group: string, proxy: string) => void }) {
  if (ruleSet.mode === 'direct') return <Alert type="info" showIcon message={zh ? '前置直连返回 RealIP；流量是否进入核心由网络包路由决定，不等同于核心直连。' : 'Frontend direct returns real IPs. Packet routing determines whether traffic enters the core; this is separate from core direct routing.'} />
  if (!proxyGroup) return <Alert type="warning" showIcon message={zh ? '规则已保存；当前核心尚未识别此代理分组，请重启核心。' : 'Rules are saved. Restart the core to load this proxy group.'} action={<RuntimeRestartButton showLabel />} />
  return <div className="grid gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-subtle)] p-3 md:grid-cols-[minmax(0,1fr)_minmax(14rem,24rem)] md:items-center">
    <div><div className="text-sm font-medium">{zh ? '核心出口快速切换' : 'Quick core outbound selection'}</div><div className="mt-1 text-xs text-[var(--muted)]">{proxyGroup.name}</div></div>
    <label className="text-sm"><span className="sr-only">{zh ? '核心出口' : 'Core outbound'}</span><Select className="w-full" value={proxyGroup.now ?? ''} showSearch options={(proxyGroup.all ?? []).map((name) => ({ value: name, label: directLabel(name) }))} filterOption={(input, option) => String(option?.label ?? '').toLowerCase().includes(input.toLowerCase()) || String(option?.value ?? '').toLowerCase().includes(input.toLowerCase())} disabled={selecting} onChange={(proxy) => onSelect(proxyGroup.name, String(proxy))} /></label>
  </div>
}


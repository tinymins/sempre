import { useState } from 'react'
import { Alert, AutoComplete } from '@acme/components'
import { RuntimeRestartButton } from '../../components/RuntimeRestartButton'
import { directLabel } from '../../lib/directLabel'
import type { ProxyNode } from '../../lib/types'
import type { DnsRoutingRuleSet } from './types'

export function RuleSetRuntime({ ruleSet, proxyGroup, selecting, zh, onSelect }: { ruleSet: Pick<DnsRoutingRuleSet, 'mode' | 'name'>; proxyGroup?: ProxyNode; selecting: boolean; zh: boolean; onSelect: (group: string, proxy: string) => void }) {
  const [search, setSearch] = useState<string | null>(null)
  if (ruleSet.mode === 'direct') return <Alert type="info" showIcon message={zh ? 'FakeIP 下返回 Real-IP 并绕过核心；Real-IP 下进入核心后显式直连。' : 'In FakeIP mode, return real IPs and bypass the core. In Real-IP mode, enter the core and explicitly route direct.'} />
  if (!proxyGroup) return <Alert type="warning" showIcon message={zh ? '规则已保存；当前核心尚未识别此代理分组，请重启核心。' : 'Rules are saved. Restart the core to load this proxy group.'} action={<RuntimeRestartButton showLabel />} />
  return <div className="grid gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-subtle)] p-3 md:grid-cols-[minmax(0,1fr)_minmax(14rem,24rem)] md:items-center">
    <div><div className="text-sm font-medium">{zh ? '代理节点快速切换' : 'Quick proxy selection'}</div><div className="mt-1 text-xs text-[var(--muted)]">{proxyGroup.name}</div></div>
    <label className="text-sm"><span className="sr-only">{zh ? '代理节点' : 'Proxy node'}</span><AutoComplete className="w-full" value={search ?? directLabel(proxyGroup.now ?? '')} options={(proxyGroup.all ?? []).map((name) => ({ value: name, label: directLabel(name) }))} filterOption={(input, option) => directLabel(option.value).toLowerCase().includes(input.toLowerCase()) || option.value.toLowerCase().includes(input.toLowerCase())} disabled={selecting} allowClear={false} onChange={setSearch} onFocus={() => setSearch('')} onBlur={() => setSearch(null)} onSelect={(proxy) => { setSearch(directLabel(proxy)); onSelect(proxyGroup.name, proxy) }} /></label>
  </div>
}


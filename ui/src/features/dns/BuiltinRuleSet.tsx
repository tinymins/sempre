import { Alert, Select, Switch } from '@acme/components'
import { directLabel } from '../../lib/directLabel'
import type { ProxyNode } from '../../lib/types'
import type { DnsSettings } from './types'
import { RuleSetRuntime } from './RuleSetRuntime'

type Policy = DnsSettings['domestic_domains']
const BUILTIN_NAME = 'Mainland China domains'

export function BuiltinRuleSet({ policy, count, saving, proxyGroups, selecting, zh, onChange, onSelectProxy }: {
  policy: Policy
  count: number
  saving: boolean
  proxyGroups: ProxyNode[]
  selecting: boolean
  zh: boolean
  onChange: (patch: Partial<Policy>) => void
  onSelectProxy: (group: string, proxy: string) => void
}) {
  const title = zh ? '中国大陆域名' : BUILTIN_NAME
  const proxyGroup = proxyGroups.find((item) => item.name === `DNS · ${BUILTIN_NAME}` && item.all?.length)
  return <div className="space-y-4">
    <div className="flex items-start justify-between gap-3">
      <div><h2 className="font-semibold">{title}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '内置 domains-min，随 Sempre 版本更新。' : 'Built-in domains-min, updated with Sempre.'}</p></div>
      <Switch aria-label={zh ? '启用中国大陆域名' : 'Enable Mainland China domains'} checked={policy.enabled} disabled={saving} onChange={(enabled) => onChange({ enabled })} />
    </div>
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm">
      <span>{zh ? '分流方式' : 'Routing mode'}</span>
      <Select className="w-48" aria-label={zh ? '中国大陆域名分流方式' : 'Mainland China routing mode'} disabled={saving || !policy.enabled} value={policy.mode} options={[{ value: 'direct', label: directLabel('direct') }, { value: 'proxy', label: zh ? '代理' : 'Proxy' }]} onChange={(mode) => onChange({ mode })} />
    </label>
    <div className="text-sm text-[var(--muted)]">{zh ? `包含 ${count.toLocaleString()} 个域名；自定义规则优先。` : `${count.toLocaleString()} domains; custom rules take priority.`}</div>
    {policy.enabled ? <RuleSetRuntime ruleSet={{ name: BUILTIN_NAME, mode: policy.mode }} proxyGroup={proxyGroup} selecting={selecting} zh={zh} onSelect={onSelectProxy} /> : <Alert type="info" showIcon message={zh ? '已关闭内置规则，域名继续按自定义规则及核心规则分流。' : 'Built-in rules are disabled. Custom and core routing rules continue to apply.'} />}
    <p className="text-xs text-[var(--muted)]">{zh ? '修改直接保存，重启核心后生效。' : 'Changes are saved directly and take effect after restarting the core.'}</p>
  </div>
}

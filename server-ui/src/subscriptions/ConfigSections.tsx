import { Button, Checkbox, Select, TextArea } from '@acme/components'
import { ArrowDown, ArrowUp } from 'lucide-react'
import { DnsConfigEditor } from './DnsConfigEditor'
import type { Subscription, SubscriptionDefaults, SubscriptionDraft, UserBrief } from './types'

interface Props {
  draft: SubscriptionDraft
  defaults: SubscriptionDefaults | null
  users: UserBrief[]
  canManageAuthorization: boolean
  update: (patch: Partial<SubscriptionDraft>) => void
}

type InheritedField = 'ruleList' | 'group' | 'filter' | 'customConfig' | 'dnsConfig'
type InheritFlag = 'useSystemRuleList' | 'useSystemGroup' | 'useSystemFilter' | 'useSystemCustomConfig' | 'useSystemDnsConfig'

const fields: Record<InheritedField, { label: string; flag: InheritFlag }> = {
  ruleList: { label: '规则列表', flag: 'useSystemRuleList' },
  group: { label: '代理分组', flag: 'useSystemGroup' },
  filter: { label: '节点过滤器', flag: 'useSystemFilter' },
  customConfig: { label: '自定义配置', flag: 'useSystemCustomConfig' },
  dnsConfig: { label: 'DNS 配置', flag: 'useSystemDnsConfig' },
}

export function InheritedConfig({ field, draft, defaults, update }: Pick<Props, 'draft' | 'defaults' | 'update'> & { field: InheritedField }) {
  const { label, flag } = fields[field]
  const useSystem = draft[flag]
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <label className="text-sm font-medium" htmlFor={`config-${field}`}>{label}</label>
        <Checkbox checked={useSystem} onChange={(event) => update({ [flag]: event.target.checked })}>使用系统默认值</Checkbox>
      </div>
      {useSystem ? (
        <>
          <p className="text-xs text-[var(--muted)]">当前生效的系统默认值。取消继承后可继续编辑原先保存的自定义值。</p>
          <TextArea id={`config-${field}`} value={defaults?.[field] ?? ''} readOnly rows={14} className="font-mono text-xs" />
        </>
      ) : field === 'dnsConfig' ? (
        <DnsConfigEditor value={draft.dnsConfig} onChange={(next) => update({ dnsConfig: next })} />
      ) : (
        <TextArea id={`config-${field}`} value={draft[field] ?? ''} onChange={(event) => update({ [field]: event.target.value || null })} rows={14} className="font-mono text-xs" />
      )}
    </div>
  )
}

export function BasicConfig({ draft, users, canManageAuthorization, update }: Omit<Props, 'defaults'>) {
  return (
    <div className="space-y-4">
      <label className="block space-y-1 text-sm">备注
        <TextArea rows={3} value={draft.remark ?? ''} onChange={(event) => update({ remark: event.target.value || null })} />
      </label>
      <label className="block space-y-1 text-sm">日志级别
        <Select value={draft.logLevel} options={['off', 'error', 'warn', 'info', 'debug'].map((level) => ({ value: level, label: level }))} onChange={(next) => update({ logLevel: next as SubscriptionDraft['logLevel'] })} className="w-full" />
      </label>
      <label className="block space-y-1 text-sm">授权用户
        <Select mode="multiple" value={draft.authorizedUserIds} disabled={!canManageAuthorization} options={users.map((user) => ({ value: user.id, label: `${user.name} (${user.email})` }))} onChange={(next) => update({ authorizedUserIds: next as string[] })} showSearch className="w-full" />
      </label>
      {!canManageAuthorization ? <p className="text-xs text-[var(--muted)]">只有配置集创建者可以修改授权用户。</p> : null}
    </div>
  )
}

export function ExtraConfig({ draft, assignedNodes, update }: Pick<Props, 'draft' | 'update'> & { assignedNodes: Subscription['assignedCustomNodes'] }) {
  const move = (index: number, direction: -1 | 1) => {
    const target = index + direction
    if (target < 0 || target >= draft.selectedCustomNodeIds.length) return
    const ids = [...draft.selectedCustomNodeIds]
    ;[ids[index], ids[target]] = [ids[target], ids[index]]
    update({ selectedCustomNodeIds: ids })
  }
  return (
    <div className="space-y-4">
      <label className="block space-y-1 text-sm">手写额外节点（JSONC）
        <TextArea rows={10} value={draft.servers ?? ''} onChange={(event) => update({ servers: event.target.value || null })} className="font-mono text-xs" />
      </label>
      <label className="block space-y-1 text-sm">选用的自定义节点
        <Select mode="multiple" value={draft.selectedCustomNodeIds} options={assignedNodes.map((node) => ({ value: node.id, label: `${node.name} · ${node.proxyType} · ${node.server}:${node.port}` }))} onChange={(next) => update({ selectedCustomNodeIds: next as string[] })} showSearch className="w-full" />
      </label>
      {draft.selectedCustomNodeIds.map((id, index) => {
        const node = assignedNodes.find((item) => item.id === id)
        return <div key={id} className="flex items-center justify-between gap-2 rounded border border-[var(--border)] px-2 py-1 text-sm">
          <span>{node?.name ?? id}</span><span className="flex gap-1"><Button size="small" icon={<ArrowUp size={14} />} aria-label={`上移自定义节点 ${index + 1}`} disabled={index === 0} onClick={() => move(index, -1)} /><Button size="small" icon={<ArrowDown size={14} />} aria-label={`下移自定义节点 ${index + 1}`} disabled={index === draft.selectedCustomNodeIds.length - 1} onClick={() => move(index, 1)} /></span>
        </div>
      })}
    </div>
  )
}

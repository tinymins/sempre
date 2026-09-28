import { Button, Checkbox, Select, TextArea } from '@acme/components'
import { ArrowDown, ArrowUp } from 'lucide-react'
import { DnsConfigEditor } from './DnsConfigEditor'
import { FilterConfig } from './FilterConfig'
import type { Subscription, SubscriptionDefaults, SubscriptionDraft, UserBrief } from './types'
import { useI18n } from '../i18n/provider'

interface Props {
  draft: SubscriptionDraft
  defaults: SubscriptionDefaults | null
  users: UserBrief[]
  canManageAuthorization: boolean
  update: (patch: Partial<SubscriptionDraft>) => void
}

type InheritedField = 'ruleList' | 'group' | 'filter' | 'customConfig' | 'dnsConfig'
type InheritFlag = 'useSystemRuleList' | 'useSystemGroup' | 'useSystemFilter' | 'useSystemCustomConfig' | 'useSystemDnsConfig'

const fields: Record<InheritedField, { labelKey: 'editor.tabRules' | 'editor.tabGroup' | 'editor.filter' | 'editor.tabCustom' | 'editor.tabDns'; flag: InheritFlag }> = {
  ruleList: { labelKey: 'editor.tabRules', flag: 'useSystemRuleList' },
  group: { labelKey: 'editor.tabGroup', flag: 'useSystemGroup' },
  filter: { labelKey: 'editor.filter', flag: 'useSystemFilter' },
  customConfig: { labelKey: 'editor.tabCustom', flag: 'useSystemCustomConfig' },
  dnsConfig: { labelKey: 'editor.tabDns', flag: 'useSystemDnsConfig' },
}

export function InheritedConfig({ field, draft, defaults, update, dnsInvalid = false, onDnsInvalidChange }: Pick<Props, 'draft' | 'defaults' | 'update'> & { field: InheritedField; dnsInvalid?: boolean; onDnsInvalidChange?: (invalid: boolean) => void }) {
  const { t } = useI18n()
  const { labelKey, flag } = fields[field]
  const useSystem = draft[flag]
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <label className="text-sm font-medium" htmlFor={`config-${field}`}>{t(labelKey)}</label>
        <Checkbox checked={useSystem} disabled={field === 'dnsConfig' && dnsInvalid} onChange={(event) => update({ [flag]: event.target.checked })}>{t('editor.inherit')}</Checkbox>
      </div>
      {useSystem ? (
        <>
          <p className="text-xs text-[var(--muted)]">{t('editor.inheritHint')}</p>
          {field === 'filter' ? <FilterConfig value={defaults?.filter ?? '[]'} readOnly /> : <TextArea id={`config-${field}`} value={defaults?.[field] ?? ''} readOnly rows={14} className="font-mono text-xs" />}
        </>
      ) : field === 'dnsConfig' ? (
        <DnsConfigEditor value={draft.dnsConfig} onChange={(next) => update({ dnsConfig: next })} onInvalidChange={onDnsInvalidChange} />
      ) : field === 'filter' ? (
        <FilterConfig value={draft.filter ?? '[]'} onChange={(next) => update({ filter: next || null })} />
      ) : (
        <TextArea id={`config-${field}`} value={draft[field] ?? ''} onChange={(event) => update({ [field]: event.target.value || null })} rows={14} className="font-mono text-xs" />
      )}
    </div>
  )
}

export function BasicConfig({ draft, users, canManageAuthorization, update }: Omit<Props, 'defaults'>) {
  const { t } = useI18n()
  return (
    <div className="space-y-4">
      <label className="block space-y-1 text-sm">{t('configs.remark')}
        <TextArea rows={3} value={draft.remark ?? ''} onChange={(event) => update({ remark: event.target.value || null })} />
      </label>
      <label className="block space-y-1 text-sm">{t('editor.logLevel')}
        <Select value={draft.logLevel} options={['off', 'error', 'warn', 'info', 'debug'].map((level) => ({ value: level, label: level }))} onChange={(next) => update({ logLevel: next as SubscriptionDraft['logLevel'] })} className="w-full" />
      </label>
      <label className="block space-y-1 text-sm">{t('editor.authorizedUsers')}
        <Select mode="multiple" value={draft.authorizedUserIds} disabled={!canManageAuthorization} options={users.map((user) => ({ value: user.id, label: `${user.name} (${user.email})` }))} onChange={(next) => update({ authorizedUserIds: next as string[] })} showSearch placeholder={t('editor.authorizedUsers')} className="w-full" />
      </label>
      {!canManageAuthorization ? <p className="text-xs text-[var(--muted)]">{t('editor.ownerOnly')}</p> : null}
    </div>
  )
}

export function ExtraConfig({ draft, assignedNodes, update }: Pick<Props, 'draft' | 'update'> & { assignedNodes: Subscription['assignedCustomNodes'] }) {
  const { t, number } = useI18n()
  const move = (index: number, direction: -1 | 1) => {
    const target = index + direction
    if (target < 0 || target >= draft.selectedCustomNodeIds.length) return
    const ids = [...draft.selectedCustomNodeIds]
    ;[ids[index], ids[target]] = [ids[target], ids[index]]
    update({ selectedCustomNodeIds: ids })
  }
  return (
    <div className="space-y-4">
      <label className="block space-y-1 text-sm">{t('editor.manualServers')}
        <TextArea rows={10} value={draft.servers ?? ''} onChange={(event) => update({ servers: event.target.value || null })} className="font-mono text-xs" />
      </label>
      <label className="block space-y-1 text-sm">{t('editor.selectedNodes')}
        <Select mode="multiple" value={draft.selectedCustomNodeIds} options={assignedNodes.map((node) => ({ value: node.id, label: `${node.name} · ${node.proxyType} · ${node.server}:${node.port}` }))} onChange={(next) => update({ selectedCustomNodeIds: next as string[] })} showSearch className="w-full" />
      </label>
      {draft.selectedCustomNodeIds.map((id, index) => {
        const node = assignedNodes.find((item) => item.id === id)
        return <div key={id} className="flex items-center justify-between gap-2 rounded border border-[var(--border)] px-2 py-1 text-sm">
          <span>{node?.name ?? id}</span><span className="flex gap-1"><Button size="small" icon={<ArrowUp size={14} />} aria-label={t('editor.moveNodeUp', { index: number(index + 1) })} disabled={index === 0} onClick={() => move(index, -1)} /><Button size="small" icon={<ArrowDown size={14} />} aria-label={t('editor.moveNodeDown', { index: number(index + 1) })} disabled={index === draft.selectedCustomNodeIds.length - 1} onClick={() => move(index, 1)} /></span>
        </div>
      })}
    </div>
  )
}

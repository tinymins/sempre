import { useMemo, useState } from 'react'
import { Alert, Button, Card, Input, Modal, Select } from '@acme/components'
import { Pencil, Plus, Trash2 } from 'lucide-react'
import { RuntimeRestartButton } from '../../components/RuntimeRestartButton'
import { useI18n } from '../../lib/i18n'
import { directLabel, isBuiltinDirect } from '../../lib/directLabel'
import { randomUuid } from '../../lib/randomUuid'
import type { ProxyNode } from '../../lib/types'
import type { DnsRoutingDomain, DnsRoutingRuleSet, DnsSettings } from './types'

const DIRECT = 'direct'
const GROUP_PREFIX = 'group:'
const NODE_PREFIX = 'node:'

export interface SimpleRoutingRow extends DnsRoutingDomain {
  ruleSetID?: string
  target: string
}

export interface SimpleRoutingSave {
  update: (current: DnsSettings) => DnsSettings
  selections: Record<string, string>
}

export function SimpleRoutingRules({ settings, proxyGroups, saving, pendingSelection, onSave }: { settings: DnsSettings; proxyGroups: ProxyNode[]; saving: boolean; pendingSelection: boolean; onSave: (value: SimpleRoutingSave) => Promise<void> }) {
  const { locale } = useI18n()
  const zh = locale === 'zh-CN'
  const rows = useMemo(() => flattenRules(settings, proxyGroups), [settings, proxyGroups])
  const [editing, setEditing] = useState<SimpleRoutingRow | null>(null)
  const options = useMemo(() => targetOptions(settings, proxyGroups, editing ? [...rows, editing] : rows), [settings, proxyGroups, rows, editing])

  const saveRows = (update: (current: SimpleRoutingRow[]) => SimpleRoutingRow[]) => {
    const { selections } = composeSimpleRouting(settings, update(rows), proxyGroups)
    return onSave({
      update: (latest) => composeSimpleRouting(latest, update(flattenRules(latest, proxyGroups)), proxyGroups).settings,
      selections,
    })
  }
  const submit = async () => {
    if (!editing || !normalizeDomain(editing.domain)) return
    const entry = { ...editing, domain: normalizeDomain(editing.domain) }
    await saveRows((current) => current.some((row) => row.id === entry.id)
      ? current.map((row) => row.id === entry.id ? entry : row)
      : [...current, entry])
    setEditing(null)
  }

  return <div className="space-y-4">
    <div className="flex min-h-10 items-start justify-between gap-4"><div><h1 className="text-xl font-semibold">{zh ? '域名分流' : 'Domain routing'}</h1><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '为域名选择直连或指定节点，修改后直接保存。' : 'Choose direct access or a node for each domain. Changes are saved directly.'}</p></div></div>
    {pendingSelection ? <Alert type="info" showIcon message={zh ? '待重启核心以应用新规则对应的节点选择。' : 'Restart the core to apply the pending node selections.'} action={<RuntimeRestartButton showLabel />} /> : null}
    <Card className="p-4 md:p-5">
      <div className="space-y-3">
        {rows.map((row, index) => <div key={row.id} className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(12rem,18rem)_5rem]">
          <span className="self-center truncate text-sm">{row.domain}</span>
          <Select aria-label={`${zh ? '分流节点' : 'Routing node'} ${index + 1}`} disabled={saving} className="w-full" popupMatchSelectWidth value={row.target} options={options} onChange={(target) => void saveRows((current) => current.map((item) => item.id === row.id ? { ...item, target: String(target) } : item)).catch(() => undefined)} />
          <div className="flex gap-1"><Button size="small" variant="text" disabled={saving} aria-label={zh ? '编辑域名' : 'Edit domain'} icon={<Pencil size={16} />} onClick={() => setEditing({ ...row })} /><Button size="small" variant="text" disabled={saving} aria-label={zh ? '删除域名' : 'Delete domain'} icon={<Trash2 size={16} />} onClick={() => void saveRows((current) => current.filter((item) => item.id !== row.id)).catch(() => undefined)} /></div>
        </div>)}
        {!rows.length ? <p className="py-6 text-center text-sm text-[var(--muted)]">{zh ? '还没有分流域名。' : 'No routed domains yet.'}</p> : null}
      </div>
      <Button className="mt-4" disabled={saving} onClick={() => setEditing({ id: randomUuid(), domain: '', include_subdomains: true, target: DIRECT })}><Plus size={16} />{zh ? '添加域名' : 'Add domain'}</Button>
    </Card>
    <Modal open={Boolean(editing)} title={zh ? '域名分流' : 'Domain routing'} okText={zh ? '保存' : 'Save'} cancelText={zh ? '取消' : 'Cancel'} confirmLoading={saving} okButtonProps={{ disabled: !normalizeDomain(editing?.domain ?? '') }} onOk={() => submit().catch(() => undefined)} onCancel={() => { if (!saving) setEditing(null) }} destroyOnClose>
      {editing ? <div className="grid gap-4">
        <label className="text-sm"><span className="mb-2 block font-medium">{zh ? '域名' : 'Domain'}</span><Input autoFocus value={editing.domain} placeholder="example.com" onChange={(event) => setEditing({ ...editing, domain: event.target.value })} /></label>
        <label className="text-sm"><span className="mb-2 block font-medium">{zh ? '分流节点' : 'Routing node'}</span><Select className="w-full" value={editing.target} options={options} onChange={(target) => setEditing({ ...editing, target: String(target) })} /></label>
      </div> : null}
    </Modal>
  </div>
}

export function composeSimpleRouting(settings: DnsSettings, rows: SimpleRoutingRow[], proxyGroups: ProxyNode[]): { settings: DnsSettings; selections: Record<string, string> } {
  const grouped = new Map<string, SimpleRoutingRow[]>()
  rows.forEach((row) => grouped.set(row.target, [...(grouped.get(row.target) ?? []), row]))
  const used = new Set<string>()
  const selections: Record<string, string> = {}
  const ruleSets: DnsRoutingRuleSet[] = []

  grouped.forEach((bucket, target) => {
    const existing = pickRuleSet(target, bucket, settings.rule_sets, proxyGroups, used)
    const node = target.startsWith(NODE_PREFIX) ? target.slice(NODE_PREFIX.length) : ''
    const mode: DnsRoutingRuleSet['mode'] = target === DIRECT ? 'direct' : 'proxy'
    const id = existing?.id ?? randomUuid()
    const name = existing?.name ?? uniqueName(node || '代理分流', settings.rule_sets.map((item) => item.name), ruleSets.map((item) => item.name))
    used.add(id)
    ruleSets.push({
      ...(existing ?? { id, name, mode }),
      id,
      name,
      mode,
      domains: bucket.map((row) => ({ id: row.id, domain: row.domain, include_subdomains: row.include_subdomains })),
    })
    if (node) selections[`DNS · ${name}`] = node
  })

  settings.rule_sets.forEach((ruleSet) => {
    if (!used.has(ruleSet.id) && ruleSet.domains.length === 0) ruleSets.push(ruleSet)
  })
  return { settings: { ...settings, rule_sets: ruleSets }, selections }
}

function flattenRules(settings: DnsSettings, proxyGroups: ProxyNode[]): SimpleRoutingRow[] {
  return settings.rule_sets.flatMap((ruleSet) => ruleSet.domains.map((domain) => ({ ...domain, ruleSetID: ruleSet.id, target: ruleSet.mode === 'direct' ? DIRECT : currentTarget(ruleSet, proxyGroups) })))
}

function currentTarget(ruleSet: DnsRoutingRuleSet, proxyGroups: ProxyNode[]) {
  const current = proxyGroups.find((group) => group.name === `DNS · ${ruleSet.name}`)?.now
  return current ? `${NODE_PREFIX}${current}` : `${GROUP_PREFIX}${ruleSet.id}`
}

function targetOptions(settings: DnsSettings, proxyGroups: ProxyNode[], rows: SimpleRoutingRow[]) {
  const nodes = [...new Set(proxyGroups.flatMap((group) => group.all ?? []).filter((name) => !isBuiltinDirect(name) && name !== 'REJECT'))].sort((left, right) => left.localeCompare(right))
  const unresolved = [...new Set(rows.map((row) => row.target).filter((target) => target.startsWith(GROUP_PREFIX)))]
  return [
    { value: DIRECT, label: directLabel(DIRECT) },
    ...nodes.map((node) => ({ value: `${NODE_PREFIX}${node}`, label: directLabel(node) })),
    ...unresolved.map((target) => ({ value: target, label: settings.rule_sets.find((item) => item.id === target.slice(GROUP_PREFIX.length))?.name ?? '原有代理规则' })),
  ]
}

function pickRuleSet(target: string, rows: SimpleRoutingRow[], ruleSets: DnsRoutingRuleSet[], proxyGroups: ProxyNode[], used: Set<string>) {
  const original = rows.map((row) => row.ruleSetID).filter(Boolean).map((id) => ruleSets.find((item) => item.id === id)).find((item) => item && !used.has(item.id) && targetMatches(item, target, proxyGroups))
  if (original) return original
  if (target.startsWith(GROUP_PREFIX)) return ruleSets.find((item) => item.id === target.slice(GROUP_PREFIX.length) && !used.has(item.id))
  return ruleSets.find((item) => !used.has(item.id) && targetMatches(item, target, proxyGroups))
}

function targetMatches(ruleSet: DnsRoutingRuleSet, target: string, proxyGroups: ProxyNode[]) {
  if (target === DIRECT) return ruleSet.mode === 'direct'
  if (ruleSet.mode !== 'proxy' || !target.startsWith(NODE_PREFIX)) return false
  return proxyGroups.find((group) => group.name === `DNS · ${ruleSet.name}`)?.now === target.slice(NODE_PREFIX.length)
}

function uniqueName(base: string, existing: string[], created: string[]) {
  const used = new Set([...existing, ...created])
  let name = base
  let index = 2
  while (used.has(name)) name = `${base} ${index++}`
  return name
}

function normalizeDomain(value: string) {
  return value.trim().toLowerCase().replace(/^\*\./, '').replace(/^\./, '').replace(/\.$/, '')
}

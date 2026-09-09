import { useMemo, useState } from 'react'
import { Alert, Select } from '@acme/components'
import { Plus, Save, Trash2 } from 'lucide-react'
import { RuntimeRestartButton } from '../../components/RuntimeRestartButton'
import { Button, Card, Input, Spinner } from '../../components/ui'
import { useI18n } from '../../lib/i18n'
import type { ProxyNode } from '../../lib/types'
import type { DnsRoutingDomain, DnsRoutingRuleSet, DnsSettings } from './types'

const DIRECT = 'direct'
const GROUP_PREFIX = 'group:'
const NODE_PREFIX = 'node:'

function newID(prefix: string) {
  return crypto.randomUUID?.() ?? `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`
}

export interface SimpleRoutingRow extends DnsRoutingDomain {
  ruleSetID?: string
  target: string
}

export interface SimpleRoutingSave {
  settings: DnsSettings
  selections: Record<string, string>
}

export function SimpleRoutingRules({ settings, proxyGroups, saving, saved, pendingSelection, error, onSave }: { settings: DnsSettings; proxyGroups: ProxyNode[]; saving: boolean; saved: boolean; pendingSelection: boolean; error?: Error | null; onSave: (value: SimpleRoutingSave) => Promise<void> }) {
  const { locale } = useI18n()
  const zh = locale === 'zh-CN'
  const initial = useMemo(() => flattenRules(settings, proxyGroups), [settings, proxyGroups])
  const [rows, setRows] = useState<SimpleRoutingRow[]>(initial)
  const [validation, setValidation] = useState('')
  const options = useMemo(() => targetOptions(settings, proxyGroups, rows), [settings, proxyGroups, rows])
  const dirty = JSON.stringify(rows) !== JSON.stringify(initial)

  const submit = async () => {
    const normalized = rows.map((row) => ({ ...row, domain: normalizeDomain(row.domain) }))
    if (normalized.some((row) => !row.domain)) {
      setValidation(zh ? '域名不能为空。' : 'Domain is required.')
      return
    }
    setValidation('')
    await onSave(composeSimpleRouting(settings, normalized, proxyGroups))
  }

  return <div className="space-y-4">
    <div className="flex min-h-10 items-start justify-between gap-4"><div><h1 className="text-xl font-semibold">{zh ? '分流规则' : 'Routing rules'}</h1><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '为域名选择直连或指定节点。相同目标会在保存时自动归入同一规则集。' : 'Choose direct access or a node for each domain. Matching targets are grouped automatically when saved.'}</p></div><Button variant="primary" disabled={!dirty || saving} onClick={() => void submit()}>{saving ? <Spinner /> : <Save size={16} />}{zh ? '保存' : 'Save'}</Button></div>
    {error ? <Alert type="error" showIcon message={error.message} /> : null}
    {saved ? <Alert type="success" showIcon message={pendingSelection ? (zh ? '分流规则已保存。重启核心后会自动选择新规则对应的节点。' : 'Routing rules saved. After the core restarts, the new rule groups will select their nodes automatically.') : (zh ? '分流规则已保存；重启核心后应用。' : 'Routing rules saved. Restart the core to apply them.')} action={pendingSelection ? <RuntimeRestartButton showLabel /> : undefined} /> : null}
    <Card className="p-4 md:p-5">
      <div className="space-y-3">
        {rows.map((row, index) => <div key={row.id} className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(12rem,18rem)_2.25rem]">
          <Input aria-label={`${zh ? '域名' : 'Domain'} ${index + 1}`} value={row.domain} placeholder="example.com" onChange={(event) => setRows((current) => current.map((item) => item.id === row.id ? { ...item, domain: event.target.value } : item))} />
          <Select aria-label={`${zh ? '分流节点' : 'Routing node'} ${index + 1}`} className="w-full" popupMatchSelectWidth value={row.target} options={options} onChange={(target) => setRows((current) => current.map((item) => item.id === row.id ? { ...item, target: String(target) } : item))} />
          <Button size="icon" variant="ghost" aria-label={zh ? '删除域名' : 'Delete domain'} onClick={() => setRows((current) => current.filter((item) => item.id !== row.id))}><Trash2 size={16} /></Button>
        </div>)}
        {!rows.length ? <p className="py-6 text-center text-sm text-[var(--muted)]">{zh ? '还没有分流域名。' : 'No routed domains yet.'}</p> : null}
      </div>
      <Button className="mt-4" onClick={() => setRows((current) => [...current, { id: newID('domain'), domain: '', include_subdomains: true, target: DIRECT }])}><Plus size={16} />{zh ? '添加域名' : 'Add domain'}</Button>
      {validation ? <p role="alert" className="mt-3 text-sm text-red-600">{validation}</p> : null}
    </Card>
  </div>
}

export function composeSimpleRouting(settings: DnsSettings, rows: SimpleRoutingRow[], proxyGroups: ProxyNode[]): SimpleRoutingSave {
  const grouped = new Map<string, SimpleRoutingRow[]>()
  rows.forEach((row) => grouped.set(row.target, [...(grouped.get(row.target) ?? []), row]))
  const used = new Set<string>()
  const selections: Record<string, string> = {}
  const ruleSets: DnsRoutingRuleSet[] = []

  grouped.forEach((bucket, target) => {
    const existing = pickRuleSet(target, bucket, settings.rule_sets, proxyGroups, used)
    const node = target.startsWith(NODE_PREFIX) ? target.slice(NODE_PREFIX.length) : ''
    const mode: DnsRoutingRuleSet['mode'] = target === DIRECT ? 'direct' : 'proxy'
    const id = existing?.id ?? newID('rule-set')
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
  const nodes = [...new Set(proxyGroups.flatMap((group) => group.all ?? []).filter((name) => !['DIRECT', 'REJECT'].includes(name)))].sort((left, right) => left.localeCompare(right))
  const unresolved = [...new Set(rows.map((row) => row.target).filter((target) => target.startsWith(GROUP_PREFIX)))]
  return [
    { value: DIRECT, label: 'DIRECT' },
    ...nodes.map((node) => ({ value: `${NODE_PREFIX}${node}`, label: node })),
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

import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { LockKeyhole, Plus, Save, Trash2 } from 'lucide-react'
import { Alert, Button, Card, Input, Select, Switch, Tag } from '@acme/components'
import type { DnsRoutingDomain, DnsRoutingRuleSet, DnsSettings, DnsSettingsResponse } from '../features/dns/types'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'

const BUILTIN_ID = 'builtin-domains-min'

export function RoutingRules() {
  const { locale } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const zh = locale === 'zh-CN'
  const [draft, setDraft] = useState<DnsSettings | null>(null)
  const [selectedId, setSelectedId] = useState(BUILTIN_ID)
  const [domain, setDomain] = useState('')
  const [includeSubdomains, setIncludeSubdomains] = useState(true)
  const settings = useQuery({
    queryKey: ['dns', 'settings'],
    queryFn: () => api<DnsSettingsResponse>(session!, '/dns/settings'),
    enabled: Boolean(session),
  })
  const save = useMutation({
    mutationFn: (candidate: DnsSettings) => api(session!, '/dns/settings', { method: 'PUT', body: JSON.stringify(candidate) }),
    onSuccess: () => {
      setDraft(null)
      queryClient.invalidateQueries({ queryKey: ['dns'] })
      queryClient.invalidateQueries({ queryKey: ['system'] })
    },
  })
  const current = draft ?? settings.data?.settings
  const active = current?.rule_sets.find((item) => item.id === selectedId)
  const builtinCount = settings.data?.status.domestic_domain_count ?? 0
  const updateSet = (update: (ruleSet: DnsRoutingRuleSet) => DnsRoutingRuleSet) => {
    if (!current || !active) return
    setDraft({ ...current, rule_sets: current.rule_sets.map((item) => item.id === active.id ? update(item) : item) })
  }
  const addSet = () => {
    if (!current) return
    const id = crypto.randomUUID()
    const used = new Set(current.rule_sets.map((item) => item.name))
    let index = current.rule_sets.length + 1
    let name = zh ? `新规则集 ${index}` : `New rule set ${index}`
    while (used.has(name)) {
      index += 1
      name = zh ? `新规则集 ${index}` : `New rule set ${index}`
    }
    setDraft({ ...current, rule_sets: [...current.rule_sets, { id, name, mode: 'direct', domains: [] }] })
    setSelectedId(id)
  }
  const deleteSet = (id: string) => {
    if (!current) return
    setDraft({ ...current, rule_sets: current.rule_sets.filter((item) => item.id !== id) })
    if (selectedId === id) setSelectedId(BUILTIN_ID)
  }
  const addDomain = () => {
    const value = normalizeDomain(domain)
    if (!value || !active) return
    updateSet((ruleSet) => ({ ...ruleSet, domains: [...ruleSet.domains, { id: crypto.randomUUID(), domain: value, include_subdomains: includeSubdomains }] }))
    setDomain('')
  }
  if (!current) return <div className="p-8 text-sm text-[var(--muted)]">{zh ? '正在加载分流规则…' : 'Loading routing rules…'}</div>
  return <div className="space-y-5">
    <div className="flex min-h-10 items-start justify-between gap-4"><div><h1 className="text-xl font-semibold">{zh ? '分流规则' : 'Routing rules'}</h1><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '前置 DNS 决定解析路径；同一规则集同时注入 sing-box 路由。' : 'Frontend DNS selects the resolver path while the same rule set is injected into sing-box routing.'}</p></div><div className="flex gap-2"><Button icon={<Plus size={16} />} onClick={addSet}>{zh ? '新增规则集' : 'Add rule set'}</Button><Button variant="primary" icon={<Save size={16} />} loading={save.isPending} onClick={() => save.mutate(current)}>{zh ? '保存' : 'Save'}</Button></div></div>
    {save.isError ? <Alert type="error" showIcon message={save.error instanceof Error ? save.error.message : String(save.error)} /> : null}
    {save.isSuccess ? <Alert type="success" showIcon message={zh ? '分流规则已保存并暂存；重启核心后应用新的核心规则和前置 DNS。' : 'Routing rules saved and staged. Restart the core to apply the core and frontend DNS changes.'} /> : null}
    <div className="grid min-h-[34rem] gap-4 lg:grid-cols-[18rem_minmax(0,1fr)]">
      <Card className="!rounded-lg" bodyStyle={{ padding: 0 }}><div className="border-b border-[var(--border)] px-4 py-3 text-sm font-medium">{zh ? '规则集' : 'Rule sets'}</div><div className="space-y-1 p-2"><RuleSetButton selected={selectedId === BUILTIN_ID} name={zh ? '中国大陆域名' : 'Mainland China domains'} mode="direct" count={builtinCount} builtin onClick={() => setSelectedId(BUILTIN_ID)} />{current.rule_sets.map((ruleSet) => <RuleSetButton key={ruleSet.id} selected={selectedId === ruleSet.id} name={ruleSet.name} mode={ruleSet.mode} count={ruleSet.domains.length} onClick={() => setSelectedId(ruleSet.id)} onDelete={() => deleteSet(ruleSet.id)} />)}</div></Card>
      <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>{selectedId === BUILTIN_ID ? <BuiltinRuleSet count={builtinCount} zh={zh} /> : active ? <EditableRuleSet ruleSet={active} domain={domain} includeSubdomains={includeSubdomains} zh={zh} setDomain={setDomain} setIncludeSubdomains={setIncludeSubdomains} addDomain={addDomain} update={updateSet} /> : null}</Card>
    </div>
  </div>
}

function RuleSetButton({ selected, name, mode, count, builtin, onClick, onDelete }: { selected: boolean; name: string; mode: string; count: number; builtin?: boolean; onClick: () => void; onDelete?: () => void }) {
  return <div className={`flex items-center rounded-md ${selected ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-400' : 'hover:bg-[var(--surface-hover)]'}`}><button className="min-w-0 flex-1 px-3 py-2.5 text-left" onClick={onClick}><div className="flex items-center gap-2"><span className="truncate text-sm font-medium">{name}</span>{builtin ? <LockKeyhole className="shrink-0" size={13} /> : null}</div><div className="mt-1 flex gap-2 text-xs text-[var(--muted)]"><span>{mode === 'direct' ? 'DIRECT' : 'PROXY'}</span><span>·</span><span>{count}</span></div></button>{onDelete ? <Button className="mr-1" size="small" variant="text" icon={<Trash2 size={14} />} title="Delete" onClick={onDelete} /> : null}</div>
}

function BuiltinRuleSet({ count, zh }: { count: number; zh: boolean }) {
  return <div className="space-y-4"><div className="flex items-start justify-between gap-3"><div><h2 className="font-semibold">{zh ? '中国大陆域名' : 'Mainland China domains'}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '内置 domains-min，随 Sempre 版本更新，不依赖运行时 URL。' : 'Built-in domains-min, updated with Sempre and never fetched from a runtime URL.'}</p></div><Tag color="green">DIRECT</Tag></div><div className="rounded-md border border-[var(--border)] p-4"><div className="text-xs text-[var(--muted)]">{zh ? '域名数量' : 'Domains'}</div><div className="mt-1 text-2xl font-semibold tabular-nums">{count}</div></div><Alert type="info" showIcon message={zh ? '这是受保护的系统规则集，固定使用直连 DNS，不能编辑或删除。' : 'This protected system rule set always uses direct DNS and cannot be edited or deleted.'} /></div>
}

function EditableRuleSet({ ruleSet, domain, includeSubdomains, zh, setDomain, setIncludeSubdomains, addDomain, update }: { ruleSet: DnsRoutingRuleSet; domain: string; includeSubdomains: boolean; zh: boolean; setDomain: (value: string) => void; setIncludeSubdomains: (value: boolean) => void; addDomain: () => void; update: (change: (ruleSet: DnsRoutingRuleSet) => DnsRoutingRuleSet) => void }) {
  const updateDomain = (id: string, change: Partial<DnsRoutingDomain>) => update((current) => ({ ...current, domains: current.domains.map((entry) => entry.id === id ? { ...entry, ...change } : entry) }))
  const rows = useMemo(() => ruleSet.domains, [ruleSet.domains])
  return <div className="space-y-5"><div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_12rem]"><label className="text-sm"><span className="mb-2 block font-medium">{zh ? '名称' : 'Name'}</span><Input value={ruleSet.name} onChange={(event) => update((current) => ({ ...current, name: event.target.value }))} /></label><label className="text-sm"><span className="mb-2 block font-medium">{zh ? '模式' : 'Mode'}</span><Select className="w-full" value={ruleSet.mode} options={[{ value: 'direct', label: zh ? '直连' : 'Direct' }, { value: 'proxy', label: zh ? '代理' : 'Proxy' }]} onChange={(mode) => update((current) => ({ ...current, mode }))} /></label></div><Alert type="info" showIcon message={ruleSet.mode === 'direct' ? (zh ? 'FakeIP 下返回 Real-IP 并绕过核心；Real-IP 下进入核心后显式走 direct。' : 'In FakeIP mode, return real IPs and bypass the core. In Real-IP mode, enter the core and explicitly route direct.') : (zh ? '查询进入核心 DNS，并在“代理”页生成此规则集的独立节点选择组。' : 'Queries use core DNS and a dedicated node selector appears on the Proxies page.')} /><div className="grid gap-3 rounded-md border border-[var(--border)] p-3 md:grid-cols-[minmax(0,1fr)_12rem_auto]"><Input value={domain} placeholder="example.com" onChange={(event) => setDomain(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') addDomain() }} /><label className="flex items-center justify-between gap-3 text-sm"><span>{zh ? '包括子域名' : 'Include subdomains'}</span><Switch checked={includeSubdomains} onChange={setIncludeSubdomains} /></label><Button variant="primary" icon={<Plus size={15} />} onClick={addDomain}>{zh ? '添加' : 'Add'}</Button></div><div className="overflow-hidden rounded-md border border-[var(--border)]"><div className="grid grid-cols-[minmax(0,1fr)_10rem_3rem] border-b border-[var(--border)] bg-[var(--surface-subtle)] px-3 py-2 text-xs text-[var(--muted)]"><span>{zh ? '域名' : 'Domain'}</span><span>{zh ? '包括子域名' : 'Subdomains'}</span><span /></div>{rows.length ? rows.map((entry) => <div key={entry.id} className="grid grid-cols-[minmax(0,1fr)_10rem_3rem] items-center gap-2 border-b border-[var(--border)] px-3 py-2 last:border-b-0"><Input value={entry.domain} onChange={(event) => updateDomain(entry.id, { domain: event.target.value })} /><Switch checked={entry.include_subdomains} onChange={(include_subdomains) => updateDomain(entry.id, { include_subdomains })} /><Button size="small" variant="text" icon={<Trash2 size={14} />} title={zh ? '删除' : 'Delete'} onClick={() => update((current) => ({ ...current, domains: current.domains.filter((item) => item.id !== entry.id) }))} /></div>) : <div className="p-8 text-center text-sm text-[var(--muted)]">{zh ? '此规则集还没有域名。' : 'This rule set has no domains yet.'}</div>}</div></div>
}

function normalizeDomain(value: string) {
  return value.trim().replace(/^\*\./, '').replace(/^\./, '').replace(/\.$/, '').toLowerCase()
}

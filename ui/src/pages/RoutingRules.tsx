import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { LockKeyhole, Pencil, Plus, Save, Settings2, Trash2 } from 'lucide-react'
import { Alert, AutoComplete, Button, Card, Input, Modal, Select, Switch, Tag } from '@acme/components'
import { RuntimeRestartButton } from '../components/RuntimeRestartButton'
import type { DnsRoutingDomain, DnsRoutingRuleSet, DnsSettings, DnsSettingsResponse } from '../features/dns/types'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { ProxyNode } from '../lib/types'

const BUILTIN_ID = 'builtin-domains-min'

type DomainDialogState = {
  mode: 'add' | 'edit'
  entry: DnsRoutingDomain
}

export function RoutingRules() {
  const { locale } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const zh = locale === 'zh-CN'
  const [draft, setDraft] = useState<DnsSettings | null>(null)
  const [selectedId, setSelectedId] = useState(BUILTIN_ID)
  const [settingsDialogOpen, setSettingsDialogOpen] = useState(false)
  const [settingsDraft, setSettingsDraft] = useState({ name: '', mode: 'direct' as DnsRoutingRuleSet['mode'] })
  const [domainDialog, setDomainDialog] = useState<DomainDialogState | null>(null)

  const settings = useQuery({
    queryKey: ['dns', 'settings'],
    queryFn: () => api<DnsSettingsResponse>(session!, '/dns/settings'),
    enabled: Boolean(session),
  })
  const proxies = useQuery({
    queryKey: ['runtime', 'proxies'],
    queryFn: () => api<ProxyNode[]>(session!, '/runtime/proxies'),
    enabled: Boolean(session),
    refetchInterval: 5000,
    retry: false,
  })
  const save = useMutation({
    mutationFn: (candidate: DnsSettings) => api(session!, '/dns/settings', { method: 'PUT', body: JSON.stringify(candidate) }),
    onSuccess: () => {
      setDraft(null)
      queryClient.invalidateQueries({ queryKey: ['dns'] })
      queryClient.invalidateQueries({ queryKey: ['system'] })
    },
  })
  const selectProxy = useMutation({
    mutationFn: ({ group, proxy }: { group: string; proxy: string }) => api(session!, '/runtime/proxies/select', { method: 'POST', body: JSON.stringify({ group, proxy }) }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['runtime', 'proxies'] }),
  })

  const current = draft ?? settings.data?.settings
  const active = current?.rule_sets.find((item) => item.id === selectedId)
  const builtinCount = settings.data?.status.domestic_domain_count ?? 0
  const updateSet = (id: string, update: (ruleSet: DnsRoutingRuleSet) => DnsRoutingRuleSet) => {
    if (!current) return
    setDraft({ ...current, rule_sets: current.rule_sets.map((item) => item.id === id ? update(item) : item) })
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
  const openSettings = () => {
    if (!active) return
    setSettingsDraft({ name: active.name, mode: active.mode })
    setSettingsDialogOpen(true)
  }
  const applySettings = () => {
    if (!active) return
    updateSet(active.id, (ruleSet) => ({ ...ruleSet, name: settingsDraft.name.trim(), mode: settingsDraft.mode }))
    setSettingsDialogOpen(false)
  }
  const openDomainDialog = (entry?: DnsRoutingDomain) => {
    setDomainDialog({
      mode: entry ? 'edit' : 'add',
      entry: entry ? { ...entry } : { id: crypto.randomUUID(), domain: '', include_subdomains: true },
    })
  }
  const applyDomain = () => {
    if (!active || !domainDialog) return
    const entry = { ...domainDialog.entry, domain: normalizeDomain(domainDialog.entry.domain) }
    updateSet(active.id, (ruleSet) => ({
      ...ruleSet,
      domains: domainDialog.mode === 'add'
        ? [...ruleSet.domains, entry]
        : ruleSet.domains.map((item) => item.id === entry.id ? entry : item),
    }))
    setDomainDialog(null)
  }
  const deleteDomain = (id: string) => {
    if (!active) return
    updateSet(active.id, (ruleSet) => ({ ...ruleSet, domains: ruleSet.domains.filter((item) => item.id !== id) }))
  }

  if (!current) return <div className="p-8 text-sm text-[var(--muted)]">{zh ? '正在加载分流规则…' : 'Loading routing rules…'}</div>
  return <div className="space-y-5">
    <div className="flex min-h-10 items-start justify-between gap-4">
      <div><h1 className="text-xl font-semibold">{zh ? '分流规则' : 'Routing rules'}</h1><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '前置 DNS 决定解析路径；同一规则集同时注入 sing-box 路由。' : 'Frontend DNS selects the resolver path while the same rule set is injected into sing-box routing.'}</p></div>
      <div className="flex gap-2"><Button icon={<Plus size={16} />} onClick={addSet}>{zh ? '新增规则集' : 'Add rule set'}</Button><Button variant="primary" icon={<Save size={16} />} loading={save.isPending} onClick={() => save.mutate(current)}>{zh ? '保存' : 'Save'}</Button></div>
    </div>
    {save.isError ? <Alert type="error" showIcon message={save.error instanceof Error ? save.error.message : String(save.error)} /> : null}
    {save.isSuccess ? <Alert type="success" showIcon message={zh ? '分流规则已保存并暂存；重启核心后应用新的核心规则和前置 DNS。' : 'Routing rules saved and staged. Restart the core to apply the core and frontend DNS changes.'} /> : null}
    <div className="grid min-h-[34rem] gap-4 lg:grid-cols-[18rem_minmax(0,1fr)]">
      <Card className="!rounded-lg" bodyStyle={{ padding: 0 }}>
        <div className="border-b border-[var(--border)] px-4 py-3 text-sm font-medium">{zh ? '规则集' : 'Rule sets'}</div>
        <div className="space-y-1 p-2">
          <RuleSetButton selected={selectedId === BUILTIN_ID} name={zh ? '中国大陆域名' : 'Mainland China domains'} mode="direct" count={builtinCount} builtin onClick={() => setSelectedId(BUILTIN_ID)} />
          {current.rule_sets.map((ruleSet) => <RuleSetButton key={ruleSet.id} selected={selectedId === ruleSet.id} name={ruleSet.name} mode={ruleSet.mode} count={ruleSet.domains.length} onClick={() => setSelectedId(ruleSet.id)} onDelete={() => deleteSet(ruleSet.id)} />)}
        </div>
      </Card>
      <Card className="!rounded-lg" bodyStyle={{ padding: '1rem' }}>
        {selectedId === BUILTIN_ID ? <BuiltinRuleSet count={builtinCount} zh={zh} /> : active ? <EditableRuleSet ruleSet={active} proxyGroups={proxies.data ?? []} selecting={selectProxy.isPending} selectError={selectProxy.error} zh={zh} onSettings={openSettings} onAdd={() => openDomainDialog()} onEdit={openDomainDialog} onDelete={deleteDomain} onSelectProxy={(group, proxy) => selectProxy.mutate({ group, proxy })} /> : null}
      </Card>
    </div>
    <Modal open={settingsDialogOpen} title={zh ? '设置规则集' : 'Rule set settings'} okText={zh ? '确定' : 'Apply'} cancelText={zh ? '取消' : 'Cancel'} okButtonProps={{ disabled: !settingsDraft.name.trim() }} onOk={() => { applySettings(); return undefined }} onCancel={() => setSettingsDialogOpen(false)} destroyOnClose>
      <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_12rem]">
        <label className="text-sm"><span className="mb-2 block font-medium">{zh ? '名称' : 'Name'}</span><Input autoFocus value={settingsDraft.name} onChange={(event) => setSettingsDraft((value) => ({ ...value, name: event.target.value }))} /></label>
        <label className="text-sm"><span className="mb-2 block font-medium">{zh ? '模式' : 'Mode'}</span><Select className="w-full" value={settingsDraft.mode} options={[{ value: 'direct', label: zh ? '直连' : 'Direct' }, { value: 'proxy', label: zh ? '代理' : 'Proxy' }]} onChange={(mode) => setSettingsDraft((value) => ({ ...value, mode }))} /></label>
      </div>
    </Modal>
    <Modal open={Boolean(domainDialog)} title={domainDialog?.mode === 'edit' ? (zh ? '编辑规则' : 'Edit rule') : (zh ? '添加规则' : 'Add rule')} okText={domainDialog?.mode === 'edit' ? (zh ? '保存' : 'Save') : (zh ? '添加' : 'Add')} cancelText={zh ? '取消' : 'Cancel'} okButtonProps={{ disabled: !normalizeDomain(domainDialog?.entry.domain ?? '') }} onOk={() => { applyDomain(); return undefined }} onCancel={() => setDomainDialog(null)} destroyOnClose>
      {domainDialog ? <div className="grid gap-4">
        <label className="text-sm"><span className="mb-2 block font-medium">{zh ? '域名' : 'Domain'}</span><Input autoFocus value={domainDialog.entry.domain} placeholder="example.com" onChange={(event) => setDomainDialog({ ...domainDialog, entry: { ...domainDialog.entry, domain: event.target.value } })} /></label>
        <label className="flex items-center justify-between gap-3 text-sm"><span>{zh ? '包括子域名' : 'Include subdomains'}</span><Switch checked={domainDialog.entry.include_subdomains} onChange={(include_subdomains) => setDomainDialog({ ...domainDialog, entry: { ...domainDialog.entry, include_subdomains } })} /></label>
      </div> : null}
    </Modal>
  </div>
}

function RuleSetButton({ selected, name, mode, count, builtin, onClick, onDelete }: { selected: boolean; name: string; mode: string; count: number; builtin?: boolean; onClick: () => void; onDelete?: () => void }) {
  return <div className={`flex items-center rounded-md ${selected ? 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-400' : 'hover:bg-[var(--surface-hover)]'}`}>
    <button className="min-w-0 flex-1 px-3 py-2.5 text-left" onClick={onClick}><div className="flex items-center gap-2"><span className="truncate text-sm font-medium">{name}</span>{builtin ? <LockKeyhole className="shrink-0" size={13} /> : null}</div><div className="mt-1 flex gap-2 text-xs text-[var(--muted)]"><span>{mode === 'direct' ? 'DIRECT' : 'PROXY'}</span><span>·</span><span>{count}</span></div></button>
    {onDelete ? <Button className="mr-1" size="small" variant="text" icon={<Trash2 size={14} />} title="Delete" onClick={onDelete} /> : null}
  </div>
}

function BuiltinRuleSet({ count, zh }: { count: number; zh: boolean }) {
  return <div className="space-y-4"><div className="flex items-start justify-between gap-3"><div><h2 className="font-semibold">{zh ? '中国大陆域名' : 'Mainland China domains'}</h2><p className="mt-1 text-sm text-[var(--muted)]">{zh ? '内置 domains-min，随 Sempre 版本更新，不依赖运行时 URL。' : 'Built-in domains-min, updated with Sempre and never fetched from a runtime URL.'}</p></div><Tag color="green">DIRECT</Tag></div><div className="rounded-md border border-[var(--border)] p-4"><div className="text-xs text-[var(--muted)]">{zh ? '域名数量' : 'Domains'}</div><div className="mt-1 text-2xl font-semibold tabular-nums">{count}</div></div><Alert type="info" showIcon message={zh ? '这是受保护的系统规则集，固定使用直连 DNS，不能编辑或删除。' : 'This protected system rule set always uses direct DNS and cannot be edited or deleted.'} /></div>
}

function EditableRuleSet({ ruleSet, proxyGroups, selecting, selectError, zh, onSettings, onAdd, onEdit, onDelete, onSelectProxy }: { ruleSet: DnsRoutingRuleSet; proxyGroups: ProxyNode[]; selecting: boolean; selectError: Error | null; zh: boolean; onSettings: () => void; onAdd: () => void; onEdit: (entry: DnsRoutingDomain) => void; onDelete: (id: string) => void; onSelectProxy: (group: string, proxy: string) => void }) {
  const groupName = `DNS · ${ruleSet.name}`
  const proxyGroup = useMemo(() => proxyGroups.find((item) => item.name === groupName && item.all?.length), [groupName, proxyGroups])
  return <div className="space-y-5">
    <div className="flex items-center justify-between gap-3 border-b border-[var(--border)] pb-3">
      <Button icon={<Settings2 size={15} />} onClick={onSettings}>{zh ? '设置规则集' : 'Rule set settings'}</Button>
      <Button variant="primary" icon={<Plus size={15} />} onClick={onAdd}>{zh ? '添加规则' : 'Add rule'}</Button>
    </div>
    <RuleSetRuntime ruleSet={ruleSet} proxyGroup={proxyGroup} selecting={selecting} selectError={selectError} zh={zh} onSelect={onSelectProxy} />
    <div className="overflow-hidden rounded-md border border-[var(--border)]">
      <div className="grid grid-cols-[minmax(0,1fr)_10rem_7rem] border-b border-[var(--border)] bg-[var(--surface-subtle)] px-3 py-2 text-xs text-[var(--muted)]"><span>{zh ? '域名' : 'Domain'}</span><span>{zh ? '包括子域名' : 'Subdomains'}</span><span className="text-right">{zh ? '操作' : 'Actions'}</span></div>
      {ruleSet.domains.length ? ruleSet.domains.map((entry) => <div key={entry.id} className="grid grid-cols-[minmax(0,1fr)_10rem_7rem] items-center gap-2 border-b border-[var(--border)] px-3 py-2 last:border-b-0"><span className="truncate text-sm">{entry.domain}</span><span className="text-sm">{entry.include_subdomains ? (zh ? '是' : 'Yes') : (zh ? '否' : 'No')}</span><span className="flex justify-end gap-1"><Button size="small" variant="text" icon={<Pencil size={14} />} title={zh ? '编辑' : 'Edit'} onClick={() => onEdit(entry)} /><Button size="small" variant="text" icon={<Trash2 size={14} />} title={zh ? '删除' : 'Delete'} onClick={() => onDelete(entry.id)} /></span></div>) : <div className="p-8 text-center text-sm text-[var(--muted)]">{zh ? '此规则集还没有规则。' : 'This rule set has no rules yet.'}</div>}
    </div>
  </div>
}

function RuleSetRuntime({ ruleSet, proxyGroup, selecting, selectError, zh, onSelect }: { ruleSet: DnsRoutingRuleSet; proxyGroup?: ProxyNode; selecting: boolean; selectError: Error | null; zh: boolean; onSelect: (group: string, proxy: string) => void }) {
  const [search, setSearch] = useState<string | null>(null)
  if (ruleSet.mode === 'direct') return <Alert type="info" showIcon message={zh ? 'FakeIP 下返回 Real-IP 并绕过核心；Real-IP 下进入核心后显式走 direct。' : 'In FakeIP mode, return real IPs and bypass the core. In Real-IP mode, enter the core and explicitly route direct.'} />
  if (!proxyGroup) return <Alert type="warning" showIcon message={zh ? '当前核心尚未识别此代理分组，请先保存并重启核心。' : 'The running core has not recognized this proxy group. Save and restart the core first.'} action={<RuntimeRestartButton showLabel />} />
  return <div className="grid gap-2 rounded-md border border-[var(--border)] bg-[var(--surface-subtle)] p-3 md:grid-cols-[minmax(0,1fr)_minmax(14rem,24rem)] md:items-center">
    <div><div className="text-sm font-medium">{zh ? '代理节点快速切换' : 'Quick proxy selection'}</div><div className="mt-1 text-xs text-[var(--muted)]">{proxyGroup.name}</div></div>
    <label className="text-sm"><span className="sr-only">{zh ? '代理节点' : 'Proxy node'}</span><AutoComplete className="w-full" value={search ?? proxyGroup.now ?? ''} options={proxyGroup.all ?? []} disabled={selecting} allowClear={false} onChange={setSearch} onFocus={() => setSearch('')} onBlur={() => setSearch(null)} onSelect={(proxy) => { setSearch(proxy); onSelect(proxyGroup.name, proxy) }} /></label>
    {selectError ? <div role="alert" className="text-xs text-red-600 md:col-start-2">{selectError.message}</div> : null}
  </div>
}

function normalizeDomain(value: string) {
  return value.trim().replace(/^\*\./, '').replace(/^\./, '').replace(/\.$/, '').toLowerCase()
}

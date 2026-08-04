import { useState } from 'react'
import CodeMirror from '@uiw/react-codemirror'
import { json } from '@codemirror/lang-json'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Activity, CheckCircle2, Download, FileJson, FlaskConical, Plus, Power, RefreshCw, Save, Trash2, Upload, X } from 'lucide-react'
import { api, streamRequest } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { parseJSONC } from '../lib/jsonc'
import { useSession } from '../lib/session'
import type { CustomNode, FieldDiff, RenderResult, SourceResult, SubscriptionCatalogResponse, SubscriptionProfile, SubscriptionSource } from '../lib/types'
import { Badge, Button, Card, Field, Input, PageTitle, Spinner } from '../components/ui'

type Section = 'sources' | 'nodes' | 'rules' | 'dns' | 'diagnostics'
type SaveResponse = { change: { Changed: boolean; NeedsRestart: boolean; Message: string }; render: RenderResult }

export function Subscriptions() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const catalog = useQuery({ queryKey: ['subscriptions'], queryFn: () => api<SubscriptionCatalogResponse>(session!, '/subscriptions') })
  const customNodes = useQuery({ queryKey: ['custom-nodes'], queryFn: () => api<{ nodes: CustomNode[] }>(session!, '/custom-nodes') })
  const [selectedState, setSelectedID] = useState('')
  const [drafts, setDrafts] = useState<Record<string, SubscriptionProfile>>({})
  const [section, setSection] = useState<Section>('sources')
  const [notice, setNotice] = useState('')
  const [newName, setNewName] = useState('')
  const [advancedDrafts, setAdvancedDrafts] = useState<Record<string, Advanced>>({})
  const [preview, setPreview] = useState<RenderResult | null>(null)
  const [nodeTrace, setNodeTrace] = useState<FieldDiff | null>(null)
  const [previewFormat, setPreviewFormat] = useState('sing-box-v13')
  const [debugEvents, setDebugEvents] = useState<string[]>([])

  const selectedID = selectedState && catalog.data?.profiles.some((profile) => profile.id === selectedState) ? selectedState : (catalog.data?.active_profile_id || catalog.data?.profiles[0]?.id || '')
  const storedProfile = catalog.data?.profiles.find((profile) => profile.id === selectedID)
  const draft = drafts[selectedID] || storedProfile || null
  const setDraft = (profile: SubscriptionProfile) => setDrafts((current) => ({ ...current, [selectedID]: profile }))
  const advanced = advancedDrafts[selectedID] || advancedFromProfile(draft, catalog.data?.defaults)
  const setAdvanced = (value: Advanced) => setAdvancedDrafts((current) => ({ ...current, [selectedID]: value }))

  const invalidate = () => { queryClient.invalidateQueries({ queryKey: ['subscriptions'] }); queryClient.invalidateQueries({ queryKey: ['system'] }); queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] }) }
  const save = useMutation({
    mutationFn: () => {
      if (!draft) throw new Error('No profile selected')
      const candidate = profileFromEditors(draft, advanced)
      return api<SaveResponse>(session!, `/subscriptions/${draft.id}`, { method: 'PUT', body: JSON.stringify(candidate) })
    },
    onSuccess: (result) => { setNotice(result.change.NeedsRestart ? t('staged') : result.change.Message); setDrafts((current) => withoutKey(current, selectedID)); setAdvancedDrafts((current) => withoutKey(current, selectedID)); invalidate() },
    onError: (error) => setNotice(error.message),
  })
  const create = useMutation({
    mutationFn: () => api<SubscriptionProfile>(session!, '/subscriptions', { method: 'POST', body: JSON.stringify({ name: newName }) }),
    onSuccess: (profile) => { setNewName(''); setSelectedID(profile.id); invalidate() }, onError: (error) => setNotice(error.message),
  })
  const remove = useMutation({ mutationFn: (id: string) => api(session!, `/subscriptions/${id}`, { method: 'DELETE' }), onSuccess: invalidate, onError: (error) => setNotice(error.message) })
  const action = useMutation({
    mutationFn: (operation: 'activate' | 'refresh') => api<SaveResponse>(session!, `/subscriptions/${selectedID}/${operation}`, { method: 'POST' }),
    onSuccess: (result) => { setNotice(result.change.NeedsRestart ? t('staged') : result.change.Message); invalidate() }, onError: (error) => setNotice(error.message),
  })
  const schedule = useMutation({
    mutationFn: (body: Record<string, unknown>) => api(session!, '/subscription', { method: 'PATCH', body: JSON.stringify(body) }),
    onSuccess: () => { setNotice(t('operationDone')); invalidate() }, onError: (error) => setNotice(error.message),
  })
  const render = useMutation({
    mutationFn: () => api<RenderResult>(session!, `/subscriptions/${selectedID}/preview`, { method: 'POST', body: JSON.stringify({ format: previewFormat, force: true }) }),
    onSuccess: (result) => { setPreview(result); setNodeTrace(null) }, onError: (error) => setNotice(error.message),
  })
  const trace = useMutation({
    mutationFn: (name: string) => api<FieldDiff>(session!, `/subscriptions/${selectedID}/trace`, { method: 'POST', body: JSON.stringify({ name, format: previewFormat }) }),
    onSuccess: setNodeTrace,
    onError: (error) => setNotice(error.message),
  })
  const sourceTest = useMutation({
    mutationFn: (source: SubscriptionSource) => api<SourceResult>(session!, '/subscriptions/source/test', { method: 'POST', body: JSON.stringify(source) }),
    onSuccess: (result) => setNotice(`${result.parse.format}: ${result.parse.nodes.length} nodes${result.parse.diagnostics.length ? `; ${result.parse.diagnostics.join('; ')}` : ''}`), onError: (error) => setNotice(error.message),
  })

  const isActive = draft?.id === catalog.data?.active_profile_id
  const tabs = catalog.data?.profiles || []
  const sections: Array<{ id: Section; label: string }> = [{ id: 'sources', label: t('sources') }, { id: 'nodes', label: t('nodeLibrary') }, { id: 'rules', label: t('groupsAndRules') }, { id: 'dns', label: t('dnsAndPrivate') }, { id: 'diagnostics', label: t('diagnostics') }]

  async function debug() {
    setDebugEvents([])
    try {
      await streamRequest(session!, `/subscriptions/${selectedID}/debug`, { format: previewFormat }, (event, data) => {
        setDebugEvents((current) => [...current, `${event}: ${JSON.stringify(data)}`])
        if (event === 'result') setPreview(data as RenderResult)
      })
    } catch (error) { setNotice(error instanceof Error ? error.message : String(error)) }
  }

  return <div className="space-y-5">
    <PageTitle title={t('subscriptions')}>
      <div className="flex flex-wrap justify-end gap-2"><Button disabled={!draft || action.isPending} onClick={() => action.mutate('refresh')}><RefreshCw size={16} />{t('updateNow')}</Button><Button variant="primary" disabled={!draft || save.isPending} onClick={() => { try { profileFromEditors(draft!, advanced); save.mutate() } catch (error) { setNotice(error instanceof Error ? error.message : String(error)) } }}>{save.isPending ? <Spinner /> : <Save size={16} />}{t('saveAndStage')}</Button></div>
    </PageTitle>
    <div className="flex items-end gap-1 overflow-x-auto border-b border-[var(--border)]">
      {tabs.map((profile) => <button key={profile.id} className={`flex h-10 shrink-0 items-center gap-2 border-b-2 px-3 text-sm font-medium ${selectedID === profile.id ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)]'}`} onClick={() => setSelectedID(profile.id)}>{profile.id === catalog.data?.active_profile_id ? <span className="size-2 rounded-full bg-emerald-500" /> : null}{profile.name || t('defaultSubscription')}</button>)}
      <div className="flex shrink-0 items-center gap-1 pb-1 pl-2"><Input className="w-36" value={newName} onChange={(event) => setNewName(event.target.value)} placeholder={t('profileName')} /><Button size="icon" title={t('addProfile')} disabled={!newName.trim() || create.isPending} onClick={() => create.mutate()}><Plus size={16} /></Button></div>
    </div>
    {notice ? <div className="border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm break-words">{notice}</div> : null}
    {draft ? <>
      <Card className="p-4">
        <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_180px_180px]">
          <Field label={t('profileName')}><Input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} /></Field>
          <Field label={t('schedule')}><Input value={catalog.data?.schedule.interval || '24h'} onChange={(event) => queryClient.setQueryData<SubscriptionCatalogResponse>(['subscriptions'], (value) => value ? { ...value, schedule: { ...value.schedule, interval: event.target.value } } : value)} onBlur={(event) => schedule.mutate({ interval: event.target.value })} /></Field>
          <label className="flex h-9 items-center justify-between self-end rounded-md border border-[var(--border)] px-3 text-sm"><span>{t('automaticRestart')}</span><input type="checkbox" className="size-4 accent-emerald-600" checked={Boolean(catalog.data?.auto_restart)} onChange={(event) => schedule.mutate({ auto_restart: event.target.checked })} /></label>
        </div>
        <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-[var(--border)] pt-4">{isActive ? <Badge tone="success">{t('activeProfile')}</Badge> : <Button disabled={action.isPending} onClick={() => action.mutate('activate')}><CheckCircle2 size={16} />{t('activate')}</Button>}<span className="text-xs text-[var(--muted)]">{draft.last_compiler_target || t('compilerTarget')} · {draft.last_result || t('noData')}</span><div className="ml-auto flex gap-2"><Button onClick={() => api(session!, '/runtime/restart', { method: 'POST' }).then(() => setNotice(t('operationAccepted')))}><Power size={16} />{t('restartNow')}</Button><Button size="icon" variant="ghost" title={t('remove')} disabled={isActive || tabs.length <= 1 || remove.isPending} onClick={() => remove.mutate(draft.id)}><Trash2 size={16} /></Button></div></div>
      </Card>
      <div className="flex gap-1 overflow-x-auto border-b border-[var(--border)]">{sections.map((item) => <button key={item.id} className={`h-10 shrink-0 border-b-2 px-3 text-sm font-medium ${section === item.id ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)]'}`} onClick={() => setSection(item.id)}>{item.label}</button>)}</div>
      {section === 'sources' ? <SourcesEditor profile={draft} setProfile={setDraft} onTest={(source) => sourceTest.mutate(source)} pending={sourceTest.isPending} /> : null}
      {section === 'nodes' ? <NodesEditor profile={draft} setProfile={setDraft} nodes={customNodes.data?.nodes || []} /> : null}
      {section === 'rules' ? <RulesEditor profile={draft} setProfile={setDraft} advanced={advanced} setAdvanced={setAdvanced} /> : null}
      {section === 'dns' ? <DNSEditor profile={draft} setProfile={setDraft} advanced={advanced} setAdvanced={setAdvanced} /> : null}
      {section === 'diagnostics' ? <Diagnostics profile={draft} formats={(catalog.data?.targets || []).map((target) => target.format)} format={previewFormat} setFormat={(value) => { setPreviewFormat(value); setNodeTrace(null) }} preview={preview} nodeTrace={nodeTrace} debugEvents={debugEvents} pending={render.isPending || trace.isPending} onPreview={() => render.mutate()} onDebug={() => void debug()} onTrace={(name) => trace.mutate(name)} onClear={() => api(session!, '/subscriptions/cache/clear', { method: 'POST' }).then(() => setNotice(t('operationDone')))} /> : null}
    </> : <Card className="grid min-h-52 place-items-center"><Spinner /></Card>}
    {preview ? <PreviewDialog result={preview} onClose={() => setPreview(null)} /> : null}
  </div>
}

function SourcesEditor({ profile, setProfile, onTest, pending }: { profile: SubscriptionProfile; setProfile: (profile: SubscriptionProfile) => void; onTest: (source: SubscriptionSource) => void; pending: boolean }) {
  const { t } = useI18n()
  const update = (index: number, patch: Partial<SubscriptionSource>) => setProfile({ ...profile, sources: profile.sources.map((source, position) => position === index ? { ...source, ...patch } : source) })
  const add = (type: 'url' | 'raw') => setProfile({ ...profile, sources: [...profile.sources, { id: crypto.randomUUID(), type, enabled: true, user_agent: 'clash.meta', fetch_mode: 'auto', ...(type === 'raw' ? { content: '' } : { url: '' }) }] })
  return <Card className="p-4"><div className="mb-4 flex flex-wrap items-center justify-between gap-2"><h2 className="text-sm font-semibold">{t('sources')}</h2><div className="flex gap-2"><Button onClick={() => add('url')}><Plus size={16} />{t('addURL')}</Button><Button onClick={() => add('raw')}><Upload size={16} />{t('addRaw')}</Button></div></div>{profile.sources.length ? <div className="divide-y divide-[var(--border)]">{profile.sources.map((source, index) => <div key={source.id} className="grid gap-3 py-4 first:pt-0 last:pb-0"><div className="flex items-center gap-3"><input className="size-4 accent-emerald-600" type="checkbox" checked={source.enabled} onChange={(event) => update(index, { enabled: event.target.checked })} /><Badge>{source.type}</Badge><Input className="max-w-56" value={source.remark || ''} placeholder={t('details')} onChange={(event) => update(index, { remark: event.target.value })} /><div className="ml-auto flex gap-1"><Button size="small" disabled={pending} onClick={() => onTest(source)}><FlaskConical size={14} />{t('test')}</Button><Button size="icon" variant="ghost" title={t('remove')} onClick={() => setProfile({ ...profile, sources: profile.sources.filter((_, position) => position !== index) })}><Trash2 size={15} /></Button></div></div>{source.type === 'url' ? <Input value={source.url || ''} placeholder="https://example.com/subscription" onChange={(event) => update(index, { url: event.target.value })} /> : <textarea className="min-h-36 w-full resize-y rounded-md border border-[var(--border)] bg-[var(--surface)] p-3 font-mono text-xs outline-none focus:border-emerald-500" value={source.content || ''} placeholder={t('rawContent')} onChange={(event) => update(index, { content: event.target.value })} />}<div className="grid gap-3 sm:grid-cols-3"><Field label={t('prefix')}><Input value={source.prefix || ''} onChange={(event) => update(index, { prefix: event.target.value })} /></Field><Field label={t('userAgent')}><Input value={source.user_agent || 'clash.meta'} onChange={(event) => update(index, { user_agent: event.target.value })} /></Field><Field label={t('fetchMode')}><select className="h-9 rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm" value={source.fetch_mode || 'auto'} onChange={(event) => update(index, { fetch_mode: event.target.value as 'auto' | 'domestic-direct' })}><option value="auto">auto</option><option value="domestic-direct">domestic-direct</option></select></Field></div>{source.last_status ? <p className="text-xs text-[var(--muted)]">{source.last_status}{source.last_error ? ` · ${source.last_error}` : ''}</p> : null}</div>)}</div> : <p className="py-12 text-center text-sm text-[var(--muted)]">{t('noSources')}</p>}</Card>
}

function NodesEditor({ profile, setProfile, nodes }: { profile: SubscriptionProfile; setProfile: (profile: SubscriptionProfile) => void; nodes: CustomNode[] }) {
  const { t } = useI18n()
  const selected = new Set(profile.custom_node_ids)
  return <Card className="overflow-hidden"><div className="border-b border-[var(--border)] px-4 py-3 text-sm font-semibold">{t('nodeLibrary')}</div>{nodes.length ? <div className="divide-y divide-[var(--border)]">{nodes.map((node) => <label key={node.id} className="flex min-h-12 items-center gap-3 px-4 py-2 text-sm hover:bg-[var(--surface-hover)]"><input className="size-4 accent-emerald-600" type="checkbox" checked={selected.has(node.id)} onChange={(event) => setProfile({ ...profile, custom_node_ids: event.target.checked ? [...profile.custom_node_ids, node.id] : profile.custom_node_ids.filter((id) => id !== node.id) })} /><span className="font-medium">{node.name}</span><Badge>{String(node.proxy.type || '')}</Badge><span className="ml-auto font-mono text-xs text-[var(--muted)]">{String(node.proxy.server || '')}:{String(node.proxy.port || '')}</span></label>)}</div> : <p className="py-12 text-center text-sm text-[var(--muted)]">{t('noData')}</p>}</Card>
}

function RulesEditor({ profile, setProfile, advanced, setAdvanced }: { profile: SubscriptionProfile; setProfile: (profile: SubscriptionProfile) => void; advanced: Advanced; setAdvanced: (value: Advanced) => void }) {
  const { t } = useI18n()
  return <Card className="grid gap-5 p-4 lg:grid-cols-2"><div className="grid gap-3 lg:col-span-2 sm:grid-cols-2 xl:grid-cols-4"><Toggle label={t('systemGroups')} checked={profile.use_system_groups} onChange={(value) => setProfile({ ...profile, use_system_groups: value })} /><Toggle label={t('systemRuleProviders')} checked={profile.use_system_rules} onChange={(value) => setProfile({ ...profile, use_system_rules: value })} /><Toggle label={t('systemFilters')} checked={profile.use_system_filters} onChange={(value) => setProfile({ ...profile, use_system_filters: value })} /><Toggle label={t('systemCustomRules')} checked={profile.use_system_custom_config} onChange={(value) => setProfile({ ...profile, use_system_custom_config: value })} /></div><JSONField label={t('groups')} value={advanced.groups} onChange={(groups) => setAdvanced({ ...advanced, groups })} /><TextField label={t('customRules')} value={advanced.rules} onChange={(rules) => setAdvanced({ ...advanced, rules })} /><JSONField label={t('ruleProviders')} value={advanced.providers} onChange={(providers) => setAdvanced({ ...advanced, providers })} /><TextField label={t('filters')} value={advanced.filters} onChange={(filters) => setAdvanced({ ...advanced, filters })} /></Card>
}

function DNSEditor({ profile, setProfile, advanced, setAdvanced }: { profile: SubscriptionProfile; setProfile: (profile: SubscriptionProfile) => void; advanced: Advanced; setAdvanced: (value: Advanced) => void }) {
  const { t } = useI18n()
  return <Card className="grid gap-5 p-4 lg:grid-cols-2"><div className="lg:col-span-2"><Toggle label={t('systemDNS')} checked={profile.use_system_dns} onChange={(value) => setProfile({ ...profile, use_system_dns: value })} /></div><JSONField label={t('dnsConfig')} value={advanced.dns} onChange={(dns) => setAdvanced({ ...advanced, dns })} /><JSONField label={t('privateAccess')} value={advanced.privateAccess} onChange={(privateAccess) => setAdvanced({ ...advanced, privateAccess })} /><div className="lg:col-span-2"><JSONField label={t('targetOverrides')} value={advanced.custom} onChange={(custom) => setAdvanced({ ...advanced, custom })} /></div></Card>
}

function Diagnostics({ profile, formats, format, setFormat, preview, nodeTrace, debugEvents, pending, onPreview, onDebug, onTrace, onClear }: { profile: SubscriptionProfile; formats: string[]; format: string; setFormat: (value: string) => void; preview: RenderResult | null; nodeTrace: FieldDiff | null; debugEvents: string[]; pending: boolean; onPreview: () => void; onDebug: () => void; onTrace: (name: string) => void; onClear: () => void }) {
  const { t } = useI18n()
  const [traceNode, setTraceNode] = useState('')
  const dropped = preview?.field_diffs?.filter((diff) => diff.dropped.length) || []
  const traceNodes = preview?.field_diffs?.map((diff) => diff.node) || []
  const selectedTraceNode = traceNodes.includes(traceNode) ? traceNode : (traceNodes[0] || '')
  return <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]"><Card className="p-4"><div className="flex flex-wrap items-end gap-3"><Field label={t('compilerTarget')}><select className="h-9 min-w-56 rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm" value={format} onChange={(event) => setFormat(event.target.value)}>{formats.map((item) => <option key={item}>{item}</option>)}</select></Field><Button disabled={pending} onClick={onPreview}><FileJson size={16} />{t('preview')}</Button><Button disabled={pending} onClick={onDebug}><Activity size={16} />{t('diagnostics')}</Button><Button onClick={onClear}><Trash2 size={16} />{t('clearCache')}</Button></div><div className="mt-5 flex flex-wrap items-end gap-3 border-t border-[var(--border)] pt-4"><Field label={t('traceNode')}><select className="h-9 min-w-56 max-w-full rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm" value={selectedTraceNode} disabled={!traceNodes.length} onChange={(event) => setTraceNode(event.target.value)}>{traceNodes.map((name) => <option key={name}>{name}</option>)}</select></Field><Button disabled={pending || !selectedTraceNode} onClick={() => onTrace(selectedTraceNode)}><Activity size={16} />{t('traceNode')}</Button></div><div className="mt-5 grid grid-cols-2 gap-4 border-t border-[var(--border)] pt-4 text-sm"><Info label={t('compilerTarget')} value={profile.last_compiler_target || '-'} /><Info label={t('lastResult')} value={profile.last_result || '-'} /><Info label="Nodes" value={String(preview?.node_count || 0)} /><Info label="Runtime validation" value={String(profile.last_runtime_validated)} /></div>{profile.last_compiler_warnings?.length ? <div className="mt-4 space-y-1 text-xs text-amber-700 dark:text-amber-400">{profile.last_compiler_warnings.map((warning) => <p key={warning}>{warning}</p>)}</div> : null}</Card><Card className="min-h-52 overflow-hidden"><div className="border-b border-[var(--border)] px-4 py-3 text-sm font-semibold">{nodeTrace ? t('traceNode') : t('droppedFields')}</div><div className="max-h-96 overflow-auto p-4 font-mono text-xs">{nodeTrace ? <pre className="whitespace-pre-wrap break-words">{JSON.stringify(nodeTrace, null, 2)}</pre> : dropped.length ? dropped.map((diff) => <p key={diff.node} className="mb-2 break-words"><span className="font-semibold">{diff.node}</span> <span className="text-[var(--muted)]">[{preview?.node_origins?.[diff.node] || 'unknown'}]</span>: {diff.dropped.join(', ')}</p>) : <p className="text-[var(--muted)]">{t('noData')}</p>}{debugEvents.map((event, index) => <p key={`${index}-${event}`} className="mt-2 break-all text-[var(--muted)]">{event}</p>)}</div></Card></div>
}

function PreviewDialog({ result, onClose }: { result: RenderResult; onClose: () => void }) {
  const { t } = useI18n()
  const download = () => { const blob = new Blob([result.content], { type: result.format.startsWith('clash') ? 'text/yaml' : 'application/json' }); const link = document.createElement('a'); link.href = URL.createObjectURL(blob); link.download = `sempre-${result.format}.${result.format.startsWith('clash') ? 'yaml' : 'json'}`; link.click(); URL.revokeObjectURL(link.href) }
  return <div className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4"><div className="flex max-h-[92vh] w-full max-w-5xl flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--surface)] shadow-2xl"><div className="flex h-14 items-center border-b border-[var(--border)] px-4"><h2 className="text-sm font-semibold">{result.format} · {result.node_count} nodes</h2><Button className="ml-auto" onClick={download}><Download size={16} />{t('export')}</Button><Button size="icon" variant="ghost" title={t('close')} onClick={onClose}><X size={17} /></Button></div><div className="min-h-0 flex-1 overflow-auto"><CodeMirror value={result.content} height="calc(92vh - 3.5rem)" editable={false} extensions={result.format.startsWith('sing-box') ? [json()] : []} theme="dark" /></div></div></div>
}

type Advanced = { groups: string; rules: string; providers: string; filters: string; dns: string; privateAccess: string; custom: string }
function advancedFromProfile(profile: SubscriptionProfile | null, defaults?: import('../lib/types').SubscriptionDefaults): Advanced { return { groups: JSON.stringify(profile?.use_system_groups ? (defaults?.groups || []) : (profile?.groups || []), null, 2), rules: ((profile?.use_system_custom_config ? defaults?.rules : profile?.rules) || []).join('\n'), providers: JSON.stringify(profile?.use_system_rules ? (defaults?.rule_providers || []) : (profile?.rule_providers || []), null, 2), filters: ((profile?.use_system_filters ? defaults?.filters : profile?.filters) || []).join('\n'), dns: JSON.stringify(profile?.use_system_dns ? (defaults?.dns || {}) : (profile?.dns || {}), null, 2), privateAccess: JSON.stringify(profile?.private_access || {}, null, 2), custom: JSON.stringify(profile?.custom_config || {}, null, 2) } }
function withoutKey<T>(value: Record<string, T>, key: string) { const result = { ...value }; delete result[key]; return result }
function profileFromEditors(profile: SubscriptionProfile, value: Advanced): SubscriptionProfile { return { ...profile, groups: parseJSONC(value.groups), rules: lines(value.rules), rule_providers: parseJSONC(value.providers), filters: lines(value.filters), dns: parseJSONC(value.dns), private_access: parseJSONC(value.privateAccess), custom_config: parseJSONC(value.custom) } }
function lines(value: string) { return value.split('\n').map((line) => line.trim()).filter(Boolean) }
function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) { return <label className="flex h-10 items-center justify-between rounded-md border border-[var(--border)] px-3 text-sm font-medium"><span>{label}</span><input className="size-4 accent-emerald-600" type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} /></label> }
function JSONField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) { return <Field label={label}><div className="overflow-hidden rounded-md border border-[var(--border)]"><CodeMirror value={value} height="220px" extensions={[json()]} theme="dark" onChange={onChange} /></div></Field> }
function TextField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) { return <Field label={label}><textarea className="min-h-56 w-full resize-y rounded-md border border-[var(--border)] bg-[var(--surface)] p-3 font-mono text-xs outline-none focus:border-emerald-500" value={value} onChange={(event) => onChange(event.target.value)} /></Field> }
function Info({ label, value }: { label: string; value: string }) { return <div><p className="text-xs text-[var(--muted)]">{label}</p><p className="mt-1 break-words font-medium">{value}</p></div> }

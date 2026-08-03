import { useMemo, useState, type ReactNode } from 'react'
import CodeMirror from '@uiw/react-codemirror'
import { json } from '@codemirror/lang-json'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, Download, FileJson, KeyRound, Package, RefreshCw, Save, ServerCog, Trash2, Upload } from 'lucide-react'
import { api, uploadUI } from '../lib/api'
import { compactHash, formatDate } from '../lib/format'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { CoresResponse, Subscription, UIMetadata } from '../lib/types'
import { Badge, Button, Card, Field, Input, PageTitle, Spinner } from '../components/ui'

type Tab = 'core' | 'subscription' | 'config' | 'web'

export function Management() {
  const { t } = useI18n()
  const [tab, setTab] = useState<Tab>('core')
  const tabs: Array<{ value: Tab; label: string; icon: typeof Package }> = [
    { value: 'core', label: t('coreTab'), icon: Package }, { value: 'subscription', label: t('subscriptionTab'), icon: Download },
    { value: 'config', label: t('configTab'), icon: FileJson }, { value: 'web', label: t('webUITab'), icon: ServerCog },
  ]
  return <div className="space-y-5"><PageTitle title={t('management')} /><div className="flex gap-1 overflow-x-auto border-b border-[var(--border)]">{tabs.map(({ value, label, icon: Icon }) => <button key={value} className={`flex h-11 shrink-0 items-center gap-2 border-b-2 px-3 text-sm font-medium ${tab === value ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)] hover:text-[var(--text)]'}`} onClick={() => setTab(value)}><Icon size={16} />{label}</button>)}</div>{tab === 'core' ? <CorePanel /> : tab === 'subscription' ? <SubscriptionPanel /> : tab === 'config' ? <ConfigPanel /> : <WebUIPanel />}</div>
}

function CorePanel() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [reference, setReference] = useState('sing-box@stable')
  const [notice, setNotice] = useState('')
  const cores = useQuery({ queryKey: ['cores'], queryFn: () => api<CoresResponse>(session!, '/cores') })
  const action = useMutation({
    mutationFn: ({ operation, value }: { operation: string; value?: string }) => api(session!, `/cores/${operation}`, { method: 'POST', body: JSON.stringify({ reference: value || '' }) }),
    onSuccess: () => { setNotice(t('operationDone')); queryClient.invalidateQueries({ queryKey: ['cores'] }); queryClient.invalidateQueries({ queryKey: ['system'] }) },
    onError: (error) => setNotice(error.message),
  })
  return <Section title={t('core')} icon={<Package size={18} />} notice={notice}>
    <div className="grid gap-4 border-b border-[var(--border)] pb-6 md:grid-cols-[minmax(0,1fr)_auto_auto]"><Field label={t('reference')}><Input value={reference} onChange={(event) => setReference(event.target.value)} placeholder="sing-box:tinymins/sing-box@1.13.15-ddns.1" /></Field><Button className="self-end" variant="primary" disabled={action.isPending} onClick={() => action.mutate({ operation: 'install', value: reference })}>{action.isPending ? <Spinner /> : <Download size={16} />}{t('install')}</Button><Button className="self-end" disabled={action.isPending} onClick={() => action.mutate({ operation: 'update', value: reference })}><RefreshCw size={16} />{t('update')}</Button></div>
    <h3 className="mt-6 text-sm font-semibold">{t('installedVersions')}</h3>
    <div className="mt-3 overflow-x-auto rounded-lg border border-[var(--border)]"><table className="w-full min-w-[820px] text-left text-sm"><thead className="text-xs text-[var(--muted)]"><tr><th className="px-3 py-3 font-medium">{t('core')}</th><th className="px-3 py-3 font-medium">{t('repository')}</th><th className="px-3 py-3 font-medium">{t('version')}</th><th className="px-3 py-3 font-medium">{t('channel')}</th><th className="px-3 py-3 font-medium">{t('details')}</th><th className="w-48" /></tr></thead><tbody>{cores.data?.installed.map((item) => {
      const selectedRepository = cores.data?.selected?.repository || ''
      const itemRepository = item.official ? '' : item.repository
      const selected = cores.data?.selected?.core === item.core && selectedRepository === itemRepository && (cores.data.selected.ref === item.version || item.channels.includes(cores.data.selected.ref))
      return <tr key={item.reference} className="border-t border-[var(--border)]"><td className="px-3 py-3 font-medium">{item.core}</td><td className="px-3 py-3"><div className="flex items-center gap-2"><Badge tone={item.official ? 'success' : 'warning'}>{item.official ? t('official') : t('custom')}</Badge><span className="font-mono text-xs text-[var(--muted)]">{item.repository}</span></div></td><td className="px-3 py-3 font-mono text-xs">{item.version}</td><td className="px-3 py-3">{item.channels.map((channel) => <Badge key={channel}>{channel}</Badge>)}</td><td className="px-3 py-3 text-xs text-[var(--muted)]">{compactHash(item.installation.digest)} · {formatDate(item.installation.installed_at)}</td><td className="px-3 py-2 text-right"><div className="flex justify-end gap-2">{selected ? <Badge tone="success">{t('selected')}</Badge> : <Button size="small" onClick={() => action.mutate({ operation: 'use', value: item.reference })}>{t('use')}</Button>}<Button size="icon" variant="ghost" title={t('remove')} disabled={selected} onClick={() => action.mutate({ operation: 'remove', value: item.reference })}><Trash2 size={15} /></Button></div></td></tr>
    })}</tbody></table></div>
  </Section>
}

function SubscriptionPanel() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [urlDraft, setURL] = useState<string | null>(null)
  const [intervalDraft, setIntervalValue] = useState<string | null>(null)
  const [notice, setNotice] = useState('')
  const subscription = useQuery({ queryKey: ['subscription'], queryFn: () => api<Subscription>(session!, '/subscription') })
  const url = urlDraft ?? subscription.data?.url ?? ''
  const interval = intervalDraft ?? subscription.data?.interval ?? '24h'
  const mutate = useMutation({
    mutationFn: (update: boolean) => api(session!, update ? '/subscription/update' : '/subscription', { method: update ? 'POST' : 'PATCH', body: update ? undefined : JSON.stringify({ url, interval }) }),
    onSuccess: () => { setNotice(t('operationDone')); setURL(null); setIntervalValue(null); queryClient.invalidateQueries({ queryKey: ['subscription'] }) }, onError: (error) => setNotice(error.message),
  })
  return <Section title={t('subscriptionTab')} icon={<Download size={18} />} notice={notice}><div className="grid max-w-3xl gap-5"><Field label={t('subscriptionURL')}><Input value={url} onChange={(event) => setURL(event.target.value)} placeholder="https://example.com/config.json" /></Field><Field label={t('schedule')}><Input value={interval} onChange={(event) => setIntervalValue(event.target.value)} placeholder="24h or off" /></Field><div className="flex gap-2"><Button variant="primary" disabled={mutate.isPending} onClick={() => mutate.mutate(false)}><Save size={16} />{t('save')}</Button><Button disabled={mutate.isPending || !subscription.data?.url} onClick={() => mutate.mutate(true)}><RefreshCw size={16} />{t('updateNow')}</Button></div>{subscription.data?.last_result ? <div className="grid grid-cols-2 gap-4 border-t border-[var(--border)] pt-5 text-sm"><Info label={t('lastResult')} value={subscription.data.last_result} /><Info label={t('update')} value={formatDate(subscription.data.last_check)} /></div> : null}</div></Section>
}

function ConfigPanel() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [mode, setMode] = useState<'common' | 'json'>('common')
  const [contentDraft, setContent] = useState<string | null>(null)
  const [notice, setNotice] = useState('')
  const current = useQuery({ queryKey: ['config'], queryFn: () => api<{ hash: string; content: string }>(session!, '/configs/current'), retry: false })
  const content = contentDraft ?? current.data?.content ?? '{}\n'
  const parsed = useMemo(() => { try { return JSON.parse(content) as Record<string, any> } catch { return {} } }, [content])
  const save = useMutation({
    mutationFn: (validateOnly: boolean) => api(session!, validateOnly ? '/configs/validate' : '/configs/current', { method: validateOnly ? 'POST' : 'PUT', body: JSON.stringify({ content }) }),
    onSuccess: (_result, validateOnly) => { setNotice(validateOnly ? t('validated') : t('operationDone')); if (!validateOnly) setContent(null); queryClient.invalidateQueries({ queryKey: ['config'] }) }, onError: (error) => setNotice(error.message),
  })
  const patchCommon = useMutation({
    mutationFn: (patch: Record<string, unknown>) => api(session!, '/configs/common', { method: 'PATCH', body: JSON.stringify(patch) }),
    onSuccess: () => { setNotice(t('operationDone')); setContent(null); queryClient.invalidateQueries({ queryKey: ['config'] }); current.refetch() }, onError: (error) => setNotice(error.message),
  })
  function commonPatch() {
    patchCommon.mutate({
      'log.level': parsed.log?.level || 'info', 'log.disabled': Boolean(parsed.log?.disabled), 'log.timestamp': parsed.log?.timestamp !== false,
      'dns.final': parsed.dns?.final || '', 'dns.strategy': parsed.dns?.strategy || '', 'dns.disable_cache': Boolean(parsed.dns?.disable_cache),
      'route.final': parsed.route?.final || '', 'route.auto_detect_interface': Boolean(parsed.route?.auto_detect_interface),
    })
  }
  const updateLocal = (path: string, value: unknown) => { const copy = structuredClone(parsed); setPath(copy, path, value); setContent(`${JSON.stringify(copy, null, 2)}\n`) }
  return <Section title={t('configTab')} icon={<FileJson size={18} />} notice={notice}>
    <div className="mb-5 flex h-9 w-fit rounded-md bg-[var(--surface-hover)] p-1"><button className={`rounded px-3 text-sm ${mode === 'common' ? 'bg-[var(--surface)] font-medium shadow-sm' : 'text-[var(--muted)]'}`} onClick={() => setMode('common')}>{t('commonSettings')}</button><button className={`rounded px-3 text-sm ${mode === 'json' ? 'bg-[var(--surface)] font-medium shadow-sm' : 'text-[var(--muted)]'}`} onClick={() => setMode('json')}>{t('jsonEditor')}</button></div>
    {mode === 'common' ? <div className="grid max-w-4xl gap-5 md:grid-cols-2"><Field label={t('logLevel')}><select className="h-9 rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm" value={parsed.log?.level || 'info'} onChange={(event) => updateLocal('log.level', event.target.value)}>{['trace','debug','info','warn','error','fatal','panic'].map((value) => <option key={value}>{value}</option>)}</select></Field><Field label={t('routeFinal')}><Input value={parsed.route?.final || ''} onChange={(event) => updateLocal('route.final', event.target.value)} /></Field><Field label={t('dnsFinal')}><Input value={parsed.dns?.final || ''} onChange={(event) => updateLocal('dns.final', event.target.value)} /></Field><Field label="DNS strategy"><select className="h-9 rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm" value={parsed.dns?.strategy || ''} onChange={(event) => updateLocal('dns.strategy', event.target.value)}><option value="">default</option>{['prefer_ipv4','prefer_ipv6','ipv4_only','ipv6_only'].map((value) => <option key={value}>{value}</option>)}</select></Field><Toggle label={t('autoInterface')} checked={Boolean(parsed.route?.auto_detect_interface)} onChange={(value) => updateLocal('route.auto_detect_interface', value)} /><Toggle label="DNS cache" checked={!parsed.dns?.disable_cache} onChange={(value) => updateLocal('dns.disable_cache', !value)} /><div className="md:col-span-2"><Button variant="primary" disabled={patchCommon.isPending} onClick={commonPatch}><Save size={16} />{t('apply')}</Button></div></div> : <div className="overflow-hidden rounded-lg border border-[var(--border)]"><CodeMirror value={content} height="min(62vh, 720px)" extensions={[json()]} theme="dark" onChange={setContent} basicSetup={{ foldGutter: true, lineNumbers: true, highlightActiveLine: true }} /></div>}
    <div className="mt-5 flex items-center gap-2"><Button disabled={save.isPending} onClick={() => save.mutate(true)}><CheckCircle2 size={16} />{t('validate')}</Button><Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(false)}>{save.isPending ? <Spinner /> : <Save size={16} />}{t('save')}</Button>{current.data?.hash ? <span className="ml-auto font-mono text-xs text-[var(--muted)]">{compactHash(current.data.hash)}</span> : null}</div>
  </Section>
}

function WebUIPanel() {
  const { t } = useI18n()
  const { session, setSession } = useSession()
  const queryClient = useQueryClient()
  const [listenDraft, setListen] = useState<string | null>(null)
  const [password, setPassword] = useState('')
  const [source, setSource] = useState('')
  const [notice, setNotice] = useState('')
  const web = useQuery({ queryKey: ['web'], queryFn: () => api<{ listen: string; local_url: string; password_set: boolean; password_warning: boolean }>(session!, '/web') })
  const ui = useQuery({ queryKey: ['ui'], queryFn: () => api<{ installed: boolean; metadata?: UIMetadata }>(session!, '/ui') })
  const listen = listenDraft ?? web.data?.listen ?? '127.0.0.1:33211'
  const webMutation = useMutation({
    mutationFn: (body: Record<string, unknown>) => api<{ local_url: string; reauthenticate?: boolean }>(session!, '/web', { method: 'PATCH', body: JSON.stringify(body) }),
    onSuccess: (result) => { setNotice(t('operationDone')); setListen(null); queryClient.invalidateQueries({ queryKey: ['web'] }); if (result.reauthenticate) setSession(null); else if (result.local_url && result.local_url !== session?.baseURL) window.location.assign(result.local_url) }, onError: (error) => setNotice(error.message),
  })
  const uiMutation = useMutation({
    mutationFn: ({ operation, body }: { operation: 'install' | 'update' | 'remove'; body?: unknown }) => api(session!, '/ui' + (operation === 'remove' ? '' : `/${operation}`), { method: operation === 'remove' ? 'DELETE' : 'POST', body: body ? JSON.stringify(body) : undefined }),
    onSuccess: () => { setNotice(t('operationDone')); queryClient.invalidateQueries({ queryKey: ['ui'] }) }, onError: (error) => setNotice(error.message),
  })
  const serviceMutation = useMutation({ mutationFn: (action: string) => api(session!, '/service/action', { method: 'POST', body: JSON.stringify({ action }) }), onSuccess: () => setNotice(t('operationDone')), onError: (error) => setNotice(error.message) })
  async function upload(file?: File) {
    if (!file) return
    try { await uploadUI(session!, file); setNotice(t('operationDone')); await ui.refetch() } catch (error) { setNotice(error instanceof Error ? error.message : String(error)) }
  }
  return <div className="grid gap-5 xl:grid-cols-2">
    <Section title="Web" icon={<ServerCog size={18} />} notice={notice}><div className="grid gap-5"><Field label={t('listenAddress')} hint="127.0.0.1:33211 / 0.0.0.0:33211"><div className="flex gap-2"><Input value={listen} onChange={(event) => setListen(event.target.value)} /><Button variant="primary" onClick={() => webMutation.mutate({ listen })}>{t('apply')}</Button></div></Field><div className="border-t border-[var(--border)] pt-5"><div className="mb-3 flex items-center gap-2"><KeyRound size={16} /><h3 className="text-sm font-semibold">{t('password')}</h3><Badge tone={web.data?.password_set ? 'success' : 'warning'}>{web.data?.password_set ? t('passwordSet') : t('emptyPassword')}</Badge></div><div className="flex flex-wrap gap-2"><Input className="min-w-56 flex-1" type="password" value={password} onChange={(event) => setPassword(event.target.value)} /><Button disabled={!password} onClick={() => webMutation.mutate({ password })}>{t('setPassword')}</Button><Button variant="danger" onClick={() => webMutation.mutate({ password: '' })}>{t('clearPassword')}</Button></div></div><div className="flex gap-2 border-t border-[var(--border)] pt-5"><Button onClick={() => serviceMutation.mutate('restart')}><RefreshCw size={16} />{t('restart')}</Button><Button variant="danger" onClick={() => serviceMutation.mutate('stop')}>{t('stop')}</Button></div></div></Section>
    <Section title="UI" icon={<Package size={18} />}><div className="mb-5 rounded-lg bg-[var(--surface-hover)] p-4"><p className="text-sm font-semibold">{ui.data?.metadata?.manifest.name || t('noData')}</p><p className="mt-1 break-all text-xs text-[var(--muted)]">{ui.data?.metadata ? `${ui.data.metadata.manifest.version} · ${ui.data.metadata.source_type} · ${compactHash(ui.data.metadata.sha256)}` : t('noDataDetail')}</p></div><div className="grid gap-4"><Button variant="primary" onClick={() => uiMutation.mutate({ operation: 'install', body: { source: 'official' } })}><Download size={16} />{t('officialUI')}</Button><Field label={t('customURL')}><div className="flex gap-2"><Input value={source} onChange={(event) => setSource(event.target.value)} placeholder="https://example.com/sempre-ui.zip" /><Button disabled={!source} onClick={() => uiMutation.mutate({ operation: 'install', body: { source } })}>{t('install')}</Button></div></Field><label className="flex h-20 cursor-pointer items-center justify-center gap-2 rounded-lg border border-dashed border-[var(--border)] text-sm text-[var(--muted)] hover:bg-[var(--surface-hover)]"><Upload size={17} />{t('uploadZIP')}<input className="sr-only" type="file" accept=".zip,application/zip" onChange={(event) => void upload(event.target.files?.[0])} /></label><div className="flex gap-2"><Button disabled={!ui.data?.installed} onClick={() => uiMutation.mutate({ operation: 'update' })}><RefreshCw size={16} />{t('update')}</Button><Button variant="danger" disabled={!ui.data?.installed} onClick={() => uiMutation.mutate({ operation: 'remove' })}><Trash2 size={16} />{t('remove')}</Button></div></div></Section>
  </div>
}

function Section({ title, icon, notice, children }: { title: string; icon: ReactNode; notice?: string; children: ReactNode }) {
  return <Card className="min-w-0 p-4 md:p-5"><div className="mb-5 flex items-center gap-2"><span className="text-emerald-600">{icon}</span><h2 className="text-sm font-semibold">{title}</h2></div>{notice ? <div className="mb-4 border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm">{notice}</div> : null}{children}</Card>
}

function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return <label className="flex h-10 items-center justify-between rounded-md border border-[var(--border)] px-3 text-sm font-medium"><span>{label}</span><input className="size-4 accent-emerald-600" type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} /></label>
}

function Info({ label, value }: { label: string; value: string }) { return <div><p className="text-xs text-[var(--muted)]">{label}</p><p className="mt-1 font-medium">{value}</p></div> }

function setPath(document: Record<string, any>, path: string, value: unknown) {
  const parts = path.split('.')
  let current = document
  for (const part of parts.slice(0, -1)) { if (!current[part] || typeof current[part] !== 'object') current[part] = {}; current = current[part] }
  current[parts.at(-1)!] = value
}

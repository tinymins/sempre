import { useState, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query'
import { Download, KeyRound, Package, Power, RefreshCw, Router, ServerCog, ShieldAlert, Trash2, Upload } from 'lucide-react'
import { Select, Table, type TableColumn } from '@acme/components'
import { api, downloadBundle, uploadUI } from '../lib/api'
import { compactHash, formatDate } from '../lib/format'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { compareDate, compareText } from '../lib/sort'
import type { CoreInstallation, CoresResponse, ManagedRuntimeStatus, NetworkSettings, NetworkSettingsResponse, SystemStatus, UIMetadata } from '../lib/types'
import { Badge, Button, Card, ConfirmDialog, Field, Input, PageTitle, Spinner } from '../components/ui'
import { AutoConfigureCard } from '../features/auto-config/AutoConfigureCard'
import { NetworkAutomationPanel } from '../components/NetworkAutomationPanel'

type Tab = 'core' | 'network' | 'web'
type ChangeResult = { NeedsRestart?: boolean; changes?: ChangeResult[] }

export function Management() {
  const { t } = useI18n()
  const [tab, setTab] = useState<Tab>('core')
  const tabs: Array<{ value: Tab; label: string; icon: typeof Package }> = [
    { value: 'core', label: t('coreTab'), icon: Package }, { value: 'network', label: t('mode'), icon: Router }, { value: 'web', label: t('webUITab'), icon: ServerCog },
  ]
  return <div className="space-y-5"><PageTitle title={t('management')} /><div className="flex gap-1 overflow-x-auto border-b border-[var(--border)]">{tabs.map(({ value, label, icon: Icon }) => <button key={value} className={`flex h-11 shrink-0 items-center gap-2 border-b-2 px-3 text-sm font-medium ${tab === value ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)] hover:text-[var(--text)]'}`} onClick={() => setTab(value)}><Icon size={16} />{label}</button>)}</div>{tab === 'core' ? <div className="space-y-5"><AutoConfigureCard /><CorePanel /></div> : tab === 'network' ? <div className="space-y-5"><NetworkModePanel /><NetworkAutomationPanel /></div> : <WebUIPanel />}</div>
}

function NetworkModePanel() {
  const { locale } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const zh = locale === 'zh-CN'
  const network = useQuery({ queryKey: ['network', 'settings'], queryFn: () => api<NetworkSettingsResponse>(session!, '/network/settings') })
  const update = useMutation({
    mutationFn: (mode: NetworkSettings['mode']) => {
      if (!network.data) throw new Error('Network settings are not loaded')
      return api<NetworkSettingsResponse>(session!, '/network/settings', { method: 'PUT', body: JSON.stringify({ ...network.data.settings, mode }) })
    },
    onSuccess: (result) => {
      queryClient.setQueryData(['network', 'settings'], result)
      queryClient.invalidateQueries({ queryKey: ['system'] })
    },
  })
  const mode = network.data?.settings.mode ?? 'local'
  const gatewayAvailable = network.data?.gateway_available ?? false
  const gatewayLabel = zh ? '网关模式' : 'Gateway mode'
  const gatewayReason = zh ? '仅 Linux 系统服务可用' : 'Linux system service only'
  return <Section title={zh ? '运行模式' : 'Operating mode'} icon={<Router size={18} />}>
    <div className="grid gap-3 md:grid-cols-[minmax(0,24rem)_minmax(0,1fr)] md:items-end">
      <Field label={zh ? '当前模式' : 'Current mode'}><Select className="w-full" popupMatchSelectWidth value={mode} loading={network.isLoading || update.isPending} options={[{ value: 'local', label: zh ? '本机模式' : 'Local mode' }, { value: 'gateway', label: gatewayAvailable ? gatewayLabel : <span className="flex w-full min-w-0 items-center gap-3"><span className="shrink-0">{gatewayLabel}</span><span className="ml-auto truncate text-xs font-normal text-[var(--text-muted)]">{gatewayReason}</span></span>, disabled: !gatewayAvailable }]} onChange={(value) => update.mutate(value)} /></Field>
      <p className="self-end py-1.5 text-sm leading-5 text-[var(--muted)]">{mode === 'gateway' ? (zh ? '默认代理内网设备；本机代理可在网关页单独开启。' : 'LAN clients are proxied by default; host proxying is optional on the Gateway page.') : (zh ? '仅管理本机流量与 DNS，不加载网关配置。' : 'Manages only this host traffic and DNS. Gateway configuration is not loaded.')}</p>
    </div>
    {update.isError ? <p className="mt-3 text-sm text-red-600">{update.error instanceof Error ? update.error.message : String(update.error)}</p> : null}
  </Section>
}

function CorePanel() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [reference, setReference] = useState('sing-box@stable')
  const [notice, setNotice] = useState('')
  const cores = useQuery({ queryKey: ['cores'], queryFn: () => api<CoresResponse>(session!, '/cores') })
  const action = useMutation({
    mutationFn: ({ operation, value }: { operation: string; value?: string }) => api<ChangeResult>(session!, `/cores/${operation}`, { method: 'POST', body: JSON.stringify({ reference: value || '' }) }),
    onSuccess: (result) => { setNotice(changeNotice(result, queryClient, t('operationDone'), t('changeDeferred'))); queryClient.invalidateQueries({ queryKey: ['cores'] }); queryClient.invalidateQueries({ queryKey: ['subscriptions'] }); queryClient.invalidateQueries({ queryKey: ['system'] }); queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] }) },
    onError: (error) => setNotice(error.message),
  })
  const installedColumns: Array<TableColumn<CoreInstallation>> = [
    { title: t('core'), dataIndex: 'core', sorter: (left, right) => compareText(left.core, right.core), render: (value) => <span className="font-medium">{value}</span> },
    { title: t('repository'), dataIndex: 'repository', sorter: (left, right) => compareText(left.repository, right.repository), render: (_value, item) => <div className="flex items-center gap-2"><Badge tone={item.official ? 'success' : 'warning'}>{item.official ? t('official') : t('custom')}</Badge><span className="font-mono text-xs text-[var(--muted)]">{item.repository}</span></div> },
    { title: t('version'), dataIndex: 'version', sorter: (left, right) => compareText(left.version, right.version), render: (value) => <span className="font-mono text-xs">{value}</span> },
    { title: t('channel'), dataIndex: 'channels', sorter: (left, right) => compareText(left.channels.join(' '), right.channels.join(' ')), render: (value) => (value as string[]).map((channel) => <Badge key={channel}>{channel}</Badge>) },
    { title: t('details'), key: 'details', sorter: (left, right) => compareDate(left.installation.installed_at, right.installation.installed_at), render: (_value, item) => <span className="text-xs text-[var(--muted)]">{compactHash(item.installation.digest)} · {formatDate(item.installation.installed_at)}</span> },
    { title: '', key: 'actions', width: 192, render: (_value, item) => { const selected = isSelectedCore(cores.data, item); return <div className="flex justify-end gap-2">{selected ? <Badge tone="success">{t('selected')}</Badge> : <Button size="small" onClick={() => action.mutate({ operation: 'use', value: item.reference })}>{t('use')}</Button>}<Button size="icon" variant="ghost" title={t('remove')} disabled={selected} onClick={() => action.mutate({ operation: 'remove', value: item.reference })}><Trash2 size={15} /></Button></div> } },
  ]
  return <Section title={t('core')} icon={<Package size={18} />} notice={notice}>
	<div className="grid gap-4 border-b border-[var(--border)] pb-6 md:grid-cols-[minmax(0,1fr)_auto_auto]"><Field label={t('reference')}><><Input list="supported-core-references" value={reference} onChange={(event) => setReference(event.target.value)} placeholder="mihomo@stable" /><datalist id="supported-core-references">{cores.data?.supported.map((core) => <option key={core} value={`${core}@stable`} />)}</datalist></></Field><Button className="self-end" variant="primary" disabled={action.isPending} onClick={() => action.mutate({ operation: 'install', value: reference })}>{action.isPending ? <Spinner /> : <Download size={16} />}{t('install')}</Button><Button className="self-end" disabled={action.isPending} onClick={() => action.mutate({ operation: 'update', value: reference })}><RefreshCw size={16} />{t('update')}</Button></div>
    <h3 className="mt-6 text-sm font-semibold">{t('installedVersions')}</h3>
    <Table<CoreInstallation> className="mt-3" rowKey="reference" loading={cores.isLoading} pagination={false} columns={installedColumns} dataSource={cores.data?.installed || []} scroll={{ x: 820 }} />
  </Section>
}

function isSelectedCore(cores: CoresResponse | undefined, item: CoreInstallation) {
  const selectedRepository = cores?.selected?.repository || ''
  const itemRepository = item.official ? '' : item.repository
  return cores?.selected?.core === item.core && selectedRepository === itemRepository && (cores.selected.ref === item.version || item.channels.includes(cores.selected.ref))
}

function WebUIPanel() {
  const { t } = useI18n()
  const { session, setSession } = useSession()
  const queryClient = useQueryClient()
  const [listenDraft, setListen] = useState<string | null>(null)
  const [password, setPassword] = useState('')
  const [source, setSource] = useState('')
  const [notice, setNotice] = useState('')
  const [serviceNotice, setServiceNotice] = useState('')
  const [serviceConfirm, setServiceConfirm] = useState<'restart' | 'stop' | null>(null)
  const [serviceConfirmOpen, setServiceConfirmOpen] = useState(false)
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
  const bundleMutation = useMutation({
    mutationFn: () => downloadBundle(session!),
    onSuccess: () => setNotice(t('operationDone')),
    onError: (error) => setNotice(error.message),
  })
  const serviceMutation = useMutation({ mutationFn: (action: string) => api(session!, '/service/action', { method: 'POST', body: JSON.stringify({ action }) }), onSuccess: () => { setServiceNotice(t('operationAccepted')); setServiceConfirmOpen(false) }, onError: (error) => setServiceNotice(error.message) })
  const openServiceConfirm = (action: 'restart' | 'stop') => {
    setServiceConfirm(action)
    setServiceConfirmOpen(true)
  }
  const finishServiceConfirmClose = (open: boolean) => {
    if (!open) setServiceConfirm(null)
  }
  async function upload(file?: File) {
    if (!file) return
    try { await uploadUI(session!, file); setNotice(t('operationDone')); await ui.refetch() } catch (error) { setNotice(error instanceof Error ? error.message : String(error)) }
  }
  return <div className="grid gap-5 xl:grid-cols-2">
    <Section title="Web" icon={<ServerCog size={18} />} notice={notice}><div className="grid gap-5"><Field label={t('listenAddress')} hint="127.0.0.1:33211 / 0.0.0.0:33211"><div className="flex gap-2"><Input value={listen} onChange={(event) => setListen(event.target.value)} /><Button variant="primary" onClick={() => webMutation.mutate({ listen })}>{t('apply')}</Button></div></Field><div className="border-t border-[var(--border)] pt-5"><div className="mb-3 flex items-center gap-2"><KeyRound size={16} /><h3 className="text-sm font-semibold">{t('password')}</h3><Badge tone={web.data?.password_set ? 'success' : 'warning'}>{web.data?.password_set ? t('passwordSet') : t('emptyPassword')}</Badge></div><div className="flex flex-wrap gap-2"><Input className="min-w-56 flex-1" type="password" value={password} onChange={(event) => setPassword(event.target.value)} /><Button disabled={!password} onClick={() => webMutation.mutate({ password })}>{t('setPassword')}</Button><Button variant="danger" onClick={() => webMutation.mutate({ password: '' })}>{t('clearPassword')}</Button></div></div><div className="border-t border-[var(--border)] pt-5"><Button disabled={bundleMutation.isPending} onClick={() => bundleMutation.mutate()}>{bundleMutation.isPending ? <Spinner /> : <Download size={16} />}{t('exportBundle')}</Button><p className="mt-2 text-xs leading-5 text-[var(--muted)]">{t('exportBundleDetail')}</p></div></div></Section>
    <Section title="UI" icon={<Package size={18} />}><div className="mb-5 rounded-lg bg-[var(--surface-hover)] p-4"><p className="text-sm font-semibold">{ui.data?.metadata?.manifest.name || t('noData')}</p><p className="mt-1 break-all text-xs text-[var(--muted)]">{ui.data?.metadata ? `${ui.data.metadata.manifest.version} · ${ui.data.metadata.source_type} · ${compactHash(ui.data.metadata.sha256)}` : t('noDataDetail')}</p></div><div className="grid gap-4"><Button variant="primary" onClick={() => uiMutation.mutate({ operation: 'install', body: { source: 'official' } })}><Download size={16} />{t('officialUI')}</Button><Field label={t('customURL')}><div className="flex gap-2"><Input value={source} onChange={(event) => setSource(event.target.value)} placeholder="https://example.com/sempre-ui.zip" /><Button disabled={!source} onClick={() => uiMutation.mutate({ operation: 'install', body: { source } })}>{t('install')}</Button></div></Field><label className="flex h-20 cursor-pointer items-center justify-center gap-2 rounded-lg border border-dashed border-[var(--border)] text-sm text-[var(--muted)] hover:bg-[var(--surface-hover)]"><Upload size={17} />{t('uploadZIP')}<input className="sr-only" type="file" accept=".zip,application/zip" onChange={(event) => void upload(event.target.files?.[0])} /></label><div className="flex gap-2"><Button disabled={!ui.data?.installed} onClick={() => uiMutation.mutate({ operation: 'update' })}><RefreshCw size={16} />{t('update')}</Button><Button variant="danger" disabled={!ui.data?.installed} onClick={() => uiMutation.mutate({ operation: 'remove' })}><Trash2 size={16} />{t('remove')}</Button></div></div></Section>
    <div className="xl:col-span-2"><Section title={t('systemServiceActions')} icon={<ShieldAlert size={18} />} notice={serviceNotice}><div className="flex flex-wrap items-center justify-between gap-4"><div><Badge tone="danger">{t('dangerZone')}</Badge><p className="mt-2 max-w-2xl text-sm text-[var(--muted)]">{t('serviceRestartWarning')}</p></div><div className="flex gap-2"><Button disabled={serviceMutation.isPending} onClick={() => openServiceConfirm('restart')}><RefreshCw size={16} />{t('restart')}</Button><Button variant="danger" disabled={serviceMutation.isPending} onClick={() => openServiceConfirm('stop')}><Power size={16} />{t('stop')}</Button></div></div></Section></div>
    {serviceConfirm ? <ConfirmDialog open={serviceConfirmOpen} title={serviceConfirm === 'stop' ? t('serviceStopTitle') : t('restart')} detail={serviceConfirm === 'stop' ? t('serviceStopWarning') : t('serviceRestartWarning')} acknowledgement={serviceConfirm === 'stop' ? t('serviceStopAcknowledgement') : undefined} confirmLabel={serviceConfirm === 'stop' ? t('stop') : t('restart')} cancelLabel={t('cancel')} pending={serviceMutation.isPending} onCancel={() => setServiceConfirmOpen(false)} onConfirm={() => serviceMutation.mutate(serviceConfirm)} afterOpenChange={finishServiceConfirmClose} /> : null}
  </div>
}

function Section({ title, icon, notice, children }: { title: string; icon: ReactNode; notice?: string; children: ReactNode }) {
  return <Card className="min-w-0 p-4 md:p-5"><div className="mb-5 flex items-center gap-2"><span className="text-emerald-600">{icon}</span><h2 className="text-sm font-semibold">{title}</h2></div>{notice ? <div className="mb-4 border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm">{notice}</div> : null}{children}</Card>
}

function changeNotice(result: ChangeResult, queryClient: QueryClient, completed: string, deferred: string) {
  const needsRestart = Boolean(result.NeedsRestart || result.changes?.some((change) => change.NeedsRestart))
  const system = queryClient.getQueryData<SystemStatus>(['system'])
  const runtime = queryClient.getQueryData<ManagedRuntimeStatus>(['runtime', 'status'])
  return needsRestart && (system?.desired_state === 'stopped' || runtime?.desired_state === 'stopped') ? deferred : completed
}

import { Button, Card, Checkbox, Input, Modal, Popconfirm, Select, Spin, Tag, TextArea, useToast } from '@acme/components'
import { useCallback, useEffect, useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { useI18n } from '../i18n/provider'
import type { MessageKey } from '../i18n/zh-CN'
import { subscriptionApi } from '../subscriptions/api'
import type { UserBrief } from '../subscriptions/types'
import { ManualProxyFields } from './ManualProxyFields'
import { defaultManualContent } from './manual-proxy'
import { proxyApi, type ProxyInput, type SubscriptionProxy } from './api'

const empty: ProxyInput = { name: '', content: '', engine: null, enabled: true, authorizedUserIds: [] }

export function SubscriptionProxiesPage() {
  const { t, date } = useI18n()
  const toast = useToast()
  const [items, setItems] = useState<SubscriptionProxy[] | null>(null)
  const [configured, setConfigured] = useState(false)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState<string | null>(null)
  const [editing, setEditing] = useState<{ id: string | null; input: ProxyInput } | null>(null)
  const [editorError, setEditorError] = useState('')
  const [inputMode, setInputMode] = useState('import')
  const [users, setUsers] = useState<UserBrief[]>([])
  const refresh = useCallback(async () => {
    try { const result = await proxyApi.list(); setItems(result.items); setConfigured(result.configured); setError('') }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
  }, [])
  useEffect(() => {
    const initialLoad = window.setTimeout(() => void refresh(), 0)
    const timer = window.setInterval(() => { if (!document.hidden) void refresh() }, 5000)
    return () => { window.clearTimeout(initialLoad); window.clearInterval(timer) }
  }, [refresh])
  const action = async (id: string, task: () => Promise<unknown>) => {
    setBusy(id)
    try { await task(); await refresh() }
    catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(null) }
  }
  const open = async (proxy?: SubscriptionProxy) => {
    setEditorError('')
    setInputMode('import')
    setBusy(proxy?.id ?? 'new')
    try {
      const [result, choices] = await Promise.all([proxy ? proxyApi.edit(proxy.id) : Promise.resolve(null), subscriptionApi.users()])
      setUsers(choices)
      setEditing({ id: proxy?.id ?? null, input: result ? { name: result.name, content: result.content, engine: result.engine, enabled: result.enabled, authorizedUserIds: result.authorizedUserIds } : { ...empty, authorizedUserIds: [] } })
    } catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(null) }
  }
  const save = async () => {
    if (!editing) return
    setBusy('save'); setEditorError('')
    try { await proxyApi.save(editing.id, editing.input); setEditing(null); await refresh() }
    catch (reason) { setEditorError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(null) }
  }
  const patch = (patch: Partial<ProxyInput>) => setEditing(current => current ? { ...current, input: { ...current.input, ...patch } } : null)
  return <section className="space-y-4">
    <div className="flex flex-wrap items-center justify-between gap-3"><div><h1 className="text-xl font-semibold">{t('proxies.title')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('proxies.description')}</p></div><div className="flex gap-2"><Button icon={<RefreshCw size={16} />} onClick={() => void refresh()}>{t('proxies.refresh')}</Button><Button variant="primary" icon={<Plus size={16} />} disabled={!configured || busy !== null} onClick={() => void open()}>{t('proxies.add')}</Button></div></div>
    {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
    {items === null && !error ? <Spin /> : null}
    {items !== null && !configured ? <Card><p className="text-sm">{t('proxies.notConfigured')}</p></Card> : null}
    <Card><div className="flex items-center gap-3"><h2 className="font-medium">{t('proxies.system')}</h2><Tag>{t('proxies.builtin')}</Tag></div><p className="mt-2 text-sm text-[var(--muted)]">{t('proxies.systemHelp')}</p></Card>
    {items?.map(proxy => <Card key={proxy.id}><div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0 space-y-2"><div className="flex flex-wrap items-center gap-2"><h2 className="break-words font-medium">{proxy.name}</h2><Tag color={proxy.status === 'running' ? 'green' : proxy.lastError ? 'red' : 'default'}>{t(`proxies.status.${proxy.status}` as MessageKey)}</Tag></div><p className="text-sm text-[var(--muted)]">{proxy.protocol} · {proxy.engine} · {t('proxies.references', { count: proxy.referenceCount })}</p>{proxy.checkedAt ? <p className="text-xs text-[var(--muted)]">{t('proxies.checked')}: {date(proxy.checkedAt)}</p> : null}{proxy.lastError ? <p role="alert" className="max-w-2xl break-words text-sm text-red-600">{proxy.lastError}</p> : null}</div>
      <div className="flex flex-wrap gap-2"><Button disabled={!proxy.enabled || busy !== null} loading={busy === proxy.id} onClick={() => void action(proxy.id, async () => { const result = await proxyApi.test(proxy.id); toast.success(t('proxies.testSuccess', { ms: result.elapsedMs })) })}>{t('proxies.test')}</Button>{proxy.canEdit ? <><Button disabled={busy !== null} onClick={() => void open(proxy)}>{t('common.edit')}</Button><Button disabled={busy !== null} onClick={() => void action(proxy.id, () => proxyApi.save(proxy.id, { name: proxy.name, engine: proxy.engine, enabled: !proxy.enabled, authorizedUserIds: proxy.authorizedUserIds }))}>{t(proxy.enabled ? 'proxies.disable' : 'proxies.enable')}</Button><Popconfirm title={t('proxies.deleteConfirm')} description={t('proxies.deleteHelp')} onConfirm={() => action(proxy.id, () => proxyApi.remove(proxy.id))}><Button danger disabled={busy !== null || proxy.referenceCount > 0}>{t('common.delete')}</Button></Popconfirm></> : null}</div></div></Card>)}
    {editing ? <Modal open title={t(editing.id ? 'proxies.edit' : 'proxies.add')} onCancel={() => { if (busy !== 'save') setEditing(null) }} footer={<div className="flex justify-end gap-2"><Button disabled={busy === 'save'} onClick={() => setEditing(null)}>{t('common.cancel')}</Button><Button variant="primary" loading={busy === 'save'} onClick={() => void save()}>{t(editing.input.enabled ? 'proxies.saveEnable' : 'common.save')}</Button></div>}>
      <div className="space-y-4">{editorError ? <p role="alert" className="text-sm text-red-600">{editorError}</p> : null}
        <label className="block space-y-1 text-sm">{t('proxies.name')}<Input disabled={busy === 'save'} value={editing.input.name} onChange={event => patch({ name: event.target.value })} placeholder={t('proxies.namePlaceholder')} /></label>
        <label className="block space-y-1 text-sm">{t('proxies.engine')}<Select disabled={busy === 'save'} value={editing.input.engine ?? 'auto'} options={[{ value: 'auto', label: t('proxies.engineAuto') }, { value: 'sing-box', label: 'sing-box' }, { value: 'xray', label: 'Xray' }]} onChange={value => patch({ engine: value === 'auto' ? null : value as ProxyInput['engine'] })} className="w-full" /></label>
        <label className="block space-y-1 text-sm">{t('proxies.inputMode')}<Select disabled={busy === 'save'} value={inputMode} options={[{ value: 'import', label: t('proxies.import') }, { value: 'manual', label: t('proxies.manual') }]} onChange={value => { setInputMode(String(value)); if (value === 'manual') patch({ content: defaultManualContent() }) }} className="w-full" /></label>
        {inputMode === 'manual' ? <ManualProxyFields disabled={busy === 'save'} onChange={content => patch({ content })} /> : <label className="block space-y-1 text-sm">{t('proxies.configuration')}<TextArea disabled={busy === 'save'} rows={9} value={editing.input.content ?? ''} onChange={event => patch({ content: event.target.value })} className="font-mono text-xs" /></label>}<p className="text-xs text-[var(--muted)]">{t('proxies.configurationHelp')}</p>
        <label className="block space-y-1 text-sm">{t('proxies.authorizedUsers')}<Select mode="multiple" disabled={busy === 'save'} value={editing.input.authorizedUserIds} options={users.map(user => ({ value: user.id, label: `${user.name} (${user.email})` }))} onChange={value => patch({ authorizedUserIds: value as string[] })} className="w-full" /></label>
        <Checkbox disabled={busy === 'save'} checked={editing.input.enabled} onChange={event => patch({ enabled: event.target.checked })}>{t('proxies.startAfterSave')}</Checkbox>
      </div>
    </Modal> : null}
  </section>
}

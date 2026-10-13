import { Button, Popconfirm, Table, Tag, useToast } from '@acme/components'
import { useCallback, useEffect, useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { useI18n } from '../i18n/provider'
import type { MessageKey } from '../i18n/zh-CN'
import { subscriptionApi } from '../subscriptions/api'
import type { CustomNode } from '../subscriptions/types'
import { proxyApi, type ProxyInput, type SubscriptionProxy } from './api'
import { ProxyEditor } from './ProxyEditor'

type ProxyRow = SubscriptionProxy | { id: 'system'; builtin: true }
const builtin = (row: ProxyRow): row is { id: 'system'; builtin: true } => 'builtin' in row
const empty: ProxyInput = { name: '', content: '', engine: null, enabled: true }

export function ProxyNodesPanel() {
  const { t, date } = useI18n()
  const toast = useToast()
  const [items, setItems] = useState<SubscriptionProxy[] | null>(null)
  const [configured, setConfigured] = useState(false)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState<string | null>(null)
  const [editing, setEditing] = useState<{ id: string | null; input: ProxyInput } | null>(null)
  const [nodes, setNodes] = useState<CustomNode[]>([])
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
    setBusy(proxy?.id ?? 'new')
    try {
      const [result, nodeChoices] = await Promise.all([proxy ? proxyApi.edit(proxy.id) : Promise.resolve(null), subscriptionApi.customNodes()])
      setNodes(nodeChoices)
      setEditing({ id: proxy?.id ?? null, input: result ? { name: result.name, content: result.content, customNodeId: result.customNodeId, engine: result.engine, enabled: result.enabled } : { ...empty } })
    } catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(null) }
  }
  return <section className="min-w-0 space-y-4 pt-4">
    <div className="flex justify-end gap-2"><Button icon={<RefreshCw size={16} />} onClick={() => void refresh()}>{t('proxies.refresh')}</Button><Button variant="primary" icon={<Plus size={16} />} disabled={!configured || busy !== null} onClick={() => void open()}>{t('proxies.add')}</Button></div>
    {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
    {items !== null && !configured ? <p className="text-sm">{t('proxies.notConfigured')}</p> : null}
    <Table<ProxyRow> size="small" rowKey="id" loading={items === null && !error} dataSource={[{ id: 'system', builtin: true }, ...(items ?? [])]} pagination={false} scroll={{ x: 960 }} columns={[
      { title: t('proxies.name'), minWidth: 180, render: (_, row) => builtin(row) ? <div className="flex items-center gap-2"><span>{t('proxies.system')}</span><Tag>{t('proxies.builtin')}</Tag></div> : <span className="block max-w-64 truncate" title={row.name}>{row.name}</span> },
      { title: `${t('common.protocol')} / ${t('proxies.engine')}`, width: 150, render: (_, row) => builtin(row) ? '—' : `${row.protocol} / ${row.engine}` },
      { title: t('common.status'), width: 130, render: (_, row) => builtin(row) ? '—' : <span title={row.lastError ?? undefined}><Tag color={row.status === 'running' ? 'green' : row.lastError ? 'red' : 'default'}>{t(`proxies.status.${row.status}` as MessageKey)}</Tag></span> },
      { title: t('proxies.referenceCount'), width: 80, render: (_, row) => builtin(row) ? '—' : <span title={t('proxies.references', { count: row.referenceCount })}>{row.referenceCount}</span> },
      { title: t('proxies.checked'), width: 170, render: (_, row) => builtin(row) || !row.checkedAt ? '—' : date(row.checkedAt) },
      { title: t('common.actions'), width: 250, render: (_, row) => builtin(row) ? '—' : <div className="flex items-center gap-1">
        <Button size="small" variant="text" disabled={!row.enabled || busy !== null} loading={busy === row.id} onClick={() => void action(row.id, async () => { const result = await proxyApi.test(row.id); toast.success(t('proxies.testSuccess', { ms: result.elapsedMs })) })}>{t('proxies.test')}</Button>
        {row.canEdit ? <><Button size="small" variant="text" disabled={busy !== null} onClick={() => void open(row)}>{t('common.edit')}</Button><Button size="small" variant="text" disabled={busy !== null} onClick={() => void action(row.id, () => proxyApi.save(row.id, { name: row.name, engine: row.engine, enabled: !row.enabled }))}>{t(row.enabled ? 'proxies.disable' : 'proxies.enable')}</Button><Popconfirm title={t('proxies.deleteConfirm')} description={t('proxies.deleteHelp')} onConfirm={() => action(row.id, () => proxyApi.remove(row.id))}><Button size="small" variant="text" danger disabled={busy !== null || row.referenceCount > 0}>{t('common.delete')}</Button></Popconfirm></> : null}
      </div> },
    ]} />
    <p className="text-xs text-[var(--muted)]">{t('proxies.systemHelp')}</p>
    {editing ? <ProxyEditor id={editing.id} initial={editing.input} nodes={nodes} onSaved={refresh} onClose={() => setEditing(null)} /> : null}
  </section>
}

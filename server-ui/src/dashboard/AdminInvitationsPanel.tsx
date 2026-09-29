import { Button, Card, InputNumber, Popconfirm, Spin, Table, Tooltip, useToast } from '@acme/components'
import { useEffect, useState } from 'react'
import { adminApi, type Invitation } from './admin-api'
import { useI18n } from '../i18n/provider'

function inviteUrl(code: string): string {
  const url = new URL(window.location.href)
  url.searchParams.set('invite', code)
  url.hash = '/subscriptions'
  return url.toString()
}

export function AdminInvitationsPanel() {
  const { t, date } = useI18n()
  const toast = useToast()
  const [items, setItems] = useState<Invitation[]>([])
  const [hours, setHours] = useState<number | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const load = async () => {
    setLoading(true); setError('')
    try { setItems(await adminApi.invitations()) }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setLoading(false) }
  }
  useEffect(() => {
    let active = true
    void adminApi.invitations().then((next) => { if (active) setItems(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [])
  const copy = async (code: string) => {
    try { await navigator.clipboard.writeText(inviteUrl(code)); toast.success(t('admin.inviteCopied')) }
    catch { toast.error(t('common.copyFailed')) }
  }
  const create = async () => {
    setBusy(true)
    try {
      const result = await adminApi.createInvitation(hours ?? undefined)
      await load()
      await copy(result.code)
    } catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const remove = async (id: string) => {
    setBusy(true)
    try { await adminApi.deleteInvitation(id); toast.success(t('common.deleted')); await load() }
    catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  return <Card><div className="space-y-4">
    <h2 className="font-semibold">{t('admin.invites')}</h2>
    <div className="space-y-2">
      <label htmlFor="invitation-hours" className="block text-sm">{t('admin.inviteHours')}</label>
      <div className="flex flex-wrap items-center gap-3">
        <InputNumber id="invitation-hours" value={hours} min={1} max={8760} onChange={setHours} className="w-48 max-w-full" />
        <Button variant="primary" loading={busy} className="h-auto! min-h-8 max-w-full whitespace-normal py-2! text-center" onClick={() => void create()}>{t('admin.generateInvite')}</Button>
      </div>
    </div>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error} <Button size="small" onClick={() => void load()}>{t('common.retry')}</Button></p> : null}
    <Table<Invitation> rowKey="id" dataSource={items} pagination={false} scroll={{ x: 900 }} columns={[
      { title: t('admin.inviteCode'), width: 270, minWidth: 270, ellipsis: true, render: (_, item) => <Tooltip title={item.code}><span className="block truncate">{item.code}</span></Tooltip> },
      { title: t('common.status'), width: 160, minWidth: 160, className: 'whitespace-nowrap', render: (_, item) => item.usedAt ? t('admin.inviteUsed') : item.expiresAt && new Date(item.expiresAt).getTime() <= Date.now() ? t('admin.inviteExpired') : t('admin.inviteAvailable') },
      { title: t('admin.expiresAt'), width: 190, minWidth: 190, className: 'whitespace-nowrap', render: (_, item) => item.expiresAt ? date(item.expiresAt) : t('common.unlimited') },
      { title: t('common.actions'), width: 280, minWidth: 280, className: 'whitespace-nowrap', render: (_, item) => <div className="flex gap-1"><Button size="small" disabled={busy} onClick={() => void copy(item.code)}>{t('admin.copyInvite')}</Button><Popconfirm title={t('admin.deleteInvite')} okText={t('common.delete')} cancelText={t('common.cancel')} onConfirm={() => void remove(item.id)} okType="danger"><Button size="small" danger disabled={busy}>{t('common.delete')}</Button></Popconfirm></div> },
    ]} locale={{ emptyText: <div className="px-4 py-6 text-left text-sm text-[var(--muted)]">{t('common.noData')}</div> }} />
  </div></Card>
}

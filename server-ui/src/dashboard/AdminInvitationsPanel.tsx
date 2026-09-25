import { Button, Card, InputNumber, Popconfirm, Spin, Table } from '@acme/components'
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
  const [items, setItems] = useState<Invitation[]>([])
  const [hours, setHours] = useState<number | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
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
    setError(''); setNotice('')
    try { await navigator.clipboard.writeText(inviteUrl(code)); setNotice(t('admin.inviteCopied')) }
    catch { setError(t('common.copyFailed')) }
  }
  const create = async () => {
    setBusy(true); setError(''); setNotice('')
    try {
      const result = await adminApi.createInvitation(hours ?? undefined)
      await load()
      await copy(result.code)
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const remove = async (id: string) => {
    setBusy(true); setError(''); setNotice('')
    try { await adminApi.deleteInvitation(id); await load() }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  return <Card className="space-y-3">
    <h2 className="font-semibold">{t('admin.invites')}</h2>
    <div className="flex flex-wrap items-end gap-2"><label className="space-y-1 text-sm">{t('admin.inviteHours')}<InputNumber value={hours} min={1} max={8760} onChange={setHours} className="w-48" /></label><Button variant="primary" loading={busy} onClick={() => void create()}>{t('admin.generateInvite')}</Button></div>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error} <Button size="small" onClick={() => void load()}>{t('common.retry')}</Button></p> : null}
    {notice ? <p role="status" className="text-sm text-emerald-600">{notice}</p> : null}
    <Table<Invitation> rowKey="id" dataSource={items} pagination={false} scroll={{ x: 700 }} columns={[
      { title: t('admin.inviteCode'), dataIndex: 'code' },
      { title: t('common.status'), render: (_, item) => item.usedAt ? t('admin.inviteUsed') : item.expiresAt && new Date(item.expiresAt).getTime() <= Date.now() ? t('admin.inviteExpired') : t('admin.inviteAvailable') },
      { title: t('admin.expiresAt'), render: (_, item) => item.expiresAt ? date(item.expiresAt) : t('common.unlimited') },
      { title: t('common.actions'), render: (_, item) => <div className="flex gap-1"><Button size="small" disabled={busy} onClick={() => void copy(item.code)}>{t('admin.copyInvite')}</Button><Popconfirm title={t('admin.deleteInvite')} okText={t('common.delete')} cancelText={t('common.cancel')} onConfirm={() => void remove(item.id)} okType="danger"><Button size="small" danger disabled={busy}>{t('common.delete')}</Button></Popconfirm></div> },
    ]} locale={{ emptyText: t('common.noData') }} />
  </Card>
}

import { Button, Card, InputNumber, Popconfirm, Spin, Table } from '@acme/components'
import { useEffect, useState } from 'react'
import { adminApi, type Invitation } from './admin-api'

function inviteUrl(code: string): string {
  const url = new URL(window.location.href)
  url.searchParams.set('invite', code)
  url.hash = '/subscriptions'
  return url.toString()
}

export function AdminInvitationsPanel() {
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
    try { await navigator.clipboard.writeText(inviteUrl(code)); setNotice('邀请链接已复制。') }
    catch { setError('复制失败，请检查剪贴板权限。') }
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
    <h2 className="font-semibold">邀请码</h2>
    <div className="flex flex-wrap items-end gap-2"><label className="space-y-1 text-sm">有效时长（小时，留空则不失效）<InputNumber value={hours} min={1} max={8760} onChange={setHours} className="w-48" /></label><Button variant="primary" loading={busy} onClick={() => void create()}>生成邀请链接</Button></div>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error} <Button size="small" onClick={() => void load()}>重试</Button></p> : null}
    {notice ? <p role="status" className="text-sm text-emerald-600">{notice}</p> : null}
    <Table<Invitation> rowKey="id" dataSource={items} pagination={false} scroll={{ x: 700 }} columns={[
      { title: '邀请码', dataIndex: 'code' },
      { title: '状态', render: (_, item) => item.usedAt ? '已使用' : item.expiresAt && new Date(item.expiresAt).getTime() <= Date.now() ? '已过期' : '可用' },
      { title: '到期时间', render: (_, item) => item.expiresAt ? new Date(item.expiresAt).toLocaleString() : '不限期' },
      { title: '操作', render: (_, item) => <div className="flex gap-1"><Button size="small" disabled={busy} onClick={() => void copy(item.code)}>复制链接</Button><Popconfirm title="删除这条邀请码？" onConfirm={() => void remove(item.id)} okType="danger"><Button size="small" danger disabled={busy}>删除</Button></Popconfirm></div> },
    ]} />
  </Card>
}

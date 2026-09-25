import { Button, Card, Input, Modal, Password, Popconfirm, Select, Spin, Table } from '@acme/components'
import { useEffect, useState } from 'react'
import { adminApi, type AdminUser } from './admin-api'

const roles = [
  { value: 'user', label: '普通用户' },
  { value: 'admin', label: '管理员' },
  { value: 'superadmin', label: '超级管理员' },
]

export function AdminUsersPanel({ currentUserId, canManage }: { currentUserId: string; canManage: boolean }) {
  const [users, setUsers] = useState<AdminUser[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [resetUser, setResetUser] = useState<AdminUser | null>(null)
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<AdminUser['role']>('user')
  const load = async () => {
    setLoading(true); setError('')
    try { setUsers(await adminApi.users()) }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setLoading(false) }
  }
  useEffect(() => {
    let active = true
    void adminApi.users().then((next) => { if (active) setUsers(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [])
  const create = async () => {
    if (!name.trim() || !email.trim() || password.length < 12) { setError('请填写姓名、邮箱和至少 12 字符的密码。'); return }
    setBusy(true); setError('')
    try { await adminApi.createUser({ name: name.trim(), email: email.trim(), password, role }); setCreateOpen(false); setName(''); setEmail(''); setPassword(''); setRole('user'); await load() }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const updateRole = async (item: AdminUser, next: AdminUser['role']) => {
    setBusy(true); setError('')
    try { await adminApi.updateRole(item.id, next); await load() }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const reset = async () => {
    if (!resetUser || password.length < 12) { setError('新密码至少需要 12 个字符。'); return }
    setBusy(true); setError('')
    try { await adminApi.resetPassword(resetUser.id, password); setResetUser(null); setPassword('') }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const remove = async (id: string) => {
    setBusy(true); setError('')
    try { await adminApi.deleteUser(id); await load() }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }

  return <Card className="space-y-3">
    <div className="flex items-center justify-between gap-2"><h2 className="font-semibold">用户</h2>{canManage ? <Button size="small" onClick={() => { setError(''); setPassword(''); setCreateOpen(true) }}>添加用户</Button> : null}</div>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error} <Button size="small" onClick={() => void load()}>重试</Button></p> : null}
    <Table<AdminUser> rowKey="id" dataSource={users} pagination={false} scroll={{ x: 700 }} columns={[
      { title: '姓名', dataIndex: 'name' }, { title: '邮箱', dataIndex: 'email' },
      { title: '角色', render: (_, item) => canManage && item.id !== currentUserId && item.role !== 'superadmin' ? <Select value={item.role} options={roles} disabled={busy} onChange={(next) => void updateRole(item, next as AdminUser['role'])} className="min-w-32" /> : item.role },
      { title: '最近登录', render: (_, item) => item.lastLoginAt ? new Date(item.lastLoginAt).toLocaleString() : '从未' },
      { title: '操作', render: (_, item) => canManage && item.id !== currentUserId && item.role !== 'superadmin' ? <div className="flex gap-1"><Button size="small" disabled={busy} onClick={() => { setError(''); setPassword(''); setResetUser(item) }}>重置密码</Button><Popconfirm title={`删除用户 ${item.name}？`} description="其配置集和自定义节点也会删除。" okType="danger" onConfirm={() => void remove(item.id)}><Button size="small" danger disabled={busy}>删除</Button></Popconfirm></div> : null },
    ]} />
    {createOpen ? <Modal open title="添加用户" okText="创建" cancelText="取消" confirmLoading={busy} onCancel={() => { if (!busy) setCreateOpen(false) }} onOk={() => void create()} closable={!busy} cancelButtonProps={{ disabled: busy }}><div className="space-y-3">{error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}<label className="block space-y-1 text-sm">姓名<Input value={name} onChange={(event) => setName(event.target.value)} /></label><label className="block space-y-1 text-sm">邮箱<Input type="email" value={email} onChange={(event) => setEmail(event.target.value)} /></label><label className="block space-y-1 text-sm">密码（至少 12 字符）<Password value={password} onChange={(event) => setPassword(event.target.value)} /></label><label className="block space-y-1 text-sm">角色<Select value={role} options={roles} onChange={(next) => setRole(next as AdminUser['role'])} className="w-full" /></label></div></Modal> : null}
    {resetUser ? <Modal open title={`重置 ${resetUser.name} 的密码`} okText="重置" cancelText="取消" confirmLoading={busy} onCancel={() => { if (!busy) setResetUser(null) }} onOk={() => void reset()} closable={!busy} cancelButtonProps={{ disabled: busy }}><div className="space-y-3">{error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}<label className="block space-y-1 text-sm">新密码（至少 12 字符）<Password value={password} onChange={(event) => setPassword(event.target.value)} /></label></div></Modal> : null}
  </Card>
}

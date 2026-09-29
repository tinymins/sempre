import { Button, Card, Input, Modal, Password, Popconfirm, Select, Spin, Table, Tooltip, useToast } from '@acme/components'
import { useEffect, useState } from 'react'
import { adminApi, type AdminUser } from './admin-api'
import { useI18n } from '../i18n/provider'

export function AdminUsersPanel({ currentUserId, canManage }: { currentUserId: string; canManage: boolean }) {
  const { t, date } = useI18n()
  const toast = useToast()
  const roles = [
    { value: 'user', label: t('admin.roleUser') },
    { value: 'admin', label: t('admin.roleAdmin') },
    { value: 'superadmin', label: t('admin.roleSuper') },
  ]
  const [users, setUsers] = useState<AdminUser[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [formError, setFormError] = useState('')
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
    if (!name.trim() || !email.trim() || password.length < 12) { setFormError(t('admin.createUserError')); return }
    setBusy(true); setFormError('')
    try { await adminApi.createUser({ name: name.trim(), email: email.trim(), password, role }); setCreateOpen(false); setName(''); setEmail(''); setPassword(''); setRole('user'); toast.success(t('common.created')); await load() }
    catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const updateRole = async (item: AdminUser, next: AdminUser['role']) => {
    setBusy(true)
    try { await adminApi.updateRole(item.id, next); toast.success(t('common.saved')); await load() }
    catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const reset = async () => {
    if (!resetUser || password.length < 12) { setFormError(t('admin.passwordMin')); return }
    setBusy(true); setFormError('')
    try { await adminApi.resetPassword(resetUser.id, password); setResetUser(null); setPassword(''); toast.success(t('account.passwordUpdated')) }
    catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const remove = async (id: string) => {
    setBusy(true)
    try { await adminApi.deleteUser(id); toast.success(t('common.deleted')); await load() }
    catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }

  return <Card className="space-y-3">
    <div className="flex items-center justify-between gap-2"><h2 className="font-semibold">{t('admin.users')}</h2>{canManage ? <Button size="small" onClick={() => { setFormError(''); setPassword(''); setCreateOpen(true) }}>{t('admin.addUser')}</Button> : null}</div>
    {loading ? <Spin /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error} <Button size="small" onClick={() => void load()}>{t('common.retry')}</Button></p> : null}
    <Table<AdminUser> rowKey="id" dataSource={users} pagination={false} scroll={{ x: 1070 }} columns={[
      { title: t('account.name'), width: 175, minWidth: 175, ellipsis: true, render: (_, item) => <Tooltip title={item.name}><span className="block truncate">{item.name}</span></Tooltip> },
      { title: t('common.email'), width: 245, minWidth: 245, ellipsis: true, render: (_, item) => <Tooltip title={item.email}><span className="block truncate">{item.email}</span></Tooltip> },
      { title: t('admin.userRole'), width: 170, minWidth: 170, className: 'whitespace-nowrap', render: (_, item) => canManage && item.id !== currentUserId && item.role !== 'superadmin' ? <Select value={item.role} options={roles} disabled={busy} onChange={(next) => void updateRole(item, next as AdminUser['role'])} className="min-w-32" /> : roles.find((role) => role.value === item.role)?.label },
      { title: t('admin.lastLogin'), width: 180, minWidth: 180, className: 'whitespace-nowrap', render: (_, item) => item.lastLoginAt ? date(item.lastLoginAt) : '—' },
      { title: t('common.actions'), width: 300, minWidth: 300, className: 'whitespace-nowrap', render: (_, item) => canManage && item.id !== currentUserId && item.role !== 'superadmin' ? <div className="flex gap-1"><Button size="small" disabled={busy} onClick={() => { setFormError(''); setPassword(''); setResetUser(item) }}>{t('admin.resetPassword')}</Button><Popconfirm title={t('admin.deleteUser', { name: item.name })} description={t('admin.deleteUserHint')} okText={t('common.delete')} cancelText={t('common.cancel')} okType="danger" onConfirm={() => void remove(item.id)}><Button size="small" danger disabled={busy}>{t('common.delete')}</Button></Popconfirm></div> : null },
    ]} locale={{ emptyText: t('common.noData') }} />
    {createOpen ? <Modal open title={t('admin.addUser')} okText={t('common.create')} cancelText={t('common.cancel')} confirmLoading={busy} onCancel={() => { if (!busy) setCreateOpen(false) }} onOk={() => void create()} closable={!busy} cancelButtonProps={{ disabled: busy }}><div className="space-y-3">{formError ? <p role="alert" className="text-sm text-red-600">{formError}</p> : null}<label className="block space-y-1 text-sm">{t('account.name')}<Input value={name} onChange={(event) => setName(event.target.value)} /></label><label className="block space-y-1 text-sm">{t('common.email')}<Input type="email" value={email} onChange={(event) => setEmail(event.target.value)} /></label><label className="block space-y-1 text-sm">{t('account.newPassword')}<Password value={password} onChange={(event) => setPassword(event.target.value)} /></label><label className="block space-y-1 text-sm">{t('admin.userRole')}<Select value={role} options={roles} onChange={(next) => setRole(next as AdminUser['role'])} className="w-full" /></label></div></Modal> : null}
    {resetUser ? <Modal open title={`${t('admin.resetPassword')} · ${resetUser.name}`} okText={t('admin.resetPassword')} cancelText={t('common.cancel')} confirmLoading={busy} onCancel={() => { if (!busy) setResetUser(null) }} onOk={() => void reset()} closable={!busy} cancelButtonProps={{ disabled: busy }}><div className="space-y-3">{formError ? <p role="alert" className="text-sm text-red-600">{formError}</p> : null}<label className="block space-y-1 text-sm">{t('account.newPassword')}<Password value={password} onChange={(event) => setPassword(event.target.value)} /></label></div></Modal> : null}
  </Card>
}

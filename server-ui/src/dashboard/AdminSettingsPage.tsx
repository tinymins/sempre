import { Button, Card, Spin, Switch } from '@acme/components'
import { useEffect, useState } from 'react'
import type { ServerUser } from '../server-api'
import { adminApi } from './admin-api'
import type { SystemSettings } from './api'
import { AdminInvitationsPanel } from './AdminInvitationsPanel'
import { AdminUsersPanel } from './AdminUsersPanel'

export function AdminSettingsPage({ user }: { user: ServerUser }) {
  const [settings, setSettings] = useState<SystemSettings | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const superadmin = user.role === 'superadmin'
  const admin = superadmin || user.role === 'admin'
  const load = async () => {
    setLoading(true); setError('')
    try { setSettings(await adminApi.settings()) }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setLoading(false) }
  }
  useEffect(() => {
    if (!admin) return
    let active = true
    void adminApi.settings().then((next) => { if (active) setSettings(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [admin])
  const update = async (value: boolean) => {
    setBusy(true); setError('')
    try { setSettings(await adminApi.updateSettings({ allowRegistration: value })) }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  if (!admin) return <Card><p role="alert">当前账号没有管理员权限。</p></Card>
  return <section className="space-y-5">
    <div><h1 className="text-2xl font-semibold">管理员设置</h1><p className="mt-1 text-sm text-[var(--muted)]">管理注册、用户及邀请。</p></div>
    {loading ? <Spin /> : null}
    {error ? <Card><p role="alert" className="text-sm text-red-600">{error}</p><Button size="small" className="mt-2" onClick={() => void load()}>重试</Button></Card> : null}
    {settings ? <Card className="space-y-4"><h2 className="font-semibold">系统设置</h2><div className="flex items-center justify-between gap-3"><div><p className="text-sm">允许公开注册</p><p className="text-xs text-[var(--muted)]">关闭后仍可使用有效邀请码注册。</p></div><Switch aria-label="允许公开注册" checked={settings.allowRegistration} disabled={busy} onChange={(next) => void update(next)} /></div></Card> : null}
    <AdminUsersPanel currentUserId={user.id} canManage={superadmin} />
    {superadmin ? <AdminInvitationsPanel /> : null}
  </section>
}

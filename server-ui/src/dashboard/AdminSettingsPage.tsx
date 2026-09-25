import { Button, Card, Spin, Switch, Tabs } from '@acme/components'
import { useEffect, useState } from 'react'
import type { ServerUser } from '../server-api'
import { adminApi } from './admin-api'
import type { SystemSettings } from './api'
import { AdminInvitationsPanel } from './AdminInvitationsPanel'
import { AdminUsersPanel } from './AdminUsersPanel'
import { useI18n } from '../i18n/provider'

export function AdminSettingsPage({ user }: { user: ServerUser }) {
  const { t } = useI18n()
  const [settings, setSettings] = useState<SystemSettings | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [tab, setTab] = useState('general')
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
  if (!admin) return <Card><p role="alert">{t('admin.forbidden')}</p></Card>
  return <section className="space-y-5">
    <div><h1 className="text-2xl font-semibold">{t('admin.title')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('admin.subtitle')}</p></div>
    <Tabs items={[{ key: 'general', label: t('admin.generalTab') }, { key: 'users', label: t('admin.usersTab') }, ...(superadmin ? [{ key: 'invitations', label: t('admin.invitesTab') }] : [])]} activeKey={tab} onChange={setTab} />
    {loading ? <Spin /> : null}
    {error ? <Card><p role="alert" className="text-sm text-red-600">{error}</p><Button size="small" className="mt-2" onClick={() => void load()}>{t('common.retry')}</Button></Card> : null}
    {tab === 'general' && settings ? <Card className="space-y-4"><h2 className="font-semibold">{t('admin.systemSettings')}</h2><div className="flex items-center justify-between gap-3"><div><p className="text-sm">{t('admin.allowRegistration')}</p><p className="text-xs text-[var(--muted)]">{t('admin.registrationHint')}</p></div><Switch aria-label={t('admin.allowRegistration')} checked={settings.allowRegistration} disabled={busy} onChange={(next) => void update(next)} /></div></Card> : null}
    {tab === 'users' ? <AdminUsersPanel currentUserId={user.id} canManage={superadmin} /> : null}
    {tab === 'invitations' && superadmin ? <AdminInvitationsPanel /> : null}
  </section>
}

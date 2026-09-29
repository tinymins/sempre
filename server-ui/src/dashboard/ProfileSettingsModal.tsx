import { Avatar, Button, Input, Modal, Password, Select, Tabs, Upload, useToast } from '@acme/components'
import { useState } from 'react'
import type { ServerUser } from '../server-api'
import { accountApi, type AccountSettings } from './account-api'
import { useI18n } from '../i18n/provider'

export function ProfileSettingsModal({ user, onClose, onUpdated }: { user: ServerUser; onClose: () => void; onUpdated: (user: ServerUser) => void }) {
  const { t } = useI18n()
  const toast = useToast()
  const languageOptions = [
    { value: 'auto', label: t('account.auto') },
    { value: 'zh-CN', label: '简体中文' }, { value: 'zh-TW', label: '繁體中文' },
    { value: 'en-US', label: 'English' }, { value: 'ja-JP', label: '日本語' }, { value: 'de-DE', label: 'Deutsch' },
  ]
  const themeOptions = [{ value: 'auto', label: t('account.auto') }, { value: 'light', label: t('account.light') }, { value: 'dark', label: t('account.dark') }]
  const accentOptions = [
    { value: 'emerald', label: t('account.emerald') }, { value: 'amber', label: t('account.amber') },
    { value: 'rose', label: t('account.rose') }, { value: 'violet', label: t('account.violet') },
    { value: 'blue', label: t('account.blue') }, { value: 'cyan', label: t('account.cyan') },
  ]
  const settings = user.settings as AccountSettings | null
  const [tab, setTab] = useState('profile')
  const [name, setName] = useState(user.name)
  const [email, setEmail] = useState(user.email)
  const [langMode, setLangMode] = useState<AccountSettings['langMode']>(settings?.langMode ?? 'auto')
  const [themeMode, setThemeMode] = useState<AccountSettings['themeMode']>(settings?.themeMode ?? 'auto')
  const [accentColor, setAccentColor] = useState<AccountSettings['accentColor']>(settings?.accentColor ?? 'emerald')
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const avatarKey = typeof settings?.avatarKey === 'string' ? settings.avatarKey : null
  const avatarUrl = avatarKey ? `/api/v1/avatars/${encodeURIComponent(avatarKey)}` : undefined

  const save = async () => {
    if (!name.trim() || !email.trim()) { setError(t('account.missingNameEmail')); return }
    setBusy(true); setError('')
    try {
      const updated = await accountApi.update({ name: name.trim(), email: email.trim(), settings: { langMode, themeMode, accentColor } })
      onUpdated(updated)
      onClose()
      toast.success(t('common.saved'))
    } catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const changePassword = async () => {
    if (newPassword.length < 12) { setError(t('account.passwordMin')); return }
    if (newPassword !== confirmPassword) { setError(t('account.passwordMismatch')); return }
    setBusy(true); setError('')
    try {
      await accountApi.changePassword(currentPassword, newPassword)
      setCurrentPassword(''); setNewPassword(''); setConfirmPassword('')
      toast.success(t('account.passwordUpdated'))
    } catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const upload = async (file: File, onSuccess?: (response?: unknown) => void, onError?: (error: Error) => void) => {
    setBusy(true); setError('')
    try {
      const result = await accountApi.uploadAvatar(file)
      onUpdated(await accountApi.profile())
      onSuccess?.(result)
      toast.success(t('account.avatarUpdated'))
    } catch (reason) {
      const failure = reason instanceof Error ? reason : new Error(String(reason))
      toast.error(failure.message); onError?.(failure)
    } finally { setBusy(false) }
  }
  const removeAvatar = async () => {
    setBusy(true); setError('')
    try { await accountApi.deleteAvatar(); onUpdated(await accountApi.profile()); toast.success(t('account.avatarRemoved')) }
    catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }

  return <Modal open title={t('account.title')} size="large" footer={null} onCancel={() => { if (!busy) onClose() }} closable={!busy} keyboard={!busy} maskClosable={false}>
    <div className="space-y-4">
      <Tabs items={[{ key: 'profile', label: t('account.profile') }, { key: 'password', label: t('account.changePassword') }]} activeKey={tab} onChange={setTab} type="segment" />
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {tab === 'profile' ? <div className="space-y-4">
        <div className="flex items-center gap-3"><Avatar src={avatarUrl} alt={user.name} size={64}>{user.name.charAt(0).toUpperCase()}</Avatar><div className="space-y-2"><Upload accept="image/png,image/jpeg,image/webp,image/gif" disabled={busy} showUploadList={false} beforeUpload={(file) => { if (file.size > 5 * 1024 * 1024) { setError(t('account.avatarMax')); return false } return true }} customRequest={({ file, onSuccess, onError }) => { void upload(file, onSuccess, onError) }}><Button size="small" disabled={busy}>{t('account.uploadAvatar')}</Button></Upload>{avatarKey ? <Button size="small" danger disabled={busy} onClick={() => void removeAvatar()}>{t('account.removeAvatar')}</Button> : null}</div></div>
        <label className="block space-y-1 text-sm">{t('account.name')}<Input value={name} onChange={(event) => setName(event.target.value)} /></label>
        <label className="block space-y-1 text-sm">{t('account.email')}<Input type="email" value={email} onChange={(event) => setEmail(event.target.value)} /></label>
        <label className="block space-y-1 text-sm">{t('account.language')}<Select value={langMode} options={languageOptions} onChange={(next) => setLangMode(next as AccountSettings['langMode'])} className="w-full" /></label>
        <label className="block space-y-1 text-sm">{t('account.theme')}<Select value={themeMode} options={themeOptions} onChange={(next) => setThemeMode(next as AccountSettings['themeMode'])} className="w-full" /></label>
        <label className="block space-y-1 text-sm">{t('account.accent')}<Select value={accentColor} options={accentOptions} onChange={(next) => setAccentColor(next as AccountSettings['accentColor'])} className="w-full" /></label>
        <p className="text-xs text-[var(--muted)]">{t('account.appearanceHint')}</p>
        <div className="flex justify-end gap-2"><Button disabled={busy} onClick={onClose}>{t('common.cancel')}</Button><Button variant="primary" loading={busy} onClick={() => void save()}>{t('account.saveProfile')}</Button></div>
      </div> : <div className="space-y-4">
        <label className="block space-y-1 text-sm">{t('account.currentPassword')}<Password autoComplete="current-password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} /></label>
        <label className="block space-y-1 text-sm">{t('account.newPassword')}<Password autoComplete="new-password" value={newPassword} onChange={(event) => setNewPassword(event.target.value)} /></label>
        <label className="block space-y-1 text-sm">{t('account.confirmPassword')}<Password autoComplete="new-password" value={confirmPassword} onChange={(event) => setConfirmPassword(event.target.value)} /></label>
        <div className="flex justify-end"><Button variant="primary" loading={busy} disabled={!currentPassword || newPassword.length < 12 || !confirmPassword} onClick={() => void changePassword()}>{t('account.updatePassword')}</Button></div>
      </div>}
    </div>
  </Modal>
}

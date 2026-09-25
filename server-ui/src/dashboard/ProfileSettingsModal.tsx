import { Avatar, Button, Input, Modal, Password, Select, Tabs, Upload } from '@acme/components'
import { useState } from 'react'
import type { ServerUser } from '../server-api'
import { accountApi, type AccountSettings } from './account-api'

const themeOptions = [
  { value: 'auto', label: '跟随系统' },
  { value: 'light', label: '浅色' },
  { value: 'dark', label: '深色' },
]
const accentOptions = [
  { value: 'emerald', label: '翡翠绿' }, { value: 'amber', label: '琥珀黄' },
  { value: 'rose', label: '玫瑰红' }, { value: 'violet', label: '紫罗兰' },
  { value: 'blue', label: '蓝色' }, { value: 'cyan', label: '青色' },
]

export function ProfileSettingsModal({ user, onClose, onUpdated }: { user: ServerUser; onClose: () => void; onUpdated: (user: ServerUser) => void }) {
  const settings = user.settings as AccountSettings | null
  const [tab, setTab] = useState('profile')
  const [name, setName] = useState(user.name)
  const [email, setEmail] = useState(user.email)
  const [themeMode, setThemeMode] = useState<AccountSettings['themeMode']>(settings?.themeMode ?? 'auto')
  const [accentColor, setAccentColor] = useState<AccountSettings['accentColor']>(settings?.accentColor ?? 'emerald')
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const avatarKey = typeof settings?.avatarKey === 'string' ? settings.avatarKey : null
  const avatarUrl = avatarKey ? `/api/v1/avatars/${encodeURIComponent(avatarKey)}` : undefined

  const save = async () => {
    if (!name.trim() || !email.trim()) { setError('姓名和邮箱不能为空。'); return }
    setBusy(true); setError(''); setNotice('')
    try {
      const updated = await accountApi.update({ name: name.trim(), email: email.trim(), settings: { themeMode, accentColor } })
      onUpdated(updated)
      onClose()
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const changePassword = async () => {
    if (newPassword.length < 12) { setError('新密码至少需要 12 个字符。'); return }
    setBusy(true); setError(''); setNotice('')
    try {
      await accountApi.changePassword(currentPassword, newPassword)
      setCurrentPassword(''); setNewPassword('')
      setNotice('密码已更新，其他登录会话已失效。')
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }
  const upload = async (file: File, onSuccess?: (response?: unknown) => void, onError?: (error: Error) => void) => {
    setBusy(true); setError(''); setNotice('')
    try {
      const result = await accountApi.uploadAvatar(file)
      onUpdated(await accountApi.profile())
      onSuccess?.(result)
      setNotice('头像已更新。')
    } catch (reason) {
      const failure = reason instanceof Error ? reason : new Error(String(reason))
      setError(failure.message); onError?.(failure)
    } finally { setBusy(false) }
  }
  const removeAvatar = async () => {
    setBusy(true); setError(''); setNotice('')
    try { await accountApi.deleteAvatar(); onUpdated(await accountApi.profile()); setNotice('头像已移除。') }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setBusy(false) }
  }

  return <Modal open title="账户设置" size="large" footer={null} onCancel={() => { if (!busy) onClose() }} closable={!busy} keyboard={!busy} maskClosable={false}>
    <div className="space-y-4">
      <Tabs items={[{ key: 'profile', label: '个人资料' }, { key: 'password', label: '修改密码' }]} activeKey={tab} onChange={setTab} type="segment" />
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {notice ? <p role="status" className="text-sm text-emerald-600">{notice}</p> : null}
      {tab === 'profile' ? <div className="space-y-4">
        <div className="flex items-center gap-3"><Avatar src={avatarUrl} alt={user.name} size={64}>{user.name.charAt(0).toUpperCase()}</Avatar><div className="space-y-2"><Upload accept="image/png,image/jpeg,image/webp,image/gif" disabled={busy} showUploadList={false} beforeUpload={(file) => { if (file.size > 5 * 1024 * 1024) { setError('头像最大 5 MB。'); return false } return true }} customRequest={({ file, onSuccess, onError }) => { void upload(file, onSuccess, onError) }}><Button size="small" disabled={busy}>上传头像</Button></Upload>{avatarKey ? <Button size="small" danger disabled={busy} onClick={() => void removeAvatar()}>移除头像</Button> : null}</div></div>
        <label className="block space-y-1 text-sm">姓名<Input value={name} onChange={(event) => setName(event.target.value)} /></label>
        <label className="block space-y-1 text-sm">邮箱<Input type="email" value={email} onChange={(event) => setEmail(event.target.value)} /></label>
        <label className="block space-y-1 text-sm">主题<Select value={themeMode} options={themeOptions} onChange={(next) => setThemeMode(next as AccountSettings['themeMode'])} className="w-full" /></label>
        <label className="block space-y-1 text-sm">强调色<Select value={accentColor} options={accentOptions} onChange={(next) => setAccentColor(next as AccountSettings['accentColor'])} className="w-full" /></label>
        <p className="text-xs text-[var(--muted)]">主题与强调色保存后立即应用到此界面。</p>
        <div className="flex justify-end gap-2"><Button disabled={busy} onClick={onClose}>取消</Button><Button variant="primary" loading={busy} onClick={() => void save()}>保存个人资料</Button></div>
      </div> : <div className="space-y-4">
        <label className="block space-y-1 text-sm">当前密码<Password autoComplete="current-password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} /></label>
        <label className="block space-y-1 text-sm">新密码（至少 12 个字符）<Password autoComplete="new-password" value={newPassword} onChange={(event) => setNewPassword(event.target.value)} /></label>
        <div className="flex justify-end"><Button variant="primary" loading={busy} disabled={!currentPassword || newPassword.length < 12} onClick={() => void changePassword()}>更新密码</Button></div>
      </div>}
    </div>
  </Modal>
}

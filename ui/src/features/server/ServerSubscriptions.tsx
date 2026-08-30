import { useEffect, useState } from 'react'
import { MoreHorizontal, Pencil, Plus, Trash2 } from 'lucide-react'
import { Dropdown, Modal } from '@acme/components'
import { useNavigate, useParams } from 'react-router-dom'
import { Button, Card, EmptyState, Field, Input, PageTitle, Spinner } from '../../components/ui'
import { ServerSubscriptionEditor } from './ServerSubscriptionEditor'
import { newServerProfile, serverAPI, type ServerProfile, type ServerSession } from './server-api'
import { useServerLocaleText } from './server-i18n'

export function ServerSubscriptions({ session }: { session: ServerSession }) {
  const t = useServerLocaleText({
    title: '订阅配置', detail: '管理订阅源、规则、策略组和各版本输出。', create: '新建配置', tabLabel: '订阅配置', rename: '重命名', remove: '删除', manage: '管理配置', empty: '还没有订阅配置', emptyDetail: '创建第一个配置后即可添加订阅源并生成客户端配置。', createTitle: '新建订阅配置', createAction: '创建', renameTitle: '重命名订阅配置', save: '保存', name: '配置名称', removeTitle: '删除订阅配置', removeDetail: '删除“{name}”后，其版本、发布产物、成员和分享链接都会一并删除，且无法恢复。',
  }, {
    title: 'Subscriptions', detail: 'Manage sources, rules, groups, and versioned outputs.', create: 'New profile', tabLabel: 'Subscription profiles', rename: 'Rename', remove: 'Delete', manage: 'Manage profile', empty: 'No subscription profiles yet', emptyDetail: 'Create a profile to add sources and generate client configurations.', createTitle: 'New subscription profile', createAction: 'Create', renameTitle: 'Rename subscription profile', save: 'Save', name: 'Profile name', removeTitle: 'Delete subscription profile', removeDetail: 'Deleting “{name}” also removes its revisions, published artifacts, members, and share links. This cannot be undone.',
  })
  const { id = '' } = useParams()
  const navigate = useNavigate()
  const [profiles, setProfiles] = useState<ServerProfile[]>([])
  const [loading, setLoading] = useState(true)
  const [createOpen, setCreateOpen] = useState(false)
  const [renameProfile, setRenameProfile] = useState<ServerProfile | null>(null)
  const [deleteProfile, setDeleteProfile] = useState<ServerProfile | null>(null)
  const [name, setName] = useState('')
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  useEffect(() => {
    let cancelled = false
    void serverAPI<ServerProfile[]>(session, '/profiles').then((next) => {
      if (cancelled) return
      setProfiles(next)
      if (!id && next[0]) navigate(`/subscriptions/${next[0].id}`, { replace: true })
    }).catch((reason: Error) => setError(reason.message)).finally(() => setLoading(false))
    return () => { cancelled = true }
  }, [id, navigate, session])

  const create = async () => {
    const trimmed = name.trim()
    if (!trimmed) return
    setPending(true)
    setError('')
    try {
      const profile = await serverAPI<ServerProfile>(session, '/profiles', { method: 'POST', body: JSON.stringify({ name: trimmed, document: newServerProfile(trimmed) }) })
      setProfiles((current) => [profile, ...current])
      setCreateOpen(false)
      setName('')
      navigate(`/subscriptions/${profile.id}`)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(false)
    }
  }
  const updateProfile = (profile: ServerProfile) => setProfiles((current) => current.map((item) => item.id === profile.id ? profile : item))
  const rename = async () => {
    if (!renameProfile || !name.trim()) return
    setPending(true)
    setError('')
    try {
      const updated = await serverAPI<ServerProfile>(session, `/profiles/${renameProfile.id}`, {
        method: 'PUT', headers: { 'If-Match': `"${renameProfile.revision}"` },
        body: JSON.stringify({ name: name.trim(), document: renameProfile.document }),
      })
      updateProfile(updated)
      setRenameProfile(null)
      setName('')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(false)
    }
  }
  const remove = async () => {
    if (!deleteProfile) return
    setPending(true)
    setError('')
    try {
      await serverAPI<void>(session, `/profiles/${deleteProfile.id}`, { method: 'DELETE' })
      const remaining = profiles.filter((profile) => profile.id !== deleteProfile.id)
      setProfiles(remaining)
      setDeleteProfile(null)
      navigate(remaining[0] ? `/subscriptions/${remaining[0].id}` : '/subscriptions')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending(false)
    }
  }

  return <div className="space-y-5">
    <PageTitle title={t.title} detail={t.detail}><Button variant="primary" onClick={() => setCreateOpen(true)}><Plus size={16} />{t.create}</Button></PageTitle>
    {error ? <p role="alert" className="border-l-2 border-red-500 px-3 py-2 text-sm text-red-700 dark:text-red-300">{error}</p> : null}
    {loading ? <Card className="grid min-h-52 place-items-center"><Spinner /></Card> : profiles.length ? <>
      <div role="tablist" aria-label={t.tabLabel} className="flex items-end gap-1 overflow-x-auto border-b border-[var(--border)]">
        {profiles.map((profile) => <div key={profile.id} className={`flex h-10 shrink-0 items-center border-b-2 ${profile.id === id ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)]'}`}><button role="tab" aria-selected={profile.id === id} type="button" className="flex h-full items-center gap-2 pl-3 pr-2 text-sm font-medium hover:text-[var(--text)]" onClick={() => navigate(`/subscriptions/${profile.id}`)}><span className="max-w-48 truncate">{profile.name}</span><span className="text-[10px] uppercase opacity-70">{profile.role}</span></button>{profile.id === id && profile.role === 'owner' ? <Dropdown trigger={['click']} placement="bottomRight" menu={{ items: [{ key: 'rename', icon: <Pencil size={15} />, label: t.rename, onClick: () => { setName(profile.name); setRenameProfile(profile) } }, { key: 'delete', icon: <Trash2 size={15} />, label: t.remove, danger: true, onClick: () => setDeleteProfile(profile) }] }}><button type="button" className="mr-1 grid size-7 place-items-center rounded hover:bg-emerald-500/10" aria-label={`${t.manage}: ${profile.name}`}><MoreHorizontal size={16} /></button></Dropdown> : null}</div>)}
        <button type="button" className="grid size-10 shrink-0 place-items-center border-b-2 border-transparent text-[var(--muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]" aria-label={t.create} onClick={() => setCreateOpen(true)}><Plus size={17} /></button>
      </div>
      {id ? <ServerSubscriptionEditor key={`${id}:${profiles.find((profile) => profile.id === id)?.revision ?? 0}`} session={session} profileId={id} onProfileChange={updateProfile} /> : null}
    </> : <EmptyState title={t.empty} detail={t.emptyDetail} action={<Button variant="primary" onClick={() => setCreateOpen(true)}><Plus size={16} />{t.create}</Button>} />}
    <Modal open={createOpen} title={t.createTitle} okText={t.createAction} confirmLoading={pending} okButtonProps={{ disabled: !name.trim() }} onOk={() => { void create(); return undefined }} onCancel={() => { if (!pending) setCreateOpen(false) }} destroyOnClose><Field label={t.name}><Input autoFocus value={name} onChange={(event) => setName(event.target.value)} /></Field></Modal>
    <Modal open={Boolean(renameProfile)} title={t.renameTitle} okText={t.save} confirmLoading={pending} okButtonProps={{ disabled: !name.trim() }} onOk={() => { void rename(); return undefined }} onCancel={() => { if (!pending) { setRenameProfile(null); setName('') } }} destroyOnClose><Field label={t.name}><Input autoFocus value={name} onChange={(event) => setName(event.target.value)} /></Field></Modal>
    <Modal open={Boolean(deleteProfile)} title={t.removeTitle} okText={t.remove} confirmLoading={pending} okButtonProps={{ danger: true }} onOk={() => { void remove(); return undefined }} onCancel={() => { if (!pending) setDeleteProfile(null) }} destroyOnClose><p className="text-sm text-[var(--muted)]">{t.removeDetail.replace('{name}', deleteProfile?.name ?? '')}</p></Modal>
  </div>
}

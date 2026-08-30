import { useCallback, useEffect, useState } from 'react'
import { Copy, Link2, Trash2, UserPlus } from 'lucide-react'
import { Select } from '@acme/components'
import { Badge, Button, Card, EmptyState, Field, Input, PageTitle, Spinner } from '../../components/ui'
import { serverAPI, type ServerMember, type ServerProfile, type ServerSession, type ServerShare } from './server-api'
import { useServerLocaleText } from './server-i18n'

export function ServerManagement({ session }: { session: ServerSession }) {
  const t = useServerLocaleText({
    title: '管理', detail: '管理账号、订阅协作者和公开订阅链接。', account: '当前账号', email: '邮箱', accountId: '账号 ID', empty: '没有可管理的配置', emptyDetail: '只有配置所有者可以管理成员和分享链接。', profile: '订阅配置', members: '成员权限', membersDetail: '编辑者可以修改和发布，查看者只能读取配置。', registeredEmail: '已注册邮箱', role: '权限', viewer: '查看者', editor: '编辑者', add: '添加', remove: '移除', noMembers: '暂无协作者。', shares: '订阅分享', sharesDetail: '创建后请立即复制完整链接，服务端不会再次显示令牌。', createShare: '创建链接', copy: '复制分享链接', active: '有效', revoked: '已撤销', revoke: '撤销', noShares: '暂无分享链接。',
  }, {
    title: 'Management', detail: 'Manage your account, profile collaborators, and public subscription links.', account: 'Current account', email: 'Email', accountId: 'Account ID', empty: 'No profiles to manage', emptyDetail: 'Only profile owners can manage members and share links.', profile: 'Subscription profile', members: 'Member access', membersDetail: 'Editors can modify and publish; viewers can only read the profile.', registeredEmail: 'Registered email', role: 'Role', viewer: 'Viewer', editor: 'Editor', add: 'Add', remove: 'Remove', noMembers: 'No collaborators yet.', shares: 'Subscription sharing', sharesDetail: 'Copy the full link immediately after creation; the token is not shown again.', createShare: 'Create link', copy: 'Copy share link', active: 'Active', revoked: 'Revoked', revoke: 'Revoke', noShares: 'No share links yet.',
  })
  const [profiles, setProfiles] = useState<ServerProfile[]>([])
  const [profileId, setProfileId] = useState('')
  const [members, setMembers] = useState<ServerMember[]>([])
  const [shares, setShares] = useState<ServerShare[]>([])
  const [email, setEmail] = useState('')
  const [role, setRole] = useState<'viewer' | 'editor'>('viewer')
  const [newShareURL, setNewShareURL] = useState('')
  const [pending, setPending] = useState('load')
  const [error, setError] = useState('')
  const ownedProfiles = profiles.filter((profile) => profile.role === 'owner')
  const selectedId = ownedProfiles.some((profile) => profile.id === profileId) ? profileId : ownedProfiles[0]?.id ?? ''

  const loadAccess = useCallback(async (id: string) => {
    if (!id) return
    setPending('load')
    setError('')
    try {
      const [nextMembers, nextShares] = await Promise.all([
        serverAPI<ServerMember[]>(session, `/profiles/${id}/members`),
        serverAPI<ServerShare[]>(session, `/profiles/${id}/shares`),
      ])
      setMembers(nextMembers)
      setShares(nextShares)
      setNewShareURL('')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending('')
    }
  }, [session])

  useEffect(() => {
    void serverAPI<ServerProfile[]>(session, '/profiles').then((next) => {
      setProfiles(next)
      const first = next.find((profile) => profile.role === 'owner')?.id ?? ''
      setProfileId(first)
      return loadAccess(first)
    }).catch((reason: Error) => { setError(reason.message); setPending('') })
  }, [loadAccess, session])

  const selectProfile = (id: string) => { setProfileId(id); void loadAccess(id) }
  const addMember = async () => {
    if (!selectedId || !email.trim()) return
    setPending('member')
    setError('')
    try {
      const member = await serverAPI<ServerMember>(session, `/profiles/${selectedId}/members`, { method: 'PUT', body: JSON.stringify({ email: email.trim(), role }) })
      setMembers((current) => [...current.filter((item) => item.user_id !== member.user_id), member].sort((left, right) => left.email.localeCompare(right.email)))
      setEmail('')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending('')
    }
  }
  const removeMember = async (member: ServerMember) => {
    setPending(`member:${member.user_id}`)
    setError('')
    try {
      await serverAPI<void>(session, `/profiles/${selectedId}/members/${member.user_id}`, { method: 'DELETE' })
      setMembers((current) => current.filter((item) => item.user_id !== member.user_id))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending('')
    }
  }
  const createShare = async () => {
    setPending('share')
    setError('')
    try {
      const share = await serverAPI<ServerShare>(session, `/profiles/${selectedId}/shares`, { method: 'POST' })
      setShares((current) => [share, ...current])
      setNewShareURL(share.url ?? '')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending('')
    }
  }
  const revokeShare = async (share: ServerShare) => {
    setPending(`share:${share.id}`)
    setError('')
    try {
      await serverAPI<void>(session, `/shares/${share.id}`, { method: 'DELETE' })
      setShares((current) => current.map((item) => item.id === share.id ? { ...item, enabled: false } : item))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setPending('')
    }
  }

  return <div className="space-y-5">
    <PageTitle title={t.title} detail={t.detail} />
    <Card className="p-5"><h2 className="font-semibold">{t.account}</h2><dl className="mt-4 grid gap-4 text-sm sm:grid-cols-2"><div><dt className="text-xs text-[var(--muted)]">{t.email}</dt><dd className="mt-1 font-medium">{session.user.email}</dd></div><div><dt className="text-xs text-[var(--muted)]">{t.accountId}</dt><dd className="mt-1 break-all font-mono text-xs">{session.user.id}</dd></div></dl></Card>
    {error ? <p role="alert" className="border-l-2 border-red-500 px-3 py-2 text-sm text-red-700 dark:text-red-300">{error}</p> : null}
    {!ownedProfiles.length && pending !== 'load' ? <EmptyState title={t.empty} detail={t.emptyDetail} /> : <>
      <Card className="p-4"><Field label={t.profile}><Select className="min-w-64" value={selectedId} options={ownedProfiles.map((profile) => ({ value: profile.id, label: profile.name }))} onChange={(value) => selectProfile(String(value))} /></Field></Card>
      {pending === 'load' ? <Card className="grid min-h-40 place-items-center"><Spinner /></Card> : <div className="grid gap-4 xl:grid-cols-2">
        <Card className="space-y-4 p-5"><div><h2 className="font-semibold">{t.members}</h2><p className="mt-1 text-xs text-[var(--muted)]">{t.membersDetail}</p></div><div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_8rem_auto]"><Field label={t.registeredEmail}><Input type="email" value={email} onChange={(event) => setEmail(event.target.value)} /></Field><Field label={t.role}><Select value={role} options={[{ value: 'viewer', label: t.viewer }, { value: 'editor', label: t.editor }]} onChange={(value) => setRole(value as 'viewer' | 'editor')} /></Field><Button className="self-end" disabled={!email.trim() || Boolean(pending)} onClick={() => void addMember()}>{pending === 'member' ? <Spinner /> : <UserPlus size={16} />}{t.add}</Button></div><div className="divide-y divide-[var(--border)]">{members.map((member) => <div key={member.user_id} className="flex items-center gap-3 py-3"><span className="min-w-0 flex-1 truncate text-sm">{member.email}</span><Badge tone={member.role === 'editor' ? 'info' : 'neutral'}>{member.role === 'editor' ? t.editor : t.viewer}</Badge><Button size="icon" variant="ghost" disabled={Boolean(pending)} aria-label={`${t.remove} ${member.email}`} onClick={() => void removeMember(member)}>{pending === `member:${member.user_id}` ? <Spinner /> : <Trash2 size={15} />}</Button></div>)}{!members.length ? <p className="py-6 text-center text-sm text-[var(--muted)]">{t.noMembers}</p> : null}</div></Card>
        <Card className="space-y-4 p-5"><div className="flex items-start justify-between gap-3"><div><h2 className="font-semibold">{t.shares}</h2><p className="mt-1 text-xs text-[var(--muted)]">{t.sharesDetail}</p></div><Button disabled={Boolean(pending)} onClick={() => void createShare()}>{pending === 'share' ? <Spinner /> : <Link2 size={16} />}{t.createShare}</Button></div>{newShareURL ? <div className="flex gap-2"><Input readOnly value={newShareURL} /><Button size="icon" aria-label={t.copy} onClick={() => void navigator.clipboard.writeText(newShareURL)}><Copy size={16} /></Button></div> : null}<div className="divide-y divide-[var(--border)]">{shares.map((share) => <div key={share.id} className="flex items-center gap-3 py-3"><span className="min-w-0 flex-1 font-mono text-xs">{share.token_prefix}…</span><Badge tone={share.enabled ? 'success' : 'neutral'}>{share.enabled ? t.active : t.revoked}</Badge>{share.enabled ? <Button size="icon" variant="ghost" disabled={Boolean(pending)} aria-label={`${t.revoke} ${share.token_prefix}`} onClick={() => void revokeShare(share)}>{pending === `share:${share.id}` ? <Spinner /> : <Trash2 size={15} />}</Button> : null}</div>)}{!shares.length ? <p className="py-6 text-center text-sm text-[var(--muted)]">{t.noShares}</p> : null}</div></Card>
      </div>}
    </>}
  </div>
}

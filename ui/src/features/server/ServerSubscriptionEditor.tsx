import { ArrowLeft, Copy, Save, Share2, UserPlus } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { Modal, Select } from '@acme/components'
import { Badge, Button, Card, Field, Input, Spinner } from '../../components/ui'
import type { SubscriptionConfigurationContext, SubscriptionEditorConfig, SubscriptionProfile, SubscriptionTarget } from '../../lib/types'
import ProxySubscribeEditor, { type ProxySubscribeEditorRef, type ProxySubscribeSaveState } from '../subscriptions/toolbox/ProxySubscribeEditor'
import { ServerCustomNodes } from './ServerCustomNodes'
import { serverAPI, serverTargets, type ServerCompileResult, type ServerCustomNode, type ServerMember, type ServerProfile, type ServerProfileStats, type ServerSession, type ServerShare } from './server-api'

const defaults: SubscriptionEditorConfig = {
  rule_list: '{}', group: '[]', filter: '[]', custom_config: '[]', dns_config: '', private_access_config: '', servers: '[]',
}

const configurationContext: SubscriptionConfigurationContext = {
  key: 'server', platform: 'server',
  capabilities: {
    features: [
      'logging.level', 'routing.rule_providers', 'routing.selector', 'routing.url_test', 'routing.rules',
      'dns.local_upstream', 'private_access', 'inbound.local_proxy', 'transparent.tun', 'transparent.tproxy',
      'transparent.ebpf', 'management.external_api',
    ],
    enum_values: {},
    protocols: ['anytls', 'hysteria2', 'shadowsocks', 'socks5', 'trojan', 'vless', 'vmess'].map((protocol) => ({ protocol, transports: [], security: [] })),
  },
}

export function ServerSubscriptionEditor({ session, onLogout }: { session: ServerSession; onLogout: () => void }) {
  const { id = '' } = useParams()
  const navigate = useNavigate()
  const editorRef = useRef<ProxySubscribeEditorRef>(null)
  const [profile, setProfile] = useState<ServerProfile | null>(null)
  const [targets, setTargets] = useState<SubscriptionTarget[]>([])
  const [target, setTarget] = useState('sing-box-v13')
  const [saveState, setSaveState] = useState<ProxySubscribeSaveState>({ profileID: '', dirty: false, saving: false })
  const [members, setMembers] = useState<ServerMember[]>([])
  const [shares, setShares] = useState<ServerShare[]>([])
  const [customNodes, setCustomNodes] = useState<ServerCustomNode[]>([])
  const [stats, setStats] = useState<ServerProfileStats | null>(null)
  const [compileResult, setCompileResult] = useState<ServerCompileResult | null>(null)
  const [newShareURL, setNewShareURL] = useState('')
  const [memberEmail, setMemberEmail] = useState('')
  const [memberRole, setMemberRole] = useState<'viewer' | 'editor'>('viewer')
  const [pending, setPending] = useState('')
  const [notice, setNotice] = useState<{ tone: 'error' | 'success'; message: string } | null>(null)

  const loadOwnerData = useCallback(async (value: ServerProfile) => {
    if (value.role !== 'owner') return
    const [nextShares, nextMembers] = await Promise.all([
      serverAPI<ServerShare[]>(session, `/profiles/${id}/shares`),
      serverAPI<ServerMember[]>(session, `/profiles/${id}/members`),
    ])
    setShares(nextShares)
    setMembers(nextMembers)
  }, [id, session])

  useEffect(() => {
    let cancelled = false
    void Promise.all([serverAPI<ServerProfile>(session, `/profiles/${id}`), serverTargets(), serverAPI<ServerCustomNode[]>(session, '/custom-nodes'), serverAPI<ServerProfileStats>(session, `/profiles/${id}/stats`)])
      .then(async ([value, nextTargets, nodes, nextStats]) => {
        if (cancelled) return
        value.document = normalizeDocument(value)
        setProfile(value)
        setTargets(nextTargets)
        setCustomNodes(nodes)
        setStats(nextStats)
        if (nextTargets.length) setTarget((current) => nextTargets.some((item) => item.format === current) ? current : nextTargets[0].format)
        await loadOwnerData(value)
      })
      .catch((reason: Error) => setNotice({ tone: 'error', message: reason.message }))
    return () => { cancelled = true }
  }, [id, loadOwnerData, session])

  const saveProfile = async (candidate: SubscriptionProfile) => {
    if (!profile || profile.role === 'viewer') return
    const updated = await serverAPI<ServerProfile>(session, `/profiles/${profile.id}`, {
      method: 'PUT', headers: { 'If-Match': `"${profile.revision}"` },
      body: JSON.stringify({ name: profile.name, document: candidate }),
    })
    updated.document = normalizeDocument(updated)
    setProfile(updated)
    setNotice({ tone: 'success', message: `Saved revision ${updated.revision}.` })
  }

  const compile = async () => {
    if (!profile) return
    setPending('compile')
    setNotice(null)
    try {
      const result = await serverAPI<ServerCompileResult>(session, `/profiles/${profile.id}/compile`, {
        method: 'POST', body: JSON.stringify({ target: targets.find((item) => item.format === target) ?? { format: target } }),
      })
      setCompileResult(result)
      setNotice({ tone: 'success', message: `Compiled ${result.node_count} nodes · ${result.artifact_hash}` })
    } catch (reason) {
      setNotice({ tone: 'error', message: reason instanceof Error ? reason.message : String(reason) })
    } finally {
      setPending('')
    }
  }

  const createShare = async () => {
    if (!profile) return
    setPending('share')
    try {
      const result = await serverAPI<ServerShare>(session, `/profiles/${profile.id}/shares`, { method: 'POST' })
      setShares((current) => [result, ...current])
      setNewShareURL(result.url ?? '')
      setNotice({ tone: 'success', message: 'Share link created. Copy it now; the token cannot be recovered later.' })
    } catch (reason) {
      setNotice({ tone: 'error', message: reason instanceof Error ? reason.message : String(reason) })
    } finally {
      setPending('')
    }
  }

  const addMember = async () => {
    if (!profile || !memberEmail.trim()) return
    setPending('member')
    try {
      const member = await serverAPI<ServerMember>(session, `/profiles/${profile.id}/members`, {
        method: 'PUT', body: JSON.stringify({ email: memberEmail.trim(), role: memberRole }),
      })
      setMembers((current) => [...current.filter((item) => item.user_id !== member.user_id), member])
      setMemberEmail('')
      setNotice({ tone: 'success', message: 'Member access updated.' })
    } catch (reason) {
      setNotice({ tone: 'error', message: reason instanceof Error ? reason.message : String(reason) })
    } finally {
      setPending('')
    }
  }

  if (!profile) return <main className="grid min-h-screen place-items-center"><Spinner /></main>
  const canWrite = profile.role !== 'viewer'
  return (
    <main className="mx-auto min-h-screen max-w-7xl space-y-5 p-5">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex items-start gap-3">
          <Button size="icon" aria-label="Back to profiles" onClick={() => navigate('/server')}><ArrowLeft size={17} /></Button>
          <div><div className="flex items-center gap-2"><h1 className="text-xl font-semibold">{profile.name}</h1><Badge tone={canWrite ? 'info' : 'neutral'}>{profile.role}</Badge></div><p className="text-sm text-[var(--muted)]">Revision {profile.revision} · {session.user.email}</p></div>
        </div>
        <div className="flex gap-2"><Button variant="ghost" onClick={onLogout}>Sign out</Button>{canWrite ? <Button variant="primary" disabled={!saveState.dirty || saveState.saving} onClick={() => editorRef.current?.saveNow()}>{saveState.saving ? <Spinner /> : <Save size={16} />}Save</Button> : null}</div>
      </header>
      {notice ? <div role={notice.tone === 'error' ? 'alert' : 'status'} className={`border-l-2 px-3 py-2 text-sm ${notice.tone === 'error' ? 'border-red-500 text-red-700 dark:text-red-300' : 'border-emerald-500 text-emerald-700 dark:text-emerald-300'}`}>{notice.message}</div> : null}
      {canWrite ? (
        <ProxySubscribeEditor
          ref={editorRef}
          key={profile.id}
          profile={profile.document}
          defaults={defaults}
          customNodes={customNodes}
          configurationContext={configurationContext}
          schedule={{ interval: '24h', autoRestart: false }}
          onScheduleSave={() => undefined}
          onSave={saveProfile}
          onSaveStateChange={setSaveState}
          sourceDebug={false}
          diagnostics={<ServerPublishing target={target} targets={targets} pending={pending} newShareURL={newShareURL} shares={shares} stats={stats} result={compileResult} onTarget={setTarget} onCompile={compile} onShare={profile.role === 'owner' ? createShare : undefined} />}
        />
      ) : <Card className="p-5"><p className="mb-3 text-sm text-[var(--muted)]">This shared profile is read-only.</p><pre className="max-h-[70vh] overflow-auto text-xs">{JSON.stringify(profile.document, null, 2)}</pre></Card>}
      {profile.role === 'owner' ? <MemberManager members={members} email={memberEmail} role={memberRole} pending={pending === 'member'} onEmail={setMemberEmail} onRole={setMemberRole} onAdd={addMember} /> : null}
      {canWrite ? <ServerCustomNodes session={session} nodes={customNodes} members={members} onChange={setCustomNodes} /> : null}
    </main>
  )
}

function ServerPublishing({ target, targets, pending, newShareURL, shares, stats, result, onTarget, onCompile, onShare }: { target: string; targets: SubscriptionTarget[]; pending: string; newShareURL: string; shares: ServerShare[]; stats: ServerProfileStats | null; result: ServerCompileResult | null; onTarget: (value: string) => void; onCompile: () => void; onShare?: () => void }) {
  const [preview, setPreview] = useState(false)
  return <div className="space-y-4"><div className="flex flex-wrap items-end gap-2"><Field label="Output target"><Select className="min-w-56" value={target} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(value) => onTarget(String(value))} /></Field><Button disabled={Boolean(pending)} onClick={onCompile}>{pending === 'compile' ? <Spinner /> : null}Compile artifact</Button>{result ? <Button onClick={() => setPreview(true)}>Preview result</Button> : null}{onShare ? <Button disabled={Boolean(pending)} onClick={onShare}>{pending === 'share' ? <Spinner /> : <Share2 size={16} />}Create share link</Button> : null}</div>{newShareURL ? <div className="flex gap-2"><Input readOnly value={newShareURL} /><Button aria-label="Copy share link" onClick={() => void navigator.clipboard.writeText(newShareURL)}><Copy size={16} /></Button></div> : null}<p className="text-xs text-[var(--muted)]">{shares.length} share record(s) · {stats?.total_accesses ?? 0} artifact download(s), {stats?.today_accesses ?? 0} today. Compile the selected target before clients synchronize it.</p><Modal open={preview} title="Compiled artifact and diagnostics" footer={null} size="almost-full" onCancel={() => setPreview(false)} destroyOnClose>{result ? <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(18rem,1fr)]"><pre className="max-h-[70vh] overflow-auto whitespace-pre-wrap break-words text-xs">{result.content}</pre><div className="max-h-[70vh] space-y-3 overflow-auto text-xs"><p>{result.node_count} represented node(s) · {result.field_diffs.filter((item) => !item.represented).length} omitted</p>{result.diagnostics.map((item, index) => <p key={`${item.source_id}-${index}`} className="border-l-2 border-amber-500 pl-2">{item.message}</p>)}{result.field_diffs.filter((item) => item.dropped?.length || item.warnings?.length).map((item) => <div key={item.node} className="border-t border-[var(--border)] pt-2"><strong>{item.node}</strong><p>{item.warnings?.join('; ') || `Dropped: ${item.dropped?.join(', ')}`}</p></div>)}</div></div> : null}</Modal></div>
}

function MemberManager({ members, email, role, pending, onEmail, onRole, onAdd }: { members: ServerMember[]; email: string; role: 'viewer' | 'editor'; pending: boolean; onEmail: (value: string) => void; onRole: (value: 'viewer' | 'editor') => void; onAdd: () => void }) {
  return <Card className="space-y-4 p-5"><h2 className="font-semibold">Members</h2><div className="flex flex-wrap items-end gap-2"><Field label="Registered user email"><Input type="email" value={email} onChange={(event) => onEmail(event.target.value)} /></Field><Field label="Role"><Select value={role} options={[{ value: 'viewer', label: 'Viewer' }, { value: 'editor', label: 'Editor' }]} onChange={(value) => onRole(value as 'viewer' | 'editor')} /></Field><Button disabled={pending || !email.trim()} onClick={onAdd}>{pending ? <Spinner /> : <UserPlus size={16} />}Add or update</Button></div><div className="space-y-2">{members.map((member) => <div key={member.user_id} className="flex justify-between border-t border-[var(--border)] pt-2 text-sm"><span>{member.email}</span><Badge>{member.role}</Badge></div>)}</div></Card>
}

function normalizeDocument(profile: ServerProfile): SubscriptionProfile {
  return { ...profile.document, id: profile.id, revision: profile.revision, name: profile.name, mode: profile.document.mode || 'local' }
}

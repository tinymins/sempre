import { ArrowLeft, Copy, Save, Share2, UserPlus } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { Checkbox, Modal, Select } from '@acme/components'
import { Badge, Button, Card, Field, Input, Spinner } from '../../components/ui'
import type { SubscriptionConfigurationContext, SubscriptionEditorConfig, SubscriptionProfile, SubscriptionTarget } from '../../lib/types'
import ProxySubscribeEditor, { type ProxySubscribeEditorRef, type ProxySubscribeSaveState } from '../subscriptions/toolbox/ProxySubscribeEditor'
import { ServerCustomNodes } from './ServerCustomNodes'
import { ServerDiagnostics } from './ServerDiagnostics'
import { serverAPI, serverTargets, type ServerCompileResult, type ServerCustomNode, type ServerMember, type ServerProfile, type ServerProfileStats, type ServerRefreshSettings, type ServerSession, type ServerShare } from './server-api'

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
  const [refreshSettings, setRefreshSettings] = useState<ServerRefreshSettings | null>(null)
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
    void Promise.all([serverAPI<ServerProfile>(session, `/profiles/${id}`), serverTargets(), serverAPI<ServerCustomNode[]>(session, '/custom-nodes'), serverAPI<ServerProfileStats>(session, `/profiles/${id}/stats`), serverAPI<ServerRefreshSettings>(session, `/profiles/${id}/refresh`)])
      .then(async ([value, nextTargets, nodes, nextStats, nextRefreshSettings]) => {
        if (cancelled) return
        value.document = normalizeDocument(value)
        setProfile(value)
        setTargets(nextTargets)
        setCustomNodes(nodes)
        setStats(nextStats)
        setRefreshSettings(nextRefreshSettings)
        if (nextTargets.length) setTarget(nextRefreshSettings.targets.find((format) => nextTargets.some((item) => item.format === format)) ?? nextTargets[0].format)
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
    if (!profile || !refreshSettings) return
    setPending('compile')
    setNotice(null)
    try {
      const settings = await serverAPI<ServerRefreshSettings>(session, `/profiles/${profile.id}/refresh`, {
        method: 'PUT', body: JSON.stringify({ enabled: refreshSettings.enabled, interval_minutes: refreshSettings.interval_minutes, targets: [target] }),
      })
      setRefreshSettings(settings)
      const [result] = await serverAPI<ServerCompileResult[]>(session, `/profiles/${profile.id}/refresh`, { method: 'POST' })
      setCompileResult(result)
      setRefreshSettings(await serverAPI<ServerRefreshSettings>(session, `/profiles/${profile.id}/refresh`))
      setNotice({ tone: 'success', message: `Published ${result.node_count} nodes · ${result.artifact_hash}` })
    } catch (reason) {
      setNotice({ tone: 'error', message: reason instanceof Error ? reason.message : String(reason) })
    } finally {
      setPending('')
    }
  }

  const saveRefreshSettings = async (change: Partial<Pick<ServerRefreshSettings, 'enabled' | 'interval_minutes' | 'targets'>>) => {
    if (!profile || !refreshSettings) return
    const updated = await serverAPI<ServerRefreshSettings>(session, `/profiles/${profile.id}/refresh`, {
      method: 'PUT',
      body: JSON.stringify({
        enabled: change.enabled ?? refreshSettings.enabled,
        interval_minutes: change.interval_minutes ?? refreshSettings.interval_minutes,
        targets: change.targets ?? refreshSettings.targets,
      }),
    })
    setRefreshSettings(updated)
  }

  const saveSchedule = async (change: { interval?: string }) => {
    if (!change.interval) return
    await saveRefreshSettings({ interval_minutes: parseInterval(change.interval) })
  }

  const changeTarget = (value: string) => {
    setTarget(value)
    void saveRefreshSettings({ targets: [value] }).catch((reason: Error) => setNotice({ tone: 'error', message: reason.message }))
  }

  const setRefreshEnabled = (enabled: boolean) => {
    void saveRefreshSettings({ enabled, targets: [target] }).catch((reason: Error) => setNotice({ tone: 'error', message: reason.message }))
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
          schedule={{ interval: formatInterval(refreshSettings?.interval_minutes ?? 1440), autoRestart: false }}
          onScheduleSave={saveSchedule}
          showAutoRestart={false}
          onSave={saveProfile}
          onSaveStateChange={setSaveState}
          sourceDebug={false}
          diagnostics={<><ServerPublishing target={target} targets={targets} pending={pending} newShareURL={newShareURL} shares={shares} stats={stats} result={compileResult} refreshSettings={refreshSettings} onTarget={changeTarget} onCompile={compile} onRefreshEnabled={setRefreshEnabled} onShare={profile.role === 'owner' ? createShare : undefined} /><ServerDiagnostics session={session} profileId={profile.id} sources={profile.document.sources} target={target} targets={targets} /></>}
        />
      ) : <Card className="p-5"><p className="mb-3 text-sm text-[var(--muted)]">This shared profile is read-only.</p><pre className="max-h-[70vh] overflow-auto text-xs">{JSON.stringify(profile.document, null, 2)}</pre></Card>}
      {profile.role === 'owner' ? <MemberManager members={members} email={memberEmail} role={memberRole} pending={pending === 'member'} onEmail={setMemberEmail} onRole={setMemberRole} onAdd={addMember} /> : null}
      {canWrite ? <ServerCustomNodes session={session} nodes={customNodes} members={members} onChange={setCustomNodes} /> : null}
    </main>
  )
}

function ServerPublishing({ target, targets, pending, newShareURL, shares, stats, result, refreshSettings, onTarget, onCompile, onRefreshEnabled, onShare }: { target: string; targets: SubscriptionTarget[]; pending: string; newShareURL: string; shares: ServerShare[]; stats: ServerProfileStats | null; result: ServerCompileResult | null; refreshSettings: ServerRefreshSettings | null; onTarget: (value: string) => void; onCompile: () => void; onRefreshEnabled: (enabled: boolean) => void; onShare?: () => void }) {
  const [preview, setPreview] = useState(false)
  return <div className="space-y-4"><div className="flex flex-wrap items-end gap-2"><Field label="Output target"><Select className="min-w-56" value={target} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(value) => onTarget(String(value))} /></Field><Button disabled={Boolean(pending)} onClick={onCompile}>{pending === 'compile' ? <Spinner /> : null}Refresh and publish now</Button>{result ? <Button onClick={() => setPreview(true)}>Preview result</Button> : null}{onShare ? <Button disabled={Boolean(pending)} onClick={onShare}>{pending === 'share' ? <Spinner /> : <Share2 size={16} />}Create share link</Button> : null}</div><label className="flex items-center gap-2 text-sm"><Checkbox checked={refreshSettings?.enabled ?? false} onChange={(event) => onRefreshEnabled(event.target.checked)} /><span>Automatically refresh and publish this target</span></label>{refreshSettings ? <p className="text-xs text-[var(--muted)]">Last refresh: {refreshSettings.last_refresh_status}{refreshSettings.last_refresh_at ? ` · ${new Date(refreshSettings.last_refresh_at).toLocaleString()}` : ''}{refreshSettings.next_refresh_at ? ` · next ${new Date(refreshSettings.next_refresh_at).toLocaleString()}` : ''}</p> : null}{refreshSettings?.last_refresh_error ? <p role="alert" className="text-xs text-red-700 dark:text-red-300">{refreshSettings.last_refresh_error}</p> : null}{newShareURL ? <div className="flex gap-2"><Input readOnly value={newShareURL} /><Button aria-label="Copy share link" onClick={() => void navigator.clipboard.writeText(newShareURL)}><Copy size={16} /></Button></div> : null}<p className="text-xs text-[var(--muted)]">{shares.length} share record(s) · {stats?.total_accesses ?? 0} artifact download(s), {stats?.today_accesses ?? 0} today. The public link keeps serving the last successful artifact if a later draft fails.</p><Modal open={preview} title="Compiled artifact and diagnostics" footer={null} size="almost-full" onCancel={() => setPreview(false)} destroyOnClose>{result ? <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(18rem,1fr)]"><pre className="max-h-[70vh] overflow-auto whitespace-pre-wrap break-words text-xs">{result.content}</pre><div className="max-h-[70vh] space-y-3 overflow-auto text-xs"><p>{result.node_count} represented node(s) · {result.field_diffs.filter((item) => !item.represented).length} omitted</p>{result.diagnostics.map((item, index) => <p key={`${item.source_id}-${index}`} className="border-l-2 border-amber-500 pl-2">{item.message}</p>)}{result.field_diffs.filter((item) => item.dropped?.length || item.warnings?.length).map((item) => <div key={item.node} className="border-t border-[var(--border)] pt-2"><strong>{item.node}</strong><p>{item.warnings?.join('; ') || `Dropped: ${item.dropped?.join(', ')}`}</p></div>)}</div></div> : null}</Modal></div>
}

function MemberManager({ members, email, role, pending, onEmail, onRole, onAdd }: { members: ServerMember[]; email: string; role: 'viewer' | 'editor'; pending: boolean; onEmail: (value: string) => void; onRole: (value: 'viewer' | 'editor') => void; onAdd: () => void }) {
  return <Card className="space-y-4 p-5"><h2 className="font-semibold">Members</h2><div className="flex flex-wrap items-end gap-2"><Field label="Registered user email"><Input type="email" value={email} onChange={(event) => onEmail(event.target.value)} /></Field><Field label="Role"><Select value={role} options={[{ value: 'viewer', label: 'Viewer' }, { value: 'editor', label: 'Editor' }]} onChange={(value) => onRole(value as 'viewer' | 'editor')} /></Field><Button disabled={pending || !email.trim()} onClick={onAdd}>{pending ? <Spinner /> : <UserPlus size={16} />}Add or update</Button></div><div className="space-y-2">{members.map((member) => <div key={member.user_id} className="flex justify-between border-t border-[var(--border)] pt-2 text-sm"><span>{member.email}</span><Badge>{member.role}</Badge></div>)}</div></Card>
}

function normalizeDocument(profile: ServerProfile): SubscriptionProfile {
  return { ...profile.document, id: profile.id, revision: profile.revision, name: profile.name, mode: profile.document.mode || 'local' }
}

function formatInterval(minutes: number) {
  if (minutes % 1440 === 0) return `${minutes / 1440}d`
  if (minutes % 60 === 0) return `${minutes / 60}h`
  return `${minutes}m`
}

function parseInterval(value: string) {
  const match = value.trim().match(/^(\d+)\s*([mhd])$/i)
  if (!match) throw new Error('Use an interval such as 30m, 12h, or 1d.')
  const multiplier = { m: 1, h: 60, d: 1440 }[match[2].toLowerCase() as 'm' | 'h' | 'd']
  return Number(match[1]) * multiplier
}

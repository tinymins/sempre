import { Copy, Save, Share2 } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { Checkbox, Modal, Select } from '@acme/components'
import { Badge, Button, Field, Input, PageTitle, Spinner } from '../../components/ui'
import type { SubscriptionConfigurationContext, SubscriptionEditorConfig, SubscriptionProfile, SubscriptionTarget } from '../../lib/types'
import ProxySubscribeEditor, { type ProxySubscribeEditorRef, type ProxySubscribeSaveState } from '../subscriptions/toolbox/ProxySubscribeEditor'
import { ServerDiagnostics } from './ServerDiagnostics'
import { serverAPI, serverTargets, type ServerCompileResult, type ServerCustomNode, type ServerProfile, type ServerProfileStats, type ServerRefreshSettings, type ServerSession, type ServerShare } from './server-api'
import { useServerT } from './server-i18n'

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

export function ServerSubscriptionEditor({ session, profileId: id, onProfileChange }: { session: ServerSession; profileId: string; onProfileChange?: (profile: ServerProfile) => void }) {
  const t = useServerT()
  const editorRef = useRef<ProxySubscribeEditorRef>(null)
  const [profile, setProfile] = useState<ServerProfile | null>(null)
  const [targets, setTargets] = useState<SubscriptionTarget[]>([])
  const [target, setTarget] = useState('sing-box-v13')
  const [saveState, setSaveState] = useState<ProxySubscribeSaveState>({ profileID: '', dirty: false, saving: false })
  const [shares, setShares] = useState<ServerShare[]>([])
  const [customNodes, setCustomNodes] = useState<ServerCustomNode[]>([])
  const [stats, setStats] = useState<ServerProfileStats | null>(null)
  const [refreshSettings, setRefreshSettings] = useState<ServerRefreshSettings | null>(null)
  const [compileResult, setCompileResult] = useState<ServerCompileResult | null>(null)
  const [newShareURL, setNewShareURL] = useState('')
  const [pending, setPending] = useState('')
  const [notice, setNotice] = useState<{ tone: 'error' | 'success'; message: string } | null>(null)

  const loadOwnerData = useCallback(async (value: ServerProfile) => {
    if (value.role !== 'owner') return
    setShares(await serverAPI<ServerShare[]>(session, `/profiles/${id}/shares`))
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
    onProfileChange?.(updated)
    setNotice({ tone: 'success', message: t('savedRevision', { revision: updated.revision }) })
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
      setNotice({ tone: 'success', message: t('published', { nodes: result.node_count, hash: result.artifact_hash }) })
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
      setNotice({ tone: 'success', message: t('shareCreated') })
    } catch (reason) {
      setNotice({ tone: 'error', message: reason instanceof Error ? reason.message : String(reason) })
    } finally {
      setPending('')
    }
  }

  if (!profile) return <main className="grid min-h-screen place-items-center"><Spinner /></main>
  const canWrite = profile.role !== 'viewer'
  return (
    <div className="space-y-5">
      <PageTitle title={profile.name} detail={`${t('revision')} ${profile.revision} · ${session.user.email}`}><div className="flex items-center gap-2"><Badge tone={canWrite ? 'info' : 'neutral'}>{profile.role}</Badge>{canWrite ? <Button variant="primary" disabled={!saveState.dirty || saveState.saving} onClick={() => editorRef.current?.saveNow()}>{saveState.saving ? <Spinner /> : <Save size={16} />}{t('save')}</Button> : null}</div></PageTitle>
      {notice ? <div role={notice.tone === 'error' ? 'alert' : 'status'} className={`border-l-2 px-3 py-2 text-sm ${notice.tone === 'error' ? 'border-red-500 text-red-700 dark:text-red-300' : 'border-emerald-500 text-emerald-700 dark:text-emerald-300'}`}>{notice.message}</div> : null}
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
          readOnly={!canWrite}
          onSave={saveProfile}
          onSaveStateChange={setSaveState}
          sourceDebug={false}
          diagnostics={<><ServerPublishing target={target} targets={targets} pending={pending} newShareURL={newShareURL} shares={shares} stats={stats} result={compileResult} refreshSettings={refreshSettings} onTarget={changeTarget} onCompile={canWrite ? compile : undefined} onRefreshEnabled={canWrite ? setRefreshEnabled : undefined} onShare={profile.role === 'owner' ? createShare : undefined} /><ServerDiagnostics session={session} profileId={profile.id} sources={profile.document.sources} target={target} targets={targets} /></>}
        />
      {!canWrite ? <p className="border-l-2 border-cyan-500 px-3 py-2 text-sm text-[var(--muted)]">{t('readOnly')}</p> : null}
    </div>
  )
}

function ServerPublishing({ target, targets, pending, newShareURL, shares, stats, result, refreshSettings, onTarget, onCompile, onRefreshEnabled, onShare }: { target: string; targets: SubscriptionTarget[]; pending: string; newShareURL: string; shares: ServerShare[]; stats: ServerProfileStats | null; result: ServerCompileResult | null; refreshSettings: ServerRefreshSettings | null; onTarget: (value: string) => void; onCompile?: () => void; onRefreshEnabled?: (enabled: boolean) => void; onShare?: () => void }) {
  const t = useServerT()
  const [preview, setPreview] = useState(false)
  return <div className="space-y-4"><div className="flex flex-wrap items-end gap-2"><Field label={t('outputTarget')}><Select className="min-w-56" value={target} options={targets.map((item) => ({ value: item.format, label: item.format }))} onChange={(value) => onTarget(String(value))} /></Field>{onCompile ? <Button disabled={Boolean(pending)} onClick={onCompile}>{pending === 'compile' ? <Spinner /> : null}{t('refreshNow')}</Button> : null}{result ? <Button onClick={() => setPreview(true)}>{t('previewResult')}</Button> : null}{onShare ? <Button disabled={Boolean(pending)} onClick={onShare}>{pending === 'share' ? <Spinner /> : <Share2 size={16} />}{t('createShare')}</Button> : null}</div>{onRefreshEnabled ? <label className="flex items-center gap-2 text-sm"><Checkbox checked={refreshSettings?.enabled ?? false} onChange={(event) => onRefreshEnabled(event.target.checked)} /><span>{t('autoRefresh')}</span></label> : null}{refreshSettings ? <p className="text-xs text-[var(--muted)]">{t('lastRefresh')}: {refreshSettings.last_refresh_status}{refreshSettings.last_refresh_at ? ` · ${new Date(refreshSettings.last_refresh_at).toLocaleString()}` : ''}{refreshSettings.next_refresh_at ? ` · ${t('nextRefresh')} ${new Date(refreshSettings.next_refresh_at).toLocaleString()}` : ''}</p> : null}{refreshSettings?.last_refresh_error ? <p role="alert" className="text-xs text-red-700 dark:text-red-300">{refreshSettings.last_refresh_error}</p> : null}{newShareURL ? <div className="flex gap-2"><Input readOnly value={newShareURL} /><Button aria-label="Copy share link" onClick={() => void navigator.clipboard.writeText(newShareURL)}><Copy size={16} /></Button></div> : null}<p className="text-xs text-[var(--muted)]">{t('shareStats', { shares: shares.length, total: stats?.total_accesses ?? 0, today: stats?.today_accesses ?? 0 })}</p><Modal open={preview} title={t('compiledTitle')} footer={null} size="almost-full" onCancel={() => setPreview(false)} destroyOnClose>{result ? <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(18rem,1fr)]"><pre className="max-h-[70vh] overflow-auto whitespace-pre-wrap break-words text-xs">{result.content}</pre><div className="max-h-[70vh] space-y-3 overflow-auto text-xs"><p>{t('represented', { nodes: result.node_count, omitted: result.field_diffs.filter((item) => !item.represented).length })}</p>{result.diagnostics.map((item, index) => <p key={`${item.source_id}-${index}`} className="border-l-2 border-amber-500 pl-2">{item.message}</p>)}{result.field_diffs.filter((item) => item.dropped?.length || item.warnings?.length).map((item) => <div key={item.node} className="border-t border-[var(--border)] pt-2"><strong>{item.node}</strong><p>{item.warnings?.join('; ') || `Dropped: ${item.dropped?.join(', ')}`}</p></div>)}</div></div> : null}</Modal></div>
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

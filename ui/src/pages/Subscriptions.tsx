import { useMemo, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import CodeMirror from '@uiw/react-codemirror'
import { json } from '@codemirror/lang-json'
import { Activity, CheckCircle2, FileJson, Plus, Power, RefreshCw, Trash2 } from 'lucide-react'
import type { ProxyDebugFormat } from '@acme/types'
import { AcmeContentBoundary } from '../components/AcmeContentBoundary'
import { Badge, Button, Card, Field, Input, PageTitle, Spinner } from '../components/ui'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { parseJSONC } from '../lib/jsonc'
import { useSession } from '../lib/session'
import type { CustomNode, SubscriptionCatalogResponse, SubscriptionProfile } from '../lib/types'
import { MessageBridge } from '../features/subscriptions/toolbox/MessageBridge'
import ProxyDebugModal, { type ProxyDebugModalRef } from '../features/subscriptions/toolbox/ProxyDebugModal'
import ProxyPreviewModal, { type ProxyPreviewModalRef } from '../features/subscriptions/toolbox/ProxyPreviewModal'
import ProxySubscribeModal from '../features/subscriptions/toolbox/ProxySubscribeModal'

type SaveResponse = { change: { changed: boolean; message: string }; render?: { warnings?: string[] } }
type Section = 'editor' | 'diagnostics'

export function Subscriptions() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [selectedID, setSelectedID] = useState('')
  const [draft, setDraft] = useState<SubscriptionProfile | null>(null)
  const [newName, setNewName] = useState('')
  const [notice, setNotice] = useState('')
  const [section, setSection] = useState<Section>('editor')
  const [format, setFormat] = useState<ProxyDebugFormat>('sing-box-v13')
  const previewRef = useRef<ProxyPreviewModalRef>(null)
  const debugRef = useRef<ProxyDebugModalRef>(null)

  const catalog = useQuery({ queryKey: ['subscriptions'], queryFn: () => api<SubscriptionCatalogResponse>(session!, '/subscriptions') })
  const customNodes = useQuery({ queryKey: ['custom-nodes'], queryFn: () => api<{ nodes: CustomNode[] }>(session!, '/custom-nodes') })
  const profiles = useMemo(() => catalog.data?.profiles ?? [], [catalog.data?.profiles])
  const effectiveSelectedID = selectedID || catalog.data?.active_profile_id || profiles[0]?.id || ''
  const storedProfile = profiles.find((item) => item.id === effectiveSelectedID) ?? null
  const currentProfile = draft?.id === effectiveSelectedID ? draft : storedProfile

  const invalidate = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['subscriptions'] }),
      queryClient.invalidateQueries({ queryKey: ['system'] }),
      queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] }),
    ])
  }

  const save = useMutation({
    mutationFn: (candidate: SubscriptionProfile) => api<SaveResponse>(session!, `/subscriptions/${candidate.id}`, { method: 'PUT', body: JSON.stringify(candidate) }),
    onSuccess: async (result) => {
      setNotice(result.change.message || t('staged'))
      await invalidate()
      setDraft(null)
    },
    onError: (error) => setNotice(error.message),
  })

  const create = useMutation({
    mutationFn: () => api<SubscriptionProfile>(session!, '/subscriptions', { method: 'POST', body: JSON.stringify({ name: newName.trim() }) }),
    onSuccess: async (profile) => {
      setNewName('')
      setSelectedID(profile.id)
      setDraft(null)
      await invalidate()
    },
    onError: (error) => setNotice(error.message),
  })

  const remove = useMutation({
    mutationFn: (id: string) => api(session!, `/subscriptions/${id}`, { method: 'DELETE' }),
    onSuccess: async () => {
      setSelectedID('')
      setDraft(null)
      await invalidate()
    },
    onError: (error) => setNotice(error.message),
  })

  const action = useMutation({
    mutationFn: (operation: 'activate' | 'refresh') => api<SaveResponse>(session!, `/subscriptions/${effectiveSelectedID}/${operation}`, { method: 'POST' }),
    onSuccess: async (result) => {
      setNotice(result.change.message)
      await invalidate()
    },
    onError: (error) => setNotice(error.message),
  })

  const schedule = useMutation({
    mutationFn: (body: Record<string, unknown>) => api(session!, '/subscription', { method: 'PATCH', body: JSON.stringify(body) }),
    onSuccess: invalidate,
    onError: (error) => setNotice(error.message),
  })

  const isActive = currentProfile?.id === catalog.data?.active_profile_id
  const resetDraft = () => setDraft(null)

  return (
    <div className="space-y-5">
      <PageTitle title={t('subscriptions')}>
        <Button disabled={!currentProfile || action.isPending} onClick={() => action.mutate('refresh')}>
          <RefreshCw size={16} />{t('updateNow')}
        </Button>
      </PageTitle>

      <div className="flex items-end gap-1 overflow-x-auto border-b border-[var(--border)]">
        {profiles.map((profile) => (
          <button
            key={profile.id}
            className={`flex h-10 shrink-0 items-center gap-2 border-b-2 px-3 text-sm font-medium ${effectiveSelectedID === profile.id ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)]'}`}
            onClick={() => { setSelectedID(profile.id); setDraft(null) }}
          >
            {profile.id === catalog.data?.active_profile_id ? <span className="size-2 rounded-full bg-emerald-500" /> : null}
            {profile.name || t('defaultSubscription')}
          </button>
        ))}
        <div className="flex shrink-0 items-center gap-1 pb-1 pl-2">
          <Input className="w-36" value={newName} onChange={(event) => setNewName(event.target.value)} placeholder={t('profileName')} />
          <Button size="icon" title={t('addProfile')} disabled={!newName.trim() || create.isPending} onClick={() => create.mutate()}><Plus size={16} /></Button>
        </div>
      </div>

      {notice ? <div className="border-l-2 border-emerald-500 bg-emerald-500/8 px-3 py-2 text-sm break-words">{notice}</div> : null}

      {currentProfile ? (
        <>
          <Card className="p-4">
            <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_180px_180px]">
              <Field label={t('profileName')}><Input value={currentProfile.name} onChange={(event) => setDraft({ ...currentProfile, name: event.target.value })} /></Field>
              <Field label={t('schedule')}>
                <Input
                  value={catalog.data?.schedule.interval || '24h'}
                  onChange={(event) => queryClient.setQueryData<SubscriptionCatalogResponse>(['subscriptions'], (value) => value ? { ...value, schedule: { ...value.schedule, interval: event.target.value } } : value)}
                  onBlur={(event) => schedule.mutate({ interval: event.target.value })}
                />
              </Field>
              <label className="flex h-9 items-center justify-between self-end rounded-md border border-[var(--border)] px-3 text-sm">
                <span>{t('automaticRestart')}</span>
                <input type="checkbox" className="size-4 accent-emerald-600" checked={Boolean(catalog.data?.auto_restart)} onChange={(event) => schedule.mutate({ auto_restart: event.target.checked })} />
              </label>
            </div>
            <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-[var(--border)] pt-4">
              {isActive ? <Badge tone="success">{t('activeProfile')}</Badge> : <Button disabled={action.isPending} onClick={() => action.mutate('activate')}><CheckCircle2 size={16} />{t('activate')}</Button>}
              <span className="text-xs text-[var(--muted)]">{currentProfile.last_compiler_target || t('compilerTarget')} · {currentProfile.last_result || t('noData')}</span>
              <div className="ml-auto flex gap-2">
                <Button onClick={() => api(session!, '/runtime/restart', { method: 'POST' }).then(() => setNotice(t('operationAccepted')))}><Power size={16} />{t('restartNow')}</Button>
                <Button size="icon" variant="ghost" title={t('remove')} disabled={isActive || profiles.length <= 1 || remove.isPending} onClick={() => remove.mutate(currentProfile.id)}><Trash2 size={16} /></Button>
              </div>
            </div>
          </Card>

          <div className="flex gap-1 overflow-x-auto border-b border-[var(--border)]">
            <button className={`h-10 border-b-2 px-3 text-sm font-medium ${section === 'editor' ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)]'}`} onClick={() => setSection('editor')}>{t('configuration')}</button>
            <button className={`h-10 border-b-2 px-3 text-sm font-medium ${section === 'diagnostics' ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)]'}`} onClick={() => setSection('diagnostics')}>{t('diagnostics')}</button>
          </div>

          <AcmeContentBoundary>
            <MessageBridge />
            <ProxyPreviewModal ref={previewRef} />
            <ProxyDebugModal ref={debugRef} />
            {section === 'editor' ? (
              <ProxySubscribeModal
                key={editorRevision(currentProfile)}
                profile={currentProfile}
                defaults={catalog.data?.editor_defaults ?? { rule_list: '{}', group: '[]', filter: '[]', custom_config: '[]', dns_config: '', private_access_config: '', servers: '[]' }}
                customNodes={customNodes.data?.nodes ?? []}
                saving={save.isPending}
                onSave={async (candidate) => { setDraft(candidate); await save.mutateAsync(candidate) }}
                onCancel={resetDraft}
              />
            ) : (
              <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
                <Card className="p-4">
                  <div className="flex flex-wrap items-end gap-3">
                    <Field label={t('compilerTarget')}>
                      <select className="h-9 min-w-56 rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm" value={format} onChange={(event) => setFormat(event.target.value as ProxyDebugFormat)}>
                        {(catalog.data?.targets ?? []).map((target) => <option key={target.format} value={target.format}>{target.format}</option>)}
                      </select>
                    </Field>
                    <Button onClick={() => previewRef.current?.open(currentProfile.id, currentProfile.remark || currentProfile.name)}><FileJson size={16} />{t('preview')}</Button>
                    <Button onClick={() => debugRef.current?.open(currentProfile.id, format)}><Activity size={16} />{t('diagnostics')}</Button>
                    <Button onClick={() => api(session!, '/subscriptions/cache/clear', { method: 'POST' }).then(() => setNotice(t('operationDone')))}><Trash2 size={16} />{t('clearCache')}</Button>
                  </div>
                  <div className="mt-5 grid grid-cols-2 gap-4 border-t border-[var(--border)] pt-4 text-sm">
                    <Info label={t('compilerTarget')} value={currentProfile.last_compiler_target || '-'} />
                    <Info label={t('lastResult')} value={currentProfile.last_result || '-'} />
                    <Info label="Runtime validation" value={String(currentProfile.last_runtime_validated)} />
                    <Info label="Config hash" value={currentProfile.last_config_hash || '-'} />
                  </div>
                  {currentProfile.last_compiler_warnings?.length ? <div className="mt-4 space-y-1 text-xs text-amber-700 dark:text-amber-400">{currentProfile.last_compiler_warnings.map((warning) => <p key={warning}>{warning}</p>)}</div> : null}
                </Card>
                <TargetOverrides profile={currentProfile} saving={save.isPending} onSave={(candidate) => save.mutate(candidate)} />
              </div>
            )}
          </AcmeContentBoundary>
        </>
      ) : <Card className="grid min-h-52 place-items-center"><Spinner /></Card>}
    </div>
  )
}

function Info({ label, value }: { label: string; value: string }) {
  return <div><p className="text-xs text-[var(--muted)]">{label}</p><p className="mt-1 break-words font-medium">{value}</p></div>
}

function editorRevision(profile: SubscriptionProfile) {
  return JSON.stringify({
    id: profile.id,
    remark: profile.remark,
    logLevel: profile.log_level,
    editor: profile.editor,
    sources: profile.sources,
    customNodeIDs: profile.custom_node_ids,
    useSystemGroups: profile.use_system_groups,
    useSystemRules: profile.use_system_rules,
    useSystemFilters: profile.use_system_filters,
    useSystemDNS: profile.use_system_dns,
    useSystemCustomConfig: profile.use_system_custom_config,
  })
}

function TargetOverrides({ profile, saving, onSave }: { profile: SubscriptionProfile; saving: boolean; onSave: (profile: SubscriptionProfile) => void }) {
  const { t } = useI18n()
  const [value, setValue] = useState(() => JSON.stringify(profile.custom_config ?? {}, null, 2))
  const [error, setError] = useState('')
  const save = () => {
    try {
      onSave({ ...profile, custom_config: parseJSONC<Record<string, unknown>>(value) })
      setError('')
    } catch (parseError) {
      setError(parseError instanceof Error ? parseError.message : String(parseError))
    }
  }
  return (
    <Card className="overflow-hidden">
      <div className="flex items-center border-b border-[var(--border)] px-4 py-3 text-sm font-semibold">
        {t('targetOverrides')}
        <Button className="ml-auto" disabled={saving} onClick={save}>{t('save')}</Button>
      </div>
      {error ? <p className="border-b border-red-500/30 bg-red-500/10 px-4 py-2 text-xs text-red-700 dark:text-red-300">{error}</p> : null}
      <CodeMirror value={value} height="360px" extensions={[json()]} theme="dark" onChange={setValue} />
    </Card>
  )
}

import { useMemo, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Activity, CheckCircle2, CircleAlert, FileJson, MoreHorizontal, Pencil, Plus, RefreshCw, RotateCw, Save as SaveIcon, Trash2 } from 'lucide-react'
import { Dropdown, Select, Tooltip } from '@acme/components'
import type { ProxyDebugFormat } from '@acme/types'
import { Button, Card, ConfirmDialog, Field, PageTitle, Spinner } from '../components/ui'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import { useRuntimeActionFeedback, type RuntimeActionNotice } from '../lib/useRuntimeActionFeedback'
import type { CustomNode, LinuxNetworkInventory, ManagedRuntimeStatus, SubscriptionCatalogResponse, SubscriptionProfile } from '../lib/types'
import { MessageBridge } from '../features/subscriptions/toolbox/MessageBridge'
import ProxyDebugModal, { type ProxyDebugModalRef } from '../features/subscriptions/toolbox/ProxyDebugModal'
import ProxyPreviewModal, { type ProxyPreviewModalRef } from '../features/subscriptions/toolbox/ProxyPreviewModal'
import ProxySubscribeEditor, { type ProxySubscribeEditorRef, type ProxySubscribeSaveState } from '../features/subscriptions/toolbox/ProxySubscribeEditor'
import { RemoteSubscriptionPanel } from '../features/subscriptions/RemoteSubscriptionPanel'
import { RestartChangeSummary, type RuntimePendingChange } from '../features/subscriptions/RestartChangeSummary'
import { SubscriptionProfileDialog, type SubscriptionMode } from '../features/subscriptions/SubscriptionProfileDialog'

type SaveResponse = { change: { Changed: boolean; NeedsRestart: boolean; Message: string }; profile?: SubscriptionProfile; render?: { warnings?: string[] } }
type NameDialogState = { mode: 'create' } | { mode: 'rename'; profile: SubscriptionProfile }
type Notice = RuntimeActionNotice
type Confirmation = 'refresh' | 'restart'
type RuntimeStatusWithChanges = ManagedRuntimeStatus & { pending_changes: RuntimePendingChange[] }

export function Subscriptions() {
  const { t } = useI18n()
  const { session } = useSession()
  const queryClient = useQueryClient()
  const [selectedID, setSelectedID] = useState('')
  const [drafts, setDrafts] = useState<Record<string, SubscriptionProfile>>({})
  const [nameDialog, setNameDialog] = useState<NameDialogState | null>(null)
  const [nameDialogOpen, setNameDialogOpen] = useState(false)
  const [nameValue, setNameValue] = useState('')
  const [nameError, setNameError] = useState('')
  const [createMode, setCreateMode] = useState<SubscriptionMode>('local')
  const [manifestURL, setManifestURL] = useState('')
  const [deleteProfile, setDeleteProfile] = useState<SubscriptionProfile | null>(null)
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null)
  const [notice, setNotice] = useState<Notice | null>(null)
  const [format, setFormat] = useState<ProxyDebugFormat>('sing-box-v13')
  const previewRef = useRef<ProxyPreviewModalRef>(null)
  const debugRef = useRef<ProxyDebugModalRef>(null)
  const editorRef = useRef<ProxySubscribeEditorRef>(null)
  const [editorSaveState, setEditorSaveState] = useState<ProxySubscribeSaveState>({ profileID: '', dirty: false, saving: false })

  const catalog = useQuery({ queryKey: ['subscriptions'], queryFn: () => api<SubscriptionCatalogResponse>(session!, '/subscriptions') })
  const customNodes = useQuery({ queryKey: ['custom-nodes'], queryFn: () => api<{ nodes: CustomNode[] }>(session!, '/custom-nodes') })
	const networkInventory = useQuery({ queryKey: ['system', 'network'], queryFn: () => api<LinuxNetworkInventory>(session!, '/system/network') })
  const runtimeStatus = useQuery({
    queryKey: ['runtime', 'status'],
    queryFn: () => api<RuntimeStatusWithChanges>(session!, '/runtime/status'),
    refetchInterval: (query) => query.state.data?.pending || ['starting', 'stopping', 'restarting'].includes(query.state.data?.runtime_state || '') ? 1000 : false,
  })
  const acceptRuntimeAction = useRuntimeActionFeedback(runtimeStatus.data, setNotice)
  const profiles = useMemo(() => catalog.data?.profiles ?? [], [catalog.data?.profiles])
  const effectiveSelectedID = selectedID || catalog.data?.active_profile_id || profiles[0]?.id || ''
  const storedProfile = profiles.find((item) => item.id === effectiveSelectedID) ?? null
  const currentProfile = drafts[effectiveSelectedID] ?? storedProfile
  const currentEditorSaveState = editorSaveState.profileID === currentProfile?.id
    ? editorSaveState
    : { profileID: currentProfile?.id ?? '', dirty: false, saving: false }
  const needsCoreRestart = Boolean(runtimeStatus.data?.pending)

  const invalidate = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['subscriptions'] }),
      queryClient.invalidateQueries({ queryKey: ['system'] }),
      queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] }),
    ])
  }

  const save = useMutation({
	mutationFn: ({ candidate, contextKey }: { candidate: SubscriptionProfile; contextKey: string }) => api<SaveResponse>(session!, `/subscriptions/${candidate.id}`, {
		method: 'PUT',
		headers: { 'X-Sempre-Configuration-Context': contextKey },
		body: JSON.stringify(candidate),
	}),
	onSuccess: (result, { candidate }) => {
	  const persisted = result.profile ?? candidate
	  queryClient.setQueryData<SubscriptionCatalogResponse>(['subscriptions'], (value) => value ? {
		...value,
		profiles: value.profiles.map((item) => item.id === persisted.id ? persisted : item),
	  } : value)
      setDrafts((current) => {
        const draft = current[candidate.id]
        if (!draft || editorRevision(draft) !== editorRevision(candidate)) return current
        const next = { ...current }
        delete next[candidate.id]
        return next
      })
	  void invalidate()
    },
  })

  const create = useMutation({
    mutationFn: ({ name, mode, manifestURL }: { name: string; mode: SubscriptionMode; manifestURL: string }) => api<SubscriptionProfile>(session!, '/subscriptions', {
      method: 'POST',
      body: JSON.stringify(mode === 'remote' ? { name, mode, manifest_url: manifestURL } : { name }),
    }),
    onSuccess: async (profile) => {
      await invalidate()
      setSelectedID(profile.id)
      setNameDialogOpen(false)
    },
    onError: (error) => setNameError(error.message),
  })

  const rename = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => api<SubscriptionProfile>(session!, `/subscriptions/${id}`, { method: 'PATCH', body: JSON.stringify({ name }) }),
    onSuccess: async (profile) => {
      queryClient.setQueryData<SubscriptionCatalogResponse>(['subscriptions'], (value) => value ? {
        ...value,
        profiles: value.profiles.map((item) => item.id === profile.id ? profile : item),
      } : value)
      setDrafts((current) => current[profile.id] ? { ...current, [profile.id]: { ...current[profile.id], name: profile.name } } : current)
      setNameDialogOpen(false)
      await invalidate()
    },
    onError: (error) => setNameError(error.message),
  })

  const remove = useMutation({
    mutationFn: (profile: SubscriptionProfile) => api(session!, `/subscriptions/${profile.id}`, { method: 'DELETE' }),
    onSuccess: async (_result, profile) => {
      setDeleteDialogOpen(false)
      setSelectedID(catalog.data?.active_profile_id || '')
      setDrafts((current) => {
        const next = { ...current }
        delete next[profile.id]
        return next
      })
      await invalidate()
    },
    onError: (error) => setNotice({ message: error.message, tone: 'error' }),
  })

  const action = useMutation({
    mutationFn: ({ id, operation }: { id: string; operation: 'activate' | 'refresh' }) => api<SaveResponse>(session!, `/subscriptions/${id}/${operation}`, { method: 'POST' }),
    onSuccess: async (result, variables) => {
      if (variables.operation === 'refresh') setConfirmation(null)
      setNotice({ message: result.change.Message, tone: 'success' })
      await invalidate()
    },
    onError: (error) => setNotice({ message: error.message, tone: 'error' }),
  })

  const restart = useMutation({
    mutationFn: () => api<{ action: string; status: ManagedRuntimeStatus }>(session!, '/runtime/restart', { method: 'POST' }),
    onSuccess: async (result) => {
      setConfirmation(null)
      queryClient.setQueryData(['runtime', 'status'], result.status)
      acceptRuntimeAction(result.status)
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['system'] }),
        queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] }),
      ])
    },
    onError: (error) => setNotice({ message: error.message, tone: 'error' }),
  })

  const schedule = useMutation({
    mutationFn: (body: Record<string, unknown>) => api(session!, '/subscription', { method: 'PATCH', body: JSON.stringify(body) }),
    onSuccess: invalidate,
  })

  const openNameDialog = (state: NameDialogState) => {
    setNameDialog(state)
    setNameValue(state.mode === 'rename' ? state.profile.name : '')
    setNameError('')
    setCreateMode('local')
    setManifestURL('')
    setNameDialogOpen(true)
  }
  const closeNameDialog = () => {
    if (create.isPending || rename.isPending) return
    setNameDialogOpen(false)
  }
  const finishNameDialogClose = (open: boolean) => {
    if (open) return
    setNameDialog(null)
    setNameValue('')
    setNameError('')
    setCreateMode('local')
    setManifestURL('')
  }
  const openDeleteDialog = (profile: SubscriptionProfile) => {
    setDeleteProfile(profile)
    setDeleteDialogOpen(true)
  }
  const finishDeleteDialogClose = (open: boolean) => {
    if (!open) setDeleteProfile(null)
  }
  const submitName = () => {
    if (!nameDialog) return
    const name = nameValue.trim()
    if (!name) {
      setNameError(t('subscriptionSetNameRequired'))
      return
    }
    const renamedID = nameDialog.mode === 'rename' ? nameDialog.profile.id : ''
    if (profiles.some((profile) => profile.id !== renamedID && profile.name.trim().toLowerCase() === name.toLowerCase())) {
      setNameError(t('subscriptionSetNameUsed'))
      return
    }
    setNameError('')
    if (nameDialog.mode === 'create') {
      if (createMode === 'remote') {
        try {
          const parsed = new URL(manifestURL.trim())
          if (!['http:', 'https:'].includes(parsed.protocol) || parsed.username || parsed.password) throw new Error()
        } catch {
          setNameError(t('remoteManifestInvalid'))
          return
        }
      }
      create.mutate({ name, mode: createMode, manifestURL: manifestURL.trim() })
    } else {
      rename.mutate({ id: nameDialog.profile.id, name })
    }
  }

  return (
    <div className="space-y-5">
      <PageTitle title={t('subscriptions')}>
        <div className="flex min-w-0 flex-wrap justify-end gap-2">
          <Button variant="primary" disabled={!currentProfile || currentProfile.mode === 'remote' || !currentEditorSaveState.dirty || currentEditorSaveState.saving} onClick={() => editorRef.current?.saveNow()}>
            {currentEditorSaveState.saving ? <Spinner /> : <SaveIcon size={16} />}{t('save')}
          </Button>
          <Button disabled={!currentProfile || action.isPending} onClick={() => setConfirmation('refresh')}>
            {action.isPending && action.variables?.operation === 'refresh' ? <Spinner /> : <RefreshCw size={16} />}{t('updateNow')}
          </Button>
          <Tooltip title={needsCoreRestart ? t('configurationRestartRequired') : undefined}>
            <Button disabled={restart.isPending} onClick={() => setConfirmation('restart')}>
              {restart.isPending ? <Spinner /> : needsCoreRestart ? <CircleAlert className="text-amber-500" size={16} /> : <RotateCw size={16} />}{t('restartNow')}
            </Button>
          </Tooltip>
        </div>
      </PageTitle>

      <div role="tablist" aria-label={t('subscriptionSets')} className="flex items-end gap-1 overflow-x-auto border-b border-[var(--border)]">
        {profiles.map((profile) => {
          const selected = effectiveSelectedID === profile.id
          return (
            <div key={profile.id} className={`flex h-10 shrink-0 items-center border-b-2 ${selected ? 'border-emerald-500 text-emerald-700 dark:text-emerald-400' : 'border-transparent text-[var(--muted)]'}`}>
              <button
                role="tab"
                aria-selected={selected}
                className="flex h-full min-w-0 items-center gap-2 pl-3 pr-2 text-sm font-medium"
                onClick={() => setSelectedID(profile.id)}
              >
                {profile.id === catalog.data?.active_profile_id ? <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-emerald-500" /> : null}
                <span className="max-w-48 truncate">{profile.name || t('defaultSubscriptionSet')}</span>
              </button>
              {selected ? (
                <SubscriptionSetMenu
                  profile={profile}
                  active={profile.id === catalog.data?.active_profile_id}
                  last={profiles.length <= 1}
                  pending={action.isPending || remove.isPending}
                  onRename={() => openNameDialog({ mode: 'rename', profile })}
                  onActivate={() => action.mutate({ id: profile.id, operation: 'activate' })}
                  onDelete={() => openDeleteDialog(profile)}
                />
              ) : null}
            </div>
          )
        })}
        <button
          type="button"
          className="grid size-10 shrink-0 place-items-center border-b-2 border-transparent text-[var(--muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500"
          title={t('newSubscriptionSet')}
          aria-label={t('newSubscriptionSet')}
          onClick={() => openNameDialog({ mode: 'create' })}
        >
          <Plus size={17} />
        </button>
      </div>

      {notice ? <div role={notice.tone === 'error' ? 'alert' : 'status'} className={`whitespace-pre-line border-l-2 px-3 py-2 text-sm break-words ${notice.tone === 'error' ? 'border-red-500 bg-red-500/8 text-red-700 dark:text-red-300' : 'border-emerald-500 bg-emerald-500/8 text-emerald-700 dark:text-emerald-300'}`}>{notice.message}</div> : null}

      {currentProfile ? (
        <>
			{currentProfile.mode === 'remote' ? (
				<RemoteSubscriptionPanel profile={currentProfile} />
			) : (
				<>
					<MessageBridge />
					<ProxyPreviewModal ref={previewRef} />
					<ProxyDebugModal ref={debugRef} />
					<ProxySubscribeEditor
				ref={editorRef}
			key={`${currentProfile.id}:${catalog.data?.configuration_context.key ?? 'common'}`}
            profile={currentProfile}
            defaults={catalog.data?.editor_defaults ?? { rule_list: '{}', group: '[]', filter: '[]', custom_config: '[]', dns_config: '', private_access_config: '', servers: '[]' }}
            customNodes={customNodes.data?.nodes ?? []}
			networkInventory={networkInventory.data}
			configurationContext={catalog.data?.configuration_context ?? {
				key: 'common', platform: 'unknown', capabilities: { features: [], enum_values: {}, protocols: [] },
			}}
            schedule={{ interval: catalog.data?.schedule.interval || '24h', autoRestart: Boolean(catalog.data?.auto_restart) }}
            onScheduleSave={async (change) => { await schedule.mutateAsync(change) }}
            onSave={async (candidate) => {
              setDrafts((current) => ({ ...current, [candidate.id]: candidate }))
				await save.mutateAsync({ candidate, contextKey: catalog.data?.configuration_context.key ?? 'common' })
            }}
				onSaveStateChange={setEditorSaveState}
            diagnostics={(
              <div className="space-y-5">
                <div className="flex flex-wrap items-end gap-3">
                  <Field label={t('compilerTarget')}>
                    <Select
                      className="h-9 min-w-56"
                      value={format}
                      options={(catalog.data?.targets ?? []).map((target) => ({ value: target.format, label: target.format }))}
                      onChange={(value) => setFormat(value as ProxyDebugFormat)}
                    />
                  </Field>
                  <Button type="button" onClick={() => previewRef.current?.open(currentProfile.id, currentProfile.remark || currentProfile.name)}><FileJson size={16} />{t('preview')}</Button>
                  <Button type="button" onClick={() => debugRef.current?.open(currentProfile.id, format)}><Activity size={16} />{t('diagnostics')}</Button>
                  <Button type="button" onClick={() => api(session!, '/subscriptions/cache/clear', { method: 'POST' }).then(() => setNotice({ message: t('operationDone'), tone: 'success' })).catch((error: Error) => setNotice({ message: error.message, tone: 'error' }))}><Trash2 size={16} />{t('clearCache')}</Button>
                </div>
                <div className="grid grid-cols-2 gap-4 border-t border-[var(--border)] pt-4 text-sm">
                  <Info label={t('compilerTarget')} value={currentProfile.last_compiler_target || '-'} />
                  <Info label={t('lastResult')} value={currentProfile.last_result || '-'} />
                  <Info label="Runtime validation" value={String(currentProfile.last_runtime_validated)} />
                  <Info label="Config hash" value={currentProfile.last_config_hash || '-'} />
                </div>
                {currentProfile.last_compiler_warnings?.length ? <div className="space-y-1 text-xs text-amber-700 dark:text-amber-400">{currentProfile.last_compiler_warnings.map((warning) => <p key={warning}>{warning}</p>)}</div> : null}
              </div>
            )}
					/>
				</>
			)}
        </>
      ) : <Card className="grid min-h-52 place-items-center"><Spinner /></Card>}

      {nameDialog ? (
        <SubscriptionProfileDialog
          open={nameDialogOpen}
          creating={nameDialog.mode === 'create'}
          name={nameValue}
          mode={createMode}
          manifestURL={manifestURL}
          error={nameError}
          pending={create.isPending || rename.isPending}
          onNameChange={(value) => { setNameValue(value); setNameError('') }}
			onModeChange={(value) => { setCreateMode(value); setNameError('') }}
			onManifestURLChange={(value) => { setManifestURL(value); setNameError('') }}
          onCancel={closeNameDialog}
          onSubmit={submitName}
          afterOpenChange={finishNameDialogClose}
        />
      ) : null}
      {deleteProfile ? (
        <ConfirmDialog
          open={deleteDialogOpen}
          title={t('deleteSubscriptionSet')}
          detail={`${t('deleteSubscriptionSetDetail')} ${deleteProfile.name || t('defaultSubscriptionSet')}`}
          confirmLabel={t('deleteSubscriptionSet')}
          cancelLabel={t('cancel')}
          pending={remove.isPending}
          onCancel={() => { if (!remove.isPending) setDeleteDialogOpen(false) }}
          onConfirm={() => remove.mutate(deleteProfile)}
          afterOpenChange={finishDeleteDialogClose}
        />
      ) : null}
      <ConfirmDialog
        open={confirmation !== null}
        title={t(confirmation === 'refresh' ? 'subscriptionUpdateConfirmTitle' : 'coreRestartConfirmTitle')}
        detail={confirmation === 'refresh'
          ? t(currentProfile?.mode === 'remote' ? 'remoteSubscriptionUpdateConfirmDetail' : 'subscriptionUpdateConfirmDetail').replace('{profile}', currentProfile?.name || t('defaultSubscriptionSet'))
          : <RestartChangeSummary detail={t('coreRestartConfirmDetail')} changes={runtimeStatus.data ? runtimeStatus.data.pending_changes : []} />}
        confirmLabel={t(confirmation === 'refresh' ? 'updateNow' : 'restartNow')}
        cancelLabel={t('cancel')}
        pending={confirmation === 'refresh' ? action.isPending : restart.isPending}
        onCancel={() => {
          if (!action.isPending && !restart.isPending) setConfirmation(null)
        }}
        onConfirm={() => {
          if (confirmation === 'refresh' && currentProfile) {
            action.mutate({ id: currentProfile.id, operation: 'refresh' })
          } else if (confirmation === 'restart') {
            restart.mutate()
          }
        }}
      />
    </div>
  )
}

function SubscriptionSetMenu({
  profile,
  active,
  last,
  pending,
  onRename,
  onActivate,
  onDelete,
}: {
  profile: SubscriptionProfile
  active: boolean
  last: boolean
  pending: boolean
  onRename: () => void
  onActivate: () => void
  onDelete: () => void
}) {
  const { t } = useI18n()
  const deleteReason = active ? t('activeSubscriptionSetDeleteReason') : last ? t('lastSubscriptionSetDeleteReason') : ''
  return (
    <div className="mr-1 shrink-0">
      <Dropdown
        trigger={['click']}
        placement="bottomRight"
        menu={{
          items: [
            { key: 'rename', icon: <Pencil size={15} />, label: t('renameSubscriptionSet'), disabled: pending, onClick: onRename },
            {
              key: 'activate',
              icon: <CheckCircle2 size={15} />,
              label: <SubscriptionSetMenuLabel label={t('activateSubscriptionSet')} reason={active ? t('alreadyActiveSubscriptionSet') : ''} />,
              disabled: active || pending,
              onClick: onActivate,
            },
            {
              key: 'delete',
              icon: <Trash2 size={15} />,
              label: <SubscriptionSetMenuLabel label={t('deleteSubscriptionSet')} reason={deleteReason} />,
              disabled: Boolean(deleteReason) || pending,
              danger: true,
              onClick: onDelete,
            },
          ],
        }}
      >
        <button
          type="button"
          className="grid size-7 place-items-center rounded text-current hover:bg-emerald-500/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500"
          title={t('manageSubscriptionSet')}
          aria-label={`${t('manageSubscriptionSet')}: ${profile.name || t('defaultSubscriptionSet')}`}
          aria-haspopup="menu"
        >
          <MoreHorizontal size={16} />
        </button>
      </Dropdown>
    </div>
  )
}

function SubscriptionSetMenuLabel({ label, reason = '' }: { label: string; reason?: string }) {
  return (
    <span className="block min-w-48">
      <span className="block">{label}</span>
      {reason ? <span className="mt-0.5 block text-xs text-[var(--muted)]">{reason}</span> : null}
    </span>
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
		coreOverrides: profile.core_overrides,
		transparentProxy: profile.transparent_proxy,
		managementAPI: profile.management_api,
		dns: profile.dns,
		privateAccess: profile.private_access,
    sources: profile.sources,
    customNodeIDs: profile.custom_node_ids,
    useSystemGroups: profile.use_system_groups,
    useSystemRules: profile.use_system_rules,
    useSystemFilters: profile.use_system_filters,
    useSystemDNS: profile.use_system_dns,
    useSystemCustomConfig: profile.use_system_custom_config,
  })
}

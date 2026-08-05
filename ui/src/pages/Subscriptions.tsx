import { useMemo, useRef, useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Activity, CheckCircle2, FileJson, MoreHorizontal, Pencil, Plus, RefreshCw, RotateCw, Trash2 } from 'lucide-react'
import { Dropdown, Modal, Select } from '@acme/components'
import type { ProxyDebugFormat } from '@acme/types'
import { Button, Card, ConfirmDialog, Field, Input, PageTitle, Spinner } from '../components/ui'
import { api } from '../lib/api'
import { useI18n } from '../lib/i18n'
import { useSession } from '../lib/session'
import type { CustomNode, SubscriptionCatalogResponse, SubscriptionProfile } from '../lib/types'
import { MessageBridge } from '../features/subscriptions/toolbox/MessageBridge'
import ProxyDebugModal, { type ProxyDebugModalRef } from '../features/subscriptions/toolbox/ProxyDebugModal'
import ProxyPreviewModal, { type ProxyPreviewModalRef } from '../features/subscriptions/toolbox/ProxyPreviewModal'
import ProxySubscribeEditor from '../features/subscriptions/toolbox/ProxySubscribeEditor'

type SaveResponse = { change: { changed: boolean; message: string }; render?: { warnings?: string[] } }
type NameDialogState = { mode: 'create' } | { mode: 'rename'; profile: SubscriptionProfile }
type Notice = { message: string; tone: 'success' | 'error' }

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
  const [deleteProfile, setDeleteProfile] = useState<SubscriptionProfile | null>(null)
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
  const [notice, setNotice] = useState<Notice | null>(null)
  const [format, setFormat] = useState<ProxyDebugFormat>('sing-box-v13')
  const previewRef = useRef<ProxyPreviewModalRef>(null)
  const debugRef = useRef<ProxyDebugModalRef>(null)

  const catalog = useQuery({ queryKey: ['subscriptions'], queryFn: () => api<SubscriptionCatalogResponse>(session!, '/subscriptions') })
  const customNodes = useQuery({ queryKey: ['custom-nodes'], queryFn: () => api<{ nodes: CustomNode[] }>(session!, '/custom-nodes') })
  const profiles = useMemo(() => catalog.data?.profiles ?? [], [catalog.data?.profiles])
  const effectiveSelectedID = selectedID || catalog.data?.active_profile_id || profiles[0]?.id || ''
  const storedProfile = profiles.find((item) => item.id === effectiveSelectedID) ?? null
  const currentProfile = drafts[effectiveSelectedID] ?? storedProfile

  const invalidate = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['subscriptions'] }),
      queryClient.invalidateQueries({ queryKey: ['system'] }),
      queryClient.invalidateQueries({ queryKey: ['runtime', 'status'] }),
    ])
  }

  const save = useMutation({
    mutationFn: (candidate: SubscriptionProfile) => api<SaveResponse>(session!, `/subscriptions/${candidate.id}`, { method: 'PUT', body: JSON.stringify(candidate) }),
    onSuccess: async (_result, candidate) => {
      await invalidate()
      setDrafts((current) => {
        const draft = current[candidate.id]
        if (!draft || editorRevision(draft) !== editorRevision(candidate)) return current
        const next = { ...current }
        delete next[candidate.id]
        return next
      })
    },
  })

  const create = useMutation({
    mutationFn: (name: string) => api<SubscriptionProfile>(session!, '/subscriptions', { method: 'POST', body: JSON.stringify({ name }) }),
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
    onSuccess: async (result) => {
      setNotice({ message: result.change.message, tone: 'success' })
      await invalidate()
    },
    onError: (error) => setNotice({ message: error.message, tone: 'error' }),
  })

  const restart = useMutation({
    mutationFn: () => api(session!, '/runtime/restart', { method: 'POST' }),
    onSuccess: async () => {
      setNotice({ message: t('operationAccepted'), tone: 'success' })
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
      create.mutate(name)
    } else {
      rename.mutate({ id: nameDialog.profile.id, name })
    }
  }

  return (
    <div className="space-y-5">
      <PageTitle title={t('subscriptions')}>
        <div className="flex min-w-0 flex-wrap justify-end gap-2">
          <Button disabled={!currentProfile || action.isPending} onClick={() => currentProfile && action.mutate({ id: currentProfile.id, operation: 'refresh' })}>
            {action.isPending && action.variables?.operation === 'refresh' ? <Spinner /> : <RefreshCw size={16} />}{t('updateNow')}
          </Button>
          <Button disabled={restart.isPending} onClick={() => restart.mutate()}>
            {restart.isPending ? <Spinner /> : <RotateCw size={16} />}{t('restartNow')}
          </Button>
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

      {notice ? <div role={notice.tone === 'error' ? 'alert' : 'status'} className={`border-l-2 px-3 py-2 text-sm break-words ${notice.tone === 'error' ? 'border-red-500 bg-red-500/8 text-red-700 dark:text-red-300' : 'border-emerald-500 bg-emerald-500/8 text-emerald-700 dark:text-emerald-300'}`}>{notice.message}</div> : null}

      {currentProfile ? (
        <>
          <MessageBridge />
          <ProxyPreviewModal ref={previewRef} />
          <ProxyDebugModal ref={debugRef} />
          <ProxySubscribeEditor
            key={currentProfile.id}
            profile={currentProfile}
            defaults={catalog.data?.editor_defaults ?? { rule_list: '{}', group: '[]', filter: '[]', custom_config: '[]', dns_config: '', private_access_config: '', servers: '[]' }}
            customNodes={customNodes.data?.nodes ?? []}
            schedule={{ interval: catalog.data?.schedule.interval || '24h', autoRestart: Boolean(catalog.data?.auto_restart) }}
            onScheduleSave={async (change) => { await schedule.mutateAsync(change) }}
            onSave={async (candidate) => {
              setDrafts((current) => ({ ...current, [candidate.id]: candidate }))
              await save.mutateAsync(candidate)
            }}
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
      ) : <Card className="grid min-h-52 place-items-center"><Spinner /></Card>}

      {nameDialog ? (
        <SubscriptionSetNameDialog
          open={nameDialogOpen}
          state={nameDialog}
          value={nameValue}
          error={nameError}
          pending={create.isPending || rename.isPending}
          onChange={(value) => { setNameValue(value); setNameError('') }}
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

function SubscriptionSetNameDialog({ open, state, value, error, pending, onChange, onCancel, onSubmit, afterOpenChange }: { open: boolean; state: NameDialogState; value: string; error: string; pending: boolean; onChange: (value: string) => void; onCancel: () => void; onSubmit: () => void; afterOpenChange: (open: boolean) => void }) {
  const { t } = useI18n()
  const creating = state.mode === 'create'
  const title = creating ? t('newSubscriptionSet') : t('renameSubscriptionSet')
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!pending) onSubmit()
  }
  return (
    <Modal
      open={open}
      title={title}
      okText={creating ? t('createSubscriptionSet') : t('renameSubscriptionSet')}
      cancelText={t('cancel')}
      onOk={() => {
        onSubmit()
        return undefined
      }}
      onCancel={onCancel}
      afterOpenChange={afterOpenChange}
      okButtonProps={{ disabled: !value.trim() }}
      cancelButtonProps={{ disabled: pending }}
      confirmLoading={pending}
      maskClosable={!pending}
      keyboard={!pending}
      closable={!pending}
      destroyOnClose
      centered
    >
      <form onSubmit={submit}>
        <div>
          <Field label={t('subscriptionSetName')}>
            <Input autoFocus aria-invalid={Boolean(error)} aria-describedby={error ? 'subscription-set-name-error' : undefined} value={value} onChange={(event) => onChange(event.target.value)} />
          </Field>
          {error ? <p id="subscription-set-name-error" className="mt-2 text-sm text-red-600 dark:text-red-400">{error}</p> : null}
        </div>
      </form>
    </Modal>
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
    advancedConfig: profile.custom_config,
    sources: profile.sources,
    customNodeIDs: profile.custom_node_ids,
    useSystemGroups: profile.use_system_groups,
    useSystemRules: profile.use_system_rules,
    useSystemFilters: profile.use_system_filters,
    useSystemDNS: profile.use_system_dns,
    useSystemCustomConfig: profile.use_system_custom_config,
  })
}

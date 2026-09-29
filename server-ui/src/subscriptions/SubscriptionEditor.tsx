import { Button, Modal, Popconfirm, Spin, Tabs, TextArea, useToast } from '@acme/components'
import { useEffect, useState } from 'react'
import { parse, type ParseError } from 'jsonc-parser'
import { ServerApiError } from '../server-api'
import { subscriptionApi } from './api'
import type { Target } from './diagnostic-types'
import { BasicConfig, ExtraConfig, InheritedConfig } from './ConfigSections'
import { SourceItemsEditor } from './SourceItemsEditor'
import { SourceDebug } from './SourceDebug'
import { SubscriptionDebug } from './SubscriptionDebug'
import { PrivateAccessEditor } from './PrivateAccessEditor'
import { draftFromSubscription, emptyDraft, type Subscription, type SubscriptionDefaults, type SubscriptionDraft, type SubscriptionSource, type UserBrief } from './types'
import { useI18n } from '../i18n/provider'
import type { MessageKey } from '../i18n/zh-CN'

interface Props {
  open: boolean
  id: string | null
  onClose: () => void
  onSaved: (value: Subscription) => void
  targets: Target[]
}

const tabs: { key: string; labelKey: MessageKey }[] = [
  { key: 'basic', labelKey: 'editor.tabBasic' },
  { key: 'sources', labelKey: 'editor.tabSources' },
  { key: 'ruleList', labelKey: 'editor.tabRules' },
  { key: 'group', labelKey: 'editor.tabGroup' },
  { key: 'customConfig', labelKey: 'editor.tabCustom' },
  { key: 'dnsConfig', labelKey: 'editor.tabDns' },
  { key: 'privateAccess', labelKey: 'editor.tabPrivate' },
  { key: 'servers', labelKey: 'editor.tabServers' },
]

export function SubscriptionEditor({ open, id, onClose, onSaved, targets }: Props) {
  const { t, number } = useI18n()
  const toast = useToast()
  const [draft, setDraft] = useState<SubscriptionDraft | null>(null)
  const [saved, setSaved] = useState<Subscription | null>(null)
  const [defaults, setDefaults] = useState<SubscriptionDefaults | null>(null)
  const [users, setUsers] = useState<UserBrief[] | null>(null)
  const [tab, setTab] = useState('basic')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [defaultsError, setDefaultsError] = useState('')
  const [usersError, setUsersError] = useState('')
  const [conflict, setConflict] = useState(false)
  const [debugOpen, setDebugOpen] = useState(false)
  const [sourceDebug, setSourceDebug] = useState<{ source: SubscriptionSource; index: number } | null>(null)
  const [dnsInvalid, setDnsInvalid] = useState(false)

  useEffect(() => {
    if (!open) return
    let active = true
    void (id ? subscriptionApi.get(id) : Promise.resolve(null))
      .then((subscription) => {
        if (!active) return
        setSaved(subscription)
        setDraft(subscription ? draftFromSubscription(subscription) : emptyDraft())
      })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    void subscriptionApi.defaults().then((next) => { if (active) setDefaults(next) })
      .catch((reason) => { if (active) setDefaultsError(reason instanceof Error ? reason.message : String(reason)) })
    void subscriptionApi.users().then((next) => { if (active) setUsers(next) })
      .catch((reason) => { if (active) setUsersError(reason instanceof Error ? reason.message : String(reason)) })
    return () => { active = false }
  }, [open, id])

  const retryDefaults = async () => {
    setDefaultsError('')
    try { setDefaults(await subscriptionApi.defaults()) }
    catch (reason) { setDefaultsError(reason instanceof Error ? reason.message : String(reason)) }
  }
  const retryUsers = async () => {
    setUsersError('')
    try { setUsers(await subscriptionApi.users()) }
    catch (reason) { setUsersError(reason instanceof Error ? reason.message : String(reason)) }
  }

  const update = (patch: Partial<SubscriptionDraft>) => {
    setDraft((current) => current ? { ...current, ...patch } : current)
    setError('')
  }
  const save = (): undefined => {
    if (!draft) return undefined
    const dnsChanged = !saved || draft.dnsConfig !== saved.dnsConfig || draft.useSystemDnsConfig !== saved.useSystemDnsConfig
    if (dnsInvalid && !draft.useSystemDnsConfig && dnsChanged) { setError(t('editor.invalidDns')); setTab('dnsConfig'); return undefined }
    const invalidField = invalidJsoncField(draft, saved)
    if (invalidField) { setError(t('editor.invalidJsonc', { field: t(invalidField.labelKey) })); setTab(invalidField.tab); return undefined }
    const newlyAuthorized = saved?.canManageAuthorization
      ? draft.authorizedUserIds.filter((id) => !saved.authorizedUserIds.includes(id))
      : []
    if (saved && newlyAuthorized.length > 0 && saved.assignedCustomNodes.length > 0) {
      Modal.confirm({
        title: t('editor.shareTitle'),
        content: t('editor.shareDetail', { users: number(newlyAuthorized.length), nodes: number(saved.assignedCustomNodes.length) }),
        okText: t('editor.confirmSave'), cancelText: t('common.cancel'),
        onOk: () => commitSave(true),
      })
      return undefined
    }
    void commitSave(false)
    return undefined
  }

  const commitSave = async (confirmShareAssignedNodes: boolean) => {
    if (!draft) return
    const cleaned: SubscriptionDraft = {
      ...draft,
      subscribeItems: draft.subscribeItems?.filter((source) => source.url.trim()) ?? null,
    }
    setSaving(true)
    setError('')
    setConflict(false)
    try {
      const result = id
        ? await subscriptionApi.update(id, { ...(saved?.canManageAuthorization ? cleaned : omitAuthorization(cleaned)), ...(confirmShareAssignedNodes ? { confirmShareAssignedNodes: true } : {}) }, saved?.updatedAt ?? '')
        : await subscriptionApi.create(cleaned)
      onSaved(result)
    } catch (reason) {
      if (reason instanceof ServerApiError && reason.status === 409) {
        setConflict(true)
        setError(t('editor.conflict'))
      } else {
        toast.error(reason instanceof Error ? reason.message : String(reason))
      }
    } finally {
      setSaving(false)
    }
  }

  const reload = async () => {
    if (!id) return
    setLoading(true)
    setError('')
    try {
      const current = await subscriptionApi.get(id)
      setSaved(current)
      setDraft(draftFromSubscription(current))
      setConflict(false)
      setDnsInvalid(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }

  return (
    <Modal open={open} title={id ? t('configs.edit') : t('configs.new')} size="almost-full" bodyStyle={{ display: 'flex', flexDirection: 'column' }} onCancel={() => { if (!saving) onClose() }} closable={!saving} keyboard={!saving} maskClosable={false} destroyOnClose footer={<div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-2 pt-4">
      <Button className="h-auto! min-h-8 min-w-0 max-w-full justify-self-start whitespace-normal text-left" disabled={!draft || targets.length === 0 || saving} onClick={() => setDebugOpen(true)}><span className="min-w-0 break-words">{t('editor.debugDraft')}</span></Button>
      <div className="flex shrink-0 gap-2">
        <Button disabled={saving} onClick={() => { if (!saving) onClose() }}>{t('common.cancel')}</Button>
        <Button variant="primary" loading={saving} disabled={loading || !draft || (saved !== null && !saved.canEdit)} onClick={save}>{t('common.save')}</Button>
      </div>
    </div>}>
      {loading ? <div className="grid min-h-48 place-items-center"><Spin size="large" /></div> : null}
      {error ? <p role="alert" className="mb-3 text-sm text-red-600">{error}</p> : null}
      {id && !draft && !loading ? <Button size="small" onClick={() => void reload()}>{t('common.retry')}</Button> : null}
      {defaultsError ? <p role="alert" className="mb-3 text-sm text-red-600">{t('editor.inherit')}: {defaultsError} <Button size="small" onClick={() => void retryDefaults()}>{t('common.retry')}</Button></p> : null}
      {usersError ? <p role="alert" className="mb-3 text-sm text-red-600">{t('editor.authorizedUsers')}: {usersError} <Button size="small" onClick={() => void retryUsers()}>{t('common.retry')}</Button></p> : null}
      {conflict ? <Popconfirm title={t('editor.reloadTitle')} description={t('editor.reloadWarning')} okText={t('common.confirm')} cancelText={t('common.cancel')} onConfirm={reload}><Button className="mb-3" size="small">{t('editor.reloadLatest')}</Button></Popconfirm> : null}
      {draft ? (
        <div className="flex min-h-[24rem] flex-1 flex-col gap-5">
          <Tabs items={tabs.map((item) => ({ key: item.key, label: t(item.labelKey) }))} activeKey={tab} onChange={setTab} type="segment" />
          <div className={['ruleList', 'group', 'customConfig', 'dnsConfig'].includes(tab) ? 'flex min-h-[20rem] flex-1 flex-col' : ''}>
          {tab === 'basic' ? <BasicConfig draft={draft} users={users} canManageAuthorization={!id || Boolean(saved?.canManageAuthorization)} update={update} /> : null}
          {tab === 'sources' ? (
            <div className="space-y-4">
              <SourceItemsEditor value={draft.subscribeItems ?? []} onChange={(source) => update({ subscribeItems: source })} onDebug={(source, index) => setSourceDebug({ source, index })} />
              <InheritedConfig field="filter" draft={draft} defaults={defaults} update={update} />
              {draft.subscribeUrl !== null ? <label className="block space-y-1 text-sm">{t('editor.oldSource')}
                <TextArea rows={4} value={draft.subscribeUrl} onChange={(event) => update({ subscribeUrl: event.target.value || null })} className="font-mono text-xs" />
              </label> : null}
            </div>
          ) : null}
          {tab === 'ruleList' ? <InheritedConfig field="ruleList" draft={draft} defaults={defaults} update={update} /> : null}
          {tab === 'group' ? <InheritedConfig field="group" draft={draft} defaults={defaults} update={update} /> : null}
          {tab === 'customConfig' ? <InheritedConfig field="customConfig" draft={draft} defaults={defaults} update={update} /> : null}
          {tab === 'dnsConfig' ? <InheritedConfig field="dnsConfig" draft={draft} defaults={defaults} update={update} onDnsInvalidChange={setDnsInvalid} /> : null}
          {tab === 'privateAccess' ? <PrivateAccessEditor value={draft.privateAccessConfig} onChange={(next) => update({ privateAccessConfig: next })} /> : null}
          {tab === 'servers' ? <ExtraConfig draft={draft} assignedNodes={saved?.assignedCustomNodes ?? []} update={update} /> : null}
          </div>
        </div>
      ) : null}
      {draft && debugOpen ? <SubscriptionDebug draft={draft} targets={targets} subscriptionId={saved?.id} onClose={() => setDebugOpen(false)} /> : null}
      {sourceDebug ? <SourceDebug source={sourceDebug.source} saved={savedSource(saved, sourceDebug.source, sourceDebug.index)} onClose={() => setSourceDebug(null)} /> : null}
    </Modal>
  )
}

function omitAuthorization(draft: SubscriptionDraft): Partial<SubscriptionDraft> {
  const rest: Partial<SubscriptionDraft> = { ...draft }
  delete rest.authorizedUserIds
  return rest
}

function savedSource(saved: Subscription | null, source: SubscriptionSource, draftIndex: number) {
  if (!saved?.subscribeItems) return undefined
  const sameSource = (item: SubscriptionSource) =>
    item.enabled === source.enabled && item.url === source.url && item.prefix === source.prefix &&
    item.cacheTtlMinutes === source.cacheTtlMinutes && item.fetchUa === source.fetchUa && item.fetchMode === source.fetchMode
  const index = saved.subscribeItems[draftIndex] && sameSource(saved.subscribeItems[draftIndex])
    ? draftIndex : saved.subscribeItems.findIndex(sameSource)
  return index < 0 ? undefined : { id: saved.id, index, source: saved.subscribeItems[index] }
}

function invalidJsoncField(draft: SubscriptionDraft, saved: Subscription | null): { labelKey: MessageKey; tab: string } | null {
  const fields = [
    { value: draft.filter, enabled: !draft.useSystemFilter, changed: !saved || draft.filter !== saved.filter || draft.useSystemFilter !== saved.useSystemFilter, labelKey: 'editor.filter', tab: 'sources' },
    { value: draft.ruleList, enabled: !draft.useSystemRuleList, changed: !saved || draft.ruleList !== saved.ruleList || draft.useSystemRuleList !== saved.useSystemRuleList, labelKey: 'editor.tabRules', tab: 'ruleList' },
    { value: draft.group, enabled: !draft.useSystemGroup, changed: !saved || draft.group !== saved.group || draft.useSystemGroup !== saved.useSystemGroup, labelKey: 'editor.tabGroup', tab: 'group' },
    { value: draft.customConfig, enabled: !draft.useSystemCustomConfig, changed: !saved || draft.customConfig !== saved.customConfig || draft.useSystemCustomConfig !== saved.useSystemCustomConfig, labelKey: 'editor.tabCustom', tab: 'customConfig' },
    { value: draft.dnsConfig, enabled: !draft.useSystemDnsConfig, changed: !saved || draft.dnsConfig !== saved.dnsConfig || draft.useSystemDnsConfig !== saved.useSystemDnsConfig, labelKey: 'editor.tabDns', tab: 'dnsConfig' },
    { value: draft.privateAccessConfig, enabled: true, changed: !saved || draft.privateAccessConfig !== saved.privateAccessConfig, labelKey: 'editor.tabPrivate', tab: 'privateAccess' },
    { value: draft.servers, enabled: true, changed: !saved || draft.servers !== saved.servers, labelKey: 'editor.tabServers', tab: 'servers' },
  ]
  for (const field of fields) {
    if (!field.enabled || !field.changed || !field.value?.trim()) continue
    const errors: ParseError[] = []
    parse(field.value, errors, { allowTrailingComma: true })
    if (errors.length) return { labelKey: field.labelKey as MessageKey, tab: field.tab }
  }
  return null
}

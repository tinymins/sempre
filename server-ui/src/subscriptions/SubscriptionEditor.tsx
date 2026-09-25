import { Button, Modal, Popconfirm, Spin, Tabs, TextArea } from '@acme/components'
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
  const [draft, setDraft] = useState<SubscriptionDraft | null>(null)
  const [saved, setSaved] = useState<Subscription | null>(null)
  const [defaults, setDefaults] = useState<SubscriptionDefaults | null>(null)
  const [users, setUsers] = useState<UserBrief[]>([])
  const [tab, setTab] = useState('basic')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [conflict, setConflict] = useState(false)
  const [debugOpen, setDebugOpen] = useState(false)
  const [sourceDebug, setSourceDebug] = useState<{ source: SubscriptionSource; index: number } | null>(null)
  const [dnsInvalid, setDnsInvalid] = useState(false)

  useEffect(() => {
    if (!open) return
    let active = true
    void Promise.all([id ? subscriptionApi.get(id) : Promise.resolve(null), subscriptionApi.defaults(), subscriptionApi.users()])
      .then(([subscription, nextDefaults, nextUsers]) => {
        if (!active) return
        setSaved(subscription)
        setDraft(subscription ? draftFromSubscription(subscription) : emptyDraft())
        setDefaults(nextDefaults)
        setUsers(nextUsers)
      })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [open, id])

  const update = (patch: Partial<SubscriptionDraft>) => {
    setDraft((current) => current ? { ...current, ...patch } : current)
    setError('')
  }
  const save = (): undefined => {
    if (!draft) return undefined
    if (dnsInvalid) { setError(t('editor.invalidDns')); setTab('dnsConfig'); return undefined }
    const invalidField = invalidJsoncField(draft)
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
        setError(reason instanceof Error ? reason.message : String(reason))
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
    <Modal open={open} title={id ? t('configs.edit') : t('configs.new')} extra={<Button size="small" disabled={!draft || targets.length === 0 || saving} onClick={() => setDebugOpen(true)}>{t('editor.debugDraft')}</Button>} size="almost-full" okText={t('common.save')} cancelText={t('common.cancel')} onOk={save} onCancel={() => { if (!saving) onClose() }} confirmLoading={saving} okButtonProps={{ disabled: loading || !draft || (saved !== null && !saved.canEdit) }} cancelButtonProps={{ disabled: saving }} closable={!saving} keyboard={!saving} maskClosable={false} destroyOnClose>
      {loading ? <div className="grid min-h-48 place-items-center"><Spin size="large" /></div> : null}
      {error ? <p role="alert" className="mb-3 text-sm text-red-600">{error}</p> : null}
      {conflict ? <Popconfirm title={t('editor.reloadTitle')} description={t('editor.reloadWarning')} okText={t('common.confirm')} cancelText={t('common.cancel')} onConfirm={reload}><Button className="mb-3" size="small">{t('editor.reloadLatest')}</Button></Popconfirm> : null}
      {draft ? (
        <div className="space-y-5">
          <Tabs items={tabs.map((item) => ({ key: item.key, label: t(item.labelKey) }))} activeKey={tab} onChange={(next) => { if (!dnsInvalid) setTab(next) }} type="segment" />
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
          {tab === 'dnsConfig' ? <InheritedConfig field="dnsConfig" draft={draft} defaults={defaults} update={update} dnsInvalid={dnsInvalid} onDnsInvalidChange={setDnsInvalid} /> : null}
          {tab === 'privateAccess' ? <PrivateAccessEditor value={draft.privateAccessConfig} onChange={(next) => update({ privateAccessConfig: next })} /> : null}
          {tab === 'servers' ? <ExtraConfig draft={draft} assignedNodes={saved?.assignedCustomNodes ?? []} update={update} /> : null}
        </div>
      ) : null}
      {draft && debugOpen ? <SubscriptionDebug draft={draft} targets={targets} subscriptionId={saved?.id} onClose={() => setDebugOpen(false)} /> : null}
      {sourceDebug ? <SourceDebug source={sourceDebug.source} saved={savedSource(saved, sourceDebug.source)} onClose={() => setSourceDebug(null)} /> : null}
    </Modal>
  )
}

function omitAuthorization(draft: SubscriptionDraft): Partial<SubscriptionDraft> {
  const rest: Partial<SubscriptionDraft> = { ...draft }
  delete rest.authorizedUserIds
  return rest
}

function savedSource(saved: Subscription | null, source: SubscriptionSource) {
  if (!saved?.subscribeItems) return undefined
  const index = saved.subscribeItems.findIndex((item) =>
    item.enabled === source.enabled && item.url === source.url && item.prefix === source.prefix &&
    item.remark === source.remark && item.cacheTtlMinutes === source.cacheTtlMinutes &&
    item.fetchUa === source.fetchUa && item.fetchMode === source.fetchMode)
  return index < 0 ? undefined : { id: saved.id, index, source: saved.subscribeItems[index] }
}

function invalidJsoncField(draft: SubscriptionDraft): { labelKey: MessageKey; tab: string } | null {
  const fields = [
    { value: draft.filter, enabled: !draft.useSystemFilter, labelKey: 'editor.filter', tab: 'sources' },
    { value: draft.ruleList, enabled: !draft.useSystemRuleList, labelKey: 'editor.tabRules', tab: 'ruleList' },
    { value: draft.group, enabled: !draft.useSystemGroup, labelKey: 'editor.tabGroup', tab: 'group' },
    { value: draft.customConfig, enabled: !draft.useSystemCustomConfig, labelKey: 'editor.tabCustom', tab: 'customConfig' },
    { value: draft.dnsConfig, enabled: !draft.useSystemDnsConfig, labelKey: 'editor.tabDns', tab: 'dnsConfig' },
    { value: draft.privateAccessConfig, enabled: true, labelKey: 'editor.tabPrivate', tab: 'privateAccess' },
    { value: draft.servers, enabled: true, labelKey: 'editor.tabServers', tab: 'servers' },
  ]
  for (const field of fields) {
    if (!field.enabled || !field.value?.trim()) continue
    const errors: ParseError[] = []
    parse(field.value, errors, { allowTrailingComma: true })
    if (errors.length) return { labelKey: field.labelKey as MessageKey, tab: field.tab }
  }
  return null
}

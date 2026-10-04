import { EditorProvider, SubscriptionConfigEditor, sourceText, normalizeSource } from '@acme/subscription-editor'
import { Button, Modal, Popconfirm, Select, Spin, TextArea, useToast } from '@acme/components'
import { useEffect, useState } from 'react'
import { parse, type ParseError } from 'jsonc-parser'
import { ServerApiError } from '../server-api'
import { subscriptionApi } from './api'
import type { Target } from './diagnostic-types'
import { AuthorizationFields } from './ConfigSections'
import { SourceDebug } from './SourceDebug'
import { SubscriptionDebug } from './SubscriptionDebug'
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

export function SubscriptionEditor({ open, id, onClose, onSaved, targets }: Props) {
  const { t, number, locale } = useI18n()
  const toast = useToast()
  const [draft, setDraft] = useState<SubscriptionDraft | null>(null)
  const [saved, setSaved] = useState<Subscription | null>(null)
  const [defaultsResult, setDefaultsResult] = useState<{ key: string; value: SubscriptionDefaults | null; error: string } | null>(null)
  const [previewFormat, setPreviewFormat] = useState('sing-box-v13-openwrt')
  const [defaultsRetry, setDefaultsRetry] = useState(0)
  const [users, setUsers] = useState<UserBrief[] | null>(null)
  const [tab, setTab] = useState('basic')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [usersError, setUsersError] = useState('')
  const [conflict, setConflict] = useState(false)
  const [debugOpen, setDebugOpen] = useState(false)
  const [sourceDebug, setSourceDebug] = useState<{ source: Extract<SubscriptionSource, { type: 'url' }>; index: number } | null>(null)
  const selectedFormat = targets.find(target => target.format === previewFormat)?.format ?? targets[0]?.format ?? null
  const defaultsKey = JSON.stringify([id, selectedFormat, defaultsRetry])
  const currentDefaults = open && defaultsResult?.key === defaultsKey ? defaultsResult : null
  const defaults = currentDefaults?.value ?? null
  const defaultsError = currentDefaults?.error ?? ''

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
    void subscriptionApi.users().then((next) => { if (active) setUsers(next) })
      .catch((reason) => { if (active) setUsersError(reason instanceof Error ? reason.message : String(reason)) })
    return () => { active = false }
  }, [open, id])

  useEffect(() => {
    if (!open) return
    let active = true
    void subscriptionApi.defaults(selectedFormat ?? undefined)
      .then(value => { if (active) setDefaultsResult({ key: defaultsKey, value, error: '' }) })
      .catch(reason => { if (active) setDefaultsResult({ key: defaultsKey, value: null, error: reason instanceof Error ? reason.message : String(reason) }) })
    return () => { active = false }
  }, [open, selectedFormat, defaultsKey])

  const retryUsers = async () => {
    setUsersError('')
    try { setUsers(await subscriptionApi.users()) }
    catch (reason) { setUsersError(reason instanceof Error ? reason.message : String(reason)) }
  }

  const close = () => {
    if (saving) return
    setDefaultsRetry(value => value + 1)
    onClose()
  }

  const update = (patch: Partial<SubscriptionDraft>) => {
    setDraft((current) => current ? { ...current, ...patch } : current)
    setError('')
  }
  const save = (): undefined => {
    if (!draft) return undefined
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
      subscribeItems: draft.subscribeItems?.filter((source) => sourceText(source).trim()) ?? null,
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
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }

  return (
    <EditorProvider locale={locale}><Modal open={open} title={id ? t('configs.edit') : t('configs.new')} size="almost-full" bodyStyle={{ display: 'flex', flexDirection: 'column' }} onCancel={close} closable={!saving} keyboard={!saving} maskClosable={false} destroyOnClose footer={<div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-2 pt-4">
      <Button className="h-auto! min-h-8 min-w-0 max-w-full justify-self-start whitespace-normal text-left" disabled={!draft || targets.length === 0 || saving} onClick={() => setDebugOpen(true)}><span className="min-w-0 break-words">{t('editor.debugDraft')}</span></Button>
      <div className="flex shrink-0 gap-2">
        <Button disabled={saving} onClick={close}>{t('common.cancel')}</Button>
        <Button variant="primary" loading={saving} disabled={loading || !draft || (saved !== null && !saved.canEdit)} onClick={save}>{t('common.save')}</Button>
      </div>
    </div>}>
      {loading ? <div className="grid min-h-48 place-items-center"><Spin size="large" /></div> : null}
      {error ? <p role="alert" className="mb-3 text-sm text-red-600">{error}</p> : null}
      {id && !draft && !loading ? <Button size="small" onClick={() => void reload()}>{t('common.retry')}</Button> : null}
      {defaultsError ? <p role="alert" className="mb-3 text-sm text-red-600">{t('editor.inherit')}: {defaultsError} <Button size="small" onClick={() => setDefaultsRetry(value => value + 1)}>{t('common.retry')}</Button></p> : null}
      {usersError ? <p role="alert" className="mb-3 text-sm text-red-600">{t('editor.authorizedUsers')}: {usersError} <Button size="small" onClick={() => void retryUsers()}>{t('common.retry')}</Button></p> : null}
      {conflict ? <Popconfirm title={t('editor.reloadTitle')} description={t('editor.reloadWarning')} okText={t('common.confirm')} cancelText={t('common.cancel')} onConfirm={reload}><Button className="mb-3" size="small">{t('editor.reloadLatest')}</Button></Popconfirm> : null}
      {draft ? <SubscriptionConfigEditor value={{ ...draft, subscribeItems: draft.subscribeItems ?? [] }} defaults={defaults} onChange={update}
        activeKey={tab} onActiveKeyChange={setTab} readOnly={saving || Boolean(saved && !saved.canEdit)}
        nodes={(saved?.assignedCustomNodes ?? []).map(node => ({ id: node.id, name: node.name, label: `${node.name} · ${node.proxyType} · ${node.server}:${node.port}` }))}
        basicExtension={<AuthorizationFields draft={draft} users={users} canManageAuthorization={!id || Boolean(saved?.canManageAuthorization)} update={update} />}
        sourceExtension={draft.subscribeUrl !== null ? <label className="block space-y-1 text-sm">{t('editor.oldSource')}<TextArea rows={4} value={draft.subscribeUrl} onChange={event => update({ subscribeUrl: event.target.value || null })} className="font-mono text-xs" /></label> : null}
        dnsPreviewControls={<div className="space-y-1 text-sm"><Select value={selectedFormat ?? ''} addonBefore={t('editor.defaultsPreviewFormat')} aria-label={t('editor.defaultsPreviewFormat')} options={targets.length ? targets.map(target => ({ value: target.format, label: target.format })) : [{ value: '', label: t('editor.defaultsPreviewGeneric') }]} disabled={targets.length === 0} onChange={next => setPreviewFormat(String(next))} className="w-full max-w-md" /><p className="text-xs text-[var(--muted)]">{t('editor.defaultsPreviewHint')}</p></div>}
        onDebugSource={(source, index) => { if (source.type === 'url') setSourceDebug({ source, index }) }}
      /> : null}
      {draft && debugOpen ? <SubscriptionDebug draft={draft} targets={targets} subscriptionId={saved?.id} onClose={() => setDebugOpen(false)} /> : null}
      {sourceDebug ? <SourceDebug source={sourceDebug.source} saved={savedSource(saved, sourceDebug.source, sourceDebug.index)} onClose={() => setSourceDebug(null)} /> : null}
    </Modal></EditorProvider>
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
  const matched = saved.subscribeItems[index]
  if (!matched || matched.type === 'raw') return undefined
  const normalized = normalizeSource(matched)
  return normalized.type === 'url' ? { id: saved.id, index, source: normalized } : undefined
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

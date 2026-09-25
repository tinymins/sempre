import { Button, Modal, Popconfirm, Spin, Tabs, TextArea } from '@acme/components'
import { useEffect, useState } from 'react'
import { ServerApiError } from '../server-api'
import { subscriptionApi } from './api'
import type { Target } from './diagnostic-types'
import { BasicConfig, ExtraConfig, InheritedConfig } from './ConfigSections'
import { SourceItemsEditor } from './SourceItemsEditor'
import { SourceDebug } from './SourceDebug'
import { SubscriptionDebug } from './SubscriptionDebug'
import { PrivateAccessEditor } from './PrivateAccessEditor'
import { draftFromSubscription, emptyDraft, type Subscription, type SubscriptionDefaults, type SubscriptionDraft, type SubscriptionSource, type UserBrief } from './types'

interface Props {
  open: boolean
  id: string | null
  onClose: () => void
  onSaved: (value: Subscription) => void
  targets: Target[]
}

const tabs = [
  { key: 'basic', label: '基础信息' },
  { key: 'sources', label: '订阅来源' },
  { key: 'ruleList', label: '规则列表' },
  { key: 'group', label: '代理分组' },
  { key: 'customConfig', label: '自定义配置' },
  { key: 'dnsConfig', label: 'DNS 配置' },
  { key: 'privateAccess', label: '内网访问' },
  { key: 'servers', label: '额外节点' },
]

export function SubscriptionEditor({ open, id, onClose, onSaved, targets }: Props) {
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
  const [sourceDebug, setSourceDebug] = useState<SubscriptionSource | null>(null)

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

  const update = (patch: Partial<SubscriptionDraft>) => setDraft((current) => current ? { ...current, ...patch } : current)
  const save = (): undefined => {
    if (!draft) return undefined
    const newlyAuthorized = saved?.canManageAuthorization
      ? draft.authorizedUserIds.filter((id) => !saved.authorizedUserIds.includes(id))
      : []
    if (saved && newlyAuthorized.length > 0 && saved.assignedCustomNodes.length > 0) {
      Modal.confirm({
        title: '确认共享已分配的节点？',
        content: `新增 ${newlyAuthorized.length} 名授权用户后，他们将能在此配置集使用或查看已分配的 ${saved.assignedCustomNodes.length} 个节点。`,
        okText: '确认并保存', cancelText: '取消',
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
        setError('配置集已被他人修改。当前草稿仍保留；重新加载会丢弃此草稿。')
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
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }

  return (
    <Modal open={open} title={id ? '编辑配置集' : '新建配置集'} extra={<Button size="small" disabled={!draft || targets.length === 0 || saving} onClick={() => setDebugOpen(true)}>调试当前草稿</Button>} size="almost-full" okText="保存" cancelText="取消" onOk={save} onCancel={() => { if (!saving) onClose() }} confirmLoading={saving} okButtonProps={{ disabled: loading || !draft || (saved !== null && !saved.canEdit) }} cancelButtonProps={{ disabled: saving }} closable={!saving} keyboard={!saving} maskClosable={false} destroyOnClose>
      {loading ? <div className="grid min-h-48 place-items-center"><Spin size="large" /></div> : null}
      {error ? <p role="alert" className="mb-3 text-sm text-red-600">{error}</p> : null}
      {conflict ? <Popconfirm title="重新加载订阅？" description="当前未保存的修改会丢失。" onConfirm={reload}><Button className="mb-3" size="small">重新加载最新版本</Button></Popconfirm> : null}
      {draft ? (
        <div className="space-y-5">
          <Tabs items={tabs} activeKey={tab} onChange={setTab} type="segment" />
          {tab === 'basic' ? <BasicConfig draft={draft} users={users} canManageAuthorization={!id || Boolean(saved?.canManageAuthorization)} update={update} /> : null}
          {tab === 'sources' ? (
            <div className="space-y-4">
              <SourceItemsEditor value={draft.subscribeItems ?? []} onChange={(sources) => update({ subscribeItems: sources })} onDebug={setSourceDebug} />
              <InheritedConfig field="filter" draft={draft} defaults={defaults} update={update} />
              {draft.subscribeUrl !== null ? <label className="block space-y-1 text-sm">原有来源配置（JSONC）
                <TextArea rows={4} value={draft.subscribeUrl} onChange={(event) => update({ subscribeUrl: event.target.value || null })} className="font-mono text-xs" />
              </label> : null}
            </div>
          ) : null}
          {tab === 'ruleList' ? <InheritedConfig field="ruleList" draft={draft} defaults={defaults} update={update} /> : null}
          {tab === 'group' ? <InheritedConfig field="group" draft={draft} defaults={defaults} update={update} /> : null}
          {tab === 'customConfig' ? <InheritedConfig field="customConfig" draft={draft} defaults={defaults} update={update} /> : null}
          {tab === 'dnsConfig' ? <InheritedConfig field="dnsConfig" draft={draft} defaults={defaults} update={update} /> : null}
          {tab === 'privateAccess' ? <PrivateAccessEditor value={draft.privateAccessConfig} onChange={(next) => update({ privateAccessConfig: next })} /> : null}
          {tab === 'servers' ? <ExtraConfig draft={draft} assignedNodes={saved?.assignedCustomNodes ?? []} update={update} /> : null}
        </div>
      ) : null}
      {draft && debugOpen ? <SubscriptionDebug draft={draft} targets={targets} subscriptionId={saved?.id} onClose={() => setDebugOpen(false)} /> : null}
      {sourceDebug ? <SourceDebug source={sourceDebug} onClose={() => setSourceDebug(null)} /> : null}
    </Modal>
  )
}

function omitAuthorization(draft: SubscriptionDraft): Partial<SubscriptionDraft> {
  const rest: Partial<SubscriptionDraft> = { ...draft }
  delete rest.authorizedUserIds
  return rest
}

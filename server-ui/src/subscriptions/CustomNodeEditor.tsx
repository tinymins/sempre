import { Button, CodeEditor, Modal, Popconfirm, Select, Spin } from '@acme/components'
import { useCallback, useEffect, useState } from 'react'
import { subscriptionApi } from './api'
import type { CustomNode, Subscription, UserBrief } from './types'
import { useI18n } from '../i18n/provider'

interface Props {
  nodeId: string | null
  currentUserId: string
  users: UserBrief[] | null
  subscriptions: Subscription[] | null
  usersError: string
  subscriptionsError: string
  retryUsers: () => Promise<void>
  retrySubscriptions: () => Promise<void>
  onClose: () => void
  onSaved: () => void
}

export function CustomNodeEditor({ nodeId, currentUserId, users, subscriptions, usersError, subscriptionsError, retryUsers, retrySubscriptions, onClose, onSaved }: Props) {
  const { t } = useI18n()
  const [node, setNode] = useState<CustomNode | null>(null)
  const [content, setContent] = useState('{\n  "name": "",\n  "type": "vless",\n  "server": "",\n  "port": 443\n}')
  const [authorizedUserIds, setAuthorizedUserIds] = useState<string[]>([])
  const [assignedSubscribeIds, setAssignedSubscribeIds] = useState<string[]>([])
  const [loading, setLoading] = useState(Boolean(nodeId))
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const load = useCallback(async () => {
    if (!nodeId) return
    try {
      const detail = await subscriptionApi.customNode(nodeId)
      setNode(detail)
      setContent(detail.content)
      setAuthorizedUserIds(detail.authorizedUserIds)
      setAssignedSubscribeIds(detail.assignments.map((item) => item.subscribeId))
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setLoading(false) }
  }, [nodeId])
  useEffect(() => {
    if (!nodeId) return
    let active = true
    void subscriptionApi.customNode(nodeId).then((detail) => {
      if (!active) return
      setNode(detail)
      setContent(detail.content)
      setAuthorizedUserIds(detail.authorizedUserIds)
      setAssignedSubscribeIds(detail.assignments.map((item) => item.subscribeId))
    }).catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [nodeId])
  const canManage = !nodeId || node?.canManageAuthorization === true
  const removedEnabled = node?.assignments.filter((item) => item.enabled && !assignedSubscribeIds.includes(item.subscribeId)) ?? []

  const save = async () => {
    if (nodeId && !node) return
    setSaving(true)
    setError('')
    try {
      if (nodeId) {
        await subscriptionApi.updateCustomNode(nodeId, canManage
          ? { content, authorizedUserIds, assignedSubscribeIds, confirmUnassignEnabled: removedEnabled.length > 0 }
          : { content })
      } else {
        await subscriptionApi.createCustomNode({ content, authorizedUserIds, assignedSubscribeIds })
      }
      onSaved()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }

  const saveButton = <Button variant="primary" loading={saving} disabled={loading || (Boolean(nodeId) && !node) || !content.trim()}>{t('common.confirm')}</Button>
  return <Modal open title={nodeId ? t('nodes.edit') : t('nodes.new')} footer={<div className="flex justify-end gap-2"><Button disabled={saving} onClick={onClose}>{t('common.cancel')}</Button>{removedEnabled.length ? <Popconfirm title={t('nodes.unassignEnabled')} description={removedEnabled.map((item) => item.remark || item.subscribeId).join('、')} okText={t('common.confirm')} cancelText={t('common.cancel')} onConfirm={save}>{saveButton}</Popconfirm> : <Button variant="primary" loading={saving} disabled={loading || (Boolean(nodeId) && !node) || !content.trim()} onClick={() => void save()}>{t('common.confirm')}</Button>}</div>} onCancel={() => { if (!saving) onClose() }} closable={!saving} keyboard={!saving} maskClosable={false} size="large">
    <div className="space-y-4">
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin size="small" /> : null}
      {nodeId && !node && !loading ? <Button size="small" onClick={() => { setLoading(true); setError(''); void load() }}>{t('editor.reload')}</Button> : null}
      <div className="space-y-1 text-sm"><span className="text-red-600" aria-hidden="true">* </span>{t('nodes.content')}
        <CodeEditor value={content} height={320} ariaLabel={t('nodes.content')} readOnly={loading || (Boolean(nodeId) && !node)} onChange={(next) => { if (!loading && (!nodeId || node)) setContent(next) }} />
      </div>
      <label className="block space-y-1 text-sm">{t('nodes.authorized')}
        <Select mode="multiple" value={authorizedUserIds} disabled={loading || !canManage || users === null} options={users?.filter((user) => user.id !== currentUserId).map((user) => ({ value: user.id, label: `${user.name} (${user.email})` })) ?? []} onChange={(next) => setAuthorizedUserIds(next as string[])} showSearch className="w-full" />
      </label>
      {usersError ? <p role="alert" className="text-sm text-red-600">{usersError} <Button size="small" onClick={() => void retryUsers()}>{t('common.retry')}</Button></p> : null}
      <label className="block space-y-1 text-sm">{t('nodes.assignments')}
        <Select mode="multiple" value={assignedSubscribeIds} disabled={loading || !canManage || subscriptions === null} options={subscriptions?.map((item) => ({ value: item.id, label: item.remark || item.id })) ?? []} onChange={(next) => setAssignedSubscribeIds(next as string[])} showSearch className="w-full" />
      </label>
      {subscriptionsError ? <p role="alert" className="text-sm text-red-600">{subscriptionsError} <Button size="small" onClick={() => void retrySubscriptions()}>{t('common.retry')}</Button></p> : null}
    </div>
  </Modal>
}

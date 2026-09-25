import { Button, Modal, Popconfirm, Select, Spin, TextArea } from '@acme/components'
import { useCallback, useEffect, useState } from 'react'
import { subscriptionApi } from './api'
import type { CustomNode, Subscription, UserBrief } from './types'

interface Props {
  nodeId: string | null
  currentUserId: string
  users: UserBrief[]
  subscriptions: Subscription[]
  onClose: () => void
  onSaved: () => void
}

export function CustomNodeEditor({ nodeId, currentUserId, users, subscriptions, onClose, onSaved }: Props) {
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

  const saveButton = <Button variant="primary" loading={saving} disabled={loading || (Boolean(nodeId) && !node) || !content.trim()}>OK</Button>
  return <Modal open title={nodeId ? '编辑节点' : '新建节点'} footer={<div className="flex justify-end gap-2"><Button disabled={saving} onClick={onClose}>Cancel</Button>{removedEnabled.length ? <Popconfirm title="移除已启用的节点分配？" description={removedEnabled.map((item) => item.remark || item.subscribeId).join('、')} onConfirm={save}>{saveButton}</Popconfirm> : <Button variant="primary" loading={saving} disabled={loading || (Boolean(nodeId) && !node) || !content.trim()} onClick={() => void save()}>OK</Button>}</div>} onCancel={() => { if (!saving) onClose() }} closable={!saving} keyboard={!saving} maskClosable={false} size="large">
    <div className="space-y-4">
      {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
      {loading ? <Spin size="small" /> : null}
      {nodeId && !node && !loading ? <Button size="small" onClick={() => { setLoading(true); setError(''); void load() }}>重新加载节点</Button> : null}
      <label className="block space-y-1 text-sm"><span className="text-red-600" aria-hidden="true">* </span>节点内容（单个 JSONC 对象）
        <TextArea rows={14} required value={content} disabled={loading || (Boolean(nodeId) && !node)} onChange={(event) => setContent(event.target.value)} className="font-mono text-xs" />
      </label>
      <label className="block space-y-1 text-sm">授权用户
        <Select mode="multiple" value={authorizedUserIds} disabled={loading || !canManage} options={users.filter((user) => user.id !== currentUserId).map((user) => ({ value: user.id, label: `${user.name} (${user.email})` }))} onChange={(next) => setAuthorizedUserIds(next as string[])} showSearch className="w-full" />
      </label>
      <label className="block space-y-1 text-sm">分配给配置集
        <Select mode="multiple" value={assignedSubscribeIds} disabled={loading || !canManage} options={subscriptions.map((item) => ({ value: item.id, label: item.remark || item.id }))} onChange={(next) => setAssignedSubscribeIds(next as string[])} showSearch className="w-full" />
      </label>
    </div>
  </Modal>
}

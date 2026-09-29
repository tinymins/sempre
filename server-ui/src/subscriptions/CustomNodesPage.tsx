import { Button, Card, Popconfirm, Table, Tag, Tooltip, useToast } from '@acme/components'
import { Edit3, Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { subscriptionApi } from './api'
import { CustomNodeEditor } from './CustomNodeEditor'
import type { CustomNode, Subscription, UserBrief } from './types'
import { useI18n } from '../i18n/provider'

export function CustomNodesPage({ currentUserId }: { currentUserId: string }) {
  const { t, number } = useI18n()
  const toast = useToast()
  const [nodes, setNodes] = useState<CustomNode[]>([])
  const [subscriptions, setSubscriptions] = useState<Subscription[] | null>(null)
  const [users, setUsers] = useState<UserBrief[] | null>(null)
  const [editing, setEditing] = useState<string | null | undefined>(undefined)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [subscriptionsError, setSubscriptionsError] = useState('')
  const [usersError, setUsersError] = useState('')
  const loadNodes = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      setNodes(await subscriptionApi.customNodes())
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally { setLoading(false) }
  }, [])
  const loadSubscriptions = async () => {
    setSubscriptionsError('')
    try { setSubscriptions(await subscriptionApi.list()) }
    catch (reason) { setSubscriptionsError(reason instanceof Error ? reason.message : String(reason)) }
  }
  const loadUsers = async () => {
    setUsersError('')
    try { setUsers(await subscriptionApi.users()) }
    catch (reason) { setUsersError(reason instanceof Error ? reason.message : String(reason)) }
  }
  useEffect(() => {
    let active = true
    void subscriptionApi.customNodes().then((next) => { if (active) setNodes(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    void subscriptionApi.list().then((next) => { if (active) setSubscriptions(next) })
      .catch((reason) => { if (active) setSubscriptionsError(reason instanceof Error ? reason.message : String(reason)) })
    void subscriptionApi.users().then((next) => { if (active) setUsers(next) })
      .catch((reason) => { if (active) setUsersError(reason instanceof Error ? reason.message : String(reason)) })
    return () => { active = false }
  }, [])
  const remove = async (id: string) => {
    try {
      await subscriptionApi.removeCustomNode(id)
      toast.success(t('common.deleted'))
      await loadNodes()
    } catch (reason) { toast.error(reason instanceof Error ? reason.message : String(reason)) }
  }
  return <section className="space-y-4">
    <div className="flex items-center justify-between gap-3"><div className="min-w-0"><h1 className="text-xl font-semibold">{t('nodes.title')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('nodes.subtitle')}</p></div><Button variant="primary" className="shrink-0 whitespace-nowrap" icon={<Plus size={16} />} onClick={() => setEditing(null)}>{t('nodes.new')}</Button></div>
    {error ? <Card><p role="alert" className="text-sm text-red-600">{error}</p><Button className="mt-2" onClick={() => void loadNodes()}>{t('common.retry')}</Button></Card> : null}
    <Table<CustomNode> rowKey="id" dataSource={nodes} loading={loading} pagination={false} scroll={{ x: 930 }} columns={[
      { title: t('common.node'), width: 200, minWidth: 200, ellipsis: true, render: (_, node) => <Tooltip title={node.name}><span className="block truncate">{node.name}</span></Tooltip> },
      { title: t('common.protocol'), width: 110, minWidth: 110, className: 'whitespace-nowrap', render: (_, node) => <Tag color="blue">{node.proxyType}</Tag> },
      { title: t('common.server'), width: 230, minWidth: 230, ellipsis: true, render: (_, node) => <Tooltip title={`${node.server}:${node.port}`}><span className="block truncate">{node.server}:{node.port}</span></Tooltip> },
      { title: t('nodes.creator'), width: 160, minWidth: 160, ellipsis: true, render: (_, node) => <Tooltip title={node.creator.name}><span className="block truncate">{node.creator.name}</span></Tooltip> },
      { title: t('nodes.assignedConfigs'), width: 130, minWidth: 130, className: 'whitespace-nowrap', render: (_, node) => number(node.assignments.length) },
      { title: t('common.actions'), width: 100, minWidth: 100, className: 'whitespace-nowrap', render: (_, node) => <div className="flex gap-1">{node.canEdit ? <Button size="small" variant="text" icon={<Edit3 size={14} />} aria-label={`${t('common.edit')} ${node.name}`} onClick={() => setEditing(node.id)} /> : null}{node.canManageAuthorization ? <Popconfirm title={t('nodes.deleteTitle')} description={t('nodes.deleteHint')} okText={t('common.delete')} cancelText={t('common.cancel')} onConfirm={() => remove(node.id)} okType="danger"><Button size="small" variant="text" danger icon={<Trash2 size={14} />} aria-label={`${t('common.delete')} ${node.name}`} /></Popconfirm> : null}</div> },
    ]} locale={{ emptyText: t('nodes.empty') }} />
    {editing !== undefined ? <CustomNodeEditor nodeId={editing} currentUserId={currentUserId} users={users} subscriptions={subscriptions} usersError={usersError} subscriptionsError={subscriptionsError} retryUsers={loadUsers} retrySubscriptions={loadSubscriptions} onClose={() => setEditing(undefined)} onSaved={() => { setEditing(undefined); void loadNodes() }} /> : null}
  </section>
}

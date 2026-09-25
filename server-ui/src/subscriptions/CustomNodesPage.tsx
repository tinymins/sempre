import { Button, Card, Popconfirm, Table, Tag } from '@acme/components'
import { Edit3, Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { subscriptionApi } from './api'
import { CustomNodeEditor } from './CustomNodeEditor'
import type { CustomNode, Subscription, UserBrief } from './types'

export function CustomNodesPage({ currentUserId }: { currentUserId: string }) {
  const [nodes, setNodes] = useState<CustomNode[]>([])
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([])
  const [users, setUsers] = useState<UserBrief[]>([])
  const [editing, setEditing] = useState<string | null | undefined>(undefined)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const [nextNodes, nextSubscriptions, nextUsers] = await Promise.all([subscriptionApi.customNodes(), subscriptionApi.list(), subscriptionApi.users()])
      setNodes(nextNodes)
      setSubscriptions(nextSubscriptions)
      setUsers(nextUsers)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally { setLoading(false) }
  }, [])
  useEffect(() => {
    let active = true
    void Promise.all([subscriptionApi.customNodes(), subscriptionApi.list(), subscriptionApi.users()]).then(([nextNodes, nextSubscriptions, nextUsers]) => {
      if (!active) return
      setNodes(nextNodes)
      setSubscriptions(nextSubscriptions)
      setUsers(nextUsers)
    }).catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [])
  const remove = async (id: string) => {
    try {
      await subscriptionApi.removeCustomNode(id)
      await load()
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
  }
  return <section className="space-y-4">
    <div className="flex items-center justify-between gap-3"><div className="min-w-0"><h1 className="text-xl font-semibold">自定义节点</h1><p className="mt-1 text-sm text-[var(--muted)]">集中管理节点内容，并将节点使用权分配给配置集。</p></div><Button variant="primary" className="shrink-0 whitespace-nowrap" icon={<Plus size={16} />} onClick={() => setEditing(null)}>新建节点</Button></div>
    {error ? <Card><p role="alert" className="text-sm text-red-600">{error}</p><Button className="mt-2" onClick={() => void load()}>重试</Button></Card> : null}
    <Table<CustomNode> rowKey="id" dataSource={nodes} loading={loading} pagination={false} scroll={{ x: 700 }} columns={[
      { title: '节点名称', dataIndex: 'name' }, { title: '协议', dataIndex: 'proxyType', render: (value) => <Tag color="blue">{value}</Tag> },
      { title: '服务器', render: (_, node) => `${node.server}:${node.port}` },
      { title: '创建者', render: (_, node) => node.creator.name },
      { title: '已分配配置', render: (_, node) => node.assignments.length },
      { title: '操作', render: (_, node) => <div className="flex gap-1">{node.canEdit ? <Button size="small" variant="text" icon={<Edit3 size={14} />} aria-label={`编辑 ${node.name}`} onClick={() => setEditing(node.id)} /> : null}{node.canManageAuthorization ? <Popconfirm title="删除这个自定义节点？" onConfirm={() => remove(node.id)} okType="danger"><Button size="small" variant="text" danger icon={<Trash2 size={14} />} aria-label={`删除 ${node.name}`} /></Popconfirm> : null}</div> },
    ]} />
    {editing !== undefined ? <CustomNodeEditor nodeId={editing} currentUserId={currentUserId} users={users} subscriptions={subscriptions} onClose={() => setEditing(undefined)} onSaved={() => { setEditing(undefined); void load() }} /> : null}
  </section>
}

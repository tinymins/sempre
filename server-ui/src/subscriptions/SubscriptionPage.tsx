import { Button, Card, Popconfirm, Table } from '@acme/components'
import { BarChart3, Edit3, Eye, Link2, Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { subscriptionApi } from './api'
import type { Target } from './diagnostic-types'
import { SubscriptionEditor } from './SubscriptionEditor'
import { SubscriptionLinks } from './SubscriptionLinks'
import { SubscriptionPreview } from './SubscriptionPreview'
import { SubscriptionStats } from './SubscriptionStats'
import type { Subscription } from './types'

export function SubscriptionPage({ initialEditId }: { initialEditId?: string }) {
  const location = useLocation()
  const navigate = useNavigate()
  const [items, setItems] = useState<Subscription[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [editing, setEditing] = useState<string | null | undefined>(initialEditId ?? (location.state?.createSubscription === true ? null : undefined))
  const [links, setLinks] = useState<Subscription | null>(null)
  const [preview, setPreview] = useState<Subscription | null>(null)
  const [stats, setStats] = useState<Subscription | null>(null)
  const [targets, setTargets] = useState<Target[]>([])
  const [targetsError, setTargetsError] = useState('')

  useEffect(() => {
    if (location.state?.createSubscription === true) navigate({ pathname: location.pathname, search: location.search }, { replace: true, state: null })
  }, [location.pathname, location.search, location.state, navigate])

  const load = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      setItems(await subscriptionApi.list())
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setLoading(false)
    }
  }, [])
  useEffect(() => {
    let active = true
    void subscriptionApi.list().then((next) => { if (active) setItems(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [])
  useEffect(() => {
    let active = true
    void subscriptionApi.targets().then((next) => { if (active) setTargets(next) })
      .catch((reason) => { if (active) setTargetsError(reason instanceof Error ? reason.message : String(reason)) })
    return () => { active = false }
  }, [])

  const remove = async (id: string) => {
    try {
      await subscriptionApi.remove(id)
      await load()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }
  const saved = (value: Subscription) => {
    closeEditor()
    setItems((current) => [value, ...current.filter((item) => item.id !== value.id)])
    void load()
  }
  const closeEditor = () => {
    setEditing(undefined)
    if (initialEditId) navigate({ pathname: '/subscriptions', search: location.search }, { replace: true })
  }
  const actions = (item: Subscription) => <div className="flex flex-wrap items-center gap-1">
    <Button size="small" variant="text" icon={<Link2 size={15} />} onClick={() => setLinks(item)}>链接</Button>
    <Button size="small" variant="text" icon={<BarChart3 size={15} />} onClick={() => setStats(item)}>统计</Button>
    <Button size="small" variant="text" icon={<Eye size={15} />} disabled={targets.length === 0} onClick={() => setPreview(item)}>预览</Button>
    {item.canEdit ? <Button size="small" variant="text" icon={<Edit3 size={15} />} onClick={() => setEditing(item.id)}>编辑</Button> : null}
    {item.canDelete ? <Popconfirm title="删除这个配置集？" description="删除后无法恢复。" onConfirm={() => remove(item.id)} okType="danger">
      <Button size="small" variant="text" danger icon={<Trash2 size={15} />}>删除</Button>
    </Popconfirm> : null}
  </div>

  return <section aria-labelledby="subscriptions-title" className="space-y-5">
    <div className="flex items-end justify-between gap-3">
      <div>
        <p className="text-xs font-medium uppercase tracking-[0.18em] text-emerald-600 dark:text-emerald-400">Sempre Server</p>
        <h1 id="subscriptions-title" className="mt-1 text-2xl font-semibold tracking-tight">配置集</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">管理配置集、输出地址和生成结果。</p>
      </div>
      <Button variant="primary" icon={<Plus size={16} />} onClick={() => setEditing(null)}>新建配置集</Button>
    </div>
    {error ? <Card><p role="alert" className="text-sm text-red-600">{error}</p><Button className="mt-3" onClick={() => void load()}>重试</Button></Card> : null}
    {targetsError ? <p role="alert" className="text-sm text-red-600">输出格式加载失败：{targetsError}</p> : null}
    <div className="space-y-3 md:hidden">
      {loading ? <Card><p className="text-sm text-[var(--muted)]">正在加载配置集…</p></Card> : null}
      {!loading && !error && items.length === 0 ? <Card><p className="text-sm text-[var(--muted)]">暂无配置集。点击“新建配置集”开始。</p></Card> : null}
      {items.map((item) => <Card key={item.id} className="space-y-3">
        <div className="flex items-start justify-between gap-2"><strong className="min-w-0 truncate">{item.remark || '未命名配置集'}</strong><span className="text-xs text-[var(--muted)]">最近生成 {item.cachedNodeCount} 节点</span></div>
        <p className="text-xs text-[var(--muted)]">{item.creator.name} · {item.accessCount} 次访问 · {new Date(item.updatedAt).toLocaleString()}</p>
        {actions(item)}
      </Card>)}
    </div>
    <div className="hidden md:block">
      <Table<Subscription> rowKey="id" loading={loading} pagination={false} dataSource={items} locale={{ emptyText: '暂无配置集。点击“新建配置集”开始。' }} columns={[
        { title: '创建者', render: (_, item) => item.creator.name },
        { title: '备注', render: (_, item) => item.remark || '未命名配置集' },
        { title: '最近生成节点数', dataIndex: 'cachedNodeCount', width: 130 },
        { title: '访问数', dataIndex: 'accessCount', width: 90 },
        { title: '更新时间', width: 170, render: (_, item) => new Date(item.updatedAt).toLocaleString() },
        { title: '操作', width: 320, render: (_, item) => actions(item) },
      ]} />
    </div>
    {editing !== undefined ? <SubscriptionEditor open id={editing} targets={targets} onClose={closeEditor} onSaved={saved} /> : null}
    {links ? <SubscriptionLinks subscription={links} targets={targets} onClose={() => setLinks(null)} /> : null}
    {preview ? <SubscriptionPreview subscription={preview} targets={targets} onClose={() => setPreview(null)} /> : null}
    {stats ? <SubscriptionStats subscription={stats} onClose={() => setStats(null)} /> : null}
  </section>
}

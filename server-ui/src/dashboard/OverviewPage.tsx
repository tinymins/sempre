import { Button, Card, Spin } from '@acme/components'
import { ArrowRight, BarChart3, Network, Plus, Rss } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import type { ServerUser } from '../server-api'
import { dashboardApi, type Overview } from './api'

export function OverviewPage({ user }: { user: ServerUser }) {
  const navigate = useNavigate()
  const location = useLocation()
  const go = (pathname: string, create = false) => navigate({ pathname, search: location.search }, create ? { state: { createSubscription: true } } : undefined)
  const [overview, setOverview] = useState<Overview | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const load = async () => {
    setLoading(true)
    setError('')
    try { setOverview(await dashboardApi.overview()) }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)) }
    finally { setLoading(false) }
  }
  useEffect(() => {
    let active = true
    void dashboardApi.overview().then((next) => { if (active) setOverview(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [])

  return <section className="space-y-6">
    <div><h1 className="text-2xl font-semibold">概览</h1><p className="mt-1 text-sm text-[var(--muted)]">欢迎回来，{user.name || user.email}</p></div>
    {loading ? <Spin /> : null}
    {error ? <Card><p role="alert" className="text-sm text-red-600">{error}</p><Button className="mt-2" onClick={() => void load()}>重试</Button></Card> : null}
    {overview ? <>
      <div className="grid gap-3 sm:grid-cols-3">
        {([
          ['配置集总数', overview.totalSubscriptions, <Rss size={19} />],
          ['有效节点', overview.totalNodes, <Network size={19} />],
          ['今日访问', overview.todayRequests, <BarChart3 size={19} />],
        ] as const).map(([label, value, icon]) => <Card key={label} className="flex items-center gap-3"><span className="text-emerald-600">{icon}</span><div><p className="text-xs text-[var(--muted)]">{label}</p><strong className="text-2xl">{value}</strong></div></Card>)}
      </div>
      <div className="grid gap-4 lg:grid-cols-5">
        <Card className="space-y-3 lg:col-span-3">
          <div className="flex items-center justify-between"><h2 className="font-semibold">最近配置集</h2><Button size="small" variant="text" icon={<ArrowRight size={14} />} onClick={() => go('/subscriptions')}>查看全部</Button></div>
          {overview.topSubscriptions.length === 0 ? <p className="text-sm text-[var(--muted)]">暂无配置集</p> : overview.topSubscriptions.slice(0, 5).map((item) => <div key={item.id} className="flex items-center justify-between gap-2 border-t border-[var(--border)] pt-2 text-sm"><span className="min-w-0 truncate">{item.remark || '未命名配置集'}<small className="ml-2 text-[var(--muted)]">{item.creator.name}</small></span><span className="shrink-0 text-xs text-[var(--muted)]">{item.lastAccessAt ? new Date(item.lastAccessAt).toLocaleDateString() : '尚未访问'}</span></div>)}
        </Card>
        <Card className="space-y-3 lg:col-span-2"><h2 className="font-semibold">快捷操作</h2><Button block icon={<Plus size={15} />} onClick={() => go('/subscriptions', true)}>新建配置集</Button><Button block icon={<Rss size={15} />} onClick={() => go('/subscriptions')}>管理配置集</Button><Button block icon={<Network size={15} />} onClick={() => go('/network')}>网络工具</Button></Card>
      </div>
    </> : null}
  </section>
}

import { Button, Card, Spin } from '@acme/components'
import { ArrowRight, BarChart3, Network, Plus, Rss } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import type { ServerUser } from '../server-api'
import { dashboardApi, type Overview } from './api'
import { useI18n } from '../i18n/provider'

export function OverviewPage({ user }: { user: ServerUser }) {
  const { t, date, number } = useI18n()
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
    <div><h1 className="text-2xl font-semibold">{t('overview.title')}</h1><p className="mt-1 text-sm text-[var(--muted)]">{t('overview.welcome', { name: user.name || user.email })}</p></div>
    {loading ? <Spin /> : null}
    {error ? <Card><p role="alert" className="text-sm text-red-600">{error}</p><Button className="mt-2" onClick={() => void load()}>{t('common.retry')}</Button></Card> : null}
    {overview ? <>
      <div className="grid gap-3 sm:grid-cols-3">
        {([
          [t('overview.totalConfigs'), overview.totalSubscriptions, <Rss size={19} />],
          [t('overview.activeNodes'), overview.totalNodes, <Network size={19} />],
          [t('overview.todayRequests'), overview.todayRequests, <BarChart3 size={19} />],
        ] as const).map(([label, value, icon]) => <Card key={label}><div className="flex items-center gap-3"><span className="text-emerald-600">{icon}</span><div><p className="text-xs text-[var(--muted)]">{label}</p><strong className="text-2xl">{number(value)}</strong></div></div></Card>)}
      </div>
      <div className="grid gap-4 lg:grid-cols-5">
        <Card className="lg:col-span-3"><div className="space-y-3">
          <div className="flex items-center justify-between"><div><h2 className="font-semibold">{t('overview.summary')}</h2><p className="text-xs text-[var(--muted)]">{t('overview.recentFive')}</p></div><Button size="small" variant="text" icon={<ArrowRight size={14} />} onClick={() => go('/subscriptions')}>{t('overview.viewAll')}</Button></div>
          {overview.topSubscriptions.length === 0 ? <p className="text-sm text-[var(--muted)]">{t('overview.noConfigs')}</p> : overview.topSubscriptions.slice(0, 5).map((item) => <Button key={item.id} block variant="text" className="justify-start border-t border-[var(--border)] text-left" onClick={() => go(`/subscriptions/${item.id}`)}><span className="min-w-0 flex-1 truncate">{item.remark || t('configs.unnamed')}<small className="ml-2 text-[var(--muted)]">{item.creator.name}</small></span><span className="shrink-0 text-xs text-[var(--muted)]">{item.lastAccessAt ? date(item.lastAccessAt, { dateStyle: 'short' }) : t('common.never')}</span></Button>)}
        </div></Card>
        <Card className="lg:col-span-2"><div className="space-y-3">
          <div><h2 className="font-semibold">{t('overview.quickActions')}</h2><p className="text-xs text-[var(--muted)]">{t('overview.quickHint')}</p></div>
          <Button block icon={<Plus size={15} />} className="h-auto! min-h-8 justify-start! whitespace-normal py-2! text-left" onClick={() => go('/subscriptions', true)}><span className="min-w-0 flex-1 break-words">{t('overview.createConfig')}</span></Button>
          <Button block icon={<Rss size={15} />} className="h-auto! min-h-8 justify-start! whitespace-normal py-2! text-left" onClick={() => go('/subscriptions')}><span className="min-w-0 flex-1 break-words">{t('overview.manageConfigs')}</span></Button>
          <Button block icon={<Network size={15} />} className="h-auto! min-h-8 justify-start! whitespace-normal py-2! text-left" onClick={() => go('/network')}><span className="min-w-0 flex-1 break-words">{t('overview.networkAction')}</span></Button>
        </div></Card>
      </div>
    </> : null}
  </section>
}

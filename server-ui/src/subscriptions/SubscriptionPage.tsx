import { Button, Card, Popconfirm, Table, useToast } from '@acme/components'
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
import { useI18n } from '../i18n/provider'

export function SubscriptionPage({ initialEditId }: { initialEditId?: string }) {
  const { t, date, number } = useI18n()
  const toast = useToast()
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
      toast.success(t('common.deleted'))
      await load()
    } catch (reason) {
      toast.error(reason instanceof Error ? reason.message : String(reason))
    }
  }
  const saved = (value: Subscription) => {
    closeEditor()
    toast.success(t('common.saved'))
    setItems((current) => [value, ...current.filter((item) => item.id !== value.id)])
    void load()
  }
  const closeEditor = () => {
    setEditing(undefined)
    if (initialEditId) navigate({ pathname: '/subscriptions', search: location.search }, { replace: true })
  }
  const actions = (item: Subscription) => <div className="flex flex-wrap items-center gap-1">
    <Button size="small" variant="text" icon={<Link2 size={15} />} aria-label={t('configs.linkAction', { name: item.remark || t('configs.unnamed') })} title={t('links.title', { name: item.remark || t('configs.unnamed') })} onClick={() => setLinks(item)} />
    <Button size="small" variant="text" icon={<BarChart3 size={15} />} aria-label={t('configs.statsAction', { name: item.remark || t('configs.unnamed') })} title={t('stats.title', { name: item.remark || t('configs.unnamed') })} onClick={() => setStats(item)} />
    <Button size="small" variant="text" icon={<Eye size={15} />} aria-label={t('configs.previewAction', { name: item.remark || t('configs.unnamed') })} title={t('preview.title', { name: item.remark || t('configs.unnamed') })} disabled={targets.length === 0} onClick={() => setPreview(item)} />
    {item.canEdit ? <Button size="small" variant="text" icon={<Edit3 size={15} />} aria-label={t('configs.editAction', { name: item.remark || t('configs.unnamed') })} title={t('configs.edit')} onClick={() => setEditing(item.id)} /> : null}
    {item.canDelete ? <Popconfirm title={t('configs.deleteTitle')} description={t('configs.deleteDetail')} okText={t('common.delete')} cancelText={t('common.cancel')} onConfirm={() => remove(item.id)} okType="danger">
      <Button size="small" variant="text" danger icon={<Trash2 size={15} />} aria-label={t('configs.deleteAction', { name: item.remark || t('configs.unnamed') })} title={t('common.delete')} />
    </Popconfirm> : null}
  </div>

  return <section aria-labelledby="subscriptions-title" className="space-y-5">
    <Card className="hidden space-y-1 md:block"><strong className="text-sm">{t('configs.introTitle')}</strong><p className="text-sm text-[var(--muted)]">{t('configs.intro')}</p></Card>
    <div className="flex items-end justify-between gap-3">
      <div>
        <h1 id="subscriptions-title" className="text-2xl font-semibold tracking-tight">{t('configs.title')}</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">{t('configs.subtitle')}</p>
      </div>
      <Button variant="primary" icon={<Plus size={16} />} onClick={() => setEditing(null)}>{t('configs.new')}</Button>
    </div>
    {error ? <Card><p role="alert" className="text-sm text-red-600">{error}</p><Button className="mt-3" onClick={() => void load()}>{t('common.retry')}</Button></Card> : null}
    {targetsError ? <p role="alert" className="text-sm text-red-600">{t('configs.outputLoadFailed', { reason: targetsError })}</p> : null}
    <div className="space-y-3 md:hidden">
      {loading ? <Card><p className="text-sm text-[var(--muted)]">{t('configs.loading')}</p></Card> : null}
      {!loading && !error && items.length === 0 ? <Card><p className="text-sm text-[var(--muted)]">{t('configs.empty')}</p></Card> : null}
      {items.map((item) => <Card key={item.id} className="space-y-3">
        <div className="flex items-start justify-between gap-2"><strong className="min-w-0 truncate">{item.remark || t('configs.unnamed')}</strong><span className="text-xs text-[var(--muted)]">{t('configs.recentNodesValue', { count: number(item.cachedNodeCount) })}</span></div>
        <p className="text-xs text-[var(--muted)]">{item.creator.name} · {t('configs.accessCountValue', { count: number(item.accessCount) })} · {date(item.updatedAt)}</p>
        {actions(item)}
      </Card>)}
    </div>
    <div className="hidden md:block">
      <Table<Subscription> rowKey="id" loading={loading} pagination={false} dataSource={items} locale={{ emptyText: t('configs.empty') }} columns={[
        { title: t('configs.creator'), render: (_, item) => item.creator.name },
        { title: t('configs.remark'), render: (_, item) => item.remark || t('configs.unnamed') },
        { title: t('configs.recentNodes'), render: (_, item) => number(item.cachedNodeCount), width: 130 },
        { title: t('configs.accessCount'), render: (_, item) => number(item.accessCount), width: 90 },
        { title: t('configs.updatedAt'), width: 170, render: (_, item) => date(item.updatedAt) },
        { title: t('common.actions'), width: 190, render: (_, item) => actions(item) },
      ]} />
    </div>
    {editing !== undefined ? <SubscriptionEditor open id={editing} targets={targets} onClose={closeEditor} onSaved={saved} /> : null}
    {links ? <SubscriptionLinks subscription={links} targets={targets} onClose={() => setLinks(null)} /> : null}
    {preview ? <SubscriptionPreview subscription={preview} targets={targets} onClose={() => setPreview(null)} /> : null}
    {stats ? <SubscriptionStats subscription={stats} onClose={() => setStats(null)} /> : null}
  </section>
}

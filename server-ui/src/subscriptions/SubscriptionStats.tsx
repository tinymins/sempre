import { Modal, Pagination, Spin, Table } from '@acme/components'
import { useEffect, useState } from 'react'
import { subscriptionApi } from './api'
import type { AccessStats } from './diagnostic-types'
import type { Subscription } from './types'
import { useI18n } from '../i18n/provider'

export function SubscriptionStats({ subscription, onClose }: { subscription: Subscription | null; onClose: () => void }) {
  const { t, date, number } = useI18n()
  const [stats, setStats] = useState<AccessStats | null>(null)
  const [page, setPage] = useState(1)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  useEffect(() => {
    if (!subscription) return
    let active = true
    void subscriptionApi.stats(subscription.id, page).then((next) => { if (active) setStats(next) })
      .catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [subscription, page])

  return <Modal open={Boolean(subscription)} title={t('stats.title', { name: subscription?.remark || t('configs.unnamed') })} footer={null} onCancel={onClose} size="large">
    {loading ? <Spin size="large" /> : null}
    {error ? <p role="alert" className="text-sm text-red-600">{error}</p> : null}
    {stats ? <div className="space-y-5">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Metric label={t('stats.retainedAccess')} value={number(stats.totalAccesses)} />
        <Metric label={t('stats.todayAccess')} value={number(stats.todayAccess)} />
        <Metric label={t('stats.cachedNodes')} value={number(stats.cachedNodeCount)} />
        <Metric label={t('stats.lastAccess')} value={stats.lastAccessAt ? date(stats.lastAccessAt) : '—'} />
      </div>
      <p className="text-xs text-[var(--muted)]">{t('stats.retentionHint')}</p>
      <section><h3 className="mb-2 text-sm font-medium">{t('stats.byFormat')}</h3>
        <div className="flex flex-wrap gap-2">{stats.accessByType.map((item) => <span key={item.type} className="rounded border border-[var(--border)] px-2 py-1 text-xs">{item.type}: {number(item.count)}</span>)}</div>
      </section>
      <section><h3 className="mb-2 text-sm font-medium">{t('stats.recent')}</h3>
        <Table rowKey="id" dataSource={stats.recentAccesses} pagination={false} size="small" scroll={{ x: 620 }} columns={[
          { title: t('common.time'), render: (_, item) => date(item.createdAt) },
          { title: t('common.format'), dataIndex: 'accessType' },
          { title: t('stats.nodeCount'), render: (_, item) => item.nodeCount === null ? '—' : number(item.nodeCount) },
          { title: t('stats.ip'), dataIndex: 'ip' },
          { title: t('common.userAgent'), dataIndex: 'userAgent' },
        ]} locale={{ emptyText: t('common.noData') }} />
        <Pagination className="mt-3" current={page} total={stats.recentAccessTotal} pageSize={20} onChange={setPage} />
      </section>
    </div> : null}
  </Modal>
}

function Metric({ label, value }: { label: string; value: string | number }) {
  return <div className="rounded-lg border border-[var(--border)] p-3"><p className="text-xs text-[var(--muted)]">{label}</p><strong className="mt-1 block text-lg">{value}</strong></div>
}
